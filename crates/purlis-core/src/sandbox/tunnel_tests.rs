//! Tunnels (#1667): where a vault's value points, what it is pointed at instead, and the
//! forwarder that carries one host and port.

use super::tunnel::pointed;

fn target(value: &str) -> Option<(String, u16)> {
    pointed(value).map(|p| (p.host().to_owned(), p.port()))
}

#[test]
fn a_database_url_points_at_its_host_and_port() {
    assert_eq!(
        target("postgres://app:pw@db.example.com:16752/orders?sslmode=require"),
        Some(("db.example.com".to_owned(), 16752))
    );
    assert_eq!(
        target("postgresql://db.example.com/orders"),
        Some(("db.example.com".to_owned(), 5432))
    );
    assert_eq!(
        target("mysql://u:p@maria.example.com/app"),
        Some(("maria.example.com".to_owned(), 3306))
    );
    assert_eq!(
        target("redis://cache.example.com"),
        Some(("cache.example.com".to_owned(), 6379))
    );
    assert_eq!(
        target("jdbc:postgresql://db.example.com:6000/x"),
        Some(("db.example.com".to_owned(), 6000))
    );
    assert_eq!(
        target("postgres://u@[2001:db8::5]:6000/x"),
        Some(("2001:db8::5".to_owned(), 6000))
    );
}

#[test]
fn a_url_pointed_at_the_tunnel_keeps_everything_but_where_it_goes() {
    let value = "postgres://app:p%40ss@db.example.com:16752/orders?sslmode=require";
    assert_eq!(
        pointed(value).expect("a target").at(41234),
        "postgres://app:p%40ss@127.0.0.1:41234/orders?sslmode=require"
    );
    assert_eq!(
        pointed("redis://cache.example.com")
            .expect("a target")
            .at(7),
        "redis://127.0.0.1:7"
    );
}

#[test]
fn a_value_that_could_be_read_two_ways_points_nowhere() {
    for value in [
        // Several hosts, a socket path, a host in the query as well.
        "postgres://u@db1.example.com,db2.example.com/x",
        "postgres:///orders?host=/tmp",
        "postgres://u@db.example.com/x?host=other.example.com",
        "postgres://u@db.example.com/x?port=1",
        // A password with a `/` in it: the authority ends early, and clients disagree.
        "postgres://u:pa/ss@db.example.com/x",
        // A scheme whose client goes through the proxy, or one purlis does not know.
        "https://db.example.com:8443/",
        "foo://db.example.com:1234",
        "mongodb+srv://cluster.example.com/x",
        // Not one place at all.
        "a plain sentence",
        "",
    ] {
        assert_eq!(target(value), None, "{value}");
    }
}

#[test]
fn a_libpq_string_points_at_its_host_and_port() {
    let value = "host=db.example.com port=16752 dbname=orders user=app password='p w'";
    let p = pointed(value).expect("a target");
    assert_eq!((p.host(), p.port()), ("db.example.com", 16752));
    assert_eq!(
        p.at(41234),
        "host=127.0.0.1 port=41234 dbname=orders user=app password='p w'"
    );
    let p = pointed("dbname=orders host=db.example.com").expect("a target");
    assert_eq!((p.host(), p.port()), ("db.example.com", 5432));
    assert_eq!(p.at(9), "dbname=orders host=127.0.0.1 port=9");
    for value in [
        "host=db1.example.com,db2.example.com dbname=x",
        "host=/tmp dbname=x",
        "host=db.example.com hostaddr=10.0.0.5",
        "dbname=orders user=app",
        "host=db.example.com this is not a pair",
    ] {
        assert_eq!(target(value), None, "{value}");
    }
}

