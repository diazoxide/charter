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

use charter_core::executor::{Acted, Executor, On};
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
        // A git repository, as a plane is: what charter watches while the probe answers is what
        // git sees (charter-app#341).
        let init = std::process::Command::new("git")
            .args(["init", "-q"])
            .current_dir(&plane)
            .status()
            .expect("git runs");
        assert!(init.success(), "git init failed: {init}");

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

    /// Run the probe's action `action` on `on`, with the operator's yes or without it.
    fn act(&self, action: &str, on: On<'_>, confirmed: bool) -> Result<Acted, String> {
        let plane = self.plane();
        Executor::default().act(
            &self.config(),
            &extension::project::Choices::read(&plane),
            "extension-probe",
            action,
            on,
            confirmed,
            |_| handed::personas(&plane, noon()),
        )
    }

    /// The notes the probe keeps in the one plane path it declares.
    fn notes(&self) -> usize {
        std::fs::read_dir(self.plane().join("notes")).map_or(0, |dir| dir.flatten().count())
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

/// The row a view answered, with the actions it offers.
fn row_actions(blocks: &[Block]) -> Vec<(&str, Vec<&str>)> {
    blocks
        .iter()
        .filter_map(|block| match block {
            Block::List { rows, .. } => Some(rows),
            _ => None,
        })
        .flatten()
        .map(|row| {
            (
                row.text.as_str(),
                row.actions.iter().map(String::as_str).collect(),
            )
        })
        .collect()
}

/// A view on the probe's row, as the window runs an action pressed on it.
fn on_its_row() -> On<'static> {
    On {
        view: Some("probe"),
        focus: None,
        row: Some("notes"),
    }
}

#[test]
fn the_probe_is_asked_through_the_executor_and_answers_what_it_was_handed() {
    let probe = Probe::approved();
    let answer = probe.ask().expect("an answer");
    // In protocol 2, the one its manifest names, handed where it may write, resolved.
    let notes_dir = format!("{}/notes/", probe.plane().display());
    assert_eq!(
        notes(&answer.blocks),
        [format!(
            "extension-probe answered the view 'probe' in protocol 2, handed 1 persona, and may \
             write {notes_dir}"
        )]
    );
    assert_eq!(answer.overreach, None);
}

// ---------------------------------------------------------------------------------------
// Palette commands, row actions and plane writes (charter-app#341)
// ---------------------------------------------------------------------------------------

#[test]
fn a_palette_command_opens_its_view_and_another_runs_its_action() {
    let probe = Probe::approved();
    let survey = extension::survey(&probe.config());
    let commands = survey.installed[0].palette_in_force();
    assert_eq!(
        commands
            .iter()
            .map(|it| (it.title.as_str(), it.does.clone()))
            .collect::<Vec<_>>(),
        [
            ("Open the probe", extension::Does::Open("probe".to_owned())),
            ("Jot a note", extension::Does::Run("jot".to_owned())),
        ]
    );

    // The first is the view's own question.
    let extension::Does::Open(view) = &commands[0].does else {
        panic!("the first command opens nothing");
    };
    assert_eq!(view, "probe");
    probe.ask().expect("the view it opens answered");

    // The second runs its action on nothing in particular — no view, no row — and there is no
    // view to refresh.
    let extension::Does::Run(action) = &commands[1].does else {
        panic!("the second command runs nothing");
    };
    let acted = probe
        .act(action, On::default(), false)
        .expect("the action ran");
    assert_eq!(acted.blocks, None);
    assert_eq!(
        probe.notes(),
        1,
        "the action it runs did not write its note"
    );
}

#[test]
fn a_row_action_runs_writes_inside_its_declared_path_and_refreshes_the_view() {
    let probe = Probe::approved();
    let answer = probe.ask().expect("the view");
    assert_eq!(
        row_actions(&answer.blocks),
        [("0 notes", vec!["jot", "sweep", "careful", "forget"])]
    );
    // What each row's button says and whether it asks first come from the manifest, beside the
    // answer, never from the answer.
    assert!(
        answer
            .actions
            .iter()
            .any(|it| it.id == "forget" && it.asks_first())
    );

    let acted = probe.act("jot", on_its_row(), false).expect("jotted");

    assert_eq!(probe.notes(), 1);
    let refreshed = acted.blocks.expect("the view, refreshed");
    assert_eq!(row_actions(&refreshed)[0].0, "1 notes");
    assert_eq!(acted.overreach, None, "a write inside notes/ was reported");
}

#[test]
fn a_write_outside_the_declared_paths_is_reported_naming_the_extension() {
    let probe = Probe::approved();

    let acted = probe.act("stray", on_its_row(), false).expect("it ran");

    let said = acted
        .overreach
        .expect("the write outside notes/ went unreported");
    assert!(said.contains("'extension-probe'"), "{said}");
    assert!(said.contains(extension_probe::STRAY), "{said}");
    assert!(said.contains("not proof of who changed it"), "{said}");
}

