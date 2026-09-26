//! `charter shell-guard` as a shell tab's shim runs it: a real process, a real socket, and a
//! stand-in harness that writes down how it was started (SI-5, ADR 0062).
//!
//! The one rule behind every test here is the hook's: it never stands in the way. Whatever
//! else happens, the harness the operator typed starts, with the arguments they typed.
#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::{Mutex, mpsc};
use std::time::Duration;

use charter_core::hookwire::{CHAT_ENV, Listener, SOCKET_ENV, StartedByHand};

const CHARTER: &str = env!("CARGO_BIN_EXE_charter");

/// A harness stand-in at `dir/name` that writes its arguments, one per line, then `PATH=` and
/// the `PATH` it was started under, to `out`.
fn stand_in(dir: &Path, name: &str, out: &Path) -> PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    let path = dir.join(name);
    std::fs::write(
        &path,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" \"PATH=$PATH\" > '{}'\n",
            out.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

struct Tab {
    dir: tempfile::TempDir,
    shims: PathBuf,
    out: PathBuf,
}

impl Tab {
    /// A shell tab's world: a shim directory first on `PATH`, holding a `claude` of its own,
    /// and the real `claude` after it.
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let shims = dir.path().join("shims");
        let out = dir.path().join("said");
        // What the shim directory holds does not matter to the guard, only that it is where
        // the shims are: a `claude` here must never be the one that runs.
        stand_in(&shims, "claude", &dir.path().join("the shim ran"));
        stand_in(&dir.path().join("real"), "claude", &out);
        Self { dir, shims, out }
    }

    fn path(&self) -> String {
        format!(
            "{}:{}:/usr/bin:/bin",
            self.shims.display(),
            self.dir.path().join("real").display()
        )
    }

    fn guard(&self, word: &str, args: &[&str], env: &[(&str, &str)]) -> Output {
        Command::new(CHARTER)
            .args(["shell-guard", "--shims"])
            .arg(&self.shims)
            .arg(word)
            .arg("--")
            .args(args)
            .current_dir(self.dir.path())
            .env_clear()
            .env("PATH", self.path())
            .envs(env.iter().copied())
            .stdin(Stdio::null())
            .output()
            .expect("charter runs")
    }

    fn said(&self) -> Vec<String> {
        std::fs::read_to_string(&self.out)
            .expect("the real harness ran")
            .lines()
            .map(str::to_owned)
            .collect()
    }
}

#[test]
fn a_harness_started_by_hand_runs_the_real_one_with_every_argument_it_was_given() {
    let tab = Tab::new();

    let ran = tab.guard("claude", &["-p", "two words", "--", "--help", "it's"], &[]);

    assert!(ran.status.success(), "{ran:?}");
    let said = tab.said();
    assert_eq!(said[..5], ["-p", "two words", "--", "--help", "it's"]);
    assert!(!tab.dir.path().join("the shim ran").exists());
}

#[test]
fn a_harness_started_by_hand_runs_with_the_shims_off_its_path_so_it_cannot_come_back() {
    let tab = Tab::new();

    let ran = tab.guard("claude", &[], &[]);

    assert!(ran.status.success(), "{ran:?}");
    let path = tab.said().pop().unwrap();
    assert_eq!(
        path,
        format!(
            "PATH={}:/usr/bin:/bin",
            tab.dir.path().join("real").display()
        )
    );
}

#[test]
fn a_harness_started_by_hand_says_it_runs_outside_charters_session_tracking() {
    let tab = Tab::new();

    let ran = tab.guard("claude", &[], &[]);

    let said = String::from_utf8_lossy(&ran.stderr);
    assert!(
        said.contains("this claude runs outside charter's session tracking"),
        "{said}"
    );
    assert_eq!(said.lines().count(), 1, "{said}");
}

#[test]
fn a_harness_started_in_a_chats_shell_tells_the_app_where_it_was_started() {
    let tab = Tab::new();
    let socket = tab.dir.path().join("app").join("hooks.sock");
    let (tx, rx) = mpsc::channel();
    let tx = Mutex::new(tx);
    let _reading = Listener::bind(tab.dir.path(), &socket)
        .expect("a socket")
        .each_answering_and_noticing(
            Box::new(|_| panic!("a harness started by hand is not a report")),
            Box::new(|_, _| panic!("a harness started by hand is not an ask")),
            Box::new(move |notice| tx.lock().unwrap().send(notice).unwrap()),
        );

    let ran = tab.guard(
        "claude",
        &[],
        &[(SOCKET_ENV, socket.to_str().unwrap()), (CHAT_ENV, "12")],
    );

    assert!(ran.status.success(), "{ran:?}");
    assert_eq!(
        rx.recv_timeout(Duration::from_secs(5)),
        Ok(StartedByHand {
            chat: 12,
            started_by_hand: "claude".to_owned(),
            cwd: Some(tab.dir.path().canonicalize().unwrap()),
        })
    );
}

#[test]
fn a_harness_still_starts_when_no_app_is_listening() {
    let tab = Tab::new();
    let gone = tab.dir.path().join("gone.sock");

    let ran = tab.guard(
        "claude",
        &["--version"],
        &[(SOCKET_ENV, gone.to_str().unwrap()), (CHAT_ENV, "12")],
    );

    assert!(ran.status.success(), "{ran:?}");
    assert_eq!(tab.said()[0], "--version");
}

#[test]
fn a_harness_found_nowhere_but_the_shims_is_refused_with_the_shells_not_found_code() {
    let tab = Tab::new();

    let ran = tab.guard("codex", &[], &[]);

    assert_eq!(ran.status.code(), Some(127), "{ran:?}");
    assert!(
        String::from_utf8_lossy(&ran.stderr).contains("no codex on PATH"),
        "{ran:?}"
    );
}
