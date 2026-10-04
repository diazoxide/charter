//! The native transport (ADR 0070 §3 and §4): a backend's [`Call`] sent over HTTPS through
//! `ureq`, with a token a [`TokenSource`] hands out when the request is sent.
//!
//! # The same request the CLI sends
//!
//! A `Call`'s fields mean here what they mean to `gh api`, so one backend body is right on
//! both transports: a `GET`'s fields are its query; a write's are its JSON body; a GraphQL
//! call's first field is the document and the rest are its variables. A [`Field::Text`] is a
//! string, and a [`Field::Typed`] is `true`, `false`, `null` or an integer when it reads as
//! one. A name `a[b]` sets key `b` of object `a`, and `a[]` appends to array `a`. The answer is
//! filled in as the CLI's would be — exit 0 for an answer; exit 1 with the forge's message and
//! `(HTTP <status>)` for a refusal, and with `GraphQL: <message>` for a GraphQL error — plus the
//! status and headers ([`Reply::status`], [`Reply::headers`]).
//!
//! # The token
//!
//! It is a [`SecretString`], read when a request is sent, set as that one request's
//! `Authorization` header and dropped. The `Bearer …` string built from it is a
//! [`Zeroizing`] string, wiped when it goes. Two copies are out of charter's reach: the
//! `HeaderValue` (`http` keeps its bytes in a `Bytes` buffer it frees without wiping, and offers
//! no way to wipe it), and `ureq`'s write buffer. Both live for one request.
//!
//! A token is sent only over HTTPS, or, in a test build alone, plain HTTP to this machine's
//! loopback (a recorded forge). A URL with user information is refused, so `http://127.0.0.1:1@evil.example/`
//! is not loopback, and so is a request whose authority differs from the API root's. No
//! redirect is followed.

use std::sync::Arc;
use std::time::{Duration, Instant};

use secrecy::{ExposeSecret, SecretString};
use serde_json::{Map, Value};
use zeroize::Zeroizing;

use super::etag::{EtagStore, Stored};
use super::transport::{Call, Endpoint, Field, NoAnswer, Reply, Transport};
use super::{Forge, ForgeError, Kind};

/// Hands out an account's token when a request is sent. FW-1 and FW-3a decide the sign-in
/// flows and write the keyring-backed source; the seam takes a source so that no caller ever
/// holds the token (ADR 0070 §4).
pub trait TokenSource: Send + Sync {
    fn token(&self) -> Result<SecretString, ForgeError>;
}

/// Where a forge's API lives: its REST root and its GraphQL endpoint.
///
/// Its fields are private, and the constructors a shipped build has, [`ApiRoot::github`],
/// [`ApiRoot::gitlab`] and [`ApiRoot::of`], always name an HTTPS root. [`ApiRoot::at`], which can name a loopback `http://` root for a
/// recorded forge, exists only in a test build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiRoot {
    /// `https://api.github.com`, `https://<host>/api/v3` on a GHES, or `https://<host>/api/v4`
    /// on a GitLab.
    rest: String,
    /// `https://api.github.com/graphql`, or `https://<host>/api/graphql` on a GHES or a GitLab.
    graphql: String,
}

impl ApiRoot {
    /// The API of forge `kind` on `host`.
    pub fn of(kind: Kind, host: &str) -> ApiRoot {
        match kind {
            Kind::GitHub => ApiRoot::github(host),
            Kind::GitLab => ApiRoot::gitlab(host),
        }
    }

    /// GitLab's API for `host`: gitlab.com's, or a self-managed instance's, which serves it at
    /// the same paths (REST v4 and GraphQL).
    pub fn gitlab(host: &str) -> ApiRoot {
        ApiRoot {
            rest: format!("https://{host}/api/v4"),
            graphql: format!("https://{host}/api/graphql"),
        }
    }

    /// GitHub's API for `host`: github.com's own, or a GitHub Enterprise Server's.
    pub fn github(host: &str) -> ApiRoot {
        if host.eq_ignore_ascii_case("github.com") {
            ApiRoot {
                rest: "https://api.github.com".into(),
                graphql: "https://api.github.com/graphql".into(),
            }
        } else {
            ApiRoot {
                rest: format!("https://{host}/api/v3"),
                graphql: format!("https://{host}/api/graphql"),
            }
        }
    }

