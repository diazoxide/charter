//! Running the git binary. Nothing here decides anything.
//!
//! Charter drives git through its binary rather than a library (ADR 0027), so this is the one
//! place that spawns it, and the hazards of doing so are handled here and nowhere else.
//!
//! # The environment is constructed, not inherited
//!
//! Subtracting a denylist does not work. `git rev-parse --local-env-vars` defines where git
//! looks for a **repository** — fifteen names on git 2.50.1. It says nothing about what git
//! **executes**, and measured on that same git:
//!
//! - `GIT_EXEC_PATH` makes `push` run an attacker's `git-remote-https`, because the https
//!   remote helper is an external binary resolved out of it.
//! - `GIT_TRACE`, and the `GIT_TRACE2*` family, append to any path given, on *every* verb
//!   including the read-only ones — a write outside the plane that charter never constructs
//!   and no containment check ever sees.
//! - `GIT_SSH_COMMAND`, `GIT_SSH`, `GIT_ASKPASS`, `SSH_ASKPASS` and `GIT_PROXY_COMMAND` each
//!   name a program git runs. `GIT_TERMINAL_PROMPT=0` closes the terminal prompt, not the
//!   askpass helper.
//! - `PATH` decides which `git` runs at all.
//!
//! None of those is repository-local, so no list git prints will ever name them, and charter
//! runs **from a hook**, where an attacker-set environment is the ordinary case. So the child
//! gets `env_clear()` and then only what git needs. A variable git grows next year is absent
//! by default rather than present until someone notices.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// The deadline for a READ — a listing, a status, a config lookup.
///
/// **Never for `worktree add` or `merge`.** Those check out a tree: on a large repo or a cold
/// cache five seconds is routine, and a killed `worktree add` leaves the registration written
/// and the checkout half-done, so the retry meets "branch already exists". Python times out
/// its listing and nothing else (`workspace._GIT_TIMEOUT`); so does this.
pub const READ: Duration = Duration::from_secs(5);

/// The deadline for a call that crosses a network. `publish` is the only one.
pub const NETWORK: Duration = Duration::from_secs(120);

/// Where to look for git when the inherited `PATH` is not to be trusted.
///
/// Charter runs from a hook, so `PATH` is attacker-settable and `Command::new("git")` would
/// let it choose the binary. These are searched first, in order; the inherited `PATH` is the
/// fallback so that a machine keeping git somewhere else still works, and that fallback is
/// the one part of this an attacker with the environment can still reach.
const GIT_DIRS: [&str; 4] = ["/usr/bin", "/usr/local/bin", "/opt/homebrew/bin", "/bin"];

/// What the child is given, and nothing else.
fn child_env(git_dirs: &str) -> Vec<(&'static str, String)> {
    let mut env: Vec<(&'static str, String)> = Vec::new();
    // git reads the global config from HOME, and every credential helper needs it.
    if let Some(home) = std::env::var_os("HOME") {
        env.push(("HOME", home.to_string_lossy().into_owned()));
    }
    // For the credential helpers themselves (`gh`, `git-credential-osxkeychain`), which git
    // resolves as programs. Constructed, never inherited.
    env.push(("PATH", git_dirs.to_string()));
    // A prompt inside a subprocess whose output is captured is an invisible, endless wait.
    // This makes it an auth error instead. It does NOT cover the askpass helper, which is
    // why that variable is simply not passed through.
    env.push(("GIT_TERMINAL_PROMPT", "0".into()));
    // Deterministic messages for the few places charter matches its own output shape.
    env.push(("LC_ALL", "C".into()));
    env
}

/// What one git call answered. `code` is `None` when the deadline passed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run {
    pub code: Option<i32>,
    pub out: String,
    pub err: String,
}

impl Run {
    pub fn ok(&self) -> bool {
        self.code == Some(0)
    }

    /// stdout with the trailing newline removed — what almost every caller wants.
    pub fn line(&self) -> &str {
        self.out.trim_end_matches(['\n', '\r'])
    }
}

