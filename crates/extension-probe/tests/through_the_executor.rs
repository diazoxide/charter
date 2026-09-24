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
        [extension::Capability::Probe],
        "the probe's own manifest asks for its capability"
    );

    let asked = extension::prompt(&found, extension::Standing::New);
    assert!(
        asked
            .declares
            .iter()
            .any(|line| line.starts_with("the capability “probe”")),
        "{:?}",
        asked.declares
    );
}

#[test]
fn changing_the_capabilities_after_approval_is_asked_about_again_and_runs_nothing() {
    // The list is inside the manifest's bytes, so the fingerprint covers it: an extension that
    // changes what it asks for after the yes is refused at the press, not run on the old yes.
    let probe = Probe::approved();
    probe.manifest_sets("capabilities", serde_json::json!([]));

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
