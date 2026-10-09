//! charter's ACP client (HP-2, ADR 0080): one level-3 chat is one ACP session on one agent
//! process, spoken to over its stdio, with charter as the client and nothing else.
//!
//! [`Chat::start`] spawns the agent from its declared argv (opencode's own ACP mode first,
//! [`crate::harness::Harness::acp_args`]) in its own process group, so the kill switch and a
//! pause reach all of it, and runs the protocol on a thread of its own:
//!
//! - **A level-3 chat starts from the start a terminal chat makes** ([`Launch::for_start`]):
//!   the one profile gate and sandbox decision, then the agent in the chat's environment and
//!   nothing else of the host's. No chat in a sandboxed project runs over ACP, opted out or
//!   not (D-87h); it starts in its terminal.
//! - **`initialize` offers nothing optional** (§2, V28a): no `fs`, no `terminal`, no elicitation.
//!   A call to a client method charter did not offer is refused with *method not found* and
//!   reported as [`Event::Refused`], the input to the audit's `acp.call.refused`.
//! - **The handshake is the fact for the run** (§4): [`Negotiated`] maps what the agent answered
//!   onto harness capabilities. An agent on another protocol version does not start
//!   ([`NotStarted::Version`]), and neither does one that needs a login
//!   ([`NotStarted::Login`]): charter never calls `authenticate` with a value, and never supplies
//!   a credential (V9).
//! - **`session/new`** gets the chat's worktree as `cwd`, and charter's MCP server in `mcpServers`
//!   ([`crate::chattools::acp_server`]), and nothing else.
//! - **What the agent reports becomes the neutral model** (ADR 0073): a turn begins with a prompt
//!   and ends with its response, a plan and usage are [`Said`] values, and the agent's message
//!   text and tool calls are events of their own. Nothing here reads the agent's text to decide
//!   anything.
//! - **A permission request is an ask** (§5): it is read by
//!   [`crate::harness::asked::acp_request_permission`] and raised in the shared
//!   [`Asks`]. Only [`Chat::answer`] answers it, with an [`Answerer`], which is made only from a
//!   human scope (V16, V75), so the agent's own stdio can ask and never answer. An ACP ask has
//!   no deadline (V28d) and nothing answers it on a timer. [`Chat::cancel`] sends
//!   `session/cancel` and answers every waiting request `cancelled`, as ACP requires.
//! - **The kill switch reaches a chat a waiting turn still holds** ([`Chat::kill`]): the
//!   agent's group ends, and every ask it left open is withdrawn as the connection ends.
//!
//! **What the agent writes is bounded.** A line past [`MOST_LINE_BYTES`] ends the chat; a
//! notification charter does not follow, or a request or update naming another session, is
//! dropped or refused; at most [`MOST_OPEN_ASKS`] asks of at most [`MOST_ASK_BYTES`] each are
//! open, and at most [`MOST_UNWRITTEN_BYTES`] wait to be written to the agent before the chat
//! ends; the handshake has [`Launch::patience`], and ends on time even while a
//! process outside the agent's group holds its stdout; and the unread events hold at most
//! [`EVENTS_HELD`] events and [`MOST_BYTES_HELD`] bytes, a long reply in pieces of
//! [`MOST_TEXT_BYTES`] and a plan cut to [`MOST_PLAN_STEPS`] steps of [`MOST_STEP_BYTES`]. The agent's process group is ended whenever the connection ends.
//!
//! **The host has no controlling terminal** (V77): the agent leads its own process group, not
//! its own session, so [`Chat::start`] refuses while the host still has a terminal it could
//! open ([`crate::noterminal`]).
//!
//! The guard's first look at a request (§5.1), "allow always" (V28c), a pause's held answer
//! and an upgrade's handover (§6) are HP-16's and FD-28's, on top of this client.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::schema::v1::{
    CancelNotification, ClientCapabilities, ContentBlock, Implementation, InitializeRequest,
    NewSessionRequest, PlanEntryStatus, PromptRequest, RequestPermissionOutcome,
    RequestPermissionRequest, RequestPermissionResponse, SelectedPermissionOutcome, SessionId,
    SessionNotification, SessionUpdate, StopReason, TextContent,
};
use agent_client_protocol::{Client, ConnectionTo, Responder, UntypedMessage};
use futures::{AsyncBufReadExt, StreamExt};

mod launch;
pub use launch::{Host, NotOffered};

use crate::harness::asks::{Answerer, Applied, AskId, Asks, Raised, Refused};
use crate::harness::model::{Plan, Said, Session, Step, Turn, Usage};

/// What starts one level-3 chat's agent.
#[derive(Debug, Clone)]
pub struct Launch {
    /// The chat, as its asks are keyed.
    pub chat: String,
    /// The agent's argv: the program, then its arguments.
    pub argv: Vec<String>,
    /// The chat's worktree: where the agent runs, and the session's `cwd`.
    pub cwd: PathBuf,
    /// The agent's whole environment: the chat's, as a terminal chat's program is started
    /// with it ([`crate::chatenv::compose`], ADR 0080 §1). Nothing of this process's own
    /// environment reaches the agent unless it is here.
    pub env: Vec<(std::ffi::OsString, std::ffi::OsString)>,
    /// The `charter` binary, whose MCP server the session is handed; `None` hands it none.
    pub charter_mcp: Option<PathBuf>,
    /// How long the agent has to answer its handshake: [`HANDSHAKE_PATIENCE`], unless a test
    /// needs less.
    pub patience: std::time::Duration,
}

/// What the agent reported, in the order it reported it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// Something in the neutral model: the session began, a turn began or ended, a plan, usage.
    Said(Said),
    /// A piece of the agent's reply, to show in the transcript.
    Text(String),
    /// A tool call the agent reported, run or about to run (W8: the audit reads every one).
    ToolCall {
        id: String,
        title: String,
        /// ACP's kind for it: `read`, `edit`, `execute` and so on.
        kind: String,
        /// `pending`, `in_progress`, `completed` or `failed`.
        status: String,
    },
    /// A tool call [`Event::ToolCall`] reported moved on: `in_progress`, `completed` or `failed`.
    ToolCallStatus { id: String, status: String },
    /// The agent asked permission, and the ask now waits on a human.
    Raised(Raised),
    /// The agent called a client method charter does not offer, and was refused.
    Refused { method: String },
    /// The agent's stdio closed: it exited, or was stopped.
    Ended,
}

