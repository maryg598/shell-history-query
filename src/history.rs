#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryEntry {
    pub command: String,
    pub timestamp: Option<i64>,
}

/// Parses a history file's contents into individual commands, regardless of
/// whether it's plain bash history, zsh's extended format, or bash history
/// written with HISTTIMEFORMAT (which precedes each command with a
/// `#<timestamp>` comment line).
pub fn parse(content: &str) -> Vec<HistoryEntry> {
    let logical_lines = join_continuations(content);
    let mut entries = Vec::new();
    let mut pending_timestamp: Option<i64> = None;

    for line in logical_lines {
        if line.trim().is_empty() {
            continue;
        }

        if let Some(ts) = parse_bash_timestamp_comment(&line) {
            pending_timestamp = Some(ts);
            continue;
        }

        if let Some((ts, command)) = parse_zsh_extended(&line) {
            entries.push(HistoryEntry {
                command,
                timestamp: Some(ts),
            });
            continue;
        }

        entries.push(HistoryEntry {
            command: line,
            timestamp: pending_timestamp.take(),
        });
    }

    entries
}

// zsh's extended history escapes an embedded newline in a multiline command
// as a trailing backslash at the end of the raw line, so a logical command
// can span several raw lines. A doubled-up backslash is a literal backslash
// in the command text, not a continuation marker, so parity is what matters.
fn join_continuations(content: &str) -> Vec<String> {
    let mut logical = Vec::new();
    let mut current = String::new();
    let mut in_continuation = false;

    for raw_line in content.split('\n') {
        if in_continuation {
            current.push('\n');
            current.push_str(raw_line);
        } else {
            current = raw_line.to_string();
        }

        let trailing_backslashes = current.chars().rev().take_while(|&c| c == '\\').count();
        if trailing_backslashes % 2 == 1 {
            current.pop();
            in_continuation = true;
        } else {
            logical.push(std::mem::take(&mut current));
            in_continuation = false;
        }
    }

    if in_continuation {
        logical.push(current);
    }

    logical
}

fn parse_bash_timestamp_comment(line: &str) -> Option<i64> {
    let rest = line.strip_prefix('#')?;
    if rest.is_empty() || !rest.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    rest.parse().ok()
}

fn parse_zsh_extended(line: &str) -> Option<(i64, String)> {
    let rest = line.strip_prefix(": ")?;
    let semicolon = rest.find(';')?;
    let (header, command) = rest.split_at(semicolon);
    let command = &command[1..];

    let colon = header.find(':')?;
    let (start, elapsed) = header.split_at(colon);
    let elapsed = &elapsed[1..];

    if start.is_empty() || !start.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if elapsed.is_empty() || !elapsed.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }

    let timestamp = start.parse().ok()?;
    Some((timestamp, command.to_string()))
}

/// True if every character of `query` appears in `command` in the same
/// order, case-insensitively, though not necessarily contiguously. This is
/// a superset of substring matching: it also catches things like `gco`
/// matching `git checkout` or `dcupd` matching `docker compose up -d`.
fn is_subsequence_match(command: &str, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }

    let query_lower = query.to_lowercase();
    let command_lower = command.to_lowercase();
    let mut query_chars = query_lower.chars();
    let mut next_query_char = query_chars.next();

    for c in command_lower.chars() {
        if next_query_char == Some(c) {
            next_query_char = query_chars.next();
        }
    }

    next_query_char.is_none()
}

