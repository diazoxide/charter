//! The human's forge token never reaches an agent or a chat's environment (V16, FI3, ADR 0070
//! §4).
//!
//! Each test signs a canary token in for the account, counts every time its source is asked
//! for it, and calls as someone who must not get it: a chat, an MCP tool call, a trigger, and
//! a `charter` command (which a chat can run). The call must go to the CLI route, here a
//! transport that writes down every request, or be refused. The token source must never have
//! been asked, the canary must be in no request the CLI route got, in no error and in no
//! variable of this process's environment, and the native endpoint must have received
//! nothing. A human in the window is the control: the same scene sends the canary natively.

use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use purlis_core::forge::http::{ApiRoot, TokenSource};
use purlis_core::forge::pr::{Pr, State};
use purlis_core::forge::route::{HostScope, Resolver, SignIn};
use purlis_core::forge::transport::{Call, NoAnswer, Reply, Transport};
use purlis_core::forge::{Account, Caller, Forge, ForgeBackend, ForgeError, Kind};
use secrecy::SecretString;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const CANARY: &str = "CANARYhumanToken7f3e9a";

/// A token source that counts how often it is asked.
#[derive(Default)]
struct Counted(AtomicUsize);
impl TokenSource for Counted {
    fn token(&self) -> Result<SecretString, ForgeError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(SecretString::from(CANARY))
    }
}

/// The CLI route: writes down every request, and answers each with one closed pull request.
#[derive(Default)]
struct Cli(Mutex<Vec<Call>>);
impl Transport for Cli {
    fn send(&self, _forge: &Forge, call: &Call) -> Result<Reply, NoAnswer> {
        self.0.lock().unwrap().push(call.clone());
        Ok(Reply::of(
            0,
            r#"{"number":7,"html_url":"https://github.com/o/r/pull/7","state":"closed","merged":false}"#
                .into(),
            String::new(),
        ))
    }
    fn check_auth(&self, _forge: &Forge) -> Result<(), ForgeError> {
        Ok(())
    }
}

struct Scene {
    rt: tokio::runtime::Runtime,
    server: MockServer,
    cli: Arc<Cli>,
    tokens: Arc<Counted>,
}

fn account() -> Account {
    Account {
        kind: Kind::GitHub,
        host: "github.com".into(),
        login: "octocat".into(),
    }
}

impl Scene {
    fn new() -> Scene {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let server = rt.block_on(MockServer::start());
        Scene {
            rt,
            server,
            cli: Arc::new(Cli::default()),
            tokens: Arc::new(Counted::default()),
        }
    }

    fn backend(&self, imported_from_cli: bool) -> Box<dyn ForgeBackend> {
        let resolver = Resolver::new(Kind::GitHub, "github.com")
            .at_root(ApiRoot::at(&self.server.uri()))
            .cli_over(self.cli.clone())
            .signed_in(
                &HostScope::for_a_test(),
                account(),
                SignIn {
                    tokens: self.tokens.clone(),
                    imported_from_cli,
                },
            );
        Forge::default_of(Kind::GitHub).backend_for(Arc::new(resolver))
    }

    fn cli_asked(&self) -> usize {
        self.cli.0.lock().unwrap().len()
    }

    fn assert_the_canary_went_nowhere(&self) {
        assert_eq!(
            self.tokens.0.load(Ordering::SeqCst),
            0,
            "the sign-in token source was asked for the token"
        );
        let asked = serde_json::to_string(&*self.cli.0.lock().unwrap()).unwrap();
        assert!(!asked.contains(CANARY), "a CLI request carried the token");
        for (name, value) in std::env::vars() {
            assert!(!value.contains(CANARY), "{name} holds the token");
        }
        let received = self.rt.block_on(self.server.received_requests());
        assert_eq!(
            received.map(|r| r.len()),
            Some(0),
            "a request reached the native endpoint"
        );
    }
}

fn pr() -> Pr {
    Pr {
        number: 7,
        url: String::new(),
    }
}

