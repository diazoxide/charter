//! The request budget per account (FI14, FW-4): every request a [`super::route::Resolver`]
//! resolves for a forge account is counted there, on either route, and a background request is
//! held back once the account's hour is spent. Only the resolver builds the native transport, so
//! nothing sends with a sign-in token unmetered. Nothing in the app drives a resolver or
//! [`super::poll`] yet: FW-7 (#735) does.
//!
//! # What counts
//!
//! A request is **counted** the way its forge counts it against the account's limit:
//!
//! - **GitHub** does not count a `304 Not Modified` to a request made with an `Authorization`
//!   header against the primary rate limit ("Best practices for using the REST API",
//!   docs.github.com). A conditional read that finds nothing new costs nothing there.
//! - **GitLab** counts every request. Its rate-limit pages (`user/gitlab_com/rate_limits`,
//!   `administration/settings/user_and_ip_rate_limits`) name API requests as what is throttled,
//!   and make no exception for a `304`, so a conditional read costs a request there too.
//!
//! [`Counting`] holds that rule per forge. A request that got no status (the CLI transport, a
//! timeout) is counted: charter cannot tell it went uncounted, so it assumes it did not.
//!
//! # The allowance, and the forge's own limit
//!
//! Two numbers, kept apart:
//!
//! - **charter's allowance**, [`HOURLY_ALLOWANCE`] counted requests an hour per account. It
//!   is charter's own share, a fifth of GitHub's 5,000-an-hour primary limit for a signed-in
//!   user, so the human's own tools keep the rest. Once it is spent a **background** request
//!   is held back until the hour turns ([`Meter::admit`]); a request a person is waiting on is
//!   never held back. A chat's or an MCP call is admitted as background ([`admission`]): only
//!   a person can be waiting.
//! - **the forge's limit**, read off each answer's rate-limit headers. Below [`FLOOR_PERCENT`]
//!   of it remaining, polling backs off ([`super::poll`]).
//!
//! # Where it is kept
//!
//! A [`Meter`] lives in the process that sends, and writes what it counted to the machine tier,
//! `<config>/forge-budget/<account>.json` ([`root`]), so `charter doctor` can show it and a
//! restart resumes the same hour. The file holds counts and the forge's last reading, never a
//! request, an answer or a credential. A chat cannot write it: the machine store is denied to a
//! chat's writes (`sandbox::Denied`, the human-powers class). A file that cannot be read or
//! written is an empty hour, never an error.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use super::backend::{Account, Caller, Principal, Priority};
use super::transport::{Call, NoAnswer, Reply, Transport};
use super::{Forge, ForgeError, Kind};

/// Counted requests an hour charter allows itself per account.
pub const HOURLY_ALLOWANCE: u32 = 1_000;

/// Below this percentage of the forge's limit remaining, polling backs off.
pub const FLOOR_PERCENT: u64 = 20;

/// The length of one budget window: an hour, in seconds.
pub const WINDOW: u64 = 3_600;

/// What time it is, in seconds since the epoch. A test gives a virtual clock.
pub trait Clock: Send + Sync {
    fn now(&self) -> u64;
}

/// The system's clock.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or_default()
    }
}

/// How a forge counts a request against an account's limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Counting {
    /// A `304 Not Modified` is not counted: GitHub.
    NotModifiedIsFree,
    /// Every request counts: GitLab.
    EveryRequest,
}

impl Counting {
    pub fn of(kind: Kind) -> Counting {
        match kind {
            Kind::GitHub => Counting::NotModifiedIsFree,
            Kind::GitLab => Counting::EveryRequest,
        }
    }
}

/// The forge's own limit, as its last answer stated it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForgeLimit {
    pub limit: u64,
    pub remaining: u64,
    /// When it resets, in seconds since the epoch, when the forge said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reset: Option<u64>,
}

impl ForgeLimit {
    /// Read off an answer's headers: GitHub's `x-ratelimit-*`, GitLab's `ratelimit-*`. The
    /// limit's name is GitHub's `x-ratelimit-resource` (`core`, `graphql`, ...) or GitLab's
    /// `ratelimit-name`, else `api`.
    fn of(reply: &Reply) -> Option<(String, ForgeLimit)> {
        let header = |name: &str| {
            reply
                .header(&format!("x-ratelimit-{name}"))
                .or_else(|| reply.header(&format!("ratelimit-{name}")))
        };
        let number = |name: &str| header(name).and_then(|v| v.trim().parse::<u64>().ok());
        let limit = number("limit")?;
        let remaining = number("remaining")?;
        let name = header("resource")
            .or_else(|| header("name"))
            .unwrap_or("api")
            .to_string();
        Some((
            name,
            ForgeLimit {
                limit,
                remaining,
                reset: number("reset"),
            },
        ))
    }

