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
//! - **a capped answer**: at most [`OUTPUT`] bytes are read back, as JSON;
//! - **a few at once, app-wide**: at most [`AT_ONCE`] readers run at the same time, across every
//!   window and branch. An ask past that waits for one to finish, within its own deadline, so
//!   many branches built to hang their reads cost a fixed number of children, not one each.
//!   Asks are let in the order they came, and one left with too little of its deadline after
//!   the wait is answered as busy rather than started (#1605);
//! - **a pause for a branch that keeps hanging**: one whose reads ran into their deadline or
//!   memory cap twice in a row is not read again for a pause that doubles with each further
//!   strike, so re-reading it cannot keep the places full ([`Strikes`], #1605).
//!
//! A read that is killed or capped fails: the window says so, and the watch counts the batch as
//! one that matters.

use std::collections::{HashMap, VecDeque};
use std::ffi::OsString;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Condvar, Mutex, PoisonError};
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

/// The most readers running at once, across the app (#1130). Each branch already has at most one
/// ignore check and one status read per window running (plus one queued); this bounds the sum
/// over many branches and windows, and so how much memory their children can take together.
pub const AT_ONCE: usize = 6;

/// The readers running now, app-wide.
static READERS: Gate = Gate::new(AT_ONCE);

/// A counting gate: at most `permits` holders at once, let in the order they came, and a wait
/// for a place that gives up.
///
/// **First come, first in** (#1605). Each ask takes its place in a line, and only the first in
/// line is let in when a place comes free: a newcomer never goes ahead of one already waiting,
/// so a waiter cannot lose its turn again and again until its deadline runs out. One that gives
/// up leaves the line, and the one behind it moves up.
#[derive(Debug)]
struct Gate {
    permits: usize,
    line: Mutex<Line>,
    moved: Condvar,
}

/// Who is inside a [`Gate`], and who waits, in the order they came.
#[derive(Debug)]
struct Line {
    inside: usize,
    waiting: VecDeque<u64>,
    /// The ticket the next ask takes.
    next: u64,
}

/// A place in a [`Gate`], given back when dropped.
#[derive(Debug)]
struct Permit<'g>(&'g Gate);

impl Gate {
    const fn new(permits: usize) -> Self {
        Self {
            permits,
            line: Mutex::new(Line {
                inside: 0,
                waiting: VecDeque::new(),
                next: 0,
            }),
            moved: Condvar::new(),
        }
    }

    /// A place, waiting at most `within` for one, behind every ask that came first; nothing
    /// when none came free in time.
    fn enter(&self, within: Duration) -> Option<Permit<'_>> {
        // The line is plain numbers, right whatever a holder did, so a poisoned lock is read.
        let mut line = self.line.lock().unwrap_or_else(PoisonError::into_inner);
        let me = line.next;
        line.next = line.next.wrapping_add(1);
        line.waiting.push_back(me);
        let my_turn = |line: &Line| line.waiting.front() == Some(&me) && line.inside < self.permits;
        let (mut line, _) = self
            .moved
            .wait_timeout_while(line, within, |line| !my_turn(line))
            .unwrap_or_else(PoisonError::into_inner);
        let entered = my_turn(&line);
        if entered {
            line.waiting.pop_front();
            line.inside += 1;
        } else {
            line.waiting.retain(|ticket| *ticket != me);
        }
        drop(line);
        // Either way the line moved: the one now first may go in, if a place is free.
        self.moved.notify_all();
        // Made only when let in: a permit made and dropped would give back a place never taken.
        entered.then(|| Permit(self))
    }
}

impl Drop for Permit<'_> {
    fn drop(&mut self) {
        let mut line = self.0.line.lock().unwrap_or_else(PoisonError::into_inner);
        line.inside = line.inside.saturating_sub(1);
        drop(line);
        // Every waiter checks whether it is first: only that one goes in.
        self.0.moved.notify_all();
    }
}

/// The least of its deadline an ask must have left after waiting for a place, for its child to
/// be started (#1605). With less, the read could only fail as one that "stopped without an
/// answer", which says nothing true; it is answered as [`busy`] instead.
const LEAST_LEFT: Duration = Duration::from_millis(250);

