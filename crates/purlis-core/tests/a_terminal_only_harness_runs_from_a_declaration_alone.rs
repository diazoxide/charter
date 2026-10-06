//! FD-14's acceptance: **a new terminal-only harness runs from a declaration alone** (ADR 0073
//! §4). A project's `harnesses/<name>.toml` is the whole of it: no release of charter, no
//! profile, no adapter. What it gets is level 1, its terminal, and what stands between the
//! file and the program is the operator's approval on this machine, given once and asked
//! again after any change (V24b).
//!
//! The program these declarations name is `true`, because a declaration names a bare program
//! found on `PATH` and `true` is on every machine a test runs on. Nothing here runs it: a start
//! is everything up to the exec.

use std::fs;
use std::path::Path;

use purlis_core::harness::SessionId;
use purlis_core::harness_declaration;
use purlis_core::profiles::{self, Source};
use purlis_core::profiletrust;
use purlis_core::start::{self, Start};

const SHELLY: &str = r#"
name = "shelly"
title = "Shelly"
program = "true"
env = ["SHELLY_*"]

[session]
chosen_by = "charter"
new = ["--session={id}"]
resume = ["--resume={id}"]
named_by = ["--resume"]

[capabilities]
resumes_by_id = "yes"
"#;

struct Project {
    dir: tempfile::TempDir,
}

impl Project {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("charter.toml"), "").unwrap();
        Self { dir }
    }

    fn root(&self) -> &Path {
        self.dir.path()
    }

    fn declares(&self, name: &str, text: &str) -> &Self {
        let dir = self.root().join(harness_declaration::DIR);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(format!("{name}.toml")), text).unwrap();
        self
    }

    /// What the picker's approve does: records the operator's yes to the profile as shown.
    fn approve(&self, name: &str) {
        let set = profiles::current(self.root());
        let p = set.get(name).expect("a profile to approve");
        let shown = profiletrust::shown(self.root(), p);
        profiletrust::approve(self.root(), p, &shown).expect("the approval is recorded");
    }

    fn start(&self, name: &str) -> Start {
        Start {
            profile: Some(name.to_owned()),
            persona: None,
            name: "ide.7".to_owned(),
            cwd: Some(self.root().to_path_buf()),
            resume: None,
            show_footer: false,
            resuming: None,
            without_sandbox: None,
            grants: Default::default(),
        }
    }
}

#[test]
fn a_declared_harness_is_a_profile_named_after_it_that_runs_its_program() {
    purlis_core::unsteered!();
    let project = Project::new();
    project.declares("shelly", SHELLY);

    let set = profiles::current(project.root());

    let shelly = set.get("shelly").expect("the declaration gave a profile");
    assert_eq!(shelly.kind, "shelly");
    assert_eq!(shelly.harness, "shelly");
    assert_eq!(shelly.command, ["true"]);
    assert_eq!(shelly.source, Source::Declared);
    // After the built-ins, which keep their places.
    let names: Vec<&str> = set.profiles().iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["claude", "opencode", "codex", "shelly"]);
}

#[test]
fn a_refused_declaration_is_listed_with_the_profiles_refused_and_gives_no_profile() {
    purlis_core::unsteered!();
    let project = Project::new();
    project.declares("shelly", "name = \"shelly\"\nprogram = \"/bin/sh\"\n");

    let set = profiles::current(project.root());

    assert!(set.get("shelly").is_none());
    let refused = set
        .refused
        .iter()
        .find(|r| r.source == "harnesses/shelly.toml")
        .expect("the refusal is listed");
    assert!(
        refused.reason.contains("not a bare program name"),
        "{refused:?}"
    );
}

#[test]
fn a_profile_in_the_local_file_may_name_a_declared_harness_as_its_kind() {
    purlis_core::unsteered!();
    let project = Project::new();
    project.declares("shelly", SHELLY);
    fs::write(
        project.root().join(profiles::LOCAL_FILE),
        "[harness.shelly-work]\nkind = \"shelly\"\ncommand = [\"true\", \"-e\"]\n",
    )
    .unwrap();

    let set = profiles::current(project.root());

    let work = set
        .get("shelly-work")
        .unwrap_or_else(|| panic!("{:?}", set.refused));
    assert_eq!(work.kind, "shelly");
    assert_eq!(work.harness, "shelly");
    assert_eq!(work.source, Source::Local);
}