    /// One root for both: a recorded forge in a test serves REST under `base` and GraphQL at
    /// `base/graphql`. Only a build with the plane fence on has it, and only a test build turns
    /// that on (`Cargo.toml`).
    #[cfg(any(test, feature = "fenced"))]
    pub fn at(base: &str) -> ApiRoot {
        let base = base.trim_end_matches('/');
        ApiRoot {
            rest: base.to_string(),
            graphql: format!("{base}/graphql"),
        }
    }
}

/// The parts of a URL a token's safety turns on: its scheme and its authority, or why it is
/// refused.
fn checked(url: &str) -> Result<(String, String), String> {
    let uri: http::Uri = url
        .parse()
        .map_err(|e| format!("{url} is not a URL: {e}"))?;
    let scheme = uri.scheme_str().unwrap_or_default().to_ascii_lowercase();
    let authority = uri
        .authority()
        .ok_or_else(|| format!("{url} names no host"))?;
    if authority.as_str().contains('@') {
        return Err(format!(
            "charter sends a forge token to no URL with user information, and {url} has some"
        ));
    }
    let host = authority.host();
    // Plain HTTP to loopback is for a recorded forge, so only a test build sends a token there.
    let loopback =
        cfg!(any(test, feature = "fenced")) && matches!(host, "127.0.0.1" | "localhost" | "[::1]");
    match scheme.as_str() {
        "https" => Ok((scheme, authority.as_str().to_ascii_lowercase())),
        "http" if loopback => Ok((scheme, authority.as_str().to_ascii_lowercase())),
        _ => Err(format!(
            "charter sends a forge token over HTTPS only, and {url} is not"
        )),
    }
}

/// HTTPS with a token charter holds.
pub struct Http {
    kind: Kind,
    agent: ureq::Agent,
    root: ApiRoot,
    authority: String,
    tokens: Arc<dyn TokenSource>,
    etags: Option<Arc<dyn EtagStore>>,
}

impl std::fmt::Debug for Http {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Http")
            .field("root", &self.root)
            .finish_non_exhaustive()
    }
}

/// The most a forge's answer may be. A page of a hundred repos is well under this.
const MOST_BODY: u64 = 64 * 1024 * 1024;

/// The headers a [`Reply`] keeps: what the backends, the ETag store and the request budget
/// ([`super::budget`]) read. GitHub spells its rate limit `x-ratelimit-*`, naming which limit
/// in `x-ratelimit-resource`, and GitLab `ratelimit-*`, naming it in `ratelimit-name`.
const KEPT: [&str; 12] = [
    "etag",
    "link",
    "retry-after",
    "x-ratelimit-limit",
    "x-ratelimit-remaining",
    "x-ratelimit-reset",
    "x-ratelimit-used",
    "x-ratelimit-resource",
    "ratelimit-limit",
    "ratelimit-remaining",
    "ratelimit-reset",
    "ratelimit-name",
];

/// The headers that say which API a request speaks: GitHub's media type and the REST version
/// charter was written against, or plain JSON for GitLab, whose v4 is in the path.
fn dialect(kind: Kind) -> &'static [(&'static str, &'static str)] {
    match kind {
        Kind::GitHub => &[
            ("accept", "application/vnd.github+json"),
            ("x-github-api-version", "2022-11-28"),
        ],
        Kind::GitLab => &[("accept", "application/json")],
    }
}

