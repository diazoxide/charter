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
        self.wired_as("claude-stand-in")
    }

    /// The same, under a program name of your choosing.
    ///
    /// Every stand-in here answers the probe ITSELF. An earlier version of the wrapper test
    /// below wrote `exec claude "$@"`, and it passed on a machine that happens to have a
    /// wired Claude Code installed — it was reading the developer's own `~/.claude` — then
    /// failed the moment CI ran it, where there is no `claude` at all. A test that needs a
    /// harness on the PATH is a test about the machine it runs on.
    fn wired_as(&self, program: &str) -> PathBuf {
        self.stand_in_named(
            program,
            "[{\"id\":\"charter@charter\",\"scope\":\"user\",\"enabled\":true}]",
        )
    }

    /// The program a profile points at: a stand-in `claude` answering the wiring probe.
    fn stand_in(&self, answer: &str) -> PathBuf {
        self.stand_in_named("claude-stand-in", answer)
    }

    fn stand_in_named(&self, program: &str, answer: &str) -> PathBuf {
        let bin = self.root().join(program);
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
    // A wrapper, which is the shape ADR 0022 names: a program that is not called `claude`.
    // It answers the probe itself rather than exec-ing a real harness, so this test is
    // about the port and not about what is installed on the machine running it.
    let wrapper = plane.wired_as("claude-wrap");
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

#[test]
fn the_persona_a_new_chat_starts_on_is_one_the_plane_actually_has() {
    // `[persona] default` is a line in a committed file and nothing checks that the persona
    // it names exists. Offering it as the picker's preselection anyway means the operator
    // presses Start and is refused over a persona they never chose.
    let plane = Plane::new();
    fs::write(
        plane.root().join("charter.toml"),
        "[persona]\ndefault = \"steward\"\n",
    )
    .unwrap();

    assert_eq!(
        start::persona_for_a_new_chat(plane.root()),
        Some("steward".to_owned()),
        "the plane has personas/steward/persona.md, so its default stands"
    );
}

#[test]
fn a_default_persona_the_plane_does_not_have_is_offered_to_nobody() {
    let plane = Plane::new();
    fs::write(
        plane.root().join("charter.toml"),
        "[persona]\ndefault = \"gone\"\n",
    )
    .unwrap();

    assert_eq!(start::persona_for_a_new_chat(plane.root()), None);
}

#[test]
fn the_shared_store_is_never_the_persona_a_new_chat_adopts() {
    // `_shared` is the store every persona reads, not a persona. It is admitted by name
    // wherever a persona directory is resolved, and it is NOT in the list of personas — so
    // a plane whose default names it would preselect a row the picker does not even draw.
    let plane = Plane::new();
    fs::create_dir_all(plane.root().join("personas/_shared")).unwrap();
    fs::write(
        plane.root().join("personas/_shared/persona.md"),
        "# shared\n",
    )
    .unwrap();
    fs::write(
        plane.root().join("charter.toml"),
        "[persona]\ndefault = \"_shared\"\n",
    )
    .unwrap();

    assert_eq!(start::persona_for_a_new_chat(plane.root()), None);
}

#[test]
fn a_plane_with_no_personas_at_all_offers_none_rather_than_failing() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("charter.toml"), "").unwrap();

    assert_eq!(start::persona_for_a_new_chat(dir.path()), None);
    assert_eq!(
        charter_core::workspaces::Plane::open(dir.path())
            .personas()
            .expect("a plane with no personas directory has no personas, and that is not a fault"),
        Vec::<String>::new()
    );
}

// ---------------------------------------------------------------------------
// From a review sweep's probes. Each was a way the picker's list and the start
// could disagree, or a way the launch could stall.
// ---------------------------------------------------------------------------

