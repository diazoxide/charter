//! The persona statistics program, installed, approved and asked through charter's real
//! executor against a real plane — the whole of ADR 0041 stage 2's path, end to end, with
//! nothing standing in for any part of it.
//!
//! `assemble` puts the built program in an extension directory exactly as the operator would,
//! `extension::install` and `extension::approve` are the dialog's two clicks, `handed::personas`
//! is what the window hands, and `Executor::ask` is what the button calls.

#![cfg(unix)]

use std::path::PathBuf;
use std::time::{Duration, Instant};

use charter_core::executor::Executor;
use charter_core::panel::{Block, Shape};
use charter_core::{extension, handed};

/// How long the program is given by every executor here but the measurement's (#422): the
/// program is copied in fresh for each test, and macOS assesses a program file the first time
/// it runs — under a loaded machine, for longer than the real five seconds. No test here is
/// about the deadline; `charter-core`'s executor tests hold it to account.
const PATIENT: Duration = Duration::from_secs(30);

/// An executor that gives its program [`PATIENT`].
fn patient() -> Executor {
    Executor::default().with_deadline(PATIENT)
}

/// A plane with two personas and some memories, the extension assembled beside it, and a
/// config root with the extension installed and approved.
struct Installed {
    dir: tempfile::TempDir,
}

impl Installed {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("a directory");
        let plane = dir.path().join("plane");
        std::fs::create_dir_all(&plane).expect("a plane");
        std::fs::write(
            plane.join("charter.toml"),
            "[persona]\ndefault = \"steward\"\n",
        )
        .expect("a manifest");
        for (name, days) in [
            ("steward", &["2026-09-22", "2026-09-10"][..]),
            ("release", &[][..]),
        ] {
            let at = plane.join("personas").join(name);
            std::fs::create_dir_all(at.join("memory")).expect("a persona");
            std::fs::write(at.join("persona.md"), "---\nrole: x\n---\n").expect("a definition");
            for (n, day) in days.iter().enumerate() {
                std::fs::write(
                    at.join("memory").join(format!("fact-{n}.md")),
                    format!("# fact {n}\n\n_{day} 10:00 · persistent_\n\nbody\n"),
                )
                .expect("a memory");
            }
        }

        let status = std::process::Command::new(env!("CARGO_BIN_EXE_persona-statistics"))
            .arg("assemble")
            .arg(dir.path().join("ext"))
            .status()
            .expect("assemble runs");
        assert!(status.success(), "assemble failed: {status}");

        let installed = Self { dir };
        let found = extension::install(
            &installed.config(),
            &extension::BuiltIn::none(),
            &installed.dir.path().join("ext"),
        )
        .expect("installed");
        extension::approve(
            &installed.config(),
            found.id(),
            &found.path,
            &found.fingerprint,
        )
        .expect("approved");
        installed
    }

    fn config(&self) -> PathBuf {
        self.dir.path().join("config")
    }

    fn plane(&self) -> PathBuf {
        self.dir.path().join("plane")
    }

    fn ask(
        &self,
        executor: &Executor,
        focus: Option<&str>,
    ) -> Result<charter_core::executor::Answer, String> {
        let plane = self.plane();
        executor.ask(
            &self.config(),
            // What the plane's own files say about extensions, as the window reads them.
            &charter_core::extension::project::Choices::read(&plane),
            "persona-statistics",
            "statistics",
            focus,
            |_| handed::personas(&plane, noon()),
        )
    }
}

fn noon() -> chrono::NaiveDateTime {
    chrono::NaiveDate::from_ymd_opt(2026, 9, 23)
        .and_then(|day| day.and_hms_opt(12, 0, 0))
        .expect("a time")
}

fn charts(blocks: &[Block]) -> Vec<&charter_core::panel::Chart> {
    blocks
        .iter()
        .filter_map(|block| match block {
            Block::Chart(chart) => Some(chart),
            _ => None,
        })
        .collect()
}

/// charter-app#341: charter learned protocol 2, and persona statistics — written for protocol 1,
/// and answering an error to any other — is asked in protocol 1, its manifest's, and stays
/// approved. `extension::tests` pins the fingerprint of a protocol-1 program across the bump;
/// this is the real extension, asked through the real executor.
#[test]
fn a_charter_that_speaks_a_later_protocol_asks_it_in_protocol_1_and_it_stays_approved() {
    let installed = Installed::new();
    const { assert!(charter_core::executor::PROTOCOL > 1) };

    let survey = charter_core::extension::survey(&installed.config(), &extension::BuiltIn::none());
    let row = &survey.installed[0];
    assert_eq!(row.found.as_ref().expect("read").manifest.protocol, 1);
    assert_eq!(row.standing, charter_core::extension::Standing::Approved);

    installed
        .ask(&patient(), None)
        .expect("asked in the protocol it speaks");
}

