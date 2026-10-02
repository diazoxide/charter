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
//!
//! # One error type per discipline
//!
//! Every **strict** method fails with [`ForgeError`], the forge's or the transport's own words.
//! The two **permissive** methods, the status line's, never fail: every failure is `Ok(None)`,
//! and [`Raised`] marks only an answer shaped where Python raised (`gl-refresh`'s contract).
//! `checks_at` folds every failure into `UNKNOWN` with its reason, because a check that could
//! not be read is one of its five values. A `ForgeError` carries its kind from ADR 0070's
//! closed set ([`super::Failure`]) beside its words: the native transport tells it from the
//! HTTP status, and a CLI refusal is `Unrecognised`.

use std::sync::Arc;

use serde_json::Value;

use super::checks::Checks;
use super::cli::Cli;
use super::pr::{AutoMerge, MergeAs, MergedAt, Opened, Pr, Request, State};
use super::transport::{Call, NoAnswer, Reply, Transport};
use super::{Forge, ForgeError, Kind, Raised};

/// Who a forge call is made for (ADR 0070 §4). It rides on every call, reads included.
///
/// It names the surface that asked, whether a person is waiting, **who** is asking (a human,
/// or a chat by its id) and, once FW-1's sign-in exists, the account. The credential a call
/// gets is decided from this and the account alone ([`super::route::route`]).
///
/// **Its fields are private, and nothing builds one from data.** A `Caller` is made by the code
/// that knows who asked: [`Caller::window`] by the window, [`Caller::command`] by a `charter`
/// command, [`Caller::chat`] and [`Caller::mcp`] by the host for a chat's own connection. It
/// has no `Deserialize`, so an MCP tool call's arguments can never name its principal (ADR
/// 0070 §4). And it grants nothing by itself: the sign-in token is reachable only through a
/// [`super::route::Resolver`] the host was given a sign-in for, and a chat or MCP `Caller`
/// never resolves to it however the resolver was built.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Caller {
    surface: Surface,
    priority: Priority,
    principal: Principal,
    account: Option<Account>,
}

/// The surface a call was made from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Surface {
    /// The app's window.
    Window,
    /// A `charter` command.
    Command,
    /// An MCP tool a chat called, served on the chat's own connection.
    Mcp,
    /// A trigger: something that fired with nobody at the keyboard.
    Trigger,
}

impl Surface {
    /// Every surface. A `match`, so a new surface does not compile until it is listed, and
    /// the routing tests that walk this list cover it.
    pub fn all() -> [Surface; 4] {
        let every = [
            Surface::Window,
            Surface::Command,
            Surface::Mcp,
            Surface::Trigger,
        ];
        for surface in every {
            match surface {
                Surface::Window | Surface::Command | Surface::Mcp | Surface::Trigger => {}
            }
        }
        every
    }
}

/// Whether a person is waiting on the answer. FW-4's budget refuses background calls below
/// its floor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Priority {
    Foreground,
    Background,
}

/// Who a call is made by.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Principal {
    /// A person, through the window or a command they typed.
    Human,
    /// A chat, by its id. Never resolves to the human's sign-in token (FI3).
    Chat(String),
}

/// One forge sign-in: a kind, a host and a login (FI2). A repo is bound to one.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Account {
    pub kind: Kind,
    pub host: String,
    pub login: String,
}

impl Account {
    /// A name for this account that is safe as one path segment: `github-github.com-octocat`.
    pub fn key(&self) -> String {
        let clean = |s: &str| {
            s.chars()
                .map(|c| {
                    if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                        c
                    } else {
                        '_'
                    }
                })
                .collect::<String>()
        };
        format!(
            "{}-{}-{}",
            self.kind.word(),
            clean(&self.host),
            clean(&self.login)
        )
    }
}

impl Caller {
    /// A `charter` command a person is waiting on. Always the CLI transport (ADR 0070 ruling 5).
    pub fn command() -> Caller {
        Caller {
            surface: Surface::Command,
            priority: Priority::Foreground,
            principal: Principal::Human,
            account: None,
        }
    }