/// How long a branch whose reads ran into their bounds [`STRIKES`] times in a row waits before
/// it is read again, the first time; the pause doubles with each further strike, up to
/// [`LONGEST_PAUSE`] (#1605, D-1605-2).
const FIRST_PAUSE: Duration = Duration::from_secs(30);

/// The longest pause a branch is given.
const LONGEST_PAUSE: Duration = Duration::from_secs(10 * 60);

/// How many reads in a row must run into their bounds before a branch is paused: one hang can
/// be a slow disk, a second in a row is the branch.
const STRIKES: u32 = 2;

/// The most branches [`Strikes`] remembers; past it, a branch whose pause is over is forgotten
/// first, and a new one is not counted until there is room.
const REMEMBERED: usize = 1024;

/// The branches whose reads keep running into their bounds, app-wide.
static STRUCK: Mutex<Option<Strikes>> = Mutex::new(None);

/// A branch as [`Strikes`] knows it: its project, workspace, repo and piece.
type BranchKey = (PathBuf, String, String, Option<String>);

/// **A per-branch back-off** (#1605, D-1605-2). A branch built to hang its read holds one of
/// the [`AT_ONCE`] places for the whole deadline, and the branch watch reads it again after
/// every write: a few such branches, re-read, could keep every place full for the comparisons
/// and searches that share them. So a branch whose reads ran into their bounds (the deadline or
/// the memory cap) [`STRIKES`] times in a row is not read again for a pause, which doubles with
/// each further strike; any answer from its child clears it.
#[derive(Debug, Default)]
struct Strikes {
    by_branch: HashMap<BranchKey, Struck>,
}

#[derive(Debug, Clone, Copy)]
struct Struck {
    times: u32,
    until: Option<Instant>,
}

impl Strikes {
    /// How much longer `branch` is paused at `now`, if it is.
    fn paused(&self, branch: &BranchKey, now: Instant) -> Option<(u32, Duration)> {
        let struck = self.by_branch.get(branch)?;
        let left = struck.until?.checked_duration_since(now)?;
        (!left.is_zero()).then_some((struck.times, left))
    }

    /// `branch`'s read ran into its bounds at `now`.
    fn struck(&mut self, branch: BranchKey, now: Instant) {
        if !self.by_branch.contains_key(&branch) && self.by_branch.len() >= REMEMBERED {
            self.by_branch
                .retain(|_, struck| struck.until.is_some_and(|until| until > now));
            if self.by_branch.len() >= REMEMBERED {
                return;
            }
        }
        let struck = self.by_branch.entry(branch).or_insert(Struck {
            times: 0,
            until: None,
        });
        struck.times = struck.times.saturating_add(1);
        if struck.times >= STRIKES {
            let doublings = (struck.times - STRIKES).min(16);
            let pause = FIRST_PAUSE
                .saturating_mul(1 << doublings)
                .min(LONGEST_PAUSE);
            struck.until = Some(now + pause);
        }
    }

    /// `branch`'s child answered: it is read as any other from now on.
    fn answered(&mut self, branch: &BranchKey) {
        self.by_branch.remove(branch);
    }
}

/// Runs `with` on the app-wide [`Strikes`].
fn strikes<T>(with: impl FnOnce(&mut Strikes) -> T) -> T {
    // Plain counts, right whatever a holder did, so a poisoned lock is read.
    let mut held = STRUCK.lock().unwrap_or_else(PoisonError::into_inner);
    with(held.get_or_insert_with(Strikes::default))
}

/// What a paused branch is answered with: a read that gave no answer, and when it is read again.
fn paused(times: u32, left: Duration) -> Refused {
    Refused::Read(format!(
        "{READ_FAILED}its last {times} reads ran past their time or memory, so it is read again \
         in {}",
        seconds((left.as_secs() + u64::from(left.subsec_nanos() > 0)).max(1))
    ))
}

/// `n` seconds, as a sentence says it.
fn seconds(n: u64) -> String {
    if n == 1 {
        "1 second".to_string()
    } else {
        format!("{n} seconds")
    }
}

