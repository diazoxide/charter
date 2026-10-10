//! A Claude Code chat through purlis's proxy (#1665): both of the chat's ports in its
//! `--settings`, and every host it reached before still reached, now decided by the proxy.

use serde_json::json;

use super::grant::{Grants, What};
use super::hosts::Host;
use super::reach::Decision;
use super::{Compiled, Machine, Os, Plane, applied_of, claude, compiler};
use crate::harness::Harness;

/// Open hosts (a preset and the project's own, a private address and port among them) and a
/// persona's hosts, which the person on this machine allowed.
const PROJECT: &str = "[sandbox]\nmode = \"on\"\negress = [\"forge\"]\n\
                       hosts = [\"10.100.39.145:6443\", \"*.internal.example\"]\n\n\
                       [sandbox.personas.devops]\n\
                       hosts = [\"db.example.net:5432\", \"*.ops.example\"]\n";

fn machine() -> Machine {
    Machine {
        env: crate::secrets::Env::of(&[]),
        home: Some(std::path::PathBuf::from("/home/op")),
        os: Os::MacOs,
    }
}

/// `PROJECT` compiled at `root` for a chat running as `persona`, with `chat`'s own grants.
fn compiled_at(root: &std::path::Path, persona: Option<&str>, chat: &Grants) -> Compiled {
    super::grant::TEST_HARNESS_TEMP.with(|roots| roots.set(&[]));
    let plane = Plane::of(Some(PROJECT));
    let policy = plane.said().policy.expect("on");
    super::persona::allow_every_as_listed(root, &policy);
    Compiled::granted(&policy, &plane, root, &machine(), persona, chat)
}

/// Claude Code's `network.allowedDomains`: what its own proxy let a chat reach before #1665.
fn allowed_domains(compiled: &Compiled) -> Vec<String> {
    claude::settings(compiled).expect("compiles").sandbox["network"]["allowedDomains"]
        .as_array()
        .expect("a list")
        .iter()
        .map(|host| host.as_str().expect("text").to_owned())
        .collect()
}

/// `hosts`, in one order: the proxy keeps them in the order its layers decide.
fn sorted(mut hosts: Vec<String>) -> Vec<String> {
    hosts.sort();
    hosts
}

/// One concrete host and port each entry of Claude Code's list let a command reach: a
/// wildcard's sub-domain, and a host with no port on 443.
fn reached_by(entry: &str) -> (String, u16) {
    let (name, port) = super::egress::entry_parts(entry);
    let port = port.unwrap_or(443);
    match name.strip_prefix("*.") {
        Some(domain) => (format!("api.{domain}"), port),
        None => (name.to_owned(), port),
    }
}

/// The proxy's answer, for a connection to `host` on `port`, as purlis's proxy asks it for a
/// Claude Code chat ([`super::Applied::serving`]).
fn decided(compiled: &Compiled, host: &str, port: u16) -> Decision {
    compiled.reach.decide_on_any_port(host, port, &[])
}

#[test]
fn open_persona_and_chat_hosts_reach_through_purlis_s_proxy_what_claude_code_let_them_reach() {
    let root = tempfile::tempdir().expect("a project");
    let mut chat = Grants::default();
    chat.add(&What::Host(Host::parse("api.example.com").expect("a host")));
    let compiled = compiled_at(root.path(), Some("devops"), &chat);
    let before = allowed_domains(&compiled);
    // Every layer is in the list, so the regression covers each.
    for listed in [
        "github.com",
        "10.100.39.145:6443",
        "*.internal.example",
        "db.example.net:5432",
        "*.ops.example",
        "api.example.com",
    ] {
        assert!(before.contains(&listed.to_owned()), "{listed}: {before:?}");
    }
    // Exactly the same hosts: none dropped, none added.
    assert_eq!(sorted(compiled.reach.hosts()), sorted(before.clone()));
    for entry in &before {
        let (host, port) = reached_by(entry);
        let decision = decided(&compiled, &host, port);
        assert!(decision.carries(), "{entry} ({host}:{port}): {decision:?}");
    }
    // And by the layer that listed each.
    use super::reach::By;
    assert_eq!(decided(&compiled, "github.com", 443), Decision::Open);
    assert_eq!(decided(&compiled, "10.100.39.145", 6443), Decision::Open);
    assert_eq!(
        decided(&compiled, "db.example.net", 5432),
        Decision::Persona
    );
    assert_eq!(
        decided(&compiled, "api.example.com", 443),
        Decision::Allowed(By::Chat)
    );
    // What was not reached before is still not reached.
    assert_eq!(decided(&compiled, "db.example.net", 5433), Decision::Ask);
    assert_eq!(
        decided(&compiled, "unlisted.example.org", 443),
        Decision::Ask
    );
}