#[test]
fn a_chat_goes_through_the_cli_login_and_never_gets_the_sign_in_token() {
    purlis_core::unsteered!();
    let scene = Scene::new();
    let chat = Caller::chat("01J9CHAT").as_account(account());
    let state = scene.backend(false).state(&chat, "o/r", &pr()).unwrap();
    assert_eq!(state, State::Closed);
    assert_eq!(scene.cli_asked(), 1, "the CLI answered the chat");
    scene.assert_the_canary_went_nowhere();
}

#[test]
fn a_chats_write_goes_to_the_cli_without_the_token() {
    purlis_core::unsteered!();
    let scene = Scene::new();
    let chat = Caller::chat("01J9CHAT").as_account(account());
    let _ =
        scene
            .backend(false)
            .open_or_update(&chat, "o/r", "charter/save", "main", "Save", "body");
    assert!(scene.cli_asked() > 0);
    scene.assert_the_canary_went_nowhere();
}

#[test]
fn an_mcp_tool_call_never_sends_the_sign_in_token() {
    purlis_core::unsteered!();
    let scene = Scene::new();
    let mcp = Caller::mcp("01J9CHAT").as_account(account());
    scene.backend(false).state(&mcp, "o/r", &pr()).unwrap();
    scene.assert_the_canary_went_nowhere();
}

#[test]
fn a_charter_command_and_a_trigger_never_read_the_sign_in_token_even_for_a_human() {
    purlis_core::unsteered!();
    for caller in [Caller::command(), Caller::trigger()] {
        let scene = Scene::new();
        scene
            .backend(false)
            .state(&caller.as_account(account()), "o/r", &pr())
            .unwrap();
        assert_eq!(scene.cli_asked(), 1);
        scene.assert_the_canary_went_nowhere();
    }
}

#[test]
fn a_chat_on_a_host_whose_cli_login_was_imported_gets_no_forge_credential() {
    purlis_core::unsteered!();
    let scene = Scene::new();
    let chat = Caller::chat("01J9CHAT").as_account(account());
    let got = scene.backend(true).state(&chat, "o/r", &pr());
    assert_eq!(
        got.as_ref().map_err(|e| e.failure().clone()),
        Err(purlis_core::forge::Failure::Forbidden),
        "a refused credential is Forbidden: {got:?}"
    );
    assert!(!format!("{got:?}").contains(CANARY));
    assert_eq!(scene.cli_asked(), 0, "the imported CLI login was used");
    scene.assert_the_canary_went_nowhere();
}

#[test]
fn a_human_in_the_window_is_the_one_caller_the_token_is_sent_for() {
    purlis_core::unsteered!();
    let scene = Scene::new();
    scene.rt.block_on(
        Mock::given(method("GET"))
            .and(path("/repos/o/r/pulls/7"))
            .and(header("authorization", format!("Bearer {CANARY}").as_str()))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(r#"{"state":"open","merged":false}"#),
            )
            .expect(1)
            .mount(&scene.server),
    );
    let human = Caller::window().as_account(account());
    assert_eq!(
        scene.backend(false).state(&human, "o/r", &pr()),
        Ok(State::Open)
    );
    assert_eq!(scene.tokens.0.load(Ordering::SeqCst), 1);
    assert_eq!(scene.cli_asked(), 0);
    scene.rt.block_on(scene.server.verify());
}

/// The `charter` binary never holds a sign-in to route to: it never claims the host's scope
/// and never hands a resolver a sign-in (ADR 0070 ruling 5).
#[test]
fn the_charter_binary_never_claims_the_hosts_scope() {
    purlis_core::unsteered!();
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../purlis-cli/src");
    let mut stack = vec![src];
    let mut read = 0;
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "rs") {
                let text = std::fs::read_to_string(&path).unwrap();
                read += 1;
                for word in ["HostScope", "SignIn", ".signed_in("] {
                    assert!(
                        !text.contains(word),
                        "{} names {word}: the charter binary holds no sign-in",
                        path.display()
                    );
                }
            }
        }
    }
    assert!(read > 0, "no purlis-cli source was read");
}
