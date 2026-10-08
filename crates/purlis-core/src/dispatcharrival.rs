//! **A teammate's project grant, when it arrives and when it goes** (#1506, spec #1483; ADR
//! 0090 as amended): what of the project's `[dispatch.grants]` is waiting for this person's
//! answer, and what this machine's earlier yes is bound to.
//!
//! # Arrival
//!
//! [`arrival`] is what the window's Notice shows: every pair, and every "any persona" grant,
//! that the project's file holds and this person has neither accepted nor said "Not on my
//! machine" to. Nothing in it is in force here until it is accepted
//! ([`crate::dispatchgrant::InForce::read`] reads acceptances only). A grant that names a
//! persona this checkout does not define is listed as such ([`Arrived::undefined`]), never as
//! one that can be used. A declined grant is never listed: Settings is where it is changed.
//!
//! # What a yes is bound to
//!
//! An acceptance kept on a machine must not outlive what it accepted. Two checks, both in
//! [`settle`], which runs wherever the grants in force are read, when the app's watcher says
//! the project's file moved, and before any acceptance is written:
//!
//! 1. **The file as it is now.** An acceptance of a grant the project's file no longer holds
//!    is dropped at once. If the file holds it again later, it waits for a new yes. A decline
//!    of a grant the file no longer holds is forgotten the same way, so a grant that comes
//!    back is told again.
//! 2. **The history since the last check.** The file as it is now cannot show a grant that
//!    was taken out and put back while nobody was reading: the text is the same. So the
//!    acceptances are kept with the commit they were last checked through
//!    (`dispatch_seen_at`), and when the checkout is at another commit, every version of the
//!    project's file committed in between is read ([`History::between`]). A grant that was in
//!    the file at the commit it was checked through, and is missing from any version since,
//!    is dropped. Where that history cannot be read, or git runs out of time reading it, every
//!    acceptance is dropped: purlis does not assume a grant stayed.
//!
//! What was dropped is remembered (`dispatch_gone`) so the person is told once that the
//! project took it away, or, where the file holds it again, that it is asked a second time
//! and why ([`Arrived::again`]).
//!
//! **What the second check does not see**, and does not claim to: a file changed and changed
//! back in the working tree with no commit and no read in between (a branch switched away and
//! back while purlis was closed). That is this machine's own checkout moving, not something a
//! teammate's push can cause. A project that is in no git repository has the first check only.
//!
//! **Only ever narrows.** Settling drops acceptances and declines; it grants nothing, and a
//! project file that is missing or does not parse for a moment changes nothing that is stored
//! (D-1503-18).

use std::path::Path;
use std::time::Duration;

use crate::dispatchgrant::{self, ANY};
use crate::sandbox::local;

/// The most versions of the project's file one settling reads. Past it the history is taken
/// as unread, and every acceptance waits for a yes again.
pub const MOST_VERSIONS: usize = 64;

/// How long one git call of a settling may take: as long as any read of a repository here
/// ([`crate::worktree::git::READ`]), since a machine under load is slow, not broken. A history
/// that runs out of it is taken as unread ([`Between::Unreadable`]): how long git takes over a
/// history is something whoever pushed it has a hand in.
const GIT: Duration = crate::worktree::git::READ;

/// A project grant as this machine's record spells it: a pair as
/// [`dispatchgrant::Pair`] is displayed, or `<asking> -> *` for "any persona".
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
        .collect()
}

/// The project's file at `root` as it is now, where it was read and parsed. `None` for a file
/// that is not there, cannot be read, or is not TOML: nothing is concluded from it.
fn file_now(root: &Path) -> Option<String> {
    let text = crate::sandbox::read_plane_file(&crate::names::manifest(root))
        .ok()
        .flatten()?;
    text.parse::<toml::Table>().is_ok().then_some(text)
}

/// What the project's history says of its file between two commits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Between {
    /// The file as the earlier commit had it (`None`: it had none), and as each commit since
    /// that changed it left it, on every line of history that was merged in.
    Versions {
        from: Option<String>,
        since: Vec<Option<String>>,
    },
    /// The history does not answer: the earlier commit is not in it any more, more versions
    /// lie between than [`MOST_VERSIONS`], or git ran out of time reading it. Nothing is
    /// assumed to have stayed.
    Unreadable,
    /// git could not be run at all just now. Nothing is changed, and the question is asked
    /// again at the next read.
    Unanswered,
}

