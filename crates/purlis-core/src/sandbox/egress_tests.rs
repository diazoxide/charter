//! Charter's egress proxy, driven over real sockets: a listed host is reached, and anything
//! else is refused before a connection to it is made.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

use super::egress::{Limits, Proxy, REFUSALS_KEPT, Refusals, allows, allows_on, reachable};
use super::reach::{By, Reach};

/// A reach that lists nothing exactly: no local address is let through.
fn none() -> Reach {
    Reach::default()
}

/// The listed host every carried test request goes to: the address itself, so a name that
/// also resolves to `::1`, where another program on the machine may listen, is never asked.
const LOOPBACK: &str = "127.0.0.1";

/// The test server's own address and port, listed exactly: the one way the local-address
/// check lets this machine be reached (#1664), so the proxy can be driven against a server here.
fn exactly(port: u16) -> String {
    format!("{LOOPBACK}:{port}")
}

/// A server on this machine that answers each connection by echoing what it is sent, and the
/// port it listens on.
fn an_echo_server() -> (TcpListener, u16) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a listener");
    let port = listener.local_addr().expect("an address").port();
    (listener, port)
}

/// The connection on `listener` whose first bytes are `first`: the proxy's. A port another
/// test of this binary let go of and asks again (one looking for a port nobody listens on)
/// may land here first, so any other connection is let go.
fn proxys(listener: &TcpListener, first: &[u8]) -> TcpStream {
    loop {
        let (stream, _) = listener.accept().expect("a connection");
        stream
            .set_read_timeout(Some(Duration::from_secs(15)))
            .expect("a timeout");
        let mut seen = vec![0u8; first.len()];
        let mut have = 0;
        while have < first.len() {
            match stream.peek(&mut seen) {
                Ok(0) | Err(_) => break,
                Ok(n) => have = n,
            }
            if have < first.len() {
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        if have >= first.len() && seen == first {
            return stream;
        }
    }
}

fn echo_once(listener: TcpListener) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut stream = proxys(&listener, b"ping");
        let mut buf = [0u8; 4];
        stream.read_exact(&mut buf).expect("four bytes");
        stream.write_all(&buf).expect("echoed");
        buf.to_vec()
    })
}

fn to(proxy: &Proxy) -> TcpStream {
    let stream = TcpStream::connect(("127.0.0.1", proxy.port())).expect("the proxy");
    stream
        .set_read_timeout(Some(Duration::from_secs(15)))
        .expect("a timeout");
    stream
}

/// The status line the proxy answers `request` with.
fn status(stream: &mut TcpStream, request: &str) -> String {
    stream.write_all(request.as_bytes()).expect("sent");
    let mut reader = BufReader::new(stream.try_clone().expect("a clone"));
    let mut line = String::new();
    reader.read_line(&mut line).expect("a status line");
    // The rest of the head, so what follows is the tunnel's.
    loop {
        let mut header = String::new();
        reader.read_line(&mut header).expect("a header");
        if header == "\r\n" || header.is_empty() {
            break;
        }
    }
    line.trim_end().to_owned()
}

#[test]
fn a_listed_host_is_tunnelled_to() {
    let (listener, port) = an_echo_server();
    let echoed = echo_once(listener);
    let proxy = Proxy::allowing(vec![exactly(port)], vec![port]).expect("a proxy");
    let mut client = to(&proxy);
    let said = status(
        &mut client,
        &format!("CONNECT 127.0.0.1:{port} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\r\n"),
    );
    assert_eq!(said, "HTTP/1.1 200 Connection established");
    client.write_all(b"ping").expect("through the tunnel");
    let mut back = [0u8; 4];
    client.read_exact(&mut back).expect("echoed back");
    assert_eq!(&back, b"ping");
    assert_eq!(echoed.join().expect("the server"), b"ping");
}

#[test]
fn an_unlisted_host_is_refused_and_never_connected_to() {
    let (listener, port) = an_echo_server();
    listener.set_nonblocking(true).expect("nonblocking");
    let proxy = Proxy::allowing(vec!["example.com".to_owned()], vec![port]).expect("a proxy");
    let mut client = to(&proxy);
    let said = status(
        &mut client,
        &format!("CONNECT 127.0.0.1:{port} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\r\n"),
    );
    assert_eq!(said, "HTTP/1.1 403 Forbidden");
    std::thread::sleep(Duration::from_millis(200));
    assert!(
        listener.accept().is_err(),
        "the proxy connected to a host it refused"
    );
}

#[test]
fn a_listed_host_on_a_port_charter_does_not_carry_is_refused() {
    let (listener, port) = an_echo_server();
    listener.set_nonblocking(true).expect("nonblocking");
    let proxy = Proxy::allowing(vec![LOOPBACK.to_owned()], vec![443]).expect("a proxy");
    let mut client = to(&proxy);
    let said = status(
        &mut client,
        &format!("CONNECT 127.0.0.1:{port} HTTP/1.1\r\n\r\n"),
    );
    assert_eq!(said, "HTTP/1.1 403 Forbidden");
    std::thread::sleep(Duration::from_millis(200));
    assert!(listener.accept().is_err());
}

