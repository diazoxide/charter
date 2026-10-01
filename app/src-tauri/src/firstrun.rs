//! The first run's two commands (FR-4, #603): what this machine has, and a repository opened
//! into the local plane, laid out from the project template the operator chose (FR-17).
//!
//! The rules are `charter_core::firstrun`'s. This module is the window's door to them: it puts
//! the answers in the shape the window draws, keeps the slow parts off the thread that draws,
//! and opens the local plane through the same trust gate every other open goes through.

use std::path::{Path, PathBuf};

use charter_core::firstrun;
use charter_core::forge::{Forge, Kind};
use charter_core::repoinstructions::{self, Standing};

use crate::opener::Opened;
use crate::planes::{PlaneId, Planes};

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

/// One forge's CLI, as the first-run screen lists it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct ForgeRow {
    /// The program (`gh`, `glab`). Its own login is `<cli> auth login`.
    pub cli: String,
    /// The forge it works with (`GitHub`, `GitLab`).
    pub title: String,
    pub installed: bool,
    /// Whether it is logged in to its default host.
    pub signed_in: bool,
}

/// One project template, as the first-run screen offers it (FR-17).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct TemplateRow {
    /// What `open_repo` is asked for it by.
    pub id: String,
    /// What the screen calls it.
    pub title: String,
    /// One line on what it is for.
    pub summary: String,
}

/// What the first-run screen shows about this machine.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
pub struct FirstRunFound {
    pub harnesses: Vec<HarnessRow>,
    /// Every forge charter works with, GitHub first. The first run comes before any repo is
    /// chosen, so which forge the project will use is not known yet: both CLIs are checked.
    pub forges: Vec<ForgeRow>,
    /// The project templates this charter ships, in the order the screen lists them.
    pub templates: Vec<TemplateRow>,
}

/// Which project template the repo's project is laid out from: `charter_core::firstrun::Choice`
/// on the wire, which the core keeps free of serde and specta.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum TemplateChoice {
    /// The one that fits the repo, or none when none does. What the screen starts on.
    Fits,
    /// No template: the screen's *None*.
    NoTemplate,
    /// This one.
    Named { id: String },
}

impl From<TemplateChoice> for firstrun::Choice {
    fn from(choice: TemplateChoice) -> Self {
        match choice {
            TemplateChoice::Fits => Self::Fits,
            TemplateChoice::NoTemplate => Self::NoTemplate,
            TemplateChoice::Named { id } => Self::Named(id),
        }
    }
}

/// Every project template this charter ships.
fn templates() -> Vec<TemplateRow> {
    charter_core::template::all()
        .iter()
        .map(|one| TemplateRow {
            id: one.id.clone(),
            title: one.title.clone(),
            summary: one.summary.clone(),
        })
        .collect()
}

/// The forges the first run checks, in the order it lists them.
const FIRST_RUN_FORGES: [Kind; 2] = [Kind::GitHub, Kind::GitLab];

/// One row per forge in [`FIRST_RUN_FORGES`]. `installed` says whether a forge's CLI is found;
/// `signed_in` is asked only of an installed one.
fn forge_rows(installed: &dyn Fn(Kind) -> bool, signed_in: &dyn Fn(Kind) -> bool) -> Vec<ForgeRow> {
    FIRST_RUN_FORGES
        .iter()
        .map(|&kind| {
            let installed = installed(kind);
            ForgeRow {
                cli: kind.cli().to_owned(),
                title: kind.display().to_owned(),
                installed,
                signed_in: installed && signed_in(kind),
            }
        })
        .collect()
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
    /// How many of the repo's agent instruction files can be added to the workspace's memory
    /// (FR-18a): the window offers them in a tab beside the first chat when there are any.
    pub instructions: u32,
    /// The project template the project was laid out from, by id, when one was (FR-17).
    pub template: Option<String>,
}

/// One agent instruction file in a workspace's clone, as the tab that offers it draws it
/// (FR-18a, `charter_core::repoinstructions`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct InstructionFile {
    /// The clone's name in the workspace.
    pub repo: String,
    /// Its path inside the clone.
    pub file: String,
    /// Its whole text: the preview. Empty when it was left out before it was read.
    pub text: String,
    pub standing: InstructionStanding,
}

