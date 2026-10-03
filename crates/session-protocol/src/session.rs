//! The session protocol: what a control lane's frames mean (FD-26, ADR 0068 §4).
//!
//! **It is small, public and versioned.** A handful of commands (list, attach and detach,
//! write, resize, answer, stop, start, subscribe and unsubscribe) and the events the host
//! pushes. It is the half of the control lane that carries the compatibility promise (E5): a
//! client and a host one version apart (N−1) always talk. `charter attach`, the fleet MCP and
//! a runner's link are its clients, and LV-2a publishes it on its own.
//!
//! **The other half is the UI RPC** ([`crate::ui`]): what the app's window needs beyond this,
//! private to one build of the app and excluded from the compatibility promise. Both ride the
//! same control lane, each frame tagged with which it is, so a link carries both side by side.
//!
//! **Its version is the link's.** [`speaks`] is what a client offers and a host answers with
//! ([`crate::version`]), so the version negotiated before admission is this protocol's. A
//! minor only adds: a command, a field, an event kind. So a reader ignores fields it does not
//! know, and a host answers a command it does not know with [`UNKNOWN_COMMAND`] and carries on,
//! which is how a newer client learns an older host lacks it. A new major is a break, and a
//! host keeps speaking the major before its own.
//!
//! **On the wire,** each control frame is one JSON object with one key saying what it is:
//!
//! - `{"call":{"id":7,"command":"stop","chat":"<ULID>"}}`, a command, from the client;
//! - `{"reply":{"re":7,"ok":"done"}}` or `{"reply":{"re":7,"refused":{"code":"…","why":"…"}}}`,
//!   its answer, from the host, in any order relative to other answers;
//! - `{"pushed":{"event":{…}}}` or `{"pushed":{"missed":{…}}}`, from the host to a client that
//!   subscribed: ADR 0066's envelope, or word that events were missed, never a silent gap;
//! - `{"ui":{…}}`, a frame of the UI RPC ([`crate::ui`]).
//!
//! Every word on the wire is snake_case: keys, command words, answer kinds, refusal codes and
//! the words a value carries, such as a chat's state (`needs_you`). The one exception is a
//! pushed event's `kind`, which keeps ADR 0066's dotted names (`run.started`). A project is named by its
//! stable id (`[project] id` in its `charter.toml`), never by a path. The whole format is
//! recorded in ADR 0068, *Amended by FD-26* (V76).
//!
//! Terminal bytes never cross as JSON: an attach opens a view ([`crate::view`]) on a stream of
//! its own, and only the bytes typed into a chat ride the lane, in base64.
//!
//! `tests/fixtures/session-protocol.jsonl` records a frame of every kind this version says.
//! A test reads each back as it was recorded, so a change that would stop an older peer
//! reading one fails CI: that file is the compatibility promise, written down. Each variant's
//! word comes from an exhaustive `match` ([`Command::word`] and its kin), so a new variant does
//! not compile without one, and fails that test until its frame is recorded.

use std::collections::{BTreeMap, HashMap};

use bytes::Bytes;
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot};

use crate::auth::Scope;
use crate::link::{Acceptor, Link, LinkError, Opener};
use crate::ui;
use crate::version::{Speaks, Version};

/// The session protocol's current version.
pub const VERSION: Version = Version { major: 1, minor: 0 };

/// What a client offers and a host answers with: this version, and from the second major on,
/// the major before it too (N−1).
pub fn speaks() -> Speaks {
    Speaks::new([VERSION])
}

/// Every command word this version knows: [`Command::WORDS`].
pub const COMMANDS: &[&str] = Command::WORDS;

/// Gives an enum its wire words from ONE list: `word()`, an exhaustive `match`, so a variant
/// added without a word does not compile, and `WORDS`, every word in the same order. The words
/// are what serde writes (`rename_all = "snake_case"`), and a test holds the two to each other
/// over every recorded frame.
macro_rules! wire_words {
    ($ty:ident { $($variant:ident => $word:literal),* $(,)? }) => {
        impl $ty {
            /// Every word this version says for this kind of frame.
            pub const WORDS: &'static [&'static str] = &[$($word),*];

            /// This variant's word on the wire.
            pub fn word(&self) -> &'static str {
                match self {
                    $($ty::$variant { .. } => $word,)*
                }
            }
        }
    };
}

