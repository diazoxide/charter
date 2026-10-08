//! **The one condition a dispatch grant carries** (#1505, ruling V100-27; ADR 0090 as
//! amended): it holds in one workspace, or in any workspace.
//!
//! # Which workspace
//!
//! **The workspace the task works in**, never the one the asking chat is in ([`works_in`]). A
//! grant "steward may dispatch to devops in runners" covers a dispatch from a steward chat
//! anywhere in the project whose task runs in runners (`--in workspace:runners`, a worktree
//! of a repo of runners, a handoff into runners, or simply a chat that works in runners and
//! names no place), and it covers no task that runs anywhere else, whichever chat asks. The
//! grant is the person's consent to what the other persona may be asked to do, and where that
//! persona works is what bounds it. Read the other way, a chat in runners could send devops to
//! work in any workspace under a grant that says "in runners".
//!
//! **A task at the project's root works in no workspace**, so no limited grant covers it, and
//! the question there offers no workspace to limit a grant to.
//!
//! # A limited grant never reads as an unlimited one
//!
//! Each place a grant is kept spells a limited one so that a build which does not know the
//! condition reads nothing, never a grant that holds everywhere:
//!
//! - **Me on this machine**: `app/sandbox.json` `dispatch_mine_in`, a key of its own. Nothing
//!   limited is ever written to `dispatch_mine` or `dispatch_any`.
//! - **The project**: an inline table in the asking persona's list of `[dispatch.grants]`,
//!   `steward = [{ to = "devops", in = "runners" }]` ([`Limited::from_entry`]). An entry that
//!   is not a string grants nothing in a build that knows only names. This machine's
//!   acceptance of it is `dispatch_seen_in`, again a key of its own, so accepting a pair for
//!   one workspace never accepts it for all, or the reverse.
//! - **This chat**: the app keeps, with the grant, the workspace the dispatch it was allowed
//!   for works in, and the grant covers that chat's dispatches there and nowhere else.
//!
//! An entry that does not read (a missing key, one more key, a name that is none) grants
//! nothing and is said in a sentence. A grant written before the condition existed is in the
//! old spelling, and holds in any workspace as it did.
//!
//! # A workspace that goes away, and one made later under its name
//!
//! **A workspace is the folder of that name under `workspaces/`, spelled exactly**
//! ([`crate::dispatchplace::workspace_folder`]). A limited grant counts only while there is
//! one. This machine counts the times it saw each name a limited grant holds gone
//! (`dispatch_workspaces`, [`noticed`]), and a grant keeps the count as it was when the
//! person made it; **the grant counts only while the two are equal**. So a workspace made
//! under the name of one that was removed inherits nothing: the grant is shown as covering
//! nothing, and the person removes it or sets its workspace again, which is the yes for the
//! one that is there now. purlis has no workspace rename; one renamed by hand is the old name
//! gone and a new one that no grant names.
//!
//! **What this does not catch**: a workspace removed and made again with no dispatch judged
//! and Settings not read in between. Nothing was looked at, so nothing was counted, and the
//! new one has the old one's grants. A workspace has no identity beside its name to tell the
//! two apart by.
//!
//! # Never is not limited
//!
//! "Never for this pair" holds in every workspace. It is the one refusal, and one that
//! stopped at a workspace's edge would be a never that a dispatch walks around by naming
//! another place.

use std::path::Path;

use crate::dispatchgrant::{ANY, Pair};
use crate::sandbox::grant::Level;
use crate::sandbox::local::{self, DispatchIn, KnownWorkspace, Limited as Record};

/// Where a grant holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Within {
    /// In any workspace, and at the project's root.
    Any,
    /// In this workspace only, by the name its folder has.
    Workspace(String),
}

impl Within {
    /// The condition a window or a record spells as `workspace`: none is any workspace.
    pub fn of(workspace: Option<&str>) -> Self {
        workspace.map_or(Self::Any, |name| Self::Workspace(name.to_owned()))
    }

    /// The workspace, where it is limited to one.
    pub fn workspace(&self) -> Option<&str> {
        match self {
            Self::Any => None,
            Self::Workspace(name) => Some(name),
        }
    }

