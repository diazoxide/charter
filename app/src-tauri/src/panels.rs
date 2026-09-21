//! The right-hand panels for the focused workspace: its repos, their branches, CI, its
//! todos and the plane's personas (spec decision 1). Read-only, as M1 says panels are.
//!
//! # Two answers, and that is the design
//!
//! `of` is a directory listing and a handful of small files. It comes back at once, so the
//! todos and the persona row are on screen the moment a workspace is focused.
//!
//! `repo_states` runs `git status` once per clone, which is bounded at five seconds each and
//! is the only slow thing a panel does. It is its own command so that nothing else waits for
//! it, and the window draws "reading…" in the meantime.
//!
//! **Nothing here crosses a network, at all.** The CI cell is read out of
//! `.charter/cache/glstate.json`, which some other process writes; `charter_core::cistate`
//! says why in full. A panel that fetched would put a forge token in the process that draws
//! the window and hold that window for as long as `gh` takes.

use std::path::Path;

use charter_core::cistate::{self, Reading};
use charter_core::repos::{self, Head};
use charter_core::workspaces::Plane;

/// One open todo. There is no state field: a closed todo is a deleted file (ADR 0004).
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
pub(crate) struct PanelTodo {
    /// The file stem, which is what a todo is closed by.
    slug: String,
    title: String,
    /// The date the todo was written, as the file records it.
    stamp: String,
}

/// Everything the panels can draw without running git.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
pub(crate) struct Panels {
    /// The workspace this is about, so a late answer can be matched to the ask and an answer
    /// for a workspace that is no longer focused can be thrown away.
    workspace: String,
    /// The clones on disk, by name, in the order the directory lists them.
    repos: Vec<String>,
    /// Repos `workspace.json` names that are not cloned here. Membership, not presence.
    absent: Vec<String>,
    /// What charter would not look at, by name and reason. Shown, never dropped: a row that
    /// is missing is otherwise merely missing.
    refused: Vec<(String, String)>,
    todos: Vec<PanelTodo>,
    /// Why the todos could not be read, where they could not. A store that is a link out of
    /// the plane is refused, and "no todos" would be the wrong thing to draw for it.
    todos_refused: Option<String>,
    /// The plane's personas, and the one a chat started here would adopt.
    personas: Vec<String>,
    persona: Option<String>,
}

/// One clone's git state, and what the forge cache last recorded for its branch.
#[derive(Debug, Clone, Default, serde::Serialize, specta::Type)]
pub(crate) struct RepoState {
    name: String,
    /// The branch the checkout is on, where it is on one.
    branch: Option<String>,
    /// Whether that branch holds no commit yet.
    unborn: bool,
    /// The commit HEAD sits on when it is on no branch.
    detached: Option<String>,
    upstream: Option<String>,
    ahead: u32,
    behind: u32,
    /// Changed files git is tracking.
    tracked: u32,
    /// Files git is not tracking.
    untracked: u32,
    /// Why charter could not read the tree. Every count above is zero when this is set, and
    /// it means charter does not know — not that the tree is clean.
    unreadable: Option<String>,
    /// What the forge cache last recorded, one of the seven states charter knows.
    ci: Option<String>,
    change: Option<u32>,
    sigil: Option<String>,
    /// How long ago the cache entry was written, which is how old this answer is.
    fetched_seconds_ago: Option<u32>,
    /// Why there is nothing from the forge to show. Absent when something was fetched, even
    /// when what was fetched named no pipeline.
    not_fetched: Option<String>,
}

/// Every clone of one workspace, as the panels draw them.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
pub(crate) struct RepoStates {
    workspace: String,
    repos: Vec<RepoState>,
    /// Why the forge cache was not read at all, where it was not. Reported once for the
    /// listing rather than repeated on every row.
    cache_refused: Option<String>,
}

/// The panels that need no git, for one workspace of one project.
///
/// **The project is handed in, never resolved here.** This used to walk up from the process's
/// working directory — the singleton resolver ADR 0034 was written to remove — which was
/// already wrong the moment #121 let a window open a project the launch had not, and is
/// plainly wrong now that a window holds several: the panels would have drawn the workspace of
/// whichever project the process happened to start in, under the heading of the one on screen.
pub(crate) fn of(root: &Path, workspace: &str) -> Result<Panels, String> {
    let plane = Plane::open(root);
    let found = repos::clones(root, workspace).map_err(|why| why.to_string())?;
    let here: Vec<String> = found.repos.iter().map(|repo| repo.name.clone()).collect();
    let absent = repos::declared(&plane, workspace)
        .into_iter()
        .filter(|name| !here.contains(name))
        .collect();
    let ws = plane.workspace(workspace).map_err(|why| why.to_string())?;
    let (todos, todos_refused): (Vec<PanelTodo>, Option<String>) = match ws.todos() {
        Ok(open) => (
            open.into_iter()
                .map(|todo| PanelTodo {
                    slug: todo.slug,
                    title: todo.title,
                    stamp: todo.stamp,
                })
                .collect(),
            None,
        ),
        Err(why) => (Vec::new(), Some(why.to_string())),
    };
    Ok(Panels {
        workspace: workspace.to_string(),
        repos: here,
        absent,
        refused: found.refused,
        todos,
        todos_refused,
        personas: plane.personas().map_err(|why| why.to_string())?,
        persona: plane.default_persona(),
    })
}