impl Http {
    /// The native transport to forge `kind`'s API at `root`, authenticated by `tokens`. Refused
    /// for a root a token may not be sent to.
    pub fn new(
        kind: Kind,
        root: ApiRoot,
        tokens: Arc<dyn TokenSource>,
    ) -> Result<Http, ForgeError> {
        let (_, authority) = checked(&root.rest).map_err(ForgeError::transport)?;
        let (_, graphql) = checked(&root.graphql).map_err(ForgeError::transport)?;
        if graphql != authority {
            return Err(ForgeError::transport(format!(
                "the GraphQL endpoint {} is not on the REST root's host",
                root.graphql
            )));
        }
        let agent = ureq::Agent::config_builder()
            // A refusal is an answer: the backend reads it.
            .http_status_as_error(false)
            // A redirect could take the token to another host; none is followed.
            .max_redirects(0)
            .timeout_global(Some(Duration::from_secs(60)))
            .user_agent(concat!("charter/", env!("CARGO_PKG_VERSION")))
            .tls_config(
                ureq::tls::TlsConfig::builder()
                    .root_certs(ureq::tls::RootCerts::PlatformVerifier)
                    .build(),
            )
            .build()
            .new_agent();
        Ok(Http {
            kind,
            agent,
            root,
            authority,
            tokens,
            etags: None,
        })
    }

    /// The same transport, its `GET`s made conditional against `store` (ADR 0070 §3).
    pub fn with_etags(mut self, store: Arc<dyn EtagStore>) -> Http {
        self.etags = Some(store);
        self
    }

    /// The URL and body `call` is sent as.
    fn request_of(&self, call: &Call) -> Result<(http::Method, String, Option<Value>), String> {
        let (method, url, body) = request_for(&self.root, call)?;
        let (_, authority) = checked(&url)?;
        if authority != self.authority {
            return Err(format!("{url} is not on the forge's API host"));
        }
        Ok((method, url, body))
    }

    fn sent(&self, call: &Call) -> Result<Reply, String> {
        let (method, url, body) = self.request_of(call)?;
        let conditional = method == http::Method::GET;
        let stored = match (&self.etags, conditional) {
            (Some(store), true) => store.get(call.path()),
            _ => None,
        };
        let token = self.tokens.token().map_err(|e| e.to_string())?;
        let bearer = Zeroizing::new(format!("Bearer {}", token.expose_secret()));
        drop(token);
        let mut authorization =
            http::HeaderValue::from_str(&bearer).map_err(|_| "the token is not a header value")?;
        drop(bearer);
        authorization.set_sensitive(true);
        // GitLab takes a personal, group or OAuth token as a bearer token too.
        let mut request = http::Request::builder()
            .method(method.clone())
            .uri(&url)
            .header(http::header::AUTHORIZATION, authorization);
        for (name, value) in dialect(self.kind) {
            request = request.header(*name, *value);
        }
        if let Some(stored) = &stored {
            request = request.header(http::header::IF_NONE_MATCH, &stored.etag);
        }
        let started = Instant::now();
        let response = match body {
            Some(body) => {
                let bytes = serde_json::to_vec(&body).map_err(|e| e.to_string())?;
                let request = request
                    .header(http::header::CONTENT_TYPE, "application/json")
                    .body(bytes)
                    .map_err(|e| e.to_string())?;
                self.agent.run(request)
            }
            None => {
                let request = request.body(()).map_err(|e| e.to_string())?;
                self.agent.run(request)
            }
        };
        // The network log (OB-15): host, method, path template, status and timing; never a body
        // or a header value.
        crate::netlog::record(crate::netlog::Call {
            feature: crate::netlog::Feature::Forge,
            via: crate::netlog::Via::Https,
            host: &self.authority,
            method: method.as_str(),
            path: call.path(),
            status: response.as_ref().ok().map(|r| r.status().as_u16()),
            answered: response.is_ok(),
            took: started.elapsed(),
        });
        let response = response.map_err(|e| format!("{method} {} failed: {e}", self.root.rest))?;
        let status = response.status().as_u16();
        let headers: Vec<(String, String)> = KEPT
            .iter()
            .filter_map(|name| {
                let value = response.headers().get(*name)?.to_str().ok()?;
                Some(((*name).to_string(), value.to_string()))
            })
            .collect();
        let bytes = response
            .into_body()
            .into_with_config()
            .limit(MOST_BODY)
            .read_to_vec()
            .map_err(|e| format!("reading the forge's answer: {e}"))?;
        let out = String::from_utf8_lossy(&bytes).into_owned();
        if status == 304
            && let Some(stored) = stored
        {
            // The stored answer, with its `Link`, under the `304`'s own status and rate-limit
            // headers: the budget counts a `304` as its forge does.
            let mut kept: Vec<(String, String)> = headers
                .into_iter()
                .filter(|(name, _)| name != "link" && name != "etag")
                .collect();
            if let Some(link) = stored.link {
                kept.push(("link".to_string(), link));
            }
            return Ok(Reply {
                code: 0,
                out: stored.body,
                err: String::new(),
                status: Some(304),
                headers: kept,
            });
        }
        let reply = reply_of(call, status, out, headers);
        if conditional
            && reply.ok()
            && status == 200
            && let (Some(store), Some(etag)) = (&self.etags, reply.header("etag"))
        {
            store.put(
                call.path(),
                Stored {
                    etag: etag.to_string(),
                    link: reply.header("link").map(str::to_string),
                    body: reply.out.clone(),
                },
            );
        }
        Ok(reply)
    }
}