    /// The window, with a person waiting.
    pub fn window() -> Caller {
        Caller {
            surface: Surface::Window,
            ..Caller::command()
        }
    }

    /// A trigger that fired with nobody at the keyboard, on a human's behalf. It never resolves
    /// to the sign-in token: nobody is there to have asked for it.
    pub fn trigger() -> Caller {
        Caller {
            surface: Surface::Trigger,
            priority: Priority::Background,
            ..Caller::command()
        }
    }

    /// A chat, by its id, running a `charter` command.
    pub fn chat(id: &str) -> Caller {
        Caller {
            principal: Principal::Chat(id.to_string()),
            ..Caller::command()
        }
    }

    /// An MCP tool chat `id` called. There is no human MCP caller: the host serves MCP on a
    /// chat's own connection only.
    pub fn mcp(id: &str) -> Caller {
        Caller {
            surface: Surface::Mcp,
            ..Caller::chat(id)
        }
    }

    /// The same caller, with nobody waiting: a refresh.
    pub fn background(self) -> Caller {
        Caller {
            priority: Priority::Background,
            ..self
        }
    }

    /// The same caller, asking as `account`.
    pub fn as_account(self, account: Account) -> Caller {
        Caller {
            account: Some(account),
            ..self
        }
    }

    pub fn surface(&self) -> Surface {
        self.surface
    }

    pub fn priority(&self) -> Priority {
        self.priority
    }

    pub fn principal(&self) -> &Principal {
        &self.principal
    }

    pub fn account(&self) -> Option<&Account> {
        self.account.as_ref()
    }

    /// Whether a chat is asking, by its principal or its surface.
    pub fn is_a_chat(&self) -> bool {
        matches!(self.principal, Principal::Chat(_)) || self.surface == Surface::Mcp
    }
}

/// Repositories: what an owner has, what an account reaches, and what a repo holds.
pub trait Repos {
    /// Every repo under `owner`, as the owner exposes them. Strict: a failure is an error,
    /// never an empty list.
    fn owned(&self, caller: &Caller, owner: &Owner) -> Result<Vec<RepoRecord>, ForgeError>;

    /// Every repo under `owner` that the account reaches, private ones included (ADR 0055).
    /// Strict.
    fn reachable(&self, caller: &Caller, owner: &Owner) -> Result<Vec<RepoRecord>, ForgeError>;

    /// The top-level file names of `repo` at `git_ref`, or its default branch. Strict.
    fn top_level(
        &self,
        caller: &Caller,
        repo: &RepoRecord,
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
    ) -> Result<Opened, ForgeError>;

    /// Where `pr` stands: open, merged or closed.
    fn state(&self, caller: &Caller, path: &str, pr: &Pr) -> Result<State, ForgeError>;

    /// The description `pr` holds, whole: GitHub's `body`, GitLab's `description`. A request
    /// with none is the empty string (ADR 0060: `charter change push` splices its block in).
    fn body(&self, caller: &Caller, path: &str, pr: &Pr) -> Result<String, ForgeError>;

    /// Replace `pr`'s description with `body`, and nothing else about it: not its title, its
    /// base or its state (ADR 0060).
    fn set_body(&self, caller: &Caller, path: &str, pr: &Pr, body: &str) -> Result<(), ForgeError>;

    /// The newest request whose head is `branch`, in any state (ADR 0060).
    fn by_head(
        &self,
        caller: &Caller,
        path: &str,
        branch: &str,
    ) -> Result<Option<Request>, ForgeError>;

    /// Ask the forge to merge `pr` once its checks pass, only at `head_sha` (ADR 0051).
    fn request_auto_merge(
        &self,
        caller: &Caller,
        path: &str,
        pr: &Pr,
        head_sha: &str,
    ) -> Result<AutoMerge, ForgeError>;

    /// Whether `pr` lands through its target branch's queue: GitHub's merge queue, GitLab's
    /// merge train (ADR 0060, ruling Q16).
    fn lands_through_queue(&self, caller: &Caller, path: &str, pr: &Pr)
    -> Result<bool, ForgeError>;

