//! The Saving view and the title bar's save indicator (charter-app#294, ADR 0051): where the
//! plane's unsaved work sits, the save journal, and the save button.
//!
//! Both are the core's answers, re-shaped for the window and nothing more. The stage is
//! [`planegit::standing`], read from git and the push record and never from the network; the
//! save is [`planegit::save_as`] with [`Trigger::Manual`] — the same function `charter save`
//! runs, so the button and the command cannot disagree about what a save does.

use std::path::Path;

use charter_core::planegit::{self, Trigger};
use charter_core::planesave;
use charter_core::repocmd::Say;
use charter_core::reposave;

use crate::planes::{PlaneId, Planes};

/// How many journal lines the view shows, newest first.
const JOURNAL_SHOWN: usize = 50;

/// One save attempt, as the journal holds it.
#[derive(Debug, Clone, PartialEq, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SaveEntry {
    /// Seconds since the epoch.
    pub at: f64,
    /// `plane`, or `repo:<workspace>/<name>`.
    pub target: String,
    pub trigger: String,
    pub mode: String,
    pub files: u32,
    pub commit: Option<String>,
    pub pr: Option<String>,
    pub outcome: String,
    pub detail: String,
}

/// Where the plane's unsaved work sits, how far a save goes, and what the last saves did.
#[derive(Debug, Clone, PartialEq, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PlaneSaving {
    /// `blocked`, `changed`, `committed`, `pr-open` or `saved`.
    pub stage: String,
    /// What the next save would commit.
    pub changed: Vec<String>,
    /// Commits the remote does not have; `null` when there is nothing to count against.
    pub ahead: Option<u32>,
    pub pr: Option<String>,
    pub blocked: Option<String>,
    /// The target branch: `[plane] branch`, or the one the plane has checked out.
    pub branch: String,
    /// Whether a save would push. When it would not, a commit is as far as a save goes.
    pub pushes: bool,
    /// Commits the last fetch found on the remote that were not pulled; `null` when there is
    /// nothing to count against.
    pub behind: Option<u32>,
    /// Why the last push did not land, when it failed rather than conflicted.
    pub push_failed: Option<String>,
    /// The LIVE workspaces, whose charter, memory and todos a save publishes (charter-app#301).
    pub live: Vec<String>,
    /// The files to settle, when conflicts are why the save is blocked.
    pub conflicts: Vec<String>,
    /// What a save cannot do here that is not a block (a PR mode with no forge to open it on).
    pub notice: Option<String>,
    /// `[plane] mode`, or `null` when the plane names none.
    pub mode: Option<String>,
    /// Where the mode came from: `charter.toml`, `charter.local.toml`, `[memory] share`, or
    /// `default`.
    pub mode_from: String,
    /// The newest [`JOURNAL_SHOWN`] saves, newest first.
    pub journal: Vec<SaveEntry>,
}

/// The plane's save standing.
///
/// On a blocking thread: it asks git.
#[tauri::command]
#[specta::specta]
pub async fn plane_saving(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
) -> Result<PlaneSaving, String> {
    let root = planes.held(&plane)?.root().to_path_buf();
    tauri::async_runtime::spawn_blocking(move || saving_of(&root))
        .await
        .map_err(|err| format!("reading the plane's save state did not finish: {err}"))
}

/// Save the plane, as the save button does: `message`, or the generated one when it is empty.
/// Answers every line the save said, or its refusal.
///
/// On a blocking thread: it commits, and may push.
#[tauri::command]
#[specta::specta]
pub async fn save_plane(
    app: tauri::AppHandle,
    planes: tauri::State<'_, Planes>,
    heard: tauri::State<'_, crate::heard::Heard>,
    plane: PlaneId,
    message: Option<String>,
) -> Result<Vec<String>, String> {
    let root = planes.held(&plane)?.root().to_path_buf();
    let at = root.clone();
    let saved = tauri::async_runtime::spawn_blocking(move || save(&at, message.as_deref()))
        .await
        .map_err(|err| format!("the save did not finish: {err}"))?;
    // Told once it is saved, on a thread of its own (charter-app#343).
    if saved.is_ok() {
        heard.tell(
            &app,
            plane,
            root,
            charter_core::extension::events::Event::PlaneSaved,
        );
    }
    saved
}