#[test]
fn a_declared_harness_is_not_started_until_the_operator_approves_it() {
    purlis_core::unsteered!();
    let project = Project::new();
    project.declares("shelly", SHELLY);

    let refused = start::ready(&project.start("shelly"), project.root())
        .expect_err("an unapproved declaration does not start");

    assert!(
        refused.contains("harnesses/shelly.toml") && refused.contains("nobody has approved it"),
        "{refused}"
    );
}

#[test]
fn an_approved_declaration_starts_its_program_at_level_one_under_the_id_charter_chose() {
    purlis_core::unsteered!();
    let project = Project::new();
    project.declares("shelly", SHELLY);
    project.approve("shelly");

    let ready = start::ready(&project.start("shelly"), project.root()).expect("it starts");

    assert!(ready.program.ends_with("/true"), "{}", ready.program);
    // Level 1: no adapter, so nothing arms it.
    assert_eq!(ready.harness, None);
    let id = ready.session.clone().expect("charter chose the id");
    assert_eq!(ready.args, [format!("--session={id}")]);
    assert!(
        ready
            .env
            .contains(&("PURLIS_HARNESS".to_owned(), "shelly".to_owned())),
        "{:?}",
        ready.env
    );
}

#[test]
fn a_declared_harness_resumes_by_its_own_template() {
    purlis_core::unsteered!();
    let project = Project::new();
    project.declares("shelly", SHELLY);
    project.approve("shelly");
    let id = SessionId::new("abc-123").unwrap();

    let ready = start::ready(
        &Start {
            resume: Some(id.clone()),
            ..project.start("shelly")
        },
        project.root(),
    )
    .expect("it starts");

    assert_eq!(ready.args, ["--resume=abc-123"]);
    assert_eq!(ready.session, Some(id));
}

#[test]
fn a_declaration_that_changed_since_it_was_approved_asks_again() {
    purlis_core::unsteered!();
    let project = Project::new();
    project.declares("shelly", SHELLY);
    project.approve("shelly");
    project.declares("shelly", &SHELLY.replace("--session", "--yolo-session"));

    let refused = start::ready(&project.start("shelly"), project.root())
        .expect_err("a changed declaration asks again");

    assert!(refused.contains("changed"), "{refused}");
}

#[test]
fn an_approval_is_recorded_in_its_own_store_keyed_by_the_declarations_name() {
    purlis_core::unsteered!();
    let project = Project::new();
    project.declares("shelly", SHELLY);
    project.approve("shelly");

    let record = fs::read_to_string(
        project
            .root()
            .join(".charter")
            .join(harness_declaration::APPROVED),
    )
    .expect("the store was written");
    let doc: serde_json::Value = serde_json::from_str(&record).unwrap();
    let digest = harness_declaration::read(project.root())
        .get("shelly")
        .unwrap()
        .digest
        .clone();
    assert_eq!(doc["shelly"]["digest"], digest.as_str());
    // Not the profile record: a declaration has no command of a profile's.
    assert!(
        !project
            .root()
            .join(".charter")
            .join(profiletrust::RECORD)
            .exists()
    );
}

#[test]
fn a_local_profile_on_a_declared_harness_asks_for_the_declaration_too() {
    purlis_core::unsteered!();
    let project = Project::new();
    project.declares("shelly", SHELLY);
    fs::write(
        project.root().join(profiles::LOCAL_FILE),
        "[harness.shelly-work]\nkind = \"shelly\"\ncommand = [\"true\"]\n",
    )
    .unwrap();
    let set = profiles::current(project.root());
    let work = set.get("shelly-work").unwrap().clone();
    profiletrust::record_launched(
        project.root(),
        &work.name,
        &profiletrust::fingerprint(&work),
    )
    .unwrap();

    let refused = start::ready(&project.start("shelly-work"), project.root())
        .expect_err("the declaration was never approved");
    assert!(refused.contains("harnesses/shelly.toml"), "{refused}");

    let shown = profiletrust::shown(project.root(), &work);
    profiletrust::approve(project.root(), &work, &shown).unwrap();
    start::ready(&project.start("shelly-work"), project.root()).expect("now it starts");
}

