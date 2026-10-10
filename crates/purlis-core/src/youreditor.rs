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
/// with nothing on its standard input or output. A thread waits for it, so it leaves no zombie
/// behind.
///
/// **An editor that fails at once is said, not dropped** (#1044). A terminal editor (vi, nano,
/// `emacs -nw`) has no terminal here, reads the end of its input and exits. So this waits up
/// to [`AT_ONCE`]: an exit that is not a success within it is refused with a sentence; one
/// still running then, or one that exited `0`, is an editor that opened. **The sentence says
/// what the editor printed** when it printed something ([`SAID_MOST`] bytes of its standard
/// error, its first line), so a windowed editor that failed for its own reason (`emacsclient`
/// with no server) is not told it needs a terminal.
///
/// **What the editor prints can never break it** (#1044). On Unix its standard error is a file
/// with no name: made owner-only and new in charter's own directory of the config home
/// ([`crate::machine::dir`], which no chat may write), and unlinked before the editor starts.
/// It is read only when the editor exited at once; otherwise it is emptied and let go. So an
/// editor that outlives the app goes on writing there, where a pipe whose reader quit would
/// answer with `EPIPE` or kill it with `SIGPIPE`; what it writes later holds disk space until it
/// exits, which for a windowed editor is little or nothing. With no such directory, and
/// off Unix, it is a pipe read on a thread of its own for as long as the editor keeps it open,
/// and past what is kept it is read and dropped, so an editor that writes a lot there is never
/// held up by a full pipe.
pub fn start(program: &str, args: &[OsString]) -> Result<(), String> {
    start_saying_in(program, args, said_dir().as_deref())
}

/// [`start`], with what the editor prints kept in an unlinked file made in `private` when there
/// is one, else read from a pipe.
fn start_saying_in(program: &str, args: &[OsString], private: Option<&Path>) -> Result<(), String> {
    let found = crate::programs::resolve(program).map_err(|missing| missing.said())?;
    let mut command = std::process::Command::new(&found);
    command
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null());
    let unnamed = private.and_then(unnamed_in);
    let said = match unnamed {
        Some((kept, given)) => {
            command.stderr(given);
            Some(Said::Unnamed(kept))
        }
        None => {
            command.stderr(std::process::Stdio::piped());
            None
        }
    };
    let mut child = crate::forklock::spawn(&mut command)
        .map_err(|e| format!("purlis could not start {program}: {e}"))?;
    // The editor holds its own copy now; this process keeps only the one it reads.
    drop(command);
    let said = said.or_else(|| child.stderr.take().map(|piped| Said::Piped(listen(piped))));
    let (exited, heard) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("your-editor".into())
        .spawn(move || {
            let _ = exited.send(child.wait());
        })
        .map_err(|e| format!("purlis could not wait for {program}: {e}"))?;
    let began = std::time::Instant::now();
    match heard.recv_timeout(AT_ONCE) {
        Ok(Ok(status)) if !status.success() => {
            // What it printed before it exited; a child it left holding its standard error
            // open is waited for no longer than what is left of the moment.
            let printed = said
                .map(|said| said.by(AT_ONCE.saturating_sub(began.elapsed()).max(SAID_GRACE)))
                .unwrap_or_default();
            Err(exited_at_once(program, status, &printed))
        }
        _ => {
            if let Some(said) = said {
                said.let_go();
            }
            Ok(())
        }
    }
}

/// The most of an editor's standard error kept to say why it failed: 2 KiB. The rest is read
/// and dropped.
pub const SAID_MOST: usize = 2048;

/// The most chars of what an editor printed that the sentence carries.
const SAID_CHARS: usize = 300;

/// How long, at least, an editor that exited is given for what it printed to arrive.
const SAID_GRACE: Duration = Duration::from_millis(100);

/// Where an editor's standard error goes, to be read if it exits at once.
enum Said {
    /// A file with no name, which the editor holds as its standard error (Unix).
    Unnamed(std::fs::File),
    /// A pipe, read on a thread of its own.
    Piped(Listening),
}

