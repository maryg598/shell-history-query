mod history;

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

struct Args {
    query: String,
    file: Option<PathBuf>,
    limit: usize,
}

fn usage() -> String {
    "usage: shq <query> [--file PATH] [--limit N]".to_string()
}

fn parse_args() -> Result<Args, String> {
    let mut query: Option<String> = None;
    let mut file: Option<PathBuf> = None;
    let mut limit = 20usize;

    let mut it = env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--file" => {
                let value = it.next().ok_or("--file needs a value")?;
                file = Some(PathBuf::from(value));
            }
            "--limit" => {
                let value = it.next().ok_or("--limit needs a value")?;
                limit = value
                    .parse::<usize>()
                    .map_err(|_| "--limit must be a positive number".to_string())?;
            }
            "-h" | "--help" => return Err(usage()),
            other if query.is_none() => query = Some(other.to_string()),
            other => return Err(format!("unexpected argument: {other}")),
        }
    }

    let query = query.ok_or_else(usage)?;
    Ok(Args { query, file, limit })
}

// $HISTFILE is what the running shell actually uses; the dotfile fallbacks
// only matter when shq is invoked from a context that never sourced it.
fn default_history_path() -> Option<PathBuf> {
    if let Ok(path) = env::var("HISTFILE") {
        if !path.is_empty() {
            return Some(PathBuf::from(path));
        }
    }

    let home = env::var("HOME").ok()?;
    for name in [".zsh_history", ".bash_history"] {
        let candidate = PathBuf::from(&home).join(name);
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(args) => args,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::FAILURE;
        }
    };

    let path = match args.file.or_else(default_history_path) {
        Some(path) => path,
        None => {
            eprintln!("could not find a history file; set HISTFILE or pass --file");
            return ExitCode::FAILURE;
        }
    };

    // History files sometimes contain non-UTF8 bytes (a pasted binary blob,
    // a stray control character) — lossy conversion beats refusing to run.
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(err) => {
            eprintln!("failed to read {}: {}", path.display(), err);
            return ExitCode::FAILURE;
        }
    };
    let content = String::from_utf8_lossy(&bytes);

    let entries = history::parse(&content);
    let ranked = history::rank_by_query(&entries, &args.query, args.limit);

    if ranked.is_empty() {
        eprintln!("no commands matched {:?}", args.query);
        return ExitCode::FAILURE;
    }

    for (count, command) in ranked {
        println!("{count}\t{command}");
    }

    ExitCode::SUCCESS
}
