//! The recorded run over the native transport (ADR 0070 §7, FW-2a): each recording's exchanges
//! served as HTTP by a `wiremock` server, and the backend sending its requests through
//! [`charter_core::forge::http::Http`] with a sign-in token.
//!
//! An exchange is served once, to the request `gh api` would have sent for its `Call`: the
//! same method, path and query, and the same JSON body. That request is worked out here from
//! the recording alone ([`expected`]), not by the transport's own code, so a transport that
//! dropped a query or misread a field fails the case. A reply with exit 0 is a `200` with the recorded answer. A request
//! nobody recorded is a `404`, which fails the case, and [`Over`]'s check fails on an exchange
//! nobody asked for. The CLI route is a transport that refuses to answer, so a case that
//! reached for the CLI fails too.

use std::sync::Arc;

use charter_core::forge::Kind;
use charter_core::forge::http::{ApiRoot, TokenSource};
use charter_core::forge::recorded::Exchange;
use charter_core::forge::route::{HostScope, Resolver, SignIn};
use charter_core::forge::transport::{Call, Endpoint, Field, NoAnswer, Reply, Transport};
use charter_core::forge::{Account, Caller, Forge, ForgeError};
use secrecy::SecretString;
use serde_json::Value;
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

use super::Over;

/// The token every native case signs in with.
pub const TOKEN: &str = "native-contract-token-5f2a";

struct Fixed;
impl TokenSource for Fixed {
    fn token(&self) -> Result<SecretString, ForgeError> {
        Ok(SecretString::from(TOKEN))
    }
}

/// The CLI route of a native run: it never answers.
struct NoCli;
impl Transport for NoCli {
    fn send(&self, _forge: &Forge, call: &Call) -> Result<Reply, NoAnswer> {
        Err(NoAnswer::Missing(format!(
            "a native case reached for the CLI: {}",
            call.path()
        )))
    }
    fn check_auth(&self, _forge: &Forge) -> Result<(), ForgeError> {
        Err(ForgeError::transport("a native case reached for the CLI"))
    }
}

/// One exchange's request, as the native transport sends it.
struct Exactly {
    method: http::Method,
    path_and_query: String,
    body: Option<Value>,
}

impl wiremock::Match for Exactly {
    fn matches(&self, request: &Request) -> bool {
        let mut asked = request.url.path().to_string();
        if let Some(query) = request.url.query() {
            asked.push('?');
            asked.push_str(query);
        }
        let body = if request.body.is_empty() {
            None
        } else {
            serde_json::from_slice::<Value>(&request.body).ok()
        };
        let token = request
            .headers
            .get("authorization")
            .and_then(|v| v.to_str().ok());
        request.method == self.method
            && asked == self.path_and_query
            && body == self.body
            && token == Some(&format!("Bearer {TOKEN}"))
    }
}

/// The JSON a field is, as `gh api` sends it: `-f` a string, `-F` a boolean, null or integer
/// when it reads as one.
fn json_of(field: &Field) -> (String, Value) {
    match field {
        Field::Text(name, value) => (name.clone(), Value::String(value.clone())),
        Field::Typed(name, value) => {
            let v = match value.as_str() {
                "true" => Value::Bool(true),
                "false" => Value::Bool(false),
                "null" => Value::Null,
                v => v
                    .parse::<i64>()
                    .map(Value::from)
                    .unwrap_or_else(|_| Value::String(v.into())),
            };
            (name.clone(), v)
        }
    }
}

/// The HTTP request a recorded call is, read from the recording.
fn expected(call: &Call) -> Exactly {
    assert!(
        call.fields.iter().all(|f| !json_of(f).0.contains('[')),
        "a recording here names no nested field"
    );
    match &call.endpoint {
        Endpoint::Graphql => {
            let (query, rest) = call.fields.split_first().expect("a GraphQL query");
            let variables: serde_json::Map<String, Value> = rest.iter().map(json_of).collect();
            Exactly {
                method: http::Method::POST,
                path_and_query: "/graphql".into(),
                body: Some(serde_json::json!({"query": json_of(query).1, "variables": variables})),
            }
        }
        Endpoint::Rest { method: None, path } => {
            assert!(
                call.fields.is_empty(),
                "a recorded GET carries its query in its path"
            );
            Exactly {
                method: http::Method::GET,
                path_and_query: format!("/{path}"),
                body: None,
            }
        }
        Endpoint::Rest {
            method: Some(method),
            path,
        } => Exactly {
            method: method.word().parse().unwrap(),
            path_and_query: format!("/{path}"),
            body: Some(Value::Object(call.fields.iter().map(json_of).collect())),
        },
    }
}

pub fn account() -> Account {
    Account {
        kind: Kind::GitHub,
        host: "github.com".into(),
        login: "octocat".into(),
    }
}

/// `kind`'s backend over the native transport, against a server answering `recording`.
pub fn over(kind: &str, recording: &str) -> Over {
    assert_eq!(
        kind, "github",
        "only GitHub has a native transport yet (FW-2b)"
    );
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let server = rt.block_on(MockServer::start());
    let root = ApiRoot::at(&server.uri());
    let file: Value = serde_json::from_str(recording).unwrap();
    for exchange in file["exchanges"].as_array().cloned().unwrap_or_default() {
        let exchange: Exchange = serde_json::from_value(exchange).unwrap();
        assert_eq!(
            exchange.reply.code, 0,
            "a native recording replays answers only"
        );
        let mock = Mock::given(expected(&exchange.call))
            .respond_with(ResponseTemplate::new(200).set_body_string(exchange.reply.out))
            .up_to_n_times(1)
            .expect(1);
        rt.block_on(mock.mount(&server));
    }
    let resolver = Resolver::new(Kind::GitHub, "github.com")
        .at_root(root)
        .cli_over(Arc::new(NoCli))
        .signed_in(
            &HostScope::for_a_test(),
            account(),
            SignIn {
                tokens: Arc::new(Fixed),
                imported_from_cli: false,
            },
        );
    let backend = Forge::default_of(Kind::GitHub).backend_for(Arc::new(resolver));
    Over {
        backend,
        caller: Caller::window().as_account(account()),
        check: Box::new(move || rt.block_on(server.verify())),
    }
}