/// #1708: a libpq value that checks the server's certificate by name (`sslmode=verify-full`)
/// keeps its `host`, so the name still matches, and connects to the tunnel through `hostaddr`,
/// which libpq connects to instead of looking the name up. Any other value is pointed as before.
#[test]
fn a_libpq_value_that_checks_the_certificate_by_name_keeps_the_name() {
    let p = pointed("host=db.example.com port=16752 dbname=orders sslmode=verify-full")
        .expect("a target");
    assert_eq!((p.host(), p.port()), ("db.example.com", 16752));
    assert_eq!(
        p.at(41234),
        "host=db.example.com port=41234 dbname=orders sslmode=verify-full hostaddr=127.0.0.1"
    );
    let p = pointed("sslmode='verify-full' host=db.example.com").expect("a target");
    assert_eq!(
        p.at(9),
        "sslmode='verify-full' host=db.example.com port=9 hostaddr=127.0.0.1"
    );
    let p = pointed("postgres://app:pw@db.example.com:16752/orders?sslmode=verify-full")
        .expect("a target");
    assert_eq!(
        p.at(41234),
        "postgres://app:pw@db.example.com:41234/orders?sslmode=verify-full&hostaddr=127.0.0.1"
    );
    // Only libpq's own schemes: another driver's answer is its own (D-1708-3).
    assert_eq!(
        pointed("mysql://u:p@maria.example.com/app?ssl-mode=VERIFY_IDENTITY")
            .expect("a target")
            .at(7),
        "mysql://u:p@127.0.0.1:7/app?ssl-mode=VERIFY_IDENTITY"
    );
    // A check that does not compare the name is pointed as before.
    assert_eq!(
        pointed("host=db.example.com sslmode=verify-ca")
            .expect("a target")
            .at(9),
        "host=127.0.0.1 sslmode=verify-ca port=9"
    );
    // A fragment after the query would swallow what is added: pointed as before.
    assert_eq!(
        pointed("postgres://db.example.com/x?sslmode=verify-full#frag")
            .expect("a target")
            .at(9),
        "postgres://127.0.0.1:9/x?sslmode=verify-full#frag"
    );
}

#[test]
fn a_bare_host_and_port_points_there() {
    let p = pointed("db.example.com:16752").expect("a target");
    assert_eq!((p.host(), p.port()), ("db.example.com", 16752));
    assert_eq!(p.at(41234), "127.0.0.1:41234");
    assert_eq!(target("db.example.com"), None, "no port, nothing to point");
    assert_eq!(target("user:secret"), None, "a port is digits");
}

#[test]
fn a_pointed_value_never_shows_more_than_where_it_points() {
    let p = pointed("postgres://app:hunter2@db.example.com:5432/x").expect("a target");
    let shown = format!("{p:?}");
    assert!(
        shown.contains("db.example.com") && !shown.contains("hunter2"),
        "{shown}"
    );
}

mod routes {
    use std::net::{IpAddr, Ipv4Addr};

    use super::super::reach::{By, Decision, Reach};
    use super::super::tunnel::{Route, route};

    fn reach() -> Reach {
        Reach::of(vec![
            ("api.example.com".to_owned(), By::Open),
            ("db.example.com:16752".to_owned(), By::Chat),
            ("10.0.0.5:5432".to_owned(), By::You),
        ])
    }

    const OWN: &[IpAddr] = &[IpAddr::V4(Ipv4Addr::new(192, 168, 1, 20))];

    #[test]
    fn only_a_host_listed_with_that_exact_port_is_tunnelled_to() {
        let through = |value: &str| match route(value, &reach(), OWN) {
            Route::Through(p, decision) => Some((p.target(), decision)),
            _ => None,
        };
        assert_eq!(
            through("postgres://u:p@db.example.com:16752/x"),
            Some((
                "db.example.com:16752".to_owned(),
                Decision::Allowed(By::Chat)
            ))
        );
        assert_eq!(
            through("host=10.0.0.5 dbname=x"),
            Some(("10.0.0.5:5432".to_owned(), Decision::Allowed(By::You)))
        );
        // Listed without a port: the proxy carries it on HTTPS's port, never a raw tunnel.
        assert!(matches!(
            route("postgres://api.example.com/x", &reach(), OWN),
            Route::Refused(p) if p.target() == "api.example.com:5432"
        ));
        // Another port of a listed host is another host and port.
        assert!(matches!(
            route("db.example.com:5432", &reach(), OWN),
            Route::Refused(_)
        ));
    }

