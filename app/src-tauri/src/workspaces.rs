//! Making a workspace and deleting one, as the window can reach them.
//!
//! Thin by design, exactly like `worktrees.rs`: what a workspace may be called, what it is
//! scaffolded with, and **what may be deleted** all live in `charter_core::wscmd`, and this
//! layer converts. A rule implemented here as well would be a second rule, and the two would
//! drift — which for [`workspace_remove`] means a recursive delete deciding for itself what is
//! safe to destroy.
//!
//! # The guard, and why the window cannot be past it
//!
//! `charter workspace remove` refuses when the workspace holds work that removing it would
//! discard: a clone charter could not read, a dirty tree, unpushed commits, a worktree holding
//! commits reachable from no other ref. That guard is [`charter_core::wscmd::work_at_risk`] and
//! it runs **inside `wscmd::remove`**, between the name check and `remove_dir_all`.
//!
//! So [`workspace_remove`] calls `wscmd::remove` and nothing else. It does not call
//! `remove_dir_all`, it does not read `work_at_risk` and then decide, and it does not have a
//! path that deletes a directory. There is no way to reach a delete in this module that has not
//! been through the guard, because there is only one delete and the guard is in front of it.
//! `force` is the operator's own second answer to a refusal they have read, and it is passed
//! only from a click on a button that named what would be lost.
//!
//! [`workspace_at_risk`] reads the same guard for the dialog to draw, and it is **advisory
//! only**: it decides nothing. The dialog shows it so that the operator knows before pressing
//! what charter is about to say; the delete asks again, in the core, at the moment it matters.
//! A preview that decided would be a check made against a disk that can change between the two.
//!
//! # The words on the button, which are not the decision (charter-app#182)
//!
//! Those two reads happen at two moments, and a workspace can move between them — a clone goes
//! dirty, a commit is made, a worktree appears. Nothing is then wrongly deleted, because the
//! core's own read still governs. What went wrong is smaller and still bad: the button that
//! offers to discard work drew its words from the **preview**, so it could name one clone while
//! the verbatim refusal printed above it named another, at the moment somebody is deciding
//! whether to throw work away.
//!
//! So [`workspace_remove`] refuses with a [`Refused`], which carries the core's sentence **and
//! the list that sentence was made from** — `wscmd::remove::Removal::refused_over`, read inside
//! the delete. The window draws the refusal's own list once there is one, and the preview only
//! before the first press.
//!
//! **This changes nothing about what decides.** The list is an output of the read that already
//! happened; it is not read here, not compared here, and no code in this module branches on it.
//!
//! **A refusal crosses unchanged**, for `worktrees.rs`' reason: the core's sentence names the
//! repair, and an operator shown a reworded version of it can neither follow it nor search for
//! it.

use std::path::Path;

use charter_core::extension::events::Event;
use charter_core::repocmd::Say;
use charter_core::wscmd;

use crate::planes::{PlaneId, Planes};

/// One reason a workspace holds work that deleting it would discard.
///
/// A mirror of [`wscmd::AtRisk`] rather than the thing itself, because `charter-core` never
/// depends on the app and the app's wire types are generated into TypeScript.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
pub struct AtRisk {
    /// What to look at: a clone's name, or `<repo>/<piece>` for a worktree.
    pub what: String,
    /// charter's own sentence about it, name included: `svc: 2 unpushed commit(s)`.
    pub said: String,
}

impl From<wscmd::AtRisk> for AtRisk {
    fn from(risk: wscmd::AtRisk) -> Self {
        Self {
            what: risk.what,
            said: risk.said,
        }
    }
}

/// Why a delete made nothing — **and the reading it was refused on** (charter-app#182).
///
/// Two fields and they are one answer. `said` is the core's sentence, verbatim, because it
/// names the repair. `at_risk` is `work_at_risk`'s list as the core read it *inside* the
/// delete, so the surface offering to discard that work names the same things the sentence
/// above it names. Empty for every refusal that is not the guard's — a name that is not a
/// workspace, a `workspaces/<ws>` that links out of the plane, a `remove_dir_all` that failed
/// — and that is the honest shape: `--force` does not get past any of those, so a window that
/// drew a force button beside one would be offering a way through that does not exist.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
pub struct Refused {
    /// The core's own words, unchanged and all of them.
    pub said: String,
    /// What the core refused on, in the order it said them.
    pub at_risk: Vec<AtRisk>,
}

