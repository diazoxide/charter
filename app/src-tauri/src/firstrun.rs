//! The first run's two commands (FR-4, #603): what this machine has, and a repository opened
//! into the local plane, laid out from the project template the operator chose (FR-17).
//!
//! The rules are `purlis_core::firstrun`'s. This module is the window's door to them: it puts
//! the answers in the shape the window draws, keeps the slow parts off the thread that draws,
//! and opens the local plane through the same trust gate every other open goes through.

use std::path::{Path, PathBuf};

use purlis_core::firstrun;
use purlis_core::forge::{Forge, Kind};
use purlis_core::noharness;
use purlis_core::repoinstructions::{self, Standing};

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
    /// Its vendor's own installer, word for word, which the harness setup tab types into a
    /// shell tab when the operator presses Install (FR-29).
    pub installer: String,
    /// The vendor's install page the command is from.
    pub installer_page: String,
}

/// A local model server found answering on this machine (FR-29's local model fallback).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct LocalModelRow {
    /// What the screen calls it (`Ollama`, `LM Studio`).
    pub title: String,
    /// The base URL a harness is pointed at.
    pub base_url: String,
    /// The harness that can use it with no account (`opencode`).
    pub harness: String,
}

/// One forge's CLI, as the first-run screen lists it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct ForgeRow {
    /// The program (`gh`, `glab`). Its own login is `<cli> auth login`.
    pub cli: String,
    /// The forge: what its sign-in names when it makes the project (#839).
    pub forge: ForgeWord,
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
    /// The local model servers answering on this machine: the fallback for somebody with no
    /// harness account (FR-29).
    pub local_models: Vec<LocalModelRow>,
}

/// One row per harness, as the first run and the harness setup tab list them.
fn harness_rows(found: &[firstrun::HarnessFound]) -> Vec<HarnessRow> {
    found
        .iter()
        .map(|found| {
            let installer = noharness::installer(found.harness);
            HarnessRow {
                name: found.harness.name().to_owned(),
                title: found.harness.title().to_owned(),
                installed: found.program.is_some(),
                signed_in: found.signed_in,
                installer: installer.command.to_owned(),
                installer_page: installer.page.to_owned(),
            }
        })
        .collect()
}

/// One row per local model server that answered.
fn local_model_rows(answered: &[noharness::LocalServer]) -> Vec<LocalModelRow> {
    answered
        .iter()
        .map(|server| LocalModelRow {
            title: server.title.to_owned(),
            base_url: server.base_url.to_owned(),
            harness: noharness::LOCAL_MODEL_HARNESS.name().to_owned(),
        })
        .collect()
}

/// Which project template the repo's project is laid out from: `purlis_core::firstrun::Choice`
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
    purlis_core::template::all()
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
                forge: kind.into(),
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
    /// Whether no harness is installed on this machine: the first chat is then the harness
    /// setup tab, with each one's official installer, instead of the picker (FR-29).
    pub none_installed: bool,
    /// How many of the repo's agent instruction files can be added to the workspace's memory
    /// (FR-18a): the window offers them in a tab beside the first chat when there are any.
    pub instructions: u32,
    /// The project template the project was laid out from, by id, when one was (FR-17).
    pub template: Option<String>,
}

/// What opening a repo on the first run came to: opened, or a question about the forge.
///
/// Two nullable fields rather than a tagged union, as `Opened` is: exactly one is ever set.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
pub struct RepoAnswer {
    /// What was opened. Null when the forge has to be asked first.
    pub opened: Option<OpenedRepo>,
    /// Why the repo's remote does not say which forge the project's repos are on (#839):
    /// the window asks GitHub or GitLab and calls again with the answer. Nothing was made.
    pub asks_forge: Option<String>,
}

/// One agent instruction file in a workspace's clone, as the tab that offers it draws it
/// (FR-18a, `purlis_core::repoinstructions`).
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

