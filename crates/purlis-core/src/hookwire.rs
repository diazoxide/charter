//! How a hook process reaches the app: one line on a unix socket the app owns.
//!
//! A hook runs inside the chat's own process tree, so everything it needs is already in its
//! environment — the socket to write to and the chat it belongs to. It looks nothing up and
//! reads no plane, which is what keeps it inside the spec's 50 ms. The one file it may open is
//! its chat's spool beside the socket, when the app does not take its line ([`spool`], FD-30).
//!
//! **It can never break a turn.** Every failure here is silent and fast: no app listening, a
//! socket that has gone, a payload that will not parse. A harness whose turn fails because
//! charter wanted to draw a spinner is worse than a spinner that is wrong.

// Only the unix half reads and writes a socket; `off_unix` names its own.
#[cfg(unix)]
use std::io;

use crate::state::{Detail, Ending, Event, Started};

// The channel is a unix socket. Windows has `AF_UNIX` but Rust's standard library does not
// expose it, and a socket file's `0600` — which is what keeps anything else on the machine
// out of the channel — has no mode bit to set there either. So Windows gets a second answer,
// and until it is written that answer is a REFUSAL and not a quiet no-op: `bind` returns
// `Unsupported`, `send` returns `Unsupported`, and an app that cannot open its hook channel
// says so instead of running with a channel nothing is listening on (charter-app#95).
//
// This used to be a `compile_error!`, which stopped the whole crate at expansion and so hid
// every OTHER thing Windows has to say about the core. What is below says the same thing at
// the same volume and lets the rest of the build be measured (M4).

/// The socket a hook writes to, in the environment of every session the app starts.
pub const SOCKET_ENV: &str = "PURLIS_HOOK_SOCKET";

/// How a hook's stderr begins when the app did not take its line: the hook says so there, and
/// the opencode shim shows that sentence in the chat's window (ruling V73c).
pub const NOT_TAKEN: &str = "purlis: the app did not take this";

/// Which chat the hook is running inside, in the same environment.
///
/// The app's own number for the chat, set at the `exec`, exactly as the Python charter sets
/// `$CHARTER_SESSION_ID` (`charter/hooks.py:_chat_id`). A hook already knows which chat it is
/// in; nothing has to be worked out from a payload.
pub const CHAT_ENV: &str = "PURLIS_CHAT";

/// The chat's own token, in the same environment: what the hook channel checks every line
/// against before it believes the chat number the line names ([`ChatTokens`]).
///
/// The app mints one per chat at the `exec` and keeps it in memory only. It is never passed on
/// to a chat another chat starts ([`NOT_INHERITED`]): a chat the app starts gets its own.
pub const TOKEN_ENV: &str = "PURLIS_CHAT_TOKEN";

/// Set to `1` in the environment of a chat the app actually started under a sandbox, and in no
/// other (#1338, #1345). Read through [`crate::sandbox::chat_is_sandboxed`]. The app sets it at
/// the launch, so a chat's own shell cannot claim it for the harness the hooks run under.
/// Never inherited ([`NOT_INHERITED`]), and nor is the chat's folder
/// ([`crate::sandboxblock::CHAT_DIR_ENV`]): an app started from a chat's shell does not hand
/// either on to the chats it starts.
pub const SANDBOXED_ENV: &str = "PURLIS_SANDBOXED";

/// Where Claude Code puts the conversation a hook is running in.
///
/// ADR 0024, C7, measured again on claude 2.1.276: it always equals the payload's
/// `session_id`. It is read only as a fallback, and it is what keeps the check against a
/// nested harness standing when a payload cannot be read — a pipe can be slow, an
/// environment cannot. A harness that sets no such variable simply has no fallback.
pub const CLAUDE_CONVERSATION_ENV: &str = "CLAUDE_CODE_SESSION_ID";

/// Where Claude Code puts the pid of the harness a hook is running under.
///
/// It is what tells `/clear` from a nested harness, and the Python charter's
/// `_record_harness_report` turns on it: a new conversation id from the SAME pid is `/clear`
/// (ADR 0024, C6) and the chat follows it; a new id from a DIFFERENT pid is a `claude` run
/// inside the chat (C5) and is ignored. Without it the two are the same event.
pub const CLAUDE_PID_ENV: &str = "CLAUDE_PID";

/// Everything a session must NOT inherit from whatever started charter.
///
/// charter may itself be launched from inside a harness session — an operator running the app
/// from a chat, or a scenario test — and then every chat it starts inherits that harness's
/// identity. A hook would report the LAUNCHER's conversation and process, so the chat's own
/// report would look like somebody else's: either refused as a nested harness, or, worse,
/// taken as one chat's when it is another's.
///
/// A scenario test found this: a fake harness with no identity of its own reported the pid of
/// the Claude Code session that had started the app.
///
/// Only the harness the app starts may set these, and it sets them for its own hooks.
///
/// **Nor where the launcher's chat was** (SI-1): `$CHARTER_WORKSPACE` and
/// `$CHARTER_PLANE_ROOT_SESSION` say where the app started THIS chat, and it sets them itself
/// (`start::ready`). An app launched from a chat pinned to `alpha` would otherwise hand
/// `alpha` to every chat it starts at the plane root, where the pin outranks the root. They are
/// removed before the chat's own are set, so a chat in a workspace still gets its own.
///
/// **Nor the launcher's chat token** ([`TOKEN_ENV`]): each chat gets its own at the `exec`,
/// and one the app inherited is never another chat's, whoever lists it.
///
/// **Nor the host's relaunch marker** ([`crate::noterminal::RELAUNCHED_ENV`]): it names the
/// app's own parent, and an app started from a chat's shell leaves that shell's terminal.
pub const NOT_INHERITED: &[&str] = &[
    CLAUDE_CONVERSATION_ENV,
    CLAUDE_PID_ENV,
    "CLAUDECODE",
    crate::active::WORKSPACE_ENV,
    crate::active::PLANE_ROOT_ENV,
    TOKEN_ENV,
    crate::noterminal::RELAUNCHED_ENV,
    SANDBOXED_ENV,
    crate::sandboxblock::CHAT_DIR_ENV,
];

/// Which conversation a report is of, and how well that is known.
///
/// **"I could not tell" and "someone is lying" are different evidence, and merging them was a
/// defect.** An earlier version answered `Option<String>` for both, so the chat's own harness
/// having a slow payload looked exactly like a nested one contradicting its environment — and
/// the honest harness's report was dropped, leaving the chat `running` for ever. A review
/// found it.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Conversation {
    /// The payload and the environment agree, or only one of them exists.
    Named(String),
    /// They disagree. Something is reporting a conversation that is not its own — a harness
    /// nested in the chat's shell is the case ADR 0024's C5 names.
    Contradicted,
    /// A payload that parsed and named no conversation charter recognises.
    ///
    /// **A harness speaking another dialect.** opencode puts it under `sessionID`, not
    /// `session_id` (ADR 0024, O1), and charter has not measured every harness there is. In a
    /// chat that has already adopted one, a report in a dialect charter cannot read is by
    /// definition not that chat's own — a review reproduced a nested `opencode` marking the
    /// outer Claude chat as waiting, mid-turn, because it inherits `CLAUDE_PID` and sets
    /// neither variable of its own.
    Foreign,
    /// Nothing could be read at all: no JSON, because charter's own deadline passed or the
    /// pipe was cut. Not evidence of anything, and never treated as such.
    #[default]
    Unknown,
}

/// What one hook tells the app.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Report {
    /// The app's number for the chat the hook ran in.
    pub chat: u32,
    /// The event the harness fired.
    pub event: Event,
    /// Which conversation the harness says this is.
    ///
    /// The app checks it against the conversation it started the chat under, so a harness
    /// nested INSIDE the chat cannot move the chat's state (ADR 0024, C5).
    #[serde(default)]
    pub conversation: Conversation,
    /// The pid of the harness that fired this, where the harness names one.
    ///
    /// The whole of what tells `/clear` (C6) from a nested harness (C5): both report an id
    /// the chat has not seen, and only the pid says which happened.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    /// The sub-agent this came from, where the payload's `agent_id` names one on a harness
    /// where that field was measured to mean a sub-agent (ADR 0066, a child run).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    /// What the harness said about this event beyond its name: what a `SessionStart` was for,
    /// and what a `SessionEnd` was for. Each is meaningless on the other's event and ignored
    /// there.
    #[serde(default)]
    pub detail: Detail,
}

impl Report {
    /// The report a hook makes, read from its environment and its payload.
    ///
    /// `payload` is the harness's JSON on stdin. It is read for one field and never required:
    /// the EVENT comes from argv, which charter wrote itself, so a payload that will not parse
    /// still reports the event — it just cannot say which conversation fired it.
    pub fn read(event: Event, payload: &str, env: &dyn Fn(&str) -> Option<String>) -> Option<Self> {
        // No chat means this harness was not started by the app — the operator's own
        // `claude` in a terminal, with hooks pointed here. There is no chat to move, and
        // inventing one would move somebody else's.
        let chat: u32 = env(CHAT_ENV)?.parse().ok()?;
        let read = serde_json::from_str::<serde_json::Value>(payload).ok();
        let field = |name: &str| {
            read.as_ref()
                .and_then(|payload| payload.get(name))
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        };
        let said = field("session_id");
        let source = field("source");
        // `/clear` fires `SessionEnd(reason=clear)` and then `SessionStart(source=clear)`
        // from the same process — measured on claude 2.1.276. Without this the first of the
        // two would end the chat and the second could never be heard.
        let reason = field("reason");
        Some(Self {
            chat,
            event,
            conversation: conversation(said, read.is_some(), env),
            pid: env(CLAUDE_PID_ENV)
                .and_then(|pid| pid.parse().ok())
                .filter(|pid| *pid > 0),
            agent: sub_agent(field("agent_id").as_deref(), env),
            detail: Detail {
                started: Started::of(source.as_deref()),
                ending: Ending::of(reason.as_deref()),
                // One bit of the prompt, and only on the event that carries one: whether the
                // person typed `/smart-close` (#1332). The prompt never leaves the hook.
                smart_close: event == Event::UserPromptSubmit
                    && field("prompt")
                        .is_some_and(|prompt| crate::state::smart_close_typed(&prompt)),
                unattended: crate::floorguard::unattended(field("permission_mode").as_deref()),
            },
        })
    }
}

/// The sub-agent a payload's `agent_id` names, only on a harness where that field was measured
/// to mean one ([`crate::handoffguard::Caller::from_a_subagent`]): elsewhere it is no agent.
pub fn sub_agent(agent_id: Option<&str>, env: &dyn Fn(&str) -> Option<String>) -> Option<String> {
    let harness = env(HARNESS_ENV);
    let caller = crate::handoffguard::Caller {
        agent_id,
        harness: harness.as_deref(),
        permission_mode: None,
    };
    caller
        .from_a_subagent()
        .then(|| agent_id.unwrap_or_default().to_owned())
}

/// The harness a chat runs, as charter put it into the chat's environment.
pub const HARNESS_ENV: &str = "PURLIS_HARNESS";

/// Which conversation this report is of, and how well that is known.
///
/// **The payload and the environment must AGREE, and this used to be an `or`.** That was
/// backwards, and a review proved it: the environment holds the OUTER chat's id — a harness
/// nested in the chat's own shell inherits it — so falling back to it when a payload is slow
/// does not lose the id, it substitutes the wrong one, and the nested harness's report is
/// then taken as the outer chat's. The very case ADR 0024's C5 exists to refuse.
///
/// The Python charter has always required the two to be equal (`charter/hooks.py`), and ADR
/// 0024's consequences say so outright: "A Claude Code report counts only when
/// `CLAUDE_CODE_SESSION_ID` equals its payload's id."
///
/// A harness that sets no such variable — Codex — has nothing to disagree with, so its
/// payload stands alone.
fn conversation(
    said: Option<String>,
    parsed: bool,
    env: &dyn Fn(&str) -> Option<String>,
) -> Conversation {
    // A payload charter READ but could not find a conversation in is a harness speaking a
    // dialect charter has not measured — not the same thing as a payload it could not read at
    // all, and the difference is what keeps a nested one out.
    if said.is_none() && parsed {
        return Conversation::Foreign;
    }
    match (said, env(CLAUDE_CONVERSATION_ENV)) {
        // Claude Code, and the two agree: this is its own hook (ADR 0024, C7).
        (Some(said), Some(here)) if said == here => Conversation::Named(said),
        // Claude Code, and they disagree. Something is reporting a conversation that is not
        // the one its own process is in.
        (Some(_), Some(_)) => Conversation::Contradicted,
        // The environment names one and the payload could not be read at all. That is not a
        // contradiction — it is charter failing to read, and the report must not be punished
        // for it. The pid is what identifies the harness in this case.
        (None, Some(_)) => Conversation::Unknown,
        // No such variable at all: a harness that is not Claude Code. Its payload is all
        // there is, and it is not contradicted by anything.
        (Some(said), None) => Conversation::Named(said),
        (None, None) => Conversation::Unknown,
    }
}

// ----------------------------------------------------------------------------------------
// each chat's token: what the channel checks a line's chat number against
// ----------------------------------------------------------------------------------------

/// One chat's token, as a hook or a `charter` command inside that chat carries it.
///
/// **Its `Debug` never shows the value**, so a line that prints one by mistake prints nothing
/// of it. The value leaves this type in two places only: into the chat's environment at the
/// `exec` ([`ChatToken::expose`]) and onto the wire, beside the line it vouches for.
#[derive(Clone, PartialEq, Eq)]
pub struct ChatToken(String);

impl ChatToken {
    /// The token in `env`'s [`TOKEN_ENV`], where there is one.
    pub fn read(env: &dyn Fn(&str) -> Option<String>) -> Option<Self> {
        env(TOKEN_ENV).filter(|value| !value.is_empty()).map(Self)
    }

    /// The token in this process's own environment, where there is one.
    pub fn from_env() -> Option<Self> {
        Self::read(&crate::envvar::var)
    }

    /// The value, for the one caller that must write it down: the chat's environment.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl From<&str> for ChatToken {
    fn from(value: &str) -> Self {
        Self(value.to_owned())
    }
}

impl std::fmt::Debug for ChatToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ChatToken(..)")
    }
}

/// How many random bytes a chat's token is: 256 bits from the operating system's generator.
const A_TOKEN_IS: usize = 32;

/// The token each chat the app started was given, by the chat's number. In memory only.
///
/// **The hook channel reads no line without it.** Every line names a chat, and the listener
/// hands a line on only when it carries that chat's token ([`ChatTokens::admits`]); anything
/// else is dropped without a word on the connection, as a line that will not parse is.
///
/// **And from inside the chat** (D-1407-6). A token is in the chat's environment, and on macOS a
/// process of the same user can read another's arguments and environment, so a token alone
/// does not say who sent a line. Each chat's program is recorded as it starts, by pid and by
/// when it started ([`ChatTokens::bind`]), and forgotten when it ends
/// ([`ChatTokens::program_ended`]). A live line is read only from a process that is that
/// program or runs inside it: in its session, or below it ([`descends`]). A chat whose program
/// is not recorded has no line read. A line its hooks spool while the app is away is checked by
/// its token's key alone: the spool is not bound to the program yet.
#[derive(Debug, Default)]
pub struct ChatTokens {
    held: std::sync::Mutex<std::collections::HashMap<u32, ChatToken>>,
    /// Each chat's program, once it has started.
    roots: std::sync::Mutex<std::collections::HashMap<u32, Program>>,
    /// The spool directory each token's key is recorded in as it is issued (FD-30), where
    /// this host has one.
    spool: Option<std::path::PathBuf>,
    /// The connections whose asks are being answered right now, by the number the listener
    /// dealt each: what an answer that takes minutes asks whether its asker is still there
    /// ([`Self::asker_gone`]).
    #[cfg(unix)]
    answering: std::sync::Mutex<std::collections::HashMap<u64, std::os::unix::net::UnixStream>>,
}

/// A chat's program: its pid, and when the process with that pid started, so a later process
/// given the same number is not taken for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Program {
    pub pid: u32,
    pub started: String,
}

impl Program {
    /// The process `pid` is now, or `None` where when it started cannot be read: such a program
    /// is never recorded, so its chat has no line read. A program that has already exited is
    /// one, whether or not it has been reaped (on macOS a zombie answers nothing).
    pub fn of(pid: u32) -> Option<Self> {
        #[cfg(unix)]
        {
            Some(Self {
                pid,
                started: purlis_same_user::Parents::started(pid)?,
            })
        }
        #[cfg(not(unix))]
        {
            let _ = pid;
            None
        }
    }
}

/// What the channel makes of a line's token and the process that sent it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Admission {
    /// The chat's token, from inside the chat.
    Admitted,
    /// No token, a wrong one, or a chat with none: dropped unread, and answered only with
    /// "this line does not carry chat N's token" (#1333), as a line that will not parse is
    /// answered that the app could not read it. Nothing the line asks is acted on.
    NoToken,
    /// The chat's token, from a process outside the chat: refused, and told so
    /// ([`OUTSIDE_THE_CHAT`]).
    Outside,
    /// The chat's token, for a chat whose program purlis could not confirm as it started:
    /// refused, and told so ([`UNCONFIRMED_PROGRAM`]).
    Unbound,
}

impl Admission {
    /// The sentence a refused line is answered with, where it is answered.
    pub fn refusal(self) -> Option<&'static str> {
        match self {
            Self::Outside => Some(OUTSIDE_THE_CHAT),
            Self::Unbound => Some(UNCONFIRMED_PROGRAM),
            Self::Admitted | Self::NoToken => None,
        }
    }
}

/// What a line for a chat whose program purlis could not confirm is answered with: the chat's
/// own start went wrong, not where the line came from.
pub const UNCONFIRMED_PROGRAM: &str = "purlis couldn't confirm this chat's program when it \
                                       started, so it can't read its lines; restart the chat.";

/// What a line that carried its chat's token from outside the chat is answered with (D-1407-9):
/// a block that says why, never a silence.
pub const OUTSIDE_THE_CHAT: &str = "This came from a process outside the chat (for example tmux, \
                                    nohup after its shell ended, or docker exec); run it from \
                                    the chat's own shell.";

impl ChatTokens {
    /// Tokens whose spool keys are recorded in `dir` as each is issued ([`spool::remember`]),
    /// so a line a chat spools can be checked by whichever host drains it.
    pub fn spooling_into(dir: std::path::PathBuf) -> Self {
        Self {
            held: std::sync::Mutex::default(),
            roots: std::sync::Mutex::default(),
            spool: Some(dir),
            #[cfg(unix)]
            answering: std::sync::Mutex::default(),
        }
    }

    /// Whether the asker on `connection` has gone: it closed its end while its ask was being
    /// answered. What an answer that waits for minutes looks at, so a command that was killed
    /// does not keep a thread of the app's waiting for it (#1441). A connection that is not
    /// being answered here has not gone: a caller that asks about one it made up waits on.
    ///
    /// **An asker says nothing while it is answered**, so anything readable on its connection
    /// is its end of file. A byte it did send out of turn is read as the same: it is not a
    /// purlis this app answers.
    pub fn asker_gone(&self, connection: u64) -> bool {
        #[cfg(unix)]
        {
            use std::io::Read;
            let answering = self
                .answering
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let Some(mut stream) = answering.get(&connection) else {
                return false;
            };
            // A look, not a wait: the shortest timeout a socket takes.
            if stream
                .set_read_timeout(Some(std::time::Duration::from_millis(1)))
                .is_err()
            {
                return true;
            }
            let mut one = [0u8; 1];
            let gone = match stream.read(&mut one) {
                Ok(_) => true,
                Err(why) => !matches!(
                    why.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ),
            };
            let _ = stream.set_read_timeout(Some(A_REPORT_TAKES_AT_MOST));
            gone
        }
        #[cfg(not(unix))]
        {
            let _ = connection;
            false
        }
    }

    /// `stream` is the connection numbered `connection`, whose ask is about to be answered.
    #[cfg(unix)]
    fn answering(&self, connection: u64, stream: std::os::unix::net::UnixStream) {
        self.answering
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(connection, stream);
    }

    /// The ask on `connection` has its answer.
    #[cfg(unix)]
    fn answered(&self, connection: u64) {
        self.answering
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&connection);
    }

    /// A fresh token for `chat`, replacing any it had: 32 bytes from the operating system's
    /// generator, written as hex.
    ///
    /// Its spool key is on disk before it returns, where these tokens spool, and stays there
    /// until the chat ends ([`spool::end_chat`], [`spool::forget_all_but`]). A key that cannot
    /// be recorded is said in the log and the token is issued all the same: the chat runs, and
    /// a line it spools reads as `no-key` at the drain rather than as the chat's.
    pub fn issue(&self, chat: u32) -> std::io::Result<ChatToken> {
        let mut bytes = [0u8; A_TOKEN_IS];
        getrandom::fill(&mut bytes).map_err(std::io::Error::other)?;
        let token = ChatToken(bytes.iter().map(|byte| format!("{byte:02x}")).collect());
        #[cfg(unix)]
        if let Some(dir) = &self.spool
            && let Err(why) = spool::remember(dir, chat, &token)
        {
            tracing::warn!(
                "purlis: chat {chat}'s spool key was not recorded ({why}); a line it spools \
                 cannot be checked"
            );
        }
        self.held().insert(chat, token.clone());
        self.rooted().remove(&chat);
        Ok(token)
    }

    /// Records `program` as chat `chat`'s program: its lines are read from it and from what runs
    /// inside it, and from nothing else.
    pub fn bind(&self, chat: u32, program: Program) {
        self.rooted().insert(chat, program);
    }

    /// Chat `chat`'s program `pid` has ended: its token and its program are forgotten, so a
    /// later process given the same number speaks for nothing. A program that is not the one
    /// recorded (an end heard after the chat started again) changes nothing.
    pub fn program_ended(&self, chat: u32, pid: u32) {
        let mut roots = self.rooted();
        if roots.get(&chat).is_some_and(|program| program.pid == pid) {
            roots.remove(&chat);
            drop(roots);
            self.held().remove(&chat);
        }
    }

    /// [`Self::issue`], with this process recorded as the chat's program: for a test that
    /// stands in for a chat's hooks itself, or starts them as its children.
    #[cfg(any(test, feature = "test-support"))]
    pub fn issue_to_this_process(&self, chat: u32) -> std::io::Result<ChatToken> {
        let token = self.issue(chat)?;
        let me = std::process::id();
        let program = Program::of(me).unwrap_or(Program {
            pid: me,
            started: String::new(),
        });
        self.bind(chat, program);
        Ok(token)
    }

    /// Forgets `chat`'s token: a line for it is dropped from now on.
    pub fn forget(&self, chat: u32) {
        self.held().remove(&chat);
        self.rooted().remove(&chat);
    }

    fn rooted(&self) -> std::sync::MutexGuard<'_, std::collections::HashMap<u32, Program>> {
        self.roots
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Whether a line for `chat` carrying `token` from process `peer` is read
    /// ([`Self::admission`] is [`Admission::Admitted`]).
    pub fn admits(&self, chat: u32, token: Option<&str>, peer: Option<u32>) -> bool {
        self.admission(chat, token, peer) == Admission::Admitted
    }

    /// What a line for `chat` carrying `token` from process `peer` is.
    ///
    /// The token is compared in constant time, so how long a wrong token takes to refuse says
    /// nothing about how much of it was right. Then `peer` must be the chat's recorded program,
    /// still the process that started then, or run inside it ([`descends`]). No peer, and a
    /// chat with no program recorded, are [`Admission::Outside`].
    pub fn admission(&self, chat: u32, token: Option<&str>, peer: Option<u32>) -> Admission {
        self.admission_by(chat, token, peer, &|peer, program| descends(*peer, program))
    }

    /// [`Self::admission`], asking `inside` whether `peer` runs inside a chat's program.
    pub(crate) fn admission_by<P>(
        &self,
        chat: u32,
        token: Option<&str>,
        peer: Option<P>,
        inside: &dyn Fn(&P, &Program) -> bool,
    ) -> Admission {
        use subtle::ConstantTimeEq;

        let Some(token) = token else {
            return Admission::NoToken;
        };
        let carried = self
            .held()
            .get(&chat)
            .is_some_and(|held| bool::from(held.0.as_bytes().ct_eq(token.as_bytes())));
        if !carried {
            return Admission::NoToken;
        }
        let Some(program) = self.rooted().get(&chat).cloned() else {
            return Admission::Unbound;
        };
        if peer.is_some_and(|peer| inside(&peer, &program)) {
            Admission::Admitted
        } else {
            Admission::Outside
        }
    }

    fn held(&self) -> std::sync::MutexGuard<'_, std::collections::HashMap<u32, ChatToken>> {
        self.held
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// A process as the channel sees it (D-1407-10): itself, the leader of its session and its
/// ancestors, each with when it started, read from the kernel in this process. A line is
/// judged by this once its token has checked, so a connection with no token, or one that never
/// writes, costs no reading at all. Every purlis sender stays until the app has let its
/// connection go, so it is still running when it is read; a process given a sender's number
/// since is told apart by when it started, since it is below no chat's program.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Seen {
    pub pid: u32,
    started: String,
    /// Its session's leader, then its parent and up: each with when it started, where known.
    around: Vec<(u32, Option<String>)>,
}

impl Seen {
    /// Process `pid` now, or `None` where it cannot be read (it has gone, or the kernel would
    /// not say), which no chat admits.
    pub fn of(pid: u32) -> Option<Self> {
        Self::all(&[pid]).and_then(|mut seen| seen.pop())
    }

    /// Each of `pids` now, or `None` where any cannot be read.
    ///
    /// Read in this process, a pid at a time: `/proc` on Linux, and `proc_pidinfo` on macOS
    /// (D-1407-8, `purlis_same_user::Parents`), where it was one `/bin/ps` run under the fork
    /// lock before. No program is started, so a burst of lines no longer queues on that lock.
    pub fn all(pids: &[u32]) -> Option<Vec<Self>> {
        #[cfg(unix)]
        {
            use purlis_same_user::Parents;
            pids.iter()
                .map(|&pid| {
                    let started = Parents::started(pid)?;
                    let mut around = Vec::new();
                    if let Ok(session) = purlis_same_user::session_of(pid) {
                        around.push((session, Parents::started(session)));
                    }
                    for up in Parents::chain(pid) {
                        around.push((up, Parents::started(up)));
                    }
                    Some(Self {
                        pid,
                        started,
                        around,
                    })
                })
                .collect()
        }
        #[cfg(not(unix))]
        {
            let _ = pids;
            None
        }
    }

    /// Whether it runs inside `program`: it is that process, or its session's leader or one of
    /// its ancestors is, each by pid AND by when it started.
    pub fn inside(&self, program: &Program) -> bool {
        std::iter::once((self.pid, Some(&self.started)))
            .chain(
                self.around
                    .iter()
                    .map(|(pid, started)| (*pid, started.as_ref())),
            )
            .any(|(pid, started)| {
                pid == program.pid && started.is_some_and(|started| *started == program.started)
            })
    }
}

/// Whether process `peer` is `program`, or runs inside it, now ([`Seen::of`] and
/// [`Seen::inside`]): every doubt is no.
pub fn descends(peer: u32, program: &Program) -> bool {
    Seen::of(peer).is_some_and(|seen| seen.inside(program))
}

/// The line `what` is on the wire: its JSON with `token` beside it, and a newline.
pub(crate) fn line_with(
    token: Option<&ChatToken>,
    what: &impl serde::Serialize,
) -> std::io::Result<Vec<u8>> {
    let mut value = serde_json::to_value(what).map_err(std::io::Error::other)?;
    if let (Some(token), Some(fields)) = (token, value.as_object_mut()) {
        fields.insert(TOKEN.to_owned(), serde_json::Value::String(token.0.clone()));
    }
    let mut line = serde_json::to_vec(&value).map_err(std::io::Error::other)?;
    line.push(b'\n');
    Ok(line)
}

/// The field a line's token travels in.
const TOKEN: &str = "token";

/// A line off the socket, read, and the token it carried taken off it — so nothing past the
/// listener ever holds one.
fn read_line(line: &str) -> Option<(Line, Option<String>)> {
    let mut value = serde_json::from_str::<serde_json::Value>(line).ok()?;
    let token = match value.as_object_mut()?.remove(TOKEN) {
        Some(serde_json::Value::String(token)) => Some(token),
        _ => None,
    };
    Some((serde_json::from_value(value).ok()?, token))
}

// ----------------------------------------------------------------------------------------
// the other direction: a chat ASKING the app to open a chat (charter-app#204)
// ----------------------------------------------------------------------------------------

/// What a chat asks the app for, when reporting is not what it wants.
///
/// **This is the one verb on this socket that makes something happen in the world**, and it is
/// why the two lines below are a handshake rather than a single message. A report moves a
/// chat that already exists; an open makes one, whose first message is a brief another chat
/// wrote and nobody approved (#1444): what may start is the app's decision, never a word of
/// the request. See [`OpenChat`] for what the ticket does and does not protect against.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Ask {
    /// Mint a ticket for the chat this asker is running inside. The app answers with
    /// [`Answer::Ticket`], or with [`Answer::No`] when it will not.
    Ticket { chat: u32 },
    /// Spend a ticket: open the chat this describes. Boxed because it carries the whole
    /// first message, which is the brief, and the other variant is two words — clippy's
    /// `large_enum_variant` is right about it.
    Open(Box<OpenChat>),
    /// Spend a ticket: hand a report back to the chat that opened this one (charter-app#259).
    /// Boxed for `Open`'s reason.
    Report(Box<ReportBack>),
    /// Write this chat's session record: a brokered write (ADR 0067 §2, #1332). The app
    /// answers [`Answer::Recorded`], or [`Answer::No`] with the refusal. Boxed: it carries the
    /// record's whole body.
    SessionRecord(Box<RecordAsk>),
    /// Write one of the project's own files for this chat: a brokered write (ADR 0067 §2,
    /// #1333), as `purlis persona remember`, `purlis workspace remember` and `purlis workspace
    /// todo` hand it over in a chat the app started. The app answers [`Answer::Written`], or
    /// [`Answer::No`] with the refusal. Boxed: it carries the memory's whole text.
    Write(Box<WriteAsk>),
    /// Clone a repo, or cut a worktree, for this chat: a brokered git action (ADR 0067 §2,
    /// #1335). The app answers [`Answer::Said`], or [`Answer::No`] with the refusal. Boxed for
    /// `Open`'s reason.
    Git(Box<GitAsk>),
    /// Commit in the branch folder this chat stands in: a brokered git action (ADR 0067 §2,
    /// #1055). The app answers [`Answer::Said`], or [`Answer::No`] with the refusal. Boxed: it
    /// carries the whole message.
    Commit(Box<CommitAsk>),
    /// List the project's vaults for this chat (#1430): `purlis vault list` from a sandboxed
    /// chat, whose sandbox denies it every provider's own files. The app answers
    /// [`Answer::Vaults`] from the registry alone: names, tags, and whether the persona it
    /// recorded for the chat may use each. **It names no persona**: that is the app's record of
    /// the chat whose token the line carries. No ticket, for a record's reason: it changes
    /// nothing.
    Vaults { chat: u32 },
    /// Where this chat is working: who asked for it, its sibling tasks and the other chats
    /// running as its persona (#1450). The app answers [`Answer::Working`] from its own record
    /// of its open chats, or [`Answer::No`] with the refusal.
    WhereWorking(WhereWorking),
    /// Spend a ticket: dispatch a task to a persona (#1436). The app answers
    /// [`Answer::Dispatched`], [`Answer::NeedsGrant`], or [`Answer::No`] with the refusal.
    /// Boxed for `Open`'s reason.
    Dispatch(Box<DispatchAsk>),
    /// Ask after a task this chat dispatched (#1441): wait for its report, list this chat's
    /// tasks, or cancel one. The app answers [`Answer::Task`], or [`Answer::No`] with the
    /// refusal. No ticket: see [`crate::dispatched::Asked`].
    Task(Box<crate::dispatched::Asked>),
}

