//! purlis's loopback egress proxy (ADR 0067 §3 as amended 2026-10-10): what a harness purlis
//! wraps reaches the network through, where the harness has no proxy of its own.
//!
//! **One chat, one pair of ports** (#1664). Each chat purlis wraps gets its own proxy, made when
//! the chat starts and gone when it ends ([`Proxy`] stops listening when dropped): an HTTP port
//! ([`Proxy::port`]) and a SOCKS5 port ([`Proxy::socks_port`]). The wrap lets the chat connect to
//! those two ports on the loopback interface and nowhere else, so a program that ignores
//! `HTTPS_PROXY` reaches nothing, and one chat never reaches another's ports. **Which chat a
//! connection is from is which proxy it arrived on**: nothing a connection sends names a chat,
//! and nothing it sends is read as one.
//!
//! The HTTP port carries:
//!
//! - **`CONNECT host:443`**, the tunnel every HTTPS client asks a proxy for, to a listed host.
//!   The bytes inside the tunnel are the client's TLS, which the proxy does not read, so what
//!   host a TLS client names inside it (SNI) is the client's to say, as with any proxy that
//!   does not decrypt. There is no TLS interception and no certificate of purlis's own.
//! - **A plain request in absolute form** (`GET http://host/path`) to a listed host on port 80,
//!   sent on in origin form with `Connection: close`, so one connection reaches one host. Its
//!   one `Host` must name the same host, and its body must state its length: the body is
//!   carried and nothing after it, so a second request cannot ride the same connection.
//!
//! The SOCKS5 port carries a `CONNECT` with no authentication (RFC 1928), to a name or an
//! address, on the ports a tunnel is carried to. Every other method, command and address type
//! is refused with SOCKS's own reply.
//!
//! **The head is read strictly**, so the proxy and the host it reaches cannot read it two ways:
//! CRLF line ends only, one space between the request line's three parts, `HTTP/1.1` or
//! `HTTP/1.0`, header names of token characters with no space before the colon, no folded
//! header, at most one `Host` and one `Content-Length` (digits only), and no
//! `Transfer-Encoding`. A head that breaks any of these is refused.
//!
//! **What is carried is decided by the core decision module** ([`super::reach`]): by host and
//! port only, open, persona, allowed, ask or refused. Anything not carried is refused before a
//! connection is made: a host nothing lists (an ask, held while the person is asked where the
//! chat's proxy has a board of live asks, [`super::asks`], #1666, and refused where it has
//! none or nobody answered in time), a tunnel to any port but 443, a plain request to any port but 80, and
//! a request that is not one of the two above.
//!
//! **Bounded** ([`Limits`]): at most a few connections at once are served, over both ports
//! together; a head or a SOCKS greeting must arrive whole within a deadline; a connection that
//! carries nothing either way for long is closed; a write the other side does not take in that
//! time ends it; and a failing accept backs off rather than spinning. No buffer grows past a
//! fixed size.
//!
//! **The local-address check.** The proxy resolves each name itself, and a name is reached only
//! at the addresses it resolves to that a chat may reach ([`reachable`]): never this machine, a
//! link-local, multicast or broadcast address, or a cloud metadata service, checked at every
//! connect, so a name that resolves or is rebound there reaches nothing. A name that resolves
//! *only* there is refused as a Block. An address literal is held to the same check: the one
//! exception is that exact address and port, listed ([`super::reach::Reach::lists_exactly`]).
//! A listed name is a name, so an address literal is never carried unless a preset or a host
//! lists it, and a wildcard never matches one.
//!
//! **A host listed with a port** (`10.0.0.5:6443`, a project's or a person's, #1341) is carried
//! on that port alone, tunnel or plain; a host without one on the ports above.
//!
//! **What it refused is its own record** ([`Refusals`]): each host and port it refused because
//! no preset or host lists it, as the request named them, once each and at most
//! [`REFUSALS_KEPT`]. A brokered `secret exec` reads it to tell the asking chat which host its
//! command was refused (`crate::secrets::brokered`), and the app raises the chat's Block from
//! it: the proxy's word, never the command's.
//!
//! **Every connection is told** ([`Reached`]): each connection carried or refused, by host and
//! port and the decision's word, coalesced to a line per host and port a minute ([`Tally`]), so
//! the app keeps every connection in the network record without a chat being able to flood it.

use std::io::{self, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use super::asks::{Answer, Asks};
use super::reach::{Decision, Reach, Refused};

/// The port a tunnel is carried to: HTTPS's.
pub const TUNNEL_PORTS: [u16; 1] = [443];

/// The port a plain request is carried to: HTTP's.
pub const PLAIN_PORTS: [u16; 1] = [80];

/// The most distinct hosts a proxy's [`Refusals`] keeps: a command that tries a thousand hosts
/// is one pattern, not a thousand Notices.
pub const REFUSALS_KEPT: usize = 8;

/// The most a request's head may be before it is refused.
const HEAD_MAX: usize = 16 * 1024;

/// How long a client has to send its request, and a host to answer the connection.
const PATIENCE: Duration = Duration::from_secs(30);

/// How long a failing accept waits before it tries again.
const BACKOFF: Duration = Duration::from_millis(100);

/// What one proxy takes on at once, and how long a connection may carry nothing.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// Connections served at once; one more is answered `503` and closed.
    pub connections: usize,
    /// How long a connection may carry nothing in either direction before it is closed, and a
    /// write may wait for the other side to take it.
    pub idle: Duration,
    /// How long a client has to send its request's whole head.
    pub head: Duration,
}

/// What a chat's proxy takes on: a harness opens a handful of connections, and a model's
/// answer streams far more often than this.
pub const LIMITS: Limits = Limits {
    connections: 64,
    idle: Duration::from_secs(600),
    head: Duration::from_secs(30),
};