impl Said {
    /// What the editor printed, up to [`SAID_MOST`] bytes. A pipe is waited on until the editor
    /// closes it or `within` passes; a file holds already all that an exited editor wrote.
    fn by(self, within: Duration) -> Vec<u8> {
        match self {
            Said::Unnamed(file) => read_from_the_start(&file),
            Said::Piped(listening) => listening.by(within),
        }
    }

    /// The editor opened: nothing it prints is read. A file is emptied, so what it printed in
    /// its first moment is not kept for as long as it runs; a pipe's thread goes on draining it.
    fn let_go(self) {
        if let Said::Unnamed(file) = self {
            let _ = file.set_len(0);
        }
    }
}

/// The first [`SAID_MOST`] bytes of `file`, from its start whatever its offset.
#[cfg(unix)]
fn read_from_the_start(file: &std::fs::File) -> Vec<u8> {
    use std::os::unix::fs::FileExt as _;
    let mut kept = vec![0u8; SAID_MOST];
    let mut got = 0;
    while got < kept.len() {
        match file.read_at(&mut kept[got..], got as u64) {
            Ok(0) => break,
            Ok(n) => got += n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => break,
        }
    }
    kept.truncate(got);
    kept
}

/// Off Unix no file is made ([`unnamed_in`]), so there is nothing to read.
#[cfg(not(unix))]
fn read_from_the_start(_file: &std::fs::File) -> Vec<u8> {
    Vec::new()
}

/// charter's own directory of the config home, where the editor's unnamed file is made: one no
/// chat may write, so none can place a file there or hold the one made open. `None` when there
/// is no config home, or it is not a real directory yet (this does not make it), and in a
/// fenced (test) build whose config home is outside the fence.
fn said_dir() -> Option<std::path::PathBuf> {
    let config = crate::machine::config_root_unheld()?;
    if crate::fence::FENCED && !crate::fence::inside(&config, &crate::fence::fence()) {
        return None;
    }
    let dir = crate::machine::dir(&config);
    std::fs::symlink_metadata(&dir)
        .ok()
        .filter(std::fs::Metadata::is_dir)
        .map(|_| dir)
}

/// A file made new and owner-only in `dir`, appended to, and unlinked at once: the copy this
/// process reads, and the one handed to the editor. `None` when it cannot be made or unlinked,
/// and the pipe is used instead.
#[cfg(unix)]
fn unnamed_in(dir: &Path) -> Option<(std::fs::File, std::process::Stdio)> {
    use std::os::unix::fs::OpenOptionsExt as _;
    // A name taken, by a crash's leftover or by chance, is drawn again; a few times is plenty.
    for _ in 0..4 {
        let mut bytes = [0u8; 12];
        getrandom::fill(&mut bytes).ok()?;
        let name: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
        let path = dir.join(format!(".editor-said-{name}"));
        let made = std::fs::OpenOptions::new()
            .read(true)
            .append(true)
            .create_new(true)
            .mode(0o600)
            .open(&path);
        let file = match made {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => return None,
        };
        // Unlinked before the editor starts: a file that kept its name is not used.
        std::fs::remove_file(&path).ok()?;
        let given = file.try_clone().ok()?;
        return Some((file, given.into()));
    }
    None
}

/// Off Unix a file open by a running program cannot be unlinked, so the pipe is used (D-1044-1).
#[cfg(not(unix))]
fn unnamed_in(_dir: &Path) -> Option<(std::fs::File, std::process::Stdio)> {
    None
}

/// What an editor prints on its standard error, kept up to [`SAID_MOST`] bytes by a thread that
/// reads it until the editor closes it.
struct Listening {
    kept: std::sync::Arc<std::sync::Mutex<Vec<u8>>>,
    closed: std::sync::mpsc::Receiver<()>,
}

