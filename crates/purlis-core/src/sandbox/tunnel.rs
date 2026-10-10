//! **Tunnels for clients that skip the proxy** (#1667, decisions N-4 and N-9 of spec #1661): a
//! database client or ssh reaches a host the chat may reach without the chat changing its
//! command.
//!
//! A database client (psql, usql, mysql, redis-cli) opens a plain TCP connection to the host its
//! connection string names, and never asks a proxy. Inside a sandbox that reaches the network
//! only through purlis's proxy it fails at the name lookup, before anything is refused that a
//! Block could name. So when `purlis secret exec` hands a command a value from a vault that
//! points at a host ([`pointed`]: a connection URL, a libpq `host=… port=…` string, or a bare
//! `host:port`), and that exact host and port is one the chat may reach ([`route`]), purlis opens
//! a [`Tunnel`]: a port on the loopback interface that carries every connection to exactly that
//! host and port, and nothing else. The value is pointed at the tunnel ([`Pointed::at`]) and the
//! run's sandbox lets the command connect to that port. A host the chat may not reach is
//! refused as the proxy refuses one, with a Block that offers Allow.
//!
//! **What a tunnel holds to.**
//!
//! - **Exactly one host and port.** The target is fixed when the tunnel opens, from the vault's
//!   value, which no chat writes; nothing a connection sends is read, so no connection can ask
//!   for another host. It is never a proxy.
//! - **Only a listing with that port opens one.** A host listed without a port (a preset's) is
//!   carried by the proxy on HTTPS's port; it never opens a raw tunnel on another port. The
//!   decision is the core decision module's ([`super::reach::Reach::decide`], with no default
//!   port), so the local-address check holds: this machine, a link-local address or a cloud
//!   metadata service is never a tunnel's target unless that exact address and port is listed.
//! - **Resolved at every connection**, and connected only at the addresses a chat may reach
//!   (the proxy's own check), so a name rebound to this machine reaches nothing.
//! - **Every connection is told** ([`super::egress::Reached`], coalesced by the proxy's
//!   [`super::egress::Tally`]), so the network record keeps it.
//! - **It ends with what opened it**: dropped, it stops listening and closes every connection
//!   it still carries.
//!
//! **ssh** goes through the chat's SOCKS port instead, which already decides by host and port:
//! [`SshRoute`] writes an ssh configuration for the chat that sends every ssh connection there,
//! and a `ssh` that reads it. Neither names a key or an agent: signing is the chat's own ssh's
//! (and the SSH agent purlis will hold for it, #1350). No connection rides a master connection
//! another ssh opened outside the sandbox (`ControlMaster no`, `ControlPath none`). A Claude Code
//! chat's sandbox sets its own `GIT_SSH_COMMAND` for each command, through the same SOCKS port
//! (purlis's, since #1665), so git over ssh goes the same way there.

use std::io;
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use super::egress::{self, Reached, Tally};
use super::reach::{Decision, Reach, Refused};

/// The address a value is pointed at instead: the loopback interface, where the tunnel listens.
pub const LOOPBACK: &str = "127.0.0.1";

/// The most tunnels one run opens: a command needs a database or two, not a fleet.
pub const TUNNELS_AT_MOST: usize = 8;

/// The most connections one tunnel carries at once.
const CONNECTIONS_AT_MOST: usize = 16;

/// How long a tunnelled connection may carry nothing either way before it is closed. A
/// database session sits idle between queries, so this is longer than the proxy's.
const IDLE: Duration = Duration::from_secs(3600);

/// How long a failing accept waits before it tries again.
const BACKOFF: Duration = Duration::from_millis(100);

/// One piece of a value pointed somewhere: its own text, kept as it was, or where it points.
#[derive(Clone, PartialEq, Eq)]
enum Piece {
    Text(String),
    /// The host, as the value spells it.
    Host,
    /// The port, as the value spells it, or where it named none.
    Port,
    /// `host:port`, as a URL's authority or a bare value spells it.
    HostPort,
}

/// **Where a value from a vault points**: the host and port a client handed it connects to, and
/// the value with that one place swapped for the tunnel's ([`Self::at`]). Its `Debug` names the
/// host and port alone, never the rest of the value, which may hold a password.
#[derive(Clone, PartialEq, Eq)]
pub struct Pointed {
    host: String,
    port: u16,
    pieces: Vec<Piece>,
}

