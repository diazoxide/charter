//! **A teammate's project grant, when it arrives and when it goes** (#1506, spec #1483; ADR
//! 0090 as amended): what of the project's `[dispatch.grants]` is waiting for this person's
//! answer, and what this machine's earlier yes is bound to.
//!
//! # Arrival
//!
//! [`arrival`] is what the window's Notice shows: every pair, and every "any persona" grant,
//! that the project's file holds and this person has neither accepted nor said "Not on my
//! machine" to. Nothing in it is in force here until it is accepted. A grant that names a
//! persona this checkout does not define is listed as such ([`Arrived::undefined`]), never as
//! one that can be used. A declined grant is never listed: Settings is where it is changed.
//!
//! # What a yes is bound to
//!
//! An acceptance kept on a machine must not outlive what it accepted, and a file that was
//! changed and changed back reads the same. So the acceptances are kept with the commit they
//! were last checked through (`dispatch_seen_at`), and [`settle`] reads the project's history
//! from that commit to the one checked out now:
//!
//! - **A transition that took a grant out drops its acceptance**: a commit whose project file
//!   lacks the grant while a parent's holds it ([`taken_out`]). Every line of history that
//!   was merged in is read, and the project's file is read under every name purlis accepts
//!   for it, so neither a merge nor a renamed file hides a removal. A decline goes the same
//!   way, so a grant put back is told again.
//! - **A history that cannot be read drops every acceptance** ([`Between::Unreadable`]): the
//!   kept commit is gone or shares nothing with this one, more commits changed the file than
//!   [`MOST_COMMITS`], an object is missing or too large, the repository has grafts or is too
//!   shallow to hold the range, or git ran out of time. Nothing is assumed to have stayed.
//! - **Where nothing answers, nothing is in force** ([`Verdict::read`] is false): git could
//!   not be run, the commit checked out could not be read, or this machine's record could not
//!   be written. Nothing stored changes, and no acceptance of the project's grants counts
//!   until a settling answers.
//!
//! What was dropped is remembered with why (`dispatch_gone`), so the person is told once that
//! the project took a grant away, and a grant that waits again says the true reason
//! ([`Again`]).
//!
//! **Absent from the working file is not a removal.** A grant the file on disk does not hold
//! is not in force, and that is worked out at each read; its acceptance is not dropped and
//! nothing is told. So a branch switched away and back moves nothing. Only a commit that took
//! the grant out, or a history that cannot be read, drops anything.
//!
//! # Limits, said plainly
//!
//! - The check is over the history as this machine has it. A history rewritten so that the
//!   removal is in no commit here (a force-push to a line that never lost the grant) shows
//!   no transition.
//! - A hand's edit that takes a grant out of the working file and puts it back, with no
//!   commit, is not a transition either. That is this machine's own checkout.
//! - A project in no git repository has no history to read: its acceptances stand while the
//!   file holds the grant.
//!
//! # One settling at a time, and never under the decision
//!
//! A settling has **one deadline** for all of its git work ([`DEADLINE`]) and reads objects
//! through one `git cat-file --batch`, each under a size cap. There is **one per project at a
//! time**: a caller that arrives while one runs waits for it and takes its answer
//! ([`settle`]). **Reading the grants in force runs no git**: [`for_read`] answers the last
//! settling's verdict, held under this module's own lock for the length of a copy. The app
//! settles before it takes the lock a dispatch is decided under, and off the watcher's
//! thread.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex, PoisonError, mpsc};
use std::time::{Duration, Instant};

use crate::dispatchgrant::{self, ANY};
use crate::sandbox::local::{self, Gone};

/// The most commits that changed the project's file one settling reads. Past it the history
/// is taken as unread.
pub const MOST_COMMITS: usize = 2000;

/// The most one version of the project's file may be, in bytes. A larger one is unread.
pub const MOST_FILE_BYTES: u64 = 1 << 20;

/// The most a commit or a folder's listing may be, in bytes.
const MOST_OBJECT_BYTES: u64 = 16 << 20;

/// How long one settling has for all of its git work: as long as any one read of a repository
/// here ([`crate::worktree::git::READ`]), since a machine under load is slow, not broken.
pub const DEADLINE: Duration = crate::worktree::git::READ;

