//! The request budget per account (FI14, FW-4), end to end: a workspace with fifty open
//! requests, polled for an hour of a virtual clock against a stand-in forge, through the
//! resolver, the native transport, its ETag store and the account's meter.
//!
//! The stand-in answers like a forge that changes: each repo's list of requests changes every
//! five minutes and each request's checks every twenty, at staggered times. It answers a
//! conditional read of something unchanged with a `304`, states its rate limit on every
//! answer, and counts what it was sent the way GitHub and GitLab count it. That count, not the
//! meter's, is what the acceptance holds: under 1,000 counted requests in the hour.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use purlis_core::forge::backend::Transports;
use purlis_core::forge::budget::{Clock, HOURLY_ALLOWANCE};
use purlis_core::forge::http::{ApiRoot, TokenSource};
use purlis_core::forge::poll::{self, Focus, Poller};
use purlis_core::forge::route::{HostScope, Resolver, SignIn};
use purlis_core::forge::transport::Call;
use purlis_core::forge::{Account, Caller, Forge, ForgeError, Kind};
use secrecy::SecretString;
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

/// The hour starts here, in seconds since the epoch.
const START: u64 = 1_800_000_000;

/// The open requests: 25 in each of two repos.
const REQUESTS: usize = 50;

struct VirtualClock(AtomicU64);
impl Clock for VirtualClock {
    fn now(&self) -> u64 {
        self.0.load(Ordering::SeqCst)
    }
}

struct Fixed;
impl TokenSource for Fixed {
    fn token(&self) -> Result<SecretString, ForgeError> {
        Ok(SecretString::from("budget-simulation-token"))
    }
}

/// What the stand-in was sent.
#[derive(Debug, Default, Clone, Copy)]
struct Seen {
    sent: u32,
    not_modified: u32,
}

impl Seen {
    /// What `kind` counts of it: GitHub not a `304`, GitLab every request.
    fn counted(self, kind: Kind) -> u32 {
        match kind {
            Kind::GitHub => self.sent - self.not_modified,
            Kind::GitLab => self.sent,
        }
    }
}

/// A forge whose answers change with the virtual clock.
struct StandIn {
    clock: Arc<VirtualClock>,
    seen: Arc<Mutex<Seen>>,
    /// What it says remains of its limit of 5,000.
    remaining: u64,
}

impl StandIn {
    /// Which version of `path` is current: a list changes every 5 minutes, a request's checks
    /// every 20, each request at its own offset.
    fn version(&self, path: &str) -> u64 {
        let now = self.clock.now();
        match path.rsplit_once("/checks/") {
            Some((_, n)) => (now + n.parse::<u64>().unwrap_or(0) * 37) / 1_200,
            None => now / 300,
        }
    }
}

impl Respond for StandIn {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let path = request.url.path().to_string();
        let etag = format!("W/\"{}-{}\"", path.replace('/', "."), self.version(&path));
        let fresh = request
            .headers
            .get("if-none-match")
            .and_then(|v| v.to_str().ok())
            == Some(etag.as_str());
        let mut seen = self.seen.lock().unwrap();
        seen.sent += 1;
        if fresh {
            seen.not_modified += 1;
        }
        let limits = |t: ResponseTemplate| {
            t.insert_header("x-ratelimit-limit", "5000")
                .insert_header("x-ratelimit-remaining", self.remaining.to_string().as_str())
                .insert_header("x-ratelimit-resource", "core")
        };
        if fresh {
            limits(ResponseTemplate::new(304))
        } else {
            limits(
                ResponseTemplate::new(200)
                    .insert_header("etag", etag.as_str())
                    .set_body_string(format!(r#"{{"path":"{path}"}}"#)),
            )
        }
    }
}

fn account(kind: Kind) -> Account {
    Account {
        kind,
        host: "forge.example".into(),
        login: "octocat".into(),
    }
}

/// The workspace's watches: each repo's open requests, and each request's checks.
fn watches() -> Vec<Call> {
    let mut calls = Vec::new();
    for repo in ["api", "web"] {
        calls.push(Call::get(
            format!("repos/o/{repo}/pulls"),
            Duration::from_secs(5),
        ));
    }
    for n in 0..REQUESTS {
        let repo = if n % 2 == 0 { "api" } else { "web" };
        calls.push(Call::get(
            format!("repos/o/{repo}/checks/{n}"),
            Duration::from_secs(5),
        ));
    }
    calls
}

/// Poll `watches` as `kind`'s account for an hour of the virtual clock at `focus`, against a
/// stand-in saying `remaining` of 5,000 left. What the stand-in saw, and what the meter
/// counted.
fn an_hour(kind: Kind, focus: Focus, watches: Vec<Call>, remaining: u64) -> (Seen, u32) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let server = rt.block_on(MockServer::start());
    let clock = Arc::new(VirtualClock(AtomicU64::new(START)));
    let seen = Arc::new(Mutex::new(Seen::default()));
    rt.block_on(
        Mock::given(wiremock::matchers::method("GET"))
            .respond_with(StandIn {
                clock: clock.clone(),
                seen: seen.clone(),
                remaining,
            })
            .mount(&server),
    );
    let resolver = Resolver::new(kind, "forge.example")
        .at_root(ApiRoot::at(&server.uri()))
        .clock(clock.clone())
        .signed_in(
            &HostScope::for_a_test(),
            account(kind),
            SignIn {
                tokens: Arc::new(Fixed),
                imported_from_cli: false,
            },
        );
    let forge = Forge::default_of(kind);
    let caller = Caller::window().as_account(account(kind)).background();
    let meter = resolver.meter(&account(kind));
    let mut poller = Poller::new(watches);
    let mut now = START;
    while now < START + 3_600 {
        clock.0.store(now, Ordering::SeqCst);
        let every = poll::interval(focus, poller.len(), &meter.usage(), now);
        for call in poller.due(now, every) {
            let transport = resolver.for_caller(&caller).unwrap();
            let reply = transport.send(&forge, &call).expect("the stand-in answers");
            assert!(reply.ok(), "{reply:?}");
        }
        match poller.next_due(now, every) {
            Some(next) => now = next.max(now + 1),
            None => break,
        }
    }
    let seen = *seen.lock().unwrap();
    (seen, meter.usage().counted)
}