#[test]
fn a_plain_request_to_a_listed_host_is_carried_in_origin_form() {
    let (listener, port) = an_echo_server();
    let upstream = std::thread::spawn(move || {
        let stream = proxys(&listener, b"GET /simple/x");
        let mut reader = BufReader::new(stream.try_clone().expect("a clone"));
        let mut first = String::new();
        reader.read_line(&mut first).expect("a request line");
        // The whole head, so the close that follows is not a reset that loses the answer.
        loop {
            let mut header = String::new();
            reader.read_line(&mut header).expect("a header");
            if header == "\r\n" || header.is_empty() {
                break;
            }
        }
        let mut stream = stream;
        stream
            .write_all(b"HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n")
            .expect("answered");
        first
    });
    let proxy = Proxy::allowing(vec![exactly(port)], vec![port]).expect("a proxy");
    let mut client = to(&proxy);
    let said = status(
        &mut client,
        &format!("GET http://127.0.0.1:{port}/simple/x HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\r\n"),
    );
    assert_eq!(said, "HTTP/1.1 204 No Content");
    assert_eq!(
        upstream.join().expect("the server"),
        "GET /simple/x HTTP/1.1\r\n"
    );
}

#[test]
fn a_plain_request_to_an_unlisted_host_is_refused() {
    let proxy = Proxy::allowing(vec![LOOPBACK.to_owned()], vec![80]).expect("a proxy");
    let mut client = to(&proxy);
    let said = status(
        &mut client,
        "GET http://example.com/ HTTP/1.1\r\nHost: example.com\r\n\r\n",
    );
    assert_eq!(said, "HTTP/1.1 403 Forbidden");
}

#[test]
fn a_request_that_is_not_http_is_refused() {
    let proxy = Proxy::allowing(vec![LOOPBACK.to_owned()], vec![80]).expect("a proxy");
    let mut client = to(&proxy);
    let said = status(&mut client, "\x16\x03\x01 not a proxy request\r\n\r\n");
    assert_eq!(said, "HTTP/1.1 400 Bad Request");
}

#[test]
fn the_production_proxy_tunnels_only_to_https_and_carries_plain_requests_only_to_http() {
    let proxy = Proxy::start(vec![LOOPBACK.to_owned()]).expect("a proxy");
    // Refused before any connection is tried, so nothing need listen on either port.
    let mut tunnel = to(&proxy);
    assert_eq!(
        status(
            &mut tunnel,
            "CONNECT 127.0.0.1:80 HTTP/1.1\r\nHost: 127.0.0.1:80\r\n\r\n"
        ),
        "HTTP/1.1 403 Forbidden"
    );
    let mut plain = to(&proxy);
    assert_eq!(
        status(
            &mut plain,
            "GET http://127.0.0.1:443/ HTTP/1.1\r\nHost: 127.0.0.1:443\r\n\r\n"
        ),
        "HTTP/1.1 403 Forbidden"
    );
}

#[test]
fn a_head_that_two_parsers_could_read_two_ways_is_refused() {
    // Each of these is read one way here and may be read another by the host it would reach:
    // a folded header, two Hosts, no Host, two lengths, a length that is not a number, a space
    // before a colon, a bare LF.
    let proxy = Proxy::allowing(vec![LOOPBACK.to_owned()], vec![80]).expect("a proxy");
    for head in [
        "GET http://127.0.0.1/ HTTP/1.1\r\nHost: 127.0.0.1\r\nX-A: 1\r\n folded\r\n\r\n",
        "GET http://127.0.0.1/ HTTP/1.1\r\nHost: 127.0.0.1\r\nHost: other.test\r\n\r\n",
        "GET http://127.0.0.1/ HTTP/1.1\r\nX-A: 1\r\n\r\n",
        "POST http://127.0.0.1/ HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: 1\r\nContent-Length: 2\r\n\r\n",
        "POST http://127.0.0.1/ HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: +1\r\n\r\n",
        "GET http://127.0.0.1/ HTTP/1.1\r\nHost : 127.0.0.1\r\n\r\n",
        "GET http://127.0.0.1/ HTTP/1.1\nHost: 127.0.0.1\r\n\r\n",
        "GET  http://127.0.0.1/ HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n",
        "GET http://127.0.0.1/ HTTP/2.0\r\nHost: 127.0.0.1\r\n\r\n",
    ] {
        let mut client = to(&proxy);
        assert_eq!(
            status(&mut client, head),
            "HTTP/1.1 400 Bad Request",
            "{head:?}"
        );
    }
}