impl Event {
    /// What the board hears of this event, in the neutral model (ADR 0073): the session and
    /// its turns, a plan and usage, and an ask raised. The agent's text and tool calls move no
    /// chat, and neither does its end here: the host reads that from the program's exit, as it
    /// does a terminal chat's.
    pub fn said(&self) -> Option<Said> {
        match self {
            Self::Said(said) => Some(said.clone()),
            Self::Raised(raised) => Some(Said::Ask(raised.ask.clone())),
            Self::Text(_)
            | Self::ToolCall { .. }
            | Self::ToolCallStatus { .. }
            | Self::Refused { .. }
            | Self::Ended => None,
        }
    }
}

/// Why a turn ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stop {
    EndTurn,
    MaxTokens,
    MaxTurnRequests,
    Refusal,
    Cancelled,
}

/// Why a turn could not run.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TurnFailed {
    #[error("the agent has exited")]
    Gone,
    #[error("the agent refused the prompt: {0}")]
    Refused(String),
    /// Refused before it was sent: written out, it would not fit in what purlis holds for the
    /// agent to read ([`MOST_UNWRITTEN_BYTES`]). The chat goes on.
    #[error("the prompt is longer than purlis sends to an agent, 8 MiB")]
    TooLong,
}

/// Why a chat did not start at level 3. Each says so in a sentence, and the chat starts at its
/// next level instead (ADR 0080 §3, §4).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NotStarted {
    #[error("the ACP agent {program:?} could not be started: {why}")]
    Spawn { program: String, why: String },
    #[error("the ACP agent speaks protocol version {offered}, and purlis speaks version 1")]
    Version { offered: u16 },
    #[error(
        "the ACP agent needs a login, which purlis never supplies: log in through the harness's own flow in a shell tab"
    )]
    Login,
    #[error("the ACP agent did not complete its handshake: {0}")]
    Handshake(String),
    #[error("a chat cannot run over ACP on this system until purlis can sandbox it here")]
    Unsupported,
    #[error(
        "purlis still has the terminal it was started from, which an agent in its session could open, so no chat runs over ACP until it is restarted"
    )]
    Terminal,
}

/// What the agent answered at `initialize`, as harness capabilities for this run (ADR 0080 §4,
/// *ADR 0073, amended*): the handshake is the fact, and silence is *no*.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Negotiated {
    /// The protocol version both sides speak.
    pub protocol: u16,
    /// The agent's name and version, as it gave them.
    pub agent: Option<String>,
    /// `loadSession`: the conversation can be loaded again by its id.
    pub resumes_by_id: bool,
    /// `promptCapabilities.image`.
    pub takes_images: bool,
    /// `promptCapabilities.audio`.
    pub takes_audio: bool,
    /// `promptCapabilities.embeddedContext`.
    pub takes_embedded_context: bool,
}

/// The longest line charter reads from an agent, in bytes. A line is one JSON-RPC message, and
/// a tool call's diff or output can be large, so the bound is generous; past it the agent is
/// writing something that is not ACP, and the chat ends rather than buffer it.
pub const MOST_LINE_BYTES: usize = 16 * 1024 * 1024;

/// How many events wait for the host to read them, at most. Past this, or past
/// [`MOST_BYTES_HELD`], an event is dropped and counted ([`Chat::events_dropped`]) rather than
/// held: an ask stays in [`Asks`] whether or not its event is read, and a host that stops
/// reading must not stop the agent.
pub const EVENTS_HELD: usize = 4096;

/// How many bytes of text the unread events hold, at most: what bounds the memory a host that
/// stops reading leaves to the agent. Each event is weighed by the text it carries
/// ([`Events`] gives the room back as the host reads).
pub const MOST_BYTES_HELD: usize = 16 * 1024 * 1024;

/// The most text one [`Event::Text`] carries: a longer piece of reply comes as several, so one
/// piece never needs the whole of [`MOST_BYTES_HELD`].
pub const MOST_TEXT_BYTES: usize = 64 * 1024;

/// The most steps a plan keeps; an agent's later steps are dropped.
pub const MOST_PLAN_STEPS: usize = 64;

/// The most text one plan step keeps; a longer step is cut short, ending in `…`.
pub const MOST_STEP_BYTES: usize = 4 * 1024;

/// The most permission requests one chat may have waiting on a human at once. One past it is
/// answered `cancelled` at once and never raised: no deadline bounds an ACP ask (V28d), so this
/// is what bounds how many an agent can hold open.
pub const MOST_OPEN_ASKS: usize = 32;

/// The most bytes charter holds that it has not yet written to the agent. Whatever the agent
/// sends, well-formed or not, can earn an answer, and an agent that stops reading its stdin lets
/// those answers pile up; past this the chat ends rather than hold them.
pub const MOST_UNWRITTEN_BYTES: usize = 8 * 1024 * 1024;

/// What a `session/prompt` line carries besides its params: `jsonrpc`, `id` and `method`, with
/// room to spare.
const PROMPT_ENVELOPE_BYTES: usize = 256;

/// The `session/prompt` request for `text`, or [`TurnFailed::TooLong`] when the line it is
/// written in would be past [`MOST_UNWRITTEN_BYTES`] on its own. Sent, such a line would close
/// the agent's stdin and end the chat; refused here, the chat goes on (#1117).
fn prompt_request(session: &SessionId, text: &str) -> Result<PromptRequest, TurnFailed> {
    let request = PromptRequest::new(
        session.clone(),
        vec![ContentBlock::Text(TextContent::new(text))],
    );
    let mut written = Counted(0);
    serde_json::to_writer(&mut written, &request).map_err(|_| TurnFailed::TooLong)?;
    if written.0 + PROMPT_ENVELOPE_BYTES > MOST_UNWRITTEN_BYTES {
        return Err(TurnFailed::TooLong);
    }
    Ok(request)
}

