//! **Open in your editor at a line** (RC-20, ADR 0081 §3, W7, R12): how charter hands a file
//! and a line to the editor the operator already uses.
//!
//! **This module says what to launch, and starts a program; it opens no URL.** The core never
//! depends on Tauri, so a [`Launch::Url`] goes to the operating system's opener from the app
//! (Tauri's opener plugin, as a persona's file is opened). A [`Launch::Program`] is started
//! here, by [`start`], as a program with its arguments.
//!
//! - **VS Code, Zed and a JetBrains IDE through their own URL schemes** (`vscode://`, `zed://`,
//!   `idea://`), so nothing has to be installed in the editor and no program of charter's
//!   choosing runs: the operating system hands the URL to whichever app registered the scheme.
//! - **`$VISUAL`, else `$EDITOR`, with `+line`**, the convention vi, Emacs, nano, gedit and Kate
//!   share. The variable is split into words the way a shell would split it, and **nothing
//!   else a shell does happens**: no expansion, no `;`, no redirection. The words are the
//!   program and its first arguments, and `+line` and the file are two more arguments after
//!   them. The file is an absolute path, so it can never be read as an option.
//!
//! **The file is always one [`crate::piecefiles::in_your_editor`] checked**: inside the piece,
//! and one the light editor's file list offers. This module takes it as already checked.

use std::path::Path;

use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};

/// Which editor the operator chose (Preferences, ADR 0081 §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Editor {
    /// Visual Studio Code, by `vscode://file/<path>:<line>:<column>`.
    VsCode,
    /// Zed, by `zed://file/<path>:<line>:<column>`.
    Zed,
    /// A JetBrains IDE, by `idea://open?file=<path>&line=<line>`.
    Idea,
    /// `$VISUAL`, else `$EDITOR`, run as `<program> [its words] +<line> <file>`.
    Variable,
}

/// What the app launches to open the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Launch {
    /// A URL for the operating system's opener (`open` on macOS, `xdg-open` on Linux).
    Url(String),
    /// A program and its arguments, spawned directly. Never a shell command line.
    Program { program: String, args: Vec<String> },
}

/// Why nothing was launched, as the sentence the window shows.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NotLaunched {
    #[error("there is no line {0}: lines are counted from 1")]
    NoLine(u32),
    #[error(
        "neither $VISUAL nor $EDITOR is set where charter was started: set one, or choose \
         VS Code, Zed or a JetBrains IDE in Preferences"
    )]
    NoVariable,
    #[error("${name} is not a command charter can split into words: {why}")]
    Unsplittable { name: &'static str, why: String },
}

/// What a path keeps as it is in a URL: letters, digits, `-`, `.`, `_`, `~` and `/`. Every
/// other byte is percent-encoded, so a `#`, `?`, `&`, `%` or space in a file's name cannot
/// end the path or start a query.
const IN_A_PATH: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~')
    .remove(b'/');

/// What to launch to open `file` at `line` in `editor`. `file` is absolute and already
/// checked; `var` reads an environment variable (the app passes its own environment).
pub fn launch(
    editor: Editor,
    file: &Path,
    line: u32,
    var: &dyn Fn(&str) -> Option<String>,
) -> Result<Launch, NotLaunched> {
    if line == 0 {
        return Err(NotLaunched::NoLine(line));
    }
    let shown = file.to_string_lossy();
    let encoded = utf8_percent_encode(&shown, IN_A_PATH);
    Ok(match editor {
        Editor::VsCode => Launch::Url(format!("vscode://file{encoded}:{line}:1")),
        Editor::Zed => Launch::Url(format!("zed://file{encoded}:{line}:1")),
        Editor::Idea => Launch::Url(format!("idea://open?file={encoded}&line={line}")),
        Editor::Variable => {
            let (name, words) = command(var)?;
            let mut words = shell_words::split(&words).map_err(|e| NotLaunched::Unsplittable {
                name,
                why: e.to_string(),
            })?;
            if words.is_empty() {
                return Err(NotLaunched::NoVariable);
            }
            let program = words.remove(0);
            words.push(format!("+{line}"));
            words.push(shown.into_owned());
            Launch::Program {
                program,
                args: words,
            }
        }
    })
}

/// `$VISUAL` when it says something, else `$EDITOR`: which one, and what it says.
fn command(var: &dyn Fn(&str) -> Option<String>) -> Result<(&'static str, String), NotLaunched> {
    ["VISUAL", "EDITOR"]
        .into_iter()
        .find_map(|name| {
            var(name)
                .filter(|said| !said.trim().is_empty())
                .map(|said| (name, said))
        })
        .ok_or(NotLaunched::NoVariable)
}

/// Starts `program` with `args`, and does not wait for it: an editor stays open for as long as
/// the operator keeps it.
///
/// **No shell, and nothing to answer.** The program is looked up as charter looks up a
/// harness ([`crate::programs::resolve`]: the app's `PATH`, then the fixed directories a
/// double-clicked app lacks) and started through the fork lock ([`crate::forklock::spawn`]),
/// with nothing on its standard input, output or error: a terminal editor, which needs a
/// terminal, reads the end of its input and exits rather than waiting unseen. A thread waits
/// for it, so it leaves no zombie behind.
pub fn start(program: &str, args: &[String]) -> Result<(), String> {
    let found = crate::programs::resolve(program).map_err(|missing| missing.said())?;
    let mut command = std::process::Command::new(&found);
    command
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    let mut child = crate::forklock::spawn(&mut command)
        .map_err(|e| format!("charter could not start {program}: {e}"))?;
    std::thread::Builder::new()
        .name("your-editor".into())
        .spawn(move || {
            let _ = child.wait();
        })
        .map_err(|e| format!("charter could not wait for {program}: {e}"))?;
    Ok(())
}