#[test]
fn a_head_that_comes_too_slowly_is_given_up_on() {
    let proxy = Proxy::limited(
        vec![LOOPBACK.to_owned()],
        vec![443],
        Limits {
            connections: 8,
            idle: Duration::from_secs(60),
            head: Duration::from_millis(400),
        },
    )
    .expect("a proxy");
    let mut client = to(&proxy);
    let began = std::time::Instant::now();
    // A byte at a time, each well inside a read's own patience.
    for byte in b"CONNECT 127.0.0.1:443 HTTP/1.1\r\n" {
        if client.write_all(&[*byte]).is_err() {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
        if began.elapsed() > Duration::from_secs(3) {
            break;
        }
    }
    let mut said = String::new();
    let _ = BufReader::new(&mut client).read_line(&mut said);
    assert!(!said.starts_with("HTTP/1.1 200"), "{said}");
    assert!(
        began.elapsed() < Duration::from_secs(3),
        "{:?}",
        began.elapsed()
    );
}

#[test]
fn a_proxy_stops_listening_when_it_is_dropped() {
    let proxy = Proxy::allowing(vec![], vec![443]).expect("a proxy");
    let port = proxy.port();
    drop(proxy);
    std::thread::sleep(Duration::from_millis(100));
    assert!(TcpStream::connect(("127.0.0.1", port)).is_err());
}

#[test]
fn a_host_is_allowed_only_as_listed_or_under_a_listed_wildcard() {
    let listed = [
        "api.openai.com".to_owned(),
        "*.githubusercontent.com".to_owned(),
    ];
    for (host, want) in [
        ("api.openai.com", true),
        ("API.OpenAI.com", true),
        ("api.openai.com.", true),
        ("openai.com", false),
        ("evil-api.openai.com", false),
        ("api.openai.com.evil.test", false),
        ("raw.githubusercontent.com", true),
        ("a.b.githubusercontent.com", true),
        ("githubusercontent.com", false),
        ("evilgithubusercontent.com", false),
        ("raw.githubusercontent.com.evil.test", false),
        ("", false),
        ("127.0.0.1", false),
    ] {
        assert_eq!(allows(&listed, host), want, "{host}");
    }
}

#[test]
fn a_plain_request_whose_host_header_names_another_host_is_refused() {
    // The address is a listed host's, and the `Host` another's: a shared front end would
    // answer for the other one.
    let (listener, port) = an_echo_server();
    listener.set_nonblocking(true).expect("nonblocking");
    let proxy = Proxy::allowing(vec![LOOPBACK.to_owned()], vec![port]).expect("a proxy");
    let mut client = to(&proxy);
    let said = status(
        &mut client,
        &format!("GET http://127.0.0.1:{port}/ HTTP/1.1\r\nHost: unlisted.example\r\n\r\n"),
    );
    assert_eq!(said, "HTTP/1.1 403 Forbidden");
    std::thread::sleep(Duration::from_millis(200));
    assert!(listener.accept().is_err());
}

#[test]
fn a_plain_request_carries_its_body_and_nothing_after_it() {
    let (listener, port) = an_echo_server();
    let upstream = std::thread::spawn(move || {
        let mut stream = proxys(&listener, b"POST /one");
        let mut got = Vec::new();
        let _ = stream.read_to_end(&mut got);
        let _ = stream.write_all(b"HTTP/1.1 204 No Content\r\n\r\n");
        String::from_utf8(got).expect("text")
    });
    let proxy = Proxy::allowing(vec![exactly(port)], vec![port]).expect("a proxy");
    let mut client = to(&proxy);
    client
        .write_all(
            format!(
                "POST http://127.0.0.1:{port}/one HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\
                 Content-Length: 4\r\n\r\nbodyGET http://127.0.0.1:{port}/two HTTP/1.1\r\n\r\n"
            )
            .as_bytes(),
        )
        .expect("sent");
    let got = upstream.join().expect("the server");
    assert!(got.starts_with("POST /one HTTP/1.1\r\n"), "{got}");
    assert!(got.ends_with("\r\n\r\nbody"), "{got}");
    assert!(!got.contains("/two"), "a second request was carried: {got}");
}

#[test]
fn a_plain_request_with_a_body_of_no_stated_length_is_refused() {
    let proxy = Proxy::allowing(vec![LOOPBACK.to_owned()], vec![80]).expect("a proxy");
    let mut client = to(&proxy);
    let said = status(
        &mut client,
        "POST http://127.0.0.1/ HTTP/1.1\r\nHost: 127.0.0.1\r\nTransfer-Encoding: chunked\r\n\r\n",
    );
    assert_eq!(said, "HTTP/1.1 400 Bad Request");
}

#[test]
fn connections_past_the_limit_are_refused_rather_than_given_a_thread() {
    let proxy = Proxy::limited(
        vec![LOOPBACK.to_owned()],
        vec![443],
        Limits {
            connections: 2,
            idle: Duration::from_secs(60),
            head: Duration::from_secs(30),
        },
    )
    .expect("a proxy");
    // Two that hold their connection open, saying nothing.
    let _held = [to(&proxy), to(&proxy)];
    std::thread::sleep(Duration::from_millis(100));
    let mut third = to(&proxy);
    let mut said = String::new();
    BufReader::new(&mut third)
        .read_line(&mut said)
        .expect("an answer");
    assert_eq!(said.trim_end(), "HTTP/1.1 503 Service Unavailable");
}

#[test]
fn a_tunnel_idle_both_ways_is_closed() {
    let (listener, port) = an_echo_server();
    let _server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().expect("a connection");
        std::thread::sleep(Duration::from_secs(5));
        drop(stream);
    });
    let proxy = Proxy::limited(
        vec![exactly(port)],
        vec![port],
        Limits {
            connections: 8,
            idle: Duration::from_millis(300),
            head: Duration::from_secs(30),
        },
    )
    .expect("a proxy");
    let mut client = to(&proxy);
    let said = status(
        &mut client,
        &format!("CONNECT 127.0.0.1:{port} HTTP/1.1\r\n\r\n"),
    );
    assert_eq!(said, "HTTP/1.1 200 Connection established");
    let began = std::time::Instant::now();
    let mut buf = [0u8; 1];
    let n = client.read(&mut buf).unwrap_or(0);
    assert_eq!(n, 0, "the tunnel stayed open");
    assert!(
        began.elapsed() < Duration::from_secs(4),
        "{:?}",
        began.elapsed()
    );
}

