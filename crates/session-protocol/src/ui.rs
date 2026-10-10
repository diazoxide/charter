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
///
/// **So is every command that ends, starts or restarts a chat on the person's word** (#1488,
/// a delegated ruling, and its list as the train's reviews completed it). The rule is what a
/// command does, and it fails closed: a command that does one of these is listed here, whether
/// or not anything has yet been found to misuse it.
///
/// - **it ends a chat, a task, or every chat of a project or of the app**: `end_task` (which
///   writes a sentence in the person's name to the chat that asked), `task_ending` (the
///   question it asks first), `stop_chat`, `close_session`, `close_chat_stopping`,
///   `end_task_that_did_not_start`, `stop_all_tasks` with `all_tasks_ending` (its question,
///   #1498), `smart_close`, `stop_every_agent`,
///   `forget_chat_that_did_not_start`, `close_plane`, `forget_project`, and the two that quit
///   the app to start it again (`restart_on_the_session_bus`, `restart_to_update`);
/// - **it starts a chat or a shell as the person**: `ask_persona_chat`, `start_chat`,
///   `start_chat_here`, `open_session`, `open_shell_in_branch`, `first_task_run`,
///   `resume_session`, `retry_chat_that_did_not_start`, `reopen_finished_task`, `curate` (a
///   chat with a curation's prompt typed), and `relaunch` (the launch's answer, which starts
///   every chat the project held);
/// - **it starts a chat again**: `restart_chat_without_sandbox`, `restart_chat`,
///   `start_chat_fresh`, `ask_chat_restart`.
///
/// - **it lets a persona's committed hosts reach past this machine's sandbox** (#1362):
///   `allow_persona_hosts`, which the person at this machine's window allows as it was shown
///   there.
///
/// None is served on a link. A command added later that does one of these belongs here.
///
/// **What this rule does not cover.** The other commands that act as the person (an answer
/// to a question other than a chat's ask, typing into a chat, saying what is on screen) end,
/// start and restart nothing. They are served to a `local-ui` peer of the same build, and
/// whether they join this list awaits a ruling.
///
/// **And every command that reads back what one chat said to another, or was sent**: what a
/// task was sent (`task_brief`, #1494), what a session and its tasks said to each other
/// (`activity`, #1495), and the question a task is paused on (`task_question`, #1496). Each
/// reads words a chat wrote, from a store kept from the project's chats, about a session the
/// caller names; no link can say which chat is asking, so none is served on one.
///
/// `answer_task_question` answers, as the person, a question a task put to its asking chat
/// (#1496). What it sends reaches a chat marked as the person's own words, so it is held to
/// what `answer_ask` is: the window's own IPC, and no link until the session protocol has a
/// scoped message for it.
///
/// **Every command that makes, widens, narrows, accepts, declines or takes back a standing
/// dispatch grant is the window's alone too** (spec #1483, train 64; [`STANDING_DISPATCH`]),
/// and so is every answer to a dispatch's question: Keep blocked keeps nothing past the chat,
/// and it is still the person's answer, which nothing on a link gives for them. **And so are
/// the reads of what stands and what waits**: they list the person's refusals, every chat's
/// grants, and what other chats asked for with what their targets work with, and a link
/// reads none of it.
/// A dispatch grant is the person's consent to what one persona's chats may ask of another,
/// given by a press in their own window; the link carries no second way to give or withdraw
/// it, whatever scope the link holds. With them, the commands of the two lists a person
/// answers from: the Notice that says a teammate's grant arrived (#1506), and the dispatches
/// refused while nobody was there (#1507), which are offered on a refused chat's word.
///
/// **And "Got it" on the Notices that the project's Internet access presets** (#1385) **or its
/// own hosts** (#1341, #1550) **changed**: each records that this person was told what chats may
/// now reach or write, so the Notice is not shown again. Only the person who read it says so; no
/// link answers it for them.
///
/// `task_branch_merge` and `dispatch_worktree_discard` are the person's acts on what a task
/// left on a branch of its own (#1511, V100-67): a merge into the branch it was cut from, and
/// a folder removed for good. Each is asked first, in the window, and no chat may cause
/// either, so neither has a form on a link.
///
/// `worktree_remove` and `worktree_merge` are the explorer's remove and merge of any branch
/// folder (#1534): the person's act on their own repo, from their window, which takes a folder
/// for good (with `force`, its uncommitted work) or moves the clone. No link carries either.
///
/// `allow_sandbox_block_for_tasks` and `keep_sandbox_block_for_tasks` answer several tasks'
/// sandbox block on one press (#1508), and `allow_sandbox_block` answers one chat's (#1538):
/// a person's answer given in their own window to the block it showed, which no link carries
/// a second way to give.
///
/// `open_extension_record` starts the person's editor on this machine's extension record
/// (#1296): a program of theirs run on a machine file, from the Extensions dialog, which no link
/// asks for.
pub const WINDOW_ONLY: &[&str] = &[
    "answer_ask",
    // Ends.
    "end_task",
    "task_ending",
    "stop_all_tasks",
    "all_tasks_ending",
    "stop_chat",
    "close_session",
    "close_chat_stopping",
    "end_task_that_did_not_start",
    "smart_close",
    "stop_every_agent",
    "forget_chat_that_did_not_start",
    "close_plane",
    "forget_project",
    "restart_on_the_session_bus",
    "restart_to_update",
    // Starts.
    "ask_persona_chat",
    "start_chat",
    "start_chat_here",
    "open_session",
    "open_shell_in_branch",
    "first_task_run",
    "resume_session",
    "retry_chat_that_did_not_start",
    "reopen_finished_task",
    "curate",
    "relaunch",
    // Starts again.
    "restart_chat_without_sandbox",
    "restart_chat",
    "start_chat_fresh",
    "ask_chat_restart",
    // Reads what a chat was sent or said to another.
    "task_brief",
    "activity",
    "task_question",
    // Answers as the person.
    "answer_task_question",
    // What stands and what waits: the person's and every chat's, so never a link's to read.
    "dispatch_grants_needed",
    "asks_waiting",
    "dispatch_standing",
    "dispatch_grants",
    // The person's answers on a dispatch's Notice: every one of them, the no included.
    "allow_dispatch",
    "allow_dispatch_anywhere",
    "keep_dispatch_blocked",
    "never_dispatch",
    // Keep blocked on a sandbox host ask (#1690): the person's no, told to the app.
    "forget_sandbox_block",
    // The Inbox's updates (#1693): what happened to the person's chats, and what they put away.
    "inbox_updates",
    "note_inbox_updates",
    "settle_inbox_updates",
    // Settings' table: every change of what stands.
    "lift_dispatch_never",
    "allow_dispatch_to_any",
    "add_dispatch_grant",
    "revoke_dispatch_to_any",
    "revoke_dispatch_grant",
    "accept_project_dispatch",
    "decline_project_dispatch",
    "accept_project_dispatch_in",
    "decline_project_dispatch_in",
    "set_dispatch_workspace",
    "give_back_dispatch",
    "remove_dormant_dispatch",
    // A teammate's grant arriving: its list and its answers.
    "dispatch_arrival",
    "answer_dispatch_arrival",
    "dispatch_gone_told",
    // A dispatch refused while nobody was there: its list and its answers.
    "dispatch_away",
    "allow_dispatch_away",
    "dismiss_dispatch_away",
    "never_dispatch_away",
    // The person's "Got it" on a Notice that the project's presets or its hosts changed.
    "acknowledge_project_presets",
    "acknowledge_project_hosts",
    // What a task left on a branch of its own: the person's merge or discard (#1511).
    "task_branch_merge",
    "dispatch_worktree_discard",
    // ...and the delete of a merged branch whose folder is gone (#1472).
    "task_branch_delete",
    // The explorer's remove and merge of any branch folder (#1534).
    "worktree_remove",
    "worktree_merge",
    // The answer to a chat's sandbox block (#1538), and one to several tasks' (#1508).
    "allow_sandbox_block",
    "allow_sandbox_block_for_tasks",
    "keep_sandbox_block_for_tasks",
    // Widens what a persona's chats reach on this machine (#1362).
    "allow_persona_hosts",
    // Starts the person's editor on this machine's extension record (#1296).
    "open_extension_record",
    // Allow on Settings' Blocked lately (#1662): widens what every chat here reaches.
    "allow_blocked_host",
];

