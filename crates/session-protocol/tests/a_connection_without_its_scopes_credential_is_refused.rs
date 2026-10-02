//! Every connection to `charterd` proves it holds the credential of one client scope, and the
//! host admits it as that scope or refuses it (FD-6, ADR 0068 §5). There is no anonymous scope,
//! not even for listing: a client with no proof, a wrong one, or one made with another scope's
//! credential never gets a link. The credential itself never crosses the wire, so whatever
//! answers at the socket's path learns nothing it can replay.

use charter_session_protocol::auth::{Credential, Credentials, Scope};
use charter_session_protocol::link::{self, LinkError};
use charter_session_protocol::version::{self, HANDSHAKE_TIMEOUT, Refused, Speaks, Version};
use tokio::io::duplex;

mod common;
use common::{HELD, read_frame, write_frame};

fn v1() -> Speaks {
    Speaks::new([Version { major: 1, minor: 0 }])
}

#[tokio::test]
async fn a_client_with_its_scopes_credential_is_admitted_as_that_scope() {
    let (a, b) = duplex(1 << 16);

    let (client, host) = tokio::join!(
        link::connect(a, v1(), Scope::Terminal, HELD.of(Scope::Terminal)),
        link::serve_any(b, v1(), &HELD)
    );

    let (mut client, mut host) = (client.unwrap(), host.unwrap());
    assert_eq!(client.scope(), Scope::Terminal);
    assert_eq!(host.scope(), Scope::Terminal);
    // And the link works past the admission.
    client
        .control()
        .send(bytes::Bytes::from_static(b"list"))
        .await
        .unwrap();
    assert_eq!(&host.control().next().await.unwrap().unwrap()[..], b"list");
}

#[tokio::test]
async fn a_wrong_credential_or_another_scopes_is_refused_on_both_ends() {
    let stranger = Credential::mint().unwrap();
    for (scope, credential) in [
        (Scope::LocalUi, &stranger),
        // The `terminal` credential used for `local-ui`: a real credential, the wrong scope.
        (Scope::LocalUi, HELD.of(Scope::Terminal)),
    ] {
        let (a, b) = duplex(1 << 16);
        let (client, host) = tokio::join!(
            link::connect(a, v1(), scope, credential),
            link::serve_any(b, v1(), &HELD)
        );
        assert!(
            matches!(host, Err(LinkError::Refused(Refused::Unauthenticated))),
            "the host admitted {scope}"
        );
        assert!(
            matches!(client, Err(LinkError::Refused(Refused::NotAdmitted(_)))),
            "the client believed it was admitted as {scope}"
        );
    }
}

/// The host's verdict on `answer`, sent by hand in reply to its challenge.
async fn verdict_on(
    answer: impl FnOnce(&str) -> String,
) -> (Result<Scope, LinkError>, serde_json::Value) {
    let (mut a, b) = duplex(1 << 16);
    let host =
        tokio::spawn(async move { link::serve_any(b, v1(), &HELD).await.map(|l| l.scope()) });
    let (_, mut io) = version::offer(&mut a, &v1()).await.unwrap();
    let challenge = read_frame(&mut io).await;
    write_frame(&mut io, &answer(challenge["challenge"].as_str().unwrap())).await;
    let said = read_frame(&mut io).await;
    (host.await.unwrap(), said)
}