/// #1341: a host listed with a port is reached on that port, and only there, through the proxy
/// every wrapped chat reaches the network through: a private cluster's API on 6443, say.
#[test]
fn a_host_listed_with_its_port_is_tunnelled_to_on_that_port_alone() {
    let (listener, port) = an_echo_server();
    let echoed = echo_once(listener);
    // The proxy a chat is given, which carries only HTTPS's and HTTP's ports for a host
    // without one.
    let proxy = Proxy::start(vec![format!("{LOOPBACK}:{port}")]).expect("a proxy");
    let mut client = to(&proxy);
    let said = status(
        &mut client,
        &format!("CONNECT 127.0.0.1:{port} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\r\n"),
    );
    assert_eq!(said, "HTTP/1.1 200 Connection established");
    client.write_all(b"ping").expect("through the tunnel");
    let mut back = [0u8; 4];
    client.read_exact(&mut back).expect("echoed back");
    assert_eq!(&back, b"ping");
    assert_eq!(echoed.join().expect("the server"), b"ping");
}

#[test]
fn a_host_listed_without_a_port_is_not_reached_on_another_one() {
    let (listener, port) = an_echo_server();
    listener.set_nonblocking(true).expect("nonblocking");
    let proxy = Proxy::start(vec![LOOPBACK.to_owned()]).expect("a proxy");
    let mut client = to(&proxy);
    let said = status(
        &mut client,
        &format!("CONNECT 127.0.0.1:{port} HTTP/1.1\r\n\r\n"),
    );
    assert_eq!(said, "HTTP/1.1 403 Forbidden");
    std::thread::sleep(Duration::from_millis(200));
    assert!(listener.accept().is_err());
}

#[test]
fn a_listed_port_holds_its_host_to_it_and_an_address_matches_as_written() {
    let listed = [
        "10.100.39.145:6443".to_owned(),
        "*.internal.example:8443".to_owned(),
        "[fd00::7]:443".to_owned(),
        "api.example.com".to_owned(),
    ];
    let defaults = [443];
    for (host, port, want) in [
        ("10.100.39.145", 6443, true),
        ("10.100.39.145", 443, false),
        ("10.100.39.14", 6443, false),
        ("a.internal.example", 8443, true),
        ("a.internal.example", 443, false),
        ("internal.example", 8443, false),
        ("fd00::7", 443, true),
        ("fd00:0::7", 443, true),
        ("fd00::7", 8443, false),
        ("api.example.com", 443, true),
        ("api.example.com", 6443, false),
    ] {
        assert_eq!(
            allows_on(&listed, host, port, &defaults),
            want,
            "{host}:{port}"
        );
    }
}

/// Review of #1341, 1: a wildcard never matches an address literal, through the proxy.
#[test]
fn a_wildcard_never_matches_an_address() {
    let listed = ["*.0.0.1".to_owned(), "*.254.169.254".to_owned()];
    assert!(!allows_on(&listed, "127.0.0.1", 443, &[443]));
    assert!(!allows_on(&listed, "169.254.169.254", 80, &[80]));
    assert!(!allows(&["*.0.0.1:8080".to_owned()], "127.0.0.1"));
}

/// Review of #1341, 3: a name is reached only at the addresses a chat may reach, as the
/// resolver answers at this connect, so a name rebound to this machine or a metadata service
/// reaches nothing. A listed address literal is reached as itself.
#[test]
fn a_name_is_reached_only_at_addresses_a_chat_may_reach() {
    let at = |ip: &str| std::net::SocketAddr::new(ip.parse().unwrap(), 443);
    let own: Vec<std::net::IpAddr> = vec!["192.168.1.7".parse().unwrap()];
    let resolver = |answer: Vec<std::net::SocketAddr>| move |_: &str, _: u16| answer.clone();
    let rebound = resolver(vec![
        at("127.0.0.1"),
        at("169.254.169.254"),
        at("192.168.1.7"),
        at("100.100.100.200"),
        at("10.100.39.145"),
    ]);
    assert_eq!(
        reachable("api.internal.example", 443, &rebound, &none(), &own),
        [at("10.100.39.145")]
    );
    let only_loopback = resolver(vec![at("127.0.0.1")]);
    assert_eq!(
        reachable("127.0.0.1.nip.io", 443, &only_loopback, &none(), &own),
        Vec::<std::net::SocketAddr>::new()
    );
    // An address literal the project listed is reached as itself.
    let literal = resolver(vec![at("10.100.39.145")]);
    assert_eq!(
        reachable("10.100.39.145", 443, &literal, &none(), &own),
        [at("10.100.39.145")]
    );
}