/// A project grant as this machine's record spells it: a pair as
/// [`dispatchgrant::Pair`] is displayed, `<asking> -> *` for "any persona", and a grant
/// limited to one workspace as [`crate::dispatchwithin::Limited`] is displayed
/// (`<asking> -> <target> in <workspace>`).
fn any_said(asking: &str) -> String {
    format!("{asking} -> {ANY}")
}

/// Every grant `text`, a project file whole, holds, each as the record spells it. A file that
/// is absent or does not parse holds none.
fn grants_in(text: Option<&str>) -> Vec<String> {
    let committed = dispatchgrant::committed(text);
    committed
        .any
        .iter()
        .map(|asking| any_said(asking))
        .chain(committed.pairs.iter().map(ToString::to_string))
        // Limited to one workspace (#1505): bound to the history as every other grant is.
        .chain(committed.limited.iter().map(ToString::to_string))
        .collect()
}

/// The project's file at `root` as it is now, where it was read and parsed.
fn file_now(root: &Path) -> Option<String> {
    let text = crate::sandbox::read_plane_file(&crate::names::manifest(root))
        .ok()
        .flatten()?;
    text.parse::<toml::Table>().is_ok().then_some(text)
}

// ---- the history -------------------------------------------------------------------------------

/// The commit the project's checkout is at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Head {
    /// The project is in no git repository, or one with no commit yet: there is no history.
    None,
    /// This commit, by its full id.
    At(String),
    /// Nothing answered: git could not be run, ran out of time, or refused. This is not "no
    /// repository".
    Unanswered,
}

/// What the project's history says happened to its grants between two commits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Between {
    /// The history was read. These grants were taken out by some commit in it.
    TakenOut(Vec<String>),
    /// The history does not answer (the module's doc lists how). Nothing is assumed to have
    /// stayed.
    Unreadable,
    /// git could not be run at all. Nothing is changed and nothing counts for this read.
    Unanswered,
}

/// The project's history, as [`settle_with`] reads it. `until` is the settling's one deadline.
pub trait History {
    fn head(&self, until: Instant) -> Head;
    fn between(&self, from: &str, to: &str, until: Instant) -> Between;
}

/// One commit of a range: the grants its project file holds, and the grants each parent's
/// holds.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Step {
    pub holds: Vec<String>,
    pub parents: Vec<Vec<String>>,
}

/// **The grants some commit of `steps` took out**: each one a commit lacks while a parent of
/// that commit holds it. A commit on a branch that never held a grant took nothing out, so a
/// branch that predates a grant and is merged later drops nothing; a commit that removes one
/// on any merged line does, whatever the merge made of it.
pub fn taken_out(steps: &[Step]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for step in steps {
        for held in step.parents.iter().flatten() {
            if !step.holds.contains(held) && !out.contains(held) {
                out.push(held.clone());
            }
        }
    }
    out
}

/// Whether `said` is a commit's full id as git prints one: the only thing ever handed to git
/// as a revision, since the stored one is read from a file a person can edit.
fn is_commit_id(said: &str) -> bool {
    matches!(said.len(), 40 | 64) && said.bytes().all(|b| b.is_ascii_hexdigit())
}

/// The history git keeps for the project at this root.
pub struct Git<'a> {
    root: &'a Path,
    /// What [`History::head`] learned of the repository, for [`History::between`].
    found: Mutex<Option<Found>>,
}

#[derive(Debug, Clone)]
struct Found {
    /// The project's folder inside the repository, `/` between its parts, empty at its top.
    prefix: String,
    shallow: bool,
    /// Where the repository would keep grafts.
    grafts: PathBuf,
}

/// What one git call of a settling came to.
enum Ran {
    Answered(i32, String, String),
    TimedOut,
    NotRun,
}

/// A reading of the history that failed.
enum Fault {
    Unreadable,
    NotRun,
}

impl From<Fault> for Between {
    fn from(fault: Fault) -> Self {
        match fault {
            Fault::Unreadable => Self::Unreadable,
            Fault::NotRun => Self::Unanswered,
        }
    }
}

/// History is read as it was written: no object stands in for another.
const AS_WRITTEN: &str = "--no-replace-objects";

impl<'a> Git<'a> {
    pub fn new(root: &'a Path) -> Self {
        Self {
            root,
            found: Mutex::new(None),
        }
    }

