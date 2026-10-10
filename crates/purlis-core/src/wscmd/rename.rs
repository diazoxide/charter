//! `charter workspace rename <old> <new>` (alias `mv`) — give a workspace another name
//! (charter#367).
//!
//! A port of `commands_workspace.cmd_workspace_rename`, and of every bug it had once.
//!
//! # What a workspace's name is kept in
//!
//! The directory is the workspace, so the move itself is one `rename(2)`. What makes this a
//! command rather than an `mv` is everything else that names the workspace:
//!
//! - **git's own records of every linked worktree** of every clone that moved, and of every
//!   worktree of the plane itself kept under the workspace — Claude Code's `.claude/worktrees/*`
//!   inside a clone included. git keeps their paths absolute on both sides, so after the move
//!   it calls a live worktree prunable, and `gc` then deletes its admin directory while it holds
//!   uncommitted work (charter#963). `git worktree repair` with the new paths mends both
//!   directions.
//! - `workspace.json`'s `name`, and the first line of the four files charter scaffolded with
//!   the name in it (`workspace.md`, `memory/MEMORY.md`, `todos/MEMORY.md`, `refs/README.md`),
//!   when that line is still the one charter wrote.
//! - the LIVE block in the plane's `.gitignore`.
//! - the pointers that select a workspace: `workspaces/.default`, every session's
//!   `.charter/sessions/<sid>.workspace` and `.lock`, every terminal's
//!   `.charter/terminals/<tid>.workspace`, the legacy `.charter/active-workspace`, and the
//!   tab order in `.charter/workspace-tab-order`.
//! - the per-workspace state charter keeps by name: `.charter/workspace-arrivals/<ws>`,
//!   `.charter/ws-autosave/<ws>` and the reports waiting in `.charter/handbacks/workspace-<ws>/`.
//! - the app's record, `.charter/app/reopen.json`: each chat's directory, the workspace a
//!   handed-off chat came from, each view tab's strip, and the key of every view keyed by the
//!   workspace's name (#1248) — so a relaunch reopens them under the new name.
//! - this machine's pins (`machine.rs`), which would otherwise dangle.
//! - the work link log (ADR 0088 §2): a chat or the workspace linked to one of its todos names
//!   it by a `todo:<workspace>/<stem>` key, which is never rewritten. Each such key gets a
//!   `renamed` alias to the same todo under the new name ([`crate::work::log::follow_rename`]),
//!   so every link still reaches it.
//!
//! # What does NOT follow: a harness's own conversation files
//!
//! Claude Code files each conversation under `~/.claude/projects/<encoded cwd>/`, and finds
//! it again only from that directory. charter does not write the harness's files (ADR 0050),
//! so a Claude Code chat recorded in the workspace cannot be resumed under the new name. The
//! rename goes ahead, and says first, by name, which chats will start a fresh conversation
//! ([`starts_fresh`]); the record then drops their conversation and says why
//! ([`crate::reopen::Chat::renamed_from`]), so reopening one starts fresh with a note instead
//! of failing to resume. A harness that finds a conversation by its id from anywhere (Codex,
//! opencode) is not affected, and is not named
//! ([`Harness::keeps_conversations_by_directory`]). The operator's ruling D10 on charter#367.
//!
//! Dispatch rows name no workspace (`docs/plane-format.md`), so there is nothing to rewrite in
//! them.
//!
//! # Why a clone with unpushed or uncommitted work is NOT refused
//!
//! `workspace remove` refuses over such work because it deletes it. A rename deletes nothing:
//! `rename(2)` moves the directory whole — working trees, `.git` directories, stashes, unpushed
//! branches — and the only thing a move can break is git's absolute paths between a clone and
//! its linked worktrees, which the repair step mends. Refusing would make an operator push work
//! they are not done with in order to change a name.
//!
//! # What IS refused, before anything is written
//!
//! - an invalid name on either side, or the same name twice;
//! - a `<new>` that exists (a case-only change on a case-insensitive disk is the same
//!   directory, and is allowed);
//! - a workspace directory that is a link out of the plane;
//! - a plane that relocates its worktree root, which this charter does not follow anywhere;
//! - **any chat running in the workspace**, named. A running harness holds the old path in its
//!   environment, its transcript's project key and every tool call it has queued, and nothing
//!   charter can write reaches inside it. The caller says which chats are running, because only
//!   the caller knows: the window from its own sessions, the terminal from the app's record while
//!   an app is listening on the plane ([`open_in_app`]).
//!
//! # Crash safety: a journal, one commit point, and steps that can all run twice
//!
//! 1. The journal `.charter/workspace-rename.json` (`{"from", "to", "moved"}`) is written.
//!    Until step 2 nothing else has changed, so a crash here leaves the old name fully
//!    working; the journal is a stale note the next rename overwrites.
//! 2. **The commit point**: `workspaces/<old>` is renamed to `workspaces/<new>` in one
//!    `rename(2)`, and the journal marks it `moved`.
//! 3. Every record above is rewritten, each step a no-op when it has already happened: a
//!    worktree already repaired lists under its new path and maps to nothing, a pointer that
//!    already says `<new>` does not say `<old>`, a block without `<old>` in it is not written.
//! 4. The journal is removed, and only when every step succeeded.
//! 5. A LIVE workspace's tracked files moved, so the plane is saved once — one save, not a
//!    commit of its own (ADR 0051).
//!
//! A crash anywhere after step 2 is finished by running the same command again, which finds
//! the journal and completes steps 3–5; any other rename is refused until it has, so two
//! renames never interleave. Nothing in step 3 can make the new name less usable than it was
//! a step earlier.

use std::path::{Path, PathBuf};

use crate::harness::Harness;
use crate::repocmd::{Say, Sink};
use crate::wscmd;

/// Where the journal of a rename in progress lives, relative to the plane's state folder
/// ([`journal_path`]).
pub const JOURNAL: &str = "workspace-rename.json";

