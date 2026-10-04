//! The UI RPC: what the app's window asks of the host beyond the session protocol (FD-26, ADR
//! 0068 §4).
//!
//! **It is private, and promises nothing across versions.** Its methods are the app's own
//! commands, generated from `ipc_commands.rs` with a typed TypeScript client, and they change
//! whenever the app does. So it is excluded from the compatibility tests that hold the session
//! protocol ([`crate::session`]), and a host serves it only to the build that it is:
//!
//! 1. The window opens it with `{"ui":{"hello":{"build":"…"}}}`, naming its build.
//! 2. The host answers `{"ui":{"welcome":{"build":"…"}}}` only when that is its own build, and
//!    the link's scope is `local-ui`, the app's window (ADR 0068 §5). Otherwise it answers
//!    `{"ui":{"refused":{"why":"…"}}}`, and the session protocol on the same link carries on:
//!    across an upgrade, the window and the host are two builds, and the session protocol alone
//!    carries that (ADR 0068 §7).
//! 3. Then `{"ui":{"call":{"id":3,"method":"rename_chat","args":{…}}}}`, answered
//!    `{"ui":{"reply":{"re":3,"ok":…}}}` or `{"ui":{"reply":{"re":3,"err":…}}}`. `method` and
//!    `args` are what the window's `invoke` sends today: the command's name, and its arguments
//!    by name. `err` is the command's own error, as its typed client reads it.
//!
//! A call before the welcome, or of a method the host does not serve, is answered `err` and
//! goes no further.

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::{mpsc, oneshot};

use crate::grants::Power;
use crate::session::{Ask as LinkAsk, CallError, Peer};

/// One frame of the UI RPC, inside the control lane's `{"ui":…}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Frame {
    Hello { build: String },
    Welcome { build: String },
    Refused { why: String },
    Call(Call),
    Reply(Reply),
}

/// One method call, numbered for its reply.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Call {
    pub id: u64,
    pub method: String,
    #[serde(default)]
    pub args: Value,
}

/// The reply to the call numbered `re`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Reply {
    pub re: u64,
    #[serde(flatten)]
    pub outcome: Outcome,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Ok(Value),
    Err(Value),
}

/// What answers a UI RPC method: the host's side of the app's commands.
pub trait Handler: Send + Sync + 'static {
    fn call(&self, method: String, args: Value) -> BoxFuture<'_, Result<Value, Value>>;
}

/// The UI RPC a host serves: its build, the methods it answers, and what answers them.
#[derive(Clone)]
pub struct Server {
    build: String,
    methods: BTreeSet<String>,
    handler: Arc<dyn Handler>,
}

/// The app's commands no link ever serves, whatever list a host is built with: they are the
/// window's alone, over Tauri's IPC.
///
/// `answer_ask` answers a chat's ask as the operator (HP-6). Only a human scope answers one (V16,
/// V75). On a link that is the session protocol's `answer`, which [`crate::grants`]' table
/// checks (FD-27); the UI RPC does not carry a second way to do it.
pub const WINDOW_ONLY: &[&str] = &["answer_ask"];

impl Server {
    /// `methods` is the app's command list (`ipc_commands.rs`), or the part of it this host
    /// answers; a method not in it, or in [`WINDOW_ONLY`], is refused before `handler` is asked.
    pub fn new(
        build: impl Into<String>,
        methods: impl IntoIterator<Item = impl Into<String>>,
        handler: impl Handler,
    ) -> Server {
        Server {
            build: build.into(),
            methods: methods
                .into_iter()
                .map(Into::into)
                .filter(|method: &String| !WINDOW_ONLY.contains(&method.as_str()))
                .collect(),
            handler: Arc::new(handler),
        }
    }
}

/// The host's UI RPC on one link: whether the window has been welcomed.
pub(crate) struct Serving {
    server: Option<Server>,
    open: bool,
}

impl Serving {
    pub(crate) fn new(server: Option<Server>) -> Serving {
        Serving {
            server,
            open: false,
        }
    }

    /// The host's answer to one frame, if it has one.
    pub(crate) async fn answer(&mut self, frame: Frame, peer: &Peer) -> Option<Frame> {
        match frame {
            Frame::Hello { build } => Some(self.hello(&build, peer)),
            Frame::Call(Call { id, method, args }) => {
                let outcome = match (&self.server, self.open) {
                    (Some(server), true) if server.methods.contains(&method) => {
                        match server.handler.call(method, args).await {
                            Ok(value) => Outcome::Ok(value),
                            Err(value) => Outcome::Err(value),
                        }
                    }
                    (Some(_), true) => Outcome::Err(Value::String(format!(
                        "this host serves no UI RPC method `{method}`"
                    ))),
                    _ => Outcome::Err(Value::String(
                        "the UI RPC is not open on this link: say hello first".into(),
                    )),
                };
                Some(Frame::Reply(Reply { re: id, outcome }))
            }
            // Only a host sends these.
            Frame::Welcome { .. } | Frame::Refused { .. } | Frame::Reply(_) => None,
        }
    }