    fn run(&self, args: &[&str], until: Instant) -> Ran {
        let left = until.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Ran::TimedOut;
        }
        match crate::worktree::git::run(self.root, args, left) {
            Ok(run) => match run.code {
                Some(code) => Ran::Answered(code, run.out, run.err),
                None => Ran::TimedOut,
            },
            Err(_) => Ran::NotRun,
        }
    }

    fn steps(&self, from: &str, to: &str, until: Instant) -> Result<Vec<Step>, Fault> {
        if !is_commit_id(from) || !is_commit_id(to) {
            return Err(Fault::Unreadable);
        }
        let found = self
            .found
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
            .ok_or(Fault::Unreadable)?;
        // Grafts give a commit parents it was not written with.
        if found.grafts.exists() {
            return Err(Fault::Unreadable);
        }
        // The two commits must share history, and in a shallow repository the earlier one
        // must be an ancestor: past a shallow boundary there are commits this machine cannot
        // read.
        let base = match self.run(&[AS_WRITTEN, "merge-base", from, to], until) {
            Ran::Answered(0, out, _) => out.trim().to_owned(),
            Ran::Answered(..) | Ran::TimedOut => return Err(Fault::Unreadable),
            Ran::NotRun => return Err(Fault::NotRun),
        };
        if !is_commit_id(&base) || (found.shallow && base != from) {
            return Err(Fault::Unreadable);
        }
        let most = format!("--max-count={}", MOST_COMMITS + 1);
        let range = format!("{from}..{to}");
        let mut args = vec![
            AS_WRITTEN.to_owned(),
            "rev-list".to_owned(),
            "--full-history".to_owned(),
            most,
            range,
            "--".to_owned(),
        ];
        // Every name the project's file may have: a removal is not hidden by renaming it.
        args.extend(
            crate::names::PLANE_MANIFEST
                .spellings()
                .map(|name| format!("./{name}")),
        );
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let listed = match self.run(&args, until) {
            Ran::Answered(0, out, _) => out,
            Ran::Answered(..) | Ran::TimedOut => return Err(Fault::Unreadable),
            Ran::NotRun => return Err(Fault::NotRun),
        };
        let commits: Vec<&str> = listed.lines().map(str::trim).collect();
        if commits.len() > MOST_COMMITS || commits.iter().any(|one| !is_commit_id(one)) {
            return Err(Fault::Unreadable);
        }
        if commits.is_empty() {
            return Ok(Vec::new());
        }
        let mut objects = Objects::open(self.root, &found.prefix, until)?;
        let mut steps = Vec::with_capacity(commits.len());
        for commit in commits {
            let (holds, parents) = objects.commit(commit)?;
            let mut step = Step {
                holds,
                parents: Vec::with_capacity(parents.len()),
            };
            for parent in parents {
                step.parents.push(objects.commit(&parent)?.0);
            }
            steps.push(step);
        }
        Ok(steps)
    }
}

impl History for Git<'_> {
    fn head(&self, until: Instant) -> Head {
        let asked = self.run(
            &[
                "rev-parse",
                "--show-prefix",
                "--is-shallow-repository",
                "--git-path",
                "info/grafts",
                "HEAD^{commit}",
            ],
            until,
        );
        let Ran::Answered(code, out, err) = asked else {
            return Head::Unanswered;
        };
        let lines: Vec<&str> = out.lines().collect();
        if code != 0 {
            // git's own words for the two states that are not a failure. Every other refusal
            // (a repository git will not open, a `.git` link purlis will not follow) is one.
            let no_repository = lines.is_empty() && err.contains("not a git repository");
            let no_commit = lines.len() == 3
                && (err.contains("unknown revision") || err.contains("bad revision"));
            return if no_repository || no_commit {
                Head::None
            } else {
                Head::Unanswered
            };
        }
        let [prefix, shallow, grafts, id] = lines[..] else {
            return Head::Unanswered;
        };
        if !is_commit_id(id) {
            return Head::Unanswered;
        }
        let grafts = Path::new(grafts);
        *self.found.lock().unwrap_or_else(PoisonError::into_inner) = Some(Found {
            prefix: prefix.to_owned(),
            shallow: shallow != "false",
            grafts: if grafts.is_absolute() {
                grafts.to_path_buf()
            } else {
                self.root.join(grafts)
            },
        });
        Head::At(id.to_owned())
    }

    fn between(&self, from: &str, to: &str, until: Instant) -> Between {
        match self.steps(from, to, until) {
            Ok(steps) => Between::TakenOut(taken_out(&steps)),
            Err(fault) => fault.into(),
        }
    }
}