/// A chat asking where it is working ([`crate::awareness`], #1450).
///
/// **It names no chat but itself, and that is the guard**, as [`RecordAsk`]'s: the picture is
/// drawn from the app's record of the chat whose token the line carries, so a line can only
/// ever learn what that chat may be told. No ticket, for a record's reason: nothing is made
/// or moved by the answer.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WhereWorking {
    /// The chat asking, from [`CHAT_ENV`].
    pub chat: u32,
    /// Why it asks: the command, the start briefing, or a turn beginning.
    #[serde(default)]
    pub tell: crate::awareness::Tell,
}

/// A task a chat asks the app to dispatch, as `purlis dispatch` and the `dispatch` chat tool
/// hand it over (#1434, #1436).
///
/// **It says nothing about the chat that asks but its number, and that is the guard.** The
/// asking chat's persona, profile, folder, name and lineage, the stamp the new chat opens with
/// and the grants in force are all the app's own record of the chat whose token the line
/// carries ([`crate::dispatchdecision`]). A request cannot say who asks, where from, or that a
/// grant exists: there is no field to say it in. What it does say is what an agent chooses:
/// which persona, what the task is called, which of the project's profiles to start it on,
/// whether it works in another workspace or a worktree of its own, and the brief. A profile is a name, only ever looked up among the profiles the project
/// offers on this machine and has approved ([`crate::personaprofile::for_dispatch`]).
///
/// The ticket is [`OpenChat`]'s, minted and spent the same way, so one run of the command
/// starts at most one chat.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DispatchAsk {
    /// The chat that asks, from [`CHAT_ENV`]: the chat whose token the line must carry.
    pub chat: u32,
    /// The persona the new chat runs as. `None` is the asking chat's own, whatever the app's
    /// record says that is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    /// The task's name, which the persona chat is called and listed under. Held to
    /// `reopen::label` by the command and again by the app.
    pub name: String,
    /// The brief, verbatim. The app puts its own stamp in front of it.
    pub brief: String,
    /// The profile the asking chat asks for the new chat to start on (`--profile`, #1445).
    /// `None` is the persona's own, else the asking chat's. One the project does not offer
    /// here, or has not approved, is a refusal and never another profile in its place.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    /// Where the new chat is to work (`--in`, #1453): `worktree`, or `workspace:<name>`.
    /// `None` is the persona's own default, else the asking chat's folder. **A word, never a
    /// place**: which repo a worktree is cut from, and what its folder and branch are called,
    /// are the app's ([`crate::dispatchplace`]), and a workspace is looked up among the
    /// project's own. There is no field for a folder or a branch.
    #[serde(default, rename = "in", skip_serializing_if = "Option::is_none")]
    pub place: Option<String>,
    /// See [`OpenChat`]. Minted by the app, spent once, never written down.
    pub ticket: String,
}

/// What a task's report says beside its text, as `purlis dispatch report` hands it over
/// (#1436). Where the persona chat's session record is, the report's fourth part, is not here:
/// the app says it, from its own record of what it wrote for that chat.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TaskReport {
    pub outcome: crate::handback::Outcome,
    /// What changed, in the persona chat's words: files, commits, a branch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub changed: Option<String>,
}

/// A write a chat asks the app to make for it ([`crate::brokered`]).
///
/// **It names no place, and that is the guard**, as [`RecordAsk`]'s: the workspace and the
/// persona it is written to are the app's record of the chat whose token the line carries.
/// No ticket, for a record's reason: what it writes is the chat's own.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WriteAsk {
    /// The chat asking, from [`CHAT_ENV`].
    pub chat: u32,
    /// What to write.
    #[serde(flatten)]
    pub write: crate::brokered::Write,
}

/// A git action a chat asks the app to take for it, as `purlis clone` and `purlis worktree add`
/// hand it over: **a brokered write** (ADR 0067 §2, #1335). A sandboxed chat may not write a
/// clone's `.git/config`, its hooks or the editor settings a checkout carries; the app writes
/// them, with the same core code the terminal's commands run ([`crate::gitbroker`]).
///
/// **What it names is checked, never trusted.** The workspace must be one the chat already
/// writes its ordinary files in, the clone's host one the chat's sandbox may reach, and who
/// the piece log credits is the app's record of the chat whose token the line carries.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GitAsk {
    /// The chat that asks, from [`CHAT_ENV`].
    pub chat: u32,
    /// The workspace, as the command resolved it.
    pub workspace: String,
    pub work: GitWork,
}

/// A commit a sandboxed chat asks the app to make in the branch folder it stands in, as
/// `purlis worktree commit` hands it over: **a brokered git action** (ADR 0067 §2, #1055). A
/// linked worktree keeps its index, its HEAD and its objects in its clone's `.git`, which the
/// chat may not write; the app stages and commits ([`crate::gitbroker::commit`]).
///
/// **It names the message and what to stage, and nothing else, and that is the guard**: no
/// repository, folder, branch, git directory, author, date or option. The folder is where the
/// app recorded the chat whose token the line carries as standing, and the branch is the one
/// that folder is on. A line that carries any other field is not read. No ticket, for a
/// record's reason: the commit is the chat's own work on its own branch.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommitAsk {
    /// The chat that asks, from [`CHAT_ENV`].
    pub chat: u32,
    /// The commit message, as written.
    pub message: String,
    /// What to stage for it.
    pub stage: Stage,
}

/// What a [`CommitAsk`] stages before it commits.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    /// Every change to a file git already tracks in the folder (`git add -u`).
    Tracked,
    /// These paths, each relative to where the chat stands and inside its folder.
    Paths(Vec<String>),
}

/// What a [`GitAsk`] asks for: one of the two commands, with its arguments.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GitWork {
    /// `purlis clone <repo>…`.
    Clone { repos: Vec<String> },
    /// `purlis worktree add <repo> <piece> [--branch]`.
    WorktreeAdd {
        repo: String,
        piece: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<String>,
    },
}

/// A session record a chat asks the app to write for it, as `purlis session record` and the
/// MCP server's `session_record` tool both hand it over: **a brokered write** (ADR 0067 §2,
/// #1332). The chat's sandbox may forbid the write; the app makes it, with the same core code
/// the terminal's command uses ([`crate::sessionrecord::record`]).
///
/// **It names no place, and that is the guard.** Where the record goes, the persona and every
/// other fact in its frontmatter are the app's record of the chat whose token the line carries,
/// never the request's. The request carries only what the model writes (the title and the
/// body) and the pieces it says it worked in, which git is asked about before any is recorded.
///
/// No ticket: a record is the chat's own to write, with or without a smart close, and a line
/// can only ever name the chat whose token it carries. What only a person grants, the close of
/// the tab, is the smart-close pass the app holds, never anything on this line.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RecordAsk {
    /// The chat whose record it is, from [`CHAT_ENV`].
    pub chat: u32,
    pub title: String,
    pub body: String,
    /// The pieces named as `<repo>/<piece>`, as `--piece` takes them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pieces: Vec<String>,
    /// The directory the asker runs in, so the piece it stands in is recorded as the terminal's
    /// command records it. Only ever looked up among the pieces git reports.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<std::path::PathBuf>,
}

/// A report a handed-off chat sends back, as `charter handoff report` hands it over.
///
/// **It names no recipient, and that is the guard.** The chat it goes to is the one the app
/// recorded as this chat's parent when it opened it for a `--report` handoff
/// (`reopen::HandedFrom`), so nothing a chat can say sends a report anywhere else. The ticket
/// is [`OpenChat`]'s, spent the same way, so no single line on the socket reports anything and
/// no line that reported can report again.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ReportBack {
    /// The chat that is reporting — the app's own number for it, from [`CHAT_ENV`].
    pub chat: u32,
    /// The report, already through `handoff::report_summary`; the app asks again.
    pub summary: String,
    /// See [`OpenChat`]. Minted by the app, spent once, never written down.
    pub ticket: String,
    /// A task's outcome and what changed (#1436). Absent from a handoff's report back, which
    /// is its summary alone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task: Option<TaskReport>,
}

/// A handoff the app is asked to open, as `charter handoff` hands it over.
///
/// Every field here has already been through the command's own refusals
/// ([`crate::handoff`]): the workspace is one this plane has (or one this call is creating),
/// the persona is one it defines, and `message` is the stamped first message a chat may be
/// started on. The app asks the questions only it can answer — is this a chat I started, is
/// its harness one that takes a first message, is the workspace's directory there — and
/// refuses rather than guessing. **And it decides the handoff, as it decides any dispatch**
/// (#1444): a handoff is a dispatch in handoff mode, so the app answers [`Answer::Opened`],
/// [`Answer::NeedsGrant`] where the person is being asked for the pair, or [`Answer::No`].
///
/// # The ticket, and exactly what it is worth
///
/// `ticket` is a value the app minted moments earlier, on this same connection, in answer to
/// [`Ask::Ticket`] for this same chat. It is spent here and can never be spent again.
///
/// **What that buys.** The socket carries no ambient "open a chat" verb: no single line on it
/// opens anything, and no line that opened something can open a second thing. The ticket
/// never touches a readable surface — not the environment, not argv (which is where the brief
/// itself travels, and which `charter handoff`'s credential refusal already calls readable by
/// any local process), not a file, not the transcript — so nothing that can read those can
/// produce one. And because a ticket belongs to the one connection it was minted on, and the app
/// forgets it the moment it is spent or the deadline passes, one `charter handoff`
/// opens at most one chat.
///
/// **What it does not buy, plainly.** It does not say which process in the chat asked. A
/// process already inside the chat's own process tree has the socket path and the chat number
/// in its environment, so it can run this same two-line exchange — or simply exec
/// `charter handoff` itself. charter cannot tell that process from the chat's own turn: a helper
/// sub-agent is refused by the tool hook where the hook can tell one made the call, and that is
/// advice it meets, not a boundary. So **what consents to a handoff is never the request**: it
/// is the app's own decision, from its own record of the asking chat and the dispatch grants
/// the person made (`dispatchdecision`), and a grant for a pair covers whatever in that chat's
/// tree asks in its name. ADR 0024 concedes exactly this for *moving* a chat, and this is the
/// same concession for opening one.
///
/// **Why that is acceptable, and it is the whole of the justification.** Opening a chat with
/// an arbitrary first message and the operator's authority is *already* within reach of any
/// process in the tree, with no app involved: `claude -p "…"`, or
/// `charter claude --workspace x "<msg>"`, starts a harness headless, in the background, where
/// nobody sees it. What the app opens instead is a tab in the target workspace's strip, in the
/// window the operator is looking at, whose first message is stamped with the chat it came from
/// (`⟨handoff from chat N · …⟩`). That is strictly more visible than what the tree could
/// already do. **Visibility is the control here, not the ticket**, which is why the app never
/// opens a handed-off chat anywhere but on a strip, and why the stamp is not optional.
/// (charter-app#204, where the operator ruled on this with the premise corrected: an earlier
/// framing called opening a chat "a larger primitive than moving a tab", and that does not
/// survive counting the harness itself.)
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct OpenChat {
    /// The chat that is handing off — the app's own number for it, from [`CHAT_ENV`].
    pub chat: u32,
    /// The workspace the new chat belongs to for life.
    pub workspace: String,
    /// `Some(vision)` when this handoff is creating that workspace, which is the only shape
    /// `--create` can reach this in: `--create` without `--vision` is refused long before.
    pub create_vision: Option<String>,
    /// The persona the operator named, or none for the plane's own answer.
    pub persona: Option<String>,
    /// The whole of what the new chat is sent: the stamp line, a blank line, the brief.
    pub message: String,
    /// See the note above. Minted by the app, spent once, never written down.
    pub ticket: String,
    /// The short task name `--name` gave, which the new chat is called instead of its default
    /// (charter-app#258). Held to `reopen::label` by the command and again by the app.
    /// Absent from a `charter` older than it, which reads as no name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Whether `--report` asked for an answer (charter-app#259). Absent reads as no.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub report: bool,
}

/// A handoff's row in the project's dispatch log (`dispatch::record_handoff`), as the app that
/// opened the chat wrote it (#1421): a sandboxed chat may not write the project's
/// `personas/_dispatch/`, and the app is not sandboxed.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Row {
    Written,
    /// Not written, and the OS's words for why ([`crate::rewrite::os_words`]).
    Unwritten {
        why: String,
    },
}

/// What the app answers an [`Ask`] with. One line, on the same connection.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Answer {
    /// A ticket to spend on the next line, and on no other connection.
    Ticket { ticket: String },
    /// The chat is open, under this number on the app's board, and what became of its row in
    /// the project's dispatch log, which the app writes (#1421). `None` from an app that leaves
    /// the row to the command, as every app did before. `note` is one sentence the app has to
    /// say about how it opened it: that the chat runs on the asking chat's profile because its
    /// persona's own is not offered on this machine (#1445, D-1445-8). None when there is
    /// nothing to say, and from an app that never says it.
    Opened {
        chat: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        row: Option<Row>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        note: Option<String>,
    },
    /// The report was handed back: to the chat named `to`, or — where that chat is gone —
    /// kept for its workspace, `kept_for`, for the next chat that starts there.
    Reported {
        to: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        kept_for: Option<String>,
    },
    /// A task's report was handed to the chat named `to`, **and the task is finished**
    /// (#1485): purlis ends the reporting chat's program once this turn is over. Said only
    /// where the app will: never for a handoff's chat, a task that came out blocked, or one
    /// the person started from a tab, which answer [`Self::Reported`].
    Finished { to: String },
    /// The app will not, and this is the sentence saying why. The asker prints the command
    /// to run in a terminal and says this underneath it: a refusal the operator cannot see
    /// is a handoff that vanished.
    No { why: String },
    /// The session record is written (#1332): where, plane-relative, and whether the chat's
    /// tab now closes, which it does only under a smart-close pass. `warnings` is what did not
    /// follow it ([`crate::sessionrecord::Recorded::warnings`]).
    Recorded {
        record: String,
        closes: bool,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        warnings: Vec<String>,
    },
    /// A brokered write is made (#1333): the workspace or persona it went to, and the file,
    /// project-relative.
    Written { to: String, path: String },
    /// A brokered git action ran (#1335): every line the command said, in order, and its exit
    /// status. The asker prints them as its own run would have.
    Said {
        lines: Vec<crate::repocmd::Say>,
        code: u8,
    },
    /// The project's vaults, for the chat that asked (#1430): the persona the app started it
    /// as, and each vault with whether that persona may use it. No value and no key name.
    Vaults {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        persona: Option<String>,
        vaults: Vec<crate::secrets::brokered::Listed>,
        /// Whether a policy forbids the person's Allow for a vault here, so the listing does
        /// not name it as a way forward.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        allow_locked: bool,
    },
    /// Where the chat is working (#1450): names, tasks and states from the app's own record,
    /// and what changed since the chat was last told. Boxed: it is the largest answer.
    Working(Box<crate::awareness::Working>),
    /// A task was dispatched (#1436): the persona chat is running, under this number on the
    /// app's board and this name, as `persona` (none for a chat on no persona). `note` is
    /// [`Self::Opened`]'s: something the asking chat should be told about how it was started,
    /// today that it runs on the asking chat's profile because its persona's own is not
    /// offered on this machine (#1445, D-1445-8).
    ///
    /// `works` says where the new chat works, where that is not the asking chat's folder
    /// (#1453): another workspace, or a worktree of its own with the branch purlis cut for
    /// it. The app's words, said after "It works".
    Dispatched {
        chat: u32,
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        persona: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        note: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        works: Option<String>,
    },
    /// Nothing has started **yet**: the asking chat's persona has no dispatch grant for `to`,
    /// and the person is being asked for one on the asking chat's tab (#1434, #1437). `from`
    /// is the asking chat's persona, by the app's record. The dispatch is held by the app:
    /// where the person allows it, it starts then, and where they keep it blocked, the asking
    /// chat is told on its next turn.
    ///
    /// `waiting` is set where the person is already being asked about this pair for an
    /// earlier task of this chat, by that task's name: **this** ask was not held, because the
    /// person saw one brief and only that one starts.
    NeedsGrant {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        from: Option<String>,
        to: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        waiting: Option<String>,
    },
    /// What became of an ask after a dispatched task (#1441).
    Task(Box<crate::dispatched::Answered>),
}

/// How long a ticket lives unspent.
///
/// `charter handoff` spends its ticket on the very next line, milliseconds after the mint, so
/// this bounds only a ticket that was minted and abandoned: a `charter` killed between the two
/// lines, or something that minted with no intention of spending. An abandoned ticket holds up
/// nothing but its own connection; it counts towards [`MOST_LIVE_TICKETS_A_CHAT`] until it
/// expires. Seconds, not minutes, for that reason.
pub const A_TICKET_LIVES: std::time::Duration = std::time::Duration::from_secs(10);

/// The most tickets one chat may have live at once.
///
/// A chat that fans out runs several commands in one step, each with a connection and a ticket
/// of its own, minted before any is spent. This is room for the most tasks one chat may have
/// running at the highest setting anyone has asked for, and still a bound: mints that are
/// never spent cannot grow the map for longer than [`A_TICKET_LIVES`].
pub const MOST_LIVE_TICKETS_A_CHAT: usize = 16;

/// The tickets an app has minted and not yet seen spent: one per connection, and at most
/// [`MOST_LIVE_TICKETS_A_CHAT`] per chat.
///
/// This is the whole of the ticket's mechanism, kept in the core so that it is plain Rust
/// with tests of its own, and so the app holds a value rather than a policy. The argument for
/// what it is worth is on [`OpenChat`].
#[derive(Debug, Default)]
pub struct Tickets {
    /// By the chat a ticket was minted for and the connection it was minted on.
    live: std::sync::Mutex<std::collections::HashMap<(u32, u64), Live>>,
}

#[derive(Debug)]
struct Live {
    ticket: String,
    until: std::time::Instant,
}

impl Tickets {
    /// A ticket for `chat`, bound to `connection`, or why not.
    ///
    /// **One live ticket per connection, and each run of a command is a connection.** A chat
    /// that dispatches six tasks in one step runs six commands, and each mints and spends its
    /// own (#1441): a ticket is the property of the run that asked for it, so no run can spend,
    /// replace or cancel another's. A second mint on a connection that already holds a live
    /// ticket is refused rather than replacing it, which is what keeps one run of a command to
    /// one thing started.
    ///
    /// The value is two v4 UUIDs, 244 random bits from the operating system's generator: the
    /// same source charter already trusts for the session ids it mints (`harness::SessionId`).
    pub fn mint(
        &self,
        chat: u32,
        connection: u64,
        now: std::time::Instant,
    ) -> Result<String, String> {
        let mut live = self.held();
        live.retain(|_, one| one.until > now);
        if live.contains_key(&(chat, connection)) {
            return Err(format!(
                "this connection already holds a ticket for chat {chat}; spend it first"
            ));
        }
        if live.keys().filter(|(of, _)| *of == chat).count() >= MOST_LIVE_TICKETS_A_CHAT {
            return Err(format!(
                "chat {chat} has {MOST_LIVE_TICKETS_A_CHAT} requests to the app under way at \
                 once; try again in a few seconds"
            ));
        }
        let ticket = format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        );
        live.insert(
            (chat, connection),
            Live {
                ticket: ticket.clone(),
                until: now + A_TICKET_LIVES,
            },
        );
        Ok(ticket)
    }

    /// Spends `ticket` for `chat` on `connection`, or says why it will not open anything.
    ///
    /// **The ticket this connection holds for the chat is gone after this whatever the
    /// answer.** A wrong guess spends the real ticket too, so a guesser gets one try per mint.
    /// A try on any other connection finds nothing to spend and costs nobody anything: a ticket
    /// never spends off the connection it was minted on. The comparison runs over every byte
    /// whatever it finds, so how long a wrong ticket takes to refuse says nothing about how
    /// much of it was right.
    pub fn spend(
        &self,
        chat: u32,
        connection: u64,
        ticket: &str,
        now: std::time::Instant,
    ) -> Result<(), String> {
        let Some(live) = self.held().remove(&(chat, connection)) else {
            return Err(NO_TICKET.to_owned());
        };
        let same = live.ticket.len() == ticket.len()
            && live
                .ticket
                .bytes()
                .zip(ticket.bytes())
                .fold(0u8, |differs, (a, b)| differs | (a ^ b))
                == 0;
        if !same || live.until <= now {
            return Err(NO_TICKET.to_owned());
        }
        Ok(())
    }

    fn held(&self) -> std::sync::MutexGuard<'_, std::collections::HashMap<(u32, u64), Live>> {
        self.live
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// What every spend that opens nothing says. One sentence for every reason, so the refusal
/// does not tell a guesser which part it got wrong.
pub const NO_TICKET: &str =
    "that open was not preceded by a ticket this app minted for this chat on this connection";

/// One line read off the socket, whichever kind it is.
///
/// **Untagged, and the report comes first.** A [`Report`] requires both `chat` and `event`,
/// and no [`Ask`] carries an `event`, so the two can never be mistaken for each other.
///
/// Every kind is read with the chat's token taken off it first ([`read_line`]), so none of
/// them carries one past the listener.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(untagged)]
enum Line {
    Report(Report),
    Ask(Ask),
    /// Last, so it can never shadow the two above: it requires `started_by_hand`, which
    /// neither carries, and a report or an ask never reads as one.
    ByHand(StartedByHand),
    /// After every other kind, for the same reason: it requires `session_saved`, which none of
    /// them carries, so no line an older `charter` writes ever reads as one.
    Saved(SessionSaved),
    /// After every other kind: it requires `commit_refused`, which none of them carries.
    Refused(CommitRefused),
    /// After every other kind: it requires `tool_hook`, which none of them carries.
    Tool(ToolCall),
    /// After every other kind: it requires `permission_request`, which none of them carries.
    Permission(permission::PermissionAsked),
    /// After every other kind: it requires `touching`, which none of them carries.
    Touching(Touching),
    /// After every other kind: it requires `sandbox_blocked`, which none of them carries.
    Blocked(SandboxBlocked),
    /// After every other kind: it requires `doing`, which none of them carries.
    Doing(Doing),
    /// After every other kind: it requires `secret_exec`, which none of them carries. Held on
    /// its connection for as long as the command runs (#1407).
    SecretExec(crate::secrets::brokered::Ask),
}

impl Line {
    /// The chat this line says it comes from, which its token has to be the token of.
    fn chat(&self) -> u32 {
        match self {
            Self::Report(report) => report.chat,
            Self::Ask(Ask::Ticket { chat }) => *chat,
            Self::Ask(Ask::Open(open)) => open.chat,
            Self::Ask(Ask::Report(back)) => back.chat,
            Self::Ask(Ask::SessionRecord(record)) => record.chat,
            Self::Ask(Ask::Write(write)) => write.chat,
            Self::Ask(Ask::Git(git)) => git.chat,
            Self::Ask(Ask::Commit(commit)) => commit.chat,
            Self::Ask(Ask::Vaults { chat }) => *chat,
            Self::Ask(Ask::WhereWorking(asks)) => asks.chat,
            Self::Ask(Ask::Dispatch(dispatch)) => dispatch.chat,
            Self::Ask(Ask::Task(asked)) => asked.chat,
            Self::ByHand(notice) => notice.chat,
            Self::Saved(saved) => saved.chat,
            Self::Refused(refused) => refused.chat,
            Self::Tool(call) => call.chat,
            Self::Permission(asked) => asked.chat,
            Self::Touching(touching) => touching.chat,
            Self::Blocked(blocked) => blocked.chat,
            Self::Doing(doing) => doing.chat,
            Self::SecretExec(ask) => ask.chat,
        }
    }
}

/// A harness the operator started by hand, in a shell tab's own shell (SI-5).
///
/// **Neither a report nor an ask.** It moves no chat — nothing about a chat's state is
/// learned from it — and it asks the app to do nothing: the window draws a banner on the tab
/// it came from, and whatever happens next is the operator's click. What sends it is `charter
/// shell-guard`, which the shims on a shell tab's `PATH` run in front of the real harness, so
/// the whole of the detection is which command was started. Nothing reads what the harness
/// then prints.
///
/// Anything that can write the socket can send one, which is the same account that can already
/// move a chat's state; the most it buys is a banner the operator can dismiss.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StartedByHand {
    /// The app's number for the shell tab's chat, from [`CHAT_ENV`].
    pub chat: u32,
    /// The harness, by the word the plane calls it (`claude`, `codex`, `opencode`).
    pub started_by_hand: String,
    /// Where the shell was standing when it started it — where a chat opened in its place
    /// would start.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<std::path::PathBuf>,
}

/// What hears a [`StartedByHand`].
pub type Noticed = Box<dyn Fn(StartedByHand) + Send + Sync + 'static>;

/// A chat wrote its session record: the end of a Smart close (SI-8, ADR 0064).
///
/// **Neither a report nor an ask, and it moves no chat.** `charter session record` sends it
/// once the record, the index and the workspace's pointer are on disk, so the app hears that
/// the one thing a Smart close waits for is done — from the command that did it, never from
/// reading what the harness printed. What the app does with it is the app's: close the tab it
/// was closing, or nothing, for a chat it was not closing. A line from a chat charter did not
/// ask to close therefore closes nothing.
///
/// Anything that can write the socket can send one, which is the account that can already
/// move a chat's state; the most it buys is the close of a tab the operator already asked to
/// close.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SessionSaved {
    /// The app's number for the chat that wrote it, from [`CHAT_ENV`].
    pub chat: u32,
    /// The record, as a path the app can open: absolute.
    pub session_saved: std::path::PathBuf,
}

/// What hears a [`SessionSaved`].
pub type Saved = Box<dyn Fn(SessionSaved) + Send + Sync + 'static>;

/// An agent's commit was refused before it was made: charter's `pre-commit` or
/// `pre-merge-commit` found a secret or personal data in what it adds (SQ-16, ADR 0074,
/// [`crate::diffscan`]).
///
/// **Neither a report nor an ask.** It does not move the chat's state — the turn goes on, and
/// the agent has git's refusal in front of it — but it is a needs-you item of its own: the
/// operator hears that a commit was stopped, and what for, masked.
///
/// **The guard is the chat's token**, as for every line: the listener hands this on only when
/// the token beside it is the one [`ChatTokens`] issued for the chat `Line::chat()` names — here
/// `chat`. So a line naming a chat is believed only from something holding that chat's token,
/// which is what the chat's own process tree holds (ADR 0068 §5, the `chat` scope). What that
/// buys a process in the chat is an item in its own chat's queue, which the operator can
/// ignore.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CommitRefused {
    /// The app's number for the chat whose commit it was, from [`CHAT_ENV`].
    pub chat: u32,
    /// What the queue item says: the repository and the first finding, masked. Never a value.
    pub commit_refused: String,
}

/// What hears a [`CommitRefused`].
///
/// It answers once the line is recorded durably, or with the error that kept it from being:
/// only an `Ok` is told to the hook as taken, so a line the host could not record is spooled
/// (FD-30).
pub type Refused = Box<dyn Fn(CommitRefused) -> std::io::Result<()> + Send + Sync + 'static>;

/// A tool hook ran: what the host's event log records of it (FD-9, #649).
///
/// **Neither a report nor an ask, and it moves no chat.** Every tool hook `charter hook`
/// answers sends one, after it has answered, so the host has one event per hook call. It never
/// carries the tool's arguments: `args` is their SHA-256 ([`crate::eventlog::args_hash`]), which
/// the host keys again before anything is written. A line that cannot be sent changes nothing
/// about the answer the harness was given.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ToolCall {
    /// The app's number for the chat the hook ran in, from [`CHAT_ENV`].
    pub chat: u32,
    /// The hook's word: `pretooluse`, `pretooluse-read`, `posttooluse`, ….
    pub tool_hook: String,
    /// The tool, by the harness's name for it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    /// The harness's id for this one call, which pairs its pre hook with its post hook.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub call: Option<String>,
    /// SHA-256 of the arguments, in hex. Never the arguments.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args: Option<String>,
    /// What charter's hook answered.
    #[serde(default)]
    pub decision: Decision,
    /// The guard rule that refused, for a `deny`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule: Option<String>,
    /// How long charter's hook took, in milliseconds.
    #[serde(default)]
    pub hook_ms: u64,
    /// The sub-agent that made the call ([`Report::agent`]'s rule).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    /// When the hook began, in milliseconds since 1970 by the hook's own clock: what pairs a
    /// call's pre and post hooks into a duration whichever the host hears first. 0 is unknown.
    #[serde(default)]
    pub at_ms: u64,
}

/// What a tool hook answered the harness.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Decision {
    /// It allowed the call.
    Allow,
    /// It asked the operator.
    Ask,
    /// It refused the call.
    Deny,
    /// It said nothing either way, so the harness decides as it would have.
    #[default]
    None,
}

impl Decision {
    /// The word an event records.
    pub fn word(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Ask => "ask",
            Self::Deny => "deny",
            Self::None => "none",
        }
    }
}

/// What hears a [`ToolCall`].
///
/// It answers as a [`Refused`] does: `Ok` once the call is recorded durably.
pub type Tooled = Box<dyn Fn(ToolCall) -> std::io::Result<()> + Send + Sync + 'static>;

