//! OB-15 (#687): every third-party call a feature makes is listed in the local network log, and
//! a no-account run lists no Charter address but the updater's.
//!
//! The forge client (FW-2a) is the feature the acceptance names. Its two transports are both
//! checked: the native one against a recorded forge on loopback, and the CLI one against the
//! stand-in `gh` and `glab` (`support/forge_cli.rs`). Both run in a child of this binary with a
//! `HOME` of its own, so the network log lands in a scratch machine store and never in the
//! operator's.
//!
//! What a line may hold is checked as well as what it must: never a token, a query, a body or
//! the names in a path.

mod support;

use std::sync::Arc;

use charter_core::forge::http::{ApiRoot, TokenSource};
use charter_core::forge::route::{HostScope, Resolver, SignIn};
use charter_core::forge::{Account, Caller, ForgeError, Kind, Owner};
use charter_core::netlog::{self, Feature, Party, Via};
use secrecy::SecretString;
use support::forge_cli::{Scene, in_a_child, in_child};
use wiremock::matchers::any;
use wiremock::{Mock, MockServer, ResponseTemplate};

const CANARY: &str = "CANARYnetlogToken91fe";

struct Canary;
impl TokenSource for Canary {
    fn token(&self) -> Result<SecretString, ForgeError> {
        Ok(SecretString::from(CANARY))
    }
}

fn account() -> Account {
    Account {
        kind: Kind::GitHub,
        host: "github.com".into(),
        login: "octocat".into(),
    }
}

/// Every line in this run's network log, as text.
fn log_text() -> String {
    let root = charter_core::machine::config_root().expect("a config root");
    let dir = netlog::dir(&root);
    let mut text = String::new();
    for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
        text.push_str(&std::fs::read_to_string(entry.path()).unwrap_or_default());
    }
    text
}

fn entries() -> Vec<netlog::Entry> {
    netlog::entries(&charter_core::machine::config_root().expect("a config root"))
}

#[test]
fn every_forge_call_is_listed_in_a_child_with_a_store_of_its_own() {
    charter_core::unsteered!();
    in_a_child("listed::", "bin");
}

mod listed {
    use super::*;

    #[test]
    fn a_native_forge_call_is_listed_with_its_host_method_path_and_status() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let server = rt.block_on(MockServer::start());
        rt.block_on(
            Mock::given(any())
                .respond_with(ResponseTemplate::new(200).set_body_string("[]"))
                .mount(&server),
        );
        let resolver = Resolver::new(Kind::GitHub, "github.com")
            .at_root(ApiRoot::at(&server.uri()))
            .signed_in(
                &HostScope::for_a_test(),
                account(),
                SignIn {
                    tokens: Arc::new(Canary),
                    imported_from_cli: false,
                },
            );
        let backend =
            charter_core::forge::Forge::default_of(Kind::GitHub).backend_for(Arc::new(resolver));
        let me = Caller::window().as_account(account());
        let _ = backend.owned(&me, &Owner::new("secret-org-name"));

