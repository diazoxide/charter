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
use std::path::{Path, PathBuf};

use purlis_core::harness::{Harness, SessionId};
use purlis_core::profiles;
use purlis_core::start::{self, Start};

/// A plane with a stand-in `claude`. Nothing is asked of it before a launch: the app arms the
/// chat itself, so these tests are about starting and nothing else.
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
        Self { dir }
    }

    /// The program a profile points at.
    fn harness(&self) -> PathBuf {
        self.harness_as("claude-stand-in")
    }

    /// The same, under a program name of your choosing. It writes down that it was run, so a
    /// test can tell a launch that ran nothing from one that asked the harness something.
    ///
    /// Through `stand_in::program`, which is how every program a charter test runs is
    /// written: one written through this process's own descriptor can lose to `ETXTBSY` when
    /// it is run straight away (charter-app#81). Never `exec claude "$@"`: a test that needs a
    /// harness on the PATH is a test about the machine it runs on.
    fn harness_as(&self, program: &str) -> PathBuf {
        stand_in::program(
            self.root(),
            program,
            &format!(
                "#!/bin/sh\ntouch {:?}\n",
                self.root().join("ran").display().to_string()
            ),
        )
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
            purlis_core::profiletrust::record_launched(
                self.root(),
                name,
                &purlis_core::profiletrust::fingerprint(p),
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
            // The default every chat starts under, so the tests below describe the app as
            // it ships (ADR 0029).
            show_footer: false,
            resuming: None,
            without_sandbox: None,
            held: None,
            grants: Default::default(),
        }
    }
}

#[test]
fn a_chat_started_on_a_profile_runs_that_profiles_command_in_that_profiles_environment() {
    purlis_core::unsteered!();
    let plane = Plane::new();
    let bin = plane.harness();
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
    purlis_core::unsteered!();
    // The whole reason this is not `Harness::of_command`: a profile's command is commonly a
    // WRAPPER — ADR 0022 says so in as many words — and a wrapper's name is not `claude`.
    // Inferring from it hands the board `None`, which is the narrowest rule there is, and
    // the chat silently loses the session id that makes it resumable at all.
    let plane = Plane::new();
    // A wrapper, which is the shape ADR 0022 names: a program that is not called `claude`.
    // A stand-in rather than a real harness, so this test is about the port and not about
    // what is installed on the machine running it.
    let wrapper = plane.harness_as("claude-wrap");
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
    purlis_core::unsteered!();
    let plane = Plane::new();
    let bin = plane.harness();
    // Nothing in the Codex home: the app arms a Codex chat with `-c` flags of its own.
    plane.profile("codex", &bin, "");

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
    purlis_core::unsteered!();
    // How a persona reaches a chat at all: `CHARTER_PERSONA`, which is the rung charter's
    // own resolver reads. A profile may not set it — a declaration that tried would be
    // refused — so charter sets it at the launch.
    let plane = Plane::new();
    let bin = plane.harness();
    plane.profile("claude", &bin, "");
    let mut start = plane.start("work");
    start.persona = Some("steward".to_owned());

    let ready = start::ready(&start, plane.root()).expect("it starts");

    assert_eq!(
        ready.env.iter().find(|(n, _)| n == "PURLIS_PERSONA"),
        Some(&("PURLIS_PERSONA".to_owned(), "steward".to_owned()))
    );
}

#[test]
fn a_chat_that_asked_for_charters_footer_carries_the_word_that_says_so() {
    purlis_core::unsteered!();
    // Charter ADR 0029. The choice is per chat and it reaches the harness the only way it
    // can: an environment variable set at the exec, which Claude Code's `statusLine` command
    // inherits — and `charter statusline` IS that command, so it reads it and draws instead of
    // printing the empty line.
    let plane = Plane::new();
    let bin = plane.harness();
    plane.profile("claude", &bin, "");
    let mut start = plane.start("work");
    start.show_footer = true;

    let ready = start::ready(&start, plane.root()).expect("it starts");

    assert_eq!(
        ready
            .env
            .iter()
            .find(|(n, _)| n == purlis_core::start::FOOTER_ENV),
        Some(&(
            purlis_core::start::FOOTER_ENV.to_owned(),
            purlis_core::start::FOOTER_SHOW.to_owned()
        ))
    );
}

#[test]
fn a_chat_that_did_not_ask_carries_no_footer_variable_at_all() {
    purlis_core::unsteered!();
    // The default, and it is an ABSENCE rather than a second word. Nothing has to be unset
    // for an ordinary chat, and there is exactly one value this variable is ever written
    // with — so a value that is not it came from somewhere that is not charter.
    let plane = Plane::new();
    let bin = plane.harness();
    plane.profile("claude", &bin, "");

    let ready = start::ready(&plane.start("work"), plane.root()).expect("it starts");

    assert!(
        !ready
            .env
            .iter()
            .any(|(n, _)| n == purlis_core::start::FOOTER_ENV),
        "{:?}",
        ready.env
    );
}

#[test]
fn a_persona_this_plane_does_not_have_is_refused_rather_than_set_on_the_harness() {
    purlis_core::unsteered!();
    let plane = Plane::new();
    let bin = plane.harness();
    plane.profile("claude", &bin, "");
    let mut start = plane.start("work");
    start.persona = Some("nobody".to_owned());

    let why = start::ready(&start, plane.root()).expect_err("it refuses");

    assert!(why.contains("no persona 'nobody'"), "{why}");
}

#[test]
fn the_profile_name_and_the_persona_ride_beside_the_kind_never_inside_charter_harness() {
    purlis_core::unsteered!();
    // `CHARTER_HARNESS` keeps the REGISTRY's name for the kind. Hooks compare it to
    // `claude-code` for session ids, resume and the working spinner, and a value of
    // `claude-work` would make each of them quietly answer "not Claude Code".
    let plane = Plane::new();
    let bin = plane.harness();
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

    assert_eq!(of("PURLIS_HARNESS"), Some("claude-code"));
    assert_eq!(of("PURLIS_HARNESS_PROFILE"), Some("work"));
    assert_eq!(of("PURLIS_PERSONA"), Some("steward"));
}