#[test]
fn a_persona_on_the_legacy_flat_layout_starts_because_the_picker_offers_it() {
    // `personas/<name>.md` is the old layout and `Plane::personas` lists it. The start used
    // to require `personas/<name>/persona.md`, so a plane on that layout offered personas
    // that could not start — a dialog arguing with itself.
    let plane = Plane::new();
    let bin = plane.wired();
    plane.profile("claude", &bin, "");
    fs::write(plane.root().join("personas/devops.md"), "# devops\n").unwrap();
    let mut start = plane.start("work");
    start.persona = Some("devops".to_owned());

    let ready = start::ready(&start, plane.root()).expect("a listed persona starts");

    assert_eq!(
        ready.env.iter().find(|(n, _)| n == "CHARTER_PERSONA"),
        Some(&("CHARTER_PERSONA".to_owned(), "devops".to_owned()))
    );
}

#[test]
fn the_shared_store_is_refused_as_a_persona_even_where_it_has_a_persona_md() {
    // `_shared` is the store every persona READS. The picker never offers it, and a caller
    // that is not the picker must not be able to point a chat's `CHARTER_PERSONA` at it.
    let plane = Plane::new();
    let bin = plane.wired();
    plane.profile("claude", &bin, "");
    fs::create_dir_all(plane.root().join("personas/_shared")).unwrap();
    fs::write(
        plane.root().join("personas/_shared/persona.md"),
        "# shared\n",
    )
    .unwrap();
    let mut start = plane.start("work");
    start.persona = Some("_shared".to_owned());

    let why = start::ready(&start, plane.root()).expect_err("it refuses");

    assert!(why.contains("no persona '_shared' on this plane"), "{why}");
}

#[test]
fn a_persona_whose_directory_leaves_the_plane_is_refused() {
    // A committed `personas/<name> -> <outside>` travels with the plane to every machine
    // that clones it. Gated on the entry that is OPENED, not on `personas/` above it.
    let plane = Plane::new();
    let bin = plane.wired();
    plane.profile("claude", &bin, "");
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("persona.md"), "# elsewhere\n").unwrap();
    std::os::unix::fs::symlink(outside.path(), plane.root().join("personas/away")).unwrap();
    let mut start = plane.start("work");
    start.persona = Some("away".to_owned());

    let why = start::ready(&start, plane.root()).expect_err("it refuses");

    assert!(why.contains("resolves outside this plane"), "{why}");
}

#[test]
fn a_probe_whose_harness_writes_more_than_a_pipe_holds_still_answers() {
    // The launch used to poll for exit and read the pipes afterwards, so a harness that
    // wrote more than a pipe holds blocked on its own write, never exited, and the launch
    // sat on the whole timeout before refusing a chat that was fine. Well over any pipe
    // buffer, and the answer has to arrive in seconds rather than in the timeout.
    let plane = Plane::new();
    let chatty = plane.root().join("chatty-claude");
    fs::write(
        &chatty,
        "#!/bin/sh\n\
         i=0\n\
         while [ $i -lt 400 ]; do\n\
         \x20 printf '%0.sx' $(seq 1 1000)\n\
         \x20 i=$((i+1))\n\
         done\n\
         printf '\\n[{\"id\":\"charter@charter\",\"scope\":\"user\",\"enabled\":true}]\\n'\n",
    )
    .unwrap();
    fs::set_permissions(&chatty, fs::Permissions::from_mode(0o755)).unwrap();
    plane.profile("claude", &chatty, "");
    let p = profiles::current(plane.root()).get("work").unwrap().clone();

    let began = std::time::Instant::now();
    let w = charter_core::wiring::detect(&p, plane.root(), plane.root());

    assert!(
        began.elapsed() < std::time::Duration::from_secs(20),
        "the probe stalled for {:?}, which is the deadlock this test exists for",
        began.elapsed()
    );
    // 400 KB of noise before the JSON, so this also pins that a probe reads it all.
    assert_eq!(
        w.state,
        charter_core::wiring::State::Unknown,
        "output that is not a list of rows is an unknown, not nothing installed: {w:?}"
    );
}
