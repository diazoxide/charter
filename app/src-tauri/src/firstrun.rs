//! The first run's two commands (FR-4, #603): what this machine has, and a repository opened
//! into the local plane.
//!
//! The rules are `charter_core::firstrun`'s. This module is the window's door to them: it puts
//! the answers in the shape the window draws, keeps the slow parts off the thread that draws,
//! and opens the local plane through the same trust gate every other open goes through.

use std::path::{Path, PathBuf};

use charter_core::firstrun;
use charter_core::forge::{Forge, Kind};

use crate::opener::Opened;
use crate::planes::Planes;

/// One harness, as the first-run screen lists it.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
pub struct HarnessRow {
    /// The word the plane calls it by (`claude`, `codex`, `opencode`).
    pub name: String,
    /// What the screen calls it.
    pub title: String,
    /// Whether its program is installed where charter looks.
    pub installed: bool,
    /// Whether a sign-in was found. `false` is not a refusal: the harness asks for its own
    /// login when its chat starts.
    pub signed_in: bool,
}

/// The forge CLI, as the first-run screen lists it.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
pub struct ForgeRow {
    /// The program (`gh`).
    pub cli: String,
    pub installed: bool,
    /// Whether it is logged in to its default host.
    pub signed_in: bool,
}

/// What the first-run screen shows about this machine.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
pub struct FirstRunFound {
    pub harnesses: Vec<HarnessRow>,
    pub forge: ForgeRow,
}

/// What opening a repo made: the plane, opened or asked about, and where the first chat
/// starts.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
pub struct OpenedRepo {
    /// The local plane: open, or the trust question to ask first.
    pub opened: Opened,
    /// The workspace, named after the repo.
    pub workspace: String,
    /// The repo's clone in it, where the first chat starts.
    pub cwd: String,
    /// The one harness installed and signed in on this machine, when exactly one is: the
    /// first chat starts on it without the picker (W10's interrupt budget). `null` when there
    /// is a choice to make.
    pub harness: Option<String>,
}

/// Which harnesses are installed and signed in, and whether `gh` is logged in.
///
/// **On a blocking thread**: `gh auth status` is a subprocess with a timeout, and the screen
/// that asked is drawn while it runs. Nothing here signs anybody in.
#[tauri::command]
#[specta::specta]
pub async fn first_run_found() -> Result<FirstRunFound, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let harnesses = firstrun::harnesses_here()
            .into_iter()
            .map(|found| HarnessRow {
                name: found.harness.name().to_owned(),
                title: found.harness.title().to_owned(),
                installed: found.program.is_some(),
                signed_in: found.signed_in,
            })
            .collect();
        let kind = Kind::GitHub;
        let installed = charter_core::forge::find_cli(kind.cli()).is_some();
        let signed_in = installed && Forge::default_of(kind).check_auth().is_ok();
        FirstRunFound {
            harnesses,
            forge: ForgeRow {
                cli: kind.cli().to_owned(),
                installed,
                signed_in,
            },
        }
    })
    .await
    .map_err(|err| format!("charter could not look at this machine: {err}"))
}

/// Opens `path`, a repo, into this machine's local plane: the plane is made when there is
/// none, the repo is cloned into a workspace named after it, and the plane is opened
/// **through the trust gate**, exactly as `create_project` opens a plane it has just made.
///
/// Nothing asks where the plane goes (W10). The repo is read and never written to.
#[tauri::command]
#[specta::specta]
pub async fn open_repo(
    planes: tauri::State<'_, Planes>,
    path: String,
) -> Result<OpenedRepo, String> {
    let config = config_of(&planes)?;
    let (root, taken, harness) = tauri::async_runtime::spawn_blocking(move || {
        let (root, taken) = taken_in(&config, Path::new(&path))?;
        let harness = firstrun::only_ready(&firstrun::harnesses_here());
        Ok::<_, String>((root, taken, harness))
    })
    .await
    .map_err(|err| format!("charter could not open the repo: {err}"))??;
    let opened = planes.open_if_approved(&root).map(Opened::from)?;
    Ok(OpenedRepo {
        opened,
        workspace: taken.workspace,
        cwd: taken.clone.display().to_string(),
        harness: harness.map(|one| one.name().to_owned()),
    })
}

