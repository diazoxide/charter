//! The native transport's own behaviour (ADR 0070 §3 and §5, FW-2a), against a recorded forge
//! over HTTP: a repeated read is conditional and a `304` is answered from the ETag store, the
//! store is the machine tier's and outlives the process, a refused token is reported and never
//! retried on the CLI, and no redirect is followed with the token.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use charter_core::forge::http::{ApiRoot, TokenSource};
use charter_core::forge::pr::{Pr, State};
use charter_core::forge::route::{HostScope, Resolver, SignIn};
use charter_core::forge::transport::{Call, NoAnswer, Reply, Transport};
use charter_core::forge::{Account, Caller, Failure, Forge, ForgeBackend, ForgeError, Kind};
use secrecy::SecretString;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const TOKEN: &str = "nativeTransportToken91c2";

struct Fixed;
impl TokenSource for Fixed {
    fn token(&self) -> Result<SecretString, ForgeError> {
        Ok(SecretString::from(TOKEN))
    }
}

/// A CLI route that counts how often it is asked, and never answers.
#[derive(Default)]
struct Counting(AtomicUsize);
impl Transport for Counting {
    fn send(&self, _forge: &Forge, _call: &Call) -> Result<Reply, NoAnswer> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Err(NoAnswer::Missing("the CLI was asked".into()))
    }
    fn check_auth(&self, _forge: &Forge) -> Result<(), ForgeError> {
        Ok(())
    }
}

fn account() -> Account {
    Account {
        kind: Kind::GitHub,
        host: "github.com".into(),
        login: "octocat".into(),
    }
}

