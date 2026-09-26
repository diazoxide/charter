//! `charter browser install` through the binary (#370), with a stand-in `npx` on `$PATH` so no
//! test reaches npm. The stand-in writes what the real generator writes — pages under
//! `.claude/skills/playwright-cli/` in the directory it runs in — and records its argv.
//!
//! **Nothing here writes outside the test's own directory.** The binary is a fenced build: a
//! plane outside `$CHARTER_PLANE_FENCE` ends the process before the generator runs, which
//! the last test holds.

#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

struct Scene {
    _dir: tempfile::TempDir,
    root: PathBuf,
    bin: PathBuf,
    log: PathBuf,
}

fn scene() -> Scene {
    let dir = tempfile::tempdir().unwrap();
    let base = std::fs::canonicalize(dir.path()).unwrap();
    let root = base.join("plane");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("charter.toml"), "schema = 1\n").unwrap();
    let bin = base.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let log = base.join("npx.log");
    let npx = bin.join("npx");
    std::fs::write(
        &npx,
        format!(
            "#!/bin/sh\necho \"$@\" >> '{}'\nmkdir -p .claude/skills/playwright-cli\n\
             echo page > .claude/skills/playwright-cli/SKILL.md\necho generated\n",
            log.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&npx, std::fs::Permissions::from_mode(0o755)).unwrap();
    Scene {
        _dir: dir,
        root,
        bin,
        log,
    }
}

fn charter(s: &Scene, args: &[&str], fence: Option<&Path>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_charter"));
    command
        .args(args)
        .current_dir(&s.root)
        .env_clear()
        .env("PATH", format!("{}:/usr/bin:/bin", s.bin.display()))
        .env("HOME", s.root.join("home"))
        .env("CHARTER_ROOT", &s.root)
        .env("NO_COLOR", "1");
    if let Some(fence) = fence {
        command.env("CHARTER_PLANE_FENCE", fence);
    }
    command.output().expect("the binary runs")
}

fn err(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[test]
fn install_generates_the_pinned_skill_and_ignores_the_trace_directory() {
    let s = scene();
    let out = charter(&s, &["browser", "install"], None);
    assert!(out.status.success(), "{}", err(&out));
    assert_eq!(
        std::fs::read_to_string(&s.log).unwrap(),
        "--yes @playwright/cli@0.1.18 install --skills\n"
    );
    assert!(
        s.root
            .join(".claude/skills/playwright-cli/SKILL.md")
            .is_file()
    );
    assert!(
        std::fs::read_to_string(s.root.join(".gitignore"))
            .unwrap()
            .contains(".playwright-cli/\n")
    );
    assert!(err(&out).contains("✓ Wrote .claude/skills/playwright-cli (1 page(s))"));
}

#[test]
fn a_version_that_npm_would_read_as_something_else_is_refused_before_npx_runs() {
    let s = scene();
    for bad in ["latest", "github:attacker/x", "^0.1.0"] {
        let out = charter(&s, &["browser", "install", "--version", bad], None);
        assert_eq!(out.status.code(), Some(1), "{bad}");
        assert!(err(&out).contains("is not a version."), "{}", err(&out));
    }
    assert!(!s.log.exists(), "npx never ran");
    let out = charter(&s, &["browser", "install", "--version", "0.2.0-rc.1"], None);
    assert!(out.status.success(), "{}", err(&out));
    assert!(
        std::fs::read_to_string(&s.log)
            .unwrap()
            .contains("@playwright/cli@0.2.0-rc.1")
    );
}

#[test]
fn a_plane_outside_the_fence_is_never_generated_into() {
    let s = scene();
    let elsewhere = tempfile::tempdir().unwrap();
    let out = charter(&s, &["browser", "install"], Some(elsewhere.path()));
    assert!(!out.status.success());
    assert!(
        err(&out).contains("a fenced build refused"),
        "{}",
        err(&out)
    );
    assert!(!s.log.exists(), "npx never ran");
    assert!(!s.root.join(".claude").exists());
}
