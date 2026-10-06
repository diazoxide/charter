//! The hosts a project and a person add to a sandboxed chat's Internet access (ADR 0067 §1 and
//! §3 as amended 2026-10-06; spec #1330, #1341), beside the presets.
//!
//! **One entry is a [`Host`]**: a domain, `*.domain` (every name under it, never the domain
//! itself), an IPv4 address or a bracketed IPv6 address, each with an optional `:port`. Private
//! ranges are hosts like any other: a cluster reached over a VPN is the case this exists for.
//! What is never a host is refused with a sentence saying why ([`Host::parse`]): this machine
//! (loopback and the unspecified address), link-local addresses (where cloud metadata services
//! answer), broadcast and multicast, a range, a URL, and a single name with no dot.
//!
//! **A host without a port means what a preset's host means on each harness**: on Claude Code
//! any port (its own `network.allowedDomains` rule), and through purlis's egress proxy, which
//! the opencode and Codex wraps reach the network through, HTTPS's port for a tunnel and HTTP's
//! for a plain request. A host with a port is that port alone, on every harness.
//!
//! **Levels** ([`Level`]). The **Project**'s hosts are `[sandbox] hosts` in the committed
//! `charter.toml`: everyone who opens the project follows them, with no approval, and each
//! teammate is told once when they change ([`super::local::hosts_changed`]). **You** are this
//! machine's: `[sandbox] hosts` in `charter.local.toml`, read only while git leaves that file
//! alone ([`crate::settings::layer_text`]). A **Persona**'s hosts (#1362) are a third level:
//! `[sandbox.personas.<name>] hosts` in the committed file, granted only to a chat running as
//! that persona ([`super::persona`]).
//!
//! **No chat writes either list.** Both files are later-code names a sandboxed chat never writes
//! ([`super::PLANTED`]): that denial is the boundary today. [`super::changes_a_sandbox_key`] is
//! the rule a brokered write asks once #1333 builds one.
//!
//! **Policy** (#1343) is [`Locks`] ([`super::policy`]): an admin's locks, which [`in_force`]
//! asks of every host before it is granted.

use std::fmt;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::path::Path;

/// The key in `[sandbox]` that holds the hosts, in both files.
pub const KEY: &str = "hosts";

/// One host a chat may reach: its name, and the one port it is held to, where it has one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Host {
    name: Name,
    port: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Name {
    Domain(String),
    /// `*.<suffix>`: every name under the suffix.
    Under(String),
    V4(Ipv4Addr),
    V6(Ipv6Addr),
}

/// What a host is written like, said where one is refused for being something else.
const LOOKS_LIKE: &str = "a domain such as api.example.com, *.example.com for every name under \
                          it, an IP address such as 10.0.0.5, or any of them with :port";

impl Host {
    /// `typed` as a host, or the one sentence saying why it is not one. Spaces around it are
    /// dropped, letters are lowered and a final dot is dropped.
    pub fn parse(typed: &str) -> Result<Self, String> {
        let typed = typed.trim();
        if typed.is_empty() {
            return Err(format!("Type a host: {LOOKS_LIKE}."));
        }
        if typed.chars().any(char::is_whitespace) {
            return Err("A host has no spaces in it.".to_owned());
        }
        if typed.chars().any(char::is_control) {
            return Err("A host has no control characters in it.".to_owned());
        }
        if let Some((_, rest)) = typed.split_once("://") {
            let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
            return Err(format!(
                "That is a URL. Type its host alone, such as {}, without the scheme or a path.",
                if host.is_empty() {
                    "api.example.com"
                } else {
                    host
                }
            ));
        }
        if let Some((before, _)) = typed.split_once('/') {
            if before.parse::<Ipv4Addr>().is_ok() || before.starts_with('[') {
                return Err(
                    "A range of addresses is not a host. Add each address the chats need."
                        .to_owned(),
                );
            }
            return Err(format!(
                "Leave out the path: a host ends before the first /, so type {before}."
            ));
        }
        if typed.contains('@') {
            return Err("Leave out the user name: type the host alone.".to_owned());
        }
        let lowered = typed.to_ascii_lowercase();
        let (name, port) = split(&lowered)?;
        let port = port.map(port_of).transpose()?;
        let name = name_of(name)?;
        Ok(Self { name, port })
    }

