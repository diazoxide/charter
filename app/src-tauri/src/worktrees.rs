//! The worktree verbs, as the window can reach them.
//!
//! Thin by design: every decision — what may be removed, what may be merged, which paths are
//! this workspace's — lives in `purlis_core::worktree`, and this layer converts. A rule
//! implemented here as well would be a second rule, and the two would drift.
//!
//! **A refusal crosses in the window's words, which the core chose.** The core's refusals are
//! sentences, and each has two: `charter worktree`'s, which names the command-line repair
//! (`--force`, `git -C <clone> …`), and the window's ([`worktree::Refusal::in_window`],
//! [`purlis_core::pieces::NotDeclared::in_window`]), which says it of a branch and its folder
//! (ADR 0072 §4, #989) and leaves the repair to the window's own rows. Nothing here rewords
//! either: no "failed to remove worktree", no error code the UI would then have to translate
//! back into English.

use std::path::{Path, PathBuf};

use purlis_core::worktree::Note;
use purlis_core::{chatpiece, worktree};

use crate::planes::{PlaneId, Planes};

/// The piece a chat's directory sits in, against a root the registry has already vouched for.
///
/// Split out from the command for the reason every other verb in this file is: the tests
/// below drive a plane they made, and a `tauri::State<'_, Planes>` is not something a test
/// can hold.
fn piece_of_chat(plane: &Path, cwd: &Path) -> Result<Option<ChatWorktree>, String> {
    let Some(found) = worktree::locate(plane, cwd) else {
        return Ok(None);
    };
    let pieces = worktree::list(plane, &found.workspace, &found.repo)
        .map_err(|refusal| refusal.in_window())?;
    let Some(row) = pieces.into_iter().find(|p| p.piece == found.piece) else {
        // git no longer has a registration for it, though the directory is where a piece
        // goes. Reported as itself rather than as nothing: the row is not a chat working
        // outside every worktree.
        return Ok(Some(ChatWorktree {
            workspace: found.workspace,
            repo: found.repo,
            piece: found.piece,
            branch: None,
            wired: false,
            stale: true,
        }));
    };
    Ok(Some(ChatWorktree {
        workspace: found.workspace,
        repo: found.repo,
        piece: found.piece,
        branch: row.branch,
        wired: row.wired,
        stale: row.prunable.is_some(),
    }))
}

/// One piece, as the window shows it.
#[derive(Debug, serde::Serialize, specta::Type)]
pub struct Piece {
    pub piece: String,
    pub path: String,
    pub branch: Option<String>,
    /// Whether charter's harness layer is in this tree.
    ///
    /// Since M1.x a worktree charter cuts is wired as it is cut, so `false` now means a tree
    /// cut by plain git, one whose wire did not land, or a plane with no layer to carry. The
    /// row still says so, because a chat in such a tree runs without the plane's ask/deny
    /// rules, without its persona's agents and without `$CHARTER_HARNESS` — and starting one
    /// there is what writes the layer or refuses.
    pub wired: bool,
    /// Set when git still has a registration whose directory is gone.
    pub stale: bool,
    /// What the piece has said: `done`, `abandoned: <reason>`, `silent <age>` for a piece
    /// charter cut that has declared nothing, or empty (charter#368). An age, never a verdict.
    pub said: String,
}

/// Where a chat is working, when it is working in a piece.
#[derive(Debug, serde::Serialize, specta::Type)]
pub struct ChatWorktree {
    pub workspace: String,
    pub repo: String,
    pub piece: String,
    pub branch: Option<String>,
    pub wired: bool,
    pub stale: bool,
}

/// The piece a chat's working directory sits in, or `None`.
///
/// Called for every chat the sidebar draws. `worktree::locate` is path arithmetic and spawns
/// nothing; the one git call is the listing, made once per repo and only for chats that are
/// in a piece at all.
// **The fourth command charter-app#127 is about, and the one its title does not name.** The
// other three took the plane as a `String`; this one took a chat's `cwd` and ran
// `plane::resolve` on it — a walk UP from a path the window chose, landing on whatever plane
// that walk happened to reach, which is the same defect arrived at from below. #125 closed the
// identical instance in `workspace_panels`/`workspace_repos`, where the path came from
// `current_dir()` instead of from an argument.
//
// The `cwd` stays, because it is not the plane: it is where a chat is working, and which piece
// that is is arithmetic INSIDE the plane the registry vouched for. A cwd that leaves that plane
// now answers `None` — `worktree::locate` is relative to the root it is given — where before it
// answered about a different plane's worktrees.
#[tauri::command]
#[specta::specta]
pub fn worktree_of_chat(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    cwd: String,
) -> Result<Option<ChatWorktree>, String> {
    piece_of_chat(planes.held(&plane)?.root(), Path::new(&cwd))
}

