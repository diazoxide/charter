//! The probe, installed, approved and asked through charter's real registry and executor — the
//! seam every extension capability is proven at (charter-app#336, "seam 1").
//!
//! `assemble` puts the built program in an extension directory as the operator would,
//! `extension::install` and `extension::approve` are the dialog's two clicks, and
//! `Executor::ask` is what a view's button calls. Each capability charter grows is added to the
//! probe's manifest and proven here: declared, fingerprinted, approved, run, and answered — and
//! refused when it is asked for wrongly.

#![cfg(unix)]

use std::path::PathBuf;

use charter_core::executor::Executor;
use charter_core::panel::Block;
use charter_core::{extension, handed};

/// A plane with one persona, the probe assembled beside it, and a config root to install it in.
struct Probe {
    dir: tempfile::TempDir,
}

impl Probe {
    /// Assembled, and not yet installed.
    fn assembled() -> Self {
        let dir = tempfile::tempdir().expect("a directory");
        let plane = dir.path().join("plane");
        std::fs::create_dir_all(plane.join("personas/steward/memory")).expect("a persona");
        std::fs::write(plane.join("charter.toml"), "").expect("a manifest");
        std::fs::write(
            plane.join("personas/steward/persona.md"),
            "---\nrole: x\n---\n",
        )
        .expect("a definition");

        let status = std::process::Command::new(env!("CARGO_BIN_EXE_extension-probe"))
            .arg("assemble")
            .arg(dir.path().join("ext"))
            .status()
            .expect("assemble runs");
        assert!(status.success(), "assemble failed: {status}");
        Self { dir }
    }

    /// Assembled, installed and approved: the two clicks.
    fn approved() -> Self {
        let probe = Self::assembled();
        let found = extension::install(&probe.config(), &probe.ext()).expect("installed");
        extension::approve(&probe.config(), found.id(), &found.path, &found.fingerprint)
            .expect("approved");
        probe
    }

    fn config(&self) -> PathBuf {
        self.dir.path().join("config")
    }

    fn ext(&self) -> PathBuf {
        self.dir.path().join("ext")
    }

    fn plane(&self) -> PathBuf {
        self.dir.path().join("plane")
    }

    /// Rewrite one top-level key of the probe's manifest, as an author editing it would.
    fn manifest_sets(&self, key: &str, value: serde_json::Value) {
        let at = self.ext().join(extension::MANIFEST);
        let mut doc: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&at).expect("the manifest"))
                .expect("JSON");
        doc[key] = value;
        std::fs::write(&at, doc.to_string()).expect("written");
    }

    fn ask(&self) -> Result<charter_core::executor::Answer, String> {
        let plane = self.plane();
        Executor::default().ask(
            &self.config(),
            &extension::project::Choices::read(&plane),
            "extension-probe",
            "probe",
            None,
            |_| handed::personas(&plane, noon()),
        )
    }
}

fn noon() -> chrono::NaiveDateTime {
    chrono::NaiveDate::from_ymd_opt(2026, 9, 25)
        .and_then(|day| day.and_hms_opt(12, 0, 0))
        .expect("a time")
}