    /// **Whether `self`, as a policy lists it, allows `other`** (#1343): the same name, or
    /// for `*.domain` any name under it (a `*.` entry under it included); and any port where
    /// `self` names none, else that port alone.
    pub fn covers(&self, other: &Host) -> bool {
        let names = match (&self.name, &other.name) {
            (Name::Under(suffix), Name::Domain(name) | Name::Under(name)) => {
                name.strip_suffix(suffix.as_str())
                    .is_some_and(|head| head.ends_with('.'))
                    || matches!(&other.name, Name::Under(same) if same == suffix)
            }
            (mine, theirs) => mine == theirs,
        };
        names && self.port.is_none_or(|port| other.port == Some(port))
    }

    /// The address it is, where it is an address rather than a name.
    pub fn address(&self) -> Option<IpAddr> {
        match &self.name {
            Name::V4(ip) => Some(IpAddr::V4(*ip)),
            Name::V6(ip) => Some(IpAddr::V6(*ip)),
            Name::Domain(_) | Name::Under(_) => None,
        }
    }

    /// The port it is held to, where it has one.
    pub fn port(&self) -> Option<u16> {
        self.port
    }
}

/// The spelling every compiler writes: a domain or `*.domain`, an IPv4 address or a bracketed
/// IPv6 address, then `:port` where it has one. Claude Code's `network.allowedDomains` takes it
/// as written, and so does purlis's egress proxy ([`super::egress`]).
impl fmt::Display for Host {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.name {
            Name::Domain(name) => write!(f, "{name}")?,
            Name::Under(suffix) => write!(f, "*.{suffix}")?,
            Name::V4(ip) => write!(f, "{ip}")?,
            Name::V6(ip) => write!(f, "[{ip}]")?,
        }
        match self.port {
            Some(port) => write!(f, ":{port}"),
            None => Ok(()),
        }
    }
}

/// `typed`, lowered, as its name and the port written after it.
fn split(typed: &str) -> Result<(&str, Option<&str>), String> {
    if typed.starts_with('[') {
        let Some(close) = typed.find(']') else {
            return Err(format!(
                "{typed} opens a bracket and never closes it: an IPv6 address is written \
                 [fd00::1], or [fd00::1]:443."
            ));
        };
        let (name, after) = typed.split_at(close + 1);
        return match after {
            "" => Ok((name, None)),
            _ => after
                .strip_prefix(':')
                .map(|port| (name, Some(port)))
                .ok_or_else(port_refusal),
        };
    }
    match typed.matches(':').count() {
        0 => Ok((typed, None)),
        1 => {
            let (name, port) = typed.split_once(':').unwrap_or((typed, ""));
            Ok((name, Some(port)))
        }
        _ => Err(match typed.parse::<Ipv6Addr>() {
            Ok(ip) => format!(
                "Write an IPv6 address in brackets, such as [{ip}], or [{ip}]:443 with a port."
            ),
            Err(_) => format!("{typed} is not a host: {LOOKS_LIKE}."),
        }),
    }
}

fn port_refusal() -> String {
    "The port must be a number from 1 to 65535.".to_owned()
}

fn port_of(typed: &str) -> Result<u16, String> {
    if typed.is_empty() || !typed.bytes().all(|b| b.is_ascii_digit()) {
        return Err(port_refusal());
    }
    typed
        .parse::<u16>()
        .ok()
        .filter(|port| *port > 0)
        .ok_or_else(port_refusal)
}

fn name_of(name: &str) -> Result<Name, String> {
    if let Some(inside) = name.strip_prefix('[').and_then(|it| it.strip_suffix(']')) {
        let ip = inside
            .parse::<Ipv6Addr>()
            .map_err(|_| format!("{inside} is not an IPv6 address."))?;
        return v6(ip).map(Name::V6);
    }
    let name = name.strip_suffix('.').unwrap_or(name);
    if !name.is_empty() && name.bytes().all(|b| b.is_ascii_digit() || b == b'.') {
        return name
            .parse::<Ipv4Addr>()
            .map_err(|_| {
                format!(
                    "{name} is not an IPv4 address: that is four numbers from 0 to 255 with no \
                     leading zeros, such as 10.0.0.5."
                )
            })
            .and_then(v4)
            .map(Name::V4);
    }
    // The WHATWG URL rule: a name whose last part is a number is an address in another
    // notation (`0x7f.1` is 127.0.0.1 to a resolver), and a wildcard over one names addresses.
    if ends_in_a_number(name) {
        return Err(format!(
            "{name} ends in a number, so it is an address written another way, not a domain. \
             Type the address as four numbers, such as 10.0.0.5."
        ));
    }
    if name == "localhost" || name.ends_with(".localhost") {
        return Err(this_machine(name));
    }
    if let Some(suffix) = name.strip_prefix("*.") {
        domain(suffix)?;
        if !suffix.contains('.') {
            return Err(format!(
                "*.{suffix} covers a whole top-level domain. Name the domain under it, such as \
                 *.example.{suffix}."
            ));
        }
        return Ok(Name::Under(suffix.to_owned()));
    }
    if name.contains('*') {
        return Err(
            "A wildcard is written at the front, as *.example.com, for every name under \
             example.com."
                .to_owned(),
        );
    }
    domain(name)?;
    if !name.contains('.') {
        return Err(format!(
            "{name} is a single name. Type its full name, such as {name}.internal.example, or \
             its IP address: Claude Code's sandbox allows only a name with a dot."
        ));
    }
    Ok(Name::Domain(name.to_owned()))
}