/// A file tool of a chat touched a path (FM-6, #1109): what the window marks in its tree.
///
/// **Never recorded, anywhere** (D-86a). It is a line of its own so that nothing that records
/// a line ever holds it: it is not a [`ToolCall`], whose line the event log keeps and the spool
/// writes to disk, and it is never spooled ([`touch`] sends it once, or not at all). The host
/// hands it to the window in memory and forgets it. What the event log keeps of the same call is
/// the digest of its arguments (ADR 0066).
///
/// The path is the chat's own word, so the host believes none of it until
/// [`crate::touching::confine`] has put it inside the chat's folder.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Touching {
    /// The app's number for the chat whose tool it was, from [`CHAT_ENV`].
    pub chat: u32,
    /// The path the tool was given, as the harness spelled it.
    pub touching: String,
}

/// What hears a [`Touching`]. Nothing is answered: the hook does not wait for it.
pub type Touched = Box<dyn Fn(Touching) + Send + Sync + 'static>;

/// What a chat's tool hook says the chat is doing (#1493): what the window says in one line
/// under a working chat's name.
///
/// **Never recorded, anywhere**, as a [`Touching`] is not: a line of its own, sent once and
/// never spooled ([`tell_doing`]), which the host keeps in memory until the chat's next one or
/// the end of its turn. It holds a kind from a fixed list and at most one short name
/// ([`crate::doing`]): never a command's arguments, a path, or anything a tool came back with.
///
/// The name is the chat's own word, so the host believes none of it until
/// [`crate::doing::Said::neutral`] has passed it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Doing {
    /// The app's number for the chat whose tool it was, from [`CHAT_ENV`].
    pub chat: u32,
    /// What the hook said.
    pub doing: crate::doing::Said,
    /// The sub-agent whose tool it was ([`Report::agent`]'s rule). The chat's line is of the
    /// chat's own work, so the host says nothing of a helper's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
}

/// What hears a [`Doing`]. Nothing is answered: the hook does not wait for it.
pub type DoingHeard = Box<dyn Fn(Doing) + Send + Sync + 'static>;

/// A chat's sandbox blocked an operation (#1338): what the window shows as a notice on the
/// chat's tab, and what `purlis doctor` counts.
///
/// **Neither a report nor an ask, and it moves no chat.** A tool hook that runs outside the
/// sandbox read the block in the harness's own report of the tool's result and sorted it
/// ([`crate::sandboxblock::detect`]); this carries only that sort: an operation, a kind, and
/// whether it was purlis's own; and, for a block a person could grant (#1342), the host or
/// path it would name ([`Self::target`]). No argument or output is on the line.
///
/// Sent once, and never spooled ([`tell_blocked`]): a block the app did not take is one nobody
/// was shown, and the count is of what the app heard. Anything holding the chat's token can send
/// one, `ours` included, which buys a notice on its own chat's tab and one more in its project's
/// count; the app takes a handful a minute per chat ([`crate::sandboxblock::Throttle`]).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SandboxBlocked {
    /// The app's number for the chat that was blocked, from [`CHAT_ENV`].
    pub chat: u32,
    /// The block, sorted.
    pub sandbox_blocked: crate::sandboxblock::Block,
    /// The harness, by the word the project calls it ([`HARNESS_ENV`]), when the chat says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub harness: Option<String>,
    /// What a grant would name, for the Notice's Allow (#1342,
    /// [`crate::sandboxblock::detect_with_targets`]): a host, or a whole path. Held by the app in
    /// memory for the Notice and checked again before anything is granted; never kept with the
    /// block, counted or reported. A chat can send any, which buys only a Notice asking the
    /// person on its own tab.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
}

/// What hears a brokered `secret exec` ([`crate::secrets::brokered`]): the ask, and its
/// connection, which it writes every frame on and reads the asker's stdin from. It returns once
/// the command has ended; the connection ends with it.
pub type SecretExecuting = Box<
    dyn Fn(
            crate::secrets::brokered::Ask,
            Box<dyn std::io::BufRead + Send>,
            Box<dyn std::io::Write + Send>,
        ) + Send
        + Sync
        + 'static,
>;

/// What hears a [`SandboxBlocked`]. Nothing is answered: the hook does not wait for it.
pub type Blocked = Box<dyn Fn(SandboxBlocked) + Send + Sync + 'static>;

/// What hears a [`Report`]. It answers as a [`Refused`] does.
pub type Reported = Box<dyn Fn(Report) -> std::io::Result<()> + Send + Sync + 'static>;

/// Who hears each kind of line on the hook channel ([`Listener::hear`]): one field per kind, so
/// a kind added later is a field every caller is made to answer.
pub struct Hearing {
    /// Every [`Report`].
    pub each: Reported,
    /// Every [`Ask`], whose [`Answer`] is written back on the connection it came on.
    pub answer: Answerer,
    /// Every [`StartedByHand`].
    pub noticed: Noticed,
    /// Every [`SessionSaved`].
    pub saved: Saved,
    /// Every [`CommitRefused`].
    pub refused: Refused,
    /// Every [`ToolCall`].
    pub tool: Tooled,
    /// Every [`PermissionAsked`]: held on its connection until it is answered ([`permission`]).
    pub permission: Permitting,
    /// Every [`Touching`].
    pub touching: Touched,
    /// Every [`SandboxBlocked`].
    pub blocked: Blocked,
    /// Every [`Doing`].
    pub doing: DoingHeard,
    /// Every brokered `secret exec`: held on its connection while the command runs.
    pub secret_exec: SecretExecuting,
}

/// Sends one report to the socket at `path`. Answers whether the app took it.
///
/// One connection is one report, written and closed. Nothing is waited for: the app has the
/// line, and a hook that waited for an answer would be spending a harness's turn on a
/// spinner.
#[cfg(unix)]
pub fn send(path: &std::path::Path, token: Option<&ChatToken>, report: &Report) -> io::Result<()> {
    use std::io::Write;

    // A path that is not there, and a socket file whose app has gone, both refuse at once
    // — `ENOENT` and `ECONNREFUSED`. Neither can hang, so no timeout is armed for them.
    let mut socket = std::os::unix::net::UnixStream::connect(path)?;
    socket.write_all(&line_with(token, report)?)?;
    socket.flush()
}

/// Tells the app at `path` a harness was started by hand. Answers whether the app took it.
///
/// [`send`]'s shape — one connection, one line, closed — with a deadline on the write as well:
/// what calls it is a harness the operator is waiting to see start, and it is started whether
/// or not this got through.
#[cfg(unix)]
pub fn tell(
    path: &std::path::Path,
    token: Option<&ChatToken>,
    notice: &StartedByHand,
) -> io::Result<()> {
    one_line_with_a_deadline(path, token, notice)
}

/// Tells the app at `path` which file a chat's tool touched (FM-6). Answers whether it was
/// written.
///
/// [`tell`]'s shape and deadline, and **never spooled**: a marker the app did not take is a
/// marker nobody needed, and a spool is a file the path would then be in (D-86a).
#[cfg(unix)]
pub fn touch(
    path: &std::path::Path,
    token: Option<&ChatToken>,
    touching: &Touching,
) -> io::Result<()> {
    one_line_with_a_deadline(path, token, touching)
}

/// Tells the app at `path` what its chat's tool hook says the chat is doing (#1493). Answers
/// whether it was written.
///
/// [`tell`]'s shape and deadline, and **never spooled**, as a touch is not: a line the app did
/// not take is one nobody was shown, and the next tool says the next thing.
#[cfg(unix)]
pub fn tell_doing(
    path: &std::path::Path,
    token: Option<&ChatToken>,
    doing: &Doing,
) -> io::Result<()> {
    one_line_with_a_deadline(path, token, doing)
}

/// Tells the app at `path` its chat's sandbox blocked an operation (#1338). Answers whether it
/// was written.
///
/// [`tell`]'s shape and deadline, and never spooled ([`SandboxBlocked`]).
#[cfg(unix)]
pub fn tell_blocked(
    path: &std::path::Path,
    token: Option<&ChatToken>,
    blocked: &SandboxBlocked,
) -> io::Result<()> {
    one_line_with_a_deadline(path, token, blocked)
}

/// Tells the app at `path` a chat's session record is saved. Answers whether the app took it.
///
/// [`tell`]'s shape and deadline: the record is on disk whether or not the app hears this, and
/// the command that sends it must not hang on an app that stopped reading.
#[cfg(unix)]
pub fn tell_saved(
    path: &std::path::Path,
    token: Option<&ChatToken>,
    saved: &SessionSaved,
) -> io::Result<()> {
    one_line_with_a_deadline(path, token, saved)
}

/// Where a hook's line went ([`deliver_report`], [`deliver_tool`], [`deliver_refused`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delivered {
    /// The host said it took the line, once its hearer had recorded it.
    Taken,
    /// The host did not take it in time, and it is in the chat's spool, durably, under this
    /// number (FD-30).
    Spooled(u64),
}

/// What the host writes back once a hook's line is recorded.
#[cfg(unix)]
#[derive(serde::Serialize, serde::Deserialize)]
struct Taken {
    taken: bool,
    /// Why it was refused, where it was (D-1407-9): the hook says this and spools nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    refused: Option<String>,
}

/// Delivers a chat's report: taken by the host at `path`, or spooled beside it.
#[cfg(unix)]
pub fn deliver_report(
    path: &std::path::Path,
    token: Option<&ChatToken>,
    report: &Report,
) -> io::Result<Delivered> {
    deliver(path, token, report.chat, report)
}

/// Delivers a tool call's line: taken by the host at `path`, or spooled beside it.
#[cfg(unix)]
pub fn deliver_tool(
    path: &std::path::Path,
    token: Option<&ChatToken>,
    call: &ToolCall,
) -> io::Result<Delivered> {
    deliver(path, token, call.chat, call)
}

/// Delivers a refused commit's line: taken by the host at `path`, or spooled beside it.
#[cfg(unix)]
pub fn deliver_refused(
    path: &std::path::Path,
    token: Option<&ChatToken>,
    refused: &CommitRefused,
) -> io::Result<Delivered> {
    deliver(path, token, refused.chat, refused)
}

/// The line to the host, and an answer from it that it took the line, within
/// [`A_NOTICE_TAKES_AT_MOST`]; otherwise the line in chat `chat`'s spool ([`spool::append`]).
///
/// **When this returns `Ok` the line is recorded or durable** (ADR 0075 §7, FD-30): a hook
/// answers its harness after this, so a host that is down, slow or gone costs the line its
/// moment, never the line. A host that took it after the wait ran out may record it as well
/// as the drain: the line is then recorded twice, which is the side this errs on.
///
/// Without a token nothing can be spooled, since a spool line is checked under the token's key,
/// and the error says the line is lost.
#[cfg(unix)]
fn deliver(
    path: &std::path::Path,
    token: Option<&ChatToken>,
    chat: u32,
    line: &impl serde::Serialize,
) -> io::Result<Delivered> {
    use std::io::{BufRead, Read, Write};

    let bytes = line_with(token, line)?;
    let to = path.to_path_buf();
    let taken = within(A_NOTICE_TAKES_AT_MOST, move || {
        let socket = std::os::unix::net::UnixStream::connect(&to)?;
        socket.set_write_timeout(Some(A_NOTICE_TAKES_AT_MOST))?;
        socket.set_read_timeout(Some(A_NOTICE_TAKES_AT_MOST))?;
        (&socket).write_all(&bytes)?;
        let mut said = String::new();
        std::io::BufReader::new(&socket)
            .take(256)
            .read_line(&mut said)?;
        match serde_json::from_str::<Taken>(&said) {
            Ok(Taken { taken: true, .. }) => Ok(Ok(())),
            Ok(Taken {
                refused: Some(why), ..
            }) => Ok(Err(why)),
            _ => Err(io::Error::other("the app did not say it took the line")),
        }
    });
    let why = match taken {
        Ok(Ok(())) => return Ok(Delivered::Taken),
        // Refused, not missed: the app is there and said no, so nothing is spooled to be read
        // as if it had been away.
        Ok(Err(refused)) => return Err(io::Error::new(io::ErrorKind::PermissionDenied, refused)),
        Err(why) => why,
    };
    let Some(token) = token else {
        return Err(io::Error::new(
            why.kind(),
            format!("{why}, and with no chat token it cannot be spooled, so it is lost"),
        ));
    };
    spool::append(&spool::dir_for(path), chat, token, line)
        .map(Delivered::Spooled)
        .map_err(|spooled| not_spooled(&why, spooled))
}

/// What a hook says of a line the host did not take (`why`) and its spool did not keep
/// (`spooled`). A spool the hook stopped waiting for ([`io::ErrorKind::TimedOut`]) may still
/// finish the write, and the next open of the project then records the line, so that one is
/// said to be perhaps lost; every other is lost.
#[cfg(unix)]
fn not_spooled(why: &io::Error, spooled: io::Error) -> io::Error {
    let so = if spooled.kind() == io::ErrorKind::TimedOut {
        "so it may be lost"
    } else {
        "so it is lost"
    };
    io::Error::new(
        spooled.kind(),
        format!("{why}, and it could not be spooled ({spooled}), {so}"),
    )
}

/// `act`, or an error once `deadline` has passed without it finishing.
#[cfg(unix)]
///
/// On a thread of its own, as `charter hook`'s payload read is: a `connect` to a socket whose
/// backlog is full blocks, on Linux, for as long as the app does not accept, and an app that
/// is frozen never does. A guard's verdict must never wait on that, so the act is let go and
/// left to finish or fail on its own; the process is on its way out anyway.
#[cfg(unix)]
fn within<T: Send + 'static>(
    deadline: std::time::Duration,
    act: impl FnOnce() -> io::Result<T> + Send + 'static,
) -> io::Result<T> {
    let (done, finished) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("charter-hook-notice".into())
        .spawn(move || {
            let _ = done.send(act());
        })?;
    finished.recv_timeout(deadline).unwrap_or_else(|_| {
        Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "the app did not take the line in time",
        ))
    })
}

/// One connection, one line, and the app's letting go waited for: at most
/// [`A_NOTICE_TAKES_AT_MOST`] spent writing it, and [`A_NOTICE_IS_SEEN_WITHIN`] in all.
#[cfg(unix)]
fn one_line_with_a_deadline(
    path: &std::path::Path,
    token: Option<&ChatToken>,
    line: &impl serde::Serialize,
) -> io::Result<()> {
    use std::io::Write;

    let bytes = line_with(token, line)?;
    let path = path.to_path_buf();
    // The connect as well as the write is bounded: see [`within`].
    within(A_NOTICE_IS_SEEN_WITHIN, move || {
        let mut socket = std::os::unix::net::UnixStream::connect(&path)?;
        socket.set_write_timeout(Some(A_NOTICE_TAKES_AT_MOST))?;
        socket.write_all(&bytes)?;
        socket.flush()?;
        // And then stays until the app has let the connection go (D-1407-10): it judges a line
        // by the process that sent it, read once the line has arrived, and one that exited the
        // moment it wrote could be gone by then. An app that says nothing closes once the line
        // is handled; one that answers is read to its end. A frozen app costs a notice up to
        // [`A_NOTICE_IS_SEEN_WITHIN`] where it once cost [`A_NOTICE_TAKES_AT_MOST`].
        socket.shutdown(std::net::Shutdown::Write)?;
        socket.set_read_timeout(Some(A_NOTICE_IS_SEEN_WITHIN))?;
        let mut rest = Vec::new();
        let _ = std::io::Read::read_to_end(&mut socket, &mut rest);
        Ok(())
    })
}

/// How long [`tell`] and [`tell_saved`] may spend writing their line: a line is under 4 KiB and the app reads it on
/// a thread of its own, so this is only a bound on an app that has stopped reading.
#[cfg(unix)]
const A_NOTICE_TAKES_AT_MOST: std::time::Duration = std::time::Duration::from_millis(250);

/// How long a notice's sender stays, all told, for the app to look at who sent it and let the
/// connection go (D-1407-10). Spent in full only on an app that has stopped reading.
#[cfg(unix)]
const A_NOTICE_IS_SEEN_WITHIN: std::time::Duration = std::time::Duration::from_millis(500);

/// The longest a tool hook can spend telling the app, after it has decided and before it
/// answers its harness, with an app that has stopped reading: its tool call's line
/// ([`deliver_tool`], and the spool it falls back to), then the two lines nothing records
/// ([`touch`], [`tell_doing`]). Whatever decides a tool call adds this to its own budget and
/// holds the sum inside every harness's timeout for the hook (#1493).
#[cfg(unix)]
pub const A_TOOL_HOOK_TELLS_WITHIN: std::time::Duration = std::time::Duration::from_millis(
    (A_NOTICE_TAKES_AT_MOST.as_millis()
        + spool::A_LINE_IS_SPOOLED_WITHIN.as_millis()
        + 2 * A_NOTICE_IS_SEEN_WITHIN.as_millis()) as u64,
);

/// One conversation with the app: lines written, and each answered on the same connection.
///
/// **The same connection is the point.** A ticket is bound to the connection it was minted
/// on ([`OpenChat`]), so the mint and the spend have to travel together, and a line copied
/// onto a connection of its own spends nothing.
#[cfg(unix)]
pub struct Asking {
    stream: std::io::BufReader<std::os::unix::net::UnixStream>,
    token: Option<ChatToken>,
}

#[cfg(unix)]
impl Asking {
    /// Connects to the app listening at `path`.
    ///
    /// A path that is not there, and a socket whose app has gone, both refuse at once —
    /// `ENOENT` and `ECONNREFUSED` — so connecting cannot hang. What can is an app that
    /// takes the line and never answers, which is what [`Asking::ask`]'s deadline is for.
    ///
    /// `token` is the asking chat's own, carried on every line this writes.
    pub fn on(path: &std::path::Path, token: Option<ChatToken>) -> io::Result<Self> {
        let stream = std::os::unix::net::UnixStream::connect(path)?;
        Ok(Self {
            stream: std::io::BufReader::new(stream),
            token,
        })
    }

    /// Writes `ask` and reads the app's answer, waiting at most `within` for each half.
    ///
    /// Every way this fails is an `Err` and never a wait: a handoff whose app did not answer
    /// prints the command to run in a terminal, which is a handoff the operator can still
    /// carry out, where a hang is a Bash tool call that never ends.
    pub fn ask(&mut self, ask: &Ask, within: std::time::Duration) -> io::Result<Answer> {
        use std::io::{BufRead, Read, Write};

        let line = line_with(self.token.as_ref(), ask)?;
        let socket = self.stream.get_mut();
        socket.set_write_timeout(Some(within))?;
        socket.set_read_timeout(Some(within))?;
        socket.write_all(&line)?;
        socket.flush()?;
        let mut said = String::new();
        // The same cap the app holds a report to, turned round: an answer is a ticket or a
        // sentence, and something that writes for ever without a newline is not the app.
        (&mut self.stream)
            .take(A_REPORT_IS_AT_MOST)
            .read_line(&mut said)?;
        if said.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "the app closed the connection without answering",
            ));
        }
        serde_json::from_str(&said).map_err(io::Error::other)
    }
}

/// Where there is no unix socket, [`Asking`], [`send`], [`Listener`] and [`Reading`] are the
/// refusals in `off_unix`, kept in a file of their own because no unix build compiles them.
#[cfg(not(unix))]
mod off_unix;

#[cfg(unix)]
pub mod spool;

pub mod permission;
#[cfg(not(unix))]
pub use off_unix::{
    Asking, Listener, Reading, deliver_refused, deliver_report, send, tell, tell_saved,
};
pub use permission::{PermissionAsked, Permitting, ask_permission};

/// What the app answers an ask with, told which connection it came on.
///
/// The connection is a number the listener deals, one per connection and never twice. It is
/// how a ticket is bound to the connection that minted it without this module knowing what a
/// ticket is.
pub type Answerer = Box<dyn Fn(u64, Ask) -> Answer + Send + Sync + 'static>;

/// The socket the app listens on for hook reports.
#[cfg(unix)]
pub struct Listener {
    socket: std::os::unix::net::UnixListener,
    path: std::path::PathBuf,
    tokens: std::sync::Arc<ChatTokens>,
    /// The only uid a connection is read from: this process's own, as it binds (FD-6).
    owner: purlis_same_user::Uid,
}

#[cfg(unix)]
impl Listener {
    /// Binds a fresh socket at `socket`, replacing one left behind by a process that is gone.
    ///
    /// `within` is where containment begins: every component from it down to the socket must
    /// not be a link (`contain::no_link_on_the_way`, shared with the record `reopen` keeps in
    /// the same directory). It is named by the caller and never worked out here — a walk from
    /// the root of the filesystem would refuse every path on macOS, where `/tmp` and `/var`
    /// are themselves links.
    ///
    /// **This is the one caller of that walk that cannot close the window after it** (charter
    /// ADR 0028). `reopen` opens the record through `contain::open_no_link`, which puts
    /// `O_NOFOLLOW` on the open so the kernel answers the last component at the instant it is
    /// opened; `bind` takes a path and there is no portable `bindat`, so the walk here, the
    /// `remove_file` of a stale socket below, the `bind` itself and the `set_permissions`
    /// after it are four path calls with three windows between them. The socket carries
    /// nothing secret and executes nothing it is handed, which is why that is accepted and
    /// written down rather than worked around; the fix is the same `openat`-beneath-a-
    /// descriptor rewrite ADR 0028 puts at M3.
    pub fn bind(within: &std::path::Path, socket: &std::path::Path) -> io::Result<Self> {
        // The same walk `reopen` uses for the record in this very directory
        // (charter-app#28), and the same one, not a second copy of it: two containment gates
        // drift, which is the failure this repo has found five times.
        crate::contain::no_link_on_the_way(within, socket)?;
        let path = socket;
        if let Some(parent) = path.parent() {
            private_directory(parent)?;
        }
        // `bind` refuses an address already in use, and a socket file outlives the process
        // that made it — so an app that was killed would stop the next one from listening at
        // all. Removing it first is the standard answer, and the single-instance plugin is
        // what makes it safe: there is no second live app whose socket this could be. On
        // Linux the app also holds a per-user lock (`instance.rs` in charter-app), because a
        // launch without a session bus has no single-instance name to hold.
        match std::fs::remove_file(path) {
            Ok(()) => {}
            Err(err) if err.kind() == io::ErrorKind::NotFound => {}
            Err(err) => return Err(err),
        }
        let socket = std::os::unix::net::UnixListener::bind(path)?;
        // Whoever can write here can move a chat's state and raise a notification. Nothing
        // secret travels over it and nothing it carries is executed, so this is not a
        // secret's lock — it is the difference between "the operator" and "anything running
        // on the machine", and it is one call.
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        // Each token's spool key is recorded beside the socket as it is issued (FD-30), so
        // whichever host drains a chat's spool can check its lines: only where the sandbox's
        // integrity denial covers the directory (V63). Anywhere else nothing spools.
        let spool = spool::dir_for(path);
        let tokens = if spool::covered(&spool) {
            private_directory(&spool)?;
            ChatTokens::spooling_into(spool)
        } else {
            tracing::warn!(
                "purlis: the hook channel at {} is outside the directory a sandboxed chat is \
                 denied, so its hooks spool nothing and a line it does not take is lost",
                path.display()
            );
            ChatTokens::default()
        };
        Ok(Self {
            socket,
            path: path.to_path_buf(),
            tokens: std::sync::Arc::new(tokens),
            owner: purlis_same_user::Uid::effective(),
        })
    }

    /// The same listener, reading only connections from `uid`. For the tests, which have no
    /// second user to connect as: told it belongs to another uid, the test's own connection
    /// is the other user's.
    #[cfg(test)]
    fn owned_by(mut self, uid: purlis_same_user::Uid) -> Self {
        self.owner = uid;
        self
    }

    /// The path a hook writes to.
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    /// The chats' tokens this listener checks every line against. It starts with none, so
    /// it reads no line until the app issues a chat its token ([`ChatTokens::issue`]).
    pub fn tokens(&self) -> std::sync::Arc<ChatTokens> {
        std::sync::Arc::clone(&self.tokens)
    }

    /// Hands every report to `each`, on a thread of its own, until the listener is dropped.
    ///
    /// One connection is one report. A caller that cannot keep up does not block a harness:
    /// the hook has already written its line and gone.
    pub fn each(self, each: Box<dyn Fn(Report) + Send + Sync + 'static>) -> Reading {
        self.each_answering(
            each,
            Box::new(|_, _| Answer::No {
                why: NOTHING_ANSWERS.to_owned(),
            }),
        )
    }

    /// [`Listener::each`], and every [`Ask`] handed to `answer`, whose [`Answer`] is written
    /// back on the connection the ask came on.
    pub fn each_answering(
        self,
        each: Box<dyn Fn(Report) + Send + Sync + 'static>,
        answer: Answerer,
    ) -> Reading {
        self.each_answering_and_noticing(each, answer, Box::new(|_| {}))
    }

    /// [`Listener::each_answering`], and every [`StartedByHand`] handed to `noticed`.
    ///
    /// A [`SessionSaved`] is heard and dropped: an app that does not close a chat on one has
    /// nothing to do with it.
    pub fn each_answering_and_noticing(
        self,
        each: Box<dyn Fn(Report) + Send + Sync + 'static>,
        answer: Answerer,
        noticed: Noticed,
    ) -> Reading {
        self.each_answering_noticing_and_saving(each, answer, noticed, Box::new(|_| {}))
    }

    /// [`Listener::each_answering_and_noticing`], and every [`SessionSaved`] handed to `saved`.
    ///
    /// A [`CommitRefused`] is heard and dropped.
    pub fn each_answering_noticing_and_saving(
        self,
        each: Box<dyn Fn(Report) + Send + Sync + 'static>,
        answer: Answerer,
        noticed: Noticed,
        saved: Saved,
    ) -> Reading {
        self.hear(Hearing {
            each: Box::new(move |report| {
                each(report);
                Ok(())
            }),
            answer,
            noticed,
            saved,
            refused: Box::new(|_| Ok(())),
            tool: Box::new(|_| Ok(())),
            permission: Box::new(|_| None),
            blocked: Box::new(|_| {}),
            doing: Box::new(|_| {}),
            touching: Box::new(|_| {}),
            secret_exec: Box::new(|_, _, writer| {
                crate::secrets::brokered::not_answered(writer);
            }),
        })
    }

    /// Hands every line to whoever `hearing` names for its kind, on a thread of its own, until
    /// the listener is dropped. The other `each…` methods are this with the kinds they leave
    /// out dropped.
    pub fn hear(self, hearing: Hearing) -> Reading {
        let stopping = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let path = self.path.clone();
        let stopped = std::sync::Arc::clone(&stopping);
        let hearing = std::sync::Arc::new(hearing);
        let tokens = std::sync::Arc::clone(&self.tokens);
        let turns = std::sync::Arc::new(InTurn::new());
        let reading = std::thread::spawn(move || {
            let mut dealt: u64 = 0;
            for connection in self.socket.incoming() {
                if stopped.load(std::sync::atomic::Ordering::SeqCst) {
                    return;
                }
                // One bad connection is one lost report, never the end of the channel: an
                // app that stopped listening because something wrote nonsense would leave
                // every chat frozen at the state it last had, and never say so. The two
                // reachable ways in — a line that will not read, and one that will not parse
                // — have a test; `accept` failing does not, because nothing a test can do
                // makes it fail. It is written the same way for the same reason.
                let Ok(connection) = connection else { continue };
                // **Only this user's connections are read** (FD-6, ADR 0068 §5). The socket is
                // `0600` in a `0700` directory, which keeps every other user out already; this
                // still holds if either is ever wrong. A peer of another uid, or one the socket
                // will not name, is closed unread, before the chat's token is even looked at.
                // Its pid too, which a line is checked against (D-1407-6).
                let peer = purlis_same_user::peer_process_of(&connection);
                let pid = peer.as_ref().ok().map(|(_, pid)| *pid);
                if let Err(refused) =
                    purlis_same_user::admit_peer(peer.map(|(uid, _)| uid), self.owner)
                {
                    if let Some(also) = PEER_REFUSALS.say() {
                        tracing::warn!(
                            "purlis: the hook channel refused a connection: {refused}{also}"
                        );
                    }
                    continue;
                }
                // **A thread per connection, and this used to be one thread for all of
                // them.** An independent review reproduced the consequence: anything that
                // connects and does not write — `nc -U` on the socket, a hook stopped in a
                // debugger, a child that inherited the descriptor — held the single reader
                // inside `read_line` forever. Every other session's report queued behind it,
                // the sidebar went quiet with nothing to say why, and quitting deadlocked in
                // this thread's `join`.
                //
                // A report is a few dozen bytes and the thread lives for as long as one
                // takes to arrive, so this is not fifty threads; it is however many hooks
                // are mid-write, which is almost always none.
                let hearing = std::sync::Arc::clone(&hearing);
                let tokens = std::sync::Arc::clone(&tokens);
                let serving = std::sync::Arc::clone(&turns);
                dealt += 1;
                let this = dealt;
                let started = std::thread::Builder::new()
                    .name("charter-hook-report".into())
                    .spawn(move || {
                        serve(connection, pid, this, &tokens, &hearing, &serving);
                    });
                // A thread that will not start costs this one report. Refusing the rest of
                // the channel over it would cost every report after it too, and so would a
                // turn left waiting for a connection nobody serves.
                if started.is_err() {
                    turns.done(this);
                }
            }
        });
        Reading {
            stopping,
            path,
            reading: Some(reading),
        }
    }
}

/// Makes `directory`, and makes sure nobody else on the machine may enter it.
///
/// **The directory, not only the socket, and the mode is set as it is CREATED.** Between a
/// `bind` and a `chmod` there is a window where the socket sits at whatever the umask
/// allowed, and the socket's own path is predictable — on Linux the fallback lives in `/tmp`,
/// which everyone can write to. A local user who got there first would own the channel: they
/// could read every report and inject their own, moving chats and raising notifications.
///
/// A review found the earlier version, which created the directory at the umask and then
/// DISCARDED the result of tightening it. Every step here is checked, and a path that is
/// already something else — a symlink, a file, a directory somebody else owns — is refused
/// rather than used.
///
/// The checks are `purlis_same_user::private_directory`'s, the same ones the credentials of
/// `charterd`'s client scopes sit behind (FD-6), so the two cannot drift: the directory is
/// checked and tightened through one descriptor opened without following a link, and one
/// another uid owns is refused before anything about it is changed.
#[cfg(unix)]
fn private_directory(directory: &std::path::Path) -> io::Result<()> {
    purlis_same_user::private_directory(directory)
}