/// How long a child was given, as a sentence says it: whole seconds when it was, else to a
/// tenth (#1605: the time the child had, not the ask's whole deadline).
fn given(deadline: Duration) -> String {
    if deadline.subsec_millis() == 0 {
        seconds(deadline.as_secs())
    } else {
        format!("{:.1} seconds", deadline.as_secs_f64())
    }
}

/// What follows [`READ_FAILED`] when an ask found every reader place taken for as long as its
/// deadline.
const BUSY: &str = "it was busy reading other branches for ";

/// An ask that found every reader place taken for as long as its deadline.
fn busy(deadline: Duration) -> Refused {
    Refused::Read(format!("{READ_FAILED}{BUSY}{} seconds", deadline.as_secs()))
}

/// Whether `why` is an ask that got no place among the [`AT_ONCE`] readers in time, rather than
/// one whose read failed: asked again later, it may well answer.
pub fn was_busy(why: &Refused) -> bool {
    matches!(why, Refused::Read(said) if said.strip_prefix(READ_FAILED).is_some_and(|rest| rest.starts_with(BUSY)))
}

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
    ///
    /// The wait for a place among the [`AT_ONCE`] readers counts against the deadline: the child
    /// is given what is left of it, and is not started when that is under [`LEAST_LEFT`]. A
    /// branch whose reads keep running into their bounds is paused ([`Strikes`]), and answered
    /// at once, taking no place.
    pub fn ask(&self, plane: &Path, branch: Branch<'_>, ask: Ask) -> Result<Answer, Refused> {
        let key: BranchKey = (
            plane.to_path_buf(),
            branch.ws.to_string(),
            branch.repo.to_string(),
            branch.piece.map(str::to_string),
        );
        if let Some((times, left)) = strikes(|struck| struck.paused(&key, Instant::now())) {
            return Err(paused(times, left));
        }
        let queued = Instant::now();
        let _permit = READERS
            .enter(self.deadline)
            .ok_or_else(|| busy(self.deadline))?;
        let deadline = self.deadline.saturating_sub(queued.elapsed());
        if deadline < LEAST_LEFT {
            return Err(busy(self.deadline));
        }
        let read = self.read(plane, branch, ask, deadline);
        match &read {
            Ended::Answered(_) => strikes(|struck| struck.answered(&key)),
            ended if a_strike(ended, deadline, self.deadline) => {
                strikes(|struck| struck.struck(key, Instant::now()));
            }
            _ => {}
        }
        match read {
            Ended::Answered(answered) => answered,
            Ended::PastDeadline(why) | Ended::PastMemory(why) | Ended::Failed(why) => Err(why),
        }
    }

    /// One child, given `deadline`, asked `ask`.
    fn read(&self, plane: &Path, branch: Branch<'_>, ask: Ask, deadline: Duration) -> Ended {
        let question = Question {
            plane: plane.to_path_buf(),
            ws: branch.ws.to_string(),
            repo: branch.repo.to_string(),
            piece: branch.piece.map(str::to_string),
            ask,
            memory: self.memory,
            deadline_ms: u64::try_from(deadline.as_millis()).unwrap_or(u64::MAX),
        };
        let asked = match serde_json::to_vec(&question) {
            Ok(asked) => asked,
            Err(e) => {
                return Ended::Failed(Refused::Read(format!(
                    "purlis could not ask the reader: {e}"
                )));
            }
        };
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
        let mut child = match crate::forklock::spawn(&mut command) {
            Ok(child) => child,
            Err(e) => return Ended::Failed(failed(format!("{e}"))),
        };
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
        // What the child was given, as the failure says it: not the ask's whole deadline, part
        // of which may have gone waiting for a place (#1605).
        let within = given(deadline);
        let started = Instant::now();
        let exited = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if started.elapsed() >= deadline + GRACE => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Ended::PastDeadline(failed(format!(
                        "the read did not finish within {within}"
                    )));
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(5)),
                Err(e) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Ended::Failed(failed(e.to_string()));
                }
            }
        };
        let out = reading.join().unwrap_or_default();
        if out.len() as u64 > OUTPUT {
            return Ended::Failed(failed("the answer was too large".into()));
        }
        let text = String::from_utf8_lossy(&out);
        let Some((_, framed)) = text.rsplit_once(FRAME) else {
            let stopped = "the read stopped without an answer";
            return match exited.code() {
                Some(STOPPED_PAST_DEADLINE) => Ended::PastDeadline(failed(format!(
                    "{stopped}: it did not finish within {within}"
                ))),
                Some(STOPPED_PAST_MEMORY) => Ended::PastMemory(failed(format!(
                    "{stopped}: it took more than {} MiB of memory",
                    self.memory / (1024 * 1024)
                ))),
                _ => Ended::Failed(failed(format!(
                    "{stopped}: it took too long or too much memory, or failed"
                ))),
            };
        };
        let line = framed.lines().next().unwrap_or_default();
        match serde_json::from_str::<Result<Answer, String>>(line) {
            Ok(answered) => Ended::Answered(answered.map_err(Refused::Read)),
            Err(e) => Ended::Failed(failed(format!("the reader's answer did not read: {e}"))),
        }
    }
}