/// A command this host does not know: a newer client's, which it can do without.
pub const UNKNOWN_COMMAND: &str = "unknown_command";
/// A command this host knows, sent in a shape it cannot read.
pub const MALFORMED: &str = "malformed";
/// The chat named is not one the host holds.
pub const NO_SUCH_CHAT: &str = "no_such_chat";
/// The project named is not one the host holds.
pub const NO_SUCH_PROJECT: &str = "no_such_project";
/// The link's scope may not do this (FD-27).
pub const NOT_ALLOWED: &str = "not_allowed";

/// Why [`Command::Answer`] is refused on every link for now: no host tells a human scope from
/// any other yet (FD-27, #664), and only a human answers an ask (V16, V75).
pub const ANSWERED_IN_THE_WINDOW: &str = "an ask is answered in charter's window: no link may answer one until client scopes are checked";

/// A command, as the client sends it. Unknown fields are ignored, so a later minor may add
/// some.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum Command {
    /// The chats the host holds.
    List,
    /// Open a view of a chat's terminal: the host opens a stream for it ([`crate::view`]),
    /// whose snapshot then live bytes follow, and answers [`Answer::Attached`].
    Attach { chat: String },
    /// Close a view the client attached. The chat keeps running.
    Detach { chat: String, view: u32 },
    /// Bytes typed into a chat's terminal, as they are: text, or a mouse report that is not
    /// text at all.
    Write {
        chat: String,
        #[serde(with = "base64_bytes")]
        bytes: Vec<u8>,
    },
    /// A chat's terminal has a new size.
    Resize { chat: String, cols: u16, rows: u16 },
    /// The answer to one of a chat's asks (a needs-you). Refused on every link for now, with
    /// [`NOT_ALLOWED`] ([`ANSWERED_IN_THE_WINDOW`]).
    Answer {
        chat: String,
        ask: String,
        answer: String,
    },
    /// End a chat and everything it started.
    Stop { chat: String },
    /// Start a chat.
    Start(Start),
    /// Every event after `since`, then each as it is written ([`Pushed`]): ADR 0066's
    /// `subscribe(since)`.
    Subscribe { since: Since },
    /// Stop pushing events.
    Unsubscribe,
}

/// What a [`Command::Start`] asks for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Start {
    /// The project the chat is in, by its stable id: the ULID in its `charter.toml`
    /// (`[project].id`, V76). Never a path, which differs from machine to machine.
    pub project: String,
    /// Its workspace, or the project's root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
    /// Its harness, or the project's default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub harness: Option<String>,
}

/// A subscription's cursor: for each device, the last `seq` the client holds of it (ADR 0066).
/// Empty is from the start.
pub type Since = BTreeMap<String, u64>;

/// What the host answers a command with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Answer {
    /// The chats it holds: [`Command::List`].
    Chats(Vec<Chat>),
    /// The view it opened: [`Command::Attach`].
    Attached(Attached),
    /// The chat it started: [`Command::Start`].
    Started { chat: String },
    /// Events follow, as [`Pushed`] frames: [`Command::Subscribe`].
    Subscribed,
    /// Done, with nothing to say.
    Done,
}

/// One chat, as [`Command::List`] lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Chat {
    /// Its id, a ULID (ADR 0066).
    pub chat: String,
    /// The project it is in, by its stable id ([`Start::project`]).
    pub project: String,
    /// Where that project is on the host's machine. A hint to show a person, never a key: the
    /// same project is at another path on another machine (V76).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_path: Option<String>,
    /// What the window calls it, where it has a name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Its state as its hooks report it, as a snake_case word (`working`, `needs_you`, `idle`,
    /// `ended`); kebab-case is UI text only (V76). A word and not a closed set, so a later
    /// minor may add one.
    pub state: String,
}

/// The view an attach opened.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attached {
    /// The view's number, which its stream's header carries ([`crate::view::ViewId`]).
    pub view: u32,
    /// The size its snapshot was drawn for.
    pub cols: u16,
    pub rows: u16,
}

/// Why the host refused a command: a code a program reads, and a sentence a person reads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[error("{why} ({code})")]
pub struct Refusal {
    pub code: String,
    pub why: String,
}

