//! A profile names a program; charter has to find it, and to say where it looked when it
//! cannot.
//!
//! **charter-app#134.** A built-in profile's command is a bare word out of the registry, and
//! a bare word is resolved against a `PATH` — which for a Finder-launched `.app` is
//! `/usr/bin:/bin:/usr/sbin:/sbin` and holds no harness at all. `crates/charter-cli/tests/
//! a_finder_launch_finds_the_harness.rs` is the launch that proves the search works, because
//! only a child process can be given a `PATH` and a `HOME` of the test's choosing. What is
//! here is the half that needs no environment: what charter SAYS when the search comes back
//! empty, and what it deliberately does not search for.

use std::fs;
use std::path::{Path, PathBuf};

use charter_core::profiles::{self, Profile};
use charter_core::start::{self, Start};
use charter_core::wiring::{self, State};

/// A bare word no machine has, so this file's answer does not depend on the machine running
/// it. A `claude` here would pass on the developer's laptop and fail on a runner, or worse,
/// the other way round.
const NOWHERE: &str = "charter-app-134-no-such-harness";

struct Plane {
    dir: tempfile::TempDir,
}

impl Plane {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("charter.toml"), "").unwrap();
        Self { dir }
    }

    fn root(&self) -> &Path {
        self.dir.path()
    }

    /// Declares `work`, of kind `claude`, running `command` — and approves it, so what these
    /// tests read is the probe's answer and never the consent gate's.
    fn running(&self, command: &str) -> Profile {
        self.declare(&format!(
            "[harness.work]\nkind = \"claude\"\ncommand = [{command:?}]\n"
        ))
    }

    /// The same for a Codex profile, which is gated on a file rather than on a probe and so
    /// needs the `CODEX_HOME` holding its three marks.
    fn running_kind(&self, kind: &str, command: &str, codex_home: &Path) -> Profile {
        self.declare(&format!(
            "[harness.work]\nkind = {kind:?}\ncommand = [{command:?}]\n\
             env = {{ CODEX_HOME = {:?} }}\n",
            codex_home.display().to_string()
        ))
    }

    fn declare(&self, toml: &str) -> Profile {
        fs::write(self.root().join(profiles::LOCAL_FILE), toml).unwrap();
        let set = profiles::current(self.root());
        let p = set.get("work").expect("the profile was declared").clone();
        charter_core::profiletrust::record_launched(
            self.root(),
            "work",
            &charter_core::profiletrust::fingerprint(&p),
        )
        .unwrap();
        p
    }

    fn start(&self) -> Start {
        Start {
            profile: Some("work".to_owned()),
            persona: None,
            name: "ide.1".to_owned(),
            cwd: Some(self.root().to_path_buf()),
            resume: None,
            show_footer: false,
        }
    }
}

#[test]
fn a_harness_charter_cannot_find_is_an_unknown_that_names_every_directory_it_looked_in() {
    let plane = Plane::new();
    let p = plane.running(NOWHERE);

    let w = wiring::detect(&p, plane.root(), plane.root());

    // Unknown and NOT Unwired: charter never opened a config folder, so it knows nothing
    // about whether the guard is installed in one. An Unwired here would be installed over.
    assert_eq!(w.state, State::Unknown, "{w:?}");
    assert!(
        w.detail
            .contains(&format!("could not find a program called {NOWHERE}")),
        "{w:?}"
    );
    // The fact the refusal used to be missing: where charter looked. The system directories
    // are there whatever the machine's own `PATH` holds.
    for dir in charter_core::programs::SYSTEM_BIN {
        assert!(
            w.detail.contains(dir),
            "the search has to be in the message, and {dir} is not: {w:?}"
        );
    }
    let home = profiles::home().expect("a home");
    for rel in charter_core::programs::USER_BIN {
        assert!(
            w.detail.contains(&home.join(rel).display().to_string()),
            "the search has to be in the message, and ~/{rel} is not: {w:?}"
        );
    }
    // And never `charter harness install`, which runs the same missing binary.
    assert!(
        w.fix.contains("absolute command in charter.local.toml"),
        "{w:?}"
    );
    assert!(!w.fix.contains("charter harness install"), "{w:?}");
}

#[test]
fn a_chat_is_not_started_on_a_harness_charter_cannot_find_and_the_refusal_says_where_it_looked() {
    let plane = Plane::new();
    plane.running(NOWHERE);

    let refusal = start::ready(&plane.start(), plane.root()).expect_err("nothing starts");

    // For a Claude Code profile the WIRING gate answers first — a probe is a run of the
    // profile's command, so it meets the missing harness before the launch does. What matters
    // is that the sentence the operator reads carries the search, which is the whole of
    // charter-app#134's second half.
    assert!(
        refusal.contains(&format!("could not find a program called {NOWHERE}")),
        "{refusal}"
    );
    assert!(refusal.contains("/usr/bin"), "{refusal}");
    assert!(refusal.ends_with("nothing was started."), "{refusal}");
}

