//! What it means to start a chat on a harness profile, before any window is involved.
//!
//! One home for it, in the core, because four callers reach it — the picker, a relaunch's
//! reopen, a handoff, and the CLI — and a gate each of them has to remember is a gate one of
//! them will not.
//!
//! The record stores the profile's NAME and the launch re-reads it, which is ADR 0022's
//! rule: a reopened chat whose profile is gone is skipped by name, never given another,
//! because another profile may be another account where that chat's resume id does not exist
//! and where its workspace's code was never meant to go.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use charter_core::harness::{Harness, SessionId};
use charter_core::profiles;
use charter_core::start::{self, Start};

/// A plane with a wired stand-in `claude`, so the wiring gate passes and these tests are
/// about starting rather than about wiring.
struct Plane {
    dir: tempfile::TempDir,
}

impl Plane {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("charter.toml"), "").unwrap();
        fs::create_dir_all(dir.path().join("personas/steward")).unwrap();
        fs::write(
            dir.path().join("personas/steward/persona.md"),
            "# steward\n",
        )
        .unwrap();
        let plane = Self { dir };
        plane.stand_in("[{\"id\":\"charter@charter\",\"scope\":\"user\",\"enabled\":true}]");
        plane
    }

    /// A stand-in whose folder reads as WIRED, which is what every test that expects a chat
    /// to start needs — the wiring gate is `wiring.rs`'s subject, not this file's.
    fn wired(&self) -> PathBuf {
        self.stand_in("[{\"id\":\"charter@charter\",\"scope\":\"user\",\"enabled\":true}]")
    }

    /// The program a profile points at: a stand-in `claude` answering the wiring probe.
    fn stand_in(&self, answer: &str) -> PathBuf {
        let bin = self.root().join("claude-stand-in");
        fs::write(&bin, format!("#!/bin/sh\ncat <<'JSON'\n{answer}\nJSON\n")).unwrap();
        fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();
        bin
    }

    fn declares(&self, toml: &str) -> &Self {
        fs::write(self.root().join(profiles::LOCAL_FILE), toml).unwrap();
        self
    }

    /// Declares one profile named `work`, of `kind`, running `program`.
    fn profile(&self, kind: &str, program: &Path, env: &str) -> &Self {
        self.declares(&format!(
            "[harness.work]\nkind = {kind:?}\ncommand = [{:?}]\n{env}",
            program.display().to_string()
        ));
        self.approve("work");
        self
    }

    fn approve(&self, name: &str) {
        let set = profiles::current(self.root());
        if let Some(p) = set.get(name) {
            charter_core::profiletrust::record_launched(
                self.root(),
                name,
                &charter_core::profiletrust::fingerprint(p),
            )
            .unwrap();
        }
    }

    fn root(&self) -> &Path {
        self.dir.path()
    }

    fn start(&self, name: &str) -> Start {
        Start {
            profile: Some(name.to_owned()),
            persona: None,
            name: "ide.7".to_owned(),
            cwd: Some(self.root().to_path_buf()),
            resume: None,
        }
    }
}

#[test]
fn a_chat_started_on_a_profile_runs_that_profiles_command_in_that_profiles_environment() {
    let plane = Plane::new();
    let bin = plane.wired();
    plane.profile(
        "claude",
        &bin,
        "env = { CLAUDE_CONFIG_DIR = \"~/.claude-work\" }\n",
    );

    let ready = start::ready(&plane.start("work"), plane.root()).expect("it starts");

    assert_eq!(ready.program, bin.display().to_string());
    let home = std::env::var("HOME").unwrap();
    assert_eq!(
        ready.env.iter().find(|(n, _)| n == "CLAUDE_CONFIG_DIR"),
        Some(&(
            "CLAUDE_CONFIG_DIR".to_owned(),
            format!("{home}/.claude-work")
        )),
        "the `~` is expanded at the launch, where no shell is there to do it"
    );
}

#[test]
fn the_harness_a_chat_runs_comes_from_the_declared_kind_and_not_from_the_program_name() {
    // The whole reason this is not `Harness::of_command`: a profile's command is commonly a
    // WRAPPER — ADR 0022 says so in as many words — and a wrapper's name is not `claude`.
    // Inferring from it hands the board `None`, which is the narrowest rule there is, and
    // the chat silently loses the session id that makes it resumable at all.
    let plane = Plane::new();
    let wrapper = plane.root().join("claude-wrap");
    fs::write(&wrapper, "#!/bin/sh\nexec claude \"$@\"\n").unwrap();
    fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o755)).unwrap();
    plane.profile("claude", &wrapper, "");

    let ready = start::ready(&plane.start("work"), plane.root()).expect("it starts");

    assert_eq!(Harness::of_command("claude-wrap"), None, "the premise");
    assert_eq!(
        ready.harness,
        Some(Harness::ClaudeCode),
        "a wrapper profile lost its harness"
    );
    assert!(
        ready.args.iter().any(|a| a == "--session-id"),
        "a wrapper profile was started with no id to resume it by: {:?}",
        ready.args
    );
}