/// What the host pushes to a client that subscribed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Pushed {
    Event(Event),
    /// The events of `device` after `after` up to `resumes_at` are no longer kept: the stream
    /// carries on from `resumes_at`, and a client rebuilds what it built from them.
    ///
    /// **It carries no snapshot (V76).** A client that missed events and shows a chat's
    /// terminal attaches it again ([`Command::Attach`]): the view's own stream opens with the
    /// terminal's snapshot, so there is one way to get a screen back, for a fallen-behind view,
    /// a reconnect and a gap alike. Doing so is the client's duty.
    Missed {
        device: String,
        after: u64,
        resumes_at: u64,
    },
}

wire_words!(Command {
    List => "list",
    Attach => "attach",
    Detach => "detach",
    Write => "write",
    Resize => "resize",
    Answer => "answer",
    Stop => "stop",
    Start => "start",
    Subscribe => "subscribe",
    Unsubscribe => "unsubscribe",
});

wire_words!(Answer {
    Chats => "chats",
    Attached => "attached",
    Started => "started",
    Subscribed => "subscribed",
    Done => "done",
});

wire_words!(Frame {
    Call => "call",
    Reply => "reply",
    Pushed => "pushed",
    Ui => "ui",
});

wire_words!(Outcome {
    Ok => "ok",
    Refused => "refused",
});

wire_words!(Pushed {
    Event => "event",
    Missed => "missed",
});

/// One event, in ADR 0066's envelope.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub v: u32,
    pub device_id: String,
    pub seq: u64,
    pub ulid: String,
    pub chat: Option<String>,
    pub run: Option<String>,
    pub parent_run: Option<String>,
    pub kind: String,
    pub body: serde_json::Value,
}

/// One frame of the control lane: the session protocol's, or the UI RPC's.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Frame {
    Call(Call),
    Reply(Reply),
    Pushed(Pushed),
    Ui(ui::Frame),
}

/// A command and the number its answer comes back under.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Call {
    pub id: u64,
    #[serde(flatten)]
    pub command: Command,
}

/// The answer to the call numbered `re`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reply {
    pub re: u64,
    #[serde(flatten)]
    pub outcome: Outcome,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Ok(Answer),
    Refused(Refusal),
}

impl Frame {
    pub(crate) fn to_bytes(&self) -> Bytes {
        Bytes::from(serde_json::to_vec(self).expect("a frame is always JSON"))
    }
}

/// Who a command came from, as the host's [`Host`] sees it.
pub struct Peer {
    scope: Scope,
    version: Version,
    views: Opener,
}

impl Peer {
    /// The scope the link was admitted as ([`crate::auth`]).
    pub fn scope(&self) -> Scope {
        self.scope
    }

    /// The version the link agreed on.
    pub fn version(&self) -> Version {
        self.version
    }

    /// Where an attach opens its view ([`crate::view::Attacher::new`]).
    pub fn views(&self) -> &Opener {
        &self.views
    }
}

/// What answers the session protocol: the host (FD-5).
pub trait Host: Send + Sync + 'static {
    /// Answer one command. [`Command::Subscribe`] and [`Command::Unsubscribe`] never come
    /// here: they go to [`Host::subscribe`], and the link holds the subscription.
    fn call(
        &self,
        command: Command,
        peer: &Peer,
    ) -> impl Future<Output = Result<Answer, Refusal>> + Send;

    /// Every event after `since`, then each as it is written, until the receiver is dropped.
    fn subscribe(
        &self,
        since: Since,
        peer: &Peer,
    ) -> impl Future<Output = Result<mpsc::Receiver<Pushed>, Refusal>> + Send {
        let _ = (since, peer);
        async {
            Err(Refusal {
                code: UNKNOWN_COMMAND.into(),
                why: "this host keeps no events".into(),
            })
        }
    }
}