/// What a workspace command said, turned into what a window draws.
///
/// **Exit 0 keeps every line, marks and all.** A removal can succeed and still have something
/// to say — charter#870's exclude block, a count of todos discarded — and those lines are the
/// only place the operator is told. Dropping everything but the `✓` would lose exactly the
/// news that is worth reading.
///
/// **A non-zero exit is a refusal, in the core's words and nothing else.** The `✗` lines are
/// the sentence; the mark is left off because a window draws a refusal as a refusal and would
/// otherwise say so twice. A command that failed with nothing marked hands back everything it
/// said rather than an empty refusal, because a refusal with no words is not one.
pub(crate) fn ran(code: u8, said: Vec<Say>) -> Result<Vec<String>, String> {
    if code == 0 {
        return Ok(said.iter().map(ToString::to_string).collect());
    }
    let refusals: Vec<String> = said
        .iter()
        .filter_map(|line| match line {
            Say::Fail(text) | Say::Plain(text) => Some(text.clone()),
            _ => None,
        })
        .collect();
    Err(if refusals.is_empty() {
        said.iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        refusals.join("\n")
    })
}

/// Make a workspace: `charter workspace create <name>`, with the vision when one was typed.
///
/// **The name is checked by the core and by nothing in the window.** `wscmd::create` runs
/// `wscmd::ensure`, which is where `contain::workspace_name_ok` is, so the app refuses exactly
/// the names a terminal refuses and says the same sentence about them. A second alphabet in
/// the dialog would be a second answer to what a workspace may be called, and the two would
/// drift the first time either moved.
///
/// LOCAL unless the dialog's Live box was ticked (charter-app#301): a LIVE workspace's manifest
/// and memory are published with the plane, so that is the operator's choice, never a default,
/// and a workspace born LIVE is saved at once, as switching one to LIVE is. It selects nothing:
/// `--use` writes a session lock that belongs to a terminal.
// Its plane is a `PlaneId` the registry vouches for, like every other command's
// (charter-app#127). Not a doc comment, because the generated bindings carry those.
#[tauri::command]
#[specta::specta]
pub fn workspace_create(
    app: tauri::AppHandle,
    planes: tauri::State<'_, Planes>,
    heard: tauri::State<'_, crate::heard::Heard>,
    plane: PlaneId,
    name: String,
    vision: Option<String>,
    live: bool,
) -> Result<Vec<String>, String> {
    let root = planes.held(&plane)?.root().to_path_buf();
    let mut said = create_in(&root, &name, vision.as_deref(), live)?;
    if live && let Some(not_saved) = crate::live::save_after(&root, &mut said) {
        said.push(not_saved);
    }
    // Told once it is made, on a thread of its own: nothing an extension answers can change
    // what this answers (charter-app#343).
    heard.tell(
        &app,
        plane,
        root,
        Event::WorkspaceCreated { workspace: name },
    );
    Ok(said)
}

/// A workspace for another module's test, as the window would make it: LOCAL.
#[cfg(test)]
pub(crate) fn create_for_tests(root: &Path, name: &str) {
    create_in(root, name, None, false).expect("a workspace for the test");
}

/// The operator brought a workspace to the front: tell the extensions that hear it
/// (charter-app#343). It does nothing else and answers nothing — focusing is the window's own
/// state, and this is only the report of it.
// Its plane is a `PlaneId` the registry vouches for; see `workspace_create`.
#[tauri::command]
#[specta::specta]
pub fn workspace_focused(
    app: tauri::AppHandle,
    planes: tauri::State<'_, Planes>,
    heard: tauri::State<'_, crate::heard::Heard>,
    plane: PlaneId,
    workspace: String,
) -> Result<(), String> {
    let root = planes.held(&plane)?.root().to_path_buf();
    if !charter_core::contain::workspace_name_ok(&workspace) {
        return Err(format!("'{workspace}' is not a workspace's name"));
    }
    heard.tell(&app, plane, root, Event::WorkspaceFocused { workspace });
    Ok(())
}