/// **One `git cat-file --batch`, asked for objects by id, under the settling's deadline.**
/// Asking by id is what keeps "this commit has no such file" apart from "the object could not
/// be read": a folder's listing either names the file or does not, and an id git cannot
/// produce is a fault, never an absence.
struct Objects {
    child: Arc<Mutex<std::process::Child>>,
    to: Option<std::process::ChildStdin>,
    from: BufReader<std::process::ChildStdout>,
    /// Dropped to tell the watchdog the read is over.
    over: Option<mpsc::Sender<()>>,
    prefix: Vec<String>,
    /// Each commit read: the grants its project file holds, and its parents.
    commits: HashMap<String, (Vec<String>, Vec<String>)>,
    /// The grants of the project file in each folder listing read, by the listing's id.
    folders: HashMap<String, Vec<String>>,
}

impl Objects {
    fn open(root: &Path, prefix: &str, until: Instant) -> Result<Self, Fault> {
        let left = until.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err(Fault::Unreadable);
        }
        let mut child =
            match crate::worktree::git::spawn_fed(root, &[AS_WRITTEN, "cat-file", "--batch"]) {
                Ok(Ok(child)) => child,
                Ok(Err(_)) => return Err(Fault::Unreadable),
                Err(_) => return Err(Fault::NotRun),
            };
        let to = child.stdin.take();
        let from = child.stdout.take().ok_or(Fault::Unreadable)?;
        // Nothing reads what it says of its troubles: an object it cannot give is said on the
        // stream that is read.
        drop(child.stderr.take());
        let child = Arc::new(Mutex::new(child));
        let (over, waited) = mpsc::channel::<()>();
        {
            let child = Arc::clone(&child);
            std::thread::spawn(move || {
                if waited.recv_timeout(left) == Err(mpsc::RecvTimeoutError::Timeout) {
                    // Past the deadline: stopped, so the read that is waiting on it ends.
                    // Never one that was waited for already: its id may be another's by now.
                    let mut child = child.lock().unwrap_or_else(PoisonError::into_inner);
                    if matches!(child.try_wait(), Ok(None)) {
                        crate::worktree::git::stop(&mut child);
                    }
                }
            });
        }
        Ok(Self {
            child,
            to,
            from: BufReader::new(from),
            over: Some(over),
            prefix: prefix
                .split('/')
                .filter(|part| !part.is_empty())
                .map(str::to_owned)
                .collect(),
            commits: HashMap::new(),
            folders: HashMap::new(),
        })
    }

    /// The object `id`, which must be a `kind` of at most `most` bytes.
    fn object(&mut self, id: &str, kind: &str, most: u64) -> Result<Vec<u8>, Fault> {
        let to = self.to.as_mut().ok_or(Fault::Unreadable)?;
        writeln!(to, "{id}")
            .and_then(|()| to.flush())
            .map_err(|_| Fault::Unreadable)?;
        let mut header = String::new();
        self.from
            .read_line(&mut header)
            .map_err(|_| Fault::Unreadable)?;
        let mut words = header.split_whitespace();
        let (Some(_), Some(is), Some(size), None) =
            (words.next(), words.next(), words.next(), words.next())
        else {
            // "<id> missing", or the stream ended: the deadline, or git gave up.
            return Err(Fault::Unreadable);
        };
        let size: u64 = size.parse().map_err(|_| Fault::Unreadable)?;
        if is != kind || size > most {
            return Err(Fault::Unreadable);
        }
        let mut bytes = vec![0; usize::try_from(size).map_err(|_| Fault::Unreadable)?];
        self.from
            .read_exact(&mut bytes)
            .map_err(|_| Fault::Unreadable)?;
        let mut end = [0; 1];
        self.from
            .read_exact(&mut end)
            .map_err(|_| Fault::Unreadable)?;
        Ok(bytes)
    }

    /// The grants commit `id`'s project file holds, and the commit's parents.
    fn commit(&mut self, id: &str) -> Result<(Vec<String>, Vec<String>), Fault> {
        if let Some(read) = self.commits.get(id) {
            return Ok(read.clone());
        }
        if !is_commit_id(id) {
            return Err(Fault::Unreadable);
        }
        let bytes = self.object(id, "commit", MOST_OBJECT_BYTES)?;
        let text = String::from_utf8_lossy(&bytes);
        let mut tree = None;
        let mut parents = Vec::new();
        for line in text.lines().take_while(|line| !line.is_empty()) {
            match line.split_once(' ') {
                Some(("tree", id)) if is_commit_id(id) => tree = Some(id.to_owned()),
                Some(("parent", id)) if is_commit_id(id) => parents.push(id.to_owned()),
                Some(("tree" | "parent", _)) => return Err(Fault::Unreadable),
                _ => {}
            }
        }
        let holds = self.holds(&tree.ok_or(Fault::Unreadable)?)?;
        let read = (holds, parents);
        self.commits.insert(id.to_owned(), read.clone());
        Ok(read)
    }

    /// The grants of the project file under the listing `top` of a commit's whole tree.
    fn holds(&mut self, top: &str) -> Result<Vec<String>, Fault> {
        let mut at = top.to_owned();
        for part in self.prefix.clone() {
            let found = self
                .listing(&at)?
                .into_iter()
                .find(|entry| entry.name == part.as_bytes() && entry.mode == b"40000");
            match found {
                Some(entry) => at = entry.id,
                // The project's folder is not in this commit: it had no project file.
                None => return Ok(Vec::new()),
            }
        }
        if let Some(read) = self.folders.get(&at) {
            return Ok(read.clone());
        }
        let entries = self.listing(&at)?;
        let mut holds = Vec::new();
        // As the names module reads a folder: the first spelling that is a file there.
        for name in crate::names::PLANE_MANIFEST.spellings() {
            let Some(entry) = entries.iter().find(|entry| entry.name == name.as_bytes()) else {
                continue;
            };
            match &entry.mode[..] {
                b"100644" | b"100755" => {
                    let bytes = self.object(&entry.id, "blob", MOST_FILE_BYTES)?;
                    holds = grants_in(std::str::from_utf8(&bytes).ok());
                    break;
                }
                // A link is this spelling, and the project's file is never read through one.
                b"120000" => break,
                // A folder of that name is no file: the next spelling is looked for.
                _ => {}
            }
        }
        self.folders.insert(at, holds.clone());
        Ok(holds)
    }

    /// The entries of the folder listing `id`.
    fn listing(&mut self, id: &str) -> Result<Vec<Entry>, Fault> {
        let bytes = self.object(id, "tree", MOST_OBJECT_BYTES)?;
        let width = id.len() / 2;
        let mut entries = Vec::new();
        let mut rest = &bytes[..];
        while !rest.is_empty() {
            let space = rest
                .iter()
                .position(|b| *b == b' ')
                .ok_or(Fault::Unreadable)?;
            let nul = rest
                .iter()
                .position(|b| *b == 0)
                .filter(|nul| *nul > space)
                .ok_or(Fault::Unreadable)?;
            let end = nul + 1 + width;
            if rest.len() < end {
                return Err(Fault::Unreadable);
            }
            entries.push(Entry {
                mode: rest[..space].to_vec(),
                name: rest[space + 1..nul].to_vec(),
                id: rest[nul + 1..end]
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect(),
            });
            rest = &rest[end..];
        }
        Ok(entries)
    }
}

