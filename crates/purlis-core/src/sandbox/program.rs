//! Whether the program a sandboxed chat starts is the harness its sandbox was compiled for, and
//! is not a file a sandboxed chat could have written (ruling V87g, and the V73d analog for a
//! profile's program).
//!
//! A harness's own sandbox, like Claude Code's `--settings` sandbox object, binds only the
//! harness that reads it: a profile of Claude Code's kind whose program is something else runs
//! with no sandbox at all, and still shows as sandboxed. And a program inside the plane, such as a
//! wrapper script, is code charter runs later outside any sandbox, which a sandboxed chat could
//! have changed. So, before a sandboxed start:
//!
//! - **Never a program where the chat can write**, for every harness: the plane, the chat's own
//!   folder, the temp directories and whatever else the compiled sandbox lets it write. Asked of
//!   the path as written and of its real path, so a link in a writable place, to a program
//!   outside it, is refused too: the chat could rewrite the link. A relative path is refused,
//!   since it would be found against a folder the chat writes. The real path is what then runs,
//!   for the check and for the chat alike. Refused, not denied writing: a wrapper is the
//!   operator's to keep where they like, and the refusal names where it is.
//! - **Nor a command that names a file there** (D-88g). Every other word of the command is held
//!   to the same places, wherever in the word a path begins: an interpreter outside, handed a
//!   script a chat wrote, runs the chat's code. A word that names no file is left alone. This
//!   is a lint that catches mistakes (D-88k). Where charter wraps the whole harness (opencode
//!   and Codex, on macOS), the wrap is the boundary; Claude Code is not wrapped, because
//!   Seatbelt cannot apply its own sandbox inside charter's (#1150).
//! - **Claude Code answers as Claude Code.** Its program and command, asked `--version`, must
//!   answer a line `X.Y.Z (Claude Code)`. A legitimate wrapper outside the plane passes its words
//!   through and answers the same. It is asked with the chat's own environment, in the chat's
//!   folder.
//!
//! **What the answer proves** is that the profile is Claude Code by intent, not by mistake. A
//! program written to answer like Claude Code passes; the profile's program is one a person
//! approved (ADR 0022), and that approval is what this rests on. So does inline code, inline config, and a
//! program outside that reads and runs a file a chat can write without being handed it.

use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

use super::NotStarted;
use crate::harness::Harness;

/// How long a program has to answer `--version`. Generous: a program the system has not seen
/// before is assessed on its first run, and a cold start under `sandbox-exec` was measured past
/// five seconds; a program that does not answer in time is told apart from one that answers
/// wrong ([`NotStarted::ProbeTimedOut`]).
pub const PATIENCE: Duration = Duration::from_secs(10);

/// Why a program gave no answer to `--version` charter could read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unanswered {
    /// It did not finish answering within [`PATIENCE`].
    TimedOut,
    /// It could not be run, or it failed.
    Failed,
}

/// The longest command word a sandboxed start checks; a longer one is refused (D-88k).
pub const WORD_MAX: usize = 4096;

/// The most of an answer that is read.
const ANSWER_MAX: u64 = 4096;

/// What answers `--version` for a program and its command, in place of running it: a test's.
pub type Probe<'a> = dyn Fn(&[String]) -> Option<String> + Sync + 'a;

/// Where a sandboxed chat starts, as the check asks it: the folder it runs in, the folders
/// its sandbox lets it write besides (the temp directories among them, [`temp_roots`]), and
/// its own environment.
#[derive(Debug, Clone, Copy)]
pub struct Chat<'a> {
    pub cwd: &'a Path,
    pub writable: &'a [PathBuf],
    pub env: &'a [(String, String)],
}

/// The temp directories every harness's sandbox may let a chat write, `env` being the chat's.
/// A caller adds these to [`Chat::writable`].
pub fn temp_roots(env: &[(String, String)]) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = ["/tmp", "/private/tmp", "/var/tmp", "/private/var/tmp"]
        .into_iter()
        .map(PathBuf::from)
        .collect();
    roots.push(std::env::temp_dir());
    roots.extend(
        env.iter()
            .filter(|(key, _)| key == "TMPDIR")
            .map(|(_, value)| PathBuf::from(value)),
    );
    roots
}

/// The program `words` start, made the one real file that runs, or why a chat sandboxed for
/// `harness` in the plane at `root` may not start on it. `probe` answers `--version` in place
/// of running it; `None` runs it ([`version_of`]).
pub fn checked(
    harness: Harness,
    words: &[String],
    root: &Path,
    chat: Chat<'_>,
    probe: Option<&Probe<'_>>,
) -> Result<Vec<String>, NotStarted> {
    let Some(written) = words.first() else {
        return Err(NotStarted::ProgramRelative);
    };
    let written = PathBuf::from(written);
    if !written.is_absolute() {
        return Err(NotStarted::ProgramRelative);
    }
    let real = written
        .canonicalize()
        .map_err(|_| NotStarted::ProgramWritable(written.clone()))?;
    let mut places = vec![root.to_path_buf(), chat.cwd.to_path_buf()];
    places.extend(chat.writable.iter().cloned());
    let places: Vec<(PathBuf, PathBuf)> = places
        .into_iter()
        .map(|dir| {
            let real_dir = dir.canonicalize().unwrap_or_else(|_| dir.clone());
            (dir, real_dir)
        })
        .collect();
    if let Some(path) = [&written, &real]
        .into_iter()
        .find(|path| writable(path, &places))
    {
        return Err(NotStarted::ProgramWritable(path.clone()));
    }
    // Ruling D-88g: an interpreter outside, handed a file the chat wrote, runs the chat's code.
    // So every other word is held to the same places, wherever in the word a path begins.
    // Ruling D-88k: this is a lint that catches a mistake, bounded so it stays one; charter's
    // own wrap around the harness, where charter wraps it, is the boundary.
    if words[1..].iter().any(|word| word.len() > WORD_MAX) {
        return Err(NotStarted::WordTooLong);
    }
    let mut lookups = Lookups::default();
    if let Some(word) = words[1..]
        .iter()
        .find(|word| names_a_writable_file(word, chat.cwd, &places, &mut lookups))
    {
        return Err(NotStarted::WordWritable(word.clone()));
    }
    let mut words = words.to_vec();
    words[0] = real.display().to_string();
    if harness == Harness::ClaudeCode {
        let said = match probe {
            Some(probe) => probe(&words).ok_or(Unanswered::Failed),
            None => version_of(&words, chat),
        };
        match said {
            Ok(said) if is_claude_code(&said) => {}
            Err(Unanswered::TimedOut) => return Err(NotStarted::ProbeTimedOut(harness)),
            Ok(_) | Err(Unanswered::Failed) => return Err(NotStarted::NotTheHarness(harness)),
        }
    }
    Ok(words)
}