    /// Whether less than [`FLOOR_PERCENT`] of the limit remains, as of `now`. A reading whose
    /// reset has passed says nothing any more.
    pub fn below_floor(&self, now: u64) -> bool {
        if self.reset.is_some_and(|reset| reset <= now) {
            return false;
        }
        // Wide enough that no forge-supplied number can overflow it.
        self.limit > 0
            && u128::from(self.remaining) * 100 < u128::from(self.limit) * u128::from(FLOOR_PERCENT)
    }
}

/// One account's hour: what charter sent as it, what the forge counted, and the forge's
/// last word on its own limit.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    /// The account, `<kind> <login>@<host>`, as the row names it.
    pub account: String,
    /// When this hour began, in seconds since the epoch.
    pub since: u64,
    /// Every request sent.
    pub sent: u32,
    /// The requests the forge counts against the account ([`Counting`]).
    pub counted: u32,
    /// Conditional requests the forge answered `304 Not Modified`.
    pub not_modified: u32,
    /// Background requests held back because the hour was spent.
    pub held_back: u32,
    /// The forge's limits by name, as last read.
    #[serde(default)]
    pub forge: BTreeMap<String, ForgeLimit>,
}

impl Usage {
    /// Whether charter's allowance for this hour is spent.
    pub fn spent(&self) -> bool {
        self.counted >= HOURLY_ALLOWANCE
    }

    /// Whether any of the forge's limits is below the floor, as of `now`.
    pub fn below_floor(&self, now: u64) -> bool {
        self.forge.values().any(|l| l.below_floor(now))
    }

    /// When the hour turns.
    pub fn turns_at(&self) -> u64 {
        self.since.saturating_add(WINDOW)
    }

    /// The hour as of `now`: a fresh one when it has turned. The forge's readings stay.
    fn rolled(mut self, now: u64) -> Usage {
        if now >= self.turns_at() || self.since > now {
            self = Usage {
                account: self.account,
                since: now,
                forge: self.forge,
                ..Usage::default()
            };
        }
        self
    }

    /// The sentence `charter doctor` shows for it.
    pub fn said(&self) -> String {
        let mut said = format!(
            "{}: {} of {} counted requests this hour ({} sent, {} answered 304 Not Modified",
            self.account, self.counted, HOURLY_ALLOWANCE, self.sent, self.not_modified
        );
        if self.held_back > 0 {
            said.push_str(&format!(", {} background held back", self.held_back));
        }
        said.push(')');
        let limits: Vec<String> = self
            .forge
            .iter()
            .map(|(name, l)| format!("{name} {} of {} left", l.remaining, l.limit))
            .collect();
        if limits.is_empty() {
            said.push_str("; the forge has stated no limit");
        } else {
            said.push_str(&format!("; the forge says {}", limits.join(", ")));
        }
        said
    }
}

/// The priority `caller` is admitted at. **Only a person can be waiting**: a chat's or an MCP
/// call is admitted as background whatever priority it carries, so an agent that loops on an
/// account's calls is held back once the hour is spent, and cannot starve the person's own
/// refreshes while it is never held itself.
pub fn admission(caller: &Caller) -> Priority {
    match (caller.principal(), caller.is_a_chat()) {
        (Principal::Human, false) => caller.priority(),
        _ => Priority::Background,
    }
}

/// Where every account's budget file lives under the machine store `config_root`.
pub fn root(config_root: &Path) -> PathBuf {
    crate::machine::dir(config_root).join("forge-budget")
}

/// Every account's hour kept under `config_root`, as of `now`, by account name. An unreadable
/// file is skipped.
pub fn kept(config_root: &Path, now: u64) -> Vec<Usage> {
    let Ok(entries) = std::fs::read_dir(root(config_root)) else {
        return Vec::new();
    };
    let mut all: Vec<Usage> = entries
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter_map(|e| std::fs::read_to_string(e.path()).ok())
        .filter_map(|text| serde_json::from_str::<Usage>(&text).ok())
        .map(|usage| usage.rolled(now))
        .collect();
    all.sort_by(|a, b| a.account.cmp(&b.account));
    all
}

/// The name a [`Usage`] gives `account`.
fn named(account: &Account) -> String {
    format!("{} {}@{}", account.kind.word(), account.login, account.host)
}