/// The panel that needs git. Call it off the thread that draws, for one project's workspace.
pub(crate) fn repo_states(root: &Path, workspace: &str) -> Result<RepoStates, String> {
    let binary = crate::charter_binary();
    states_of(root, workspace, binary.as_deref())
}

/// [`repo_states`] with the `charter` binary said out loud rather than discovered.
///
/// Split so the refresh trigger below is a test's to drive: `charter_binary` looks beside the
/// running executable, and a test that may not touch the environment cannot point it anywhere
/// (`std::env::set_var` is `unsafe`, and this workspace forbids that).
fn states_of(root: &Path, workspace: &str, binary: Option<&Path>) -> Result<RepoStates, String> {
    let found = repos::clones(root, workspace).map_err(|why| why.to_string())?;
    // Read once for the whole listing, so every row ages against the same instant and a
    // refusal is reported once rather than on every row.
    let (cache, cache_refused) = match cistate::read(root) {
        Ok(cache) => (Some(cache), None),
        Err(why) => (None, Some(why.to_string())),
    };
    // Read FIRST, then decide whether to refresh: this listing draws what the cache holds now,
    // and the refresh is for the next one. Python's render path does the two in this order for
    // the same reason (`glstate.read_for`, then `glstate.maybe_spawn`).
    refresh_if_it_is_due(root, workspace, binary);
    Ok(RepoStates {
        workspace: workspace.to_string(),
        repos: found
            .repos
            .iter()
            .map(|repo| one(repo, cache.as_ref()))
            .collect(),
        cache_refused,
    })
}

/// Kick off a background forge refresh for this workspace, if the policy says one is due —
/// charter-app#69.
///
/// **This is the trigger, and it is a user action rather than a timer.** Focusing a workspace
/// is what runs this panel, and no daemon runs anywhere in charter-app: an app nobody touches
/// makes no forge call, ever. `charter_core::glstate` holds the decision — the refresh window,
/// the cooldown, the stuck window, and the lock that names the refresh in flight — so that two
/// panels in quick succession are one refresh and a wedged one is not replaced every two
/// minutes, each replacement holding the forge credential.
///
/// It **spawns**, never waits: the spec's budget for a workspace switch is 100 ms, and the
/// slow thing in this command is already the one `git status` per clone above.
///
/// Silent where a refusal is routine — cooling down, already running, nothing stale — and out
/// loud where it is not, because a CI column that never fills over a `charter` binary that
/// went missing would otherwise look exactly like one nobody has refreshed.
fn refresh_if_it_is_due(root: &Path, workspace: &str, binary: Option<&Path>) {
    use charter_core::glstate::Refreshing;

    let Some(binary) = binary else {
        // Already said once, at startup, by the launch that could not find it. Saying it again
        // on every workspace focus would be the same sentence fifty times an hour.
        return;
    };
    // The same list the refresher itself walks, and the same one the panel draws.
    let Ok(targets) = charter_core::glrefresh::trees(root, workspace) else {
        return;
    };
    match charter_core::glstate::maybe_spawn(root, workspace, &targets.trees, binary) {
        // Cooling down, one already in flight, nothing stale, or the operator's own brake:
        // every one of these is the policy working, and none of them is news.
        Refreshing::Started { .. } | Refreshing::Declined(_) => {}
        Refreshing::NotStarted { why } => {
            eprintln!("charter: a forge refresh for '{workspace}' would not start ({why})");
        }
    }
}