/// The project's history, as [`settle_with`] reads it.
pub trait History {
    /// The commit the project's checkout is at; `None` where there is none to read.
    fn head(&self) -> Option<String>;
    /// The project's file from commit `from` to commit `to`.
    fn between(&self, from: &str, to: &str) -> Between;
}

/// The history git keeps for the project at this root.
pub struct Git<'a>(pub &'a Path);

/// Whether `said` is a commit's full id as git prints one: the only thing ever handed to git
/// as a revision, since the stored one is read from a file a person can edit.
fn is_commit_id(said: &str) -> bool {
    matches!(said.len(), 40 | 64) && said.bytes().all(|b| b.is_ascii_hexdigit())
}

/// What one git call of a settling came to.
enum Ran {
    /// git answered, with this exit code and output.
    Answered(i32, String),
    /// git ran and did not finish in time.
    TimedOut,
    /// git could not be run.
    NotRun,
}

impl Git<'_> {
    fn run(&self, args: &[&str]) -> Ran {
        match crate::worktree::git::run(self.0, args, GIT) {
            Ok(run) => match run.code {
                Some(code) => Ran::Answered(code, run.out),
                None => Ran::TimedOut,
            },
            Err(_) => Ran::NotRun,
        }
    }

    /// The project's file, as git names it from the project's root.
    fn file(&self) -> Option<String> {
        let manifest = crate::names::manifest(self.0);
        Some(format!("./{}", manifest.file_name()?.to_str()?))
    }
}

impl History for Git<'_> {
    fn head(&self) -> Option<String> {
        let Ran::Answered(0, out) =
            self.run(&["rev-parse", "--verify", "--quiet", "HEAD^{commit}"])
        else {
            return None;
        };
        let id = out.trim();
        is_commit_id(id).then(|| id.to_owned())
    }

    fn between(&self, from: &str, to: &str) -> Between {
        if !is_commit_id(from) || !is_commit_id(to) {
            return Between::Unreadable;
        }
        let Some(file) = self.file() else {
            return Between::Unreadable;
        };
        let most = format!("--max-count={}", MOST_VERSIONS + 1);
        let range = format!("{from}..{to}");
        // Every line of history, not only the one a merge kept: a version without a grant on
        // a branch that was merged in is a version this machine pulled.
        let listed = match self.run(&["rev-list", "--full-history", &most, &range, "--", &file]) {
            Ran::Answered(0, out) => out,
            Ran::Answered(..) | Ran::TimedOut => return Between::Unreadable,
            Ran::NotRun => return Between::Unanswered,
        };
        let commits: Vec<&str> = listed.lines().map(str::trim).collect();
        if commits.len() > MOST_VERSIONS || commits.iter().any(|one| !is_commit_id(one)) {
            return Between::Unreadable;
        }
        let mut versions = Vec::with_capacity(commits.len() + 1);
        for commit in std::iter::once(from).chain(commits) {
            match self.run(&["show", &format!("{commit}:{file}")]) {
                Ran::Answered(0, text) => versions.push(Some(text)),
                // git answers a commit that has no such file with an error: it held no grant.
                Ran::Answered(..) => versions.push(None),
                Ran::TimedOut => return Between::Unreadable,
                Ran::NotRun => return Between::Unanswered,
            }
        }
        let since = versions.split_off(1);
        Between::Versions {
            from: versions.pop().flatten(),
            since,
        }
    }
}

/// **Drops what this machine keeps of the project's grants that no longer holds**, in the
/// project at `root`, read against git's history. The module's doc is the rule. Best effort:
/// what could not be written is still not in force while the file lacks it.
pub fn settle(root: &Path) {
    settle_with(root, &Git(root));
}