impl Drop for Objects {
    fn drop(&mut self) {
        // The watchdog is told first. Then its input ends, so it ends; and it is waited for,
        // by its own id, unless the watchdog did that already.
        drop(self.over.take());
        drop(self.to.take());
        let mut child = self.child.lock().unwrap_or_else(PoisonError::into_inner);
        if matches!(child.try_wait(), Ok(None)) {
            crate::worktree::git::stop(&mut child);
        }
    }
}

/// One entry of a folder's listing, as git writes it.
struct Entry {
    mode: Vec<u8>,
    name: Vec<u8>,
    id: String,
}

// ---- settling ----------------------------------------------------------------------------------

/// What one settling found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Verdict {
    /// Whether the history answered and this machine's record is as the settling left it.
    /// Where it is false, **no acceptance of the project's grants is in force**.
    pub read: bool,
}

/// One project's settlings: one at a time, and the last one's verdict.
#[derive(Default)]
struct Slot {
    /// Held for the length of a settling.
    flight: Mutex<()>,
    /// How many settlings have finished.
    finished: AtomicU64,
    /// The last one's verdict. **The lock a reader of the grants in force takes**, for the
    /// length of a copy.
    last: Mutex<Option<Verdict>>,
    /// Whether a first settling was started off the reader's thread ([`for_read`]).
    begun: std::sync::atomic::AtomicBool,
}