/// Whether `host` is one `listed` names: the same name, ignoring case and a final dot, or a
/// name under a listed `*.suffix` (never the suffix itself).
pub fn allows(listed: &[String], host: &str) -> bool {
    let host = host.strip_suffix('.').unwrap_or(host).to_ascii_lowercase();
    if host.is_empty() {
        return false;
    }
    listed.iter().any(|listed| {
        let listed = listed.to_ascii_lowercase();
        match listed.strip_prefix("*.") {
            // A wildcard names names, never an address literal (#1341).
            Some(_) if host.parse::<std::net::IpAddr>().is_ok() => false,
            Some(suffix) => host
                .strip_suffix(suffix)
                .is_some_and(|head| head.len() > 1 && head.ends_with('.')),
            None => host == listed,
        }
    })
}

/// Whether `host` on `port` is one `listed` names (#1341): an entry with a port (`host:port`,
/// `[v6]:port`) on that port alone, and one without on `defaults`, the ports the proxy carries
/// for what a preset lists. An IPv6 address is matched as an address, whatever its spelling.
pub fn allows_on(listed: &[String], host: &str, port: u16, defaults: &[u16]) -> bool {
    listed.iter().any(|entry| {
        let (name, held) = entry_parts(entry);
        let on = match held {
            Some(held) => held == port,
            None => defaults.contains(&port),
        };
        on && match (
            name.parse::<std::net::Ipv6Addr>(),
            host.parse::<std::net::Ipv6Addr>(),
        ) {
            (Ok(listed), Ok(asked)) => listed == asked,
            _ => allows(&[name.to_owned()], host),
        }
    })
}

/// A listed entry's name and the port it is held to, where it has one: `[v6]` and `[v6]:port`,
/// `name:port` with one colon, or the name alone.
pub(crate) fn entry_parts(entry: &str) -> (&str, Option<u16>) {
    if let Some(rest) = entry.strip_prefix('[')
        && let Some((inside, after)) = rest.split_once(']')
    {
        return (
            inside,
            after.strip_prefix(':').and_then(|port| port.parse().ok()),
        );
    }
    match entry.split_once(':') {
        Some((name, port)) if !port.contains(':') => match port.parse() {
            Ok(port) => (name, Some(port)),
            // Not a port: never a match, rather than the name on every port.
            Err(_) => ("", None),
        },
        _ => (entry, None),
    }
}

/// What is told each host and port a proxy refuses, the first time it refuses it.
pub type Told = Arc<dyn Fn(&str, u16) + Send + Sync + 'static>;

/// **The hosts a proxy refused because nothing lists them**: each host and port once, in the
/// order refused, at most [`REFUSALS_KEPT`], and told to whoever is listening as it happens.
/// A request the proxy could not read, or one whose `Host` names another host, is not one: no
/// grant would let it through.
#[derive(Clone, Default)]
pub struct Refusals {
    kept: Kept,
    told: Option<Told>,
    /// Told each host and port refused by the local-address check (#1664), once each and at
    /// most [`REFUSALS_KEPT`]: never one a person could allow, so never kept in [`Self::refused`].
    local: Option<(Told, Kept)>,
}

/// Hosts and ports a [`Refusals`] has told, shared by its clones.
type Kept = Arc<Mutex<Vec<(String, u16)>>>;

impl std::fmt::Debug for Refusals {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Refusals")
            .field("kept", &self.refused())
            .finish_non_exhaustive()
    }
}

impl Refusals {
    /// A record that tells `told` each refusal it keeps, as it keeps it.
    pub fn telling(told: Told) -> Self {
        Self {
            kept: Arc::default(),
            told: Some(told),
            local: None,
        }
    }

    /// This record, telling `told` each host and port the local-address check refuses as well.
    #[must_use]
    pub fn telling_local(self, told: Told) -> Self {
        Self {
            local: Some((told, Arc::default())),
            ..self
        }
    }

    /// Tells `host` on `port` as refused by the local-address check, unless it was told already
    /// or [`REFUSALS_KEPT`] were.
    pub fn heard_local(&self, host: &str, port: u16) {
        let Some((told, seen)) = &self.local else {
            return;
        };
        if !once(seen, host, port) {
            return;
        }
        told(host, port);
    }

    /// Keeps `host` on `port` as refused, and tells it, unless it is kept already or the
    /// record is full.
    pub fn heard(&self, host: &str, port: u16) {
        if !once(&self.kept, host, port) {
            return;
        }
        if let Some(told) = &self.told {
            told(host, port);
        }
    }

    /// Each refused host and port, as one names it to a grant: `host:port`, an IPv6 address
    /// in brackets.
    pub fn refused(&self) -> Vec<String> {
        self.kept
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .map(|(host, port)| host_and_port(host, *port))
            .collect()
    }
}

/// Keeps `host` on `port` in `kept`, and says whether it was new and there was room.
fn once(kept: &Mutex<Vec<(String, u16)>>, host: &str, port: u16) -> bool {
    let mut kept = kept
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if kept.len() >= REFUSALS_KEPT
        || kept
            .iter()
            .any(|(h, p)| *p == port && h.eq_ignore_ascii_case(host))
    {
        return false;
    }
    kept.push((host.to_owned(), port));
    true
}

/// `host` on `port` as a grant names it: `host:port`, an IPv6 address in brackets.
pub fn host_and_port(host: &str, port: u16) -> String {
    if host.contains(':') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    }
}