/// Review of #1341, round 3: an AAAA answer that carries a refused IPv4 address is dropped.
#[test]
fn an_aaaa_answer_carrying_this_machine_or_metadata_is_dropped() {
    let at = |ip: &str| std::net::SocketAddr::new(ip.parse().unwrap(), 443);
    let answer = vec![
        at("64:ff9b::a9fe:a9fe"),
        at("::7f00:1"),
        at("2002:7f00:1::1"),
        at("fd00::7"),
    ];
    let resolve = move |_: &str, _: u16| answer.clone();
    assert_eq!(
        reachable("api.internal.example", 443, &resolve, &none(), &[]),
        [at("fd00::7")]
    );
}

/// What it refused because nothing lists it is its own record, each host and port once, told
/// as it happens; a request it could not read, or whose `Host` names another host, is not.
#[test]
fn a_host_nothing_lists_is_kept_once_and_told_as_the_proxy_refused_it() {
    let told: std::sync::Arc<std::sync::Mutex<Vec<String>>> = std::sync::Arc::default();
    let refusals = Refusals::telling({
        let told = std::sync::Arc::clone(&told);
        std::sync::Arc::new(move |host: &str, port: u16| {
            told.lock().unwrap().push(format!("{host}:{port}"));
        })
    });
    let proxy = Proxy::start_keeping(vec!["example.com".to_owned()], refusals).expect("a proxy");
    for _ in 0..3 {
        let said = status(
            &mut to(&proxy),
            "CONNECT api.cluster.example-k8s.com:6443 HTTP/1.1\r\n\r\n",
        );
        assert_eq!(said, "HTTP/1.1 403 Forbidden");
    }
    // A listed host on a port it is not listed on is refused, and kept, on that port.
    let said = status(&mut to(&proxy), "CONNECT example.com:6443 HTTP/1.1\r\n\r\n");
    assert_eq!(said, "HTTP/1.1 403 Forbidden");
    // Neither of these is a host a grant would let through.
    let said = status(
        &mut to(&proxy),
        "GET http://example.com/ HTTP/1.1\r\nHost: other.example\r\n\r\n",
    );
    assert_eq!(said, "HTTP/1.1 403 Forbidden");
    let said = status(&mut to(&proxy), "NONSENSE\r\n\r\n");
    assert_eq!(said, "HTTP/1.1 400 Bad Request");
    assert_eq!(
        proxy.refusals().refused(),
        vec![
            "api.cluster.example-k8s.com:6443".to_owned(),
            "example.com:6443".to_owned()
        ]
    );
    assert_eq!(*told.lock().unwrap(), proxy.refusals().refused());
}

#[test]
fn a_proxys_record_keeps_a_few_hosts_and_names_an_ipv6_one_in_brackets() {
    let refusals = Refusals::default();
    refusals.heard("::1", 6443);
    refusals.heard("API.example.com", 443);
    refusals.heard("api.example.com", 443);
    for n in 0..100 {
        refusals.heard(&format!("h{n}.example"), 443);
    }
    let refused = refusals.refused();
    assert_eq!(refused.len(), REFUSALS_KEPT);
    assert_eq!(refused[..2], ["[::1]:6443", "API.example.com:443"]);
}

// ---- #1664: a pair of ports per chat, SOCKS5, the local-address check, every connection told -

/// What a test hears from a proxy: each refusal, each local-address refusal, each carried line.
#[derive(Default, Clone)]
struct Heard {
    refused: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    local: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    reached: std::sync::Arc<std::sync::Mutex<Vec<super::egress::Line>>>,
}

impl Heard {
    fn serving(&self, reach: Reach) -> super::egress::Serving {
        let push = |into: &std::sync::Arc<std::sync::Mutex<Vec<String>>>| {
            let into = std::sync::Arc::clone(into);
            std::sync::Arc::new(move |host: &str, port: u16| {
                into.lock().unwrap().push(format!("{host}:{port}"));
            }) as super::egress::Told
        };
        let reached = std::sync::Arc::clone(&self.reached);
        super::egress::Serving {
            refusals: Refusals::telling(push(&self.refused)).telling_local(push(&self.local)),
            reached: Some(std::sync::Arc::new(move |target, by, times| {
                reached
                    .lock()
                    .unwrap()
                    .push((target.map(str::to_owned), by, times));
            })),
            ..super::egress::Serving::of(reach)
        }
    }

    fn refused(&self) -> Vec<String> {
        self.refused.lock().unwrap().clone()
    }

    fn local(&self) -> Vec<String> {
        self.local.lock().unwrap().clone()
    }

    fn reached(&self) -> Vec<super::egress::Line> {
        self.reached.lock().unwrap().clone()
    }
}

fn to_socks(proxy: &Proxy) -> TcpStream {
    let stream = TcpStream::connect(("127.0.0.1", proxy.socks_port())).expect("the SOCKS port");
    stream
        .set_read_timeout(Some(Duration::from_secs(15)))
        .expect("a timeout");
    stream
}