#[test]
fn an_action_that_deletes_without_saying_so_is_reported_even_inside_its_paths() {
    let probe = Probe::approved();
    probe.act("jot", on_its_row(), false).expect("jotted");
    // The note has to be something git lists as there for its deleting to be seen: committed,
    // so a delete is a change git reports.
    let commit = std::process::Command::new("git")
        .args([
            "-c",
            "user.name=probe",
            "-c",
            "user.email=probe@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "add",
            "-A",
        ])
        .current_dir(probe.plane())
        .status()
        .and_then(|_| {
            std::process::Command::new("git")
                .args([
                    "-c",
                    "user.name=probe",
                    "-c",
                    "user.email=probe@example.invalid",
                    "-c",
                    "commit.gpgsign=false",
                    "commit",
                    "-qm",
                    "notes",
                ])
                .current_dir(probe.plane())
                .status()
        })
        .expect("git runs");
    assert!(commit.success());

    let swept = probe.act("sweep", on_its_row(), false).expect("it ran");
    let said = swept
        .overreach
        .expect("an undeclared delete went unreported");
    assert!(
        said.contains("deleted though it does not say it deletes"),
        "{said}"
    );
    assert!(said.contains("notes/note-1.md"), "{said}");

    // The same delete by the action that says it deletes is what it said it would do.
    probe.act("jot", on_its_row(), false).expect("jotted");
    let forgot = probe.act("forget", on_its_row(), true).expect("it ran");
    assert_eq!(forgot.overreach, None);
}

#[test]
fn an_action_that_asks_first_runs_only_with_a_yes_and_one_that_deletes_always_asks() {
    let probe = Probe::approved();

    let refused = probe
        .act("careful", on_its_row(), false)
        .expect_err("it ran with nobody asked");
    assert!(refused.contains("nobody said yes"), "{refused}");
    probe
        .act("careful", on_its_row(), true)
        .expect("it ran once said yes to");

    // `forget` says `confirm: false`, and deletes: charter asks anyway.
    probe.act("jot", on_its_row(), false).expect("jotted");
    let refused = probe
        .act("forget", on_its_row(), false)
        .expect_err("a delete ran with nobody asked");
    assert!(
        refused.contains("asks before every action that deletes"),
        "{refused}"
    );
    assert_eq!(probe.notes(), 1, "the refused delete deleted");
    probe
        .act("forget", on_its_row(), true)
        .expect("it ran once said yes to");
    assert_eq!(probe.notes(), 0);
}

#[test]
fn an_extension_changed_on_disk_offers_no_commands_or_actions_and_runs_none() {
    let probe = Probe::approved();
    probe.manifest_sets("name", serde_json::json!("Extension probe, edited"));

    let survey = extension::survey(&probe.config());
    let row = &survey.installed[0];
    assert!(row.palette_in_force().is_empty());
    assert!(row.actions_in_force().is_empty());
    let refused = probe
        .act("jot", on_its_row(), false)
        .expect_err("it ran changed");
    assert!(
        refused.contains("changed since you approved it"),
        "{refused}"
    );
    assert_eq!(probe.notes(), 0);
}

#[test]
fn the_approval_prompt_shows_its_actions_commands_and_write_paths() {
    let probe = Probe::assembled();
    let found = extension::install(&probe.config(), &probe.ext()).expect("installed");
    let asked = extension::prompt(&found, extension::Standing::New).declares;
    for line in [
        "an action on its rows, “Forget the notes” — it deletes, so charter always asks you first",
        "an action on its rows, “Careful” — charter asks you first",
        "a palette command, “Extension probe: Jot a note” — runs its action “Jot a note”",
        "plane paths it writes: notes/",
    ] {
        assert!(
            asked.iter().any(|it| it == line),
            "{line:?} not in {asked:#?}"
        );
    }
}

#[test]
fn changing_its_write_paths_after_approval_is_asked_about_again() {
    let probe = Probe::approved();
    let mut contributes: serde_json::Value = serde_json::from_str(extension_probe::MANIFEST)
        .map(|doc: serde_json::Value| doc["contributes"].clone())
        .expect("the manifest");
    contributes["writes"] = serde_json::json!(["notes/", "workspaces/*/todos/"]);
    probe.manifest_sets("contributes", contributes);

    let survey = extension::survey(&probe.config());
    assert_eq!(survey.installed[0].standing, extension::Standing::Changed);
}

#[test]
fn the_approval_prompt_names_every_capability_the_probe_asks_for() {
    let probe = Probe::assembled();
    let found = extension::install(&probe.config(), &probe.ext()).expect("installed");
    assert_eq!(
        found.manifest.capabilities,
        [
            extension::Capability::Probe,
            extension::Capability::Palette,
            extension::Capability::Actions,
            extension::Capability::Writes,
        ],
        "the probe's own manifest asks for every capability charter grants"
    );

    let asked = extension::prompt(&found, extension::Standing::New);
    // First, and in exactly the words the window draws (`Extensions.test.tsx` renders this
    // same sentence).
    assert_eq!(
        asked.declares[0],
        "the capability “probe” — charter's test capability, which grants nothing"
    );
    for (at, word) in [(1, "palette"), (2, "actions"), (3, "writes")] {
        assert!(
            asked.declares[at].starts_with(&format!("the capability “{word}” — ")),
            "{:?}",
            asked.declares[at]
        );
    }
}

#[test]
fn changing_the_capabilities_after_approval_is_asked_about_again_and_runs_nothing() {
    // The list is inside the manifest's bytes, so the fingerprint covers it: an extension that
    // changes what it asks for after the yes is refused at the press, not run on the old yes.
    let probe = Probe::approved();
    probe.manifest_sets(
        "capabilities",
        serde_json::json!(["writes", "actions", "palette", "probe"]),
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
