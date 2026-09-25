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

use crate::planes::{PlaneId, Planes};

/// How many journal lines the view shows, newest first.
const JOURNAL_SHOWN: usize = 50;

/// One save attempt, as the journal holds it.
#[derive(Debug, Clone, PartialEq, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SaveEntry {
    /// Seconds since the epoch.
    pub at: f64,
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
    /// The files the last rebase conflicted in, when that is why the save is blocked.
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

/// One journal line, with what it lacks read as empty rather than refused: the journal is
/// charter's own note, and a line an older charter wrote is still worth showing.
fn entry_of(line: &serde_json::Value) -> SaveEntry {
    let text = |key: &str| line.get(key).and_then(serde_json::Value::as_str);
    SaveEntry {
        at: line
            .get("at")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(0.0),
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

    #[test]
    fn a_save_refused_answers_the_refusal_in_the_cores_words() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("charter.toml"), "").unwrap();

        let err = save(dir.path(), None).expect_err("not a repository");

        assert!(err.contains("is not a git repository"), "{err}");
    }
}