/// Whether a file can go into memory, as `repoinstructions::Standing` says.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum InstructionStanding {
    /// It can. `caution` is why its box starts unticked, when it does.
    Offered { caution: Option<String> },
    /// A memory already holds its text.
    InMemory,
    /// It cannot, and why.
    LeftOut { why: String },
}

/// One file the operator ticked, with the text the preview showed them: the wire's spelling of
/// `repoinstructions::Shown`, which the core keeps free of serde and specta.
#[derive(Debug, Clone, serde::Deserialize, specta::Type)]
pub struct ChosenInstruction {
    pub repo: String,
    pub file: String,
    pub text: String,
}

/// Which harnesses are installed and signed in, and whether `gh` and `glab` are logged in.
///
/// **On a blocking thread**: `gh auth status` and `glab auth status` are subprocesses with a
/// timeout, and the screen that asked is drawn while it runs. Nothing here signs anybody in.
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
        let forges = forge_rows(
            &|kind| charter_core::forge::find_cli(kind.cli()).is_some(),
            &|kind| Forge::default_of(kind).check_auth().is_ok(),
        );
        FirstRunFound {
            harnesses,
            forges,
            templates: templates(),
        }
    })
    .await
    .map_err(|err| format!("charter could not look at this machine: {err}"))
}

/// Opens `path`, a repo, into this machine's local plane: the plane is made when there is
/// none, laid out from the project template `template` names (FR-17), the repo is cloned into
/// a workspace named after it, and the plane is opened **through the trust gate**, exactly as
/// `create_project` opens a plane it has just made.
///
/// Nothing asks where the plane goes (W10). The repo is read and never written to.
#[tauri::command]
#[specta::specta]
pub async fn open_repo(
    planes: tauri::State<'_, Planes>,
    path: String,
    template: TemplateChoice,
) -> Result<OpenedRepo, String> {
    let config = config_of(&planes)?;
    let (root, taken, harness, instructions) = tauri::async_runtime::spawn_blocking(move || {
        let (root, taken) = taken_in(&config, Path::new(&path), &template.into())?;
        let harness = firstrun::only_ready(&firstrun::harnesses_here());
        // Counted, not written: the tab that offers them is where the operator says yes.
        let instructions = offered(&root, &taken.workspace);
        Ok::<_, String>((root, taken, harness, instructions))
    })
    .await
    .map_err(|err| format!("charter could not open the repo: {err}"))??;
    let opened = planes.open_if_approved(&root).map(Opened::from)?;
    Ok(OpenedRepo {
        opened,
        workspace: taken.workspace,
        cwd: taken.clone.display().to_string(),
        harness: harness.map(|one| one.name().to_owned()),
        instructions,
        template: taken.template,
    })
}

/// The project template that fits the repo at `path`, by id, or `null` when none does or `path`
/// is not a full path to a directory: what the first run's "Fits the repo" says it will pick
/// (FR-17). It asks only whether files are there, and reads nothing.
#[tauri::command]
#[specta::specta]
pub async fn template_that_fits(path: String) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || fits(&path))
        .await
        .map_err(|err| format!("charter could not look at the repo: {err}"))
}

fn fits(path: &str) -> Option<String> {
    let repo = Path::new(path);
    if !repo.is_absolute() || !repo.is_dir() {
        return None;
    }
    charter_core::template::detect(repo).map(|one| one.id.clone())
}

/// How many of workspace `ws`'s instruction files can be added to its memory. A workspace
/// charter could not read offers none: the first chat still starts.
fn offered(root: &Path, ws: &str) -> u32 {
    repoinstructions::found(root, ws).map_or(0, |found| {
        let count = found
            .iter()
            .filter(|one| matches!(one.standing, Standing::Offered { .. }))
            .count();
        u32::try_from(count).unwrap_or(u32::MAX)
    })
}

/// The agent instruction files in workspace `workspace`'s clones, each with its whole text:
/// the preview the import tab draws (FR-18a). Reads, and writes nothing.
#[tauri::command]
#[specta::specta]
pub async fn repo_instructions(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    workspace: String,
) -> Result<Vec<InstructionFile>, String> {
    let root = planes.held(&plane)?.root().to_path_buf();
    tauri::async_runtime::spawn_blocking(move || instruction_files(&root, &workspace))
        .await
        .map_err(|err| format!("charter could not read the repo's instructions: {err}"))?
}

