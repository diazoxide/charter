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
//!   them. The file is an absolute path, so it can never be read as an option, and it is
//!   handed on as the operating system spelled it, so a name that is not UTF-8 reaches the
//!   editor unchanged (#1044).
//!
//! **The file is always one [`crate::files::in_your_editor`] checked**: inside the piece,
//! and one the light editor's file list offers. This module takes it as already checked.

use std::ffi::OsString;
use std::path::Path;
use std::time::Duration;

#[cfg(not(unix))]
use percent_encoding::utf8_percent_encode;
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC};

/// Which editor the operator chose (Settings, ADR 0081 §3).
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
    /// A program and its arguments, spawned directly. Never a shell command line. The
    /// arguments are the operating system's strings, so the file's path is never rewritten.
    Program {
        program: String,
        args: Vec<OsString>,
    },
}

/// Why nothing was launched, as the sentence the window shows.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NotLaunched {
    #[error("there is no line {0}: lines are counted from 1")]
    NoLine(u32),
    #[error(
        "neither $VISUAL nor $EDITOR is set where purlis was started: set one, or choose \
         VS Code, Zed or a JetBrains IDE in Settings"
    )]
    NoVariable,
    #[error("${name} is not a command purlis can split into words: {why}")]
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
    let encoded = encoded(file);
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
            let mut args: Vec<OsString> = words.into_iter().map(OsString::from).collect();
            args.push(format!("+{line}").into());
            args.push(file.as_os_str().to_owned());
            Launch::Program { program, args }
        }
    })
}

/// `file`'s path percent-encoded for a URL: its bytes as the operating system holds them, so a
/// name that is not UTF-8 is encoded byte for byte rather than replaced.
#[cfg(unix)]
fn encoded(file: &Path) -> String {
    use std::os::unix::ffi::OsStrExt as _;
    percent_encoding::percent_encode(file.as_os_str().as_bytes(), IN_A_PATH).to_string()
}

/// `file`'s path percent-encoded for a URL. Off Unix a path is UTF-16, and a URL's is UTF-8.
#[cfg(not(unix))]
fn encoded(file: &Path) -> String {
    utf8_percent_encode(&file.to_string_lossy(), IN_A_PATH).to_string()
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

/// How long [`start`] waits to hear whether the editor exited at once. A terminal editor with
/// no terminal reads the end of its input and exits within this; a windowed editor is still
/// running, or (as `gvim` and `emacsclient -n` do) has handed the file on and exited `0`.
pub const AT_ONCE: Duration = Duration::from_secs(1);

/// Starts `program` with `args`, and does not wait for it to finish: an editor stays open for
/// as long as the operator keeps it.
///
/// **No shell.** The program is looked up as charter looks up a harness
/// ([`crate::programs::resolve`]: the app's `PATH`, then the fixed directories a
/// double-clicked app lacks) and started through the fork lock ([`crate::forklock::spawn`]),
/// with nothing on its standard input, output or error. A thread waits for it, so it leaves no
/// zombie behind.
///
/// **An editor that fails at once is said, not dropped** (#1044). A terminal editor (vi, nano,
/// `emacs -nw`) has no terminal here, reads the end of its input and exits. So this waits up
/// to [`AT_ONCE`]: an exit that is not a success within it is refused with a sentence; one
/// still running then, or one that exited `0`, is an editor that opened.
pub fn start(program: &str, args: &[OsString]) -> Result<(), String> {
    let found = crate::programs::resolve(program).map_err(|missing| missing.said())?;
    let mut command = std::process::Command::new(&found);
    command
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    let mut child = crate::forklock::spawn(&mut command)
        .map_err(|e| format!("purlis could not start {program}: {e}"))?;
    let (exited, heard) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("your-editor".into())
        .spawn(move || {
            let _ = exited.send(child.wait());
        })
        .map_err(|e| format!("purlis could not wait for {program}: {e}"))?;
    match heard.recv_timeout(AT_ONCE) {
        Ok(Ok(status)) if !status.success() => Err(exited_at_once(program, status)),
        _ => Ok(()),
    }
}

/// What the window says of an editor that exited at once, and not with success.
fn exited_at_once(program: &str, status: std::process::ExitStatus) -> String {
    let how = match status.code() {
        Some(code) => format!("with code {code}"),
        None => "on a signal".to_string(),
    };
    format!(
        "{program} exited at once, {how}: a terminal editor needs a terminal, and purlis \
         starts your editor without one. Set $VISUAL to an editor that opens its own window, \
         or choose VS Code, Zed or a JetBrains IDE in Settings"
    )
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt as _;

    /// A path whose last name is not UTF-8: `caf\xe9.md`, as Latin-1 spells it.
    fn not_utf8() -> &'static Path {
        Path::new(OsStr::from_bytes(b"/work/piece/caf\xe9.md"))
    }

    #[test]
    fn a_path_that_is_not_utf8_reaches_the_variables_editor_unchanged() {
        let var = |name: &str| (name == "EDITOR").then(|| "gvim".to_string());

        let launched = launch(Editor::Variable, not_utf8(), 3, &var).unwrap();

        assert_eq!(
            launched,
            Launch::Program {
                program: "gvim".to_string(),
                args: vec![OsString::from("+3"), not_utf8().as_os_str().to_owned()],
            }
        );
    }

    #[test]
    fn a_path_that_is_not_utf8_is_percent_encoded_byte_for_byte_in_a_url() {
        let launched = launch(Editor::VsCode, not_utf8(), 3, &|_| None).unwrap();

        assert_eq!(
            launched,
            Launch::Url("vscode://file/work/piece/caf%E9.md:3:1".to_string())
        );
    }
}
