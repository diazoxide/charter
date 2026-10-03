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
//! - **Claude Code answers as Claude Code.** Its program and command, asked `--version`, must
//!   answer a line `X.Y.Z (Claude Code)`. A legitimate wrapper outside the plane passes its words
//!   through and answers the same. It is asked with the chat's own environment, in the chat's
//!   folder.
//!
//! **What the answer proves** is that the profile is Claude Code by intent, not by mistake. A
//! program written to answer like Claude Code passes; the profile's program is one a person
//! approved (ADR 0022), and that approval is what this rests on.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::NotStarted;
use crate::harness::Harness;

/// How long a program has to answer `--version`.
const PATIENCE: Duration = Duration::from_secs(5);

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
    let mut roots = vec![root.to_path_buf(), chat.cwd.to_path_buf()];
    roots.extend(chat.writable.iter().cloned());
    for dir in roots {
        let real_dir = dir.canonicalize().unwrap_or_else(|_| dir.clone());
        for path in [&written, &real] {
            if path.starts_with(&dir) || path.starts_with(&real_dir) {
                return Err(NotStarted::ProgramWritable(path.clone()));
            }
        }
    }
    let mut words = words.to_vec();
    words[0] = real.display().to_string();
    if harness == Harness::ClaudeCode {
        let said = match probe {
            Some(probe) => probe(&words),
            None => version_of(&words, chat),
        };
        if !said.is_some_and(|said| is_claude_code(&said)) {
            return Err(NotStarted::NotTheHarness(harness));
        }
    }
    Ok(words)
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
/// [`ANSWER_MAX`] bytes, or `None`.
///
/// The program runs in a process group of its own, and the whole group is killed when it does
/// not answer in time. A child that left the group and holds the answer's pipe open does not
/// hold the start up: the answer is read on a thread of its own, and given up on at the
/// deadline.
pub fn version_of(words: &[String], chat: Chat<'_>) -> Option<String> {
    let (program, rest) = words.split_first()?;
    let mut command = std::process::Command::new(program);
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
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = crate::forklock::spawn(&mut command).ok()?;
    let stdout = child.stdout.take()?;
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
    // Its own children may still hold the answer's pipe open: the group goes too.
    #[cfg(unix)]
    {
        use rustix::process::{Pid, Signal, kill_process_group};
        let _ = kill_process_group(Pid::from_child(&child), Signal::KILL);
    }
    let _ = child.kill();
    let status = child.wait().ok()?;
    // A child that left the group and kept the pipe cannot hold the start past the deadline.
    let left = deadline.saturating_duration_since(Instant::now());
    let out = answer
        .recv_timeout(left.max(Duration::from_millis(100)))
        .ok()?;
    status
        .success()
        .then(|| String::from_utf8_lossy(&out).trim().to_owned())
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
            "this plane runs every chat sandboxed, and the program lives where this chat can \
             write: /p/node_modules/.bin/claude, so it was not started sandboxed. Keep the \
             program outside the plane and outside what a chat may write."
        );
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
        let pid_file = fx.outside.path().join("pid");
        let hang = stand_in::program(
            fx.outside.path(),
            "claude",
            &format!(
                "#!/bin/sh\nsleep 60 &\necho $! > '{}'\nwait\n",
                pid_file.display()
            ),
        );
        let began = Instant::now();
        assert_eq!(
            fx.check(Harness::ClaudeCode, &hang, &[], None),
            Err(NotStarted::NotTheHarness(Harness::ClaudeCode))
        );
        assert!(
            began.elapsed() < Duration::from_secs(10),
            "{:?}",
            began.elapsed()
        );
        let pid = std::fs::read_to_string(&pid_file).expect("the child's pid");
        std::thread::sleep(Duration::from_millis(100));
        let alive =
            crate::forklock::status(std::process::Command::new("kill").args(["-0", pid.trim()]))
                .expect("kill runs");
        assert!(!alive.success(), "the wrapper's own child was left running");
    }

    #[test]
    fn a_child_that_leaves_the_group_holding_the_answer_does_not_hold_the_start() {
        // It answers at once and exits, but a child it started in a group of its own keeps the
        // answer's pipe open for a minute.
        let fx = Fixture::new();
        let pid_file = fx.outside.path().join("pid");
        let leaver = stand_in::program(
            fx.outside.path(),
            "claude",
            &format!(
                "#!/bin/sh\necho 'not claude'\nperl -e 'setpgrp(0,0); sleep 60' &\necho $! > '{}'\nexit 0\n",
                pid_file.display()
            ),
        );
        let began = Instant::now();
        assert!(fx.check(Harness::ClaudeCode, &leaver, &[], None).is_err());
        assert!(
            began.elapsed() < Duration::from_secs(10),
            "{:?}",
            began.elapsed()
        );
        if let Ok(pid) = std::fs::read_to_string(&pid_file) {
            let _ = crate::forklock::status(std::process::Command::new("kill").arg(pid.trim()));
        }
    }

    #[test]
    fn the_answer_is_asked_in_the_chats_folder_with_its_own_environment() {
        let fx = Fixture::new();
        let said = fx.outside.path().join("said");
        let teller = stand_in::program(
            fx.outside.path(),
            "claude",
            &format!(
                "#!/bin/sh\npwd > '{}'\necho \"${{CHARTER_PROBE_SEEN-unset}} ${{APP_ONLY-unset}}\" >> '{}'\necho '2.1.288 (Claude Code)'\n",
                said.display(),
                said.display()
            ),
        );
        let cwd = fx.cwd();
        let env = vec![("CHARTER_PROBE_SEEN".to_owned(), "chat".to_owned())];
        // SAFETY of the test: a variable only this test sets, read only by the child.
        let words = checked(
            Harness::ClaudeCode,
            &[teller.display().to_string()],
            fx.plane.path(),
            Chat {
                cwd: &cwd,
                writable: &[],
                env: &env,
            },
            None,
        )
        .expect("starts");
        assert_eq!(words.len(), 1);
        let text = std::fs::read_to_string(&said).expect("what it saw");
        let mut lines = text.lines();
        assert_eq!(
            lines.next().map(PathBuf::from),
            Some(cwd.canonicalize().expect("real"))
        );
        assert_eq!(lines.next(), Some("chat unset"));
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