/// What `charter workspace rename` was asked for.
pub struct Request<'a> {
    pub root: &'a Path,
    pub old: &'a str,
    pub new: &'a str,
    /// The chats running in this workspace — under either name, so a rename finished after a
    /// crash is guarded too — by the name the operator sees each one under. Any at all and the
    /// rename is refused, naming them.
    pub running: &'a [String],
    /// This machine's config home, where its pins are kept, or `None` for a machine with none.
    pub config_root: Option<&'a Path>,
}

/// How a workspace's paths and name map from the old name to the new one.
///
/// The one rule for everything that holds a path or a name, so the core's rewrite of the app's
/// record and the window's own copy of it cannot come to disagree.
#[derive(Debug, Clone)]
pub struct Move {
    pub old: String,
    pub new: String,
    /// `(from, to)` directory pairs: the plane's workspace directory as it is spelled, and as
    /// the filesystem resolves it (`/tmp` is a link on macOS, and git records resolved paths).
    dirs: Vec<(PathBuf, PathBuf)>,
    /// The harness each of the plane's profiles declares, by the profile's name: a chat on a
    /// profile runs the harness its `kind` says, whatever its command is called.
    kinds: std::collections::BTreeMap<String, Harness>,
}

impl Move {
    /// The move of `workspaces/<old>` to `workspaces/<new>` inside the plane at `root`.
    pub fn in_plane(root: &Path, old: &str, new: &str) -> Self {
        let dirs = spellings(root)
            .into_iter()
            .map(|plane| {
                (
                    plane.join("workspaces").join(old),
                    plane.join("workspaces").join(new),
                )
            })
            .collect();
        let kinds = crate::profiles::current(root)
            .profiles()
            .iter()
            .filter_map(|p| Some((p.name.clone(), Harness::of_kind(&p.kind)?)))
            .collect();
        Self {
            old: old.to_string(),
            new: new.to_string(),
            dirs,
            kinds,
        }
    }

    /// The harness a recorded chat runs: its profile's declared kind, else what its program
    /// is called — the rule the reopen starts it by.
    fn harness_of(&self, chat: &crate::reopen::Chat) -> Option<Harness> {
        match chat.profile.as_deref() {
            Some(profile) => self.kinds.get(profile).copied(),
            None => chat.harness(),
        }
    }

    /// Whether this move leaves `chat` with a conversation its harness can no longer find: it
    /// has one, it runs in the workspace, and its harness finds a conversation by directory.
    fn loses_conversation(&self, chat: &crate::reopen::Chat) -> bool {
        chat.resume.is_some()
            && chat.cwd.as_deref().and_then(|cwd| self.path(cwd)).is_some()
            && self
                .harness_of(chat)
                .is_some_and(Harness::keeps_conversations_by_directory)
    }

    /// Where `path` is after the move, or `None` for a path the move did not touch.
    pub fn path(&self, path: &Path) -> Option<PathBuf> {
        self.dirs.iter().find_map(|(from, to)| {
            let rest = path.strip_prefix(from).ok()?;
            Some(if rest.as_os_str().is_empty() {
                to.clone()
            } else {
                to.join(rest)
            })
        })
    }

    /// Follows the move in one chat of the app's record; `true` when it changed.
    ///
    /// A chat whose harness would no longer find its conversation drops it, and records the
    /// workspace it came from, so its next start is a fresh one that says why.
    pub fn chat(&self, chat: &mut crate::reopen::Chat) -> bool {
        let mut changed = false;
        if self.loses_conversation(chat) {
            chat.resume = None;
            if chat.renamed_from.is_none() {
                chat.renamed_from = Some(self.old.clone());
            }
            changed = true;
        }
        if let Some(cwd) = chat.cwd.as_deref().and_then(|cwd| self.path(cwd)) {
            chat.cwd = Some(cwd);
            changed = true;
        }
        if let Some(from) = chat.from.as_mut()
            && from.workspace.workspace() == Some(self.old.as_str())
        {
            from.workspace = crate::active::Place::Workspace(self.new.clone());
            changed = true;
        }
        changed
    }

    /// Follows the move in one view tab of the app's record; `true` when it changed.
    ///
    /// A view on the workspace's strip moves with it, and **every view keyed by the workspace's
    /// name is keyed by the new one** ([`renamed_key`], #1248) — its settings, its changes, a
    /// piece's files — titled by it too while its title is the one the window gave it under the
    /// old name. The window follows the same rule (`app/src/tabs.ts` `renamedView`), so the two
    /// agree on what comes back at the next launch.
    pub fn view(&self, view: &mut crate::reopen::View) -> bool {
        let mut changed = false;
        if view.workspace.as_deref() == Some(self.old.as_str()) {
            view.workspace = Some(self.new.clone());
            changed = true;
        }
        if view.from.is_none()
            && let Some(key) = renamed_key(&view.view, &view.key, &self.old, &self.new)
        {
            view.key = key;
            if let Some(new) = title_of(&view.view, &self.new)
                && titled_by(&view.view, &self.old, &view.title)
            {
                view.title = new;
            }
            changed = true;
        }
        changed
    }

    /// Follows the move through a whole record; `true` when anything in it changed.
    pub fn record(&self, record: &mut crate::reopen::Record) -> bool {
        let mut changed = false;
        for chat in &mut record.chats {
            changed |= self.chat(chat);
        }
        for view in &mut record.views {
            changed |= self.view(view);
        }
        if let Some(focus) = record.focus.as_mut() {
            changed |= self.focus(focus);
        }
        changed
    }

    /// Follows the move in the branch the window focused its sidebar on (FM-5); `true` when it
    /// changed.
    pub fn focus(&self, focus: &mut crate::reopen::Focus) -> bool {
        if focus.workspace != self.old {
            return false;
        }
        focus.workspace.clone_from(&self.new);
        true
    }
}