/// One account's meter: counts what is sent as it, and holds back a background request once
/// its hour is spent.
pub struct Meter {
    counting: Counting,
    clock: Arc<dyn Clock>,
    file: Option<PathBuf>,
    usage: Mutex<Usage>,
}

impl Meter {
    /// A meter for `account` kept in memory only.
    pub fn new(account: &Account, clock: Arc<dyn Clock>) -> Meter {
        let now = clock.now();
        Meter {
            counting: Counting::of(account.kind),
            usage: Mutex::new(Usage {
                account: named(account),
                since: now,
                ..Usage::default()
            }),
            clock,
            file: None,
        }
    }

    /// A meter for `account` kept in the machine tier under `config_root`: it reads the hour
    /// its file holds before each change and writes it after, so every process sending as the
    /// account shares one count. Two processes counting at the same instant can lose one of
    /// the two counts; the budget is a guard, not a ledger.
    pub fn kept_in(config_root: &Path, account: &Account, clock: Arc<dyn Clock>) -> Meter {
        let mut meter = Meter::new(account, clock);
        meter.file = Some(root(config_root).join(format!("{}.json", account.key())));
        meter
    }

    /// What the file holds, for this meter's account, when it holds anything readable.
    fn read_kept(&self, account: &str) -> Option<Usage> {
        let text = std::fs::read_to_string(self.file.as_ref()?).ok()?;
        let usage = serde_json::from_str::<Usage>(&text).ok()?;
        Some(Usage {
            account: account.to_string(),
            ..usage
        })
    }

    /// The hour as of now.
    pub fn usage(&self) -> Usage {
        let now = self.clock.now();
        self.change(|u| u.clone(), now, false)
    }

    /// Whether a request of `priority` may be sent now. A background request is held back,
    /// and counted as such, once the hour is spent; a foreground one never is.
    pub fn admit(&self, priority: Priority) -> Result<(), NoAnswer> {
        let now = self.clock.now();
        let held = self.change(
            |u| {
                let held = priority == Priority::Background && u.spent();
                if held {
                    u.held_back = u.held_back.saturating_add(1);
                }
                held.then(|| (u.account.clone(), u.turns_at()))
            },
            now,
            true,
        );
        match held {
            None => Ok(()),
            Some((account, turns_at)) => Err(NoAnswer::HeldBack {
                said: format!(
                    "charter's request budget for {account} is spent for this hour \
                     ({HOURLY_ALLOWANCE} counted requests), so a background refresh waits \
                     until it turns"
                ),
                reset: Some(turns_at),
            }),
        }
    }

    /// Count one request that was sent and answered `reply`, or got no status at all (`None`).
    pub fn record(&self, reply: Option<&Reply>) {
        let now = self.clock.now();
        let counting = self.counting;
        self.change(
            |u| {
                u.sent = u.sent.saturating_add(1);
                let not_modified = reply.is_some_and(|r| r.status == Some(304));
                if not_modified {
                    u.not_modified = u.not_modified.saturating_add(1);
                }
                if !(not_modified && counting == Counting::NotModifiedIsFree) {
                    u.counted = u.counted.saturating_add(1);
                }
                if let Some((name, limit)) = reply.and_then(ForgeLimit::of) {
                    u.forge.insert(name, limit);
                }
            },
            now,
            true,
        )
    }

    /// Apply `f` to the hour as of `now`, and keep the result when `write`.
    fn change<T>(&self, f: impl FnOnce(&mut Usage) -> T, now: u64, write: bool) -> T {
        let mut usage = self.usage.lock().unwrap_or_else(|e| e.into_inner());
        // A kept hour is read again first: every process that sends as the account, and the
        // window and a `charter` command among them, spends the one count (ADR 0070 §6).
        if let Some(kept) = self.read_kept(&usage.account) {
            *usage = kept;
        }
        *usage = std::mem::take(&mut *usage).rolled(now);
        let out = f(&mut usage);
        if write
            && let Some(file) = &self.file
            && let Ok(text) = serde_json::to_string(&*usage)
            && let Some(dir) = file.parent()
        {
            let _ = super::etag::write_private(dir, file, text.as_bytes());
        }
        out
    }
}

/// A transport whose every request is admitted and counted by one account's [`Meter`]: what
/// [`super::route::Resolver`] hands out for any call it resolves for an account, on either route.
pub struct Metered {
    inner: Arc<dyn Transport>,
    meter: Arc<Meter>,
    priority: Priority,
}

impl Metered {
    pub fn new(inner: Arc<dyn Transport>, meter: Arc<Meter>, priority: Priority) -> Metered {
        Metered {
            inner,
            meter,
            priority,
        }
    }
}