/// Ranks commands whose characters contain `query` as a subsequence, in
/// order but not necessarily contiguous and case-insensitive, by how often
/// they occur, most frequent first, ties broken alphabetically for stable
/// output.
pub fn rank_by_query(entries: &[HistoryEntry], query: &str, limit: usize) -> Vec<(usize, String)> {
    use std::collections::HashMap;

    let mut counts: HashMap<&str, usize> = HashMap::new();
    for entry in entries {
        if is_subsequence_match(&entry.command, query) {
            *counts.entry(entry.command.as_str()).or_insert(0) += 1;
        }
    }

    let mut ranked: Vec<(usize, String)> = counts
        .into_iter()
        .map(|(command, count)| (count, command.to_string()))
        .collect();
    ranked.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    ranked.truncate(limit);
    ranked
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Case {
        name: &'static str,
        input: &'static str,
        expected: &'static [(&'static str, Option<i64>)],
    }

    #[test]
    fn parses_awkward_history_formats() {
        let cases = [
            Case {
                name: "empty file",
                input: "",
                expected: &[],
            },
            Case {
                name: "plain bash history, no timestamps",
                input: "ls -la\ncd /tmp\n",
                expected: &[("ls -la", None), ("cd /tmp", None)],
            },
            Case {
                name: "zsh extended, single line",
                input: ": 1690000000:0;ls -la\n",
                expected: &[("ls -la", Some(1690000000))],
            },
            Case {
                name: "zsh extended command containing a literal semicolon",
                input: ": 1690000000:0;echo a; echo b\n",
                expected: &[("echo a; echo b", Some(1690000000))],
            },
            Case {
                name: "zsh extended multiline command",
                input: ": 1690000000:0;echo one \\\necho two\n",
                expected: &[("echo one \necho two", Some(1690000000))],
            },
            Case {
                name: "bash HISTTIMEFORMAT comment lines pair with the next command",
                input: "#1690000000\nls -la\n#1690000100\ncd /tmp\n",
                expected: &[("ls -la", Some(1690000000)), ("cd /tmp", Some(1690000100))],
            },
            Case {
                name: "blank lines between commands are ignored",
                input: "ls -la\n\n\ncd /tmp\n",
                expected: &[("ls -la", None), ("cd /tmp", None)],
            },
            Case {
                name: "duplicate consecutive commands are kept as separate entries",
                input: "ls -la\nls -la\n",
                expected: &[("ls -la", None), ("ls -la", None)],
            },
            Case {
                name: "a line that looks like a zsh header but isn't falls back to plain",
                input: ": not-a-real-header\n",
                expected: &[(": not-a-real-header", None)],
            },
            Case {
                name: "trailing backslash at end of file with nothing to continue into",
                input: "echo done\\",
                expected: &[("echo done", None)],
            },
        ];

        for case in cases {
            let got = parse(case.input);
            let got: Vec<(&str, Option<i64>)> =
                got.iter().map(|e| (e.command.as_str(), e.timestamp)).collect();
            assert_eq!(got, case.expected, "case failed: {}", case.name);
        }
    }

    #[test]
    fn ranks_by_frequency_then_alphabetically() {
        let entries = parse("docker ps\ndocker ps\ndocker exec -it web bash\ndocker ps\n");
        let ranked = rank_by_query(&entries, "docker", 10);
        assert_eq!(
            ranked,
            vec![
                (3, "docker ps".to_string()),
                (1, "docker exec -it web bash".to_string()),
            ]
        );
    }

    #[test]
    fn rank_by_query_ignores_non_matching_commands() {
        let entries = parse("ls -la\ndocker ps\n");
        let ranked = rank_by_query(&entries, "docker", 10);
        assert_eq!(ranked, vec![(1, "docker ps".to_string())]);
    }

    #[test]
    fn subsequence_matching_is_case_insensitive_and_order_sensitive() {
        assert!(is_subsequence_match("docker ps", "docker"));
        assert!(is_subsequence_match("docker ps", "DOCKER"));
        assert!(is_subsequence_match("git checkout main", "gco"));
        assert!(is_subsequence_match("docker compose up -d", "dcupd"));
        assert!(is_subsequence_match("anything", ""));

        // characters present but out of order should not match
        assert!(!is_subsequence_match("docker ps", "sp"));
        // query longer than the command can never match
        assert!(!is_subsequence_match("ls", "lsla"));
    }

    #[test]
    fn rank_by_query_matches_fuzzy_subsequences() {
        let entries = parse("git checkout main\ngit checkout main\nls -la\n");
        let ranked = rank_by_query(&entries, "gco", 10);
        assert_eq!(ranked, vec![(2, "git checkout main".to_string())]);
    }
}
