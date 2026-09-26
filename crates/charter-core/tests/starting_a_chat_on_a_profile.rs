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

use charter_core::harness::{Harness, SessionId};
use charter_core::profiles;
use charter_core::start::{self, Start};

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
            // The default every chat starts under, so the tests below describe the app as
            // it ships (ADR 0029).
            show_footer: false,
        }
    }
}

#[test]
fn a_chat_started_on_a_profile_runs_that_profiles_command_in_that_profiles_environment() {
    charter_core::unsteered!();
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
    charter_core::unsteered!();
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
    charter_core::unsteered!();
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
    charter_core::unsteered!();
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
        ready.env.iter().find(|(n, _)| n == "CHARTER_PERSONA"),
        Some(&("CHARTER_PERSONA".to_owned(), "steward".to_owned()))
    );
}

#[test]
fn a_chat_that_asked_for_charters_footer_carries_the_word_that_says_so() {
    charter_core::unsteered!();
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
            .find(|(n, _)| n == charter_core::start::FOOTER_ENV),
        Some(&(
            charter_core::start::FOOTER_ENV.to_owned(),
            charter_core::start::FOOTER_SHOW.to_owned()
        ))
    );
}

#[test]
fn a_chat_that_did_not_ask_carries_no_footer_variable_at_all() {
    charter_core::unsteered!();
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
            .any(|(n, _)| n == charter_core::start::FOOTER_ENV),
        "{:?}",
        ready.env
    );
}

#[test]
fn a_persona_this_plane_does_not_have_is_refused_rather_than_set_on_the_harness() {
    charter_core::unsteered!();
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
    charter_core::unsteered!();
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

    assert_eq!(of("CHARTER_HARNESS"), Some("claude-code"));
    assert_eq!(of("CHARTER_HARNESS_PROFILE"), Some("work"));
    assert_eq!(of("CHARTER_PERSONA"), Some("steward"));
}

#[test]
fn a_chat_whose_profile_is_gone_is_skipped_by_name_and_never_given_another() {
    charter_core::unsteered!();
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
    charter_core::unsteered!();
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
    charter_core::unsteered!();
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
    charter_core::unsteered!();
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
            .contains(&("CHARTER_HARNESS".to_owned(), "opencode".to_owned())),
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
    charter_core::unsteered!();
    // The shim rides in `OPENCODE_CONFIG_CONTENT`, so the command line is the profile's and
    // charter's own words, exactly.
    let plane = Plane::new();
    a_wrapper_profile(&plane, "opencode", &["oc-work"]);
    let shim = plane
        .root()
        .join("plugin")
        .join(charter_core::opencode::SHIM);
    fs::create_dir_all(shim.parent().unwrap()).unwrap();
    fs::write(
        &shim,
        charter_core::opencode::shim(charter_core::opencode::Arming::Session),
    )
    .unwrap();

    let ready = start::ready(&plane.start("work"), plane.root()).expect("it starts");
    let binary = plane.root().join("charter");
    let plugin = plane.root().join("plugin");
    let kit = charter_core::harness::Kit {
        binary: &binary,
        plugin: Some(&plugin),
    };
    let charter_core::harness::StateHooks::ThisSessionOnly { args, env, .. } =
        Harness::Opencode.state_hooks(kit, Some(plane.root()), &ready.plugins)
    else {
        panic!("opencode is armed");
    };
    assert_eq!(ready.command_line(args), ["oc-work"]);
    let config = env
        .iter()
        .find(|(name, _)| name == charter_core::opencode::CONFIG_ENV)
        .map(|(_, value)| value.clone())
        .expect("the shim is handed over");
    assert!(config.contains("opencode/charter.ts"), "{config}");
}

#[test]
fn an_opencode_profile_that_would_load_no_plugin_is_refused_where_the_chat_starts() {
    charter_core::unsteered!();
    let plane = Plane::new();
    a_wrapper_profile(&plane, "opencode", &["--pure"]);

    let why = start::ready(&plane.start("work"), plane.root()).expect_err("it refuses");

    assert!(why.contains("without charter's guard"), "{why}");
}

#[test]
fn the_persona_a_new_chat_starts_on_is_one_the_plane_actually_has() {
    charter_core::unsteered!();
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
    charter_core::unsteered!();
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
    charter_core::unsteered!();
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
    charter_core::unsteered!();
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
    charter_core::unsteered!();
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
        ready.env.iter().find(|(n, _)| n == "CHARTER_PERSONA"),
        Some(&("CHARTER_PERSONA".to_owned(), "devops".to_owned()))
    );
}

#[test]
fn the_shared_store_is_refused_as_a_persona_even_where_it_has_a_persona_md() {
    charter_core::unsteered!();
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
    charter_core::unsteered!();
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
    let kit = charter_core::harness::Kit {
        binary: &binary,
        plugin: Some(&plugin),
    };
    match harness.state_hooks(kit, Some(root), plugins) {
        charter_core::harness::StateHooks::ThisSessionOnly { args, .. } => args,
        charter_core::harness::StateHooks::None => panic!("{harness:?} is armed"),
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
    charter_core::unsteered!();
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
    charter_core::unsteered!();
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
    charter_core::unsteered!();
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
    charter_core::unsteered!();
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
            "charter@inline": true,
            "charter@charter": false,
            "charter-app@inline": false,
            "figma@official": false,
            "serena@official": true,
        })
    );
}

#[test]
fn a_codex_chat_is_started_with_no_plugin_choice_whatever_its_project_says() {
    charter_core::unsteered!();
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
    charter_core::unsteered!();
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
            "charter@inline": true,
            "charter@charter": false,
            "charter-app@inline": false,
            "figma@official": false,
            "humanizer@h": false,
            "serena@official": true,
        }),
        "the workspace overrides Shared, and Local overrides the workspace"
    );
    assert_eq!(
        enabled(plane.root().to_path_buf()),
        serde_json::json!({
            "charter@inline": true,
            "charter@charter": false,
            "charter-app@inline": false,
            "figma@official": true,
            "serena@official": true,
        }),
        "a chat outside the workspace is not handed its choices"
    );
}

#[test]
fn a_chat_started_in_no_directory_starts_in_the_plane_not_where_the_app_was_launched() {
    charter_core::unsteered!();
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
