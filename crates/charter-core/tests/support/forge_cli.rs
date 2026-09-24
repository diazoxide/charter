//! A stand-in `gh` and `glab` that answer only what a test wrote down, and the child run
//! that puts them first on `PATH`. `a_forge_cli_is_asked_exactly_what_python_asked.rs`'s module
//! docs say why a test reaches them through a child of its own binary, and what they answer.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Set in the child run; unset, every test run in it is a no-op.
pub const CHILD: &str = "CHARTER_TEST_FORGE_CLI_CHILD";
/// The directory the child's stand-ins live in, for the tests that check the path is pinned.
pub const BIN: &str = "CHARTER_TEST_FORGE_CLI_BIN";

const STAND_IN: &str = r#"#!/bin/sh
# A forge CLI that answers only the questions a test wrote down, each exactly once.
host=
prev=
for a in "$@"; do
  if [ "$prev" = --hostname ]; then host=$a; fi
  prev=$a
done
dir="$HOME/hosts/$host"
if [ -z "$host" ] || [ ! -d "$dir" ]; then
  echo "stand-in: no answers for host '$host'" >&2
  exit 97
fi
q="$dir/question.$$"
printf '%s\037' "${0##*/}" > "$q"
for a in "$@"; do printf '%s\037' "$a" >> "$q"; done
for want in "$dir"/*.args; do
  [ -f "$want" ] || continue
  if cmp -s "$q" "$want"; then
    rm -f "$q"
    base=${want%.args}
    if ! mkdir "$base.asked" 2>/dev/null; then
      echo "stand-in: asked twice: $want" >&2
      exit 98
    fi
    env > "$base.env"
    cat "$base.out"
    cat "$base.err" >&2
    exit "$(cat "$base.code")"
  fi
done
echo "stand-in: nobody wrote down the question in $q" >&2
exit 99
"#;

/// Run every test whose name contains `filter` in a child of this binary, with a stand-in
/// `gh` and `glab` first on its `PATH` in a directory called `bin_name`.
pub fn in_a_child(filter: &str, bin_name: &str) {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let scratch = std::fs::canonicalize(scratch.path())
        .map(|p| (scratch, p))
        .expect("a resolved scratch directory");
    let bin = scratch.1.join(bin_name);
    std::fs::create_dir_all(&bin).unwrap();
    stand_in::program(&bin, "gh", STAND_IN);
    stand_in::program(&bin, "glab", STAND_IN);
    let home = scratch.1.join("home");
    std::fs::create_dir_all(home.join("hosts")).unwrap();
    // **Each stand-in is run once here, with no deadline, before anything times it**
    // (charter-app#306). macOS checks a program file the first time it runs, and on a busy
    // machine that check queues: measured beside `cargo test -p charter-core --lib`, every
    // question that reached a new `gh` before its first run had finished waited ~30 s, and
    // the auth checks and best-effort calls, on `STATUS_TIMEOUT`'s 10 s, were cut off.
    // Every later call took 20–60 ms. With no `--hostname` a stand-in exits 97 and writes
    // nothing, so this run has no side effects.
    for cli in ["gh", "glab"] {
        let ran = charter_core::forklock::output(
            Command::new(bin.join(cli)).env_clear().env("HOME", &home),
        )
        .unwrap_or_else(|e| panic!("the stand-in {cli} runs: {e}"));
        assert_eq!(ran.status.code(), Some(97), "the stand-in {cli}: {ran:?}");
    }

    let out = charter_core::forklock::output(
        Command::new(std::env::current_exe().expect("the test binary"))
            .args([filter, "--nocapture"])
            .env(CHILD, "1")
            .env(BIN, &bin)
            .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
            .env("HOME", &home)
            .env("GH_TOKEN", "tok-under-test")
            .env("CHARTER_TEST_NOT_A_CREDENTIAL", "1"),
    )
    .expect("the test binary runs again");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{stdout}\n{stderr}");
    assert!(
        !stdout.contains("running 0 tests"),
        "the filter {filter} matched nothing: {stdout}"
    );
}

/// One test's host, and the questions written down for it.
pub struct Scene {
    pub host: String,
    pub dir: PathBuf,
    written: std::cell::Cell<usize>,
}

impl Scene {
    pub fn new(host: &str) -> Scene {
        let home = PathBuf::from(std::env::var_os("HOME").expect("HOME"));
        let dir = home.join("hosts").join(host);
        std::fs::create_dir_all(&dir).unwrap();
        Scene {
            host: host.to_string(),
            dir,
            written: std::cell::Cell::new(0),
        }
    }

    /// Write down that `cli args…` is answered with `code`, `out` and `err`.
    pub fn answers(&self, cli: &str, args: &[&str], code: i32, out: &str, err: &str) -> PathBuf {
        let n = self.written.get();
        self.written.set(n + 1);
        let base = self.dir.join(format!("q{n}"));
        let mut asked = format!("{cli}\x1f");
        for arg in args {
            asked.push_str(arg);
            asked.push('\x1f');
        }
        std::fs::write(base.with_extension("args"), asked).unwrap();
        std::fs::write(base.with_extension("code"), code.to_string()).unwrap();
        std::fs::write(base.with_extension("out"), out).unwrap();
        std::fs::write(base.with_extension("err"), err).unwrap();
        base
    }

    /// `gh api --hostname <host> <path>`, answered.
    pub fn gh_api(&self, path: &str, code: i32, out: &str, err: &str) -> PathBuf {
        self.answers(
            "gh",
            &["api", "--hostname", &self.host, path],
            code,
            out,
            err,
        )
    }

    /// `glab --hostname <host> api <path>`, answered.
    pub fn glab_api(&self, path: &str, code: i32, out: &str, err: &str) -> PathBuf {
        self.answers(
            "glab",
            &["--hostname", &self.host, "api", path],
            code,
            out,
            err,
        )
    }

    pub fn forge(&self, kind: &str) -> charter_core::forge::Forge {
        charter_core::forge::Forge::build(kind, Some(&self.host)).expect("a forge")
    }
}

/// Whether a question written down was asked.
pub fn was_asked(base: &Path) -> bool {
    base.with_extension("asked").is_dir()
}

pub fn in_child() -> bool {
    std::env::var_os(CHILD).is_some()
}