impl Transport for Metered {
    fn send(&self, forge: &Forge, call: &Call) -> Result<Reply, NoAnswer> {
        self.meter.admit(self.priority)?;
        let answer = self.inner.send(forge, call);
        match &answer {
            Ok(reply) => self.meter.record(Some(reply)),
            // It may have reached the forge, so it is counted.
            Err(NoAnswer::Timeout(_)) => self.meter.record(None),
            // It was never sent.
            Err(NoAnswer::Missing(_) | NoAnswer::Refused(_) | NoAnswer::HeldBack { .. }) => {}
        }
        answer
    }

    fn check_auth(&self, forge: &Forge) -> Result<(), ForgeError> {
        // The native transport asks the forge who it is; the CLI asks its own login. Either
        // way charter cannot see the status, so it is counted as one request.
        let checked = self.inner.check_auth(forge);
        self.meter.record(None);
        checked
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    struct At(AtomicU64);
    impl Clock for At {
        fn now(&self) -> u64 {
            self.0.load(Ordering::SeqCst)
        }
    }

    fn at(now: u64) -> Arc<At> {
        Arc::new(At(AtomicU64::new(now)))
    }

    fn account(kind: Kind) -> Account {
        Account {
            kind,
            host: "example.com".into(),
            login: "octocat".into(),
        }
    }

    fn reply(status: u16, headers: &[(&str, &str)]) -> Reply {
        Reply {
            code: 0,
            out: String::new(),
            err: String::new(),
            status: Some(status),
            headers: headers
                .iter()
                .map(|(n, v)| (n.to_string(), v.to_string()))
                .collect(),
        }
    }

    #[test]
    fn github_does_not_count_a_304_and_gitlab_does() {
        for (kind, counted) in [(Kind::GitHub, 1), (Kind::GitLab, 2)] {
            let meter = Meter::new(&account(kind), at(1_000));
            meter.record(Some(&reply(200, &[])));
            meter.record(Some(&reply(304, &[])));
            let usage = meter.usage();
            assert_eq!(
                (usage.sent, usage.counted, usage.not_modified),
                (2, counted, 1),
                "{kind:?}"
            );
        }
    }

    #[test]
    fn only_a_person_is_admitted_in_the_foreground() {
        assert_eq!(admission(&Caller::window()), Priority::Foreground);
        assert_eq!(admission(&Caller::command()), Priority::Foreground);
        assert_eq!(
            admission(&Caller::window().background()),
            Priority::Background
        );
        assert_eq!(admission(&Caller::trigger()), Priority::Background);
        // A chat and an MCP call carry the foreground priority, and are admitted as background.
        for chat in [Caller::chat("c1"), Caller::mcp("c1")] {
            assert_eq!(chat.priority(), Priority::Foreground);
            assert_eq!(admission(&chat), Priority::Background);
        }
    }

    #[test]
    fn numbers_a_forge_or_a_file_states_cannot_overflow_the_arithmetic() {
        let meter = Meter::new(&account(Kind::GitLab), at(1_000));
        let max = u64::MAX.to_string();
        meter.record(Some(&reply(
            200,
            &[
                ("ratelimit-limit", max.as_str()),
                ("ratelimit-remaining", max.as_str()),
                ("ratelimit-reset", max.as_str()),
            ],
        )));
        let usage = meter.usage();
        assert!(!usage.below_floor(1_000));
        let limit = ForgeLimit {
            limit: u64::MAX,
            remaining: u64::MAX / 10,
            reset: None,
        };
        assert!(
            limit.below_floor(0),
            "a tenth of the most is below a fifth of it"
        );
        // A kept file near the very end of time, its counters full.
        let full = Usage {
            since: u64::MAX - 10,
            sent: u32::MAX,
            counted: u32::MAX,
            not_modified: u32::MAX,
            held_back: u32::MAX,
            ..Usage::default()
        };
        assert_eq!(full.turns_at(), u64::MAX);
        let config = tempfile::tempdir().unwrap();
        let file = root(config.path()).join(format!("{}.json", account(Kind::GitLab).key()));
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, serde_json::to_string(&full).unwrap()).unwrap();
        let kept = Meter::kept_in(config.path(), &account(Kind::GitLab), at(u64::MAX - 5));
        assert!(kept.admit(Priority::Background).is_err());
        kept.record(Some(&reply(304, &[])));
        let usage = kept.usage();
        assert_eq!((usage.sent, usage.counted), (u32::MAX, u32::MAX));
    }

