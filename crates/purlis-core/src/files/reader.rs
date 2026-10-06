//! The bounded child that reads a branch with gitoxide (FM-4, D-88h).
//!
//! **Why a child.** The app reads a branch's changes by itself after every write an agent
//! makes, in a folder that agent controls. Read inside the app, a branch built to hurt the read
//! hurt the app: an ignore file linked to an endless device grew it by gigabytes in under a
//! second, a FIFO named as an ignore file hung the read for ever, and gitoxide's status kept
//! memory per untracked file on every read. So the read runs in a short-lived child of
//! charter's own binary, which the app can kill:
//! - **charter's own binary, never git, never a shell**: the app's executable, started again
//!   with [`READ_ARG`], answers one question and exits ([`serve_if_asked`]). It is the same code
//!   at the same version, with nothing to install or find on `PATH`;
//! - **a cleared environment**: only `HOME` and `XDG_CONFIG_HOME`, which the read uses to find
//!   the operator's own git config (their global ignore file among it), and nothing of `GIT_*`;
//! - **a deadline**: the child stops itself past [`git::READ`](crate::worktree::git::READ), and
//!   the app kills it [`GRACE`] after that, so a child whose app quit or crashed during a hung
//!   read still ends;
//! - **a memory cap**: the child watches its own resident memory and stops itself past
//!   [`MEMORY`] — nothing else kills it for that (on every platform, Windows included:
//!   `RLIMIT_AS` cannot be set below what a macOS process has already mapped, so a limit the
//!   kernel enforces is not available there);
//! - **a capped answer**: at most [`OUTPUT`] bytes are read back, as JSON.
//!
//! A read that is killed or capped fails: the window says so, and the watch counts the batch as
//! one that matters.

use std::ffi::OsString;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// How every read the capped reader gives no answer to begins: past its deadline or memory
/// cap, a child that fails, an answer too large. A refusal the reader answered with does not.
pub const READ_FAILED: &str = "purlis could not read the branch: ";

use super::status::{AheadBehind, Status, ahead_behind_here, status_here};
use super::watch::{folders_here, matters_here, root_here};
use super::{Branch, Refused};
use crate::worktree::git;

/// The argument that makes charter's binary the reader: the last one it is given.
pub const READ_ARG: &str = "charter-read-a-branch";

/// The most resident memory one read may take before it stops itself: 1 GiB.
pub const MEMORY: u64 = 1024 * 1024 * 1024;

/// The most bytes of answer read back from one read: 64 MiB.
pub const OUTPUT: u64 = 64 * 1024 * 1024;

/// How long after its own deadline the app kills a child that has not stopped itself.
pub const GRACE: Duration = Duration::from_secs(2);

/// What starts an answer on the child's standard output: a test binary prints its own lines
/// around it.
const FRAME: &str = "\u{1e}charter-read\u{1e}";

/// What is asked of a branch.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum Ask {
    /// Its folder, resolved.
    Root,
    /// What it changed.
    Status,
    /// The folders git knows in it.
    Folders,
    /// Whether any of these paths, relative to its folder, is one git does not ignore.
    Matters(Vec<PathBuf>),
    /// How far it is from the branch it was cut from (FM-5).
    AheadBehind,
    /// A comparison's file list (RC-2).
    Compare(super::Comparison),
    /// One file of a comparison at the sides its list was computed at (RC-2).
    CompareFile {
        sides: super::Sides,
        path: String,
        from: Option<String>,
    },
}

/// What the child answers.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum Answer {
    Root(PathBuf),
    Status(Status),
    Folders(Vec<PathBuf>),
    Matters(bool),
    AheadBehind(AheadBehind),
    Compared(super::Compared),
    FileDiff(super::FileDiff),
}

/// One question, as it crosses to the child.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct Question {
    plane: PathBuf,
    ws: String,
    repo: String,
    piece: Option<String>,
    ask: Ask,
    memory: u64,
    /// How long the child may take, in milliseconds, before it stops itself.
    deadline_ms: u64,
}

/// How to start the reader, and its bounds.
#[derive(Debug, Clone)]
pub struct Reader {
    program: PathBuf,
    args: Vec<OsString>,
    deadline: Duration,
    memory: u64,
}

impl Reader {
    /// This process's own binary, started again as the reader.
    pub fn this_binary() -> Result<Self, Refused> {
        let program = std::env::current_exe()
            .map_err(|e| Refused::Read(format!("purlis could not find its own binary: {e}")))?;
        Ok(Self::new(program, [OsString::from(READ_ARG)]))
    }

    /// `program` with `args`, whose last must be [`READ_ARG`]: a test binary, run again with a
    /// filter that picks the test calling [`serve_if_asked`].
    pub fn new(program: PathBuf, args: impl IntoIterator<Item = OsString>) -> Self {
        Self {
            program,
            args: args.into_iter().collect(),
            deadline: git::READ,
            memory: MEMORY,
        }
    }

    /// The same reader, stopping itself past `deadline` (and killed [`GRACE`] after that).
    pub fn deadline(mut self, deadline: Duration) -> Self {
        self.deadline = deadline;
        self
    }

    /// The same reader, stopping itself past `memory` bytes resident.
    pub fn memory(mut self, memory: u64) -> Self {
        self.memory = memory;
        self
    }