/// The creation itself, against a root the registry has already vouched for.
fn create_in(
    root: &Path,
    name: &str,
    vision: Option<&str>,
    live: bool,
) -> Result<Vec<String>, String> {
    // An empty box is no vision, not a vision that is empty: `set_vision` would otherwise write
    // an empty `## Vision` section into the workspace's charter and charter would read it back
    // as one that had been recorded.
    let vision = vision.map(str::trim).filter(|text| !text.is_empty());
    let ids = charter_core::active::Ids::default();
    let mut said = Vec::new();
    let code = wscmd::create::create(
        &wscmd::create::Request {
            root,
            name,
            vision,
            live,
            use_it: false,
            force: false,
            repos: &[],
            now: chrono::Utc::now(),
            ids: &ids,
        },
        &mut |line: Say| said.push(line),
    );
    ran(code, said)
}

/// What deleting this workspace would discard, for the dialog to show **before** anything is
/// pressed.
///
/// The core's own guard, read for drawing. It decides nothing: [`workspace_remove`] asks again,
/// inside `wscmd::remove`, against the disk as it is at the moment of the delete. A window that
/// treated this answer as the decision would be deciding on a reading that is already old — and
/// worse, one taken while the operator read a dialog.
// Its plane is a `PlaneId` the registry vouches for; see `workspace_create`.
#[tauri::command]
#[specta::specta]
pub fn workspace_at_risk(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    workspace: String,
) -> Result<Vec<AtRisk>, String> {
    let root = planes.held(&plane)?.root().to_path_buf();
    Ok(at_risk_in(&root, &workspace))
}

/// The reading itself, against a root the registry has already vouched for.
fn at_risk_in(root: &Path, workspace: &str) -> Vec<AtRisk> {
    wscmd::work_at_risk(root, workspace)
        .into_iter()
        .map(AtRisk::from)
        .collect()
}

/// Delete a workspace and its clones: `charter workspace remove <name> [--force]`.
///
/// **This is the one delete, and the guard is inside it.** See this module's header. `force` is
/// the operator saying to discard work the core found — it is never passed on their behalf, and
/// the window asks for it only after showing them the refusal the core gave.
// Its plane is a `PlaneId` the registry vouches for; see `workspace_create`.
#[tauri::command]
#[specta::specta]
pub fn workspace_remove(
    app: tauri::AppHandle,
    planes: tauri::State<'_, Planes>,
    heard: tauri::State<'_, crate::heard::Heard>,
    plane: PlaneId,
    workspace: String,
    force: bool,
) -> Result<Vec<String>, Refused> {
    let root = planes
        .held(&plane)
        .map_err(|why| Refused {
            said: why,
            at_risk: Vec::new(),
        })?
        .root()
        .to_path_buf();
    let removed = remove_in(&root, &workspace, force);
    if removed.is_ok() {
        heard.tell(&app, plane, root, Event::WorkspaceRemoved { workspace });
    }
    removed
}

/// The removal itself, against a root the registry has already vouched for.
///
/// Still one call, deliberately: everything this command is, is `wscmd::remove`. What it
/// answers with now carries `refused_over` — the list the core made its refusal from — and
/// that list is copied across the wire and read by nothing on this side (charter-app#182).
fn remove_in(root: &Path, workspace: &str, force: bool) -> Result<Vec<String>, Refused> {
    let mut said = Vec::new();
    let done = wscmd::remove::remove(root, workspace, force, &mut |line: Say| said.push(line));
    // `ran` is untouched and shared with `workspace_create`: which lines are a refusal, and
    // whether there was one at all, is still decided in exactly one place.
    ran(done.code, said).map_err(|said| Refused {
        said,
        at_risk: done.refused_over.into_iter().map(AtRisk::from).collect(),
    })
}