/// Adds the files the operator ticked to workspace `workspace`'s memory — the preview's yes
/// (FR-18a). Each must still hold the text the preview showed, or nothing is written.
#[tauri::command]
#[specta::specta]
pub async fn import_instructions(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    workspace: String,
    chosen: Vec<ChosenInstruction>,
) -> Result<u32, String> {
    let root = planes.held(&plane)?.root().to_path_buf();
    tauri::async_runtime::spawn_blocking(move || {
        imported(
            &root,
            &workspace,
            &chosen,
            chrono::Local::now().naive_local(),
        )
    })
    .await
    .map_err(|err| format!("charter could not add them to memory: {err}"))?
}

fn instruction_files(root: &Path, workspace: &str) -> Result<Vec<InstructionFile>, String> {
    Ok(repoinstructions::found(root, workspace)?
        .into_iter()
        .map(|one| InstructionFile {
            repo: one.shown.repo,
            file: one.shown.file,
            text: one.shown.text,
            standing: match one.standing {
                Standing::Offered { caution } => InstructionStanding::Offered { caution },
                Standing::InMemory => InstructionStanding::InMemory,
                Standing::LeftOut(why) => InstructionStanding::LeftOut { why },
            },
        })
        .collect())
}

fn imported(
    root: &Path,
    workspace: &str,
    chosen: &[ChosenInstruction],
    stamp: chrono::NaiveDateTime,
) -> Result<u32, String> {
    let chosen: Vec<repoinstructions::Shown> = chosen
        .iter()
        .map(|one| repoinstructions::Shown {
            repo: one.repo.clone(),
            file: one.file.clone(),
            text: one.text.clone(),
        })
        .collect();
    let written = repoinstructions::import(root, workspace, &chosen, stamp)?;
    Ok(u32::try_from(written).unwrap_or(u32::MAX))
}

/// Opens this machine's local project with no repo in it, made first when there is none, and
/// through the trust gate: what "Sign in to GitHub" (or GitLab) on the first run opens, so the
/// sign-in has a shell tab to run in (W10).
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