    /// As a sentence ends with it: `in any workspace`, or `in runners`.
    pub fn said(&self) -> String {
        match self {
            Self::Any => "in any workspace".to_owned(),
            Self::Workspace(name) => format!("in {}", crate::shown::short(name)),
        }
    }
}

/// **The workspace a dispatched task works in**: the one a handoff moves into, else the one
/// the dispatch names or cuts a worktree in ([`crate::dispatchplace::Ground::workspace`]),
/// else the asking chat's own. `None` is the project's root.
///
/// Each part is the app's own: the workspace named is looked up among the project's and
/// answered by its folder's name, a worktree's is the repo the app records the asking chat as
/// standing in, and the asking chat's is where the app started it. **The chat is then started
/// in that same place**, so what is judged and where the task runs cannot differ.
pub fn works_in<'a>(
    moves_into: Option<&'a str>,
    ground: Option<&'a str>,
    asker: Option<&'a str>,
) -> Option<&'a str> {
    moves_into.or(ground).or(asker)
}

/// **A grant limited to one workspace**: chats running as `asking` may dispatch to `target`
/// (a persona's name, or [`ANY`]) for work in `workspace`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Limited {
    pub asking: String,
    pub target: String,
    pub workspace: String,
}

impl Limited {
    /// The grant, or why it is none: `asking` a persona's name, `target` another persona's or
    /// [`ANY`], and `workspace` a name a workspace can have.
    pub fn new(asking: &str, target: &str, workspace: &str) -> Result<Self, String> {
        if target == ANY {
            if !crate::personas::valid_name(asking) {
                return Err(format!(
                    "{} is not a persona's name, so purlis keeps no dispatch grant for it.",
                    crate::shown::short(asking)
                ));
            }
        } else {
            Pair::new(asking, target)?;
        }
        if !crate::contain::workspace_name_ok(workspace) {
            return Err(format!(
                "{} cannot name a workspace, so purlis keeps no dispatch grant for it.",
                crate::shown::short(workspace)
            ));
        }
        Ok(Self {
            asking: asking.to_owned(),
            target: target.to_owned(),
            workspace: workspace.to_owned(),
        })
    }

    /// `pair`, limited to `workspace`.
    pub fn of(pair: &Pair, workspace: &str) -> Result<Self, String> {
        Self::new(&pair.asking, &pair.target, workspace)
    }

    /// Whether it is "any persona".
    pub fn any(&self) -> bool {
        self.target == ANY
    }

    /// **One entry of an asking persona's list in `[dispatch.grants]`**, where it is a table:
    /// exactly `{ to = "<persona or *>", in = "<workspace>" }`. Anything else is a sentence
    /// and grants nothing: a key missing, a key more (a condition a later build may add is
    /// not one this build can honour, so its grant is not read as a wider one), or a value
    /// that is not such a name.
    pub fn from_entry(asking: &str, entry: &toml::Table) -> Result<Self, String> {
        let at = format!(
            "{}.{}.{}",
            crate::dispatchgrant::TABLE,
            crate::dispatchgrant::KEY,
            crate::shown::short(asking)
        );
        let shape = || {
            format!(
                "{at} holds a grant for one workspace that is not written as \
                 {{ to = \"devops\", in = \"runners\" }}, which grants nothing"
            )
        };
        if entry.len() != 2 {
            return Err(shape());
        }
        let (Some(to), Some(within)) = (
            entry.get(TO).and_then(toml::Value::as_str),
            entry.get(IN).and_then(toml::Value::as_str),
        ) else {
            return Err(shape());
        };
        Self::new(asking, to, within)
    }

    /// As the file keeps it on this machine, having seen its workspace gone `seen` times.
    fn on_disk(&self, seen: u32) -> DispatchIn {
        DispatchIn {
            asking: self.asking.clone(),
            target: self.target.clone(),
            any: self.any(),
            workspace: self.workspace.clone(),
            seen,
        }
    }

    /// What `kept` holds, where it is a grant: one whose names are no persona's, or whose
    /// `any` and target disagree, is none and grants nothing.
    fn kept(kept: &DispatchIn) -> Option<Self> {
        if kept.any != (kept.target == ANY) {
            return None;
        }
        Self::new(&kept.asking, &kept.target, &kept.workspace).ok()
    }

    fn is(&self, kept: &DispatchIn) -> bool {
        kept.is(&self.asking, &self.target, self.any(), &self.workspace)
            && kept.any == (kept.target == ANY)
    }
}