/// Why `name` is not a domain, if it is not one.
fn domain(name: &str) -> Result<(), String> {
    if !name.is_ascii() {
        return Err(format!(
            "{name} has letters outside ASCII. Type an internationalised name in its xn-- \
             form, as DNS has it."
        ));
    }
    if name.len() > 253 {
        return Err("A domain is at most 253 characters long.".to_owned());
    }
    for label in name.split('.') {
        if label.is_empty() {
            return Err(format!(
                "{name} has an empty part between dots: each part of a domain has a letter or \
                 digit."
            ));
        }
        if label.len() > 63 {
            return Err(format!(
                "A part of {name} is longer than 63 characters, which no domain's part is."
            ));
        }
        if !label
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        {
            return Err(format!(
                "{name} is not a domain: a domain holds letters, digits, hyphens and dots only."
            ));
        }
        if label.starts_with('-') || label.ends_with('-') {
            return Err(format!(
                "{name} is not a domain: no part of a domain starts or ends with a hyphen."
            ));
        }
    }
    Ok(())
}

fn this_machine(name: &str) -> String {
    format!(
        "{name} is this machine. A chat's sandbox never reaches this machine's own services \
         as a host, since that would reach every one of them."
    )
}

/// Whether `name`'s last part is a number: all digits, or `0x` and hex digits.
fn ends_in_a_number(name: &str) -> bool {
    let last = name.rsplit('.').next().unwrap_or(name);
    if !last.is_empty() && last.bytes().all(|b| b.is_ascii_digit()) {
        return true;
    }
    last.strip_prefix("0x")
        .is_some_and(|hex| hex.bytes().all(|b| b.is_ascii_hexdigit()))
}

/// The cloud metadata addresses outside link-local that Claude Code's sandbox runtime refuses
/// (2.1.291): Alibaba's, Azure's wire server, and Oracle's.
const METADATA_V4: [Ipv4Addr; 3] = [
    Ipv4Addr::new(100, 100, 100, 200),
    Ipv4Addr::new(168, 63, 129, 16),
    Ipv4Addr::new(192, 0, 0, 192),
];

/// The IPv6 cloud metadata addresses the same runtime refuses, besides `fd00:ec2::/32` (AWS).
const METADATA_V6: [Ipv6Addr; 5] = [
    Ipv6Addr::new(0xfd20, 0xce, 0, 0, 0, 0, 0, 0x254),
    Ipv6Addr::new(0xfd00, 0xc1, 0, 0, 0, 0, 0xa9fe, 0xa9fe),
    Ipv6Addr::new(0xfd00, 0x42, 0, 0, 0, 0, 0, 0x42),
    Ipv6Addr::new(0xfd00, 0xa9fe, 0xa9fe, 0, 0, 0, 0, 1),
    Ipv6Addr::new(0xfd00, 0x100, 0, 0, 0, 0, 0x100, 0x200),
];

fn metadata_v4(ip: Ipv4Addr) -> bool {
    METADATA_V4.contains(&ip)
}

fn metadata_v6(ip: Ipv6Addr) -> bool {
    let [first, second, ..] = ip.segments();
    (first == 0xfd00 && second == 0x0ec2) || METADATA_V6.contains(&ip)
}

fn metadata(name: &str) -> String {
    format!("{name} is a cloud metadata address, so it is never a host a chat reaches.")
}

/// **Whether a chat is never let reach `ip`**, as a name resolves it (#1341): this machine
/// (loopback, `0.0.0.0/8`, the unspecified address, and each of `own`, this machine's
/// interface addresses), link-local, multicast and broadcast addresses, and the cloud metadata
/// addresses — the set Claude Code's sandbox runtime refuses. purlis's egress proxy asks it of
/// every address a listed name resolves to, at every connect, so a name that resolves, or is
/// rebound, to one of them reaches nothing.
pub fn refused_address(ip: IpAddr, own: &[IpAddr]) -> bool {
    if own.contains(&ip) {
        return true;
    }
    match ip {
        IpAddr::V4(ip) => {
            ip.is_loopback()
                || ip.octets()[0] == 0
                || ip.is_link_local()
                || ip.is_multicast()
                || ip.is_broadcast()
                || metadata_v4(ip)
        }
        IpAddr::V6(ip) => match embedded_v4(ip) {
            Some(carried) => refused_address(IpAddr::V4(carried), own),
            None => {
                ip.is_loopback()
                    || ip.is_unspecified()
                    || ip.is_unicast_link_local()
                    || ip.is_multicast()
                    || metadata_v6(ip)
            }
        },
    }
}

