//! What a host entry may be, and why one is refused (#1341).

use super::hosts::{Granted, Host, Level, Locks, in_force};

fn ok(typed: &str) -> String {
    Host::parse(typed)
        .unwrap_or_else(|why| panic!("{typed} refused: {why}"))
        .to_string()
}

fn refused(typed: &str) -> String {
    match Host::parse(typed) {
        Ok(host) => panic!("{typed} allowed as {host}"),
        Err(why) => why,
    }
}

#[test]
fn a_domain_a_wildcard_an_address_and_any_of_them_with_a_port_are_hosts() {
    assert_eq!(ok("api.example.com"), "api.example.com");
    assert_eq!(ok("  API.Example.COM.  "), "api.example.com");
    assert_eq!(ok("*.internal.example"), "*.internal.example");
    assert_eq!(ok("api.example.com:8443"), "api.example.com:8443");
    assert_eq!(ok("*.internal.example:443"), "*.internal.example:443");
    // The operator's case: a private cluster over a VPN.
    assert_eq!(ok("10.100.39.145:6443"), "10.100.39.145:6443");
    assert_eq!(ok("192.168.1.20"), "192.168.1.20");
    assert_eq!(ok("172.16.0.9:5432"), "172.16.0.9:5432");
    // Carrier-grade NAT, which mesh VPNs hand out.
    assert_eq!(ok("100.101.102.103:22"), "100.101.102.103:22");
    assert_eq!(ok("[fd00::1]"), "[fd00::1]");
    assert_eq!(ok("[FD00:0:0::1]:6443"), "[fd00::1]:6443");
}

#[test]
fn an_empty_entry_says_what_a_host_looks_like() {
    let why = refused("   ");
    assert!(why.starts_with("Type a host"), "{why}");
    assert!(why.contains("*.example.com"), "{why}");
}

#[test]
fn a_url_is_refused_and_told_to_keep_the_host() {
    let why = refused("https://api.example.com/v1");
    assert!(why.contains("URL"), "{why}");
    assert!(why.contains("api.example.com"), "{why}");
    let why = refused("api.example.com/v1");
    assert!(why.contains("path"), "{why}");
}

#[test]
fn a_range_of_addresses_is_not_a_host() {
    let why = refused("10.0.0.0/8");
    assert!(why.contains("range"), "{why}");
}

#[test]
fn a_port_outside_one_to_65535_is_refused() {
    for typed in [
        "api.example.com:0",
        "api.example.com:65536",
        "api.example.com:",
        "a.example:x1",
    ] {
        let why = refused(typed);
        assert!(why.contains("1 to 65535"), "{typed}: {why}");
    }
}

#[test]
fn a_wildcard_over_a_whole_top_level_domain_or_anywhere_but_the_front_is_refused() {
    assert!(refused("*.com").contains("top-level"));
    assert!(refused("*").contains("*.example.com"));
    assert!(refused("api.*.example.com").contains("*.example.com"));
    assert!(refused("*example.com").contains("*.example.com"));
}

#[test]
fn a_single_name_needs_its_full_name_or_its_address() {
    let why = refused("kube-api");
    assert!(why.contains("full name"), "{why}");
}

#[test]
fn a_name_that_is_not_a_domain_is_refused_with_why() {
    assert!(refused("-api.example.com").contains("hyphen"));
    assert!(refused("api..example.com").contains("empty"));
    assert!(refused("api_x.example.com").contains("letters, digits"));
    assert!(refused("bücher.example").contains("xn--"));
    assert!(refused("user@host.example").contains("user"));
    assert!(refused("a b.example").contains("space"));
    let long = format!("{}.example", "a".repeat(64));
    assert!(refused(&long).contains("63"));
}

#[test]
fn an_address_that_is_not_one_is_refused() {
    let why = refused("10.0.0.256");
    assert!(why.contains("IPv4"), "{why}");
    let why = refused("010.0.0.1");
    assert!(why.contains("IPv4"), "{why}");
}