/// What is told each connection a proxy carried or refused, coalesced by its [`Tally`]: the host
/// and port, the decision's word ([`super::reach::Decision::word`]: the layer that let it
/// through, or `ask` or `refused`), and how many connections the line stands for. A target of `None` stands for hosts past what one tally tells apart.
pub type Reached = Arc<dyn Fn(Option<&str>, &'static str, u64) + Send + Sync + 'static>;

/// How long one tally line stands for: a host and port is told once a [`TALLY_WINDOW`], with
/// how many connections went to it since it was last told.
pub const TALLY_WINDOW: Duration = Duration::from_secs(60);

/// The most hosts and ports one tally tells apart at once; connections past it are counted
/// together, with no target.
pub const TALLY_HOSTS: usize = 32;

/// **Every connection a proxy carried, coalesced** (#1664): the first to a host and port is told
/// at once; those after it in the same [`TALLY_WINDOW`] are counted and told as one line when
/// the window has passed (at the next connection to it) or when the proxy stops. So a chat that
/// opens a thousand connections to its registry is a line or two a minute, never a thousand,
/// and the network record keeps every connection without a chat being able to turn it over.
#[derive(Debug, Default)]
pub struct Tally {
    seen: Vec<Seen>,
    /// Connections past [`TALLY_HOSTS`] since they were last told, per layer, and when they
    /// began.
    others: Vec<(&'static str, Instant, u64)>,
}

#[derive(Debug)]
struct Seen {
    target: String,
    by: &'static str,
    since: Instant,
    /// Connections since it was last told.
    untold: u64,
}

/// One line a [`Tally`] tells: a target (none for the hosts past what it tells apart), the
/// layer, and how many connections.
pub type Line = (Option<String>, &'static str, u64);

impl Tally {
    /// A connection to `target`, let through by `by`, at `now`: the lines to tell now.
    pub fn heard(&mut self, target: &str, by: &'static str, now: Instant) -> Vec<Line> {
        let mut out = Vec::new();
        // What has waited a window is told, and a host told with nothing since is let go.
        self.seen.retain_mut(|seen| {
            if now.saturating_duration_since(seen.since) < TALLY_WINDOW {
                return true;
            }
            if seen.untold > 0 {
                out.push((Some(seen.target.clone()), seen.by, seen.untold));
            }
            false
        });
        self.others.retain(|(layer, since, untold)| {
            if now.saturating_duration_since(*since) < TALLY_WINDOW {
                return true;
            }
            out.push((None, layer, *untold));
            false
        });
        if let Some(seen) = self
            .seen
            .iter_mut()
            .find(|seen| seen.by == by && seen.target.eq_ignore_ascii_case(target))
        {
            seen.untold += 1;
        } else if self.seen.len() < TALLY_HOSTS {
            self.seen.push(Seen {
                target: target.to_owned(),
                by,
                since: now,
                untold: 0,
            });
            out.push((Some(target.to_owned()), by, 1));
        } else if let Some((_, _, untold)) = self.others.iter_mut().find(|(layer, ..)| *layer == by)
        {
            *untold += 1;
        } else {
            self.others.push((by, now, 1));
        }
        out
    }

    /// Everything not told yet, as the proxy stops.
    pub fn ended(&mut self) -> Vec<Line> {
        let mut out: Vec<Line> = self
            .seen
            .drain(..)
            .filter(|seen| seen.untold > 0)
            .map(|seen| (Some(seen.target), seen.by, seen.untold))
            .collect();
        out.extend(
            self.others
                .drain(..)
                .map(|(layer, _, untold)| (None, layer, untold)),
        );
        out
    }
}

/// What a proxy carries, and to whom it tells what it did.
#[derive(Clone)]
pub struct Serving {
    /// Each host the chat's sandbox lists, with its layer.
    pub reach: Reach,
    /// The ports a tunnel (HTTP `CONNECT`, SOCKS5) is carried to for a host listed without one.
    pub tunnel_ports: Vec<u16>,
    /// The ports a plain request is carried to for a host listed without one.
    pub plain_ports: Vec<u16>,
    /// Whether a host listed without a port is carried on every port instead (#1665): what a
    /// Claude Code chat's own proxy did, by name alone ([`Reach::decide_on_any_port`]).
    pub any_port: bool,
    pub limits: Limits,
    pub refusals: Refusals,
    /// Told each connection carried, coalesced ([`Tally`]); none tells nobody.
    pub reached: Option<Reached>,
    /// The chat's live asks (#1666): a connection to a host nothing lists is held while the
    /// person is asked. None refuses it at once, as where policy turns asking off.
    pub asks: Option<Arc<Asks>>,
}

impl Serving {
    /// A chat's proxy for `reach`: tunnels on [`TUNNEL_PORTS`], plain requests on
    /// [`PLAIN_PORTS`], within [`LIMITS`], keeping nothing.
    pub fn of(reach: Reach) -> Self {
        Self {
            reach,
            tunnel_ports: TUNNEL_PORTS.to_vec(),
            plain_ports: PLAIN_PORTS.to_vec(),
            any_port: false,
            limits: LIMITS,
            refusals: Refusals::default(),
            reached: None,
            asks: None,
        }
    }
}

/// A running proxy, on a pair of ports of the loopback interface: HTTP and SOCKS5. It stops
/// listening on both when dropped, and tells what its tally still holds; a tunnel already open
/// runs until either end closes it.
pub struct Proxy {
    http: SocketAddr,
    socks: SocketAddr,
    ports: Vec<u16>,
    stop: Arc<AtomicBool>,
    refusals: Refusals,
    allowed: Arc<Allowed>,
}

impl std::fmt::Debug for Proxy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Proxy")
            .field("http", &self.http)
            .field("socks", &self.socks)
            .field("ports", &self.ports)
            .finish_non_exhaustive()
    }
}

impl Proxy {
    /// A proxy that tunnels to `hosts` on [`TUNNEL_PORTS`] and carries plain requests to them
    /// on [`PLAIN_PORTS`].
    pub fn start(hosts: Vec<String>) -> io::Result<Self> {
        Self::start_keeping(hosts, Refusals::default())
    }

    /// [`Self::start`], keeping what it refuses in `refusals`.
    pub fn start_keeping(hosts: Vec<String>, refusals: Refusals) -> io::Result<Self> {
        Self::serving(Serving {
            refusals,
            ..Serving::of(Reach::open(hosts))
        })
    }

    /// A proxy that carries `hosts` on `ports`, tunnels and plain requests alike.
    pub fn allowing(hosts: Vec<String>, ports: Vec<u16>) -> io::Result<Self> {
        Self::limited(hosts, ports, LIMITS)
    }

    /// A proxy that carries `hosts` on `ports`, tunnels and plain requests alike, within
    /// `limits`.
    pub fn limited(hosts: Vec<String>, ports: Vec<u16>, limits: Limits) -> io::Result<Self> {
        Self::serving(Serving {
            tunnel_ports: ports.clone(),
            plain_ports: ports,
            limits,
            ..Serving::of(Reach::open(hosts))
        })
    }

    /// **A chat's proxy**: two new ports on the loopback interface, serving `serving`.
    pub fn serving(serving: Serving) -> io::Result<Self> {
        let http = TcpListener::bind(("127.0.0.1", 0))?;
        let socks = TcpListener::bind(("127.0.0.1", 0))?;
        let (http_addr, socks_addr) = (http.local_addr()?, socks.local_addr()?);
        let stop = Arc::new(AtomicBool::new(false));
        let ports: Vec<u16> = serving
            .tunnel_ports
            .iter()
            .chain(&serving.plain_ports)
            .copied()
            .collect();
        let limits = serving.limits;
        let allowed = Arc::new(Allowed {
            reach: serving.reach,
            tunnel_ports: serving.tunnel_ports,
            plain_ports: serving.plain_ports,
            any_port: serving.any_port,
            idle: limits.idle,
            head: limits.head,
            refusals: serving.refusals.clone(),
            reached: serving.reached,
            asks: serving.asks,
            tally: Mutex::new(Tally::default()),
        });
        // One count for both ports: the limit is the chat's, whichever port it uses.
        let open = Arc::new(AtomicUsize::new(0));
        listen(http, Speaks::Http, &allowed, &open, &stop, limits)?;
        listen(socks, Speaks::Socks, &allowed, &open, &stop, limits)?;
        Ok(Self {
            http: http_addr,
            socks: socks_addr,
            ports,
            stop,
            refusals: serving.refusals,
            allowed,
        })
    }

    /// What it has refused because nothing lists it.
    pub fn refusals(&self) -> &Refusals {
        &self.refusals
    }

    /// The loopback port its HTTP proxy listens on.
    pub fn port(&self) -> u16 {
        self.http.port()
    }

    /// The loopback port its SOCKS5 proxy listens on.
    pub fn socks_port(&self) -> u16 {
        self.socks.port()
    }

    /// The ports it carries.
    pub fn ports(&self) -> &[u16] {
        &self.ports
    }

    /// The URL a chat's `HTTPS_PROXY` and `HTTP_PROXY` name it by.
    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port())
    }
}