    fn hello(&mut self, build: &str, peer: &Peer) -> Frame {
        // A hello that is refused closes what an earlier one opened.
        self.open = false;
        let refused = |why: String| Frame::Refused { why };
        let Some(server) = &self.server else {
            return refused("this host serves no UI RPC".into());
        };
        if !peer.scope().may(Power::UiRpc) {
            return refused(format!(
                "the UI RPC is the app's window's alone (`local-ui`), and this link is `{}`",
                peer.scope().word()
            ));
        }
        if build != server.build {
            return refused(format!(
                "the UI RPC is private to one build: this host is `{}`, the window `{build}`; \
                 the session protocol carries on",
                server.build
            ));
        }
        self.open = true;
        Frame::Welcome {
            build: server.build.clone(),
        }
    }
}

/// What a [`Client`] asks the link's driver for.
pub(crate) enum Ask {
    Hello(String, oneshot::Sender<Result<(), CallError>>),
    Call(String, Value, Answering),
}

/// Where a call's answer goes: the command's value or its own error, or why none came.
type Answering = oneshot::Sender<Result<Result<Value, Value>, CallError>>;

/// The client driver's UI RPC: who waits for which answer.
#[derive(Default)]
pub(crate) struct Waiting {
    hellos: Vec<oneshot::Sender<Result<(), CallError>>>,
    calls: HashMap<u64, Answering>,
}

impl Waiting {
    /// The frame to send for `ask`, numbered `id`.
    pub(crate) fn ask(&mut self, id: u64, ask: Ask) -> Frame {
        match ask {
            Ask::Hello(build, reply) => {
                self.hellos.push(reply);
                Frame::Hello { build }
            }
            Ask::Call(method, args, reply) => {
                self.calls.insert(id, reply);
                Frame::Call(Call { id, method, args })
            }
        }
    }

    /// A frame the host sent.
    pub(crate) fn answered(&mut self, frame: Frame) {
        match frame {
            Frame::Welcome { .. } => {
                if !self.hellos.is_empty() {
                    let _ = self.hellos.remove(0).send(Ok(()));
                }
            }
            Frame::Refused { why } => {
                if !self.hellos.is_empty() {
                    let _ = self.hellos.remove(0).send(Err(CallError::Refused(
                        crate::session::Refusal {
                            code: crate::session::NOT_ALLOWED.into(),
                            why,
                        },
                    )));
                }
            }
            Frame::Reply(Reply { re, outcome }) => {
                if let Some(reply) = self.calls.remove(&re) {
                    let _ = reply.send(Ok(match outcome {
                        Outcome::Ok(value) => Ok(value),
                        Outcome::Err(value) => Err(value),
                    }));
                }
            }
            Frame::Hello { .. } | Frame::Call(_) => {}
        }
    }

    pub(crate) fn closed(&mut self, why: &str) {
        for reply in self.hellos.drain(..) {
            let _ = reply.send(Err(CallError::Closed(why.to_owned())));
        }
        for (_, reply) in self.calls.drain() {
            let _ = reply.send(Err(CallError::Closed(why.to_owned())));
        }
    }
}

/// The window's end of the UI RPC, once the host welcomed it.
#[derive(Clone)]
pub struct Client {
    asks: mpsc::UnboundedSender<LinkAsk>,
}

impl Client {
    pub(crate) async fn open(
        asks: mpsc::UnboundedSender<LinkAsk>,
        build: &str,
    ) -> Result<Client, CallError> {
        let (reply, welcomed) = oneshot::channel();
        asks.send(LinkAsk::Ui(Ask::Hello(build.to_owned(), reply)))
            .map_err(|_| CallError::Closed("the link is closed".into()))?;
        welcomed
            .await
            .map_err(|_| CallError::Closed("the link is closed".into()))??;
        Ok(Client { asks })
    }

    /// Call `method` with `args`: the command's value, or its own error.
    pub async fn call(&self, method: &str, args: Value) -> Result<Result<Value, Value>, CallError> {
        let (reply, answer) = oneshot::channel();
        self.asks
            .send(LinkAsk::Ui(Ask::Call(method.to_owned(), args, reply)))
            .map_err(|_| CallError::Closed("the link is closed".into()))?;
        answer
            .await
            .map_err(|_| CallError::Closed("the link is closed".into()))?
    }
}