#[test]
fn a_fifty_request_workspace_stays_under_a_thousand_counted_requests_an_hour() {
    purlis_core::unsteered!();
    for kind in [Kind::GitHub, Kind::GitLab] {
        // The busiest focus there is: a Work view open on it all hour.
        let (seen, metered) = an_hour(kind, Focus::WorkView, watches(), 4_000);
        let counted = seen.counted(kind);
        assert!(
            counted < 1_000,
            "{kind:?}: {counted} counted of {} sent",
            seen.sent
        );
        assert_eq!(
            metered, counted,
            "{kind:?}: the meter counts as the forge does"
        );
        assert!(
            seen.sent <= poll::PACED + 52,
            "{kind:?}: {} sent, paced to {}",
            seen.sent,
            poll::PACED
        );
        assert!(seen.not_modified > 0, "{kind:?}: no read was conditional");
        assert!(metered < HOURLY_ALLOWANCE);
    }
}

#[test]
fn every_read_after_the_first_is_conditional_so_github_counts_only_what_changed() {
    purlis_core::unsteered!();
    let (seen, _) = an_hour(Kind::GitHub, Focus::WorkView, watches(), 4_000);
    // 2 lists changing 12 times an hour and 50 checks 3 times, each read once per change at
    // most, plus the first read of each: an upper bound on what GitHub counts.
    let changes = 2 * 13 + REQUESTS as u32 * 4;
    assert!(
        seen.counted(Kind::GitHub) <= changes,
        "{} counted, at most {changes} changes",
        seen.counted(Kind::GitHub)
    );
}

#[test]
fn a_hidden_window_polls_nothing() {
    purlis_core::unsteered!();
    let (seen, metered) = an_hour(Kind::GitHub, Focus::Hidden, watches(), 4_000);
    assert_eq!((seen.sent, metered), (0, 0));
}

#[test]
fn each_focus_polls_less_than_the_one_above_it() {
    purlis_core::unsteered!();
    let one = || vec![Call::get("repos/o/api/pulls", Duration::from_secs(5))];
    let sent = |focus| an_hour(Kind::GitHub, focus, one(), 4_000).0.sent;
    assert_eq!(sent(Focus::WorkView), 60);
    assert_eq!(sent(Focus::Visible), 12);
    assert_eq!(sent(Focus::Background), 4);
}

#[test]
fn below_a_fifth_of_the_forges_limit_polling_backs_off() {
    purlis_core::unsteered!();
    let one = || vec![Call::get("repos/o/api/pulls", Duration::from_secs(5))];
    // The first answer says 900 of 5,000 left: every read after it waits four times longer.
    let (seen, _) = an_hour(Kind::GitHub, Focus::WorkView, one(), 900);
    assert_eq!(seen.sent, 15);
}