/// How a view of charter's own spells a workspace's name in its key, as `(spelling, whole)`:
/// `whole` is a key that is the spelling and nothing more, otherwise the spelling is the key's
/// first part and what follows its `/` is kept — a todo's slug, a memory's, a piece and a file, a
/// session record's file. `None` for a view not keyed by a workspace's name.
///
/// The window's table (`app/src/tabs.ts` `BY_WORKSPACE`), and tested against the same keys. The
/// views keyed by a clone's directory (`first-task`, `harness-setup`) are not here: a path is
/// never a key the record holds (`reopen::ViewOnDisk::held`).
fn spelling(view: &str, workspace: &str) -> Option<(String, bool)> {
    match view {
        "workspace-settings" | "changes" | "repo-instructions" => {
            Some((workspace.to_owned(), true))
        }
        "todo" | "piece-files" | "piece-file" | "piece-diff" => Some((workspace.to_owned(), false)),
        "memory" => Some((format!("workspace/{workspace}"), false)),
        "memory-archive" => Some((format!("workspace/{workspace}"), true)),
        "session" => Some((format!("workspaces/{workspace}"), false)),
        _ => None,
    }
}

/// `key`, a key of charter's own view `view`, after workspace `old` is renamed `new`; `None`
/// when it does not name `old`.
fn renamed_key(view: &str, key: &str, old: &str, new: &str) -> Option<String> {
    let (before, whole) = spelling(view, old)?;
    let (after, _) = spelling(view, new)?;
    if key == before {
        return Some(after);
    }
    if whole {
        return None;
    }
    let rest = key.strip_prefix(&before)?.strip_prefix('/')?;
    Some(format!("{after}/{rest}"))
}

/// What the window titles a view keyed by a workspace's name, where the title says the name
/// (`app/src/tabs.ts` `TITLED_BY_WORKSPACE`).
fn title_of(view: &str, workspace: &str) -> Option<String> {
    let before = match view {
        // "Settings", as at every other level, with the workspace it is at (D-1192-1).
        "workspace-settings" => "Settings",
        "changes" => "Changes",
        "repo-instructions" => "Memory from the repo",
        "memory-archive" => "Archived memory",
        _ => return None,
    };
    Some(format!("{before} · {workspace}"))
}

/// Whether `title` is what the window titled view `view` under workspace `workspace`'s name, now
/// or in an earlier version: Settings at a workspace's level was "Workspace settings · <ws>"
/// until D-1192-1, and a record from a launch before it still holds that title. The window puts
/// such a tab back under the new title (`app/src/tabs.ts` `viewNamedNow`); a rename between the
/// two launches still follows it.
fn titled_by(view: &str, workspace: &str, title: &str) -> bool {
    title_of(view, workspace).is_some_and(|now| now == title)
        || (view == "workspace-settings" && title == format!("Workspace settings · {workspace}"))
}

/// The steps of a rename, in order, named for a test to stop one dead after: the journal, the
/// commit point, then the steps after it — each of which can run twice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    /// The journal is written; nothing else has changed.
    Journal,
    /// `workspaces/<old>` is `workspaces/<new>`: the commit point.
    Move,
    Repair,
    Manifest,
    Headings,
    Live,
    Pointers,
    State,
    Links,
    Reopen,
    Pins,
}

#[cfg(test)]
thread_local! {
    /// A test's crash: the rename stops dead after this step, as a killed process would.
    static CRASH_AFTER: std::cell::Cell<Option<Step>> = const { std::cell::Cell::new(None) };
}

/// Whether a test has asked for the process to "die" after `step`.
fn crashed(step: Step) -> bool {
    #[cfg(test)]
    {
        CRASH_AFTER.with(|at| at.get() == Some(step))
    }
    #[cfg(not(test))]
    {
        let _ = step;
        false
    }
}

/// What the journal holds.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct Journal {
    from: String,
    to: String,
    #[serde(default)]
    moved: bool,
}

/// `path` as the operator knows it: from the plane root, as `.charter/app/reopen.json`.
fn shown_in(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}

pub fn journal_path(root: &Path) -> PathBuf {
    crate::names::state(root).join(JOURNAL)
}

fn read_journal(root: &Path) -> Option<Journal> {
    let path = journal_path(root);
    crate::contain::no_link_on_the_way(root, &path).ok()?;
    let text = std::fs::read_to_string(&path).ok()?;
    let journal: Journal = serde_json::from_str(&text).ok()?;
    (crate::contain::workspace_name_ok(&journal.from)
        && crate::contain::workspace_name_ok(&journal.to))
    .then_some(journal)
}

/// The two names of a workspace rename that is in progress, or was interrupted, as
/// `(from, to)`: what the journal says, or `None` when there is none.
///
/// For a reader that must not take either name for a workspace that is gone while the rename
/// is between its steps: the directory moves first and the pins follow last (`finish`), so in
/// between a pin names a directory that is not there yet, or no longer.
pub fn in_flight(root: &Path) -> Option<(String, String)> {
    read_journal(root).map(|journal| (journal.from, journal.to))
}

fn write_journal(root: &Path, journal: &Journal) -> std::io::Result<()> {
    let dir = crate::names::state(root);
    crate::plane::private_dir(root, &dir)?;
    let text = serde_json::to_string(journal).map_err(std::io::Error::other)? + "\n";
    crate::rewrite::replace(
        root,
        &journal_path(root),
        text.as_bytes(),
        crate::rewrite::Mode::Private,
    )
}

impl Journal {
    /// Whether this rename got past its commit point: the journal says so, or the disk does.
    fn committed(&self, root: &Path) -> bool {
        self.moved
            || (!wscmd::workspace_dir_exists(root, &self.from)
                && wscmd::workspace_dir_exists(root, &self.to))
    }
}

