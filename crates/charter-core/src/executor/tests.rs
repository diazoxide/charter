//! The executor, attacked.
//!
//! **Each test here is a question an adversarial reviewer asks of the first thing in charter
//! that runs a stranger's code**, answered against a real program started by the real path:
//! can it run without approval, after its directory changed, after the record went bad, from
//! somewhere else; can it hang the caller, flood it, outlive it, or take another extension's
//! slot; does it get more of charter's environment than it needs; does anything it answers
//! reach the window outside the vocabulary. The mutation run that saw each one go red with its
//! guard removed is in the PR that landed them (ADR 0041 gate item 6).
//!
//! The programs are shell scripts written with `stand_in::program` (charter-app#81), and the
//! evidence that something did NOT run is a marker file it would have written — absence of a
//! side effect, checked after the refusal, which is the only honest way to test that nothing
//! started.

#![cfg(unix)]

use super::*;
use std::path::PathBuf;

/// An extension directory, a config root, and a place outside both for markers.
struct Rig {
    dir: tempfile::TempDir,
}

const VIEW_MANIFEST: &str = r#"{"version":1,"id":"probe","name":"Probe",
    "contributes":{"runs":"bin/run",
                   "views":[{"id":"stats","title":"Probe statistics","about":"personas"}]}}"#;

/// The one line a well-behaved program answers.
const ANSWER: &str = r#"{"charter":1,"blocks":[{"kind":"note","text":"answered"}]}"#;

impl Rig {
    fn new() -> Self {
        Self {
            dir: tempfile::tempdir().expect("a directory"),
        }
    }

    fn at(&self) -> PathBuf {
        self.dir.path().join("ext")
    }

    fn config(&self) -> PathBuf {
        self.dir.path().join("config")
    }