    /// Merge `pr` now, as `how` says, and only while its head is `head_sha`: the forge's own
    /// guard refuses a head that moved (ADR 0060 D3). It never asks the forge to merge later;
    /// [`MergedAt::Later`] is a forge that did so anyway and could not be undone.
    fn merge_at(
        &self,
        caller: &Caller,
        path: &str,
        pr: &Pr,
        head_sha: &str,
        how: &MergeAs,
    ) -> Result<MergedAt, ForgeError>;

    /// Put `pr` in its target branch's queue, only while its head is `head_sha` (ruling Q16).
    /// `Ok` once the forge's answer names the entry.
    fn enqueue_at(
        &self,
        caller: &Caller,
        path: &str,
        pr: &Pr,
        head_sha: &str,
        how: &MergeAs,
    ) -> Result<(), ForgeError>;

    /// The checks at exactly `sha` (ADR 0060). `request` is the request's number: GitLab reads
    /// a merge request's own pipelines, and GitHub reads a commit's checks with no request at
    /// all, so its body does not use it.
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

/// What a forge can do, for one reach (ADR 0070 §2).
pub trait Capabilities {
    /// Whether `what` is there at `at`. Silence never reads as yes: what charter has not
    /// asked about is [`Support::Unknown`], and an unknown capability takes its fallback.
    fn support(&self, caller: &Caller, at: &Reach, what: Capability) -> Support;
}

/// What a capability question is about: the account's instance, one owner, or one repo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reach {
    Instance,
    Owner(String),
    Repo(String),
}

/// A forge's own identifier for a thing (a GitHub node id, a GitLab global id), carried
/// opaquely so that a round trip never re-derives it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ForgeRef(pub String);

/// Whose repos a listing asks for: a GitHub organisation or user, or a GitLab group by its
/// full path (`grp/sub`), as the plane's forge config declares it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Owner(String);

impl Owner {
    pub fn new(owner: impl Into<String>) -> Owner {
        Owner(owner.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether `repo` is under this owner. A subgroup's repos are under its group's path, so
    /// they are held too. Case does not matter, as on both forges.
    pub fn holds(&self, repo: &RepoRecord) -> bool {
        repo.path_with_namespace
            .to_lowercase()
            .starts_with(&format!("{}/", self.0.to_lowercase()))
    }
}

/// One repo as a forge lists it, in neutral fields (ADR 0070 §1). The field names are the
/// keys the inventory writes (`inventory::record`), so a record reads the same in both.
///
/// A field the forge left out, or gave as null, empty or not a string, is empty here: `""`,
/// `[]`, or `None` for the two that can be absent. The one exception is `description`: a
/// truthy non-string there holds what Python's `str()` prints of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoRecord {
    /// The forge's own id for the repo. GitLab addresses a repo's tree by it.
    pub id: Option<ForgeRef>,
    /// The repo's name in its path: GitHub's `name`, GitLab's `path` (else its `name`).
    pub name: String,
    /// `owner/name` on GitHub, the full namespace path on GitLab.
    pub path_with_namespace: String,
    pub default_branch: Option<String>,
    pub description: String,
    /// The repo's page: GitHub's `html_url`, GitLab's `web_url`.
    pub web_url: String,
    /// GitHub's `ssh_url`, GitLab's `ssh_url_to_repo`.
    pub ssh_url: String,
    pub topics: Vec<String>,
    /// The forge that listed it.
    pub forge: Kind,
}

/// One thing a forge may or may not do for one repo. Not an extension's capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Capability {
    AutoMerge,
    MergeQueue,
    SubIssues,
    IssueTypes,
    Epics,
    Iterations,
    Boards,
    Dependencies,
}

impl Capability {
    /// Every capability, for a test that walks them.
    pub const ALL: [Capability; 8] = [
        Capability::AutoMerge,
        Capability::MergeQueue,
        Capability::SubIssues,
        Capability::IssueTypes,
        Capability::Epics,
        Capability::Iterations,
        Capability::Boards,
        Capability::Dependencies,
    ];