/// The method, URL and JSON body `call` is sent as to `root`, read as `gh api` reads it.
pub fn request_for(
    root: &ApiRoot,
    call: &Call,
) -> Result<(http::Method, String, Option<Value>), String> {
    match &call.endpoint {
        Endpoint::Graphql => {
            let (query, variables) = match call.fields.split_first() {
                Some((Field::Text(name, query), rest)) if name == "query" => (query, rest),
                _ => return Err("a GraphQL call's first field is its query".to_string()),
            };
            let mut body = Map::new();
            body.insert("query".into(), Value::String(query.clone()));
            let mut vars = Map::new();
            for field in variables {
                if field_name(field) == "operationName" {
                    body.insert("operationName".into(), field_value(field));
                } else {
                    set(&mut vars, field_name(field), field_value(field))?;
                }
            }
            body.insert("variables".into(), Value::Object(vars));
            Ok((
                http::Method::POST,
                root.graphql.clone(),
                Some(Value::Object(body)),
            ))
        }
        Endpoint::Rest { method, path } => {
            let method = match method {
                None => http::Method::GET,
                Some(m) => http::Method::from_bytes(m.word().as_bytes())
                    .map_err(|e| format!("{}: {e}", m.word()))?,
            };
            let mut url = format!("{}/{}", root.rest, path.trim_start_matches('/'));
            if method == http::Method::GET {
                for field in &call.fields {
                    let joint = if url.contains('?') { '&' } else { '?' };
                    let value = match field_value(field) {
                        Value::String(s) => s,
                        other => other.to_string(),
                    };
                    url.push(joint);
                    url.push_str(&super::quote(field_name(field)));
                    url.push('=');
                    url.push_str(&super::quote(&value));
                }
                return Ok((method, url, None));
            }
            let mut body = Map::new();
            for field in &call.fields {
                set(&mut body, field_name(field), field_value(field))?;
            }
            Ok((method, url, Some(Value::Object(body))))
        }
    }
}

fn field_name(field: &Field) -> &str {
    match field {
        Field::Text(name, _) | Field::Typed(name, _) => name,
    }
}

/// A field's JSON value: a text field is a string; a typed one is `true`, `false`, `null` or an
/// integer when it reads as one, as `gh api -F` reads it, and a string otherwise.
fn field_value(field: &Field) -> Value {
    match field {
        Field::Text(_, value) => Value::String(value.clone()),
        Field::Typed(_, value) => match value.as_str() {
            "true" => Value::Bool(true),
            "false" => Value::Bool(false),
            "null" => Value::Null,
            v => v
                .parse::<i64>()
                .map(Value::from)
                .unwrap_or_else(|_| Value::String(v.to_string())),
        },
    }
}