#[test]
fn a_chat_whose_profile_is_gone_is_skipped_by_name_and_never_given_another() {
    purlis_core::unsteered!();
    // ADR 0022: another profile may be another account, where that chat's resume id does not
    // exist and where its workspace's code was never meant to go. It stays in the record, so
    // declaring the profile again brings it back.
    let plane = Plane::new();
    let bin = plane.harness();
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
fn a_chat_starts_with_nothing_installed_and_its_harness_is_not_run_to_get_there() {
    purlis_core::unsteered!();
    // The Python charter's plugin used to be probed for — `plugin list --json` — and
    // installed from its marketplace when it was missing. The launch runs nothing now.
    let plane = Plane::new();
    let bin = plane.harness();
    plane.profile("claude", &bin, "");

    start::ready(&plane.start("work"), plane.root()).expect("it starts");

    assert!(
        !plane.root().join("ran").exists(),
        "the harness was run before the chat was"
    );
}

#[test]
fn a_chat_with_a_conversation_recorded_comes_back_resumed_on_the_same_profile() {
    purlis_core::unsteered!();
    let plane = Plane::new();
    let bin = plane.harness();
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
fn an_opencode_chat_starts_plain_and_resumes_by_its_session_flag() {
    purlis_core::unsteered!();
    // #371. opencode takes no id for a new session, so charter chooses none and adopts the
    // one its shim reports; `-s <id>` brings one back.
    let plane = Plane::new();
    let bin = plane.harness();
    plane.profile("opencode", &bin, "");

    let ready = start::ready(&plane.start("work"), plane.root()).expect("it starts");
    assert_eq!(ready.harness, Some(Harness::Opencode));
    assert_eq!(ready.args, Vec::<String>::new());
    assert_eq!(ready.session, None);
    assert!(
        ready
            .env
            .contains(&("PURLIS_HARNESS".to_owned(), "opencode".to_owned())),
        "{:?}",
        ready.env
    );

    let id = SessionId::new("ses_f232c39feffecxEyWLvftyLSYU").unwrap();
    let mut again = plane.start("work");
    again.resume = Some(id.clone());
    let ready = start::ready(&again, plane.root()).expect("it resumes");
    assert_eq!(ready.args, ["-s", id.as_str()]);
    assert_eq!(ready.session, Some(id));
}

#[test]
fn an_opencode_chat_is_armed_through_its_environment_and_nothing_on_its_line() {
    purlis_core::unsteered!();
    // The shim rides in `OPENCODE_CONFIG_CONTENT`, so the command line is the profile's and
    // charter's own words, exactly.
    let plane = Plane::new();
    a_wrapper_profile(&plane, "opencode", &["oc-work"]);
    let shim = purlis_core::opencode::shim_in(&plane.root().join("plugin"));
    fs::create_dir_all(shim.parent().unwrap()).unwrap();
    fs::write(
        &shim,
        purlis_core::opencode::shim(purlis_core::opencode::Arming::Session),
    )
    .unwrap();

    let ready = start::ready(&plane.start("work"), plane.root()).expect("it starts");
    let binary = plane.root().join("charter");
    let plugin = plane.root().join("plugin");
    let kit = purlis_core::harness::Kit {
        binary: &binary,
        plugin: Some(&plugin),
    };
    let purlis_core::harness::StateHooks::ThisSessionOnly { args, env, .. } =
        Harness::Opencode.state_hooks(kit, Some(plane.root()), &ready.plugins, None)
    else {
        panic!("opencode is armed");
    };
    assert_eq!(ready.command_line(args), ["oc-work"]);
    let config = env
        .iter()
        .find(|(name, _)| name == purlis_core::opencode::CONFIG_ENV)
        .map(|(_, value)| value.clone())
        .expect("the shim is handed over");
    assert!(config.contains("opencode/purlis.ts"), "{config}");
}

#[test]
fn an_opencode_profile_that_would_load_no_plugin_is_refused_where_the_chat_starts() {
    purlis_core::unsteered!();
    let plane = Plane::new();
    a_wrapper_profile(&plane, "opencode", &["--pure"]);

    let why = start::ready(&plane.start("work"), plane.root()).expect_err("it refuses");

    assert!(why.contains("without purlis's guard"), "{why}");
}

#[test]
fn the_persona_a_new_chat_starts_on_is_one_the_plane_actually_has() {
    purlis_core::unsteered!();
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
    purlis_core::unsteered!();
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
    purlis_core::unsteered!();
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
    purlis_core::unsteered!();
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("charter.toml"), "").unwrap();

    assert_eq!(start::persona_for_a_new_chat(dir.path()), None);
    assert_eq!(
        purlis_core::workspaces::Plane::open(dir.path())
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
    purlis_core::unsteered!();
    // `personas/<name>.md` is the old layout and `Plane::personas` lists it. The start used
    // to require `personas/<name>/persona.md`, so a plane on that layout offered personas
    // that could not start — a dialog arguing with itself.
    let plane = Plane::new();
    let bin = plane.harness();
    plane.profile("claude", &bin, "");
    fs::write(plane.root().join("personas/devops.md"), "# devops\n").unwrap();
    let mut start = plane.start("work");
    start.persona = Some("devops".to_owned());

    let ready = start::ready(&start, plane.root()).expect("a listed persona starts");

    assert_eq!(
        ready.env.iter().find(|(n, _)| n == "PURLIS_PERSONA"),
        Some(&("PURLIS_PERSONA".to_owned(), "devops".to_owned()))
    );
}

#[test]
fn the_shared_store_is_refused_as_a_persona_even_where_it_has_a_persona_md() {
    purlis_core::unsteered!();
    // `_shared` is the store every persona READS. The picker never offers it, and a caller
    // that is not the picker must not be able to point a chat's `CHARTER_PERSONA` at it.
    let plane = Plane::new();
    let bin = plane.harness();
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
    purlis_core::unsteered!();
    // A committed `personas/<name> -> <outside>` travels with the plane to every machine
    // that clones it. Gated on the entry that is OPENED, not on `personas/` above it.
    let plane = Plane::new();
    let bin = plane.harness();
    plane.profile("claude", &bin, "");
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("persona.md"), "# elsewhere\n").unwrap();
    std::os::unix::fs::symlink(outside.path(), plane.root().join("personas/away")).unwrap();
    let mut start = plane.start("work");
    start.persona = Some("away".to_owned());

    let why = start::ready(&start, plane.root()).expect_err("it refuses");

    assert!(why.contains("resolves outside this plane"), "{why}");
}

/// What the app arms `harness` with for this session, as the app asks for it: the bundled
/// plugin for Claude Code, the `-c` flags for Codex.
fn armed(harness: Harness, root: &Path) -> Vec<String> {
    armed_with(harness, root, &std::collections::BTreeMap::new())
}

/// The same, for a chat handed `plugins` — what `start::ready` put on the launch.
fn armed_with(
    harness: Harness,
    root: &Path,
    plugins: &std::collections::BTreeMap<String, bool>,
) -> Vec<String> {
    let binary = root.join("charter");
    let plugin = root.join("plugin");
    let kit = purlis_core::harness::Kit {
        binary: &binary,
        plugin: Some(&plugin),
    };
    match harness.state_hooks(kit, Some(root), plugins, None) {
        purlis_core::harness::StateHooks::ThisSessionOnly { args, .. } => args,
        purlis_core::harness::StateHooks::None => panic!("{harness:?} is armed"),
    }
}

/// A profile `work` of `kind` whose command is the stand-in followed by `rest`.
fn a_wrapper_profile(plane: &Plane, kind: &str, rest: &[&str]) -> PathBuf {
    let bin = plane.harness_as("ccs");
    let mut words = vec![format!("{:?}", bin.display().to_string())];
    words.extend(rest.iter().map(|w| format!("{w:?}")));
    plane.declares(&format!(
        "[harness.work]\nkind = {kind:?}\ncommand = [{}]\n",
        words.join(", ")
    ));
    plane.approve("work");
    bin
}

#[test]
fn a_wrapper_profiles_own_words_come_before_everything_the_app_adds() {
    purlis_core::unsteered!();
    // M8.3: `["ccs", "work"]` is ONE command the operator wrote — `work` is the wrapper's own
    // subcommand, read before it hands the rest to the harness. The app's flags went straight
    // after argv[0] and started `ccs --plugin-dir … --settings … work`, which a wrapper that
    // expects its subcommand first cannot read.
    let plane = Plane::new();
    let bin = a_wrapper_profile(&plane, "claude", &["work"]);

    let ready = start::ready(&plane.start("work"), plane.root()).expect("it starts");
    let hooks = armed(Harness::ClaudeCode, plane.root());
    let line = ready.command_line(hooks.clone());

    assert_eq!(ready.program, bin.display().to_string());
    assert_eq!(
        line[0], "work",
        "the wrapper's subcommand is first: {line:?}"
    );
    assert_eq!(line[1..1 + hooks.len()], hooks[..], "then the app's flags");
    assert_eq!(hooks[0], "--plugin-dir", "the premise");
    let id = ready.session.clone().expect("an id was chosen");
    assert_eq!(
        line[1 + hooks.len()..],
        ["--session-id", id.as_str(), "--name", "ide.7"],
        "and charter's own words last"
    );
}

#[test]
fn a_plain_profile_is_started_on_the_line_it_always_was() {
    purlis_core::unsteered!();
    let plane = Plane::new();
    let bin = plane.harness();
    plane.profile("claude", &bin, "");

    let ready = start::ready(&plane.start("work"), plane.root()).expect("it starts");
    let hooks = armed(Harness::ClaudeCode, plane.root());

    assert_eq!(ready.command, Vec::<String>::new());
    let id = ready.session.clone().expect("an id was chosen");
    let mut want = hooks.clone();
    want.extend(["--session-id", id.as_str(), "--name", "ide.7"].map(str::to_owned));
    assert_eq!(ready.command_line(hooks), want);
}

#[test]
fn a_codex_wrapper_resumes_after_its_own_words_and_the_apps_flags() {
    purlis_core::unsteered!();
    // Codex takes the same rule: its `-c` flags after the wrapper's words, and the `resume`
    // SUBCOMMAND last of all.
    let plane = Plane::new();
    a_wrapper_profile(&plane, "codex", &["codex-work"]);
    let id = SessionId::new("11111111-2222-4333-8444-555555555555").unwrap();
    let mut start = plane.start("work");
    start.resume = Some(id.clone());

    let ready = start::ready(&start, plane.root()).expect("it starts");
    let hooks = armed(Harness::Codex, plane.root());
    let line = ready.command_line(hooks.clone());

    assert_eq!(hooks[0], "-c", "the premise");
    assert_eq!(line[0], "codex-work", "{line:?}");
    assert_eq!(line[1..1 + hooks.len()], hooks[..]);
    assert_eq!(line[1 + hooks.len()..], ["resume", id.as_str()]);
}

// -------------------------------------------------------------------------------------
// The project's harness plugins (charter-app#274, ADR 0050)
// -------------------------------------------------------------------------------------

/// A Claude Code config directory, as a fixture, holding `installed`.
fn claude_config(plane: &Plane, installed: &[&str]) -> PathBuf {
    let dir = plane.root().join("claude-config");
    fs::create_dir_all(dir.join("plugins")).unwrap();
    let plugins: Vec<String> = installed
        .iter()
        .map(|id| format!("{id:?}: [{{\"scope\": \"user\"}}]"))
        .collect();
    fs::write(
        dir.join("plugins/installed_plugins.json"),
        format!(
            "{{\"version\": 2, \"plugins\": {{{}}}}}",
            plugins.join(", ")
        ),
    )
    .unwrap();
    dir
}

#[test]
fn a_claude_code_chat_is_started_with_exactly_the_plugins_its_project_chose() {
    purlis_core::unsteered!();
    // Shared turns two off and one on, Local turns one of them back on; the one nobody names is
    // left to Claude Code, and the one this machine has not installed is handed to nothing. The
    // listing is read from the chat's own CLAUDE_CONFIG_DIR, so a profile on another account is
    // listed against that account.
    let plane = Plane::new();
    let config = claude_config(
        &plane,
        &["figma@official", "serena@official", "humanizer@h"],
    );
    fs::write(
        plane.root().join("charter.toml"),
        "[harness_plugins.claude]\n\"figma@official\" = false\n\"serena@official\" = false\n\
         \"acme@corp\" = true\n",
    )
    .unwrap();
    let bin = plane.harness();
    plane.profile(
        "claude",
        &bin,
        &format!(
            "env = {{ CLAUDE_CONFIG_DIR = {:?} }}\n\n[harness_plugins.claude]\n\"serena@official\" = true\n",
            config.display().to_string()
        ),
    );

    let ready = start::ready(&plane.start("work"), plane.root()).expect("it starts");
    let line = ready.command_line(armed_with(
        Harness::ClaudeCode,
        plane.root(),
        &ready.plugins,
    ));

    let at = line
        .iter()
        .position(|word| word == "--settings")
        .expect("settings");
    let settings: serde_json::Value = serde_json::from_str(&line[at + 1]).expect("JSON");
    assert_eq!(
        settings["enabledPlugins"],
        serde_json::json!({
            "purlis@inline": true,
            "charter@charter": false,
            "charter@inline": false,
            "charter-app@inline": false,
            "charter@charter-app": false,
            "figma@official": false,
            "serena@official": true,
        })
    );
}

#[test]
fn a_codex_chat_is_started_with_no_plugin_choice_whatever_its_project_says() {
    purlis_core::unsteered!();
    // Codex's adapter cannot apply per chat (measured on 0.147.0), so the choice is shown in the
    // settings tab as not supported yet and the launch carries nothing for it.
    let plane = Plane::new();
    let home = plane.root().join("codex-home");
    fs::create_dir_all(&home).unwrap();
    fs::write(
        home.join("config.toml"),
        "[plugins.\"charter@charter\"]\nenabled = true\n",
    )
    .unwrap();
    fs::write(
        plane.root().join("charter.toml"),
        "[harness_plugins.codex]\n\"charter@charter\" = false\n",
    )
    .unwrap();
    let bin = plane.harness_as("codex");
    plane.profile(
        "codex",
        &bin,
        &format!(
            "env = {{ CODEX_HOME = {:?} }}\n",
            home.display().to_string()
        ),
    );

    let ready = start::ready(&plane.start("work"), plane.root()).expect("it starts");

    assert!(ready.plugins.is_empty(), "{:?}", ready.plugins);
    let line = ready.command_line(armed_with(Harness::Codex, plane.root(), &ready.plugins));
    assert!(
        line.iter().all(|word| !word.contains("plugins")),
        "{line:?}"
    );
}

#[test]
fn a_chat_started_in_a_workspace_is_handed_that_workspaces_plugin_choices_between_shared_and_local()
{
    purlis_core::unsteered!();
    // charter-app#282: `settings.harness_plugins` in the workspace's `workspace.json` sits
    // between charter.toml and charter.local.toml. The chat is in the workspace because its
    // directory is, which is how the window files it under one.
    let plane = Plane::new();
    let config = claude_config(
        &plane,
        &["figma@official", "serena@official", "humanizer@h"],
    );
    fs::write(
        plane.root().join("charter.toml"),
        "[harness_plugins.claude]\n\"figma@official\" = true\n\"serena@official\" = true\n",
    )
    .unwrap();
    let alpha = plane.root().join("workspaces/alpha");
    fs::create_dir_all(&alpha).unwrap();
    fs::write(
        alpha.join("workspace.json"),
        r#"{"name": "alpha", "settings": {"harness_plugins": {"claude": {"figma@official": false, "serena@official": false, "humanizer@h": false}}}}"#,
    )
    .unwrap();
    let bin = plane.harness();
    plane.profile(
        "claude",
        &bin,
        &format!(
            "env = {{ CLAUDE_CONFIG_DIR = {:?} }}\n\n[harness_plugins.claude]\n\"serena@official\" = true\n",
            config.display().to_string()
        ),
    );
    let enabled = |cwd: PathBuf| {
        let ready = start::ready(
            &Start {
                cwd: Some(cwd),
                ..plane.start("work")
            },
            plane.root(),
        )
        .expect("it starts");
        let line = ready.command_line(armed_with(
            Harness::ClaudeCode,
            plane.root(),
            &ready.plugins,
        ));
        let at = line
            .iter()
            .position(|word| word == "--settings")
            .expect("settings");
        let settings: serde_json::Value = serde_json::from_str(&line[at + 1]).expect("JSON");
        settings["enabledPlugins"].clone()
    };

    assert_eq!(
        enabled(alpha.clone()),
        serde_json::json!({
            "purlis@inline": true,
            "charter@charter": false,
            "charter@inline": false,
            "charter-app@inline": false,
            "charter@charter-app": false,
            "figma@official": false,
            "humanizer@h": false,
            "serena@official": true,
        }),
        "the workspace overrides Shared, and Local overrides the workspace"
    );
    assert_eq!(
        enabled(plane.root().to_path_buf()),
        serde_json::json!({
            "purlis@inline": true,
            "charter@charter": false,
            "charter@inline": false,
            "charter-app@inline": false,
            "charter@charter-app": false,
            "figma@official": true,
            "serena@official": true,
        }),
        "a chat outside the workspace is not handed its choices"
    );
}

#[test]
fn a_chat_started_in_no_directory_starts_in_the_plane_not_where_the_app_was_launched() {
    purlis_core::unsteered!();
    // A plane with no workspace yet has nowhere else to start its outer chat, and the window
    // sends no directory for it. Left unset, the terminal takes the app's own directory,
    // which is `/` for an app opened from the Finder or the Dock.
    let plane = Plane::new();
    let bin = plane.harness();
    plane.profile("claude", &bin, "");
    let start = Start {
        cwd: None,
        ..plane.start("work")
    };

    let ready = start::ready(&start, plane.root()).expect("it starts");

    assert_eq!(ready.cwd.as_deref(), Some(plane.root()));
}

// ---- SI-1: a chat knows where it was started ------------------------------------------------

/// What `name` is in a started chat's environment, or `None`.
fn env_of<'a>(ready: &'a start::Ready, name: &str) -> Option<&'a str> {
    ready
        .env
        .iter()
        .find(|(n, _)| n == name)
        .map(|(_, v)| v.as_str())
}