    #[test]
    fn this_machine_and_metadata_are_never_tunnelled_to() {
        for value in [
            "postgres://127.0.0.1:5432/x",
            "postgres://[::1]:5432/x",
            "redis://169.254.169.254:6379",
            "postgres://192.168.1.20:5432/x",
        ] {
            assert!(
                matches!(route(value, &reach(), OWN), Route::Local(_)),
                "{value}"
            );
        }
    }

    #[test]
    fn a_value_that_points_nowhere_is_left_as_it_is() {
        assert_eq!(route("hunter2", &reach(), OWN), Route::Untouched);
        assert_eq!(
            route("https://api.example.com/", &reach(), OWN),
            Route::Untouched
        );
    }
}

mod ssh {
    use super::super::tunnel::{GIT_SSH_ENV, SshRoute};

    #[test]
    fn a_chats_ssh_leaves_through_its_socks_port_and_still_reads_the_persons_config() {
        let tmp = tempfile::tempdir().expect("a temp dir");
        let route = SshRoute::write(tmp.path(), 41999).expect("written");
        let config = std::fs::read_to_string(route.config()).expect("its config");
        let lines: Vec<&str> = config.lines().map(str::trim).collect();
        let proxy = lines
            .iter()
            .position(|l| *l == "ProxyCommand /usr/bin/nc -X 5 -x 127.0.0.1:41999 %h %p")
            .expect("the SOCKS port, for every host");
        let include = lines
            .iter()
            .position(|l| *l == "Include ~/.ssh/config")
            .expect("the person's own config");
        assert!(
            proxy < include,
            "purlis's ProxyCommand is read first, so it wins:\n{config}"
        );
        // No connection rides a master another ssh opened outside the sandbox: first, so it wins.
        for line in ["ControlMaster no", "ControlPath none"] {
            let at = lines.iter().position(|l| *l == line).expect(line);
            assert!(at < include, "{line} first:\n{config}");
        }
        assert!(
            !config.contains("IdentityFile") && !config.contains("IdentityAgent"),
            "signing is not the route's:\n{config}"
        );
        let ssh = std::fs::read_to_string(route.bin().join("ssh")).expect("its ssh");
        assert!(ssh.starts_with("#!/bin/sh\n"), "{ssh}");
        assert!(
            ssh.contains(&format!(
                "exec /usr/bin/ssh -F '{}' \"$@\"",
                route.config().display()
            )),
            "{ssh}"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(route.bin().join("ssh"))
                .expect("its ssh")
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o700, "run by the chat's user alone");
        }
        assert_eq!(
            route.env(),
            vec![(
                GIT_SSH_ENV.to_owned(),
                format!("'{}'", route.bin().join("ssh").display())
            )],
            "git over ssh takes the route whatever the chat's PATH"
        );
    }

    #[test]
    fn a_path_that_starts_with_the_route_finds_its_ssh_first() {
        let tmp = tempfile::tempdir().expect("a temp dir");
        let route = SshRoute::write(tmp.path(), 1).expect("written");
        assert_eq!(
            route.on_path("/opt/homebrew/bin:/usr/bin"),
            format!("{}:/opt/homebrew/bin:/usr/bin", route.bin().display())
        );
        assert_eq!(route.on_path(""), route.bin().display().to_string());
        assert_eq!(
            route.first_on_path(vec![
                ("HOME".into(), "/h".into()),
                ("PATH".into(), "/usr/bin".into())
            ]),
            vec![
                ("HOME".into(), "/h".into()),
                ("PATH".into(), format!("{}:/usr/bin", route.bin().display()))
            ]
        );
    }
}

/// The forwarder, through real sockets: first run on CI.
mod forwarding {
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use super::super::reach::{By, Decision, Reach};
    use super::super::tunnel::{Tunnel, pointed};