impl Drop for Proxy {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        // Wakes each accept, which then sees the stop and closes its listener.
        for addr in [self.http, self.socks] {
            let _ = TcpStream::connect_timeout(&addr, Duration::from_secs(1));
        }
        self.allowed.tell(|tally| tally.ended());
    }
}

/// Which protocol a port speaks.
#[derive(Debug, Clone, Copy)]
enum Speaks {
    Http,
    Socks,
}

/// Accepts on `listener` until `stop`, serving each connection on a thread of its own while
/// fewer than `limits.connections` are open across the proxy's ports.
fn listen(
    listener: TcpListener,
    speaks: Speaks,
    allowed: &Arc<Allowed>,
    open: &Arc<AtomicUsize>,
    stop: &Arc<AtomicBool>,
    limits: Limits,
) -> io::Result<()> {
    let (allowed, open, stopping) = (Arc::clone(allowed), Arc::clone(open), Arc::clone(stop));
    std::thread::Builder::new()
        .name("purlis-egress".into())
        .spawn(move || {
            for stream in listener.incoming() {
                if stopping.load(Ordering::SeqCst) {
                    break;
                }
                let Ok(mut stream) = stream else {
                    std::thread::sleep(BACKOFF);
                    continue;
                };
                if open.fetch_add(1, Ordering::SeqCst) >= limits.connections {
                    open.fetch_sub(1, Ordering::SeqCst);
                    let _ = stream.set_write_timeout(Some(BACKOFF));
                    match speaks {
                        Speaks::Http => answer(
                            &mut stream,
                            "503 Service Unavailable",
                            "purlis's egress proxy is carrying all the connections it takes",
                        ),
                        // Not knowing yet what the client asks, the greeting is answered with
                        // no acceptable method, and the connection closed.
                        Speaks::Socks => {
                            let _ = stream.write_all(&[SOCKS_VERSION, NO_METHOD]);
                            let _ = stream.shutdown(Shutdown::Both);
                        }
                    }
                    continue;
                }
                let held = Held(Arc::clone(&open));
                let allowed = Arc::clone(&allowed);
                let _ = std::thread::Builder::new()
                    .name("purlis-egress-conn".into())
                    .spawn(move || {
                        let _held = held;
                        match speaks {
                            Speaks::Http => serve(stream, &allowed),
                            Speaks::Socks => serve_socks(stream, &allowed),
                        }
                    });
            }
        })?;
    Ok(())
}

/// One connection being served, counted until it ends.
struct Held(Arc<AtomicUsize>);

impl Drop for Held {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

struct Allowed {
    reach: Reach,
    tunnel_ports: Vec<u16>,
    plain_ports: Vec<u16>,
    any_port: bool,
    idle: Duration,
    head: Duration,
    refusals: Refusals,
    reached: Option<Reached>,
    asks: Option<Arc<Asks>>,
    tally: Mutex<Tally>,
}

impl Allowed {
    /// The decision for a connection to `host` on `port`, a tunnel or not.
    fn decide(&self, host: &str, port: u16, tunnel: bool, own: &[std::net::IpAddr]) -> Decision {
        if self.any_port {
            return self.reach.decide_on_any_port(host, port, own);
        }
        let ports = if tunnel {
            &self.tunnel_ports
        } else {
            &self.plain_ports
        };
        self.reach.decide(host, port, ports, own)
    }