/// This workspace's pieces for one repo.
// **It names its plane, and the registry vouches for it** (charter-app#127). It used to take
// the root as a `String` and hand it straight to the core, so whatever could reach the command
// chose which directory git ran in. A `PlaneId` has no constructor outside `planes.rs` — a
// caller hands one back, it never spells one — and `held` refuses one this window never
// opened. Not a doc comment, because the generated bindings carry those and this is about the
// Rust.
#[tauri::command]
#[specta::specta]
pub fn worktree_list(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    workspace: String,
    repo: String,
) -> Result<Vec<Piece>, String> {
    pieces_of(planes.held(&plane)?.root(), &workspace, &repo)
}

/// The listing itself, against a root the registry has already vouched for.
fn pieces_of(plane: &Path, workspace: &str, repo: &str) -> Result<Vec<Piece>, String> {
    let now = chrono::Utc::now();
    worktree::list(plane, workspace, repo)
        .map(|pieces| {
            pieces
                .into_iter()
                .map(|p| Piece {
                    said: purlis_core::pieces::said(plane, workspace, repo, &p.piece, now),
                    piece: p.piece,
                    path: p.path.display().to_string(),
                    branch: p.branch,
                    wired: p.wired,
                    stale: p.prunable.is_some(),
                })
                .collect()
        })
        .map_err(|refusal| refusal.in_window())
}

/// Remove a piece. The refusal is the core's sentence for the window.
///
/// `force` is the operator saying to discard work the guards found — it is never passed on
/// their behalf, and the window asks for it only after showing them what the refusal said.
// Its plane is a `PlaneId` the registry vouches for, like every other command's
// (charter-app#127); see `worktree_list` above. Not a doc comment, for the reason given there.
#[tauri::command]
#[specta::specta]
pub fn worktree_remove(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    workspace: String,
    repo: String,
    piece: String,
    force: bool,
) -> Result<(), String> {
    remove_piece(
        planes.held(&plane)?.root(),
        &workspace,
        &repo,
        &piece,
        force,
    )
}

/// **The explorer's own Merge and Remove never act on a branch folder purlis cut for a task**
/// (#1534). A task's folder is merged or discarded from its Changes tab, where the guards a
/// task's folder needs hold (`purlis_core::dispatchplace::merge` and `discard`): the commit the
/// person was shown, the task ended and no chat standing in the folder, the brokered route, and
/// only the branch purlis cut. These two have none of them, so a folder a dispatch record
/// names is refused here with where to go instead, `force` or not.
///
/// Two Removes are let through. One whose folder is already gone: it only takes git's stale
/// registration, and nothing is in it to lose. And one of an ended task in a repo the brokered
/// route runs no git in (its own git settings name a program): Discard is refused there, and
/// its sentence sends the person to this row, which is theirs to use on their own repo.
fn not_a_task_s(
    plane: &Path,
    workspace: &str,
    repo: &str,
    piece: &str,
    merging: bool,
) -> Result<(), String> {
    let Some(record) = purlis_core::dispatchplace::task_in_folder(plane, workspace, repo, piece)
    else {
        return Ok(());
    };
    let there = worktree::path_for(plane, workspace, repo, piece)
        .is_ok_and(|folder| folder.symlink_metadata().is_ok());
    if !merging && !there {
        return Ok(());
    }
    if !merging && !record.running() && no_discard_there(plane, workspace, repo) {
        return Ok(());
    }
    let task = record.task.unwrap_or(record.worker.chat.name);
    Err(purlis_core::dispatchplace::left_to_its_task(
        &task, piece, merging,
    ))
}

/// Whether the brokered route refuses to run git in `repo`'s clone, so a task's Discard there
/// is refused too (`purlis_core::gitbroker::runs_a_program`).
fn no_discard_there(plane: &Path, workspace: &str, repo: &str) -> bool {
    let clone = plane.join("workspaces").join(workspace).join(repo);
    purlis_core::worktree::git::isolated(&crate::gitbroker::isolation(), || {
        purlis_core::gitbroker::runs_a_program(&clone).is_err()
    })
}

/// The removal itself, against a root the registry has already vouched for.
fn remove_piece(
    plane: &Path,
    workspace: &str,
    repo: &str,
    piece: &str,
    force: bool,
) -> Result<(), String> {
    not_a_task_s(plane, workspace, repo, piece, false)?;
    worktree::remove(plane, workspace, repo, piece, force, false)
        .map(|_| ())
        .map_err(|refusal| refusal.in_window())
}

