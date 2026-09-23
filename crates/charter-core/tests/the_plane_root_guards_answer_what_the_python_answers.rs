//! Every command line A3's and A3b's docstrings name as a bypass that shipped — and every defect
//! filed from their port — answered here the way the frozen Python answers it, with no Python
//! present.
//!
//! The answers are not written by hand. `tests/differential/planeroot.py --record` ran each case
//! through the Python charter pinned at the fixture planes' commit, against a git-repository
//! fixture it built from `fixtures/corpora/planeroot-fixture.json`; `--check` fails if either file
//! stops being what the oracle says. This builds the SAME fixture from the same steps and replays
//! the recording through the same code the differential's Rust side runs
//! (`examples/planeroot_oracle.rs`, included below), so the ordinary `cargo test` job holds the
//! line and the differential job, which fuzzes against the live oracle, is the wider net.
//!
//! # Why the replay runs in a child process
//!
//! Four of the answers ask git, and git reads its global config from `HOME`, `XDG_CONFIG_HOME`
//! and `GIT_CONFIG_COUNT` — which the recording set to the fixture's, and which carry three of the
//! aliases the corpus depends on. This crate forbids `unsafe`, and setting a variable in a running
//! test process is `unsafe` in Rust 2024 (and racy besides), so the test re-executes its own
//! binary for the one test, with the environment the harness gave both sides, and asserts on the
//! child's verdict.

mod oracle_corpus;
mod planeroot_answer;

use std::path::{Path, PathBuf};

use serde_json::Value;

/// Set in the child to the fixture's base directory.
const CHILD: &str = "PLANEROOT_REPLAY_BASE";

/// The token a recorded path carries where the fixture's directory was.
const B: &str = "@B@";

fn repo_file(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(rel)
}

fn sub(s: &str, base: &str) -> String {
    s.replace(B, base)
}