#[tokio::test]
async fn an_answer_with_no_proof_no_scope_or_a_scope_that_carries_no_credential_is_refused() {
    let credential = HELD.of(Scope::LocalUi).expose().to_owned();
    for answer in [
        r#"{"scope":"local-ui"}"#.to_owned(),
        r#"{"proof":"00"}"#.to_owned(),
        // The scopes that never authenticate with a credential file are not scopes here.
        r#"{"scope":"chat","proof":"00"}"#.to_owned(),
        r#"{"scope":"remote-link","proof":"00"}"#.to_owned(),
        // The credential itself, sent as a bearer token, is not a proof.
        format!(r#"{{"scope":"local-ui","credential":"{credential}"}}"#),
        format!(r#"{{"scope":"local-ui","proof":"{credential}"}}"#),
    ] {
        let (verdict, said) = verdict_on(|_| answer.clone()).await;

        assert!(
            matches!(verdict, Err(LinkError::Refused(Refused::Unauthenticated))),
            "{answer} was admitted"
        );
        assert!(
            said.get("refuse").is_some(),
            "{answer}: the host said {said}"
        );
    }
}

#[tokio::test]
async fn a_fake_host_learns_nothing_it_can_replay_to_the_real_one() {
    // Something of the same user bound the socket's path while the host was down. It answers
    // the version, sends a challenge of its own and keeps whatever the client sends back.
    let (client_end, mut fake_end) = duplex(1 << 16);
    let fake = tokio::spawn(async move {
        let (_, mut io) = version::answer(&mut fake_end, &v1()).await.unwrap();
        write_frame(
            &mut io,
            &format!(r#"{{"challenge":"{}"}}"#, "ab".repeat(32)),
        )
        .await;
        read_frame(&mut io).await
    });
    let fooled = link::connect(client_end, v1(), Scope::Approval, HELD.of(Scope::Approval)).await;
    assert!(
        fooled.is_err(),
        "the client believed the fake host admitted it"
    );
    let kept = fake.await.unwrap();

    // It holds no credential: only a proof bound to the challenge it chose.
    assert!(
        !kept.to_string().contains(HELD.of(Scope::Approval).expose()),
        "{kept}"
    );

    // Replayed to the real host, which challenges with a fresh value, the proof is refused.
    let (verdict, said) = verdict_on(|_| kept.to_string()).await;
    assert!(
        matches!(verdict, Err(LinkError::Refused(Refused::Unauthenticated))),
        "a replayed proof was admitted as {verdict:?}"
    );
    assert!(said.get("refuse").is_some(), "{said}");
}

#[tokio::test]
async fn two_challenges_are_never_the_same() {
    let mut seen = std::collections::HashSet::new();
    for _ in 0..32 {
        let (verdict, _) = verdict_on(|challenge| {
            assert!(
                seen.insert(challenge.to_owned()),
                "challenge {challenge} came twice"
            );
            "{}".to_owned()
        })
        .await;
        assert!(verdict.is_err());
    }
}

#[tokio::test(start_paused = true)]
async fn a_client_that_stops_after_the_version_is_refused_at_the_deadline() {
    let (mut a, b) = duplex(1 << 16);
    let host = tokio::spawn(async move { link::serve_any(b, v1(), &HELD).await.map(|_| ()) });
    let (_, _io) = version::offer(&mut a, &v1()).await.unwrap();
    let started = tokio::time::Instant::now();

    let refused = host.await.unwrap();

    assert!(matches!(refused, Err(LinkError::TimedOut)), "{refused:?}");
    assert_eq!(started.elapsed(), HANDSHAKE_TIMEOUT);
}

#[tokio::test(start_paused = true)]
async fn one_deadline_covers_the_whole_handshake_however_its_steps_are_spread() {
    // Each step on its own is well inside the deadline; together they are past it.
    let step = HANDSHAKE_TIMEOUT * 6 / 10;
    let (mut a, b) = duplex(1 << 16);
    let started = tokio::time::Instant::now();
    let host = tokio::spawn(async move {
        let served = link::serve_any(b, v1(), &HELD).await.map(|_| ());
        (served, started.elapsed())
    });
    tokio::time::sleep(step).await;
    let (_, io) = version::offer(&mut a, &v1()).await.unwrap();
    tokio::time::sleep(step).await;
    let _io =
        charter_session_protocol::auth::present(io, Scope::LocalUi, HELD.of(Scope::LocalUi)).await;

    let (refused, at) = host.await.unwrap();

    assert!(matches!(refused, Err(LinkError::TimedOut)), "{refused:?}");
    assert_eq!(at, HANDSHAKE_TIMEOUT);
}

/// The whole path a connection to `charterd.sock` takes: the uid check, then the proof. This
/// test's own uid passes the first, and only the credential decides the second.
#[cfg(unix)]
#[tokio::test]
async fn over_the_hosts_socket_this_uid_is_admitted_only_with_its_credential() {
    use charter_session_protocol::local::Listener;

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("charterd.sock");
    let listener = Listener::new(tokio::net::UnixListener::bind(&path).unwrap());
    let stranger = Credential::mint().unwrap();
    let host = tokio::spawn(async move {
        let mut scopes = Vec::new();
        for _ in 0..2 {
            let stream = listener.accept().await.unwrap().expect("this uid");
            scopes.push(link::serve(stream, v1(), &HELD).await.map(|l| l.scope()));
        }
        scopes
    });

    let refused = link::connect(
        tokio::net::UnixStream::connect(&path).await.unwrap(),
        v1(),
        Scope::Approval,
        &stranger,
    )
    .await;
    let admitted = link::connect(
        tokio::net::UnixStream::connect(&path).await.unwrap(),
        v1(),
        Scope::Approval,
        HELD.of(Scope::Approval),
    )
    .await;

    assert!(matches!(
        refused,
        Err(LinkError::Refused(Refused::NotAdmitted(_)))
    ));
    assert_eq!(admitted.unwrap().scope(), Scope::Approval);
    let seen = host.await.unwrap();
    assert!(matches!(
        seen[0],
        Err(LinkError::Refused(Refused::Unauthenticated))
    ));
    assert_eq!(seen[1].as_ref().ok(), Some(&Scope::Approval));
}

#[test]
fn credentials_are_minted_fresh_and_never_written_out_by_debug() {
    let one = Credentials::mint().unwrap();
    let two = Credentials::mint().unwrap();
    for scope in Scope::ALL {
        let credential = one.of(scope);
        assert_ne!(
            credential,
            two.of(scope),
            "{scope} was minted twice the same"
        );
        assert_eq!(credential.expose().len(), 64);
        assert!(!format!("{credential:?}").contains(credential.expose()));
        assert!(!format!("{one:?}").contains(credential.expose()));
    }
    // Each scope has its own.
    assert_ne!(one.of(Scope::LocalUi), one.of(Scope::Terminal));
}