impl std::fmt::Debug for Pointed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pointed")
            .field("host", &self.host)
            .field("port", &self.port)
            .finish_non_exhaustive()
    }
}

impl Pointed {
    /// The host the value names.
    pub fn host(&self) -> &str {
        &self.host
    }

    /// The port the value names, or its scheme's own where it names none.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// `host:port`, as a grant and a Block name it.
    pub fn target(&self) -> String {
        egress::host_and_port(&self.host, self.port)
    }

    /// The value, pointed at a tunnel on loopback port `local`: everything else as it was.
    pub fn at(&self, local: u16) -> String {
        let mut out = String::new();
        for piece in &self.pieces {
            match piece {
                Piece::Text(text) => out.push_str(text),
                Piece::Host => out.push_str(LOOPBACK),
                Piece::Port => out.push_str(&local.to_string()),
                Piece::HostPort => out.push_str(&format!("{LOOPBACK}:{local}")),
            }
        }
        out
    }
}

/// The port a database URL's scheme connects to when it names none, for the schemes whose
/// clients open a plain TCP connection. A scheme not here is never pointed at a tunnel: an
/// `https://` URL goes through the proxy as it is.
fn scheme_port(scheme: &str) -> Option<u16> {
    // A driver's variant (`mysql+pymysql`) is its scheme's. A `+srv` one looks its hosts up
    // itself, so it is no one place.
    if scheme.ends_with("+srv") {
        return None;
    }
    let scheme = scheme.split('+').next().unwrap_or(scheme);
    Some(match scheme {
        "postgres" | "postgresql" | "pg" | "pgsql" => 5432,
        "mysql" | "mariadb" | "maria" | "my" => 3306,
        "sqlserver" | "mssql" | "ms" => 1433,
        "redis" | "rediss" => 6379,
        "mongodb" => 27017,
        "amqp" => 5672,
        "amqps" => 5671,
        "clickhouse" | "ch" => 9000,
        "oracle" | "or" => 1521,
        "cockroachdb" | "cockroach" | "cr" => 26257,
        _ => return None,
    })
}

/// Whether `host` is spelled as a name or an address a client connects to: nothing a URL or a
/// connection string could read two ways.
fn plain_host(host: &str) -> bool {
    !host.is_empty()
        && host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'))
}

/// `port` as digits, and a port.
fn digits(port: &str) -> Option<u16> {
    (!port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()))
        .then(|| port.parse().ok())
        .flatten()
        .filter(|port| *port > 0)
}

/// **Where `value` points**, or `None` where it is not one place a client connects to:
///
/// - a database URL (`postgres://user:pw@host:port/db?…`, with an optional `jdbc:` before it),
///   its port its scheme's own where it names none ([`scheme_port`]);
/// - a libpq connection string (`host=… port=… dbname=…`), its port 5432 where it names none;
/// - `host:port`, alone.
///
/// Anything that could be read two ways is not pointed anywhere: several hosts, a host given in
/// a URL's query as well, an `@` after the authority, a socket path, a scheme purlis does not
/// know.
pub fn pointed(value: &str) -> Option<Pointed> {
    url(value)
        .or_else(|| key_values(value))
        .or_else(|| host_port(value))
}