/// This machine's own interface addresses, as the system lists them now; none where it cannot.
pub fn own_addresses() -> Vec<IpAddr> {
    #[cfg(unix)]
    {
        let Ok(found) = nix::ifaddrs::getifaddrs() else {
            return Vec::new();
        };
        found
            .filter_map(|one| {
                let address = one.address?;
                address
                    .as_sockaddr_in()
                    .map(|v4| IpAddr::V4(v4.ip()))
                    .or_else(|| address.as_sockaddr_in6().map(|v6| IpAddr::V6(v6.ip())))
            })
            .collect()
    }
    #[cfg(not(unix))]
    Vec::new()
}

fn v4(ip: Ipv4Addr) -> Result<Ipv4Addr, String> {
    if ip.is_loopback() || ip.octets()[0] == 0 {
        return Err(this_machine(&ip.to_string()));
    }
    if metadata_v4(ip) {
        return Err(metadata(&ip.to_string()));
    }
    if ip.is_link_local() {
        return Err(link_local(&ip.to_string()));
    }
    if ip.is_broadcast() || ip.is_multicast() {
        return Err(not_one_machine(&ip.to_string()));
    }
    Ok(ip)
}

/// The IPv4 address an IPv6 address carries, in each form Claude Code's sandbox runtime
/// decodes (2.1.291): IPv4-mapped `::ffff:0:0/96`, SIIT `::ffff:0:0:0/96`, NAT64
/// `64:ff9b::/96`, IPv4-compatible `::/96` (not `::` or `::1`), and 6to4 `2002::/16`.
fn embedded_v4(ip: Ipv6Addr) -> Option<Ipv4Addr> {
    let s = ip.segments();
    let of = |hi: u16, lo: u16| {
        let ([a, b], [c, d]) = (hi.to_be_bytes(), lo.to_be_bytes());
        Ipv4Addr::new(a, b, c, d)
    };
    let low = || of(s[6], s[7]);
    if let Some(mapped) = ip.to_ipv4_mapped() {
        return Some(mapped);
    }
    match s {
        [0, 0, 0, 0, 0xffff, 0, _, _] => Some(low()),
        [0x64, 0xff9b, 0, 0, 0, 0, _, _] => Some(low()),
        [0, 0, 0, 0, 0, 0, _, _] if !ip.is_unspecified() && !ip.is_loopback() => Some(low()),
        [0x2002, hi, lo, ..] => Some(of(hi, lo)),
        _ => None,
    }
}

fn v6(ip: Ipv6Addr) -> Result<Ipv6Addr, String> {
    if ip.to_ipv4_mapped().is_some() {
        return Err(format!(
            "[{ip}] is an IPv4 address written as IPv6. Type it as IPv4, such as 10.0.0.5."
        ));
    }
    // An address that carries an IPv4 one reaches it, so it is judged as that one.
    if let Some(carried) = embedded_v4(ip) {
        v4(carried).map_err(|why| format!("[{ip}] carries the IPv4 address {carried}. {why}"))?;
    }
    if ip.is_loopback() || ip.is_unspecified() {
        return Err(this_machine(&format!("[{ip}]")));
    }
    if ip.is_unicast_link_local() {
        return Err(link_local(&format!("[{ip}]")));
    }
    if metadata_v6(ip) {
        return Err(metadata(&format!("[{ip}]")));
    }
    if ip.is_multicast() {
        return Err(not_one_machine(&format!("[{ip}]")));
    }
    Ok(ip)
}

fn link_local(name: &str) -> String {
    format!(
        "{name} is a link-local address, where cloud metadata services answer, so it is never \
         a host a chat reaches."
    )
}

fn not_one_machine(name: &str) -> String {
    format!("{name} is a broadcast or multicast address, not one machine.")
}

/// Who a host was added by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    /// The project's committed `charter.toml`: everyone who opens the project follows it.
    Project,
    /// This machine's `charter.local.toml`.
    You,
    /// The persona a chat runs as: `[sandbox.personas.<name>]` in the committed `charter.toml`
    /// (#1362).
    Persona,
}

