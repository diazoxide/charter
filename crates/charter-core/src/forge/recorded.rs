//! A transport that answers from recorded exchanges, and fails every request nobody recorded
//! (ADR 0070 §7, the recorded run).
//!
//! The contract suite (`tests/forge_contract.rs`) runs each backend operation through it,
//! once per forge. A recording is a JSON file: the source its answers were taken from, and the
//! exchanges in the order they are expected.
//!
//! ```json
//! {"source": "GitLab 19.4 REST API docs, merge_requests.md",
//!  "auth": "ok",
//!  "exchanges": [{"call": {"endpoint": {"rest": {"method": null, "path": "…"}}, "fields": []},
//!                 "reply": {"code": 0, "out": "[]"}}]}
//! ```
//!
//! Each exchange is answered once, in order of recording among equal requests, as the stand-in
//! CLI does. A request that matches none is [`NoAnswer::Missing`] naming the request, and
//! [`Recorded::unspent`] names every exchange nobody asked for, so a test sees both a request
//! too many and a request too few.

use std::sync::Mutex;

use serde::Deserialize;

use super::transport::{Call, NoAnswer, Reply, Transport};
use super::{Forge, ForgeError};

/// One recorded request and its answer.
#[derive(Debug, Clone, Deserialize)]
pub struct Exchange {
    pub call: Call,
    pub reply: Reply,
}

#[derive(Debug, Deserialize)]
struct File {
    #[allow(dead_code)]
    source: String,
    /// What the auth check answers: `"ok"`, or the refusal's words.
    #[serde(default)]
    auth: Option<String>,
    #[serde(default)]
    exchanges: Vec<Exchange>,
}

/// The recorded transport.
#[derive(Debug)]
pub struct Recorded {
    auth: Option<String>,
    exchanges: Mutex<Vec<(Exchange, bool)>>,
}

impl Recorded {
    /// A recording read from its JSON text.
    pub fn parse(text: &str) -> Result<Recorded, String> {
        let file: File = serde_json::from_str(text).map_err(|e| format!("a recording: {e}"))?;
        Ok(Recorded {
            auth: file.auth,
            exchanges: Mutex::new(file.exchanges.into_iter().map(|e| (e, false)).collect()),
        })
    }

    /// Every recorded exchange nobody asked for, as the request it was recorded for.
    pub fn unspent(&self) -> Vec<Call> {
        self.exchanges
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .filter(|(_, spent)| !spent)
            .map(|(e, _)| e.call.clone())
            .collect()
    }
}

/// Whether two requests are the same request: endpoint and fields. How long a transport would
/// wait is not part of what was asked.
fn same(a: &Call, b: &Call) -> bool {
    a.endpoint == b.endpoint && a.fields == b.fields
}

impl Transport for Recorded {
    fn send(&self, _forge: &Forge, call: &Call) -> Result<Reply, NoAnswer> {
        let mut exchanges = self.exchanges.lock().unwrap_or_else(|e| e.into_inner());
        match exchanges
            .iter_mut()
            .find(|(e, spent)| !spent && same(&e.call, call))
        {
            Some((exchange, spent)) => {
                *spent = true;
                Ok(exchange.reply.clone())
            }
            None => Err(NoAnswer::Missing(format!(
                "nothing recorded this request: {}",
                serde_json::to_string(call).unwrap_or_default()
            ))),
        }
    }

    fn check_auth(&self, forge: &Forge) -> Result<(), ForgeError> {
        match self.auth.as_deref() {
            Some("ok") => Ok(()),
            Some(refused) => Err(ForgeError(refused.to_string())),
            None => Err(ForgeError(format!(
                "nothing recorded the auth check for {}",
                forge.host
            ))),
        }
    }
}