/// Serve the session protocol, and the UI RPC beside it when `ui` is given, on a link the
/// host admitted, until the client closes it. Commands are answered in the order they come.
pub async fn serve<H: Host>(
    mut link: Link,
    host: H,
    ui: Option<ui::Server>,
) -> Result<(), LinkError> {
    let peer = Peer {
        scope: link.scope(),
        version: link.version(),
        views: link.opener(),
    };
    let mut ui = ui::Serving::new(ui);
    let mut events: Option<mpsc::Receiver<Pushed>> = None;
    loop {
        let next = tokio::select! {
            frame = link.control().next() => Next::Frame(frame),
            pushed = next_pushed(&mut events) => Next::Pushed(pushed),
        };
        let frame = match next {
            Next::Pushed(Some(pushed)) => {
                link.control()
                    .send(Frame::Pushed(pushed).to_bytes())
                    .await?;
                continue;
            }
            Next::Pushed(None) => {
                events = None;
                continue;
            }
            Next::Frame(None) => return Ok(()),
            Next::Frame(Some(frame)) => frame?,
        };
        let answer = match serde_json::from_slice::<Frame>(&frame) {
            Ok(Frame::Call(Call { id, command })) => {
                let outcome = match command {
                    Command::Subscribe { since } => match host.subscribe(since, &peer).await {
                        Ok(receiver) => {
                            events = Some(receiver);
                            Outcome::Ok(Answer::Subscribed)
                        }
                        Err(refusal) => Outcome::Refused(refusal),
                    },
                    Command::Unsubscribe => {
                        events = None;
                        Outcome::Ok(Answer::Done)
                    }
                    // Refused here, for every host (HP-6): only a human scope answers an ask
                    // (V16, V75), and telling one link from another is FD-27's scope check
                    // (#664). Until a host has it, an ask is answered in the window.
                    Command::Answer { .. } => Outcome::Refused(Refusal {
                        code: NOT_ALLOWED.into(),
                        why: ANSWERED_IN_THE_WINDOW.into(),
                    }),
                    command => match host.call(command, &peer).await {
                        Ok(answer) => Outcome::Ok(answer),
                        Err(refusal) => Outcome::Refused(refusal),
                    },
                };
                Some(Frame::Reply(Reply { re: id, outcome }))
            }
            Ok(Frame::Ui(frame)) => ui.answer(frame, &peer).await.map(Frame::Ui),
            // Only a host sends these; a client that does is ignored, not believed.
            Ok(Frame::Reply(_) | Frame::Pushed(_)) => None,
            Err(_) => unreadable_call(&frame),
        };
        if let Some(answer) = answer {
            link.control().send(answer.to_bytes()).await?;
        }
    }
}

enum Next {
    Frame(Option<Result<Bytes, LinkError>>),
    Pushed(Option<Pushed>),
}

async fn next_pushed(events: &mut Option<mpsc::Receiver<Pushed>>) -> Option<Pushed> {
    match events {
        Some(events) => events.recv().await,
        None => std::future::pending().await,
    }
}

/// A frame this host cannot read. A call it can find the number of is refused, so the client
/// hears why and the link carries on: a command from a later minor is [`UNKNOWN_COMMAND`]. Any
/// other is a frame kind from a later version, and is ignored.
fn unreadable_call(frame: &[u8]) -> Option<Frame> {
    let value: serde_json::Value = serde_json::from_slice(frame).ok()?;
    let call = value.get("call")?;
    let id = call.get("id")?.as_u64()?;
    let word = call.get("command").and_then(|c| c.as_str());
    let refusal = match word {
        Some(word) if COMMANDS.contains(&word) => Refusal {
            code: MALFORMED.into(),
            why: format!("a `{word}` command this host cannot read"),
        },
        Some(word) => Refusal {
            code: UNKNOWN_COMMAND.into(),
            why: format!("this host does not know the command `{word}`"),
        },
        None => Refusal {
            code: MALFORMED.into(),
            why: "a call that names no command".into(),
        },
    };
    Some(Frame::Reply(Reply {
        re: id,
        outcome: Outcome::Refused(refusal),
    }))
}