#[test]
fn a_chat_started_in_a_workspace_is_told_which_one() {
    purlis_core::unsteered!();
    // The defect: the app filed this chat under `alpha` and set nothing, so the chat's own
    // briefing asked the operator which workspace it was in.
    let plane = Plane::new();
    let bin = plane.harness();
    plane.profile("claude", &bin, "");
    fs::create_dir_all(plane.root().join("workspaces/alpha/svc")).unwrap();

    for cwd in ["workspaces/alpha", "workspaces/alpha/svc"] {
        let start = Start {
            cwd: Some(plane.root().join(cwd)),
            ..plane.start("work")
        };
        let ready = start::ready(&start, plane.root()).expect("it starts");
        assert_eq!(env_of(&ready, "PURLIS_WORKSPACE"), Some("alpha"), "{cwd}");
        assert_eq!(env_of(&ready, "PURLIS_PLANE_ROOT_SESSION"), None, "{cwd}");
    }
}

#[test]
fn a_chat_started_at_the_plane_root_is_told_it_is_in_no_workspace() {
    purlis_core::unsteered!();
    let plane = Plane::new();
    let bin = plane.harness();
    plane.profile("claude", &bin, "");
    fs::create_dir_all(plane.root().join("workspaces/alpha")).unwrap();

    for cwd in [Some(plane.root().to_path_buf()), None] {
        let start = Start {
            cwd: cwd.clone(),
            ..plane.start("work")
        };
        let ready = start::ready(&start, plane.root()).expect("it starts");
        assert_eq!(
            env_of(&ready, "PURLIS_PLANE_ROOT_SESSION"),
            Some("1"),
            "{cwd:?}"
        );
        assert_eq!(env_of(&ready, "PURLIS_WORKSPACE"), None, "{cwd:?}");
    }
}