/// The hook channel's refusals of a peer that is not this user, said in the log at most once
/// per [`A_REFUSAL_IS_SAID_EVERY`].
///
/// A refusal is something to know about, and a client that connects in a loop must not be
/// able to fill the log with them, so the ones in between are counted and the next line that is
/// said gives the count. **One limiter per kind of refusal**, so one kind cannot spend the
/// interval and hide the other: a throwaway line without a token does not keep a peer of
/// another uid out of the log, nor the reverse.
#[cfg(unix)]
static PEER_REFUSALS: RateLimited = RateLimited::new();

/// The hook channel's refusals of a line that did not carry its chat's token, limited apart
/// from [`PEER_REFUSALS`].
#[cfg(unix)]
static TOKEN_REFUSALS: RateLimited = RateLimited::new();

/// How often a refusal on the hook channel is said in the log at most.
#[cfg(unix)]
const A_REFUSAL_IS_SAID_EVERY: std::time::Duration = std::time::Duration::from_secs(10);

#[cfg(unix)]
struct RateLimited {
    state: std::sync::Mutex<(Option<std::time::Instant>, u64)>,
}

#[cfg(unix)]
impl RateLimited {
    const fn new() -> Self {
        RateLimited {
            state: std::sync::Mutex::new((None, 0)),
        }
    }

    /// `Some` with what to add to the line when this one is to be said (how many were not
    /// since the last), and `None` when it is one of the ones counted instead.
    fn say(&self) -> Option<String> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let now = std::time::Instant::now();
        let (last, held) = &mut *state;
        if last.is_some_and(|last| now.duration_since(last) < A_REFUSAL_IS_SAID_EVERY) {
            *held += 1;
            return None;
        }
        *last = Some(now);
        let also = match std::mem::take(held) {
            0 => String::new(),
            n => format!(" ({n} more refused since the last line)"),
        };
        Some(also)
    }
}

/// How long one connection has to say its piece.
///
/// A hook writes its line and closes at once; this is only a bound on something that does
/// not. Short, because the thread holding it is doing nothing else, and generous next to the
/// 1.8 ms the whole hook call was measured at.
///
/// `cfg(unix)` because its one reader is: off unix there is no connection to bound.
#[cfg(unix)]
const A_REPORT_TAKES_AT_MOST: std::time::Duration = std::time::Duration::from_secs(2);

/// The most one report may be.
///
/// A report is a chat number, an event word, a uuid and a pid — well under 200 bytes. The cap
/// is what stops a client that writes without ever sending a newline from growing a `String`
/// in the app's memory until there is none left: the deadline above bounds how LONG one may
/// write, and two seconds of writing is gigabytes.
///
/// `cfg(unix)` for the same reason as the deadline above it.
#[cfg(unix)]
const A_REPORT_IS_AT_MOST: u64 = 64 * 1024;

/// The most lines one connection may carry.
///
/// A hook's is one report; a handoff's is a mint and a spend. Anything past that is not
/// charter, and bounding it is what keeps one connection from holding a thread in a loop.
#[cfg(unix)]
const A_CONNECTION_SAYS_AT_MOST: usize = 4;

/// The most one line may be once asks share the socket with reports.
///
/// [`A_REPORT_IS_AT_MOST`] was sized for a report, and an open carries a whole first message:
/// up to [`crate::handoff::FIRST_MESSAGE_MAX_BYTES`] bytes, which JSON can grow sixfold where
/// the brief holds control characters (`\u001b`). This is that, with room, and it is still
/// a bound: the reason the cap exists is a client that never sends a newline.
#[cfg(unix)]
const A_LINE_IS_AT_MOST: u64 = 6 * crate::handoff::FIRST_MESSAGE_MAX_BYTES as u64 + 4096;

/// How long a brokered `secret exec`'s frame may take to write before its asker counts as gone.
#[cfg(unix)]
const A_FRAME_WRITE_TAKES_AT_MOST: std::time::Duration = std::time::Duration::from_secs(60);

/// What an app with no answerer says to an ask, so an asker never waits on a silence.
pub const NOTHING_ANSWERS: &str = "this app does not open chats on request";

/// Reads one connection to its end: each report handed to `each`, each ask answered on it.
///
/// **A report still costs exactly what it did.** A hook writes its one line and closes, so
/// the read after it sees the end at once; nothing here waits on a hook for a second line.
#[cfg(unix)]
fn serve(
    connection: std::os::unix::net::UnixStream,
    peer: Option<u32>,
    this: u64,
    tokens: &ChatTokens,
    hearing: &Hearing,
    turns: &InTurn,
) {
    let mut turn = Turn {
        turns,
        this,
        done: false,
    };
    use std::io::{BufRead, Read, Write};

    // Both directions: a client that connects and neither writes nor closes must not hold
    // this thread past the deadline.
    let _ = connection.set_read_timeout(Some(A_REPORT_TAKES_AT_MOST));
    let _ = connection.set_write_timeout(Some(A_REPORT_TAKES_AT_MOST));
    let Ok(mut writer) = connection.try_clone() else {
        return;
    };
    let deadline = std::sync::Arc::new(std::sync::Mutex::new(None));
    let mut reader = std::io::BufReader::new(Deadlined {
        socket: connection,
        until: std::sync::Arc::clone(&deadline),
    });
    for _ in 0..A_CONNECTION_SAYS_AT_MOST {
        let mut line = String::new();
        // One deadline for the whole line, not one per read (D-1407-6): a sender dripping a
        // byte at a time would otherwise hold the line open while the pid it connected from is
        // given to another process.
        *deadline
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) =
            Some(std::time::Instant::now() + A_REPORT_TAKES_AT_MOST);
        // The cap is per line: `take` on the reader would make it per connection, and a
        // brief is most of what an open carries.
        match (&mut reader).take(A_LINE_IS_AT_MOST).read_line(&mut line) {
            Ok(0) | Err(_) => return,
            Ok(_) => {}
        }
        *deadline
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
        // A line the app will not read ends this connection, never the channel, and is
        // answered with the reason first (#1333): an asker can then tell an app that refused
        // from one that never took the ask. A hook reads anything but "taken" as not taken,
        // and spools the line, as it did when nothing was written back.
        let refuse = |writer: &mut std::os::unix::net::UnixStream, why: String| {
            if let Ok(mut said) = serde_json::to_vec(&Answer::No { why }) {
                said.push(b'\n');
                let _ = writer.write_all(&said);
            }
        };
        if !line.ends_with('\n') && line.len() as u64 >= A_LINE_IS_AT_MOST {
            refuse(
                &mut writer,
                format!("the line was longer than the app reads ({A_LINE_IS_AT_MOST} bytes)"),
            );
            return;
        }
        let Some((line, token)) = read_line(&line) else {
            refuse(
                &mut writer,
                "the app could not read this line: it is from a purlis this app does not know"
                    .to_owned(),
            );
            return;
        };
        // Every line is checked against the token of the chat it names, and one that does not
        // carry it ends the connection unread, said so and never acted on. The chat's number
        // is said in the app's log (#647); the token is not.
        let chat = line.chat();
        // The peer as the socket names it now, and as it named it at the accept: both must be
        // inside the chat, so a line finished by a process the first one handed the connection
        // to is judged by that process too.
        let now = purlis_same_user::peer_process_of(&writer)
            .ok()
            .map(|(_, pid)| pid);
        // The process that connected and the one the socket names now (a connection handed on
        // mid-line names its new holder on macOS): each must run inside the chat, read from
        // the kernel only once the line's token has checked (D-1407-10).
        let senders: Vec<u32> = match (peer, now) {
            (Some(peer), Some(now)) if peer != now => vec![peer, now],
            (Some(pid), _) | (None, Some(pid)) => vec![pid],
            (None, None) => Vec::new(),
        };
        let admission = tokens.admission_by(
            chat,
            token.as_deref(),
            (!senders.is_empty()).then_some(senders),
            &|senders, program| {
                Seen::all(senders).is_some_and(|seen| seen.iter().all(|one| one.inside(program)))
            },
        );
        match admission {
            Admission::Admitted => {}
            Admission::NoToken => {
                if let Some(also) = TOKEN_REFUSALS.say() {
                    tracing::warn!(
                        "purlis: a line on the hook channel for chat {chat} did not carry that \
                         chat's token, so it was dropped{also}"
                    );
                }
                refuse(
                    &mut writer,
                    format!("this line does not carry chat {chat}'s token, so nothing was done"),
                );
                return;
            }
            // The chat's own token from outside it, or for a chat whose program was never
            // confirmed: refused, and told why (D-1407-9). A hook spools nothing it is told this.
            refused @ (Admission::Outside | Admission::Unbound) => {
                tracing::warn!(
                    "purlis: a line for chat {chat} carried its token and was refused ({refused:?})"
                );
                if let Some(why) = refused.refusal() {
                    refuse_with(&line, why, &mut writer);
                }
                return;
            }
        }
        // The first line waits for the connections that arrived before this one (FD-9).
        if !turn.done {
            turns.wait(this);
        }
        let first = !turn.done;
        // A report, a tool call and a refused commit are answered once their hearer has
        // returned, which in the app is once the line is recorded (FD-30): the hook answers its
        // harness only after this, or after it has spooled the line.
        let mut recorded = None;
        match line {
            Line::Report(report) => recorded = Some((hearing.each)(report)),
            Line::Ask(ask) => {
                // An ask is answered for as long as opening a chat takes, and nothing about
                // the order of hook calls hangs on it: the turn is let go before it is.
                turn.finish();
                // Known by its number while it is answered, so an answer that waits can ask
                // whether its asker is still there.
                if let Ok(asker) = writer.try_clone() {
                    tokens.answering(this, asker);
                }
                let answer = (hearing.answer)(this, ask);
                tokens.answered(this);
                let Ok(mut said) = serde_json::to_vec(&answer) else {
                    return;
                };
                said.push(b'\n');
                if writer.write_all(&said).is_err() {
                    return;
                }
            }
            Line::ByHand(notice) => (hearing.noticed)(notice),
            Line::Saved(record) => (hearing.saved)(record),
            Line::Refused(refused) => recorded = Some((hearing.refused)(refused)),
            Line::Tool(call) => recorded = Some((hearing.tool)(call)),
            Line::Permission(asked) => {
                // Held for as long as the operator takes, so the turn is let go first, as an
                // ask's is, and the connection ends with its one reply.
                turn.finish();
                permission::hold(&mut reader, &mut writer, (hearing.permission)(asked));
                return;
            }
            Line::Touching(touching) => (hearing.touching)(touching),
            Line::Blocked(blocked) => (hearing.blocked)(blocked),
            Line::Doing(doing) => (hearing.doing)(doing),
            Line::SecretExec(ask) => {
                // Held for as long as the command runs, so the turn is let go first, as an
                // ask's is. The asker's stdin may be quiet for as long as the child is, so no
                // read deadline; a write that cannot land in a minute is an asker gone.
                turn.finish();
                let _ = writer.set_read_timeout(None);
                let _ = writer.set_write_timeout(Some(A_FRAME_WRITE_TAKES_AT_MOST));
                (hearing.secret_exec)(ask, Box::new(reader), Box::new(writer));
                return;
            }
        }
        // Told as taken only once it is recorded durably: a line the hearer could not record
        // gets no answer, so its hook spools it (FD-30). An older hook has closed its end
        // already, and what it was not waiting for is lost with nothing to say.
        if let Some(recorded) = recorded {
            if let Err(why) = recorded {
                tracing::warn!(
                    "purlis: a line on the hook channel for chat {chat} was not recorded \
                     ({why}), so its hook was not told it was taken"
                );
            } else if let Ok(mut said) = serde_json::to_vec(&Taken {
                taken: true,
                refused: None,
            }) {
                said.push(b'\n');
                let _ = writer.write_all(&said);
            }
        }
        if first {
            turn.finish();
        }
    }
}

/// A connection read with one deadline for a whole line: each read is given what is left of it,
/// and none once it has passed. With no deadline set, a read waits as the socket's own timeout
/// says.
#[cfg(unix)]
pub(crate) struct Deadlined {
    socket: std::os::unix::net::UnixStream,
    until: std::sync::Arc<std::sync::Mutex<Option<std::time::Instant>>>,
}

#[cfg(unix)]
impl Deadlined {
    /// The connection itself.
    pub(crate) fn socket(&self) -> &std::os::unix::net::UnixStream {
        &self.socket
    }
}

#[cfg(unix)]
impl std::io::Read for Deadlined {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let until = *self
            .until
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(until) = until {
            let left = until.saturating_duration_since(std::time::Instant::now());
            if left.is_zero() {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "the line took longer than a line may",
                ));
            }
            self.socket.set_read_timeout(Some(left))?;
        }
        self.socket.read(buf)
    }
}

/// The answer `why` to a line that carried its chat's token and was refused (D-1407-9): from a
/// process outside the chat, or for a chat whose program was never confirmed. In the shape its sender waits for: a report, a tool call or a refused commit is
/// told it was refused (so its hook prints why and spools nothing), an ask is answered no, a
/// permission ask is given no option, and a brokered `secret exec` is refused. A line nothing
/// waits on gets nothing.
#[cfg(unix)]
fn refuse_with(line: &Line, why: &str, writer: &mut std::os::unix::net::UnixStream) {
    use std::io::Write;
    let said = match line {
        Line::Report(_) | Line::Tool(_) | Line::Refused(_) => serde_json::to_vec(&Taken {
            taken: false,
            refused: Some(why.to_owned()),
        }),
        Line::Ask(_) => serde_json::to_vec(&Answer::No {
            why: why.to_owned(),
        }),
        Line::Permission(_) => serde_json::to_vec(&permission::Chosen { chosen: None }),
        Line::SecretExec(_) => serde_json::to_vec(&crate::secrets::brokered::Frame::Refused {
            why: why.to_owned(),
            code: 1,
        }),
        Line::ByHand(_)
        | Line::Saved(_)
        | Line::Touching(_)
        | Line::Blocked(_)
        | Line::Doing(_) => return,
    };
    if let Ok(mut said) = said {
        said.push(b'\n');
        let _ = writer.write_all(&said);
    }
}

/// The order connections arrived in, which their first lines are handed on in (FD-9).
///
/// **A thread per connection reads lines in whatever order the threads are scheduled**, and
/// the host's event log wants the order the hooks ran in: a tool call's post hook connects only
/// after its pre hook has exited, so it should be recorded after it. Each connection is dealt
/// a number as it is accepted, and hands its first line to its hearer only once every earlier
/// connection's hearer has returned, which in the app is once that line is recorded.
///
/// **Best effort: ordered unless a recording takes longer than [`A_TURN_IS_WAITED_AT_MOST`].**
/// A connection that says nothing, or a hearer that is slow, holds the ones after it for that
/// long at most, so the channel is never held hostage by one; past it, lines can be recorded
/// out of order, and the event log's pairing of a tool call's two ends does not depend on the
/// order (`eventlog::Recorder::tool`). An ask lets its turn go before it is answered.
#[cfg(unix)]
struct InTurn {
    /// The lowest number not yet done, and the numbers above it that are.
    state: std::sync::Mutex<(u64, std::collections::BTreeSet<u64>)>,
    turned: std::sync::Condvar,
}

#[cfg(unix)]
impl InTurn {
    fn new() -> Self {
        Self {
            state: std::sync::Mutex::new((1, std::collections::BTreeSet::new())),
            turned: std::sync::Condvar::new(),
        }
    }

    /// Waits until every connection before `this` is done, or the wait has gone on too long.
    fn wait(&self, this: u64) {
        let until = std::time::Instant::now() + A_TURN_IS_WAITED_AT_MOST;
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        while state.0 < this {
            let left = until.saturating_duration_since(std::time::Instant::now());
            if left.is_zero() {
                return;
            }
            state = self
                .turned
                .wait_timeout(state, left)
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .0;
        }
    }

    /// Marks `this` done: its first line is handed on, or it never had one.
    fn done(&self, this: u64) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.1.insert(this);
        loop {
            let next = state.0;
            if !state.1.remove(&next) {
                break;
            }
            state.0 += 1;
        }
        drop(state);
        self.turned.notify_all();
    }
}

/// Marks a connection done however its serving ends.
#[cfg(unix)]
struct Turn<'a> {
    turns: &'a InTurn,
    this: u64,
    done: bool,
}

#[cfg(unix)]
impl Turn<'_> {
    fn finish(&mut self) {
        if !self.done {
            self.done = true;
            self.turns.done(self.this);
        }
    }
}

#[cfg(unix)]
impl Drop for Turn<'_> {
    fn drop(&mut self) {
        self.finish();
    }
}

/// How long a line waits for the lines whose connections arrived before it.
#[cfg(unix)]
const A_TURN_IS_WAITED_AT_MOST: std::time::Duration = std::time::Duration::from_millis(50);

/// A listener being read on its own thread. Dropping it stops the reading.
#[cfg(unix)]
pub struct Reading {
    stopping: std::sync::Arc<std::sync::atomic::AtomicBool>,
    path: std::path::PathBuf,
    reading: Option<std::thread::JoinHandle<()>>,
}

/// How long a dropped [`Reading`] waits for its thread to be woken and go.
#[cfg(unix)]
const A_READING_STOPS_WITHIN: std::time::Duration = std::time::Duration::from_secs(5);