/// A writer that keeps only how many bytes went into it.
struct Counted(usize);

impl std::io::Write for Counted {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0 += buf.len();
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// The most text one permission request may carry: its command, title, options and the rest, as
/// the agent sent them. A larger one is answered `cancelled` and never raised; it is not cut
/// short, because a command shown for approval without its tail is not the command that runs.
pub const MOST_ASK_BYTES: usize = 64 * 1024;

/// What an event weighs against [`MOST_BYTES_HELD`]: the text it carries, and a little for
/// itself.
fn weight(event: &Event) -> usize {
    const ITSELF: usize = 64;
    ITSELF
        + match event {
            Event::Text(text) => text.len(),
            Event::ToolCall {
                id,
                title,
                kind,
                status,
            } => id.len() + title.len() + kind.len() + status.len(),
            Event::ToolCallStatus { id, status } => id.len() + status.len(),
            Event::Raised(raised) => serde_json::to_string(raised).map_or(0, |json| json.len()),
            Event::Refused { method } => method.len(),
            Event::Said(Said::Plan(plan)) => plan.steps.iter().map(|step| step.text.len()).sum(),
            Event::Said(_) | Event::Ended => 0,
        }
}

/// The events of one chat, as the host reads them: a channel holding at most [`EVENTS_HELD`]
/// events and [`MOST_BYTES_HELD`] bytes, which each read gives back.
#[derive(Debug)]
pub struct Events {
    channel: mpsc::Receiver<Event>,
    held: Arc<AtomicUsize>,
    unread: Arc<AtomicUsize>,
}

impl Events {
    fn read(&self, event: Event) -> Event {
        self.held.fetch_sub(weight(&event), Ordering::AcqRel);
        self.unread.fetch_sub(1, Ordering::AcqRel);
        event
    }

    /// The next event, waiting for it; an error once the chat's protocol has ended.
    pub fn recv(&self) -> Result<Event, mpsc::RecvError> {
        self.channel.recv().map(|event| self.read(event))
    }

    /// The next event, waiting at most `patience` for it.
    pub fn recv_timeout(
        &self,
        patience: std::time::Duration,
    ) -> Result<Event, mpsc::RecvTimeoutError> {
        self.channel
            .recv_timeout(patience)
            .map(|event| self.read(event))
    }

    /// The next event, if one is waiting.
    pub fn try_recv(&self) -> Result<Event, mpsc::TryRecvError> {
        self.channel.try_recv().map(|event| self.read(event))
    }

    /// Every event waiting now.
    pub fn try_iter(&self) -> impl Iterator<Item = Event> + '_ {
        self.channel.try_iter().map(|event| self.read(event))
    }
}

/// How long an agent has to answer `initialize` and `session/new` before the chat is ended. The
/// handshake has a bound; an ask has none (V28d).
pub const HANDSHAKE_PATIENCE: std::time::Duration = std::time::Duration::from_secs(60);

enum Command {
    Prompt(String, mpsc::Sender<Result<Stop, TurnFailed>>),
    Cancel,
}

/// A permission request charter still owes an answer, and what ends the watch on the agent
/// withdrawing it: dropped with the waiter, once it is answered one way or another.
struct Waiter {
    responder: Responder<RequestPermissionResponse>,
    _answered: futures::channel::oneshot::Sender<()>,
}

/// What the chat's handle and its protocol thread share.
struct Shared {
    chat: String,
    asks: Arc<Asks>,
    /// The requests waiting on an answer. Raising, answering, cancelling and withdrawing each
    /// hold it from start to finish, so none of them sees another half done.
    waiting: Mutex<HashMap<AskId, Waiter>>,
    /// The session's id, once `session/new` gives it. Anything naming another is refused.
    session: OnceLock<String>,
    /// Where events go, until [`Shared::end`] takes it: dropped after [`Event::Ended`], so a
    /// host that read the backlog then reads that the channel is closed.
    events: Mutex<Option<mpsc::SyncSender<Event>>>,
    /// How many events are unread, against [`EVENTS_HELD`]; shared with [`Events`].
    unread: Arc<AtomicUsize>,
    /// What the unread events weigh ([`weight`]), shared with [`Events`].
    held: Arc<AtomicUsize>,
    events_dropped: AtomicU64,
    notifications_ignored: AtomicU64,
    /// Permission requests answered `cancelled` because [`MOST_OPEN_ASKS`] were open.
    asks_refused: AtomicU64,
    /// The agent's process group, which the protocol thread ends when the connection does.
    group: u32,
}

