//! `charter news` and the adoption half of `charter update`, through the binary.
//!
//! The rendering of an entry is compared against the Python charter by
//! `tests/differential/run.py`, one scenario per version, and this file does not repeat that.
//! What it covers is the two things that harness cannot:
//!
//! * **`--pending`**, whose Python answer is a function of the MACHINE — the five probes the
//!   corpus ships run `persona lint` and `frame-probe`, and whether those exit 0 depends on the
//!   runner's tmux and on what lint makes of a fixture plane. A differential scenario would be
//!   asserting something about the runner. Here the answer is fixed: this binary has neither
//!   command, so every probe is unchecked and the report says so instead of ticking — and
//!   WHICH of charter's three "no answer" sentences each one gets is pinned, because that is
//!   the part a port can get subtly wrong without anything else noticing.
//! * **`update`**, which in Python reaches PyPI and runs `uv tool install`. It cannot be run in
//!   a test harness at all — charter's own suite stubs its installer — and the half of it that
//!   IS ported is `charter news --since`, which the differential suite does cover.

use std::path::Path;
use std::process::{Command, Output};

fn charter(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_charter"))
        .args(args)
        .current_dir(root)
        .env("CHARTER_ROOT", root)
        // `update` reads the machine store for the update channel (charter ADR 0042), so the
        // store is pinned too, inside the run's own temp tree. Unpinned, a
        // fenced build dies rather than read the operator's `~/.config` (charter-app#129).
        .env("CHARTER_CONFIG_HOME", config_home(root))
        .env("NO_COLOR", "1")
        .env("TERM", "dumb")
        .output()
        .expect("the binary runs")
}

/// The config home a test's `charter` keeps its machine store in: inside the plane's own temp
/// directory, so it goes when the plane does, and under a name that is not `.charter`, so a test
/// asserting the plane gained no state is not answered by the store.
fn config_home(root: &Path) -> std::path::PathBuf {
    root.join("machine-config-home")
}

fn plane() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("charter.toml"), "").unwrap();
    // Made here because the fence resolves the path it is handed, and a directory that does
    // not exist yet resolves nowhere — so an absent config home reads as outside the fence.
    std::fs::create_dir_all(config_home(dir.path())).unwrap();
    dir
}

