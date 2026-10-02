//! Every connection to `charterd` presents a credential for one client scope, and the host
//! admits it as that scope or refuses it (FD-6, ADR 0068 §5). There is no anonymous scope,
//! not even for listing: a client with no credential, a wrong one, or another scope's never
//! gets a link.

use charter_session_protocol::auth::{Credential, Credentials, Scope};
use charter_session_protocol::link::{self, LinkError};
use charter_session_protocol::version::{Refused, Speaks, Version};
use tokio::io::AsyncWriteExt;

fn v1() -> Speaks {
    Speaks::new([Version { major: 1, minor: 0 }])
}

#[tokio::test]
async fn a_client_with_its_scopes_credential_is_admitted_as_that_scope() {
    let held = Credentials::mint().unwrap();
    let (a, b) = tokio::io::duplex(1 << 16);

    let (client, host) = tokio::join!(
        link::connect(a, v1(), Scope::Terminal, held.of(Scope::Terminal)),
        link::serve(b, v1(), &held)
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
    let held = Credentials::mint().unwrap();
    let stranger = Credential::mint().unwrap();
    for (scope, credential) in [
        (Scope::LocalUi, &stranger),
        // The `terminal` credential presented as `local-ui`: right bytes, wrong scope.
        (Scope::LocalUi, held.of(Scope::Terminal)),
    ] {
        let (a, b) = tokio::io::duplex(1 << 16);
        let (client, host) = tokio::join!(
            link::connect(a, v1(), scope, credential),
            link::serve(b, v1(), &held)
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

#[tokio::test]
async fn a_client_that_presents_no_credential_or_an_unknown_scope_is_refused() {
    let held = Credentials::mint().unwrap();
    for said in [
        // A credential frame with no credential in it.
        r#"{"scope":"local-ui"}"#,
        // The scopes that never authenticate with a file's credential are not scopes here.
        r#"{"scope":"chat","credential":"00"}"#,
        r#"{"scope":"remote-link","credential":"00"}"#,
        // No scope at all: there is no anonymous one.
        r#"{"credential":"00"}"#,
    ] {
        let (mut a, b) = tokio::io::duplex(1 << 16);
        let held = held.clone();
        let host = tokio::spawn(async move { link::serve(b, v1(), &held).await });
        // Negotiate the version by hand, then send the frame above in place of a credential.
        let (_, mut io) = charter_session_protocol::version::offer(&mut a, &v1())
            .await
            .unwrap();
        io.write_all(charter_session_protocol::version::MAGIC)
            .await
            .unwrap();
        io.write_all(&u16::try_from(said.len()).unwrap().to_be_bytes())
            .await
            .unwrap();
        io.write_all(said.as_bytes()).await.unwrap();
        io.flush().await.unwrap();

        let refused = host.await.unwrap();
        assert!(
            matches!(refused, Err(LinkError::Refused(_))),
            "{said} was admitted"
        );
    }
}

#[tokio::test]
async fn a_client_that_stops_after_the_version_is_refused_in_time() {
    tokio::time::pause();
    let held = Credentials::mint().unwrap();
    let (mut a, b) = tokio::io::duplex(1 << 16);
    let host = tokio::spawn(async move { link::serve(b, v1(), &held).await });
    let (_, _io) = charter_session_protocol::version::offer(&mut a, &v1())
        .await
        .unwrap();

    let refused = host.await.unwrap();

    assert!(
        matches!(refused, Err(LinkError::Refused(Refused::TimedOut))),
        "{:?}",
        refused.err()
    );
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

/// The whole path a connection to `charterd.sock` takes: the uid check, then the credential.
/// This test's own uid passes the first, and only the credential decides the second.
#[cfg(unix)]
#[tokio::test]
async fn over_the_hosts_socket_this_uid_is_admitted_only_with_its_credential() {
    use charter_session_protocol::local::Listener;

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("charterd.sock");
    let listener = Listener::new(tokio::net::UnixListener::bind(&path).unwrap());
    let held = Credentials::mint().unwrap();
    let stranger = Credential::mint().unwrap();
    let host_held = held.clone();
    let host = tokio::spawn(async move {
        let mut scopes = Vec::new();
        for _ in 0..2 {
            let stream = listener.accept().await.unwrap().expect("this uid");
            scopes.push(
                link::serve(stream, v1(), &host_held)
                    .await
                    .map(|l| l.scope()),
            );
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
        held.of(Scope::Approval),
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