/// Declare a piece done, from its row (charter#368).
///
/// The operator speaking for the piece, which is theirs to call: the worker's own `charter
/// worktree done` writes the same line from inside it. Recorded with no session or persona —
/// the window is neither — and this machine's name, so the listing's claimant reads as the
/// host. Refused, in the core's words, for a piece git no longer has.
// Its plane is a `PlaneId` the registry vouches for, like every other command's
// (charter-app#127); see `worktree_list` above. Not a doc comment, for the reason given there.
#[tauri::command]
#[specta::specta]
pub fn worktree_done(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    workspace: String,
    repo: String,
    piece: String,
) -> Result<(), String> {
    declare_done(
        planes.held(&plane)?.root(),
        planes.config(),
        &workspace,
        &repo,
        &piece,
    )
}

/// The declaration itself, against a root the registry has already vouched for. `config` is
/// the machine store the device id is read from (FD-25).
fn declare_done(
    plane: &Path,
    config: Option<&Path>,
    workspace: &str,
    repo: &str,
    piece: &str,
) -> Result<(), String> {
    let who = window(config);
    purlis_core::pieces::declare(
        plane,
        workspace,
        repo,
        piece,
        purlis_core::pieces::Declaration::Done,
        &who,
        chrono::Utc::now(),
    )
    .map(|_| ())
    .map_err(|why| why.in_window())
}

/// The window speaking for a piece: no session or persona, this machine's name as the label,
/// and the log named by the device id the store at `config` keeps (FD-25).
fn window(config: Option<&Path>) -> purlis_core::pieces::Who {
    let host = purlis_core::dispatch::host();
    purlis_core::pieces::Who {
        session: None,
        persona: None,
        log: purlis_core::dispatch::log_name(config, &host),
        host,
    }
}

/// Said when a branch was cut but the piece log could not be written.
const UNLOGGED: &str = "purlis could not record that it cut this branch, so it will not say \
                        how long the branch has been quiet.";

/// Log a branch the window cut as `claimed`, and say what there is to say about it in the
/// window's words: what the cut found (ADR 0072 §3), and a log that could not be written.
fn claimed(plane: &Path, config: Option<&Path>, cut: &chatpiece::Cut) -> Vec<String> {
    let mut said: Vec<String> = cut.notes.iter().map(Note::in_window).collect();
    if chatpiece::claim(plane, cut, &window(config), chrono::Utc::now()).is_none() {
        said.push(UNLOGGED.to_string());
    }
    said
}

/// The line a chat started on its own branch says first: which branch, in which repo, and what
/// it was cut from (ADR 0072 §4's *`chat-1` in api*). The first-run chat, which no picker
/// asked about, is told this way too.
fn on_branch(cut: &chatpiece::Cut) -> String {
    let from = match &cut.base {
        worktree::Base::Branch(base) => base.clone(),
        worktree::Base::Detached(sha) => format!("commit {}", &sha[..sha.len().min(12)]),
    };
    format!(
        "On branch {} in {}, a branch of its own cut from {from}.",
        cut.branch, cut.repo
    )
}

/// A branch the window cut: the piece it is, and what git calls it (ADR 0072 §4).
#[derive(Debug, serde::Serialize, specta::Type)]
pub struct NewBranch {
    pub piece: String,
    pub path: String,
    pub branch: String,
    /// What the cut found to say — a dirty clone whose changes stayed behind, a layer that did
    /// not land — in the window's words.
    pub warnings: Vec<String>,
}

/// **New branch**, from a repo's row (GL-1): cut a piece off the clone's HEAD and log it
/// `claimed`. It starts nothing. `branch` is the name the operator typed, used exactly or
/// refused; `None` is charter's next free `chat-<n>`.
///
/// Off the main thread: a checkout can take seconds on a large repo, and the window must not
/// freeze for it (GL-1 review S3).
// Its plane is a `PlaneId` the registry vouches for, like every other command's
// (charter-app#127); see `worktree_list` above. Not a doc comment, for the reason given there.
#[tauri::command]
#[specta::specta]
pub async fn worktree_add(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    workspace: String,
    repo: String,
    branch: Option<String>,
) -> Result<NewBranch, String> {
    let root = planes.held(&plane)?.root().to_path_buf();
    let config = planes.config().map(Path::to_path_buf);
    tauri::async_runtime::spawn_blocking(move || {
        cut_branch(
            &root,
            config.as_deref(),
            &workspace,
            &repo,
            branch.as_deref(),
        )
    })
    .await
    .map_err(|err| format!("purlis could not cut the branch: {err}"))?
}

/// The cut itself, against a root the registry has already vouched for.
fn cut_branch(
    plane: &Path,
    config: Option<&Path>,
    workspace: &str,
    repo: &str,
    branch: Option<&str>,
) -> Result<NewBranch, String> {
    let naming = match branch {
        Some(typed) => chatpiece::Naming::Exactly(typed.to_string()),
        None => chatpiece::Naming::After(None),
    };
    let cut = chatpiece::cut(plane, workspace, repo, &naming).map_err(|why| why.in_window())?;
    let warnings = claimed(plane, config, &cut);
    Ok(NewBranch {
        piece: cut.piece,
        path: cut.path.display().to_string(),
        branch: cut.branch,
        warnings,
    })
}