#[test]
fn a_codex_chat_whose_program_is_missing_is_refused_by_the_launch_and_not_by_a_probe() {
    // The one kind where the LAUNCH's own resolution is what answers. Codex's wiring check
    // reads `$CODEX_HOME/config.toml` and never spawns anything (a hook Codex has not trusted
    // is inert, so the marks are in a file), so a Codex profile can pass every gate and reach
    // the terminal with a program that is nowhere. Before charter-app#134 that was a pane that
    // opened onto nothing; now it is a refusal naming the search.
    let plane = Plane::new();
    let codex_home = plane.root().join("codex");
    let hooks = codex_home.join("plugins/cache/charter/charter/0.62.1/hooks");
    fs::create_dir_all(&hooks).unwrap();
    fs::write(
        hooks.join("hooks.json"),
        r#"{"hooks":{"PreToolUse":[{"hooks":[{"command":"charter hook pretooluse"}]}]}}"#,
    )
    .unwrap();
    fs::write(
        codex_home.join("config.toml"),
        "[plugins.\"charter@charter\"]\nenabled = true\n\n\
         [shell_environment_policy.set]\nCHARTER_HARNESS = \"codex\"\n\n\
         [hooks.state.\"charter@charter:hooks/hooks.json:pre_tool_use:0:0\"]\n\
         trusted_hash = \"written-by-a-test\"\n",
    )
    .unwrap();
    plane.running_kind("codex", NOWHERE, &codex_home);

    let refusal = start::ready(&plane.start(), plane.root()).expect_err("nothing starts");

    assert!(
        refusal.contains(&format!("could not find a program called {NOWHERE}")),
        "{refusal}"
    );
    assert!(refusal.contains("/usr/bin"), "{refusal}");
    assert!(refusal.ends_with("Nothing was started."), "{refusal}");
}

#[test]
fn a_command_the_operator_wrote_as_a_path_is_never_searched_for_and_fails_where_it_always_did() {
    // The guard on the other side of the search. A profile naming a place — `/nope/claude`,
    // `./claude`, `bin/claude` — is handed to the spawn exactly as declared: charter neither
    // hunts for its basename in `~/.local/bin` (which would start a DIFFERENT program than
    // the one the operator wrote down) nor rejects it here with a sentence about a search
    // that never happened.
    let plane = Plane::new();
    let missing = plane.root().join("nowhere").join("claude");
    let p = plane.running(&missing.display().to_string());

    let w = wiring::detect(&p, plane.root(), plane.root());

    assert_eq!(w.state, State::Unknown, "{w:?}");
    assert!(
        !w.detail.contains("could not find a program called"),
        "a declared path is not a search: {w:?}"
    );
    assert!(w.detail.contains("could not be read in"), "{w:?}");
    // And the program the spawn was given is the one that was written down, named in the
    // message so the operator can see which path charter actually tried.
    assert!(w.detail.contains(&missing.display().to_string()), "{w:?}");
}

#[test]
fn a_profile_that_names_an_absolute_program_starts_that_exact_program() {
    // Resolution must not canonicalise, rewrite or re-search a path that is already one: on
    // macOS a temp directory is `/var/folders/…` and its resolved spelling is
    // `/private/var/…`, and a launch that silently swapped them would start a program under a
    // path the record then could not match.
    let plane = Plane::new();
    let bin = stand_in::program(
        plane.root(),
        "claude-stand-in",
        "#!/bin/sh\n\
         cat <<'JSON'\n\
         [{\"id\":\"charter@charter\",\"scope\":\"user\",\"enabled\":true}]\n\
         JSON\n",
    );
    plane.running(&bin.display().to_string());

    let ready = start::ready(&plane.start(), plane.root()).expect("it starts");

    assert_eq!(ready.program, bin.display().to_string());
}

#[test]
fn a_harness_charter_cannot_find_is_one_failed_install_step_and_not_two_silent_ones() {
    // `install` runs the harness twice — a marketplace add, then a plugin install. Without
    // the same resolution the probe uses it would spawn a missing binary twice and report two
    // failures that each name a command line and no reason.
    let plane = Plane::new();
    let p = plane.running(NOWHERE);

    let steps = wiring::install(&p, plane.root());

    assert_eq!(steps.len(), 1, "{steps:?}");
    assert_eq!(steps[0].status, "failed", "{steps:?}");
    assert!(
        steps[0]
            .detail
            .contains(&format!("could not find a program called {NOWHERE}")),
        "{steps:?}"
    );
}

#[test]
fn every_caller_of_the_search_gets_the_same_answer_out_of_it() {
    // One search, asked three ways: `programs::find` (a harness), `programs::on_path` (the
    // doctor's listing) and `forge::find_cli` (the forge CLI that becomes a git credential
    // helper). Before charter-app#134 these were three separate walks that had already drifted
    // — `doctor::profiles::on_path` read `PATH` alone and called any file runnable off unix,
    // `forge::is_executable` answered `false` there, and `worktree::git` searched a fixed list
    // first — so the same machine gave three answers to one question.
    //
    // Real program names, so the check is an equivalence and not two `None`s agreeing. `sh` is
    // on every unix charter builds for; `NOWHERE` is on none.
    let dirs = charter_core::programs::search_dirs();
    for name in ["sh", "ls", "git", NOWHERE] {
        let found = charter_core::programs::find(name, &dirs);
        assert_eq!(
            charter_core::forge::find_cli(name),
            found,
            "find_cli disagreed with the search about {name}"
        );
        assert_eq!(
            charter_core::programs::on_path(name),
            found.is_some(),
            "on_path disagreed with the search about {name}"
        );
    }
    assert!(
        charter_core::programs::find("sh", &dirs).is_some(),
        "a search that finds nothing at all would make the equivalence above vacuous"
    );
    let nowhere: PathBuf = ["/", NOWHERE].iter().collect();
    assert!(
        !dirs.contains(&nowhere),
        "the fixed list is a constant, not something a machine adds to"
    );
}