fn notes(blocks: &[Block]) -> Vec<&str> {
    blocks
        .iter()
        .filter_map(|block| match block {
            Block::Note { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

#[test]
fn the_probe_is_asked_through_the_executor_and_answers_what_it_was_handed() {
    let probe = Probe::approved();
    let answer = probe.ask().expect("an answer");
    assert_eq!(
        notes(&answer.blocks),
        ["extension-probe answered the view 'probe' in protocol 1, handed 1 persona"]
    );
}

#[test]
fn the_approval_prompt_names_every_capability_the_probe_asks_for() {
    let probe = Probe::assembled();
    let found = extension::install(&probe.config(), &probe.ext()).expect("installed");
    assert_eq!(
        found.manifest.capabilities,
        [
            extension::Capability::Probe,
            extension::Capability::Badges,
            extension::Capability::RepoColumns
        ],
        "the probe's own manifest asks for its capabilities"
    );

    let asked = extension::prompt(&found, extension::Standing::New);
    // First, and in exactly the words the window draws (`Extensions.test.tsx` renders this
    // same sentence).
    assert_eq!(
        asked.declares[0],
        "the capability “probe” — charter's test capability, which grants nothing"
    );
}

#[test]
fn changing_the_capabilities_after_approval_is_asked_about_again_and_runs_nothing() {
    // The list is inside the manifest's bytes, so the fingerprint covers it: an extension that
    // changes what it asks for after the yes is refused at the press, not run on the old yes.
    let probe = Probe::approved();
    probe.manifest_sets(
        "capabilities",
        serde_json::json!(["repo-columns", "badges", "probe"]),
    );

    let refused = probe.ask().expect_err("it ran on the old approval");
    assert!(
        refused.contains("changed since you approved it"),
        "{refused}"
    );
    let survey = extension::survey(&probe.config());
    assert_eq!(survey.installed[0].standing, extension::Standing::Changed);
}

#[test]
fn a_capability_this_charter_does_not_know_is_refused_by_name_and_nothing_is_loaded() {
    let probe = Probe::assembled();
    probe.manifest_sets("capabilities", serde_json::json!(["teleport"]));

    let refused = extension::install(&probe.config(), &probe.ext())
        .expect_err("an unknown capability was installed");
    let said = refused.to_string();
    assert!(
        said.contains("asks for the capability \"teleport\", which this charter does not know"),
        "{said}"
    );
    // Never partly loaded: nothing was recorded, so there is nothing to approve and nothing runs.
    assert!(extension::read(&probe.config()).registry.entries.is_empty());
    let not_run = probe
        .ask()
        .expect_err("an extension nobody installed answered");
    assert!(not_run.contains("no extension called"), "{not_run}");
}

#[test]
fn an_installed_extension_that_later_asks_for_an_unknown_capability_contributes_nothing() {
    let probe = Probe::approved();
    probe.manifest_sets("capabilities", serde_json::json!(["probe", "teleport"]));

    let survey = extension::survey(&probe.config());
    let row = &survey.installed[0];
    assert!(row.views_in_force().is_empty());
    let why = row.refused.as_deref().expect("a sentence");
    assert!(why.contains("\"teleport\""), "{why}");
    let not_run = probe.ask().expect_err("it ran");
    assert!(not_run.contains("\"teleport\""), "{not_run}");
}

// ---------------------------------------------------------------------------------------
// The facts file: badges and repo columns (charter-app#340)
// ---------------------------------------------------------------------------------------

use charter_core::extension::facts::{self, Reading, Surface};

impl Probe {
    /// What the one core reader says for this probe's plane, as the window asks it.
    fn facts(&self) -> facts::Facts {
        self.facts_as(
            Reading::Window,
            &extension::project::Choices::read(&self.plane()),
        )
    }

    fn facts_as(&self, reading: Reading, choices: &extension::project::Choices) -> facts::Facts {
        facts::gather(&self.config(), choices, now(), reading)
    }

    fn facts_file(&self) -> PathBuf {
        self.ext().join("state").join(facts::FILE)
    }

    /// Write the facts file by hand, as a broken or hostile extension would.
    fn facts_are(&self, text: &str) {
        std::fs::create_dir_all(self.ext().join("state")).expect("the state directory");
        std::fs::write(self.facts_file(), text).expect("written");
    }
}

/// The clock the reader measures ages against.
fn now() -> chrono::DateTime<chrono::Utc> {
    chrono::Utc::now()
}

fn seconds_ago(seconds: i64) -> i64 {
    now().timestamp() - seconds
}

#[test]
fn the_probe_declares_a_badge_and_a_repo_column_and_the_prompt_lists_both() {
    let probe = Probe::assembled();
    let found = extension::install(&probe.config(), &probe.ext()).expect("installed");
    let asked = extension::prompt(&found, extension::Standing::New);
    assert!(
        asked.declares.iter().any(|line| line
            == "a badge, “asked” — shown in the status bar and the terminal footer, read from \
                its facts file and fresh for 1h; charter never starts its program to draw it"),
        "{:#?}",
        asked.declares
    );
    assert!(
        asked.declares.iter().any(|line| line
            == "a repo column, “Asked” — a column in the repo table, read from its facts file \
                and fresh for 1h; charter never starts its program to draw it"),
        "{:#?}",
        asked.declares
    );
}

#[test]
fn nothing_is_shown_and_no_program_is_started_until_the_probe_is_asked_a_question() {
    let probe = Probe::approved();

    // The reader never starts the program: before a question there is no facts file, and
    // reading the facts does not make one.
    let before = probe.facts();
    assert!(before.badges.is_empty(), "{before:#?}");
    assert!(
        before.columns.iter().all(|it| it.cells.is_empty()),
        "{before:#?}"
    );
    assert!(before.notes.is_empty(), "{before:#?}");
    assert!(
        !probe.facts_file().exists(),
        "reading the facts started the probe"
    );

    probe.ask().expect("an answer");

    let after = probe.facts();
    let badge = after.badges.first().expect("the probe's badge");
    assert_eq!(
        (badge.extension.as_str(), badge.label.as_str()),
        ("extension-probe", "asked")
    );
    assert_eq!(badge.value, "1");
    assert!(!badge.stale);
    assert_eq!(badge.surfaces, [Surface::StatusBar, Surface::Footer]);
    let column = after.columns.first().expect("the probe's column");
    assert_eq!(column.title, "Asked");
    let cell = column.cells.get(extension_probe::REPO).expect("a cell");
    assert_eq!(cell.value, "1");
}

#[test]
fn the_footer_draws_the_probes_badge_from_the_same_reader() {
    let probe = Probe::approved();
    probe.ask().expect("an answer");

    let footer = probe.facts_as(
        Reading::Footer,
        &extension::project::Choices::read(&probe.plane()),
    );
    assert_eq!(footer.badges.len(), 1, "{footer:#?}");
    assert!(footer.columns.is_empty(), "the footer has no repo table");

    let config = probe.config();
    let drawn = charter_core::footer::render(
        &probe.plane(),
        &serde_json::Value::Null,
        &charter_core::footer::Ambient {
            env: &|name| (name == "COLUMNS").then(|| "120".to_owned()),
            cwd: &probe.plane(),
            now: now(),
            config: Some(&config),
        },
    );
    assert!(drawn.contains("asked"), "{drawn}");
}

#[test]
fn a_value_older_than_its_freshness_is_marked_stale_with_its_age() {
    let probe = Probe::approved();
    probe.facts_are(&format!(
        r#"{{"badges": {{"asked": {{"value": "7", "at": {}}}}}}}"#,
        seconds_ago(3 * 3600)
    ));
    let badge = probe.facts().badges.remove(0);
    assert!(badge.stale, "{badge:#?}");
    assert!(
        (3 * 3600..3 * 3600 + 60).contains(&badge.age_seconds),
        "{badge:#?}"
    );
}

#[test]
fn a_field_the_manifest_does_not_declare_contributes_nothing_and_is_reported() {
    let probe = Probe::approved();
    probe.facts_are(&format!(
        r#"{{"badges": {{"asked": {{"value": "1", "at": {at}}},
                        "smuggled": {{"value": "9", "at": {at}}}}},
            "repo-columns": {{"hidden": {{"svc": {{"value": "9", "at": {at}}}}}}}}}"#,
        at = seconds_ago(0)
    ));
    let read = probe.facts();
    assert_eq!(read.badges.len(), 1, "{read:#?}");
    assert_eq!(read.badges[0].id, "asked");
    assert!(read.columns.iter().all(|it| it.id != "hidden"));
    assert!(
        read.notes
            .iter()
            .any(|it| it.contains("\"smuggled\"") && it.contains("does not declare")),
        "{:#?}",
        read.notes
    );
    assert!(
        read.notes.iter().any(|it| it.contains("\"hidden\"")),
        "{:#?}",
        read.notes
    );
}

