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
        let found = extension::install(&self.config(), &extension::BuiltIn::none(), &self.at())
            .expect("installed");
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

/// The deadline of every executor here but the ones whose subject is a deadline.
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
    Executor::default().with_deadline(PATIENT)
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

/// Kill the helper whose pid `marker` holds, if it wrote one: by the pid this test caused to
/// exist, never by name.
fn kill_escaped(marker: &Path) {
    if let Some(pid) = alive_from(marker)
        && let Some(it) = rustix::process::Pid::from_raw(i32::try_from(pid).unwrap_or(0))
    {
        let _ = rustix::process::kill_process(it, rustix::process::Signal::KILL);
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
    extension::install(&rig.config(), &extension::BuiltIn::none(), &rig.at()).expect("installed");

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
    let found = extension::install(&rig.config(), &extension::BuiltIn::none(), &rig.at())
        .expect("installed");
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
    let other = extension::install(&rig.config(), &extension::BuiltIn::none(), &rig.at())
        .expect("installed as other");
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
    let found = extension::install(&rig.config(), &extension::BuiltIn::none(), &rig.at())
        .expect("installed");
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
        .ask(&Executor::default().with_deadline(Duration::from_secs(1)))
        .expect_err("an answer from nothing");

    assert!(
        refused.starts_with("'probe' did not answer within 1 seconds, so charter stopped it."),
        "{refused}"
    );
}

#[test]
fn a_program_that_never_answers_is_said_to_have_left_its_question_unread_only_when_it_did() {
    let short = || Executor::default().with_deadline(Duration::from_secs(1));
    let rig = Rig::new();
    rig.approved("#!/bin/sh\nexec sleep 60\n");

    // A question the socket holds whole is written, whether or not the program reads it.
    let refused = rig.ask(&short()).expect_err("an answer from nothing");
    assert!(
        refused.contains("It was asked one question. It printed"),
        "{refused}"
    );

    // One larger than the socket holds is written only as far as the program reads it.
    let refused = short()
        .ask(
            &rig.config(),
            &project::Choices::default(),
            "probe",
            "stats",
            None,
            |_| serde_json::Value::String("x".repeat(4 << 20)),
        )
        .expect_err("an answer from nothing");
    assert!(
        refused.contains("It was asked one question, and it never finished reading it."),
        "{refused}"
    );
}

#[test]
fn every_executor_charter_makes_gives_its_programs_the_real_deadline() {
    // **The real [`DEADLINE`], held to account without a program racing it** (#422). This used
    // to be the one test that ran a program against the real five seconds, and on a machine too
    // busy to start a shell in five seconds its program never wrote its pid and the test failed
    // on that rather than on anything about the deadline. What it proved is two facts, and each
    // is asserted on its own now: every executor charter makes has this deadline (here), and an
    // executor stops a program at whatever deadline it has (the test below, against a short one).
    assert_eq!(Executor::default().deadline, DEADLINE);
    assert_eq!(
        Executor::with_built_in(extension::BuiltIn::none()).deadline,
        DEADLINE
    );
    assert_eq!(DEADLINE, Duration::from_secs(5), "ADR 0041's five seconds");
}

#[test]
fn a_program_that_never_answers_is_stopped_at_the_deadline() {
    const SHORT: Duration = Duration::from_millis(500);
    let rig = Rig::new();
    rig.approved("#!/bin/sh\nexec sleep 60\n");
    let executor = Executor::default().with_deadline(SHORT);

    let refused = rig.ask(&executor).expect_err("an answer from nothing");
    let returned = Instant::now();

    // Stopped by the deadline it was given, and not by the real one: the refusal names it.
    assert!(
        refused.contains("did not answer within 0.5 seconds"),
        "{refused}"
    );
    // **Which process, and from when, is the executor's own record of what it started**, not a
    // pid the program writes (#465). A program has to run to write one, and on a loaded machine
    // a shell can take longer than the deadline to get there — ten tries in a row at load 118
    // — so the test failed on how busy the machine was. The group is recorded the moment the
    // program exists, whether or not it got as far as saying anything; and the deadline is
    // timed from there, so the work before the start — the fingerprint, the fork — which a
    // loaded machine makes take seconds, is not counted against it.
    let groups = executor
        .table
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .groups
        .clone();
    let [(group, at)] = groups[..] else {
        panic!("one program started, not {groups:?}");
    };
    let took = returned.duration_since(at);
    assert!(took >= SHORT, "gave up after {took:?}, before the deadline");
    // Under the real deadline: it was this executor's short one that stopped it.
    assert!(
        took < DEADLINE,
        "the caller waited {took:?} after the start, past the deadline"
    );
    assert!(
        gone(u32::try_from(group).expect("a pid")),
        "a program that timed out is still running"
    );
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
    let found = extension::install(&slow.config(), &extension::BuiltIn::none(), &quick_at)
        .expect("installed");
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
    extension::install(&rig.config(), &extension::BuiltIn::none(), &rig.at()).expect("installed");
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
    let found = extension::install(&rig.config(), &extension::BuiltIn::none(), &rig.at())
        .expect("installed");
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
    extension::install(&rig.config(), &extension::BuiltIn::none(), &rig.at()).expect("installed");

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
    //
    // **It answers only once the helper has left the group** (#465), as the command test below
    // waits for its own. Answering first raced the helper's start: charter kills the group the
    // moment it has its answer, and on a loaded machine that landed before perl had called
    // `setpgrp` — the helper died with the program, never wrote where it was, and the test
    // failed thirty seconds later on a helper that never escaped rather than on the write.
    let rig = Rig::new();
    let escaped = rig.marker("escaped");
    rig.approved(&format!(
        "#!/bin/sh\nexec 3<&0\nperl -e 'setpgrp(0,0); open(F,\">{0}\"); print F $$; close F; \
         while(1){{ sysread(STDIN,$b,1) or exit; sleep 1 }}' <&3 &\n\
         while [ ! -s '{0}' ]; do sleep 0.01; done\n\
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
    // That also ends the question if charter was still waiting on it, so the thread can be
    // joined either way.
    kill_escaped(&escaped);
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
        let found = extension::install(&self.config(), &extension::BuiltIn::none(), &self.at())
            .expect("installed");
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

// -------------------------------------------------------------------------------------
// ...when it is run as a command, told an event, or asked for a briefing? (charter-app#342,
// charter-app#343) And what does the watch on the plane see? (charter-app#341)
// -------------------------------------------------------------------------------------

/// A protocol-2 extension that does everything a program can be asked: a view, two actions
/// that ask first, a write path, an event, a briefing section and two commands, one that says
/// it writes and one that says it only reads.
const TALKING_MANIFEST: &str = r#"{"version":2,"id":"probe","name":"Probe",
    "capabilities":["actions","writes","events","briefing","cli"],
    "contributes":{"runs":"bin/run",
                   "views":[{"id":"stats","title":"Probe statistics","about":"personas"}],
                   "actions":[{"id":"careful","title":"Careful","confirm":true},
                              {"id":"forget","title":"Forget","confirm":false,"deletes":true}],
                   "writes":["notes/"],
                   "events":{"hears":["plane-saved"]},
                   "briefing":{"title":"Probe"},
                   "cli":[{"name":"look","title":"Look","writes":false},
                          {"name":"stamp","title":"Stamp","writes":true}]}}"#;

impl Rig {
    fn talking(&self, script: &str) -> &Self {
        self.write(TALKING_MANIFEST, script);
        let found = extension::install(&self.config(), &extension::BuiltIn::none(), &self.at())
            .expect("installed");
        extension::approve(&self.config(), found.id(), &found.path, &found.fingerprint)
            .expect("approved");
        self
    }

    /// A plane that is a git repository, as a plane is, made on first use.
    fn plane(&self) -> PathBuf {
        let plane = self.dir.path().join("plane");
        if !plane.exists() {
            std::fs::create_dir_all(&plane).expect("the plane");
            crate::testgit::run(&plane, &["init", "-q", "."]);
        }
        plane
    }

    fn command_in(
        &self,
        executor: &Executor,
        project: &project::Choices,
        name: &str,
    ) -> Result<Ran, String> {
        executor.command(&self.config(), project, "probe", name, &[])
    }

    fn command(&self, executor: &Executor) -> Result<Ran, String> {
        self.command_in(executor, &project::Choices::default(), "look")
    }

    fn tell(
        &self,
        executor: &Executor,
        event: &extension::events::Event,
    ) -> Result<Option<String>, String> {
        executor.tell(&self.config(), &project::Choices::default(), "probe", event)
    }
}

/// Ask `ask` with a deadline long enough for the program to get as far as the test needs
/// before it passes, and answer what it said and the deadline it said it under.
///
/// **For a test whose program has to reach some point before its deadline and then outlast
/// it** — close its output, see a helper leave its group. Nothing but the program running can
/// get it there, and on a loaded machine a shell takes seconds to start (#465): ten tries at
/// one second each were not enough at load 118. So an attempt refused as not having finished
/// at all is asked again with twice the deadline, up to about [`PATIENT`] in all, the time
/// a program here is given to start. The first one usually decides it, so a passing test
/// on a quiet machine still takes a second.
fn once_started(ask: impl Fn(&Executor) -> Result<Ran, String>) -> (Result<Ran, String>, Duration) {
    let mut deadline = Duration::from_secs(1);
    loop {
        let said = ask(&Executor::default().with_deadline(deadline));
        match &said {
            Err(why) if why.contains("did not finish within") && deadline < PATIENT / 2 => {
                deadline *= 2;
            }
            _ => return (said, deadline),
        }
    }
}

#[test]
fn a_command_s_output_and_exit_status_are_passed_back_as_the_program_wrote_them() {
    let rig = Rig::new();
    rig.talking("#!/bin/sh\nprintf 'out\\n'\nprintf 'err\\n' >&2\nexit 7\n");

    let ran = rig.command(&patient()).expect("it ran");

    assert_eq!(ran.stdout, b"out\n");
    assert_eq!(ran.stderr, b"err\n");
    assert_eq!(ran.status, 7);
}

#[test]
fn a_command_that_exits_without_reading_its_question_still_has_its_output_passed_back() {
    // It sleeps first, so the whole question is sitting unread in its socket when it exits —
    // which on Linux makes charter's read answer `ECONNRESET` after the output, rather than
    // the end of it. The program ending that way is still the program ending.
    let rig = Rig::new();
    rig.talking("#!/bin/sh\nsleep 0.3\nprintf 'out\\n'\n");

    let ran = rig.command(&patient()).expect("it ran");

    assert_eq!(ran.stdout, b"out\n");
    assert_eq!(ran.status, 0);
}

#[test]
fn a_command_that_closes_its_output_before_it_exits_is_waited_for_and_its_status_kept() {
    let rig = Rig::new();
    rig.talking("#!/bin/sh\nprintf 'out\\n'\nexec >&- <&-\nsleep 0.3\nexit 4\n");

    let ran = rig.command(&patient()).expect("it ran");

    assert_eq!(ran.stdout, b"out\n");
    assert_eq!(ran.status, 4);
}

#[test]
fn a_command_that_closes_its_output_and_never_exits_passes_nothing_on() {
    let rig = Rig::new();
    rig.talking("#!/bin/sh\nprintf 'out\\n'\nexec >&- <&-\nexec sleep 60\n");

    let (said, deadline) = once_started(|executor| rig.command(executor));
    let refused = said.expect_err("a command that never exited was passed on");

    assert!(
        refused.contains(&format!(
            "closed its output and did not exit within {} seconds",
            deadline.as_secs_f32()
        )),
        "{refused}"
    );
}

#[test]
fn a_command_that_never_finishes_is_refused_as_too_late_never_as_a_lost_connection() {
    let rig = Rig::new();
    rig.talking("#!/bin/sh\nexec sleep 60\n");

    let refused = rig
        .command(&Executor::default().with_deadline(Duration::from_secs(1)))
        .expect_err("an answer from nothing");

    assert!(
        refused.starts_with("'probe' did not finish within 1 seconds, so charter stopped it"),
        "{refused}"
    );
}

#[test]
fn a_command_s_output_of_exactly_the_most_charter_passes_on_is_passed_and_one_byte_more_is_not() {
    let printing = |bytes: usize| format!("#!/bin/sh\nhead -c {bytes} /dev/zero | tr '\\0' x\n");
    let rig = Rig::new();
    rig.talking(&printing(MOST_ANSWER_BYTES));
    let ran = rig.command(&patient()).expect("exactly the most");
    assert_eq!(ran.stdout.len(), MOST_ANSWER_BYTES);

    let rig = Rig::new();
    rig.talking(&printing(MOST_ANSWER_BYTES + 1));
    let refused = rig
        .command(&patient())
        .expect_err("one byte more was passed on");
    assert!(
        refused.contains("printed more than 512 KiB, so charter stopped it"),
        "{refused}"
    );
}

#[test]
fn a_command_s_stderr_of_exactly_the_most_charter_passes_on_is_passed_and_one_byte_more_is_not() {
    let printing =
        |bytes: usize| format!("#!/bin/sh\nhead -c {bytes} /dev/zero | tr '\\0' e >&2\n");
    let rig = Rig::new();
    rig.talking(&printing(MOST_ANSWER_BYTES));
    let ran = rig.command(&patient()).expect("exactly the most");
    assert_eq!(ran.stderr.len(), MOST_ANSWER_BYTES);

    let rig = Rig::new();
    rig.talking(&printing(MOST_ANSWER_BYTES + 1));
    let refused = rig
        .command(&patient())
        .expect_err("one byte more was passed on");
    assert!(
        refused.contains("printed more than 512 KiB on stderr"),
        "{refused}"
    );
}

#[test]
fn a_command_whose_stderr_something_it_started_still_holds_passes_nothing_on() {
    // The helper leaves the group, so charter's kill does not reach it, and keeps the
    // program's stderr open past the deadline: what was read of it is not all of it. The
    // program exits only once the helper has left, or the kill could land first.
    //
    // **Each attempt's helper writes a marker of its own**, named by the program's pid (#465):
    // with one shared marker, the helper of an attempt refused as too slow could write it
    // after the next attempt began, and that attempt's program would print before its own
    // helper had left.
    let rig = Rig::new();
    let escaped = rig.marker("escaped");
    std::fs::create_dir(&escaped).expect("a directory for the markers");
    rig.talking(&format!(
        "#!/bin/sh\ne='{0}'/$$\n\
         perl -e 'setpgrp(0,0); open(F,\">$ARGV[0]\"); print F $$; close F; sleep 30' \"$e\" \
         >/dev/null </dev/null &\nwhile [ ! -s \"$e\" ]; do sleep 0.01; done\nprintf 'out\\n'\n",
        escaped.display()
    ));

    let (said, deadline) = once_started(|executor| rig.command(executor));

    for marker in std::fs::read_dir(&escaped).expect("the markers").flatten() {
        kill_escaped(&marker.path());
    }
    let refused = said.expect_err("a stderr still open was passed on as the whole of it");
    assert!(
        refused.contains(&format!(
            "its stderr did not end within {} seconds",
            deadline.as_secs_f32()
        )),
        "{refused}"
    );
}

#[test]
fn a_command_s_stderr_is_read_to_its_end_after_the_stop_while_its_deadline_allows() {
    use std::io::Write;
    let (ours, mut theirs) = std::os::unix::net::UnixStream::pair().expect("a pair");
    // Stopped already, and well inside the deadline: the end comes after the moment a view's
    // quoted tail would have been cut off ([`STDERR_AFTER_STOP`]), and is still waited for.
    let stop = std::sync::atomic::AtomicBool::new(true);
    let late = std::thread::spawn(move || {
        std::thread::sleep(STDERR_AFTER_STOP * 3);
        theirs.write_all(b"late words").expect("written");
    });

    let drained = drain(&ours, &stop, 1 << 10, Some(Instant::now() + PATIENT));

    late.join().expect("the writer");
    assert!(drained.ended, "the drain stopped before the end");
    assert_eq!(drained.kept, b"late words");
    assert!(!drained.overflowed);
}

#[test]
fn a_read_that_fails_is_a_lost_connection_never_a_late_or_whole_answer() {
    // What charter reads from is a socket; a descriptor that is not one fails every read with
    // an error that is none of the ones a program ending or running long produces.
    let dir = tempfile::tempdir().expect("a directory");
    let not_a_socket = std::os::fd::OwnedFd::from(std::fs::File::open(dir.path()).expect("open"));
    let ours = std::os::unix::net::UnixStream::from(not_a_socket);

    let heard = listen_to_the_end(&ours, Instant::now() + PATIENT);

    assert!(matches!(heard, Heard::Broken(_)), "not a lost connection");
}

#[test]
fn a_command_that_says_it_only_reads_is_held_to_no_path_and_one_that_writes_to_its_own() {
    let rig = Rig::new();
    let plane = rig.plane();
    let asked = rig.marker("asked");
    rig.talking(&format!(
        "#!/bin/sh\ncat > '{}'\nmkdir -p '{1}/notes'\necho $$ > '{1}/notes/n'\n",
        asked.display(),
        plane.display()
    ));
    let here = project::Choices::read(&plane);

    let looked = rig.command_in(&patient(), &here, "look").expect("it ran");
    let told: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&asked).expect("the question"))
            .expect("JSON");
    assert_eq!(told["writes"], serde_json::json!([]));
    let said = looked
        .overreach
        .expect("a write by a command that only reads went unreported");
    assert!(said.contains("notes/n"), "{said}");

    let stamped = rig.command_in(&patient(), &here, "stamp").expect("it ran");
    let told: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&asked).expect("the question"))
            .expect("JSON");
    assert_eq!(
        told["writes"],
        serde_json::json!(extension::writes::resolve(&plane, &["notes/".to_owned()]))
    );
    assert_eq!(
        stamped.overreach, None,
        "a write inside its own paths was reported"
    );
}

