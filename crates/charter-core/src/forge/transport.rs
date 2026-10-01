//! A forge request, the answer to it, and the [`Transport`] that carries one to the other
//! (ADR 0070 §3 and §5).
//!
//! A backend ([`super::github`], [`super::gitlab`]) builds each request **once**, as a
//! [`Call`]: a REST path or a GraphQL document, a method, and its fields. A transport sends it.
//! Two exist today:
//!
//! - [`super::cli::Cli`], the fallback transport: the forge's own CLI, `gh api` or
//!   `glab api`, with that CLI's own login. It is the only one a request takes until FW-2a/b
//!   build the native HTTP transport and FW-1 gives charter a sign-in of its own.
//! - [`super::recorded::Recorded`], which answers from recorded exchanges and fails a request
//!   nobody recorded. The contract suite runs every backend operation through it.
//!
//! # Why the answer is shaped like a CLI's
//!
//! [`Reply`] is an exit code, stdout and stderr, because every caller's error text is pinned by
//! the recorded Python behaviour (ADR 0046) and is built from those three. The native transport
//! fills it from the HTTP status and body, and spells a refusal as the CLI does,
//! `HTTP <status>`, so a backend reads both the same way. When FW-2a/b move a recorded contract
//! to a status-based answer, they move it on purpose, as ADR 0046 requires.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::{Failure, Forge, ForgeError, Kind};

/// An HTTP method other than `GET`, which is what a [`Call`] with no method sends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Method {
    Post,
    Put,
    Patch,
    Delete,
}

impl Method {
    pub fn word(self) -> &'static str {
        match self {
            Method::Post => "POST",
            Method::Put => "PUT",
            Method::Patch => "PATCH",
            Method::Delete => "DELETE",
        }
    }
}

/// One field of a request's body, or one GraphQL variable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Field {
    /// Sent as a literal string. Every value that is not charter's own: a branch, a title, a
    /// body. The CLI transport sends it with `-f`, which never reads a leading `@` as a file
    /// (charter #323).
    Text(String, String),
    /// Typed, so `true` is a boolean and `12` a number. Only for values charter writes itself.
    /// The CLI transport sends it with `-F`.
    Typed(String, String),
}

impl Field {
    pub fn text(name: &str, value: &str) -> Field {
        Field::Text(name.to_string(), value.to_string())
    }

    pub fn typed(name: &str, value: &str) -> Field {
        Field::Typed(name.to_string(), value.to_string())
    }
}

/// Where a request goes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Endpoint {
    /// A REST path relative to the API root, query included: `GET` unless `method` says
    /// otherwise.
    Rest {
        method: Option<Method>,
        path: String,
    },
    /// GitHub's GraphQL endpoint. The document is the first field, named `query`.
    Graphql,
}

/// One request, built once by a backend and sent by whichever transport the account uses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Call {
    pub endpoint: Endpoint,
    #[serde(default)]
    pub fields: Vec<Field>,
    /// How long the transport waits. Not part of what a recording matches.
    #[serde(skip, default = "list_timeout")]
    pub timeout: Duration,
}

/// The timeout a deserialized `Call` (a recording) is given: the strict budget. A recording
/// never sends, so it is never waited on.
fn list_timeout() -> Duration {
    super::LIST_TIMEOUT
}

impl Call {
    /// A `GET` of `path`.
    pub fn get(path: impl Into<String>, timeout: Duration) -> Call {
        Call {
            endpoint: Endpoint::Rest {
                method: None,
                path: path.into(),
            },
            fields: Vec::new(),
            timeout,
        }
    }

    /// A write to `path` with `fields`.
    pub fn write(method: Method, path: impl Into<String>, fields: Vec<Field>) -> Call {
        Call {
            endpoint: Endpoint::Rest {
                method: Some(method),
                path: path.into(),
            },
            fields,
            timeout: super::LIST_TIMEOUT,
        }
    }