/// A SOCKS5 `CONNECT` to `host` on `port` with no authentication: the reply code.
fn socks_connect(stream: &mut TcpStream, host: &str, port: u16) -> u8 {
    stream.write_all(&[5, 1, 0]).expect("a greeting");
    let mut chosen = [0u8; 2];
    stream.read_exact(&mut chosen).expect("a method");
    assert_eq!(chosen, [5, 0], "no authentication is the method served");
    let mut request = vec![5, 1, 0];
    match host.parse::<std::net::IpAddr>() {
        Ok(std::net::IpAddr::V4(ip)) => {
            request.push(1);
            request.extend(ip.octets());
        }
        Ok(std::net::IpAddr::V6(ip)) => {
            request.push(4);
            request.extend(ip.octets());
        }
        Err(_) => {
            request.push(3);
            request.push(u8::try_from(host.len()).expect("a short name"));
            request.extend(host.as_bytes());
        }
    }
    request.extend(port.to_be_bytes());
    stream.write_all(&request).expect("a request");
    let mut reply = [0u8; 10];
    stream.read_exact(&mut reply).expect("a reply");
    assert_eq!(reply[0], 5);
    reply[1]
}

#[test]
fn a_chats_proxy_listens_on_a_pair_of_ports_of_its_own() {
    let proxy = Proxy::start(vec!["example.com".to_owned()]).expect("a proxy");
    assert_ne!(proxy.port(), proxy.socks_port());
    let other = Proxy::start(vec!["example.com".to_owned()]).expect("a proxy");
    let mut ports = [
        proxy.port(),
        proxy.socks_port(),
        other.port(),
        other.socks_port(),
    ];
    ports.sort_unstable();
    ports
        .windows(2)
        .for_each(|pair| assert_ne!(pair[0], pair[1]));
}

#[test]
fn a_socks_connect_to_a_listed_host_is_tunnelled_to() {
    let (listener, port) = an_echo_server();
    let echoed = echo_once(listener);
    let heard = Heard::default();
    let proxy = Proxy::serving(super::egress::Serving {
        tunnel_ports: vec![port],
        ..heard.serving(Reach::of(vec![(exactly(port), By::You)]))
    })
    .expect("a proxy");
    let mut client = to_socks(&proxy);
    assert_eq!(socks_connect(&mut client, LOOPBACK, port), 0);
    client.write_all(b"ping").expect("through the tunnel");
    let mut back = [0u8; 4];
    client.read_exact(&mut back).expect("echoed back");
    assert_eq!(&back, b"ping");
    assert_eq!(echoed.join().expect("the server"), b"ping");
    assert_eq!(heard.reached(), [(Some(exactly(port)), "you", 1)]);
}

#[test]
fn a_socks_connect_to_a_host_nothing_lists_is_refused_with_socks_own_reply() {
    let (listener, port) = an_echo_server();
    listener.set_nonblocking(true).expect("nonblocking");
    let heard = Heard::default();
    let proxy = Proxy::serving(heard.serving(Reach::open(vec!["example.com".to_owned()])))
        .expect("a proxy");
    // Not allowed by the ruleset: SOCKS's own word for it.
    assert_eq!(
        socks_connect(&mut to_socks(&proxy), "unlisted.example.org", 443),
        2
    );
    assert_eq!(heard.refused(), ["unlisted.example.org:443"]);
    assert_eq!(socks_connect(&mut to_socks(&proxy), LOOPBACK, port), 2);
    std::thread::sleep(Duration::from_millis(200));
    assert!(
        listener.accept().is_err(),
        "the proxy connected to a host it refused"
    );
    // Each refused connection is counted for the record too.
    assert_eq!(
        heard.reached(),
        [
            (Some("unlisted.example.org:443".to_owned()), "ask", 1),
            (Some(exactly(port)), "refused", 1),
        ]
    );
}

#[test]
fn a_socks_client_that_asks_anything_but_an_unauthenticated_connect_is_refused() {
    let proxy = Proxy::start(vec!["example.com".to_owned()]).expect("a proxy");
    // Only a username and password offered: no acceptable method.
    let mut client = to_socks(&proxy);
    client.write_all(&[5, 1, 2]).expect("a greeting");
    let mut chosen = [0u8; 2];
    client.read_exact(&mut chosen).expect("a method");
    assert_eq!(chosen, [5, 0xff]);
    // BIND: command not supported.
    let mut client = to_socks(&proxy);
    client.write_all(&[5, 1, 0]).expect("a greeting");
    client.read_exact(&mut chosen).expect("a method");
    client
        .write_all(&[
            5, 2, 0, 3, 11, b'e', b'x', b'a', b'm', b'p', b'l', b'e', b'.', b'c', b'o', b'm', 1,
            187,
        ])
        .expect("a request");
    let mut reply = [0u8; 10];
    client.read_exact(&mut reply).expect("a reply");
    assert_eq!(reply[1], 7);
    // SOCKS4 is not spoken: closed with no reply.
    let mut client = to_socks(&proxy);
    client
        .write_all(&[4, 1, 0, 80, 1, 2, 3, 4, 0])
        .expect("a request");
    let mut buf = [0u8; 1];
    assert_eq!(client.read(&mut buf).unwrap_or(0), 0);
}