    /// **The decision, once the person had their say** (#1666): an ask is held on the chat's
    /// board while the person is asked, and carried as an Allowed host at the scope they chose;
    /// refused where they kept it blocked or nobody answered in time. Either refusal is in the
    /// record's tally, and raises no second Block: the ask's Notice stays up. `None` is the
    /// decision as it was: carry it, or refuse it as [`Self::refused`] does.
    fn asked(
        &self,
        host: &str,
        port: u16,
        tunnel: bool,
        decision: Decision,
    ) -> Result<Decision, GaveUp> {
        let (Decision::Ask, Some(asks)) = (&decision, &self.asks) else {
            return Ok(decision);
        };
        let ports = if self.any_port {
            vec![port]
        } else if tunnel {
            self.tunnel_ports.clone()
        } else {
            self.plain_ports.clone()
        };
        match asks.hold(host, port, &ports) {
            Answer::Allowed(by) => Ok(Decision::by(by)),
            // The board is full: refused as before, with a Block of its own.
            Answer::Busy => Ok(decision),
            answer => {
                let target = host_and_port(host, port);
                self.tell(|tally| tally.heard(&target, decision.word(), Instant::now()));
                Err(if answer == Answer::TimedOut {
                    GaveUp::TimedOut
                } else {
                    GaveUp::KeptBlocked
                })
            }
        }
    }

    /// Tells what `step` makes of the tally, outside its lock.
    fn tell(&self, step: impl FnOnce(&mut Tally) -> Vec<Line>) {
        let Some(reached) = &self.reached else { return };
        let lines = step(
            &mut self
                .tally
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        );
        for (target, by, times) in lines {
            reached(target.as_deref(), by, times);
        }
    }

    /// A connection to `host` on `port` carried, by `decision`.
    fn carried(&self, host: &str, port: u16, decision: &Decision) {
        let target = host_and_port(host, port);
        self.tell(|tally| tally.heard(&target, decision.word(), Instant::now()));
    }

    /// The decision's refusal of `host` on `port`, told where it is told: an ask, or a host
    /// that could never be one, as nothing lists it; a local address as such. Counted in the
    /// tally too, so every refused connection is in the record, not only the first a Block is
    /// raised for.
    fn refused(&self, host: &str, port: u16, decision: &Decision) {
        let target = host_and_port(host, port);
        self.tell(|tally| tally.heard(&target, decision.word(), Instant::now()));
        match decision {
            Decision::Refused(Refused::LocalAddress) => self.refusals.heard_local(host, port),
            _ => self.refusals.heard(host, port),
        }
    }
}

/// How a held connection ended without being carried (#1666).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GaveUp {
    TimedOut,
    KeptBlocked,
}

/// Why a held connection was refused, as the client is told: fixed words, so a chat reads what
/// to do next.
fn held_said(host: &str, port: u16, held: GaveUp) -> String {
    match held {
        GaveUp::TimedOut => format!(
            "purlis's sandbox does not allow {host}:{port} yet: purlis asked the person while \
             this connection waited, and nobody answered in time. The ask stays with them; once \
             they allow it, run the command again"
        ),
        GaveUp::KeptBlocked => format!(
            "purlis's sandbox does not allow {host}:{port}: the person kept it blocked. Do not \
             try it again unless they ask"
        ),
    }
}

/// Why a decision refused, as the client is told.
fn refusal_said(host: &str, port: u16, decision: &Decision) -> String {
    match decision {
        Decision::Refused(Refused::Policy) => format!(
            "purlis's sandbox does not allow {host}:{port}: an administrator's policy pins it as \
             never allowed"
        ),
        Decision::Refused(Refused::LocalAddress) => format!(
            "purlis's sandbox does not allow {host}:{port}: it is this machine, a link-local \
             address or a cloud metadata service"
        ),
        _ => format!(
            "purlis's sandbox does not allow {host}:{port}: no egress preset or host of this \
             project lists it"
        ),
    }
}

/// One client connection: its request is read, judged, and either carried or refused.
fn serve(mut client: TcpStream, allowed: &Allowed) {
    let _ = client.set_write_timeout(Some(allowed.idle.min(PATIENCE)));
    let Some((head, rest)) = read_head(&mut client, allowed.head) else {
        answer(
            &mut client,
            "400 Bad Request",
            "purlis's egress proxy: no request",
        );
        return;
    };
    let Some(request) = Request::parse(&head) else {
        answer(
            &mut client,
            "400 Bad Request",
            "purlis's egress proxy carries CONNECT, and plain absolute-form requests whose \
             body states its length, only",
        );
        return;
    };
    if let Some(named) = &request.host_header
        && !same_host(named, &request.host, request.port)
    {
        answer(
            &mut client,
            "403 Forbidden",
            &format!(
                "purlis's egress proxy carries a request to {} only when its Host names it",
                request.host
            ),
        );
        return;
    }
    // This machine's addresses, read once for the decision and the connect.
    let own = super::hosts::own_addresses();
    let tunnel = request.forward.is_none();
    let decision = allowed.decide(&request.host, request.port, tunnel, &own);
    let decision = match allowed.asked(&request.host, request.port, tunnel, decision) {
        Ok(decision) => decision,
        Err(held) => {
            answer(
                &mut client,
                "403 Forbidden",
                &held_said(&request.host, request.port, held),
            );
            return;
        }
    };
    if !decision.carries() {
        allowed.refused(&request.host, request.port, &decision);
        answer(
            &mut client,
            "403 Forbidden",
            &refusal_said(&request.host, request.port, &decision),
        );
        return;
    }
    let _ = client.set_write_timeout(Some(allowed.idle));
    let mut upstream = match connect(&request.host, request.port, &allowed.reach, &own) {
        Connected::To(upstream) => upstream,
        Connected::OnlyLocal => {
            let local = Decision::Refused(Refused::LocalAddress);
            allowed.refused(&request.host, request.port, &local);
            answer(
                &mut client,
                "403 Forbidden",
                &refusal_said(&request.host, request.port, &local),
            );
            return;
        }
        Connected::Not => {
            answer(
                &mut client,
                "502 Bad Gateway",
                &format!("purlis's egress proxy could not reach {}", request.host),
            );
            return;
        }
    };
    let _ = upstream.set_write_timeout(Some(allowed.idle));
    let sent = match &request.forward {
        None => client.write_all(b"HTTP/1.1 200 Connection established\r\n\r\n"),
        Some(head) => upstream.write_all(head.as_bytes()),
    };
    if sent.is_err() {
        return;
    }
    allowed.carried(&request.host, request.port, &decision);
    // A plain request's body, and nothing after it; a tunnel's bytes, all of them.
    let up_to = request.body.unwrap_or(u64::MAX);
    let rest = &rest[..rest.len().min(usize::try_from(up_to).unwrap_or(usize::MAX))];
    if upstream.write_all(rest).is_err() {
        return;
    }
    splice(client, upstream, up_to - rest.len() as u64, allowed.idle);
}