/// `charter workspace rename <old> <new>`, and its exit code: 0 renamed, 1 refused or not
/// finished.
pub fn rename(request: &Request, say: Sink) -> u8 {
    let Request { root, old, new, .. } = *request;
    for name in [old, new] {
        if wscmd::workspace_dir(root, name).is_none() {
            say(Say::Fail(format!(
                "invalid workspace name '{}' (use letters, digits, '.', '_', '-'; must not \
                 start with a dot)",
                crate::personas::one_line(name)
            )));
            return 1;
        }
    }
    if old == new {
        say(Say::Fail(format!(
            "'{old}' is already called that — nothing to rename."
        )));
        return 1;
    }

    // A rename that got past its commit point and did not finish is finished before anything
    // else is renamed, so two renames never interleave.
    let resuming = match read_journal(root) {
        Some(journal) if journal.from == old && journal.to == new => journal.committed(root),
        Some(journal) if journal.committed(root) => {
            say(Say::Fail(format!(
                "renaming '{}' to '{}' was interrupted and has not finished — finish it first: \
                 purlis workspace rename {} {}",
                journal.from, journal.to, journal.from, journal.to
            )));
            return 1;
        }
        // Written and never acted on: the old name still works, and this rename replaces it.
        _ => false,
    };

    if !request.running.is_empty() {
        let (are, them) = if request.running.len() == 1 {
            ("a chat is", "it")
        } else {
            ("chats are", "them")
        };
        say(Say::Fail(format!(
            "Refusing to rename '{old}' — {are} running in it: {}. A running chat keeps the old \
             path; close {them} first.",
            request.running.join(", "),
        )));
        return 1;
    }
    if let Err(why) = crate::worktree::relocation_refusal(root) {
        say(Say::Fail(format!(
            "'{old}' was not renamed: {why}, and a rename would leave its worktrees under the \
             old name."
        )));
        return 1;
    }

    let from = root.join("workspaces").join(old);
    let to = root.join("workspaces").join(new);
    if !resuming {
        if !wscmd::workspace_dir_exists(root, old) {
            say(Say::Fail(format!("no workspace '{old}'")));
            return 1;
        }
        if let Err(why) = crate::contain::no_link_on_the_way(root, &from) {
            say(Say::Fail(format!(
                "'{old}' does not resolve to a directory inside this plane, so it was not \
                 renamed ({why})."
            )));
            return 1;
        }
        if wscmd::workspace_dir_exists(root, new) && !same_directory(&from, &to) {
            say(Say::Fail(format!(
                "workspace '{new}' already exists — pick another name or remove it first."
            )));
            return 1;
        }
        if let Some(warning) = fresh_warning(&starts_fresh_on_disk(root, old)) {
            say(Say::Warn(warning));
        }
        let journal = Journal {
            from: old.to_string(),
            to: new.to_string(),
            moved: false,
        };
        if let Err(why) = write_journal(root, &journal) {
            say(Say::Fail(format!(
                "could not record the rename before starting it ({why}), so nothing was \
                 renamed."
            )));
            return 1;
        }
        if crashed(Step::Journal) {
            return 1;
        }
        // A dispatch grant limited to the new name was made for a workspace that is gone:
        // counted so now, while the name is nobody's, so this workspace inherits none (#1505).
        crate::dispatchwithin::noticed(root);
        // The commit point. Taken under the workspace's manifest lock (#1292, D-1292-1): a
        // writer that read `workspace.json` before the move finishes its write first, so it
        // never lands in a folder that has just gone. The lock is on the directory's inode,
        // which the move keeps, so it is let go here and the rewrite below takes it again.
        let moving = crate::rewrite::Lock::on(&from);
        let moved = std::fs::rename(&from, &to);
        drop(moving);
        if let Err(why) = moved {
            let _ = std::fs::remove_file(journal_path(root));
            say(Say::Fail(format!(
                "could not rename workspaces/{old} to workspaces/{new} ({why}), so nothing was \
                 renamed."
            )));
            return 1;
        }
        // Best effort: `Journal::committed` reads the disk as well, so a journal that still
        // says `moved: false` after a crash here is resumed all the same.
        //
        // **Not best effort.** Once the move is recorded, the journal alone says this rename is
        // past its commit point, whatever recreates `workspaces/<old>` before a rerun. A journal
        // that cannot say so is not trusted to: the move is put back and nothing is renamed.
        if let Err(why) = write_journal(
            root,
            &Journal {
                moved: true,
                ..journal
            },
        ) {
            let back = std::fs::rename(&to, &from);
            let _ = std::fs::remove_file(journal_path(root));
            say(Say::Fail(match back {
                Ok(()) => format!(
                    "could not record that workspaces/{old} moved ({why}), so it was put back \
                     and nothing was renamed."
                ),
                Err(stuck) => format!(
                    "could not record that workspaces/{old} moved ({why}), nor put it back \
                     ({stuck}). It is at workspaces/{new}: finish with purlis workspace \
                     rename {old} {new}"
                ),
            }));
            return 1;
        }
    } else {
        say(Say::Info(format!(
            "Finishing the rename of '{old}' to '{new}' that was interrupted."
        )));
        // A directory under the old name that something created since the move is not this
        // workspace, and is left alone and said.
        if wscmd::workspace_dir_exists(root, old) {
            say(Say::Warn(format!(
                "workspaces/{old} exists again — something recreated it after the rename. It \
                 is left as it is."
            )));
        }
    }
    if crashed(Step::Move) {
        return 1;
    }
    finish(request, say)
}

/// Whether two paths are one directory — a case-only rename on a case-insensitive disk.
fn same_directory(one: &Path, other: &Path) -> bool {
    same_entry(one, other)
}

/// Whether two paths name one file or directory, without following a link.
#[cfg(unix)]
fn same_entry(one: &Path, other: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    match (
        std::fs::symlink_metadata(one),
        std::fs::symlink_metadata(other),
    ) {
        (Ok(a), Ok(b)) => a.dev() == b.dev() && a.ino() == b.ino(),
        _ => false,
    }
}

#[cfg(not(unix))]
fn same_entry(one: &Path, other: &Path) -> bool {
    matches!(
        (std::fs::canonicalize(one), std::fs::canonicalize(other)),
        (Ok(a), Ok(b)) if a == b
    )
}