/// Why a call got no answer.
#[derive(Debug, thiserror::Error)]
pub enum CallError {
    /// The host refused it.
    #[error(transparent)]
    Refused(#[from] Refusal),
    /// The link ended before the answer came.
    #[error("the link ended before the host answered: {0}")]
    Closed(String),
    /// The host answered with something that is not an answer to this command.
    #[error("the host answered {0}, which is not an answer to this command")]
    Unexpected(String),
}

impl CallError {
    /// The host's refusal, when that is what this is.
    pub fn refusal(&self) -> Option<&Refusal> {
        match self {
            CallError::Refused(refusal) => Some(refusal),
            _ => None,
        }
    }
}

/// What a client asks its link's driver to do.
pub(crate) enum Ask {
    Session(Command, oneshot::Sender<Result<Answer, CallError>>),
    Ui(ui::Ask),
}

/// The client's end of the session protocol: calls from any task, answered in any order.
pub struct Client {
    asks: mpsc::UnboundedSender<Ask>,
    version: Version,
    scope: Scope,
}

/// The events a subscription pushes, in order.
pub type Events = mpsc::UnboundedReceiver<Pushed>;

impl Client {
    /// Speak the session protocol on `link`. Answers with the client, the streams the host
    /// opens (each attach's view, [`crate::view::Viewer::accept`]), and the events it pushes
    /// once subscribed.
    pub fn new(mut link: Link) -> (Client, Acceptor, Events) {
        let views = link.acceptor();
        let (asks, asked) = mpsc::unbounded_channel();
        let (pushed, events) = mpsc::unbounded_channel();
        let client = Client {
            asks,
            version: link.version(),
            scope: link.scope(),
        };
        tokio::spawn(drive(link, asked, pushed));
        (client, views, events)
    }

    /// The version the link agreed on.
    pub fn version(&self) -> Version {
        self.version
    }

    /// The scope the link was admitted as.
    pub fn scope(&self) -> Scope {
        self.scope
    }

    /// Send one command and wait for its answer.
    pub async fn call(&self, command: Command) -> Result<Answer, CallError> {
        let (reply, answer) = oneshot::channel();
        self.asks
            .send(Ask::Session(command, reply))
            .map_err(|_| CallError::Closed("the link is closed".into()))?;
        answer
            .await
            .map_err(|_| CallError::Closed("the link is closed".into()))?
    }

    pub async fn list(&self) -> Result<Vec<Chat>, CallError> {
        match self.call(Command::List).await? {
            Answer::Chats(chats) => Ok(chats),
            other => Err(unexpected(&other)),
        }
    }

    pub async fn attach(&self, chat: &str) -> Result<Attached, CallError> {
        match self.call(Command::Attach { chat: chat.into() }).await? {
            Answer::Attached(attached) => Ok(attached),
            other => Err(unexpected(&other)),
        }
    }

    pub async fn detach(&self, chat: &str, view: u32) -> Result<(), CallError> {
        self.done(Command::Detach {
            chat: chat.into(),
            view,
        })
        .await
    }

    pub async fn write(&self, chat: &str, bytes: &[u8]) -> Result<(), CallError> {
        self.done(Command::Write {
            chat: chat.into(),
            bytes: bytes.to_vec(),
        })
        .await
    }

    pub async fn resize(&self, chat: &str, cols: u16, rows: u16) -> Result<(), CallError> {
        self.done(Command::Resize {
            chat: chat.into(),
            cols,
            rows,
        })
        .await
    }

    pub async fn answer(&self, chat: &str, ask: &str, answer: &str) -> Result<(), CallError> {
        self.done(Command::Answer {
            chat: chat.into(),
            ask: ask.into(),
            answer: answer.into(),
        })
        .await
    }

    pub async fn stop(&self, chat: &str) -> Result<(), CallError> {
        self.done(Command::Stop { chat: chat.into() }).await
    }

    /// Start a chat, and answer with its id.
    pub async fn start(&self, start: Start) -> Result<String, CallError> {
        match self.call(Command::Start(start)).await? {
            Answer::Started { chat } => Ok(chat),
            other => Err(unexpected(&other)),
        }
    }

    /// Subscribe from `since`: the events come on the [`Events`] [`Client::new`] gave.
    pub async fn subscribe(&self, since: Since) -> Result<(), CallError> {
        match self.call(Command::Subscribe { since }).await? {
            Answer::Subscribed => Ok(()),
            other => Err(unexpected(&other)),
        }
    }

    pub async fn unsubscribe(&self) -> Result<(), CallError> {
        self.done(Command::Unsubscribe).await
    }

