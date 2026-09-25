//! `charter <extension id> <command> …`, as a chat or a script runs it (charter-app#342).
//!
//! The extension is the test-only `extension-probe`, assembled, installed and approved as the
//! Extensions dialog would; then the real `charter` binary is run. Everything the executor does
//! is proven through it in `extension-probe`'s own tests — what is here is that the command line
//! reaches it, passes on what it printed unchanged, and says why when it will not.

#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use charter_core::extension;

const CHARTER: &str = env!("CARGO_BIN_EXE_charter");

/// The probe's program, built beside this binary. `cargo test --workspace` has built it already;
/// a narrower run builds it here, once, with the cargo that is running this test.
fn probe_program() -> PathBuf {
    static BUILT: OnceLock<PathBuf> = OnceLock::new();
    BUILT
        .get_or_init(|| {
            let beside = Path::new(CHARTER).with_file_name("extension-probe");
            if !beside.is_file() {
                let status = Command::new(env!("CARGO"))
                    .args(["build", "--quiet", "-p", "extension-probe"])
                    .status()
                    .expect("cargo runs");
                assert!(status.success(), "the probe did not build: {status}");
            }
            beside
        })
        .clone()
}

/// A plane, a config home, and the probe assembled beside them — approved, or only installed.
struct Setup {
    dir: tempfile::TempDir,
}

impl Setup {
    fn approved() -> Self {
        let setup = Self::installed();
        let found = extension::read_at(&setup.ext()).expect("readable");
        extension::approve(&setup.config(), found.id(), &found.path, &found.fingerprint)
            .expect("approved");
        setup
    }

    fn installed() -> Self {
        let dir = tempfile::tempdir().expect("a directory");
        let root = std::fs::canonicalize(dir.path()).expect("a real path");
        let plane = root.join("plane");
        std::fs::create_dir_all(&plane).expect("a plane");
        std::fs::write(plane.join("charter.toml"), "schema = 1\n").expect("a manifest");
        let status = Command::new(probe_program())
            .arg("assemble")
            .arg(root.join("ext"))
            .status()
            .expect("assemble runs");
        assert!(status.success(), "assemble failed: {status}");
        let setup = Self { dir };
        extension::install(&setup.config(), &extension::BuiltIn::none(), &setup.ext())
            .expect("installed");
        setup
    }

    fn root(&self) -> PathBuf {
        std::fs::canonicalize(self.dir.path()).expect("a real path")
    }

    fn plane(&self) -> PathBuf {
        self.root().join("plane")
    }

    fn ext(&self) -> PathBuf {
        self.root().join("ext")
    }

    fn config(&self) -> PathBuf {
        self.root().join("config")
    }

