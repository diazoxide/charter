//! The forge seam: one trait per area, each implemented once per forge (ADR 0070 §1).
//!
//! Every forge operation charter's core performs is a method here, with a GitHub body in
//! [`super::github`] and a GitLab body in [`super::gitlab`]. Nothing outside those two
//! matches on the forge to build a request. `crates/charter-core/docs/forges.md` holds the
//! parity table, and the contract suite (`tests/forge_contract.rs`) runs every method on both
//! forges; its parity test fails when a method here has no contract case.
//!
//! No method has a default body, so a method added to an area does not compile until both
//! backends answer it.

use std::sync::Arc;

use serde_json::Value;

use super::checks::Checks;
use super::cli::Cli;
use super::pr::{AutoMerge, Opened, Pr, Request, State};
use super::transport::{Call, NoAnswer, Reply, Transport};
use super::{Forge, ForgeError, Kind, Raised};

/// Who a forge call is made for (ADR 0070 §4). It rides on every call, reads included.
///
/// Today it carries what a call site knows: which surface asked, and whether a person is
/// waiting. The account, the principal and the human on whose behalf join it with FW-1's
/// sign-in and FD-27's scopes. Until then every `Caller` resolves to the CLI transport, which is
/// what charter did before the seam (ADR 0070 §4: *"every call a `charter` command makes
/// resolves to the CLI transport"*).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Caller {
    pub surface: Surface,
    pub priority: Priority,
}

/// The surface a call was made from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Surface {
    /// The app's window.
    Window,
    /// A `charter` command.
    Command,
}

/// Whether a person is waiting on the answer. FW-4's budget refuses background calls below
/// its floor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Priority {
    Foreground,
    Background,
}

impl Caller {
    /// A `charter` command a person is waiting on.
    pub fn command() -> Caller {
        Caller {
            surface: Surface::Command,
            priority: Priority::Foreground,
        }
    }

    /// The window, with a person waiting.
    pub fn window() -> Caller {
        Caller {
            surface: Surface::Window,
            priority: Priority::Foreground,
        }
    }

    /// The same caller, with nobody waiting: a refresh.
    pub fn background(self) -> Caller {
        Caller {
            priority: Priority::Background,
            ..self
        }
    }
}

/// Repositories: what an owner has, what an account reaches, and what a repo holds.
pub trait Repos {
    /// Every repo under `owner`, as the owner exposes them, in the neutral record shape.
    /// Strict: a failure is an error, never an empty list.
    fn owned(&self, caller: &Caller, owner: &str) -> Result<Vec<Value>, ForgeError>;

    /// Every repo under `owner` that the account reaches, private ones included (ADR 0055).
    /// Strict.
    fn reachable(&self, caller: &Caller, owner: &str) -> Result<Vec<Value>, ForgeError>;

    /// The top-level file names of `repo` (a neutral record) at `git_ref`, or its default
    /// branch. Strict.
    fn top_level(
        &self,
        caller: &Caller,
        repo: &Value,
        git_ref: Option<&str>,
    ) -> Result<Vec<String>, ForgeError>;
}

/// Pull requests, merge requests on GitLab: opening, reading, merging and their checks.
pub trait Requests {
    /// Open a request from `head` into `base` in the repo at `path`, or update charter's own
    /// open one (ADR 0051).
    fn open_or_update(
        &self,
        caller: &Caller,
        path: &str,
        head: &str,
        base: &str,
        title: &str,
        body: &str,
    ) -> Result<Opened, String>;

    /// Where `pr` stands: open, merged or closed.
    fn state(&self, caller: &Caller, path: &str, pr: &Pr) -> Result<State, String>;

    /// The newest request whose head is `branch`, in any state (ADR 0060).
    fn by_head(&self, caller: &Caller, path: &str, branch: &str)
    -> Result<Option<Request>, String>;

    /// Ask the forge to merge `pr` once its checks pass, only at `head_sha` (ADR 0051).
    fn request_auto_merge(
        &self,
        caller: &Caller,
        path: &str,
        pr: &Pr,
        head_sha: &str,
    ) -> Result<AutoMerge, String>;

    /// The checks at exactly `sha`; `request` is the request's number (ADR 0060).
    fn checks_at(&self, caller: &Caller, path: &str, sha: &str, request: u64) -> Checks;