fn url(value: &str) -> Option<Pointed> {
    let (scheme, rest) = value.split_once("://")?;
    let bare = scheme.strip_prefix("jdbc:").unwrap_or(scheme);
    if bare.is_empty()
        || !bare
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'-' | b'.'))
    {
        return None;
    }
    let default = scheme_port(&bare.to_ascii_lowercase())?;
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let (authority, tail) = rest.split_at(end);
    // An `@` past the authority means a password with a `/` or a `?` in it, which clients read
    // differently.
    if tail.contains('@') {
        return None;
    }
    let query = tail.split_once('?').map_or("", |(_, query)| query);
    let query = query.split('#').next().unwrap_or(query);
    if query.split('&').any(|pair| {
        let key = pair.split('=').next().unwrap_or("").to_ascii_lowercase();
        matches!(key.as_str(), "host" | "hostaddr" | "port")
    }) {
        return None;
    }
    let (userinfo, hostport) = match authority.rfind('@') {
        Some(at) => (&authority[..=at], &authority[at + 1..]),
        None => ("", authority),
    };
    let (host, port) = split_host_port(hostport)?;
    let port = match port {
        Some(port) => digits(port)?,
        None => default,
    };
    // libpq's own URL, checking the certificate by name: the name stays, and libpq connects to
    // `hostaddr` instead (#1708). Not past a fragment, which would swallow what is added.
    let libpq = scheme == bare
        && matches!(
            bare.to_ascii_lowercase().as_str(),
            "postgres" | "postgresql"
        );
    if libpq && !tail.contains('#') && query.split('&').any(|pair| pair == VERIFY_FULL) {
        let spelled = if host.contains(':') {
            format!("[{host}]")
        } else {
            host.clone()
        };
        return Some(Pointed {
            host,
            port,
            pieces: vec![
                Piece::Text(format!("{scheme}://{userinfo}{spelled}:")),
                Piece::Port,
                Piece::Text(format!("{tail}&hostaddr=")),
                Piece::Host,
            ],
        });
    }
    Some(Pointed {
        host,
        port,
        pieces: vec![
            Piece::Text(format!("{scheme}://{userinfo}")),
            Piece::HostPort,
            Piece::Text(tail.to_owned()),
        ],
    })
}

/// libpq's word for a check of the server's certificate that compares its name: the one a
/// tunnel on loopback would fail unless the name is kept (#1708).
const VERIFY_FULL: &str = "sslmode=verify-full";

/// `host[:port]` or `[v6][:port]`: the host, unbracketed, and the port as written.
fn split_host_port(hostport: &str) -> Option<(String, Option<&str>)> {
    if let Some(rest) = hostport.strip_prefix('[') {
        let (inside, after) = rest.split_once(']')?;
        inside.parse::<std::net::Ipv6Addr>().ok()?;
        let port = match after {
            "" => None,
            _ => Some(after.strip_prefix(':')?),
        };
        return Some((inside.to_owned(), port));
    }
    let (host, port) = match hostport.split_once(':') {
        Some((host, port)) => (host, Some(port)),
        None => (hostport, None),
    };
    plain_host(host).then(|| (host.to_owned(), port))
}