    /// Asks the child one question about `branch`, and waits for its answer within the bounds.
    pub fn ask(&self, plane: &Path, branch: Branch<'_>, ask: Ask) -> Result<Answer, Refused> {
        let question = Question {
            plane: plane.to_path_buf(),
            ws: branch.ws.to_string(),
            repo: branch.repo.to_string(),
            piece: branch.piece.map(str::to_string),
            ask,
            memory: self.memory,
            deadline_ms: u64::try_from(self.deadline.as_millis()).unwrap_or(u64::MAX),
        };
        let asked = serde_json::to_vec(&question)
            .map_err(|e| Refused::Read(format!("purlis could not ask the reader: {e}")))?;
        let failed = |why: String| Refused::Read(format!("{READ_FAILED}{why}"));

        let mut command = Command::new(&self.program);
        command
            .args(&self.args)
            .env_clear()
            .current_dir(std::env::temp_dir())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        for name in ["HOME", "XDG_CONFIG_HOME"] {
            if let Some(value) = std::env::var_os(name) {
                command.env(name, value);
            }
        }
        let mut child = crate::forklock::spawn(&mut command).map_err(|e| failed(format!("{e}")))?;
        if let Some(mut stdin) = child.stdin.take() {
            std::thread::spawn(move || {
                let _ = stdin.write_all(&asked);
            });
        }
        let stdout = child.stdout.take();
        let reading = std::thread::spawn(move || {
            let mut out = Vec::new();
            if let Some(stdout) = stdout {
                let _ = stdout.take(OUTPUT + 1).read_to_end(&mut out);
            }
            out
        });
        let started = Instant::now();
        loop {
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) if started.elapsed() >= self.deadline + GRACE => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(failed(format!(
                        "the read did not finish within {} seconds",
                        self.deadline.as_secs()
                    )));
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(5)),
                Err(e) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(failed(e.to_string()));
                }
            }
        }
        let out = reading.join().unwrap_or_default();
        if out.len() as u64 > OUTPUT {
            return Err(failed("the answer was too large".into()));
        }
        let text = String::from_utf8_lossy(&out);
        let Some((_, framed)) = text.rsplit_once(FRAME) else {
            return Err(failed(
                "the read stopped without an answer: it took too long or too much memory, or \
                 failed"
                    .into(),
            ));
        };
        let line = framed.lines().next().unwrap_or_default();
        let answered: Result<Answer, String> = serde_json::from_str(line)
            .map_err(|e| failed(format!("the reader's answer did not read: {e}")))?;
        answered.map_err(Refused::Read)
    }
}

/// What a branch changed, read by `reader` (see [`status_here`] for what is read and how).
pub fn status(reader: &Reader, plane: &Path, branch: Branch<'_>) -> Result<Status, Refused> {
    match reader.ask(plane, branch, Ask::Status)? {
        Answer::Status(status) => Ok(status),
        _ => Err(Refused::Read("the reader answered something else".into())),
    }
}

/// How far a branch is from the branch it was cut from, read by `reader` (see
/// [`ahead_behind_here`]): commits ahead and behind, starting no git.
pub fn ahead_behind(
    reader: &Reader,
    plane: &Path,
    branch: Branch<'_>,
) -> Result<AheadBehind, Refused> {
    match reader.ask(plane, branch, Ask::AheadBehind)? {
        Answer::AheadBehind(apart) => Ok(apart),
        _ => Err(Refused::Read("the reader answered something else".into())),
    }
}

/// When this process was started as the reader — [`READ_ARG`] is its last argument — answers
/// the one question on its standard input and says the exit code; otherwise `None`.
///
/// The app's `main` calls this first, before anything else starts.
pub fn serve_if_asked() -> Option<i32> {
    if std::env::args_os().last().as_deref() != Some(std::ffi::OsStr::new(READ_ARG)) {
        return None;
    }
    Some(serve())
}

/// The child: one question in, one answer out.
fn serve() -> i32 {
    let mut asked = Vec::new();
    if std::io::stdin()
        .take(OUTPUT)
        .read_to_end(&mut asked)
        .is_err()
    {
        return 2;
    }
    let Ok(question) = serde_json::from_slice::<Question>(&asked) else {
        return 2;
    };
    watch_own_bounds(question.memory, Duration::from_millis(question.deadline_ms));
    let branch = Branch {
        ws: &question.ws,
        repo: &question.repo,
        piece: question.piece.as_deref(),
    };
    let plane = question.plane.as_path();
    let answered: Result<Answer, String> = match question.ask {
        Ask::Root => root_here(plane, branch).map(Answer::Root),
        Ask::Status => status_here(plane, branch).map(Answer::Status),
        Ask::Folders => folders_here(plane, branch).map(Answer::Folders),
        Ask::Matters(paths) => matters_here(plane, branch, &paths).map(Answer::Matters),
        Ask::AheadBehind => ahead_behind_here(plane, branch).map(Answer::AheadBehind),
        Ask::Compare(comparison) => {
            super::compare::compare_here(plane, branch, &comparison).map(Answer::Compared)
        }
        Ask::CompareFile { sides, path, from } => {
            super::compare::compare_file_here(plane, branch, &sides, &path, from.as_deref())
                .map(Answer::FileDiff)
        }
    }
    .map_err(|refused| refused.to_string());
    let Ok(json) = serde_json::to_string(&answered) else {
        return 2;
    };
    let mut out = std::io::stdout().lock();
    if writeln!(out, "{FRAME}{json}")
        .and_then(|()| out.flush())
        .is_err()
    {
        return 2;
    }
    0
}

/// Stops this process, with no answer, once its resident memory passes `cap` or `deadline` has
/// passed: checked every 10 ms on a thread of its own, so a read swelling at gigabytes a second
/// overshoots by tens of megabytes at most, and a read hung in a blocking open still ends when
/// nobody is left to kill it.
fn watch_own_bounds(cap: u64, deadline: Duration) {
    let started = Instant::now();
    std::thread::spawn(move || {
        loop {
            if let Some(used) = memory_stats::memory_stats()
                && used.physical_mem as u64 > cap
            {
                std::process::exit(3);
            }
            if started.elapsed() >= deadline {
                std::process::exit(4);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    });
}