static SLOTS: LazyLock<Mutex<HashMap<PathBuf, Arc<Slot>>>> = LazyLock::new(Default::default);

fn slot_of(root: &Path) -> Arc<Slot> {
    Arc::clone(
        SLOTS
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entry(root.to_path_buf())
            .or_default(),
    )
}

/// **Settles what this machine keeps of the project's grants against git's history**, in the
/// project at `root`, and answers the verdict. One settling runs per project at a time: a
/// caller that arrives while one runs waits for it and takes its verdict instead of reading
/// the same history again. **Runs git: never call it under the lock a dispatch is decided
/// under.**
pub fn settle(root: &Path) -> Verdict {
    settle_with(root, &Git::new(root))
}

/// [`settle`], against `history`.
pub fn settle_with(root: &Path, history: &dyn History) -> Verdict {
    settle_in(root, history, true)
}

/// [`settle`], for a caller that has just written an acceptance: it runs its own settling
/// even where another finished while it waited, since that one may have read the record
/// before the write.
pub fn settle_afresh(root: &Path) -> Verdict {
    settle_in(root, &Git::new(root), false)
}

fn settle_in(root: &Path, history: &dyn History, reuse: bool) -> Verdict {
    let slot = slot_of(root);
    let before = slot.finished.load(Ordering::SeqCst);
    let _flight = slot.flight.lock().unwrap_or_else(PoisonError::into_inner);
    if reuse
        && slot.finished.load(Ordering::SeqCst) != before
        && let Some(last) = *slot.last.lock().unwrap_or_else(PoisonError::into_inner)
    {
        return last;
    }
    let verdict = settle_once(root, history);
    *slot.last.lock().unwrap_or_else(PoisonError::into_inner) = Some(verdict);
    slot.finished.fetch_add(1, Ordering::SeqCst);
    verdict
}

/// Every acceptance `bound` keeps, as the record spells each.
fn accepted_of(bound: &local::DispatchBound) -> Vec<String> {
    bound
        .any_seen
        .iter()
        .map(|asking| any_said(asking))
        .chain(bound.seen.iter().cloned())
        .chain(bound.seen_in.iter().cloned())
        .chain(bound.aside.iter().cloned())
        .collect()
}

/// Whether `bound` keeps nothing a settling would read the history for.
fn kept_nothing(bound: &local::DispatchBound) -> bool {
    accepted_of(bound).is_empty() && bound.declined.is_empty() && bound.at.is_none()
}

/// **The verdict a read of the grants in force goes by: the last settling's, and no git, on
/// any thread.**
///
/// Where this process has settled nothing for the project yet: a project that keeps no
/// acceptance has nothing to settle, and reads as settled. One that keeps some is settled on
/// a thread of its own, started here once, and **until that settling lands no project grant
/// accepted here is in force** (fail closed): a reader meanwhile, the window's included, sees
/// them as not in force, never waits on git, and is told again when the window's next read
/// comes. A dispatch is never decided on this answer alone: the app settles before it
/// decides one.
pub fn for_read(root: &Path) -> Verdict {
    let slot = slot_of(root);
    let last = *slot.last.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(last) = last {
        return last;
    }
    if kept_nothing(&local::dispatch_bound(root)) {
        return READ;
    }
    if !slot.begun.swap(true, Ordering::SeqCst) {
        let root = root.to_path_buf();
        let started = std::thread::Builder::new()
            .name("purlis-dispatch-settle".to_owned())
            .spawn(move || {
                settle(&root);
            });
        if started.is_err() {
            slot.begun.store(false, Ordering::SeqCst);
        }
    }
    UNREAD
}

