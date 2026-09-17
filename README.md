# shq

Shell history accumulates thousands of lines in whatever format the shell
that wrote them happens to use. zsh's extended history wraps every command
in `: <start>:<elapsed>;` and escapes embedded newlines with a trailing
backslash. bash either writes plain lines or, if `HISTTIMEFORMAT` is set,
precedes each command with a `#<timestamp>` comment line. Multiline commands
get mangled by anything that treats the file as one-command-per-line.

`fzf`-style reverse search shows recency, not frequency. When you're trying
to remember "how did I invoke that thing a few weeks ago", the answer is
usually the one you ran the most, not the one you ran last. `shq` reads your
history file, parses whichever format it's actually in, and ranks matching
commands by how often you've run them.

## usage

```
$ shq docker
14  docker compose up -d
9   docker exec -it web bash
3   docker system prune -af
```

By default it reads `$HISTFILE`, falling back to `~/.zsh_history` or
`~/.bash_history`. Point it at a specific file or cap the output:

```
$ shq "git rebase" --file ~/.bash_history --limit 5
```

## building

Standard library only, no dependencies to fetch:

```
$ cargo build --release
```

## status

Parsing and frequency ranking both work today. Not done yet: fuzzy matching
(currently plain substring), a `--since` date filter, and skipping lines
that look like they contain a pasted secret. The parser's test suite in
`src/history.rs` lists the history-format edge cases it already handles.

## license

MIT, see LICENSE.
