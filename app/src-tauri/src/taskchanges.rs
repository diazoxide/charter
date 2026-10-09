//! The app's half of what a task changed (#1511, spec #1483): the list its Changes tab draws,
//! the person's merge of a task's own branch, and the warning for two tasks in one folder.
//!
//! `purlis_core::taskchanges` argues how a task's changes are told from its siblings', and
//! `purlis_core::dispatchplace` holds the merge. This module names the task: **every command
//! here takes a dispatch's id and nothing else of it**, and the folder, the repo and the
//! branch are read from the app's own record of that dispatch. A window never names a path or
//! a branch to merge.
//!
//! - **The list** ([`task_changes`]). A task on a branch of its own: what that branch changed
//!   against the branch it was cut from, which is the task's alone. A task that worked in a
//!   folder other chats work in: the files its own edit tools wrote ([`Touched`], in memory
//!   only) that git still finds uncommitted there, each marked where another chat's edit tools
//!   wrote it too. It cannot see a shell command's edits, the person's, the task's own commits
//!   there, or anything after a restart, and the tab says so. Where purlis holds no such list it
//!   says that too.
//! - **Merge** ([`task_branch_merge_question`], [`task_branch_merge`]). The person's act, in
//!   two steps: the question reads what would land, and the answer hands it back, so what is
//!   merged is what they were shown or nothing is. A fast-forward or a refusal that says why.
//!   Refused while the task still runs or a chat stands in the folder.
//! - **Discard** is `crate::dispatches`' (#1453), asked the same way.
//!
//! **Only the person has purlis merge or discard.** Both are commands of the window alone
//! (`purlis_session_protocol::ui::WINDOW_ONLY`): no link serves them, and no line on the hook
//! channel names either. A chat may ask for a merge in words; nothing it sends to purlis
//! performs one. (A chat started without the sandbox runs as the person, and can run git in
//! the clone itself; a sandboxed one cannot.)

use std::collections::BTreeMap;
use std::sync::{Mutex, PoisonError};

use purlis_core::dispatchplace::{self, Standing, Tidied, Tree};
use purlis_core::dispatchrecord::{self, Record};
use purlis_core::files::Branch;
use purlis_core::reopen::Mode;
use purlis_core::taskchanges::{self as core, Paths, Working};

use crate::piecefiles::{ChangeMark, FileChange};
use crate::planes::{Held, PlaneId, Planes};

/// The files each chat's edit tools wrote while this app has been running
/// (`purlis_core::taskchanges::Touched`): in memory only, and never written (D-86a).
#[derive(Default)]
pub struct Touched(Mutex<core::Touched>);

impl Touched {
    fn note(&self, chat: &str, name: &str, path: String) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .note(chat, name, path);
    }

    fn of(&self, chat: &str) -> Option<Paths> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .of(chat)
            .cloned()
    }

    /// Every other chat whose edit tools wrote `path`, by name ([`core::Touched::also`]).
    fn also(&self, but: &str, path: &str) -> Vec<String> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .also(but, path)
    }
}

/// A file tool of chat `touching.chat` named a path: kept where the tool writes
/// ([`core::keeps`]), once it is confined to that chat's own folder, as a path of the project.
/// **Every chat's, not only a task's** (#1534): a task's Changes list is its own writes, and
/// another chat's writes to the same file, the asking chat's or any other's, are what mark it
/// as maybe not the task's alone. The chat's word, so it marks nothing by itself: the Changes
/// tab lists only the ones git also finds changed.
pub(crate) fn touched(held: &Held, touching: &purlis_core::hookwire::Touching) {
    // Only what an edit tool wrote: a file a chat read is not its change.
    if !core::keeps(touching) {
        return;
    }
    let Some(at) = held.chats().chat_at(touching.chat) else {
        return;
    };
    let (Some(id), Some(cwd)) = (at.id, at.cwd) else {
        return;
    };
    let Some(inside) = purlis_core::touching::confine(&cwd, &touching.touching) else {
        return;
    };
    if let Some(path) = core::in_project(held.root(), &cwd, &inside) {
        let name = held
            .chats()
            .shown_name(touching.chat)
            .unwrap_or_else(|| "another chat".to_owned());
        held.touched_files().note(&id, &name, path);
    }
}

// ---------------------------------------------------------------------------------------
// The Changes tab
// ---------------------------------------------------------------------------------------