/// **A writing chat starts on a branch of its own** (GL-1, ADR 0072 §4).
///
/// When `cwd` is a repo's clone and `new_branch` is set — the picker's default — a piece is cut
/// off the clone's HEAD, named after the chat's `label` or `chat-<n>`, and `start` is handed
/// that piece's directory instead. Anywhere else, or with `new_branch` cleared, `start` is
/// handed `cwd` as it came and nothing is cut.
///
/// Answers what `start` answered and the lines the chat's pane should say: which branch it is
/// on, then what the cut found. **A start that is refused or fails takes its branch back**
/// (`chatpiece::Held`). Nothing has written to it, so git's safe removal takes the folder and
/// the branch, and the refusal the operator reads is the start's own sentence, followed by what
/// could not be taken back if anything. A branch is logged `claimed` only once its chat has
/// started. A crash between the cut and the start is not covered: the release build aborts on
/// a panic, so no cleanup runs and the branch and its folder stay (see `chatpiece::Held`).
pub fn on_a_branch<T>(
    plane: &Path,
    config: Option<&Path>,
    cwd: Option<&Path>,
    label: Option<&str>,
    new_branch: bool,
    start: impl FnOnce(Option<PathBuf>) -> Result<T, String>,
) -> Result<(T, Vec<String>), String> {
    on_a_branch_cut(plane, config, cwd, label, new_branch, |cwd, _| start(cwd))
}

/// [`on_a_branch`], with `start` also handed the branch it cut, when it cut one: what a caller
/// needs to say what the branch was cut from (the first task's diff, FR-28).
pub fn on_a_branch_cut<T>(
    plane: &Path,
    config: Option<&Path>,
    cwd: Option<&Path>,
    label: Option<&str>,
    new_branch: bool,
    start: impl FnOnce(Option<PathBuf>, Option<&chatpiece::Cut>) -> Result<T, String>,
) -> Result<(T, Vec<String>), String> {
    let clone = cwd
        .filter(|_| new_branch)
        .and_then(|at| chatpiece::clone_at(plane, at));
    let Some((workspace, repo)) = clone else {
        return start(cwd.map(Path::to_path_buf), None).map(|started| (started, Vec::new()));
    };
    let naming = chatpiece::Naming::After(label.map(str::to_string));
    let cut = chatpiece::cut(plane, &workspace, &repo, &naming).map_err(|why| why.in_window())?;
    let held = chatpiece::Held::new(plane, cut);
    match start(Some(held.cut().path.clone()), Some(held.cut())) {
        Ok(started) => {
            let cut = held.keep();
            let mut said = vec![on_branch(&cut)];
            said.extend(claimed(plane, config, &cut));
            Ok((started, said))
        }
        Err(refused) => match held.take_back() {
            (_, Ok(chatpiece::Undone::Gone)) => Err(refused),
            (cut, Ok(chatpiece::Undone::BranchKept)) => Err(format!(
                "{refused}\nIts folder was taken back, and git kept the branch {} in {}.",
                cut.branch, cut.repo
            )),
            (cut, Err(kept)) => Err(format!(
                "{refused}\nThe branch {} cut for it in {} could not be taken back: {}",
                cut.branch,
                cut.repo,
                kept.in_window()
            )),
        },
    }
}

/// What a merge did, for the window to report.
#[derive(Debug, serde::Serialize, specta::Type)]
pub struct Merged {
    pub branch: String,
    pub was: String,
    pub now: String,
}

/// Land a piece in its clone, fast-forward only. Never pushes.
// Its plane is a `PlaneId` the registry vouches for, like every other command's
// (charter-app#127); see `worktree_list` above. Not a doc comment, for the reason given there.
#[tauri::command]
#[specta::specta]
pub fn worktree_merge(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    workspace: String,
    repo: String,
    piece: String,
) -> Result<Merged, String> {
    merge_piece(planes.held(&plane)?.root(), &workspace, &repo, &piece)
}

