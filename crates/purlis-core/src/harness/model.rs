//! The neutral harness model (ADR 0073, FD-13): what a chat's harness tells charter, in
//! charter's words and not any one harness's.
//!
//! Every level speaks it. At level 2 a harness's own hooks report in charter's hook words
//! (`charter hook stop`, the words [`crate::state::Event`] parses), and
//! [`crate::state::Event::said`] reads one of those into this model. At level 3 a protocol client (HP-2 for ACP, HP-3 for a
//! harness's own protocol) will produce the same values, so nothing that reads a chat's state
//! has to know which level or which harness it came from.
//!
//! Six things, as the ticket names them: a **session**, a **turn**, an **item** inside a turn,
//! an **ask** that hands control to the operator, a **plan**, and **usage**. Level 2 carries
//! the first four today. A plan and usage have no hook that reports them as a state charter
//! draws; they are here so that a level-3 adapter has a place to put them, and the chat board
//! moves on none of them.

/// One thing a harness said about a chat.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Said {
    Session(Session),
    Turn(Turn),
    Item(Item),
    Ask(Ask),
    Plan(Plan),
    Usage(Usage),
}

/// What happened to the harness's session: the conversation charter follows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Session {
    /// A session is here: started, resumed, or a new conversation after a clear.
    Began(Began),
    /// The conversation was compacted. Nothing about the turn changed.
    Compacted,
    /// The conversation was cleared away, and another begins at once in the same process.
    /// The session did not end.
    ClearedAway,
    /// The session itself is over.
    Ended,
}

/// How a session came to be here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Began {
    /// Started or resumed, or nothing said which: what a session beginning means when the
    /// harness says nothing else.
    Fresh,
    /// A new conversation in the same process, after the old one was cleared.
    Cleared,
}

/// A turn: one prompt and everything the agent does about it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Turn {
    /// A prompt was submitted, and the agent is working on it.
    Began,
    /// The agent has nothing more to do, so the next move is the operator's.
    Ended,
    /// The agent stopped for now while helpers it started in the background are still at
    /// work, and the harness wakes it with what they say (#1626). Its work goes on: the next
    /// move is nobody's yet, and the turn that follows their end is the one that ends.
    AwaitsItsHelpers,
    /// The harness nudged that the chat sits idle at its prompt, its turn over (#1626): a
    /// nudge it says is one, never a permission or a question.
    SitsIdle,
}

/// One thing done inside a turn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    /// A child agent the turn dispatched has finished. The turn that dispatched it goes on.
    ChildEnded,
}

/// The harness handed control to the operator: a permission, a question, or a nudge that the
/// chat is idle. HP-5 normalises every source into this one shape (`super::asked` reads each
/// harness's own), so the inbox, the audit and every client read an ask the same way whatever
/// harness and level it came from.
///
/// An ask is a value. The id charter answers it by is given when it is raised
/// ([`super::asks::Asks::raise`]), because the sources' own request ids are neither unique
/// across chats nor present at every level. A level-2 `Notification` carries nothing more than
/// that it asked, so it is [`Ask::default`]: no action, no options, answered in the pane.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct Ask {
    /// What the agent wants to do.
    pub action: Action,
    /// The answers the source offers, in its order and its words. Empty where the source did
    /// not say, and then the ask is answered in the pane.
    pub options: Vec<Choice>,
    /// Who may answer it.
    pub may_answer: MayAnswer,
    /// How long charter holds it before the source's own prompt decides.
    pub deadline: Deadline,
    /// How dangerous it is to say yes.
    pub risk: Risk,
    /// One line saying what is asked, with every credential shape masked. Kept on this machine.
    pub summary: Summary,
    /// Whether answering it hands over a value a secret could be: such an ask is answered only
    /// by the operator, through a dialog the app owns, so the value reaches the tool and never
    /// the transcript (ADR 0080 §2).
    pub elicits_secret: bool,
    /// Where the answer goes back: the source's own channel, never keystrokes (04 §5.1).
    pub channel: Channel,
}

/// What an ask is about.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum Action {
    /// The source did not say: a level-2 nudge, or a level-1 harness.
    #[default]
    Unsaid,
    /// Run a command line.
    Command { line: String },
    /// Change a file.
    Edit { path: String },
    /// Call any other tool, with its input as the source gave it (compact JSON).
    Tool { name: String, input: String },
    /// Hand over values the agent asks for: an ACP elicitation's form, by its fields' labels.
    /// The form itself goes to the window with the ask (`crate::acp::Event::Elicited`).
    Elicit { fields: Vec<String> },
}

/// One answer a source offers.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Choice {
    /// What the source is sent back to choose it: its own option id or word.
    pub id: String,
    /// What the source calls it.
    pub label: String,
    pub kind: ChoiceKind,
    /// How long a yes or a no lasts.
    pub scope: ChoiceScope,
}

/// Whether a [`Choice`] lets the action run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChoiceKind {
    Allow,
    Reject,
    /// Stop the turn rather than answer (the Codex app-server's `cancel`).
    Cancel,
}

/// How long a [`Choice`] holds: this once, the rest of the session, or from now on, which a
/// source may keep in the worktree's harness settings (V28c limits that one).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChoiceScope {
    Once,
    Session,
    Always,
}