/// One file a task changed.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub(crate) struct TaskFile {
    /// Its path inside the repo's or the branch's folder.
    pub path: String,
    pub mark: ChangeMark,
    /// Where a renamed file came from: a name to show, never a path to open.
    pub from: Option<String>,
    pub uncommitted: bool,
    /// The other chats whose edit tools wrote this file too, by name: a sibling task, the
    /// asking chat or any other this app heard from. Its change may be theirs in part. The
    /// person's own edits, and a shell command's, are not marked: no file tool names them.
    pub also: Vec<String>,
}

/// The files a task changed in one repo's folder, or in one branch's.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub(crate) struct ChangedIn {
    pub workspace: String,
    pub repo: String,
    /// The branch folder; `null` for the repo's own.
    pub piece: Option<String>,
    /// What the changes are counted against; `null` when against the last commit.
    pub base: Option<String>,
    pub files: Vec<TaskFile>,
    /// How many changes git found past the most it lists. Of a task in a shared folder, a file
    /// of its own past them is not listed.
    pub more: u32,
    /// Why purlis could not read what changed there, where it could not.
    pub unread: Option<String>,
}

/// A task's own branch, as its Changes tab says it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub(crate) struct OwnBranch {
    pub repo: String,
    pub branch: Option<String>,
    /// `kept`, `merged`, `merged-branch-kept`, `discarded` or `gone`
    /// (`purlis_core::dispatchplace::Standing::word`).
    pub standing: String,
    /// Whether Merge and Discard are offered: its folder is there and the task has ended.
    /// Each is still refused while a chat stands in the folder.
    pub acts: bool,
}

/// What a task changed, as its Changes tab draws it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub(crate) struct TaskChanges {
    /// The dispatch's id.
    pub id: String,
    /// The task's name.
    pub task: String,
    /// Whether the task is still running: what is listed is what it has changed so far.
    pub running: bool,
    /// Its own branch, where the dispatch gave it one: then `places` is everything that
    /// branch changed since it was cut.
    pub own: Option<OwnBranch>,
    /// The files it changed, by where they are.
    pub places: Vec<ChangedIn>,
    /// Paths its edit tools wrote that lie in no repo, relative to the project: purlis has
    /// nothing to compare them against, so they are named and not said to have changed.
    pub elsewhere: Vec<String>,
    /// Whether its edit tools wrote more files than purlis kept.
    pub more: bool,
    /// What its report says changed, in the task's own words.
    pub said: Option<String>,
    /// Why no file is listed, where purlis cannot say which files were this task's.
    pub unknown: Option<String>,
}

/// What a shared-folder task's tab says where purlis holds no list of its files.
const NOT_KEPT: &str = "purlis cannot say which files this task changed. It worked in a folder \
                        other chats work in, where what tells tasks apart is the files each \
                        one's own edit tools wrote. Those are kept in memory only, and none is \
                        held for this task: the app was started again since it ran, more than \
                        128 chats were heard from since, its harness reports no file tool, or it \
                        wrote no file with one (a shell command's edits are not seen).";

/// The task's name: the one its dispatch gave it, else its chat's.
fn name_of(record: &Record) -> String {
    record
        .task
        .clone()
        .unwrap_or_else(|| record.worker.chat.name.clone())
}

/// The dispatch `id`, where purlis has a record of it that it draws.
fn record_of(held: &Held, id: &str) -> Result<Record, String> {
    dispatchrecord::read(held.root(), id)
        .filter(dispatchrecord::sound)
        .ok_or_else(|| "purlis has no record of that task.".to_owned())
}

/// What `branch` changed, as one place of a Changes tab, each file as `file` makes it.
fn changed_in(
    held: &Held,
    (workspace, repo, piece): (&str, &str, Option<&str>),
    file: impl Fn(FileChange) -> Option<TaskFile>,
) -> ChangedIn {
    let named = piece.map(str::to_owned);
    let read = crate::piecefiles::status_of(
        held.root(),
        crate::piecefiles::branch(workspace, repo, &named),
    );
    let (base, files, more, unread) = match read {
        Ok(status) => (
            status.base,
            status.changes.into_iter().filter_map(file).collect(),
            status.more,
            None,
        ),
        Err(why) => (None, Vec::new(), 0, Some(why)),
    };
    ChangedIn {
        workspace: workspace.to_owned(),
        repo: repo.to_owned(),
        piece: named,
        base,
        files,
        more,
        unread,
    }
}