#[test]
fn a_declared_harness_in_a_sandboxed_project_is_not_started_unconfined() {
    purlis_core::unsteered!();
    // ADR 0067 §1: fail closed. A harness with no adapter has no compiled sandbox yet.
    let project = Project::new();
    fs::write(
        project.root().join("charter.toml"),
        "[sandbox]\nmode = \"on\"\n",
    )
    .unwrap();
    project.declares("shelly", SHELLY);
    project.approve("shelly");

    let refused = start::ready(&project.start("shelly"), project.root())
        .expect_err("a sandboxed project does not start an unconfined harness");

    assert!(refused.contains("sandbox"), "{refused}");
}

#[test]
fn the_approval_dialog_shows_every_word_that_will_run_and_the_whole_digest() {
    purlis_core::unsteered!();
    let project = Project::new();
    project.declares("shelly", SHELLY);
    let set = profiles::current(project.root());
    let shelly = set.get("shelly").unwrap();
    let digest = harness_declaration::read(project.root())
        .get("shelly")
        .unwrap()
        .digest
        .clone();

    let shown = profiletrust::shown(project.root(), shelly);

    assert_eq!(
        shown,
        format!(
            "true (kind shelly, declared in harnesses/shelly.toml, {digest}; program true; a new chat adds \
             '--session={{id}}'; a resumed chat adds '--resume={{id}}')"
        )
    );
    assert_eq!(digest.len(), "sha256:".len() + 64);
}

#[test]
fn a_declaration_changed_between_the_dialog_and_the_click_is_not_approved() {
    purlis_core::unsteered!();
    let project = Project::new();
    project.declares("shelly", SHELLY);
    let set = profiles::current(project.root());
    let shelly = set.get("shelly").unwrap().clone();
    let shown = profiletrust::shown(project.root(), &shelly);
    // Between the dialog and the click, the same program with other words.
    project.declares("shelly", &SHELLY.replace("--session", "--yolo"));

    let refused = profiletrust::approve(project.root(), &shelly, &shown)
        .expect_err("what was shown is not what is on disk");

    assert!(
        refused.contains("changed while you were reading it"),
        "{refused}"
    );
    assert!(refused.contains("--yolo"), "{refused}");
    assert!(
        !project
            .root()
            .join(".charter")
            .join(harness_declaration::APPROVED)
            .exists(),
        "nothing is recorded"
    );
    start::ready(&project.start("shelly"), project.root()).expect_err("and nothing starts");
}

#[test]
fn a_declaration_changed_between_the_check_and_the_start_runs_the_words_that_were_approved() {
    purlis_core::unsteered!();
    let project = Project::new();
    project.declares("shelly", SHELLY);
    project.approve("shelly");
    // The launch reads once; a writer then races it.
    let read = harness_declaration::read(project.root());
    project.declares("shelly", &SHELLY.replace("--session", "--yolo"));

    let ready = start::ready_in(&project.start("shelly"), project.root(), &read)
        .expect("the approved declaration starts");

    assert!(ready.args[0].starts_with("--session="), "{:?}", ready.args);
    // And a launch that reads after the change is refused.
    let refused = start::ready(&project.start("shelly"), project.root())
        .expect_err("the changed declaration was never approved");
    assert!(refused.contains("changed"), "{refused}");
}

