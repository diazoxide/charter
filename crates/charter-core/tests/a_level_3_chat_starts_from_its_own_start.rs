//! A chat at level 3 starts from the same start as a terminal chat (ADR 0080 §1, HP-2): the one
//! profile gate and the one sandbox decision ([`charter_core::start::ready`]), then
//! [`charter_core::acp::Launch::for_start`], which says what runs as the ACP agent, in which
//! environment, or why it does not run over ACP.
#![cfg(unix)]

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use charter_core::acp::{Host, Launch, NotOffered};
use charter_core::profiles;
use charter_core::start::{self, Start};

struct Plane {
    dir: tempfile::TempDir,
}

impl Plane {
    fn new(charter_toml: &str) -> Self {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("charter.toml"), charter_toml).unwrap();
        Self { dir }
    }

    fn root(&self) -> &Path {
        self.dir.path()
    }

    /// Declares profile `work` of `kind` running `command`, and approves it.
    fn profile(&self, kind: &str, command: &[String]) {
        let words: Vec<String> = command.iter().map(|word| format!("{word:?}")).collect();
        fs::write(
            self.root().join(profiles::LOCAL_FILE),
            format!(
                "[harness.work]\nkind = {kind:?}\ncommand = [{}]\n",
                words.join(", ")
            ),
        )
        .unwrap();
        let set = profiles::current(self.root());
        let profile = set.get("work").expect("declared");
        charter_core::profiletrust::record_launched(
            self.root(),
            "work",
            &charter_core::profiletrust::fingerprint(profile),
        )
        .unwrap();
    }

    fn start(&self) -> Start {
        Start {
            profile: Some("work".to_owned()),
            name: "ide.7".to_owned(),
            cwd: Some(self.root().to_path_buf()),
            ..Start::default()
        }
    }
}

/// A program outside every plane, which nothing here runs.
fn program(outside: &stand_in::NoChatWrites, name: &str) -> String {
    stand_in::program(outside.path(), name, "#!/bin/sh\nexit 0\n")
        .display()
        .to_string()
}

fn os(pairs: &[(&str, &str)]) -> Vec<(OsString, OsString)> {
    pairs
        .iter()
        .map(|(name, value)| ((*name).into(), (*value).into()))
        .collect()
}

/// The host's side, as a host fills it in: the app's own environment, the chat's own variables,
/// and the `charter` binary whose MCP server the session is handed.
fn host<'a>(strip: &'a [String]) -> Host<'a> {
    Host {
        chat: "7".to_owned(),
        app_env: os(&[
            ("PATH", "/usr/bin:/bin"),
            ("HOME", "/home/me"),
            ("SOME_TOOL_SETTING", "the app's"),
            ("OP_SESSION_me", "a vault session"),
            ("VAULT_TOKEN_NAME", "a declared identity"),
        ]),
        operator: &[],
        strip,
        own: vec![("CHARTER_SESSION_ID".to_owned(), "7".to_owned())],
        git_hooks: None,
        charter: Some(PathBuf::from("/app/charter")),
    }
}

fn named(env: &[(OsString, OsString)]) -> Vec<String> {
    env.iter()
        .map(|(name, _)| name.to_string_lossy().into_owned())
        .collect()
}

#[test]
fn an_opencode_chat_runs_its_profile_s_program_as_its_acp_agent_in_the_chat_s_environment() {
    charter_core::unsteered!();
    let outside = stand_in::NoChatWrites::new();
    let opencode = program(&outside, "opencode");
    let plane = Plane::new("");
    plane.profile("opencode", std::slice::from_ref(&opencode));
    let ready = start::ready(&plane.start(), plane.root()).expect("it starts");
    let strip = ["VAULT_TOKEN_NAME".to_owned()];

    let launch = Launch::for_start(&ready, host(&strip)).expect("offered at level 3");

    assert_eq!(launch.argv, [opencode, "acp".to_owned()]);
    assert_eq!(launch.cwd, plane.root());
    assert_eq!(launch.chat, "7");
    assert_eq!(launch.charter_mcp, Some(PathBuf::from("/app/charter")));
    let names = named(&launch.env);
    assert!(names.contains(&"PATH".to_owned()), "{names:?}");
    assert!(names.contains(&"HOME".to_owned()), "{names:?}");
    for kept_out in ["SOME_TOOL_SETTING", "OP_SESSION_me", "VAULT_TOKEN_NAME"] {
        assert!(
            !names.contains(&kept_out.to_owned()),
            "{kept_out}: {names:?}"
        );
    }
    // The chat's own, from its start: charter's variables, as a terminal chat has them.
    for (name, value) in &ready.env {
        assert!(
            launch
                .env
                .contains(&(name.as_str().into(), value.as_str().into())),
            "{name} is the chat's: {names:?}"
        );
    }
    // And what the host adds for the chat itself, last, so nothing before it wins.
    assert_eq!(
        launch.env.last(),
        Some(&("CHARTER_SESSION_ID".into(), "7".into()))
    );
}

