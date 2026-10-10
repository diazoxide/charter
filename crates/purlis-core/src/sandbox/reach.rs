//! **What a chat may reach, decided once** (spec #1661, #1664): the core decision module every
//! adapter asks, given a host and port and the layers a chat's sandbox was compiled with.
//!
//! A [`Reach`] is each host a chat's sandbox lists, with the layer that lists it ([`By`]): the
//! **Open hosts** (the presets in force and the project's own hosts), the **Persona hosts** the
//! person on this machine allowed for the persona the chat runs as, and the **Allowed hosts** at
//! the scope they were kept at (this project on this machine, or only this chat). Where two
//! layers list a host, the first decides it, in that order.
//!
//! [`Reach::decide`] answers one [`Decision`]: open, persona, allowed (with its scope), ask, or
//! refused (with why). It goes by host and port alone: nothing a connection carries after its
//! first line is read, so there is no TLS interception and no certificate of purlis's own.
//!
//! **The local-address check** is part of the decision. A chat never reaches this machine (its
//! loopback, the unspecified address and each of its own interface addresses), a link-local
//! address or a cloud metadata service ([`super::hosts::refused_address`]), unless that exact
//! address and port is listed. purlis's egress proxy asks [`Reach::lists_exactly`] of every
//! address a name resolves to as well, so a name pointed at one of them reaches nothing.
//!
//! **Ask** is a host nothing lists that a person could allow. Until purlis holds a connection
//! while it asks (#1666), the proxy refuses it, and the chat's sandbox raises a Block with an
//! Allow on it.

use std::net::IpAddr;

use super::egress::{allows_on, entry_parts};
use super::hosts::{Host, refused_address};

/// The layer that lists a host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum By {
    /// A preset in force, or the project's own hosts: an **Open host**.
    Open,
    /// The persona the chat runs as, as the person on this machine allowed its list.
    Persona,
    /// An **Allowed host** kept for this project on this machine.
    You,
    /// An **Allowed host** kept for this chat alone.
    Chat,
}

/// Why a connection is refused outright, never asked about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refused {
    /// What the connection named is not something a person could allow: a single word, a
    /// wildcard, a malformed name. The sentence says why.
    NeverAHost(String),
    /// This machine, a link-local address or a cloud metadata service, by address or by what a
    /// name resolved to.
    LocalAddress,
}

/// What a chat's connection to a host and port is answered with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Open,
    Persona,
    /// An Allowed host, at the scope it was kept at ([`By::You`] or [`By::Chat`]).
    Allowed(By),
    /// Nothing lists it, and a person could allow it.
    Ask,
    Refused(Refused),
}

impl Decision {
    /// Whether the connection is carried.
    pub fn carries(&self) -> bool {
        matches!(self, Self::Open | Self::Persona | Self::Allowed(_))
    }

    /// The word the network record keeps it by.
    pub fn word(&self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Persona => "persona",
            Self::Allowed(By::You) => "you",
            Self::Allowed(By::Chat) => "chat",
            // Never built: an Open or a Persona listing decides as itself.
            Self::Allowed(By::Open) => "open",
            Self::Allowed(By::Persona) => "persona",
            Self::Ask => "ask",
            Self::Refused(_) => "refused",
        }
    }

    fn of(by: By) -> Self {
        match by {
            By::Open => Self::Open,
            By::Persona => Self::Persona,
            By::You | By::Chat => Self::Allowed(by),
        }
    }
}

/// Every host a chat's sandbox lists, each with the layer that lists it, in the order the
/// layers decide.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Reach {
    listed: Vec<(String, By)>,
}

impl Reach {
    /// `listed`: each entry a host as [`super::egress::allows_on`] reads one, put in the order
    /// the layers decide (Open, Persona, then Allowed for this project on this machine, then
    /// for this chat), each layer's entries as written.
    pub fn of(mut listed: Vec<(String, By)>) -> Self {
        listed.sort_by_key(|(_, by)| match by {
            By::Open => 0,
            By::Persona => 1,
            By::You => 2,
            By::Chat => 3,
        });
        Self { listed }
    }

    /// `hosts`, every one an Open host.
    pub fn open(hosts: Vec<String>) -> Self {
        Self::of(hosts.into_iter().map(|host| (host, By::Open)).collect())
    }

    /// Each host listed, once, in the order the layers decide.
    pub fn hosts(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for (host, _) in &self.listed {
            if !out.contains(host) {
                out.push(host.clone());
            }
        }
        out
    }

    /// **The decision** for a connection to `host` on `port`, where a host listed without a
    /// port is carried on `defaults`, and `own` is this machine's interface addresses.
    pub fn decide(&self, host: &str, port: u16, defaults: &[u16], own: &[IpAddr]) -> Decision {
        let host = host.strip_suffix('.').unwrap_or(host);
        let literal = host.parse::<IpAddr>().ok();
        if let Some(ip) = literal
            && refused_address(ip, own)
        {
            return match self.exactly(ip, port) {
                Some(by) => Decision::of(by),
                None => Decision::Refused(Refused::LocalAddress),
            };
        }
        if let Some((_, by)) = self
            .listed
            .iter()
            .find(|(entry, _)| allows_on(std::slice::from_ref(entry), host, port, defaults))
        {
            return Decision::of(*by);
        }
        let named = super::egress::host_and_port(host, port);
        match Host::parse(&named) {
            Err(why) => Decision::Refused(Refused::NeverAHost(why)),
            Ok(parsed) if parsed.to_string().starts_with("*.") => Decision::Refused(
                Refused::NeverAHost(format!("{named} names many hosts, not one.")),
            ),
            Ok(_) => Decision::Ask,
        }
    }

    /// Whether `ip` on `port` is listed as exactly that address and that port: the one way a
    /// local address is ever carried.
    pub fn lists_exactly(&self, ip: IpAddr, port: u16) -> bool {
        self.exactly(ip, port).is_some()
    }

    fn exactly(&self, ip: IpAddr, port: u16) -> Option<By> {
        self.listed.iter().find_map(|(entry, by)| {
            let (name, held) = entry_parts(entry);
            (held == Some(port) && name.parse::<IpAddr>().is_ok_and(|listed| listed == ip))
                .then_some(*by)
        })
    }
}