/// The merge itself, against a root the registry has already vouched for.
fn merge_piece(plane: &Path, workspace: &str, repo: &str, piece: &str) -> Result<Merged, String> {
    not_a_task_s(plane, workspace, repo, piece, true)?;
    worktree::merge(plane, workspace, repo, piece)
        .map(|m| Merged {
            branch: m.branch,
            was: m.was,
            now: m.now,
        })
        .map_err(|refusal| refusal.in_window())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A plane with a clone and one piece in it.
    fn plane() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        std::fs::write(root.join("charter.toml"), "schema = 1\n").unwrap();
        let clone = root.join("workspaces/alpha/thing");
        std::fs::create_dir_all(&clone).unwrap();
        // Through `git`, which checks the exit status and never signs: a fixture whose setup
        // failed must not pass for a plane, and the developer's signer must not be asked.
        git(&clone, &["init", "-q", "-b", "main", "."]);
        git(&clone, &["config", "user.email", "t@e.invalid"]);
        git(&clone, &["config", "user.name", "t"]);
        git(&clone, &["config", "commit.gpgsign", "false"]);
        std::fs::write(clone.join("README.md"), "one\n").unwrap();
        git(&clone, &["add", "-A"]);
        git(&clone, &["commit", "-q", "-m", "one"]);
        (dir, root, clone)
    }

    #[test]
    fn a_chat_outside_every_worktree_has_none() {
        let (_dir, root, clone) = plane();

        let seen = piece_of_chat(&root, &clone).unwrap();

        assert!(seen.is_none(), "the shared clone is not a piece");
    }

    #[test]
    fn a_chat_in_a_piece_reports_its_branch_and_that_the_layer_is_there() {
        let (_dir, root, _clone) = plane();
        // A plane with something to carry. Without it `want` is empty, charter writes
        // nothing, and this would assert `wired` against a plane that has no layer at all —
        // a test that passes whatever the wire does.
        std::fs::create_dir_all(root.join(".claude")).unwrap();
        std::fs::write(
            root.join(".claude/settings.json"),
            "{\"env\": {\"CHARTER_HARNESS\": \"claude-code\"}}\n",
        )
        .unwrap();
        let added = worktree::add(&root, "alpha", "thing", "piece", None).unwrap();

        let seen = piece_of_chat(&root, &added.path)
            .unwrap()
            .expect("a chat in a piece has one");

        assert_eq!(seen.piece, "piece");
        assert_eq!(seen.branch.as_deref(), Some("piece"));
        assert!(
            seen.wired,
            "a worktree charter cut carries the plane's layer (M1.x, closing ADR 0027's gap)"
        );
        assert!(!seen.stale);
    }

    #[test]
    fn a_piece_of_another_plane_is_no_piece_of_this_one() {
        // **charter-app#127, in the shape that caused it.** The command used to take the
        // chat's `cwd` alone and walk UP from it (`plane::resolve`), so the plane it answered
        // about was whatever that walk landed on — never the project whose sidebar was
        // asking. With project tabs (#125) the window holds several planes at once, so a chat
        // working in one of them would have been reported as a piece of another's repo.
        //
        // Now the plane is the one the registry vouched for and the `cwd` is only arithmetic
        // inside it, so the answer is `None`: this plane has no such piece.
        let (_dir, root, _clone) = plane();
        let (_other_dir, other, _other_clone) = plane();
        let added = worktree::add(&other, "alpha", "thing", "piece", None).unwrap();

        assert!(
            piece_of_chat(&other, &added.path).unwrap().is_some(),
            "the plane it belongs to does have it — without this the next line passes \
             against a command that answers None for everything"
        );
        assert!(
            piece_of_chat(&root, &added.path).unwrap().is_none(),
            "a chat in another project's worktree was reported as a piece of this one"
        );
    }

    #[test]
    fn a_worktree_cut_by_plain_git_still_reads_unwired() {
        // The label is derived from the tree, not from what charter remembers doing, so it is
        // still the honest answer for a tree charter did not wire.
        let (_dir, root, clone) = plane();
        let by_hand = root.join("workspaces/alpha/.worktrees/thing/hand");
        std::fs::create_dir_all(by_hand.parent().unwrap()).unwrap();
        purlis_core::forklock::output(
            std::process::Command::new("git")
                .arg("-C")
                .arg(&clone)
                .args(["worktree", "add", "-q", "-b", "hand"])
                .arg(&by_hand),
        )
        .unwrap();

        let seen = piece_of_chat(&root, &by_hand)
            .unwrap()
            .expect("a chat in a piece has one");

        assert!(!seen.wired);
    }

    /// Whether `said` is in the window's words: of a branch and its folder, never of a
    /// worktree, and with no command-line repair in it (#989, ADR 0072 §4).
    fn in_the_windows_words(said: &str) {
        for word in ["worktree", "git -C", "--force"] {
            assert!(
                !said.to_lowercase().contains(&word.to_lowercase()),
                "the window was handed {word:?}: {said}"
            );
        }
    }

    #[test]
    fn a_removal_is_refused_in_the_windows_words_and_removes_nothing() {
        // The core's refusal, in the sentence the window has for it: `charter worktree
        // remove` keeps its own, which names `--force`, and the window's answer to the same
        // refusal is its discard row, not a flag.
        let (_dir, root, _clone) = plane();
        let added = worktree::add(&root, "alpha", "thing", "piece", None).unwrap();
        std::fs::write(added.path.join("wip.txt"), "unsaved\n").unwrap();

        let through = remove_piece(&root, "alpha", "thing", "piece", false)
            .expect_err("a dirty piece is refused");
        let core = worktree::remove(&root, "alpha", "thing", "piece", false, false)
            .expect_err("the same refusal");

        assert_eq!(through, core.in_window());
        assert!(through.contains("uncommitted"), "{through}");
        in_the_windows_words(&through);
        assert!(added.path.is_dir(), "and nothing was removed");
        in_the_windows_words(
            &remove_piece(&root, "alpha", "thing", "nope", false).expect_err("no such piece"),
        );
    }

    #[test]
    fn a_merge_is_refused_in_the_windows_words() {
        let (_dir, root, _clone) = plane();
        let added = worktree::add(&root, "alpha", "thing", "piece", None).unwrap();
        std::fs::write(added.path.join("wip.txt"), "unsaved\n").unwrap();

        let dirty = merge_piece(&root, "alpha", "thing", "piece").expect_err("a dirty piece");
        assert!(dirty.contains("uncommitted"), "{dirty}");
        in_the_windows_words(&dirty);
        let missing = merge_piece(&root, "alpha", "thing", "nope").expect_err("no such piece");
        assert!(
            missing.contains("no branch folder called 'nope'"),
            "{missing}"
        );
        in_the_windows_words(&missing);
    }

    /// A dispatch record that names branch folder `piece` of `thing` in `alpha` as its
    /// worktree, for a task called `check the queue` that is still running. Answers its id.
    fn a_task_in(root: &Path, piece: &str) -> String {
        use purlis_core::dispatchrecord::{self, Asker, ChatRef, Mode, Opening, Place, Worker};
        let chat = |n: u32, name: &str| ChatRef {
            chat: n,
            id: None,
            name: name.to_owned(),
            persona: None,
        };
        let opening = Opening {
            mode: Mode::Task,
            asker: Asker {
                chat: chat(3, "steward 3"),
                ..Default::default()
            },
            persona: None,
            worker: Worker {
                chat: chat(7, "devops 7"),
                ..Default::default()
            },
            task: Some("check the queue".to_owned()),
            place: Place {
                workspace: Some("alpha".to_owned()),
                folder: None,
                worktree: Some(dispatchrecord::Worktree {
                    repo: "thing".to_owned(),
                    piece: piece.to_owned(),
                    branch: Some(piece.to_owned()),
                    removed: None,
                }),
            },
            brief: "b".to_owned(),
            report_owed: true,
        };
        dispatchrecord::open(root, opening, chrono::Utc::now())
            .unwrap()
            .id
    }

    #[test]
    fn a_task_s_branch_folder_is_merged_and_discarded_from_its_changes_and_not_here() {
        // #1534: the explorer's own Merge and Remove have none of the guards a task's folder
        // needs, so a folder a dispatch record names is refused with where to go instead.
        let (_dir, root, clone) = plane();
        let added = worktree::add(&root, "alpha", "thing", "check", None).unwrap();
        std::fs::write(added.path.join("work.txt"), "work\n").unwrap();
        git(&added.path, &["add", "-A"]);
        git(&added.path, &["commit", "-q", "-m", "work"]);
        a_task_in(&root, "check");

        let merged = merge_piece(&root, "alpha", "thing", "check").unwrap_err();
        assert_eq!(
            merged,
            purlis_core::dispatchplace::left_to_its_task("check the queue", "check", true)
        );
        in_the_windows_words(&merged);
        for force in [false, true] {
            let removed = remove_piece(&root, "alpha", "thing", "check", force).unwrap_err();
            assert_eq!(
                removed,
                purlis_core::dispatchplace::left_to_its_task("check the queue", "check", false)
            );
            in_the_windows_words(&removed);
        }
        assert!(added.path.is_dir(), "nothing was removed");
        assert!(!clone.join("work.txt").exists(), "nothing was merged");

        // A branch folder no record names is the explorer's, as before.
        worktree::add(&root, "alpha", "thing", "mine", None).unwrap();
        remove_piece(&root, "alpha", "thing", "mine", false).expect("not a task's");
    }

    #[test]
    fn a_task_s_folder_that_is_already_gone_can_still_be_cleared_from_the_explorer() {
        let (_dir, root, _clone) = plane();
        let added = worktree::add(&root, "alpha", "thing", "check", None).unwrap();
        a_task_in(&root, "check");
        std::fs::remove_dir_all(&added.path).unwrap();

        let removed = remove_piece(&root, "alpha", "thing", "check", false);

        assert!(
            !removed
                .as_ref()
                .is_err_and(|why| why.contains("Changes tab")),
            "a stale registration is the core's to answer: {removed:?}"
        );
        assert!(
            merge_piece(&root, "alpha", "thing", "check")
                .unwrap_err()
                .contains("Changes tab")
        );
    }

    #[test]
    fn an_ended_task_s_folder_in_a_repo_purlis_runs_no_git_in_is_removed_from_here() {
        // Discard is refused where the repo's own git settings name a program, and its sentence
        // sends the person to this row: not a dead end once the task has ended.
        let (_dir, root, clone) = plane();
        let added = worktree::add(&root, "alpha", "thing", "check", None).unwrap();
        let id = a_task_in(&root, "check");
        git(&clone, &["config", "filter.lfs.clean", "cat"]);
        assert!(
            remove_piece(&root, "alpha", "thing", "check", false)
                .unwrap_err()
                .contains("Changes tab"),
            "a running task's folder stays the task's"
        );

        purlis_core::dispatchrecord::close(
            &root,
            &id,
            purlis_core::dispatchrecord::Ending {
                report: None,
                usage: None,
            },
            chrono::Utc::now(),
        )
        .unwrap();

        remove_piece(&root, "alpha", "thing", "check", false).expect("the way out Discard names");
        assert!(!added.path.exists());
        assert!(
            merge_piece(&root, "alpha", "thing", "check").is_err(),
            "and a merge is still never the explorer's"
        );
    }

    #[test]
    fn a_listing_is_refused_in_the_windows_words() {
        let (_dir, root, _clone) = plane();

        let refused = pieces_of(&root, "alpha", "nothing-here").expect_err("no such clone");

        assert!(refused.contains("nothing-here"), "{refused}");
        in_the_windows_words(&refused);
    }

    #[test]
    fn a_piece_declared_done_from_its_row_says_so_on_the_row() {
        let (_dir, root, _clone) = plane();
        worktree::add(&root, "alpha", "thing", "piece", None).unwrap();
        assert_eq!(
            pieces_of(&root, "alpha", "thing").unwrap()[0].said,
            "",
            "cut by the core directly, so nothing claimed it and nothing is silent"
        );

        declare_done(&root, None, "alpha", "thing", "piece").unwrap();

        assert_eq!(pieces_of(&root, "alpha", "thing").unwrap()[0].said, "done");
        let refused = declare_done(&root, None, "alpha", "thing", "nope").unwrap_err();
        assert!(
            refused.contains("no branch folder called 'nope'"),
            "{refused}"
        );
        in_the_windows_words(&refused);
    }

    /// What a start was handed, for the tests that stand in for the harness.
    fn started_in(cwd: Option<PathBuf>) -> Result<Option<PathBuf>, String> {
        Ok(cwd)
    }

    /// git in `dir`, for a test's own setup.
    fn git(dir: &Path, args: &[&str]) {
        let ran = purlis_core::forklock::output(
            std::process::Command::new("git")
                .arg("-C")
                .arg(dir)
                .args(["-c", "user.email=t@e.invalid", "-c", "user.name=t"])
                .args(["-c", "commit.gpgsign=false"])
                .args(args),
        )
        .unwrap();
        assert!(ran.status.success(), "git {args:?}: {ran:?}");
    }

    /// Whether the clone still has `branch`.
    fn has_branch(clone: &Path, branch: &str) -> bool {
        let listed = purlis_core::forklock::output(
            std::process::Command::new("git")
                .arg("-C")
                .arg(clone)
                .args(["branch", "--list", branch]),
        )
        .unwrap();
        !listed.stdout.is_empty()
    }

    #[test]
    fn a_chat_started_in_a_clone_starts_on_a_branch_of_its_own_and_is_told_which() {
        let (_dir, root, clone) = plane();

        let (cwd, said) = on_a_branch(&root, None, Some(&clone), None, true, started_in).unwrap();

        assert_eq!(
            cwd,
            Some(root.join("workspaces/alpha/.worktrees/thing/chat-1")),
            "the chat starts on its branch's folder"
        );
        assert_eq!(
            said,
            ["On branch chat-1 in thing, a branch of its own cut from main."],
            "ADR 0072 §4: the operator is told the branch and the repo"
        );
        let listed = pieces_of(&root, "alpha", "thing").unwrap();
        assert_eq!(listed.len(), 1);
        assert!(
            listed[0].said.starts_with("silent"),
            "logged claimed once the chat started: {:?}",
            listed[0].said
        );
    }

    #[test]
    fn a_chat_told_to_share_the_clone_starts_in_the_clone() {
        let (_dir, root, clone) = plane();

        let (cwd, said) = on_a_branch(&root, None, Some(&clone), None, false, started_in).unwrap();

        assert!(said.is_empty());
        assert_eq!(cwd, Some(clone));
        assert!(pieces_of(&root, "alpha", "thing").unwrap().is_empty());
    }

    #[test]
    fn a_chat_that_does_not_start_in_a_clone_is_cut_nothing() {
        let (_dir, root, _clone) = plane();
        let workspace = root.join("workspaces/alpha");

        for cwd in [None, Some(workspace.as_path()), Some(root.as_path())] {
            let (_, said) = on_a_branch(&root, None, cwd, None, true, started_in).unwrap();
            assert!(said.is_empty(), "{cwd:?} is not a repo's clone");
        }
        assert!(pieces_of(&root, "alpha", "thing").unwrap().is_empty());
    }

    #[test]
    fn a_start_that_panics_takes_its_branch_back_on_the_way_out() {
        let (_dir, root, clone) = plane();

        let unwound = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            on_a_branch(
                &root,
                None,
                Some(&clone),
                None,
                true,
                |_| -> Result<(), String> { panic!("the start fell over") },
            )
        }));

        assert!(unwound.is_err());
        assert!(pieces_of(&root, "alpha", "thing").unwrap().is_empty());
        assert!(!has_branch(&clone, "chat-1"));
    }

    #[test]
    fn a_refused_start_whose_branch_git_keeps_says_so_in_the_windows_words() {
        let (_dir, root, clone) = plane();

        let refused = on_a_branch(
            &root,
            None,
            Some(&clone),
            None,
            true,
            |cwd| -> Result<(), String> {
                // Work lands on the branch, and another ref holds it: the folder can go, and
                // `branch -d` still refuses, because it is not merged.
                let at = cwd.unwrap();
                std::fs::write(at.join("work.txt"), "work\n").unwrap();
                git(&at, &["add", "-A"]);
                git(&at, &["commit", "-q", "-m", "work"]);
                git(&at, &["branch", "also"]);
                Err("agents are stopped".to_string())
            },
        )
        .unwrap_err();

        assert_eq!(
            refused,
            "agents are stopped\nIts folder was taken back, and git kept the branch chat-1 in thing."
        );
        assert!(has_branch(&clone, "chat-1"));
    }

    #[test]
    fn a_chat_that_was_refused_takes_its_branch_back_with_it() {
        let (_dir, root, clone) = plane();

        let refused = on_a_branch(&root, None, Some(&clone), Some("fix login"), true, |_| {
            Err::<(), _>("agents are stopped".to_string())
        })
        .unwrap_err();

        assert_eq!(refused, "agents are stopped", "the start's own sentence");
        assert!(pieces_of(&root, "alpha", "thing").unwrap().is_empty());
        let branch = purlis_core::forklock::output(
            std::process::Command::new("git")
                .arg("-C")
                .arg(&clone)
                .args(["branch", "--list", "fix-login"]),
        )
        .unwrap();
        assert!(branch.stdout.is_empty(), "the branch went with its folder");
    }

    #[test]
    fn a_branch_cut_from_the_window_is_refused_in_the_windows_words() {
        let (_dir, root, _clone) = plane();
        cut_branch(&root, None, "alpha", "thing", Some("spike")).unwrap();

        let taken = cut_branch(&root, None, "alpha", "thing", Some("spike")).unwrap_err();

        assert_eq!(
            taken,
            "branch 'spike' already exists in thing. Pick another name, or delete that branch \
             if nothing on it is needed."
        );
    }

    #[test]
    fn a_branch_cut_from_the_window_is_named_as_typed_and_logged() {
        let (_dir, root, _clone) = plane();

        let cut = cut_branch(&root, None, "alpha", "thing", Some("spike")).unwrap();
        let generated = cut_branch(&root, None, "alpha", "thing", None).unwrap();
        let taken = cut_branch(&root, None, "alpha", "thing", Some("spike")).unwrap_err();

        assert_eq!(
            (cut.piece.as_str(), cut.branch.as_str()),
            ("spike", "spike")
        );
        assert_eq!(generated.branch, "chat-1");
        assert!(taken.contains("already exists"), "{taken}");
        let listed = pieces_of(&root, "alpha", "thing").unwrap();
        assert!(
            listed.iter().all(|p| p.said.starts_with("silent")),
            "{listed:?}"
        );
    }

    #[test]
    fn forcing_is_a_second_decision_and_it_goes_through() {
        let (_dir, root, _clone) = plane();
        let added = worktree::add(&root, "alpha", "thing", "piece", None).unwrap();
        std::fs::write(added.path.join("wip.txt"), "unsaved\n").unwrap();

        remove_piece(&root, "alpha", "thing", "piece", true)
            .expect("the operator said to discard it");

        assert!(!added.path.exists());
    }
}