#[test]
fn a_chat_on_another_persona_reaches_none_of_its_hosts_through_the_proxy_either() {
    let root = tempfile::tempdir().expect("a project");
    let compiled = compiled_at(root.path(), Some("qa"), &Grants::default());
    let before = allowed_domains(&compiled);
    assert!(
        !before.contains(&"db.example.net:5432".to_owned()),
        "{before:?}"
    );
    assert_eq!(sorted(compiled.reach.hosts()), sorted(before.clone()));
    assert_eq!(decided(&compiled, "db.example.net", 5432), Decision::Ask);
}

/// A host you allowed on this machine (`charter.local.toml`, confirmed in Settings). Writes a
/// manifest, so it first runs on CI.
#[test]
fn a_host_you_allowed_here_reaches_through_the_proxy_as_it_did_before() {
    let root = tempfile::tempdir().expect("a project");
    std::fs::write(
        root.path().join("charter.local.toml"),
        "[sandbox]\nhosts = [\"[fd00::7]:8443\", \"build.example.org\"]\n",
    )
    .expect("charter.local.toml");
    super::local::confirm_host(root.path(), "[fd00::7]:8443").expect("confirmed");
    super::local::confirm_host(root.path(), "build.example.org").expect("confirmed");
    let compiled = compiled_at(root.path(), None, &Grants::default());
    let before = allowed_domains(&compiled);
    assert!(before.contains(&"[fd00::7]:8443".to_owned()), "{before:?}");
    assert!(
        before.contains(&"build.example.org".to_owned()),
        "{before:?}"
    );
    assert_eq!(sorted(compiled.reach.hosts()), sorted(before.clone()));
    assert_eq!(
        decided(&compiled, "build.example.org", 443),
        Decision::Allowed(super::reach::By::You)
    );
    assert!(decided(&compiled, "fd00::7", 8443).carries());
}

#[test]
fn through_purlis_s_proxy_a_claude_code_chat_names_both_ports_and_nothing_else_changes() {
    let root = tempfile::tempdir().expect("a project");
    let compiled = compiled_at(root.path(), None, &Grants::default());
    let settings = claude::settings(&compiled).expect("compiles");
    let through = settings.through([41_000, 41_001]);
    let network = &through.sandbox["network"];
    // Both, always: a port left out is Claude Code's own proxy, which purlis does not see.
    assert_eq!(network["httpProxyPort"], json!(41_000));
    assert_eq!(network["socksProxyPort"], json!(41_001));
    // The allowed domains stay: Claude Code restricts a command's network only while they are
    // set, and with the ports set they no longer decide anything.
    assert_eq!(
        network["allowedDomains"],
        settings.sandbox["network"]["allowedDomains"]
    );
    let mut rest = through.sandbox.clone();
    for key in ["httpProxyPort", "socksProxyPort"] {
        rest["network"]
            .as_object_mut()
            .expect("a network object")
            .remove(key);
    }
    assert_eq!(rest, settings.sandbox);
    assert_eq!(through.deny, settings.deny);
}

/// A chat whose commands could connect to any loopback port would reach every local service,
/// another chat's proxy port among them (#1664's port identity rests on that never happening).
/// Claude Code's sandbox allows that exactly when a settings source turns `allowLocalBinding`
/// on, so purlis names it off, which outranks every other source but an administrator's.
#[test]
fn no_settings_source_lets_a_claude_code_chat_connect_to_other_local_ports() {
    let root = tempfile::tempdir().expect("a project");
    let compiled = compiled_at(root.path(), None, &Grants::default());
    let settings = claude::settings(&compiled).expect("compiles");
    assert_eq!(
        settings.sandbox["network"]["allowLocalBinding"],
        json!(false)
    );
    let through = settings.through([41_000, 41_001]);
    assert_eq!(
        through.sandbox["network"]["allowLocalBinding"],
        json!(false)
    );
}

#[test]
fn claude_code_s_version_is_read_from_its_answer() {
    use super::program::claude_code_version;
    assert_eq!(
        claude_code_version("2.1.296 (Claude Code)"),
        Some((2, 1, 296))
    );
    assert_eq!(
        claude_code_version("some banner\n  2.1.285 (Claude Code)\n"),
        Some((2, 1, 285))
    );
    assert_eq!(claude_code_version("2.1 (Claude Code)"), None);
    assert_eq!(claude_code_version("2.1.296"), None);
}