#[test]
fn an_oversized_facts_file_contributes_nothing_and_says_why() {
    let probe = Probe::approved();
    probe.facts_are(&format!(
        r#"{{"badges": {{"asked": {{"value": "1", "at": {}}}}}, "pad": "{}"}}"#,
        seconds_ago(0),
        "x".repeat(usize::try_from(facts::MOST_BYTES).expect("small"))
    ));
    let read = probe.facts();
    assert!(read.badges.is_empty(), "{read:#?}");
    assert!(
        read.notes
            .iter()
            .any(|it| it.contains("charter reads no more than")),
        "{:#?}",
        read.notes
    );
}

#[test]
fn a_malformed_facts_file_contributes_nothing_and_says_why() {
    let probe = Probe::approved();
    probe.facts_are("{\"badges\": ");
    let read = probe.facts();
    assert!(read.badges.is_empty(), "{read:#?}");
    assert!(
        read.notes.iter().any(|it| it.contains("is not JSON")),
        "{:#?}",
        read.notes
    );
}

#[test]
fn an_extension_that_changed_on_disk_contributes_no_badge_and_no_cell() {
    let probe = Probe::approved();
    probe.ask().expect("an answer");
    std::fs::write(probe.ext().join("README"), "added after the yes").expect("written");

    let read = probe.facts();
    assert!(read.badges.is_empty(), "{read:#?}");
    assert!(read.columns.is_empty(), "{read:#?}");
    assert!(
        read.notes
            .iter()
            .any(|it| it.contains("changed since you approved it")),
        "{:#?}",
        read.notes
    );
}