/// Opens this machine's local project with no repo in it, made first when there is none, and
/// through the trust gate: what "Sign in to GitHub" on the first run opens, so the sign-in has
/// a shell tab to run in (W10).
#[tauri::command]
#[specta::specta]
pub async fn open_local_project(planes: tauri::State<'_, Planes>) -> Result<Opened, String> {
    let config = config_of(&planes)?;
    let root = tauri::async_runtime::spawn_blocking(move || firstrun::ensure_local_plane(&config))
        .await
        .map_err(|err| format!("charter could not open its project: {err}"))??;
    planes.open_if_approved(&root).map(Opened::from)
}

/// Where the local project goes, or why this machine has nowhere to keep it.
fn config_of(planes: &Planes) -> Result<PathBuf, String> {
    planes.config().map(Path::to_path_buf).ok_or_else(|| {
        "charter has nowhere to keep a project on this machine. Open a project, or make one \
         under New project → Advanced."
            .to_owned()
    })
}

/// The local plane under `config`, made if it is not there, with `repo` taken in.
fn taken_in(config: &Path, repo: &Path) -> Result<(PathBuf, firstrun::TakenIn), String> {
    if !repo.is_absolute() {
        return Err(format!(
            "'{}' is not a full path, so charter cannot tell which directory it means. Pick a \
             folder, or type the whole path.",
            repo.display()
        ));
    }
    let root = firstrun::ensure_local_plane(config)?;
    let taken = firstrun::take_in(&root, repo)?;
    Ok((root, taken))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A repo with one commit, made from charter-core's git template so it never asks the
    /// developer's signer (charter-app#191) — `testgit`'s rule, which `extensions.rs`'s
    /// `test_plane` follows the same way from this crate.
    fn a_repo(at: &Path) -> PathBuf {
        let template = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../crates/charter-core/tests/support/git-template"
        );
        std::fs::create_dir_all(at).expect("the repo's directory");
        for argv in [
            vec![
                "init".to_owned(),
                "-q".to_owned(),
                "-b".to_owned(),
                "main".to_owned(),
                format!("--template={template}"),
                ".".to_owned(),
            ],
            vec!["config".into(), "user.email".into(), "t@e.invalid".into()],
            vec!["config".into(), "user.name".into(), "t".into()],
            vec![
                "commit".into(),
                "-q".into(),
                "--allow-empty".into(),
                "-m".into(),
                "first".into(),
            ],
        ] {
            let done = charter_core::forklock::output(
                std::process::Command::new("git")
                    .arg("-C")
                    .arg(at)
                    .args(&argv),
            )
            .expect("git runs in a test");
            assert!(done.status.success(), "git {argv:?}");
        }
        at.to_path_buf()
    }

    #[test]
    fn a_new_machine_gets_a_local_plane_with_the_repository_as_a_workspace_of_its_name() {
        let dir = tempfile::tempdir().expect("a directory");
        let config = dir.path().join("config");
        let repo = a_repo(&dir.path().join("widget"));

        let (root, taken) = taken_in(&config, &repo).expect("the repository is opened");

        assert_eq!(
            root,
            firstrun::local_plane(&config).canonicalize().expect("made")
        );
        assert_eq!(taken.workspace, "widget");
        assert_eq!(taken.clone, root.join("workspaces/widget/widget"));
    }

    #[test]
    fn a_second_repository_goes_into_the_same_local_plane() {
        let dir = tempfile::tempdir().expect("a directory");
        let config = dir.path().join("config");
        let (first, _) =
            taken_in(&config, &a_repo(&dir.path().join("one"))).expect("the first repository");

        let (second, taken) =
            taken_in(&config, &a_repo(&dir.path().join("two"))).expect("the second repository");

        assert_eq!(first, second);
        assert_eq!(taken.workspace, "two");
        assert!(first.join("workspaces/one/one/.git").exists());
    }

    #[test]
    fn a_path_that_is_not_a_full_one_is_refused_before_anything_is_made() {
        let dir = tempfile::tempdir().expect("a directory");
        let config = dir.path().join("config");

        let refused = taken_in(&config, Path::new("widget")).expect_err("refused");

        assert!(refused.contains("is not a full path"), "{refused}");
        assert!(!firstrun::local_plane(&config).exists());
    }
}
