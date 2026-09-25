//! **Recorded behaviour**: every scenario the Python differential ran, replayed against this
//! build's `charter` and compared with what was recorded (ADR 0046).
//!
//! Until 2026-09-23 each of these ran twice, once through the Python charter pinned at
//! `50d31dc` and once through the Rust binary, and the two had to agree
//! (`tests/differential/run.py`, `doctor_scenarios.py`). The operator ruled that charter-app
//! stands alone, so the Python side was run ONE last time and its answers frozen into
//! `tests/fixtures/recorded/behaviour.jsonl`: each row is one scenario — the fixture plane it
//! starts from, what its setup changed (`start`), the command, its environment and stdin, and
//! what the run must leave (`expect`). A row was only written for a scenario the original
//! differential passed on the same head, so where the two implementations were compared the
//! recorded text IS Python's answer, and where they differed by decision (a `Divergence`, a
//! rewrite, a stream the differential did not compare) it is the app's, with a `notes` line
//! saying which.
//!
//! What each scenario checks is what the differential checked of the Rust side, rule for rule:
//!
//! - the exit status;
//! - stdout and stderr, byte for byte after the scenario's own masks — or up to a declared cut,
//!   or, for a refusal whose words were never compared, that it says what it refuses with;
//! - a hook's verdict (`denies`, `allows`, `says`), a status line's alert rows, and strings
//!   neither stream may print (`never_says`);
//! - every file, link and directory the plane holds afterwards, bytes and file modes included,
//!   except the paths the scenario ignores (a `.git`'s index and reflogs), which it asks git
//!   about instead (`facts`);
//! - that nothing was written beside the plane, except where the command's work lands (a push
//!   to a stand-in forge), and that it did land there.
//!
//! **Run it**: `cargo test -p charter-cli --test recorded_behaviour`, with scenario names (or any
//! part of one) after `--` to run only those. **Change a recorded answer on purpose**:
//! `CHARTER_RECORDED_BLESS=1 cargo test -p charter-cli --test recorded_behaviour -- <names>`
//! rewrites the named rows from what the binary does now; the diff of the fixture is then the
//! change, and the PR says why (ADR 0046).

// The recording is of unix runs — modes, links, fifos, a unix socket — and so is the replay.
// Windows has no charter port yet (ADR 0031); its CI job is evidence, and a replay that cannot
// compile there would only add noise to the list it gathers.
#[cfg(unix)]
mod facts;
#[cfg(unix)]
mod fixture;
#[cfg(unix)]
mod replay;
#[cfg(unix)]
mod serve;
#[cfg(unix)]
mod text;
#[cfg(unix)]
mod tree;

fn main() {
    #[cfg(unix)]
    text::only_the_apps_own_version_becomes_the_token();
    #[cfg(unix)]
    replay::main();
    #[cfg(not(unix))]
    println!("recorded behaviour: the recording is of unix runs, and this is not one");
}