impl Shared {
    fn emit(&self, event: Event) {
        let events = lock(&self.events);
        let Some(sender) = events.as_ref() else {
            return;
        };
        let weighs = weight(&event);
        // Room taken before the send, so a read racing it never gives back more than was taken.
        let before = self.held.fetch_add(weighs, Ordering::AcqRel);
        let waiting = self.unread.fetch_add(1, Ordering::AcqRel);
        let sent = before + weighs <= MOST_BYTES_HELD
            && waiting < EVENTS_HELD
            && match sender.try_send(event) {
                Ok(()) => true,
                Err(mpsc::TrySendError::Full(_)) => false,
                // Nobody reads any more: nothing to count.
                Err(mpsc::TrySendError::Disconnected(_)) => {
                    self.held.fetch_sub(weighs, Ordering::AcqRel);
                    self.unread.fetch_sub(1, Ordering::AcqRel);
                    return;
                }
            };
        if !sent {
            self.held.fetch_sub(weighs, Ordering::AcqRel);
            self.unread.fetch_sub(1, Ordering::AcqRel);
            self.events_dropped.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Sends [`Event::Ended`] into the slot kept for it past [`EVENTS_HELD`], whatever is
    /// unread, and lets the channel go, so the host hears the end and then that nothing more
    /// comes. Nothing is emitted after it.
    fn end(&self) {
        let Some(sender) = lock(&self.events).take() else {
            return;
        };
        self.held.fetch_add(weight(&Event::Ended), Ordering::AcqRel);
        self.unread.fetch_add(1, Ordering::AcqRel);
        let _ = sender.try_send(Event::Ended);
    }

    fn ours(&self, session: &SessionId) -> bool {
        self.session.get().is_some_and(|ours| **ours == *session.0)
    }

    fn ignore(&self) {
        self.notifications_ignored.fetch_add(1, Ordering::Relaxed);
    }
}

/// One level-3 chat: its agent process and its ACP session. Dropping it ends the agent's
/// process group.
pub struct Chat {
    session: String,
    negotiated: Negotiated,
    commands: futures::channel::mpsc::UnboundedSender<Command>,
    shared: Arc<Shared>,
    child: std::process::Child,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl std::fmt::Debug for Chat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Chat")
            .field("chat", &self.shared.chat)
            .field("session", &self.session)
            .field("negotiated", &self.negotiated)
            .finish_non_exhaustive()
    }
}

/// What the protocol's thread hands back once the session exists.
type Ready = Result<(Negotiated, String), NotStarted>;

impl Chat {
    /// Starts `launch`'s agent, initializes it and opens its session, waiting at most
    /// `launch.patience` for both. [`Events`] hears every [`Event`] until [`Event::Ended`],
    /// as many as [`EVENTS_HELD`] and [`MOST_BYTES_HELD`] allow unread. Asks are raised in
    /// `asks`, which every client that answers shares.
    pub fn start(launch: Launch, asks: Arc<Asks>) -> Result<(Self, Events), NotStarted> {
        if !cfg!(unix) {
            // ADR 0067, amended: no generated profile wraps it here, so it fails closed.
            return Err(NotStarted::Unsupported);
        }
        // V77: the agent shares the host's session, so the host must have no terminal for it
        // to open. The host leaves it as it starts (`crate::noterminal::leave`); this is the
        // check that it did.
        if crate::noterminal::has_one() {
            return Err(NotStarted::Terminal);
        }
        let mut child = spawn(&launch)?;
        let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
            kill_group(child.id());
            reap(&mut child);
            return Err(NotStarted::Handshake("its stdio was not piped".to_owned()));
        };
        let transport = match lines(stdin, stdout) {
            Ok(transport) => transport,
            Err(err) => {
                kill_group(child.id());
                reap(&mut child);
                return Err(NotStarted::Spawn {
                    program: launch.argv[0].clone(),
                    why: err.to_string(),
                });
            }
        };
        // One slot more than ordinary events may take: the end's.
        let (events, heard) = mpsc::sync_channel(EVENTS_HELD + 1);
        let held = Arc::new(AtomicUsize::new(0));
        let unread = Arc::new(AtomicUsize::new(0));
        let heard = Events {
            channel: heard,
            held: Arc::clone(&held),
            unread: Arc::clone(&unread),
        };
        // Fired when the handshake runs out of time, so the connection ends without waiting
        // for the agent's stdout to close, which a process that left its group can hold open.
        let (abandon, abandoned) = futures::channel::oneshot::channel::<()>();
        let (ready, readied) = mpsc::channel();
        let (commands, received) = futures::channel::mpsc::unbounded();
        let shared = Arc::new(Shared {
            chat: launch.chat.clone(),
            asks,
            waiting: Mutex::default(),
            session: OnceLock::new(),
            events: Mutex::new(Some(events)),
            unread,
            held,
            events_dropped: AtomicU64::new(0),
            notifications_ignored: AtomicU64::new(0),
            asks_refused: AtomicU64::new(0),
            group: child.id(),
        });
        let protocol = Protocol {
            session_new: session_new(&launch),
            shared: Arc::clone(&shared),
        };
        let thread = std::thread::Builder::new()
            .name(format!("acp {}", launch.chat))
            .spawn(move || {
                futures::executor::block_on(protocol.run(transport, ready, received, abandoned));
            });
        let thread = match thread {
            Ok(thread) => thread,
            Err(err) => {
                kill_group(child.id());
                reap(&mut child);
                return Err(NotStarted::Spawn {
                    program: launch.argv[0].clone(),
                    why: err.to_string(),
                });
            }
        };
        let refused = |child: &mut std::process::Child, thread: std::thread::JoinHandle<()>| {
            kill_group(child.id());
            let _ = thread.join();
            reap(child);
        };
        let (negotiated, session) = match readied.recv_timeout(launch.patience) {
            Ok(Ok(ready)) => ready,
            Ok(Err(err)) => {
                // Each way out closes the commands first, so a protocol that finished its
                // handshake late finds no chat to serve and ends.
                commands.close_channel();
                refused(&mut child, thread);
                return Err(err);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                let _ = abandon.send(());
                commands.close_channel();
                refused(&mut child, thread);
                return Err(NotStarted::Handshake(format!(
                    "the agent did not answer within {} s",
                    launch.patience.as_secs_f32()
                )));
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                commands.close_channel();
                refused(&mut child, thread);
                return Err(NotStarted::Handshake("the agent exited".to_owned()));
            }
        };
        Ok((
            Self {
                session,
                negotiated,
                commands,
                shared,
                child,
                thread: Some(thread),
            },
            heard,
        ))
    }

    /// The session's id, as the agent gave it.
    pub fn session(&self) -> &str {
        &self.session
    }

    /// What the agent answered at `initialize`.
    pub fn negotiated(&self) -> &Negotiated {
        &self.negotiated
    }

    /// How many events were dropped because the host had [`EVENTS_HELD`] events, or
    /// [`MOST_BYTES_HELD`] bytes of them, unread.
    pub fn events_dropped(&self) -> u64 {
        self.shared.events_dropped.load(Ordering::Relaxed)
    }

    /// How many notifications the agent sent that charter does not follow, or that named
    /// another session, and were dropped.
    pub fn notifications_ignored(&self) -> u64 {
        self.shared.notifications_ignored.load(Ordering::Relaxed)
    }

    /// How many permission requests were answered `cancelled` without being raised, because
    /// [`MOST_OPEN_ASKS`] of this chat's were already open or one carried more than
    /// [`MOST_ASK_BYTES`].
    pub fn asks_refused(&self) -> u64 {
        self.shared.asks_refused.load(Ordering::Relaxed)
    }