/// SOCKS's version byte (RFC 1928).
const SOCKS_VERSION: u8 = 5;

/// The one method served: no authentication. A port is one chat's, so nothing a client sends
/// is asked to say whose it is.
const NO_AUTH: u8 = 0;

/// The method reply that refuses every method offered.
const NO_METHOD: u8 = 0xff;

/// SOCKS's reply codes the proxy answers with.
mod reply {
    pub const SUCCEEDED: u8 = 0;
    pub const NOT_ALLOWED: u8 = 2;
    pub const HOST_UNREACHABLE: u8 = 4;
    pub const COMMAND_NOT_SUPPORTED: u8 = 7;
    pub const ADDRESS_NOT_SUPPORTED: u8 = 8;
}

/// One SOCKS5 client connection: the greeting, one `CONNECT`, judged as a tunnel is, then
/// carried or refused with SOCKS's own reply. The whole of the greeting and the request must
/// arrive within the head's deadline, and neither is more than a few hundred bytes.
fn serve_socks(mut client: TcpStream, allowed: &Allowed) {
    let _ = client.set_write_timeout(Some(allowed.idle.min(PATIENCE)));
    let deadline = Instant::now() + allowed.head;
    let Some(target) = socks_request(&mut client, deadline) else {
        let _ = client.shutdown(Shutdown::Both);
        return;
    };
    let (host, port) = match target {
        Ok(target) => target,
        Err(code) => return socks_reply(&mut client, code),
    };
    let own = super::hosts::own_addresses();
    let decision = match allowed.asked(&host, port, true, allowed.decide(&host, port, true, &own)) {
        Ok(decision) => decision,
        Err(_) => return socks_reply(&mut client, reply::NOT_ALLOWED),
    };
    if !decision.carries() {
        allowed.refused(&host, port, &decision);
        return socks_reply(&mut client, reply::NOT_ALLOWED);
    }
    let _ = client.set_write_timeout(Some(allowed.idle));
    let upstream = match connect(&host, port, &allowed.reach, &own) {
        Connected::To(upstream) => upstream,
        Connected::OnlyLocal => {
            allowed.refused(&host, port, &Decision::Refused(Refused::LocalAddress));
            return socks_reply(&mut client, reply::NOT_ALLOWED);
        }
        Connected::Not => return socks_reply(&mut client, reply::HOST_UNREACHABLE),
    };
    let _ = upstream.set_write_timeout(Some(allowed.idle));
    if client.write_all(&socks_answer(reply::SUCCEEDED)).is_err() {
        return;
    }
    allowed.carried(&host, port, &decision);
    splice(client, upstream, u64::MAX, allowed.idle);
}

/// Answers a SOCKS request with `code` and closes the connection.
fn socks_reply(client: &mut TcpStream, code: u8) {
    let _ = client.write_all(&socks_answer(code));
    let _ = client.shutdown(Shutdown::Both);
}

/// SOCKS's reply with `code`. The bound address is not this proxy's to say: zeros, as many
/// proxies answer.
fn socks_answer(code: u8) -> [u8; 10] {
    [SOCKS_VERSION, code, 0, 1, 0, 0, 0, 0, 0, 0]
}

/// Reads the greeting, answers it, and reads the request: the host and port a `CONNECT` names,
/// or the reply code that refuses what it asked. `None` where the client said nothing a SOCKS5
/// client says, or not in time: the connection is closed with no reply.
fn socks_request(client: &mut TcpStream, deadline: Instant) -> Option<Result<(String, u16), u8>> {
    let [version, methods] = read_exact::<2>(client, deadline)?;
    if version != SOCKS_VERSION || methods == 0 {
        return None;
    }
    let mut offered = [0u8; 255];
    read_into(client, &mut offered[..usize::from(methods)], deadline)?;
    if !offered[..usize::from(methods)].contains(&NO_AUTH) {
        let _ = client.write_all(&[SOCKS_VERSION, NO_METHOD]);
        return None;
    }
    client.write_all(&[SOCKS_VERSION, NO_AUTH]).ok()?;
    let [version, command, _, kind] = read_exact::<4>(client, deadline)?;
    if version != SOCKS_VERSION {
        return None;
    }
    let host = match kind {
        1 => std::net::Ipv4Addr::from(read_exact::<4>(client, deadline)?).to_string(),
        3 => {
            let [len] = read_exact::<1>(client, deadline)?;
            let mut name = [0u8; 255];
            let name = &mut name[..usize::from(len)];
            read_into(client, name, deadline)?;
            match std::str::from_utf8(name) {
                Ok(name) if !name.is_empty() && name.bytes().all(|b| b.is_ascii_graphic()) => {
                    name.to_owned()
                }
                _ => return Some(Err(reply::ADDRESS_NOT_SUPPORTED)),
            }
        }
        4 => std::net::Ipv6Addr::from(read_exact::<16>(client, deadline)?).to_string(),
        _ => return Some(Err(reply::ADDRESS_NOT_SUPPORTED)),
    };
    let port = u16::from_be_bytes(read_exact::<2>(client, deadline)?);
    if command != 1 {
        return Some(Err(reply::COMMAND_NOT_SUPPORTED));
    }
    Some(Ok((host, port)))
}

