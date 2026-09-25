//! A charter process started the way macOS starts a double-clicked `.app` — and the harness
//! it has to find anyway.
//!
//! **charter-app#134, and the shape of test the suite was missing.** macOS gives a
//! Finder-launched `.app` `PATH=/usr/bin:/bin:/usr/sbin:/sbin`: `launchd` starts a GUI
//! process and no login shell is involved. The operator's `claude` is in `~/.local/bin`,
//! where Claude Code's own installer puts it, so the harness could not be found, `wiring`
//! answered `State::Unknown`, and a double-clicked charter refused every chat with *"an
//! unknown is not a pass — nothing was started"*.
//!
//! Nothing in this repository had ever launched charter that way. CI runs it from a shell,
//! the scenario tests run it from a shell, and every agent runs it from a shell — and a shell
//! hands down a `PATH` that already holds the harness, so the whole class of defect was
//! invisible. **This file is the launch nobody was doing**: `env_clear`, the four directories
//! Finder gives, a `HOME` of the test's own, and the real binary as a real process.
//!
//! It is at the CLI and not in `charter-core` for one reason: `PATH` and `HOME` are the
//! process's, `unsafe_code = "forbid"` rules out `set_var`, and a test that reads the
//! developer's own `$HOME` is a test about the machine it runs on. A child process is the
//! only honest way to ask this question, so these tests spawn one.

use std::path::PathBuf;
use std::process::{Command, Output};

/// Exactly what macOS hands a `.app` opened from Finder, and nothing else.
const FINDER_PATH: &str = "/usr/bin:/bin:/usr/sbin:/sbin";

struct Machine {
    _dir: tempfile::TempDir,
    root: PathBuf,
    home: PathBuf,
    config: PathBuf,
}

impl Machine {
    /// A plane, a home, and a machine store of this test's own — nothing shared with the
    /// developer's or the runner's.
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        // Resolved: a macOS temp directory is under `/var/folders/…`, itself a link to
        // `/private/var/…`, and the paths a refusal prints are the resolved ones.
        let at = std::fs::canonicalize(dir.path()).unwrap();
        let root = at.join("plane");
        let home = at.join("home");
        let config = at.join("config");
        for d in [&root, &home, &config] {
            std::fs::create_dir_all(d).unwrap();
        }
        std::fs::write(root.join("charter.toml"), "schema = 1\n").unwrap();
        // The doctor has a `git identity` row and a machine with none exits 1, which has
        // nothing to do with this file's subject.
        std::fs::write(
            home.join(".gitconfig"),
            "[user]\n\tname = Fixture User\n\temail = fixture@example.invalid\n",
        )
        .unwrap();
        Self {
            _dir: dir,
            root,
            home,
            config,
        }
    }

    /// A `claude` in `$HOME/<dir>` — and **nowhere on `PATH`**, which is the whole point.
    fn harness_in(&self, dir: &str) -> PathBuf {
        let at = self.home.join(dir);
        std::fs::create_dir_all(&at).unwrap();
        stand_in::program(&at, "claude", "#!/bin/sh\n")
    }

    /// `charter doctor --json`, started the way Finder starts an app.
    ///
    /// `env_clear` and then four variables: a process that inherited this test runner's own
    /// environment would inherit its `PATH` too, and would pass for the same reason the bug
    /// hid for a month.
    fn doctor_from_finder(&self) -> Output {
        Command::new(env!("CARGO_BIN_EXE_charter"))
            .args(["doctor", "--json"])
            .current_dir(&self.root)
            .env_clear()
            .env("PATH", FINDER_PATH)
            .env("HOME", &self.home)
            .env("CHARTER_ROOT", &self.root)
            .env("CHARTER_CONFIG_HOME", &self.config)
            .output()
            .expect("charter runs")
    }
}

fn rows(out: &Output) -> Vec<serde_json::Value> {
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "--json prints JSON ({e}): {} / {}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
    })
}

fn row<'a>(rows: &'a [serde_json::Value], name: &str) -> Option<&'a serde_json::Value> {
    rows.iter().find(|r| r["name"] == name)
}