    /// Sends `text` as a prompt and waits for its turn to end.
    pub fn prompt(&self, text: &str) -> Result<Stop, TurnFailed> {
        let (reply, replied) = mpsc::channel();
        self.commands
            .unbounded_send(Command::Prompt(text.to_owned(), reply))
            .map_err(|_| TurnFailed::Gone)?;
        replied.recv().unwrap_or(Err(TurnFailed::Gone))
    }

    /// Answers ask `id` with `option`, from `by`: the first answer wins (HP-5), and the chosen
    /// option goes back to the agent on its stdio. Only this chat's asks: the registry is
    /// shared, and another chat's ask is [`Refused::Unknown`] here.
    pub fn answer(&self, id: &AskId, option: &str, by: Answerer) -> Result<Applied, Refused> {
        let shared = &self.shared;
        // Held across the answer and the reply, so a cancel cannot slip between them and send
        // the agent `cancelled` for an ask the operator was told they answered.
        let mut waiting = lock(&shared.waiting);
        let applied = shared
            .asks
            .answer(&shared.chat, id, option, by, Instant::now())?;
        if let Some(waiter) = waiting.remove(id) {
            let _ = waiter.responder.respond(RequestPermissionResponse::new(
                RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(
                    applied.choice.id.clone(),
                )),
            ));
        }
        Ok(applied)
    }

    /// Cancels the turn: `session/cancel`, and every waiting request answered `cancelled`, its
    /// ask withdrawn (ADR 0080 §5.5).
    pub fn cancel(&self) {
        let _ = self.commands.unbounded_send(Command::Cancel);
    }

    /// Ends the agent's whole process group now, sending nothing and waiting for nothing: the
    /// kill switch (ADR 0071, ADR 0080 §5.5). It reaches a chat a waiting turn still holds,
    /// which is never dropped while it waits. The connection then ends as it does when the
    /// agent exits: the turn fails as [`TurnFailed::Gone`], every ask still open is withdrawn,
    /// and [`Event::Ended`] is heard. The leader is reaped only when the chat is dropped, so
    /// its id names this group until then.
    ///
    /// The connection is ended by charter's own act too, not only by the agent's stdout
    /// closing: a process that left the group can hold that open, and must not keep a killed
    /// chat, or its asks, alive.
    pub fn kill(&self) {
        self.commands.close_channel();
        kill_group(self.shared.group);
    }

    /// The agent's process id, which is also its process group's: what a pause stops
    /// (V27c) and what the run records as the chat's program.
    ///
    /// Valid only while this `Chat` is alive: the leader is reaped when it is dropped, and the
    /// id can then name another process. A pause holds the chat for as long as it uses the id.
    pub fn process_id(&self) -> u32 {
        self.shared.group
    }
}

impl Drop for Chat {
    fn drop(&mut self) {
        {
            let mut waiting = lock(&self.shared.waiting);
            for (id, _) in waiting.drain() {
                self.shared.asks.withdraw(&id);
            }
        }
        self.commands.close_channel();
        // The group first and the reaping last: until the leader is reaped its id cannot be
        // given to another process, so the protocol thread's own end of the group is safe too.
        kill_group(self.child.id());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        reap(&mut self.child);
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Spawns the agent in its own process group, with two pipes and no terminal. The host has no
/// controlling terminal of its own to hand it ([`crate::noterminal`], V77).
fn spawn(launch: &Launch) -> Result<std::process::Child, NotStarted> {
    let Some((program, args)) = launch.argv.split_first() else {
        return Err(NotStarted::Spawn {
            program: String::new(),
            why: "no program was declared".to_owned(),
        });
    };
    let mut command = std::process::Command::new(program);
    command
        .args(args)
        .current_dir(&launch.cwd)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    command
        .env_clear()
        .envs(launch.env.iter().map(|(name, value)| (name, value)));
    // Never the host's relaunch marker, even where a caller set it: an app started from the
    // agent leaves its own terminal (V77).
    for name in crate::envvar::spellings(crate::noterminal::RELAUNCHED_ENV) {
        command.env_remove(name);
    }
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut command, 0);
    crate::forklock::spawn(&mut command).map_err(|err| NotStarted::Spawn {
        program: program.clone(),
        why: err.to_string(),
    })
}

/// Ends the agent's whole process group, led by `leader`. Safe to call until the leader is
/// reaped, and only then.
fn kill_group(leader: u32) {
    #[cfg(unix)]
    if let Some(group) = i32::try_from(leader)
        .ok()
        .and_then(rustix::process::Pid::from_raw)
    {
        let _ = rustix::process::kill_process_group(group, rustix::process::Signal::KILL);
    }
    #[cfg(not(unix))]
    let _ = leader;
}

fn reap(child: &mut std::process::Child) {
    let _ = child.kill();
    let _ = child.wait();
}

/// The agent's stdio as JSON-RPC lines, each read line at most [`MOST_LINE_BYTES`] long, and the
/// lines written to it held in [`Unwritten`] until a writer thread of their own writes them.
fn lines(
    stdin: std::process::ChildStdin,
    stdout: std::process::ChildStdout,
) -> std::io::Result<
    agent_client_protocol::Lines<
        impl futures::Sink<String, Error = std::io::Error> + Send + 'static,
        impl futures::Stream<Item = std::io::Result<String>> + Send + 'static,
    >,
> {
    let reader = futures::io::BufReader::new(blocking::Unblock::new(stdout));
    let incoming = futures::stream::unfold(Some(reader), async |reader| {
        let mut reader = reader?;
        let line = capped_line(&mut reader).await;
        // One line, then a turn for the rest of the connection: lines the agent has already
        // written are always ready, and read back to back they would queue inside the SDK
        // faster than it answers them, out of reach of [`MOST_UNWRITTEN_BYTES`].
        yield_now().await;
        match line {
            Ok(Some(line)) => Some((Ok(line), Some(reader))),
            Ok(None) => None,
            // One error ends the stream, and with it the connection.
            Err(err) => Some((Err(err), None)),
        }
    });
    let unwritten = Arc::new(Unwritten::default());
    let writing = Arc::clone(&unwritten);
    std::thread::Builder::new()
        .name("acp writer".to_owned())
        .spawn(move || writing.write_to(stdin))?;
    let outgoing = futures::sink::unfold(Closing(unwritten), async |held, line: String| {
        held.0.hold(line)?;
        Ok::<_, std::io::Error>(held)
    });
    Ok(agent_client_protocol::Lines::new(
        outgoing,
        Box::pin(incoming),
    ))
}

/// Lets every other future the executor runs take a turn before this one goes on.
async fn yield_now() {
    let mut yielded = false;
    futures::future::poll_fn(|cx| {
        if yielded {
            std::task::Poll::Ready(())
        } else {
            yielded = true;
            cx.waker().wake_by_ref();
            std::task::Poll::Pending
        }
    })
    .await;
}

/// The lines charter has handed the agent and not yet written to its stdin, weighed in bytes.
/// Past [`MOST_UNWRITTEN_BYTES`] it refuses the next, which fails the connection's sink and ends
/// the chat. The bound holds whatever the agent sent to earn the answers, so nothing here has to
/// tell a request from an answer the way the SDK does.
#[derive(Default)]
struct Unwritten {
    queue: Mutex<Queue>,
    ready: std::sync::Condvar,
}

#[derive(Default)]
struct Queue {
    lines: std::collections::VecDeque<Vec<u8>>,
    /// What the held lines weigh, the one being written included.
    bytes: usize,
    /// The connection is over: the writer stops, and nothing more is held.
    closed: bool,
}

impl Unwritten {
    /// Holds `line` for the writer, or refuses it past the budget and closes.
    fn hold(&self, line: String) -> std::io::Result<()> {
        let mut bytes = line.into_bytes();
        bytes.push(b'\n');
        let mut queue = lock(&self.queue);
        if queue.closed {
            return Err(std::io::Error::other("the agent's stdin is closed"));
        }
        if queue.bytes + bytes.len() > MOST_UNWRITTEN_BYTES {
            queue.closed = true;
            queue.lines.clear();
            self.ready.notify_all();
            return Err(std::io::Error::other(format!(
                "the agent left more than {MOST_UNWRITTEN_BYTES} bytes purlis wrote to it unread"
            )));
        }
        queue.bytes += bytes.len();
        queue.lines.push_back(bytes);
        self.ready.notify_all();
        Ok(())
    }