/// Whether a child given `given` of a reader's whole `deadline` had a fair try, so that its
/// running into its bounds counts against its branch ([`Strikes`]): at least half of it. One
/// that waited most of its deadline for a place, on a busy machine, ran out of the time the
/// line left it and says nothing about the branch, so a healthy branch is never paused for the
/// load around it.
fn a_fair_try(given: Duration, deadline: Duration) -> bool {
    given >= deadline / 2
}

/// Whether a read that ended as `ended`, its child given `given` of the reader's whole
/// `deadline`, counts against its branch ([`Strikes`]). Running past the memory cap always does:
/// how much memory a read takes is the branch's own doing, whatever time the line left it.
/// Running past the deadline does only on [`a_fair_try`]. An answer or another failure never
/// does.
fn a_strike(ended: &Ended, given: Duration, deadline: Duration) -> bool {
    match ended {
        Ended::PastMemory(_) => true,
        Ended::PastDeadline(_) => a_fair_try(given, deadline),
        Ended::Answered(_) | Ended::Failed(_) => false,
    }
}

/// How one child's read ended.
#[derive(Debug)]
enum Ended {
    /// The child answered: what it read, or the refusal it read instead.
    Answered(Result<Answer, Refused>),
    /// It ran into its deadline: a strike against the branch on a fair try ([`a_strike`]).
    PastDeadline(Refused),
    /// It ran into its memory cap: always a strike against the branch ([`a_strike`]).
    PastMemory(Refused),
    /// It failed otherwise: it could not start, or its answer did not read.
    Failed(Refused),
}

/// The exit code of a child that stopped itself past its memory cap.
const STOPPED_PAST_MEMORY: i32 = 3;