/// **The commands of who may dispatch to whom**, by name (spec #1483): every one is on
/// [`WINDOW_ONLY`] (a test holds that), and the list stands on its own, so a window-only
/// command of another kind added to [`WINDOW_ONLY`] is never counted as one of these.
pub const STANDING_DISPATCH: &[&str] = &[
    "dispatch_grants_needed",
    "dispatch_standing",
    "dispatch_grants",
    "allow_dispatch",
    "allow_dispatch_anywhere",
    "keep_dispatch_blocked",
    "never_dispatch",
    "lift_dispatch_never",
    "allow_dispatch_to_any",
    "add_dispatch_grant",
    "revoke_dispatch_to_any",
    "revoke_dispatch_grant",
    "accept_project_dispatch",
    "decline_project_dispatch",
    "accept_project_dispatch_in",
    "decline_project_dispatch_in",
    "set_dispatch_workspace",
    "give_back_dispatch",
    "remove_dormant_dispatch",
    "dispatch_arrival",
    "answer_dispatch_arrival",
    "dispatch_gone_told",
    "dispatch_away",
    "allow_dispatch_away",
    "dismiss_dispatch_away",
    "never_dispatch_away",
];

/// The commands that take a vault's sign-in from the person and use it (#1527, ADR 0052 as
/// amended 2026-10-09): a credential the person gives purlis reaches it through the window's
/// own IPC, from the window that began the set-up, and is never carried by a link. The window's
/// alone, as [`WINDOW_ONLY`]'s are.
pub const WINDOW_ONLY_CREDENTIALS: &[&str] = &[
    "vault_setup_accounts",
    "vault_setup_begin",
    "vault_setup_test",
    "vault_setup_create",
    "vault_setup_change",
    "vault_setup_cancel",
];