    fn close(&self) {
        let mut queue = lock(&self.queue);
        queue.closed = true;
        queue.lines.clear();
        self.ready.notify_all();
    }

    /// Writes each held line to `stdin`, oldest first, until the connection closes or a write
    /// fails. A line weighs until it is written, so an agent that stops reading keeps the budget
    /// full.
    fn write_to(&self, mut stdin: impl std::io::Write) {
        loop {
            let line = {
                let mut queue = lock(&self.queue);
                loop {
                    if queue.closed {
                        return;
                    }
                    if let Some(line) = queue.lines.pop_front() {
                        break line;
                    }
                    queue = self
                        .ready
                        .wait(queue)
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                }
            };
            let written = stdin.write_all(&line).and_then(|()| stdin.flush());
            let mut queue = lock(&self.queue);
            queue.bytes = queue.bytes.saturating_sub(line.len());
            if written.is_err() {
                queue.closed = true;
                queue.lines.clear();
                return;
            }
        }
    }
}

/// The sink's hold on [`Unwritten`]: it closes it when the connection lets the sink go. The
/// writer thread then ends once it is not in a write, which is once the agent's stdin closes:
/// a process that left the agent's group and holds that stdin keeps the thread in its last
/// write for as long as it lives (#1117).
struct Closing(Arc<Unwritten>);

impl Drop for Closing {
    fn drop(&mut self) {
        self.0.close();
    }
}

/// The next line from `reader`, without its line ending, or `None` at the end of input. A line
/// past [`MOST_LINE_BYTES`] is an error before it is held whole.
async fn capped_line(
    reader: &mut (impl futures::AsyncBufRead + Unpin),
) -> std::io::Result<Option<String>> {
    let mut line = Vec::new();
    loop {
        let available = reader.fill_buf().await?;
        if available.is_empty() {
            return if line.is_empty() {
                Ok(None)
            } else {
                text(line).map(Some)
            };
        }
        let (taken, content, ended) = match memchr::memchr(b'\n', available) {
            Some(at) => (at + 1, at, true),
            None => (available.len(), available.len(), false),
        };
        if line.len() + content > MOST_LINE_BYTES {
            return Err(std::io::Error::other(format!(
                "the agent wrote a line longer than {MOST_LINE_BYTES} bytes"
            )));
        }
        line.extend_from_slice(&available[..content]);
        reader.consume_unpin(taken);
        if ended {
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            return text(line).map(Some);
        }
    }
}

fn text(line: Vec<u8>) -> std::io::Result<String> {
    String::from_utf8(line).map_err(std::io::Error::other)
}

/// The session the agent is asked to open: the chat's worktree, and charter's MCP server.
fn session_new(launch: &Launch) -> NewSessionRequest {
    let servers = launch
        .charter_mcp
        .iter()
        .map(|binary| crate::chattools::acp_server(binary, &launch.env))
        .collect();
    NewSessionRequest::new(launch.cwd.clone()).mcp_servers(servers)
}

/// The protocol's side of a chat, run on its own thread.
struct Protocol {
    session_new: NewSessionRequest,
    shared: Arc<Shared>,
}

impl Protocol {
    async fn run(
        self,
        transport: impl agent_client_protocol::ConnectTo<Client> + 'static,
        ready: mpsc::Sender<Ready>,
        commands: futures::channel::mpsc::UnboundedReceiver<Command>,
        abandoned: futures::channel::oneshot::Receiver<()>,
    ) {
        let Self {
            session_new,
            shared,
        } = self;
        let (reporting, asking, refusing, ignoring, serving) = (
            Arc::clone(&shared),
            Arc::clone(&shared),
            Arc::clone(&shared),
            Arc::clone(&shared),
            Arc::clone(&shared),
        );
        let failed = ready.clone();
        let outcome = Client
            .builder()
            .name("charter")
            .on_receive_notification(
                async move |notification: SessionNotification, _cx| {
                    if !reporting.ours(&notification.session_id) {
                        reporting.ignore();
                        return Ok(());
                    }
                    for event in reported_as(notification.update) {
                        reporting.emit(event);
                    }
                    Ok(())
                },
                agent_client_protocol::on_receive_notification!(),
            )
            .on_receive_notification(
                // Every other notification is dropped here. Unhandled, the SDK keeps one that
                // names a session for a handler that may come later, and charter adds none.
                async move |_notification: UntypedMessage, _cx| {
                    ignoring.ignore();
                    Ok(())
                },
                agent_client_protocol::on_receive_notification!(),
            )
            .on_receive_request(
                async move |request: RequestPermissionRequest,
                            responder: Responder<RequestPermissionResponse>,
                            cx: ConnectionTo<agent_client_protocol::Agent>| {
                    if !asking.ours(&request.session_id) {
                        return responder
                            .respond_with_error(agent_client_protocol::Error::invalid_params());
                    }
                    raise(&asking, &request, responder, &cx)
                },
                agent_client_protocol::on_receive_request!(),
            )
            .on_receive_request(
                async move |request: UntypedMessage,
                            responder: Responder<serde_json::Value>,
                            _cx| {
                    refusing.emit(Event::Refused {
                        method: request.method().to_owned(),
                    });
                    responder.respond_with_error(agent_client_protocol::Error::method_not_found())
                },
                agent_client_protocol::on_receive_request!(),
            )
            .connect_with(
                transport,
                async move |cx: ConnectionTo<agent_client_protocol::Agent>| {
                    let handshaken =
                        futures::future::select(Box::pin(handshake(&cx, session_new)), abandoned)
                            .await;
                    let futures::future::Either::Left((handshaken, _)) = handshaken else {
                        // Out of time, or `start` gave up: end without the agent's say.
                        return Ok(());
                    };
                    let session = match handshaken {
                        Ok((negotiated, session)) => {
                            let _ = serving.session.set(session.0.to_string());
                            let _ = ready.send(Ok((negotiated, session.to_string())));
                            serving.emit(Event::Said(Said::Session(Session::Began(
                                crate::harness::model::Began::Fresh,
                            ))));
                            session
                        }
                        Err(err) => {
                            let _ = ready.send(Err(err));
                            return Ok(());
                        }
                    };
                    serve(&cx, &session, commands, &serving).await
                },
            )
            .await;
        if let Err(err) = outcome {
            let _ = failed.send(Err(NotStarted::Handshake(err.to_string())));
        }
        // The connection is over, whatever ended it: a line past the cap, the agent's exit, the
        // chat dropped. Its group goes with it; the leader is not reaped yet, so its id is
        // still the agent's.
        kill_group(shared.group);
        // Nothing can answer the agent any more, so nothing it asked waits on a human. Under
        // the lock from the first ask to the last, as a cancel and a drop withdraw them, so an
        // answer never finds an ask still raised for an agent that is gone.
        {
            let mut waiting = lock(&shared.waiting);
            for (id, _) in waiting.drain() {
                shared.asks.withdraw(&id);
            }
        }
        shared.end();
    }
}

/// Raises a permission request as an ask, the responder stored first, under the same lock, so
/// an answer never finds the ask raised and its request not yet held. The agent may withdraw
/// the request (`$/cancel_request`), and then the ask is withdrawn too.
fn raise(
    shared: &Arc<Shared>,
    request: &RequestPermissionRequest,
    responder: Responder<RequestPermissionResponse>,
    cx: &ConnectionTo<agent_client_protocol::Agent>,
) -> Result<(), agent_client_protocol::Error> {
    let params = serde_json::to_value(request).unwrap_or_default();
    // Weighed as the agent sent it, before anything is kept of it.
    if params.to_string().len() > MOST_ASK_BYTES {
        shared.asks_refused.fetch_add(1, Ordering::Relaxed);
        return responder.respond(cancelled());
    }
    let ask = crate::harness::asked::acp_request_permission(&params);
    let withdrawn = responder.cancellation();
    let (answered, settled) = futures::channel::oneshot::channel();
    let mut waiting = lock(&shared.waiting);
    if waiting.len() >= MOST_OPEN_ASKS {
        drop(waiting);
        shared.asks_refused.fetch_add(1, Ordering::Relaxed);
        return responder.respond(cancelled());
    }
    let raising = shared.asks.raise(&shared.chat, ask, Instant::now());
    for old in &raising.superseded {
        if let Some(old) = waiting.remove(old) {
            let _ = old.responder.respond(cancelled());
        }
    }
    let id = raising.raised.id.clone();
    waiting.insert(
        id.clone(),
        Waiter {
            responder,
            _answered: answered,
        },
    );
    drop(waiting);
    shared.emit(Event::Raised(raising.raised));
    let shared = Arc::clone(shared);
    cx.spawn(async move {
        // Ends when the agent withdraws the request, or when charter answers it and drops its
        // waiter, whichever is first.
        let _ = futures::future::select(Box::pin(withdrawn.cancelled()), settled).await;
        if withdrawn.is_cancelled() {
            let taken = lock(&shared.waiting).remove(&id);
            if let Some(waiter) = taken {
                shared.asks.withdraw(&id);
                let _ = waiter
                    .responder
                    .respond_with_error(agent_client_protocol::Error::request_cancelled());
            }
        }
        Ok(())
    })
}

/// `initialize` and `session/new`.
async fn handshake(
    cx: &ConnectionTo<agent_client_protocol::Agent>,
    session_new: NewSessionRequest,
) -> Result<(Negotiated, SessionId), NotStarted> {
    let init = cx
        .send_request(
            InitializeRequest::new(ProtocolVersion::V1)
                .client_capabilities(ClientCapabilities::default())
                .client_info(Implementation::new("charter", env!("CARGO_PKG_VERSION"))),
        )
        .block_task()
        .await
        .map_err(|err| NotStarted::Handshake(err.to_string()))?;
    if init.protocol_version != ProtocolVersion::V1 {
        return Err(NotStarted::Version {
            offered: init.protocol_version.as_u16(),
        });
    }
    let capabilities = &init.agent_capabilities;
    let negotiated = Negotiated {
        protocol: init.protocol_version.as_u16(),
        agent: init
            .agent_info
            .as_ref()
            .map(|agent| format!("{} {}", agent.name, agent.version)),
        resumes_by_id: capabilities.load_session,
        takes_images: capabilities.prompt_capabilities.image,
        takes_audio: capabilities.prompt_capabilities.audio,
        takes_embedded_context: capabilities.prompt_capabilities.embedded_context,
    };
    let session = cx
        .send_request(session_new)
        .block_task()
        .await
        .map_err(|err| {
            if err.code == agent_client_protocol::ErrorCode::AuthRequired {
                NotStarted::Login
            } else {
                NotStarted::Handshake(err.to_string())
            }
        })?;
    Ok((negotiated, session.session_id))
}

/// Serves the chat's commands until the chat is dropped or the agent goes.
async fn serve(
    cx: &ConnectionTo<agent_client_protocol::Agent>,
    session: &SessionId,
    mut commands: futures::channel::mpsc::UnboundedReceiver<Command>,
    shared: &Arc<Shared>,
) -> Result<(), agent_client_protocol::Error> {
    loop {
        let next = futures::future::select(commands.next(), Box::pin(cx.incoming_closed())).await;
        let command = match next {
            futures::future::Either::Left((Some(command), _)) => command,
            _ => return Ok(()),
        };
        match command {
            Command::Prompt(text, reply) => {
                let request = match prompt_request(session, &text) {
                    Ok(request) => request,
                    Err(refused) => {
                        let _ = reply.send(Err(refused));
                        continue;
                    }
                };
                shared.emit(Event::Said(Said::Turn(Turn::Began)));
                let turn_ended = Arc::clone(shared);
                cx.send_request(request)
                    .on_receiving_result(async move |result| {
                        turn_ended.emit(Event::Said(Said::Turn(Turn::Ended)));
                        let _ =
                            reply.send(result.map(|response| stop(response.stop_reason)).map_err(
                                |err| {
                                    if agent_client_protocol::is_incoming_transport_closed(&err) {
                                        TurnFailed::Gone
                                    } else {
                                        TurnFailed::Refused(err.to_string())
                                    }
                                },
                            ));
                        Ok(())
                    })?;
            }
            Command::Cancel => {
                // The lock first and through to the last `cancelled`: an answer racing this
                // either lands before `session/cancel` is sent or finds its ask withdrawn, and
                // never reaches the agent after it, as ACP has a client do it.
                let mut waiting = lock(&shared.waiting);
                cx.send_notification(CancelNotification::new(session.clone()))?;
                for (id, waiter) in waiting.drain() {
                    shared.asks.withdraw(&id);
                    waiter.responder.respond(cancelled())?;
                }
            }
        }
    }
}

fn cancelled() -> RequestPermissionResponse {
    RequestPermissionResponse::new(RequestPermissionOutcome::Cancelled)
}

fn stop(reason: StopReason) -> Stop {
    match reason {
        StopReason::EndTurn => Stop::EndTurn,
        StopReason::MaxTokens => Stop::MaxTokens,
        StopReason::MaxTurnRequests => Stop::MaxTurnRequests,
        StopReason::Refusal => Stop::Refusal,
        _ => Stop::Cancelled,
    }
}

/// What one `session/update` reports, as events. An update charter has no place for yet (the
/// agent's thoughts, its commands and modes) reports nothing.
fn reported_as(update: SessionUpdate) -> Vec<Event> {
    match update {
        SessionUpdate::AgentMessageChunk(chunk) => match chunk.content {
            ContentBlock::Text(text) => pieces(&text.text).map(Event::Text).collect(),
            _ => Vec::new(),
        },
        SessionUpdate::ToolCall(call) => vec![Event::ToolCall {
            id: call.tool_call_id.to_string(),
            title: call.title,
            kind: word(&call.kind),
            status: word(&call.status),
        }],
        SessionUpdate::ToolCallUpdate(update) => update
            .fields
            .status
            .map(|status| Event::ToolCallStatus {
                id: update.tool_call_id.to_string(),
                status: word(&status),
            })
            .into_iter()
            .collect(),
        SessionUpdate::Plan(plan) => vec![Event::Said(Said::Plan(Plan {
            steps: plan
                .entries
                .into_iter()
                .take(MOST_PLAN_STEPS)
                .map(|entry| Step {
                    text: cut(entry.content, MOST_STEP_BYTES),
                    done: entry.status == PlanEntryStatus::Completed,
                })
                .collect(),
        }))],
        SessionUpdate::UsageUpdate(usage) => vec![Event::Said(Said::Usage(Usage {
            input_tokens: None,
            output_tokens: None,
            context_percent: usage
                .used
                .saturating_mul(100)
                .checked_div(usage.size)
                .and_then(|percent| u8::try_from(percent.min(100)).ok()),
        }))],
        _ => Vec::new(),
    }
}

/// `text`, or as much of it as fits in `most` bytes with `…` after it, cut on a character
/// boundary.
fn cut(text: String, most: usize) -> String {
    if text.len() <= most {
        return text;
    }
    let mut at = most - '…'.len_utf8();
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    let mut cut = text[..at].to_owned();
    cut.push('…');
    cut
}

/// `text` in pieces of at most [`MOST_TEXT_BYTES`], each ending on a character boundary.
fn pieces(text: &str) -> impl Iterator<Item = String> + '_ {
    let mut rest = text;
    std::iter::from_fn(move || {
        if rest.is_empty() {
            return None;
        }
        let mut at = rest.len().min(MOST_TEXT_BYTES);
        while !rest.is_char_boundary(at) {
            at -= 1;
        }
        let (piece, after) = rest.split_at(at);
        rest = after;
        Some(piece.to_owned())
    })
}

/// A protocol enum's wire word: `execute`, `in_progress`.
fn word(value: &impl serde::Serialize) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests;