/// `N` bytes from `client`, all of them before `deadline`.
fn read_exact<const N: usize>(client: &mut TcpStream, deadline: Instant) -> Option<[u8; N]> {
    let mut buf = [0u8; N];
    read_into(client, &mut buf, deadline)?;
    Some(buf)
}

/// Fills `buf` from `client` before `deadline`, however slowly each byte comes.
fn read_into(client: &mut TcpStream, buf: &mut [u8], deadline: Instant) -> Option<()> {
    let mut have = 0;
    while have < buf.len() {
        let left = deadline.checked_duration_since(Instant::now())?;
        if left.is_zero() || client.set_read_timeout(Some(left)).is_err() {
            return None;
        }
        match client.read(&mut buf[have..]) {
            Ok(0) => return None,
            Ok(n) => have += n,
            Err(err) if err.kind() == io::ErrorKind::Interrupted => {}
            Err(_) => return None,
        }
    }
    Some(())
}

/// Whether a `Host` header's `named` is `host` on `port`.
fn same_host(named: &str, host: &str, port: u16) -> bool {
    let (name, named_port) = match named.rsplit_once(':') {
        Some((name, port)) if !name.ends_with(']') || name.starts_with('[') => {
            (name, port.parse().ok())
        }
        _ => (named, None),
    };
    let name = name
        .strip_prefix('[')
        .and_then(|it| it.strip_suffix(']'))
        .unwrap_or(name);
    let name = name.strip_suffix('.').unwrap_or(name);
    let host = host.strip_suffix('.').unwrap_or(host);
    name.eq_ignore_ascii_case(host) && named_port.unwrap_or(80) == port
}

/// The request's head, as text, and whatever the client sent after it.
fn read_head(client: &mut TcpStream, within: Duration) -> Option<(String, Vec<u8>)> {
    let deadline = Instant::now() + within;
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        // The whole head within the deadline, however slowly each byte comes.
        let left = deadline.checked_duration_since(Instant::now())?;
        if left.is_zero() || client.set_read_timeout(Some(left)).is_err() {
            return None;
        }
        let n = client.read(&mut chunk).ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(end) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            let rest = buf.split_off(end + 4);
            return String::from_utf8(buf).ok().map(|head| (head, rest));
        }
        if buf.len() > HEAD_MAX {
            return None;
        }
    }
}

/// A request the proxy can carry.
struct Request {
    host: String,
    port: u16,
    /// For a plain request, the head sent on in its place; none for a tunnel.
    forward: Option<String>,
    /// For a plain request, its `Host` header, where it has one.
    host_header: Option<String>,
    /// For a plain request, its body's length; none for a tunnel.
    body: Option<u64>,
}

impl Request {
    /// The request in `head`, which ends in its blank line, read strictly (see the module's
    /// docs): `None` for anything two parsers could read two ways.
    fn parse(head: &str) -> Option<Self> {
        let lines: Vec<&str> = head.strip_suffix("\r\n\r\n")?.split("\r\n").collect();
        if lines.iter().any(|line| line.contains(['\r', '\n'])) {
            return None;
        }
        let parts: Vec<&str> = lines[0].split(' ').collect();
        let [method, target, version] = parts[..] else {
            return None;
        };
        if !matches!(version, "HTTP/1.1" | "HTTP/1.0")
            || method.is_empty()
            || !method.bytes().all(|b| b.is_ascii_uppercase())
        {
            return None;
        }
        let mut headers = Vec::new();
        for line in &lines[1..] {
            // A folded line, a name with a space before its colon, a line with no colon.
            let (name, value) = line.split_once(':')?;
            if name.is_empty() || !name.bytes().all(is_token) {
                return None;
            }
            let value = value.trim_matches([' ', '\t']);
            if value.bytes().any(|b| (b < 0x20 && b != b'\t') || b == 0x7f) {
                return None;
            }
            headers.push((name, value));
        }
        let named = |wanted: &str| -> Vec<&str> {
            headers
                .iter()
                .filter(|(name, _)| name.eq_ignore_ascii_case(wanted))
                .map(|(_, value)| *value)
                .collect()
        };
        let hosts = named("host");
        let lengths = named("content-length");
        if hosts.len() > 1 || lengths.len() > 1 || !named("transfer-encoding").is_empty() {
            return None;
        }
        if method == "CONNECT" {
            let (host, port) = host_port(target)?;
            return Some(Self {
                host,
                port,
                forward: None,
                // A tunnel's `Host` says nothing the target does not; the target decides.
                host_header: None,
                body: None,
            });
        }
        let [host_header] = hosts[..] else {
            return None;
        };
        let after = target.strip_prefix("http://")?;
        let (authority, path) = match after.find('/') {
            Some(at) => (&after[..at], &after[at..]),
            None => (after, "/"),
        };
        let (host, port) = match authority.rsplit_once(':') {
            Some(_) => host_port(authority)?,
            None => (authority.to_owned(), 80),
        };
        if host.is_empty() || authority.contains('@') {
            return None;
        }
        let body = match lengths[..] {
            [] => 0,
            [length] if !length.is_empty() && length.bytes().all(|b| b.is_ascii_digit()) => {
                length.parse().ok()?
            }
            _ => return None,
        };
        let mut forward = format!("{method} {path} {version}\r\n");
        for (name, value) in &headers {
            if [
                "connection",
                "proxy-connection",
                "proxy-authorization",
                "keep-alive",
            ]
            .iter()
            .any(|it| name.eq_ignore_ascii_case(it))
            {
                continue;
            }
            forward.push_str(&format!("{name}: {value}\r\n"));
        }
        forward.push_str("Connection: close\r\n\r\n");
        Some(Self {
            host,
            port,
            forward: Some(forward),
            host_header: Some(host_header.to_owned()),
            body: Some(body),
        })
    }
}