/// What a task on its own branch changed: everything that branch holds against the branch it
/// was cut from. Nobody else works in its folder, so it is the task's alone.
fn of_its_own_branch(held: &Held, record: &Record, changes: &mut TaskChanges) {
    let Some(recorded) = record.place.worktree.as_ref() else {
        return;
    };
    let standing = dispatchplace::standing(held.root(), record).unwrap_or(Standing::Gone);
    let tree = Tree::of(record).filter(|_| standing == Standing::Kept);
    changes.own = Some(OwnBranch {
        repo: recorded.repo.clone(),
        branch: recorded.branch.clone(),
        standing: standing.word().to_owned(),
        acts: tree.is_some() && !record.running(),
    });
    match tree {
        Some(tree) => changes.places.push(changed_in(
            held,
            (&tree.workspace, &tree.repo, Some(&tree.piece)),
            |change| {
                Some(TaskFile {
                    path: change.path,
                    mark: change.mark,
                    from: change.from,
                    uncommitted: change.uncommitted,
                    also: Vec::new(),
                })
            },
        )),
        None => {
            changes.unknown = Some(
                match standing {
                    Standing::Merged | Standing::MergedBranchKept => {
                        "Its branch was merged into the branch it was cut from, and its folder \
                         is gone: what it changed is in that branch now."
                    }
                    Standing::Discarded => {
                        "Its branch's folder was discarded, so there is no folder to compare. A \
                         branch that held a commit is still in the repo."
                    }
                    Standing::Kept | Standing::Gone => {
                        "Its branch's folder is gone, so there is no folder to compare. The \
                         branch may still be in the repo."
                    }
                }
                .to_owned(),
            );
        }
    }
}

/// What a task that worked in a folder other chats work in changed: the files its own edit
/// tools wrote that git still finds uncommitted there, and nothing else of that folder's.
fn of_a_shared_folder(held: &Held, record: &Record, changes: &mut TaskChanges) {
    let Some(chat) = record.worker.chat.id.as_deref() else {
        changes.unknown = Some(NOT_KEPT.to_owned());
        return;
    };
    let Some(kept) = held.touched_files().of(chat) else {
        changes.unknown = Some(NOT_KEPT.to_owned());
        return;
    };
    changes.more = kept.more;
    // By place, each with the paths named inside it and the project's path for each.
    let mut by_place: BTreeMap<(String, String, Option<String>), BTreeMap<String, String>> =
        BTreeMap::new();
    for path in &kept.paths {
        match core::placed(path) {
            Some(at) => {
                by_place
                    .entry((at.workspace, at.repo, at.piece))
                    .or_default()
                    .insert(at.path, path.clone());
            }
            None => changes.elsewhere.push(path.clone()),
        }
    }
    for ((workspace, repo, piece), named) in by_place {
        let place = changed_in(held, (&workspace, &repo, piece.as_deref()), |change| {
            let whole = named.get(&change.path)?;
            Some(TaskFile {
                also: held.touched_files().also(chat, whole),
                path: change.path,
                mark: change.mark,
                from: change.from,
                uncommitted: change.uncommitted,
            })
        });
        // Listed in a place is what this task wrote there. `more` stays git's count past its
        // cap: a file of this task's past it is not listed, and the tab says so.
        changes.places.push(place);
    }
}

/// What the task of dispatch `id` changed, read now.
pub(crate) fn changes_of(held: &Held, id: &str) -> Result<TaskChanges, String> {
    let record = record_of(held, id)?;
    let mut changes = TaskChanges {
        id: record.id.clone(),
        task: name_of(&record),
        running: record.running(),
        own: None,
        places: Vec::new(),
        elsewhere: Vec::new(),
        more: false,
        said: record
            .report
            .as_ref()
            .and_then(|report| report.changed.said.clone()),
        unknown: None,
    };
    if record.place.worktree.is_some() {
        of_its_own_branch(held, &record, &mut changes);
    } else {
        of_a_shared_folder(held, &record, &mut changes);
    }
    Ok(changes)
}

/// What the task of dispatch `id` changed, and no other task's (#1511): for a task on a
/// branch of its own, everything that branch holds against the branch it was cut from; for a
/// task that worked in a folder other chats work in, the files its own edit tools wrote that
/// git still finds uncommitted there, each marked where another chat's edit tools wrote it too:
/// not a shell command's edits, and not what the task committed there. Where purlis cannot
/// say which files were the task's, it says so and lists none.
#[tauri::command]
#[specta::specta]
pub(crate) async fn task_changes(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    id: String,
) -> Result<TaskChanges, String> {
    let held = planes.held(&plane)?;
    crate::off_the_window("reading what the task changed", move || {
        changes_of(&held, &id)
    })
    .await
}