/// The Applied a Claude Code chat at `root` starts under, before its program answered.
fn applied_at(root: &std::path::Path) -> super::Applied {
    let compiled = compiled_at(root, None, &Grants::default());
    applied_of(
        Harness::ClaudeCode,
        compiler(Harness::ClaudeCode).expect("a compiler"),
        &compiled,
        root,
        &machine(),
    )
    .expect("compiles")
}

#[test]
fn only_a_claude_code_that_takes_the_ports_goes_through_purlis_s_proxy() {
    let root = tempfile::tempdir().expect("a project");
    let mut applied = applied_at(root.path());
    // Until it answered, it keeps its own proxy.
    assert!(!applied.through_purlis_proxy());
    applied.answered("2.1.285 (Claude Code)");
    assert!(applied.through_purlis_proxy());
    assert_eq!(applied.older_notice(), None);
    let mut newer = applied_at(root.path());
    newer.answered("2.2.0 (Claude Code)");
    assert!(newer.through_purlis_proxy());
    // An answer it cannot read keeps it on its own proxy, as an older one does.
    let mut unread = applied_at(root.path());
    unread.answered("something else");
    assert!(!unread.through_purlis_proxy());
}

#[test]
fn an_older_claude_code_keeps_its_own_proxy_and_hosts_and_says_so_once() {
    let root = tempfile::tempdir().expect("a project");
    let mut applied = applied_at(root.path());
    applied.answered("2.1.184 (Claude Code)");
    let said = applied.older_notice().expect("said the first time");
    assert!(!applied.through_purlis_proxy());
    assert!(said.contains("2.1.184"), "{said}");
    assert!(said.contains("2.1.285"), "{said}");
    for word in ["egress", "grant", "allowlist"] {
        assert!(!said.to_lowercase().contains(word), "{word}: {said}");
    }
    // Its settings are as they were: the allowed domains, and no port of purlis's.
    let super::Form::ClaudeCode(settings) = applied.form() else {
        panic!("compiled for Claude Code");
    };
    assert!(settings.sandbox["network"].get("httpProxyPort").is_none());
    assert!(settings.sandbox["network"].get("socksProxyPort").is_none());
    // Said once: the next chat on the same Claude Code says nothing.
    let mut again = applied_at(root.path());
    again.answered("2.1.184 (Claude Code)");
    assert_eq!(again.older_notice(), None);
    assert!(!again.through_purlis_proxy());
}

/// The chat's pair of ports, made when it starts. Binds two loopback ports, so it first runs
/// on CI.
#[test]
fn a_claude_code_chat_through_purlis_s_proxy_is_given_its_own_pair_of_ports() {
    let root = tempfile::tempdir().expect("a project");
    let mut applied = applied_at(root.path());
    assert!(applied.confine().expect("nothing to start").is_none());
    applied.answered("2.1.296 (Claude Code)");
    let one = applied.confine().expect("started").expect("a proxy");
    let two = applied.confine().expect("started").expect("a proxy");
    let [http, socks] = one.proxy_ports();
    assert_ne!(http, socks);
    assert!(
        two.proxy_ports()
            .iter()
            .all(|port| ![http, socks].contains(port))
    );
}

/// The `--settings` a chat is armed with, carrying `settings` as compiled.
fn armed_with(settings: &claude::Settings) -> Vec<String> {
    let blob = json!({
        "enabledPlugins": {},
        "permissions": { "allow": [], "deny": settings.deny },
        "sandbox": settings.sandbox,
    });
    vec!["--settings".to_owned(), blob.to_string()]
}

fn settings_in(armed: &[String]) -> serde_json::Value {
    serde_json::from_str(&armed[1]).expect("JSON")
}

