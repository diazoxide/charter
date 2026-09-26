//! `charter plugin install|uninstall` as a process: every folder it could write is a
//! temporary one, handed in through the environment, and nothing is inherited.
//!
//! What each adapter writes is `charter-core`'s tests; these are the parts only the binary
//! has — which charter the hooks name, where the bundle comes from, and the exit status.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

struct Machine {
    _dir: tempfile::TempDir,
    root: PathBuf,
}

impl Machine {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        for d in ["home/.claude", "home/.codex", "config"] {
            std::fs::create_dir_all(root.join(d)).unwrap();
        }
        Self { _dir: dir, root }
    }

    fn charter(&self, args: &[&str]) -> Output {
        let bundle = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../app/src-tauri/plugin");
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_charter"));
        cmd.args(args);
        if args.get(1) == Some(&"install") {
            cmd.arg("--plugin-from").arg(bundle);
        }
        cmd.current_dir(&self.root)
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", self.root.join("home"))
            .env("CLAUDE_CONFIG_DIR", self.root.join("home/.claude"))
            .env("CODEX_HOME", self.root.join("home/.codex"))
            .env("CHARTER_CONFIG_HOME", self.root.join("config"))
            .output()
            .expect("charter runs")
    }

    fn read(&self, rel: &str) -> String {
        std::fs::read_to_string(self.root.join(rel)).unwrap()
    }
}

fn said(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

#[test]
fn install_wires_both_harnesses_to_this_charter_and_a_second_run_changes_nothing() {
    let m = Machine::new();
    let out = m.charter(&["plugin", "install"]);
    assert_eq!(out.status.code(), Some(0), "{}", said(&out));
    let text = said(&out);
    assert!(text.contains("claude:\n"), "{text}");
    assert!(text.contains("codex:\n"), "{text}");
    assert!(
        text.contains("done    enable charter@charter-app"),
        "{text}"
    );

    let binary = PathBuf::from(env!("CARGO_BIN_EXE_charter"))
        .canonicalize()
        .unwrap();
    let hooks = m.read("config/charter/plugin/hooks/hooks.json");
    assert!(
        hooks.contains(&format!("'{}' hook pretooluse", binary.display())),
        "{hooks}"
    );
    let config = m.read("home/.codex/config.toml");
    assert!(
        config.contains(&format!("'{}' hook pretooluse", binary.display())),
        "{config}"
    );
    let settings = m.read("home/.claude/settings.json");
    assert!(
        settings.contains("\"charter@charter-app\": true"),
        "{settings}"
    );
    assert!(!settings.contains("\"charter@charter\""), "{settings}");

    let again = m.charter(&["plugin", "install"]);
    assert_eq!(again.status.code(), Some(0));
    let text = said(&again);
    assert!(!text.contains("done "), "{text}");
    assert_eq!(settings, m.read("home/.claude/settings.json"));
    assert_eq!(config, m.read("home/.codex/config.toml"));
}

#[test]
fn a_dry_run_writes_nothing_and_uninstall_takes_it_all_back() {
    let m = Machine::new();
    let dry = m.charter(&["plugin", "install", "--dry-run"]);
    assert_eq!(dry.status.code(), Some(0));
    assert!(said(&dry).contains("would   enable charter@charter-app"));
    assert!(!m.root.join("home/.claude/settings.json").exists());
    assert!(!m.root.join("home/.codex/config.toml").exists());
    assert!(!m.root.join("config/charter/plugin").exists());

    assert_eq!(m.charter(&["plugin", "install"]).status.code(), Some(0));
    let out = m.charter(&["plugin", "uninstall"]);
    assert_eq!(out.status.code(), Some(0), "{}", said(&out));
    assert!(!m.root.join("config/charter/plugin").exists());
    assert!(
        !m.read("home/.claude/settings.json")
            .contains("charter@charter-app")
    );
    assert!(
        !m.read("home/.codex/config.toml")
            .contains("hook pretooluse")
    );
}

#[test]
fn only_the_named_harness_is_touched() {
    let m = Machine::new();
    let out = m.charter(&["plugin", "install", "--harness", "codex"]);
    assert_eq!(out.status.code(), Some(0), "{}", said(&out));
    assert!(m.root.join("home/.codex/config.toml").is_file());
    assert!(!m.root.join("home/.claude/settings.json").exists());
    assert!(!said(&out).contains("claude:"));
}

#[test]
fn a_file_it_cannot_read_fails_the_run_and_is_left_as_it_was() {
    let m = Machine::new();
    std::fs::write(m.root.join("home/.claude/settings.json"), "{broken").unwrap();
    let out = m.charter(&["plugin", "install", "--harness", "claude"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(said(&out).contains("nothing changed:"), "{}", said(&out));
    assert_eq!(m.read("home/.claude/settings.json"), "{broken");
}