    /// A GraphQL document with its variables.
    pub fn graphql(query: &str, variables: Vec<Field>, timeout: Duration) -> Call {
        let mut fields = vec![Field::text("query", query)];
        fields.extend(variables);
        Call {
            endpoint: Endpoint::Graphql,
            fields,
            timeout,
        }
    }

    /// The REST path, or `graphql`: what an error message names.
    pub fn path(&self) -> &str {
        match &self.endpoint {
            Endpoint::Rest { path, .. } => path,
            Endpoint::Graphql => "graphql",
        }
    }
}

/// What a transport got back: an exit code, the answer, and the refusal's own words, and, from
/// the native transport, the HTTP status and headers.
///
/// The CLI transport fills only the first three: `gh api` without `--include` prints no status
/// or headers, and its argv is pinned (ADR 0046). The native transport fills all five, so a
/// refusal's kind ([`super::Failure`]), the `Link` header and the ETag are read from it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reply {
    pub code: i32,
    #[serde(default)]
    pub out: String,
    #[serde(default)]
    pub err: String,
    /// The HTTP status, when the transport saw one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<u16>,
    /// The response headers the backend reads (`etag`, `link`, `x-ratelimit-*`), lower-case,
    /// when the transport saw them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub headers: Vec<(String, String)>,
}

impl Reply {
    /// An answer with no status or headers: what the CLI transport gets back.
    pub fn of(code: i32, out: String, err: String) -> Reply {
        Reply {
            code,
            out,
            err,
            status: None,
            headers: Vec::new(),
        }
    }

    /// The value of header `name` (lower-case), when the transport saw it.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }

    /// What kind of failure a refusal is: from its HTTP status when the transport saw one,
    /// and [`Failure::Unrecognised`] when it did not (the CLI's sentence is not parsed for it).
    pub fn failure(&self) -> Failure {
        let spent = self.header("x-ratelimit-remaining") == Some("0")
            || self.header("retry-after").is_some();
        match self.status {
            Some(401) => Failure::Auth,
            Some(403 | 429) if spent => Failure::RateLimited {
                reset: self
                    .header("x-ratelimit-reset")
                    .and_then(|r| r.parse().ok()),
            },
            Some(403) => Failure::Forbidden,
            Some(404) => Failure::NotFound,
            Some(409 | 422) => Failure::Conflict,
            _ => Failure::Unrecognised,
        }
    }

    /// Whether the forge answered the request, rather than refusing it.
    pub fn ok(&self) -> bool {
        self.code == 0
    }

    /// A failed call's own words: stderr, else stdout, else the exit. Python's `detail`.
    pub fn said(&self, kind: Kind) -> String {
        let said = if !self.err.trim().is_empty() {
            self.err.trim()
        } else {
            self.out.trim()
        };
        if said.is_empty() {
            format!("{} exited {}", kind.cli(), self.code)
        } else {
            said.to_string()
        }
    }

    /// Both streams, for a check that looks for the forge's status in either.
    pub fn both(&self) -> String {
        format!("{} {}", self.out, self.err)
    }
}

/// How a request failed to get any answer at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NoAnswer {
    /// The deadline passed. Carries Python's `ProcTimeout` sentence.
    Timeout(String),
    /// It could not be sent: the CLI is missing, or nothing recorded this request.
    Missing(String),
}

/// What carries a [`Call`] to a forge and brings back its [`Reply`] (ADR 0070 §3).
///
/// Chosen once, when a backend is built for an account, and never switched after a request has
/// failed (ADR 0070 §5): a retry under a different transport could be a different identity.
pub trait Transport: Send + Sync {
    /// Send one request to `forge`.
    fn send(&self, forge: &Forge, call: &Call) -> Result<Reply, NoAnswer>;

    /// Refuse unless this transport can speak to `forge` as someone: for the CLI transport,
    /// that the CLI is installed and logged in for the host.
    fn check_auth(&self, forge: &Forge) -> Result<(), ForgeError>;
}