#[test]
fn an_ipv6_address_is_written_in_brackets() {
    let why = refused("fd00::1");
    assert!(why.contains("[fd00::1]"), "{why}");
    assert!(refused("[nope]").contains("IPv6"));
    assert!(refused("[fd00::1]x").contains("1 to 65535"));
}

/// Fail closed: what is never a host to add, each with a class-level reason.
#[test]
fn this_machine_link_local_and_unspecified_addresses_are_never_hosts() {
    for typed in [
        "localhost",
        "localhost:3000",
        "127.0.0.1",
        "127.1.2.3:8080",
        "[::1]",
        "0.0.0.0",
        "[::]",
    ] {
        assert!(refused(typed).contains("this machine"), "{typed}");
    }
    for typed in ["169.254.169.254", "[fe80::1]"] {
        assert!(refused(typed).contains("link-local"), "{typed}");
    }
    for typed in ["224.0.0.1", "255.255.255.255", "[ff02::1]"] {
        assert!(refused(typed).contains("one machine"), "{typed}");
    }
    // An IPv4 address dressed as IPv6 is judged as what it is.
    assert!(refused("[::ffff:127.0.0.1]").contains("IPv4"));
}

#[test]
fn every_host_is_one_claude_codes_sandbox_takes_as_written() {
    // Claude Code's `network.allowedDomains` takes a domain, `*.domain`, an IPv4 address and a
    // bracketed IPv6 address, each with an optional `:port`, and nothing with two colons outside
    // brackets (its bundled schema, 2.1.291). Every spelling purlis writes is one of those.
    for typed in [
        "a.example",
        "*.a.example:1",
        "10.1.2.3:6443",
        "[fd00::1]:443",
        "[fd00::2]",
    ] {
        let spelled = ok(typed);
        let bare = spelled.strip_prefix('[').map_or(spelled.as_str(), |rest| {
            rest.split_once(']').map_or(rest, |(_, after)| after)
        });
        assert!(bare.matches(':').count() <= 1, "{spelled}");
        assert!(
            !spelled.contains('/') && !spelled.contains(' '),
            "{spelled}"
        );
    }
}

fn hosts(typed: &[&str]) -> Vec<Host> {
    typed
        .iter()
        .map(|one| Host::parse(one).expect("a host"))
        .collect()
}

#[test]
fn the_project_s_hosts_come_first_and_a_personal_one_already_there_is_not_repeated() {
    let granted = in_force(
        &hosts(&["a.example", "10.0.0.5:6443"]),
        &hosts(&["10.0.0.5:6443", "b.example"]),
        &[],
        &Locks::none(),
    );
    assert_eq!(
        granted,
        vec![
            Granted {
                host: Host::parse("a.example").unwrap(),
                level: Level::Project
            },
            Granted {
                host: Host::parse("10.0.0.5:6443").unwrap(),
                level: Level::Project
            },
            Granted {
                host: Host::parse("b.example").unwrap(),
                level: Level::You
            },
        ]
    );
}

/// Review of #1341, 1: a wildcard over a numeric suffix names addresses (`*.0.0.1` is
/// 127.0.0.1 to the egress proxy), so it is refused.
#[test]
fn a_wildcard_over_a_numeric_suffix_is_refused() {
    for typed in ["*.0.0.1", "*.254.169.254", "*.0.0.1:8080"] {
        assert!(refused(typed).contains("ends in a number"), "{typed}");
    }
}

/// Review of #1341, 2: an address in another notation resolves to it, so a name ending in a
/// number (digits, or `0x` hex) is refused.
#[test]
fn an_address_in_another_notation_is_refused() {
    for typed in ["0x7f.1", "0x7f.0.0.1", "a.example.123", "a.0x", "1.2.3.4.5"] {
        let why = refused(typed);
        assert!(
            why.contains("ends in a number") || why.contains("IPv4"),
            "{typed}: {why}"
        );
    }
    // A hex-looking word that is not a number stays a name.
    assert_eq!(ok("0xcafe.example"), "0xcafe.example");
}