/// **Why what this machine accepted of the project's grants is not settled now**, as a reader
/// that explains it says it (#1543): the two states [`for_read`] answers `read: false` for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unsettled {
    /// No settling has landed in this process yet: the first moments after a launch. A
    /// dispatch settles before it is decided, so where the history reads, an accepted grant
    /// counts for it.
    NotYet,
    /// The last settling could not read the project's history, or could not keep what it
    /// found: no accepted grant of the project's counts until one does. Each dispatch
    /// settles again first.
    Unread,
}

/// **Whether, and why, what this machine accepted of the project's grants is not settled now**
/// in the project at `root`: `None` where the last settling answered, **and where nothing of
/// the project's is accepted here** (a decline alone, or a commit kept from an earlier
/// settling, has nothing to count). Runs no git: [`for_read`]'s verdict, told apart by whether
/// any settling has landed in this process.
pub fn unsettled(root: &Path) -> Option<Unsettled> {
    let bound = local::dispatch_bound(root);
    if bound.seen.is_empty() && bound.any_seen.is_empty() && bound.seen_in.is_empty() {
        return None;
    }
    let landed = slot_of(root)
        .last
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .is_some();
    if for_read(root).read {
        return None;
    }
    Some(if landed {
        Unsettled::Unread
    } else {
        Unsettled::NotYet
    })
}

const READ: Verdict = Verdict { read: true };
const UNREAD: Verdict = Verdict { read: false };

fn settle_once(root: &Path, history: &dyn History) -> Verdict {
    let bound = local::dispatch_bound(root);
    // Every acceptance this machine keeps of the project's grants: pairs, "any persona",
    // grants limited to one workspace, and what was set aside for a name that changed hands
    // (given back by the person later, so it must not outlive a removal meanwhile).
    let accepted = accepted_of(&bound);
    if kept_nothing(&bound) {
        return READ;
    }
    let until = Instant::now() + DEADLINE;
    let unread = |accepted: &[String]| -> Vec<Gone> {
        accepted
            .iter()
            .map(|said| Gone {
                said: said.clone(),
                why: Gone::UNREAD.to_owned(),
            })
            .collect()
    };
    let settled = match history.head(until) {
        Head::Unanswered => return UNREAD,
        // A history that was there and is not: nothing is assumed to have stayed.
        Head::None if bound.at.is_some() => local::DispatchSettled {
            dropped: unread(&accepted),
            undeclined: Vec::new(),
            at: None,
        },
        Head::None => return READ,
        Head::At(head) => match bound.at.as_deref() {
            // Kept before this machine kept a commit (an earlier build's record, or a
            // project that was in no repository): bound from here on.
            None => local::DispatchSettled {
                at: Some(head),
                ..Default::default()
            },
            Some(was) if was == head => return READ,
            Some(was) => match history.between(was, &head, until) {
                Between::Unanswered => return UNREAD,
                Between::Unreadable => local::DispatchSettled {
                    dropped: unread(&accepted),
                    undeclined: Vec::new(),
                    at: Some(head),
                },
                Between::TakenOut(gone) => local::DispatchSettled {
                    dropped: accepted
                        .iter()
                        .filter(|said| gone.contains(said))
                        .map(|said| Gone {
                            said: said.clone(),
                            why: Gone::REMOVED.to_owned(),
                        })
                        .collect(),
                    undeclined: bound
                        .declined
                        .iter()
                        .filter(|said| gone.contains(said))
                        .cloned()
                        .collect(),
                    at: Some(head),
                },
            },
        },
    };
    // What could not be written did not happen: nothing counts until a settling lands.
    match local::settle_dispatch(root, &settled) {
        Ok(()) => READ,
        Err(_) => UNREAD,
    }
}

// ---- what waits ----------------------------------------------------------------------------------

/// Why a grant this person accepted before waits for a yes again.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Again {
    /// A commit of the project's history took it out, and the file holds it again.
    TakenOut,
    /// The project's history since the acceptance could not be read.
    Unread,
    /// The persona named here was not in the project for a time, and one of that name is.
    Persona(String),
}

/// One grant of the project's waiting for this person's answer.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Arrived {
    /// The persona whose chats it lets dispatch.
    pub asking: String,
    /// The persona they may dispatch to, or [`ANY`] for every persona.
    pub target: String,
    /// A name in it that is no persona of the project as it is checked out here: the grant
    /// can be used by nothing, and is not accepted.
    pub undefined: Option<String>,
    /// Why it is asked again, where this person accepted it before.
    pub again: Option<Again>,
}