/// The local plane under `config`, made if it is not there, with `repo` taken in and the
/// project laid out from the template `choice` names.
fn taken_in(
    config: &Path,
    repo: &Path,
    choice: &firstrun::Choice,
) -> Result<(PathBuf, firstrun::TakenIn), String> {
    if !repo.is_absolute() {
        return Err(format!(
            "'{}' is not a full path, so charter cannot tell which directory it means. Pick a \
             folder, or type the whole path.",
            repo.display()
        ));
    }
    let root = firstrun::ensure_local_plane(config)?;
    let taken = firstrun::take_in_from(&root, repo, choice)?;
    Ok((root, taken))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_run_checks_both_forge_clis_and_asks_only_an_installed_one_for_its_login() {
        let asked = std::cell::RefCell::new(Vec::new());
        let rows = forge_rows(&|kind| kind == Kind::GitLab, &|kind| {
            asked.borrow_mut().push(kind);
            true
        });

        let row = |cli: &str, title: &str, installed, signed_in| ForgeRow {
            cli: cli.to_owned(),
            title: title.to_owned(),
            installed,
            signed_in,
        };
        assert_eq!(
            rows,
            [
                row("gh", "GitHub", false, false),
                row("glab", "GitLab", true, true),
            ]
        );
        assert_eq!(
            *asked.borrow(),
            [Kind::GitLab],
            "gh is not installed, so not asked"
        );
    }

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
    fn the_window_names_a_template_by_its_kind_and_id() {
        let said = |json: &str| {
            firstrun::Choice::from(
                serde_json::from_str::<TemplateChoice>(json).expect("the window's spelling"),
            )
        };

        assert_eq!(said(r#"{"kind":"fits"}"#), firstrun::Choice::Fits);
        assert_eq!(
            said(r#"{"kind":"no-template"}"#),
            firstrun::Choice::NoTemplate
        );
        assert_eq!(
            said(r#"{"kind":"named","id":"rust"}"#),
            firstrun::Choice::Named("rust".to_owned())
        );
    }

    #[test]
    fn the_first_run_names_the_template_that_fits_a_typed_path() {
        let dir = tempfile::tempdir().expect("a directory");
        std::fs::write(dir.path().join("go.mod"), "").expect("a marker");

        assert_eq!(
            fits(&dir.path().display().to_string()).as_deref(),
            Some("go")
        );
        assert_eq!(
            fits("widget"),
            None,
            "a path that is not a full one names nothing"
        );
        assert_eq!(fits(&dir.path().join("gone").display().to_string()), None);
    }

    #[test]
    fn the_first_run_offers_every_template_charter_ships() {
        let offered: Vec<String> = templates().into_iter().map(|row| row.id).collect();

        assert_eq!(
            offered,
            ["docs", "go", "monorepo", "python", "rust", "typescript"].map(str::to_owned)
        );
    }

    #[test]
    fn a_repo_opened_on_the_first_run_is_laid_out_from_the_template_it_was_given() {
        let dir = tempfile::tempdir().expect("a directory");
        let config = dir.path().join("config");
        let repo = a_repo(&dir.path().join("widget"));

        let (root, taken) =
            taken_in(&config, &repo, &firstrun::Choice::Named("go".to_owned())).expect("opened");

        assert_eq!(taken.template.as_deref(), Some("go"));
        assert!(root.join("personas/go-reviewer/refs/REVIEW.md").is_file());
    }

    #[test]
    fn a_new_machine_gets_a_local_plane_with_the_repository_as_a_workspace_of_its_name() {
        let dir = tempfile::tempdir().expect("a directory");
        let config = dir.path().join("config");
        let repo = a_repo(&dir.path().join("widget"));

        let (root, taken) =
            taken_in(&config, &repo, &firstrun::Choice::Fits).expect("the repository is opened");

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
        let (first, _) = taken_in(
            &config,
            &a_repo(&dir.path().join("one")),
            &firstrun::Choice::Fits,
        )
        .expect("the first repository");

        let (second, taken) = taken_in(
            &config,
            &a_repo(&dir.path().join("two")),
            &firstrun::Choice::Fits,
        )
        .expect("the second repository");

        assert_eq!(first, second);
        assert_eq!(taken.workspace, "two");
        assert!(first.join("workspaces/one/one/.git").exists());
    }

    #[test]
    fn a_path_that_is_not_a_full_one_is_refused_before_anything_is_made() {
        let dir = tempfile::tempdir().expect("a directory");
        let config = dir.path().join("config");

        let refused =
            taken_in(&config, Path::new("widget"), &firstrun::Choice::Fits).expect_err("refused");

        assert!(refused.contains("is not a full path"), "{refused}");
        assert!(!firstrun::local_plane(&config).exists());
    }

    #[test]
    fn a_repo_opened_on_the_first_run_counts_its_instructions_and_adds_them_only_when_asked() {
        let dir = tempfile::tempdir().expect("a directory");
        let config = dir.path().join("config");
        let repo = a_repo(&dir.path().join("widget"));
        std::fs::write(repo.join("AGENTS.md"), "Run make check.\n").expect("instructions");
        for argv in [
            ["add", "AGENTS.md"].as_slice(),
            &["commit", "-q", "-m", "agents"],
        ] {
            let done = charter_core::forklock::output(
                std::process::Command::new("git")
                    .arg("-C")
                    .arg(&repo)
                    .args(argv),
            )
            .expect("git runs in a test");
            assert!(done.status.success(), "git {argv:?}");
        }
        let (root, taken) = taken_in(&config, &repo, &firstrun::Choice::Fits).expect("opened");

        assert_eq!(offered(&root, &taken.workspace), 1);
        let files = instruction_files(&root, &taken.workspace).expect("read");
        assert_eq!(
            files,
            vec![InstructionFile {
                repo: "widget".into(),
                file: "AGENTS.md".into(),
                text: "Run make check.\n".into(),
                standing: InstructionStanding::Offered { caution: None },
            }]
        );
        let workspace = charter_core::workspaces::Plane::open(&root)
            .workspace("widget")
            .expect("the workspace");
        assert!(workspace.memories().unwrap_or_default().is_empty());

        let chosen = [ChosenInstruction {
            repo: "widget".into(),
            file: "AGENTS.md".into(),
            text: "Run make check.\n".into(),
        }];
        let stamp = chrono::NaiveDate::from_ymd_opt(2026, 10, 1)
            .and_then(|d| d.and_hms_opt(9, 0, 0))
            .expect("a time");
        assert_eq!(imported(&root, "widget", &chosen, stamp), Ok(1));

        assert_eq!(workspace.memories().expect("read").len(), 1);
        assert_eq!(offered(&root, &taken.workspace), 0);
        assert_eq!(
            instruction_files(&root, "widget").expect("read")[0].standing,
            InstructionStanding::InMemory
        );
    }
}