/// Rename a workspace: `charter workspace rename <workspace> <name>` (charter#367).
///
/// **The core decides and does everything**: which names are refused, the move, the worktree
/// repair, every record that names the workspace — the app's record on disk and this machine's
/// pins included — and the plane save a LIVE workspace's move takes. This layer adds the one
/// thing only the window knows, which chats are running in it: a running chat refuses the
/// rename, named as its tab names it. Once renamed, what the window holds in memory follows
/// ([`crate::chats::Chats::follow`]), or its next write of the record would put the old name
/// back.
// Its plane is a `PlaneId` the registry vouches for; see `workspace_create`.
#[tauri::command]
#[specta::specta]
pub async fn workspace_rename(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    workspace: String,
    name: String,
) -> Result<Vec<String>, String> {
    let held = planes.held(&plane)?;
    let config = planes.config().map(Path::to_path_buf);
    tauri::async_runtime::spawn_blocking(move || {
        rename_in(&held, config.as_deref(), &workspace, &name)
    })
    .await
    .map_err(|err| format!("the rename did not finish: {err}"))?
}

/// The chats that will start a fresh conversation if `workspace` is renamed, by the name each
/// tab shows — what the Rename dialog says before it is answered (charter#367, D10).
///
/// Asked of what the window holds, which is what the record is written from: a Claude Code
/// chat in the workspace with a conversation to resume. The rename itself still goes ahead.
// Its plane is a `PlaneId` the registry vouches for; see `workspace_create`.
#[tauri::command]
#[specta::specta]
pub async fn workspace_starts_fresh(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    workspace: String,
) -> Result<Vec<String>, String> {
    let held = planes.held(&plane)?;
    tauri::async_runtime::spawn_blocking(move || starts_fresh_in(&held, &workspace))
        .await
        .map_err(|err| format!("could not ask which chats start fresh: {err}"))
}

/// [`workspace_starts_fresh`], against a plane the registry holds.
pub(crate) fn starts_fresh_in(held: &crate::planes::Held, workspace: &str) -> Vec<String> {
    wscmd::rename::starts_fresh(held.root(), workspace, &held.chats().record())
}

/// The rename itself, against a plane the registry holds.
pub(crate) fn rename_in(
    held: &crate::planes::Held,
    config_root: Option<&Path>,
    workspace: &str,
    name: &str,
) -> Result<Vec<String>, String> {
    let root = held.root().to_path_buf();
    let chats = held.chats();
    let on_disk = charter_core::workspaces::Plane::open(&root);
    // Under either name, so a rename finished after a crash is guarded as well.
    let running: Vec<String> = chats
        .open_now()
        .into_iter()
        .filter(|one| {
            one.cwd
                .as_deref()
                .and_then(|cwd| on_disk.workspace_of(cwd))
                .is_some_and(|ws| ws == workspace || ws == name)
        })
        .filter_map(|one| chats.shown_name(one.session))
        .collect();
    let mut said = Vec::new();
    let code = wscmd::rename::rename(
        &wscmd::rename::Request {
            root: &root,
            old: workspace,
            new: name,
            running: &running,
            config_root,
        },
        &mut |line: Say| said.push(line),
    );
    if code == 0 {
        chats.follow(&wscmd::rename::Move::in_plane(&root, workspace, name));
    }
    ran(code, said)
}

/// One repo the picker offers: what the operator reads to choose it.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
pub struct ReachableRepo {
    /// The name it is cloned under, and asked for by.
    pub name: String,
    /// `owner/name` on its forge.
    pub path: String,
    pub description: String,
}

/// What the picker draws: the repos this operator's forge logins reach, and a sentence for
/// each forge that did not answer.
#[derive(Debug, Clone, Default, serde::Serialize, specta::Type)]
pub struct ReachableRepos {
    pub repos: Vec<ReachableRepo>,
    pub trouble: Vec<String>,
}

/// The repos the operator's own forge login reaches, asked now and held nowhere (ADR 0055).
///
/// On a blocking thread: it is one forge call per page and per host, each with the forge
/// CLI's own deadline.
// Its plane is a `PlaneId` the registry vouches for; see `workspace_create`.
#[tauri::command]
#[specta::specta]
pub async fn reachable_repos(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
) -> Result<ReachableRepos, String> {
    let root = planes.held(&plane)?.root().to_path_buf();
    tauri::async_runtime::spawn_blocking(move || reachable_in(&root))
        .await
        .map_err(|err| format!("the forge listing did not finish: {err}"))?
}