/// Whether `path` lies in one of `places`, each given as written and as its real path. The one
/// test of "where a chat can write", for a harness here and for a provider's program
/// ([`crate::secrets::program`]).
pub(crate) fn writable(path: &Path, places: &[(PathBuf, PathBuf)]) -> bool {
    places
        .iter()
        .any(|(dir, real_dir)| path.starts_with(dir) || path.starts_with(real_dir))
}

/// Whether a command's word names a file where the chat can write, anywhere in it: after a
/// flag (`--config=/x`, `-I/x`), in a URL (`file:///x`), after `@`, or inside inline code or
/// inline JSON.
///
/// The word is read every way a program might read it: as written, JSON-unescaped, percent-
/// decoded, and each of those two after the other. In every reading:
///
/// - **Every part that begins at a `/`** is asked as an absolute path: as written, with its
///   `.`, `..` and `//` folded by text (as a URL's reader folds them), and resolved through the
///   folders of it that exist (as the system resolves them), so a link, a `..` after a missing
///   folder, or another letter case is seen through.
/// - **Every piece between two characters no file name holds** (whitespace, quotes, brackets,
///   `<>`, `,;:=` and the like), a short flag's attached value (`-Ilib`), and what follows such
///   a character to the word's end, is asked as a relative path against the chat's folder,
///   where it would be found, as written and folded. One that
///   names something there is refused.
///
/// A word that names nothing is no file: a flag, a model's name. This errs toward refusing: a
/// word with a writable place's path inside it, or with a piece that names a file in the chat's
/// folder, is refused even where it means something else (D-88j). It is a lint (D-88k).
fn names_a_writable_file(
    word: &str,
    cwd: &Path,
    places: &[(PathBuf, PathBuf)],
    lookups: &mut Lookups,
) -> bool {
    readings(word).iter().any(|word| {
        word.char_indices()
            .filter(|&(_, c)| c == '/')
            .any(|(at, _)| absolute_writable(Path::new(&word[at..]), places, lookups))
            || relative_parts(word).any(|part| {
                !part.is_empty()
                    && !part.starts_with('/')
                    && (cwd.join(part).symlink_metadata().is_ok()
                        || folded(&cwd.join(part)).symlink_metadata().is_ok())
            })
    })
}

/// The ways a program might read `word`: as written, JSON-unescaped, percent-decoded, and each
/// of those two after the other.
fn readings(word: &str) -> Vec<String> {
    let unescaped = json_unescaped(word);
    let decoded = percent_decoded(word);
    let mut all = vec![
        word.to_owned(),
        percent_decoded(&unescaped),
        json_unescaped(&decoded),
        unescaped,
        decoded,
    ];
    all.sort();
    all.dedup();
    all
}

/// Whether the absolute `path` lies in one of `places`: as written, folded by text, or resolved
/// by the system, before or after the folding.
fn absolute_writable(path: &Path, places: &[(PathBuf, PathBuf)], lookups: &mut Lookups) -> bool {
    let plain = folded(path);
    if writable(path, places) || writable(&plain, places) {
        return true;
    }
    let real = lookups.resolved(path);
    writable(&real, places)
        || writable(&folded(&real), places)
        || writable(&lookups.resolved(&plain), places)
}

/// `path` with `.` and `//` dropped and each `..` taking off the name before it, by text alone.
fn folded(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            other => out.push(other),
        }
    }
    out
}

/// What the system finds for a path, one name at a time, each answer kept: a word's parts share
/// their folders, so each folder is looked up once. The word's parts are still scanned one by
/// one, so the check grows with the square of the word's length, which `WORD_MAX` bounds.
#[derive(Default)]
struct Lookups {
    found: std::collections::HashMap<PathBuf, Option<PathBuf>>,
}

impl Lookups {
    /// The absolute `path` as the system resolves it: each name in turn, a link followed and a
    /// `..` the parent of the real folder before it, until a name that is not there. The rest is
    /// put back after it, as written: the file `path` names, or would name once made.
    fn resolved(&mut self, path: &Path) -> PathBuf {
        let mut real = PathBuf::from("/");
        let mut parts = path.components();
        while let Some(part) = parts.next() {
            match part {
                Component::RootDir | Component::CurDir | Component::Prefix(_) => {}
                Component::ParentDir => {
                    real.pop();
                }
                Component::Normal(name) => {
                    let next = real.join(name);
                    let found = self
                        .found
                        .entry(next.clone())
                        .or_insert_with(|| {
                            next.symlink_metadata().ok()?;
                            next.canonicalize().ok()
                        })
                        .clone();
                    match found {
                        Some(found) => real = found,
                        None => {
                            real.push(name);
                            real.extend(parts);
                            return real;
                        }
                    }
                }
            }
        }
        real
    }
}