impl Arrived {
    /// As this machine's record spells the grant.
    pub fn said(&self) -> String {
        format!("{} -> {}", self.asking, self.target)
    }
}

/// What the project's dispatch grants ask of this person now.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Arrival {
    /// The grants waiting for an answer: "any persona" first, then pairs, in file order.
    pub waiting: Vec<Arrived>,
    /// The grants accepted here that a commit took out of the project's file, each as the
    /// record spells it: told once ([`told_gone`]).
    pub gone: Vec<String>,
    /// Whether the project's history could not be asked just now, so that no acceptance of
    /// its grants counts until it can.
    pub unread: bool,
}

/// **What is waiting for this person in the project at `root`**, after [`settle`]. Runs git.
pub fn arrival(root: &Path) -> Arrival {
    let verdict = settle(root);
    arrival_as(root, verdict)
}

/// [`arrival`], against `history`.
pub fn arrival_with(root: &Path, history: &dyn History) -> Arrival {
    let verdict = settle_with(root, history);
    arrival_as(root, verdict)
}

fn arrival_as(root: &Path, verdict: Verdict) -> Arrival {
    let bound = local::dispatch_bound(root);
    let Some(text) = file_now(root) else {
        // A file that is not there or does not parse holds nothing to wait, and says nothing
        // of what a commit did.
        return Arrival {
            waiting: Vec::new(),
            gone: Vec::new(),
            unread: !verdict.read,
        };
    };
    let committed = dispatchgrant::committed(Some(&text));
    let personas = crate::dispatchdormant::personas_of(root);
    // Where the personas cannot be listed no name is called undefined. Nothing is accepted
    // for a name that is not known either: the app's Accept checks each name itself.
    let undefined = |names: &[&str]| {
        let personas = personas.as_ref()?;
        names
            .iter()
            .find(|name| !personas.iter().any(|one| one == *name))
            .map(|name| (*name).to_owned())
    };
    // The names that were seen gone and are another persona's now: what an earlier persona
    // of the name was allowed waits for the person again (#1504's rule, told here).
    let returned = crate::dispatchdormant::state(root, &[])
        .map(|state| state.returned)
        .unwrap_or_default();
    let aside = local::accepted_aside_dispatch(root);
    let all = committed
        .any
        .iter()
        .map(|asking| (asking.clone(), ANY.to_owned(), vec![asking.as_str()]))
        .chain(committed.pairs.iter().map(|pair| {
            (
                pair.asking.clone(),
                pair.target.clone(),
                vec![pair.asking.as_str(), pair.target.as_str()],
            )
        }));
    let mut waiting = Vec::new();
    for (asking, target, names) in all {
        let said = format!("{asking} -> {target}");
        let accepted = if target == ANY {
            bound.any_seen.contains(&asking)
        } else {
            bound.seen.contains(&said)
        };
        let came_back = names
            .iter()
            .find(|name| returned.iter().any(|one| one == *name))
            .map(|name| (*name).to_owned());
        let again = if accepted {
            // Still in the record, and in force for nobody: the name changed hands.
            match came_back {
                Some(name) => Some(Again::Persona(name)),
                None => continue,
            }
        } else if bound.declined.contains(&said) {
            continue;
        } else if let Some(gone) = bound.gone.iter().find(|one| one.said == said) {
            Some(if gone.why == Gone::REMOVED {
                Again::TakenOut
            } else {
                Again::Unread
            })
        } else {
            aside
                .iter()
                .find(|one| one.said == said)
                .map(|one| Again::Persona(one.was.clone()))
        };
        waiting.push(Arrived {
            undefined: undefined(&names),
            asking,
            target,
            again,
        });
    }
    let now = grants_in(Some(&text));
    Arrival {
        waiting,
        gone: bound
            .gone
            .into_iter()
            .filter(|one| one.why == Gone::REMOVED && !now.contains(&one.said))
            .map(|one| one.said)
            .collect(),
        unread: !verdict.read,
    }
}

/// Records that the person was told `shown`, grants the project took away: each is told once.
pub fn told_gone(root: &Path, shown: &[String]) -> std::io::Result<()> {
    local::forget_dispatch_gone(root, shown)
}

#[cfg(test)]
#[path = "dispatcharrival_tests.rs"]
mod tests;
