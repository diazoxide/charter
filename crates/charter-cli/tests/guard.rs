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
        Command::new(env!("CARGO_BIN_EXE_charter"))
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

#[test]
fn a_plane_that_lost_its_handoff_rule_gets_it_back_and_nothing_else_moves() {
    let plane = Plane::init();
    let rule = serde_json::json!("Bash(charter handoff *)");
    let mut settings = plane.settings();
    assert!(
        settings["permissions"]["ask"]
            .as_array()
            .unwrap()
            .contains(&rule),
        "init writes the rule: {settings}"
    );
    assert_eq!(gate(&plane)["status"], "ok");

    // The operator's other keys, and the rule removed.
    settings["permissions"]["ask"] = serde_json::json!([]);
    settings["theme"] = serde_json::json!("dark");
    std::fs::write(
        plane.root.join(".claude/settings.json"),
        serde_json::to_string_pretty(&settings).unwrap(),
    )
    .unwrap();
    let lost = gate(&plane);
    assert_eq!(lost["status"], "warn", "{lost}");
    assert!(
        lost["hint"]
            .as_str()
            .unwrap()
            .contains("`charter guard handoff`"),
        "{lost}"
    );

    let out = plane.charter(&["guard", "handoff"]);
    assert_eq!(out.status.code(), Some(0), "{}", said(&out));
    let after = plane.settings();
    assert_eq!(after["permissions"]["ask"], serde_json::json!([rule]));
    assert_eq!(after["theme"], "dark");
    assert_eq!(after["env"], settings["env"]);
    assert_eq!(gate(&plane)["status"], "ok");

    let listed = plane.charter(&["guard"]);
    assert!(
        String::from_utf8_lossy(&listed.stdout).contains("ask   Bash(charter handoff *)"),
        "{}",
        said(&listed)
    );
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