/// Answer the question a plane with no mode is asked once (ADR 0051): how far its saves go.
/// Written as `[plane] mode` into `charter.toml`, through the core's own writer, and answered
/// with the plane's save standing as it now is.
#[tauri::command]
#[specta::specta]
pub async fn choose_plane_mode(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    mode: String,
) -> Result<PlaneSaving, String> {
    let root = planes.held(&plane)?.root().to_path_buf();
    tauri::async_runtime::spawn_blocking(move || choose_mode(&root, &mode))
        .await
        .map_err(|err| format!("choosing the mode did not finish: {err}"))?
}

/// [`choose_plane_mode`], without a runtime.
pub fn choose_mode(root: &Path, mode: &str) -> Result<PlaneSaving, String> {
    use charter_core::settings::{self, Edit, Step, Value, Which};
    if planesave::Mode::parse(mode).is_none() {
        return Err(format!(
            "{mode} is not a mode — one of off, commit, push, pr, pr-merge"
        ));
    }
    let read = settings::read(root, Which::Shared)?;
    let text = settings::edited(
        &read.text,
        &[Edit {
            path: vec![Step::Key("plane".into()), Step::Key("mode".into())],
            value: Some(Value::Text(mode.to_owned())),
        }],
    )?;
    settings::save(
        root,
        Which::Shared,
        read.exists.then_some(read.text.as_str()),
        &text,
    )
    .map_err(|reasons| reasons.join("\n"))?;
    Ok(saving_of(root))
}

/// [`plane_saving`], without a runtime.
pub fn saving_of(root: &Path) -> PlaneSaving {
    let standing = planegit::standing(root);
    let plane = planesave::Settings::read(root).plane;
    let mode_from = if plane.from_share {
        "[memory] share".to_owned()
    } else {
        plane.mode.source.file().unwrap_or("default").to_owned()
    };
    let mut journal: Vec<SaveEntry> = planegit::journal(root)
        .iter()
        .rev()
        .take(JOURNAL_SHOWN)
        .map(entry_of)
        .collect();
    journal.shrink_to_fit();
    PlaneSaving {
        stage: standing.stage.word().to_owned(),
        changed: standing.changed,
        ahead: standing.ahead,
        pr: standing.pr,
        blocked: standing.blocked,
        branch: standing.branch,
        pushes: standing.pushes,
        behind: standing.behind,
        push_failed: standing.push_failed,
        live: charter_core::wscmd::live_workspaces(root)
            .into_iter()
            .collect(),
        conflicts: standing.conflicts,
        notice: standing.notice,
        mode: plane.mode.value.map(|m| m.as_str().to_owned()),
        mode_from,
        journal,
    }
}

/// [`save_plane`], without a runtime.
pub fn save(root: &Path, message: Option<&str>) -> Result<Vec<String>, String> {
    save_as(root, message, Trigger::Manual)
}

/// A save of the plane at `root`, started by `trigger` — the button, or auto-save
/// (`autosave.rs`). One at a time per plane, whoever started it.
pub fn save_as(
    root: &Path,
    message: Option<&str>,
    trigger: Trigger,
) -> Result<Vec<String>, String> {
    let message = message.map(str::trim).filter(|m| !m.is_empty());
    let mut said: Vec<Say> = Vec::new();
    let mut say = |line: Say| said.push(line);
    let code = planegit::save_as(
        &planegit::Request {
            root,
            message,
            sign: false,
            no_push: false,
            cwd: root,
        },
        trigger,
        &mut say,
    );
    crate::workspaces::ran(code, said)
}