/// The exit code of a child that stopped itself past its deadline.
const STOPPED_PAST_DEADLINE: i32 = 4;

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
                std::process::exit(STOPPED_PAST_MEMORY);
            }
            if started.elapsed() >= deadline {
                std::process::exit(STOPPED_PAST_DEADLINE);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn no_more_readers_run_at_once_than_the_gate_lets_in() {
        let gate = Arc::new(Gate::new(3));
        let running = Arc::new(AtomicUsize::new(0));
        let most = Arc::new(AtomicUsize::new(0));
        let asks: Vec<_> = (0..4)
            .map(|_| {
                let (gate, running, most) = (gate.clone(), running.clone(), most.clone());
                std::thread::spawn(move || {
                    let _permit = gate.enter(Duration::from_secs(10)).expect("let in");
                    let now = running.fetch_add(1, Ordering::SeqCst) + 1;
                    most.fetch_max(now, Ordering::SeqCst);
                    std::thread::sleep(Duration::from_millis(100));
                    running.fetch_sub(1, Ordering::SeqCst);
                })
            })
            .collect();
        for ask in asks {
            ask.join().unwrap();
        }
        // Three at once, and the fourth went in once one had left.
        assert_eq!(most.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn a_queued_ask_gives_up_within_its_own_deadline() {
        let gate = Gate::new(1);
        let held = gate
            .enter(Duration::from_secs(1))
            .expect("the first is let in");
        let started = Instant::now();
        assert!(gate.enter(Duration::from_millis(50)).is_none());
        let waited = started.elapsed();
        assert!(waited >= Duration::from_millis(50), "{waited:?}");
        assert!(waited < Duration::from_secs(1), "{waited:?}");
        drop(held);
        assert!(gate.enter(Duration::from_millis(50)).is_some());
    }

    #[test]
    fn a_busy_reader_fails_as_a_read_that_gave_no_answer() {
        let busy = busy(Duration::from_secs(30));
        let Refused::Read(said) = &busy else {
            panic!("{busy:?}")
        };
        assert!(said.starts_with(READ_FAILED), "{said}");
        assert_eq!(
            said,
            "purlis could not read the branch: it was busy reading other branches for 30 seconds"
        );
        assert!(was_busy(&busy));
        assert!(!was_busy(&Refused::Read(format!(
            "{READ_FAILED}the read did not finish within 30 seconds"
        ))));
    }

    /// #1605: a gate of one lets its waiters in the order they came, whoever wakes first.
    #[test]
    fn waiters_are_let_in_the_order_they_came() {
        let gate = Arc::new(Gate::new(1));
        let held = gate
            .enter(Duration::from_secs(1))
            .expect("the first is let in");
        let order = Arc::new(Mutex::new(Vec::new()));
        let waiting = |gate: &Gate| gate.line.lock().unwrap().waiting.len();
        let mut asks = Vec::new();
        for who in 0..5 {
            let (gate2, order) = (gate.clone(), order.clone());
            asks.push(std::thread::spawn(move || {
                let _permit = gate2.enter(Duration::from_secs(20)).expect("let in");
                order.lock().unwrap().push(who);
                std::thread::sleep(Duration::from_millis(5));
            }));
            // Each takes its place in line before the next one comes.
            while waiting(&gate) <= who {
                std::thread::sleep(Duration::from_millis(1));
            }
        }
        // A newcomer arriving as the place comes free still goes behind the line.
        drop(held);
        let late = gate.enter(Duration::from_secs(20)).expect("let in last");
        order.lock().unwrap().push(99);
        drop(late);
        for ask in asks {
            ask.join().unwrap();
        }
        assert_eq!(*order.lock().unwrap(), [0, 1, 2, 3, 4, 99]);
    }

    /// #1605: one that gives up leaves the line, and the one behind it moves up.
    #[test]
    fn a_waiter_that_gave_up_does_not_hold_up_the_line() {
        let gate = Arc::new(Gate::new(1));
        let held = gate
            .enter(Duration::from_secs(1))
            .expect("the first is let in");
        assert!(gate.enter(Duration::from_millis(20)).is_none());
        let behind = {
            let gate = gate.clone();
            std::thread::spawn(move || gate.enter(Duration::from_secs(20)).is_some())
        };
        let waiting = |gate: &Gate| gate.line.lock().unwrap().waiting.len();
        while waiting(&gate) == 0 {
            std::thread::sleep(Duration::from_millis(1));
        }
        drop(held);
        assert!(behind.join().unwrap());
        assert_eq!(waiting(&gate), 0);
    }

    /// #1605: an ask with less than [`LEAST_LEFT`] of its deadline starts no child, and is
    /// answered as busy.
    #[test]
    fn an_ask_with_too_little_of_its_deadline_left_starts_no_child() {
        // A program that does not exist: a child started would fail as one that could not start.
        let reader = Reader::new(
            PathBuf::from("/nonexistent/purlis-reader"),
            [OsString::from(READ_ARG)],
        )
        .deadline(LEAST_LEFT / 2);
        let plane = tempfile::tempdir().unwrap();

        let said = reader
            .ask(plane.path(), Branch::repo("alpha", "svc"), Ask::Root)
            .expect_err("no answer");

        assert!(was_busy(&said), "{said:?}");
    }

    /// #1605: only a child given at least half the deadline counts against its branch.
    #[test]
    fn a_read_the_line_left_little_time_is_no_strike() {
        let whole = Duration::from_secs(30);
        assert!(a_fair_try(whole, whole));
        assert!(a_fair_try(Duration::from_secs(15), whole));
        assert!(!a_fair_try(Duration::from_millis(14_999), whole));
        assert!(!a_fair_try(LEAST_LEFT, whole));
    }

    /// #1605: a memory-cap stop is the branch's own doing however little time the child had,
    /// so it always counts; running out of time counts only on a fair try.
    #[test]
    fn a_memory_cap_stop_always_counts_and_a_deadline_only_on_a_fair_try() {
        let whole = Duration::from_secs(30);
        let why = || Refused::Read(String::new());
        assert!(a_strike(&Ended::PastMemory(why()), LEAST_LEFT, whole));
        assert!(a_strike(&Ended::PastMemory(why()), whole, whole));
        assert!(a_strike(&Ended::PastDeadline(why()), whole, whole));
        assert!(!a_strike(&Ended::PastDeadline(why()), LEAST_LEFT, whole));
        assert!(!a_strike(&Ended::Failed(why()), whole, whole));
        assert!(!a_strike(&Ended::Answered(Err(why())), whole, whole));
    }

    /// #1605: a failure names the time the child had, not the ask's whole deadline.
    #[test]
    fn a_failure_names_the_time_the_child_was_given() {
        assert_eq!(given(Duration::from_secs(30)), "30 seconds");
        assert_eq!(given(Duration::from_secs(1)), "1 second");
        assert_eq!(given(Duration::from_millis(29_350)), "29.4 seconds");
        assert_eq!(given(Duration::from_millis(400)), "0.4 seconds");
    }

    fn key(piece: &str) -> BranchKey {
        (
            PathBuf::from("/plane"),
            "alpha".into(),
            "svc".into(),
            Some(piece.into()),
        )
    }

    /// #1605 (D-1605-2): one hang is not enough; the second in a row pauses the branch, and the
    /// pause doubles with each further one, up to its longest.
    #[test]
    fn a_branch_that_keeps_hanging_is_paused_for_longer_each_time() {
        let mut strikes = Strikes::default();
        let now = Instant::now();

        strikes.struck(key("hung"), now);
        assert_eq!(strikes.paused(&key("hung"), now), None);

        strikes.struck(key("hung"), now);
        assert_eq!(strikes.paused(&key("hung"), now), Some((2, FIRST_PAUSE)));
        assert_eq!(strikes.paused(&key("other"), now), None);
        assert_eq!(strikes.paused(&key("hung"), now + FIRST_PAUSE), None);

        strikes.struck(key("hung"), now);
        assert_eq!(
            strikes.paused(&key("hung"), now),
            Some((3, FIRST_PAUSE * 2))
        );
        for _ in 0..40 {
            strikes.struck(key("hung"), now);
        }
        assert_eq!(
            strikes.paused(&key("hung"), now).map(|p| p.1),
            Some(LONGEST_PAUSE)
        );
    }

    /// #1605: any answer from its child clears a branch's strikes.
    #[test]
    fn an_answer_clears_a_branchs_strikes() {
        let mut strikes = Strikes::default();
        let now = Instant::now();
        strikes.struck(key("hung"), now);
        strikes.answered(&key("hung"));
        strikes.struck(key("hung"), now);
        assert_eq!(strikes.paused(&key("hung"), now), None);
    }

    /// #1605: the strikes remembered are bounded; a branch whose pause is over goes first.
    #[test]
    fn the_strikes_remembered_are_bounded() {
        let mut strikes = Strikes::default();
        let now = Instant::now();
        for n in 0..REMEMBERED {
            strikes.struck(key(&n.to_string()), now);
        }
        strikes.struck(key("new"), now);
        assert_eq!(strikes.by_branch.len(), 1);
        for _ in 0..2 {
            strikes.struck(key("hung"), now);
        }
        assert!(strikes.paused(&key("hung"), now).is_some());
    }

    /// A paused branch is a read that gave no answer, not a busy one: the branch watch does not
    /// ask it again at once.
    #[test]
    fn a_paused_branch_is_said_with_when_it_is_read_again() {
        let said = paused(2, Duration::from_millis(29_500));
        let Refused::Read(text) = &said else {
            panic!("{said:?}")
        };
        assert_eq!(
            text,
            "purlis could not read the branch: its last 2 reads ran past their time or memory, \
             so it is read again in 30 seconds"
        );
        assert!(!was_busy(&said));
    }
}