#[test]
fn a_protocol_1_program_asked_in_a_plane_is_told_nothing_about_where_it_may_write() {
    let rig = Rig::new();
    let plane = rig.plane();
    let asked = rig.marker("asked");
    rig.approved(&format!(
        "#!/bin/sh\ncat > '{}'\nprintf '%s\\n' '{ANSWER}'\n",
        asked.display()
    ));

    patient()
        .ask(
            &rig.config(),
            &project::Choices::read(&plane),
            "probe",
            "stats",
            None,
            |_| serde_json::Value::Null,
        )
        .expect("an answer");

    let told: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&asked).expect("the question"))
            .expect("JSON");
    assert_eq!(told.get("writes"), None, "{told}");
}

#[test]
fn a_command_the_extension_does_not_declare_starts_nothing_and_names_the_ones_it_has() {
    let rig = Rig::new();
    let marker = rig.marker("ran");
    rig.talking(&marking(&marker));

    let refused = rig
        .command_in(&patient(), &project::Choices::default(), "rm-rf")
        .expect_err("an undeclared command ran");

    assert_eq!(
        refused,
        "'probe' has no command called 'rm-rf'. It has: look, stamp."
    );
    assert!(!marker.exists(), "the program was started");
}

#[test]
fn an_action_that_asks_first_is_run_only_with_a_yes_and_says_why_it_asks() {
    let rig = Rig::new();
    let marker = rig.marker("ran");
    rig.talking(&format!(
        "#!/bin/sh\ntouch '{}'\nread line\nprintf '%s\\n' '{{\"charter\":2}}'\n",
        marker.display()
    ));
    let act = |action: &str, confirmed: bool| {
        patient().act(
            &rig.config(),
            &project::Choices::default(),
            "probe",
            action,
            On::default(),
            confirmed,
            |_| serde_json::Value::Null,
        )
    };

    let refused = act("careful", false).expect_err("run without a yes");
    assert_eq!(
        refused,
        "'probe''s action “Careful” was not run: its extension asks charter to ask you first, \
         and nobody said yes."
    );
    let refused = act("forget", false).expect_err("a delete run without a yes");
    assert_eq!(
        refused,
        "'probe''s action “Forget” was not run: it deletes, and charter asks before every \
         action that deletes, and nobody said yes."
    );
    assert!(!marker.exists(), "the program was started");

    act("careful", true).expect("run with a yes");
    assert!(marker.exists(), "the program was not started");
}