/// `run_steps` in the harness, step for step.
fn build(base: &Path) {
    let text = std::fs::read_to_string(repo_file("fixtures/corpora/planeroot-fixture.json"))
        .expect("the fixture's steps are checked in");
    let steps: Vec<Value> = serde_json::from_str(&text).expect("the steps are JSON");
    let b = base.to_str().expect("a UTF-8 temporary directory");
    let at = |rel: &str| base.join(rel);
    for s in &steps {
        if let Some(cwd) = s["git"].as_str() {
            let args: Vec<String> = s["args"]
                .as_array()
                .expect("args")
                .iter()
                .map(|a| sub(a.as_str().expect("an argument"), b))
                .collect();
            let argv: Vec<&str> = args.iter().map(String::as_str).collect();
            let run = charter_core::worktree::git::run(
                &at(cwd),
                &argv,
                charter_core::worktree::git::READ,
            )
            .expect("git runs");
            assert!(run.ok(), "fixture step {s} failed: {}", run.err);
        } else if let Some(rel) = s["mkdir"].as_str() {
            std::fs::create_dir_all(at(rel)).unwrap();
        } else if let Some(rel) = s["write"].as_str() {
            std::fs::create_dir_all(at(rel).parent().unwrap()).unwrap();
            std::fs::write(at(rel), sub(s["text"].as_str().unwrap(), b)).unwrap();
        } else if let Some(rel) = s["append_hex"].as_str() {
            use std::io::Write;
            let hex = s["hex"].as_str().unwrap();
            let bytes: Vec<u8> = (0..hex.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
                .collect();
            let mut f = std::fs::OpenOptions::new()
                .append(true)
                .open(at(rel))
                .unwrap();
            f.write_all(&bytes).unwrap();
        } else if let Some(rel) = s["symlink"].as_str() {
            #[cfg(unix)]
            std::os::unix::fs::symlink(sub(s["target"].as_str().unwrap(), b), at(rel)).unwrap();
            #[cfg(not(unix))]
            let _ = rel;
        } else {
            panic!("a fixture step nothing runs: {s}");
        }
    }
}

/// The harness's `normalise`: the base directory as `@B@`, a config probe's scratch directory as
/// `@CFG@`.
fn normalise(v: &Value, base: &str) -> Value {
    match v {
        Value::String(s) => {
            let s = s.replace(base, B);
            let re = regex::Regex::new(r"@B@/cfg(?:py|rs-\d+)").unwrap();
            Value::String(re.replace_all(&s, "@CFG@").into_owned())
        }
        Value::Array(a) => Value::Array(a.iter().map(|x| normalise(x, base)).collect()),
        Value::Object(o) => Value::Object(
            o.iter()
                .map(|(k, x)| (k.clone(), normalise(x, base)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// A recorded request with the fixture's directory put back.
fn request(v: &Value, base: &str) -> Value {
    match v {
        Value::String(s) => Value::String(sub(s, base)),
        Value::Array(a) => Value::Array(a.iter().map(|x| request(x, base)).collect()),
        Value::Object(o) => Value::Object(
            o.iter()
                .map(|(k, x)| (k.clone(), request(x, base)))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn replay(base: &str) -> Vec<String> {
    // The curated rows, then the frozen subset of the seeded fuzz: one replay, one comparison.
    let mut rows = oracle_corpus::jsonl("planeroot-oracle.jsonl");
    assert!(
        rows.len() > 150,
        "the corpus has {} rows — it was cut, not answered",
        rows.len()
    );
    let generated = oracle_corpus::jsonl_gz("planeroot-generated.jsonl.gz");
    assert!(
        generated.len() > 1500,
        "the frozen fuzz subset has {} rows — it was cut, not answered",
        generated.len()
    );
    rows.extend(generated);
    // Every sentence the two guards can refuse with is only evidence while the corpus really holds
    // a refusal that says it — asserted by name, so an edit to the corpus cannot quietly take one
    // away and leave this replaying nothing but allows.
    for (key, said) in [
        ("bra", "would switch to '"),
        ("bra", "would create '"),
        ("bra", "would create a branch in"),
        ("bra", "would detach HEAD at '"),
        ("bra", "would detach HEAD in"),
        ("bra", "cannot tell what `git checkout"),
        ("bra", "would move HEAD in the PLANE ROOT: '"),
        ("bra", "cannot read this `git"),
        ("bra", "(A restore needs no options here"),
        ("bra", "`git checkout main` — putting the root back"),
        ("bra", "`git checkout master` — putting the root back"),
        ("bra", "`git checkout trunk` — putting the root back"),
        (
            "bra",
            "Putting the root back on its default branch is always allowed.",
        ),
        (
            "rst",
            "would delete 1 commit from the PLANE ROOT that is not on origin/main",
        ),
        (
            "rst",
            "would delete 2 commits from the PLANE ROOT that are not on origin/main",
        ),
    ] {
        let n = rows
            .iter()
            .filter(|r| {
                r.row["answer"][key]
                    .as_str()
                    .is_some_and(|s| s.contains(said))
            })
            .count();
        assert!(n > 0, "no recorded `{key}` refusal says {said:?}");
    }
    // On threads, as the example runs them: each row asks git up to a few dozen times, and each
    // thread has a scratch directory of its own for the config probes.
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get().clamp(1, 16));
    let chunk = rows.len().div_ceil(threads);
    std::thread::scope(|s| {
        let handles: Vec<_> = rows
            .chunks(chunk)
            .enumerate()
            .map(|(t, part)| {
                let scratch = Path::new(base).join(format!("cfgrs-{t}"));
                s.spawn(move || {
                    let mut wrong = Vec::new();
                    for row in part {
                        check(row, base, &scratch, &mut wrong);
                    }
                    wrong
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().expect("a replay thread finished"))
            .collect()
    })
}

fn check(recorded: &oracle_corpus::Row, base: &str, scratch: &Path, wrong: &mut Vec<String>) {
    let (at, row) = (&recorded.at, &recorded.row);
    let got = normalise(
        &planeroot_answer::answer(&request(&row["request"], base), scratch),
        base,
    );
    let want = &row["answer"];
    for (key, value) in want.as_object().expect("an answer object") {
        if got.get(key) != Some(value) {
            wrong.push(format!(
                "{at} {:?} [{key}]\n    python: {value}\n    rust:   {}",
                row["case"]["cmd"],
                got.get(key).unwrap_or(&Value::Null)
            ));
        }
    }
    for key in got.as_object().expect("an answer object").keys() {
        if want.get(key).is_none() {
            wrong.push(format!(
                "{at} {:?}: the Rust answered `{key}` and the recording has no such key",
                row["case"]["cmd"]
            ));
        }
    }
}

#[test]
fn the_recorded_plane_root_cases_answer_what_the_python_answered() {
    if let Ok(base) = std::env::var(CHILD) {
        let wrong = replay(&base);
        assert!(
            wrong.is_empty(),
            "{} recorded answers differ:\n{}",
            wrong.len(),
            wrong.join("\n")
        );
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let base = charter_core::pypath::realpath(dir.path().to_str().unwrap());
    build(Path::new(&base));
    let mut cmd = std::process::Command::new(std::env::current_exe().unwrap());
    cmd.args([
        "--exact",
        "the_recorded_plane_root_cases_answer_what_the_python_answered",
        "--nocapture",
        "--test-threads=1",
    ])
    .current_dir(&base)
    .env(CHILD, &base)
    .env("HOME", format!("{base}/home"))
    .env("XDG_CONFIG_HOME", format!("{base}/xdg"))
    .env("GIT_CONFIG_NOSYSTEM", "1")
    .env("GIT_CONFIG_COUNT", "1")
    .env("GIT_CONFIG_KEY_0", "alias.envco")
    .env("GIT_CONFIG_VALUE_0", "checkout")
    .env_remove("GIT_CONFIG_GLOBAL")
    .env_remove("GIT_DIR")
    .env_remove("GIT_WORK_TREE")
    .stdout(std::process::Stdio::piped())
    .stderr(std::process::Stdio::piped());
    let out = charter_core::forklock::spawn(&mut cmd)
        .unwrap()
        .wait_with_output()
        .unwrap();
    assert!(
        out.status.success(),
        "the replay failed:\n{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    // A child that ran nothing also exits 0; the filter has to have matched this very test.
    let said = String::from_utf8_lossy(&out.stdout);
    assert!(
        said.contains("1 passed"),
        "the child ran no replay:\n{said}"
    );
}