/// Whether `byte` may be in a header's name (RFC 9110's token).
fn is_token(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte)
}

/// `host:port`, with an IPv6 literal's brackets taken off.
fn host_port(authority: &str) -> Option<(String, u16)> {
    let (host, port) = authority.rsplit_once(':')?;
    let host = host
        .strip_prefix('[')
        .and_then(|it| it.strip_suffix(']'))
        .unwrap_or(host);
    if port.is_empty() || !port.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let port = port.parse().ok()?;
    (!host.is_empty()).then(|| (host.to_owned(), port))
}

/// What a connect came to.
pub(crate) enum Connected {
    To(TcpStream),
    /// The name resolved, and only to addresses a chat never reaches: refused, as a Block.
    OnlyLocal,
    /// It did not resolve, or nothing answered.
    Not,
}

/// Connects to `host` on `port`, only at the addresses it resolves to now that a chat may reach
/// ([`reachable`]), checked at every connect so a name rebound to this machine or a metadata
/// service reaches nothing. An address literal was decided as itself
/// ([`super::reach::Reach::decide`]) and is held to the same check here.
pub(crate) fn connect(host: &str, port: u16, reach: &Reach, own: &[std::net::IpAddr]) -> Connected {
    let resolve = |host: &str, port: u16| {
        (host, port)
            .to_socket_addrs()
            .map(Iterator::collect)
            .unwrap_or_default()
    };
    let resolved: Vec<SocketAddr> = resolve(host, port);
    if resolved.is_empty() {
        return Connected::Not;
    }
    let reached = reachable_from(&resolved, reach, own);
    if reached.is_empty() {
        return Connected::OnlyLocal;
    }
    reached
        .into_iter()
        .find_map(|addr| TcpStream::connect_timeout(&addr, PATIENCE).ok())
        .map_or(Connected::Not, Connected::To)
}

/// The addresses `host` on `port` is reached at, as `resolve` answers: each one a chat may
/// reach ([`super::hosts::refused_address`], with `own` this machine's interface addresses),
/// and one it may not only where `reach` lists that exact address and port. An address literal
/// is held to it as a name is.
pub fn reachable(
    host: &str,
    port: u16,
    resolve: &dyn Fn(&str, u16) -> Vec<SocketAddr>,
    reach: &Reach,
    own: &[std::net::IpAddr],
) -> Vec<SocketAddr> {
    reachable_from(&resolve(host, port), reach, own)
}

fn reachable_from(
    resolved: &[SocketAddr],
    reach: &Reach,
    own: &[std::net::IpAddr],
) -> Vec<SocketAddr> {
    resolved
        .iter()
        .copied()
        .filter(|addr| {
            !super::hosts::refused_address(addr.ip(), own)
                || reach.lists_exactly(addr.ip(), addr.port())
        })
        .collect()
}

fn answer(client: &mut TcpStream, status: &str, why: &str) {
    let _ = write!(
        client,
        "HTTP/1.1 {status}\r\nContent-Type: text/plain; charset=utf-8\r\nConnection: close\r\n\
         Content-Length: {}\r\n\r\n{why}",
        why.len()
    );
    let _ = client.shutdown(Shutdown::Both);
}

/// Copies each way until either side ends, at most `up_to` more bytes from the client, and
/// closes both once nothing has moved either way for `idle`.
pub(crate) fn splice(client: TcpStream, upstream: TcpStream, up_to: u64, idle: Duration) {
    let (Ok(client_in), Ok(upstream_out)) = (client.try_clone(), upstream.try_clone()) else {
        return;
    };
    let moved = Arc::new(Mutex::new(Instant::now()));
    let up_moved = Arc::clone(&moved);
    let up = std::thread::Builder::new()
        .name("charter-egress-up".into())
        .spawn(move || {
            pump(client_in, &upstream_out, up_to, idle, &up_moved);
            let _ = upstream_out.shutdown(Shutdown::Write);
        });
    pump(upstream.try_clone().ok(), &client, u64::MAX, idle, &moved);
    // Either side done, or both idle: the whole connection ends.
    let _ = client.shutdown(Shutdown::Both);
    let _ = upstream.shutdown(Shutdown::Both);
    if let Ok(up) = up {
        let _ = up.join();
    }
}

/// Copies from `from` to `to` until `from` ends, `up_to` bytes have gone, or nothing has moved
/// either way (`moved`) for `idle`.
fn pump(
    from: impl Into<Option<TcpStream>>,
    mut to: &TcpStream,
    up_to: u64,
    idle: Duration,
    moved: &Mutex<Instant>,
) {
    let Some(mut from) = from.into() else { return };
    let tick = idle
        .min(Duration::from_secs(5))
        .max(Duration::from_millis(50));
    if from.set_read_timeout(Some(tick)).is_err() {
        return;
    }
    let mut left = up_to;
    let mut buf = [0u8; 16 * 1024];
    while left > 0 {
        let want = buf.len().min(usize::try_from(left).unwrap_or(usize::MAX));
        match from.read(&mut buf[..want]) {
            Ok(0) => return,
            Ok(n) => {
                if to.write_all(&buf[..n]).is_err() {
                    return;
                }
                left -= n as u64;
                *moved
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = Instant::now();
            }
            Err(err)
                if matches!(
                    err.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                let last = *moved
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                if last.elapsed() >= idle {
                    return;
                }
            }
            Err(err) if err.kind() == io::ErrorKind::Interrupted => {}
            Err(_) => return,
        }
    }
}