    /// What charter does instead when this capability is unavailable or unknown.
    pub fn fallback(self) -> Fallback {
        match self {
            // No auto-merge: the person merges on the forge.
            Capability::AutoMerge => Fallback::HumanClick,
            // No merge queue: auto-merge, with charter keeping the landing order (ADR 0060).
            Capability::MergeQueue => Fallback::AutoMergeInLandingOrder,
            // No sub-issues or dependencies: a link block in the body.
            Capability::SubIssues | Capability::Dependencies => Fallback::ParentLinkInBody,
            // No issue types, epics or iterations: labels and milestones (FW-6b).
            Capability::IssueTypes | Capability::Epics | Capability::Iterations => {
                Fallback::LabelsAndMilestones
            }
            // No boards: the control is not shown.
            Capability::Boards => Fallback::Hidden,
        }
    }
}

/// Whether a capability is there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Support {
    Available,
    Unavailable(Unavailable),
    Unknown(UnknownWhy),
}

/// What charter does about one capability: use it, or take a fallback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Taken {
    Feature,
    Fallback(Fallback),
}

impl Support {
    /// What charter does about `what` given this answer. **An `Unknown` capability takes its
    /// fallback** (ADR 0070 ruling 8): only `Available` uses the feature.
    pub fn taken(&self, what: Capability) -> Taken {
        match self {
            Support::Available => Taken::Feature,
            Support::Unavailable(u) => Taken::Fallback(u.fallback),
            Support::Unknown(_) => Taken::Fallback(what.fallback()),
        }
    }
}

/// Why charter does not know whether a capability is there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnknownWhy {
    /// It depends on a repo's settings, an owner's plan or a host's version, which charter has
    /// not probed (FG-2 writes the probes).
    NotProbed,
}

/// A capability that is not there, why, and what charter does instead. The same value is what
/// [`Capabilities::support`] answers and what an operation fails with
/// ([`super::Failure::Unavailable`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unavailable {
    pub what: Capability,
    pub reason: Reason,
    pub fallback: Fallback,
}