fn out(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn err(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

#[test]
fn a_probe_this_charter_cannot_run_is_unchecked_and_never_ticked() {
    // ADR 0013, in the one place it costs something to honour: the absence of information is
    // not evidence of health. Every entry shipping a `check:` names a command this binary does
    // not have, so nothing can be reported adopted — and the ✓ line, which claims every probe
    // reported adopted, must not be printed under the warnings that contradict it.
    let dir = plane();
    let said = charter(dir.path(), &["news", "--pending"]);
    assert!(said.status.success(), "{said:?}");
    assert_eq!(
        out(&said),
        "",
        "nothing can be pending when nothing was checked"
    );
    assert!(
        !err(&said).contains("nothing pending — every entry with a probe reports adopted"),
        "the green tick was printed over unchecked probes:\n{}",
        err(&said)
    );
    assert!(
        err(&said).contains("could not be checked — which is not the same as nothing to adopt"),
        "{}",
        err(&said)
    );
    // And each one says which command it was, so the reader can see it is this CLI's gap.
    //
    // **The two sentences are different, and the difference is charter's own rule meeting a
    // state charter's CLI never reaches.** `news::dispatch` tells apart a first token this
    // charter does not register (`_NOT_RUN` — "did not run here", a fact about this machine)
    // from a command path a `check:` may not name (`_UNLISTED` — a defect in the entry).
    // `frame-probe` is the first kind. `persona lint` is neither, quite: this binary HAS
    // `persona` and has no `lint` under it, so `command_path` stops at `("persona",)`, which is
    // not on `PROBEABLE`, and the entry gets the sentence written for an entry that named
    // something silly. Python's `_command_path` does exactly the same thing — it is only that
    // charter's own CLI has `persona lint`, so it never lands there.
    //
    // Pinned as it is rather than given a fourth sentence of charter-app's own: a reason that
    // exists in one charter and not the other is a fork, and the differential suite cannot see
    // this path at all. Reported in the PR instead.
    assert!(
        err(&said).contains("`charter frame-probe` did not run here"),
        "a first token this CLI does not register is news about this machine:\n{}",
        err(&said)
    );
    assert!(
        err(&said).contains("`charter persona lint` is not a command a `check:` may name"),
        "a path this CLI does not register falls to the unlisted sentence:\n{}",
        err(&said)
    );
    // Whichever sentence each one got, the slug names the entry the reader has to look at.
    for slug in [
        "delegate-when",
        "lint-sees-project-skills",
        "persona-bin",
        "charter-runs-the-harness",
        "the-charter-a-sub-agent-reads-cannot-drift-from-the-persona",
    ] {
        assert!(
            err(&said).contains(&format!("! {slug}: ")),
            "{slug} was not named:\n{}",
            err(&said)
        );
    }
}

#[test]
fn outside_a_plane_the_probes_are_not_run_against_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let said = Command::new(env!("CARGO_BIN_EXE_charter"))
        .args(["news", "--pending"])
        .current_dir(dir.path())
        .env_remove("CHARTER_ROOT")
        .env("NO_COLOR", "1")
        .output()
        .expect("the binary runs");
    assert!(said.status.success());
    assert!(
        String::from_utf8_lossy(&said.stderr).contains("no control plane here"),
        "{said:?}"
    );
}

#[test]
fn the_release_gate_refuses_before_it_prints_and_prints_nothing_when_it_does() {
    let dir = plane();
    // 0.56.0 quotes six headlines. The gate exists because that release published them.
    let refused = charter(dir.path(), &["news", "--for", "0.56.0"]);
    assert_eq!(refused.status.code(), Some(1));
    assert_eq!(out(&refused), "", "a refused body must not also be printed");
    assert!(err(&refused).contains("quotes a value charter does not unquote"));

    let missing = charter(dir.path(), &["news", "--for", "9.9.9"]);
    assert_eq!(missing.status.code(), Some(1));
    assert_eq!(out(&missing), "");
    assert!(err(&missing).contains("no news entry for 9.9.9."));
}

#[test]
fn a_release_body_goes_to_stdout_so_a_workflow_can_redirect_it() {
    let dir = plane();
    let body = charter(dir.path(), &["news", "--for", "0.44.1"]);
    assert!(body.status.success(), "{body:?}");
    assert_eq!(
        err(&body),
        "",
        "the body is stdout's and nothing else is said"
    );
    assert!(out(&body).starts_with("### "));
    assert!(
        out(&body).ends_with('\n'),
        "`print` ends the body with a newline"
    );
}

#[test]
fn the_range_defaults_to_the_newest_version_this_build_ships() {
    let dir = plane();
    // Not `--version`'s number: this binary carries the workspace's, which is below every entry
    // in the corpus, and defaulting to it would answer "nothing new" forever.
    let ranged = charter(dir.path(), &["news", "--since", "0.62.0"]);
    assert!(ranged.status.success(), "{ranged:?}");
    assert!(ranged.stdout.starts_with(b"0.62.1  "), "{}", out(&ranged));
}

#[test]
fn update_says_which_half_it_does_and_installs_nothing() {
    let dir = plane();
    let said = charter(dir.path(), &["update"]);
    assert!(said.status.success(), "{said:?}");
    assert!(
        err(&said).contains("does not install anything here"),
        "{}",
        err(&said)
    );
    assert!(
        err(&said).contains("no update baseline recorded"),
        "{}",
        err(&said)
    );
    // Nothing was created: this command reads.
    assert!(!dir.path().join(".charter").exists(), "update wrote state");

    // And a baseline a real update left behind becomes the range.
    let baseline = charter_core::adopt::baseline_file(dir.path());
    std::fs::create_dir_all(baseline.parent().unwrap()).unwrap();
    std::fs::write(&baseline, "0.62.0\n").unwrap();
    let ranged = charter(dir.path(), &["update"]);
    assert!(ranged.status.success(), "{ranged:?}");
    assert!(out(&ranged).contains("0.62.1"), "{}", out(&ranged));
}

#[test]
fn the_flags_that_move_a_python_package_are_answered_rather_than_rejected() {
    // An agent that typed `--bump` is owed the reason. clap refusing it as an unknown flag
    // would exit 2, which is the code a harness hook reads as "block".
    let dir = plane();
    let said = charter(dir.path(), &["update", "--to", "0.63.0", "--bump"]);
    assert!(said.status.success(), "{said:?}");
    assert!(
        err(&said).contains("--to names a published version"),
        "{}",
        err(&said)
    );
    assert!(
        err(&said).contains("the pin was left alone"),
        "{}",
        err(&said)
    );
}

#[test]
fn update_names_the_channel_and_moves_it_only_to_a_channel_that_exists() {
    let dir = plane();
    let home = config_home(dir.path());
    let said = charter(dir.path(), &["update"]);
    assert!(
        err(&said).contains("updates from the stable channel"),
        "{}",
        err(&said)
    );

    let moved = charter(dir.path(), &["update", "--channel", "dev"]);
    assert!(moved.status.success(), "{moved:?}");
    assert_eq!(
        charter_core::machine::read(&home).store.channel,
        charter_core::updates::Channel::Dev
    );
    let said = charter(dir.path(), &["update"]);
    assert!(
        err(&said).contains("updates from the dev channel"),
        "{}",
        err(&said)
    );

    // A typo exits 1 and leaves the channel where it was, rather than guessing.
    let refused = charter(dir.path(), &["update", "--channel", "nightly"]);
    assert_eq!(refused.status.code(), Some(1), "{refused:?}");
    assert_eq!(
        charter_core::machine::read(&home).store.channel,
        charter_core::updates::Channel::Dev
    );
}
