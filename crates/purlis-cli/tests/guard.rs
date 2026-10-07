//! `charter guard` as a process, on a plane `charter init` made: the rule a plane lost is put
//! back, nothing else in the file moves, and the doctor then says the gate is in force.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

struct Plane {
    _tmp: tempfile::TempDir,
    root: PathBuf,
    home: PathBuf,
}

impl Plane {
    fn init() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path().canonicalize().unwrap();
        let root = base.join("plane");
        let home = base.join("home");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&home).unwrap();
        let plane = Self {
            _tmp: tmp,
            root,
            home,
        };
        let out = plane.charter(&["init", "--forge", "github", "--owner", "acme"]);
        assert!(out.status.success(), "{}", said(&out));
        plane
    }

    fn charter(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_purlis"))
            .args(args)
            .current_dir(&self.root)
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", &self.home)
            .env("CHARTER_ROOT", &self.root)
            .env("CHARTER_CONFIG_HOME", self.home.join(".config"))
            .output()
            .expect("charter runs")
    }

    fn settings(&self) -> serde_json::Value {
        read(&self.root.join(".claude/settings.json"))
    }
}

fn read(path: &Path) -> serde_json::Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn said(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

fn gate(plane: &Plane) -> serde_json::Value {
    let out = plane.charter(&["doctor", "--json"]);
    let rows: Vec<serde_json::Value> = serde_json::from_slice(&out.stdout).expect("JSON");
    rows.into_iter()
        .find(|r| r["name"] == "handoff gate")
        .expect("a handoff gate row")
}

/// A handoff is a dispatch (#1444): `init` writes no harness rule for one, the doctor's row
/// is fine without it, and `guard handoff` writes nothing and says what consents now.
#[test]
fn a_new_plane_has_no_handoff_rule_and_guard_handoff_writes_none() {
    let plane = Plane::init();
    let before = std::fs::read(plane.root.join(".claude/settings.json")).unwrap();
    let asks = plane.settings()["permissions"]["ask"].to_string();
    assert!(
        !asks.contains("handoff"),
        "init wrote a handoff rule: {asks}"
    );
    assert_eq!(gate(&plane)["status"], "ok", "{}", gate(&plane));

    let out = plane.charter(&["guard", "handoff"]);
    assert_eq!(out.status.code(), Some(2), "{}", said(&out));
    let told = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        told.contains("is retired, and nothing was written"),
        "{told}"
    );
    assert!(
        told.contains("purlis guard ask 'purlis handoff*'"),
        "{told}"
    );
    assert_eq!(
        std::fs::read(plane.root.join(".claude/settings.json")).unwrap(),
        before
    );
}

/// The rule an older `init` wrote: the doctor warns while it is there, and `doctor --fix
/// handoff-rule` removes that rule and no other key of the operator's.
#[test]
fn the_handoff_rule_an_older_init_wrote_is_removed_by_its_fix_and_nothing_else_moves() {
    let plane = Plane::init();
    let mut settings = plane.settings();
    let asks = settings["permissions"]["ask"].as_array().unwrap().clone();
    let mut older = vec![
        serde_json::json!("Bash(charter handoff *)"),
        serde_json::json!("Bash(purlis handoff *)"),
    ];
    older.extend(asks.clone());
    settings["permissions"]["ask"] = serde_json::json!(older);
    settings["theme"] = serde_json::json!("dark");
    std::fs::write(
        plane.root.join(".claude/settings.json"),
        serde_json::to_string_pretty(&settings).unwrap(),
    )
    .unwrap();
    let there = gate(&plane);
    assert_eq!(there["status"], "warn", "{there}");
    assert_eq!(there["fix"], "handoff-rule", "{there}");

    // Bare `--fix` leaves it: the file is one every teammate pulls.
    plane.charter(&["doctor", "--fix"]);
    assert_eq!(gate(&plane)["status"], "warn");

    let out = plane.charter(&["doctor", "--fix", "handoff-rule"]);
    let told = said(&out);
    assert!(
        told.contains("removed Bash(charter handoff *), Bash(purlis handoff *)"),
        "{told}"
    );
    let after = plane.settings();
    assert_eq!(after["permissions"]["ask"], serde_json::json!(asks));
    assert_eq!(after["theme"], "dark");
    assert_eq!(after["env"], settings["env"]);
    assert_eq!(gate(&plane)["status"], "ok");
}

#[test]
fn a_pattern_no_rule_can_say_is_refused_and_nothing_is_written() {
    let plane = Plane::init();
    let before = std::fs::read(plane.root.join(".claude/settings.json")).unwrap();
    let out = plane.charter(&["guard", "ask", "mcp__slack__send *"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(said(&out).contains("cannot write"), "{}", said(&out));
    assert_eq!(
        before,
        std::fs::read(plane.root.join(".claude/settings.json")).unwrap()
    );
}

/// The doctor's `ask rules` row, as `--json` prints it.
fn ask_rules(plane: &Plane) -> serde_json::Value {
    let out = plane.charter(&["doctor", "--json"]);
    let rows: Vec<serde_json::Value> = serde_json::from_slice(&out.stdout).expect("JSON");
    rows.into_iter()
        .find(|r| r["name"] == "ask rules")
        .expect("an ask rules row")
}

#[test]
fn a_plane_that_lost_its_report_rule_gets_it_back_from_guard_report() {
    // #363 D11: `init` writes it, the doctor names its absence, `guard report` puts it back.
    let plane = Plane::init();
    let rule = serde_json::json!("Bash(charter report *--yes*)");
    let mut settings = plane.settings();
    assert!(
        settings["permissions"]["ask"]
            .as_array()
            .unwrap()
            .contains(&rule),
        "init writes the rule: {settings}"
    );
    assert_eq!(ask_rules(&plane)["status"], "ok");

    settings["permissions"]["ask"] = serde_json::json!(["Bash(terraform *)"]);
    std::fs::write(
        plane.root.join(".claude/settings.json"),
        serde_json::to_string_pretty(&settings).unwrap(),
    )
    .unwrap();
    let lost = ask_rules(&plane);
    assert_eq!(lost["status"], "warn", "{lost}");
    assert!(
        lost["hint"]
            .as_str()
            .unwrap()
            .contains("`purlis guard report`"),
        "{lost}"
    );

    let out = plane.charter(&["guard", "report"]);
    assert_eq!(out.status.code(), Some(0), "{}", said(&out));
    assert_eq!(
        plane.settings()["permissions"]["ask"],
        serde_json::json!(["Bash(terraform *)", rule])
    );
    assert_eq!(ask_rules(&plane)["status"], "ok");
}