#[test]
fn a_chat_started_anywhere_in_the_plane_outside_every_workspace_is_at_the_plane_root() {
    purlis_core::unsteered!();
    // SI-1b: a chat started in `docs/` was told nothing, fell to the plane's default workspace
    // and was asked which workspace it was in. Anywhere under the plane that is not a
    // workspace's is the plane root, which is also the tab the window files it on.
    let plane = Plane::new();
    let bin = plane.harness();
    plane.profile("claude", &bin, "");
    fs::create_dir_all(plane.root().join("docs/adr")).unwrap();
    fs::create_dir_all(plane.root().join("workspaces/alpha")).unwrap();

    for cwd in ["docs", "docs/adr", ".charter", "workspaces"] {
        fs::create_dir_all(plane.root().join(cwd)).unwrap();
        let start = Start {
            cwd: Some(plane.root().join(cwd)),
            ..plane.start("work")
        };
        let ready = start::ready(&start, plane.root()).expect("it starts");
        assert_eq!(
            env_of(&ready, "PURLIS_PLANE_ROOT_SESSION"),
            Some("1"),
            "{cwd}"
        );
        assert_eq!(env_of(&ready, "PURLIS_WORKSPACE"), None, "{cwd}");
    }
}

#[test]
fn a_chat_started_outside_the_plane_is_pinned_to_nothing() {
    purlis_core::unsteered!();
    // Not in the plane at all: charter says nothing it does not know, and the chat's own
    // ladder answers as it always has.
    let plane = Plane::new();
    let bin = plane.harness();
    plane.profile("claude", &bin, "");
    let elsewhere = tempfile::tempdir().unwrap();
    let start = Start {
        cwd: Some(elsewhere.path().to_path_buf()),
        ..plane.start("work")
    };

    let ready = start::ready(&start, plane.root()).expect("it starts");

    assert_eq!(env_of(&ready, "PURLIS_WORKSPACE"), None);
    assert_eq!(env_of(&ready, "PURLIS_PLANE_ROOT_SESSION"), None);
}