/// The key of a limited entry that names the target persona, or [`ANY`].
pub const TO: &str = "to";

/// The key of a limited entry that names the workspace.
pub const IN: &str = "in";

/// As the audit and a sentence name it: `steward -> devops in runners`.
impl std::fmt::Display for Limited {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} -> {} in {}",
            self.asking, self.target, self.workspace
        )
    }
}

/// **What this machine sees of the workspaces limited grants name**: which are there now, and
/// how many times each was seen gone.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Seen {
    known: Vec<KnownWorkspace>,
    /// Whether the project's workspaces could be looked at. Where they could not, no limited
    /// grant counts and nothing is recorded as gone.
    listed: bool,
    root: std::path::PathBuf,
}

impl Seen {
    /// As the project at `root` stands now. Reads only.
    pub fn read(root: &Path) -> Self {
        Self {
            known: local::dispatch_workspaces(root),
            listed: root.join("workspaces").read_dir().is_ok(),
            root: root.to_path_buf(),
        }
    }

    /// Whether `name` is a workspace of the project now: a folder of exactly that name under
    /// `workspaces/`, reached through no link.
    pub fn is_there(&self, name: &str) -> bool {
        self.listed
            && crate::dispatchplace::workspace_folder(&self.root, name)
                .is_ok_and(|(own, _)| own == name)
    }

    /// How many times `name` was seen gone.
    pub fn gone(&self, name: &str) -> u32 {
        self.known
            .iter()
            .find(|one| one.name == name)
            .map_or(0, |one| one.gone)
    }

    /// Whether a grant for the workspace `name`, made when the name had been seen gone `seen`
    /// times, counts now.
    pub fn stands(&self, name: &str, seen: u32) -> bool {
        self.why_not(name, seen).is_none()
    }

    /// Why such a grant covers nothing now, for the person; `None` where it counts.
    pub fn why_not(&self, name: &str, seen: u32) -> Option<String> {
        let shown = crate::shown::short(name);
        if !self.listed {
            return Some(
                "purlis could not look at this project's workspaces, so this grant covers \
                 nothing for now."
                    .to_owned(),
            );
        }
        if !self.is_there(name) {
            return Some(format!(
                "{shown} is not a workspace of this project now, so this grant covers nothing."
            ));
        }
        (self.gone(name) != seen).then(|| {
            format!(
                "A workspace named {shown} was removed after this grant was made, so it \
                 covers nothing in the one that is there now."
            )
        })
    }
}

/// Every workspace a limited grant names in the project at `root`: yours, what this machine
/// accepted, and the project's file's.
fn named(root: &Path) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    let kept = local::dispatch_mine_in(root)
        .into_iter()
        .chain(local::dispatch_seen_in(root))
        .map(|one| one.workspace);
    for name in kept.chain(committed_at(root).into_iter().map(|one| one.workspace)) {
        if crate::contain::workspace_name_ok(&name) && !names.contains(&name) {
            names.push(name);
        }
    }
    names
}

/// **What this machine sees of the workspaces, brought up to now**: each name a limited grant
/// holds that is gone, and was not when last looked at, is counted gone once more
/// ([`local::note_dispatch_workspaces`]). **The one thing reading the limited grants writes.**
/// It moves no grant and only ever takes one out of force. Best effort: a count that could
/// not be written leaves the grant out of force all the same while the name is gone.
pub fn noticed(root: &Path) -> Seen {
    let seen = Seen::read(root);
    let names = named(root);
    if !seen.listed || names.is_empty() {
        return seen;
    }
    if let Err(why) = local::note_dispatch_workspaces(root, &names, &|name| seen.is_there(name)) {
        tracing::warn!("purlis: a workspace that is gone was not recorded so ({why})");
    }
    Seen::read(root)
}

/// **How many times `workspace` has been seen gone, as of now**: what a grant made or
/// confirmed at this moment keeps. The name is looked at first, though no grant holds it yet,
/// so a grant made for a workspace that is not there (a handoff about to make it) is made
/// after that absence and counts once the workspace is.
fn seen_now(root: &Path, workspace: &str) -> u32 {
    let seen = Seen::read(root);
    if seen.listed
        && let Err(why) = local::note_dispatch_workspaces(root, &[workspace.to_owned()], &|name| {
            seen.is_there(name)
        })
    {
        tracing::warn!("purlis: a workspace that is gone was not recorded so ({why})");
    }
    Seen::read(root).gone(workspace)
}