/// One workspace repo's save standing, for its row in the Saving view (charter-app#299).
#[derive(Debug, Clone, PartialEq, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RepoSaving {
    pub name: String,
    /// `[repos.<name>] mode`: `off` when neither file says.
    pub mode: String,
    /// The file that decided the mode, or `default`.
    pub mode_from: String,
    /// Whether it is saved by itself — off unless its table turns it on.
    pub autosave: bool,
    /// `off` for a repo charter never saves; else `blocked`, `changed`, `committed`,
    /// `pr-open` or `saved`.
    pub stage: String,
    /// The branch the clone is on; `null` on none.
    pub branch: Option<String>,
    /// Files a save would take.
    pub changed: u32,
    /// Commits the remote's copy of the branch lacks; `null` for a branch never pushed.
    pub ahead: Option<u32>,
    pub pr: Option<String>,
    pub blocked: Option<String>,
    /// Whether a save would push.
    pub pushes: bool,
    /// Where a PR mode's pull request goes: `[repos.<name>] branch`, else the repo's default
    /// branch; `null` in the other modes, or when charter cannot tell — so the row and the Save
    /// all confirmation can say where a save goes before anyone presses it.
    pub target: Option<String>,
    /// Whether a PR mode's save would commit on a branch of charter's own,
    /// `charter/<workspace>/…`, because the clone stands on its pull request's base or on the
    /// repo's default branch — a PR mode never pushes either (`reposave`).
    pub own_branch: bool,
}

/// Every clone in `workspace`, as the Saving view draws them. Read from git and the journal,
/// never the network.
///
/// On a blocking thread: it asks git once or twice per clone.
#[tauri::command]
#[specta::specta]
pub async fn workspace_saving(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    workspace: String,
) -> Result<Vec<RepoSaving>, String> {
    let root = planes.held(&plane)?.root().to_path_buf();
    tauri::async_runtime::spawn_blocking(move || repos_saving(&root, &workspace))
        .await
        .map_err(|err| format!("reading the repos' save state did not finish: {err}"))?
}

/// Save one repo of `workspace`, as its row's button does. Refused, with whom it waits for,
/// while any chat in the workspace is mid-turn: a save the operator asked for does not queue
/// itself to run later, unwatched.
///
/// On a blocking thread: it commits, and may push and open a pull request.
#[tauri::command]
#[specta::specta]
pub async fn save_repo(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    workspace: String,
    name: String,
    message: Option<String>,
) -> Result<Vec<String>, String> {
    let held = planes.held(&plane)?;
    let root = held.root().to_path_buf();
    tauri::async_runtime::spawn_blocking(move || {
        // Asked on this thread, twice — before the save and right before `git add` — so a
        // turn that starts while the save waits is still seen.
        let mid_turn = || held.mid_turn_in(&workspace);
        save_repo_in(&root, &workspace, &name, message.as_deref(), &mid_turn)
    })
    .await
    .map_err(|err| format!("the save did not finish: {err}"))?
}

/// [`workspace_saving`], without a runtime.
pub fn repos_saving(root: &Path, workspace: &str) -> Result<Vec<RepoSaving>, String> {
    let found = charter_core::repos::clones(root, workspace).map_err(|why| why.to_string())?;
    let settings = planesave::Settings::read(root);
    Ok(found
        .repos
        .iter()
        .map(|repo| {
            let standing = reposave::standing(root, workspace, repo);
            let opens_a_pr = matches!(
                standing.mode,
                planesave::Mode::Pr | planesave::Mode::PrMerge
            );
            let default = opens_a_pr
                .then(|| reposave::default_branch(root, &repo.name, &repo.path))
                .flatten();
            let target = opens_a_pr
                .then(|| settings.repo(&repo.name).branch.value.or(default.clone()))
                .flatten();
            let own_branch = opens_a_pr
                && standing
                    .branch
                    .as_ref()
                    .is_some_and(|on| Some(on) == target.as_ref() || Some(on) == default.as_ref());
            RepoSaving {
                target,
                own_branch,
                stage: if standing.mode == planesave::Mode::Off {
                    "off".to_owned()
                } else {
                    standing.stage.word().to_owned()
                },
                name: standing.name,
                mode: standing.mode.as_str().to_owned(),
                mode_from: standing.mode_from.to_owned(),
                autosave: standing.autosave,
                branch: standing.branch,
                changed: standing.changed,
                ahead: standing.ahead,
                pr: standing.pr,
                blocked: standing.blocked,
                pushes: standing.pushes,
            }
        })
        .collect())
}