// -------------------------------------------------------------------------------------
// The sandbox (ADR 0067): a plane that turned it on starts every chat sandboxed, or not at all
// -------------------------------------------------------------------------------------

#[test]
fn a_chat_in_a_plane_that_says_nothing_of_the_sandbox_starts_unsandboxed_as_before() {
    purlis_core::unsteered!();
    let plane = Plane::new();
    let bin = plane.harness();
    plane.profile("claude", &bin, "");

    let ready = start::ready(&plane.start("work"), plane.root()).expect("it starts");

    assert_eq!(ready.sandbox, None);
}

#[test]
fn a_chat_on_a_profile_in_a_project_whose_manifest_has_gone_is_not_started() {
    purlis_core::unsteered!();
    // D-1410e: a project with no manifest at its root cannot say whether it runs chats
    // sandboxed, so the chat is refused rather than started unsandboxed.
    let plane = Plane::new();
    let bin = plane.harness();
    plane.profile("claude", &bin, "");
    fs::remove_file(plane.root().join("charter.toml")).unwrap();

    let refused = start::ready(&plane.start("work"), plane.root()).expect_err("not started");

    assert!(refused.contains("is missing"), "{refused}");
    assert!(!plane.root().join("ran").exists(), "the harness ran");
}

/// The plane's own stand-in harness, in a folder no chat may write. A sandboxed start refuses a
/// program anywhere a chat can write, the system temp folders included (ruling V87g).
fn harness_outside(plane: &Plane, outside: &stand_in::NoChatWrites, program: &str) -> PathBuf {
    stand_in::program(
        outside.path(),
        program,
        &format!(
            "#!/bin/sh\ntouch {:?}\n",
            plane.root().join("ran").display().to_string()
        ),
    )
}

/// Declares profile `work` of `kind` running `command`, and approves it.
fn command_profile(plane: &Plane, kind: &str, command: &[String]) {
    let words: Vec<String> = command.iter().map(|word| format!("{word:?}")).collect();
    plane.declares(&format!(
        "[harness.work]\nkind = {kind:?}\ncommand = [{}]\n",
        words.join(", ")
    ));
    plane.approve("work");
}

/// A plane that runs every chat sandboxed.
fn a_sandboxed_plane() -> Plane {
    let plane = Plane::new();
    fs::write(
        plane.root().join("charter.toml"),
        "[sandbox]\nmode = \"on\"\n",
    )
    .unwrap();
    plane
}

/// A plane with the sandbox on and an opencode profile whose program is outside it, and what
/// starting it answers.
fn an_opencode_chat_in_a_sandboxed_plane()
-> (Plane, stand_in::NoChatWrites, Result<start::Ready, String>) {
    let plane = a_sandboxed_plane();
    let outside = stand_in::NoChatWrites::new();
    let bin = harness_outside(&plane, &outside, "opencode");
    plane.profile("opencode", &bin, "");
    let started = start::ready(&plane.start("work"), plane.root());
    (plane, outside, started)
}

/// A profile's command, made in a plane.
type Command<'a> = dyn Fn(&Plane) -> Vec<String> + 'a;