#[cfg(unix)]
impl Drop for Reading {
    fn drop(&mut self) {
        self.stopping
            .store(true, std::sync::atomic::Ordering::SeqCst);
        // `accept` is blocked in the thread and nothing else will wake it, so the flag alone
        // would leave it there for the life of the process. One connection of our own is
        // what it is waiting for.
        let _ = std::os::unix::net::UnixStream::connect(&self.path);
        let Some(reading) = self.reading.take() else {
            return;
        };
        // **The wait is bounded.** The connection above goes to whatever listens at the path
        // now. Where another listener has since been bound there (a second one on the same
        // project in one process; `bind` takes a path that is in use), it reached that one, or
        // nothing at all once that one had gone and taken the file with it, and this thread
        // is still in `accept` with nothing left that can wake it. Joining it then would hold
        // whoever dropped this for the life of the process, with nothing said.
        let began = std::time::Instant::now();
        while !reading.is_finished() && began.elapsed() < A_READING_STOPS_WITHIN {
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        if !reading.is_finished() {
            tracing::warn!(
                "purlis: the hook channel at {} did not stop when it was closed: another \
                 listener was bound at its path. Its thread is left; nothing reaches it",
                self.path.display()
            );
            // And the file is not this listener's to remove any more.
            return;
        }
        let _ = reading.join();
        let _ = std::fs::remove_file(&self.path);
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    // Named here rather than at the top of the module: the module's own code needs neither
    // of these off unix, and an import that is only right on one platform belongs with the
    // code that is only compiled there.
    use std::io::Read;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::Arc;
    use std::sync::mpsc;

    #[test]
    fn an_opened_answer_carries_the_app_s_row_and_one_without_it_still_reads() {
        // #1421: an app that leaves the row to the command answers no `row` at all.
        let older: Answer = serde_json::from_str(r#"{"opened":{"chat":9}}"#).expect("parsed");
        assert_eq!(
            older,
            Answer::Opened {
                chat: 9,
                row: None,
                note: None,
            }
        );
        for row in [
            Row::Written,
            Row::Unwritten {
                why: "No space left on device".to_owned(),
            },
        ] {
            let said = Answer::Opened {
                chat: 9,
                row: Some(row),
                // #1445: and what the app has to say about the profile it started on.
                note: Some("runs on the asking chat's profile".to_owned()),
            };
            let line = serde_json::to_string(&said).expect("written");
            assert_eq!(serde_json::from_str::<Answer>(&line).expect("read"), said);
        }
    }

    fn env_of(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> + use<> {
        let pairs: Vec<(String, String)> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        move |want| {
            pairs
                .iter()
                .find(|(k, _)| k == want)
                .map(|(_, v)| v.clone())
        }
    }

    const CLAUDE_STOP: &str = r#"{"session_id":"11111111-2222-4333-8444-555555555555",
        "transcript_path":"/tmp/t.jsonl","cwd":"/tmp","hook_event_name":"Stop",
        "stop_hook_active":false,"last_assistant_message":"pong"}"#;

    /// A Claude Code `UserPromptSubmit` payload whose prompt is `prompt`.
    fn prompt_payload(prompt: &str) -> String {
        serde_json::json!({
            "session_id": "11111111-2222-4333-8444-555555555555",
            "hook_event_name": "UserPromptSubmit",
            "prompt": prompt,
        })
        .to_string()
    }

    #[test]
    fn a_harness_running_with_its_prompts_off_is_one_bit_of_every_report() {
        // #1446: the app keeps it as the chat's mark. Only the mode the harness names for
        // "nobody answers" sets it, on whatever event carries it.
        let env = env_of(&[(SOCKET_ENV, "/tmp/s.sock"), (CHAT_ENV, "7")]);
        let payload = |mode: Option<&str>| {
            let mut said = serde_json::json!({
                "session_id": "11111111-2222-4333-8444-555555555555",
            });
            if let Some(mode) = mode {
                said["permission_mode"] = mode.into();
            }
            said.to_string()
        };
        for event in [Event::SessionStart, Event::Stop, Event::UserPromptSubmit] {
            let off = Report::read(event, &payload(Some("bypassPermissions")), &env).expect("one");
            assert!(off.detail.unattended, "{event:?}");
            let written = serde_json::to_string(&off).expect("json");
            assert!(written.contains(r#""unattended":true"#), "{written}");
        }
        for mode in [None, Some("default"), Some("acceptEdits"), Some("plan")] {
            let asks = Report::read(Event::Stop, &payload(mode), &env).expect("one");
            assert!(!asks.detail.unattended, "{mode:?}");
            // And a report that does not say it writes the line it always wrote.
            assert!(!serde_json::to_string(&asks).unwrap().contains("unattended"));
        }
    }

    #[test]
    fn a_typed_smart_close_is_one_bit_of_the_prompt_and_the_prompt_goes_nowhere() {
        let env = env_of(&[(SOCKET_ENV, "/tmp/s.sock"), (CHAT_ENV, "7")]);
        for typed in [
            "/smart-close",
            "/purlis:smart-close",
            "  /smart-close now please",
        ] {
            let report = Report::read(Event::UserPromptSubmit, &prompt_payload(typed), &env)
                .expect("a report");
            assert!(
                report.detail.smart_close,
                "{typed:?} was not heard as smart close"
            );
            let line = serde_json::to_string(&report).expect("a line");
            assert!(!line.contains("please"), "the prompt left the hook: {line}");
        }
        for typed in [
            "please /smart-close",
            "/smart-closer",
            "/other:smart-close",
            "smart-close",
            "",
        ] {
            let report = Report::read(Event::UserPromptSubmit, &prompt_payload(typed), &env)
                .expect("a report");
            assert!(
                !report.detail.smart_close,
                "{typed:?} was heard as smart close"
            );
        }
        // Only the event that carries a person's prompt says it.
        let stop = Report::read(Event::Stop, &prompt_payload("/smart-close"), &env).expect("one");
        assert!(!stop.detail.smart_close);
        // And a report that is not one says nothing new on the wire.
        let plain =
            Report::read(Event::UserPromptSubmit, &prompt_payload("hi"), &env).expect("a report");
        let line = serde_json::to_string(&plain).expect("a line");
        assert!(!line.contains("smart_close"), "{line}");
    }

    #[test]
    fn a_brokered_write_reads_back_off_the_socket_as_the_write_it_is_from_the_chat_it_names() {
        // #1333: every write a command hands the app arrives whole, and is checked against the
        // token of the chat it names.
        for write in [
            crate::brokered::Write::PersonaRemember {
                text: "fact".to_owned(),
                title: Some("T".to_owned()),
                shared: true,
            },
            crate::brokered::Write::WorkspaceRemember {
                text: "memo".to_owned(),
                title: None,
            },
            crate::brokered::Write::Todo {
                text: "todo".to_owned(),
            },
        ] {
            let ask = Ask::Write(Box::new(WriteAsk { chat: 3, write }));
            let token = ChatToken("t".repeat(64));
            let line = line_with(Some(&token), &ask).expect("a line");
            let (read, carried) = read_line(std::str::from_utf8(&line).unwrap()).expect("a line");
            assert_eq!(read.chat(), 3);
            assert_eq!(carried.as_deref(), Some(token.0.as_str()));
            let Line::Ask(read) = read else {
                panic!("not read as an ask: {read:?}");
            };
            assert_eq!(read, ask);
        }
    }

    #[test]
    fn the_largest_write_a_chat_may_ask_for_fits_on_one_line_of_the_socket() {
        // Every byte a control character, which JSON spells in six: what `Write::check` lets
        // through at its worst is still read whole and answered, never cut (#1333).
        let most = crate::brokered::MOST_TEXT_BYTES;
        let write = crate::brokered::Write::PersonaRemember {
            text: "\u{1}".repeat(most),
            title: None,
            shared: true,
        };
        assert_eq!(write.check(), Ok(()));
        let ask = Ask::Write(Box::new(WriteAsk {
            chat: u32::MAX,
            write,
        }));
        let line = line_with(Some(&ChatToken("t".repeat(64))), &ask).expect("a line");
        assert!(
            (line.len() as u64) < A_LINE_IS_AT_MOST,
            "{} bytes does not fit in {A_LINE_IS_AT_MOST}",
            line.len()
        );
    }

    #[test]
    fn a_written_answer_reads_back_as_written() {
        let answer = Answer::Written {
            to: "alpha".to_owned(),
            path: "workspaces/alpha/todos/x.md".to_owned(),
        };
        let text = serde_json::to_string(&answer).expect("json");
        assert_eq!(
            serde_json::from_str::<Answer>(&text).expect("an answer"),
            answer
        );
    }

    #[test]
    fn the_largest_record_a_chat_may_write_fits_on_one_line_of_the_socket() {
        // Every byte of the body one JSON doubles, and the longest title: what `check` lets
        // through at its worst still reaches the app whole (#1332).
        let ask = Ask::SessionRecord(Box::new(RecordAsk {
            chat: u32::MAX,
            title: "\"".repeat(crate::sessionrecord::MOST_TITLE_CHARS),
            body: "\"".repeat(crate::sessionrecord::MOST_BODY_BYTES),
            pieces: vec!["repo/piece".to_owned(); 8],
            cwd: Some(std::path::PathBuf::from("/a/long/enough/path/to/a/piece")),
        }));
        let token = ChatToken("t".repeat(64));
        let line = line_with(Some(&token), &ask).expect("a line");
        assert!(
            (line.len() as u64) < A_LINE_IS_AT_MOST,
            "{} bytes does not fit in {A_LINE_IS_AT_MOST}",
            line.len()
        );
    }

    #[test]
    fn a_git_ask_is_read_back_whole_and_names_the_chat_its_token_must_be() {
        // #1335: an ask for a clone and one for a worktree, through the reading every line gets.
        for work in [
            GitWork::Clone {
                repos: vec!["widget".to_owned(), "gadget".to_owned()],
            },
            GitWork::WorktreeAdd {
                repo: "widget".to_owned(),
                piece: "p1".to_owned(),
                branch: Some("feature/p1".to_owned()),
            },
        ] {
            let ask = Ask::Git(Box::new(GitAsk {
                chat: 7,
                workspace: "alpha".to_owned(),
                work,
            }));
            let token = ChatToken("t".repeat(64));
            let line = line_with(Some(&token), &ask).expect("a line");
            let (read, carried) =
                read_line(std::str::from_utf8(&line).expect("text")).expect("it reads");
            assert_eq!(carried.as_deref(), Some(token.expose()));
            assert_eq!(read.chat(), 7);
            let Line::Ask(back) = read else {
                panic!("a git ask read as another kind of line");
            };
            assert_eq!(back, ask);
        }
    }

    #[test]
    fn a_commit_ask_names_the_message_and_what_to_stage_and_nothing_else() {
        // #1055: the whole ask. Where it commits, and on which branch, is the app's record.
        let ask = Ask::Commit(Box::new(CommitAsk {
            chat: 7,
            message: "one\n\ntwo".to_owned(),
            stage: Stage::Paths(vec!["src/a.rs".to_owned(), "-n".to_owned()]),
        }));
        let line = line_with(None, &ask).expect("a line");
        let text = std::str::from_utf8(&line).expect("text");
        assert_eq!(
            text,
            "{\"commit\":{\"chat\":7,\"message\":\"one\\n\\ntwo\",\
             \"stage\":{\"paths\":[\"src/a.rs\",\"-n\"]}}}\n"
        );
        let (read, _) = read_line(text).expect("it reads");
        assert_eq!(read.chat(), 7);
        let Line::Ask(back) = read else {
            panic!("the ask read as another kind of line");
        };
        assert_eq!(back, ask);
        let tracked = "{\"commit\":{\"chat\":7,\"message\":\"m\",\"stage\":\"tracked\"}}";
        let (read, _) = read_line(tracked).expect("it reads");
        assert!(matches!(
            read,
            Line::Ask(Ask::Commit(commit)) if commit.stage == Stage::Tracked
        ));
        // A line that names anything else is not a commit ask: no folder, repository, branch,
        // git directory, author, date, or option such as amend.
        for extra in [
            "cwd",
            "folder",
            "repo",
            "workspace",
            "branch",
            "git_dir",
            "author",
            "date",
            "amend",
            "options",
        ] {
            let line = format!(
                "{{\"commit\":{{\"chat\":7,\"message\":\"m\",\"stage\":\"tracked\",\"{extra}\":\"x\"}}}}"
            );
            assert!(
                !matches!(read_line(&line), Some((Line::Ask(Ask::Commit(_)), _))),
                "{extra} was read"
            );
        }
        // And no other word for what to stage.
        for stage in [
            "\"all\"",
            "{\"amend\":true}",
            "{\"reset\":[]}",
            "{\"push\":[]}",
        ] {
            let line = format!("{{\"commit\":{{\"chat\":7,\"message\":\"m\",\"stage\":{stage}}}}}");
            assert!(
                !matches!(read_line(&line), Some((Line::Ask(Ask::Commit(_)), _))),
                "{stage} was read"
            );
        }
    }

    #[test]
    fn asking_where_a_chat_works_names_only_the_chat_its_token_must_be() {
        // #1450: the whole ask is the chat's number and why it asks.
        let ask = Ask::WhereWorking(WhereWorking {
            chat: 7,
            tell: crate::awareness::Tell::Turn,
        });
        let line = line_with(None, &ask).expect("a line");
        let text = std::str::from_utf8(&line).expect("text");
        assert_eq!(text, "{\"where_working\":{\"chat\":7,\"tell\":\"turn\"}}\n");
        let (read, _) = read_line(text).expect("it reads");
        assert_eq!(read.chat(), 7);
        let Line::Ask(back) = read else {
            panic!("the ask read as another kind of line");
        };
        assert_eq!(back, ask);
        // And one that says no more than its chat is the command's.
        let (bare, _) = read_line("{\"where_working\":{\"chat\":7}}").expect("it reads");
        let Line::Ask(Ask::WhereWorking(bare)) = bare else {
            panic!("the ask read as another kind of line");
        };
        assert_eq!(bare.tell, crate::awareness::Tell::Asked);
    }

    #[cfg(unix)]
    #[test]
    fn asking_where_a_chat_works_under_a_forged_sender_is_refused_and_never_reaches_the_app() {
        // #1450, D-1407-6/10: chat 3's picture is told only to a line carrying chat 3's token.
        let dir = tempfile::tempdir().expect("a directory");
        let path = dir.path().join("hooks.sock");
        let listener = Listener::bind(dir.path(), &path).expect("a socket");
        let three = listener.tokens().issue_to_this_process(3).expect("a token");
        let four = listener.tokens().issue_to_this_process(4).expect("a token");
        let asked = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counted = Arc::clone(&asked);
        let _reading = listener.each_answering(
            Box::new(|_| {}),
            Box::new(move |_, _| {
                counted.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Answer::No {
                    why: "the app was asked".to_owned(),
                }
            }),
        );
        let within = std::time::Duration::from_secs(5);
        let ask = Ask::WhereWorking(WhereWorking {
            chat: 3,
            tell: crate::awareness::Tell::Asked,
        });

        for token in [None, Some(ChatToken::from("nope")), Some(four)] {
            let answered = Asking::on(&path, token)
                .expect("connected")
                .ask(&ask, within);
            assert!(
                matches!(&answered, Ok(Answer::No { why }) if why.contains("token")),
                "answered {answered:?}"
            );
        }
        assert_eq!(asked.load(std::sync::atomic::Ordering::SeqCst), 0);

        let answered = Asking::on(&path, Some(three))
            .expect("connected")
            .ask(&ask, within);
        assert!(
            matches!(&answered, Ok(Answer::No { why }) if why == "the app was asked"),
            "{answered:?}"
        );
    }

    // ----- a dispatch (#1436) ----------------------------------------------------------------

    fn a_dispatch(chat: u32) -> Ask {
        Ask::Dispatch(Box::new(DispatchAsk {
            chat,
            to: Some("devops".to_owned()),
            name: "check the queue".to_owned(),
            brief: "# Check the queue\nbody\n".to_owned(),
            profile: None,
            place: None,
            ticket: "t".repeat(64),
        }))
    }

    #[test]
    fn a_dispatch_is_read_back_whole_and_names_the_chat_its_token_must_be() {
        let ask = a_dispatch(7);
        let token = ChatToken("t".repeat(64));
        let line = line_with(Some(&token), &ask).expect("a line");
        let (read, carried) =
            read_line(std::str::from_utf8(&line).expect("text")).expect("it reads");
        assert_eq!(carried.as_deref(), Some(token.expose()));
        assert_eq!(read.chat(), 7, "sender binding checks this chat's token");
        let Line::Ask(back) = read else {
            panic!("a dispatch read as another kind of line");
        };
        assert_eq!(back, ask);
    }

    #[test]
    fn a_dispatch_has_no_field_that_says_who_asks_but_the_chats_number() {
        // The asker's persona, folder, lineage and grants are the app's record of chat 7. A
        // line that claims them anyway is read without them: there is nowhere to keep them.
        // The one thing more it may say is a profile's name, which the app only ever looks up
        // among the profiles the project offers here.
        let written = serde_json::to_value(a_dispatch(7)).expect("json");
        let mut keys: Vec<&str> = written["dispatch"]
            .as_object()
            .expect("an object")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(keys, ["brief", "chat", "name", "ticket", "to"]);

        let forged = concat!(
            r#"{"dispatch":{"chat":7,"name":"x","brief":"a b","ticket":"t","#,
            r#""from":"devops","persona":"devops","asker":3,"profile":"prod","cwd":"/","#,
            r#""grant":"in_force","mode":"handoff","depth":0,"without_sandbox":true,"#,
            r#""root":"01J9ZQ3V5N8X4T2K7M6P0R1S2A","by":"person","attended":true,"#,
            r#""permission_mode":"bypassPermissions","grants":{"hosts":["evil.example"]}}}"#
        );
        let (read, _) = read_line(forged).expect("it reads");
        assert_eq!(read.chat(), 7);
        let Line::Ask(Ask::Dispatch(read)) = read else {
            panic!("a dispatch");
        };
        assert_eq!(
            *read,
            DispatchAsk {
                chat: 7,
                to: None,
                name: "x".to_owned(),
                brief: "a b".to_owned(),
                profile: Some("prod".to_owned()),
                place: None,
                ticket: "t".to_owned(),
            }
        );
    }

    #[test]
    fn a_dispatch_says_where_in_one_word_and_has_no_field_for_a_folder_or_a_branch() {
        // #1453: `in` is the only thing a line says about where the new chat works. A line
        // that names a folder, a repo, a piece or a branch anyway is read without them.
        let forged = concat!(
            r#"{"dispatch":{"chat":7,"name":"x","brief":"a b","ticket":"t","in":"worktree","#,
            r#""branch":"main","piece":"../../main","folder":"/etc","repo":"other","#,
            r#""worktree":{"branch":"main","path":"/"},"workspace":"beta","base":"main"}}"#
        );
        let (read, _) = read_line(forged).expect("it reads");
        let Line::Ask(Ask::Dispatch(read)) = read else {
            panic!("a dispatch");
        };
        assert_eq!(
            *read,
            DispatchAsk {
                chat: 7,
                to: None,
                name: "x".to_owned(),
                brief: "a b".to_owned(),
                profile: None,
                place: Some("worktree".to_owned()),
                ticket: "t".to_owned(),
            }
        );
        // And it is written back as that one word.
        let written = serde_json::to_value(Ask::Dispatch(read)).expect("json");
        assert_eq!(written["dispatch"]["in"], "worktree");
        assert_eq!(written["dispatch"].get("place"), None);
    }

    #[cfg(unix)]
    #[test]
    fn a_dispatch_that_names_another_chat_as_its_asker_is_refused_and_never_reaches_the_app() {
        // A forged line: chat 4's own token, naming chat 3 as the asker. And no token at all.
        let dir = tempfile::tempdir().expect("a directory");
        let path = dir.path().join("hooks.sock");
        let listener = Listener::bind(dir.path(), &path).expect("a socket");
        let three = listener.tokens().issue_to_this_process(3).expect("a token");
        let four = listener.tokens().issue_to_this_process(4).expect("a token");
        let asked = Arc::new(std::sync::Mutex::new(Vec::new()));
        let heard = Arc::clone(&asked);
        let _reading = listener.each_answering(
            Box::new(|_| {}),
            Box::new(move |_, ask| {
                heard.lock().unwrap().push(ask);
                Answer::Dispatched {
                    chat: 9,
                    name: "check the queue".to_owned(),
                    persona: None,
                    note: None,
                    works: None,
                }
            }),
        );
        let within = std::time::Duration::from_secs(5);

        for token in [None, Some(four)] {
            let answered = Asking::on(&path, token)
                .expect("connected")
                .ask(&a_dispatch(3), within);
            assert!(
                matches!(&answered, Ok(Answer::No { why }) if why.contains("token")),
                "answered {answered:?}"
            );
        }
        assert!(asked.lock().unwrap().is_empty(), "the app was never asked");

        // The chat's own line is the one the app hears.
        let answered = Asking::on(&path, Some(three))
            .expect("connected")
            .ask(&a_dispatch(3), within);
        assert!(
            matches!(answered, Ok(Answer::Dispatched { chat: 9, .. })),
            "{answered:?}"
        );
        assert_eq!(asked.lock().unwrap().clone(), vec![a_dispatch(3)]);
    }

    #[cfg(unix)]
    #[test]
    fn a_dispatch_with_its_chats_token_from_outside_the_chat_is_refused_saying_why() {
        // D-1407-6/10: the token alone is not enough. A process that is not inside the chat's
        // own program is told so, and the app is never asked.
        let dir = tempfile::tempdir().expect("a directory");
        let path = dir.path().join("hooks.sock");
        let listener = Listener::bind(dir.path(), &path).expect("a socket");
        let (token, mut elsewhere) = a_chat_elsewhere(&listener.tokens(), 5);
        let asked = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counted = Arc::clone(&asked);
        let _reading = listener.each_answering(
            Box::new(|_| {}),
            Box::new(move |_, _| {
                counted.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Answer::Dispatched {
                    chat: 9,
                    name: "x".to_owned(),
                    persona: None,
                    note: None,
                    works: None,
                }
            }),
        );

        let answered = Asking::on(&path, Some(token))
            .expect("connected")
            .ask(&a_dispatch(5), std::time::Duration::from_secs(5));

        let _ = elsewhere.kill();
        let _ = elsewhere.wait();
        assert_eq!(
            answered.ok(),
            Some(Answer::No {
                why: OUTSIDE_THE_CHAT.to_owned()
            })
        );
        assert_eq!(asked.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    #[test]
    fn a_report_back_is_written_as_it_always_was_and_a_task_s_adds_its_outcome() {
        let plain = Ask::Report(Box::new(ReportBack {
            chat: 7,
            summary: "done".to_owned(),
            ticket: "t".to_owned(),
            task: None,
        }));
        let as_before = r#"{"report":{"chat":7,"summary":"done","ticket":"t"}}"#;
        assert_eq!(serde_json::to_string(&plain).unwrap(), as_before);
        // And a line from a purlis older than tasks reads as a report with no task part.
        let (read, _) = read_line(as_before).expect("reads");
        assert!(matches!(read, Line::Ask(back) if back == plain));

        let task = Ask::Report(Box::new(ReportBack {
            chat: 7,
            summary: "done".to_owned(),
            ticket: "t".to_owned(),
            task: Some(TaskReport {
                outcome: crate::handback::Outcome::Blocked,
                changed: Some("svc: 2 files".to_owned()),
            }),
        }));
        let line = line_with(None, &task).expect("a line");
        let (read, _) = read_line(std::str::from_utf8(&line).unwrap()).expect("reads");
        assert!(matches!(read, Line::Ask(back) if back == task));
    }

    #[cfg(unix)]
    #[test]
    fn an_answer_that_takes_longer_than_a_report_may_still_reaches_the_asker() {
        // A brokered clone runs for as long as git's network limit (#1335): the listener's own
        // deadline on the connection is for reading a line, and must not cut the answer off.
        let dir = tempfile::tempdir().expect("a directory");
        let path = dir.path().join("hooks.sock");
        let listener = Listener::bind(dir.path(), &path).expect("a socket");
        let token = listener.tokens().issue_to_this_process(3).expect("a token");
        let slow = A_REPORT_TAKES_AT_MOST + std::time::Duration::from_secs(1);
        let _reading = listener.each_answering(
            Box::new(|_| {}),
            Box::new(move |_, _| {
                std::thread::sleep(slow);
                Answer::Said {
                    lines: vec![crate::repocmd::Say::Done("cloned".to_owned())],
                    code: 0,
                }
            }),
        );
        let answer = Asking::on(&path, Some(token))
            .expect("connected")
            .ask(
                &Ask::Git(Box::new(GitAsk {
                    chat: 3,
                    workspace: "alpha".to_owned(),
                    work: GitWork::Clone {
                        repos: vec!["widget".to_owned()],
                    },
                })),
                slow * 4,
            )
            .expect("answered");
        assert!(matches!(answer, Answer::Said { code: 0, .. }), "{answer:?}");
    }

    #[test]
    fn the_largest_git_ask_a_command_line_makes_fits_on_one_line_of_the_socket() {
        // A clone of 256 repos with long names, every byte one JSON doubles: what a person
        // would type at its worst still reaches the app whole (#1335).
        let ask = Ask::Git(Box::new(GitAsk {
            chat: u32::MAX,
            workspace: "w".repeat(100),
            work: GitWork::Clone {
                repos: vec!["\"".repeat(100); 256],
            },
        }));
        let line = line_with(Some(&ChatToken("t".repeat(64))), &ask).expect("a line");
        assert!(
            (line.len() as u64) < A_LINE_IS_AT_MOST,
            "{} bytes does not fit in {A_LINE_IS_AT_MOST}",
            line.len()
        );
    }

    #[test]
    fn what_a_brokered_git_action_said_is_read_back_line_for_line() {
        use crate::repocmd::Say;
        let said = Answer::Said {
            lines: vec![
                Say::Info("workspace: alpha  (via --workspace)".to_owned()),
                Say::Done("widget → workspaces/alpha/widget".to_owned()),
                Say::Warn("w".to_owned()),
                Say::Fail("f".to_owned()),
                Say::Plain("p".to_owned()),
                Say::Out("o".to_owned()),
            ],
            code: 2,
        };
        let line = serde_json::to_string(&said).expect("a line");
        assert_eq!(
            serde_json::from_str::<Answer>(&line).expect("it reads"),
            said
        );
    }

    #[test]
    fn a_hook_reports_the_chat_its_environment_names() {
        // Measured on claude 2.1.276: the payload carries `session_id`, and it is the same
        // value as `$CLAUDE_CODE_SESSION_ID` (ADR 0024, C7).
        let env = env_of(&[(SOCKET_ENV, "/tmp/s.sock"), (CHAT_ENV, "7")]);

        let report = Report::read(Event::Stop, CLAUDE_STOP, &env).expect("a report");

        assert_eq!(report.chat, 7);
        assert_eq!(report.event, Event::Stop);
        assert_eq!(
            report.conversation,
            Conversation::Named("11111111-2222-4333-8444-555555555555".to_owned())
        );
    }

    #[test]
    fn a_hook_outside_a_chat_the_app_started_reports_nothing() {
        // No `CHARTER_CHAT` means this harness was not started by the app — the operator's
        // own `claude` in a terminal, with the plugin's hooks pointed here. There is no chat
        // to move, and inventing one would move somebody else's.
        let env = env_of(&[(SOCKET_ENV, "/tmp/s.sock")]);

        assert_eq!(Report::read(Event::Stop, CLAUDE_STOP, &env), None);
    }

    #[test]
    fn a_chat_number_that_is_not_one_reports_nothing() {
        for bad in ["", "seven", "-1", "4294967296", " 7"] {
            let env = env_of(&[(SOCKET_ENV, "/tmp/s.sock"), (CHAT_ENV, bad)]);
            assert_eq!(
                Report::read(Event::Stop, CLAUDE_STOP, &env),
                None,
                "{bad:?} was taken as a chat"
            );
        }
    }

    #[test]
    fn a_payload_that_will_not_parse_still_reports_the_event() {
        // The event came from argv, which charter wrote into the harness's own settings. A
        // truncated or empty payload costs the conversation and nothing else — losing the
        // EVENT would leave a chat marked as working forever.
        let env = env_of(&[(SOCKET_ENV, "/tmp/s.sock"), (CHAT_ENV, "7")]);

        for payload in ["", "{", "not json at all"] {
            let report = Report::read(Event::Stop, payload, &env)
                .unwrap_or_else(|| panic!("{payload:?} reported nothing"));
            assert_eq!(report.chat, 7);
            assert_eq!(report.event, Event::Stop);
            assert_eq!(report.conversation, Conversation::Unknown);
        }
    }

    #[test]
    fn a_payload_that_parses_but_names_no_conversation_is_a_dialect_charter_cannot_read() {
        // Different from a payload charter could not read at ALL, and the difference is what
        // keeps a nested harness out: opencode names its conversation under `sessionID`
        // (ADR 0024, O1), inherits `CLAUDE_PID` and sets neither variable of its own. A
        // review reproduced one marking the outer Claude chat as waiting, mid-turn.
        let env = env_of(&[(SOCKET_ENV, "/tmp/s.sock"), (CHAT_ENV, "7")]);

        for payload in [
            "null",
            "[]",
            r#"{"session_id":42}"#,
            r#"{"sessionID":"11111111-2222-4333-8444-555555555555"}"#,
        ] {
            let report = Report::read(Event::Stop, payload, &env)
                .unwrap_or_else(|| panic!("{payload:?} reported nothing"));
            assert_eq!(
                report.conversation,
                Conversation::Foreign,
                "{payload:?} was read as purlis's own dialect"
            );
        }
    }

    #[test]
    fn a_conversation_counts_only_when_the_payload_and_the_environment_agree() {
        // ADR 0024, C7: every Claude Code hook's `CLAUDE_CODE_SESSION_ID` equals its
        // payload's `session_id`. The Python charter requires the two to be equal and so
        // does this.
        let env = env_of(&[
            (SOCKET_ENV, "/tmp/s.sock"),
            (CHAT_ENV, "7"),
            (
                CLAUDE_CONVERSATION_ENV,
                "11111111-2222-4333-8444-555555555555",
            ),
            (CLAUDE_PID_ENV, "4242"),
        ]);

        let report = Report::read(Event::Stop, CLAUDE_STOP, &env).expect("a report");

        assert_eq!(
            report.conversation,
            Conversation::Named("11111111-2222-4333-8444-555555555555".to_owned())
        );
        assert_eq!(report.pid, Some(4242));
    }

    #[test]
    fn a_lost_payload_names_no_conversation_rather_than_the_one_in_the_environment() {
        // **This is a defect an independent review found, kept as a test so it cannot come
        // back.** The environment holds the OUTER chat's id, because a harness nested in the
        // chat's shell inherits it. Falling back to it when a payload is slow does not lose
        // the id — it substitutes the wrong one, and the nested harness's report is then
        // taken as the outer chat's. Exactly what ADR 0024's C5 exists to refuse.
        let env = env_of(&[
            (SOCKET_ENV, "/tmp/s.sock"),
            (CHAT_ENV, "7"),
            (CLAUDE_CONVERSATION_ENV, "the-outer-chat"),
        ]);

        let report = Report::read(Event::Stop, "", &env).expect("a report");

        assert_eq!(
            report.conversation,
            Conversation::Unknown,
            "the outer chat's id was borrowed"
        );
    }

    #[test]
    fn a_payload_that_disagrees_with_the_environment_names_no_conversation() {
        // A nested harness whose payload charter can read: it says its own id, and the
        // environment says the outer one. Neither is this chat's report.
        let env = env_of(&[
            (SOCKET_ENV, "/tmp/s.sock"),
            (CHAT_ENV, "7"),
            (CLAUDE_CONVERSATION_ENV, "the-outer-chat"),
        ]);

        let report = Report::read(Event::Stop, CLAUDE_STOP, &env).expect("a report");

        // Contradicted, not merely unknown: charter read both and they disagree. That is
        // different evidence from "I could not tell", and the board treats them differently.
        assert_eq!(report.conversation, Conversation::Contradicted);
    }

    #[test]
    fn a_conversation_that_merely_starts_the_same_way_is_a_different_conversation() {
        // A surviving mutant: `said == here` weakened to `said.starts_with(&here)` passed
        // every test, because nothing compared two ids that were nearly the same. This one
        // line is what the whole check against a nested harness rests on.
        let env = env_of(&[
            (SOCKET_ENV, "/tmp/s.sock"),
            (CHAT_ENV, "7"),
            (
                CLAUDE_CONVERSATION_ENV,
                "11111111-2222-4333-8444-555555555555",
            ),
        ]);
        let longer = r#"{"session_id":"11111111-2222-4333-8444-555555555555-nested"}"#;

        assert_eq!(
            Report::read(Event::Stop, longer, &env)
                .expect("a report")
                .conversation,
            Conversation::Contradicted
        );
    }

    #[test]
    fn a_harness_that_names_no_conversation_in_its_environment_is_taken_at_its_payload() {
        // Codex sets no `CLAUDE_CODE_SESSION_ID`, so its payload is contradicted by nothing.
        let env = env_of(&[(SOCKET_ENV, "/tmp/s.sock"), (CHAT_ENV, "7")]);

        let report = Report::read(Event::Stop, CLAUDE_STOP, &env).expect("a report");

        assert_eq!(
            report.conversation,
            Conversation::Named("11111111-2222-4333-8444-555555555555".to_owned())
        );
        assert_eq!(report.pid, None);
    }

    #[test]
    fn the_variables_a_session_must_not_inherit_are_the_ones_that_name_a_harness() {
        // Whatever the app inherited, a chat must start without these: they say which
        // conversation and which process a hook belongs to, and only the harness the app
        // starts may answer that.
        assert!(NOT_INHERITED.contains(&CLAUDE_CONVERSATION_ENV));
        assert!(NOT_INHERITED.contains(&CLAUDE_PID_ENV));
        // Nor where the chat that launched the app was pinned (SI-1).
        assert!(NOT_INHERITED.contains(&crate::active::WORKSPACE_ENV));
        assert!(NOT_INHERITED.contains(&crate::active::PLANE_ROOT_ENV));
        // Nor the launcher's own chat token: a chat is given its own at the `exec`.
        assert!(NOT_INHERITED.contains(&TOKEN_ENV));
        assert!(NOT_INHERITED.contains(&SANDBOXED_ENV));
        assert!(NOT_INHERITED.contains(&crate::sandboxblock::CHAT_DIR_ENV));
        assert!(NOT_INHERITED.contains(&crate::noterminal::RELAUNCHED_ENV));
        // charter's own two are set per session, after these are removed, so they are not
        // here — removing them would remove what the app just put in.
        assert!(!NOT_INHERITED.contains(&SOCKET_ENV));
        assert!(!NOT_INHERITED.contains(&CHAT_ENV));
    }

    #[test]
    fn a_pid_that_is_not_one_is_no_pid() {
        // `charter/hooks.py` treats a missing or non-numeric `CLAUDE_PID` as "no report" for
        // adoption. A zero is not a pid either.
        for bad in ["", "0", "-1", "nine", "12x"] {
            let env = env_of(&[
                (SOCKET_ENV, "/tmp/s.sock"),
                (CHAT_ENV, "7"),
                (CLAUDE_PID_ENV, bad),
            ]);
            assert_eq!(
                Report::read(Event::Stop, CLAUDE_STOP, &env)
                    .expect("a report")
                    .pid,
                None,
                "{bad:?} was taken as a pid"
            );
        }
    }

    #[test]
    fn a_listener_another_was_bound_over_is_dropped_without_waiting_for_good() {
        // Two listeners at one path in one process: the second takes the path. Dropping the
        // first cannot wake its own thread any more (its connection reaches the second), and
        // it must come back all the same. A join there held a whole test run for an hour.
        let dir = tempfile::tempdir().expect("a directory");
        let path = dir.path().join("hooks.sock");
        let first = Listener::bind(dir.path(), &path)
            .expect("a socket")
            .each(Box::new(|_| {}));
        let listener = Listener::bind(dir.path(), &path).expect("the same path, bound again");
        let token = listener.tokens().issue_to_this_process(7).expect("a token");
        let (tx, rx) = mpsc::channel();
        let second = listener.each(Box::new(move |report| {
            let _ = tx.send(report);
        }));

        let (dropped, came_back) = mpsc::channel();
        std::thread::spawn(move || {
            drop(first);
            let _ = dropped.send(());
        });
        came_back
            .recv_timeout(A_READING_STOPS_WITHIN + std::time::Duration::from_secs(10))
            .expect("dropping a listener another was bound over never came back");

        // The second is untouched by it: its file is still there, and it still hears.
        let sent = Report {
            chat: 7,
            event: Event::Notification,
            conversation: Conversation::Named("abc".to_owned()),
            pid: Some(99),
            agent: None,
            detail: Detail::default(),
        };
        send(&path, Some(&token), &sent).expect("the listener at the path took it");
        assert_eq!(rx.recv_timeout(std::time::Duration::from_secs(5)), Ok(sent));

        // And one dropped the ordinary way still stops at once and takes its file.
        let began = std::time::Instant::now();
        drop(second);
        assert!(began.elapsed() < A_READING_STOPS_WITHIN);
        assert!(!path.exists());
    }

    #[test]
    fn a_report_reaches_the_app_that_is_listening() {
        let dir = tempfile::tempdir().expect("a directory");
        let path = dir.path().join("hooks.sock");
        let listener = Listener::bind(dir.path(), &path).expect("a socket");
        let token = listener.tokens().issue_to_this_process(7).expect("a token");
        let (tx, rx) = mpsc::channel();
        let _reading = listener.each(Box::new(move |report| {
            let _ = tx.send(report);
        }));

        let sent = Report {
            chat: 7,
            event: Event::Notification,
            conversation: Conversation::Named("abc".to_owned()),
            pid: Some(99),
            agent: None,
            detail: Detail::default(),
        };
        send(&path, Some(&token), &sent).expect("the app took it");

        assert_eq!(rx.recv_timeout(std::time::Duration::from_secs(5)), Ok(sent));
    }

    #[test]
    fn every_report_arrives_even_when_many_hooks_fire_at_once() {
        // Fifty live sessions is the product's scale, and a turn ending in each of them is
        // fifty hook processes at the same moment.
        let dir = tempfile::tempdir().expect("a directory");
        let path = dir.path().join("hooks.sock");
        let listener = Listener::bind(dir.path(), &path).expect("a socket");
        let tokens = listener.tokens();
        let (tx, rx) = mpsc::channel();
        let _reading = listener.each(Box::new(move |report| {
            let _ = tx.send(report.chat);
        }));

        let path = Arc::new(path);
        let senders: Vec<_> = (0..50u32)
            .map(|chat| {
                let path = Arc::clone(&path);
                let token = tokens.issue_to_this_process(chat).expect("a token");
                std::thread::spawn(move || {
                    send(
                        &path,
                        Some(&token),
                        &Report {
                            chat,
                            event: Event::Stop,
                            conversation: Conversation::Unknown,
                            pid: None,
                            agent: None,
                            detail: Detail::default(),
                        },
                    )
                })
            })
            .collect();
        for sender in senders {
            sender.join().expect("the thread").expect("it was taken");
        }

        let mut seen: Vec<u32> = (0..50)
            .map(|_| {
                rx.recv_timeout(std::time::Duration::from_secs(10))
                    .expect("a report")
            })
            .collect();
        seen.sort_unstable();
        assert_eq!(seen, (0..50).collect::<Vec<_>>());
    }

    #[test]
    fn one_connection_that_says_nothing_useful_does_not_silence_the_channel() {
        // An app that stopped listening because something wrote nonsense at it would leave
        // every chat frozen at the state it last had — and never say so. Anything at all can
        // reach this socket: a stale hook from an older charter, a port scanner, a person
        // with `nc`.
        use std::io::Write;

        let dir = tempfile::tempdir().expect("a directory");
        let path = dir.path().join("hooks.sock");
        let listener = Listener::bind(dir.path(), &path).expect("a socket");
        let token = listener.tokens().issue_to_this_process(7).expect("a token");
        let (tx, rx) = mpsc::channel();
        let _reading = listener.each(Box::new(move |report| {
            let _ = tx.send(report);
        }));

        // Connected and closed without a word; a line that is not JSON; JSON that is not a
        // report; and a report missing the one field that names a chat.
        drop(std::os::unix::net::UnixStream::connect(&path).expect("a connection"));
        for rubbish in [
            "not json at all\n",
            "{}\n",
            r#"{"event":"stop"}"#,
            r#"{"chat":"seven","event":"stop"}"#,
        ] {
            let mut socket = std::os::unix::net::UnixStream::connect(&path).expect("a connection");
            socket.write_all(rubbish.as_bytes()).expect("it is written");
        }

        let good = Report {
            chat: 7,
            event: Event::Stop,
            conversation: Conversation::Unknown,
            pid: None,
            agent: None,
            detail: Detail::default(),
        };
        send(&path, Some(&token), &good).expect("the app took it");

        assert_eq!(
            rx.recv_timeout(std::time::Duration::from_secs(5)),
            Ok(good),
            "the channel went quiet after something wrote rubbish at it"
        );
    }

    #[test]
    fn a_client_that_connects_and_says_nothing_does_not_freeze_the_channel() {
        // **A defect an independent review reproduced, kept as a test.** The listener used
        // to read every connection on one thread, so anything that connected and did not
        // write — `nc -U` on the socket, a hook stopped in a debugger, a child that
        // inherited the descriptor — held it inside `read_line` forever. Every other
        // session's report queued behind it, and the sidebar went quiet with nothing to say
        // why.
        let dir = tempfile::tempdir().expect("a directory");
        let path = dir.path().join("hooks.sock");
        let (tx, rx) = mpsc::channel();
        let listener = Listener::bind(dir.path(), &path).expect("a socket");
        let token = listener.tokens().issue_to_this_process(7).expect("a token");
        let _reading = listener.each(Box::new(move |report| {
            let _ = tx.send(report);
        }));

        // Connected, never written to, and held open for the rest of the test.
        let silent = std::os::unix::net::UnixStream::connect(&path).expect("a connection");
        silent
            .set_read_timeout(Some(A_REPORT_TAKES_AT_MOST * 4))
            .expect("a deadline of our own");

        let good = Report {
            chat: 7,
            event: Event::Stop,
            conversation: Conversation::Unknown,
            pid: None,
            agent: None,
            detail: Detail::default(),
        };
        send(&path, Some(&token), &good).expect("the app took it");

        // Well inside the deadline the silent connection will eventually hit: the point is
        // that the good report does not WAIT for it. One reader for every connection would
        // still get there, two seconds later, with fifty sessions' reports behind it.
        assert_eq!(
            rx.recv_timeout(A_REPORT_TAKES_AT_MOST / 4),
            Ok(good),
            "a report queued behind a client that never spoke"
        );

        // And the silent one is let go rather than held forever: the app closes its end when
        // its deadline passes. Without that, every such connection costs a thread for as long
        // as the app runs.
        //
        // End of file OR an error, and the distinction that matters is WHICH error: a clean
        // close reads `Ok(0)` on macOS and can read `ECONNRESET` on Linux, and CI found that
        // difference. `WouldBlock` is the one answer that means the app did NOT let go — it
        // is this client's own deadline firing, four times longer than the app's.
        let mut nothing = [0u8; 1];
        let let_go = (&silent).read(&mut nothing);
        assert!(
            !matches!(&let_go, Err(err) if err.kind() == io::ErrorKind::WouldBlock),
            "the app never let go of a client that said nothing"
        );
    }

    #[test]
    fn a_client_that_connects_and_says_nothing_does_not_hold_up_shutting_down() {
        // The same defect's other half: `Reading::drop` joins the reading thread, and a
        // thread stuck in `read_line` never returns — so quitting the app hung. Quitting is
        // the one thing that must always finish.
        let dir = tempfile::tempdir().expect("a directory");
        let path = dir.path().join("hooks.sock");
        let reading = Listener::bind(dir.path(), &path)
            .expect("a socket")
            .each(Box::new(|_| {}));
        let _silent = std::os::unix::net::UnixStream::connect(&path).expect("a connection");

        let (done, waited) = mpsc::channel();
        std::thread::spawn(move || {
            drop(reading);
            let _ = done.send(());
        });

        assert_eq!(
            waited.recv_timeout(std::time::Duration::from_secs(10)),
            Ok(()),
            "shutting down waited on a client that never spoke"
        );
    }

    #[test]
    fn a_client_that_writes_without_ever_ending_a_line_is_cut_off() {
        // Nothing stops a client writing bytes and never sending a newline. Without a cap
        // the app grows a `String` for it until there is no memory left.
        use std::io::Write;

        let dir = tempfile::tempdir().expect("a directory");
        let path = dir.path().join("hooks.sock");
        let (tx, rx) = mpsc::channel();
        let listener = Listener::bind(dir.path(), &path).expect("a socket");
        let token = listener.tokens().issue_to_this_process(7).expect("a token");
        let _reading = listener.each(Box::new(move |report| {
            let _ = tx.send(report);
        }));

        let mut flood = std::os::unix::net::UnixStream::connect(&path).expect("a connection");
        // Set before anything is written, while the connection is certainly healthy.
        flood
            .set_read_timeout(Some(A_REPORT_TAKES_AT_MOST * 4))
            .expect("a deadline of our own");
        flood
            .set_write_timeout(Some(A_REPORT_TAKES_AT_MOST * 4))
            .expect("a deadline of our own");
        // Comfortably past the cap, with not one newline in it. The app stops reading at the
        // cap and closes, so this either errors or the read below sees end of file — what it
        // must never do is keep taking bytes.
        for _ in 0..40 {
            if flood.write_all(&vec![b'x'; 8 * 1024]).is_err() {
                break;
            }
        }
        // **Inside the deadline, not merely eventually.** Without the cap the app keeps
        // reading until the read deadline passes, so an assertion with no clock in it passes
        // either way — a review pointed that out. The cap is observable by WHEN the app lets
        // go, not by memory.
        let began = std::time::Instant::now();
        let mut nothing = [0u8; 1];
        let let_go = (&flood).read(&mut nothing);
        assert!(
            !matches!(&let_go, Err(err) if err.kind() == io::ErrorKind::WouldBlock),
            "the app is still reading a line that will never end"
        );
        assert!(
            began.elapsed() < A_REPORT_TAKES_AT_MOST / 4,
            "the app read for {:?}, so it was the deadline that stopped it and not the cap",
            began.elapsed()
        );

        let good = Report {
            chat: 7,
            event: Event::Stop,
            conversation: Conversation::Unknown,
            pid: None,
            agent: None,
            detail: Detail::default(),
        };
        send(&path, Some(&token), &good).expect("the app took it");
        assert_eq!(rx.recv_timeout(std::time::Duration::from_secs(5)), Ok(good));
    }

    #[test]
    fn the_socket_sits_in_a_directory_nobody_else_may_enter() {
        // The socket's own mode is set after `bind`, and between the two it is at whatever
        // the umask allowed. A directory nobody else may enter closes that window, and is
        // what actually holds under a permissive umask.
        let dir = tempfile::tempdir().expect("a directory");
        let inside = dir.path().join("app");
        let path = inside.join("hooks.sock");

        let _listener = Listener::bind(dir.path(), &path).expect("a socket");

        let mode = std::fs::metadata(&inside)
            .expect("the directory is there")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o700, "the socket's directory is {mode:o}");
    }

    #[test]
    fn a_socket_will_not_be_bound_inside_a_directory_somebody_else_could_have_made() {
        // The fallback path is predictable and, on Linux, lives in `/tmp`. Somebody who gets
        // there first must not end up owning the channel — they could read every report and
        // inject their own. A directory that is not a directory is the shape of that attempt
        // charter can always detect; one owned by another account fails the chmod below it.
        let dir = tempfile::tempdir().expect("a directory");
        let squatted = dir.path().join("app");
        std::fs::write(&squatted, b"not a directory").expect("something else is there first");

        let refused = Listener::bind(dir.path(), &squatted.join("hooks.sock"));

        assert!(
            refused.is_err(),
            "purlis bound a socket under something it does not own"
        );
    }

    #[test]
    fn a_socket_reached_through_a_link_higher_up_is_refused() {
        // **The gate sat one level shallower than the write.** `private_directory` checked
        // the directory it was about to make and nothing above it, so `.charter` being a link
        // redirected where charter created a directory and bound a socket — outside the
        // plane, somewhere chosen by whoever wrote the link. `app` itself being a link was
        // caught; the component above it was not.
        //
        // charter-app#28 ruled this exact directory may not be reached through a link, for
        // the record it already keeps there. The socket is charter's own file in charter's
        // own directory: a link anywhere on the way to it has no honest use.
        let plane = tempfile::tempdir().expect("a plane");
        let elsewhere = tempfile::tempdir().expect("somewhere outside it");
        std::os::unix::fs::symlink(elsewhere.path(), plane.path().join(".charter"))
            .expect("a link in the way");

        let refused = Listener::bind(
            plane.path(),
            &plane.path().join(".charter").join("app").join("hooks.sock"),
        );

        assert!(refused.is_err(), "purlis bound a socket through a link");
        assert!(
            !elsewhere.path().join("app").exists(),
            "it made a directory outside the plane on the way"
        );
    }

    #[test]
    fn a_component_charter_cannot_even_look_at_is_refused_rather_than_walked_past() {
        // A surviving mutant: taking ANY error as "not there yet" passed everything. It is
        // only `NotFound` that means charter is about to make it — a component it cannot
        // stat is one it knows nothing about, and walking past it would be assuming the rest
        // of the path is what it looks like.
        let plane = tempfile::tempdir().expect("a plane");
        let shut = plane.path().join(".charter");
        std::fs::create_dir(&shut).expect("a directory");
        std::fs::set_permissions(&shut, std::fs::Permissions::from_mode(0o000))
            .expect("shut to everyone");

        let refused = Listener::bind(plane.path(), &shut.join("app").join("hooks.sock"));

        // Put it back before any assertion, so a failure does not leave an unreadable
        // directory behind for the tempdir to trip over.
        let _ = std::fs::set_permissions(&shut, std::fs::Permissions::from_mode(0o700));
        assert!(
            refused.is_err(),
            "purlis walked past a component it could not look at"
        );
    }

    #[test]
    fn a_socket_under_a_root_with_nothing_linked_on_the_way_is_bound() {
        // The containment starts at a root the caller names and walks DOWN. It cannot start
        // at `/`: on macOS `/tmp` and `/var` are themselves links, so a walk from the root of
        // the filesystem refuses every path on the machine. That is the shape of resolver
        // this repo has got subtly wrong five times, so there is no resolver here — only a
        // walk of the components below a root the caller already trusts.
        let plane = tempfile::tempdir().expect("a plane");

        let listener = Listener::bind(
            plane.path(),
            &plane.path().join(".charter").join("app").join("hooks.sock"),
        )
        .expect("an ordinary plane binds");

        assert!(listener.path().exists());
    }

    #[test]
    fn a_symlink_standing_in_for_the_socket_directory_is_refused() {
        // A surviving mutant: `symlink_metadata` weakened to `metadata` passed everything,
        // because nothing put a symlink there. It is the precise attack the guard exists to
        // stop — on the predictable path under a shared temp directory, a link left by
        // somebody else would be followed, and `set_permissions` would tighten whatever it
        // aims at while charter listened inside it.
        let dir = tempfile::tempdir().expect("a directory");
        let elsewhere = dir.path().join("elsewhere");
        std::fs::create_dir(&elsewhere).expect("somewhere to point at");
        let link = dir.path().join("app");
        std::os::unix::fs::symlink(&elsewhere, &link).expect("a link in the way");

        let refused = Listener::bind(dir.path(), &link.join("hooks.sock"));

        assert!(
            refused.is_err(),
            "purlis listened inside a symlink it did not make"
        );
        // And it did not tighten what the link pointed at on the way past.
        let mode = std::fs::metadata(&elsewhere)
            .expect("still there")
            .permissions()
            .mode();
        assert_ne!(
            mode & 0o777,
            0o700,
            "it followed the link and chmod'd the target"
        );
    }

    #[test]
    fn a_socket_directory_left_at_the_umask_by_an_older_charter_is_tightened() {
        use std::os::unix::fs::DirBuilderExt;

        let dir = tempfile::tempdir().expect("a directory");
        let loose = dir.path().join("app");
        std::fs::DirBuilder::new()
            .mode(0o755)
            .create(&loose)
            .expect("a directory an older purlis might have left");

        let listener = Listener::bind(dir.path(), &loose.join("hooks.sock")).expect("a socket");

        let mode = std::fs::metadata(loose)
            .expect("it is there")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o700, "the directory is still {mode:o}");
        assert!(listener.path().exists());
    }

    #[test]
    fn a_socket_whose_directory_does_not_exist_yet_is_still_bound() {
        // The app's own path is `<plane>/.charter/app/hooks.sock`, and `.charter/app` may
        // not exist on a plane the app has never run in. A surviving mutant is why this is
        // here: every other test binds inside a directory that already exists.
        let dir = tempfile::tempdir().expect("a directory");
        let path = dir.path().join("never").join("made").join("hooks.sock");

        let listener = Listener::bind(dir.path(), &path).expect("the directory is made");

        assert!(listener.path().exists());
    }

    #[test]
    fn a_socket_directory_that_cannot_be_made_is_refused_for_that_reason() {
        // Not "already exists", which the checks after it judge: any other failure to make the
        // directory is the answer, and the reason it gives is the one the operator reads.
        let dir = tempfile::tempdir().expect("a directory");
        let locked = dir.path().join("locked");
        std::fs::create_dir(&locked).unwrap();
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o555)).unwrap();

        let refused = Listener::bind(dir.path(), &locked.join("app").join("hooks.sock"));

        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
        let Err(err) = refused else {
            panic!("bound inside a directory it could not make")
        };
        assert_eq!(err.kind(), io::ErrorKind::PermissionDenied, "{err}");
    }

    #[test]
    fn something_that_is_not_a_socket_where_the_socket_goes_is_refused_as_it_is() {
        // A stale socket is removed; a directory there is not, and the refusal is the removal's
        // own — not the "address in use" that binding over it would say instead.
        let dir = tempfile::tempdir().expect("a directory");
        let path = dir.path().join("app").join("hooks.sock");
        std::fs::create_dir_all(path.join("inside")).unwrap();

        let Err(err) = Listener::bind(dir.path(), &path) else {
            panic!("bound over a directory")
        };
        assert_ne!(err.kind(), io::ErrorKind::AddrInUse, "{err}");
        assert!(
            path.join("inside").is_dir(),
            "the directory was left as it was"
        );
    }

    #[test]
    fn a_socket_nothing_is_listening_on_refuses_rather_than_hangs() {
        // The app is not running, or has quit. The hook must find out at once and get out of
        // the harness's way.
        let dir = tempfile::tempdir().expect("a directory");
        let path = dir.path().join("nobody.sock");

        let began = std::time::Instant::now();
        let sent = send(
            &path,
            None,
            &Report {
                chat: 1,
                event: Event::Stop,
                conversation: Conversation::Unknown,
                pid: None,
                agent: None,
                detail: Detail::default(),
            },
        );

        assert!(sent.is_err(), "a socket with no app behind it was taken");
        assert!(
            began.elapsed() < std::time::Duration::from_millis(50),
            "it took {:?} to find out nothing was listening",
            began.elapsed()
        );
    }

    #[test]
    fn a_report_is_written_as_one_line_because_that_is_what_the_reader_reads() {
        // The reader takes a line at a time. A surviving mutant is why this is pinned: with
        // the newline dropped, every test still passed — the reader's `read_line` returns at
        // end of file too, so nothing noticed until two reports shared a connection.
        let report = Report {
            chat: 7,
            event: Event::Stop,
            conversation: Conversation::Unknown,
            pid: None,
            agent: None,
            detail: Detail::default(),
        };
        let wire = line_with(Some(&ChatToken::from("t")), &report).expect("it serialises");

        assert_eq!(wire.last(), Some(&b'\n'));
        assert_eq!(wire.iter().filter(|byte| **byte == b'\n').count(), 1);
    }

    #[test]
    fn the_socket_is_taken_away_when_the_app_stops_listening() {
        // A surviving mutant: nothing asserted the socket file was cleaned up. One left
        // behind makes the next `bind` unlink a path it does not own.
        let dir = tempfile::tempdir().expect("a directory");
        let path = dir.path().join("hooks.sock");
        let reading = Listener::bind(dir.path(), &path)
            .expect("a socket")
            .each(Box::new(|_| {}));
        assert!(path.exists());

        drop(reading);

        assert!(
            !path.exists(),
            "the socket outlived the app listening on it"
        );
    }

    #[test]
    fn nobody_else_on_the_machine_can_write_to_the_socket() {
        // Anything that reaches this socket moves a chat's state. Nothing secret goes over
        // it and nothing it says is executed, so the worst another local process could do is
        // make the sidebar lie and raise a notification — but a socket left at whatever the
        // umask happened to be is not something to leave to the umask.
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().expect("a directory");
        let path = dir.path().join("hooks.sock");

        let listener = Listener::bind(dir.path(), &path).expect("a socket");

        let mode = std::fs::metadata(listener.path())
            .expect("the socket is there")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(
            mode, 0o600,
            "the socket is {mode:o}, so another account on this machine can move a chat"
        );
    }

    #[test]
    fn a_socket_left_behind_by_a_process_that_is_gone_is_replaced() {
        // The app was killed, or crashed. A stale socket file must not stop the next launch
        // from listening — and `bind` refuses an address already in use.
        let dir = tempfile::tempdir().expect("a directory");
        let path = dir.path().join("hooks.sock");
        drop(Listener::bind(dir.path(), &path).expect("the first socket"));
        assert!(
            path.exists(),
            "the file is what makes this the interesting case"
        );

        let listener = Listener::bind(dir.path(), &path).expect("the socket is taken over");

        assert_eq!(listener.path(), path);
    }

    // ---- the ask, and the ticket (charter-app#204) --------------------------------------

    fn an_open(chat: u32, ticket: &str) -> Ask {
        Ask::Open(Box::new(OpenChat {
            chat,
            workspace: "alpha".to_owned(),
            create_vision: None,
            persona: None,
            message: "⟨handoff from chat 3 · workspace default · 2026-05-04 11:32⟩\n\nbrief"
                .to_owned(),
            ticket: ticket.to_owned(),
            name: None,
            report: false,
        }))
    }

    #[test]
    fn an_open_from_a_charter_that_predates_names_and_reports_reads_as_neither() {
        let line = r#"{"open":{"chat":3,"workspace":"alpha","create_vision":null,"persona":null,"message":"m","ticket":"t"}}"#;

        let Ask::Open(open) = serde_json::from_str::<Ask>(line).expect("it parses") else {
            panic!("an open")
        };

        assert_eq!(open.name, None);
        assert!(!open.report);
    }

    #[test]
    fn a_report_back_is_an_ask_and_never_a_report() {
        let ask = Ask::Report(Box::new(ReportBack {
            chat: 7,
            summary: "done".to_owned(),
            ticket: "t".to_owned(),
            task: None,
        }));
        let line = serde_json::to_string(&ask).unwrap();

        assert!(matches!(
            serde_json::from_str::<Line>(&line),
            Ok(Line::Ask(_))
        ));
    }

    #[test]
    fn an_asker_that_hung_up_while_it_was_answered_is_seen_to_have_gone() {
        // #1441, M3: a waiting command that was killed must not hold a thread for minutes.
        let tokens = ChatTokens::default();
        let (ours, theirs) = std::os::unix::net::UnixStream::pair().expect("a pair");
        assert!(!tokens.asker_gone(7), "nobody is being answered on 7");

        tokens.answering(7, ours);
        assert!(!tokens.asker_gone(7), "still there, and saying nothing");
        assert!(!tokens.asker_gone(7), "and a look changes nothing");

        drop(theirs);
        assert!(tokens.asker_gone(7));
        tokens.answered(7);
        assert!(
            !tokens.asker_gone(7),
            "answered: no longer anyone's to ask about"
        );
    }

    #[test]
    fn a_ticket_opens_once_and_never_again() {
        let tickets = Tickets::default();
        let now = std::time::Instant::now();
        let ticket = tickets.mint(3, 1, now).expect("a ticket");

        assert_eq!(tickets.spend(3, 1, &ticket, now), Ok(()));
        assert_eq!(
            tickets.spend(3, 1, &ticket, now),
            Err(NO_TICKET.to_owned()),
            "a replay of the same line opens nothing"
        );
    }

    #[test]
    fn a_ticket_spends_only_on_the_connection_that_minted_it() {
        // A line copied onto a connection of its own is a replay, whatever it carries.
        let tickets = Tickets::default();
        let now = std::time::Instant::now();
        let ticket = tickets.mint(3, 1, now).expect("a ticket");

        assert_eq!(tickets.spend(3, 2, &ticket, now), Err(NO_TICKET.to_owned()));
    }

    #[test]
    fn a_ticket_spends_only_for_the_chat_it_was_minted_for() {
        let tickets = Tickets::default();
        let now = std::time::Instant::now();
        let ticket = tickets.mint(3, 1, now).expect("a ticket");

        assert_eq!(tickets.spend(4, 1, &ticket, now), Err(NO_TICKET.to_owned()));
        assert_eq!(
            tickets.spend(3, 1, &ticket, now),
            Ok(()),
            "3's is untouched"
        );
    }

    #[test]
    fn a_wrong_guess_spends_the_real_ticket_too() {
        // One try per mint. Without this a guesser could keep trying against a live ticket
        // for as long as it lived.
        let tickets = Tickets::default();
        let now = std::time::Instant::now();
        let ticket = tickets.mint(3, 1, now).expect("a ticket");

        assert!(tickets.spend(3, 1, "not-it", now).is_err());
        assert_eq!(tickets.spend(3, 1, &ticket, now), Err(NO_TICKET.to_owned()));
    }

    #[test]
    fn a_ticket_left_unspent_expires() {
        let tickets = Tickets::default();
        let now = std::time::Instant::now();
        let ticket = tickets.mint(3, 1, now).expect("a ticket");

        assert_eq!(
            tickets.spend(3, 1, &ticket, now + A_TICKET_LIVES),
            Err(NO_TICKET.to_owned())
        );
    }

    #[test]
    fn a_second_mint_for_a_chat_stands_beside_the_first_and_never_replaces_it() {
        // Replacing would let a second process cancel the handoff the chat is making,
        // silently. Each run of a command has its own ticket, on its own connection.
        let tickets = Tickets::default();
        let now = std::time::Instant::now();
        let first = tickets.mint(3, 1, now).expect("a ticket");
        let second = tickets.mint(3, 2, now).expect("a ticket of its own");

        assert_ne!(first, second);
        // Neither spends on the other's connection, and a try there costs the ticket that
        // connection holds, never the other's.
        assert_eq!(tickets.spend(3, 2, &first, now), Err(NO_TICKET.to_owned()));
        assert_eq!(tickets.spend(3, 2, &second, now), Err(NO_TICKET.to_owned()));
        assert_eq!(tickets.spend(3, 1, &first, now), Ok(()));
    }

    #[test]
    fn six_dispatches_at_once_from_one_chat_each_spend_a_ticket_of_their_own_once() {
        // Fan-out: one chat, six runs of the command, six connections, all minted before any
        // is spent. Every one spends, in any order, and none spends twice.
        let tickets = Tickets::default();
        let now = std::time::Instant::now();
        let minted: Vec<(u64, String)> = (1..=6)
            .map(|connection| {
                (
                    connection,
                    tickets.mint(3, connection, now).expect("a ticket"),
                )
            })
            .collect();
        let distinct: std::collections::HashSet<&String> =
            minted.iter().map(|(_, ticket)| ticket).collect();
        assert_eq!(distinct.len(), 6);

        for (connection, ticket) in minted.iter().rev() {
            assert_eq!(tickets.spend(3, *connection, ticket, now), Ok(()));
        }
        for (connection, ticket) in &minted {
            assert_eq!(
                tickets.spend(3, *connection, ticket, now),
                Err(NO_TICKET.to_owned()),
                "a replay opens nothing"
            );
        }
    }

    #[test]
    fn a_connection_holds_one_ticket_so_one_run_of_a_command_starts_one_chat() {
        let tickets = Tickets::default();
        let now = std::time::Instant::now();
        let first = tickets.mint(3, 1, now).expect("a ticket");

        let again = tickets
            .mint(3, 1, now)
            .expect_err("one ticket a connection");
        assert!(again.contains("already holds a ticket"), "{again}");
        assert_eq!(tickets.spend(3, 1, &first, now), Ok(()), "the first stands");
        assert!(
            tickets.mint(3, 1, now).is_ok(),
            "and once it is spent, the connection can mint again"
        );
    }

    #[test]
    fn a_chat_s_live_tickets_are_bounded_and_the_refusal_names_no_handoff() {
        // Abandoned mints cannot pile up without end, and the sentence is every ask's.
        let tickets = Tickets::default();
        let now = std::time::Instant::now();
        for connection in 0..MOST_LIVE_TICKETS_A_CHAT as u64 {
            tickets.mint(3, connection, now).expect("room");
        }

        let refused = tickets.mint(3, 999, now).expect_err("full");
        assert_eq!(
            refused,
            format!(
                "chat 3 has {MOST_LIVE_TICKETS_A_CHAT} requests to the app under way at once; \
                 try again in a few seconds"
            )
        );
        assert!(
            tickets.mint(4, 999, now).is_ok(),
            "another chat's are its own"
        );
        assert!(
            tickets.mint(3, 999, now + A_TICKET_LIVES).is_ok(),
            "and the abandoned ones expire"
        );
    }

    #[test]
    fn an_abandoned_ticket_never_holds_up_the_chat_s_next_ask() {
        let tickets = Tickets::default();
        let now = std::time::Instant::now();
        let _abandoned = tickets.mint(3, 1, now).expect("a ticket");

        assert!(tickets.mint(3, 2, now).is_ok());
    }

    #[test]
    fn two_tickets_are_never_the_same() {
        let tickets = Tickets::default();
        let now = std::time::Instant::now();
        let one = tickets.mint(3, 1, now).expect("a ticket");
        let two = tickets.mint(4, 1, now).expect("a ticket");

        assert_ne!(one, two);
        assert_eq!(one.len(), 64);
        assert!(one.bytes().all(|b| b.is_ascii_hexdigit()));
    }

    #[test]
    fn a_guess_as_long_as_the_ticket_or_a_prefix_of_it_opens_nothing() {
        // The comparison needs the lengths equal AND every byte equal. A guess of the right
        // length that differs in one byte, and the ticket cut short — whose every byte the
        // zip does compare equal — are both refused.
        let now = std::time::Instant::now();
        for guess in [
            |t: &str| {
                let mut g = t.as_bytes().to_vec();
                g[63] = if g[63] == b'0' { b'1' } else { b'0' };
                String::from_utf8(g).unwrap()
            },
            |t: &str| t[..32].to_owned(),
            |t: &str| format!("{t}0"),
            // Two bytes wrong by the same bit: differences are OR-ed together, so a second
            // one cannot cancel the first.
            |t: &str| {
                let mut g = t.as_bytes().to_vec();
                g[0] ^= 1;
                g[1] ^= 1;
                String::from_utf8(g).unwrap()
            },
        ] {
            let tickets = Tickets::default();
            let ticket = tickets.mint(3, 1, now).expect("a ticket");
            let wrong = guess(&ticket);
            assert_ne!(wrong, ticket);
            assert_eq!(
                tickets.spend(3, 1, &wrong, now),
                Err(NO_TICKET.to_owned()),
                "{wrong}"
            );
        }
    }

    /// A listener whose every ask is answered with `answer`.
    fn an_app_answering(
        path: &std::path::Path,
        within: &std::path::Path,
        answer: Answer,
    ) -> (Reading, ChatToken) {
        let listener = Listener::bind(within, path).expect("a socket");
        let token = listener.tokens().issue_to_this_process(3).expect("a token");
        let reading =
            listener.each_answering(Box::new(|_| {}), Box::new(move |_, _| answer.clone()));
        (reading, token)
    }

    #[test]
    fn an_answer_up_to_the_report_cap_is_read_whole_and_one_past_it_is_not() {
        let dir = tempfile::tempdir().expect("a directory");
        let within = std::time::Duration::from_secs(5);
        // `{"no":{"why":"…"}}` and its newline, sized to land exactly on the cap and one past.
        let frame = serde_json::to_vec(&Answer::No { why: String::new() })
            .unwrap()
            .len()
            + 1;
        let cap = usize::try_from(A_REPORT_IS_AT_MOST).unwrap();
        assert_eq!(cap, 65_536);

        let whole = Answer::No {
            why: "w".repeat(cap - frame),
        };
        let path = dir.path().join("whole.sock");
        let (_reading, token) = an_app_answering(&path, dir.path(), whole.clone());
        let got = Asking::on(&path, Some(token))
            .expect("connected")
            .ask(&Ask::Ticket { chat: 3 }, within)
            .expect("an answer exactly at the cap");
        assert_eq!(got, whole);

        let over = Answer::No {
            why: "w".repeat(cap - frame + 2),
        };
        let path = dir.path().join("over.sock");
        let (_reading, token) = an_app_answering(&path, dir.path(), over);
        assert!(
            Asking::on(&path, Some(token))
                .expect("connected")
                .ask(&Ask::Ticket { chat: 3 }, within)
                .is_err(),
            "an answer past the cap was read"
        );
    }

    #[test]
    fn an_open_carrying_the_largest_first_message_arrives_and_a_longer_line_does_not() {
        let dir = tempfile::tempdir().expect("a directory");
        let path = dir.path().join("hooks.sock");
        let (_reading, token) = an_app_at(&path, dir.path());
        let within = std::time::Duration::from_secs(5);

        // The worst case the cap was sized for: a whole first message of a character JSON
        // writes as six bytes.
        let mut asking = Asking::on(&path, Some(token.clone())).expect("connected");
        let ticket = ticket_from(&mut asking);
        let Ask::Open(mut open) = an_open(3, &ticket) else {
            unreachable!()
        };
        open.message = "\u{1b}".repeat(crate::handoff::FIRST_MESSAGE_MAX_BYTES);
        assert_eq!(
            asking
                .ask(&Ask::Open(open), within)
                .expect("the largest first message is read"),
            Answer::Opened {
                chat: 9,
                row: None,
                note: None,
            }
        );

        // A line longer than the cap is cut, and a cut line is no ask: the connection ends,
        // with a refusal said first where the asker is still there to read it (#1333).
        let mut asking = Asking::on(&path, Some(token.clone())).expect("connected");
        let ticket = ticket_from(&mut asking);
        let Ask::Open(mut open) = an_open(3, &ticket) else {
            unreachable!()
        };
        open.message = "m".repeat(usize::try_from(A_LINE_IS_AT_MOST).unwrap());
        let answered = asking.ask(&Ask::Open(open), within);
        assert!(
            matches!(&answered, Err(_) | Ok(Answer::No { .. })),
            "a line past the cap was read: {answered:?}"
        );
    }

    /// A listener at `path` whose answers are the ticket half and an open that always
    /// succeeds as chat 9.
    fn an_app_at(path: &std::path::Path, within: &std::path::Path) -> (Reading, ChatToken) {
        let listener = Listener::bind(within, path).expect("a socket");
        let token = listener.tokens().issue_to_this_process(3).expect("a token");
        let tickets = Tickets::default();
        let reading = listener.each_answering(
            Box::new(|_| {}),
            Box::new(move |connection, ask| {
                let now = std::time::Instant::now();
                // A dispatch and a report spend a ticket as an open does.
                match &ask {
                    Ask::Dispatch(dispatch) => {
                        let spent = tickets.spend(dispatch.chat, connection, &dispatch.ticket, now);
                        return match spent {
                            Ok(()) => Answer::Dispatched {
                                chat: 9,
                                name: dispatch.name.clone(),
                                persona: None,
                                note: None,
                                works: None,
                            },
                            Err(why) => Answer::No { why },
                        };
                    }
                    Ask::Report(back) => {
                        return match tickets.spend(back.chat, connection, &back.ticket, now) {
                            Ok(()) => Answer::Reported {
                                to: "steward 3".to_owned(),
                                kept_for: None,
                            },
                            Err(why) => Answer::No { why },
                        };
                    }
                    _ => {}
                }
                match ask {
                    Ask::Ticket { chat } => match tickets.mint(chat, connection, now) {
                        Ok(ticket) => Answer::Ticket { ticket },
                        Err(why) => Answer::No { why },
                    },
                    Ask::Open(open) => {
                        match tickets.spend(open.chat, connection, &open.ticket, now) {
                            Ok(()) => Answer::Opened {
                                chat: 9,
                                row: None,
                                note: None,
                            },
                            Err(why) => Answer::No { why },
                        }
                    }
                    Ask::Report(_)
                    | Ask::SessionRecord(_)
                    | Ask::Write(_)
                    | Ask::Git(_)
                    | Ask::Commit(_)
                    | Ask::Vaults { .. }
                    | Ask::WhereWorking(_)
                    | Ask::Dispatch(_)
                    | Ask::Task(_) => Answer::No {
                        why: "not here".to_owned(),
                    },
                }
            }),
        );
        (reading, token)
    }

    fn ticket_from(asking: &mut Asking) -> String {
        match asking
            .ask(&Ask::Ticket { chat: 3 }, std::time::Duration::from_secs(2))
            .expect("an answer")
        {
            Answer::Ticket { ticket } => ticket,
            other => panic!("a ticket, not {other:?}"),
        }
    }

    #[test]
    fn six_asks_at_once_from_one_chat_are_each_answered_on_a_ticket_of_their_own() {
        // #1441: a chat that fans out runs six commands in one step. Every one has minted
        // before any spends, which is the case one live ticket per chat used to refuse.
        let dir = tempfile::tempdir().expect("a directory");
        let path = dir.path().join("hooks.sock");
        let (_reading, token) = an_app_at(&path, dir.path());
        let all_minted = Arc::new(std::sync::Barrier::new(6));

        let asked: Vec<_> = (0..6)
            .map(|_| {
                let path = path.clone();
                let token = token.clone();
                let all_minted = Arc::clone(&all_minted);
                std::thread::spawn(move || {
                    let mut asking = Asking::on(&path, Some(token)).expect("connected");
                    let ticket = ticket_from(&mut asking);
                    all_minted.wait();
                    asking.ask(&an_open(3, &ticket), std::time::Duration::from_secs(5))
                })
            })
            .collect();

        for one in asked {
            let answered = one.join().expect("it ran").expect("an answer");
            assert!(
                matches!(answered, Answer::Opened { chat: 9, .. }),
                "{answered:?}"
            );
        }
    }

    #[test]
    fn a_mint_and_a_spend_on_one_connection_open_a_chat() {
        let dir = tempfile::tempdir().expect("a directory");
        let path = dir.path().join("hooks.sock");
        let (_reading, token) = an_app_at(&path, dir.path());

        let mut asking = Asking::on(&path, Some(token.clone())).expect("connected");
        let ticket = ticket_from(&mut asking);

        assert_eq!(
            asking
                .ask(&an_open(3, &ticket), std::time::Duration::from_secs(2))
                .expect("an answer"),
            Answer::Opened {
                chat: 9,
                row: None,
                note: None,
            }
        );
    }

    /// **The ticket's rule, over the socket, for an ask that spends one** (#1441): it is taken
    /// only on the connection that minted its ticket, and once; the same ask copied onto a
    /// connection that minted none is refused and costs the real one nothing; and a wrong
    /// guess burns the ticket of the connection it was made on, and no other's.
    ///
    /// A ticket used to be one a chat, and a copy elsewhere spent it. It is one a connection
    /// now, so a copy elsewhere has nothing to spend: it can neither use the ticket nor take
    /// it away from the run of the command that asked for it.
    fn the_ticket_is_its_connection_s_alone(ask: impl Fn(&str) -> Ask) {
        let dir = tempfile::tempdir().expect("a directory");
        let path = dir.path().join("hooks.sock");
        let (_reading, token) = an_app_at(&path, dir.path());
        let within = std::time::Duration::from_secs(2);
        let on = || Asking::on(&path, Some(token.clone())).expect("connected");
        let refused = Answer::No {
            why: NO_TICKET.to_owned(),
        };

        // A copy, on a connection that minted no ticket: refused.
        let mut asking = on();
        let ticket = ticket_from(&mut asking);
        let mut elsewhere = on();
        assert_eq!(
            elsewhere.ask(&ask(&ticket), within).expect("an answer"),
            refused,
            "a copy on another connection was taken"
        );
        // Even where that connection holds a live ticket of its own for the chat: it spends
        // its own on the wrong guess, and the copied one is not what it was given.
        let mut holding = on();
        let its_own = ticket_from(&mut holding);
        assert_eq!(
            holding.ask(&ask(&ticket), within).expect("an answer"),
            refused,
            "another connection's ticket was taken"
        );
        assert_eq!(
            holding.ask(&ask(&its_own), within).expect("an answer"),
            refused,
            "the wrong guess did not burn that connection's own ticket"
        );
        // Neither cost the minting connection anything: its ask is taken there, once.
        let taken = asking.ask(&ask(&ticket), within).expect("an answer");
        assert!(!matches!(taken, Answer::No { .. }), "{taken:?}");
        assert_eq!(
            asking.ask(&ask(&ticket), within).expect("an answer"),
            refused,
            "taken a second time"
        );
    }

    #[test]
    fn an_open_is_taken_only_on_the_connection_that_minted_its_ticket_and_only_once() {
        the_ticket_is_its_connection_s_alone(|ticket| an_open(3, ticket));
    }

    #[test]
    fn a_dispatch_is_taken_only_on_the_connection_that_minted_its_ticket_and_only_once() {
        the_ticket_is_its_connection_s_alone(|ticket| {
            Ask::Dispatch(Box::new(DispatchAsk {
                chat: 3,
                to: None,
                name: "check the queue".to_owned(),
                brief: "# Check the queue\nbody\n".to_owned(),
                profile: None,
                place: None,
                ticket: ticket.to_owned(),
            }))
        });
    }

    #[test]
    fn a_report_is_taken_only_on_the_connection_that_minted_its_ticket_and_only_once() {
        the_ticket_is_its_connection_s_alone(|ticket| {
            Ask::Report(Box::new(ReportBack {
                chat: 3,
                summary: "Forty are stuck.".to_owned(),
                ticket: ticket.to_owned(),
                task: None,
            }))
        });
    }

    #[test]
    fn a_listener_nobody_answers_for_refuses_an_ask_rather_than_going_quiet() {
        let dir = tempfile::tempdir().expect("a directory");
        let path = dir.path().join("hooks.sock");
        let listener = Listener::bind(dir.path(), &path).expect("a socket");
        let token = listener.tokens().issue_to_this_process(3).expect("a token");
        let _reading = listener.each(Box::new(|_| {}));

        let answer = Asking::on(&path, Some(token))
            .expect("connected")
            .ask(&Ask::Ticket { chat: 3 }, std::time::Duration::from_secs(2))
            .expect("an answer");

        assert_eq!(
            answer,
            Answer::No {
                why: NOTHING_ANSWERS.to_owned()
            }
        );
    }

    #[test]
    fn a_report_written_by_hand_is_still_a_report_on_a_listener_that_answers() {
        // The bytes a hook writes, with its chat's token beside them. The ask must not have
        // taken the report's place.
        use std::io::Write;

        let dir = tempfile::tempdir().expect("a directory");
        let path = dir.path().join("hooks.sock");
        let listener = Listener::bind(dir.path(), &path).expect("a socket");
        let token = listener.tokens().issue_to_this_process(7).expect("a token");
        let (tx, rx) = mpsc::channel();
        let tx = std::sync::Mutex::new(tx);
        let _reading = listener.each_answering(
            Box::new(move |report| tx.lock().unwrap().send(report).unwrap()),
            Box::new(|_, _| panic!("a report is not an ask")),
        );

        let mut socket = std::os::unix::net::UnixStream::connect(&path).expect("connected");
        socket
            .write_all(
                format!(
                    "{{\"chat\":7,\"event\":\"stop\",\"token\":\"{}\"}}\n",
                    token.expose()
                )
                .as_bytes(),
            )
            .expect("written");
        drop(socket);

        let report = rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .expect("the report arrives");
        assert_eq!(report.chat, 7);
        assert_eq!(report.event, Event::Stop);
    }

    #[test]
    fn asking_where_no_app_is_listening_fails_at_once() {
        let dir = tempfile::tempdir().expect("a directory");
        let started = std::time::Instant::now();

        assert!(Asking::on(&dir.path().join("gone.sock"), None).is_err());
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
    }

    // ------------------------------------------------------------------------------------
    // a harness started by hand in a shell tab (SI-5)
    // ------------------------------------------------------------------------------------

    fn by_hand() -> StartedByHand {
        StartedByHand {
            chat: 4,
            started_by_hand: "claude".to_owned(),
            cwd: Some(std::path::PathBuf::from("/work/alpha")),
        }
    }

    #[test]
    fn a_harness_started_by_hand_reaches_the_app_as_a_notice_and_never_as_a_report() {
        let dir = tempfile::tempdir().expect("a directory");
        let path = dir.path().join("hooks.sock");
        let listener = Listener::bind(dir.path(), &path).expect("a socket");
        let token = listener.tokens().issue_to_this_process(4).expect("a token");
        let (tx, rx) = mpsc::channel();
        let tx = std::sync::Mutex::new(tx);
        let _reading = listener.each_answering_and_noticing(
            Box::new(|_| panic!("a notice is not a report")),
            Box::new(|_, _| panic!("a notice is not an ask")),
            Box::new(move |notice| tx.lock().unwrap().send(notice).unwrap()),
        );

        tell(&path, Some(&token), &by_hand()).expect("the app took it");

        assert_eq!(
            rx.recv_timeout(std::time::Duration::from_secs(5)),
            Ok(by_hand())
        );
    }

    #[test]
    fn a_report_is_never_read_as_a_harness_started_by_hand() {
        let line = serde_json::to_string(&Report {
            chat: 4,
            event: Event::Stop,
            conversation: Conversation::Unknown,
            pid: None,
            agent: None,
            detail: Detail::default(),
        })
        .unwrap();

        assert!(matches!(
            serde_json::from_str::<Line>(&line),
            Ok(Line::Report(_))
        ));
        let line = serde_json::to_string(&by_hand()).unwrap();
        assert!(matches!(
            serde_json::from_str::<Line>(&line),
            Ok(Line::ByHand(_))
        ));
    }

    #[test]
    fn telling_an_app_that_is_not_listening_fails_at_once_rather_than_waiting() {
        let dir = tempfile::tempdir().expect("a directory");
        let started = std::time::Instant::now();

        assert!(tell(&dir.path().join("gone.sock"), None, &by_hand()).is_err());
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
    }
    // ------------------------------------------------------------------------------------
    // a session record saved: a Smart close's end (SI-8, ADR 0064)
    // ------------------------------------------------------------------------------------

    fn saved() -> SessionSaved {
        SessionSaved {
            chat: 4,
            session_saved: std::path::PathBuf::from(
                "/plane/workspaces/alpha/sessions/20260928-140312-ship-it.md",
            ),
        }
    }

    #[test]
    fn a_session_saved_reaches_the_app_as_its_own_line_and_never_as_a_report_an_ask_or_a_notice() {
        let dir = tempfile::tempdir().expect("a directory");
        let path = dir.path().join("hooks.sock");
        let listener = Listener::bind(dir.path(), &path).expect("a socket");
        let token = listener.tokens().issue_to_this_process(4).expect("a token");
        let (tx, rx) = mpsc::channel();
        let tx = std::sync::Mutex::new(tx);
        let _reading = listener.each_answering_noticing_and_saving(
            Box::new(|_| panic!("a session saved is not a report")),
            Box::new(|_, _| panic!("a session saved is not an ask")),
            Box::new(|_| panic!("a session saved is not a harness started by hand")),
            Box::new(move |saved| tx.lock().unwrap().send(saved).unwrap()),
        );

        tell_saved(&path, Some(&token), &saved()).expect("the app took it");

        assert_eq!(
            rx.recv_timeout(std::time::Duration::from_secs(5)),
            Ok(saved())
        );
    }

    #[test]
    fn no_other_line_is_ever_read_as_a_session_saved() {
        let report = serde_json::to_string(&Report {
            chat: 4,
            event: Event::Stop,
            conversation: Conversation::Unknown,
            pid: None,
            agent: None,
            detail: Detail::default(),
        })
        .unwrap();
        assert!(matches!(
            serde_json::from_str::<Line>(&report),
            Ok(Line::Report(_))
        ));
        let notice = serde_json::to_string(&by_hand()).unwrap();
        assert!(matches!(
            serde_json::from_str::<Line>(&notice),
            Ok(Line::ByHand(_))
        ));
        let ask = serde_json::to_string(&Ask::Ticket { chat: 4 }).unwrap();
        assert!(matches!(
            serde_json::from_str::<Line>(&ask),
            Ok(Line::Ask(_))
        ));
        let line = serde_json::to_string(&saved()).unwrap();
        assert!(matches!(
            serde_json::from_str::<Line>(&line),
            Ok(Line::Saved(_))
        ));
    }

    #[test]
    #[cfg(unix)]
    fn a_line_is_handed_on_after_every_line_whose_connection_arrived_before_it() {
        let turns = std::sync::Arc::new(InTurn::new());
        let (tx, rx) = mpsc::channel();
        let second = {
            let turns = std::sync::Arc::clone(&turns);
            let tx = tx.clone();
            std::thread::spawn(move || {
                turns.wait(2);
                tx.send(2).unwrap();
                turns.done(2);
            })
        };
        std::thread::sleep(std::time::Duration::from_millis(20));
        tx.send(1).unwrap();
        turns.done(1);
        second.join().unwrap();

        assert_eq!(rx.try_iter().collect::<Vec<_>>(), vec![1, 2]);
    }

    #[test]
    #[cfg(unix)]
    fn a_connection_that_never_says_anything_holds_the_next_one_only_so_long() {
        let turns = InTurn::new();
        let began = std::time::Instant::now();

        turns.wait(2);

        assert!(
            began.elapsed() < A_TURN_IS_WAITED_AT_MOST * 4,
            "{:?}",
            began.elapsed()
        );
    }

    #[test]
    #[cfg(unix)]
    fn a_notice_that_cannot_be_delivered_in_time_is_given_up_on_and_does_not_hold_the_hook() {
        let began = std::time::Instant::now();

        let gave_up = within(std::time::Duration::from_millis(50), || {
            std::thread::sleep(std::time::Duration::from_secs(5));
            Ok(())
        });

        assert!(gave_up.is_err());
        assert!(
            began.elapsed() < std::time::Duration::from_secs(2),
            "{:?}",
            began.elapsed()
        );
        assert!(within(std::time::Duration::from_secs(1), || Ok(())).is_ok());
    }

    #[test]
    fn a_vault_listing_is_an_ask_and_no_line_can_tag_a_vault_or_name_a_persona() {
        let read = read_line(r#"{"vaults":{"chat":7},"token":"t"}"#).expect("an ask");
        assert!(
            matches!(&read.0, Line::Ask(Ask::Vaults { chat: 7 })),
            "{read:?}"
        );
        assert_eq!(
            read.0.chat(),
            7,
            "its token is checked for the chat it names"
        );
        // The ask names a chat and nothing else: a persona beside it is not read.
        assert_eq!(
            serde_json::to_value(Ask::Vaults { chat: 7 }).unwrap(),
            serde_json::json!({"vaults": {"chat": 7}})
        );
        // No kind of line grants a vault or sets a persona: each of these is no line at all,
        // so the listener drops it unanswered.
        for forged in [
            r#"{"chat":7,"vault_grant":{"vault":"ops","persona":"steward"}}"#,
            r#"{"chat":7,"allow_vault":"ops","persona":"steward"}"#,
            r#"{"vault_grant":{"chat":7,"vault":"ops","persona":"steward"}}"#,
            r#"{"grant_vault":{"chat":7,"vault":"ops"}}"#,
            r#"{"persona":{"chat":7,"name":"ops"}}"#,
            r#"{"chat":7,"persona":"ops"}"#,
        ] {
            assert!(read_line(forged).is_none(), "{forged}");
        }
    }

    #[test]
    fn no_line_a_chat_sends_makes_widens_or_revokes_a_dispatch_grant() {
        // #1437: a dispatch grant is the person's, made by a press in the window. The socket
        // has no line for one, so a forged line that spells a grant, an Allow or a revoke is
        // either no line at all, or one of the kinds below, none of which carries a grant.
        let forged = [
            r#"{"dispatch_grant":{"asking":"steward","target":"devops","level":"project"}}"#,
            r#"{"chat":4,"dispatch_grant":{"asking":"steward","target":"devops","level":"you"}}"#,
            r#"{"allow_dispatch":{"chat":4,"id":1,"level":"project"}}"#,
            r#"{"chat":4,"allow_dispatch":{"id":1,"level":"chat"},"token":"t"}"#,
            r#"{"revoke_dispatch_grant":{"chat":4,"id":"you\u001fsteward\u001fdevops"}}"#,
            r#"{"keep_dispatch_blocked":{"chat":4,"id":1}}"#,
            // #1502: an Allow that ticks boxes and says what it was shown.
            r#"{"allow_dispatch":{"chat":4,"id":1,"level":"you","also":["qa","docs"],"shown":"0f"}}"#,
            r#"{"chat":4,"allow_dispatch":{"id":1,"level":"project","also":["*"],"shown":""},"token":"t"}"#,
            r#"{"wants":{"persona":"steward","names":["devops","qa"]}}"#,
            // #1503: never, its lifting, and "any persona", by each window command's name.
            r#"{"never_dispatch":{"chat":4,"id":1}}"#,
            r#"{"chat":4,"never_dispatch":{"id":1},"token":"t"}"#,
            r#"{"lift_dispatch_never":{"chat":4,"asking":"steward","target":"devops"}}"#,
            r#"{"allow_dispatch_to_any":{"chat":4,"asking":"steward","level":"project"}}"#,
            r#"{"chat":4,"allow_dispatch_to_any":{"asking":"steward","level":"you"},"token":"t"}"#,
            r#"{"revoke_dispatch_to_any":{"chat":4,"asking":"steward","level":"you"}}"#,
            r#"{"dispatch_standing":{"chat":4}}"#,
            r#"{"dispatch_never":[{"asking":"steward","target":"devops"}]}"#,
            r#"{"dispatch_any":["steward"]}"#,
            // #1504: Settings' table. Accept, Not on my machine, and a grant set aside given
            // back or removed, by each window command's name; and the keys it keeps.
            r#"{"accept_project_dispatch":{"chat":4,"asking":"steward","target":"*"}}"#,
            r#"{"chat":4,"accept_project_dispatch":{"asking":"steward","target":"devops"},"token":"t"}"#,
            r#"{"decline_project_dispatch":{"chat":4,"asking":"steward","target":"devops"}}"#,
            r#"{"give_back_dispatch":{"chat":4,"name":"devops"}}"#,
            r#"{"chat":4,"give_back_dispatch":{"name":"devops"},"token":"t"}"#,
            r#"{"remove_dormant_dispatch":{"chat":4,"asking":"steward","target":"devops","any":false}}"#,
            r#"{"dispatch_dormant":[{"asking":"steward","target":"*","any":true,"was":"steward"}]}"#,
            r#"{"dispatch_known":[{"name":"devops","hash":"","away":false}]}"#,
            r#"{"dispatch_accepted_aside":[{"said":"steward -> devops","was":"devops"}]}"#,
            r#"{"dispatch_declined":["steward -> devops"]}"#,
            r#"{"grant":{"chat":4,"what":"dispatch","target":"steward -> devops","level":"project"}}"#,
            // #1505: a grant limited to one workspace. The wider Allow, the change of a
            // grant's workspace, Accept and Not on my machine for a limited grant, by each
            // window command's name; and the keys it keeps.
            r#"{"allow_dispatch_anywhere":{"chat":4,"id":1,"level":"project"}}"#,
            r#"{"chat":4,"allow_dispatch_anywhere":{"id":1,"level":"you"},"token":"t"}"#,
            r#"{"set_dispatch_workspace":{"chat":4,"asking":"steward","target":"devops","level":"you","from":"runners","to":null}}"#,
            r#"{"chat":4,"set_dispatch_workspace":{"asking":"steward","target":"*","level":"project","from":null,"to":"runners"},"token":"t"}"#,
            r#"{"accept_project_dispatch_in":{"chat":4,"asking":"steward","target":"devops","workspace":"runners"}}"#,
            r#"{"decline_project_dispatch_in":{"chat":4,"asking":"steward","target":"devops","workspace":"runners"}}"#,
            r#"{"dispatch_mine_in":[{"asking":"steward","target":"devops","workspace":"runners","seen":0}]}"#,
            r#"{"dispatch_seen_in":[{"asking":"steward","target":"*","any":true,"workspace":"runners","seen":0}]}"#,
            r#"{"dispatch_workspaces":[{"name":"runners","gone":0}]}"#,
        ];
        for line in forged {
            assert!(read_line(line).is_none(), "read as a line: {line}");
        }
        // Riding on a line the listener does read, the forged fields are dropped with every
        // other field that line does not have: what is read is that line and nothing more.
        let riding = r#"{"chat":4,"event":"stop","dispatch_grant":{"asking":"steward","target":"devops","level":"project"},"allow_dispatch":{"id":1,"level":"project"}}"#;
        let (read, _) = read_line(riding).expect("a report");
        let Line::Report(report) = read else {
            panic!("not a report: {read:?}");
        };
        let kept = serde_json::to_string(&report).expect("written");
        assert!(!kept.contains("dispatch"), "{kept}");
        assert!(!kept.contains("devops"), "{kept}");
        // And an ask names a chat to open or a report to hand back, never a grant: no variant
        // of it reads from a line that only spells one.
        for line in forged {
            assert!(serde_json::from_str::<Ask>(line).is_err(), "an ask: {line}");
        }
    }

    #[test]
    fn a_brokered_secret_exec_is_its_own_line_and_names_the_chat_its_token_is_checked_for() {
        let ask = crate::secrets::brokered::Ask {
            chat: 9,
            secret_exec: crate::secrets::brokered::Wanted {
                vault: "devops".to_owned(),
                command: vec!["kubectl".to_owned(), "get".to_owned(), "pods".to_owned()],
                ..Default::default()
            },
        };
        let line = line_with(Some(&ChatToken::from("t")), &ask).unwrap();
        let (read, token) = read_line(std::str::from_utf8(&line).unwrap()).expect("a line");
        assert!(
            matches!(&read, Line::SecretExec(got) if *got == ask),
            "{read:?}"
        );
        assert_eq!(read.chat(), 9);
        assert_eq!(token.as_deref(), Some("t"));
        for other in [
            serde_json::to_string(&saved()).unwrap(),
            r#"{"chat":4,"event":"stop"}"#.to_owned(),
            r#"{"session_record":{"chat":4,"title":"t","body":"b"}}"#.to_owned(),
        ] {
            assert!(
                !matches!(
                    serde_json::from_str::<Line>(&other),
                    Ok(Line::SecretExec(_))
                ),
                "no other line reads as a brokered exec: {other}"
            );
        }
    }

    #[test]
    fn a_tool_call_is_its_own_line_and_is_handed_to_the_app_with_its_chats_token() {
        let call = ToolCall {
            chat: 4,
            tool_hook: "pretooluse".to_owned(),
            tool: Some("Bash".to_owned()),
            call: Some("toolu_01".to_owned()),
            args: Some("ab".repeat(32)),
            decision: Decision::Deny,
            rule: Some("no-force-push".to_owned()),
            hook_ms: 2,
            agent: None,
            at_ms: 0,
        };
        let line = serde_json::to_string(&call).unwrap();
        assert!(
            matches!(serde_json::from_str::<Line>(&line), Ok(Line::Tool(_))),
            "{line}"
        );
        for other in [
            serde_json::to_string(&saved()).unwrap(),
            r#"{"chat":4,"event":"stop"}"#.to_owned(),
        ] {
            assert!(
                !matches!(serde_json::from_str::<Line>(&other), Ok(Line::Tool(_))),
                "no older line reads as a tool call: {other}"
            );
        }

        let dir = tempfile::tempdir().expect("a directory");
        let path = dir.path().join("hooks.sock");
        let listener = Listener::bind(dir.path(), &path).expect("a socket");
        let token = listener.tokens().issue_to_this_process(4).expect("a token");
        let (tx, rx) = mpsc::channel();
        let tx = std::sync::Mutex::new(tx);
        let _reading = listener.hear(Hearing {
            secret_exec: Box::new(|_, _, writer| crate::secrets::brokered::not_answered(writer)),
            blocked: Box::new(|_| {}),
            doing: Box::new(|_| {}),
            touching: Box::new(|_| {}),
            each: Box::new(|_| panic!("no report was sent")),
            answer: Box::new(|_, _| panic!("no ask was sent")),
            noticed: Box::new(|_| panic!("no harness was started by hand")),
            saved: Box::new(|_| panic!("no record was saved")),
            refused: Box::new(|_| panic!("no commit was refused")),
            tool: Box::new(move |call| {
                tx.lock().unwrap().send(call).unwrap();
                Ok(())
            }),
            permission: Box::new(|_| None),
        });
        one_line_with_a_deadline(&path, None, &call).expect("the line is written");
        one_line_with_a_deadline(&path, Some(&token), &call).expect("the line is written");
        assert_eq!(rx.recv_timeout(std::time::Duration::from_secs(5)), Ok(call));
        assert!(
            rx.recv_timeout(std::time::Duration::from_millis(200))
                .is_err(),
            "the line without the chat's token was dropped"
        );
    }

    #[test]
    fn a_touched_path_is_its_own_line_handed_to_the_app_with_its_chats_token() {
        let touching = Touching {
            chat: 4,
            touching: "/w/branch/src/a.rs".to_owned(),
        };
        let line = serde_json::to_string(&touching).unwrap();
        assert!(
            matches!(serde_json::from_str::<Line>(&line), Ok(Line::Touching(_))),
            "{line}"
        );
        for other in [
            serde_json::to_string(&saved()).unwrap(),
            r#"{"chat":4,"event":"stop"}"#.to_owned(),
            r#"{"chat":4,"tool_hook":"pretooluse-read"}"#.to_owned(),
        ] {
            assert!(
                !matches!(serde_json::from_str::<Line>(&other), Ok(Line::Touching(_))),
                "no other line reads as a touch: {other}"
            );
        }

        let dir = tempfile::tempdir().expect("a directory");
        let path = dir.path().join("hooks.sock");
        let listener = Listener::bind(dir.path(), &path).expect("a socket");
        let token = listener.tokens().issue_to_this_process(4).expect("a token");
        let (tx, rx) = mpsc::channel();
        let tx = std::sync::Mutex::new(tx);
        let _reading = listener.hear(Hearing {
            secret_exec: Box::new(|_, _, writer| crate::secrets::brokered::not_answered(writer)),
            each: Box::new(|_| panic!("no report was sent")),
            answer: Box::new(|_, _| panic!("no ask was sent")),
            noticed: Box::new(|_| panic!("no harness was started by hand")),
            saved: Box::new(|_| panic!("no record was saved")),
            refused: Box::new(|_| panic!("no commit was refused")),
            tool: Box::new(|_| panic!("no tool call was sent")),
            blocked: Box::new(|_| {}),
            doing: Box::new(|_| {}),
            touching: Box::new(move |touching| tx.lock().unwrap().send(touching).unwrap()),
            permission: Box::new(|_| None),
        });
        touch(&path, None, &touching).expect("the line is written");
        touch(&path, Some(&token), &touching).expect("the line is written");
        assert_eq!(
            rx.recv_timeout(std::time::Duration::from_secs(5)),
            Ok(touching.clone())
        );
        assert!(
            rx.recv_timeout(std::time::Duration::from_millis(200))
                .is_err(),
            "the line without the chat's token was dropped"
        );
    }

    #[test]
    fn what_a_chat_is_doing_is_its_own_line_handed_to_the_app_with_its_chats_token() {
        use crate::doing::{Kind, Said};
        let doing = Doing {
            chat: 4,
            doing: Said::Began {
                kind: Kind::Command,
                name: Some("cargo".to_owned()),
            },
            agent: None,
        };
        let line = serde_json::to_string(&doing).unwrap();
        assert_eq!(
            line,
            r#"{"chat":4,"doing":{"is":"began","kind":"command","name":"cargo"}}"#
        );
        assert!(
            matches!(serde_json::from_str::<Line>(&line), Ok(Line::Doing(_))),
            "{line}"
        );
        for other in [
            serde_json::to_string(&saved()).unwrap(),
            r#"{"chat":4,"event":"stop"}"#.to_owned(),
            r#"{"chat":4,"tool_hook":"pretooluse"}"#.to_owned(),
            r#"{"chat":4,"touching":"/w/a"}"#.to_owned(),
            // A kind this build does not know is no line at all.
            r#"{"chat":4,"doing":{"is":"began","kind":"notice"}}"#.to_owned(),
            r#"{"chat":4,"doing":"needs you"}"#.to_owned(),
        ] {
            assert!(
                !matches!(serde_json::from_str::<Line>(&other), Ok(Line::Doing(_))),
                "no other line reads as this one: {other}"
            );
        }

        let dir = tempfile::tempdir().expect("a directory");
        let path = dir.path().join("hooks.sock");
        let listener = Listener::bind(dir.path(), &path).expect("a socket");
        let token = listener.tokens().issue_to_this_process(4).expect("a token");
        let other = listener.tokens().issue_to_this_process(5).expect("a token");
        let (tx, rx) = mpsc::channel();
        let tx = std::sync::Mutex::new(tx);
        let _reading = listener.hear(Hearing {
            secret_exec: Box::new(|_, _, writer| crate::secrets::brokered::not_answered(writer)),
            each: Box::new(|_| panic!("no report was sent")),
            answer: Box::new(|_, _| panic!("no ask was sent")),
            noticed: Box::new(|_| panic!("no harness was started by hand")),
            saved: Box::new(|_| panic!("no record was saved")),
            refused: Box::new(|_| panic!("no commit was refused")),
            tool: Box::new(|_| panic!("no tool call was sent")),
            touching: Box::new(|_| panic!("no file was touched")),
            blocked: Box::new(|_| panic!("nothing was blocked")),
            doing: Box::new(move |doing| tx.lock().unwrap().send(doing).unwrap()),
            permission: Box::new(|_| None),
        });
        // Forged, with every field set: no token, a token nobody was issued, another chat's
        // token, and this chat's own token on a line that names another chat.
        let of_another = Doing {
            chat: 5,
            agent: Some("agent-1".to_owned()),
            ..doing.clone()
        };
        tell_doing(&path, None, &doing).expect("the line is written");
        tell_doing(&path, Some(&ChatToken::from("not-a-token")), &doing)
            .expect("the line is written");
        tell_doing(&path, Some(&other), &doing).expect("the line is written");
        tell_doing(&path, Some(&token), &of_another).expect("the line is written");
        assert!(
            rx.recv_timeout(std::time::Duration::from_secs(1)).is_err(),
            "a line without its own chat's token was dropped"
        );
        tell_doing(&path, Some(&token), &doing).expect("the line is written");
        assert_eq!(
            rx.recv_timeout(std::time::Duration::from_secs(5)),
            Ok(doing.clone())
        );
        assert!(
            rx.recv_timeout(std::time::Duration::from_millis(200))
                .is_err(),
            "and it was heard once"
        );
    }

    #[test]
    fn what_a_chat_is_doing_is_lost_when_no_app_takes_it_and_never_spooled() {
        let dir = tempfile::tempdir().expect("a directory");
        let path = dir.path().join("hooks.sock");
        let doing = Doing {
            chat: 4,
            doing: crate::doing::Said::Began {
                kind: crate::doing::Kind::Editing,
                name: Some("CANARY-doing.rs".to_owned()),
            },
            agent: None,
        };
        assert!(tell_doing(&path, Some(&ChatToken::from("t")), &doing).is_err());
        let written = walkdir_all(dir.path());
        assert!(written.is_empty(), "nothing was written: {written:?}");
    }

    #[test]
    fn a_sandbox_block_is_its_own_line_handed_to_the_app_with_its_chats_token() {
        use crate::sandboxblock::{Block, Kind, Operation};
        let blocked = SandboxBlocked {
            chat: 4,
            sandbox_blocked: Block {
                operation: Operation::Write,
                kind: Kind::ProjectFiles,
                ours: true,
            },
            harness: Some("claude".to_owned()),
            target: None,
        };
        let line = serde_json::to_string(&blocked).unwrap();
        assert_eq!(
            line,
            r#"{"chat":4,"sandbox_blocked":{"operation":"write","kind":"project-files","ours":true},"harness":"claude"}"#
        );
        assert!(
            matches!(serde_json::from_str::<Line>(&line), Ok(Line::Blocked(_))),
            "{line}"
        );
        for other in [
            serde_json::to_string(&saved()).unwrap(),
            r#"{"chat":4,"event":"stop"}"#.to_owned(),
            r#"{"chat":4,"tool_hook":"posttooluse-blocked"}"#.to_owned(),
            r#"{"chat":4,"touching":"/w/a"}"#.to_owned(),
        ] {
            assert!(
                !matches!(serde_json::from_str::<Line>(&other), Ok(Line::Blocked(_))),
                "no other line reads as a block: {other}"
            );
        }

        let dir = tempfile::tempdir().expect("a directory");
        let path = dir.path().join("hooks.sock");
        let listener = Listener::bind(dir.path(), &path).expect("a socket");
        let token = listener.tokens().issue_to_this_process(4).expect("a token");
        let (tx, rx) = mpsc::channel();
        let tx = std::sync::Mutex::new(tx);
        let _reading = listener.hear(Hearing {
            secret_exec: Box::new(|_, _, writer| crate::secrets::brokered::not_answered(writer)),
            each: Box::new(|_| panic!("no report was sent")),
            answer: Box::new(|_, _| panic!("no ask was sent")),
            noticed: Box::new(|_| panic!("no harness was started by hand")),
            saved: Box::new(|_| panic!("no record was saved")),
            refused: Box::new(|_| panic!("no commit was refused")),
            tool: Box::new(|_| panic!("no tool call was sent")),
            doing: Box::new(|_| {}),
            touching: Box::new(|_| panic!("no file was touched")),
            blocked: Box::new(move |blocked| tx.lock().unwrap().send(blocked).unwrap()),
            permission: Box::new(|_| None),
        });
        tell_blocked(&path, None, &blocked).expect("the line is written");
        tell_blocked(&path, Some(&token), &blocked).expect("the line is written");
        assert_eq!(
            rx.recv_timeout(std::time::Duration::from_secs(5)),
            Ok(blocked.clone())
        );
        assert!(
            rx.recv_timeout(std::time::Duration::from_millis(200))
                .is_err(),
            "the line without the chat's token was dropped"
        );
    }

    #[test]
    fn a_touch_no_app_takes_is_lost_and_never_spooled() {
        let dir = tempfile::tempdir().expect("a directory");
        let path = dir.path().join("hooks.sock");
        let touching = Touching {
            chat: 4,
            touching: "/w/branch/CANARY-touched.rs".to_owned(),
        };
        assert!(touch(&path, Some(&ChatToken::from("t")), &touching).is_err());
        let mut written = Vec::new();
        for entry in walkdir_all(dir.path()) {
            written.push(entry.display().to_string());
            let text = std::fs::read(&entry).unwrap_or_default();
            assert!(
                !String::from_utf8_lossy(&text).contains("CANARY-touched"),
                "{} holds the touched path",
                entry.display()
            );
        }
        assert!(written.is_empty(), "nothing was written: {written:?}");
    }

    fn walkdir_all(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
        let mut out = Vec::new();
        for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                out.extend(walkdir_all(&path));
            } else {
                out.push(path);
            }
        }
        out
    }

    #[test]
    fn a_refused_commit_is_its_own_line_and_is_handed_to_the_app_with_its_chats_token() {
        let refused = CommitRefused {
            chat: 4,
            commit_refused: "app: app.py:2  an email address  ad****".to_owned(),
        };
        let line = serde_json::to_string(&refused).unwrap();
        assert!(matches!(
            serde_json::from_str::<Line>(&line),
            Ok(Line::Refused(_))
        ));
        let line = serde_json::to_string(&saved()).unwrap();
        assert!(matches!(
            serde_json::from_str::<Line>(&line),
            Ok(Line::Saved(_))
        ));

        let dir = tempfile::tempdir().expect("a directory");
        let path = dir.path().join("hooks.sock");
        let listener = Listener::bind(dir.path(), &path).expect("a socket");
        let token = listener.tokens().issue_to_this_process(4).expect("a token");
        let (tx, rx) = mpsc::channel();
        let tx = std::sync::Mutex::new(tx);
        let _reading = listener.hear(Hearing {
            secret_exec: Box::new(|_, _, writer| crate::secrets::brokered::not_answered(writer)),
            blocked: Box::new(|_| {}),
            doing: Box::new(|_| {}),
            touching: Box::new(|_| {}),
            each: Box::new(|_| panic!("no report was sent")),
            answer: Box::new(|_, _| panic!("no ask was sent")),
            noticed: Box::new(|_| panic!("no harness was started by hand")),
            saved: Box::new(|_| panic!("no record was saved")),
            refused: Box::new(move |refused| {
                tx.lock().unwrap().send(refused).unwrap();
                Ok(())
            }),
            tool: Box::new(|_| panic!("no tool hook ran")),
            permission: Box::new(|_| None),
        });
        // Without the token it is dropped, as every line is.
        one_line_with_a_deadline(&path, None, &refused).expect("the line is written");
        one_line_with_a_deadline(&path, Some(&token), &refused).expect("the line is written");
        assert_eq!(
            rx.recv_timeout(std::time::Duration::from_secs(5)),
            Ok(refused)
        );
        assert!(
            rx.recv_timeout(std::time::Duration::from_millis(200))
                .is_err()
        );
    }

    #[test]
    fn an_app_that_hears_no_session_saved_still_takes_every_other_line() {
        let dir = tempfile::tempdir().expect("a directory");
        let path = dir.path().join("hooks.sock");
        let listener = Listener::bind(dir.path(), &path).expect("a socket");
        let saving = listener.tokens().issue_to_this_process(4).expect("a token");
        let reporting = listener.tokens().issue_to_this_process(9).expect("a token");
        let (tx, rx) = mpsc::channel();
        let tx = std::sync::Mutex::new(tx);
        // What the app calls today: no listener for a saved record at all.
        let _reading = listener.each_answering_and_noticing(
            Box::new(move |report| tx.lock().unwrap().send(report.chat).unwrap()),
            Box::new(|_, _| panic!("no ask was sent")),
            Box::new(|_| panic!("no harness was started by hand")),
        );

        tell_saved(&path, Some(&saving), &saved()).expect("the app took it");
        send(
            &path,
            Some(&reporting),
            &Report {
                chat: 9,
                event: Event::Stop,
                conversation: Conversation::Unknown,
                pid: None,
                agent: None,
                detail: Detail::default(),
            },
        )
        .expect("the app took it");

        assert_eq!(rx.recv_timeout(std::time::Duration::from_secs(5)), Ok(9));
    }

    #[test]
    fn telling_a_session_saved_where_no_app_is_listening_fails_at_once() {
        let dir = tempfile::tempdir().expect("a directory");
        let started = std::time::Instant::now();

        assert!(tell_saved(&dir.path().join("gone.sock"), None, &saved()).is_err());
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
    }

    // ------------------------------------------------------------------------------------
    // each chat's token: the channel checks it on every line
    // ------------------------------------------------------------------------------------

    fn a_stop(chat: u32) -> Report {
        Report {
            chat,
            event: Event::Stop,
            conversation: Conversation::Unknown,
            pid: None,
            agent: None,
            detail: Detail::default(),
        }
    }

    #[test]
    fn a_line_carrying_its_chats_token_is_read_and_the_token_goes_no_further() {
        let dir = tempfile::tempdir().expect("a directory");
        let path = dir.path().join("hooks.sock");
        let listener = Listener::bind(dir.path(), &path).expect("a socket");
        let token = listener.tokens().issue_to_this_process(7).expect("a token");
        let (tx, rx) = mpsc::channel();
        let _reading = listener.each(Box::new(move |report| {
            let _ = tx.send(report);
        }));

        send(&path, Some(&token), &a_stop(7)).expect("the app took it");

        let heard = rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("the report arrives");
        assert_eq!(heard, a_stop(7));
        // What the app is handed holds no token, however it is written out.
        let written = serde_json::to_string(&heard).unwrap();
        assert!(!written.contains(token.expose()), "{written}");
        assert!(!format!("{heard:?}").contains(token.expose()));
    }

    #[test]
    fn a_report_with_no_token_a_wrong_one_or_another_chats_is_dropped() {
        let dir = tempfile::tempdir().expect("a directory");
        let path = dir.path().join("hooks.sock");
        let listener = Listener::bind(dir.path(), &path).expect("a socket");
        let seven = listener.tokens().issue_to_this_process(7).expect("a token");
        let eight = listener.tokens().issue_to_this_process(8).expect("a token");
        let (tx, rx) = mpsc::channel();
        let _reading = listener.each(Box::new(move |report| {
            let _ = tx.send(report.chat);
        }));

        let wrong = ChatToken::from("0".repeat(seven.expose().len()).as_str());
        for token in [None, Some(&wrong), Some(&eight)] {
            send(&path, token, &a_stop(7)).expect("written");
        }
        // And one line with the token of a chat that has none of its own.
        send(&path, Some(&seven), &a_stop(9)).expect("written");

        assert_eq!(
            rx.recv_timeout(std::time::Duration::from_secs(2)),
            Err(mpsc::RecvTimeoutError::Timeout),
            "a line without its chat's token was read"
        );
        // The channel is still open to the chat's own.
        send(&path, Some(&eight), &a_stop(8)).expect("written");
        assert_eq!(rx.recv_timeout(std::time::Duration::from_secs(5)), Ok(8));
    }

    #[test]
    fn an_ask_without_its_chats_token_is_refused_unanswered_and_ends_at_once() {
        let dir = tempfile::tempdir().expect("a directory");
        let path = dir.path().join("hooks.sock");
        let listener = Listener::bind(dir.path(), &path).expect("a socket");
        let three = listener.tokens().issue_to_this_process(3).expect("a token");
        let four = listener.tokens().issue_to_this_process(4).expect("a token");
        let asked = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counted = Arc::clone(&asked);
        let _reading = listener.each_answering(
            Box::new(|_| {}),
            Box::new(move |_, _| {
                counted.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Answer::Ticket {
                    ticket: "t".to_owned(),
                }
            }),
        );
        let within = std::time::Duration::from_secs(5);

        for token in [None, Some(ChatToken::from("nope")), Some(four)] {
            let began = std::time::Instant::now();
            let answered = Asking::on(&path, token)
                .expect("connected")
                .ask(&Ask::Ticket { chat: 3 }, within);
            // Refused, and said so (#1333): never answered as the chat it names.
            assert!(
                matches!(&answered, Ok(Answer::No { why }) if why.contains("token")),
                "answered {answered:?}"
            );
            assert!(began.elapsed() < within / 2, "it waited for a deadline");
        }
        assert_eq!(asked.load(std::sync::atomic::Ordering::SeqCst), 0);

        let answered = Asking::on(&path, Some(three))
            .expect("connected")
            .ask(&Ask::Ticket { chat: 3 }, within);
        assert!(
            matches!(answered, Ok(Answer::Ticket { .. })),
            "{answered:?}"
        );
    }

    #[test]
    fn a_notice_or_a_saved_record_without_its_chats_token_is_dropped() {
        let dir = tempfile::tempdir().expect("a directory");
        let path = dir.path().join("hooks.sock");
        let listener = Listener::bind(dir.path(), &path).expect("a socket");
        let four = listener.tokens().issue_to_this_process(4).expect("a token");
        let other = listener.tokens().issue_to_this_process(5).expect("a token");
        let (tx, rx) = mpsc::channel();
        let noticed = std::sync::Mutex::new(tx.clone());
        let saving = std::sync::Mutex::new(tx);
        let _reading = listener.each_answering_noticing_and_saving(
            Box::new(|_| {}),
            Box::new(|_, _| Answer::No { why: String::new() }),
            Box::new(move |_| noticed.lock().unwrap().send("notice").unwrap()),
            Box::new(move |_| saving.lock().unwrap().send("saved").unwrap()),
        );

        for token in [None, Some(&other)] {
            tell(&path, token, &by_hand()).expect("written");
            tell_saved(&path, token, &saved()).expect("written");
        }
        assert_eq!(
            rx.recv_timeout(std::time::Duration::from_secs(2)),
            Err(mpsc::RecvTimeoutError::Timeout)
        );

        tell(&path, Some(&four), &by_hand()).expect("written");
        assert_eq!(
            rx.recv_timeout(std::time::Duration::from_secs(5)),
            Ok("notice")
        );
    }

    #[test]
    fn a_connection_from_another_uid_is_refused_before_it_says_anything_even_with_its_token() {
        // FD-6: the hook channel checks the peer's uid as well as the chat's token. The
        // listener is told it belongs to a uid this test is not, so this test's own
        // connection is the other user's: it carries the right token, and is still dropped.
        let dir = tempfile::tempdir().expect("a directory");
        let path = dir.path().join("hooks.sock");
        let ours = purlis_same_user::Uid::effective();
        let listener = Listener::bind(dir.path(), &path)
            .expect("a socket")
            .owned_by(purlis_same_user::Uid::from_raw(
                ours.as_raw().wrapping_add(1),
            ));
        let seven = listener.tokens().issue_to_this_process(7).expect("a token");
        let (tx, rx) = mpsc::channel();
        let _reading = listener.each(Box::new(move |report| {
            let _ = tx.send(report.chat);
        }));

        let _ = send(&path, Some(&seven), &a_stop(7));

        assert_eq!(
            rx.recv_timeout(std::time::Duration::from_secs(2)),
            Err(mpsc::RecvTimeoutError::Timeout),
            "a line from another uid was read"
        );
    }

    #[test]
    fn refusals_are_said_once_per_interval_and_the_rest_are_counted() {
        let refusals = RateLimited::new();
        assert_eq!(refusals.say().as_deref(), Some(""));
        for _ in 0..1000 {
            assert_eq!(refusals.say(), None);
        }
        // Past the interval the next one is said, with the count of the ones that were not.
        refusals.state.lock().unwrap().0 =
            std::time::Instant::now().checked_sub(A_REFUSAL_IS_SAID_EVERY);
        assert_eq!(
            refusals.say().as_deref(),
            Some(" (1000 more refused since the last line)")
        );
    }

    #[test]
    fn a_connection_from_the_listeners_own_uid_with_its_token_is_read() {
        let dir = tempfile::tempdir().expect("a directory");
        let path = dir.path().join("hooks.sock");
        let listener = Listener::bind(dir.path(), &path)
            .expect("a socket")
            .owned_by(purlis_same_user::Uid::effective());
        let seven = listener.tokens().issue_to_this_process(7).expect("a token");
        let (tx, rx) = mpsc::channel();
        let _reading = listener.each(Box::new(move |report| {
            let _ = tx.send(report.chat);
        }));

        send(&path, Some(&seven), &a_stop(7)).expect("the app took it");

        assert_eq!(rx.recv_timeout(std::time::Duration::from_secs(5)), Ok(7));
    }

    fn program(pid: u32) -> Program {
        Program {
            pid,
            started: "then".to_owned(),
        }
    }

    #[test]
    fn a_line_with_a_chats_token_is_read_only_from_inside_that_chat() {
        let tokens = ChatTokens::default();
        let token = tokens.issue(5).expect("a token");
        let inside = |peer: &u32, program: &Program| *peer == program.pid || *peer == 101;
        let of = |peer| tokens.admission_by(5, Some(token.expose()), peer, &inside);
        assert_eq!(
            of(Some(100)),
            Admission::Unbound,
            "no program is recorded for the chat"
        );
        tokens.bind(5, program(100));
        assert_eq!(of(Some(100)), Admission::Admitted);
        assert_eq!(
            of(Some(101)),
            Admission::Admitted,
            "a process inside the chat"
        );
        assert_eq!(
            of(Some(200)),
            Admission::Outside,
            "the chat's token, read by a process outside it"
        );
        assert_eq!(of(None), Admission::Outside);
        assert_eq!(
            tokens.admission_by(5, Some("not-it"), Some(100), &inside),
            Admission::NoToken,
            "a wrong token is dropped, not answered"
        );
        tokens.issue(5).expect("a new token");
        assert_eq!(
            tokens.admission_by(5, Some(token.expose()), Some(100), &inside),
            Admission::NoToken,
            "a new start's token is its own"
        );
    }

    #[test]
    fn a_refusal_names_its_own_cause() {
        assert_eq!(Admission::Outside.refusal(), Some(OUTSIDE_THE_CHAT));
        assert_eq!(
            Admission::Unbound.refusal(),
            Some(UNCONFIRMED_PROGRAM),
            "an unconfirmed program is the chat's start, not tmux or nohup"
        );
        assert!(!UNCONFIRMED_PROGRAM.contains("tmux"));
        assert_eq!(
            Admission::NoToken.refusal(),
            None,
            "a wrong token gets silence"
        );
        assert_eq!(Admission::Admitted.refusal(), None);
    }

    fn seen(pid: u32, started: &str, around: &[(u32, Option<&str>)]) -> Seen {
        Seen {
            pid,
            started: started.to_owned(),
            around: around
                .iter()
                .map(|(pid, started)| (*pid, started.map(str::to_owned)))
                .collect(),
        }
    }

    #[test]
    fn a_sender_is_judged_by_what_it_was_when_it_connected() {
        let program = program(100);
        assert!(
            seen(100, "then", &[]).inside(&program),
            "the program itself"
        );
        assert!(
            seen(205, "now", &[(100, Some("then"))]).inside(&program),
            "below it, or in its session"
        );
        assert!(
            !seen(205, "now", &[(100, Some("before"))]).inside(&program),
            "below a process that had the program's number at another time"
        );
        assert!(
            !seen(205, "now", &[(100, None)]).inside(&program),
            "below a process whose start could not be read"
        );
        assert!(
            !seen(100, "another time", &[]).inside(&program),
            "the program's number, started at another time"
        );
    }

    /// Run by [`a_sender_that_writes_and_exits_at_once_is_still_heard`] in a process of its
    /// own, which tells the app and then ends, as `purlis session save` and a shell's guard do.
    #[test]
    #[ignore = "run in a child by a_sender_that_writes_and_exits_at_once_is_still_heard"]
    fn a_sender_that_writes_and_exits() {
        let (Some(path), Some(token)) = (
            crate::envvar::var_os("PURLIS_TEST_HOOK_SOCKET"),
            crate::envvar::var("PURLIS_TEST_HOOK_TOKEN"),
        ) else {
            return;
        };
        let notice = StartedByHand {
            chat: 12,
            started_by_hand: "claude".to_owned(),
            cwd: None,
        };
        let token = ChatToken::from(token.as_str());
        if crate::envvar::var("PURLIS_TEST_HOOK_RAW").is_some() {
            // A sender that does not wait for the app: its line written, its connection
            // closed, and it stays a moment longer only as any process would on its way out.
            use std::io::Write;
            if let Ok(mut socket) = std::os::unix::net::UnixStream::connect(&path)
                && let Ok(line) = line_with(Some(&token), &notice)
            {
                let _ = socket.write_all(&line);
            }
            std::thread::sleep(std::time::Duration::from_millis(500));
            return;
        }
        let _ = tell(std::path::Path::new(&path), Some(&token), &notice);
    }

    #[test]
    fn a_sender_that_writes_and_exits_at_once_is_still_heard() {
        let dir = tempfile::tempdir().expect("a directory");
        let path = dir.path().join("hooks.sock");
        let listener = Listener::bind(dir.path(), &path).expect("a socket");
        let token = listener
            .tokens()
            .issue_to_this_process(12)
            .expect("a token");
        let (tx, rx) = mpsc::channel();
        let tx = std::sync::Mutex::new(tx);
        let _reading = listener.each_answering_and_noticing(
            Box::new(|_| panic!("a notice is not a report")),
            Box::new(|_, _| panic!("a notice is not an ask")),
            Box::new(move |notice| tx.lock().unwrap().send(notice).unwrap()),
        );
        let ran = crate::forklock::output(
            std::process::Command::new(std::env::current_exe().expect("this test binary"))
                .args([
                    "--exact",
                    "hookwire::tests::a_sender_that_writes_and_exits",
                    "--ignored",
                    "--quiet",
                ])
                .env("PURLIS_TEST_HOOK_SOCKET", &path)
                .env("PURLIS_TEST_HOOK_TOKEN", token.expose()),
        )
        .expect("the sender runs");
        assert!(ran.status.success(), "{ran:?}");
        let heard = rx.recv_timeout(std::time::Duration::from_secs(5));
        assert_eq!(
            heard.map(|notice| notice.chat),
            Ok(12),
            "the line of a sender that has already exited"
        );
    }

    #[test]
    fn a_sender_that_does_not_wait_is_heard_when_it_is_still_running_as_its_line_is_read() {
        let dir = tempfile::tempdir().expect("a directory");
        let path = dir.path().join("hooks.sock");
        let listener = Listener::bind(dir.path(), &path).expect("a socket");
        let token = listener
            .tokens()
            .issue_to_this_process(12)
            .expect("a token");
        let (tx, rx) = mpsc::channel();
        let tx = std::sync::Mutex::new(tx);
        let _reading = listener.each_answering_and_noticing(
            Box::new(|_| panic!("a notice is not a report")),
            Box::new(|_, _| panic!("a notice is not an ask")),
            Box::new(move |notice| tx.lock().unwrap().send(notice).unwrap()),
        );
        let ran = crate::forklock::output(
            std::process::Command::new(std::env::current_exe().expect("this test binary"))
                .args([
                    "--exact",
                    "hookwire::tests::a_sender_that_writes_and_exits",
                    "--ignored",
                    "--quiet",
                ])
                .env("PURLIS_TEST_HOOK_SOCKET", &path)
                .env("PURLIS_TEST_HOOK_TOKEN", token.expose())
                .env("PURLIS_TEST_HOOK_RAW", "1"),
        )
        .expect("the sender runs");
        assert!(ran.status.success(), "{ran:?}");
        let heard = rx.recv_timeout(std::time::Duration::from_secs(5));
        assert_eq!(
            heard.map(|notice| notice.chat),
            Ok(12),
            "the line of a sender that did not wait, read while it still ran"
        );
    }

    #[test]
    fn a_chat_whose_program_ended_speaks_for_nothing_and_a_late_end_changes_nothing() {
        let tokens = ChatTokens::default();
        let inside = |peer: &u32, program: &Program| *peer == program.pid;
        let first = tokens.issue(5).expect("a token");
        tokens.bind(5, program(100));
        tokens.program_ended(5, 100);
        assert_eq!(
            tokens.admission_by(5, Some(first.expose()), Some(100), &inside),
            Admission::NoToken,
            "a process later given the program's number"
        );
        // Started again under the same number: the first program's end, heard late, is not
        // the second's.
        let second = tokens.issue(5).expect("a token");
        tokens.bind(5, program(300));
        tokens.program_ended(5, 100);
        assert_eq!(
            tokens.admission_by(5, Some(second.expose()), Some(300), &inside),
            Admission::Admitted
        );
    }

    #[test]
    fn a_process_is_inside_the_chat_it_descends_from_and_not_one_beside_it() {
        let me = Program::of(std::process::id()).expect("this process, read");
        let mut child = crate::forklock::spawn(
            std::process::Command::new("/bin/sleep")
                .arg("5")
                .stdin(std::process::Stdio::null()),
        )
        .expect("a child");
        let them = Program::of(child.id()).expect("the child, read");
        assert!(descends(me.pid, &me));
        assert!(
            descends(child.id(), &me),
            "a child of this process is inside it"
        );
        assert!(
            !descends(me.pid, &them),
            "this process is not inside its own child"
        );
        let reused = Program {
            pid: me.pid,
            started: "a process that had this number before".to_owned(),
        };
        assert!(
            !descends(me.pid, &reused),
            "the same number, started at another time, is another process"
        );
        let _ = child.kill();
        let _ = child.wait();
    }

    /// A chat whose program is a process this one is not inside: a sleeping child.
    fn a_chat_elsewhere(tokens: &ChatTokens, chat: u32) -> (ChatToken, std::process::Child) {
        let child = crate::forklock::spawn(
            std::process::Command::new("/bin/sleep")
                .arg("30")
                .stdin(std::process::Stdio::null()),
        )
        .expect("a child");
        let token = tokens.issue(chat).expect("a token");
        tokens.bind(chat, Program::of(child.id()).expect("the child, read"));
        (token, child)
    }

    #[test]
    fn a_report_with_its_chats_token_from_outside_the_chat_is_refused_with_why_and_not_spooled() {
        let dir = tempfile::tempdir().expect("a directory");
        // Where a project's socket is, so a line the host did not take would be spooled.
        let app = dir.path().join(".purlis/app");
        std::fs::create_dir_all(&app).expect("the state folder");
        let path = app.join("hooks.sock");
        assert!(
            spool::covered(&spool::dir_for(&path)),
            "a spool here is one hooks write"
        );
        let listener = Listener::bind(dir.path(), &path).expect("a socket");
        let (token, mut child) = a_chat_elsewhere(&listener.tokens(), 7);
        let _reading = listener.each(Box::new(|_| panic!("no report is read")));
        let report = Report {
            chat: 7,
            event: Event::Notification,
            conversation: Conversation::Named("abc".to_owned()),
            pid: Some(99),
            agent: None,
            detail: Detail::default(),
        };
        let refused = deliver_report(&path, Some(&token), &report).expect_err("refused");
        assert_eq!(refused.to_string(), OUTSIDE_THE_CHAT);
        assert!(
            !spool::dir_for(&path).join("7").exists(),
            "nothing spooled as if the app were away"
        );
        let mut asking = Asking::on(&path, Some(token.clone())).expect("connects");
        assert_eq!(
            asking
                .ask(&Ask::Ticket { chat: 7 }, std::time::Duration::from_secs(5))
                .expect("an answer"),
            Answer::No {
                why: OUTSIDE_THE_CHAT.to_owned()
            }
        );
        let _ = child.kill();
        let _ = child.wait();
    }

    #[test]
    fn a_line_dripped_past_its_deadline_is_never_read() {
        use std::io::Write;
        let dir = tempfile::tempdir().expect("a directory");
        let path = dir.path().join("hooks.sock");
        let listener = Listener::bind(dir.path(), &path).expect("a socket");
        let token = listener.tokens().issue_to_this_process(7).expect("a token");
        let (tx, rx) = mpsc::channel();
        let _reading = listener.each(Box::new(move |report| {
            let _ = tx.send(report);
        }));
        let report = Report {
            chat: 7,
            event: Event::Notification,
            conversation: Conversation::Named("abc".to_owned()),
            pid: Some(99),
            agent: None,
            detail: Detail::default(),
        };
        let line = line_with(Some(&token), &report).expect("a line");
        let mut socket = std::os::unix::net::UnixStream::connect(&path).expect("connects");
        // A byte every half second for four seconds: each read is quick, the line is not.
        let (dripped, rest) = line.split_at(8);
        for byte in dripped {
            let _ = socket.write_all(&[*byte]);
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
        let _ = socket.write_all(rest);
        assert!(
            rx.recv_timeout(std::time::Duration::from_secs(1)).is_err(),
            "the line was read after its deadline"
        );
    }

    #[test]
    fn each_chat_is_issued_a_token_of_its_own_and_only_its_latest_counts() {
        let tokens = ChatTokens::default();
        // The token alone: every process counts as inside here, which the binding's own tests
        // take apart.
        let admitted = |tokens: &ChatTokens, chat: u32, token: Option<&str>, peer: Option<u32>| {
            tokens.bind(chat, program(1));
            tokens.admission_by(chat, token, peer, &|_, _| true) == Admission::Admitted
        };
        let one = tokens.issue(1).expect("a token");
        let two = tokens.issue(2).expect("a token");

        assert_ne!(one, two);
        for token in [&one, &two] {
            // 32 bytes from the generator, as hex.
            assert_eq!(token.expose().len(), 64);
            assert!(token.expose().bytes().all(|b| b.is_ascii_hexdigit()));
        }
        assert!(admitted(&tokens, 1, Some(one.expose()), Some(1)));
        assert!(!admitted(&tokens, 1, Some(two.expose()), Some(1)));
        assert!(!admitted(&tokens, 1, None, Some(1)));
        assert!(
            !admitted(&tokens, 3, Some(one.expose()), Some(1)),
            "a chat with no token"
        );
        // A prefix, and the right length with one byte wrong.
        assert!(!admitted(&tokens, 1, Some(&one.expose()[..63]), Some(1)));
        let mut near = one.expose().to_owned().into_bytes();
        near[63] = if near[63] == b'0' { b'1' } else { b'0' };
        assert!(!admitted(
            &tokens,
            1,
            Some(std::str::from_utf8(&near).unwrap()),
            Some(1)
        ));

        let again = tokens.issue(1).expect("a token");
        assert!(
            !admitted(&tokens, 1, Some(one.expose()), Some(1)),
            "the old one still counts"
        );
        assert!(admitted(&tokens, 1, Some(again.expose()), Some(1)));

        tokens.forget(1);
        assert!(!admitted(&tokens, 1, Some(again.expose()), Some(1)));
    }

    #[test]
    fn a_token_is_never_written_out_by_debug() {
        let tokens = ChatTokens::default();
        let token = tokens.issue_to_this_process(1).expect("a token");

        assert!(!format!("{token:?}").contains(token.expose()));
        assert!(!format!("{tokens:?}").contains(token.expose()));
    }

    #[test]
    fn a_token_is_read_from_its_own_variable_and_an_empty_one_is_none() {
        assert_eq!(
            ChatToken::read(&env_of(&[(TOKEN_ENV, "abc")])),
            Some(ChatToken::from("abc"))
        );
        assert_eq!(ChatToken::read(&env_of(&[(TOKEN_ENV, "")])), None);
        assert_eq!(ChatToken::read(&env_of(&[(CHAT_ENV, "7")])), None);
    }

    #[test]
    fn a_token_travels_beside_the_line_and_is_taken_off_before_it_is_read() {
        let token = ChatToken::from("abc");
        let wire = line_with(Some(&token), &Ask::Ticket { chat: 3 }).unwrap();
        let text = std::str::from_utf8(&wire).unwrap();

        let (line, carried) = read_line(text).expect("a line");
        assert!(matches!(line, Line::Ask(Ask::Ticket { chat: 3 })));
        assert_eq!(carried.as_deref(), Some("abc"));

        let bare = line_with(None, &a_stop(7)).unwrap();
        let (line, carried) = read_line(std::str::from_utf8(&bare).unwrap()).expect("a line");
        assert_eq!(line.chat(), 7);
        assert_eq!(carried, None);
    }
}

#[cfg(all(test, unix))]
#[path = "hookwire/delivery_tests.rs"]
mod delivery_tests;
