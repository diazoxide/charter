//! Charter's egress proxy, driven over real sockets: a listed host is reached, and anything
//! else is refused before a connection to it is made.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

use super::egress::{Limits, Proxy, REFUSALS_KEPT, Refusals, allows, allows_on, reachable};

/// The listed host every carried test request goes to: the address itself, so a name that
/// also resolves to `::1`, where another program on the machine may listen, is never asked.
const LOOPBACK: &str = "127.0.0.1";

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
    let proxy = Proxy::allowing(vec![LOOPBACK.to_owned()], vec![port]).expect("a proxy");
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
    let proxy = Proxy::allowing(vec![LOOPBACK.to_owned()], vec![port]).expect("a proxy");
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
    let proxy = Proxy::allowing(vec![LOOPBACK.to_owned()], vec![port]).expect("a proxy");
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
        vec![LOOPBACK.to_owned()],
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
        reachable("api.internal.example", 443, &rebound, &own),
        [at("10.100.39.145")]
    );
    let only_loopback = resolver(vec![at("127.0.0.1")]);
    assert_eq!(
        reachable("127.0.0.1.nip.io", 443, &only_loopback, &own),
        Vec::<std::net::SocketAddr>::new()
    );
    // An address literal the project listed is reached as itself.
    let literal = resolver(vec![at("10.100.39.145")]);
    assert_eq!(
        reachable("10.100.39.145", 443, &literal, &own),
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
        reachable("api.internal.example", 443, &resolve, &[]),
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