/// What the harness setup tab shows (FR-29): the harnesses and the local model servers, and
/// not the forge CLIs, whose sign-in checks are subprocesses the tab has no use for.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
pub struct HarnessSetupFound {
    pub harnesses: Vec<HarnessRow>,
    pub local_models: Vec<LocalModelRow>,
}

/// Which harnesses are installed and signed in, and which local model servers answer: the
/// harness setup tab's look at the machine, and its Check again (FR-29). On a blocking thread,
/// for the local probes.
#[tauri::command]
#[specta::specta]
pub async fn harness_setup_found() -> Result<HarnessSetupFound, String> {
    tauri::async_runtime::spawn_blocking(|| HarnessSetupFound {
        harnesses: harness_rows(&firstrun::harnesses_here()),
        local_models: local_model_rows(&noharness::local_models_here()),
    })
    .await
    .map_err(|err| format!("purlis could not look at this machine: {err}"))
}

/// Types harness `harness`'s official installer into shell session `session`, and runs it
/// (FR-29, ruling V65).
///
/// **The window names the harness, never the command.** The line is charter's own,
/// compiled in (`purlis_core::noharness::installer`) and shown word for word on the tab
/// before the press, so no text from a project, a plane or the window can reach the shell
/// through here. A word that is not a harness charter starts is refused, and nothing is typed.
#[tauri::command]
#[specta::specta]
pub fn type_installer(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    session: u32,
    harness: String,
) -> Result<(), String> {
    let line = installer_line(&harness)?;
    planes
        .held(&plane)?
        .operator_input(session, line.as_bytes())
}

/// The line [`type_installer`] types for `harness`: its installer and a newline.
fn installer_line(harness: &str) -> Result<String, String> {
    let Some(harness) = purlis_core::harness::Harness::of_kind(harness) else {
        return Err(format!(
            "{harness:?} is not a harness purlis starts, so there is no installer to type"
        ));
    };
    Ok(format!("{}\n", noharness::installer(harness).command))
}

/// Which harnesses are installed and signed in, and whether `gh` and `glab` are logged in.
///
/// **On a blocking thread**: `gh auth status` and `glab auth status` are subprocesses with a
/// timeout, and the screen that asked is drawn while it runs. Nothing here signs anybody in.
#[tauri::command]
#[specta::specta]
pub async fn first_run_found() -> Result<FirstRunFound, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let harnesses = harness_rows(&firstrun::harnesses_here());
        let forges = forge_rows(
            &|kind| purlis_core::forge::find_cli(kind.cli()).is_some(),
            &|kind| Forge::default_of(kind).check_auth().is_ok(),
        );
        FirstRunFound {
            harnesses,
            forges,
            templates: templates(),
            local_models: local_model_rows(&noharness::local_models_here()),
        }
    })
    .await
    .map_err(|err| format!("purlis could not look at this machine: {err}"))
}