    /// The open request on `branch`, as the forge's own field holds it. Permissive: every
    /// failure is `Ok(None)`, and a shape Python raised on is `Err(Raised)` (`gl-refresh`).
    fn open_on_branch(
        &self,
        caller: &Caller,
        path: &str,
        branch: &str,
    ) -> Result<Option<Value>, Raised>;

    /// The branch's last CI result as one of `cistate::CI_STATES`. Permissive, as above.
    fn ci_word(&self, caller: &Caller, path: &str, branch: &str) -> Result<Option<String>, Raised>;
}

/// Everything a forge backend answers. It grows as each area lands (ADR 0070 §1).
pub trait ForgeBackend: Repos + Requests + Send + Sync {}

impl<T: Repos + Requests + Send + Sync> ForgeBackend for T {}

impl Forge {
    /// This forge's backend, over the transport every call takes today: its CLI.
    pub fn backend(&self) -> Box<dyn ForgeBackend> {
        self.backend_over(Arc::new(Cli::default()))
    }

    /// This forge's backend over `transport`.
    pub fn backend_over(&self, transport: Arc<dyn Transport>) -> Box<dyn ForgeBackend> {
        let asker = Asker {
            forge: self.clone(),
            transport,
        };
        match self.kind {
            Kind::GitHub => Box::new(super::github::GitHub(asker)),
            Kind::GitLab => Box::new(super::gitlab::GitLab(asker)),
        }
    }

    /// Refuse unless this forge can be asked as someone: its CLI installed and logged in.
    pub fn check_auth(&self) -> Result<(), ForgeError> {
        Cli::default().check_auth(self)
    }
}

/// A forge and the transport its requests take: what both backends are built on.
pub(super) struct Asker {
    pub(super) forge: Forge,
    pub(super) transport: Arc<dyn Transport>,
}

impl Asker {
    pub(super) fn kind(&self) -> Kind {
        self.forge.kind
    }

    pub(super) fn send(&self, call: &Call) -> Result<Reply, NoAnswer> {
        self.transport.send(&self.forge, call)
    }

    /// Send `call` and read its answer as JSON. Any failure is an error in the forge's own
    /// words; nothing here reads a failure as "none".
    pub(super) fn ask(&self, call: &Call, doing: &str) -> Result<Value, String> {
        let kind = self.kind();
        let answer = match self.send(call) {
            Ok(answer) => answer,
            Err(NoAnswer::Timeout(why)) => return Err(format!("{doing}: {why}")),
            Err(NoAnswer::Missing(why)) => return Err(why),
        };
        if !answer.ok() {
            return Err(format!("{doing} failed: {}", answer.said(kind)));
        }
        serde_json::from_str(&answer.out)
            .map_err(|e| format!("{doing}: {} answered malformed JSON: {e}", kind.cli()))
    }

    /// Send a write and keep the forge's refusal apart from a transport that could not answer:
    /// the outer error is "no answer", the inner one is the forge's own words.
    pub(super) fn said(&self, call: &Call) -> Result<Result<(), String>, String> {
        match self.send(call) {
            Ok(answer) if answer.ok() => Ok(Ok(())),
            Ok(answer) => Ok(Err(answer.said(self.kind()))),
            Err(NoAnswer::Timeout(why)) | Err(NoAnswer::Missing(why)) => Err(why),
        }
    }

    /// One strict JSON GET: a failure raises and never reads as "empty". Python's
    /// `_api_strict`.
    pub(super) fn strict(&self, path: &str, what_failed: &str) -> Result<Value, ForgeError> {
        let answer = match self.send(&Call::get(path, super::LIST_TIMEOUT)) {
            Ok(answer) => answer,
            Err(NoAnswer::Timeout(why)) => {
                return Err(ForgeError(format!("{what_failed} ({path}) {why}")));
            }
            Err(NoAnswer::Missing(why)) => return Err(ForgeError(why)),
        };
        if !answer.ok() {
            return Err(ForgeError(format!(
                "{what_failed} failed ({path}): {}",
                answer.said(self.kind())
            )));
        }
        super::parse(self.kind(), &answer.out, path)
    }

    /// Best-effort JSON GET. `None` on **every** failure. Python's `_api` on both backends.
    pub(super) fn best_effort(&self, path: &str) -> Option<Value> {
        let answer = self.send(&Call::get(path, super::STATUS_TIMEOUT)).ok()?;
        if !answer.ok() || answer.out.trim().is_empty() {
            return None;
        }
        serde_json::from_str(&answer.out).ok()
    }
}