/// A libpq connection string: `key=value` pairs apart by spaces, a value in single quotes where
/// it holds a space (`\'` and `\\` inside). Every word must be a pair, and one of them `host`.
fn key_values(value: &str) -> Option<Pointed> {
    let bytes = value.as_bytes();
    let mut at = 0;
    // Each pair's key, and where its value starts and ends.
    let mut pairs: Vec<(&str, usize, usize)> = Vec::new();
    loop {
        while at < bytes.len() && bytes[at].is_ascii_whitespace() {
            at += 1;
        }
        if at == bytes.len() {
            break;
        }
        let key_start = at;
        while at < bytes.len() && (bytes[at].is_ascii_lowercase() || bytes[at] == b'_') {
            at += 1;
        }
        if at == key_start || bytes.get(at) != Some(&b'=') {
            return None;
        }
        let key = &value[key_start..at];
        at += 1;
        let start = at;
        if bytes.get(at) == Some(&b'\'') {
            at += 1;
            loop {
                match bytes.get(at)? {
                    b'\\' => at += 2,
                    b'\'' => {
                        at += 1;
                        break;
                    }
                    _ => at += 1,
                }
            }
        } else {
            while at < bytes.len() && !bytes[at].is_ascii_whitespace() {
                at += 1;
            }
        }
        pairs.push((key, start, at.min(bytes.len())));
    }
    let named = |wanted: &str| -> Vec<(usize, usize)> {
        pairs
            .iter()
            .filter(|(key, ..)| *key == wanted)
            .map(|(_, start, end)| (*start, *end))
            .collect()
    };
    if !named("hostaddr").is_empty() {
        return None;
    }
    let unquoted = |(start, end): (usize, usize)| -> String {
        let raw = &value[start..end];
        raw.strip_prefix('\'')
            .and_then(|it| it.strip_suffix('\''))
            .map_or_else(
                || raw.to_owned(),
                |inside| inside.replace("\\'", "'").replace("\\\\", "\\"),
            )
    };
    let [host_at] = named("host")[..] else {
        return None;
    };
    let host = unquoted(host_at);
    if !plain_host(&host) {
        return None;
    }
    let port_at = match named("port")[..] {
        [] => None,
        [one] => Some(one),
        _ => return None,
    };
    let port = match port_at {
        Some(at) => digits(&unquoted(at))?,
        None => 5432,
    };
    // Checking the certificate by name (#1708): the host's value stays, so the name matches,
    // and libpq connects to `hostaddr`, added, instead of looking the name up.
    let by_name = matches!(named("sslmode")[..], [one] if format!("sslmode={}", unquoted(one)) == VERIFY_FULL);
    // The value as it was, with the host's value and the port's swapped, and a port added where
    // it named none.
    let mut places: Vec<(usize, usize, Piece)> = Vec::new();
    if !by_name {
        places.push((host_at.0, host_at.1, Piece::Host));
    }
    if let Some((start, end)) = port_at {
        places.push((start, end, Piece::Port));
    }
    places.sort_by_key(|(start, ..)| *start);
    let mut pieces = Vec::new();
    let mut from = 0;
    for (start, end, piece) in places {
        pieces.push(Piece::Text(value[from..start].to_owned()));
        pieces.push(piece);
        from = end;
    }
    pieces.push(Piece::Text(value[from..].to_owned()));
    if port_at.is_none() {
        pieces.push(Piece::Text(" port=".to_owned()));
        pieces.push(Piece::Port);
    }
    if by_name {
        pieces.push(Piece::Text(" hostaddr=".to_owned()));
        pieces.push(Piece::Host);
    }
    Some(Pointed { host, port, pieces })
}

/// `host:port` alone.
fn host_port(value: &str) -> Option<Pointed> {
    let (host, port) = split_host_port(value)?;
    let port = digits(port?)?;
    Some(Pointed {
        host,
        port,
        pieces: vec![Piece::HostPort],
    })
}

/// What `route` makes of a value a run is handed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Route {
    /// It points at a host and port the chat may reach: through a tunnel, as `decision` lets it.
    Through(Pointed, Decision),
    /// It points at a host and port the chat may not reach, which a person could allow.
    Refused(Pointed),
    /// It points at this machine, a link-local address or a cloud metadata service.
    Local(Pointed),
    /// It points nowhere a tunnel could carry, or at something no person could allow.
    Untouched,
}

/// **Whether a value is carried through a tunnel**, by the core decision module: only where it
/// points at a host and that exact port is listed for the chat (no default port), with the
/// local-address check, `own` being this machine's interface addresses.
pub fn route(value: &str, reach: &Reach, own: &[IpAddr]) -> Route {
    let Some(pointed) = pointed(value) else {
        return Route::Untouched;
    };
    match reach.decide(pointed.host(), pointed.port(), &[], own) {
        decision if decision.carries() => Route::Through(pointed, decision),
        Decision::Ask => Route::Refused(pointed),
        Decision::Refused(Refused::LocalAddress) => Route::Local(pointed),
        _ => Route::Untouched,
    }
}

/// **A running tunnel**: a port on the loopback interface carrying each connection to exactly
/// one host and port. Dropped, it stops listening and closes every connection it carries.
pub struct Tunnel {
    addr: SocketAddr,
    target: String,
    carrying: Arc<Carrying>,
}

impl std::fmt::Debug for Tunnel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tunnel")
            .field("port", &self.addr.port())
            .field("target", &self.target)
            .finish_non_exhaustive()
    }
}

/// What a tunnel's connections share: where they go, what may be reached, and the record.
struct Carrying {
    host: String,
    port: u16,
    reach: Reach,
    word: &'static str,
    reached: Option<Reached>,
    tally: Mutex<Tally>,
    /// A handle on each connection carried now, so the tunnel's end closes them.
    live: Mutex<Vec<(u64, TcpStream)>>,
    next: AtomicU64,
    open: AtomicUsize,
    /// Set as the tunnel ends: what it accepts after that is closed, not carried.
    ended: AtomicBool,
}