    /// A path outside the extension, so a marker written there changes no fingerprint.
    fn marker(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    /// Write the manifest and the program, and nothing else.
    fn write(&self, manifest: &str, script: &str) {
        std::fs::create_dir_all(self.at().join("bin")).expect("the directory");
        std::fs::write(self.at().join(extension::MANIFEST), manifest).expect("the manifest");
        stand_in::program(&self.at().join("bin"), "run", script);
    }

    /// Install it and approve exactly what is on disk, as the dialog's two clicks do.
    fn approved(&self, script: &str) -> &Self {
        self.write(VIEW_MANIFEST, script);
        let found = extension::install(&self.config(), &self.at()).expect("installed");
        extension::approve(&self.config(), found.id(), &found.path, &found.fingerprint)
            .expect("approved");
        self
    }

    fn ask(&self, executor: &Executor) -> Result<Answer, String> {
        executor.ask(
            &self.config(),
            "probe",
            "stats",
            None,
            |_| serde_json::json!({ "handed": true }),
        )
    }
}

/// A script that leaves a marker, then answers.
fn marking(marker: &Path) -> String {
    format!(
        "#!/bin/sh\ntouch '{}'\nread line\nprintf '%s\\n' '{ANSWER}'\n",
        marker.display()
    )
}

/// Whether `pid` is gone, waiting a little for it to be.
///
/// **Not one `kill(pid, 0)`**: a killed helper whose parent died with it is a zombie until init
/// reaps it, and a zombie answers the null signal. On a Linux runner that reaping is a few
/// milliseconds after the kill — long enough for an immediate check to read "alive" and a test
/// to go red for a process that is in fact dead.
fn gone(pid: u32) -> bool {
    let until = Instant::now() + Duration::from_secs(5);
    while crate::process::alive(pid) {
        if Instant::now() > until {
            return false;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    true
}

fn alive_from(marker: &Path) -> Option<u32> {
    std::fs::read_to_string(marker)
        .ok()
        .and_then(|text| text.trim().parse().ok())
}

/// Wait for a program to write its pid, up to a bound, for a test that has to act while it
/// runs. Until the file PARSES, not until it exists: `echo $$ > pid` creates the file before it
/// writes to it, and a test that acted in between killed a program that had not said who it
/// was yet.
fn wait_for(marker: &Path) {
    let until = Instant::now() + Duration::from_secs(10);
    while alive_from(marker).is_none() {
        assert!(
            Instant::now() < until,
            "{} never appeared",
            marker.display()
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

// -------------------------------------------------------------------------------------
// Can a program run without approval?
// -------------------------------------------------------------------------------------

#[test]
fn an_approved_program_is_asked_and_its_answer_is_drawn() {
    let rig = Rig::new();
    let marker = rig.marker("ran");
    rig.approved(&marking(&marker));

    let answer = rig.ask(&Executor::default()).expect("an answer");

    assert!(marker.exists(), "the program did not run");
    assert_eq!(
        answer.blocks,
        vec![panel::Block::Note {
            text: "answered".into(),
            tone: panel::Tone::Plain
        }]
    );
}

#[test]
fn an_installed_program_the_operator_has_not_approved_never_starts() {
    let rig = Rig::new();
    let marker = rig.marker("ran");
    rig.write(VIEW_MANIFEST, &marking(&marker));
    extension::install(&rig.config(), &rig.at()).expect("installed");

    let refused = rig
        .ask(&Executor::default())
        .expect_err("it ran unapproved");

    assert!(refused.contains("not approved"), "{refused}");
    assert!(!marker.exists(), "an unapproved program ran");
}

#[test]
fn a_program_nobody_installed_never_starts() {
    let rig = Rig::new();
    let marker = rig.marker("ran");
    rig.write(VIEW_MANIFEST, &marking(&marker));

    let refused = rig
        .ask(&Executor::default())
        .expect_err("it ran uninstalled");

    assert!(refused.contains("is installed"), "{refused}");
    assert!(!marker.exists());
}

// -------------------------------------------------------------------------------------
// ...after its directory changed? The gate is re-taken at the press, over the whole tree.
// -------------------------------------------------------------------------------------

#[test]
fn a_program_whose_own_bytes_changed_after_approval_is_asked_about_again_rather_than_run() {
    let rig = Rig::new();
    let marker = rig.marker("ran");
    rig.approved(&marking(&marker));
    // Rewritten after the yes: the attack the whole fingerprint exists for.
    stand_in::program(
        &rig.at().join("bin"),
        "run",
        &format!("{}# and something new\n", marking(&marker)),
    );

    let refused = rig.ask(&Executor::default()).expect_err("it ran changed");

    assert!(
        refused.contains("changed since you approved it"),
        "{refused}"
    );
    assert!(
        !marker.exists(),
        "a program that changed after approval ran"
    );
}

#[test]
fn an_undeclared_file_added_beside_the_program_stops_it_running() {
    // charter-app#152's case, at the moment it matters: a `.dylib` or a sourced script dropped
    // next to an approved program is exactly what the program would load.
    let rig = Rig::new();
    let marker = rig.marker("ran");
    rig.approved(&marking(&marker));
    std::fs::write(rig.at().join("bin/helper.sh"), "echo planted\n").expect("a sibling");

    let refused = rig.ask(&Executor::default()).expect_err("it ran");

    assert!(
        refused.contains("changed since you approved it"),
        "{refused}"
    );
    assert!(!marker.exists());
}

#[test]
fn a_write_into_the_state_directory_does_not_stop_it_running() {
    // The one carve-out, and the reason it exists: an extension that keeps a cache is not
    // re-asked about every time it writes one.
    let rig = Rig::new();
    let marker = rig.marker("ran");
    let manifest =
        VIEW_MANIFEST.replace(r#""name":"Probe","#, r#""name":"Probe","state":"cache","#);
    rig.write(&manifest, &marking(&marker));
    let found = extension::install(&rig.config(), &rig.at()).expect("installed");
    extension::approve(&rig.config(), found.id(), &found.path, &found.fingerprint)
        .expect("approved");
    std::fs::create_dir_all(rig.at().join("cache")).expect("the state directory");
    std::fs::write(rig.at().join("cache/seen.json"), "{}").expect("state");

    rig.ask(&Executor::default()).expect("an answer");
    assert!(marker.exists());
}

#[test]
fn an_approval_recorded_at_another_path_does_not_start_the_program_here() {
    let rig = Rig::new();
    let marker = rig.marker("ran");
    rig.approved(&marking(&marker));
    let elsewhere = rig.dir.path().join("moved");
    std::fs::rename(rig.at(), &elsewhere).expect("moved");
    std::fs::create_dir_all(rig.at()).expect("an empty directory where it was");

    let refused = rig.ask(&Executor::default()).expect_err("it ran");

    assert!(refused.contains("could not re-read"), "{refused}");
    assert!(!marker.exists());
}

#[test]
fn an_extension_is_started_under_its_own_approval_and_never_under_another_s() {
    // The record is keyed by id and `standing` looks the entry up by the id the MANIFEST says.
    // Two rows can name one directory — installed once as `probe`, its manifest then edited to
    // say `other` and installed again — and without the executor's own id check, asking for
    // `probe` would run that directory's program on `other`'s yes: an approval borrowed across
    // names, which is the one thing a per-extension yes exists to prevent.
    let rig = Rig::new();
    let marker = rig.marker("ran");
    rig.approved(&marking(&marker));
    std::fs::write(
        rig.at().join(extension::MANIFEST),
        VIEW_MANIFEST.replace(r#""id":"probe""#, r#""id":"other""#),
    )
    .expect("the manifest renamed");
    let other = extension::install(&rig.config(), &rig.at()).expect("installed as other");
    extension::approve(&rig.config(), other.id(), &other.path, &other.fingerprint)
        .expect("other approved");

    let refused = rig
        .ask(&Executor::default())
        .expect_err("probe ran on other's approval");

    assert!(
        refused.contains("changed since you approved it"),
        "{refused}"
    );
    assert!(
        !marker.exists(),
        "a program ran under another extension's approval"
    );
}

#[test]
fn a_record_charter_cannot_read_starts_nothing() {
    let rig = Rig::new();
    let marker = rig.marker("ran");
    rig.approved(&marking(&marker));
    std::fs::write(extension::file(&rig.config()), "{ not json").expect("a broken record");

    let refused = rig.ask(&Executor::default()).expect_err("it ran");

    assert!(refused.contains("could not read its record"), "{refused}");
    assert!(!marker.exists());
}

#[test]
fn a_program_that_is_not_executable_is_refused_with_what_to_do() {
    let rig = Rig::new();
    rig.write(VIEW_MANIFEST, "#!/bin/sh\necho hi\n");
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            rig.at().join("bin/run"),
            std::fs::Permissions::from_mode(0o644),
        )
        .expect("the mode");
    }
    let found = extension::install(&rig.config(), &rig.at()).expect("installed");
    extension::approve(&rig.config(), found.id(), &found.path, &found.fingerprint)
        .expect("approved");

    let refused = rig.ask(&Executor::default()).expect_err("it ran");

    assert!(refused.contains("not executable"), "{refused}");
}

#[test]
fn a_view_the_extension_does_not_declare_starts_nothing() {
    let rig = Rig::new();
    let marker = rig.marker("ran");
    rig.approved(&marking(&marker));

    let refused = Executor::default()
        .ask(&rig.config(), "probe", "elsewhere", None, |_| {
            serde_json::Value::Null
        })
        .expect_err("it ran");

    assert!(refused.contains("no view called"), "{refused}");
    assert!(!marker.exists());
}

// -------------------------------------------------------------------------------------
// Can it stall the caller, flood it, or crash it?
// -------------------------------------------------------------------------------------

#[test]
fn a_program_that_never_answers_is_stopped_at_the_deadline() {
    let rig = Rig::new();
    let pid = rig.marker("pid");
    rig.approved(&format!(
        "#!/bin/sh\necho $$ > '{}'\nexec sleep 60\n",
        pid.display()
    ));

    let began = Instant::now();
    let refused = rig
        .ask(&Executor::default())
        .expect_err("an answer from nothing");
    let took = began.elapsed();

    assert!(refused.contains("did not answer within"), "{refused}");
    assert!(
        took < DEADLINE + Duration::from_secs(2),
        "the caller waited {took:?}, past the deadline"
    );
    let pid = alive_from(&pid).expect("its pid");
    assert!(gone(pid), "a program that timed out is still running");
}

#[test]
fn a_program_that_answers_forever_is_cut_off_without_any_of_it_drawn() {
    let rig = Rig::new();
    rig.approved("#!/bin/sh\nexec yes x\n");

    let refused = rig
        .ask(&Executor::default())
        .expect_err("a flood was drawn");

    // `yes` writes newlines, so the first line is `x` and it is not an answer. The case this
    // pins is the one below; this one pins that a line that is not JSON is refused as such.
    assert!(refused.contains("not one line of JSON"), "{refused}");
}

#[test]
fn a_line_longer_than_charter_reads_is_refused_as_too_much() {
    let rig = Rig::new();
    rig.approved("#!/bin/sh\nexec tr '\\0' x < /dev/zero\n");

    let refused = rig
        .ask(&Executor::default())
        .expect_err("a flood was drawn");

    assert!(refused.contains("more than"), "{refused}");
}

#[test]
fn a_program_that_crashes_says_so_with_its_own_last_words() {
    let rig = Rig::new();
    rig.approved("#!/bin/sh\necho 'thread main panicked at src/main.rs' >&2\nexit 3\n");

    let refused = rig
        .ask(&Executor::default())
        .expect_err("a crash was drawn");

    assert!(refused.contains("exited with status 3"), "{refused}");
    assert!(refused.contains("panicked at"), "{refused}");
}

#[test]
fn a_program_that_logs_more_than_a_socket_holds_is_not_mistaken_for_a_hung_one() {
    // stderr is drained while the program runs. Without that, a chatty program blocks on its
    // own diagnostics and reads as hung — a program doing nothing wrong refused as a stall.
    let rig = Rig::new();
    rig.approved(&format!(
        "#!/bin/sh\nhead -c 262144 /dev/zero | tr '\\0' e >&2\nread line\nprintf '%s\\n' '{ANSWER}'\n"
    ));

    rig.ask(&Executor::default())
        .expect("the answer after the noise");
}

// -------------------------------------------------------------------------------------
// Can it outlive the question, or the window?
// -------------------------------------------------------------------------------------

#[test]
fn a_helper_the_program_started_is_stopped_with_it() {
    let rig = Rig::new();
    let helper = rig.marker("helper");
    rig.approved(&format!(
        "#!/bin/sh\nsleep 60 &\necho $! > '{}'\nread line\nprintf '%s\\n' '{ANSWER}'\n",
        helper.display()
    ));

    rig.ask(&Executor::default()).expect("an answer");

    let helper = alive_from(&helper).expect("the helper's pid");
    assert!(
        gone(helper),
        "a helper outlived the question it was started for"
    );
}

#[test]
fn stopping_everything_kills_a_program_that_is_still_answering() {
    // What the app does at exit. A program mid-question is killed rather than left behind the
    // window that asked it.
    let rig = Rig::new();
    let pid = rig.marker("pid");
    rig.approved(&format!(
        "#!/bin/sh\necho $$ > '{}'\nexec sleep 60\n",
        pid.display()
    ));
    let executor = std::sync::Arc::new(Executor::default());
    let asking = {
        let executor = std::sync::Arc::clone(&executor);
        let config = rig.config();
        std::thread::spawn(move || {
            let began = Instant::now();
            let said = executor.ask(&config, "probe", "stats", None, |_| serde_json::Value::Null);
            (said, began.elapsed())
        })
    };
    wait_for(&pid);
    // The table knows the group only once `started` has run, a moment after the fork; the pid
    // file can appear before that.
    let until = Instant::now() + Duration::from_secs(5);
    while executor
        .running
        .lock()
        .expect("the table")
        .get("probe")
        .copied()
        .unwrap_or(0)
        == 0
    {
        assert!(
            Instant::now() < until,
            "the program never reached the table"
        );
        std::thread::sleep(Duration::from_millis(5));
    }

    executor.stop_all();
    let (said, took) = asking.join().expect("the asking thread");

    assert!(said.is_err(), "a killed program answered: {said:?}");
    assert!(
        took < DEADLINE,
        "stopping everything waited out the deadline ({took:?})"
    );
    let pid = alive_from(&pid).expect("its pid");
    assert!(gone(pid), "the program outlived stop_all");
    assert!(executor.running().is_empty(), "the slot was never released");
}

#[test]
fn a_program_that_escapes_its_group_outlives_the_question_and_cannot_hold_charter_up() {
    // **The honest limit, pinned rather than hidden.** A helper that leaves the process group
    // is not killed with it — the operator declined a sandbox, and running as him it can do
    // that. What charter guarantees is narrower and is what this holds: the question still
    // ends on time, and the escaped helper writing to stderr for ever does not hold the
    // blocking thread or the extension's slot. Perl, because `setsid(1)` is not on macOS and
    // perl is on every machine this runs on.
    let rig = Rig::new();
    let escaped = rig.marker("escaped");
    rig.approved(&format!(
        "#!/bin/sh\nperl -e 'setpgrp(0,0); open(F,\">{}\"); print F $$; close F; \
         $|=1; while(1){{print STDERR \"x\"; select(undef,undef,undef,0.001)}}' &\n\
         read line\nprintf '%s\\n' '{ANSWER}'\n",
        escaped.display()
    ));
    let executor = Executor::default();

    let began = Instant::now();
    rig.ask(&executor).expect("the answer");
    let took = began.elapsed();

    wait_for(&escaped);
    let pid = alive_from(&escaped).expect("the escaped helper's pid");
    let outlived = crate::process::alive(pid);
    // Killed by the pid this test itself caused to exist, never by name.
    if let Some(it) = rustix::process::Pid::from_raw(i32::try_from(pid).unwrap_or(0)) {
        let _ = rustix::process::kill_process(it, rustix::process::Signal::KILL);
    }
    assert!(
        took < Duration::from_secs(3),
        "an escaped helper held the question for {took:?}"
    );
    assert!(executor.running().is_empty(), "the slot was never released");
    assert!(
        outlived,
        "the escaped helper was killed — if charter can now do that, this test and the \
         executor's docstring are both out of date"
    );
}

// -------------------------------------------------------------------------------------
// Can one extension's program affect another's?
// -------------------------------------------------------------------------------------

/// A script that says it has started, then waits for the test to say go before answering —
/// so a test can act while it is running without racing a sleep.
///
/// **Not a `sleep`, and the reason is measured.** macOS assesses a program file the first time
/// it is executed (~200 ms here, alone), and under a parallel test run those assessments queue:
/// a `sleep 1` program took 4.2 s to answer with the rest of this module running beside it.
fn waiting(pid: &Path, go: &Path) -> String {
    format!(
        "#!/bin/sh\necho $$ > '{}'\nwhile [ ! -e '{}' ]; do sleep 0.02; done\nread line\n\
         printf '%s\\n' '{ANSWER}'\n",
        pid.display(),
        go.display()
    )
}

#[test]
fn one_extension_is_asked_one_thing_at_a_time() {
    let rig = Rig::new();
    let pid = rig.marker("pid");
    let go = rig.marker("go");
    rig.approved(&waiting(&pid, &go));
    let executor = std::sync::Arc::new(Executor::default());
    let first = {
        let executor = std::sync::Arc::clone(&executor);
        let config = rig.config();
        std::thread::spawn(move || {
            executor.ask(&config, "probe", "stats", None, |_| serde_json::Value::Null)
        })
    };
    wait_for(&pid);

    let second = rig.ask(&executor).expect_err("a second copy started");
    assert!(second.contains("still answering"), "{second}");
    std::fs::write(&go, "").expect("go");
    first
        .join()
        .expect("the first thread")
        .expect("the first answer");
    // And the slot comes back.
    rig.ask(&executor)
        .expect("asked again once the first was done");
}

#[test]
fn a_busy_extension_does_not_hold_up_a_different_one() {
    let slow = Rig::new();
    let pid = slow.marker("pid");
    slow.approved(&format!(
        "#!/bin/sh\necho $$ > '{}'\nexec sleep 60\n",
        pid.display()
    ));
    // A second extension, installed into the same record under another id.
    let quick_at = slow.dir.path().join("quick");
    std::fs::create_dir_all(quick_at.join("bin")).expect("the directory");
    std::fs::write(
        quick_at.join(extension::MANIFEST),
        VIEW_MANIFEST.replace(r#""id":"probe""#, r#""id":"quick""#),
    )
    .expect("the manifest");
    stand_in::program(
        &quick_at.join("bin"),
        "run",
        &format!("#!/bin/sh\nread line\nprintf '%s\\n' '{ANSWER}'\n"),
    );
    let found = extension::install(&slow.config(), &quick_at).expect("installed");
    extension::approve(&slow.config(), found.id(), &found.path, &found.fingerprint)
        .expect("approved");

    let executor = std::sync::Arc::new(Executor::default());
    // Asked once before the measurement, so what is timed below is the executor and not macOS
    // assessing a program file it has never seen (see `waiting`).
    executor
        .ask(&slow.config(), "quick", "stats", None, |_| {
            serde_json::Value::Null
        })
        .expect("the quick one, warmed");
    let stalled = {
        let executor = std::sync::Arc::clone(&executor);
        let config = slow.config();
        std::thread::spawn(move || {
            executor.ask(&config, "probe", "stats", None, |_| serde_json::Value::Null)
        })
    };
    wait_for(&pid);

    let began = Instant::now();
    executor
        .ask(&slow.config(), "quick", "stats", None, |_| {
            serde_json::Value::Null
        })
        .expect("the quick one answered");
    assert!(
        began.elapsed() < Duration::from_secs(2),
        "one extension waited on another's stall"
    );
    executor.stop_all();
    let _ = stalled.join();
}

// -------------------------------------------------------------------------------------
// What it is given, and where
// -------------------------------------------------------------------------------------

#[test]
fn the_program_is_asked_one_line_holding_what_was_handed_and_nothing_else() {
    let rig = Rig::new();
    let asked = rig.marker("asked");
    rig.approved(&format!(
        "#!/bin/sh\ncat > '{}'\nprintf '%s\\n' '{ANSWER}'\n",
        asked.display()
    ));

    Executor::default()
        .ask(&rig.config(), "probe", "stats", Some("steward"), |about| {
            assert_eq!(about, Subject::Personas);
            serde_json::json!({ "handed": [1, 2] })
        })
        .expect("an answer");

    let text = std::fs::read_to_string(&asked).expect("the question");
    assert!(
        text.ends_with('\n') && text.matches('\n').count() == 1,
        "{text:?}"
    );
    let asked: serde_json::Value = serde_json::from_str(&text).expect("JSON");
    assert_eq!(
        asked,
        serde_json::json!({
            "charter": PROTOCOL, "extension": "probe", "view": "stats", "about": "personas",
            "focus": "steward", "given": { "handed": [1, 2] }
        })
    );
}

#[test]
fn the_program_gets_a_program_s_environment_and_not_charter_s() {
    let rig = Rig::new();
    let seen = rig.marker("env");
    rig.approved(&format!(
        "#!/bin/sh\nenv > '{}'\nread line\nprintf '%s\\n' '{ANSWER}'\n",
        seen.display()
    ));

    rig.ask(&Executor::default()).expect("an answer");

    let text = std::fs::read_to_string(&seen).expect("its environment");
    let names: Vec<&str> = text
        .lines()
        .filter_map(|line| line.split_once('=').map(|(name, _)| name))
        .collect();
    // `cargo test` runs this with dozens of CARGO_* variables set. None may reach a stranger.
    assert!(
        names.iter().all(|name| !name.starts_with("CARGO")),
        "charter's own environment reached the program: {names:?}"
    );
    assert!(names.contains(&"CHARTER_EXTENSION"), "{names:?}");
    assert!(
        !names.contains(&"CHARTER_PLANE_FENCE"),
        "the plane fence was handed to a stranger: {names:?}"
    );
    assert!(text.contains("CHARTER_EXTENSION=probe"), "{text}");
}

#[test]
fn the_program_runs_in_its_own_directory() {
    let rig = Rig::new();
    let seen = rig.marker("cwd");
    rig.approved(&format!(
        "#!/bin/sh\npwd -P > '{}'\nread line\nprintf '%s\\n' '{ANSWER}'\n",
        seen.display()
    ));

    rig.ask(&Executor::default()).expect("an answer");

    let cwd = std::fs::read_to_string(&seen).expect("its directory");
    assert_eq!(
        PathBuf::from(cwd.trim()),
        rig.at().canonicalize().expect("the extension")
    );
}

#[test]
fn nothing_is_handed_to_a_program_the_gate_refused() {
    // `hand` reads the plane. A refused extension must cost no plane read at all.
    let rig = Rig::new();
    rig.write(VIEW_MANIFEST, "#!/bin/sh\n");
    extension::install(&rig.config(), &rig.at()).expect("installed");

    let mut handed = false;
    let _ = Executor::default().ask(&rig.config(), "probe", "stats", None, |_| {
        handed = true;
        serde_json::Value::Null
    });
    assert!(
        !handed,
        "charter read the plane for an extension it would not run"
    );
}

// -------------------------------------------------------------------------------------
// Does anything it answers reach the window outside the vocabulary?
// -------------------------------------------------------------------------------------

fn answering(line: &str) -> Result<Answer, String> {
    let rig = Rig::new();
    rig.approved(&format!("#!/bin/sh\nread line\nprintf '%s\\n' '{line}'\n"));
    rig.ask(&Executor::default())
}

#[test]
fn an_answer_that_puts_a_verb_on_a_row_is_refused() {
    let refused = answering(
        r#"{"charter":1,"blocks":[{"kind":"list","rows":[{"key":"a","text":"a","runs":"chat.new"}]}]}"#,
    )
    .expect_err("a verb reached the window");
    assert!(refused.contains(panel::NO_VERB), "{refused}");
}

#[test]
fn an_answer_that_is_not_a_block_charter_draws_is_refused() {
    let refused = answering(r#"{"charter":1,"blocks":[{"kind":"html","html":"<script>"}]}"#)
        .expect_err("markup reached the window");
    assert!(refused.contains("\"html\""), "{refused}");
}

#[test]
fn an_answer_in_another_protocol_is_refused_with_the_number() {
    let refused = answering(r#"{"charter":2,"blocks":[]}"#).expect_err("read under other rules");
    assert!(refused.contains("protocol 2"), "{refused}");
}

#[test]
fn an_answer_that_says_no_protocol_is_refused() {
    let refused = answering(r#"{"blocks":[]}"#).expect_err("read with no protocol");
    assert!(refused.contains("which protocol"), "{refused}");
}

#[test]
fn an_error_the_program_answers_is_said_in_its_words() {
    let refused = answering(r#"{"charter":1,"error":"no personas to count"}"#).expect_err("drawn");
    assert!(refused.contains("no personas to count"), "{refused}");
}

#[test]
fn a_chart_is_drawn_from_an_answer() {
    let answer = answering(
        r#"{"charter":1,"blocks":[{"kind":"chart","title":"Memories","shape":"bars","unit":"memories","points":[{"label":"steward","value":3}]}]}"#,
    )
    .expect("an answer");
    assert_eq!(
        answer.blocks,
        vec![panel::Block::Chart(panel::Chart {
            title: "Memories".into(),
            shape: panel::Shape::Bars,
            unit: Some("memories".into()),
            points: vec![panel::Point {
                label: "steward".into(),
                value: 3,
                note: None
            }],
        })]
    );
}

// -------------------------------------------------------------------------------------
// What it costs
// -------------------------------------------------------------------------------------

#[test]
#[ignore = "a measurement, not a guard: cargo test -p charter-core executor -- --ignored --nocapture"]
fn what_a_round_trip_costs() {
    // ADR 0041 gate item 8: *"the plugin round trip is measured against ADR 0026's limits
    // before the protocol is fixed. hookwire's 1.8 ms is a one-way line, not a round trip."*
    // This is the round trip with a shell program that answers at once — the floor, which is
    // the cost of the protocol rather than of any program's work.
    let rig = Rig::new();
    rig.approved(&format!(
        "#!/bin/sh\nread line\nprintf '%s\\n' '{ANSWER}'\n"
    ));
    let executor = Executor::default();
    rig.ask(&executor).expect("a warm-up");
    let rounds = 20u32;
    let (mut gate, mut trip, mut whole) = (Duration::ZERO, Duration::ZERO, Duration::ZERO);
    for _ in 0..rounds {
        let began = Instant::now();
        let answer = rig.ask(&executor).expect("an answer");
        whole += began.elapsed();
        gate += answer.gate;
        trip += answer.round_trip;
    }
    println!(
        "gate {:?} + round trip {:?} = {:?} per ask ({rounds} rounds, a /bin/sh program)",
        gate / rounds,
        trip / rounds,
        whole / rounds
    );
}
