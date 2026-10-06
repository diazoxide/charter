//! Charter's loopback egress proxy (ADR 0067 §3): what a harness charter wraps reaches the
//! network through, where the harness has no proxy of its own.
//!
//! The wrap lets the chat connect to this proxy's port on the loopback interface and nowhere
//! else, so a program that ignores `HTTPS_PROXY` reaches nothing. The proxy carries:
//!
//! - **`CONNECT host:443`**, the tunnel every HTTPS client asks a proxy for, to a listed host.
//!   The bytes inside the tunnel are the client's TLS, which the proxy does not read, so what
//!   host a TLS client names inside it (SNI) is the client's to say, as with any proxy that
//!   does not decrypt.
//! - **A plain request in absolute form** (`GET http://host/path`) to a listed host on port 80,
//!   sent on in origin form with `Connection: close`, so one connection reaches one host. Its
//!   one `Host` must name the same host, and its body must state its length: the body is
//!   carried and nothing after it, so a second request cannot ride the same connection.
//!
//! **The head is read strictly**, so the proxy and the host it reaches cannot read it two ways:
//! CRLF line ends only, one space between the request line's three parts, `HTTP/1.1` or
//! `HTTP/1.0`, header names of token characters with no space before the colon, no folded
//! header, at most one `Host` and one `Content-Length` (digits only), and no
//! `Transfer-Encoding`. A head that breaks any of these is refused.
//!
//! Anything else is refused before a connection is made: a host no preset lists, a tunnel to
//! any port but 443, a plain request to any port but 80, and a request that is not one of the
//! two above.
//!
//! **Bounded** ([`Limits`]): it runs in the app, so at most a few connections at once are
//! served, a head must arrive whole within a deadline, a connection that carries nothing either
//! way for long is closed, a write the other side does not take in that time ends it, and a
//! failing accept backs off rather than spinning.
//!
//! The proxy resolves each name itself. A listed name is a name, so an address literal is
//! never carried unless a preset lists it.

use std::io::{self, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

/// The port a tunnel is carried to: HTTPS's.
pub const TUNNEL_PORTS: [u16; 1] = [443];

/// The port a plain request is carried to: HTTP's.
pub const PLAIN_PORTS: [u16; 1] = [80];

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
            Some(suffix) => host
                .strip_suffix(suffix)
                .is_some_and(|head| head.len() > 1 && head.ends_with('.')),
            None => host == listed,
        }
    })
}

/// A running proxy, on a port of the loopback interface. It stops listening when dropped;
/// a tunnel already open runs until either end closes it.
#[derive(Debug)]
pub struct Proxy {
    addr: SocketAddr,
    ports: Vec<u16>,
    stop: Arc<AtomicBool>,
}

impl Proxy {
    /// A proxy that tunnels to `hosts` on [`TUNNEL_PORTS`] and carries plain requests to them
    /// on [`PLAIN_PORTS`].
    pub fn start(hosts: Vec<String>) -> io::Result<Self> {
        Self::carrying(hosts, TUNNEL_PORTS.to_vec(), PLAIN_PORTS.to_vec(), LIMITS)
    }

    /// A proxy that carries `hosts` on `ports`, tunnels and plain requests alike.
    pub fn allowing(hosts: Vec<String>, ports: Vec<u16>) -> io::Result<Self> {
        Self::limited(hosts, ports, LIMITS)
    }

    /// A proxy that carries `hosts` on `ports`, tunnels and plain requests alike, within
    /// `limits`.
    pub fn limited(hosts: Vec<String>, ports: Vec<u16>, limits: Limits) -> io::Result<Self> {
        Self::carrying(hosts, ports.clone(), ports, limits)
    }

    fn carrying(
        hosts: Vec<String>,
        tunnel_ports: Vec<u16>,
        plain_ports: Vec<u16>,
        limits: Limits,
    ) -> io::Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        let addr = listener.local_addr()?;
        let stop = Arc::new(AtomicBool::new(false));
        let ports: Vec<u16> = tunnel_ports.iter().chain(&plain_ports).copied().collect();
        let allowed = Arc::new(Allowed {
            hosts,
            tunnel_ports,
            plain_ports,
            idle: limits.idle,
            head: limits.head,
        });
        let open = Arc::new(AtomicUsize::new(0));
        let stopping = Arc::clone(&stop);
        std::thread::Builder::new()
            .name("charter-egress".into())
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
                        answer(
                            &mut stream,
                            "503 Service Unavailable",
                            "purlis's egress proxy is carrying all the connections it takes",
                        );
                        continue;
                    }
                    let held = Held(Arc::clone(&open));
                    let allowed = Arc::clone(&allowed);
                    let _ = std::thread::Builder::new()
                        .name("charter-egress-conn".into())
                        .spawn(move || {
                            let _held = held;
                            serve(stream, &allowed);
                        });
                }
            })?;
        Ok(Self { addr, ports, stop })
    }

    /// The loopback port it listens on.
    pub fn port(&self) -> u16 {
        self.addr.port()
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
        // Wakes the accept, which then sees the stop and closes the listener.
        let _ = TcpStream::connect_timeout(&self.addr, Duration::from_secs(1));
    }
}

/// One connection being served, counted until it ends.
struct Held(Arc<AtomicUsize>);

impl Drop for Held {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

#[derive(Debug)]
struct Allowed {
    hosts: Vec<String>,
    tunnel_ports: Vec<u16>,
    plain_ports: Vec<u16>,
    idle: Duration,
    head: Duration,
}

impl Allowed {
    fn carries(&self, request: &Request) -> bool {
        let ports = match request.forward {
            None => &self.tunnel_ports,
            Some(_) => &self.plain_ports,
        };
        ports.contains(&request.port) && allows(&self.hosts, &request.host)
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
    if !allowed.carries(&request) {
        answer(
            &mut client,
            "403 Forbidden",
            &format!(
                "purlis's sandbox does not allow {}:{}: no egress preset of this project \
                 lists it",
                request.host, request.port
            ),
        );
        return;
    }
    let _ = client.set_write_timeout(Some(allowed.idle));
    let Some(mut upstream) = connect(&request.host, request.port) else {
        answer(
            &mut client,
            "502 Bad Gateway",
            &format!("purlis's egress proxy could not reach {}", request.host),
        );
        return;
    };
    let _ = upstream.set_write_timeout(Some(allowed.idle));
    let sent = match &request.forward {
        None => client.write_all(b"HTTP/1.1 200 Connection established\r\n\r\n"),
        Some(head) => upstream.write_all(head.as_bytes()),
    };
    if sent.is_err() {
        return;
    }
    // A plain request's body, and nothing after it; a tunnel's bytes, all of them.
    let up_to = request.body.unwrap_or(u64::MAX);
    let rest = &rest[..rest.len().min(usize::try_from(up_to).unwrap_or(usize::MAX))];
    if upstream.write_all(rest).is_err() {
        return;
    }
    splice(client, upstream, up_to - rest.len() as u64, allowed.idle);
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

fn connect(host: &str, port: u16) -> Option<TcpStream> {
    (host, port)
        .to_socket_addrs()
        .ok()?
        .find_map(|addr| TcpStream::connect_timeout(&addr, PATIENCE).ok())
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
fn splice(client: TcpStream, upstream: TcpStream, up_to: u64, idle: Duration) {
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