// ---------------------------------------------------------------------------------------
// Merge
// ---------------------------------------------------------------------------------------

/// What merging a task's own branch would land, as the window shows it before it asks, and as
/// the window hands it back with the answer: **what is merged is what the person was shown,
/// or nothing is.**
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
pub(crate) struct BranchMerge {
    /// The task, by the name its row has.
    pub task: String,
    /// The repo the branch is in.
    pub repo: String,
    /// The branch purlis cut for the task.
    pub branch: String,
    /// The branch it was cut from, which is where it lands; `null` where purlis has no record
    /// of one, and the merge is then refused with that reason.
    pub into: Option<String>,
    /// The commit the task's branch is at.
    pub tip: String,
    /// How many commits it would land.
    pub ahead: u32,
    /// How many commits the branch it was cut from has gained since. Above 0 the merge is
    /// refused: purlis fast-forwards or does nothing.
    pub behind: u32,
    /// The uncommitted paths in the task's folder. Any, and the merge is refused.
    pub uncommitted: Vec<String>,
}

/// What a merge did.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub(crate) struct TaskMerged {
    pub branch: String,
    /// Whether the task's folder was taken away now that its branch is merged: it held
    /// nothing else. One holding an ignored file of the task's stays, with Discard.
    pub folder_removed: bool,
}

/// Why a task's branch is not merged while the task runs or a chat stands in its folder.
fn still_open(task: &str) -> String {
    format!(
        "'{task}' is still running, or a chat is still open in its branch's folder. purlis \
         merges a task's branch once the task has ended and no chat works in that folder."
    )
}

/// What a merge is told of a branch whose folder is no longer there.
const NOTHING_TO_MERGE: &str =
    "That branch's folder is gone, so purlis has nothing to merge from. Nothing was merged.";

/// The dispatch `id` and its worktree, where it has one that is still kept, the task has
/// ended and no chat stands in the way.
fn mergeable(held: &Held, id: &str) -> Result<(Record, Tree), String> {
    let record = record_of(held, id)?;
    if dispatchplace::standing(held.root(), &record) != Some(Standing::Kept) {
        return Err(NOTHING_TO_MERGE.to_owned());
    }
    let tree = Tree::of(&record).ok_or_else(|| NOTHING_TO_MERGE.to_owned())?;
    if crate::dispatches::chat_in_the_way(held, &record, &tree) {
        return Err(still_open(&name_of(&record)));
    }
    Ok((record, tree))
}

/// What merging the branch of dispatch `id` would land, read now and nothing changed.
pub(crate) fn merge_asked(held: &Held, id: &str) -> Result<BranchMerge, String> {
    let (record, tree) = mergeable(held, id)?;
    let branch = tree.branch.clone().unwrap_or_else(|| tree.piece.clone());
    let read = dispatchplace::merge_asked(held.root(), &tree, &crate::gitbroker::isolation())
        .map_err(|not| not.in_window(&tree.repo, &branch))?;
    let apart = crate::piecefiles::ahead_behind_of(
        held.root(),
        Branch::piece(&tree.workspace, &tree.repo, &tree.piece),
    )?;
    Ok(BranchMerge {
        task: name_of(&record),
        repo: tree.repo.clone(),
        branch: read.branch,
        into: apart.base,
        tip: read.tip,
        ahead: apart.ahead,
        behind: apart.behind,
        uncommitted: read.uncommitted,
    })
}

/// **Merges the branch of dispatch `id` into the branch it was cut from**, where that is
/// exactly what `seen` says the person was shown. A fast-forward or nothing: a branch that
/// does not apply cleanly is refused with the reason, and nothing is changed.
///
/// Read again at this moment and compared: a commit made, or the branch it was cut from moved,
/// since the question was asked is something the person did not agree to, so nothing is merged
/// and they are asked again. Once merged, its folder is looked at as a closed chat's is, and
/// goes where it holds nothing else.
pub(crate) fn merge(held: &Held, id: &str, seen: &BranchMerge) -> Result<TaskMerged, String> {
    let now = merge_asked(held, id)?;
    if now != *seen {
        return Err(
            "What that branch would land has changed since you were asked, so nothing was \
             merged. Press Merge again to see what would land now."
                .to_owned(),
        );
    }
    // Asked once more, last: a chat started there while git was read stands in the way too.
    let (record, tree) = mergeable(held, id)?;
    let isolation = crate::gitbroker::isolation();
    let merged = dispatchplace::merge(held.root(), &tree, &seen.tip, &isolation)
        .map_err(|not| not.in_window(&tree.repo, &seen.branch))?;
    tracing::info!(
        "purlis: the person merged the branch of task '{}' in {}",
        purlis_core::shown::short(&seen.task),
        tree.repo
    );
    let tidied = dispatchplace::tidy_recorded(held.root(), &record, &isolation);
    Ok(TaskMerged {
        branch: merged.branch,
        folder_removed: tidied != Tidied::Kept,
    })
}