#[test]
fn the_statistics_are_drawn_from_the_plane_as_it_is_now() {
    let installed = Installed::new();
    let executor = patient();

    let answer = installed.ask(&executor, None).expect("an answer");
    let drawn = charts(&answer.blocks);
    let per = drawn
        .iter()
        .find(|chart| chart.title == "Memories per persona")
        .expect("the per-persona chart");
    assert_eq!(per.shape, Shape::Bars);
    assert_eq!(per.points[0].label, "steward");
    assert_eq!(per.points[0].value, 2);

    // **Live, which is the whole point of a producer** (ADR 0043: a declared chart holds
    // numbers written at install time). A memory remembered now is counted on the next open,
    // with no reinstall and no re-approval — the plane is not in the extension's tree.
    std::fs::write(
        installed.plane().join("personas/release/memory/new.md"),
        "# new\n\n_2026-09-23 09:00 · persistent_\n\nbody\n",
    )
    .expect("a new memory");
    let answer = installed.ask(&executor, None).expect("an answer");
    let per = charts(&answer.blocks)
        .into_iter()
        .find(|chart| chart.title == "Memories per persona")
        .expect("the per-persona chart")
        .clone();
    let release = per
        .points
        .iter()
        .find(|point| point.label == "release")
        .expect("release");
    assert_eq!(release.value, 1, "the new memory was not counted");
}

#[test]
fn opened_from_a_persona_s_card_it_answers_about_that_persona_first() {
    let installed = Installed::new();
    let answer = installed
        .ask(&patient(), Some("steward"))
        .expect("an answer");
    match &answer.blocks[0] {
        Block::Note { text, .. } => assert!(text.starts_with("steward remembers 2"), "{text}"),
        other => panic!("the first block is not a sentence about steward: {other:?}"),
    }
}

#[test]
fn a_rebuilt_program_is_asked_about_again_before_it_runs() {
    // The operator's own case: he rebuilds the extension, and charter must not run the new
    // binary on the old yes.
    let installed = Installed::new();
    let program = installed.dir.path().join("ext/bin/persona-statistics");
    let mut bytes = std::fs::read(&program).expect("the program");
    bytes.extend_from_slice(b"\0rebuilt");
    let beside = installed.dir.path().join("ext/bin/.rebuilt");
    std::fs::write(&beside, &bytes).expect("a rebuilt program");
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&beside, std::fs::Permissions::from_mode(0o755))
            .expect("the mode");
    }
    std::fs::rename(&beside, &program).expect("in place");

    let refused = installed
        .ask(&patient(), None)
        .expect_err("a rebuilt program ran on the old approval");
    assert!(
        refused.contains("changed since you approved it"),
        "{refused}"
    );
}

fn round_trips(
    installed: &Installed,
    executor: &Executor,
    rounds: u32,
) -> (Duration, Duration, Duration) {
    let (mut gate, mut trip, mut whole) = (Duration::ZERO, Duration::ZERO, Duration::ZERO);
    for _ in 0..rounds {
        let began = Instant::now();
        let answer = installed.ask(executor, None).expect("an answer");
        whole += began.elapsed();
        gate += answer.gate;
        trip += answer.round_trip;
    }
    (gate / rounds, trip / rounds, whole / rounds)
}

#[test]
#[ignore = "a measurement, not a guard: cargo test -p persona-statistics -- --ignored --nocapture"]
fn what_asking_the_real_producer_costs() {
    // ADR 0041 gate item 8, with the real program: the gate (re-reading the record and
    // re-hashing the extension's whole directory, which is dominated by the binary's size), the
    // round trip (start, one line each way, stop), and everything around them including the
    // plane read `handed::personas` does.
    let installed = Installed::new();
    let executor = Executor::default();
    installed.ask(&executor, None).expect("a warm-up");
    let size = std::fs::metadata(installed.dir.path().join("ext/bin/persona-statistics"))
        .map(|it| it.len())
        .unwrap_or(0);
    let (gate, trip, whole) = round_trips(&installed, &executor, 20);
    println!(
        "binary {} KiB: gate {gate:?} + round trip {trip:?}; whole ask {whole:?} (20 rounds)",
        size >> 10
    );
}

#[test]
fn it_exits_on_end_of_input_rather_than_waiting_for_a_question() {
    // charter cannot kill a program when charter itself dies; what the program sees is its stdin
    // reaching end-of-file. So a program owes charter this: EOF where its question should be is
    // the end, not a wait. Nothing written to stdout, because there is nobody to read it.
    let mut program = std::process::Command::new(env!("CARGO_BIN_EXE_persona-statistics"));
    program
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let began = Instant::now();
    let out = program.output().expect("it runs");

    assert!(out.status.success(), "{:?}", out);
    assert!(
        out.stdout.is_empty(),
        "it answered nobody: {:?}",
        out.stdout
    );
    assert!(
        began.elapsed() < Duration::from_secs(5),
        "it waited {:?} on a closed stdin",
        began.elapsed()
    );
}