/// The parts of `word` a program could take for a relative path.
fn relative_parts(word: &str) -> impl Iterator<Item = &str> {
    let pieces = word.split(|c: char| !in_a_name(c));
    let attached = word
        .split(|c: char| !in_a_name(c))
        .filter(|piece| piece.starts_with('-') && !piece.starts_with("--"))
        .filter_map(|piece| piece.get(2..));
    let after_a_mark = word
        .char_indices()
        .filter(|&(_, c)| !in_a_name(c))
        .map(move |(at, c)| &word[at + c.len_utf8()..]);
    std::iter::once(word)
        .chain(pieces)
        .chain(attached)
        .chain(after_a_mark)
}

/// Whether `c` can be part of a file's name as programs read names off a command line.
fn in_a_name(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '.' | '_' | '~' | '+' | '-' | '/')
}

/// `word` with JSON's string escapes read: `\"`, `\\`, `\/`, `\b`, `\f`, `\n`, `\r`, `\t` and
/// `\uXXXX`, a surrogate pair included. Any other `\` stays as written.
fn json_unescaped(word: &str) -> String {
    let mut out = String::with_capacity(word.len());
    let mut chars = word.chars().peekable();
    let hex4 = |chars: &mut std::iter::Peekable<std::str::Chars<'_>>| -> Option<u32> {
        let digits: String = chars.clone().take(4).collect();
        let code = (digits.len() == 4)
            .then(|| u32::from_str_radix(&digits, 16).ok())
            .flatten()?;
        for _ in 0..4 {
            chars.next();
        }
        Some(code)
    };
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.peek().copied() {
            Some(simple @ ('"' | '\\' | '/')) => {
                chars.next();
                out.push(simple);
            }
            Some(letter @ ('b' | 'f' | 'n' | 'r' | 't')) => {
                chars.next();
                out.push(match letter {
                    'b' => '\u{8}',
                    'f' => '\u{c}',
                    'n' => '\n',
                    'r' => '\r',
                    _ => '\t',
                });
            }
            Some('u') => {
                let mut ahead = chars.clone();
                ahead.next();
                let Some(high) = hex4(&mut ahead) else {
                    out.push(c);
                    continue;
                };
                chars = ahead;
                let code = if (0xD800..0xDC00).contains(&high) {
                    let mut pair = chars.clone();
                    let low = (pair.next() == Some('\\') && pair.next() == Some('u'))
                        .then(|| hex4(&mut pair))
                        .flatten()
                        .filter(|low| (0xDC00..0xE000).contains(low));
                    match low {
                        Some(low) => {
                            chars = pair;
                            0x10000 + ((high - 0xD800) << 10) + (low - 0xDC00)
                        }
                        None => high,
                    }
                } else {
                    high
                };
                out.push(char::from_u32(code).unwrap_or('\u{FFFD}'));
            }
            _ => out.push(c),
        }
    }
    out
}