/// What merging the own branch of the task of dispatch `id` would land (#1511): the branch,
/// the branch it was cut from, its commit, how many commits it is ahead and behind, and what
/// is not committed in its folder, for the question the window asks before it merges. Refused
/// while the task still runs or a chat is open in the folder.
#[tauri::command]
#[specta::specta]
pub(crate) async fn task_branch_merge_question(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    id: String,
) -> Result<BranchMerge, String> {
    let held = planes.held(&plane)?;
    crate::off_the_window("reading what the branch would land", move || {
        merge_asked(&held, &id)
    })
    .await
}

/// **Merge** on a finished task's own branch (#1511): lands it in the branch it was cut from,
/// as a fast-forward or not at all. `seen` is what the window showed the person would land,
/// as `task_branch_merge_question` answered it: where the branch holds anything else by now,
/// nothing is merged. A merge that does not apply cleanly changes nothing and says why. The
/// person's own act: this is a command of the window alone, served on no link.
#[tauri::command]
#[specta::specta]
pub(crate) async fn task_branch_merge(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    id: String,
    seen: BranchMerge,
) -> Result<TaskMerged, String> {
    let held = planes.held(&plane)?;
    crate::off_the_window("merging the task's branch", move || {
        merge(&held, &id, &seen)
    })
    .await
}

// ---------------------------------------------------------------------------------------
// Two tasks in one folder
// ---------------------------------------------------------------------------------------

/// One open task, as the same-folder question reads it: the chat that asked for it, and it.
struct OpenTask {
    asker: u32,
    working: Working,
}

/// Every open task of the project, of whichever chat asked for it, in the order they started.
/// `but` leaves one out.
fn open_tasks(held: &Held, but: Option<u32>) -> Vec<OpenTask> {
    let mut open: Vec<_> = held
        .chats()
        .open_now()
        .into_iter()
        .filter(|chat| Some(chat.session) != but)
        .filter_map(|chat| {
            let from = chat.from.as_ref().filter(|from| from.mode == Mode::Task)?;
            Some((from.chat, chat))
        })
        .collect();
    // A chat's number is dealt as it starts, so the numbers are the order they started in.
    open.sort_by_key(|(_, chat)| chat.session);
    open.into_iter()
        .filter_map(|(asker, chat)| {
            Some(OpenTask {
                asker,
                working: Working {
                    name: held
                        .chats()
                        .shown_name(chat.session)
                        .unwrap_or_else(|| chat.name.clone()),
                    cwd: chat.cwd?,
                },
            })
        })
        .collect()
}

/// What `tasks` are, as the core's same-folder rules read them.
fn working(tasks: Vec<OpenTask>) -> Vec<Working> {
    tasks.into_iter().map(|task| task.working).collect()
}

/// **What is said beside the start of task `new`, where another open task already works in its
/// folder** (V100-68): both by name, whichever chat asked for the other (#1534). To the asking chat, with the word that gives a task a branch of its own;
/// to the person, who chose the place in the window, in the window's words. Nothing where it
/// works alone: a task given a branch of its own stands in a folder cut for it.
pub(crate) fn shares_a_folder(held: &Held, new: u32, by_person: bool) -> Option<String> {
    let at = held.chats().chat_at(new)?;
    let cwd = at.cwd?;
    let others = core::sharing(&cwd, &working(open_tasks(held, Some(new))));
    if others.is_empty() {
        return None;
    }
    let name = held.chats().shown_name(new)?;
    let folder = crate::dispatches::folder(held.root(), &cwd);
    Some(if by_person {
        let mut all = others;
        all.push(name);
        core::said_in_window(&all, &folder)
            .trim_end_matches('.')
            .to_owned()
    } else {
        core::told_the_asker(&name, &others, &folder)
    })
}

/// A folder two or more open tasks work in at once, one of them a task of the chat whose tab
/// says it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub(crate) struct SharedFolder {
    /// The folder, relative to the project.
    pub folder: String,
    /// The tasks working in it, by name, in the order they started.
    pub tasks: Vec<String>,
    /// The sentence the tab says.
    pub says: String,
}