    /// `charter <args…>` standing in the plane, with this setup's config home.
    fn charter(&self, args: &[&str]) -> Ran {
        let out = Command::new(CHARTER)
            .args(args)
            .current_dir(self.plane())
            .env_clear()
            .env("PATH", std::env::var("PATH").unwrap_or_default())
            .env("HOME", self.root())
            .env("CHARTER_ROOT", self.plane())
            .env("CHARTER_CONFIG_HOME", self.config())
            .output()
            .expect("charter runs");
        Ran {
            stdout: out.stdout,
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
            code: out.status.code().unwrap_or(-1),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct Ran {
    stdout: Vec<u8>,
    stderr: String,
    code: i32,
}

#[test]
fn an_extension_command_reaches_the_caller_with_its_output_and_status_unchanged() {
    let setup = Setup::approved();

    let ran = setup.charter(&["extension-probe", "echo", "one", "two  three"]);
    assert_eq!(ran.stdout, b"one two  three\n");
    assert_eq!(ran.stderr, "echoed 2 words\n");
    assert_eq!(ran.code, 0);

    let failed = setup.charter(&["extension-probe", "fail", "7"]);
    assert_eq!(failed.stdout, b"");
    assert_eq!(failed.stderr, "failing with 7, as asked\n");
    assert_eq!(failed.code, 7);
}

#[test]
fn a_command_of_an_extension_changed_on_disk_or_never_approved_says_so_and_does_not_run() {
    let unapproved = Setup::installed();
    let ran = unapproved.charter(&["extension-probe", "echo", "hi"]);
    assert_eq!(ran.stdout, b"");
    assert!(
        ran.stderr.contains("you have not approved"),
        "{}",
        ran.stderr
    );
    assert_eq!(ran.code, 1);

    let changed = Setup::approved();
    let manifest = changed.ext().join(extension::MANIFEST);
    let edited = std::fs::read_to_string(&manifest)
        .expect("the manifest")
        .replace("\"Extension probe\"", "\"Extension probe, edited\"");
    std::fs::write(&manifest, edited).expect("edited");
    let ran = changed.charter(&["extension-probe", "echo", "hi"]);
    assert_eq!(ran.stdout, b"");
    assert!(
        ran.stderr.contains("changed since you approved it"),
        "{}",
        ran.stderr
    );
    assert_eq!(ran.code, 1);
}

#[test]
fn a_word_that_is_neither_charters_nor_an_installed_extensions_is_clap_s_error_and_says_why() {
    let setup = Setup::approved();
    let ran = setup.charter(&["stauts"]);
    assert_eq!(ran.code, 1);
    // clap's own sentence and its suggestion, as before extensions had commands…
    assert!(
        ran.stderr.contains("unrecognized subcommand 'stauts'") && ran.stderr.contains("status"),
        "{}",
        ran.stderr
    );
    // …and one line saying where else charter looked, and where it cannot look yet.
    assert!(
        ran.stderr
            .contains("No extension installed on this machine is called 'stauts' either")
            && ran.stderr.contains("built-in"),
        "{}",
        ran.stderr
    );
}

#[test]
fn an_extension_with_no_command_named_lists_the_ones_it_has() {
    let setup = Setup::approved();
    let ran = setup.charter(&["extension-probe"]);
    assert_eq!(ran.stdout, b"");
    assert_eq!(ran.code, 1);
    assert!(
        ran.stderr.contains("charter extension-probe <command>")
            && ran.stderr.contains("stamp — Stamp a note (writes)")
            && ran.stderr.contains("echo — Say its words back"),
        "{}",
        ran.stderr
    );
    // And asked for help, the same list is the answer rather than a refusal.
    let help = setup.charter(&["extension-probe", "--help"]);
    assert_eq!(help.code, 0);
    assert!(
        help.stderr.contains("echo — Say its words back"),
        "{}",
        help.stderr
    );
}

#[test]
fn a_command_of_an_extension_the_project_turned_off_says_so_and_does_not_run() {
    let setup = Setup::approved();
    std::fs::write(
        setup.plane().join("charter.toml"),
        "schema = 1\n\n[extensions.extension-probe]\nenabled = false\n",
    )
    .expect("the project turns it off");
    let ran = setup.charter(&["extension-probe", "stamp"]);
    assert_eq!(ran.stdout, b"");
    assert!(
        ran.stderr.contains("is turned off in charter.toml"),
        "{}",
        ran.stderr
    );
    assert_eq!(ran.code, 1);
    assert!(!setup.plane().join("notes").exists(), "it ran");
}

#[test]
fn a_bad_word_under_a_core_command_is_clap_s_error_alone() {
    // clap says a nested word is unknown the same way it says a first one is; the line about
    // extensions is only about a first word.
    let setup = Setup::approved();
    let ran = setup.charter(&["ws", "bogus"]);
    assert_eq!(ran.code, 1);
    assert!(
        ran.stderr.contains("unrecognized subcommand 'bogus'"),
        "{}",
        ran.stderr
    );
    assert!(!ran.stderr.contains("No extension"), "{}", ran.stderr);
}

#[test]
fn a_core_owned_alias_forwards_to_the_extension_command_with_identical_output() {
    // The mechanism `charter ws todo` will use once todos is an extension: a core word that
    // forwards to an extension's command, keeping its own words. Only a test build has one.
    let setup = Setup::approved();
    for args in [&["hi", "there"][..], &[][..]] {
        let direct: Vec<&str> = ["extension-probe", "echo"]
            .into_iter()
            .chain(args.iter().copied())
            .collect();
        let aliased: Vec<&str> = ["ws", "probe-echo"]
            .into_iter()
            .chain(args.iter().copied())
            .collect();
        assert_eq!(setup.charter(&aliased), setup.charter(&direct), "{args:?}");
    }
    assert_eq!(
        setup.charter(&["ws", "probe-fail", "3"]),
        setup.charter(&["extension-probe", "fail", "3"])
    );
}

impl Setup {
    /// Give the plane a front-door persona, `ops`, whose tools declare `charter`.
    fn with_a_persona_that_runs_charter(self) -> Self {
        let plane = self.plane();
        std::fs::write(
            plane.join("charter.toml"),
            "schema = 1\n\n[persona]\ndefault = \"ops\"\n",
        )
        .expect("a manifest");
        let persona = plane.join("personas/ops");
        std::fs::create_dir_all(persona.join("memory")).expect("a persona");
        std::fs::write(
            persona.join("persona.md"),
            "---\nrole: Operations\ntools: charter\n---\n\n# ops\n",
        )
        .expect("its charter");
        self
    }

    /// What `charter hook pretooluse` answers a chat's Bash call of `command`.
    fn guard(&self, command: &str) -> String {
        use std::io::Write;
        let payload = serde_json::json!({"session_id": "s-1", "cwd": ".", "tool_name": "Bash",
            "tool_input": {"command": command}})
        .to_string();
        let mut child = Command::new(CHARTER)
            .args(["hook", "pretooluse"])
            .current_dir(self.plane())
            .env_clear()
            .env("PATH", std::env::var("PATH").unwrap_or_default())
            .env("HOME", self.root())
            .env("CHARTER_ROOT", self.plane())
            .env("CHARTER_CONFIG_HOME", self.config())
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("charter runs");
        child
            .stdin
            .take()
            .expect("stdin")
            .write_all(payload.as_bytes())
            .expect("the payload is written");
        let out = child.wait_with_output().expect("charter finishes");
        String::from_utf8_lossy(&out.stdout).into_owned()
    }
}

#[test]
fn a_chats_extension_command_that_writes_is_never_waved_through_by_a_persona_grant() {
    let setup = Setup::approved().with_a_persona_that_runs_charter();
    let allowed = |said: &str| said.contains(r#""permissionDecision": "allow""#);

    // A core command the persona's grant covers, and a command the probe says only reads: both
    // run without a prompt, as any `charter` call the persona declares does.
    assert!(
        allowed(&setup.guard("charter status")),
        "the grant covers charter"
    );
    assert!(allowed(&setup.guard("charter extension-probe echo hi")));
    // One that writes still meets the operator's prompt, and so does one charter cannot say
    // reads — a command the extension does not declare, or an extension nobody installed.
    for writes in [
        "charter extension-probe stamp",
        "charter extension-probe teleport",
        "charter no-such-extension anything",
    ] {
        let said = setup.guard(writes);
        assert!(!allowed(&said), "{writes:?} was waved through: {said}");
    }
}
