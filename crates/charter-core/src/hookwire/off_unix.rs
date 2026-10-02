//! The hook channel where there is no unix socket: every way in refuses, with the reason.
//!
//! **A file of its own so that what no unix build compiles is one place.** Nothing here is
//! built on macOS or Linux, where charter's tests and its nightly mutation run live, so no test
//! there can observe a change to it; `.cargo/mutants.toml` excludes this file for that reason
//! and no other. The `windows` job in `ci.yml` is what compiles it.

use std::io;

use super::{
    Answer, Answerer, Ask, ChatToken, ChatTokens, CommitRefused, Hearing, Noticed, Report, Saved,
    SessionSaved, StartedByHand,
};

/// [`Asking`]'s counterpart where there is no unix socket: it refuses, so a handoff there
/// prints the command to run in a terminal, as it always has.
pub enum Asking {}

impl Asking {
    pub fn on(_path: &std::path::Path, _token: Option<ChatToken>) -> io::Result<Self> {
        Err(no_channel())
    }

    pub fn ask(&mut self, _ask: &Ask, _within: std::time::Duration) -> io::Result<Answer> {
        match *self {}
    }
}

/// There is no channel to send on where charter has no unix socket.
///
/// The hook's own contract already covers this — every failure here is silent and fast, and
/// `charter hook` drops the error on the floor — so a Windows hook costs its turn nothing.
/// What it does NOT do is pretend: the error names the platform, so a `doctor` that asks
/// gets an answer rather than a success that moved nothing.
pub fn send(
    _path: &std::path::Path,
    _token: Option<&ChatToken>,
    _report: &Report,
) -> io::Result<()> {
    Err(no_channel())
}

/// [`send`]'s refusal, for a harness started by hand: a shell tab here has no shims, so
/// nothing calls this, and it says why all the same.
pub fn tell(
    _path: &std::path::Path,
    _token: Option<&ChatToken>,
    _notice: &StartedByHand,
) -> io::Result<()> {
    Err(no_channel())
}

/// [`send`]'s refusal, for a refused commit: no chat here is armed with charter's git hooks,
/// so nothing calls this, and it says why all the same.
pub fn tell_refused(
    _path: &std::path::Path,
    _token: Option<&ChatToken>,
    _refused: &CommitRefused,
) -> io::Result<()> {
    Err(no_channel())
}

/// [`send`]'s refusal, for a saved session record: the record is written all the same, and
/// `charter session record` says the tab will not close by itself.
pub fn tell_saved(
    _path: &std::path::Path,
    _token: Option<&ChatToken>,
    _saved: &SessionSaved,
) -> io::Result<()> {
    Err(no_channel())
}

/// [`send`]'s refusal, for every line a hook delivers: nothing is spooled either, since the
/// spool sits beside a socket there is none of here.
pub fn deliver_report(
    _path: &std::path::Path,
    _token: Option<&ChatToken>,
    _report: &Report,
) -> io::Result<super::Delivered> {
    Err(no_channel())
}

/// [`deliver_report`]'s refusal, for a refused commit.
pub fn deliver_refused(
    _path: &std::path::Path,
    _token: Option<&ChatToken>,
    _refused: &CommitRefused,
) -> io::Result<super::Delivered> {
    Err(no_channel())
}

/// The one refusal both halves give, so the two cannot drift into two different stories.
fn no_channel() -> io::Error {
    io::Error::new(
        io::ErrorKind::Unsupported,
        "charter's hook channel is a unix socket with a 0600 mode on it, and neither has an \
         answer on this platform yet (charter-app#95)",
    )
}

/// The channel the app could not open, where there is no unix socket to open.
///
/// **An empty enum and not a struct with a stub in it.** Nothing can construct one, so
/// [`Listener::path`] and [`Listener::each`] below are not "unimplemented" — they are
/// unreachable, and the compiler is the one saying so. The only way in is [`Listener::bind`],
/// and it refuses. An app that takes that refusal for what it is cannot end up holding a
/// channel that quietly carries nothing.
pub enum Listener {}

impl Listener {
    /// Refuses, with the reason. See the note at the top of `hookwire`.
    pub fn bind(_within: &std::path::Path, _socket: &std::path::Path) -> io::Result<Self> {
        Err(no_channel())
    }

    /// Unreachable: no `Listener` is ever constructed on this platform.
    pub fn path(&self) -> &std::path::Path {
        match *self {}
    }

    /// Unreachable, for the same reason.
    pub fn tokens(&self) -> std::sync::Arc<ChatTokens> {
        match *self {}
    }

    /// Unreachable, for the same reason.
    pub fn each(self, _each: Box<dyn Fn(Report) + Send + Sync + 'static>) -> Reading {
        match self {}
    }

    /// Unreachable, for the same reason.
    pub fn each_answering(
        self,
        _each: Box<dyn Fn(Report) + Send + Sync + 'static>,
        _answer: Answerer,
    ) -> Reading {
        match self {}
    }

    /// Unreachable, for the same reason.
    pub fn each_answering_and_noticing(
        self,
        _each: Box<dyn Fn(Report) + Send + Sync + 'static>,
        _answer: Answerer,
        _noticed: Noticed,
    ) -> Reading {
        match self {}
    }

    /// Unreachable, for the same reason.
    pub fn each_answering_noticing_and_saving(
        self,
        _each: Box<dyn Fn(Report) + Send + Sync + 'static>,
        _answer: Answerer,
        _noticed: Noticed,
        _saved: Saved,
    ) -> Reading {
        match self {}
    }

    /// Unreachable, for the same reason.
    pub fn hear(self, _hearing: Hearing) -> Reading {
        match self {}
    }
}

/// [`Reading`]'s counterpart on a platform with no channel, and empty for the same reason.
pub enum Reading {}