impl Listening {
    /// What was kept, once the editor closed its standard error or `within` passed.
    fn by(self, within: Duration) -> Vec<u8> {
        let _ = self.closed.recv_timeout(within);
        self.kept
            .lock()
            .map(|kept| kept.clone())
            .unwrap_or_else(|poisoned| poisoned.into_inner().clone())
    }
}

/// Reads `stderr` on a thread of its own until it closes, keeping its first [`SAID_MOST`] bytes.
fn listen(mut stderr: impl std::io::Read + Send + 'static) -> Listening {
    let kept = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let (closed, heard) = std::sync::mpsc::channel();
    let keeping = kept.clone();
    // A thread that could not start drops the pipe and the sender with it: nothing is said.
    let _ = std::thread::Builder::new()
        .name("your-editor-said".into())
        .spawn(move || {
            let mut buf = [0u8; 4096];
            loop {
                match stderr.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        if let Ok(mut kept) = keeping.lock() {
                            let room = SAID_MOST.saturating_sub(kept.len());
                            kept.extend_from_slice(&buf[..n.min(room)]);
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                    Err(_) => break,
                }
            }
            let _ = closed.send(());
        });
    Listening {
        kept,
        closed: heard,
    }
}

/// The first line of what an editor printed, as the window shows it: read as UTF-8 where it is
/// not, with no control characters, trimmed, and at most [`SAID_CHARS`] chars. Nothing when it
/// printed nothing but space.
fn first_line(printed: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(printed);
    let line = text.lines().map(str::trim).find(|line| !line.is_empty())?;
    let clean: String = line.chars().filter(|c| !c.is_control()).collect();
    let clean = clean.trim();
    if clean.is_empty() {
        return None;
    }
    let mut said: String = clean.chars().take(SAID_CHARS).collect();
    if clean.chars().count() > SAID_CHARS {
        said.push('…');
    }
    Some(said)
}