impl Unavailable {
    /// `what` is unavailable for `reason`, with its capability's own fallback.
    pub fn because(what: Capability, reason: Reason) -> Unavailable {
        Unavailable {
            what,
            reason,
            fallback: what.fallback(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reason {
    NotOnThisForge,
    NeedsTier(Tier),
    HostTooOld(String),
    NoRight(String),
    /// W7: the twin ships one release after the first forge's.
    NotYetBuilt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    GitHubOrganisation,
    GitLabPremium,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fallback {
    AutoMergeInLandingOrder,
    LabelsAndMilestones,
    ParentLinkInBody,
    HumanClick,
    Hidden,
}

/// Everything a forge backend answers. It grows as each area lands (ADR 0070 §1).
pub trait ForgeBackend: Repos + Requests + Capabilities + Send + Sync {}

impl<T: Repos + Requests + Capabilities + Send + Sync> ForgeBackend for T {}

impl Forge {
    /// This forge's backend, over the transport every call takes today: its CLI.
    pub fn backend(&self) -> Box<dyn ForgeBackend> {
        self.backend_over(Arc::new(Cli::default()))
    }

    /// This forge's backend over `transport`, for every caller.
    pub fn backend_over(&self, transport: Arc<dyn Transport>) -> Box<dyn ForgeBackend> {
        self.backend_for(Arc::new(Fixed(transport)))
    }

    /// This forge's backend, each call sent through the transport `transports` resolves for
    /// that call's [`Caller`].
    pub fn backend_for(&self, transports: Arc<dyn Transports>) -> Box<dyn ForgeBackend> {
        let asker = Asker {
            forge: self.clone(),
            transports,
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

/// Which transport one call takes: decided from its [`Caller`], once per request, by a rule
/// that does not change between the requests of one call (ADR 0070 §5).
pub trait Transports: Send + Sync {
    fn for_caller(&self, caller: &Caller) -> Result<Arc<dyn Transport>, ForgeError>;
}

/// One transport for every caller: the CLI today, a recording in a test.
pub(super) struct Fixed(pub(super) Arc<dyn Transport>);

impl Transports for Fixed {
    fn for_caller(&self, _caller: &Caller) -> Result<Arc<dyn Transport>, ForgeError> {
        Ok(self.0.clone())
    }
}

/// A forge and the transports its requests take: what both backends are built on.
pub(super) struct Asker {
    pub(super) forge: Forge,
    pub(super) transports: Arc<dyn Transports>,
}

impl Asker {
    pub(super) fn kind(&self) -> Kind {
        self.forge.kind
    }

    /// Send `call` for `caller`, through the transport resolved for it. A caller refused any
    /// credential has no answer, in the resolver's words.
    pub(super) fn send(&self, caller: &Caller, call: &Call) -> Result<Reply, NoAnswer> {
        let transport = self
            .transports
            .for_caller(caller)
            .map_err(|refused| NoAnswer::Refused(refused.to_string()))?;
        transport.send(&self.forge, call)
    }

    /// Send `call` and read its answer as JSON. Any failure is an error in the forge's own
    /// words; nothing here reads a failure as "none".
    pub(super) fn ask(
        &self,
        caller: &Caller,
        call: &Call,
        doing: &str,
    ) -> Result<Value, ForgeError> {
        let kind = self.kind();
        let answer = match self.send(caller, call) {
            Ok(answer) => answer,
            Err(NoAnswer::Timeout(why)) => {
                return Err(ForgeError::transport(format!("{doing}: {why}")));
            }
            Err(no @ (NoAnswer::Missing(_) | NoAnswer::Refused(_))) => {
                return Err(no.error(no.said().to_string()));
            }
        };
        if !answer.ok() {
            return Err(ForgeError::of(
                answer.failure(),
                format!("{doing} failed: {}", answer.said(kind)),
            ));
        }
        serde_json::from_str(&answer.out).map_err(|e| {
            ForgeError::new(format!(
                "{doing}: {} answered malformed JSON: {e}",
                kind.cli()
            ))
        })
    }

    /// Send a write and keep the forge's refusal apart from a transport that could not answer:
    /// the outer error is "no answer", the inner one is the forge's own words.
    pub(super) fn said(
        &self,
        caller: &Caller,
        call: &Call,
    ) -> Result<Result<(), String>, ForgeError> {
        match self.send(caller, call) {
            Ok(answer) if answer.ok() => Ok(Ok(())),
            Ok(answer) => Ok(Err(answer.said(self.kind()))),
            Err(no) => Err(no.error(no.said().to_string())),
        }
    }

    /// One strict JSON GET: a failure raises and never reads as "empty". Python's
    /// `_api_strict`.
    pub(super) fn strict(
        &self,
        caller: &Caller,
        path: &str,
        what_failed: &str,
    ) -> Result<Value, ForgeError> {
        let answer = match self.send(caller, &Call::get(path, super::LIST_TIMEOUT)) {
            Ok(answer) => answer,
            Err(NoAnswer::Timeout(why)) => {
                return Err(ForgeError::transport(format!(
                    "{what_failed} ({path}) {why}"
                )));
            }
            Err(no @ (NoAnswer::Missing(_) | NoAnswer::Refused(_))) => {
                return Err(no.error(no.said().to_string()));
            }
        };
        if !answer.ok() {
            return Err(ForgeError::of(
                answer.failure(),
                format!(
                    "{what_failed} failed ({path}): {}",
                    answer.said(self.kind())
                ),
            ));
        }
        super::parse(self.kind(), &answer.out, path)
    }

    /// Best-effort JSON GET. `None` on **every** failure. Python's `_api` on both backends.
    pub(super) fn best_effort(&self, caller: &Caller, path: &str) -> Option<Value> {
        let answer = self
            .send(caller, &Call::get(path, super::STATUS_TIMEOUT))
            .ok()?;
        if !answer.ok() || answer.out.trim().is_empty() {
            return None;
        }
        serde_json::from_str(&answer.out).ok()
    }
}