/// Whether `method` is the window's alone: in [`WINDOW_ONLY`] or [`WINDOW_ONLY_CREDENTIALS`].
pub fn window_only(method: &str) -> bool {
    WINDOW_ONLY.contains(&method) || WINDOW_ONLY_CREDENTIALS.contains(&method)
}

impl Server {
    /// `methods` is the app's command list (`ipc_commands.rs`), or the part of it this host
    /// answers; a method not in it, or the window's alone ([`window_only`]), is refused before
    /// `handler` is asked.
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
                .filter(|method: &String| !window_only(method))
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

#[cfg(test)]
mod credential_tests {
    use super::*;

    struct Nothing;

    impl Handler for Nothing {
        fn call(&self, _: String, _: Value) -> BoxFuture<'_, Result<Value, Value>> {
            Box::pin(async { Ok(Value::Null) })
        }
    }

    #[test]
    fn a_vaults_sign_in_set_up_is_never_among_the_methods_a_link_serves() {
        // #1527: a host built with the app's whole command list still leaves them out.
        let server = Server::new(
            "0.0.0+test",
            WINDOW_ONLY_CREDENTIALS
                .iter()
                .chain(["rename_chat", "answer_ask"].iter())
                .copied(),
            Nothing,
        );
        assert_eq!(
            server
                .methods
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["rename_chat"]
        );
        for method in WINDOW_ONLY_CREDENTIALS {
            assert!(window_only(method), "{method}");
        }
        assert!(window_only("answer_ask"));
        assert!(!window_only("rename_chat"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_command_of_who_may_dispatch_to_whom_is_on_the_window_only_list() {
        for command in STANDING_DISPATCH {
            assert!(WINDOW_ONLY.contains(command), "{command}");
        }
        // Named once each.
        let mut sorted = STANDING_DISPATCH.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), STANDING_DISPATCH.len());
    }
}