/// Steps 3–5: everything that names the workspace, then the journal, then the save.
fn finish(request: &Request, say: Sink) -> u8 {
    let Request {
        root,
        old,
        new,
        config_root,
        ..
    } = *request;
    let moved = Move::in_plane(root, old, new);
    let mut left: Vec<String> = Vec::new();
    // **Dispatch grants do not follow a rename** (#1505): the old name is counted gone, here
    // and so on a rename finished after a crash too, and the grants limited to it cover
    // nothing until the person sets each one's workspace again.
    let grants_left = crate::dispatchwithin::naming(root, old);
    crate::dispatchwithin::noticed(root);

    left.extend(repair_worktrees(root, &moved));
    if crashed(Step::Repair) {
        return 1;
    }
    if let Err(why) = rename_manifest(root, new) {
        left.push(format!(
            "workspaces/{new}/workspace.json still names '{old}' ({why})"
        ));
    }
    if crashed(Step::Manifest) {
        return 1;
    }
    left.extend(rename_headings(root, old, new));
    if crashed(Step::Headings) {
        return 1;
    }
    let live = match wscmd::rename_live(root, old, new) {
        Ok(_) => wscmd::live_workspaces(root).contains(new),
        Err(why) => {
            left.push(format!(
                "the plane's .gitignore still marks '{old}' LIVE ({why})"
            ));
            false
        }
    };
    if crashed(Step::Live) {
        return 1;
    }
    left.extend(rename_pointers(root, old, new));
    if crashed(Step::Pointers) {
        return 1;
    }
    left.extend(rename_state(root, old, new));
    if crashed(Step::State) {
        return 1;
    }
    let links_left_behind = match follow_in_links(root, old, new, config_root) {
        Ok(followed) => followed.refused.len(),
        Err(why) => {
            left.push(format!(
                "the work links to the todos of '{old}' were not carried to '{new}' ({why})"
            ));
            0
        }
    };
    if crashed(Step::Links) {
        return 1;
    }
    if let Err(why) = follow_in_reopen(root, &moved, config_root) {
        left.push(format!(
            "the app's record ({}) still names '{old}' ({why})",
            shown_in(root, &crate::reopen::path(root))
        ));
    }
    if crashed(Step::Reopen) {
        return 1;
    }
    if let Some(config) = config_root
        && let Err(why) = follow_in_pins(config, root, old, new)
    {
        left.push(format!(
            "this machine's pin on '{old}' was not moved ({why})"
        ));
    }
    if crashed(Step::Pins) {
        return 1;
    }

    if !left.is_empty() {
        say(Say::Fail(format!(
            "Renamed workspaces/{old} to workspaces/{new}, but not everything that names it \
             followed. Run the same command again to finish: purlis workspace rename {old} {new}"
        )));
        wscmd::say_each(say, left);
        return 1;
    }
    let _ = std::fs::remove_file(journal_path(root));
    for stale in config_named(root, old) {
        say(Say::Warn(stale));
    }
    say(Say::Done(format!("Renamed workspace '{old}' to '{new}'.")));
    if links_left_behind > 0 {
        say(Say::Warn(format!(
            "The work links to {links_left_behind} todo(s) of '{new}' still resolve under \
             '{old}': it was called '{new}' before, and an alias back to a name a key left \
             would close a cycle."
        )));
    }
    if grants_left > 0 {
        say(Say::Warn(crate::dispatchwithin::left_behind_said(
            grants_left,
            old,
            Some(new),
        )));
    }

    // Something tracked moved only for a LIVE workspace or a committed default — asked of the
    // disk, so a rename finished after a crash saves what the first run changed.
    let default_is_new = std::fs::read_to_string(wscmd::select::default_file(root))
        .is_ok_and(|text| crate::memstore::py_strip(&text) == new);
    if live || default_is_new {
        save(root, old, new, say);
    }
    0
}

/// The plane save a rename of tracked files takes (ADR 0051): one save, and none when the
/// plane has not been told how it saves — a rename is not an answer to that question.
fn save(root: &Path, old: &str, new: &str, say: Sink) {
    if crate::planesave::Settings::read(root)
        .plane
        .mode
        .value
        .is_none()
    {
        say(Say::Info(
            "Not saved: this plane has not been told how it is saved yet — the next \
             `purlis save` takes the rename."
                .to_string(),
        ));
        return;
    }
    // The plane's history keeps it: the program as the plane spells it (D-RN11a-4).
    let message = format!(
        "{} workspace rename {old} {new}",
        crate::names::BINARY.writes_for(root)
    );
    let code = crate::planegit::save_as(
        &crate::planegit::Request {
            root,
            message: Some(&message),
            sign: false,
            no_push: false,
            cwd: root,
            provenance: None,
        },
        crate::planegit::Trigger::Rename,
        say,
    );
    if code != 0 {
        say(Say::Warn(
            "The rename is done on disk; the save did not finish, and the next save takes it."
                .to_string(),
        ));
    }
}

// ----------------------------------------------------------------------------------------
// the steps
// ----------------------------------------------------------------------------------------