/// **Which chat a connection is from is the port it came in on** (#1664): a chat that reaches
/// another chat's proxy is that chat's, decided by that chat's layers, never by anything the
/// connection says, and the first chat hears nothing of it. The sandbox lets no chat reach
/// another chat's ports (`a_chats_profile_reaches_only_its_own_two_ports`); this is what the
/// proxy holds besides.
#[test]
fn a_connection_is_the_chat_whose_port_it_came_in_on_whatever_it_says() {
    let (listener, port) = an_echo_server();
    let echoed = echo_once(listener);
    let (mine, theirs) = (Heard::default(), Heard::default());
    let my_proxy = Proxy::serving(heard_on(
        &mine,
        Reach::open(vec!["example.com".to_owned()]),
        port,
    ))
    .expect("my proxy");
    let their_proxy = Proxy::serving(heard_on(
        &theirs,
        Reach::of(vec![(exactly(port), By::Chat)]),
        port,
    ))
    .expect("their proxy");
    // On my port, naming the other chat's credentials as best a client can: still mine.
    let said = status(
        &mut to(&my_proxy),
        &format!(
            "CONNECT 127.0.0.1:{port} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\
             Proxy-Authorization: Basic dGhlaXJzOg==\r\nX-Purlis-Chat: 2\r\n\r\n"
        ),
    );
    assert_eq!(said, "HTTP/1.1 403 Forbidden");
    assert_eq!(mine.local(), [exactly(port)]);
    assert!(theirs.local().is_empty() && theirs.refused().is_empty());
    // On theirs, it is theirs, by their layers.
    let mut client = to(&their_proxy);
    let said = status(
        &mut client,
        &format!("CONNECT 127.0.0.1:{port} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\r\n"),
    );
    assert_eq!(said, "HTTP/1.1 200 Connection established");
    client.write_all(b"ping").expect("through the tunnel");
    assert_eq!(echoed.join().expect("the server"), b"ping");
    assert_eq!(theirs.reached(), [(Some(exactly(port)), "chat", 1)]);
    assert_eq!(mine.reached(), [(Some(exactly(port)), "refused", 1)]);
}

fn heard_on(heard: &Heard, reach: Reach, port: u16) -> super::egress::Serving {
    super::egress::Serving {
        tunnel_ports: vec![port],
        plain_ports: vec![port],
        ..heard.serving(reach)
    }
}

/// The local-address check (#1664): this machine, link-local and cloud metadata addresses are
/// refused as a Block, listed or not, unless that exact address and port is listed, and never
/// connected to.
#[test]
fn this_machine_and_metadata_are_refused_unless_listed_exactly() {
    let (listener, port) = an_echo_server();
    listener.set_nonblocking(true).expect("nonblocking");
    let heard = Heard::default();
    let proxy = Proxy::serving(heard_on(
        &heard,
        Reach::open(vec![LOOPBACK.to_owned(), "169.254.169.254".to_owned()]),
        port,
    ))
    .expect("a proxy");
    for target in [
        format!("127.0.0.1:{port}"),
        format!("169.254.169.254:{port}"),
    ] {
        let said = status(
            &mut to(&proxy),
            &format!("CONNECT {target} HTTP/1.1\r\n\r\n"),
        );
        assert_eq!(said, "HTTP/1.1 403 Forbidden", "{target}");
    }
    assert_eq!(
        socks_connect(&mut to_socks(&proxy), "169.254.169.254", 80),
        2
    );
    std::thread::sleep(Duration::from_millis(200));
    assert!(listener.accept().is_err(), "the proxy reached this machine");
    assert_eq!(
        heard.local(),
        [
            format!("127.0.0.1:{port}"),
            format!("169.254.169.254:{port}"),
            "169.254.169.254:80".to_owned()
        ]
    );
    assert!(
        heard.refused().is_empty(),
        "never offered as a host to allow"
    );
}

/// A name that resolves only to this machine is refused as a local address, listed or not.
#[test]
fn a_name_resolving_to_this_machine_is_refused() {
    let (listener, port) = an_echo_server();
    listener.set_nonblocking(true).expect("nonblocking");
    let heard = Heard::default();
    let proxy = Proxy::serving(heard_on(
        &heard,
        Reach::open(vec!["localhost".to_owned()]),
        port,
    ))
    .expect("a proxy");
    let said = status(
        &mut to(&proxy),
        &format!("CONNECT localhost:{port} HTTP/1.1\r\n\r\n"),
    );
    assert_eq!(said, "HTTP/1.1 403 Forbidden");
    assert_eq!(socks_connect(&mut to_socks(&proxy), "localhost", port), 2);
    std::thread::sleep(Duration::from_millis(200));
    assert!(listener.accept().is_err(), "the proxy reached this machine");
    assert_eq!(heard.local(), [format!("localhost:{port}")]);
}

/// The local-address check on what a name resolves to, without a resolver: the exact address
/// and port listed is let through, nothing else of this machine is.
#[test]
fn a_resolved_local_address_is_reached_only_where_listed_exactly() {
    let at = |ip: &str, port: u16| std::net::SocketAddr::new(ip.parse().unwrap(), port);
    let own: Vec<std::net::IpAddr> = vec!["192.168.1.7".parse().unwrap()];
    let answer = vec![at("127.0.0.1", 5432), at("192.168.1.7", 5432)];
    let resolve = move |_: &str, _: u16| answer.clone();
    let listed = Reach::of(vec![("127.0.0.1:5432".to_owned(), By::You)]);
    assert_eq!(
        reachable("db.internal.example", 5432, &resolve, &listed, &own),
        [at("127.0.0.1", 5432)]
    );
    assert!(reachable("db.internal.example", 5432, &resolve, &none(), &own).is_empty());
}