    /// Open the UI RPC beside the session protocol, as one build of the app: the host serves
    /// it only to `local-ui`, and only to its own build ([`crate::ui`]).
    pub async fn ui(&self, build: &str) -> Result<ui::Client, CallError> {
        ui::Client::open(self.asks.clone(), build).await
    }

    async fn done(&self, command: Command) -> Result<(), CallError> {
        match self.call(command).await? {
            Answer::Done => Ok(()),
            other => Err(unexpected(&other)),
        }
    }
}

fn unexpected(answer: &Answer) -> CallError {
    CallError::Unexpected(serde_json::to_string(answer).unwrap_or_default())
}

/// The client's driver: it numbers each call, sends it, and hands each answer to whoever
/// waits for it, and each pushed event to [`Events`].
async fn drive(
    mut link: Link,
    mut asked: mpsc::UnboundedReceiver<Ask>,
    pushed: mpsc::UnboundedSender<Pushed>,
) {
    let mut next_id: u64 = 1;
    let mut waiting: HashMap<u64, oneshot::Sender<Result<Answer, CallError>>> = HashMap::new();
    let mut ui = ui::Waiting::default();
    let why = loop {
        let next = tokio::select! {
            ask = asked.recv() => Driven::Ask(ask),
            frame = link.control().next() => Driven::Frame(frame),
        };
        match next {
            // Every handle is gone: nobody is left to answer.
            Driven::Ask(None) => return,
            Driven::Ask(Some(ask)) => {
                let id = next_id;
                next_id += 1;
                let frame = match ask {
                    Ask::Session(command, reply) => {
                        waiting.insert(id, reply);
                        Frame::Call(Call { id, command })
                    }
                    Ask::Ui(ask) => Frame::Ui(ui.ask(id, ask)),
                };
                if let Err(e) = link.control().send(frame.to_bytes()).await {
                    break e.to_string();
                }
            }
            Driven::Frame(None) => break "the host closed the link".to_owned(),
            Driven::Frame(Some(Err(e))) => break e.to_string(),
            Driven::Frame(Some(Ok(frame))) => match serde_json::from_slice::<Frame>(&frame) {
                Ok(Frame::Reply(Reply { re, outcome })) => {
                    if let Some(reply) = waiting.remove(&re) {
                        let _ = reply.send(match outcome {
                            Outcome::Ok(answer) => Ok(answer),
                            Outcome::Refused(refusal) => Err(CallError::Refused(refusal)),
                        });
                    }
                }
                Ok(Frame::Pushed(event)) => {
                    let _ = pushed.send(event);
                }
                Ok(Frame::Ui(frame)) => ui.answered(frame),
                // A reply this client cannot read fails its call, rather than leaving it to
                // wait for an answer that has come.
                Err(e) => {
                    if let Some(reply) = unreadable_reply(&frame).and_then(|re| waiting.remove(&re))
                    {
                        let _ = reply.send(Err(CallError::Unexpected(format!(
                            "a reply this client cannot read ({e})"
                        ))));
                    }
                }
                // A call from the host: not this client's to answer.
                Ok(Frame::Call(_)) => {}
            },
        }
    };
    for (_, reply) in waiting.drain() {
        let _ = reply.send(Err(CallError::Closed(why.clone())));
    }
    ui.closed(&why);
}

/// The number of the call a frame that did not read as a frame answers, if it is a reply.
fn unreadable_reply(frame: &[u8]) -> Option<u64> {
    let value: serde_json::Value = serde_json::from_slice(frame).ok()?;
    value.get("reply")?.get("re")?.as_u64()
}

enum Driven {
    Ask(Option<Ask>),
    Frame(Option<Result<Bytes, LinkError>>),
}

/// Bytes as standard base64 in JSON, so a keystroke that is not UTF-8 crosses whole.
mod base64_bytes {
    use base64::Engine;
    use base64::engine::general_purpose::STANDARD;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(bytes: &[u8], to: S) -> Result<S::Ok, S::Error> {
        to.serialize_str(&STANDARD.encode(bytes))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(from: D) -> Result<Vec<u8>, D::Error> {
        let text = String::deserialize(from)?;
        STANDARD.decode(text).map_err(serde::de::Error::custom)
    }
}