#[test]
fn a_wrapper_profile_keeps_its_own_words_before_the_acp_word() {
    charter_core::unsteered!();
    // ADR 0022: a profile's command is commonly a wrapper, which reads its own words first.
    let outside = stand_in::NoChatWrites::new();
    let wrapper = program(&outside, "ocw");
    let plane = Plane::new("");
    plane.profile("opencode", &[wrapper.clone(), "work".to_owned()]);
    let ready = start::ready(&plane.start(), plane.root()).expect("it starts");

    let launch = Launch::for_start(&ready, host(&[])).expect("offered at level 3");

    assert_eq!(launch.argv, [wrapper, "work".to_owned(), "acp".to_owned()]);
}

#[test]
fn a_harness_charter_runs_over_acp_nowhere_yet_starts_in_its_terminal_and_says_why() {
    charter_core::unsteered!();
    let outside = stand_in::NoChatWrites::new();
    let codex = program(&outside, "codex");
    let plane = Plane::new("");
    plane.profile("codex", &[codex]);
    let ready = start::ready(&plane.start(), plane.root()).expect("it starts");

    let not = Launch::for_start(&ready, host(&[])).expect_err("not at level 3");

    assert_eq!(not, NotOffered::NoAgent("codex".to_owned()));
    assert_eq!(
        not.to_string(),
        "charter runs no ACP agent for a codex chat yet, so it starts in its terminal"
    );
}

/// The message every refusal in a sandboxed project gives: it starts in its terminal, and it
/// never offers running unconfined as the way to get ACP.
const SANDBOXED: &str = "this project turns the sandbox on, and charter cannot sandbox a chat over ACP yet, so it starts in its terminal, where its sandbox is shown for as long as it runs";

/// D-87h: a level-3 start goes through the same sandbox decision as a terminal start, and a
/// sandboxed project refuses level 3 until charter can wrap a level-3 agent. opencode is wrapped
/// on macOS only; elsewhere its sandboxed start is refused before this is asked.
#[cfg(target_os = "macos")]
#[test]
fn a_sandboxed_chat_does_not_run_over_acp_and_starts_in_its_terminal() {
    charter_core::unsteered!();
    let outside = stand_in::NoChatWrites::new();
    let opencode = program(&outside, "opencode");
    let plane = Plane::new("[sandbox]\nmode = \"on\"\n");
    plane.profile("opencode", &[opencode]);
    let ready = start::ready(&plane.start(), plane.root()).expect("it starts sandboxed");
    assert!(ready.sandbox.is_some(), "{ready:?}");

    let not = Launch::for_start(&ready, host(&[])).expect_err("not at level 3");

    assert_eq!(not, NotOffered::Sandboxed);
    assert_eq!(not.to_string(), SANDBOXED);
}

#[test]
fn a_chat_a_person_started_without_the_sandbox_does_not_run_over_acp_either() {
    charter_core::unsteered!();
    // D-87h refuses level 3 in a sandboxed PROJECT, whatever the chat. A person's opt-out
    // (ADR 0067 §7, V78a) needs its unsandboxed badge shown for the chat's whole life, and a
    // chat at level 3 has no terminal to show it on (V24a), so it is a terminal chat.
    let outside = stand_in::NoChatWrites::new();
    let opencode = program(&outside, "opencode");
    let plane = Plane::new("[sandbox]\nmode = \"on\"\n");
    plane.profile("opencode", std::slice::from_ref(&opencode));
    let start = Start {
        without_sandbox: Some(charter_core::sandbox::OptOut::default()),
        ..plane.start()
    };
    let ready = start::ready(&start, plane.root()).expect("it starts unsandboxed");
    assert!(ready.unsandboxed.is_some(), "{ready:?}");

    let not = Launch::for_start(&ready, host(&[])).expect_err("not at level 3");

    assert_eq!(not, NotOffered::Sandboxed);
    assert_eq!(not.to_string(), SANDBOXED);
}
