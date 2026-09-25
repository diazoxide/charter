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
use crate::extension::project;
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
            &project::Choices::default(),
            "probe",
            "stats",
            None,
            |_| serde_json::json!({ "handed": true }),
        )
    }
}

/// The deadline of every executor here but the one test whose subject is [`DEADLINE`].
///
/// **Not the real five seconds, because this module's programs are strangers to the machine**
/// (charter-app#303). macOS assesses a program file the first time it runs, and under a busy
/// machine those assessments queue: at load 30 to 113, a dozen tests here were refused "did not
/// answer within 5 seconds" by programs that had not started yet. None of them was about the
/// deadline. A test that measures a bound measures it against [`Executor::with_deadline`]'s,
/// so it stays a test of the same thing, and a regression it catches takes this long to fail.
const PATIENT: Duration = Duration::from_secs(30);

/// An executor that gives its programs [`PATIENT`].
fn patient() -> Executor {
    Executor::with_deadline(PATIENT)
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
/// was yet. The bound is [`PATIENT`], the time a program here is given to start: it returns
/// the moment the file parses, so the bound costs a passing test nothing.
fn wait_for(marker: &Path) {
    let until = Instant::now() + PATIENT;
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

    let answer = rig.ask(&patient()).expect("an answer");

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

    let refused = rig.ask(&patient()).expect_err("it ran unapproved");

    assert!(refused.contains("not approved"), "{refused}");
    assert!(!marker.exists(), "an unapproved program ran");
}

#[test]
fn a_program_nobody_installed_never_starts() {
    let rig = Rig::new();
    let marker = rig.marker("ran");
    rig.write(VIEW_MANIFEST, &marking(&marker));

    let refused = rig.ask(&patient()).expect_err("it ran uninstalled");

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

    let refused = rig.ask(&patient()).expect_err("it ran changed");

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

    let refused = rig.ask(&patient()).expect_err("it ran");

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

    rig.ask(&patient()).expect("an answer");
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

    let refused = rig.ask(&patient()).expect_err("it ran");

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
        .ask(&patient())
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

    let refused = rig.ask(&patient()).expect_err("it ran");

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

    let refused = rig.ask(&patient()).expect_err("it ran");

    assert!(refused.contains("not executable"), "{refused}");
}

#[test]
fn a_view_the_extension_does_not_declare_starts_nothing() {
    let rig = Rig::new();
    let marker = rig.marker("ran");
    rig.approved(&marking(&marker));

    let refused = patient()
        .ask(
            &rig.config(),
            &project::Choices::default(),
            "probe",
            "elsewhere",
            None,
            |_| serde_json::Value::Null,
        )
        .expect_err("it ran");

    assert!(refused.contains("no view called"), "{refused}");
    assert!(!marker.exists());
}

// -------------------------------------------------------------------------------------
// Can it stall the caller, flood it, or crash it?
// -------------------------------------------------------------------------------------

#[test]
fn a_program_that_never_answers_is_refused_as_too_late_never_as_a_lost_connection() {
    // What the refusal SAYS, whatever the machine's load: a program that never answers has
    // nothing to say by any deadline, so a short one decides nothing but how long this takes.
    // How long the real one is, is the test below's subject.
    let rig = Rig::new();
    rig.approved("#!/bin/sh\nexec sleep 60\n");

    let refused = rig
        .ask(&Executor::with_deadline(Duration::from_secs(1)))
        .expect_err("an answer from nothing");

    assert!(
        refused.starts_with("'probe' did not answer within 1 seconds, so charter stopped it."),
        "{refused}"
    );
}

#[test]
fn a_program_that_never_answers_is_stopped_at_the_deadline() {
    // **The one test on the real executor**, because its subject is the real [`DEADLINE`]:
    // what every executor charter makes gives a program. It is the one test here a machine too
    // busy to start a shell in five seconds can still fail (charter-app#303).
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

    let refused = rig.ask(&patient()).expect_err("a flood was drawn");

    // `yes` writes newlines, so the first line is `x` and it is not an answer. The case this
    // pins is the one below; this one pins that a line that is not JSON is refused as such.
    assert!(refused.contains("not one line of JSON"), "{refused}");
}

#[test]
fn a_line_longer_than_charter_reads_is_refused_as_too_much() {
    let rig = Rig::new();
    rig.approved("#!/bin/sh\nexec tr '\\0' x < /dev/zero\n");

    let refused = rig.ask(&patient()).expect_err("a flood was drawn");

    assert!(refused.contains("more than"), "{refused}");
}

#[test]
fn a_program_that_crashes_says_so_with_its_own_last_words() {
    let rig = Rig::new();
    rig.approved("#!/bin/sh\necho 'thread main panicked at src/main.rs' >&2\nexit 3\n");

    let refused = rig.ask(&patient()).expect_err("a crash was drawn");

    assert!(refused.contains("exited with status 3"), "{refused}");
    assert!(refused.contains("panicked at"), "{refused}");
}

#[test]
fn a_program_that_dies_without_reading_its_question_is_reported_as_ended_not_as_a_lost_connection()
{
    // It sleeps first, so the whole question is sitting unread in its socket when it exits.
    // On Linux that makes charter's read return `ECONNRESET` instead of end-of-file, every
    // time — which is what turned main red when the timing of the test above allowed it.
    let rig = Rig::new();
    rig.approved("#!/bin/sh\nsleep 0.3\necho 'thread main panicked at src/main.rs' >&2\nexit 3\n");

    let refused = rig.ask(&patient()).expect_err("a crash was drawn");

    assert!(refused.contains("exited with status 3"), "{refused}");
    assert!(refused.contains("panicked at"), "{refused}");
    assert!(!refused.contains("lost its connection"), "{refused}");
}

#[test]
fn a_program_that_logs_more_than_a_socket_holds_is_not_mistaken_for_a_hung_one() {
    // stderr is drained while the program runs. Without that, a chatty program blocks on its
    // own diagnostics and reads as hung — a program doing nothing wrong refused as a stall.
    let rig = Rig::new();
    rig.approved(&format!(
        "#!/bin/sh\nhead -c 262144 /dev/zero | tr '\\0' e >&2\nread line\nprintf '%s\\n' '{ANSWER}'\n"
    ));

    rig.ask(&patient()).expect("the answer after the noise");
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

    rig.ask(&patient()).expect("an answer");

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
    let executor = std::sync::Arc::new(patient());
    let asking = {
        let executor = std::sync::Arc::clone(&executor);
        let config = rig.config();
        std::thread::spawn(move || {
            let began = Instant::now();
            let said = executor.ask(
                &config,
                &project::Choices::default(),
                "probe",
                "stats",
                None,
                |_| serde_json::Value::Null,
            );
            (said, began.elapsed())
        })
    };
    wait_for(&pid);
    // No waiting for the table to learn the group: a `stop_all` that comes between the program
    // starting and its group being recorded is caught as it is recorded (`Executor::started`).

    executor.stop_all();
    let (said, took) = asking.join().expect("the asking thread");

    // What ended it was `stop_all`, not the deadline: said by the refusal itself, which names
    // the deadline only when the deadline is what stopped it, and by the clock against this
    // executor's own deadline.
    let said = said.expect_err("a killed program answered");
    assert!(
        !said.contains("did not answer within"),
        "stopping everything waited out the deadline: {said}"
    );
    assert!(
        took < PATIENT,
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
    // ends, and the escaped helper writing to stderr for ever does not hold the blocking
    // thread or the extension's slot. Perl, because `setsid(1)` is not on macOS and perl is on
    // every machine this runs on.
    //
    // **The evidence is an order, not a clock** (charter-app#287). This used to assert the
    // question took under 3 s, and a loaded Mac took longer without charter holding anything
    // up: `spawn` measured under 1 ms and charter's part after the answer ~100 ms
    // ([`STDERR_AFTER_STOP`]); the rest was the program starting and answering, which queued
    // first-run assessments stretch without limit. So the helper now runs until the test lets
    // it go, and the question returning while it is still alive is the proof that charter did
    // not wait for it. The watchdog only turns a regression into a sentence instead of a run
    // that never ends; it is twice [`PATIENT`] because the question's own bound is one
    // deadline from the spawn plus [`STDERR_AFTER_STOP`], and a return past that is charter
    // waiting.
    //
    // **The program answers only once the helper has left the group.** Otherwise, under load,
    // charter's group kill can land before perl reaches `setpgrp` and kill a helper that never
    // escaped — a race, not the limit.
    //
    // **It ignores SIGPIPE, or the test measures a race instead of the limit.** Once charter
    // stops draining stderr and closes its end, the helper's next write raises SIGPIPE, which
    // kills it — on Linux sometimes before `alive` is asked. That death is the pipe's, not
    // charter's, and it made this test fail on CI as "the escaped helper was killed". A
    // helper that shrugs off the closed pipe is the one this limit is about.
    let rig = Rig::new();
    let escaped = rig.marker("escaped");
    let release = rig.marker("release");
    rig.approved(&format!(
        "#!/bin/sh\nperl -e '$SIG{{PIPE}}=\"IGNORE\"; setpgrp(0,0); open(F,\">{escaped}\"); \
         print F $$; close F; $|=1; \
         until(-e \"{release}\"){{print STDERR \"x\"; select(undef,undef,undef,0.001)}}' &\n\
         while [ ! -s '{escaped}' ]; do sleep 0.01; done\n\
         read line\nprintf '%s\\n' '{ANSWER}'\n",
        escaped = escaped.display(),
        release = release.display(),
    ));
    let executor = patient();
    let watchdog = PATIENT * 2;

    let (returned, has_returned) = std::sync::mpsc::channel();
    let (answer, outlived) = std::thread::scope(|scope| {
        let asking = scope.spawn(|| {
            let answer = rig.ask(&executor);
            // Asked before the helper is let go, so this is "alive when the question returned".
            let outlived = alive_from(&escaped).is_some_and(crate::process::alive);
            let _ = returned.send(());
            (answer, outlived)
        });
        let in_time = has_returned.recv_timeout(watchdog).is_ok();
        // Let the helper go whatever happened, so a charter that waits for it still returns
        // and the scope can end.
        std::fs::write(&release, "").expect("the release");
        let said = asking.join().expect("the asking thread");
        assert!(
            in_time,
            "the question did not return within {watchdog:?} while an escaped helper held its \
             stderr — charter waited for a process that left the group"
        );
        said
    });

    if let Some(pid) = alive_from(&escaped).filter(|&pid| !gone(pid)) {
        // Killed by the pid this test itself caused to exist, never by name.
        if let Some(it) = rustix::process::Pid::from_raw(i32::try_from(pid).unwrap_or(0)) {
            let _ = rustix::process::kill_process(it, rustix::process::Signal::KILL);
        }
    }
    answer.expect("the answer");
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
    let executor = std::sync::Arc::new(patient());
    let first = {
        let executor = std::sync::Arc::clone(&executor);
        let config = rig.config();
        std::thread::spawn(move || {
            executor.ask(
                &config,
                &project::Choices::default(),
                "probe",
                "stats",
                None,
                |_| serde_json::Value::Null,
            )
        })
    };
    wait_for(&pid);
    // The extension is named as running for exactly as long as its question is in flight.
    assert_eq!(executor.running(), ["probe"]);

    let second = rig.ask(&executor).expect_err("a second copy started");
    assert!(second.contains("still answering"), "{second}");
    std::fs::write(&go, "").expect("go");
    first
        .join()
        .expect("the first thread")
        .expect("the first answer");
    assert!(executor.running().is_empty());
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

    let executor = std::sync::Arc::new(patient());
    // Asked once first, so the quick one below is a program macOS has already assessed and its
    // answer does not queue behind that (see `waiting`).
    executor
        .ask(
            &slow.config(),
            &project::Choices::default(),
            "quick",
            "stats",
            None,
            |_| serde_json::Value::Null,
        )
        .expect("the quick one, warmed");
    let stalled = {
        let executor = std::sync::Arc::clone(&executor);
        let config = slow.config();
        std::thread::spawn(move || {
            executor.ask(
                &config,
                &project::Choices::default(),
                "probe",
                "stats",
                None,
                |_| serde_json::Value::Null,
            )
        })
    };
    wait_for(&pid);
    let stalled_pid = alive_from(&pid).expect("the stalled one's pid");

    executor
        .ask(
            &slow.config(),
            &project::Choices::default(),
            "quick",
            "stats",
            None,
            |_| serde_json::Value::Null,
        )
        .expect("the quick one answered");
    // **Answered while the stall is still running**, which is the claim, and it needs no
    // clock (charter-app#303). The stalled program sleeps a minute and ends only when its own
    // deadline or `stop_all` below kills it, so a quick one that waited on it answers after it
    // is gone. A two-second bound here was a bound on how fast a busy machine starts a shell.
    let still_stalled = !stalled.is_finished() && crate::process::alive(stalled_pid);
    executor.stop_all();
    assert!(still_stalled, "one extension waited on another's stall");
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

    patient()
        .ask(
            &rig.config(),
            &project::Choices::default(),
            "probe",
            "stats",
            Some("steward"),
            |about| {
                assert_eq!(about, Subject::Personas);
                serde_json::json!({ "handed": [1, 2] })
            },
        )
        .expect("an answer");

    let text = std::fs::read_to_string(&asked).expect("the question");
    assert!(
        text.ends_with('\n') && text.matches('\n').count() == 1,
        "{text:?}"
    );
    let asked: serde_json::Value = serde_json::from_str(&text).expect("JSON");
    // In protocol 1, the one its manifest names — not the newest this charter speaks.
    assert_eq!(
        asked,
        serde_json::json!({
            "charter": 1, "extension": "probe", "view": "stats", "about": "personas",
            "focus": "steward", "given": { "handed": [1, 2] }
        })
    );
}

// -------------------------------------------------------------------------------------
// ...in a project that turned it off? (charter-app#253, ADR 0048)
// -------------------------------------------------------------------------------------

#[test]
fn an_approved_program_a_project_turned_off_never_starts_in_that_project() {
    let rig = Rig::new();
    let marker = rig.marker("ran");
    rig.approved(&marking(&marker));
    let off = project::Choices::from_text(
        Some("[extensions.probe]\nenabled = true\n"),
        Some("[extensions.probe]\nenabled = false\n"),
    );

    let refused = patient()
        .ask(&rig.config(), &off, "probe", "stats", None, |_| {
            serde_json::Value::Null
        })
        .expect_err("it ran in a project that turned it off");

    assert!(
        refused.contains("turned off in charter.local.toml"),
        "{refused}"
    );
    assert!(!marker.exists(), "a program the project turned off ran");
}

#[test]
fn an_approved_program_a_workspace_turned_off_never_starts_in_that_workspace() {
    // charter-app#280: the workspace layer is asked at the press, like the project's two files.
    let rig = Rig::new();
    let marker = rig.marker("ran");
    rig.approved(&marking(&marker));
    let off = project::Choices::from_text(Some("[extensions.probe]\nenabled = true\n"), None)
        .in_workspace(
            "alpha",
            Some(r#"{"settings": {"extensions": {"probe": {"enabled": false}}}}"#),
        );

    let refused = patient()
        .ask(&rig.config(), &off, "probe", "stats", None, |_| {
            serde_json::Value::Null
        })
        .expect_err("it ran in a workspace that turned it off");

    assert_eq!(
        refused,
        "'probe' is turned off in workspaces/alpha/workspace.json for this workspace, so charter \
         will not start its program here. Turn it on in Workspace settings to use this view."
    );
    assert!(!marker.exists(), "a program the workspace turned off ran");
}

#[test]
fn a_project_cannot_start_a_program_this_machine_has_not_approved() {
    let rig = Rig::new();
    let marker = rig.marker("ran");
    rig.write(VIEW_MANIFEST, &marking(&marker));
    extension::install(&rig.config(), &rig.at()).expect("installed");
    let on = project::Choices::from_text(Some("[extensions.probe]\nenabled = true\n"), None);

    let refused = patient()
        .ask(&rig.config(), &on, "probe", "stats", None, |_| {
            serde_json::Value::Null
        })
        .expect_err("a project's yes stood in for this machine's");

    assert!(refused.contains("not approved"), "{refused}");
    assert!(!marker.exists());
}

#[test]
fn the_settings_a_project_chose_are_handed_with_the_question() {
    let rig = Rig::new();
    let asked = rig.marker("asked");
    rig.write(
        r#"{"version":1,"id":"probe","name":"Probe",
            "settings":[{"key":"window","type":"choice","choices":["7d","30d"],"default":"30d"},
                        {"key":"compact","type":"bool"}],
            "contributes":{"runs":"bin/run",
                           "views":[{"id":"stats","title":"Probe statistics","about":"personas"}]}}"#,
        &format!(
            "#!/bin/sh\ncat > '{}'\nprintf '%s\\n' '{ANSWER}'\n",
            asked.display()
        ),
    );
    let found = extension::install(&rig.config(), &rig.at()).expect("installed");
    extension::approve(&rig.config(), found.id(), &found.path, &found.fingerprint)
        .expect("approved");
    let chose = project::Choices::from_text(
        Some("[extensions.probe.settings]\nwindow = \"7d\"\n"),
        Some("[extensions.probe.settings]\ncompact = true\n"),
    );

    patient()
        .ask(&rig.config(), &chose, "probe", "stats", None, |_| {
            serde_json::Value::Null
        })
        .expect("an answer");

    let asked: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&asked).expect("the question"))
            .expect("JSON");
    assert_eq!(
        asked["settings"],
        serde_json::json!({ "window": "7d", "compact": true })
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

    rig.ask(&patient()).expect("an answer");

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

    rig.ask(&patient()).expect("an answer");

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
    let _ = patient().ask(
        &rig.config(),
        &project::Choices::default(),
        "probe",
        "stats",
        None,
        |_| {
            handed = true;
            serde_json::Value::Null
        },
    );
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
    rig.ask(&patient())
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
    let executor = patient();
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

// -------------------------------------------------------------------------------------
// What else it can reach, and what cleans up after it (charter-app#212's review)
// -------------------------------------------------------------------------------------

/// A program that records which descriptors it holds, then answers.
fn listing_fds(out: &Path) -> String {
    format!(
        "#!/bin/sh\nls /dev/fd > '{}'\nread line\nprintf '%s\\n' '{ANSWER}'\n",
        out.display()
    )
}

#[test]
fn a_descriptor_charter_holds_without_close_on_exec_does_not_reach_the_program() {
    // Anything in the app can hold a descriptor without FD_CLOEXEC — a C library, or a socket
    // on macOS in the moment between `socket(2)` and `FIOCLEX`. `dup` sets no CLOEXEC, so this
    // is one. The program runs as the operator and is owed nothing of charter's.
    let rig = Rig::new();
    let seen = rig.marker("fds");
    rig.approved(&listing_fds(&seen));
    let file = std::fs::File::create(rig.marker("held")).expect("a file");
    // **Held at 100 or above, where the program's own descriptors never are.** The program
    // lists what it has with `ls /dev/fd`, and `ls` opens descriptors of its own (3, 4) to do
    // it. A `dup` takes the LOWEST free number, which in a process where other tests have
    // already run can be 4, and then the listing shows `ls`'s own descriptor under the
    // number charter held, and the test reports a leak that is not there. It did, on macOS,
    // in a serial run of this module. Duplicated high, then CLOEXEC cleared, because that
    // is the case under test: a descriptor charter holds WITHOUT close-on-exec.
    let held = rustix::io::fcntl_dupfd_cloexec(&file, 100).expect("dup above 100");
    rustix::io::fcntl_setfd(&held, rustix::io::FdFlags::empty()).expect("CLOEXEC cleared");
    let number = std::os::fd::AsRawFd::as_raw_fd(&held);
    assert!(number >= 100, "held at {number}");

    rig.ask(&patient()).expect("an answer");

    let listed = std::fs::read_to_string(&seen).expect("its descriptors");
    let fds: Vec<i32> = listed
        .split_whitespace()
        .filter_map(|s| s.parse().ok())
        .collect();
    assert!(
        !fds.contains(&number),
        "descriptor {number}, held by charter, reached the program: {fds:?}"
    );
}

/// How many descriptors above 2 a `listing_fds` program saw — `ls`'s own among them.
fn inherited(listed: &str) -> usize {
    listed
        .split_whitespace()
        .filter_map(|s| s.parse::<i32>().ok())
        .filter(|fd| *fd > 2)
        .count()
}

#[test]
fn a_socket_made_on_another_thread_as_the_program_starts_never_reaches_it() {
    // On macOS a socket pair is `socketpair(2)` and then a close-on-exec call per end. A thread
    // making pairs outside the fork lock — anything in the process that is not the executor —
    // hands its pair to a program started in between: before the program closed what it
    // inherited, 112 and 141 of 300 programs did get one. Now none may.
    let rig = Rig::new();
    let seen = rig.marker("fds");
    rig.approved(&listing_fds(&seen));
    let executor = patient();
    // What a program sees with nothing leaking: the shell's and `ls`'s own descriptors.
    rig.ask(&executor).expect("an answer");
    let baseline = inherited(&std::fs::read_to_string(&seen).expect("its descriptors"));
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let makers: Vec<_> = (0..4)
        .map(|_| {
            let stop = std::sync::Arc::clone(&stop);
            std::thread::spawn(move || {
                while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                    let _ = std::os::unix::net::UnixStream::pair();
                }
            })
        })
        .collect();
    let mut leaked = Vec::new();
    for _ in 0..100 {
        let _ = std::fs::remove_file(&seen);
        rig.ask(&executor).expect("an answer");
        let listed = std::fs::read_to_string(&seen).unwrap_or_default();
        if inherited(&listed) > baseline {
            leaked.push(listed);
        }
    }
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    for maker in makers {
        let _ = maker.join();
    }
    assert!(
        leaked.is_empty(),
        "{} of 100 programs inherited a descriptor: {:?}",
        leaked.len(),
        leaked.first()
    );
}

#[test]
fn a_program_s_channel_is_never_half_made_while_another_program_starts() {
    // macOS makes a socket pair in two system calls, and a program started between them inherits
    // both ends — for the executor's pair, possibly another extension's program, holding a
    // channel charter says is one extension's alone. So the pair is made under the fork lock's
    // write side, and this holds the read side exactly as a spawn in progress does.
    let _alone = crate::forklock::tests::alone();
    let spawning = crate::forklock::tests::as_a_fork_does();
    let (made, was_made) = std::sync::mpsc::channel();
    let maker = std::thread::spawn(move || {
        let channel = channel();
        made.send(()).expect("somebody is listening");
        channel.map(|_| ())
    });

    assert_eq!(
        was_made.recv_timeout(Duration::from_millis(300)),
        Err(std::sync::mpsc::RecvTimeoutError::Timeout),
        "a socket pair was made while a program was being started"
    );
    drop(spawning);
    assert_eq!(was_made.recv_timeout(Duration::from_secs(10)), Ok(()));
    maker
        .join()
        .expect("the thread")
        .expect("the pairs, once nothing was starting");
}

#[test]
fn a_helper_that_holds_stdin_and_reads_slowly_cannot_hold_the_question_past_the_deadline() {
    // A socket's write timeout bounds one `write(2)` and `write_all` loops, so a helper that
    // left the group, kept the program's stdin and read a byte a second held the question for
    // as long as a large request took to trickle — 15.6 s measured. The program itself answers
    // at once; what is timed is charter giving up on the rest of the write.
    let rig = Rig::new();
    let escaped = rig.marker("escaped");
    rig.approved(&format!(
        "#!/bin/sh\nexec 3<&0\nperl -e 'setpgrp(0,0); open(F,\">{}\"); print F $$; close F; \
         while(1){{ sysread(STDIN,$b,1) or exit; sleep 1 }}' <&3 &\n\
         printf '%s\\n' '{ANSWER}'\n",
        escaped.display()
    ));
    let executor = std::sync::Arc::new(patient());
    // On a thread of its own, so that a regression reads as a failure with a sentence on it
    // rather than as a test run that never ends.
    let (done, is_done) = std::sync::mpsc::channel();
    let asking = {
        let executor = std::sync::Arc::clone(&executor);
        let config = rig.config();
        std::thread::spawn(move || {
            let began = Instant::now();
            let said = executor.ask(
                &config,
                &project::Choices::default(),
                "probe",
                "stats",
                None,
                |_| serde_json::Value::String("x".repeat(4 << 20)),
            );
            let _ = done.send(began.elapsed());
            said
        })
    };
    let took = is_done.recv_timeout(PATIENT + Duration::from_secs(5));

    wait_for(&escaped);
    // Killed by the pid this test itself caused to exist, never by name. That also ends the
    // question if charter was still waiting on it, so the thread can be joined either way.
    if let Some(pid) = alive_from(&escaped)
        && let Some(it) = rustix::process::Pid::from_raw(i32::try_from(pid).unwrap_or(0))
    {
        let _ = rustix::process::kill_process(it, rustix::process::Signal::KILL);
    }
    let said = asking.join().expect("the asking thread");
    // Under the deadline, not merely near it: the program answered at once, and once it has
    // there is nothing left worth waiting for — charter's end is shut down, which ends the
    // write the helper is still trickling through. Against this executor's own deadline: the
    // write that held it is bounded by what is left of the deadline, so a charter that waits
    // for it waits past whichever deadline it was given.
    let took = took.expect("a slow reader of the question held it past the deadline");
    said.expect("the answer the program gave at once");
    assert!(
        took < PATIENT,
        "a slow reader of the question held it for {took:?}"
    );
    assert!(executor.running().is_empty(), "the slot was never released");
}

#[test]
fn a_change_made_while_the_question_is_built_is_seen_by_the_gate() {
    // ADR 0028's race, narrowed: the fingerprint is taken AFTER the plane is read for the
    // question, so the slow part of an ask is no longer inside the window between the hash and
    // the start. A file planted while `hand` runs is a change the gate sees.
    let rig = Rig::new();
    let marker = rig.marker("ran");
    rig.approved(&marking(&marker));
    let planted = rig.at().join("bin/planted.sh");

    let refused = patient()
        .ask(
            &rig.config(),
            &project::Choices::default(),
            "probe",
            "stats",
            None,
            |_| {
                std::fs::write(&planted, "echo planted\n").expect("a planted file");
                serde_json::Value::Null
            },
        )
        .expect_err("a directory that changed while the question was built ran");

    assert!(
        refused.contains("changed since you approved it"),
        "{refused}"
    );
    assert!(!marker.exists(), "the program ran");
}

/// A program in its own group that starts a helper, says the helper's pid, and waits.
fn with_a_helper(helper: &Path) -> std::process::Child {
    use std::os::unix::process::CommandExt;
    let mut command = std::process::Command::new("/bin/sh");
    command
        .arg("-c")
        .arg(r#"sleep 60 & echo $! > "$1"; wait"#)
        .arg("sh")
        .arg(helper)
        .process_group(0);
    let child = crate::forklock::spawn(&mut command).expect("a program");
    wait_for(helper);
    child
}

#[test]
fn a_started_program_is_stopped_and_reaped_however_the_question_ends() {
    // An error or a panic between the start and the answer — a thread that could not be
    // started — returns through `Running`'s drop, and that has to stop the whole group and
    // reap the program, exactly as the ordinary path does.
    let dir = tempfile::tempdir().expect("a directory");
    let helper = dir.path().join("helper");
    let executor = patient();
    let held = executor.hold("probe").expect("the slot");
    let child = with_a_helper(&helper);
    let pid = child.id();

    drop(Running::new(&executor, "probe", child));

    assert!(gone(pid), "the program was left running, or never reaped");
    let helper = alive_from(&helper).expect("the helper's pid");
    assert!(gone(helper), "its helper was left running");
    assert_eq!(
        executor
            .table
            .lock()
            .expect("the table")
            .running
            .get("probe"),
        Some(&0),
        "a group still in the table after its program was reaped"
    );
    drop(held);
}

#[test]
fn a_program_started_as_charter_closes_is_killed_as_it_is_recorded() {
    // `stop_all` kills what is in the table. A program started a moment before it and recorded a
    // moment after would be in no table when it looked — so recording one after `stop_all` kills
    // it on the spot.
    let dir = tempfile::tempdir().expect("a directory");
    let helper = dir.path().join("helper");
    let executor = patient();
    let _held = executor.hold("probe").expect("the slot");
    let child = with_a_helper(&helper);

    executor.stop_all();
    let running = Running::new(&executor, "probe", child);

    // Asked while `running` still holds the program, so what killed the helper was the
    // recording, not the drop below.
    let helper = alive_from(&helper).expect("the helper's pid");
    assert!(
        gone(helper),
        "a program recorded after stop_all was left running"
    );
    drop(running);
}

#[test]
fn nothing_is_started_once_charter_is_closing() {
    let rig = Rig::new();
    let marker = rig.marker("ran");
    rig.approved(&marking(&marker));
    let executor = patient();
    executor.stop_all();

    let refused = rig
        .ask(&executor)
        .expect_err("a program started after stop_all");

    assert!(refused.contains("closing"), "{refused}");
    assert!(!marker.exists(), "the program ran");
}

// -------------------------------------------------------------------------------------
// What it is told when it goes wrong (charter-app#311)
// -------------------------------------------------------------------------------------

#[test]
fn the_last_words_quoted_are_the_end_of_what_it_printed_not_the_start() {
    let rig = Rig::new();
    rig.approved(
        "#!/bin/sh\nprintf 'FIRST' >&2\nhead -c 4000 /dev/zero | tr '\\0' a >&2\n\
         printf 'THE END' >&2\nexit 3\n",
    );

    let refused = rig.ask(&patient()).expect_err("a crash was drawn");

    assert!(refused.contains("exited with status 3"), "{refused}");
    assert!(refused.ends_with("aaaaTHE END"), "{refused}");
    assert!(!refused.contains("FIRST"), "{refused}");
    // The tail kept is the last `MOST_STDERR_BYTES` bytes, quoted whole.
    let quoted = refused
        .split("The last of what it printed: ")
        .nth(1)
        .expect("its last words");
    assert_eq!(quoted.len(), MOST_STDERR_BYTES, "{quoted}");
}

#[test]
fn a_program_that_ends_part_way_through_a_line_is_said_to_have() {
    let rig = Rig::new();
    rig.approved("#!/bin/sh\nprintf '{\"charter\":'\nexit 0\n");
    let refused = rig.ask(&patient()).expect_err("half a line was drawn");
    assert!(
        refused.starts_with(
            "'probe' exited with status 0 without answering — it wrote part of a line and \
             never finished it."
        ),
        "{refused}"
    );

    let rig = Rig::new();
    rig.approved("#!/bin/sh\nexit 0\n");
    let refused = rig.ask(&patient()).expect_err("nothing was drawn");
    assert_eq!(
        refused,
        "'probe' exited with status 0 without answering. It printed nothing to stderr."
    );
}

#[test]
fn a_program_whose_interpreter_cannot_be_run_is_told_it_must_be_executable() {
    let rig = Rig::new();
    // The program itself is executable, so it passes charter's own check; the interpreter its
    // first line names is a file that is not, and the system refuses to start it.
    let interpreter = rig.marker("not-an-interpreter");
    std::fs::write(&interpreter, "just text\n").expect("the file");
    rig.approved(&format!("#!{}\necho never\n", interpreter.display()));

    let refused = rig.ask(&patient()).expect_err("it could not start");

    assert!(refused.contains("could not be started"), "{refused}");
    assert!(
        refused.ends_with(
            "charter runs an extension's program directly, never through a shell, so it has to \
             be executable by you."
        ),
        "{refused}"
    );

    // An interpreter that does not exist at all is the other sentence.
    let rig = Rig::new();
    rig.approved(&format!(
        "#!{}\necho never\n",
        rig.marker("no-such-interpreter").display()
    ));
    let refused = rig.ask(&patient()).expect_err("it could not start");
    assert!(
        refused.ends_with(
            "If it is a script, its first line has to name an interpreter that exists on this \
             machine."
        ),
        "{refused}"
    );
}

/// An answer line of exactly `size` bytes before its newline: one note, padded.
fn answer_of(size: usize) -> String {
    let frame = r#"{"charter":1,"blocks":[{"kind":"note","text":""}]}"#;
    let pad = "x".repeat(size - frame.len());
    format!(r#"{{"charter":1,"blocks":[{{"kind":"note","text":"{pad}"}}]}}"#)
}

#[test]
fn an_answer_of_exactly_the_most_charter_reads_is_read_and_one_byte_more_is_not() {
    let frame = r#"{"charter":1,"blocks":[{"kind":"note","text":""}]}"#;
    assert_eq!(frame.len(), 50);
    for (size, fits) in [(MOST_ANSWER_BYTES, true), (MOST_ANSWER_BYTES + 1, false)] {
        let rig = Rig::new();
        let answer = rig.marker("answer");
        std::fs::write(&answer, format!("{}\n", answer_of(size))).expect("the answer");
        rig.approved(&format!(
            "#!/bin/sh\nread line\ncat '{}'\n",
            answer.display()
        ));

        let refused = rig
            .ask(&patient())
            .expect_err("a note that long is never drawn");

        // At the bound the line is read whole, and it is the panel that refuses what it says;
        // past it, the line is not read at all.
        if fits {
            assert_eq!(
                refused,
                format!(
                    "'probe' answered a block at 0 that has a 'text' of {} bytes, and charter \
                     draws at most 8192",
                    size - 50
                )
            );
        } else {
            assert!(refused.contains("answered more than 512 KiB"), "{refused}");
        }
    }
}

#[test]
fn the_highest_descriptor_is_at_least_every_one_this_process_holds_and_at_most_the_bound() {
    let file = std::fs::File::open("/dev/null").expect("a descriptor");
    let held = std::os::fd::AsRawFd::as_raw_fd(&file);
    let highest = highest_descriptor();
    assert!(highest >= held, "{highest} < {held}");
    assert!(
        u64::try_from(highest).unwrap() <= MOST_DESCRIPTORS,
        "{highest}"
    );
    assert_eq!(MOST_DESCRIPTORS, 1_048_576);
    let limit = rustix::process::getrlimit(rustix::process::Resource::Nofile);
    if let Some(soft) = limit.current.filter(|soft| *soft <= MOST_DESCRIPTORS) {
        assert_eq!(u64::try_from(highest).unwrap(), soft);
    }
}

// -------------------------------------------------------------------------------------
// ...when a row offers an action its manifest does not declare? (charter-app#341)
// -------------------------------------------------------------------------------------

/// A protocol-2 extension with one view and one action, `close`.
const ACTING_MANIFEST: &str = r#"{"version":2,"id":"probe","name":"Probe",
    "capabilities":["actions"],
    "contributes":{"runs":"bin/run",
                   "views":[{"id":"stats","title":"Probe statistics","about":"personas"}],
                   "actions":[{"id":"close","title":"Close","confirm":false}]}}"#;

impl Rig {
    fn acting(&self, script: &str) -> &Self {
        self.write(ACTING_MANIFEST, script);
        let found = extension::install(&self.config(), &self.at()).expect("installed");
        extension::approve(&self.config(), found.id(), &found.path, &found.fingerprint)
            .expect("approved");
        self
    }
}

/// A script that answers `line` whatever it is asked.
fn answering_with(line: &str) -> String {
    format!("#!/bin/sh\nread line\nprintf '%s\\n' '{line}'\n")
}

#[test]
fn a_row_offering_an_action_its_manifest_does_not_declare_is_refused_whole() {
    let rig = Rig::new();
    rig.acting(&answering_with(
        r#"{"charter":2,"blocks":[{"kind":"list","rows":[{"key":"a","text":"A","actions":["close","delete-everything"]}]}]}"#,
    ));

    let refused = rig
        .ask(&patient())
        .expect_err("an unapproved verb was drawn");

    assert!(
        refused.contains(
            "offering the action \"delete-everything\", which its manifest does not declare"
        ),
        "{refused}"
    );
}

#[test]
fn a_row_offering_only_declared_actions_is_drawn_with_the_manifest_s_actions_beside_it() {
    let rig = Rig::new();
    rig.acting(&answering_with(
        r#"{"charter":2,"blocks":[{"kind":"list","rows":[{"key":"a","text":"A","actions":["close"]}]}]}"#,
    ));

    let answer = rig.ask(&patient()).expect("an answer");

    let panel::Block::List { rows, .. } = &answer.blocks[0] else {
        panic!("not a list: {:?}", answer.blocks);
    };
    assert_eq!(rows[0].actions, ["close"]);
    assert_eq!(answer.actions[0].title, "Close");
}

#[test]
fn an_action_is_asked_as_run_action_on_its_subject_in_protocol_2() {
    let rig = Rig::new();
    let asked = rig.marker("asked");
    rig.acting(&format!(
        "#!/bin/sh\ncat > '{}'\nprintf '%s\\n' '{{\"charter\":2}}'\n",
        asked.display()
    ));

    let acted = patient()
        .act(
            &rig.config(),
            &project::Choices::default(),
            "probe",
            "close",
            On {
                view: Some("stats"),
                focus: Some("steward"),
                row: Some("a"),
            },
            false,
            |_| serde_json::json!({ "handed": true }),
        )
        .expect("it ran");

    // Done, with no blocks: the view stands as it was.
    assert_eq!(acted.blocks, None);
    let asked: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&asked).expect("the question"))
            .expect("JSON");
    assert_eq!(
        asked,
        serde_json::json!({
            "charter": 2, "extension": "probe", "action": "close", "view": "stats",
            "about": "personas", "focus": "steward", "row": "a",
            "given": { "handed": true }, "writes": []
        })
    );
}

#[test]
fn a_view_s_answer_still_owes_blocks_where_an_action_s_may_owe_none() {
    let rig = Rig::new();
    rig.acting(&answering_with(r#"{"charter":2}"#));

    let refused = rig
        .ask(&patient())
        .expect_err("nothing was drawn as an answer");

    assert!(
        refused.contains("neither 'blocks' nor 'error'"),
        "{refused}"
    );
}

#[test]
fn an_action_this_extension_does_not_declare_starts_nothing() {
    let rig = Rig::new();
    let marker = rig.marker("ran");
    rig.acting(&marking(&marker));

    let refused = patient()
        .act(
            &rig.config(),
            &project::Choices::default(),
            "probe",
            "rm-rf",
            On::default(),
            true,
            |_| serde_json::Value::Null,
        )
        .expect_err("an undeclared action ran");

    assert!(
        refused.contains("has no action called 'rm-rf'"),
        "{refused}"
    );
    assert!(!marker.exists(), "the program was started");
}