#[test]
fn a_sandboxed_chat_whose_command_hands_its_program_a_file_a_chat_can_write_is_not_started() {
    purlis_core::unsteered!();
    // D-88g: an interpreter outside, handed a file the chat wrote, runs the chat's code.
    const CLAUDE: &str = "#!/bin/sh\necho '2.1.288 (Claude Code)'\n";
    let outside = stand_in::NoChatWrites::new();
    let claude = stand_in::program(outside.path(), "claude", CLAUDE);
    let s = |path: &Path| path.display().to_string();
    let temp = tempfile::tempdir().unwrap();
    let in_temp = stand_in::program(temp.path(), "c.sh", CLAUDE);
    let cases: Vec<(&str, Box<Command<'_>>)> = vec![
        (
            "a shell on a script in the workspace",
            Box::new(|plane: &Plane| {
                let ws = plane.root().join("workspaces/work");
                fs::create_dir_all(&ws).unwrap();
                vec!["/bin/sh".into(), s(&stand_in::program(&ws, "c.sh", CLAUDE))]
            }),
        ),
        (
            "env on a script at the plane's root",
            Box::new(|plane: &Plane| {
                vec![
                    "/usr/bin/env".into(),
                    s(&stand_in::program(plane.root(), "c.sh", CLAUDE)),
                ]
            }),
        ),
        (
            "a shell on a relative script in the chat's folder",
            Box::new(|plane: &Plane| {
                stand_in::program(plane.root(), "c.sh", CLAUDE);
                vec!["/bin/sh".into(), "c.sh".into()]
            }),
        ),
        (
            "a shell on a script in temp",
            Box::new(|_: &Plane| vec!["/bin/sh".into(), s(&in_temp)]),
        ),
        (
            "a flag's value naming a file in the plane",
            Box::new(|plane: &Plane| {
                let config = plane.root().join("x.json");
                fs::write(&config, "{}").unwrap();
                vec![s(&claude), format!("--mcp-config={}", s(&config))]
            }),
        ),
    ];
    for (what, command) in cases {
        let plane = a_sandboxed_plane();
        command_profile(&plane, "claude", &command(&plane));

        let refused = start::ready(&plane.start("work"), plane.root()).expect_err(what);

        assert!(
            refused.contains("which lies where this chat can write"),
            "{what}: {refused}"
        );
    }
}

#[test]
fn a_sandboxed_claude_code_chat_takes_words_that_name_no_file() {
    purlis_core::unsteered!();
    let plane = a_sandboxed_plane();
    let outside = stand_in::NoChatWrites::new();
    let claude = stand_in::program(
        outside.path(),
        "claude",
        "#!/bin/sh\necho '2.1.288 (Claude Code)'\n",
    );
    command_profile(
        &plane,
        "claude",
        &[claude.display().to_string(), "--model=x".into()],
    );

    let started = start::ready(&plane.start("work"), plane.root());

    // Refused for nothing the command names. Off macOS a missing backend may still refuse it,
    // and says so.
    match started {
        Ok(ready) => assert_eq!(
            ready.sandbox.map(|applied| applied.harness()),
            Some(Harness::ClaudeCode)
        ),
        Err(refused) => assert!(
            !cfg!(target_os = "macos") && !refused.contains("where this chat can write"),
            "{refused}"
        ),
    }
}

#[test]
fn a_sandboxed_chat_whose_program_is_inside_the_plane_is_not_started_sandboxed() {
    purlis_core::unsteered!();
    // Ruling V87g: a wrapper inside the plane is code charter runs later, which a sandboxed
    // chat could have changed.
    let plane = Plane::new();
    fs::write(
        plane.root().join("charter.toml"),
        "[sandbox]\nmode = \"on\"\n",
    )
    .unwrap();
    // Claude Code, whose sandbox every system charter runs on can apply; opencode is wrapped on
    // macOS only, and on Linux its refusal would come first.
    let bin = plane.harness_as("claude");
    plane.profile("claude", &bin, "");

    let refused = start::ready(&plane.start("work"), plane.root()).expect_err("not started");

    assert!(
        refused.contains("the program lives where this chat can write"),
        "{refused}"
    );
    assert!(!plane.root().join("ran").exists(), "the harness was run");
}

#[test]
fn a_sandboxed_chat_whose_program_is_a_relative_path_is_not_started_sandboxed() {
    purlis_core::unsteered!();
    // Ruling V87g: a relative program is found from the chat's folder, which the chat writes.
    let plane = Plane::new();
    fs::write(
        plane.root().join("charter.toml"),
        "[sandbox]\nmode = \"on\"\n",
    )
    .unwrap();
    plane.declares("[harness.work]\nkind = \"claude\"\ncommand = [\"./claude\"]\n");
    plane.approve("work");

    let refused = start::ready(&plane.start("work"), plane.root()).expect_err("not started");

    assert!(
        refused.contains("this profile's program is a relative path"),
        "{refused}"
    );
}