/// The folders where an open task of chat `asker` works beside another open task, of that chat
/// or of any other (#1534).
pub(crate) fn shared_by(held: &Held, asker: u32) -> Vec<SharedFolder> {
    let tasks = open_tasks(held, None);
    let real = |path: &std::path::Path| std::fs::canonicalize(path).unwrap_or(path.to_path_buf());
    let mine: Vec<_> = tasks
        .iter()
        .filter(|task| task.asker == asker)
        .map(|task| real(&task.working.cwd))
        .collect();
    core::shared(&working(tasks))
        .into_iter()
        .filter(|shared| mine.contains(&real(&shared.folder)))
        .map(|shared| {
            let folder = crate::dispatches::folder(held.root(), &shared.folder);
            SharedFolder {
                says: core::said_in_window(&shared.tasks, &folder),
                folder,
                tasks: shared.tasks,
            }
        })
        .collect()
}

/// The folders where an open task of chat `session` works with no branch of its own beside
/// another open task, of that chat or of any other (#1511, #1534, V100-68): what its tab warns
/// about, by every task's name. There are no file locks between tasks, so a file two of them
/// change cannot be told apart afterwards.
#[tauri::command]
#[specta::specta]
pub(crate) fn tasks_sharing_a_folder(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    session: u32,
) -> Result<Vec<SharedFolder>, String> {
    let held = planes.held(&plane)?;
    Ok(shared_by(&held, session))
}

/// What the dialog asking for a task from chat `session`'s tab says before it starts one at
/// `place`, where open tasks already work there with no branch of their own; `None` where none
/// does. `place` is the dialog's word: `None` for that chat's folder, `workspace:<name>` for
/// another workspace's. A task given a branch of its own works in a folder purlis cuts for it
/// alone, so nothing is said of `worktree`.
pub(crate) fn sharing_before_asking(
    held: &Held,
    session: u32,
    place: Option<&str>,
) -> Option<String> {
    let folder = match dispatchplace::asked(place).ok()? {
        None => held.chats().chat_at(session)?.cwd?,
        Some(dispatchplace::Where::Workspace(name)) => {
            dispatchplace::workspace_folder(held.root(), &name).ok()?.1
        }
        Some(dispatchplace::Where::Worktree) => return None,
    };
    let others = core::sharing(&folder, &working(open_tasks(held, None)));
    (!others.is_empty()).then(|| {
        core::said_before_asking(&others, &crate::dispatches::folder(held.root(), &folder))
    })
}

/// What **Ask a persona…** from chat `session`'s tab says before it starts a task at `place`
/// (#1534): the open tasks already working in that folder with no branch of their own, by
/// name, or `null` where none is. `place` is the dialog's word for where it works: `null` for
/// that chat's folder, `workspace:<name>` for another workspace, `worktree` for a branch of its
/// own, of which nothing is said. It starts nothing and changes nothing.
#[tauri::command]
#[specta::specta]
pub(crate) fn task_folder_shared(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    session: u32,
    place: Option<String>,
) -> Result<Option<String>, String> {
    let held = planes.held(&plane)?;
    Ok(sharing_before_asking(&held, session, place.as_deref()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_files_a_task_s_tools_named_are_kept_by_its_chat_and_no_other_s() {
        let touched = Touched::default();
        touched.note("talk", "talk", "workspaces/alpha/api/a.rs".to_owned());
        touched.note("review", "review", "workspaces/alpha/api/b.rs".to_owned());

        let talk = touched.of("talk").expect("heard from");

        assert_eq!(
            talk.paths.into_iter().collect::<Vec<_>>(),
            ["workspaces/alpha/api/a.rs"]
        );
        assert_eq!(touched.of("nobody"), None);
    }

    #[test]
    fn the_cannot_tell_sentence_names_the_number_of_tasks_kept() {
        assert!(
            NOT_KEPT.contains(&format!("more than {} chats", core::CHATS_KEPT)),
            "{NOT_KEPT}"
        );
    }

    #[test]
    fn a_merge_and_a_discard_are_commands_no_link_serves() {
        // The person's acts on a task's branch (#1511): the window invokes them over Tauri's
        // IPC, and the UI RPC a host serves on a link never carries either.
        for command in ["task_branch_merge", "dispatch_worktree_discard"] {
            assert!(
                purlis_session_protocol::ui::WINDOW_ONLY.contains(&command),
                "{command} would be served on a link"
            );
        }
    }
}