#[test]
fn an_event_it_hears_is_told_and_nothing_is_reported_when_nothing_changed() {
    let rig = Rig::new();
    let asked = rig.marker("asked");
    rig.talking(&format!(
        "#!/bin/sh\ncat > '{}'\nprintf '%s\\n' '{{\"charter\":2}}'\n",
        asked.display()
    ));

    let told = rig.tell(&patient(), &extension::events::Event::PlaneSaved);

    assert_eq!(told, Ok(None));
    let asked: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&asked).expect("the question"))
            .expect("JSON");
    assert_eq!(asked["event"], "plane-saved");
}

#[test]
fn an_event_it_does_not_hear_is_refused_and_starts_nothing() {
    let rig = Rig::new();
    let marker = rig.marker("ran");
    rig.talking(&marking(&marker));

    let refused = rig
        .tell(
            &patient(),
            &extension::events::Event::WorkspaceFocused {
                workspace: "ide".into(),
            },
        )
        .expect_err("told an event it does not hear");

    assert_eq!(
        refused,
        "'probe' does not hear a workspace being focused, so charter does not tell it"
    );
    assert!(!marker.exists(), "the program was started");
}

#[test]
fn an_event_s_answer_is_that_it_heard_or_an_error_and_nothing_else() {
    let told = |line: &str| {
        let rig = Rig::new();
        rig.talking(&answering_with(line));
        rig.tell(&patient(), &extension::events::Event::PlaneSaved)
    };

    let refused = told(r#"{"charter":2,"error":"no disk"}"#).expect_err("an error heard as done");
    assert!(
        refused.contains("answered that it could not: no disk"),
        "{refused}"
    );

    let refused = told(r#"{"charter":2,"blocks":[]}"#).expect_err("blocks from an event");
    assert!(
        refused.contains(
            "answered \"blocks\", which is not part of charter's protocol 2 — this answer is \
             'charter', and 'error'"
        ),
        "{refused}"
    );

    let refused = told(r#"{"charter":1}"#).expect_err("an answer in another protocol");
    assert!(
        refused.contains("answered in protocol 1, and charter asked it in protocol 2"),
        "{refused}"
    );
}

#[test]
fn an_event_s_program_that_writes_outside_its_paths_is_reported() {
    let rig = Rig::new();
    let plane = rig.plane();
    rig.talking(&format!(
        "#!/bin/sh\nread line\necho $$ > '{}/stray'\nprintf '%s\\n' '{{\"charter\":2}}'\n",
        plane.display()
    ));

    let told = patient()
        .tell(
            &rig.config(),
            &project::Choices::read(&plane),
            "probe",
            &extension::events::Event::PlaneSaved,
        )
        .expect("it heard");

    let said = told.expect("a write outside notes/ went unreported");
    assert!(said.starts_with("While 'probe' was answering"), "{said}");
    assert!(said.contains(": stray."), "{said}");
}

#[test]
fn an_event_waits_for_the_question_in_flight_rather_than_being_refused() {
    let rig = Rig::new();
    let pid = rig.marker("pid");
    let go = rig.marker("go");
    rig.talking(&format!(
        "#!/bin/sh\nread line\ncase \"$line\" in\n*'\"event\"'*) printf '%s\\n' '{{\"charter\":2}}' ;;\n\
         *) echo $$ > '{}'\nwhile [ ! -e '{}' ]; do sleep 0.01; done\n\
         printf '%s\\n' '{{\"charter\":2,\"blocks\":[]}}' ;;\nesac\n",
        pid.display(),
        go.display()
    ));
    let executor = patient();

    std::thread::scope(|scope| {
        let asking = scope.spawn(|| rig.ask(&executor));
        wait_for(&pid);
        let telling = scope.spawn(|| rig.tell(&executor, &extension::events::Event::PlaneSaved));
        // Long enough for the event to be waiting on the slot; a charter that refused it
        // would have done so by now.
        std::thread::sleep(Duration::from_millis(200));
        std::fs::write(&go, "").expect("go");

        asking
            .join()
            .expect("the asking thread")
            .expect("the view's answer");
        assert_eq!(
            telling.join().expect("the telling thread"),
            Ok(None),
            "the event was refused rather than waiting its turn"
        );
    });
}

#[test]
fn a_briefing_section_comes_back_as_the_program_wrote_it() {
    let rig = Rig::new();
    rig.talking(&answering_with(r#"{"charter":2,"section":"two open PRs"}"#));

    let section = patient().brief(
        &rig.config(),
        &project::Choices::default(),
        "probe",
        serde_json::json!({ "workspace": "ide" }),
    );

    assert_eq!(section, Ok("two open PRs".to_owned()));
}

#[test]
fn how_it_runs_names_only_the_moments_this_extension_is_started_at() {
    let rig = Rig::new();
    rig.write(
        r#"{"version":2,"id":"probe","name":"Probe","capabilities":["cli"],
            "contributes":{"runs":"bin/run",
                           "views":[{"id":"stats","title":"Probe statistics","about":"personas"}],
                           "cli":[{"name":"look","title":"Look","writes":false}]}}"#,
        "#!/bin/sh\n",
    );
    let manifest = extension::manifest_at(&rig.at()).expect("a manifest");

    assert_eq!(
        how_it_runs(&manifest),
        format!(
            "charter starts it when you open one of this extension's views, and when you or a \
             chat run one of its commands (`charter probe <command>`) — never at launch and \
             never on a timer. Each time it is asked one question, given at most {} seconds to \
             answer, and then stopped along with anything it started. A program set on \
             outliving that can, because it runs as you do.",
            DEADLINE.as_secs()
        )
    );
}

#[test]
fn an_executor_made_with_the_app_s_built_in_extensions_holds_them() {
    let dir = tempfile::tempdir().expect("a directory");
    let built_in = extension::BuiltIn::at(dir.path().to_path_buf());

    let executor = Executor::with_built_in(built_in.clone());

    assert_eq!(executor.built_in(), &built_in);
}

// ---- the watch on the plane ----------------------------------------------------------

/// A plane that is a git repository, with one file committed in `notes/`.
fn watched_plane() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("a directory");
    let plane = dir.path();
    crate::testgit::run(plane, &["init", "-q", "."]);
    std::fs::create_dir_all(plane.join("notes")).expect("notes");
    std::fs::write(plane.join("notes/kept.md"), "kept\n").expect("a note");
    crate::testgit::run(plane, &["add", "-A"]);
    crate::testgit::run(
        plane,
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@example.invalid",
            "commit",
            "-qm",
            "notes",
        ],
    );
    dir
}

fn notes() -> Vec<String> {
    vec!["notes/".to_owned()]
}

const NOT_PROOF: &str = ". charter does not confine an extension (ADR 0041): this is what \
     changed while it answered, not proof of who changed it.";

#[test]
fn a_plane_that_is_not_a_repository_is_not_watched() {
    let dir = tempfile::tempdir().expect("a directory");
    assert!(watch::Before::take(dir.path()).is_none());

    let plane = watched_plane();
    assert!(watch::Before::take(plane.path()).is_some());
}

#[test]
fn a_plane_where_nothing_changed_is_reported_as_nothing() {
    let plane = watched_plane();
    // Listed by git before and after, and the same on disk both times.
    std::fs::write(plane.path().join("draft.md"), "draft\n").expect("an untracked file");
    let before = watch::Before::take(plane.path()).expect("a repository");

    assert_eq!(before.overreach("probe", &notes(), false), None);
}

#[test]
fn a_write_outside_the_declared_paths_is_reported_in_charter_s_words() {
    let plane = watched_plane();
    let before = watch::Before::take(plane.path()).expect("a repository");
    std::fs::write(plane.path().join("stray.md"), "stray\n").expect("a stray write");
    std::fs::write(plane.path().join("notes/new.md"), "new\n").expect("a write inside");

    assert_eq!(
        before.overreach("probe", &notes(), false),
        Some(format!(
            "While 'probe' was answering, charter saw these change outside the plane paths it \
             declares it writes: stray.md{NOT_PROOF}"
        ))
    );
}

#[test]
fn a_file_already_changed_that_changes_again_is_seen() {
    let plane = watched_plane();
    std::fs::write(plane.path().join("draft.md"), "draft\n").expect("an untracked file");
    let before = watch::Before::take(plane.path()).expect("a repository");
    std::fs::write(plane.path().join("draft.md"), "a longer draft\n").expect("rewritten");

    let said = before
        .overreach("probe", &notes(), false)
        .expect("unreported");
    assert!(said.contains(": draft.md."), "{said}");
}

#[test]
fn a_delete_inside_the_declared_paths_is_reported_unless_the_action_says_it_deletes() {
    let deleting = || {
        let plane = watched_plane();
        let before = watch::Before::take(plane.path()).expect("a repository");
        std::fs::remove_file(plane.path().join("notes/kept.md")).expect("deleted");
        (plane, before)
    };

    let (_plane, before) = deleting();
    assert_eq!(
        before.overreach("probe", &notes(), false),
        Some(format!(
            "While 'probe' was answering, charter saw these deleted though it does not say it \
             deletes: notes/kept.md{NOT_PROOF}"
        ))
    );

    let (_plane, before) = deleting();
    assert_eq!(before.overreach("probe", &notes(), true), None);
}

#[test]
fn a_change_inside_the_declared_paths_that_is_not_a_delete_is_not_reported() {
    let plane = watched_plane();
    let before = watch::Before::take(plane.path()).expect("a repository");
    std::fs::write(plane.path().join("notes/kept.md"), "kept, and more\n").expect("rewritten");

    assert_eq!(before.overreach("probe", &notes(), false), None);
}

#[test]
fn a_write_outside_and_a_delete_inside_are_both_reported_in_one_sentence() {
    let plane = watched_plane();
    let before = watch::Before::take(plane.path()).expect("a repository");
    std::fs::write(plane.path().join("stray.md"), "stray\n").expect("a stray write");
    std::fs::remove_file(plane.path().join("notes/kept.md")).expect("deleted");

    assert_eq!(
        before.overreach("probe", &notes(), false),
        Some(format!(
            "While 'probe' was answering, charter saw these change outside the plane paths it \
             declares it writes: stray.md, and these deleted though it does not say it deletes: \
             notes/kept.md{NOT_PROOF}"
        ))
    );
}

#[test]
fn a_report_names_five_paths_and_counts_the_rest() {
    let plane = watched_plane();
    let before = watch::Before::take(plane.path()).expect("a repository");
    for n in 1..=7 {
        std::fs::write(plane.path().join(format!("stray-{n}.md")), "x").expect("a stray write");
    }

    let said = before
        .overreach("probe", &notes(), false)
        .expect("unreported");
    assert!(
        said.contains(
            ": stray-1.md, stray-2.md, stray-3.md, stray-4.md, stray-5.md and 2 more. charter"
        ),
        "{said}"
    );
}