/// Set `name` to `value` in `into`: `a[b]` is key `b` of object `a`, `a[]` appends to array
/// `a`, as `gh api` builds nested parameters.
fn set(into: &mut Map<String, Value>, name: &str, value: Value) -> Result<(), String> {
    let Some(open) = name.find('[') else {
        into.insert(name.to_string(), value);
        return Ok(());
    };
    let (head, rest) = name.split_at(open);
    let close = rest
        .find(']')
        .ok_or_else(|| format!("the field {name} opens a bracket it never closes"))?;
    let key = &rest[1..close];
    let tail = &rest[close + 1..];
    if key.is_empty() {
        if !tail.is_empty() {
            return Err(format!("the field {name} nests past an array"));
        }
        let slot = into
            .entry(head.to_string())
            .or_insert_with(|| Value::Array(Vec::new()));
        match slot {
            Value::Array(items) => items.push(value),
            _ => return Err(format!("the field {name} is both a value and a list")),
        }
        return Ok(());
    }
    let slot = into
        .entry(head.to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    let Value::Object(inner) = slot else {
        return Err(format!("the field {name} is both a value and an object"));
    };
    set(inner, &format!("{key}{tail}"), value)
}

/// What a refusal's JSON says: GitHub's and GitLab's `message`, which GitLab makes an object of
/// field errors when it refuses a write, or an OAuth refusal's `error_description`, else its
/// `error`.
fn refusal_words(answer: &Value) -> Option<String> {
    match &answer["message"] {
        Value::String(said) => return Some(said.clone()),
        Value::Null => {}
        other => return Some(other.to_string()),
    }
    ["error_description", "error"]
        .iter()
        .find_map(|key| answer[*key].as_str().map(str::to_string))
}

/// The [`Reply`] a forge's HTTP answer is, spelled as the CLI would have.
fn reply_of(call: &Call, status: u16, out: String, headers: Vec<(String, String)>) -> Reply {
    let refusal = |err: String| Reply {
        code: 1,
        out: out.clone(),
        err,
        status: Some(status),
        headers: headers.clone(),
    };
    if !(200..300).contains(&status) {
        let message = serde_json::from_str::<Value>(&out)
            .ok()
            .and_then(|v| refusal_words(&v))
            .unwrap_or_else(|| out.trim().to_string());
        return refusal(format!("{message} (HTTP {status})"));
    }
    if call.endpoint == Endpoint::Graphql
        && let Ok(answer) = serde_json::from_str::<Value>(&out)
        && let Some(errors) = answer["errors"].as_array().filter(|e| !e.is_empty())
    {
        let said: Vec<&str> = errors
            .iter()
            .filter_map(|e| e["message"].as_str())
            .collect();
        return refusal(format!("GraphQL: {}", said.join(", ")));
    }
    Reply {
        code: 0,
        out,
        err: String::new(),
        status: Some(status),
        headers,
    }
}

impl Transport for Http {
    fn send(&self, _forge: &Forge, call: &Call) -> Result<Reply, NoAnswer> {
        self.sent(call).map_err(NoAnswer::Missing)
    }

    fn check_auth(&self, forge: &Forge) -> Result<(), ForgeError> {
        let reply = self
            .sent(&Call::get("user", super::STATUS_TIMEOUT))
            .map_err(ForgeError::transport)?;
        if reply.ok() {
            Ok(())
        } else {
            Err(ForgeError::of(
                reply.failure(),
                format!(
                    "charter's sign-in for {} was refused: {}",
                    forge.host,
                    reply.said(forge.kind)
                ),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::forge::Kind;
    use crate::forge::transport::Method;

    struct Fixed(&'static str);
    impl TokenSource for Fixed {
        fn token(&self) -> Result<SecretString, ForgeError> {
            Ok(SecretString::from(self.0))
        }
    }

    #[test]
    fn a_token_is_sent_over_https_or_to_loopback_and_nowhere_else() {
        assert!(checked("https://api.github.com").is_ok());
        assert!(checked("http://127.0.0.1:4000").is_ok());
        assert!(checked("http://localhost/x").is_ok());
        assert!(checked("http://[::1]:8/").is_ok());
        assert!(checked("http://ghe.example.com/api/v3").is_err());
        assert!(checked("http://127.0.0.1.evil.example/").is_err());
        assert!(checked("ftp://127.0.0.1/").is_err());
    }

    #[test]
    fn a_url_with_user_information_is_never_loopback_and_never_sent_a_token() {
        for url in [
            "http://127.0.0.1:1@evil.example/",
            "http://localhost@evil.example/",
            "https://user:pass@api.github.com/",
            "https://api.github.com@evil.example/",
        ] {
            assert!(checked(url).is_err(), "{url}");
            assert!(
                Http::new(Kind::GitHub, ApiRoot::at(url), Arc::new(Fixed("canary"))).is_err(),
                "{url}"
            );
        }
    }

    #[test]
    fn a_path_cannot_move_a_request_off_the_api_roots_host() {
        let t = Http::new(
            Kind::GitHub,
            ApiRoot::github("github.com"),
            Arc::new(Fixed("canary")),
        )
        .unwrap();
        for path in ["//evil.example/x", "@evil.example/x", "/../../x"] {
            let (_, url, _) = t
                .request_of(&Call::get(path, super::super::LIST_TIMEOUT))
                .unwrap();
            assert_eq!(checked(&url).unwrap().1, "api.github.com", "{path}: {url}");
        }
    }

    #[test]
    fn the_transports_debug_never_shows_the_token() {
        let t = Http::new(
            Kind::GitHub,
            ApiRoot::github("github.com"),
            Arc::new(Fixed("canary-4b1d")),
        )
        .unwrap();
        assert!(!format!("{t:?}").contains("canary-4b1d"));
    }

    #[test]
    fn github_com_and_a_ghes_have_their_own_api_roots() {
        let get = Call::get("user", super::super::LIST_TIMEOUT);
        let url = |root: &ApiRoot, call: &Call| request_for(root, call).unwrap().1;
        assert_eq!(
            url(&ApiRoot::github("github.com"), &get),
            "https://api.github.com/user"
        );
        let ghes = ApiRoot::github("ghe.example.com");
        assert_eq!(url(&ghes, &get), "https://ghe.example.com/api/v3/user");
        let gql = Call::graphql("query{viewer{login}}", vec![], super::super::LIST_TIMEOUT);
        assert_eq!(url(&ghes, &gql), "https://ghe.example.com/api/graphql");
    }

    #[test]
    fn gitlab_com_and_a_self_managed_gitlab_have_their_own_api_roots() {
        let get = Call::get("projects/acme%2Fapi", super::super::LIST_TIMEOUT);
        let gql = Call::graphql("query{currentUser{id}}", vec![], super::super::LIST_TIMEOUT);
        let url = |root: &ApiRoot, call: &Call| request_for(root, call).unwrap().1;
        let dotcom = ApiRoot::gitlab("gitlab.com");
        assert_eq!(
            url(&dotcom, &get),
            "https://gitlab.com/api/v4/projects/acme%2Fapi"
        );
        assert_eq!(url(&dotcom, &gql), "https://gitlab.com/api/graphql");
        let own = ApiRoot::of(Kind::GitLab, "git.example.com:8443");
        assert_eq!(
            url(&own, &get),
            "https://git.example.com:8443/api/v4/projects/acme%2Fapi"
        );
        assert_eq!(url(&own, &gql), "https://git.example.com:8443/api/graphql");
        assert_eq!(
            ApiRoot::of(Kind::GitHub, "github.com"),
            ApiRoot::github("github.com")
        );
    }

    #[test]
    fn gitlab_is_asked_in_its_own_dialect_and_github_in_its_own() {
        let names = |kind| {
            dialect(kind)
                .iter()
                .map(|(n, v)| format!("{n}: {v}"))
                .collect::<Vec<_>>()
        };
        assert_eq!(names(Kind::GitLab), ["accept: application/json"]);
        assert_eq!(
            names(Kind::GitHub),
            [
                "accept: application/vnd.github+json",
                "x-github-api-version: 2022-11-28"
            ]
        );
    }

    #[test]
    fn a_gitlab_refusal_reads_as_glabs_would() {
        let get = Call::get("x", super::super::LIST_TIMEOUT);
        let said = |status, body: &str| reply_of(&get, status, body.into(), vec![]).err;
        assert_eq!(
            said(404, r#"{"message":"404 Project Not Found"}"#),
            "404 Project Not Found (HTTP 404)"
        );
        // A validation refusal names its fields in an object.
        let invalid = said(400, r#"{"message":{"title":["can't be blank"]}}"#);
        assert!(
            invalid.contains("title") && invalid.contains("can't be blank"),
            "{invalid}"
        );
        assert!(invalid.ends_with("(HTTP 400)"), "{invalid}");
        // An OAuth refusal says why in `error_description`, else `error`.
        assert_eq!(
            said(
                403,
                r#"{"error":"insufficient_scope","error_description":"The request requires higher privileges."}"#
            ),
            "The request requires higher privileges. (HTTP 403)"
        );
        assert_eq!(
            said(401, r#"{"error":"invalid_token"}"#),
            "invalid_token (HTTP 401)"
        );
    }

    #[test]
    fn gitlabs_rate_limit_headers_are_kept_and_read() {
        let get = Call::get("x", super::super::LIST_TIMEOUT);
        let spent = reply_of(
            &get,
            429,
            "Retry later".into(),
            vec![
                ("ratelimit-remaining".into(), "0".into()),
                ("ratelimit-reset".into(), "1790000000".into()),
            ],
        );
        assert_eq!(
            spent.failure(),
            crate::forge::Failure::RateLimited {
                reset: Some(1790000000)
            }
        );
        for name in ["ratelimit-limit", "ratelimit-remaining", "ratelimit-reset"] {
            assert!(KEPT.contains(&name), "{name}");
        }
    }

    #[test]
    fn fields_mean_what_they_mean_to_gh_api() {
        let root = ApiRoot::at("https://x.example");
        let write = Call::write(
            Method::Post,
            "repos/o/r/issues",
            vec![
                Field::text("title", "@not-a-file"),
                Field::typed("milestone", "3"),
                Field::typed("draft", "false"),
                Field::text("labels[]", "bug"),
                Field::text("labels[]", "p1"),
            ],
        );
        let (method, url, body) = request_for(&root, &write).unwrap();
        assert_eq!(
            (method, url.as_str()),
            (http::Method::POST, "https://x.example/repos/o/r/issues")
        );
        assert_eq!(
            body.unwrap(),
            serde_json::json!({"title": "@not-a-file", "milestone": 3, "draft": false,
                               "labels": ["bug", "p1"]})
        );
        let gql = Call::graphql(
            "mutation($v:ProjectV2FieldValue!){x}",
            vec![
                Field::text("value[singleSelectOptionId]", "opt"),
                Field::typed("number", "12"),
            ],
            super::super::LIST_TIMEOUT,
        );
        let (_, _, body) = request_for(&root, &gql).unwrap();
        assert_eq!(
            body.unwrap(),
            serde_json::json!({"query": "mutation($v:ProjectV2FieldValue!){x}",
                               "variables": {"value": {"singleSelectOptionId": "opt"},
                                             "number": 12}})
        );
    }

    #[test]
    fn a_refusal_and_a_graphql_error_read_as_the_clis_would() {
        let get = Call::get("x", super::super::LIST_TIMEOUT);
        let r = reply_of(&get, 404, r#"{"message":"Not Found"}"#.into(), vec![]);
        assert_eq!((r.code, r.err.as_str()), (1, "Not Found (HTTP 404)"));
        assert_eq!(r.failure(), crate::forge::Failure::NotFound);
        let gql = Call::graphql("q", vec![], super::super::LIST_TIMEOUT);
        let r = reply_of(
            &gql,
            200,
            r#"{"data":null,"errors":[{"type":"UNPROCESSABLE","message":"Pull request is in clean status"}]}"#
                .into(),
            vec![],
        );
        assert_eq!(r.code, 1);
        assert!(r.err.contains("clean status"), "{}", r.err);
        let ok = reply_of(
            &get,
            200,
            "[]".into(),
            vec![("etag".into(), "\"v\"".into())],
        );
        assert!(ok.ok());
        assert_eq!(ok.header("etag"), Some("\"v\""));
    }
}