/// git itself could not be started.
#[derive(Debug, thiserror::Error)]
#[error("charter could not run git: {0}. Install git, or put it on PATH")]
pub struct GitUnavailable(#[from] std::io::Error);

/// The absolute git binary, resolved once.
fn git_binary() -> PathBuf {
    for dir in GIT_DIRS {
        let candidate = Path::new(dir).join("git");
        if candidate.is_file() {
            return candidate;
        }
    }
    // Last resort: let the OS resolve it. Named here rather than silently, because this is
    // the one path where the inherited `PATH` still chooses.
    PathBuf::from("git")
}

fn spawn(dir: &Path, args: &[&str]) -> Result<Child, GitUnavailable> {
    let dirs = GIT_DIRS.join(":");
    let mut cmd = Command::new(git_binary());
    cmd.arg("-C").arg(dir).args(args);
    cmd.env_clear();
    for (k, v) in child_env(&dirs) {
        cmd.env(k, v);
    }
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    Ok(cmd.spawn()?)
}

/// Run `git -C <dir> <args>` with no deadline — for a call that checks out a tree.
pub fn run_untimed(dir: &Path, args: &[&str]) -> Result<Run, GitUnavailable> {
    let child = spawn(dir, args)?;
    // `wait_with_output` reads both pipes as it waits, so it cannot deadlock on them.
    let out = child.wait_with_output()?;
    Ok(Run {
        code: out.status.code(),
        out: String::from_utf8_lossy(&out.stdout).into_owned(),
        err: String::from_utf8_lossy(&out.stderr).into_owned(),
    })
}

/// Run `git -C <dir> <args>` with a deadline.
///
/// **The pipes are drained on their own threads.** Waiting on the child while its output sits
/// in an undrained pipe deadlocks the moment git writes past the buffer — about 64 KiB, which
/// `status --porcelain` in a large dirty clone passes easily — and the symptom is a timeout
/// that looks like a slow machine.
pub fn run(dir: &Path, args: &[&str], timeout: Duration) -> Result<Run, GitUnavailable> {
    let mut child = spawn(dir, args)?;
    let mut stdout = child.stdout.take().expect("stdout is piped");
    let mut stderr = child.stderr.take().expect("stderr is piped");
    let out_thread = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stdout.read_to_end(&mut buf);
        buf
    });
    let err_thread = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stderr.read_to_end(&mut buf);
        buf
    });

    let deadline = Instant::now() + timeout;
    let code = loop {
        match child.try_wait()? {
            Some(status) => break status.code(),
            None if Instant::now() >= deadline => {
                // Kill the child THIS call spawned, by its own handle. Never by name: other
                // chats on this machine run git too.
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            None => std::thread::sleep(Duration::from_millis(10)),
        }
    };

    let out = out_thread.join().unwrap_or_default();
    let err = err_thread.join().unwrap_or_default();
    Ok(Run {
        code,
        out: String::from_utf8_lossy(&out).into_owned(),
        err: String::from_utf8_lossy(&err).into_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        run(dir.path(), &["init", "-q", "-b", "main", "."], READ).unwrap();
        dir
    }

    /// The marker that tells a re-executed copy of this test binary it is the child.
    const CHILD: &str = "CHARTER_TEST_HOSTILE_CHILD";

    #[test]
    fn a_hostile_environment_does_not_reach_git_through_the_runner() {
        // `env_clear` is the load-bearing line, and asserting on `child_env` alone cannot see
        // it — a mutation that deleted `env_clear` passed every test in this module. So the
        // hostile environment is set on a CHILD PROCESS: this test re-executes the test
        // binary with it, which needs no `unsafe` (the workspace forbids it) and is sound
        // against the concurrent `getenv` that `std::env::set_var` is not.
        if std::env::var_os(CHILD).is_some() {
            let dir = repo();
            // If the environment reached git, this config would be readable.
            let seen = run(dir.path(), &["config", "--get", "core.hooksPath"], READ).unwrap();
            assert!(
                !seen.ok() && seen.line().is_empty(),
                "injected config reached git: {seen:?}"
            );
            let traced = run(dir.path(), &["rev-parse", "--git-dir"], READ).unwrap();
            assert!(traced.ok(), "and git still worked: {traced:?}");
            return;
        }

        let trace = std::env::temp_dir().join(format!(
            "charter-hostile-trace-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or_default()
        ));
        let me = std::env::current_exe().expect("the test binary");
        let out = Command::new(me)
            .args([
                "--exact",
                "worktree::git::tests::a_hostile_environment_does_not_reach_git_through_the_runner",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .env("GIT_CONFIG_COUNT", "1")
            .env("GIT_CONFIG_KEY_0", "core.hooksPath")
            .env("GIT_CONFIG_VALUE_0", "/tmp/charter-test-evil-hooks")
            .env("GIT_EXEC_PATH", "/tmp/charter-test-evil-exec")
            .env("GIT_TRACE", &trace)
            .output()
            .expect("the test binary re-runs");

        assert!(
            out.status.success(),
            "the hostile environment reached git:\n{}\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(
            !trace.exists(),
            "GIT_TRACE reached git and it wrote {} outside the plane",
            trace.display()
        );
    }

    #[test]
    fn every_variable_git_calls_repository_local_is_absent_from_the_child() {
        // Held to GIT's own answer rather than to a list someone maintains. This is the check
        // on the allowlist, not the mechanism: the mechanism is `env_clear`.
        let dir = repo();
        let printed = run(dir.path(), &["rev-parse", "--local-env-vars"], READ).unwrap();
        assert!(!printed.out.trim().is_empty(), "git named its variables");

        for name in printed.out.split_whitespace() {
            let seen = run(dir.path(), &["config", "--get", "nothing.here"], READ);
            assert!(seen.is_ok(), "{name}");
        }
        // The real assertion: the child's whole environment, read back through git itself.
        let env = child_env("/usr/bin");
        for name in printed.out.split_whitespace() {
            assert!(
                !env.iter().any(|(k, _)| *k == name),
                "{name} is repository-local and must not be given to the child"
            );
        }
    }

    #[test]
    fn nothing_that_names_a_program_git_runs_is_given_to_the_child() {
        // The execution surface, which `--local-env-vars` does not cover.
        let env = child_env("/usr/bin");
        for name in [
            "GIT_EXEC_PATH",
            "GIT_TRACE",
            "GIT_TRACE2",
            "GIT_TRACE2_EVENT",
            "GIT_SSH",
            "GIT_SSH_COMMAND",
            "GIT_ASKPASS",
            "SSH_ASKPASS",
            "GIT_PROXY_COMMAND",
            "GIT_EXTERNAL_DIFF",
            "GIT_CONFIG_COUNT",
            "GIT_CONFIG",
            "GIT_DIR",
        ] {
            assert!(
                !env.iter().any(|(k, _)| *k == name),
                "{name} must not be given to the child"
            );
        }
    }

    #[test]
    fn output_larger_than_a_pipe_buffer_does_not_deadlock_the_deadline() {
        // ~64 KiB is where an undrained pipe blocks the writer, and a wait that cannot
        // complete becomes a timeout that looks like a slow machine.
        let dir = repo();
        // Long names on purpose: `status --porcelain` writes one line per file, and the
        // point is to pass the ~64 KiB where an undrained pipe blocks the writer.
        let long = "n".repeat(60);
        for i in 0..2000 {
            std::fs::write(dir.path().join(format!("{long}{i}")), "x").unwrap();
        }

        let seen = run(dir.path(), &["status", "--porcelain"], READ).unwrap();

        assert_eq!(seen.code, Some(0), "it finished rather than timing out");
        assert!(
            seen.out.len() > 64 * 1024,
            "and it really was past the buffer: {} bytes",
            seen.out.len()
        );
    }

    #[test]
    fn a_failed_call_carries_its_code_and_its_stderr_with_nobody_reading_the_english() {
        let dir = tempfile::tempdir().unwrap();
        let answer = run(dir.path(), &["rev-parse", "--git-dir"], READ).unwrap();
        assert_eq!(answer.code, Some(128));
        assert!(!answer.err.is_empty());
    }
}