/// Your grants limited to one workspace, in the project at `root`, each with what it had seen
/// of its workspace. One that is no grant as written is left out and grants nothing.
pub fn yours(root: &Path) -> Vec<(Limited, u32)> {
    local::dispatch_mine_in(root)
        .iter()
        .filter_map(|kept| Some((Limited::kept(kept)?, kept.seen)))
        .collect()
}

/// The project's grants limited to one workspace, as its file at `root` writes them, accepted
/// here or not.
pub fn committed_at(root: &Path) -> Vec<Limited> {
    let text = crate::sandbox::read_plane_file(&crate::names::manifest(root))
        .ok()
        .flatten();
    crate::dispatchgrant::committed(text.as_deref()).limited
}

/// The project's limited grants this machine accepted **and** the file still holds, each with
/// what the acceptance had seen of its workspace.
pub fn accepted(root: &Path) -> Vec<(Limited, u32)> {
    let kept = local::dispatch_seen_in(root);
    committed_at(root)
        .into_iter()
        .filter_map(|one| {
            let seen = kept.iter().find(|kept| one.is(kept))?.seen;
            Some((one, seen))
        })
        .collect()
}

/// The project's limited grants nobody on this machine has accepted: in the file, in force
/// for no chat here. **Settings is where each is accepted** ([`accept`]); the project's
/// one-time Notice does not name them.
pub fn unaccepted(root: &Path) -> Vec<Limited> {
    let kept = local::dispatch_seen_in(root);
    committed_at(root)
        .into_iter()
        .filter(|one| !kept.iter().any(|kept| one.is(kept)))
        .collect()
}

/// **Drops this machine's acceptance of each limited grant the project's file no longer
/// holds**, so the grant is not in force unasked if the file comes to hold it again
/// ([`crate::dispatchgrant::forget_any_the_file_dropped`] is the same for "any persona").
/// Only against a project file that was read and parsed. Best effort: an acceptance that
/// could not be dropped still covers nothing while the file lacks the grant.
pub fn forget_what_the_file_dropped(root: &Path) {
    let kept = local::dispatch_seen_in(root);
    if kept.is_empty() {
        return;
    }
    let Some(text) = crate::sandbox::read_plane_file(&crate::names::manifest(root))
        .ok()
        .flatten()
    else {
        return;
    };
    if text.parse::<toml::Table>().is_err() {
        return;
    }
    let held = crate::dispatchgrant::committed(Some(&text)).limited;
    for one in kept
        .iter()
        .filter(|kept| !held.iter().any(|held| held.is(kept)))
    {
        let _ = local::drop_dispatch_in(
            root,
            Record::Accepted,
            &one.asking,
            &one.target,
            one.any,
            &one.workspace,
        );
    }
}

/// **The limited grants in force in the project at `root`**: yours, and the project's that
/// this machine accepted, each only while its workspace stands ([`Seen::stands`]).
pub fn in_force(root: &Path) -> Vec<(Level, Limited)> {
    forget_what_the_file_dropped(root);
    let seen = noticed(root);
    let counts = |(one, at): (Limited, u32)| seen.stands(&one.workspace, at).then_some(one);
    let mine = yours(root)
        .into_iter()
        .filter_map(counts)
        .map(|one| (Level::You, one));
    let ours = accepted(root)
        .into_iter()
        .filter_map(counts)
        .map(|one| (Level::Project, one));
    mine.chain(ours).collect()
}

/// **Keeps the person's grant of `pair`, for them on this machine, `within`**: the one way a
/// grant at that level is written, limited or not. Never a chat's to call: an answer to the
/// grant Notice, an item the person answers afterwards, or Settings. The caller audits first.
///
/// A limited grant is kept with what this machine has seen of its workspace now, so it holds
/// for the workspace of that name the person is looking at.
pub fn grant_yours(root: &Path, pair: &Pair, within: &Within) -> std::io::Result<()> {
    match within {
        Within::Any => local::grant_dispatch(root, &pair.asking, &pair.target),
        Within::Workspace(workspace) => {
            let one = Limited::of(pair, workspace).map_err(std::io::Error::other)?;
            let seen = seen_now(root, workspace);
            local::keep_dispatch_in(root, Record::Mine, &one.on_disk(seen))
        }
    }
}