/// Opens `path`, a repo, into this machine's local plane: the plane is made when there is
/// none, laid out from the project template `template` names (FR-17), the repo is cloned into
/// a workspace named after it, and the plane is opened **through the trust gate**, exactly as
/// `create_project` opens a plane it has just made.
///
/// Nothing asks where the plane goes (W10). The repo is read and never written to.
///
/// The project's forge is read from the repo's `origin`, as `charter init` reads it (#839).
/// When the remote does not say, nothing is made and the answer asks for it; `forge` is the
/// operator's answer (`github`, `gitlab`) on the call after. A local project that is already
/// there asks nothing.
#[tauri::command]
#[specta::specta]
pub async fn open_repo(
    planes: tauri::State<'_, Planes>,
    path: String,
    template: TemplateChoice,
    forge: Option<ForgeWord>,
) -> Result<RepoAnswer, String> {
    let data = data_home()?;
    let forge = forge.map(Kind::from);
    let made = tauri::async_runtime::spawn_blocking(move || {
        let (root, taken) = match taken_in(&data, Path::new(&path), &template.into(), forge)? {
            Taken::In(root, taken) => (root, taken),
            Taken::AsksForge(why) => return Ok(Err(why)),
        };
        let here = firstrun::harnesses_here();
        let harness = firstrun::only_ready(&here);
        let none_installed = noharness::none_installed(&here);
        // Counted, not written: the tab that offers them is where the operator says yes.
        let instructions = offered(&root, &taken.workspace);
        Ok::<_, String>(Ok((root, taken, harness, none_installed, instructions)))
    })
    .await
    .map_err(|err| format!("purlis could not open the repo: {err}"))??;
    let (root, taken, harness, none_installed, instructions) = match made {
        Ok(made) => made,
        Err(why) => {
            return Ok(RepoAnswer {
                opened: None,
                asks_forge: Some(why),
            });
        }
    };
    let opened = planes.open_if_approved(&root).map(Opened::from)?;
    Ok(RepoAnswer {
        opened: Some(OpenedRepo {
            opened,
            workspace: taken.workspace,
            cwd: taken.clone.display().to_string(),
            harness: harness.map(|one| one.name().to_owned()),
            none_installed,
            instructions,
            template: taken.template,
        }),
        asks_forge: None,
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
        .map_err(|err| format!("purlis could not look at the repo: {err}"))
}

fn fits(path: &str) -> Option<String> {
    let repo = Path::new(path);
    if !repo.is_absolute() || !repo.is_dir() {
        return None;
    }
    purlis_core::template::detect(repo).map(|one| one.id.clone())
}

/// A forge on the wire: the window's spelling of `purlis_core::forge::Kind`, which the core
/// keeps free of serde and specta.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum ForgeWord {
    Github,
    Gitlab,
}

impl From<ForgeWord> for Kind {
    fn from(word: ForgeWord) -> Kind {
        match word {
            ForgeWord::Github => Kind::GitHub,
            ForgeWord::Gitlab => Kind::GitLab,
        }
    }
}

impl From<Kind> for ForgeWord {
    fn from(kind: Kind) -> ForgeWord {
        match kind {
            Kind::GitHub => ForgeWord::Github,
            Kind::GitLab => ForgeWord::Gitlab,
        }
    }
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
        .map_err(|err| format!("purlis could not read the repo's instructions: {err}"))?
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
    .map_err(|err| format!("purlis could not add them to memory: {err}"))?
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
/// through the trust gate: what "Sign in to GitHub" or "Sign in to GitLab" on the first run
/// opens, so the sign-in has a shell tab to run in (W10).
///
/// `forge` is the forge whose sign-in was pressed (`github`, `gitlab`). With no repo to read,
/// that press is the operator's answer to which forge a new project tracks (#839).
#[tauri::command]
#[specta::specta]
pub async fn open_local_project(
    planes: tauri::State<'_, Planes>,
    forge: ForgeWord,
) -> Result<Opened, String> {
    let data = data_home()?;
    let kind = Kind::from(forge);
    let root = tauri::async_runtime::spawn_blocking(move || {
        firstrun::ensure_local_plane(&data, firstrun::ForgeFrom::Named(kind))
            .map_err(|why| why.to_string())
    })
    .await
    .map_err(|err| format!("purlis could not open its project: {err}"))??;
    planes.open_if_approved(&root).map(Opened::from)
}

/// Where the local project goes: purlis's data home, never the config home a chat's sandbox
/// denies (#1670); or why this machine has nowhere to keep it.
fn data_home() -> Result<PathBuf, String> {
    purlis_core::datahome::root().ok_or_else(|| {
        "purlis has nowhere to keep a project on this machine. Open a project, or make one \
         under New project → Advanced."
            .to_owned()
    })
}

/// The local project moved out of the config home at the app's launch, where an older purlis
/// made it (`purlis_core::localproject`), and what came of it said once, in the app's log. A
/// project left where it is is said again by the doctor's `local project` row when it opens.
pub fn move_the_local_project() {
    match purlis_core::localproject::at_launch() {
        None => {}
        Some(moved @ purlis_core::localproject::Moved::Moved { .. }) => {
            tracing::info!("purlis: {moved}");
        }
        Some(moved) => tracing::warn!("purlis: {moved}"),
    }
}

/// What [`taken_in`] came to.
#[derive(Debug)]
enum Taken {
    /// The local plane, with the repo in it.
    In(PathBuf, firstrun::TakenIn),
    /// The forge has to be asked first, and why. Nothing was made.
    AsksForge(String),
}

/// The local plane in the data home `data`, made if it is not there, with `repo` taken in and the
/// project laid out from the template `choice` names. A plane that is made takes its forge
/// from `forge`, the operator's answer, else from `repo`'s remote; a plane that is there adds
/// the forge of a self-managed remote by that answer (#1669).
fn taken_in(
    data: &Path,
    repo: &Path,
    choice: &firstrun::Choice,
    forge: Option<Kind>,
) -> Result<Taken, String> {
    if !repo.is_absolute() {
        return Err(format!(
            "'{}' is not a full path, so purlis cannot tell which directory it means. Pick a \
             folder, or type the whole path.",
            repo.display()
        ));
    }
    // An answer was asked for by this repo's remote: its host is carried over with it (#881).
    let from = forge.map_or(firstrun::ForgeFrom::Repo(repo), |kind| {
        firstrun::ForgeFrom::Answered { kind, repo }
    });
    let root = match firstrun::ensure_local_plane(data, from) {
        Ok(root) => root,
        Err(firstrun::NotMade::AsksForForge(why)) => return Ok(Taken::AsksForge(why)),
        Err(firstrun::NotMade::Refused(why)) => return Err(why),
    };
    // A project that is there asks the same question of a self-managed repo (#1669), before
    // anything is copied; the answer adds the forge on the remote's host.
    if forge.is_none()
        && let Some(why) = firstrun::forge_question(&root, repo)
    {
        return Ok(Taken::AsksForge(why));
    }
    let taken = firstrun::take_in_as(&root, repo, choice, forge)?;
    // The repo is in whatever came of its forge; a forge that could not be added is the
    // operator's to add in Settings › Project › Forges, and the window has no field to say it.
    if let firstrun::ForgeTaken::NotAdded(why) = &taken.forge {
        tracing::warn!(
            "purlis: the forge of {} was not added ({why})",
            repo.display()
        );
    }
    Ok(Taken::In(root, taken))
}

#[cfg(test)]
mod tests {
    use super::*;

    const GITHUB: Option<Kind> = Some(Kind::GitHub);

    /// The plane and the repo taken in, from a run that was not asked a question.
    fn taken(answer: Taken) -> (PathBuf, firstrun::TakenIn) {
        match answer {
            Taken::In(root, taken) => (root, taken),
            Taken::AsksForge(why) => panic!("asked for the forge: {why}"),
        }
    }

    /// #839: a repo whose remote does not say asks for the forge, makes nothing, and the
    /// answer makes the project; a repo on github.com or gitlab.com is never asked about.
    #[test]
    fn a_repo_whose_remote_does_not_say_asks_for_the_forge_before_anything_is_made() {
        let dir = tempfile::tempdir().expect("a directory");
        let data = dir.path().join("data");
        let repo = a_repo(&dir.path().join("widget"));

        let asked = taken_in(&data, &repo, &firstrun::Choice::Fits, None).expect("answered");

        assert!(
            matches!(&asked, Taken::AsksForge(why) if why.ends_with("has no `origin` remote")),
            "{asked:?}"
        );
        assert!(!firstrun::local_plane(&data).exists());
        let (root, _) = taken(
            taken_in(&data, &repo, &firstrun::Choice::Fits, Some(Kind::GitLab)).expect("answered"),
        );
        let manifest = std::fs::read_to_string(root.join("charter.toml")).expect("made");
        assert!(manifest.contains("kind = \"gitlab\""), "{manifest}");
    }

    #[test]
    fn a_repo_on_github_names_the_projects_forge_and_owner_without_asking() {
        let dir = tempfile::tempdir().expect("a directory");
        let data = dir.path().join("data");
        let repo = a_repo(&dir.path().join("widget"));
        with_origin(&repo, "https://github.com/acme/widget.git");

        let (root, _) =
            taken(taken_in(&data, &repo, &firstrun::Choice::Fits, None).expect("answered"));

        let manifest = std::fs::read_to_string(root.join("charter.toml")).expect("made");
        assert!(manifest.contains("kind = \"github\""), "{manifest}");
        assert!(manifest.contains("owner = \"acme\""), "{manifest}");
        assert!(
            !manifest.contains("host ="),
            "github.com wrote a host: {manifest}"
        );
    }

    /// Points the repo at `repo`'s `origin` at `url`.
    fn with_origin(repo: &Path, url: &str) {
        let added = purlis_core::forklock::output(
            std::process::Command::new("git")
                .arg("-C")
                .arg(repo)
                .args(["remote", "add", "origin", url]),
        )
        .expect("git runs in a test");
        assert!(added.status.success(), "git remote add origin {url}");
    }

    /// #881: a self-managed remote asks for the forge's kind, and the answer's project keeps
    /// the remote's host, and the owner its path names, without either being typed.
    #[test]
    fn a_self_managed_remote_named_gitlab_makes_a_project_on_its_host() {
        let dir = tempfile::tempdir().expect("a directory");
        let data = dir.path().join("data");
        let repo = a_repo(&dir.path().join("widget"));
        with_origin(&repo, "git@git.example.com:platform/widget.git");

        let asked = taken_in(&data, &repo, &firstrun::Choice::Fits, None).expect("answered");
        assert!(matches!(asked, Taken::AsksForge(_)), "{asked:?}");
        let (root, _) = taken(
            taken_in(&data, &repo, &firstrun::Choice::Fits, Some(Kind::GitLab)).expect("answered"),
        );

        let cfg = purlis_core::forge::load_config(&root).expect("charter.toml reads");
        let forges = purlis_core::forge::to_query(&cfg).expect("the forges read");
        assert_eq!(forges.len(), 1, "{forges:?}");
        let (forge, owner, _) = &forges[0];
        assert_eq!(forge.kind, Kind::GitLab);
        assert_eq!(forge.host, "git.example.com");
        assert_eq!(owner, "platform");
    }

    /// #1669: a self-managed repo opened into the first-run project that is already there asks
    /// for its forge's kind, copies nothing until it is answered, and the answer adds a block on
    /// the remote's host.
    #[test]
    fn a_self_managed_repo_opened_into_the_project_that_is_there_asks_first() {
        let dir = tempfile::tempdir().expect("a directory");
        let data = dir.path().join("data");
        let site = a_repo(&dir.path().join("site"));
        with_origin(&site, "https://github.com/acme/site.git");
        let (root, _) =
            taken(taken_in(&data, &site, &firstrun::Choice::NoTemplate, None).expect("answered"));
        let repo = a_repo(&dir.path().join("widget"));
        with_origin(&repo, "git@git.example.com:platform/widget.git");

        let asked = taken_in(&data, &repo, &firstrun::Choice::NoTemplate, None).expect("answered");
        assert!(
            matches!(&asked, Taken::AsksForge(why) if why.contains("git.example.com")),
            "{asked:?}"
        );
        assert!(
            !root.join("workspaces").join("widget").exists(),
            "asking copied the repo"
        );

        let (_, taken_in_after) = taken(
            taken_in(
                &data,
                &repo,
                &firstrun::Choice::NoTemplate,
                Some(Kind::GitLab),
            )
            .expect("answered"),
        );
        assert!(
            matches!(&taken_in_after.forge, firstrun::ForgeTaken::Added(found) if found.kind == Kind::GitLab),
            "{:?}",
            taken_in_after.forge
        );
        let cfg = purlis_core::forge::load_config(&root).expect("charter.toml reads");
        let hosts: Vec<String> = purlis_core::forge::to_query(&cfg)
            .expect("the forges read")
            .into_iter()
            .map(|(forge, _, _)| forge.host)
            .collect();
        assert!(hosts.contains(&"git.example.com".to_owned()), "{hosts:?}");
        assert_eq!(taken_in_after.workspace, "widget");
        assert!(
            root.join("workspaces").join("widget").exists(),
            "answered, it is copied"
        );
    }

    /// #880: the first run's one project tracks the forge of every repo taken into it, so the
    /// repo picker and `discover` (which both read `forge::to_query`) see both forges, and a
    /// second repo on a forge already tracked adds nothing.
    #[test]
    fn a_repo_on_a_second_forge_adds_that_forge_to_the_first_run_project_once() {
        let dir = tempfile::tempdir().expect("a directory");
        let data = dir.path().join("data");
        let on = |name: &str, url: &str| {
            let repo = a_repo(&dir.path().join(name));
            with_origin(&repo, url);
            repo
        };
        let take = |repo: &Path| {
            taken(taken_in(&data, repo, &firstrun::Choice::NoTemplate, None).expect("answered"))
        };

        let (root, first) = take(&on("site", "https://github.com/acme/site.git"));
        let (_, second) = take(&on("api", "git@gitlab.com:platform/api.git"));
        let (_, third) = take(&on("web", "https://gitlab.com/platform/web.git"));

        assert_eq!(first.forge, firstrun::ForgeTaken::Tracked);
        assert!(
            matches!(&second.forge, firstrun::ForgeTaken::Added(found) if found.kind == Kind::GitLab),
            "{:?}",
            second.forge
        );
        assert_eq!(third.forge, firstrun::ForgeTaken::Tracked);
        let cfg = purlis_core::forge::load_config(&root).expect("charter.toml reads");
        let seen: Vec<(Kind, String)> = purlis_core::forge::to_query(&cfg)
            .expect("the forges read")
            .into_iter()
            .map(|(forge, owner, _)| (forge.kind, owner))
            .collect();
        assert_eq!(
            seen,
            [
                (Kind::GitHub, "acme".to_owned()),
                (Kind::GitLab, "platform".to_owned())
            ]
        );
    }

    #[test]
    fn the_first_run_checks_both_forge_clis_and_asks_only_an_installed_one_for_its_login() {
        let asked = std::cell::RefCell::new(Vec::new());
        let rows = forge_rows(&|kind| kind == Kind::GitLab, &|kind| {
            asked.borrow_mut().push(kind);
            true
        });

        let row = |cli: &str, title: &str, installed, signed_in| ForgeRow {
            cli: cli.to_owned(),
            forge: if cli == "gh" {
                ForgeWord::Github
            } else {
                ForgeWord::Gitlab
            },
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

    /// FR-29: a harness that is not installed is listed with its vendor's own installer, so
    /// the setup tab can type it into a shell tab.
    #[test]
    fn every_harness_row_carries_its_vendors_installer() {
        let found = [firstrun::HarnessFound {
            harness: purlis_core::harness::Harness::Opencode,
            program: None,
            signed_in: false,
        }];

        let rows = harness_rows(&found);

        assert_eq!(
            rows[0].installer,
            "curl -fsSL https://opencode.ai/install | bash"
        );
        assert_eq!(rows[0].installer_page, "https://opencode.ai/docs/");
        assert!(!rows[0].installed);
    }

    /// FR-29's local model fallback: a server that answered is named with the harness that
    /// can use it with no account.
    #[test]
    fn a_local_model_server_that_answered_is_offered_to_opencode() {
        let rows = local_model_rows(&[noharness::LOCAL_SERVERS[0]]);

        assert_eq!(
            rows,
            [LocalModelRow {
                title: "Ollama".to_owned(),
                base_url: "http://127.0.0.1:11434/v1".to_owned(),
                harness: "opencode".to_owned(),
            }]
        );
    }

    /// V65: the line typed is the compiled-in installer of the harness named, and nothing
    /// else is ever typed.
    #[test]
    fn the_installer_typed_is_the_compiled_in_one_for_the_harness_named() {
        assert_eq!(
            installer_line("codex").as_deref(),
            Ok("curl -fsSL https://chatgpt.com/codex/install.sh | sh\n")
        );
        assert_eq!(
            installer_line("claude").as_deref(),
            Ok("curl -fsSL https://claude.ai/install.sh | bash\n")
        );
        let refused = installer_line("rm -rf ~").expect_err("not a harness");
        assert!(refused.contains("not a harness"), "{refused}");
    }

    /// A repo with one commit, made from purlis-core's git template so it never asks the
    /// developer's signer (charter-app#191) — `testgit`'s rule, which `extensions.rs`'s
    /// `test_plane` follows the same way from this crate.
    fn a_repo(at: &Path) -> PathBuf {
        let template = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../crates/purlis-core/tests/support/git-template"
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
            let done = purlis_core::forklock::output(
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
        let data = dir.path().join("data");
        let repo = a_repo(&dir.path().join("widget"));

        let (root, taken) = taken_in(
            &data,
            &repo,
            &firstrun::Choice::Named("go".to_owned()),
            GITHUB,
        )
        .map(taken)
        .expect("opened");

        assert_eq!(taken.template.as_deref(), Some("go"));
        assert!(root.join("personas/go-reviewer/refs/REVIEW.md").is_file());
    }

    #[test]
    fn a_new_machine_gets_a_local_plane_with_the_repository_as_a_workspace_of_its_name() {
        let dir = tempfile::tempdir().expect("a directory");
        let data = dir.path().join("data");
        let repo = a_repo(&dir.path().join("widget"));

        let (root, taken) = taken_in(&data, &repo, &firstrun::Choice::Fits, GITHUB)
            .map(taken)
            .expect("the repository is opened");

        assert_eq!(
            root,
            firstrun::local_plane(&data).canonicalize().expect("made")
        );
        assert_eq!(taken.workspace, "widget");
        assert_eq!(taken.clone, root.join("workspaces/widget/widget"));
    }

    #[test]
    fn a_second_repository_goes_into_the_same_local_plane() {
        let dir = tempfile::tempdir().expect("a directory");
        let data = dir.path().join("data");
        let (first, _) = taken_in(
            &data,
            &a_repo(&dir.path().join("one")),
            &firstrun::Choice::Fits,
            GITHUB,
        )
        .map(taken)
        .expect("the first repository");

        let (second, taken) = taken_in(
            &data,
            &a_repo(&dir.path().join("two")),
            &firstrun::Choice::Fits,
            GITHUB,
        )
        .map(taken)
        .expect("the second repository");

        assert_eq!(first, second);
        assert_eq!(taken.workspace, "two");
        assert!(first.join("workspaces/one/one/.git").exists());
    }

    #[test]
    fn a_path_that_is_not_a_full_one_is_refused_before_anything_is_made() {
        let dir = tempfile::tempdir().expect("a directory");
        let data = dir.path().join("data");

        let refused = taken_in(&data, Path::new("widget"), &firstrun::Choice::Fits, GITHUB)
            .expect_err("refused");

        assert!(refused.contains("is not a full path"), "{refused}");
        assert!(!firstrun::local_plane(&data).exists());
    }

    #[test]
    fn a_repo_opened_on_the_first_run_counts_its_instructions_and_adds_them_only_when_asked() {
        let dir = tempfile::tempdir().expect("a directory");
        let data = dir.path().join("data");
        let repo = a_repo(&dir.path().join("widget"));
        std::fs::write(repo.join("AGENTS.md"), "Run make check.\n").expect("instructions");
        for argv in [
            ["add", "AGENTS.md"].as_slice(),
            &["commit", "-q", "-m", "agents"],
        ] {
            let done = purlis_core::forklock::output(
                std::process::Command::new("git")
                    .arg("-C")
                    .arg(&repo)
                    .args(argv),
            )
            .expect("git runs in a test");
            assert!(done.status.success(), "git {argv:?}");
        }
        let (root, taken) = taken_in(&data, &repo, &firstrun::Choice::Fits, GITHUB)
            .map(taken)
            .expect("opened");

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
        let workspace = purlis_core::workspaces::Plane::open(&root)
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