/// [`settle`], against `history`.
pub fn settle_with(root: &Path, history: &dyn History) {
    let bound = local::dispatch_bound(root);
    let accepted: Vec<String> = bound
        .any_seen
        .iter()
        .map(|asking| any_said(asking))
        .chain(bound.seen.iter().cloned())
        .collect();
    if accepted.is_empty() && bound.declined.is_empty() && bound.at.is_none() {
        return;
    }
    // Only a file that was read and parsed says what it holds.
    let Some(text) = file_now(root) else {
        return;
    };
    let now = grants_in(Some(&text));
    let (standing, mut dropped): (Vec<String>, Vec<String>) =
        accepted.into_iter().partition(|said| now.contains(said));
    let undeclined: Vec<String> = bound
        .declined
        .iter()
        .filter(|said| !now.contains(said))
        .cloned()
        .collect();
    let mut at = bound.at.clone();
    if !standing.is_empty() {
        match (bound.at.as_deref(), history.head()) {
            // No commit to read: the file as it is now is the whole check.
            (_, None) => {}
            // Accepted before this machine kept a commit: bound from here on.
            (None, Some(head)) => at = Some(head),
            (Some(was), Some(head)) if was == head => {}
            (Some(was), Some(head)) => match history.between(was, &head) {
                Between::Unanswered => {}
                Between::Unreadable => {
                    dropped.extend(standing.iter().cloned());
                    at = Some(head);
                }
                Between::Versions { from, since } => {
                    // A grant the earlier commit did not hold was this machine's own edit,
                    // not yet committed: no version in between is expected to hold it.
                    let then = grants_in(from.as_deref());
                    let since: Vec<Vec<String>> = since
                        .iter()
                        .map(|version| grants_in(version.as_deref()))
                        .collect();
                    dropped.extend(
                        standing
                            .iter()
                            .filter(|said| {
                                then.contains(said)
                                    && since.iter().any(|version| !version.contains(said))
                            })
                            .cloned(),
                    );
                    at = Some(head);
                }
            },
        }
    }
    if standing.iter().all(|said| dropped.contains(said)) {
        at = None;
    }
    if dropped.is_empty() && undeclined.is_empty() && at == bound.at {
        return;
    }
    let _ = local::settle_dispatch(
        root,
        &local::DispatchSettled {
            dropped,
            undeclined,
            at,
        },
    );
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
    /// Whether this person accepted it before and it is asked again: the project took it
    /// away and put it back, or its history since could not be read.
    pub again: bool,
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
    /// The grants accepted here that the project took away, each as the record spells it:
    /// told once ([`told_gone`]).
    pub gone: Vec<String>,
}

/// **What is waiting for this person in the project at `root`**, after [`settle`].
pub fn arrival(root: &Path) -> Arrival {
    arrival_with(root, &Git(root))
}

/// [`arrival`], against `history`.
pub fn arrival_with(root: &Path, history: &dyn History) -> Arrival {
    settle_with(root, history);
    let bound = local::dispatch_bound(root);
    let Some(text) = file_now(root) else {
        return Arrival {
            waiting: Vec::new(),
            gone: bound.gone,
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
    let any = committed
        .any
        .iter()
        .filter(|asking| !bound.any_seen.contains(asking))
        .map(|asking| (asking.clone(), ANY.to_owned(), undefined(&[asking])));
    let pairs = committed
        .pairs
        .iter()
        .filter(|pair| !bound.seen.contains(&pair.to_string()))
        .map(|pair| {
            (
                pair.asking.clone(),
                pair.target.clone(),
                undefined(&[&pair.asking, &pair.target]),
            )
        });
    let waiting: Vec<Arrived> = any
        .chain(pairs)
        .map(|(asking, target, undefined)| Arrived {
            again: bound.gone.contains(&format!("{asking} -> {target}")),
            asking,
            target,
            undefined,
        })
        .filter(|one| !bound.declined.contains(&one.said()))
        .collect();
    let now = grants_in(Some(&text));
    Arrival {
        waiting,
        gone: bound
            .gone
            .into_iter()
            .filter(|said| !now.contains(said))
            .collect(),
    }
}

/// Records that the person was told `shown`, grants the project took away: each is told once.
pub fn told_gone(root: &Path, shown: &[String]) -> std::io::Result<()> {
    local::forget_dispatch_gone(root, shown)
}

#[cfg(test)]
#[path = "dispatcharrival_tests.rs"]
mod tests;
