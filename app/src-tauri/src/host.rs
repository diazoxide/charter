//! What the app asks of whatever runs its sessions: the session host.
//!
//! Today the one host is [`crate::sessions::Sessions`], in the app's own process. ADR 0068
//! (proposed) moves the sessions into `charterd`, behind a socket; this trait is that seam
//! drawn first, in place, with nothing moved and nothing changed (FD-3). Everything above it —
//! [`crate::chats::Chats`], the plane it belongs to, the window's commands — reaches a session
//! only through it.
//!
//! In the ticket's words: [`SessionHost::open`] spawns, [`SessionHost::input`] writes,
//! [`SessionHost::resize`] resizes, [`SessionHost::watch`] attaches, [`SessionHost::close`]
//! ends, and [`SessionHost::when_one_ends`] is the exit notify. The rest is the numbering a
//! chat is keyed on (charter-app#90) and the two readings the curation prompt waits on.
//!
//! What sits on the host's side of it is what hears a session end — the board in `hooks`, and
//! auto-save — through the exit notify, and what a session is told to report to (the hook
//! channel and its tokens, [`crate::sessions::Reporting`]), given to the host when it is made.
//! `planewatch` touches no session at all.

use charter_core::engine::Size;
use charter_core::session::Exit;

/// What to run in a new session. No program is the operator's shell.
#[derive(Debug, Clone)]
pub struct Opening {
    pub program: Option<String>,
    pub args: Vec<String>,
    pub cwd: Option<String>,
    pub size: Size,
    /// Set in the program's environment, on top of what the app itself was started with.
    pub env: Vec<(String, String)>,
    /// Extra variable names kept out of this chat, beyond the `OP_*` prefix: every identity
    /// source and target a vault of this plane declares (`registry::identity_vars`), so a vault
    /// bound to a non-`OP_` variable (`--token-env PROD_1P_TOKEN`) leaks it to no chat either
    /// (#271 review, U6). Removed from both the set env and the inherited one.
    pub env_strip: Vec<String>,
    /// The harness the chat runs, whose own declared variables it is also started with
    /// ([`charter_core::harness::Harness::env_passed`]). None is a shell.
    pub harness: Option<charter_core::harness::Harness>,
    /// The operator's own additions to what a chat is started with, from the chat's plane
    /// (`[chat_env] pass`, [`charter_core::chatenv::read`]).
    pub env_pass: Vec<String>,
}

/// Where a view's text goes. It is called on the view's own thread, one batch at a time.
pub type Sink = Box<dyn FnMut(String) + Send>;

/// Told how each session ended, as it ends, with the session's id.
pub type Ends = Box<dyn Fn(u32, Exit) + Send + Sync>;

/// A view the UI has open, and the size the screen it opened on was drawn for.
#[derive(Debug, Clone, Copy)]
pub struct Watching {
    pub view: u32,
    pub size: Size,
}

/// Runs the app's sessions, and is the only way anything above it reaches one.
///
/// Every call that names a session that is not running is refused with a sentence, never a
/// panic.
pub trait SessionHost: Send + Sync {
    /// Starts a session, and answers with the id it is called by from now on.
    ///
    /// `wanted` is the number the chat already answers to, where it has one; a number a
    /// session is already running under is refused and a fresh one dealt. `announce` is called
    /// with the id BEFORE the program starts, so its first hook lands somewhere.
    fn open(
        &self,
        wanted: Option<u32>,
        opening: &Opening,
        announce: &dyn Fn(u32),
    ) -> Result<u32, String>;

    /// Writes what a pane sent to the session's program, as bytes.
    fn input(&self, id: u32, bytes: &[u8]) -> Result<(), String>;

    fn resize(&self, id: u32, size: Size) -> Result<(), String>;

    /// Opens a view of a session: `sink` is sent the screen as it already is and then the
    /// session's output, until the view is closed.
    fn watch(&self, id: u32, sink: Sink) -> Result<Watching, String>;

    /// Closes a view. Its session keeps running.
    fn unwatch(&self, id: u32, view: u32) -> Result<(), String>;

    /// Ends a session and everything it started.
    fn close(&self, id: u32) -> Result<(), String>;

    /// Ends every session, and does not return until their programs are gone.
    fn end_all(&self);

    /// Calls `tell` as each session's program ends, with the id and how it ended.
    fn when_one_ends(&self, tell: Ends);

    /// The sessions that are running, in the order they were opened.
    fn running(&self) -> Vec<u32>;

    /// Whether a session's terminal is still in the kernel's line editing, or none where the
    /// platform cannot say.
    fn edits_lines(&self, id: u32) -> Result<Option<bool>, String>;

    /// How long a session's program has written nothing.
    fn quiet_for(&self, id: u32) -> Result<std::time::Duration, String>;

    /// A number no chat has had, taken now for a chat about to be started with it.
    fn deal(&self) -> u32;

    /// The highest number dealt, for the record to keep.
    fn dealt(&self) -> u32;

    /// Says `dealt` numbers have already gone out, so none of them is dealt again.
    fn already_dealt(&self, dealt: u32);
}
