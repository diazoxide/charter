//! The command line ships as `purlis`, and `charter` keeps running it for the rename's window
//! (RN-3, #1255; V93k).
//!
//! `charter` is a small alias that hands its whole invocation to the `purlis` beside it: the
//! arguments, stdin, the environment, the working directory and the exit code. These run both
//! names the way a terminal, a hook and a script do, including through a link on `PATH`.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const PURLIS: &str = env!("CARGO_BIN_EXE_purlis");
const CHARTER: &str = env!("CARGO_BIN_EXE_charter");

/// `binary args…` with nothing of this run's environment but a `PATH` that holds no purlis, and
/// a home, a config home and a plane of its own, so neither name can reach this machine's.
fn run(binary: &Path, args: &[&str], stdin: &str) -> Output {
    let here = tempfile::tempdir().expect("a home");
    let home = here.path().canonicalize().expect("the home");
    let mut child = Command::new(binary)
        .args(args)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", &home)
        .env("CHARTER_CONFIG_HOME", home.join("config"))
        .env("CHARTER_ROOT", &home)
        .current_dir(&home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the binary starts");
    child
        .stdin
        .take()
        .expect("a stdin")
        .write_all(stdin.as_bytes())
        .expect("stdin is written");
    child.wait_with_output().expect("the binary finishes")
}

#[test]
fn purlis_and_charter_both_run_the_cli() {
    for binary in [PURLIS, CHARTER] {
        let out = run(Path::new(binary), &["--version"], "");
        assert_eq!(out.status.code(), Some(0), "{binary}: {out:?}");
        assert!(
            String::from_utf8_lossy(&out.stdout).contains(env!("CARGO_PKG_VERSION")),
            "{binary}: {out:?}"
        );
    }
}

#[test]
fn the_version_line_names_the_profile_the_binary_was_built_with() {
    // Cargo files a binary under its profile's name (`target/<profile>/purlis`), which is the
    // answer the line has to give without having been told it: `dev-release` for a dev channel
    // build and `release` for a stable one (ADR 0092), `debug` here. `PROFILE` in a build
    // script would call the first of those `release`.
    let profile = Path::new(PURLIS)
        .parent()
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .expect("the binary is in its profile's directory");
    let line = format!("purlis {} ({profile} profile)\n", env!("CARGO_PKG_VERSION"));
    for binary in [PURLIS, CHARTER] {
        let out = run(Path::new(binary), &["--version"], "");
        assert_eq!(String::from_utf8_lossy(&out.stdout), line, "{binary}");
    }
}

#[test]
fn the_charter_alias_answers_exactly_what_purlis_answers() {
    // A usage error: its exit code and its sentence, carried through unchanged — except the
    // usage line, which names the command the way it was called (`argv[0]` is handed on).
    let purlis = run(Path::new(PURLIS), &["--no-such-flag"], "");
    let charter = run(Path::new(CHARTER), &["--no-such-flag"], "");
    assert_ne!(purlis.status.code(), Some(0), "{purlis:?}");
    assert_eq!(charter.status.code(), purlis.status.code());
    assert_eq!(charter.stdout, purlis.stdout);
    let purlis_said = String::from_utf8_lossy(&purlis.stderr);
    let charter_said = String::from_utf8_lossy(&charter.stderr);
    assert!(purlis_said.contains("Usage: purlis"), "{purlis_said}");
    assert!(charter_said.contains("Usage: charter"), "{charter_said}");
    assert_eq!(charter_said, purlis_said.replace("purlis", "charter"));
}

#[test]
fn the_charter_alias_hands_purlis_its_stdin() {
    // `hook pretooluse` reads its payload from stdin: a guard that never saw it would let the
    // call through. Both names deny a vault read, which they can only do having read it.
    let read = format!("cat .charter/{}/db.json", "vaults");
    let payload = format!(r#"{{"tool_name":"Bash","tool_input":{{"command":"{read}"}}}}"#);
    for binary in [PURLIS, CHARTER] {
        let out = run(Path::new(binary), &["hook", "pretooluse"], &payload);
        assert_eq!(out.status.code(), Some(0), "{binary}: {out:?}");
        let said = String::from_utf8_lossy(&out.stdout);
        assert!(
            said.contains(r#""permissionDecision": "deny""#),
            "{binary}: {said}"
        );
    }
}

/// The PATH install links `charter` at the app's binary from a directory of its own, so the
/// alias has to find `purlis` beside where it really is, not beside the link.
#[cfg(unix)]
#[test]
fn the_charter_alias_finds_purlis_through_a_link_in_another_directory() {
    let bin = tempfile::tempdir().expect("a PATH directory");
    let link: PathBuf = bin.path().join("charter");
    std::os::unix::fs::symlink(CHARTER, &link).expect("the link");

    let out = run(&link, &["--version"], "");

    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert!(
        String::from_utf8_lossy(&out.stdout).contains(env!("CARGO_PKG_VERSION")),
        "{out:?}"
    );
}

/// A `charter` with no `purlis` beside it says so and fails, rather than running something
/// else called `purlis` that a `PATH` lookup happens to find.
#[cfg(unix)]
#[test]
fn a_charter_alias_with_no_purlis_beside_it_says_so() {
    let alone = tempfile::tempdir().expect("a directory");
    let copy = alone.path().join("charter");
    std::fs::copy(CHARTER, &copy).expect("a copy of the alias alone");

    let out = run(&copy, &["--version"], "");

    assert_ne!(out.status.code(), Some(0), "{out:?}");
    let said = String::from_utf8_lossy(&out.stderr);
    assert!(said.contains("purlis"), "{said}");
}