fn reachable_in(root: &Path) -> Result<ReachableRepos, String> {
    let found = charter_core::repocmd::reachable::reachable(root)?;
    let text = |r: &serde_json::Value, key: &str| {
        r.get(key)
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    Ok(ReachableRepos {
        repos: found
            .repos
            .iter()
            .map(|r| ReachableRepo {
                name: text(r, "name"),
                path: text(r, "path_with_namespace"),
                description: text(r, "description"),
            })
            .collect(),
        trouble: found.trouble,
    })
}

/// Add the repos the operator picked to the inventory, beside what it lists, so each can be
/// cloned by name (ADR 0055). Asked once for a whole pick, before the clones.
// Its plane is a `PlaneId` the registry vouches for; see `workspace_create`.
#[tauri::command]
#[specta::specta]
pub async fn take_repos(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    repos: Vec<String>,
) -> Result<(), String> {
    let root = planes.held(&plane)?.root().to_path_buf();
    tauri::async_runtime::spawn_blocking(move || {
        charter_core::repocmd::reachable::take(&root, &repos)
    })
    .await
    .map_err(|err| format!("the inventory was not updated: {err}"))?
}

/// Clone ONE repo into a workspace: `charter clone <repo> -w <workspace>`.
///
/// One per call, and the window calls it once per repo in turn: that is what lets it show each
/// repo's own state as it lands and retry one that failed, and no two calls race to write the
/// workspace's manifest. On a blocking thread, because a clone can take the network's two
/// minutes.
// Its plane is a `PlaneId` the registry vouches for; see `workspace_create`.
#[tauri::command]
#[specta::specta]
pub async fn clone_repo(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    workspace: String,
    repo: String,
) -> Result<Vec<String>, String> {
    let root = planes.held(&plane)?.root().to_path_buf();
    tauri::async_runtime::spawn_blocking(move || clone_into(&root, &workspace, &repo))
        .await
        .map_err(|err| format!("the clone did not finish: {err}"))?
}

fn clone_into(root: &Path, workspace: &str, repo: &str) -> Result<Vec<String>, String> {
    let author = wscmd::ensure::author();
    let mut said = Vec::new();
    let code = charter_core::repocmd::clone::clone(
        &charter_core::repocmd::clone::Request {
            root,
            ws: workspace,
            repos: &[repo.to_string()],
            now: chrono::Utc::now(),
            author: &author,
        },
        &mut |line: Say| said.push(line),
    );
    ran(code, said)
}

/// Take one repo out of a workspace, clone and manifest row both (ADR 0055).
///
/// **The guard is inside the delete**, exactly as [`workspace_remove`]'s is: this calls
/// `wscmd::drop::drop_repo` and nothing else, and there is no `force`.
// Its plane is a `PlaneId` the registry vouches for; see `workspace_create`.
#[tauri::command]
#[specta::specta]
pub async fn drop_repo(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    workspace: String,
    repo: String,
) -> Result<Vec<String>, Refused> {
    let root = planes
        .held(&plane)
        .map_err(|why| Refused {
            said: why,
            at_risk: Vec::new(),
        })?
        .root()
        .to_path_buf();
    tauri::async_runtime::spawn_blocking(move || drop_in(&root, &workspace, &repo))
        .await
        .map_err(|err| Refused {
            said: format!("the removal did not finish: {err}"),
            at_risk: Vec::new(),
        })?
}

fn drop_in(root: &Path, workspace: &str, repo: &str) -> Result<Vec<String>, Refused> {
    let mut said = Vec::new();
    let done = wscmd::drop::drop_repo(root, workspace, repo, &mut |line: Say| said.push(line));
    ran(done.code, said).map_err(|said| Refused {
        said,
        at_risk: done.refused_over.into_iter().map(AtRisk::from).collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// A plane charter would read, with the settings a workspace's layer is cut from.
    fn plane() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("a directory");
        let root = std::fs::canonicalize(dir.path()).expect("it resolves");
        std::fs::write(root.join("charter.toml"), "schema = 1\n").expect("a manifest");
        std::fs::create_dir_all(root.join(".claude")).expect("a settings directory");
        std::fs::write(
            root.join(".claude/settings.json"),
            "{\"env\": {\"CHARTER_HARNESS\": \"claude-code\"}}\n",
        )
        .expect("settings");
        std::fs::create_dir_all(root.join("workspaces")).expect("a workspaces directory");
        (dir, root)
    }

    fn git(at: &Path, argv: &[&str]) {
        charter_core::forklock::output(
            std::process::Command::new("git")
                .arg("-C")
                .arg(at)
                .args(argv),
        )
        .expect("git runs in a test");
    }

    /// A clone with one commit in it, where a workspace's clone goes.
    fn clone_in(root: &Path, workspace: &str, name: &str) -> PathBuf {
        let at = root.join("workspaces").join(workspace).join(name);
        std::fs::create_dir_all(&at).expect("a clone directory");
        git(&at, &["init", "-q", "-b", "main", "."]);
        git(&at, &["config", "user.email", "t@e.invalid"]);
        git(&at, &["config", "user.name", "t"]);
        // **A fixture repository may not inherit the developer's signing configuration.** With
        // `commit.gpgsign = true` and an SSH signer in `~/.gitconfig` — an ordinary setup, and
        // the operator's — the commit below either parks on a biometric prompt or fails with
        // the agent's own error, and every test of this delete goes red for a reason that has
        // nothing to do with it. **CI cannot see it**: a runner has no signing config. charter's
        // Python suite hit exactly this and turned it off everywhere
        // (its 0.54.0 news note, in `diazoxide/charter-plane`); this
        // port carried the helper over without it.
        git(&at, &["config", "commit.gpgsign", "false"]);
        std::fs::write(at.join("README.md"), "one\n").expect("a file");
        git(&at, &["add", "-A"]);
        git(&at, &["commit", "-q", "-m", "one"]);
        at
    }

    #[test]
    fn a_workspace_is_created_with_the_baseline_the_cli_gives_it() {
        let (_dir, root) = plane();

        let said = create_in(&root, "alpha", None, false).expect("a plain name is created");

        let ws = root.join("workspaces/alpha");
        assert!(ws.join("workspace.json").exists(), "{said:?}");
        assert!(ws.join("workspace.md").exists(), "{said:?}");
        assert!(ws.join("memory/MEMORY.md").exists(), "{said:?}");
        assert!(
            said.iter().any(|line| line.contains("Workspace 'alpha'")),
            "{said:?}"
        );
    }

    #[test]
    fn the_name_is_the_cores_rule_and_the_refusal_is_the_cores_sentence() {
        // The whole point of going through `wscmd::create`: the window refuses exactly what a
        // terminal refuses, in the words a terminal uses. A name check written here would be a
        // second answer, and `../escape` is what a second answer gets wrong.
        let (_dir, root) = plane();

        for bad in ["../escape", "/absolute", ".hidden", "", "has space"] {
            let refused = create_in(&root, bad, None, false)
                .expect_err(&format!("'{bad}' is not a workspace name"));
            assert!(
                refused.contains("invalid workspace name"),
                "{bad}: {refused}"
            );
        }
        assert!(
            !root.parent().expect("a parent").join("escape").exists(),
            "nothing was written outside the plane"
        );
        assert!(!root.join("escape").exists());
    }

    #[test]
    fn a_vision_is_recorded_and_an_empty_box_is_not_a_vision() {
        let (_dir, root) = plane();

        create_in(&root, "alpha", Some("  ship the thing  "), false).expect("created");
        create_in(&root, "beta", Some("   "), false).expect("created");
        create_in(&root, "gamma", None, false).expect("created");

        // Read back the way charter reads it, which answers "" for its own placeholder — so
        // "no vision" is the core's own judgement and not this test's reading of a file.
        let vision = |name: &str| {
            charter_core::workspaces::Plane::open(&root)
                .workspace(name)
                .expect("a workspace")
                .vision()
                .trim()
                .to_owned()
        };
        assert_eq!(vision("alpha"), "ship the thing");
        assert_eq!(vision("beta"), "");
        assert_eq!(vision("gamma"), "");

        // **And `vision()` is not enough to catch this, which is why the file is read too.**
        // `set_vision("")` empties the section, and `vision()` answers "" for an empty section
        // exactly as it answers "" for charter's placeholder — so a window that recorded an
        // empty box would look identical through that door. What actually goes is the PROMPT:
        // the line in `workspace.md` that asks for the one thing a fork inherits. A workspace
        // made with an empty box has to be the workspace made with no vision at all.
        let charter_of = |name: &str| {
            std::fs::read_to_string(root.join("workspaces").join(name).join("workspace.md"))
                .expect("its charter")
        };
        let placeholder = charter_core::workspaces::VISION_PLACEHOLDER;
        assert!(
            charter_of("beta").contains(placeholder),
            "{}",
            charter_of("beta")
        );
        assert!(charter_of("gamma").contains(placeholder));
        assert!(!charter_of("alpha").contains(placeholder));
    }

    #[test]
    fn an_empty_workspace_is_deleted() {
        let (_dir, root) = plane();
        create_in(&root, "alpha", None, false).expect("created");

        let said = remove_in(&root, "alpha", false).expect("nothing is at risk in it");

        assert!(!root.join("workspaces/alpha").exists(), "{said:?}");
    }

    #[test]
    fn a_dirty_clone_refuses_the_delete_and_nothing_is_removed() {
        // **The guard, through the command the window calls.** `wscmd::work_at_risk` is what
        // decides, and it decides inside `wscmd::remove`; this asserts the window's own path
        // reaches it, refuses, and leaves the directory where it was.
        let (_dir, root) = plane();
        let clone = clone_in(&root, "alpha", "svc");
        std::fs::write(clone.join("README.md"), "changed\n").expect("a change");

        let refused = remove_in(&root, "alpha", false).expect_err("a dirty clone is refused");

        assert!(
            refused.said.contains("this would discard work"),
            "{refused:?}"
        );
        assert!(
            refused.said.contains("svc: uncommitted changes"),
            "{refused:?}"
        );
        assert!(clone.exists(), "the guard fired, so nothing was deleted");
        assert!(clone.join("README.md").exists());
    }

    #[test]
    fn unticking_a_dirty_repo_is_refused_with_the_list_it_was_refused_on() {
        // The picker's removal goes through the same guard, and the window gets the reading
        // that refused it, not a second one (charter-app#182).
        let (_dir, root) = plane();
        let clone = clone_in(&root, "alpha", "svc");
        std::fs::write(clone.join("README.md"), "changed\n").expect("a change");

        let refused = drop_in(&root, "alpha", "svc").expect_err("a dirty clone is refused");

        assert_eq!(refused.at_risk.len(), 1, "{refused:?}");
        assert_eq!(refused.at_risk[0].what, "svc");
        assert!(clone.join("README.md").exists());
    }

    #[test]
    fn unticking_a_clean_repo_removes_only_that_clone() {
        let (_dir, root) = plane();
        let gone = clone_in(&root, "alpha", "gone");
        let kept = clone_in(&root, "alpha", "kept");

        drop_in(&root, "alpha", "gone").expect("a clean clone is removed");

        assert!(!gone.exists());
        assert!(kept.exists());
    }

    #[test]
    fn a_worktree_holding_commits_that_exist_nowhere_else_refuses_the_delete() {
        // The second half of the guard, and the half a clone walk cannot see (charter#91): a
        // clone's `.git` is a directory and a linked worktree's is a file, so a delete that
        // only checked clones took worktrees with it while reporting nothing.
        let (_dir, root) = plane();
        let clone = clone_in(&root, "alpha", "svc");
        let piece = root.join("workspaces/alpha/.worktrees/svc/task");
        git(
            &clone,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                "task",
                &piece.display().to_string(),
            ],
        );
        std::fs::write(piece.join("work.md"), "x\n").expect("a file");
        git(&piece, &["add", "-A"]);
        git(&piece, &["commit", "-q", "-m", "unique"]);

        let refused = remove_in(&root, "alpha", false).expect_err("unique commits are refused");

        assert!(
            refused
                .said
                .contains("svc/task: 1 commit(s) that exist nowhere else"),
            "{refused:?}"
        );
        assert!(piece.exists());
    }

    #[test]
    fn a_clone_charter_cannot_read_refuses_the_delete() {
        // charter#917, and the sharpest instance of it: what the guard's list is empty of is
        // what is handed to a recursive delete. A `git status` that FAILED must never read as
        // a clean tree.
        let (_dir, root) = plane();
        let clone = root.join("workspaces/alpha/svc");
        std::fs::create_dir_all(clone.join(".git")).expect("a .git that is not a repository");

        let refused = remove_in(&root, "alpha", false).expect_err("an unreadable clone refuses");

        assert!(refused.said.contains("could not be read"), "{refused:?}");
        assert!(clone.exists());
    }

    #[test]
    fn forcing_is_a_second_decision_and_it_goes_through() {
        let (_dir, root) = plane();
        let clone = clone_in(&root, "alpha", "svc");
        std::fs::write(clone.join("README.md"), "changed\n").expect("a change");
        remove_in(&root, "alpha", false).expect_err("refused first");

        remove_in(&root, "alpha", true).expect("the operator said to discard it");

        assert!(!root.join("workspaces/alpha").exists());
    }

    #[test]
    fn what_the_dialog_previews_is_the_guards_own_list() {
        // The preview draws the core's sentences and writes none of its own — so a refusal the
        // operator reads on the dialog is word for word the refusal the delete will give.
        let (_dir, root) = plane();
        let clone = clone_in(&root, "alpha", "svc");
        std::fs::write(clone.join("README.md"), "changed\n").expect("a change");

        let shown = at_risk_in(&root, "alpha");
        let refused = remove_in(&root, "alpha", false).expect_err("refused");

        assert_eq!(shown.len(), 1, "{shown:?}");
        assert_eq!(shown[0].what, "svc");
        assert_eq!(shown[0].said, "svc: uncommitted changes");
        assert!(refused.said.contains(&shown[0].said), "{refused:?}");
    }

    /// **The refusal carries the list it was refused on, and the preview is not it**
    /// (charter-app#182).
    ///
    /// The workspace is made to move between the two reads, which is the whole failure: the
    /// preview sees one clone at risk, a second goes dirty while the dialog is up, and the
    /// core refuses over both. Before this, the only list the window had to name what it was
    /// about to discard was the first one — so the force button named `svc` under a sentence
    /// naming `svc` and `lib`, and the operator was reading two descriptions of one act that
    /// did not agree.
    #[test]
    fn a_refusal_names_what_the_core_read_and_not_what_the_preview_read() {
        let (_dir, root) = plane();
        let svc = clone_in(&root, "alpha", "svc");
        let lib = clone_in(&root, "alpha", "lib");
        std::fs::write(svc.join("README.md"), "changed\n").expect("a change");

        let preview = at_risk_in(&root, "alpha");

        // The workspace moves while the dialog is up.
        std::fs::write(lib.join("README.md"), "changed too\n").expect("a second change");

        let refused = remove_in(&root, "alpha", false).expect_err("both clones are at risk now");

        assert_eq!(
            preview.iter().map(|r| &r.what).collect::<Vec<_>>(),
            vec!["svc"],
            "the preview is the older reading, and it stays older"
        );
        assert_eq!(
            refused.at_risk.iter().map(|r| &r.what).collect::<Vec<_>>(),
            vec!["lib", "svc"]
        );
        // The claim: every name the button would draw is a name the sentence above it uses.
        for risk in &refused.at_risk {
            assert!(refused.said.contains(&risk.said), "{refused:?}");
        }
    }

    #[test]
    fn a_workspace_that_is_a_link_out_of_the_plane_is_refused_and_its_target_survives() {
        let (_dir, root) = plane();
        let outside = tempfile::tempdir().expect("a directory");
        std::fs::create_dir_all(outside.path().join("treasure")).expect("a directory");
        std::fs::write(outside.path().join("treasure/keep.txt"), "x").expect("a file");
        std::os::unix::fs::symlink(
            outside.path().join("treasure"),
            root.join("workspaces/alpha"),
        )
        .expect("a link where a workspace goes");

        let refused = remove_in(&root, "alpha", true).expect_err("a link out of the plane");

        assert!(
            refused
                .said
                .contains("does not resolve to a directory inside this plane"),
            "{refused:?}"
        );
        // Not the guard's refusal, so there is nothing to offer forcing past — and a window
        // that drew a force button here would be offering a way through that does not exist.
        assert!(refused.at_risk.is_empty(), "{refused:?}");
        assert!(
            outside.path().join("treasure/keep.txt").exists(),
            "--force is not a way out of the plane"
        );
    }
}