    #[test]
    fn a_request_with_no_status_is_counted() {
        let meter = Meter::new(&account(Kind::GitHub), at(1_000));
        meter.record(None);
        assert_eq!(meter.usage().counted, 1);
    }

    #[test]
    fn a_spent_hour_holds_back_background_requests_and_never_a_person_waiting() {
        let clock = at(1_000);
        let meter = Meter::new(&account(Kind::GitLab), clock.clone());
        for _ in 0..HOURLY_ALLOWANCE {
            assert!(meter.admit(Priority::Background).is_ok());
            meter.record(Some(&reply(200, &[])));
        }
        let held = meter.admit(Priority::Background).unwrap_err();
        assert!(
            matches!(
                held,
                NoAnswer::HeldBack {
                    reset: Some(4_600),
                    ..
                }
            ),
            "{held:?}"
        );
        assert_eq!(
            held.error("x".into()).failure(),
            &super::super::Failure::RateLimited { reset: Some(4_600) }
        );
        assert!(meter.admit(Priority::Foreground).is_ok());
        assert_eq!(meter.usage().held_back, 1);
        // The hour turns.
        clock.0.store(4_600, Ordering::SeqCst);
        assert!(meter.admit(Priority::Background).is_ok());
        assert_eq!(meter.usage().counted, 0);
    }

    #[test]
    fn the_forges_limit_is_read_from_either_forges_headers() {
        let meter = Meter::new(&account(Kind::GitHub), at(1_000));
        meter.record(Some(&reply(
            200,
            &[
                ("x-ratelimit-limit", "5000"),
                ("x-ratelimit-remaining", "999"),
                ("x-ratelimit-reset", "2000"),
                ("x-ratelimit-resource", "core"),
            ],
        )));
        let usage = meter.usage();
        assert_eq!(
            usage.forge.get("core"),
            Some(&ForgeLimit {
                limit: 5000,
                remaining: 999,
                reset: Some(2000)
            })
        );
        assert!(usage.below_floor(1_500));
        assert!(!usage.below_floor(2_000), "a reading past its reset");
        let lab = Meter::new(&account(Kind::GitLab), at(1_000));
        lab.record(Some(&reply(
            304,
            &[("ratelimit-limit", "2000"), ("ratelimit-remaining", "400")],
        )));
        let usage = lab.usage();
        assert_eq!(usage.forge["api"].remaining, 400);
        assert!(
            !usage.below_floor(1_000),
            "exactly 20% left is not below it"
        );
    }

    #[test]
    fn a_kept_hour_resumes_in_a_new_process_and_is_listed_for_the_doctor() {
        let config = tempfile::tempdir().unwrap();
        let clock = at(1_000);
        let first = Meter::kept_in(config.path(), &account(Kind::GitHub), clock.clone());
        first.record(Some(&reply(200, &[])));
        first.record(Some(&reply(304, &[])));
        clock.0.store(1_100, Ordering::SeqCst);
        let again = Meter::kept_in(config.path(), &account(Kind::GitHub), clock.clone());
        again.record(Some(&reply(200, &[])));
        let all = kept(config.path(), 1_200);
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].account, "github octocat@example.com");
        assert_eq!((all[0].sent, all[0].counted), (3, 2));
        assert_eq!(kept(config.path(), 4_600)[0].sent, 0, "the hour turned");
    }

    #[test]
    fn two_processes_sending_as_one_account_spend_one_count() {
        let config = tempfile::tempdir().unwrap();
        let clock = at(1_000);
        let window = Meter::kept_in(config.path(), &account(Kind::GitLab), clock.clone());
        let command = Meter::kept_in(config.path(), &account(Kind::GitLab), clock.clone());
        window.record(Some(&reply(200, &[])));
        command.record(Some(&reply(200, &[])));
        window.record(Some(&reply(200, &[])));
        assert_eq!(window.usage().counted, 3);
        assert_eq!(command.usage().counted, 3);
    }

    #[test]
    fn the_doctors_sentence_names_the_budget_its_use_and_the_forges_limit() {
        let meter = Meter::new(&account(Kind::GitHub), at(1_000));
        meter.record(Some(&reply(
            200,
            &[
                ("x-ratelimit-limit", "5000"),
                ("x-ratelimit-remaining", "4321"),
            ],
        )));
        meter.record(Some(&reply(304, &[])));
        assert_eq!(
            meter.usage().said(),
            "github octocat@example.com: 1 of 1000 counted requests this hour (2 sent, 1 \
             answered 304 Not Modified); the forge says api 4321 of 5000 left"
        );
    }
}