#[test]
fn repo_columns_and_badges_follow_the_project_and_the_workspace_turning_it_off() {
    let probe = Probe::approved();
    probe.ask().expect("an answer");

    let off = extension::project::Choices::from_text(
        Some("[extensions.extension-probe]\nenabled = false\n"),
        None,
    );
    let read = probe.facts_as(Reading::Window, &off);
    assert!(
        read.columns.is_empty() && read.badges.is_empty(),
        "{read:#?}"
    );

    let off_in_workspace = extension::project::Choices::from_text(None, None).in_workspace(
        "alpha",
        Some(r#"{"settings": {"extensions": {"extension-probe": {"enabled": false}}}}"#),
    );
    let read = probe.facts_as(Reading::Window, &off_in_workspace);
    assert!(
        read.columns.is_empty() && read.badges.is_empty(),
        "{read:#?}"
    );

    let on = extension::project::Choices::from_text(None, None);
    assert_eq!(probe.facts_as(Reading::Window, &on).columns.len(), 1);
}

#[test]
fn a_contribution_without_its_capability_is_refused_by_name() {
    let probe = Probe::assembled();
    probe.manifest_sets("capabilities", serde_json::json!(["probe", "repo-columns"]));
    let refused = extension::install(&probe.config(), &probe.ext())
        .expect_err("a contribution without its capability was installed")
        .to_string();
    assert!(refused.contains("\"badges\""), "{refused}");
}

#[test]
fn a_capability_that_declares_nothing_is_refused_by_name() {
    let probe = Probe::assembled();
    let at = probe.ext().join(extension::MANIFEST);
    let mut doc: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&at).expect("the manifest")).expect("JSON");
    doc["contributes"]
        .as_object_mut()
        .expect("contributes")
        .remove("repo-columns");
    std::fs::write(&at, doc.to_string()).expect("written");
    let refused = extension::install(&probe.config(), &probe.ext())
        .expect_err("a capability declaring nothing was installed")
        .to_string();
    assert!(refused.contains("\"repo-columns\""), "{refused}");
}
