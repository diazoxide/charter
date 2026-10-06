//! `charter plugin install|uninstall` as a process: every folder it could write is a
//! temporary one, handed in through the environment, and nothing is inherited.
//!
//! What each adapter writes is `purlis-core`'s tests; these are the parts only the binary
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
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_purlis"));
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
    assert!(text.contains("done     enable purlis@purlis-app"), "{text}");

    // The command line itself, `purlis`, though the install ran through its `charter` alias:
    // a hook runs the binary, not the alias that handed it on (RN-3).
    let binary = PathBuf::from(env!("CARGO_BIN_EXE_purlis"))
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
        settings.contains("\"purlis@purlis-app\": true"),
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
    assert!(said(&dry).contains("would    enable purlis@purlis-app"));
    assert!(!m.root.join("home/.claude/settings.json").exists());
    assert!(!m.root.join("home/.codex/config.toml").exists());
    assert!(!m.root.join("config/charter/plugin").exists());

    assert_eq!(m.charter(&["plugin", "install"]).status.code(), Some(0));
    let out = m.charter(&["plugin", "uninstall"]);
    assert_eq!(out.status.code(), Some(0), "{}", said(&out));
    assert!(!m.root.join("config/charter/plugin").exists());
    assert!(
        !m.read("home/.claude/settings.json")
            .contains("purlis@purlis-app")
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

#[test]
fn install_puts_the_opencode_guard_where_opencode_reads_plugins_and_uninstall_takes_it_back() {
    // #371. Named explicitly, so the folder is created; unnamed, a machine without opencode's
    // config folder is skipped.
    let m = Machine::new();
    let out = m.charter(&["plugin", "install", "--harness", "opencode"]);
    assert_eq!(out.status.code(), Some(0), "{}", said(&out));
    assert!(said(&out).contains("opencode:\n"), "{}", said(&out));

    // The command line itself, `purlis`, though the install ran through its `charter` alias:
    // a hook runs the binary, not the alias that handed it on (RN-3).
    let binary = PathBuf::from(env!("CARGO_BIN_EXE_purlis"))
        .canonicalize()
        .unwrap();
    let shim = m.read("home/.config/opencode/plugin/purlis.ts");
    assert!(shim.starts_with(purlis_core::opencode::MARK), "{shim}");
    assert_eq!(
        purlis_core::opencode::binary_in(&shim),
        Some(binary),
        "the guard runs this charter by its path: {shim}"
    );

    let again = m.charter(&["plugin", "install", "--harness", "opencode"]);
    assert!(!said(&again).contains("done "), "{}", said(&again));

    let gone = m.charter(&["plugin", "uninstall", "--harness", "opencode"]);
    assert_eq!(gone.status.code(), Some(0), "{}", said(&gone));
    assert!(
        !m.root
            .join("home/.config/opencode/plugin/purlis.ts")
            .exists()
    );
}

#[test]
fn install_refuses_to_replace_an_opencode_plugin_charter_did_not_write() {
    let m = Machine::new();
    let dir = m.root.join("home/.config/opencode/plugin");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("purlis.ts"),
        "export const Theirs = async () => ({})\n",
    )
    .unwrap();

    let out = m.charter(&["plugin", "install", "--harness", "opencode"]);
    assert_eq!(out.status.code(), Some(1), "{}", said(&out));
    assert!(
        said(&out).contains("purlis did not write it"),
        "{}",
        said(&out)
    );
    assert_eq!(
        m.read("home/.config/opencode/plugin/purlis.ts"),
        "export const Theirs = async () => ({})\n"
    );
}

#[test]
fn a_file_at_the_shims_old_name_that_charter_did_not_write_is_left_beside_it() {
    // #1266: the shim is `purlis.ts` now. A `charter.ts` of charter's own is taken away so
    // opencode never loads two; anybody else's is not charter's to remove.
    let m = Machine::new();
    let dir = m.root.join("home/.config/opencode/plugin");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("charter.ts"),
        "export const Theirs = async () => ({})\n",
    )
    .unwrap();

    let out = m.charter(&["plugin", "install", "--harness", "opencode"]);
    assert_eq!(out.status.code(), Some(0), "{}", said(&out));
    assert_eq!(
        m.read("home/.config/opencode/plugin/charter.ts"),
        "export const Theirs = async () => ({})\n"
    );
    assert!(
        m.read("home/.config/opencode/plugin/purlis.ts")
            .starts_with("// purlis's opencode plugin")
    );
}