    type Told = Arc<Mutex<Vec<(Option<String>, &'static str, u64)>>>;

    fn opened(target: &str, listed: &[&str]) -> (Tunnel, Told) {
        let told: Told = Arc::default();
        let reach = Reach::of(
            listed
                .iter()
                .map(|host| ((*host).to_owned(), By::Chat))
                .collect(),
        );
        let tunnel = Tunnel::open(
            &pointed(target).expect("a target"),
            &Decision::Allowed(By::Chat),
            reach,
            Some({
                let told = Arc::clone(&told);
                Arc::new(move |target: Option<&str>, by: &'static str, times: u64| {
                    told.lock()
                        .unwrap()
                        .push((target.map(str::to_owned), by, times));
                })
            }),
        )
        .expect("a tunnel");
        (tunnel, told)
    }

    #[test]
    fn a_tunnel_carries_each_connection_to_exactly_its_host_and_port_and_tells_it() {
        let host = TcpListener::bind("127.0.0.1:0").expect("a host");
        let at = format!("127.0.0.1:{}", host.local_addr().unwrap().port());
        let (tunnel, told) = opened(&at, &[at.as_str()]);
        let serving = std::thread::spawn(move || {
            for _ in 0..2 {
                let (mut conn, _) = host.accept().expect("a connection");
                let mut asked = [0u8; 4];
                conn.read_exact(&mut asked).unwrap();
                conn.write_all(&asked).unwrap();
            }
        });
        for _ in 0..2 {
            let mut client = TcpStream::connect(("127.0.0.1", tunnel.port())).expect("in");
            // Whatever the client says, it reaches the one host: nothing it sends is a target.
            client.write_all(b"CONN").unwrap();
            let mut back = [0u8; 4];
            client.read_exact(&mut back).unwrap();
            assert_eq!(&back, b"CONN");
        }
        serving.join().unwrap();
        drop(tunnel);
        let told = told.lock().unwrap().clone();
        let total: u64 = told
            .iter()
            .filter(|(target, by, _)| target.as_deref() == Some(at.as_str()) && *by == "chat")
            .map(|(_, _, n)| n)
            .sum();
        assert_eq!(total, 2, "every connection in the record: {told:?}");
    }

    #[test]
    fn a_name_that_resolves_to_this_machine_is_never_reached_through_a_tunnel() {
        let host = TcpListener::bind("127.0.0.1:0").expect("a host");
        host.set_nonblocking(true).unwrap();
        let port = host.local_addr().unwrap().port();
        // Listed by name, never as the exact address: the local-address check refuses it.
        let target = format!("localhost:{port}");
        let (tunnel, told) = opened(&target, &[target.as_str()]);
        let mut client = TcpStream::connect(("127.0.0.1", tunnel.port())).expect("in");
        client
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut buf = [0u8; 1];
        assert_eq!(
            client.read(&mut buf).unwrap_or(0),
            0,
            "closed, nothing carried"
        );
        assert!(host.accept().is_err(), "the host was never connected to");
        drop(tunnel);
        assert!(
            told.lock()
                .unwrap()
                .iter()
                .any(|(t, by, _)| t.as_deref() == Some(target.as_str()) && *by == "refused"),
            "{:?}",
            told.lock().unwrap()
        );
    }

    #[test]
    fn a_tunnel_that_ends_closes_what_it_carries_and_takes_no_more() {
        let host = TcpListener::bind("127.0.0.1:0").expect("a host");
        let at = format!("127.0.0.1:{}", host.local_addr().unwrap().port());
        let (tunnel, _) = opened(&at, &[at.as_str()]);
        let port = tunnel.port();
        let mut client = TcpStream::connect(("127.0.0.1", port)).expect("in");
        let (_held, _) = host.accept().expect("carried");
        drop(tunnel);
        client
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut buf = [0u8; 1];
        assert_eq!(
            client.read(&mut buf).unwrap_or(0),
            0,
            "closed with the tunnel"
        );
        // Nothing listens there any more, or what does closes at once.
        if let Ok(mut late) = TcpStream::connect(("127.0.0.1", port)) {
            late.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
            assert_eq!(late.read(&mut buf).unwrap_or(0), 0);
        }
    }
}