/// Takes back your limited grant `one`. Answers whether there was one.
pub fn revoke_yours(root: &Path, one: &Limited) -> std::io::Result<bool> {
    local::drop_dispatch_in(
        root,
        Record::Mine,
        &one.asking,
        &one.target,
        one.any(),
        &one.workspace,
    )
}

/// **Accepts the project's limited grant `one` on this machine**: it records the acceptance,
/// with what this machine has seen of the workspace now, and never writes the committed file.
/// Refused where the file does not hold the grant.
pub fn accept(root: &Path, one: &Limited) -> Result<(), String> {
    if !committed_at(root).contains(one) {
        return Err("purlis changed nothing: the project no longer has that grant.".to_owned());
    }
    let seen = seen_now(root, &one.workspace);
    local::keep_dispatch_in(root, Record::Accepted, &one.on_disk(seen))
        .map_err(|why| format!("purlis could not record it as allowed on this machine: {why}"))
}

/// Takes this machine's acceptance of the project's limited grant `one` away: "Not on my
/// machine", and what follows a grant the file lost. Answers whether it was accepted.
pub fn unaccept(root: &Path, one: &Limited) -> std::io::Result<bool> {
    local::drop_dispatch_in(
        root,
        Record::Accepted,
        &one.asking,
        &one.target,
        one.any(),
        &one.workspace,
    )
}

/// Whether your grant of `asking` to `target` ([`ANY`]: any persona) is kept as holding
/// `within`.
pub fn yours_holds(root: &Path, asking: &str, target: &str, within: &Within) -> bool {
    match within {
        Within::Workspace(workspace) => yours(root).iter().any(|(one, _)| {
            one.asking == asking && one.target == target && one.workspace == *workspace
        }),
        Within::Any if target == ANY => crate::dispatchgrant::any_yours(root)
            .iter()
            .any(|one| one == asking),
        Within::Any => crate::dispatchgrant::yours(root)
            .iter()
            .any(|pair| pair.asking == asking && pair.target == target),
    }
}

/// Whether the project's file grants `asking` to `target` ([`ANY`]: any persona) `within`.
pub fn project_holds(root: &Path, asking: &str, target: &str, within: &Within) -> bool {
    match within {
        Within::Workspace(workspace) => committed_at(root)
            .iter()
            .any(|one| one.asking == asking && one.target == target && one.workspace == *workspace),
        Within::Any => crate::dispatchgrant::project_grants(root, asking, target),
    }
}

/// Why the grant of `asking` to `target` cannot be set to hold `to`, before anything is
/// audited or written: the names, and for a workspace that it is one of the project's now.
pub fn can_set(root: &Path, asking: &str, target: &str, to: &Within) -> Result<(), String> {
    match to {
        Within::Workspace(workspace) => {
            Limited::new(asking, target, workspace)?;
            if !Seen::read(root).is_there(workspace) {
                return Err(format!(
                    "{} is not a workspace of this project now, so nothing was changed.",
                    crate::shown::short(workspace)
                ));
            }
            Ok(())
        }
        Within::Any if target == ANY => {
            crate::dispatchgrant::can_allow_any(root, asking, Level::You)
        }
        Within::Any => Pair::new(asking, target).map(|_| ()),
    }
}

/// **Sets where your grant of `asking` to `target` ([`ANY`]: any persona) holds**, from
/// `from` to `to`, in one write: Settings' own action, narrowing or widening. Setting it to
/// the workspace it names already is how a grant whose workspace was made again is confirmed
/// for the one there now. Answers whether the grant was there as `from` says; where it was
/// not, nothing is written. The caller checks ([`can_set`]) and audits first.
pub fn set_yours(
    root: &Path,
    asking: &str,
    target: &str,
    from: &Within,
    to: &Within,
) -> std::io::Result<bool> {
    local::move_dispatch_mine(
        root,
        asking,
        target,
        target == ANY,
        from.workspace(),
        to.workspace().map(|name| (name, seen_now(root, name))),
    )
}

#[cfg(test)]
#[path = "dispatchwithin_tests.rs"]
mod tests;
