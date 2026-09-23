//! `getpass.getpass`: a value typed at the terminal without it being echoed.
//!
//! `secret set` with no `--stdin`, `--from-file` or `--value` and a terminal on stdin asks for
//! the value this way — the one path on which a person, not a pipe, supplies it.

use std::io::{BufRead, Write};

/// Print `prompt` on stderr, read one line from stdin with echo off, and restore the terminal.
/// The trailing newline is not part of the value.
pub fn read_hidden(prompt: &str) -> std::io::Result<String> {
    let mut err = std::io::stderr();
    err.write_all(prompt.as_bytes())?;
    err.flush()?;
    #[cfg(unix)]
    let restore = {
        use rustix::termios::{LocalModes, OptionalActions, tcgetattr, tcsetattr};
        let stdin = std::io::stdin();
        match tcgetattr(&stdin) {
            Ok(before) => {
                let mut quiet = before.clone();
                quiet.local_modes.remove(LocalModes::ECHO);
                let _ = tcsetattr(&stdin, OptionalActions::Now, &quiet);
                Some(before)
            }
            Err(_) => None,
        }
    };
    let mut line = String::new();
    let read = std::io::stdin().lock().read_line(&mut line);
    #[cfg(unix)]
    if let Some(before) = restore {
        let _ = rustix::termios::tcsetattr(
            std::io::stdin(),
            rustix::termios::OptionalActions::Now,
            &before,
        );
    }
    let _ = err.write_all(b"\n");
    read?;
    if line.ends_with('\n') {
        line.pop();
        if line.ends_with('\r') {
            line.pop();
        }
    }
    Ok(line)
}