impl Level {
    /// The word Settings and a Notice name it by.
    pub fn word(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::You => "you",
            Self::Persona => "persona",
        }
    }
}

/// A host a chat in this project may reach, and the level it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Granted {
    pub host: Host,
    pub level: Level,
}

pub use super::policy::Locks;

/// **The hosts in force**: the project's, then this machine's, then the persona's the chat runs
/// as, each once (where two levels list a host, it is the first's), less what `locks` refuse.
pub fn in_force(
    project: &[Host],
    personal: &[Host],
    persona: &[Host],
    locks: &Locks,
) -> Vec<Granted> {
    let mut out: Vec<Granted> = Vec::new();
    let levels = [
        (Level::Project, project),
        (Level::You, personal),
        (Level::Persona, persona),
    ];
    for (level, hosts) in levels {
        for host in hosts {
            if out.iter().any(|kept| kept.host == *host) {
                continue;
            }
            let granted = Granted {
                host: host.clone(),
                level,
            };
            if locks.refuses(&granted).is_none() {
                out.push(granted);
            }
        }
    }
    out
}

/// `granted` less each host that is one of `own`, this machine's interface addresses: a LAN or
/// VPN address of this machine reaches its own services, which no host may (#1341).
pub fn off_this_machine(granted: Vec<Granted>, own: &[IpAddr]) -> Vec<Granted> {
    granted
        .into_iter()
        .filter(|one| one.host.address().is_none_or(|ip| !own.contains(&ip)))
        .collect()
}

/// What a file's `[sandbox] hosts` holds.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Listed {
    /// Each host it names, once.
    pub hosts: Vec<Host>,
    /// Each entry it does not take, as TOML writes it, and why.
    pub refused: Vec<(String, String)>,
}

/// `hosts` that is not a list: no host of that level is taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotAList;

/// What a file's `[sandbox] hosts`, `value`, holds. Absent is no host.
pub fn read(value: Option<&toml::Value>) -> Result<Listed, NotAList> {
    let Some(value) = value else {
        return Ok(Listed::default());
    };
    let Some(entries) = value.as_array() else {
        return Err(NotAList);
    };
    let mut hosts: Vec<Host> = Vec::new();
    let mut refused = Vec::new();
    for entry in entries {
        match entry.as_str().map(Host::parse) {
            Some(Ok(host)) => {
                if !hosts.contains(&host) {
                    hosts.push(host);
                }
            }
            Some(Err(why)) => refused.push((entry.to_string(), why)),
            None => refused.push((
                entry.to_string(),
                "A host is written as text in quotes.".to_owned(),
            )),
        }
    }
    Ok(Listed { hosts, refused })
}

/// The hosts `top`'s `[sandbox] hosts` names, with nothing refused: what a reader takes.
pub fn of_table(top: Option<&toml::Table>) -> Vec<Host> {
    top.and_then(|top| top.get(super::TABLE))
        .and_then(toml::Value::as_table)
        .and_then(|table| read(table.get(KEY)).ok())
        .map(|listed| listed.hosts)
        .unwrap_or_default()
}

/// **This machine's hosts** for the project at `root`: `charter.local.toml`'s `[sandbox] hosts`,
/// each only once you confirmed it on this machine ([`confirmed_only`]).
///
/// **Confirmed, because nothing in git proves the file is yours.** A sandboxed chat can write
/// all of a clone's git state (its index, ignore files, objects and refs), so no check of git
/// can tell a host you added from one a teammate committed, or one a chat wrote. The record of
/// what you confirmed is kept in `.charter/app/sandbox.json`, which no sandboxed chat writes
/// (the integrity class), and only Settings writes it: adding a host there confirms it, and a
/// host the file holds that you did not add there is shown "not yet confirmed", with Confirm.
///
/// Read with the sandbox's own reader ([`super::read_plane_file`]): a link, or anything but a
/// regular file, grants nothing. A file git would carry grants nothing either.
pub fn personal(root: &Path) -> Vec<Host> {
    let path = crate::names::local_settings(root);
    let Ok(Some(text)) = super::read_plane_file(&path) else {
        return Vec::new();
    };
    if !crate::profiles::ignore_check(root).passes() {
        return Vec::new();
    }
    confirmed_only(
        of_table(text.parse::<toml::Table>().ok().as_ref()),
        &super::local::confirmed_hosts(root),
    )
}

/// The hosts of `listed` that `confirmed`, the record of what you confirmed, holds.
pub fn confirmed_only(listed: Vec<Host>, confirmed: &[String]) -> Vec<Host> {
    listed
        .into_iter()
        .filter(|host| confirmed.contains(&host.to_string()))
        .collect()
}