fn human() -> Caller {
    Caller::window().as_account(account())
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

fn resolver(server: &MockServer, cli: Arc<Counting>) -> Resolver {
    Resolver::new(Kind::GitHub, "github.com")
        .at_root(ApiRoot::at(&server.uri()))
        .cli_over(cli)
        .signed_in(
            &HostScope::for_a_test(),
            account(),
            SignIn {
                tokens: Arc::new(Fixed),
                imported_from_cli: false,
            },
        )
}

fn backend(resolver: Resolver) -> Box<dyn ForgeBackend> {
    Forge::default_of(Kind::GitHub).backend_for(Arc::new(resolver))
}

fn pr() -> Pr {
    Pr {
        number: 7,
        url: String::new(),
    }
}

/// `/repos/o/r/pulls/7` answered once with ETag `"v1"`, and then only conditionally, with a
/// `304`.
fn mount_conditional(rt: &tokio::runtime::Runtime, server: &MockServer) {
    // Mounted first, so the conditional read meets the 304 and not the first answer.
    rt.block_on(
        Mock::given(method("GET"))
            .and(path("/repos/o/r/pulls/7"))
            .and(header("if-none-match", "\"v1\""))
            .respond_with(ResponseTemplate::new(304))
            .expect(1)
            .mount(server),
    );
    rt.block_on(
        Mock::given(method("GET"))
            .and(path("/repos/o/r/pulls/7"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(r#"{"state":"open","merged":false}"#)
                    .insert_header("etag", "\"v1\""),
            )
            .up_to_n_times(1)
            .expect(1)
            .mount(server),
    );
}

#[test]
fn a_second_read_is_conditional_and_a_304_is_answered_from_the_etag_store() {
    charter_core::unsteered!();
    let rt = runtime();
    let server = rt.block_on(MockServer::start());
    mount_conditional(&rt, &server);
    let github = backend(resolver(&server, Arc::default()));
    assert_eq!(github.state(&human(), "o/r", &pr()), Ok(State::Open));
    assert_eq!(github.state(&human(), "o/r", &pr()), Ok(State::Open));
    rt.block_on(server.verify());
}

#[test]
fn the_etag_store_is_the_machine_tiers_outlives_the_process_and_holds_no_token() {
    charter_core::unsteered!();
    let rt = runtime();
    let server = rt.block_on(MockServer::start());
    mount_conditional(&rt, &server);
    let config = tempfile::tempdir().unwrap();
    let fresh = || backend(resolver(&server, Arc::default()).etags_in(config.path()));
    assert_eq!(fresh().state(&human(), "o/r", &pr()), Ok(State::Open));
    // A second process: its own resolver, the same machine store.
    assert_eq!(fresh().state(&human(), "o/r", &pr()), Ok(State::Open));
    rt.block_on(server.verify());
    let store = config
        .path()
        .join("charter/forge-etags/native-github-github.com-octocat");
    let files: Vec<_> = std::fs::read_dir(&store).unwrap().flatten().collect();
    assert_eq!(files.len(), 1);
    let written = std::fs::read_to_string(files[0].path()).unwrap();
    assert!(!written.contains(TOKEN), "the store holds no credential");
}

#[test]
fn a_refused_token_is_auth_and_is_never_retried_through_the_cli() {
    charter_core::unsteered!();
    let rt = runtime();
    let server = rt.block_on(MockServer::start());
    rt.block_on(
        Mock::given(method("GET"))
            .and(path("/repos/o/r/pulls/7"))
            .respond_with(
                ResponseTemplate::new(401).set_body_string(r#"{"message":"Bad credentials"}"#),
            )
            .expect(1)
            .mount(&server),
    );
    let cli = Arc::new(Counting::default());
    let got = backend(resolver(&server, cli.clone())).state(&human(), "o/r", &pr());
    let error = got.unwrap_err();
    assert_eq!(error.failure(), &Failure::Auth);
    assert!(
        error.to_string().contains("Bad credentials (HTTP 401)"),
        "{error}"
    );
    assert!(!error.to_string().contains(TOKEN));
    assert_eq!(cli.0.load(Ordering::SeqCst), 0, "retried through the CLI");
    rt.block_on(server.verify());
}

#[test]
fn a_redirect_is_not_followed_with_the_token() {
    charter_core::unsteered!();
    let rt = runtime();
    let server = rt.block_on(MockServer::start());
    let elsewhere = rt.block_on(MockServer::start());
    rt.block_on(
        Mock::given(method("GET"))
            .and(path("/repos/o/r/pulls/7"))
            .respond_with(
                ResponseTemplate::new(302)
                    .insert_header("location", format!("{}/stolen", elsewhere.uri()).as_str()),
            )
            .expect(1)
            .mount(&server),
    );
    let got = backend(resolver(&server, Arc::default())).state(&human(), "o/r", &pr());
    assert!(got.is_err(), "{got:?}");
    let reached = rt
        .block_on(elsewhere.received_requests())
        .unwrap_or_default();
    assert!(reached.is_empty(), "the redirect was followed");
    rt.block_on(server.verify());
}

/// The same transport on GitLab (FW-2b): its paths, its weak ETags and its rate-limit headers.
mod gitlab {
    use super::*;

    const MR: &str = "/projects/acme%2Fapi/merge_requests/7";

    fn account() -> Account {
        Account {
            kind: Kind::GitLab,
            host: "gitlab.com".into(),
            login: "octocat".into(),
        }
    }

    fn human() -> Caller {
        Caller::window().as_account(account())
    }

    fn backend(server: &MockServer, cli: Arc<Counting>) -> Box<dyn ForgeBackend> {
        let resolver = Resolver::new(Kind::GitLab, "gitlab.com")
            .at_root(ApiRoot::at(&server.uri()))
            .cli_over(cli)
            .signed_in(
                &HostScope::for_a_test(),
                account(),
                SignIn {
                    tokens: Arc::new(Fixed),
                    imported_from_cli: false,
                },
            );
        Forge::default_of(Kind::GitLab).backend_for(Arc::new(resolver))
    }

    #[test]
    fn a_second_read_sends_gitlabs_weak_etag_back_and_a_304_is_answered_from_the_store() {
        charter_core::unsteered!();
        let rt = runtime();
        let server = rt.block_on(MockServer::start());
        rt.block_on(
            Mock::given(method("GET"))
                .and(path(MR))
                .and(header("if-none-match", "W/\"a1b2\""))
                .respond_with(ResponseTemplate::new(304))
                .expect(1)
                .mount(&server),
        );
        rt.block_on(
            Mock::given(method("GET"))
                .and(path(MR))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_string(r#"{"iid":7,"state":"opened"}"#)
                        .insert_header("etag", "W/\"a1b2\""),
                )
                .up_to_n_times(1)
                .expect(1)
                .mount(&server),
        );
        let gitlab = backend(&server, Arc::default());
        assert_eq!(gitlab.state(&human(), "acme/api", &pr()), Ok(State::Open));
        assert_eq!(gitlab.state(&human(), "acme/api", &pr()), Ok(State::Open));
        rt.block_on(server.verify());
    }

    #[test]
    fn a_refused_token_is_auth_in_gitlabs_words_and_is_never_retried_through_the_cli() {
        charter_core::unsteered!();
        let rt = runtime();
        let server = rt.block_on(MockServer::start());
        rt.block_on(
            Mock::given(method("GET"))
                .and(path(MR))
                .respond_with(
                    ResponseTemplate::new(401).set_body_string(r#"{"message":"401 Unauthorized"}"#),
                )
                .expect(1)
                .mount(&server),
        );
        let cli = Arc::new(Counting::default());
        let error = backend(&server, cli.clone())
            .state(&human(), "acme/api", &pr())
            .unwrap_err();
        assert_eq!(error.failure(), &Failure::Auth);
        assert!(
            error.to_string().contains("401 Unauthorized (HTTP 401)"),
            "{error}"
        );
        assert!(!error.to_string().contains(TOKEN));
        assert_eq!(cli.0.load(Ordering::SeqCst), 0, "retried through the CLI");
        rt.block_on(server.verify());
    }

    #[test]
    fn a_throttled_read_is_rate_limited_with_gitlabs_reset() {
        charter_core::unsteered!();
        let rt = runtime();
        let server = rt.block_on(MockServer::start());
        rt.block_on(
            Mock::given(method("GET"))
                .and(path(MR))
                .respond_with(
                    ResponseTemplate::new(429)
                        .set_body_string("Retry later\n")
                        .insert_header("ratelimit-remaining", "0")
                        .insert_header("ratelimit-reset", "1790000000"),
                )
                .expect(1)
                .mount(&server),
        );
        let error = backend(&server, Arc::default())
            .state(&human(), "acme/api", &pr())
            .unwrap_err();
        assert_eq!(
            error.failure(),
            &Failure::RateLimited {
                reset: Some(1790000000)
            }
        );
        rt.block_on(server.verify());
    }
}