/// What the window says of an editor that exited at once, and not with success: what it printed,
/// when it printed something, else the likeliest cause, a terminal editor with no terminal.
fn exited_at_once(program: &str, status: std::process::ExitStatus, printed: &[u8]) -> String {
    let how = match status.code() {
        Some(code) => format!("with code {code}"),
        None => "on a signal".to_string(),
    };
    match first_line(printed) {
        Some(said) => format!("{program} exited at once, {how}, and said: {said}"),
        None => format!(
            "{program} exited at once, {how}: a terminal editor needs a terminal, and purlis \
             starts your editor without one. Set $VISUAL to an editor that opens its own \
             window, or choose VS Code, Zed or a JetBrains IDE in Settings"
        ),
    }
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

    fn sh(script: &str) -> Result<(), String> {
        start("/bin/sh", &[OsString::from("-c"), OsString::from(script)])
    }

    /// #1044: a windowed editor that fails for its own reason is said by what it printed, not
    /// told it needs a terminal.
    #[test]
    fn an_editor_that_failed_at_once_is_said_by_what_it_printed() {
        let said =
            sh("printf '\\nemacsclient: can'\\''t find socket\\nmore\\n' >&2; exit 1").unwrap_err();

        assert_eq!(
            said,
            "/bin/sh exited at once, with code 1, and said: emacsclient: can't find socket"
        );
    }

    /// One that printed nothing keeps the likeliest cause.
    #[test]
    fn an_editor_that_failed_at_once_saying_nothing_is_told_it_needs_a_terminal() {
        let said = sh("exit 1").unwrap_err();

        assert!(
            said.contains("a terminal editor needs a terminal"),
            "{said}"
        );
    }

    /// An editor that writes more than is kept is never held up by a full pipe: its exit is
    /// still heard at once.
    #[test]
    fn an_editor_that_prints_a_lot_is_not_held_up() {
        let said = sh("head -c 300000 /dev/zero | tr '\\0' x >&2; exit 3").unwrap_err();

        assert!(said.contains("with code 3, and said: xxx"), "{said}");
        assert!(said.ends_with("x…"), "{said}");
    }

    /// [`start`] with its standard error an unnamed file made in `dir`.
    fn sh_in(dir: &Path, script: &str) -> Result<(), String> {
        start_saying_in(
            "/bin/sh",
            &[OsString::from("-c"), OsString::from(script)],
            Some(dir),
        )
    }

    /// What `dir` holds by name.
    fn names_in(dir: &Path) -> Vec<OsString> {
        std::fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect()
    }

    /// #1044: an editor that outlives what read its standard error (the app quit) and writes
    /// there then is not killed by `SIGPIPE`: it writes to a file with no name, not a pipe.
    #[test]
    fn an_editor_that_writes_after_nothing_reads_it_is_not_killed() {
        let dir = tempfile::tempdir().unwrap();
        let alive = dir.path().join("alive");
        let script = format!(
            "if [ -p /dev/fd/2 ]; then held=pipe; else held=file; fi; sleep 2; \
             echo late >&2; echo late again >&2; echo \"$held\" > '{}'",
            alive.display()
        );

        sh_in(dir.path(), &script).unwrap();

        let deadline = std::time::Instant::now() + Duration::from_secs(15);
        while !alive.exists() && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(50));
        }
        assert_eq!(
            std::fs::read_to_string(&alive).expect("the editor lived past its late writes"),
            "file\n",
            "its standard error is a file, never a pipe whose reader can go"
        );
    }

    /// The file has no name by the time the editor runs, whether it opened or failed.
    #[test]
    fn the_editors_file_is_unlinked_before_it_starts() {
        let dir = tempfile::tempdir().unwrap();
        let seen = tempfile::tempdir().unwrap();
        let listed = seen.path().join("listed");
        let script = format!("ls -A '{}' > '{}'", dir.path().display(), listed.display());

        sh_in(dir.path(), &script).unwrap();
        let _ = sh_in(dir.path(), "exit 1").unwrap_err();

        assert_eq!(std::fs::read_to_string(&listed).unwrap(), "");
        assert_eq!(names_in(dir.path()), Vec::<OsString>::new());
    }

    /// What an editor printed in the unnamed file is said as from a pipe.
    #[test]
    fn an_editor_that_failed_at_once_is_said_by_what_it_wrote_to_its_file() {
        let dir = tempfile::tempdir().unwrap();

        let said = sh_in(
            dir.path(),
            "printf '\\nemacsclient: can'\\''t find socket\\nmore\\n' >&2; exit 1",
        )
        .unwrap_err();

        assert_eq!(
            said,
            "/bin/sh exited at once, with code 1, and said: emacsclient: can't find socket"
        );
        let said = sh_in(dir.path(), "exit 1").unwrap_err();
        assert!(
            said.contains("a terminal editor needs a terminal"),
            "{said}"
        );
    }

    /// Only the first [`SAID_MOST`] bytes of the file are read.
    #[test]
    fn an_editor_that_wrote_a_lot_to_its_file_is_said_by_its_start() {
        let dir = tempfile::tempdir().unwrap();

        let said = sh_in(
            dir.path(),
            "head -c 300000 /dev/zero | tr '\\0' x >&2; exit 3",
        )
        .unwrap_err();

        assert!(said.contains("with code 3, and said: xxx"), "{said}");
        assert!(said.ends_with("x…"), "{said}");
    }

    #[test]
    fn what_an_editor_printed_is_one_clean_line() {
        assert_eq!(first_line(b""), None);
        assert_eq!(first_line(b"  \n\t\n"), None);
        assert_eq!(
            first_line(b"\x1b[31mno server\x07\r\nnext"),
            Some("[31mno server".to_string())
        );
        assert_eq!(first_line(b"caf\xe9"), Some("caf\u{FFFD}".to_string()));
        let long = "y".repeat(SAID_CHARS + 5);
        assert_eq!(
            first_line(long.as_bytes()),
            Some(format!("{}…", "y".repeat(SAID_CHARS)))
        );
    }
}