        let authority = server.uri().trim_start_matches("http://").to_string();
        let native: Vec<_> = entries()
            .into_iter()
            .filter(|e| e.via == Via::Https && e.host == authority)
            .collect();
        assert!(
            !native.is_empty(),
            "the native call is not listed:\n{}",
            log_text()
        );
        let one = &native[0];
        assert_eq!(one.feature, Feature::Forge);
        assert_eq!(one.to, Party::ThirdParty);
        assert_eq!(one.method, "GET");
        assert_eq!(one.status, Some(200));
        assert!(
            !one.path.contains("secret-org-name") && !one.path.contains('?'),
            "the path keeps a name or a query: {}",
            one.path
        );
        let text = log_text();
        assert!(!text.contains(CANARY), "the network log holds the token");
        assert!(
            !text.contains("secret-org-name"),
            "the network log names the owner"
        );
    }

    #[test]
    fn a_native_gitlab_call_is_listed_with_its_nested_group_masked() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let server = rt.block_on(MockServer::start());
        rt.block_on(
            Mock::given(any())
                .respond_with(ResponseTemplate::new(200).set_body_string("[]"))
                .mount(&server),
        );
        let account = Account {
            kind: Kind::GitLab,
            host: "gitlab.com".into(),
            login: "octocat".into(),
        };
        let resolver = Resolver::new(Kind::GitLab, "gitlab.com")
            .at_root(ApiRoot::at(&server.uri()))
            .signed_in(
                &HostScope::for_a_test(),
                account.clone(),
                SignIn {
                    tokens: Arc::new(Canary),
                    imported_from_cli: false,
                },
            );
        let backend =
            charter_core::forge::Forge::default_of(Kind::GitLab).backend_for(Arc::new(resolver));
        let me = Caller::window().as_account(account);
        let _ = backend.owned(&me, &Owner::new("secret-group/api"));

        let authority = server.uri().trim_start_matches("http://").to_string();
        let native: Vec<_> = entries()
            .into_iter()
            .filter(|e| e.via == Via::Https && e.host == authority)
            .collect();
        assert_eq!(native.len(), 1, "{native:?}\n{}", log_text());
        let one = &native[0];
        assert_eq!(
            (one.feature, one.to, one.method.as_str(), one.status),
            (Feature::Forge, Party::ThirdParty, "GET", Some(200))
        );
        assert_eq!(one.path, "groups/{}/projects");
        let text = log_text();
        assert!(!text.contains(CANARY), "the network log holds the token");
        assert!(
            !text.contains("secret-group"),
            "the network log names the group"
        );
    }

    #[test]
    fn a_forge_call_through_the_cli_is_listed_with_its_host() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let scene = Scene::new("netlog-cli.example");
        scene.gh_api("orgs/hidden-team/repos?per_page=100&page=1", 0, "[]", "");
        let _ = scene
            .forge("github")
            .backend()
            .owned(&Caller::command(), &Owner::new("hidden-team"));

        let listed: Vec<_> = entries()
            .into_iter()
            .filter(|e| e.host == "netlog-cli.example")
            .collect();
        assert_eq!(listed.len(), 1, "{listed:?}\n{}", log_text());
        let one = &listed[0];
        assert_eq!(
            (one.feature, one.via, one.to),
            (Feature::Forge, Via::Gh, Party::ThirdParty)
        );
        assert_eq!(one.path, "orgs/{}/repos");
        assert!(!log_text().contains("hidden-team"));
    }

    #[test]
    fn a_no_account_run_lists_no_charter_address_but_the_updaters() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        // The forge work a run does with no account: an auth check and a listing through the
        // CLI, and a listing through the native client. The updater's own check is the app's,
        // and `updates::tests::the_real_check_lists_its_read_in_the_network_log` drives it.
        let scene = Scene::new("netlog-run.example");
        scene.answers(
            "gh",
            &["auth", "status", "--hostname", "netlog-run.example"],
            0,
            "",
            "",
        );
        scene.gh_api("orgs/acme/repos?per_page=100&page=1", 0, "[]", "");
        let forge = scene.forge("github");
        let _ = forge.check_auth();
        let _ = forge
            .backend()
            .owned(&Caller::command(), &Owner::new("acme"));

        let all: Vec<_> = entries()
            .into_iter()
            .filter(|e| e.host == "netlog-run.example")
            .collect();
        assert_eq!(all.len(), 2, "the run is not listed whole: {all:?}");
        // By host, not by path: a template keeps no names, so the host is what says where a
        // call went. Charter's files and tracker are on GitHub's hosts, so any line to one that
        // is not the updater's is a call to Charter this run had no business making.
        for e in entries() {
            assert!(
                !is_githubs(&e.host) || e.feature == Feature::Updater,
                "a run without an account reached a GitHub host: {e:?}"
            );
            assert_ne!(e.to, Party::Charter, "{e:?}");
        }
    }
}

/// GitHub's hosts, where Charter's release files and tracker are.
fn is_githubs(host: &str) -> bool {
    let host = host.split(':').next().unwrap_or_default();
    host == "github.com"
        || host == "api.github.com"
        || host == "githubusercontent.com"
        || host.ends_with(".githubusercontent.com")
        || host.ends_with(".github.com")
}

#[test]
fn githubs_hosts_are_told_from_the_rest() {
    charter_core::unsteered!();
    for host in [
        "github.com",
        "api.github.com",
        "objects.githubusercontent.com",
        "github.com:443",
    ] {
        assert!(is_githubs(host), "{host}");
    }
    for host in [
        "netlog-run.example",
        "127.0.0.1:8080",
        "notgithub.com",
        "ghe.example.com",
    ] {
        assert!(!is_githubs(host), "{host}");
    }
}