impl Carrying {
    fn tell(&self, step: impl FnOnce(&mut Tally) -> Vec<egress::Line>) {
        let Some(reached) = &self.reached else { return };
        let lines = step(&mut self.tally.lock().unwrap_or_else(PoisonError::into_inner));
        for (target, by, times) in lines {
            reached(target.as_deref(), by, times);
        }
    }
}

impl Tunnel {
    /// A tunnel to `pointed`'s host and port, as `decision` lets the chat reach it, held to
    /// `reach` at every connection, telling `reached` each one.
    pub fn open(
        pointed: &Pointed,
        decision: &Decision,
        reach: Reach,
        reached: Option<Reached>,
    ) -> io::Result<Self> {
        let listener = TcpListener::bind((LOOPBACK, 0))?;
        let addr = listener.local_addr()?;
        let carrying = Arc::new(Carrying {
            host: pointed.host().to_owned(),
            port: pointed.port(),
            reach,
            word: decision.word(),
            reached,
            tally: Mutex::new(Tally::default()),
            live: Mutex::new(Vec::new()),
            next: AtomicU64::new(0),
            open: AtomicUsize::new(0),
            ended: AtomicBool::new(false),
        });
        let shared = Arc::clone(&carrying);
        std::thread::Builder::new()
            .name("purlis-tunnel".into())
            .spawn(move || accept(&listener, &shared))?;
        Ok(Self {
            addr,
            target: pointed.target(),
            carrying,
        })
    }

    /// The loopback port it listens on.
    pub fn port(&self) -> u16 {
        self.addr.port()
    }

    /// The host and port it carries to.
    pub fn target(&self) -> &str {
        &self.target
    }
}

impl Drop for Tunnel {
    fn drop(&mut self) {
        // Set under the lock that keeps the connections, so one taken on as this ends is either
        // closed here or sees the end itself.
        {
            let mut live = self
                .carrying
                .live
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            self.carrying.ended.store(true, Ordering::SeqCst);
            for (_, stream) in live.drain(..) {
                let _ = stream.shutdown(std::net::Shutdown::Both);
            }
        }
        // Wakes the accept, which then sees the end and closes its listener.
        let _ = TcpStream::connect_timeout(&self.addr, Duration::from_secs(1));
        self.carrying.tell(Tally::ended);
    }
}

fn accept(listener: &TcpListener, carrying: &Arc<Carrying>) {
    for stream in listener.incoming() {
        if carrying.ended.load(Ordering::SeqCst) {
            break;
        }
        let Ok(client) = stream else {
            std::thread::sleep(BACKOFF);
            continue;
        };
        if carrying.open.fetch_add(1, Ordering::SeqCst) >= CONNECTIONS_AT_MOST {
            carrying.open.fetch_sub(1, Ordering::SeqCst);
            let _ = client.shutdown(std::net::Shutdown::Both);
            continue;
        }
        let carrying = Arc::clone(carrying);
        let _ = std::thread::Builder::new()
            .name("purlis-tunnel-conn".into())
            .spawn(move || {
                carry(client, &carrying);
                carrying.open.fetch_sub(1, Ordering::SeqCst);
            });
    }
}

/// One connection: to the target, at an address a chat may reach, told, then spliced.
fn carry(client: TcpStream, carrying: &Carrying) {
    let id = carrying.next.fetch_add(1, Ordering::SeqCst);
    {
        let mut live = carrying.live.lock().unwrap_or_else(PoisonError::into_inner);
        match client.try_clone() {
            Ok(handle) if !carrying.ended.load(Ordering::SeqCst) => live.push((id, handle)),
            // Ended already, or no handle to close it by: not carried.
            _ => {
                let _ = client.shutdown(std::net::Shutdown::Both);
                return;
            }
        }
    }
    // Let go of on every way out.
    struct Held<'a>(&'a Carrying, u64);
    impl Drop for Held<'_> {
        fn drop(&mut self) {
            self.0
                .live
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .retain(|(held, _)| *held != self.1);
        }
    }
    let _held = Held(carrying, id);
    let own = super::hosts::own_addresses();
    let target = egress::host_and_port(&carrying.host, carrying.port);
    let upstream = match egress::connect(&carrying.host, carrying.port, &carrying.reach, &own) {
        egress::Connected::To(upstream) => upstream,
        egress::Connected::OnlyLocal => {
            carrying.tell(|tally| tally.heard(&target, "refused", Instant::now()));
            let _ = client.shutdown(std::net::Shutdown::Both);
            return;
        }
        egress::Connected::Not => {
            let _ = client.shutdown(std::net::Shutdown::Both);
            return;
        }
    };
    carrying.tell(|tally| tally.heard(&target, carrying.word, Instant::now()));
    let _ = upstream.set_write_timeout(Some(IDLE));
    let _ = client.set_write_timeout(Some(IDLE));
    egress::splice(client, upstream, u64::MAX, IDLE);
}