/// Who may answer an ask: the operator, through a human scope, and nobody else for now (V75).
/// Never the agent that asked (V16): [`super::asks::Answerer`] is made only from a connection
/// the host admitted as a human scope. AC-11 may add a persona here, under rules of its own.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MayAnswer {
    /// The operator, from the window or `charter inbox`.
    #[default]
    Operator,
}

/// How long charter holds an ask from when it is raised.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case", tag = "until", content = "millis")]
pub enum Deadline {
    /// Until it is answered, withdrawn or stopped. An ACP agent waits for its answer, so its
    /// ask has no deadline and charter adds none (ADR 0080 §5, V28d).
    #[default]
    None,
    /// This many milliseconds, after which the source's own prompt decides.
    Within(u64),
}

/// What charter leaves between an ask's deadline and the hook timeout it sits below: time for
/// the answer to be written back and for the hook to exit before the harness gives up on it.
pub const HOOK_MARGIN: std::time::Duration = std::time::Duration::from_secs(2);

impl Deadline {
    /// The deadline for an ask that came on a hook the harness kills after `timeout`: below it
    /// by [`HOOK_MARGIN`], so charter's answer lands before the harness stops waiting. `None`
    /// when the timeout leaves no room, and then the ask can be answered only in the pane.
    pub fn below_hook_timeout(timeout: std::time::Duration) -> Option<Self> {
        let room = timeout.checked_sub(HOOK_MARGIN)?;
        let millis = u64::try_from(room.as_millis()).ok()?;
        (millis > 0).then_some(Self::Within(millis))
    }

    /// The deadline as a duration, where there is one.
    pub fn duration(self) -> Option<std::time::Duration> {
        match self {
            Self::None => None,
            Self::Within(millis) => Some(std::time::Duration::from_millis(millis)),
        }
    }
}

/// How dangerous saying yes is (N30). A risky ask will need the operator's local verification
/// before its answer counts (SD-18); a normal one needs none.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Risk {
    #[default]
    Normal,
    /// A push to a protected branch.
    ProtectedPush,
    /// Deleting something.
    Delete,
    /// Using or handing over a secret.
    SecretUse,
    /// Acting on a production-like host.
    ProductionHost,
}

impl Risk {
    pub fn is_risky(self) -> bool {
        self != Self::Normal
    }
}

/// One line saying what an ask is, with every credential shape masked. It can only be made by
/// masking, read back from the wire included, so no path yields an unmasked one.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(from = "String", into = "String")]
pub struct Summary(String);

/// The most characters a [`Summary`] keeps.
pub const SUMMARY_WIDTH: usize = 200;

impl Summary {
    /// `text` on one line, cut to [`SUMMARY_WIDTH`], with each credential shape, as written and
    /// through its escapes ([`crate::secretshape::leak_spans`], overlapping ones joined),
    /// replaced by a mask that names its kind.
    pub fn of(text: &str) -> Self {
        let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
        let mut out = String::with_capacity(line.len());
        let mut at = 0;
        for (span, kind) in crate::secretshape::leak_spans(&line) {
            out.push_str(&line[at..span.start]);
            out.push_str(&format!("[{kind}]"));
            at = span.end;
        }
        out.push_str(&line[at..]);
        if out.chars().count() > SUMMARY_WIDTH {
            out = out.chars().take(SUMMARY_WIDTH - 1).collect();
            out.push('…');
        }
        Self(out)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for Summary {
    fn from(text: String) -> Self {
        Self::of(&text)
    }
}

impl From<Summary> for String {
    fn from(summary: Summary) -> Self {
        summary.0
    }
}

/// Where an ask came from and its answer goes back, with the source's own request id where it
/// has one. HP-6 and HP-16 answer through it.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case", tag = "via")]
pub enum Channel {
    /// The chat's pane: a level-1 harness, or a level-2 nudge with nothing structured in it.
    #[default]
    Pane,
    /// The hook that asked, answered on its own connection (Claude Code's or Codex's
    /// `PermissionRequest`).
    Hook,
    /// The Codex app-server's approval request.
    CodexAppServer {
        thread: String,
        turn: String,
        item: String,
    },
    /// opencode's permission request, answered on its server.
    Opencode { session: String, request: String },
    /// ACP's `session/request_permission`, answered on the agent's stdio.
    Acp { session: String, tool_call: String },
}

impl Channel {
    /// The source's own id for the request, where it has one: two asks with the same one are
    /// the same request, so the later supersedes the earlier.
    pub fn request(&self) -> Option<(&str, &str)> {
        let (source, request) = match self {
            Self::Pane | Self::Hook => return None,
            Self::CodexAppServer { thread, item, .. } => (thread, item),
            Self::Opencode { session, request } => (session, request),
            Self::Acp { session, tool_call } => (session, tool_call),
        };
        // A payload that left an id out names no request, and so is never the same as another.
        (!source.is_empty() && !request.is_empty()).then_some((source.as_str(), request.as_str()))
    }
}

/// The steps the agent says it will take, in its order.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Plan {
    pub steps: Vec<Step>,
}

/// One step of a [`Plan`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub text: String,
    pub done: bool,
}

/// What a turn cost, as far as the harness said.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Usage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    /// How much of the context window is used, as a whole percentage.
    pub context_percent: Option<u8>,
}