/// Review of #1341, 4: the cloud metadata addresses outside link-local are never hosts.
#[test]
fn cloud_metadata_addresses_are_never_hosts() {
    for typed in [
        "[fd00:ec2::254]",
        "100.100.100.200",
        "168.63.129.16",
        "192.0.0.192",
        "[fd00:ec2::23]:80",
        "[fd20:ce::254]",
        "[fd00:c1::a9fe:a9fe]",
        "[fd00:42::42]",
        "[fd00:a9fe:a9fe::1]",
        "[fd00:100::100:200]",
    ] {
        assert!(refused(typed).contains("cloud metadata"), "{typed}");
    }
    assert!(refused("0.1.2.3").contains("this machine"));
}

/// Review of #1341, 3 and 4: what a name may resolve to, and this machine's own addresses.
#[test]
fn a_resolved_address_a_chat_never_reaches_is_refused() {
    use super::hosts::refused_address;
    let own: Vec<std::net::IpAddr> = vec!["192.168.1.7".parse().unwrap()];
    for ip in [
        "127.0.0.1",
        "0.0.0.0",
        "0.1.2.3",
        "169.254.169.254",
        "224.0.0.1",
        "255.255.255.255",
        "100.100.100.200",
        "168.63.129.16",
        "::1",
        "fe80::1",
        "fd00:ec2::254",
        "::ffff:127.0.0.1",
        "192.168.1.7",
    ] {
        assert!(refused_address(ip.parse().unwrap(), &own), "{ip}");
    }
    for ip in [
        "10.100.39.145",
        "192.168.1.8",
        "100.101.102.103",
        "fd00::7",
        "8.8.8.8",
    ] {
        assert!(!refused_address(ip.parse().unwrap(), &own), "{ip}");
    }
}

#[test]
fn a_host_that_is_one_of_this_machine_s_addresses_is_dropped() {
    let own: Vec<std::net::IpAddr> = vec!["10.0.0.5".parse().unwrap()];
    let kept = super::hosts::off_this_machine(
        in_force(
            &hosts(&["10.0.0.5:22", "10.0.0.6", "a.example"]),
            &[],
            &[],
            &Locks::none(),
        ),
        &own,
    );
    let kept: Vec<String> = kept.iter().map(|one| one.host.to_string()).collect();
    assert_eq!(kept, ["10.0.0.6", "a.example"]);
}

/// Review of #1341, round 3: an IPv6 address that carries an IPv4 one is judged as it, as a
/// host and as a name's resolved address (NAT64, IPv4-compatible, 6to4, SIIT).
#[test]
fn an_ipv6_address_carrying_a_refused_ipv4_one_is_refused() {
    for typed in [
        "[::7f00:1]",
        "[64:ff9b::a9fe:a9fe]",
        "[2002:7f00:1::1]",
        "[::ffff:0:7f00:1]",
    ] {
        let why = refused(typed);
        assert!(why.contains("carries the IPv4 address"), "{typed}: {why}");
    }
    // One that carries an address a chat may reach is a host.
    assert_eq!(ok("[64:ff9b::a64:2791]:6443"), "[64:ff9b::a64:2791]:6443");

    use super::hosts::refused_address;
    for ip in [
        "::7f00:1",
        "64:ff9b::a9fe:a9fe",
        "2002:7f00:1::1",
        "::ffff:0:a9fe:a9fe",
    ] {
        assert!(refused_address(ip.parse().unwrap(), &[]), "{ip}");
    }
    assert!(!refused_address("64:ff9b::a64:2791".parse().unwrap(), &[]));
}

#[test]
fn only_the_hosts_you_confirmed_are_yours() {
    let mine = super::hosts::confirmed_only(
        hosts(&["10.0.0.6", "10.0.0.7:22", "a.example"]),
        &["10.0.0.7:22".to_owned(), "b.example".to_owned()],
    );
    assert_eq!(mine, hosts(&["10.0.0.7:22"]));
}