/// One row: what git said, and then what the cache says about the branch git named.
fn one(repo: &repos::Repo, cache: Option<&cistate::Cache>) -> RepoState {
    let mut row = RepoState {
        name: repo.name.clone(),
        ..RepoState::default()
    };
    let state = match repos::state_of(&repo.path) {
        Ok(state) => state,
        Err(why) => {
            row.unreadable = Some(why.to_string());
            // The cache is keyed by branch, and charter does not know which branch this is.
            row.not_fetched = Some(
                "charter could not read the checkout, so it cannot say which branch to ask \
                 about"
                    .into(),
            );
            return row;
        }
    };
    let branch = match &state.head {
        Head::Branch(name) => {
            row.branch = Some(name.clone());
            Some(name.clone())
        }
        Head::Unborn(name) => {
            row.branch = Some(name.clone());
            row.unborn = true;
            Some(name.clone())
        }
        Head::Detached(at) => {
            row.detached = Some(at.clone());
            None
        }
    };
    row.upstream = state.upstream;
    row.ahead = state.ahead;
    row.behind = state.behind;
    row.tracked = state.tracked;
    row.untracked = state.untracked;

    let Some(branch) = branch else {
        row.not_fetched =
            Some("this checkout is on no branch, and forge state is recorded per branch".into());
        return row;
    };
    // A cache charter refused is reported once for the listing; leaving the row's own reason
    // empty is what keeps the window from saying the same sentence on every line.
    let Some(cache) = cache else {
        return row;
    };
    match cache.about(&repo.path, &branch) {
        Reading::Fetched {
            state,
            change,
            sigil,
            seconds_ago,
        } => {
            row.ci = state;
            row.change = u32::try_from(change.unwrap_or(0)).ok().filter(|n| *n > 0);
            row.sigil = sigil.map(String::from);
            row.fetched_seconds_ago = Some(u32::try_from(seconds_ago).unwrap_or(u32::MAX));
        }
        Reading::NotFetched(why) => row.not_fetched = Some(why),
    }
    row
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// A plane with one workspace holding one clone, and nothing fetched for it.
    fn plane_with_a_clone() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("a plane");
        let root = std::fs::canonicalize(dir.path()).expect("a resolved plane");
        std::fs::write(root.join("charter.toml"), "").expect("a manifest");
        std::fs::create_dir_all(root.join("workspaces/alpha/svc/.git")).expect("a clone");
        std::fs::write(
            root.join("workspaces/alpha/svc/.git/HEAD"),
            "ref: refs/heads/main\n",
        )
        .expect("a HEAD");
        (dir, root)
    }

    /// A stand-in `charter` that records how it was called and then ends at once.
    fn stand_in(at: &Path) -> PathBuf {
        let binary = at.join("charter-stand-in");
        std::fs::write(
            &binary,
            "#!/bin/sh\nprintf '%s %s\\n' \"$1\" \"$3\" >> \"$(dirname \"$0\")/ran\"\n",
        )
        .expect("the stand-in is written");
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755))
            .expect("it is runnable");
        binary
    }

    /// Everything the stand-in has recorded so far, once it has recorded anything.
    fn ran(at: &Path) -> String {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            if let Ok(text) = std::fs::read_to_string(at.join("ran"))
                && !text.is_empty()
            {
                // Give a second line the chance to arrive, so "exactly one" is not merely
                // "the first one got there first".
                std::thread::sleep(std::time::Duration::from_millis(200));
                return std::fs::read_to_string(at.join("ran")).unwrap_or(text);
            }
            assert!(
                std::time::Instant::now() < deadline,
                "the refresh never ran"
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }

    #[test]
    fn focusing_a_workspace_is_what_kicks_a_forge_refresh() {
        // **The wiring, and the whole of charter-app#69's visible half.** Until this, nothing
        // anywhere called the refresher: the CI column showed whatever the last
        // `charter gl-refresh` typed by hand had left. `glstate` holds the policy, and this is
        // the only thing that asks it — a user action, never a timer.
        let (_plane, root) = plane_with_a_clone();
        let beside = tempfile::tempdir().expect("somewhere for the stand-in");
        let binary = stand_in(beside.path());

        let drawn = states_of(&root, "alpha", Some(&binary)).expect("the panel draws");

        assert_eq!(drawn.workspace, "alpha");
        assert_eq!(
            ran(beside.path()).trim(),
            "gl-refresh alpha",
            "the refresh was not asked for the workspace that was focused"
        );
    }

    #[test]
    fn focusing_it_again_does_not_start_a_second_refresh() {
        // The cooldown reaching all the way out to the trigger: an operator clicking between
        // two workspaces, or a panel asked twice, is one forge process and not two.
        let (_plane, root) = plane_with_a_clone();
        let beside = tempfile::tempdir().expect("somewhere for the stand-in");
        let binary = stand_in(beside.path());

        states_of(&root, "alpha", Some(&binary)).expect("the panel draws");
        let once = ran(beside.path());
        states_of(&root, "alpha", Some(&binary)).expect("the panel draws again");
        std::thread::sleep(std::time::Duration::from_millis(300));

        assert_eq!(once.lines().count(), 1, "the first focus ran {once:?}");
        assert_eq!(
            std::fs::read_to_string(beside.path().join("ran")).unwrap_or_default(),
            once,
            "the second focus started another refresh"
        );
    }

    #[test]
    fn a_panel_drawn_with_no_charter_beside_the_app_still_draws() {
        // Without a binary there is nothing to spawn, and a panel is not the place to say so:
        // the launch already said it once, and repeating it per focus is fifty lines an hour.
        let (_plane, root) = plane_with_a_clone();

        let drawn = states_of(&root, "alpha", None).expect("the panel draws");

        assert_eq!(drawn.repos.len(), 1);
    }
}