#[test]
fn a_harness_in_the_operators_own_bin_directory_is_found_with_no_path_to_find_it_by() {
    let machine = Machine::new();
    let claude = machine.harness_in(".local/bin");

    let out = machine.doctor_from_finder();
    let rows = rows(&out);
    let profile = row(&rows, "profile claude").unwrap_or_else(|| {
        panic!(
            "charter-app#134: the built-in claude profile was not even listed, with {} \
             installed and PATH={FINDER_PATH}: {}",
            claude.display(),
            String::from_utf8_lossy(&out.stdout)
        )
    });
    assert_eq!(
        profile["status"], "ok",
        "the harness was found, so a chat on it is armed by the app: {profile}"
    );
    let detail = profile["detail"].as_str().unwrap();
    assert!(
        detail.contains("its own plugin, charter@inline"),
        "{profile}"
    );
    assert!(
        detail.contains(&claude.display().to_string()),
        "the row names the harness it found: {profile}"
    );
}

#[test]
fn every_directory_the_search_promises_is_one_a_finder_launch_actually_reaches() {
    // Not a restatement of the constant: this asks the real binary, under the real Finder
    // `PATH`, whether a harness in each of those directories is found — which is the only
    // thing that makes the list in `programs::USER_BIN` worth anything. One at a time,
    // because a `claude` left in an earlier directory would answer for the next one.
    for dir in [
        ".local/bin",
        "bin",
        ".opencode/bin",
        ".bun/bin",
        ".volta/bin",
        ".npm-global/bin",
    ] {
        let machine = Machine::new();
        machine.harness_in(dir);
        let out = machine.doctor_from_finder();
        let rows = rows(&out);
        let profile = row(&rows, "profile claude")
            .unwrap_or_else(|| panic!("a claude in ~/{dir} was not found from a Finder launch"));
        assert_eq!(profile["status"], "ok", "~/{dir}: {profile}");
    }
}

#[test]
fn a_harness_that_is_nowhere_is_a_refusal_that_says_so_and_carries_its_own_fix() {
    // charter-app#134's second half: the message named the config folder and the working
    // directory — neither of which had anything to do with the failure — and never the
    // search. A profile is declared here rather than left built-in because a built-in whose
    // program is missing is not listed at all, and this is about what the row SAYS.
    let machine = Machine::new();
    std::fs::write(
        machine.root.join("charter.local.toml"),
        "[harness.work]\nkind = \"claude\"\ncommand = [\"claude\"]\n",
    )
    .unwrap();
    // Approved, so the row is the search's answer and not the consent gate's.
    approve(&machine, "work");

    let out = machine.doctor_from_finder();
    let rows = rows(&out);
    let profile = row(&rows, "profile work").expect("a declared profile is always listed");
    let detail = profile["detail"].as_str().unwrap();
    assert!(
        detail.contains("could not find a program called claude"),
        "{profile}"
    );
    // **The fix survives the doctor's clip, and that is why it is written before the search.**
    // A doctor row is `DISPLAY_LIMIT` — 160 characters, Python's number — and a `$HOME`-shaped
    // directory list passes that on its own. The sentence is ordered so what is lost is the
    // tail of the search rather than the action; the whole of it is in the app's refusal, and
    // in `charter-core/tests/a_harness_is_found_the_way_a_shell_finds_it.rs`.
    assert!(
        detail.contains("name it by its absolute path in charter.local.toml"),
        "the fix has to survive the row's own clip: {profile}"
    );
    assert!(
        detail.contains("It l") && detail.contains("not shown"),
        "and what is clipped is the search, counted rather than elided: {profile}"
    );
    // Never `charter harness install`, which would run the same missing binary and loop.
    assert!(!detail.contains("charter harness install"), "{profile}");
}

/// Record the operator's approval of `name`, the way a launch records it.
///
/// `charter-core`'s own call rather than a hand-written file: the record lives in the plane
/// (`.charter/`), its shape is charter's to spell, and a test that spelt it itself would stop
/// testing the same thing the day that spelling changed. There is no CLI verb for it — a
/// launch writes it, and `charter doctor` is not a launch.
fn approve(machine: &Machine, name: &str) {
    let set = charter_core::profiles::current(&machine.root);
    let p = set.get(name).expect("the profile was just declared");
    charter_core::profiletrust::record_launched(
        &machine.root,
        name,
        &charter_core::profiletrust::fingerprint(p),
    )
    .expect("the approval is recorded");
}