/// `git worktree repair` for every worktree the move touched, answering what could not be
/// repaired, each with the command that repairs it.
///
/// **Asked of git's own listing, after the move.** A moved clone still lists its moved
/// worktrees under their old paths (as prunable), which is exactly what maps them to their new
/// ones; once repaired they list under the new paths and map to nothing, so this runs twice
/// harmlessly. Every clone is repaired even with no path to hand, because that form mends the
/// back-links of its worktrees that did NOT move. The plane's own repository is asked too, for
/// a worktree of the plane kept inside the workspace, and each top-level checkout whose `.git`
/// is a file is repaired from its own side, which mends a repository outside the workspace.
fn repair_worktrees(root: &Path, moved: &Move) -> Vec<String> {
    use crate::worktree::git;
    let mut left = Vec::new();
    let dir = root.join("workspaces").join(&moved.new);
    let mut mains: Vec<(PathBuf, bool)> = match crate::repos::clones(root, &moved.new) {
        Ok(found) => found.repos.into_iter().map(|r| (r.path, true)).collect(),
        Err(why) => {
            left.push(format!(
                "the clones of '{}' could not be listed ({why})",
                moved.new
            ));
            Vec::new()
        }
    };
    if root.join(".git").exists() {
        mains.push((root.to_path_buf(), false));
    }
    for (main, always) in mains {
        let listed = match git::run(&main, &["worktree", "list", "--porcelain"], git::READ) {
            Ok(run) if run.ok() => run,
            Ok(run) => {
                left.push(format!(
                    "{}: git would not list its worktrees ({}) — repair them: git -C {} \
                     worktree repair",
                    main.display(),
                    run.err.trim(),
                    main.display()
                ));
                continue;
            }
            Err(why) => {
                left.push(format!("{}: git could not run ({why})", main.display()));
                continue;
            }
        };
        let paths: Vec<String> = crate::worktree::porcelain::parse(&listed.out)
            .into_iter()
            .skip(1)
            .filter_map(|row| moved.path(&row.path))
            .filter(|path| path.exists())
            .map(|path| path.display().to_string())
            .collect();
        if !always && paths.is_empty() {
            continue;
        }
        let mut argv = vec!["worktree", "repair"];
        argv.extend(paths.iter().map(String::as_str));
        let answer = git::run(&main, &argv, git::READ);
        if !answer.as_ref().is_ok_and(git::Run::ok) {
            let why = match answer {
                Ok(run) => run.err.trim().to_string(),
                Err(why) => why.to_string(),
            };
            left.push(format!(
                "{}: its worktrees were not repaired ({why}) — run: git -C {} {}",
                main.display(),
                main.display(),
                argv.join(" ")
            ));
        }
    }
    // A checkout directly in the workspace that is itself a linked worktree of a repository
    // elsewhere: repaired from its own side.
    if let Ok(reader) = std::fs::read_dir(&dir) {
        let mut loose: Vec<PathBuf> = reader
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|tree| {
                std::fs::symlink_metadata(tree.join(".git")).is_ok_and(|m| m.file_type().is_file())
            })
            .collect();
        loose.sort();
        for tree in loose {
            let answer = git::run(&tree, &["worktree", "repair"], git::READ);
            if !answer.as_ref().is_ok_and(git::Run::ok) {
                left.push(format!(
                    "{}: its link to its repository was not repaired — run: git -C {} \
                     worktree repair",
                    tree.display(),
                    tree.display()
                ));
            }
        }
    }
    left
}

/// `workspace.json`'s `name`, rewritten, keeping who owns the file: one charter wrote is
/// stamped again, one a hand wrote stays unstamped. No manifest, nothing to do.
fn rename_manifest(root: &Path, new: &str) -> std::io::Result<()> {
    let plane = crate::workspaces::Plane::open(root);
    let ws = plane
        .workspace(new)
        .map_err(|why| std::io::Error::other(why.to_string()))?;
    let _held = ws.manifest_lock()?;
    let (doc, owner) = ws.manifest();
    let Some(serde_json::Value::Object(mut map)) = doc else {
        return Ok(());
    };
    if map.get("name").and_then(serde_json::Value::as_str) == Some(new) {
        return Ok(());
    }
    map.insert("name".into(), serde_json::Value::String(new.to_string()));
    ws.write_manifest_as(
        &serde_json::Value::Object(map),
        owner == crate::manifest::Ownership::Charter,
    )
}

/// A scaffolded file's first line, for a workspace's name.
type Heading = fn(&str) -> String;

/// The first line of each file charter scaffolded with the workspace's name in it, where it is
/// still the line charter wrote. A heading the operator rewrote is theirs and is left.
fn rename_headings(root: &Path, old: &str, new: &str) -> Vec<String> {
    let dir = root.join("workspaces").join(new);
    let headings: [(&str, Heading); 4] = [
        ("workspace.md", |n| format!("# {n}")),
        ("memory/MEMORY.md", |n| format!("# {n} — task memory")),
        ("todos/MEMORY.md", |n| format!("# Todos — workspace `{n}`")),
        ("refs/README.md", |n| format!("# {n} — task references")),
    ];
    let mut left = Vec::new();
    for (rel, heading) in headings {
        let path = dir.join(rel);
        if crate::contain::no_link_on_the_way(root, &path).is_err() {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let (first, rest) = text.split_once('\n').unwrap_or((&text, ""));
        if first != heading(old) {
            continue;
        }
        let next = format!("{}\n{rest}", heading(new));
        if let Err(why) =
            crate::rewrite::replace(root, &path, next.as_bytes(), crate::rewrite::Mode::Kept)
        {
            left.push(format!(
                "workspaces/{new}/{rel} still says '{old}' in its heading ({why})"
            ));
        }
    }
    left
}

/// Every pointer that selects `old`, pointed at `new`. Answers what could not be.
fn rename_pointers(root: &Path, old: &str, new: &str) -> Vec<String> {
    let state = crate::names::state(root);
    let mut left = Vec::new();
    let mut files: Vec<(PathBuf, crate::rewrite::Mode)> = vec![
        (
            wscmd::select::default_file(root),
            crate::rewrite::Mode::Kept,
        ),
        (
            state.join("active-workspace"),
            crate::rewrite::Mode::Private,
        ),
    ];
    for (sub, suffixes) in [
        ("sessions", &[".workspace", ".lock"][..]),
        ("terminals", &[".workspace"][..]),
    ] {
        let Ok(reader) = std::fs::read_dir(state.join(sub)) else {
            continue;
        };
        let mut found: Vec<PathBuf> = reader
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                let name = path.file_name().unwrap_or_default().to_string_lossy();
                suffixes.iter().any(|s| name.ends_with(s))
            })
            .collect();
        found.sort();
        files.extend(
            found
                .into_iter()
                .map(|path| (path, crate::rewrite::Mode::Private)),
        );
    }
    for (path, mode) in files {
        if crate::contain::no_link_on_the_way(root, &path).is_err()
            || !std::fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_file())
        {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        if crate::memstore::py_strip(&text) != old {
            continue;
        }
        if let Err(why) = crate::rewrite::replace(root, &path, format!("{new}\n").as_bytes(), mode)
        {
            left.push(format!("{} still selects '{old}' ({why})", path.display()));
        }
    }
    // The strip's order, one name per line.
    let order = state.join("workspace-tab-order");
    if crate::contain::no_link_on_the_way(root, &order).is_ok()
        && let Ok(text) = std::fs::read_to_string(&order)
        && text.lines().any(|line| line.trim() == old)
    {
        let next: String = text
            .lines()
            .map(|line| if line.trim() == old { new } else { line })
            .map(|line| format!("{line}\n"))
            .collect();
        if let Err(why) =
            crate::rewrite::replace(root, &order, next.as_bytes(), crate::rewrite::Mode::Private)
        {
            left.push(format!("{} still lists '{old}' ({why})", order.display()));
        }
    }
    left
}