/// A listed name pointed at this machine, a link-local address or a metadata service, in any
/// spelling an IPv6 answer can carry one, reaches nothing; a public address beside it is kept.
#[test]
fn a_listed_name_resolving_to_any_spelling_of_a_local_address_reaches_nothing() {
    let at = |ip: &str, port: u16| std::net::SocketAddr::new(ip.parse().unwrap(), port);
    let own: Vec<std::net::IpAddr> = vec!["2001:db8::7".parse().unwrap()];
    let listed = Reach::open(vec!["rebound.example.com".to_owned()]);
    for local in [
        "127.0.0.1",
        "0.0.0.0",
        "169.254.169.254",
        "::1",
        "::",
        "fe80::1",
        "fd00:ec2::254",
        "::ffff:127.0.0.1",
        "::ffff:169.254.169.254",
        "64:ff9b::a9fe:a9fe",
        "2002:7f00:1::1",
        "2001:db8::7",
    ] {
        let answer = vec![at(local, 443)];
        let resolve = move |_: &str, _: u16| answer.clone();
        assert!(
            reachable("rebound.example.com", 443, &resolve, &listed, &own).is_empty(),
            "{local}"
        );
    }
    let answer = vec![at("::1", 443), at("2001:db8::8", 443)];
    let resolve = move |_: &str, _: u16| answer.clone();
    assert_eq!(
        reachable("rebound.example.com", 443, &resolve, &listed, &own),
        [at("2001:db8::8", 443)]
    );
}

#[test]
fn both_ports_stop_listening_when_the_chat_ends() {
    let proxy = Proxy::start(vec!["example.com".to_owned()]).expect("a proxy");
    let (http, socks) = (proxy.port(), proxy.socks_port());
    drop(proxy);
    std::thread::sleep(Duration::from_millis(200));
    assert!(TcpStream::connect(("127.0.0.1", http)).is_err());
    assert!(TcpStream::connect(("127.0.0.1", socks)).is_err());
}

/// The connection limit is the chat's, over both its ports together.
#[test]
fn the_connection_limit_counts_both_ports_together() {
    let proxy = Proxy::limited(
        vec!["example.com".to_owned()],
        vec![443],
        Limits {
            connections: 2,
            idle: Duration::from_secs(60),
            head: Duration::from_secs(30),
        },
    )
    .expect("a proxy");
    let _held = [to(&proxy), to_socks(&proxy)];
    std::thread::sleep(Duration::from_millis(100));
    let mut third = to_socks(&proxy);
    third.write_all(&[5, 1, 0]).ok();
    let mut chosen = [0u8; 2];
    third.read_exact(&mut chosen).expect("an answer");
    assert_eq!(chosen, [5, 0xff]);
}

#[test]
fn every_connection_carried_is_told_once_a_window_with_how_many() {
    use super::egress::{TALLY_HOSTS, TALLY_WINDOW, Tally};
    let start = std::time::Instant::now();
    let mut tally = Tally::default();
    let a = "registry.npmjs.org:443";
    assert_eq!(
        tally.heard(a, "open", start),
        [(Some(a.to_owned()), "open", 1)]
    );
    for n in 1..=4 {
        assert!(
            tally
                .heard(a, "open", start + Duration::from_secs(n))
                .is_empty()
        );
    }
    // A window on: the four held back are told, with the one now.
    let later = start + TALLY_WINDOW + Duration::from_secs(1);
    assert_eq!(
        tally.heard(a, "open", later),
        [
            (Some(a.to_owned()), "open", 4),
            (Some(a.to_owned()), "open", 1)
        ]
    );
    assert!(tally.heard(a, "open", later).is_empty());
    // Past what one tally tells apart, connections are counted together, per layer.
    let mut many = Tally::default();
    for n in 0..TALLY_HOSTS {
        assert_eq!(
            many.heard(&format!("h{n}.example:443"), "you", start).len(),
            1
        );
    }
    assert!(many.heard("past.example:443", "you", start).is_empty());
    assert!(many.heard("past2.example:443", "you", start).is_empty());
    let ended = many.ended();
    assert_eq!(ended, [(None, "you", 2)]);
    assert_eq!(tally.ended(), [(Some(a.to_owned()), "open", 1)]);
}

/// A chat's Seatbelt profile lets it connect to its own proxy's two ports and nowhere else on
/// the network, so no chat reaches another chat's ports (#1664).
#[test]
fn a_chats_profile_reaches_only_its_own_two_ports() {
    let dir = tempfile::tempdir().expect("a dir");
    let (cwd, tmp) = (dir.path().join("chat"), dir.path().join("tmp"));
    std::fs::create_dir_all(&cwd).expect("cwd");
    std::fs::create_dir_all(&tmp).expect("tmp");
    let profile = super::seatbelt::profile(
        &[],
        &super::seatbelt::Own::default(),
        &cwd,
        &tmp,
        &[4040, 4041],
        None,
    )
    .expect("a profile");
    let outbound: Vec<&str> = profile
        .lines()
        .filter(|line| line.contains("network-outbound") && line.contains("remote ip"))
        .collect();
    assert_eq!(
        outbound,
        [
            "(allow network-outbound (remote ip \"localhost:4040\"))",
            "(allow network-outbound (remote ip \"localhost:4041\"))",
        ]
    );
    assert!(!profile.contains("localhost:*"));
}