/// **The ssh route of a chat** (#1667): an ssh configuration that sends every ssh connection
/// through the chat's SOCKS port, which carries only the hosts and ports the chat may reach, and
/// a `ssh` that reads it, written into the chat's own temp directory. The person's own
/// `~/.ssh/config` is still read after it, for names, users and keys; only how a connection
/// leaves is purlis's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SshRoute {
    dir: PathBuf,
}

/// The variable git runs its ssh by: pointed at the route's `ssh`, so git over ssh takes it
/// whatever the chat's `PATH` is.
pub const GIT_SSH_ENV: &str = "GIT_SSH_COMMAND";

impl SshRoute {
    /// The route through SOCKS port `socks`, written in `within`/`purlis-ssh` (made if
    /// missing): its configuration and its `ssh`, each the chat's user's alone.
    pub fn write(within: &Path, socks: u16) -> io::Result<Self> {
        let dir = within.join("purlis-ssh");
        let route = Self { dir };
        std::fs::create_dir_all(route.bin())?;
        std::fs::write(
            route.config(),
            format!(
                "# purlis: this chat's ssh leaves through its SOCKS port, which carries only the\n\
                 # hosts and ports this chat may reach. Your own ~/.ssh/config is read after it.\n\
                 Host *\n  \
                 ProxyCommand /usr/bin/nc -X 5 -x {LOOPBACK}:{socks} %h %p\n  \
                 ControlMaster no\n  \
                 ControlPath none\n  \
                 Include ~/.ssh/config\n"
            ),
        )?;
        let ssh = route.bin().join("ssh");
        std::fs::write(
            &ssh,
            format!(
                "#!/bin/sh\n# purlis: ssh through this chat's SOCKS port (#1667).\n\
                 exec /usr/bin/ssh -F {} \"$@\"\n",
                sh_quoted(&route.config().display().to_string())
            ),
        )?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&ssh, std::fs::Permissions::from_mode(0o700))?;
            std::fs::set_permissions(route.config(), std::fs::Permissions::from_mode(0o600))?;
        }
        Ok(route)
    }

    /// Its ssh configuration.
    pub fn config(&self) -> PathBuf {
        self.dir.join("config")
    }

    /// The folder whose `ssh` takes the route: what a chat's `PATH` starts with.
    pub fn bin(&self) -> PathBuf {
        self.dir.join("bin")
    }

    /// What a chat's environment gains for it: git's ssh, pointed at the route's.
    pub fn env(&self) -> Vec<(String, String)> {
        vec![(
            GIT_SSH_ENV.to_owned(),
            sh_quoted(&self.bin().join("ssh").display().to_string()),
        )]
    }

    /// `env`, a chat's environment, with the route's folder first on its `PATH`, where it has
    /// one.
    pub fn first_on_path(&self, mut env: Vec<(String, String)>) -> Vec<(String, String)> {
        for (key, value) in &mut env {
            if key == "PATH" {
                *value = self.on_path(value);
            }
        }
        env
    }

    /// `path`, a `PATH`'s value, with the route's folder first, so a bare `ssh` is its.
    pub fn on_path(&self, path: &str) -> String {
        let bin = self.bin().display().to_string();
        if path.is_empty() {
            bin
        } else {
            format!("{bin}:{path}")
        }
    }
}

/// `text` in single quotes, as a POSIX shell reads one word.
fn sh_quoted(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}