/// The work link log follows the rename (ADR 0088 §2): each `todo:` key of the old name that a
/// log names gets a `renamed` alias to the same todo under the new name, written to this
/// device's log of the renamed workspace. A key aliased already is left alone, so a rename
/// finished after a crash writes each alias once. The device id is asked only when there is an
/// alias to write.
fn follow_in_links(
    root: &Path,
    old: &str,
    new: &str,
    config_root: Option<&Path>,
) -> std::io::Result<crate::work::log::Followed> {
    let device = || match config_root {
        Some(config) => crate::machine::device_id(config),
        None => Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "there is no config home to keep this device's id in",
        )),
    };
    crate::work::log::follow_rename(root, old, new, &device, chrono::Utc::now())
}

/// The state charter keeps under the workspace's name in `.charter/`, moved to the new one.
fn rename_state(root: &Path, old: &str, new: &str) -> Vec<String> {
    let state = crate::names::state(root);
    let mut left = Vec::new();
    for (from, to) in [
        (
            state.join("workspace-arrivals").join(old),
            state.join("workspace-arrivals").join(new),
        ),
        (
            state.join("ws-autosave").join(old),
            state.join("ws-autosave").join(new),
        ),
        (
            crate::handback::dir(root).join(format!("workspace-{old}")),
            crate::handback::dir(root).join(format!("workspace-{new}")),
        ),
    ] {
        if let Err(why) = move_over(root, &from, &to) {
            // Where the chat's sandbox denies it what waits for a chat's next turn (a harness
            // whose hooks run outside its sandbox, D-T59-19), a rename run inside that chat
            // leaves the reports kept for the workspace, and says who moves them instead.
            if from.starts_with(crate::handback::dir(root))
                && why.kind() == std::io::ErrorKind::PermissionDenied
            {
                left.push(format!(
                    "{} was not moved: this chat's sandbox does not let it move kept reports; \
                     the app moves them when the project is next opened",
                    from.display()
                ));
                continue;
            }
            left.push(format!(
                "{} was not moved to {} ({why})",
                from.display(),
                to.display()
            ));
        }
    }
    left
}

/// **The reports kept for a workspace follow a rename a sandboxed chat could not finish**
/// (D-T59-19): called by the app as it opens the project, which is not sandboxed.
///
/// A rename run inside a chat whose sandbox denies it the kept reports moves the workspace and
/// cannot move `handbacks/workspace-<old>`, so it leaves its journal behind ([`journal_path`]) with the
/// move made. Where that journal says so, the old workspace is gone and the new one is there,
/// the kept reports are moved under the new name, where the next chat to start in it reads
/// them. Nothing else of the rename is finished here: the same command, run again, does that.
///
/// The journal is a file a chat can write. All it can do here is move reports kept for a
/// workspace that no longer exists to one that does, both names held to a workspace's rule.
pub fn kept_reports_follow(root: &Path) -> std::io::Result<bool> {
    let Some(journal) = read_journal(root) else {
        return Ok(false);
    };
    let (old, new) = (journal.from.as_str(), journal.to.as_str());
    if !journal.moved
        || !crate::contain::workspace_name_ok(old)
        || !crate::contain::workspace_name_ok(new)
        || wscmd::workspace_dir_exists(root, old)
        || !wscmd::workspace_dir_exists(root, new)
    {
        return Ok(false);
    }
    let from = crate::handback::dir(root).join(format!("workspace-{old}"));
    if std::fs::symlink_metadata(&from).is_err() {
        return Ok(false);
    }
    move_over(
        root,
        &from,
        &crate::handback::dir(root).join(format!("workspace-{new}")),
    )?;
    Ok(true)
}

/// Moves `from` to `to`. A directory that is already at `to` takes `from`'s entries one by one,
/// so two reports left for one workspace are both kept; a file already at `to` is newer than
/// the one it replaces, which is dropped.
fn move_over(root: &Path, from: &Path, to: &Path) -> std::io::Result<()> {
    let Ok(found) = std::fs::symlink_metadata(from) else {
        return Ok(());
    };
    crate::contain::no_link_on_the_way(root, from)?;
    crate::contain::no_link_on_the_way(root, to)?;
    match std::fs::symlink_metadata(to) {
        Err(_) => std::fs::rename(from, to),
        // A case-only rename on a case-insensitive disk: `to` IS `from`, and renaming it is
        // what changes its case. The merge below would delete it as a duplicate of itself.
        Ok(_) if same_entry(from, to) => std::fs::rename(from, to),
        Ok(there) if found.is_dir() && there.is_dir() => {
            for entry in std::fs::read_dir(from)? {
                let entry = entry?;
                let target = to.join(entry.file_name());
                if std::fs::symlink_metadata(&target).is_err() {
                    std::fs::rename(entry.path(), target)?;
                }
            }
            std::fs::remove_dir_all(from)
        }
        Ok(_) if found.is_dir() => std::fs::remove_dir_all(from),
        Ok(_) => std::fs::remove_file(from),
    }
}

/// The app's record, following the move. Nothing is written when nothing in it changed.
///
/// **Written, then vouched for** (`machine::Store::vouch`'s wiring contract): the record's
/// directories are part of the trust fingerprint, so a record charter rewrote and did not
/// vouch for would ask the operator about their own chats at the next launch.
fn follow_in_reopen(root: &Path, moved: &Move, config: Option<&Path>) -> std::io::Result<()> {
    if std::fs::symlink_metadata(crate::reopen::path(root)).is_err() {
        return Ok(());
    }
    let mut record = crate::reopen::read_or_refusal(root)?;
    if !moved.record(&mut record) {
        return Ok(());
    }
    crate::reopen::write(root, &record)?;
    let Some(config) = config else {
        return Ok(());
    };
    let contributed = crate::machine::Contribution::of(root);
    let when = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());
    let planes = spellings(root);
    crate::machine::update(config, |store| {
        for plane in &planes {
            store.vouch(plane, contributed.clone(), when);
        }
    })
    .map(|_| ())
}