#[test]
fn the_dialog_warns_beside_a_word_that_names_a_file_or_folder_in_the_project() {
    purlis_core::unsteered!();
    // Ruling of 2026-10-03: a word or value naming a file in the project is a residual the
    // approval covers, so the dialog says so beside it.
    let project = Project::new();
    project.declares(
        "shelly",
        "name = \"shelly\"\nprogram = \"true\"\n[session]\nchosen_by = \"charter\"\nnew = [\"--session={id}\", \"--cfg=rel.toml\", \"run\", \"--mode=fast\"]\n",
    );
    fs::write(project.root().join("rel.toml"), "x = 1\n").unwrap();
    fs::create_dir_all(project.root().join("run")).unwrap();
    let set = profiles::current(project.root());

    let shown = profiletrust::shown(project.root(), set.get("shelly").unwrap());

    assert!(
        shown.contains(
            "--cfg=rel.toml names the file rel.toml in this project; the program may read it"
        ),
        "{shown}"
    );
    assert!(
        shown.contains("run names the folder run in this project; the program may read it"),
        "{shown}"
    );
    assert!(!shown.contains("fast names"), "{shown}");
}

#[test]
fn a_declared_word_holding_a_space_never_reads_as_two_words() {
    purlis_core::unsteered!();
    // #1014: `["--a b"]` and `["--a", "b"]` run differently, so the approval says them
    // differently. A project's declaration refuses a word with a space today; the line
    // quotes anyway, so it never depends on that rule to read one way.
    let dir = tempfile::tempdir().unwrap();
    let harnesses = dir.path().join(harness_declaration::DIR);
    fs::create_dir_all(&harnesses).unwrap();
    fs::write(harnesses.join("shelly.toml"), SHELLY).unwrap();
    let declared = harness_declaration::read(dir.path());
    assert_eq!(declared.refused, Vec::new());
    let shelly = profiles::Profile {
        name: "shelly".into(),
        kind: "shelly".into(),
        harness: "shelly".into(),
        command: vec!["true".into()],
        env: Vec::new(),
        source: Source::Declared,
    };
    let shown = |new: &[&str]| {
        let mut declared = declared.clone();
        let d = declared
            .declared
            .iter_mut()
            .find(|d| d.name == "shelly")
            .expect("shelly is declared");
        d.session.new = new.iter().map(|w| (*w).to_owned()).collect();
        profiletrust::shown_in(dir.path(), &shelly, &declared)
    };

    let one = shown(&["--a b"]);
    let two = shown(&["--a", "b"]);

    assert!(one.contains("; a new chat adds '--a b';"), "{one}");
    assert!(two.contains("; a new chat adds --a b;"), "{two}");
}

#[test]
fn a_declared_template_of_the_word_nothing_never_reads_as_an_empty_one() {
    purlis_core::unsteered!();
    // #1014: the empty case is drawn in a form quoting never produces.
    let dir = tempfile::tempdir().unwrap();
    let harnesses = dir.path().join(harness_declaration::DIR);
    fs::create_dir_all(&harnesses).unwrap();
    fs::write(harnesses.join("shelly.toml"), SHELLY).unwrap();
    let declared = harness_declaration::read(dir.path());
    let shelly = profiles::Profile {
        name: "shelly".into(),
        kind: "shelly".into(),
        harness: "shelly".into(),
        command: vec!["true".into()],
        env: Vec::new(),
        source: Source::Declared,
    };
    let shown = |new: &[&str], resume: Option<&[&str]>| {
        let mut declared = declared.clone();
        let d = declared
            .declared
            .iter_mut()
            .find(|d| d.name == "shelly")
            .expect("shelly is declared");
        d.session.new = new.iter().map(|w| (*w).to_owned()).collect();
        d.session.resume = resume.map(|r| r.iter().map(|w| (*w).to_owned()).collect());
        profiletrust::shown_in(dir.path(), &shelly, &declared)
    };

    let empty = shown(&[], None);
    let words = shown(&["nothing"], Some(&["nothing,", "it", "cannot", "resume"]));

    assert_ne!(empty, words);
    assert!(
        empty.contains(&format!(
            "a new chat adds {}; a resumed chat adds {}",
            profiletrust::NOTHING,
            profiletrust::CANNOT_RESUME
        )),
        "{empty}"
    );
    assert!(
        words.contains("a new chat adds nothing; a resumed chat adds nothing, it cannot resume"),
        "{words}"
    );
}