/// [`save_repo`], without a runtime.
pub fn save_repo_in(
    root: &Path,
    workspace: &str,
    name: &str,
    message: Option<&str>,
    mid_turn: &dyn Fn() -> Vec<String>,
) -> Result<Vec<String>, String> {
    let found = charter_core::repos::clones(root, workspace).map_err(|why| why.to_string())?;
    let repo = found
        .repos
        .iter()
        .find(|repo| repo.name == name)
        .ok_or_else(|| format!("{workspace} holds no clone called {name}"))?;
    let message = message.map(str::trim).filter(|m| !m.is_empty());
    let mut said: Vec<Say> = Vec::new();
    let code = reposave::save_as(
        &reposave::Request {
            plane: root,
            workspace,
            name,
            clone: &repo.path,
            message,
            no_push: false,
            mid_turn,
        },
        Trigger::Manual,
        &mut |line| said.push(line),
    );
    crate::workspaces::ran(code, said)
}

/// One journal line, with what it lacks read as empty rather than refused: the journal is
/// charter's own note, and a line an older charter wrote is still worth showing.
fn entry_of(line: &serde_json::Value) -> SaveEntry {
    let text = |key: &str| line.get(key).and_then(serde_json::Value::as_str);
    SaveEntry {
        at: line
            .get("at")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(0.0),
        target: text("target").unwrap_or("plane").to_owned(),
        trigger: text("trigger").unwrap_or("").to_owned(),
        mode: text("mode").unwrap_or("").to_owned(),
        files: line
            .get("files")
            .and_then(serde_json::Value::as_u64)
            .and_then(|n| u32::try_from(n).ok())
            .unwrap_or(0),
        commit: text("commit").map(str::to_owned),
        pr: text("pr").map(str::to_owned),
        outcome: text("outcome").unwrap_or("").to_owned(),
        detail: text("detail").unwrap_or("").to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    /// git for a fixture: never the developer's global config, so never their signer.
    fn git(dir: &Path, args: &[&str]) {
        let mut command = Command::new("git");
        command
            .arg("-C")
            .arg(dir)
            .args(["-c", "commit.gpgsign=false"])
            .args(args)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@example.invalid")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@example.invalid");
        let out = charter_core::forklock::output(&mut command).expect("git runs");
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    /// A plane with one commit, its own identity, and no remote.
    fn plane(toml: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(root.join("charter.toml"), toml).unwrap();
        std::fs::write(root.join(".gitignore"), ".charter/\n").unwrap();
        git(root, &["init", "-q", "-b", "main"]);
        git(root, &["config", "user.name", "t"]);
        git(root, &["config", "user.email", "t@example.invalid"]);
        git(root, &["add", "-A"]);
        git(root, &["commit", "-q", "-m", "one"]);
        dir
    }

    #[test]
    fn a_file_written_into_the_plane_is_what_the_next_save_takes() {
        let dir = plane("");
        std::fs::write(dir.path().join("note.md"), "n").unwrap();

        let got = saving_of(dir.path());

        assert_eq!(got.stage, "changed");
        assert_eq!(got.changed, ["note.md"]);
        assert_eq!((got.mode, got.mode_from.as_str()), (None, "default"));
    }

    #[test]
    fn the_save_button_commits_with_the_message_typed_and_the_journal_says_so() {
        let dir = plane("[plane]\nmode = \"commit\"\n");
        std::fs::write(dir.path().join("note.md"), "n").unwrap();

        let said = save(dir.path(), Some("  from the button  ")).expect("saved");

        assert!(
            said.iter().any(|l| l.contains("from the button")),
            "{said:?}"
        );
        let got = saving_of(dir.path());
        assert_eq!(got.changed, Vec::<String>::new());
        assert_eq!(
            (got.mode.as_deref(), got.mode_from.as_str()),
            (Some("commit"), "charter.toml")
        );
        let last = &got.journal[0];
        assert_eq!(
            (last.trigger.as_str(), last.outcome.as_str(), last.files),
            ("manual", "committed", 1)
        );
    }

    #[test]
    fn a_second_save_while_one_runs_is_refused_in_words_and_the_first_is_untouched() {
        let dir = plane("");
        std::fs::write(dir.path().join("note.md"), "n").unwrap();
        let running = planegit::Claim::of(dir.path()).expect("the first save");

        let err = save(dir.path(), None).expect_err("a second save");

        assert_eq!(err, planegit::ALREADY_SAVING);
        assert_eq!(
            saving_of(dir.path()).changed,
            ["note.md"],
            "the second save committed"
        );
        drop(running);
        assert!(
            save(dir.path(), None).is_ok(),
            "the next save, once the first is done"
        );
    }

    #[test]
    fn the_answer_to_how_a_project_is_saved_is_written_as_its_mode_keeping_the_rest_of_the_file() {
        let dir = plane("# the team's settings\n[memory]\nshare = \"local\"\n");
        assert_eq!(saving_of(dir.path()).mode, None, "not asked yet");

        let got = choose_mode(dir.path(), "commit").expect("chosen");

        assert_eq!(
            (got.mode.as_deref(), got.mode_from.as_str()),
            (Some("commit"), "charter.toml")
        );
        let text = std::fs::read_to_string(dir.path().join("charter.toml")).unwrap();
        assert!(text.starts_with("# the team's settings\n"), "{text}");
        assert!(text.contains("[plane]\nmode = \"commit\"\n"), "{text}");
    }

    #[test]
    fn a_mode_charter_does_not_know_is_refused_and_nothing_is_written() {
        let dir = plane("");
        let err = choose_mode(dir.path(), "yolo").expect_err("refused");
        assert_eq!(
            err,
            "yolo is not a mode — one of off, commit, push, pr, pr-merge"
        );
        assert_eq!(
            std::fs::read_to_string(dir.path().join("charter.toml")).unwrap(),
            ""
        );
    }

    #[test]
    fn what_came_in_and_was_not_pulled_is_counted() {
        let dir = plane("");
        assert_eq!(
            saving_of(dir.path()).behind,
            None,
            "nothing to count against"
        );
    }

    /// A plane saying `toml`, with a workspace `alpha` holding a clone `widget` of one commit.
    fn plane_with_a_repo(toml: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = plane(toml);
        let clone = dir.path().join("workspaces/alpha/widget");
        std::fs::create_dir_all(&clone).unwrap();
        git(&clone, &["init", "-q", "-b", "main"]);
        git(&clone, &["config", "user.name", "t"]);
        git(&clone, &["config", "user.email", "t@example.invalid"]);
        std::fs::write(clone.join("README.md"), "one").unwrap();
        git(&clone, &["add", "-A"]);
        git(&clone, &["commit", "-q", "-m", "one"]);
        (dir, clone)
    }

    #[test]
    fn each_repo_in_the_workspace_is_a_row_with_its_mode_and_stage() {
        let (dir, clone) = plane_with_a_repo("[repos.widget]\nmode = \"commit\"\n");
        std::fs::write(clone.join("a.md"), "a").unwrap();

        let rows = repos_saving(dir.path(), "alpha").expect("the workspace reads");

        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        assert_eq!(
            (row.name.as_str(), row.mode.as_str(), row.mode_from.as_str()),
            ("widget", "commit", "charter.toml")
        );
        assert_eq!((row.stage.as_str(), row.changed), ("changed", 1));
        assert_eq!(row.branch.as_deref(), Some("main"));
        assert!(!row.autosave, "a repo's auto-save is off by default");
    }

    #[test]
    fn a_pr_mode_row_says_where_its_pull_request_goes_and_whether_it_needs_a_branch_of_its_own() {
        // On `main`, the PR's base: the save would cut `charter/alpha/…` rather than push main.
        let (dir, clone) = plane_with_a_repo("[repos.widget]\nmode = \"pr\"\nbranch = \"main\"\n");
        let row = &repos_saving(dir.path(), "alpha").unwrap()[0];
        assert_eq!(
            (row.target.as_deref(), row.own_branch),
            (Some("main"), true)
        );

        // On a feature branch, the save pushes that branch and opens the PR from it.
        git(&clone, &["checkout", "-q", "-b", "feature/x"]);
        let row = &repos_saving(dir.path(), "alpha").unwrap()[0];
        assert_eq!(
            (row.target.as_deref(), row.own_branch),
            (Some("main"), false)
        );
    }

    #[test]
    fn a_row_that_opens_no_pull_request_names_no_target() {
        let (dir, _clone) = plane_with_a_repo("[repos.widget]\nmode = \"push\"\n");
        let row = &repos_saving(dir.path(), "alpha").unwrap()[0];
        assert_eq!((row.target.as_deref(), row.own_branch), (None, false));
    }

    #[test]
    fn a_repo_charter_never_saves_says_off_rather_than_a_stage() {
        let (dir, clone) = plane_with_a_repo("[repos.widget]\nmode = \"off\"\n");
        std::fs::write(clone.join("a.md"), "a").unwrap();

        assert_eq!(repos_saving(dir.path(), "alpha").unwrap()[0].stage, "off");
    }

    #[test]
    fn a_repo_save_waits_for_a_chat_mid_turn_and_names_it() {
        let (dir, clone) = plane_with_a_repo("[repos.widget]\nmode = \"commit\"\n");
        std::fs::write(clone.join("a.md"), "a").unwrap();

        let err = save_repo_in(dir.path(), "alpha", "widget", None, &|| {
            vec!["alpha.2".to_owned()]
        })
        .expect_err("held back");

        assert_eq!(
            err,
            "Not saved: alpha.2 is mid-turn in alpha. A repo is saved between turns — save \
             again when the turn ends."
        );
        assert_eq!(
            repos_saving(dir.path(), "alpha").unwrap()[0].stage,
            "changed"
        );

        let said = save_repo_in(
            dir.path(),
            "alpha",
            "widget",
            Some(" from the row "),
            &Vec::new,
        )
        .expect("saved once the turn ended");
        assert!(said.iter().any(|l| l.contains("from the row")), "{said:?}");
        assert_eq!(repos_saving(dir.path(), "alpha").unwrap()[0].stage, "saved");
        let last = &saving_of(dir.path()).journal[0];
        assert_eq!(
            (last.target.as_str(), last.outcome.as_str()),
            ("repo:alpha/widget", "committed")
        );
    }

    #[test]
    fn a_repo_the_workspace_does_not_hold_is_refused_by_name() {
        let (dir, _clone) = plane_with_a_repo("");
        assert_eq!(
            save_repo_in(dir.path(), "alpha", "gadget", None, &Vec::new).unwrap_err(),
            "alpha holds no clone called gadget"
        );
    }

    #[test]
    fn a_save_refused_answers_the_refusal_in_the_cores_words() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("charter.toml"), "").unwrap();

        let err = save(dir.path(), None).expect_err("not a repository");

        assert!(err.contains("is not a git repository"), "{err}");
    }
}
