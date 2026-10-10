//! The decision module: a host and port, and the layers a chat's sandbox was compiled with, go
//! in; one decision comes out.

use std::net::{IpAddr, Ipv4Addr};

use super::reach::{By, Decision, Reach, Refused};

const HTTPS: &[u16] = &[443];

fn reach() -> Reach {
    Reach::of(vec![
        ("api.anthropic.com".to_owned(), By::Open),
        ("*.github.com".to_owned(), By::Open),
        ("10.0.0.5:6443".to_owned(), By::Open),
        ("grafana.example.com".to_owned(), By::Persona),
        ("pypi.example.com".to_owned(), By::You),
        ("one-off.example.com:8443".to_owned(), By::Chat),
    ])
}

fn decide(host: &str, port: u16) -> Decision {
    reach().decide(host, port, HTTPS, &[])
}

#[test]
fn a_host_is_decided_by_the_layer_that_lists_it() {
    assert_eq!(decide("api.anthropic.com", 443), Decision::Open);
    assert_eq!(decide("codeload.github.com", 443), Decision::Open);
    assert_eq!(decide("10.0.0.5", 6443), Decision::Open);
    assert_eq!(decide("grafana.example.com", 443), Decision::Persona);
    assert_eq!(decide("pypi.example.com", 443), Decision::Allowed(By::You));
    assert_eq!(
        decide("one-off.example.com", 8443),
        Decision::Allowed(By::Chat)
    );
}

#[test]
fn a_host_nothing_lists_is_asked_about() {
    assert_eq!(decide("example.org", 443), Decision::Ask);
    // Listed, on another port: a host is held to the ports it is listed for.
    assert_eq!(decide("api.anthropic.com", 22), Decision::Ask);
    assert_eq!(decide("10.0.0.5", 443), Decision::Ask);
}

#[test]
fn what_could_never_be_a_host_is_refused_not_asked() {
    assert!(matches!(
        decide("intranet", 443),
        Decision::Refused(Refused::NeverAHost(_))
    ));
    assert!(matches!(
        decide("*.example.org", 443),
        Decision::Refused(Refused::NeverAHost(_))
    ));
}

#[test]
fn this_machine_link_local_and_metadata_are_refused_as_local_addresses() {
    for host in [
        "127.0.0.1",
        "169.254.169.254",
        "0.0.0.0",
        "::1",
        "fe80::1",
        "100.100.100.200",
    ] {
        assert_eq!(
            decide(host, 443),
            Decision::Refused(Refused::LocalAddress),
            "{host}"
        );
    }
    let own = [IpAddr::V4(Ipv4Addr::new(192, 168, 1, 20))];
    assert_eq!(
        reach().decide("192.168.1.20", 443, HTTPS, &own),
        Decision::Refused(Refused::LocalAddress)
    );
    // Another machine on the same network is a host like any other.
    assert_eq!(
        reach().decide("192.168.1.21", 443, HTTPS, &own),
        Decision::Ask
    );
}

/// An address literal is held to the check in every IPv6 spelling that carries a local
/// IPv4 address, and as this machine's own IPv6 address.
#[test]
fn every_ipv6_spelling_of_a_local_address_is_refused() {
    for host in [
        "::",
        "::ffff:127.0.0.1",
        "::ffff:169.254.169.254",
        "64:ff9b::a9fe:a9fe",
        "2002:7f00:1::1",
        "fd00:ec2::254",
    ] {
        assert_eq!(
            decide(host, 443),
            Decision::Refused(Refused::LocalAddress),
            "{host}"
        );
    }
    let own = ["2001:db8::7".parse::<IpAddr>().unwrap()];
    assert_eq!(
        reach().decide("2001:db8::7", 443, HTTPS, &own),
        Decision::Refused(Refused::LocalAddress)
    );
}

#[test]
fn a_local_address_is_carried_only_where_that_exact_address_and_port_is_listed() {
    let listed = Reach::of(vec![
        ("127.0.0.1".to_owned(), By::Open),
        ("127.0.0.1:5432".to_owned(), By::You),
    ]);
    assert_eq!(
        listed.decide("127.0.0.1", 5432, HTTPS, &[]),
        Decision::Allowed(By::You)
    );
    // Listed without its port: not the exact address and port, so still refused.
    assert_eq!(
        listed.decide("127.0.0.1", 443, HTTPS, &[]),
        Decision::Refused(Refused::LocalAddress)
    );
}

#[test]
fn the_exception_is_asked_of_a_resolved_address_by_its_address_and_port() {
    let listed = Reach::of(vec![("127.0.0.1:5432".to_owned(), By::You)]);
    let local: IpAddr = "127.0.0.1".parse().expect("an address");
    assert!(listed.lists_exactly(local, 5432));
    assert!(!listed.lists_exactly(local, 5433));
    assert!(!reach().lists_exactly(local, 443));
}

#[test]
fn the_first_layer_to_list_a_host_decides_it() {
    let twice = Reach::of(vec![
        ("api.example.com".to_owned(), By::Open),
        ("api.example.com".to_owned(), By::You),
    ]);
    assert_eq!(
        twice.decide("api.example.com", 443, HTTPS, &[]),
        Decision::Open
    );
}

#[test]
fn hosts_are_every_listed_entry_once_in_order() {
    let twice = Reach::of(vec![
        ("a.example.com".to_owned(), By::Open),
        ("b.example.com".to_owned(), By::You),
        ("a.example.com".to_owned(), By::Chat),
    ]);
    assert_eq!(twice.hosts(), ["a.example.com", "b.example.com"]);
    assert_eq!(
        Reach::open(vec!["x.example.com".to_owned()]).hosts(),
        ["x.example.com"]
    );
}

#[test]
fn each_decision_has_the_word_the_record_keeps() {
    assert_eq!(Decision::Open.word(), "open");
    assert_eq!(Decision::Persona.word(), "persona");
    assert_eq!(Decision::Allowed(By::You).word(), "you");
    assert_eq!(Decision::Allowed(By::Chat).word(), "chat");
    assert_eq!(Decision::Ask.word(), "ask");
    assert_eq!(Decision::Refused(Refused::LocalAddress).word(), "refused");
    assert!(Decision::Open.carries());
    assert!(Decision::Allowed(By::Chat).carries());
    assert!(!Decision::Ask.carries());
    assert!(!Decision::Refused(Refused::LocalAddress).carries());
}
