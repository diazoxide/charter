//! The live ask (#1666): a connection to a host nothing lists is held while the person is asked,
//! and the person's answer, from the window alone, lets it on or refuses it.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::asks::{Answer, Asks, Heard};
use super::hosts::Host;
use super::reach::By;

/// A board that holds for `hold` and groups for `group`, and what it told.
fn board(hold: Duration, group: Duration) -> (Arc<Asks>, Arc<Mutex<Vec<Heard>>>) {
    let heard: Arc<Mutex<Vec<Heard>>> = Arc::default();
    let told = Arc::clone(&heard);
    let asks = Asks::timed(
        Arc::new(move |one: Heard| told.lock().unwrap().push(one)),
        hold,
        group,
    );
    (Arc::new(asks), heard)
}

fn host(typed: &str) -> Host {
    Host::parse(typed).unwrap()
}

/// Holds `host` on `port` on a thread of its own, deciding by the board's live hosts on `[443]`.
fn held(asks: &Arc<Asks>, host: &'static str, port: u16) -> std::thread::JoinHandle<Answer> {
    let asks = Arc::clone(asks);
    std::thread::spawn(move || asks.hold(host, port, &[443]))
}

/// Waits until the board says it holds `n` connections.
fn until_holding(asks: &Asks, n: usize) {
    for _ in 0..500 {
        if asks.holding() == n {
            return;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("the board never held {n}");
}

#[test]
fn a_held_connection_goes_on_once_its_host_is_allowed() {
    let (asks, heard) = board(Duration::from_secs(30), Duration::from_millis(50));
    let waiting = held(&asks, "api.example.com", 443);
    until_holding(&asks, 1);
    std::thread::sleep(Duration::from_millis(120));
    assert_eq!(
        *heard.lock().unwrap(),
        vec![Heard::Asked(vec!["api.example.com:443".to_owned()])],
        "the person is asked while the connection waits"
    );
    let retry = asks.allow(&host("api.example.com"), By::You);
    assert!(
        retry.is_empty(),
        "nothing gave up, so nothing is told to retry"
    );
    assert_eq!(waiting.join().unwrap(), Answer::Allowed(By::You));
    assert_eq!(asks.holding(), 0);
}

#[test]
fn a_held_connection_nobody_answers_gives_up_and_a_later_allow_says_retry() {
    let (asks, heard) = board(Duration::from_millis(150), Duration::from_millis(20));
    let answer = held(&asks, "api.example.com", 443).join().unwrap();
    assert_eq!(answer, Answer::TimedOut);
    assert!(
        heard
            .lock()
            .unwrap()
            .contains(&Heard::TimedOut("api.example.com:443".to_owned())),
        "the timeout is told, so it is recorded: {:?}",
        heard.lock().unwrap()
    );
    let retry = asks.allow(&host("api.example.com"), By::Chat);
    assert_eq!(retry, vec!["api.example.com:443".to_owned()]);
    // Told once: a second Allow of it has nothing left to retry.
    assert!(asks.allow(&host("api.example.com"), By::Chat).is_empty());
    // And the next connection to it goes straight on.
    assert_eq!(
        asks.hold("api.example.com", 443, &[443]),
        Answer::Allowed(By::Chat)
    );
}

#[test]
fn new_hosts_within_the_group_window_are_one_ask_each_named_whole() {
    let (asks, heard) = board(Duration::from_secs(30), Duration::from_millis(300));
    let one = held(&asks, "api.example.com", 443);
    until_holding(&asks, 1);
    let two = held(&asks, "cdn.example.net", 443);
    let three = held(&asks, "api.example.com", 443);
    until_holding(&asks, 3);
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(
        *heard.lock().unwrap(),
        vec![Heard::Asked(vec![
            "api.example.com:443".to_owned(),
            "cdn.example.net:443".to_owned(),
        ])],
        "one ask, each host once and whole"
    );
    // A host after the window is a new ask.
    let four = held(&asks, "late.example.org", 443);
    until_holding(&asks, 4);
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(
        heard.lock().unwrap().last(),
        Some(&Heard::Asked(vec!["late.example.org:443".to_owned()]))
    );
    asks.keep_blocked(&host("api.example.com"));
    asks.keep_blocked(&host("cdn.example.net"));
    asks.keep_blocked(&host("late.example.org"));
    for waiting in [one, two, three, four] {
        assert_eq!(waiting.join().unwrap(), Answer::Refused);
    }
}

#[test]
fn keep_blocked_refuses_what_is_held_and_what_comes_after() {
    let (asks, _) = board(Duration::from_secs(30), Duration::from_millis(10));
    let waiting = held(&asks, "api.example.com", 443);
    until_holding(&asks, 1);
    asks.keep_blocked(&host("api.example.com"));
    assert_eq!(waiting.join().unwrap(), Answer::Refused);
    assert_eq!(asks.hold("api.example.com", 443, &[443]), Answer::Refused);
    // Another host is still asked about.
    let other = held(&asks, "other.example.com", 443);
    until_holding(&asks, 1);
    asks.allow(&host("other.example.com"), By::You);
    assert_eq!(other.join().unwrap(), Answer::Allowed(By::You));
}

#[test]
fn an_allow_after_keep_blocked_lets_it_through() {
    let (asks, _) = board(Duration::from_secs(30), Duration::from_millis(10));
    asks.keep_blocked(&host("api.example.com"));
    asks.allow(&host("api.example.com"), By::You);
    assert_eq!(
        asks.hold("api.example.com", 443, &[443]),
        Answer::Allowed(By::You)
    );
}

#[test]
fn an_allow_held_to_a_port_lets_only_that_port_on() {
    let (asks, _) = board(Duration::from_millis(100), Duration::from_millis(10));
    asks.allow(&host("db.example.com:5432"), By::You);
    assert_eq!(
        asks.hold("db.example.com", 5432, &[443]),
        Answer::Allowed(By::You)
    );
    assert_eq!(asks.hold("db.example.com", 443, &[443]), Answer::TimedOut);
}

#[test]
fn a_removed_allow_reaches_nothing_again() {
    let (asks, _) = board(Duration::from_millis(100), Duration::from_millis(10));
    asks.allow(&host("api.example.com"), By::You);
    asks.forget(&host("api.example.com"), By::You);
    assert_eq!(asks.hold("api.example.com", 443, &[443]), Answer::TimedOut);
}

#[test]
fn removing_an_allow_at_one_scope_leaves_the_same_host_at_another() {
    let (asks, _) = board(Duration::from_millis(100), Duration::from_millis(10));
    asks.allow(&host("api.example.com"), By::Chat);
    asks.allow(&host("api.example.com"), By::You);
    asks.forget(&host("api.example.com"), By::You);
    assert_eq!(
        asks.hold("api.example.com", 443, &[443]),
        Answer::Allowed(By::Chat)
    );
}

#[test]
fn past_the_most_it_holds_a_connection_is_refused_at_once_not_held() {
    let (asks, heard) = board(Duration::from_secs(30), Duration::from_millis(10));
    let names: Vec<&'static str> = (0..super::asks::MOST_HELD)
        .map(|n| &*Box::leak(format!("h{n}.example.com").into_boxed_str()))
        .collect();
    let waiting: Vec<_> = names.iter().map(|name| held(&asks, name, 443)).collect();
    until_holding(&asks, super::asks::MOST_HELD);
    assert_eq!(asks.hold("one-more.example.com", 443, &[443]), Answer::Busy);
    for name in &names {
        asks.keep_blocked(&host(name));
    }
    for one in waiting {
        assert_eq!(one.join().unwrap(), Answer::Refused);
    }
    let told = heard.lock().unwrap();
    assert!(
        told.iter().all(|one| match one {
            Heard::Asked(hosts) => !hosts.iter().any(|h| h.starts_with("one-more")),
            Heard::TimedOut(_) => true,
        }),
        "a connection refused for want of room is not asked about: {told:?}"
    );
}

#[test]
fn a_held_ask_names_a_host_as_the_proxy_heard_it_never_a_wildcard() {
    let (asks, heard) = board(Duration::from_millis(80), Duration::from_millis(10));
    assert_eq!(asks.hold("API.Example.COM.", 443, &[443]), Answer::TimedOut);
    assert_eq!(
        heard.lock().unwrap().first(),
        Some(&Heard::Asked(vec!["api.example.com:443".to_owned()]))
    );
}

#[test]
fn a_host_allowed_already_since_the_chat_started_goes_on_at_once_and_is_never_asked() {
    let (asks, heard) = board(Duration::from_secs(30), Duration::from_millis(500));
    let asks = Arc::try_unwrap(asks)
        .unwrap()
        .knowing(Arc::new(|host: &Host| {
            Host::parse("*.blob.core.windows.net")
                .unwrap()
                .covers(host)
                .then_some(By::You)
        }));
    let started = std::time::Instant::now();
    assert_eq!(
        asks.hold("results.blob.core.windows.net", 443, &[443]),
        Answer::Allowed(By::You)
    );
    assert!(started.elapsed() < Duration::from_millis(200), "not held");
    assert!(heard.lock().unwrap().is_empty(), "never asked");
}

#[test]
fn after_a_timeout_or_keep_blocked_the_host_is_no_longer_said_to_be_waiting() {
    let (asks, heard) = board(Duration::from_millis(100), Duration::from_millis(10));
    assert_eq!(asks.hold("api.example.com", 443, &[443]), Answer::TimedOut);
    assert!(!asks.asks_about(&host("api.example.com")));
    // A later connection is asked about again, not held a minute in silence.
    assert_eq!(asks.hold("api.example.com", 443, &[443]), Answer::TimedOut);
    let asked = heard
        .lock()
        .unwrap()
        .iter()
        .filter(|one| matches!(one, Heard::Asked(_)))
        .count();
    assert_eq!(asked, 2);
    let (asks, _) = board(Duration::from_secs(30), Duration::from_millis(10));
    let waiting = held(&asks, "api.example.com", 443);
    until_holding(&asks, 1);
    assert!(asks.asks_about(&host("api.example.com")));
    asks.keep_blocked(&host("api.example.com"));
    assert_eq!(waiting.join().unwrap(), Answer::Refused);
    assert!(!asks.asks_about(&host("api.example.com")));
}

#[test]
fn a_host_allowed_live_reaches_a_run_started_after_and_a_removed_one_does_not() {
    use super::reach::Reach;
    use super::tunnel::{Route, route};
    let (asks, _) = board(Duration::from_millis(100), Duration::from_millis(10));
    let dsn = "postgres://app@db.example.com:6543/app";
    asks.allow(&host("db.example.com:6543"), By::Chat);
    let live = asks.allowed_live();
    assert_eq!(live, vec!["db.example.com:6543".to_owned()]);
    assert!(matches!(
        route(dsn, &Reach::open(live), &[]),
        Route::Through(..)
    ));
    asks.forget(&host("db.example.com:6543"), By::Chat);
    assert!(asks.allowed_live().is_empty());
    assert!(matches!(
        route(dsn, &Reach::open(asks.allowed_live()), &[]),
        Route::Refused(_)
    ));
}