/// `word` with each `%XX` read as the byte it stands for, as a URL's reader would.
fn percent_decoded(word: &str) -> String {
    let bytes = word.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        let hex = |b: u8| (b as char).to_digit(16);
        if bytes[at] == b'%'
            && let (Some(high), Some(low)) = (
                bytes.get(at + 1).copied().and_then(hex),
                bytes.get(at + 2).copied().and_then(hex),
            )
        {
            out.push((high * 16 + low) as u8);
            at += 3;
        } else {
            out.push(bytes[at]);
            at += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Whether `said` holds a line Claude Code answers `--version` with: `X.Y.Z (Claude Code)`.
pub fn is_claude_code(said: &str) -> bool {
    said.lines().any(|line| {
        let Some(version) = line.trim().strip_suffix(" (Claude Code)") else {
            return false;
        };
        let parts: Vec<&str> = version.split('.').collect();
        parts.len() == 3
            && parts
                .iter()
                .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
    })
}

/// What `words` answer to `--version`, run in `chat`'s folder with `chat`'s environment (and
/// the app's `HOME` and `PATH`, which every chat also gets), within [`PATIENCE`] and at most
/// [`ANSWER_MAX`] bytes, or why there is none.
///
/// **Inside charter's wrap** (D-88k), on macOS: the probe is the first time charter runs the
/// program, so it runs under a profile that lets it write nothing but a temp directory of its
/// own (its `TMPDIR`, removed after) and reach no network ([`super::seatbelt::probe_profile`]).
/// A command that loads a file a chat wrote gets nothing out of the probe.
///
/// The program runs in a process group of its own, and the whole group is killed when it does
/// not answer in time. A child that left the group and holds the answer's pipe open does not
/// hold the start up: the answer is read on a thread of its own, and given up on at the
/// deadline.
pub fn version_of(words: &[String], chat: Chat<'_>) -> Result<String, Unanswered> {
    answer_of(words, chat, &mut |_| {})
}

/// [`version_of`], telling `seen` the id of the probe's process, which leads its process
/// group: a test's way to see that the whole group is gone after.
pub(crate) fn answer_of(
    words: &[String],
    chat: Chat<'_>,
    seen: &mut dyn FnMut(u32),
) -> Result<String, Unanswered> {
    let failed = Unanswered::Failed;
    let (program, rest) = words.split_first().ok_or(failed)?;
    // D-88k: inside charter's own wrap, which writes nothing but its own temp directory and
    // reaches no network. On macOS; the Linux wrap is #1040, and there it runs as before.
    let tmp = tempfile::Builder::new()
        .prefix("charter-probe-")
        .tempdir()
        .map_err(|_| failed)?;
    let mut command = if cfg!(target_os = "macos") {
        let profile = super::seatbelt::probe_profile(tmp.path()).map_err(|_| failed)?;
        let mut wrapped = std::process::Command::new(super::backend::SANDBOX_EXEC);
        wrapped.args(["-p", &profile]).arg(program);
        wrapped
    } else {
        std::process::Command::new(program)
    };
    command
        .args(rest)
        .arg("--version")
        .current_dir(chat.cwd)
        .env_clear()
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    for key in ["HOME", "PATH"] {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    command.envs(chat.env.iter().map(|(key, value)| (key, value)));
    command.env("TMPDIR", tmp.path());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = crate::forklock::spawn(&mut command).map_err(|_| failed)?;
    seen(child.id());
    let stdout = child.stdout.take().ok_or(failed)?;
    let (sender, answer) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut out = Vec::new();
        let _ = stdout.take(ANSWER_MAX).read_to_end(&mut out);
        let _ = sender.send(out);
    });
    let deadline = Instant::now() + PATIENCE;
    // Wait without reaping: while the probe is an unreaped zombie its id stays taken, so
    // the group kill below cannot reach a stranger that was handed the id afterwards.
    while !exited(&mut child) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    let in_time = exited(&mut child);
    // Its own children may still hold the answer's pipe open: the group goes too.
    #[cfg(unix)]
    {
        use rustix::process::{Pid, Signal, kill_process_group};
        let _ = kill_process_group(Pid::from_child(&child), Signal::KILL);
    }
    let _ = child.kill();
    let status = child.wait().map_err(|_| failed)?;
    if !in_time {
        return Err(Unanswered::TimedOut);
    }
    // A child that left the group and kept the pipe cannot hold the start past the deadline.
    let left = deadline.saturating_duration_since(Instant::now());
    let out = answer
        .recv_timeout(left.max(Duration::from_millis(100)))
        .map_err(|_| Unanswered::TimedOut)?;
    status
        .success()
        .then(|| String::from_utf8_lossy(&out).trim().to_owned())
        .ok_or(failed)
}

/// Whether `child` has exited, leaving it unreaped.
#[cfg(unix)]
fn exited(child: &mut std::process::Child) -> bool {
    use rustix::process::{Pid, WaitId, WaitIdOptions, waitid};
    let options = WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT;
    // An error (the child is gone already) counts as exited, so the loop never spins on it.
    !matches!(
        waitid(WaitId::Pid(Pid::from_child(child)), options),
        Ok(None)
    )
}

/// Whether `child` has exited.
#[cfg(not(unix))]
fn exited(child: &mut std::process::Child) -> bool {
    !matches!(child.try_wait(), Ok(None))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    /// A plane, a chat folder in it, and a place outside both that the test counts as not
    /// writable by the chat (the temp roots are the caller's to add, and left out here).
    struct Fixture {
        plane: tempfile::TempDir,
        outside: tempfile::TempDir,
    }

    impl Fixture {
        fn new() -> Self {
            let plane = tempfile::tempdir().expect("a plane");
            std::fs::create_dir_all(plane.path().join("workspaces/w")).expect("a workspace");
            Self {
                plane,
                outside: tempfile::tempdir().expect("outside"),
            }
        }

        fn cwd(&self) -> PathBuf {
            self.plane.path().join("workspaces/w")
        }

        fn check(
            &self,
            harness: Harness,
            program: &Path,
            writable: &[PathBuf],
            probe: Option<&Probe<'_>>,
        ) -> Result<Vec<String>, NotStarted> {
            let cwd = self.cwd();
            checked(
                harness,
                &[program.display().to_string()],
                self.plane.path(),
                Chat {
                    cwd: &cwd,
                    writable,
                    env: &[],
                },
                probe,
            )
        }
    }

    const CLAUDE: &str = "#!/bin/sh\necho '2.1.288 (Claude Code)'\n";

    #[test]
    fn a_claude_code_profile_starts_sandboxed_only_on_claude_code_where_no_chat_writes() {
        let fx = Fixture::new();
        let claude = stand_in::program(fx.outside.path(), "my-claude", CLAUDE);
        let renamed = stand_in::program(
            fx.outside.path(),
            "claude",
            "#!/bin/sh\necho 'codex-cli 0.147.0'\n",
        );
        let real = claude.canonicalize().expect("real");
        // A legitimate wrapper outside, answering as Claude Code: it runs by its real path.
        assert_eq!(
            fx.check(Harness::ClaudeCode, &claude, &[], None),
            Ok(vec![real.display().to_string()])
        );
        // A program named `claude` that is something else: its sandbox would bind nothing.
        assert_eq!(
            fx.check(Harness::ClaudeCode, &renamed, &[], None),
            Err(NotStarted::NotTheHarness(Harness::ClaudeCode))
        );
    }

    #[test]
    fn a_program_where_the_chat_can_write_is_refused_by_its_path_and_by_its_real_path() {
        let fx = Fixture::new();
        let real_outside = stand_in::program(fx.outside.path(), "claude", CLAUDE);
        let refused = |result: Result<Vec<String>, NotStarted>| {
            matches!(result, Err(NotStarted::ProgramWritable(_)))
        };
        // A wrapper in the plane, and one in the chat's own folder.
        let inside = stand_in::program(fx.plane.path(), "claude-wrap", CLAUDE);
        let in_cwd = stand_in::program(&fx.cwd(), "claude-here", CLAUDE);
        // A link in the plane to a real program outside: the chat could rewrite the link.
        let link = fx.cwd().join("claude-link");
        std::os::unix::fs::symlink(&real_outside, &link).expect("a link");
        // A linked folder in the plane, to the folder outside the program is in.
        let dir_link = fx.cwd().join("bin");
        std::os::unix::fs::symlink(fx.outside.path(), &dir_link).expect("a linked folder");
        // A link outside to a program in the plane.
        let elsewhere = tempfile::tempdir().expect("elsewhere");
        let out_link = elsewhere.path().join("claude");
        std::os::unix::fs::symlink(&inside, &out_link).expect("a link out");
        for (what, program) in [
            ("in the plane", inside.clone()),
            ("in the chat's folder", in_cwd),
            ("a link in the plane", link),
            (
                "through a linked folder in the plane",
                dir_link.join("claude"),
            ),
            ("a link outside to the plane", out_link),
        ] {
            for harness in [Harness::ClaudeCode, Harness::Opencode] {
                assert!(
                    refused(fx.check(harness, &program, &[], None)),
                    "{what}, {harness:?}"
                );
            }
        }
        // Anywhere else the sandbox lets the chat write: a temp folder, opencode's own data.
        let writable = tempfile::tempdir().expect("a writable place");
        let in_writable = stand_in::program(writable.path(), "claude", CLAUDE);
        assert!(refused(fx.check(
            Harness::ClaudeCode,
            &in_writable,
            &[writable.path().to_path_buf()],
            None
        )));
        // The refusal names where.
        assert_eq!(
            NotStarted::ProgramWritable(PathBuf::from("/p/node_modules/.bin/claude")).to_string(),
            "this project runs every chat sandboxed, and the program lives where this chat can \
             write: /p/node_modules/.bin/claude, so it was not started sandboxed. Keep the \
             program outside the plane and outside what a chat may write."
        );
    }

    #[test]
    fn every_word_of_the_command_is_held_to_where_the_chat_can_write() {
        // D-88g: an interpreter outside, handed a file a chat can write, runs that file.
        let fx = Fixture::new();
        let claude = stand_in::program(fx.outside.path(), "claude", CLAUDE);
        let in_cwd = stand_in::program(&fx.cwd(), "c.sh", CLAUDE);
        let at_root = stand_in::program(fx.plane.path(), "c.sh", CLAUDE);
        let temp = tempfile::tempdir().expect("a temp folder");
        let in_temp = stand_in::program(temp.path(), "c.sh", CLAUDE);
        let config = fx.plane.path().join("x.json");
        std::fs::write(&config, "{}").expect("a config");
        let not_yet = fx.cwd().join("later.json").display().to_string();
        let lib = fx.plane.path().join("lib");
        std::fs::create_dir_all(&lib).expect("a lib");
        std::fs::create_dir_all(fx.cwd().join("lib")).expect("a lib in the chat's folder");
        std::fs::write(fx.cwd().join("lib/x.mjs"), "").expect("a module in the chat's folder");
        let module = lib.join("x.mjs");
        std::fs::write(&module, "").expect("a module");
        let url = format!("file://{}", module.display());
        let url_flag = format!("--import={url}");
        let short_url = format!("--import=file:{}", module.display());
        // The plane's first letter written as `%XX`: only a URL's reader sees the plane in it.
        let plane_text = fx.plane.path().display().to_string();
        let first = plane_text.chars().nth(1).expect("a letter");
        let encoded_url = format!(
            "--import=file:///%{:02X}{}/lib/x.mjs",
            u32::from(first),
            &plane_text[2..]
        );
        let at_file = format!("@{}", config.display());
        let second_eq = format!("--a=b={}", config.display());
        let colon = format!("--settings:{}", config.display());
        let inline = format!(". {}/c.sh", fx.plane.path().display());
        let json = format!(
            r#"{{"mcpServers":{{"x":{{"command":"/bin/sh","args":["{}"]}}}}}}"#,
            at_root.display().to_string().replace('/', "\\/")
        );
        // Two siblings in one folder: a file the chat has yet to make, reached from outside.
        let plane_name = fx.plane.path().file_name().expect("a name").to_owned();
        let dotdot = format!(
            "--out={}/../{}/later.json",
            fx.outside.path().display(),
            plane_name.to_string_lossy()
        );
        let linked_dir = fx.outside.path().join("to-plane");
        std::os::unix::fs::symlink(fx.plane.path(), &linked_dir).expect("a link to the plane");
        let linked = format!("--out={}/later.json", linked_dir.display());
        let nope_abs = format!(
            "{}/nope/../../{}/lib/x.mjs",
            fx.outside.path().display(),
            plane_name.to_string_lossy()
        );
        let nope_url = format!("--import=file://{nope_abs}");
        let u = |text: &str, from: char| {
            text.chars()
                .map(|c| {
                    if c == from {
                        format!("\\u{:04X}", u32::from(c))
                    } else {
                        c.to_string()
                    }
                })
                .collect::<String>()
        };
        let json_u = format!(
            r#"{{"command":"/bin/sh","args":["{}"]}}"#,
            u(&at_root.display().to_string(), '/')
        );
        let json_letter = format!(
            r#"{{"command":"/bin/sh","args":["{}"]}}"#,
            u(&at_root.display().to_string(), first)
        );
        let json_pct = format!(
            r#"{{"args":["--import=file:///\u0025{:02X}{}/lib/x.mjs"]}}"#,
            u32::from(first),
            &plane_text[2..]
        );
        let json_url = format!(
            r#"{{"args":["--import=file:\/\/\/%{:02X}{}\/lib\/x.mjs"]}}"#,
            u32::from(first),
            plane_text[2..].replace('/', "\\/")
        );
        let s = |path: &Path| path.display().to_string();
        let writable = [temp.path().to_path_buf()];
        let probed = std::sync::atomic::AtomicBool::new(false);
        let probe = |_: &[String]| {
            probed.store(true, std::sync::atomic::Ordering::SeqCst);
            Some("2.1.288 (Claude Code)".to_owned())
        };
        let cwd = fx.cwd();
        let check = |words: &[String]| {
            checked(
                Harness::ClaudeCode,
                words,
                fx.plane.path(),
                Chat {
                    cwd: &cwd,
                    writable: &writable,
                    env: &[],
                },
                Some(&probe),
            )
        };
        let words = |list: &[&str]| list.iter().map(|w| (*w).to_owned()).collect::<Vec<_>>();
        for (what, command, named) in [
            (
                "a shell on a script in the chat's folder",
                words(&["/bin/sh", &s(&in_cwd)]),
                s(&in_cwd),
            ),
            (
                "env on a script at the plane's root",
                words(&["/usr/bin/env", &s(&at_root)]),
                s(&at_root),
            ),
            (
                "a shell on a relative script",
                words(&["/bin/sh", "c.sh"]),
                "c.sh".to_owned(),
            ),
            (
                "a shell on a script in temp",
                words(&["/bin/sh", &s(&in_temp)]),
                s(&in_temp),
            ),
            (
                "a flag's value naming a file in the plane",
                words(&[&s(&claude), &format!("--mcp-config={}", s(&config))]),
                format!("--mcp-config={}", s(&config)),
            ),
            (
                "a flag's argument naming a file in the plane",
                words(&[&s(&claude), "--mcp-config", &s(&config)]),
                s(&config),
            ),
            (
                "a file a chat could make before it runs",
                words(&[&s(&claude), &format!("--settings={not_yet}")]),
                format!("--settings={not_yet}"),
            ),
            (
                "the chat's folder itself",
                words(&[&s(&claude), "--add-dir", "."]),
                ".".to_owned(),
            ),
            (
                "a short flag's attached path",
                words(&[&s(&claude), &format!("-I{}", s(&lib))]),
                format!("-I{}", s(&lib)),
            ),
            (
                "a short flag's attached file",
                words(&[&s(&claude), &format!("-r{}", s(&config))]),
                format!("-r{}", s(&config)),
            ),
            (
                "a short flag's attached relative path",
                words(&[&s(&claude), "-Ilib"]),
                "-Ilib".to_owned(),
            ),
            (
                "a file URL as a flag's value",
                words(&[&s(&claude), &url_flag.clone()]),
                url_flag.clone(),
            ),
            (
                "a file URL as its own word",
                words(&[&s(&claude), &url.clone()]),
                url.clone(),
            ),
            (
                "a short file URL",
                words(&[&s(&claude), &short_url.clone()]),
                short_url.clone(),
            ),
            (
                "a percent-encoded file URL",
                words(&[&s(&claude), &encoded_url.clone()]),
                encoded_url.clone(),
            ),
            (
                "a relative file URL",
                words(&[&s(&claude), "--import=file:lib/x.mjs"]),
                "--import=file:lib/x.mjs".to_owned(),
            ),
            (
                "an argument file",
                words(&[&s(&claude), &at_file.clone()]),
                at_file.clone(),
            ),
            (
                "a value after a second `=`",
                words(&[&s(&claude), &second_eq.clone()]),
                second_eq.clone(),
            ),
            (
                "a value after a `:`",
                words(&[&s(&claude), &colon.clone()]),
                colon.clone(),
            ),
            (
                "a later file through `..`",
                words(&[&s(&claude), &dotdot.clone()]),
                dotdot.clone(),
            ),
            (
                "a later file through a link",
                words(&[&s(&claude), &linked.clone()]),
                linked.clone(),
            ),
            (
                "a path inside inline JSON, its slashes escaped",
                words(&[&s(&claude), "--mcp-config", &json]),
                json.clone(),
            ),
            (
                "`..` after a folder that is not there, in a file URL",
                words(&[&s(&claude), &nope_url.clone()]),
                nope_url.clone(),
            ),
            (
                "`..` after a folder that is not there",
                words(&[&s(&claude), &nope_abs.clone()]),
                nope_abs.clone(),
            ),
            (
                "a relative `..` after a folder that is not there",
                words(&[&s(&claude), "--import=./nope/../lib/x.mjs"]),
                "--import=./nope/../lib/x.mjs".to_owned(),
            ),
            (
                "a relative `..` after a folder that is not there, alone",
                words(&[&s(&claude), "nope/../lib/x.mjs"]),
                "nope/../lib/x.mjs".to_owned(),
            ),
            (
                "a relative path inside inline code",
                words(&[&s(&claude), r#"require("./lib/x.mjs")"#]),
                r#"require("./lib/x.mjs")"#.to_owned(),
            ),
            (
                "a relative path in quotes",
                words(&[&s(&claude), "do 'c.sh'"]),
                "do 'c.sh'".to_owned(),
            ),
            (
                "a relative path after a tab",
                words(&[&s(&claude), ".\tc.sh"]),
                ".\tc.sh".to_owned(),
            ),
            (
                "a relative path after `<`",
                words(&[&s(&claude), "cat<c.sh"]),
                "cat<c.sh".to_owned(),
            ),
            (
                "a relative path first in a list",
                words(&[&s(&claude), "c.sh:/usr/lib"]),
                "c.sh:/usr/lib".to_owned(),
            ),
            (
                "a relative path in inline JSON",
                words(&[&s(&claude), r#"{"command":"/bin/sh","args":["c.sh"]}"#]),
                r#"{"command":"/bin/sh","args":["c.sh"]}"#.to_owned(),
            ),
            (
                "a plane path in inline JSON, every slash `\\u002F`",
                words(&[&s(&claude), &json_u.clone()]),
                json_u.clone(),
            ),
            (
                "a plane path in inline JSON, one letter `\\u`-escaped",
                words(&[&s(&claude), &json_letter.clone()]),
                json_letter.clone(),
            ),
            (
                "a file URL in inline JSON whose `%` is itself escaped",
                words(&[&s(&claude), &json_pct]),
                json_pct.clone(),
            ),
            (
                "a file URL in inline JSON, escaped and percent-encoded",
                words(&[&s(&claude), &json_url.clone()]),
                json_url.clone(),
            ),
            (
                "a path inside inline code",
                words(&["/bin/sh", "-c", &inline]),
                inline.clone(),
            ),
        ] {
            assert_eq!(
                check(&command),
                Err(NotStarted::WordWritable(named)),
                "{what}"
            );
        }
        assert!(
            !probed.load(std::sync::atomic::Ordering::SeqCst),
            "a refused command was run"
        );
        // Words that name no file are left alone: flags, model names, values.
        let real = claude.canonicalize().expect("real").display().to_string();
        assert_eq!(
            check(&words(&[&s(&claude), "--model=x", "--model", "opus", "-p"])),
            Ok(words(&[&real, "--model=x", "--model", "opus", "-p"]))
        );
        assert_eq!(
            NotStarted::WordWritable("/p/c.sh".to_owned()).to_string(),
            "this project runs every chat sandboxed, and this profile's command names /p/c.sh, \
             which lies where this chat can write, so it was not started sandboxed. Keep every \
             file the command names outside the plane and outside what a chat may write."
        );
    }

    #[test]
    fn a_word_over_4_kib_is_refused_and_one_under_it_is_checked_in_bounded_time() {
        let fx = Fixture::new();
        let claude = stand_in::program(fx.outside.path(), "claude", CLAUDE);
        let cwd = fx.cwd();
        let probe = |_: &[String]| Some("2.1.288 (Claude Code)".to_owned());
        let check = |word: String| {
            checked(
                Harness::ClaudeCode,
                &[claude.display().to_string(), word],
                fx.plane.path(),
                Chat {
                    cwd: &cwd,
                    writable: &[],
                    env: &[],
                },
                Some(&probe),
            )
        };
        assert_eq!(
            check("x".repeat(WORD_MAX + 1)),
            Err(NotStarted::WordTooLong)
        );
        // Every `/` begins a path, and every part of each is looked up: shared, not repeated.
        let began = Instant::now();
        for unit in ["/a", "/./a", "/usr/../bin/..", "/tmp/../usr", "/x/y"] {
            let word: String = unit.repeat(WORD_MAX / unit.len());
            assert!(check(word).is_ok(), "{unit}");
        }
        assert!(
            began.elapsed() < Duration::from_secs(10),
            "{:?}",
            began.elapsed()
        );
        assert_eq!(
            NotStarted::WordTooLong.to_string(),
            "this project runs every chat sandboxed, and a word of this profile's command is \
             longer than 4 KiB, which purlis does not check, so it was not started sandboxed. \
             Keep what it says in a file outside the plane and name that file instead."
        );
    }

    #[test]
    fn a_word_is_read_as_json_reads_its_strings() {
        assert_eq!(
            json_unescaped(r#"\ud83d\ude00 \u002F\/\"\\ \n\t \q \u12 \ud800x"#),
            "\u{1F600} //\"\\ \n\t \\q \\u12 \u{FFFD}x"
        );
        assert_eq!(percent_decoded("%2Fa%2fb%zz%"), "/a/b%zz%");
        assert_eq!(folded(Path::new("/a/./b//../c/..")), PathBuf::from("/a"));
    }

    #[test]
    fn a_relative_program_is_refused() {
        let fx = Fixture::new();
        for program in ["./claude", "bin/claude", "claude"] {
            assert_eq!(
                fx.check(Harness::ClaudeCode, Path::new(program), &[], None),
                Err(NotStarted::ProgramRelative),
                "{program}"
            );
        }
    }

    #[test]
    fn the_answer_is_asked_of_the_real_path_the_chat_then_runs() {
        let fx = Fixture::new();
        let real = stand_in::program(fx.outside.path(), "claude", CLAUDE);
        let elsewhere = tempfile::tempdir().expect("elsewhere");
        let link = elsewhere.path().join("claude");
        std::os::unix::fs::symlink(&real, &link).expect("a link");
        let asked = std::sync::Mutex::new(Vec::new());
        let probe = |words: &[String]| {
            asked.lock().expect("lock").push(words.to_vec());
            Some("2.1.288 (Claude Code)".to_owned())
        };
        let words = fx
            .check(Harness::ClaudeCode, &link, &[], Some(&probe))
            .expect("starts");
        let real = real.canonicalize().expect("real").display().to_string();
        assert_eq!(words, std::slice::from_ref(&real));
        assert_eq!(*asked.lock().expect("lock"), [vec![real]]);
    }

    #[test]
    fn a_program_that_hangs_is_refused_and_its_whole_group_is_killed() {
        let fx = Fixture::new();
        let hang = stand_in::program(fx.outside.path(), "claude", "#!/bin/sh\nsleep 60 &\nwait\n");
        let began = Instant::now();
        let mut probe = None;
        let said = answer_of(
            &[hang.display().to_string()],
            Chat {
                cwd: &fx.cwd(),
                writable: &[],
                env: &[],
            },
            &mut |pid| probe = Some(pid),
        );
        assert_eq!(said, Err(Unanswered::TimedOut));
        assert!(
            began.elapsed() < PATIENCE + Duration::from_secs(5),
            "{:?}",
            began.elapsed()
        );
        // The probe led its own group; nothing of the group is left, its child included.
        let pid = probe.expect("the probe ran");
        let leader =
            rustix::process::Pid::from_raw(i32::try_from(pid).expect("a pid")).expect("not zero");
        // A killed child stays a zombie, still in the group, until init reaps it,
        // so give the reaping a moment rather than reading the group at once.
        let reaped = Instant::now() + Duration::from_secs(5);
        while rustix::process::test_kill_process_group(leader).is_ok() && Instant::now() < reaped {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(
            rustix::process::test_kill_process_group(leader).is_err(),
            "the wrapper's own child was left running"
        );
        // And the start says it timed out, not that the program is something else.
        let refused = fx.check(Harness::ClaudeCode, &hang, &[], None);
        assert_eq!(refused, Err(NotStarted::ProbeTimedOut(Harness::ClaudeCode)));
        assert!(
            refused
                .expect_err("refused")
                .to_string()
                .contains("start the chat again"),
        );
    }

    #[test]
    fn a_child_that_leaves_the_group_holding_the_answer_does_not_hold_the_start() {
        // It answers at once and exits, but a child it started in a group of its own keeps the
        // answer's pipe open past the deadline. That child ends on its own, so the test kills
        // nothing it did not start.
        let fx = Fixture::new();
        let leaver = stand_in::program(
            fx.outside.path(),
            "claude",
            "#!/bin/sh\necho 'not claude'\nperl -e 'setpgrp(0,0); sleep 20' &\nexit 0\n",
        );
        let began = Instant::now();
        assert!(fx.check(Harness::ClaudeCode, &leaver, &[], None).is_err());
        assert!(
            began.elapsed() < PATIENCE + Duration::from_secs(5),
            "{:?}",
            began.elapsed()
        );
    }

    #[test]
    fn the_answer_is_asked_in_the_chats_folder_with_its_own_environment() {
        let fx = Fixture::new();
        let teller = stand_in::program(
            fx.outside.path(),
            "claude",
            "#!/bin/sh\npwd\necho \"${CHARTER_PROBE_SEEN-unset} ${APP_ONLY-unset}\"\necho '2.1.288 (Claude Code)'\n",
        );
        let cwd = fx.cwd();
        let env = vec![("CHARTER_PROBE_SEEN".to_owned(), "chat".to_owned())];
        // SAFETY of the test: a variable only this test sets, read only by the child.
        let said = version_of(
            &[teller.display().to_string()],
            Chat {
                cwd: &cwd,
                writable: &[],
                env: &env,
            },
        )
        .expect("an answer");
        let mut lines = said.lines();
        assert_eq!(
            lines.next().map(PathBuf::from),
            Some(cwd.canonicalize().expect("real"))
        );
        assert_eq!(lines.next(), Some("chat unset"));
    }

    /// D-88k: the probe is the first time charter runs the program, before any sandbox of the
    /// chat's, so it runs inside charter's own wrap: it writes nothing but its own temp
    /// directory and reaches no network, whatever the command loads.
    #[cfg(target_os = "macos")]
    #[test]
    fn the_answer_is_asked_inside_charters_wrap_which_writes_nothing_and_reaches_nothing() {
        let fx = Fixture::new();
        let mark = fx.outside.path().join("written");
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a listener");
        let port = listener.local_addr().expect("an address").port();
        let home_dir = stand_in::NoChatWrites::new();
        let home_mark = home_dir.path().join("written");
        let program = stand_in::program(
            fx.outside.path(),
            "claude",
            &format!(
                "#!/bin/sh\necho x > '{}' && echo wrote\necho x > '{}' && echo wrote-home\n\
                 /usr/bin/nc -z -w 1 127.0.0.1 {port} && echo reached\n\
                 echo x > \"$TMPDIR/own\" && echo own-temp\necho '2.1.288 (Claude Code)'\n",
                mark.display(),
                home_mark.display()
            ),
        );
        let cwd = fx.cwd();
        let said = version_of(
            &[program.display().to_string()],
            Chat {
                cwd: &cwd,
                writable: &[],
                env: &[],
            },
        );
        assert_eq!(said.as_deref(), Ok("own-temp\n2.1.288 (Claude Code)"));
        assert!(!mark.exists(), "the probe wrote outside its temp directory");
        assert!(
            !home_mark.exists(),
            "the probe wrote outside its temp directory"
        );
    }

    #[test]
    fn an_answer_is_read_only_up_to_its_cap() {
        let fx = Fixture::new();
        let noisy = stand_in::program(
            fx.outside.path(),
            "claude",
            "#!/bin/sh\nyes 'not claude' | head -c 1000000\necho '2.1.288 (Claude Code)'\n",
        );
        assert!(fx.check(Harness::ClaudeCode, &noisy, &[], None).is_err());
    }

    #[test]
    fn only_a_line_claude_code_answers_with_counts() {
        for (said, is) in [
            ("2.1.288 (Claude Code)", true),
            ("warning\n2.1.288 (Claude Code)\n", true),
            ("2.1.288", false),
            ("2.1 (Claude Code)", false),
            ("x2.1.288 (Claude Code)", false),
            ("codex-cli 0.147.0", false),
            ("", false),
        ] {
            assert_eq!(is_claude_code(said), is, "{said:?}");
        }
    }

    #[test]
    fn the_temp_directories_are_counted_as_writable() {
        let roots = temp_roots(&[("TMPDIR".to_owned(), "/chat/tmp".to_owned())]);
        for want in ["/tmp", "/private/tmp", "/chat/tmp"] {
            assert!(roots.contains(&PathBuf::from(want)), "{want}");
        }
        assert!(roots.contains(&std::env::temp_dir()));
    }
}