#[test]
fn the_line_names_the_chat_s_own_ports_beside_its_hook_socket() {
    use crate::harness::claude::reporting_on;
    let root = tempfile::tempdir().expect("a project");
    let compiled = compiled_at(root.path(), None, &Grants::default());
    let settings = claude::settings(&compiled).expect("compiles");
    let armed = armed_with(&settings);

    // Ports alone.
    let line =
        reporting_on(armed.clone(), &settings, None, Some([41_000, 41_001])).expect("starts");
    assert_eq!(
        settings_in(&line)["sandbox"],
        settings.through([41_000, 41_001]).sandbox
    );

    // Ports and the hook socket.
    let sockets = tempfile::tempdir().expect("a folder");
    let socket = sockets.path().join("hooks.sock");
    let line = reporting_on(
        armed.clone(),
        &settings,
        Some(&socket),
        Some([41_000, 41_001]),
    )
    .expect("starts");
    let network = &settings_in(&line)["sandbox"]["network"];
    assert_eq!(network["httpProxyPort"], json!(41_000));
    assert_eq!(network["socksProxyPort"], json!(41_001));
    assert_eq!(
        network["allowUnixSockets"],
        json!([super::real(&socket).display().to_string()])
    );

    // Neither: as armed.
    assert_eq!(
        reporting_on(armed.clone(), &settings, None, None).expect("starts"),
        armed
    );
}

#[test]
fn ports_are_never_added_to_a_sandbox_the_adapter_did_not_compile() {
    use crate::harness::claude::reporting_on;
    let root = tempfile::tempdir().expect("a project");
    let compiled = compiled_at(root.path(), None, &Grants::default());
    let settings = claude::settings(&compiled).expect("compiles");
    for armed in [
        Vec::new(),
        vec!["--settings".to_owned(), "not json".to_owned()],
        vec![
            "--settings".to_owned(),
            r#"{"sandbox":{"enabled":false}}"#.to_owned(),
        ],
    ] {
        let why = reporting_on(armed.clone(), &settings, None, Some([41_000, 41_001]))
            .expect_err("refused");
        assert!(why.contains("nothing was started"), "{armed:?}: {why}");
    }
}

/// Claude Code's own proxy let a command reach a host listed without a port on any port (git
/// over ssh to github.com among them); through purlis's proxy it still does, and a host listed
/// with one still on that one alone.
#[test]
fn a_host_listed_without_a_port_reaches_every_port_as_it_did_before() {
    let root = tempfile::tempdir().expect("a project");
    let mut applied = applied_at(root.path());
    applied.answered("2.1.296 (Claude Code)");
    let serving = applied.serving(super::egress::Refusals::default(), None);
    assert!(serving.any_port);
    let compiled = compiled_at(root.path(), None, &Grants::default());
    for port in [22, 443, 80, 9418] {
        assert_eq!(
            decided(&compiled, "github.com", port),
            Decision::Open,
            "{port}"
        );
    }
    assert_eq!(decided(&compiled, "10.100.39.145", 6443), Decision::Open);
    assert_eq!(decided(&compiled, "10.100.39.145", 22), Decision::Ask);
    // The local-address check is the same on every port.
    for port in [22, 443, 41_000] {
        assert_eq!(
            decided(&compiled, "127.0.0.1", port),
            Decision::Refused(super::reach::Refused::LocalAddress)
        );
    }
}

#[test]
fn a_wrapped_chat_s_proxy_keeps_its_default_ports() {
    let root = tempfile::tempdir().expect("a project");
    let compiled = compiled_at(root.path(), None, &Grants::default());
    let applied = applied_of(
        Harness::Opencode,
        compiler(Harness::Opencode).expect("a compiler"),
        &compiled,
        root.path(),
        &machine(),
    )
    .expect("compiles");
    assert!(applied.through_purlis_proxy());
    assert!(
        !applied
            .serving(super::egress::Refusals::default(), None)
            .any_port
    );
}

/// Fail closed: a Claude Code chat that goes through purlis's proxy never opens without its
/// ports, which would leave it on Claude Code's own proxy, out of the record.
#[test]
fn a_claude_code_chat_through_the_proxy_does_not_open_without_its_ports() {
    let dir = tempfile::tempdir().expect("a project");
    // As the kernel names it: no chat opens in a folder reached through a link.
    let root = dir.path().canonicalize().expect("real");
    let cwd = root.join("w");
    std::fs::create_dir_all(&cwd).expect("a folder");
    let mut applied = applied_at(&root);
    applied.answered("2.1.296 (Claude Code)");
    let super::Form::ClaudeCode(settings) = applied.form_in(Some(&cwd)) else {
        panic!("compiled for Claude Code");
    };
    let why = applied
        .line(
            super::Words {
                program: "claude".to_owned(),
                command: Vec::new(),
                armed: armed_with(&settings),
                charters: Vec::new(),
            },
            &super::At {
                cwd: Some(&cwd),
                hook_socket: None,
                confinement: None,
                no_opt_out: false,
            },
        )
        .expect_err("refused");
    assert!(why.contains("network proxy was not started"), "{why}");
}
