//! A chat starts on a profile with nothing installed anywhere, and the harness is asked
//! nothing on the way.
//!
//! The app arms every chat it starts with its own plugin, for that session alone
//! (`charter_core::plugin`), so there is no config folder to probe and no marketplace to clone.
//! This used to be `a_chat_never_starts_on_an_unguarded_profile`: a stand-in `claude` answered
//! `plugin list --json`, and a folder without the Python charter's plugin refused the chat.
//! What survives of that gate is what a launch still needs — the kind, the file, the approval —
//! and each of those is proven here by a stand-in that writes down every time it is run.

use std::fs;
use std::path::{Path, PathBuf};

use charter_core::profiles::{self, Profile};
use charter_core::wiring::{self, State};

/// The program the declared profile points at.
const STAND_IN: &str = "claude-stand-in";

/// A plane declaring one profile whose command is a stand-in that writes down every run.
///
/// What the stand-in was given is written to a file whose path is baked into the script: an
/// environment variable would need `set_var`, and `unsafe` is forbidden here.
struct Stand {
    dir: tempfile::TempDir,
}

impl Stand {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("charter.toml"), "").unwrap();
        let stand = Self { dir };
        // Through `stand_in::program`: a program this process wrote through its own
        // descriptor can lose to `ETXTBSY` (charter-app#81).
        stand_in::program(
            stand.root(),
            STAND_IN,
            &format!(
                "#!/bin/sh\nprintf '%s\\n' \"$*\" >> {:?}\n",
                stand.asked().display().to_string()
            ),
        );
        stand.declares("claude", &stand.bin().display().to_string());
        stand
    }

    fn declares(&self, kind: &str, command: &str) {
        fs::write(
            self.root().join(profiles::LOCAL_FILE),
            format!("[harness.work]\nkind = {kind:?}\ncommand = [{command:?}]\n"),
        )
        .unwrap();
    }

    fn bin(&self) -> PathBuf {
        self.root().join(STAND_IN)
    }

    fn asked(&self) -> PathBuf {
        self.root().join("asked.txt")
    }

    fn root(&self) -> &Path {
        self.dir.path()
    }

    fn declared(&self) -> Profile {
        profiles::current(self.root()).get("work").unwrap().clone()
    }

    /// The profile, already approved — the trust gate is not what most of these are about.
    fn approved(&self) -> Profile {
        let p = self.declared();
        charter_core::profiletrust::record_launched(
            self.root(),
            &p.name,
            &charter_core::profiletrust::fingerprint(&p),
        )
        .unwrap();
        p
    }

    /// Whether the harness was run at all.
    fn was_run(&self) -> bool {
        self.asked().exists()
    }
}

#[test]
fn an_approved_profile_may_start_and_its_harness_is_asked_nothing() {
    charter_core::unsteered!();
    // The whole change: no `plugin list`, no `marketplace add`, no `plugin install`. The app
    // arms the chat itself, so the launch gate runs nothing of the profile's.
    let stand = Stand::new();
    let p = stand.approved();

    assert_eq!(wiring::refusal(&p, stand.root()), None);
    let w = wiring::detect(&p, stand.root());
    assert_eq!(w.state, State::Wired, "{w:?}");
    assert!(
        !stand.was_run(),
        "the harness was run to answer a question nobody asks any more"
    );
}

#[test]
fn a_profile_charter_may_not_run_a_command_for_is_refused_and_never_run() {
    charter_core::unsteered!();
    let stand = Stand::new();
    let p = stand.declared();

    let why = wiring::refusal(&p, stand.root()).expect("an unapproved profile is refused");

    assert!(why.contains("profile 'work' is new"), "{why}");
    assert!(!stand.was_run());
    assert_eq!(wiring::detect(&p, stand.root()).state, State::Unknown);
}

#[test]
fn a_kind_this_app_does_not_start_is_refused_by_that_name() {
    charter_core::unsteered!();
    // Parsing is not launching. `profiles` knows all three kinds, including opencode, because
    // the Python charter accepts one and an operator must not get two answers about their own
    // file. What this app will not do is START one (spec decision 6).
    let stand = Stand::new();
    stand.declares("opencode", "opencode");
    let p = stand.approved();

    assert_eq!(
        wiring::refusal(&p, stand.root()).as_deref(),
        Some(
            "profile 'work' runs opencode, which this app does not start — charter-app v1 \
             starts Claude Code and Codex, and opencode follows."
        )
    );
}

#[test]
fn the_kind_refusal_comes_before_every_gate_that_would_ask_anything() {
    charter_core::unsteered!();
    // A profile that can never start is not worth approving, and the sentence an operator
    // wants is the one about v1 rather than one about consent.
    let stand = Stand::new();
    stand.declares("opencode", "opencode");
    // Deliberately NOT approved, which is the gate that would otherwise answer first.
    let p = stand.declared();

    let why = wiring::refusal(&p, stand.root()).expect("refused");
    assert!(why.contains("which this app does not start"), "{why}");
}

#[test]
fn both_kinds_this_app_does_start_are_taken_by_their_declared_word() {
    charter_core::unsteered!();
    for kind in ["claude", "codex"] {
        assert!(
            charter_core::harness::Harness::of_kind(kind).is_some(),
            "{kind} is a v1 harness and was not recognised by its declared word"
        );
    }
    assert_eq!(charter_core::harness::Harness::of_kind("opencode"), None);
}

#[test]
fn detect_says_a_kind_is_not_startable_rather_than_that_it_could_not_look() {
    charter_core::unsteered!();
    // `doctor` reaches `detect` directly, so the v1 decision has to be its sentence too.
    let stand = Stand::new();
    stand.declares("opencode", "opencode");

    let w = wiring::detect(&stand.approved(), stand.root());

    assert_eq!(
        w.state,
        State::Unknown,
        "a kind this app cannot start is not a pass"
    );
    assert!(w.detail.contains("which this app does not start"), "{w:?}");
}

#[test]
fn a_codex_profile_needs_nothing_in_its_home_either() {
    charter_core::unsteered!();
    // Codex used to need the Python charter's Codex plugin, a policy line and a trusted guard
    // hook in `$CODEX_HOME/config.toml`. The app arms a Codex chat with `-c` flags now, so an
    // empty home is a home a chat can start from.
    let stand = Stand::new();
    stand.declares("codex", &stand.bin().display().to_string());
    let p = stand.approved();

    assert_eq!(wiring::refusal(&p, stand.root()), None);
    let w = wiring::detect(&p, stand.root());
    assert_eq!(w.state, State::Wired, "{w:?}");
    assert!(w.detail.contains("trust"), "{w:?}");
    assert!(!stand.was_run());
}