/// The plane as a machine store may have remembered it ([`crate::machine::spellings_of`]).
fn spellings(root: &Path) -> Vec<PathBuf> {
    crate::machine::spellings_of(root)
}

/// This machine's pin on the workspace, moved to the new name — under the plane as it was
/// opened, which may be the path as typed or as resolved.
fn follow_in_pins(config: &Path, root: &Path, old: &str, new: &str) -> std::io::Result<()> {
    let planes = spellings(root);
    // Only a store that has this plane is written: a rename is not a reason to create one.
    let store = crate::machine::read(config);
    if !planes
        .iter()
        .any(|plane| store.store.recent(plane).is_some())
    {
        return Ok(());
    }
    crate::machine::update(config, |store| {
        for plane in &planes {
            store.rename_workspace(plane, old, new);
        }
    })
    .map(|_| ())
}

/// A sentence for each settings file whose `[workspace] default` still names `old`.
///
/// Said, not rewritten: `charter.toml` is the team's and `charter.local.toml` the operator's,
/// both hand-edited, and a rename is not a settings edit.
fn config_named(root: &Path, old: &str) -> Vec<String> {
    [
        crate::names::manifest(root),
        crate::names::local_settings(root),
    ]
    .into_iter()
    .filter(|file| {
        std::fs::read_to_string(file)
            .ok()
            .and_then(|text| text.parse::<toml::Table>().ok())
            .and_then(|doc| {
                doc.get("workspace")?
                    .as_table()?
                    .get("default")?
                    .as_str()
                    .map(|name| name == old)
            })
            .unwrap_or(false)
    })
    .map(|file| {
        let file = shown_in(root, &file);
        format!(
            "{file} still sets [workspace] default = \"{old}\" — edit it if the default \
                 should follow the rename."
        )
    })
    .collect()
}

// ----------------------------------------------------------------------------------------
// which chats start a fresh conversation after it
// ----------------------------------------------------------------------------------------

/// The chats in `record` that will start a fresh conversation once `workspace` is renamed,
/// whatever to, by the name each is shown under (charter#367, D10).
///
/// A chat is named when it has a conversation to resume, runs in the workspace, and its
/// harness finds a conversation by the directory it ran in — Claude Code. A Codex or opencode
/// chat resumes by its id wherever it runs, and is not named.
pub fn starts_fresh(root: &Path, workspace: &str, record: &crate::reopen::Record) -> Vec<String> {
    // Only where a path moves FROM decides which chats lose theirs, so any new name will do.
    let moved = Move::in_plane(root, workspace, workspace);
    record
        .chats
        .iter()
        .filter(|chat| moved.loses_conversation(chat))
        .map(|chat| crate::reopen::shown_name(chat, moved.harness_of(chat).map(Harness::name)))
        .collect()
}

/// [`starts_fresh`], for the app's record on disk — what a terminal's rename knows. A record
/// that cannot be read names nothing.
pub fn starts_fresh_on_disk(root: &Path, workspace: &str) -> Vec<String> {
    crate::reopen::read_or_refusal(root)
        .map(|record| starts_fresh(root, workspace, &record))
        .unwrap_or_default()
}

/// The sentence that names them, or none when there are none.
pub fn fresh_warning(chats: &[String]) -> Option<String> {
    let (one, they) = match chats.len() {
        0 => return None,
        1 => ("This chat", "its conversation"),
        _ => ("These chats", "their conversations"),
    };
    Some(format!(
        "{one} will start a fresh conversation after the rename: {}. Claude Code keeps \
         {they} under the folder it ran in, and purlis does not move that folder.",
        chats.join(", ")
    ))
}

// ----------------------------------------------------------------------------------------
// who is running in it, as a terminal can tell
// ----------------------------------------------------------------------------------------

/// The chats an app listening on this plane has open in any of `workspaces`, by the name each
/// is shown under — what a terminal's `charter workspace rename` refuses over.
///
/// **The app's record is what it has open while it runs**: it rewrites `reopen.json` whenever
/// a chat starts or closes. After it quits the record lists what it will reopen, which is
/// nothing running — so the record is read only while something accepts on the app's socket
/// (`.charter/app/hooks.sock`), the question the status line asks. A plane too deep for its
/// socket to live beside the record puts it elsewhere, and a harness started by hand in a
/// terminal is not the app's: neither is seen here.
pub fn open_in_app(root: &Path, workspaces: &[&str]) -> Vec<String> {
    let plane = crate::workspaces::Plane::open(root);
    open_in_app_where(root, |chat| {
        chat.cwd
            .as_deref()
            .and_then(|cwd| plane.workspace_of(cwd))
            .is_some_and(|ws| workspaces.contains(&ws.as_str()))
    })
}

/// The chats an app listening on this plane has open as persona `persona`, by the name each is
/// shown under: the live runs that have adopted it, which removing it is refused over (V27d,
/// ADR 0076 §10). Read as [`open_in_app`] reads the record, and only while the app listens.
pub fn adopting_in_app(root: &Path, persona: &str) -> Vec<String> {
    open_in_app_where(root, |chat| chat.persona.as_deref() == Some(persona))
}

/// The chats the app's record holds that `keep` keeps, by the name each is shown under, while
/// an app listens on this plane; none otherwise.
fn open_in_app_where(root: &Path, keep: impl Fn(&crate::reopen::Chat) -> bool) -> Vec<String> {
    if !app_is_listening(root) {
        return Vec::new();
    }
    let Ok(record) = crate::reopen::read_or_refusal(root) else {
        return Vec::new();
    };
    record
        .chats
        .iter()
        .filter(|chat| keep(chat))
        .map(|chat| crate::reopen::shown_name(chat, chat.harness().map(|h| h.name())))
        .collect()
}

#[cfg(unix)]
fn app_is_listening(root: &Path) -> bool {
    std::os::unix::net::UnixStream::connect(crate::names::state(root).join("app/hooks.sock"))
        .is_ok()
}

#[cfg(not(unix))]
fn app_is_listening(_root: &Path) -> bool {
    false
}

#[cfg(test)]
mod tests;