#[test]
fn a_codex_chat_is_started_plain_because_codex_names_no_flag_to_choose_an_id() {
    let plane = Plane::new();
    let bin = plane.wired();
    // Codex's wiring is a file read, so a home with all three marks makes it startable.
    let codex_home = plane.root().join("codex");
    fs::create_dir_all(codex_home.join("plugins/cache/charter/charter/0.42.0/hooks")).unwrap();
    fs::write(
        codex_home.join("plugins/cache/charter/charter/0.42.0/hooks/hooks.json"),
        r#"{"hooks":{"PreToolUse":[{"hooks":[{"command":"charter hook pretooluse"}]}]}}"#,
    )
    .unwrap();
    fs::write(
        codex_home.join("config.toml"),
        "[plugins.\"charter@charter\"]\nenabled = true\n\
         [shell_environment_policy.set]\nCHARTER_HARNESS = \"codex\"\n\
         [hooks.state.\"charter@charter:hooks/hooks.json:pre_tool_use:0:0\"]\n\
         trusted_hash = \"abc\"\n",
    )
    .unwrap();
    plane.profile(
        "codex",
        &bin,
        &format!(
            "env = {{ CODEX_HOME = {:?} }}\n",
            codex_home.display().to_string()
        ),
    );

    let ready = start::ready(&plane.start("work"), plane.root()).expect("it starts");

    assert_eq!(ready.harness, Some(Harness::Codex));
    assert_eq!(ready.args, Vec::<String>::new());
    assert_eq!(
        ready.session, None,
        "charter chose no id it could not hand over"
    );
}

#[test]
fn the_persona_a_chat_adopts_rides_on_its_environment() {
    // How a persona reaches a chat at all: `CHARTER_PERSONA`, which is the rung charter's
    // own resolver reads. A profile may not set it — a declaration that tried would be
    // refused — so charter sets it at the launch.
    let plane = Plane::new();
    let bin = plane.wired();
    plane.profile("claude", &bin, "");
    let mut start = plane.start("work");
    start.persona = Some("steward".to_owned());

    let ready = start::ready(&start, plane.root()).expect("it starts");

    assert_eq!(
        ready.env.iter().find(|(n, _)| n == "CHARTER_PERSONA"),
        Some(&("CHARTER_PERSONA".to_owned(), "steward".to_owned()))
    );
}

#[test]
fn a_persona_this_plane_does_not_have_is_refused_rather_than_set_on_the_harness() {
    let plane = Plane::new();
    let bin = plane.wired();
    plane.profile("claude", &bin, "");
    let mut start = plane.start("work");
    start.persona = Some("nobody".to_owned());

    let why = start::ready(&start, plane.root()).expect_err("it refuses");

    assert!(why.contains("no persona 'nobody'"), "{why}");
}

#[test]
fn the_profile_name_and_the_persona_ride_beside_the_kind_never_inside_charter_harness() {
    // `CHARTER_HARNESS` keeps the REGISTRY's name for the kind. Hooks compare it to
    // `claude-code` for session ids, resume and the working spinner, and a value of
    // `claude-work` would make each of them quietly answer "not Claude Code".
    let plane = Plane::new();
    let bin = plane.wired();
    plane.profile("claude", &bin, "");
    let mut start = plane.start("work");
    start.persona = Some("steward".to_owned());

    let ready = start::ready(&start, plane.root()).expect("it starts");
    let of = |name: &str| {
        ready
            .env
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    };

    assert_eq!(of("CHARTER_HARNESS"), Some("claude-code"));
    assert_eq!(of("CHARTER_HARNESS_PROFILE"), Some("work"));
    assert_eq!(of("CHARTER_PERSONA"), Some("steward"));
}

#[test]
fn a_chat_whose_profile_is_gone_is_skipped_by_name_and_never_given_another() {
    // ADR 0022: another profile may be another account, where that chat's resume id does not
    // exist and where its workspace's code was never meant to go. It stays in the record, so
    // declaring the profile again brings it back.
    let plane = Plane::new();
    let bin = plane.wired();
    plane.profile("claude", &bin, "");
    plane.declares("[harness.other]\nkind = \"claude\"\ncommand = [\"claude\"]\n");

    let why = start::ready(&plane.start("work"), plane.root()).expect_err("it refuses");

    assert!(
        why.contains("profile 'work' is not declared on this machine"),
        "{why}"
    );
    assert!(!why.contains("other"), "it offered another profile: {why}");
}

#[test]
fn a_profile_that_is_not_wired_refuses_the_start_rather_than_running_the_command() {
    let plane = Plane::new();
    // A stand-in that answers "no charter plugin here".
    let bin = plane.stand_in("[]");
    plane.profile("claude", &bin, "");

    let why = start::ready(&plane.start("work"), plane.root()).expect_err("it refuses");

    assert!(why.contains("is not wired"), "{why}");
}

#[test]
fn a_chat_with_a_conversation_recorded_comes_back_resumed_on_the_same_profile() {
    let plane = Plane::new();
    let bin = plane.wired();
    plane.profile("claude", &bin, "");
    let mut start = plane.start("work");
    let id = SessionId::new("11111111-2222-4333-8444-555555555555").unwrap();
    start.resume = Some(id.clone());

    let ready = start::ready(&start, plane.root()).expect("it starts");

    assert_eq!(
        ready.args,
        vec!["--resume", id.as_str(), "--name", "ide.7"],
        "the same conversation did not come back"
    );
    assert_eq!(ready.session, Some(id));
}

#[test]
fn an_opencode_profile_refuses_here_too_because_this_is_where_a_chat_starts() {
    let plane = Plane::new();
    let bin = plane.wired();
    plane.profile("opencode", &bin, "");

    let why = start::ready(&plane.start("work"), plane.root()).expect_err("it refuses");

    assert!(
        why.contains("charter-app v1 starts Claude Code and Codex"),
        "{why}"
    );
}