#[test]
fn a_claude_code_profile_whose_program_is_not_claude_code_is_not_started_sandboxed() {
    purlis_core::unsteered!();
    // Ruling V87g: Claude Code's own sandbox binds only Claude Code.
    let plane = Plane::new();
    fs::write(
        plane.root().join("charter.toml"),
        "[sandbox]\nmode = \"on\"\n",
    )
    .unwrap();
    let outside = stand_in::NoChatWrites::new();
    let bin = harness_outside(&plane, &outside, "claude");
    plane.profile("claude", &bin, "");

    let refused = start::ready(&plane.start("work"), plane.root()).expect_err("not started");

    assert!(
        refused.ends_with(
            "this profile's program does not answer as Claude Code, whose sandbox it was \
             given, so it was not started sandboxed."
        ),
        "{refused}"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn an_opencode_chat_in_a_sandboxed_plane_starts_inside_charters_wrap() {
    purlis_core::unsteered!();
    let (plane, _outside, started) = an_opencode_chat_in_a_sandboxed_plane();

    let ready = started.expect("it starts");

    let applied = ready.sandbox.expect("sandboxed");
    assert_eq!(applied.harness(), Harness::Opencode);
    assert!(!plane.root().join("ran").exists(), "the harness was run");
}

#[cfg(not(target_os = "macos"))]
#[test]
fn an_opencode_chat_in_a_sandboxed_plane_is_not_started_rather_than_started_unconfined() {
    purlis_core::unsteered!();
    let (plane, _outside, started) = an_opencode_chat_in_a_sandboxed_plane();

    let refused = started.expect_err("not started");

    assert!(
        refused.starts_with("this plane runs every chat sandboxed"),
        "{refused}"
    );
    assert!(!plane.root().join("ran").exists(), "the harness was run");
}

/// The per-chat opt-out (ADR 0067 §7, ruling V78 a): the picker's "Start without the
/// sandbox" starts this one chat unsandboxed, says so on its tab, and hands the app what the
/// audit records — on every system, the ones the sandbox cannot be applied on included.
#[test]
fn a_person_starts_one_chat_without_the_sandbox_and_its_tab_says_so() {
    purlis_core::unsteered!();
    let (plane, _outside, _) = an_opencode_chat_in_a_sandboxed_plane();
    let start = Start {
        without_sandbox: Some(purlis_core::sandbox::OptOut {
            reason: Some("needs the network".to_owned()),
        }),
        ..plane.start("work")
    };

    let ready = start::ready(&start, plane.root()).expect("it starts");

    assert_eq!(ready.sandbox, None);
    let lifted = ready.unsandboxed.expect("audited as lifted");
    assert_eq!(lifted.by, purlis_core::sandbox::By::Person);
    assert_eq!(lifted.reason.as_deref(), Some("needs the network"));
    assert!(
        ready.notices.contains(&lifted.notice()),
        "{:?}",
        ready.notices
    );
}

#[test]
fn a_chat_with_no_opt_out_is_never_started_as_if_it_had_one() {
    purlis_core::unsteered!();
    let plane = Plane::new();
    let bin = plane.harness();
    plane.profile("claude", &bin, "");

    let ready = start::ready(&plane.start("work"), plane.root()).expect("it starts");

    assert_eq!(ready.unsandboxed, None, "nothing lifted, nothing to audit");
}

// -------------------------------------------------------------------------------------
// The picker's view of the sandbox, asked as the start asks it (ruling V87g)
// -------------------------------------------------------------------------------------

/// A machine where every harness has a sandbox charter compiles.
fn a_mac() -> purlis_core::sandbox::Machine {
    purlis_core::sandbox::Machine {
        env: purlis_core::secrets::Env::of(&[]),
        home: None,
        os: purlis_core::sandbox::Os::MacOs,
    }
}

/// What the picker says for the `work` profile, approved or not, with every program run
/// recorded in `ran`.
fn picker_says(
    plane: &Plane,
    ran: &std::sync::Mutex<Vec<Vec<String>>>,
) -> purlis_core::sandbox::Ahead {
    picker_says_on(plane, ran, &a_mac(), &|_| true)
}

/// [`picker_says`] on `machine`, with `has` saying which backend programs it has.
fn picker_says_on(
    plane: &Plane,
    ran: &std::sync::Mutex<Vec<Vec<String>>>,
    machine: &purlis_core::sandbox::Machine,
    has: &dyn Fn(&str) -> bool,
) -> purlis_core::sandbox::Ahead {
    let set = profiles::current(plane.root());
    let profile = set.get("work").expect("declared");
    let probe = |words: &[String]| {
        ran.lock().unwrap().push(words.to_vec());
        Some("2.1.288 (Claude Code)".to_owned())
    };
    start::sandbox_ahead(profile, plane.root(), machine, has, "", Some(&probe)).expect("a harness")
}

/// What the start answers for `work` on the machine [`picker_says`] asks about, so the two are
/// compared on one machine whatever the one the test runs on can apply.
fn start_on_a_mac(plane: &Plane) -> Result<start::Ready, String> {
    start::ready_on(
        &plane.start("work"),
        plane.root(),
        &purlis_core::harness_declaration::read(plane.root()),
        &a_mac(),
        &|_| true,
    )
}

/// The picker gives the start's FIRST refusal: on a machine that cannot apply the sandbox, the
/// start refuses for the machine before it looks at the program, and so does the picker — even
/// for a program the check would refuse too.
#[test]
fn on_a_machine_that_cannot_sandbox_the_picker_and_the_start_say_the_same_first_refusal() {
    purlis_core::unsteered!();
    let plane = Plane::new();
    let inside = plane.root().join("bin/claude");
    fs::create_dir_all(inside.parent().unwrap()).unwrap();
    fs::write(&inside, "#!/bin/sh\n").unwrap();
    fs::write(
        plane.root().join("charter.toml"),
        "[sandbox]\nmode = \"on\"\n",
    )
    .unwrap();
    plane.profile("claude", &inside, "");
    let linux = purlis_core::sandbox::Machine {
        os: purlis_core::sandbox::Os::Linux,
        ..a_mac()
    };
    let without_socat = |program: &str| program == "bwrap";
    let ran = std::sync::Mutex::new(Vec::new());

    let purlis_core::sandbox::Ahead::Refused { why, install } =
        picker_says_on(&plane, &ran, &linux, &without_socat)
    else {
        panic!("refused");
    };
    let started = start::ready_on(
        &plane.start("work"),
        plane.root(),
        &purlis_core::harness_declaration::read(plane.root()),
        &linux,
        &without_socat,
    );

    assert!(
        why.contains("socat is not installed"),
        "the machine first: {why}"
    );
    assert_eq!(started.err().as_deref(), Some(why.as_str()));
    assert!(install.is_none() || install.as_deref().is_some_and(|it| it.contains("socat")));
    assert!(ran.lock().unwrap().is_empty(), "nothing was run");
}

#[test]
fn the_picker_checks_an_approved_program_with_the_starts_own_check() {
    purlis_core::unsteered!();
    let outside = stand_in::NoChatWrites::new();
    let plane = Plane::new();
    let script = harness_outside(&plane, &outside, "claude");
    fs::write(
        plane.root().join("charter.toml"),
        "[sandbox]\nmode = \"on\"\n",
    )
    .unwrap();
    command_profile(
        &plane,
        "claude",
        &["/bin/sh".to_owned(), script.display().to_string()],
    );
    let ran = std::sync::Mutex::new(Vec::new());

    let said = picker_says(&plane, &ran);

    assert_eq!(said, purlis_core::sandbox::Ahead::Sandboxed);
    let real = Path::new("/bin/sh")
        .canonicalize()
        .unwrap()
        .display()
        .to_string();
    let ran = ran.lock().unwrap();
    assert_eq!(ran.len(), 1, "{ran:?}");
    assert_eq!(
        ran[0][0], real,
        "the resolved real file, as the start runs it"
    );
    assert_eq!(
        ran[0][1],
        script.display().to_string(),
        "the whole command, as the start checks it, not the program alone"
    );
}

#[test]
fn the_picker_runs_no_program_nobody_approved() {
    purlis_core::unsteered!();
    let outside = stand_in::NoChatWrites::new();
    let plane = Plane::new();
    let script = harness_outside(&plane, &outside, "claude");
    fs::write(
        plane.root().join("charter.toml"),
        "[sandbox]\nmode = \"on\"\n",
    )
    .unwrap();
    // Declared, and nobody approved it.
    plane.declares(&format!(
        "[harness.work]\nkind = \"claude\"\ncommand = [\"/bin/sh\", {:?}]\n",
        script.display().to_string()
    ));
    let ran = std::sync::Mutex::new(Vec::new());

    let said = picker_says(&plane, &ran);

    assert_eq!(said, purlis_core::sandbox::Ahead::Sandboxed);
    assert!(
        ran.lock().unwrap().is_empty(),
        "an unapproved program was run"
    );
}

/// The picker's gate is the start's own (`wiring::refusal`), not the approval alone: an
/// approved profile from a `charter.local.toml` git would carry is refused by the start, so
/// the picker does not run its program either.
#[test]
fn the_picker_runs_no_program_the_starts_gate_refuses() {
    purlis_core::unsteered!();
    let outside = stand_in::NoChatWrites::new();
    let plane = Plane::new();
    let script = harness_outside(&plane, &outside, "claude");
    fs::write(
        plane.root().join("charter.toml"),
        "[sandbox]\nmode = \"on\"\n",
    )
    .unwrap();
    command_profile(
        &plane,
        "claude",
        &["/bin/sh".to_owned(), script.display().to_string()],
    );
    // A repository that would carry `charter.local.toml`: nothing ignores it.
    let mut init = std::process::Command::new("git");
    init.args([
        "init",
        "-q",
        &format!(
            "--template={}",
            concat!(env!("CARGO_MANIFEST_DIR"), "/tests/support/git-template")
        ),
    ])
    .current_dir(plane.root())
    .env("GIT_CONFIG_GLOBAL", "/dev/null")
    .env("GIT_CONFIG_SYSTEM", "/dev/null");
    let out = purlis_core::forklock::output(&mut init).expect("git runs");
    assert!(out.status.success(), "{out:?}");
    let set = profiles::current(plane.root());
    let profile = set.get("work").expect("declared");
    assert!(
        purlis_core::profiletrust::approval_needed(plane.root(), profile).is_none(),
        "approved"
    );
    assert!(
        !start::may_check_program(profile, plane.root()),
        "the start refuses it"
    );
    let ran = std::sync::Mutex::new(Vec::new());

    let _ = picker_says(&plane, &ran);

    assert!(
        ran.lock().unwrap().is_empty(),
        "a program the start would refuse was run"
    );
}

#[test]
fn the_picker_shows_a_program_where_the_chat_can_write_as_the_starts_refusal() {
    purlis_core::unsteered!();
    let plane = Plane::new();
    let inside = plane.root().join("bin/claude");
    fs::create_dir_all(inside.parent().unwrap()).unwrap();
    fs::write(&inside, "#!/bin/sh\n").unwrap();
    let plane = {
        fs::write(
            plane.root().join("charter.toml"),
            "[sandbox]\nmode = \"on\"\n",
        )
        .unwrap();
        plane.profile("claude", &inside, "");
        plane
    };
    let ran = std::sync::Mutex::new(Vec::new());

    let purlis_core::sandbox::Ahead::Refused { why, install } = picker_says(&plane, &ran) else {
        panic!("refused");
    };

    assert!(
        matches!(
            start_on_a_mac(&plane),
            Err(ref refused) if *refused == why
        ),
        "the picker says what the start says: {why}"
    );
    assert_eq!(install, None);
    assert!(
        ran.lock().unwrap().is_empty(),
        "a refused program is not run"
    );
}

#[test]
fn the_picker_shows_a_relative_program_as_the_starts_refusal() {
    purlis_core::unsteered!();
    let plane = Plane::new();
    fs::write(
        plane.root().join("charter.toml"),
        "[sandbox]\nmode = \"on\"\n",
    )
    .unwrap();
    plane.profile("claude", Path::new("./bin/claude"), "");
    fs::create_dir_all(plane.root().join("bin")).unwrap();
    fs::write(plane.root().join("bin/claude"), "#!/bin/sh\n").unwrap();
    let ran = std::sync::Mutex::new(Vec::new());

    let said = picker_says(&plane, &ran);

    assert!(
        matches!(&said, purlis_core::sandbox::Ahead::Refused { .. }),
        "{said:?}"
    );
    assert!(
        ran.lock().unwrap().is_empty(),
        "a refused program is not run"
    );
}

/// D-88g, before the start: a command that names a file where the chat can write — a script
/// in a temp folder, run by `/bin/sh` — is shown as the start's own refusal, and nothing runs.
#[test]
fn the_picker_shows_a_command_word_where_the_chat_can_write_as_the_starts_refusal() {
    purlis_core::unsteered!();
    let temp = tempfile::tempdir().unwrap();
    let plane = Plane::new();
    let script = temp.path().join("claude.sh");
    fs::write(&script, "echo '2.1.288 (Claude Code)'\n").unwrap();
    fs::write(
        plane.root().join("charter.toml"),
        "[sandbox]\nmode = \"on\"\n",
    )
    .unwrap();
    command_profile(
        &plane,
        "claude",
        &["/bin/sh".to_owned(), script.display().to_string()],
    );
    let ran = std::sync::Mutex::new(Vec::new());

    let purlis_core::sandbox::Ahead::Refused { why, install } = picker_says(&plane, &ran) else {
        panic!("refused");
    };

    assert!(why.contains("this profile's command names"), "{why}");
    assert!(
        matches!(
            start_on_a_mac(&plane),
            Err(ref refused) if *refused == why
        ),
        "the picker says what the start says: {why}"
    );
    assert_eq!(install, None);
    assert!(ran.lock().unwrap().is_empty(), "nothing was run");
}

/// Round 10's every-path rule, before the start: a path embedded in a word — here a
/// `--import=file://` URL into the plane — is refused by the picker exactly as by the start.
#[test]
fn the_picker_refuses_a_path_embedded_in_a_word_as_the_start_does() {
    purlis_core::unsteered!();
    let outside = stand_in::NoChatWrites::new();
    let plane = Plane::new();
    let script = harness_outside(&plane, &outside, "claude");
    fs::write(
        plane.root().join("charter.toml"),
        "[sandbox]\nmode = \"on\"\n",
    )
    .unwrap();
    let embedded = format!("--import=file://{}/x.mjs", plane.root().display());
    command_profile(
        &plane,
        "claude",
        &["/bin/sh".to_owned(), script.display().to_string(), embedded],
    );
    let ran = std::sync::Mutex::new(Vec::new());

    let purlis_core::sandbox::Ahead::Refused { why, install } = picker_says(&plane, &ran) else {
        panic!("refused");
    };

    assert!(why.contains("this profile's command names"), "{why}");
    assert!(
        matches!(
            start_on_a_mac(&plane),
            Err(ref refused) if *refused == why
        ),
        "the picker refuses the word the start refuses: {why}"
    );
    assert_eq!(install, None);
    assert!(ran.lock().unwrap().is_empty(), "nothing was run");
}
