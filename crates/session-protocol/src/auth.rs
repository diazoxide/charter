//! Admission: every connection to `charterd` is one client scope, proved by that scope's
//! credential, before anything else it says is read (FD-6, ADR 0068 §5).
//!
//! **The exchange is mutual**, and comes right after the version is agreed and before the
//! multiplexer starts (ADR 0068 §5, as amended by FD-6):
//!
//! 1. The host sends a fresh random challenge.
//! 2. The client answers with the scope it asks to be, a fresh random nonce of its own, and its
//!    proof: an HMAC-SHA256, keyed by that scope's credential, over [`CLIENT_LABEL`], the scope
//!    and the host's challenge.
//! 3. The host computes the same proof and compares the two in constant time. It admits the
//!    client with a proof of its own, an HMAC-SHA256 under the same credential over
//!    [`HOST_LABEL`], the scope, its challenge and the client's nonce, or it refuses.
//! 4. The client checks the host's proof in constant time, and refuses a host that cannot make
//!    it.
//!
//! Only a connection both ends admitted becomes a [`crate::link::Link`], and the link knows
//! which scope it is, so what a scope may do (FD-27) is decided against a fact the host
//! checked, never against a claim. The two labels differ, so neither side's proof can ever be
//! reflected back as the other's.
//!
//! **The credential never crosses the wire, and whatever answers at the socket's path must
//! hold it.** A process of the same user that bound the path while the host was down is told
//! only a proof bound to the challenge it sent itself, which the real host, never sending the
//! same challenge twice, refuses; and it cannot make the host's proof for the client's fresh
//! nonce, so the client refuses it before it says anything else. A same-user process that can
//! already put itself between a client and a running host could relay one live admission;
//! such a process can already open the credential files, which is why the chat sandbox denies
//! a chat both the files and the socket (ADR 0068 §5).
//!
//! **It fails closed.** There is no anonymous scope, not even for listing. An answer that
//! names no scope, a scope this end does not know, no proof, a wrong one, or one made with
//! another scope's credential, is refused, and so is a client that has not finished within
//! [`crate::version::HANDSHAKE_TIMEOUT`].
//!
//! **Which scopes carry a credential here.** The human client scopes on `charterd.sock`:
//! [`Scope::ALL`]. The two others authenticate elsewhere and are not scopes of this exchange: a
//! chat reaches its host only on its plane's hook socket, with its own per-chat token
//! (`hookwire`), and a peer device by its link key inside a Noise handshake (ADR 0078 §3).
//!
//! **A credential lives as long as the host that minted it.** The host mints one per scope
//! every time it starts ([`Credentials::mint_into`]), into one `0600` file per scope in a
//! `0700` directory the chat sandbox denies (ADR 0067 §5, class 3). A client, the app's
//! window included, reads its own scope's file ([`Credential::read`]), so there is one
//! mechanism whoever started the host, and a crash or an upgrade rotates every credential.
//!
//! **On the wire,** the three messages are framed as the version's are
//! ([`crate::version::MAGIC`], then one length-delimited frame of JSON):
//! `{"challenge":"<hex>"}`, then `{"scope":"terminal","nonce":"<hex>","proof":"<hex>"}`,
//! then `{"admit":{"scope":"terminal","proof":"<hex>"}}` or `{"refuse":{"why":"…"}}`.

use std::io::Cursor;
use std::path::Path;

use bytes::BytesMut;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, join, split};

use crate::version::{self, Negotiated, Refused};

/// What the client's proof is made over first, so it can only ever be a client's proof of this
/// exchange, in this version of it, and never a MAC the credential made for anything else.
pub const CLIENT_LABEL: &[u8] = b"charter session protocol 1: the client proves its scope";

/// What the host's proof is made over first. Distinct from [`CLIENT_LABEL`], so a client's
/// proof can never be reflected back to it as the host's.
pub const HOST_LABEL: &[u8] =
    b"charter session protocol 1: the host proves it minted the credential";

/// How many random bytes a credential is: 256 bits from the operating system's generator,
/// the size of a chat's token.
const A_CREDENTIAL_IS: usize = 32;

/// A human client scope on `charterd.sock` (ADR 0068 §5, ADR 0081 for `editor`). What each
/// may do is FD-27's; this is who it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Scope {
    /// The app's window.
    LocalUi,
    /// `charter attach` and `charter ls` from the operator's shell.
    Terminal,
    /// The fleet MCP server.
    FleetMcp,
    /// `charter inbox` and approvals from a shell.
    Approval,
    /// The editor's charter extension.
    Editor,
}

impl Scope {
    /// Every scope that carries a credential file.
    pub const ALL: [Scope; 5] = [
        Scope::LocalUi,
        Scope::Terminal,
        Scope::FleetMcp,
        Scope::Approval,
        Scope::Editor,
    ];

    /// The scope's word: on the wire, and as its credential file's name.
    pub fn word(self) -> &'static str {
        match self {
            Scope::LocalUi => "local-ui",
            Scope::Terminal => "terminal",
            Scope::FleetMcp => "fleet-mcp",
            Scope::Approval => "approval",
            Scope::Editor => "editor",
        }
    }

    /// The scope a word names, where it names one.
    pub fn from_word(word: &str) -> Option<Scope> {
        Scope::ALL.into_iter().find(|scope| scope.word() == word)
    }
}

impl std::fmt::Display for Scope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.word())
    }
}

impl Serialize for Scope {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.word())
    }
}

impl<'de> Deserialize<'de> for Scope {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let word = String::deserialize(deserializer)?;
        Scope::from_word(&word)
            .ok_or_else(|| serde::de::Error::custom(format!("no client scope is named {word:?}")))
    }
}

/// One scope's credential: 32 random bytes, as hex. Never written out by `Debug`, and compared
/// in constant time.
#[derive(Clone)]
pub struct Credential(String);

impl PartialEq for Credential {
    fn eq(&self, other: &Self) -> bool {
        use subtle::ConstantTimeEq;
        bool::from(self.0.as_bytes().ct_eq(other.0.as_bytes()))
    }
}

impl Eq for Credential {}

impl Credential {
    /// A fresh one, from the operating system's generator.
    pub fn mint() -> std::io::Result<Self> {
        let mut bytes = [0u8; A_CREDENTIAL_IS];
        getrandom::fill(&mut bytes).map_err(std::io::Error::other)?;
        Ok(Credential(
            bytes.iter().map(|byte| format!("{byte:02x}")).collect(),
        ))
    }

    /// `scope`'s credential, as the host that is running minted it into `dir`. The file must be
    /// a regular file of this user's that nobody else may read or write, never a link
    /// (`charter_same_user::read_private_file`). A missing file, and one that does not hold a
    /// credential, are errors, never an empty credential.
    #[cfg(unix)]
    pub fn read(dir: &Path, scope: Scope) -> std::io::Result<Self> {
        let text = charter_same_user::read_private_file(&dir.join(scope.word()))?;
        text.trim_end_matches('\n')
            .parse()
            .map_err(|_: NotACredential| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("the {scope} credential file does not hold a credential"),
                )
            })
    }

    /// The credential's text, for the wire.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

/// Text that is not a credential: not 64 hex digits.
#[derive(Debug, thiserror::Error)]
#[error("not a credential")]
pub struct NotACredential;

impl std::str::FromStr for Credential {
    type Err = NotACredential;

    /// A credential from its text, as a client is handed it by other means than the file.
    fn from_str(text: &str) -> Result<Self, NotACredential> {
        if text.len() == 2 * A_CREDENTIAL_IS && text.bytes().all(|b| b.is_ascii_hexdigit()) {
            Ok(Credential(text.to_owned()))
        } else {
            Err(NotACredential)
        }
    }
}

impl std::fmt::Debug for Credential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Credential(…)")
    }
}

/// The credential of every scope, as the host holds them for one start.
#[derive(Clone, Debug)]
pub struct Credentials([Credential; Scope::ALL.len()]);

impl Credentials {
    /// A fresh credential for every scope, held in memory only.
    pub fn mint() -> std::io::Result<Self> {
        Ok(Credentials([
            Credential::mint()?,
            Credential::mint()?,
            Credential::mint()?,
            Credential::mint()?,
            Credential::mint()?,
        ]))
    }

    /// A fresh credential for every scope, each written to its own `0600` file in `dir`, which
    /// is made `0700` and must be a directory of this user's, never a link
    /// (`charter_same_user::private_directory`). A file already
    /// there is replaced whole, by a rename, so a client never reads half of one.
    #[cfg(unix)]
    pub fn mint_into(dir: &Path) -> std::io::Result<Self> {
        use std::io::Write;

        charter_same_user::private_directory(dir)?;
        let held = Credentials::mint()?;
        for scope in Scope::ALL {
            let mut file = tempfile::Builder::new()
                .prefix(".minting-")
                .permissions(std::os::unix::fs::PermissionsExt::from_mode(0o600))
                .tempfile_in(dir)?;
            file.write_all(held.of(scope).expose().as_bytes())?;
            file.as_file().sync_all()?;
            file.persist(dir.join(scope.word()))
                .map_err(|failed| failed.error)?;
        }
        Ok(held)
    }

    /// `scope`'s credential.
    pub fn of(&self, scope: Scope) -> &Credential {
        let at = Scope::ALL
            .iter()
            .position(|each| *each == scope)
            .unwrap_or_default();
        &self.0[at]
    }

    /// Whether `proof` is the client's proof for `scope` and `challenge`.
    fn admits(&self, scope: Scope, challenge: &str, proof: &str) -> bool {
        same(&client_proof(self.of(scope), scope, challenge), proof)
    }
}

/// Whether two proofs are the same, compared in constant time, so how long a wrong one takes
/// to refuse says nothing about how much of it was right.
fn same(expected: &str, given: &str) -> bool {
    use subtle::ConstantTimeEq;
    bool::from(expected.as_bytes().ct_eq(given.as_bytes()))
}

/// The client's proof that it holds `scope`'s credential, for the host's `challenge`.
fn client_proof(credential: &Credential, scope: Scope, challenge: &str) -> String {
    prove(
        credential,
        CLIENT_LABEL,
        &[scope.word().as_bytes(), challenge.as_bytes()],
    )
}

/// The host's proof that it holds `scope`'s credential, for its own `challenge` and the
/// client's `nonce`.
fn host_proof(credential: &Credential, scope: Scope, challenge: &str, nonce: &str) -> String {
    prove(
        credential,
        HOST_LABEL,
        &[
            scope.word().as_bytes(),
            challenge.as_bytes(),
            nonce.as_bytes(),
        ],
    )
}

/// HMAC-SHA256 keyed by `credential`, over `label` and then each of `parts`, each ended by a
/// zero byte so no two of them can run together. As hex.
fn prove(credential: &Credential, label: &[u8], parts: &[&[u8]]) -> String {
    use hmac::{KeyInit, Mac};
    let mut mac = hmac::Hmac::<sha2::Sha256>::new_from_slice(credential.0.as_bytes())
        .unwrap_or_else(|_| unreachable!("HMAC takes a key of any length"));
    for part in std::iter::once(label).chain(parts.iter().copied()) {
        mac.update(part);
        mac.update(&[0]);
    }
    hex(&mac.finalize().into_bytes())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// How many random bytes a challenge, and a client's nonce, is.
const A_CHALLENGE_IS: usize = 32;

/// Whether `text` is a challenge or a nonce: 32 bytes, as hex.
fn is_a_challenge(text: &str) -> bool {
    text.len() == 2 * A_CHALLENGE_IS && text.bytes().all(|b| b.is_ascii_hexdigit())
}

/// A fresh challenge, or nonce, as hex.
fn challenge() -> std::io::Result<String> {
    let mut bytes = [0u8; A_CHALLENGE_IS];
    getrandom::fill(&mut bytes).map_err(std::io::Error::other)?;
    Ok(hex(&bytes))
}

/// What the host sends first.
#[derive(Serialize, Deserialize)]
struct Challenge {
    challenge: String,
}

/// What the client answers. Every field is optional on the host's side, so an answer that
/// lacks one is met with a refusal rather than a closed stream.
#[derive(Serialize, Deserialize)]
struct Answer {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    scope: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    nonce: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    proof: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Verdict {
    /// Admitted as `scope`, with the host's own proof that it holds the scope's credential.
    Admit {
        scope: Scope,
        proof: String,
    },
    Refuse {
        why: String,
    },
}

/// What the host says to every refused answer, whatever was wrong with it, so a refusal says
/// nothing about which part was right.
const NOT_ADMITTED: &str = "a connection to charterd names one client scope and proves it holds \
     the credential the running host minted for it";

/// The client's side: prove to be `scope` with its `credential`, and return the stream to go
/// on with once the host has admitted it. Refused if the host has not finished within
/// [`crate::version::HANDSHAKE_TIMEOUT`].
pub async fn present<S: AsyncRead + AsyncWrite>(
    io: S,
    scope: Scope,
    credential: &Credential,
) -> Result<Negotiated<S>, Refused> {
    version::within_the_deadline(present_unbounded(io, scope, credential)).await
}

/// The host's side: challenge the client, and admit it as the scope it names when its proof
/// is that scope's credential's, returning the scope and the stream to go on with. Anything
/// else is refused, and the client is told so. Refused if the client has not finished within
/// [`crate::version::HANDSHAKE_TIMEOUT`].
pub async fn admit<S: AsyncRead + AsyncWrite>(
    io: S,
    held: &Credentials,
) -> Result<(Scope, Negotiated<S>), Refused> {
    version::within_the_deadline(admit_unbounded(io, held)).await
}

/// [`present`] with no deadline of its own, for a caller that holds one over the whole
/// handshake ([`crate::link::connect`]).
pub(crate) async fn present_unbounded<S: AsyncRead + AsyncWrite>(
    io: S,
    scope: Scope,
    credential: &Credential,
) -> Result<Negotiated<S>, Refused> {
    let (mut reads, mut writes) = split(io);
    let (challenge, left): (Challenge, BytesMut) = version::receive(&mut reads).await?;
    if !is_a_challenge(&challenge.challenge) {
        return Err(Refused::Malformed(
            "the host's challenge is not one".to_owned(),
        ));
    }
    if !left.is_empty() {
        return Err(Refused::Malformed(
            "the host sent more before this client answered".to_owned(),
        ));
    }
    let nonce = self::challenge()?;
    version::send(
        &mut writes,
        &Answer {
            scope: Some(scope.word().to_owned()),
            nonce: Some(nonce.clone()),
            proof: Some(client_proof(credential, scope, &challenge.challenge)),
        },
    )
    .await?;
    let (verdict, left): (Verdict, BytesMut) = version::receive(&mut reads).await?;
    match verdict {
        Verdict::Admit {
            scope: admitted,
            proof,
        } if admitted == scope => {
            if same(
                &host_proof(credential, scope, &challenge.challenge, &nonce),
                &proof,
            ) {
                Ok(join(Cursor::new(left).chain(reads), writes))
            } else {
                Err(Refused::HostUnproven)
            }
        }
        Verdict::Admit {
            scope: admitted, ..
        } => Err(Refused::Malformed(format!(
            "the host admitted this client as {admitted}, which it never asked to be"
        ))),
        Verdict::Refuse { why } => Err(Refused::NotAdmitted(why)),
    }
}

/// [`admit`] with no deadline of its own, for a caller that holds one over the whole handshake
/// ([`crate::link::serve`]).
pub(crate) async fn admit_unbounded<S: AsyncRead + AsyncWrite>(
    io: S,
    held: &Credentials,
) -> Result<(Scope, Negotiated<S>), Refused> {
    let (mut reads, mut writes) = split(io);
    let challenge = challenge()?;
    version::send(
        &mut writes,
        &Challenge {
            challenge: challenge.clone(),
        },
    )
    .await?;
    let (answer, left): (Answer, BytesMut) = version::receive(&mut reads).await?;
    let scope = answer.scope.as_deref().and_then(Scope::from_word);
    let nonce = answer
        .nonce
        .as_deref()
        .filter(|nonce| is_a_challenge(nonce));
    match (scope, nonce, answer.proof.as_deref()) {
        (Some(scope), Some(nonce), Some(proof)) if held.admits(scope, &challenge, proof) => {
            let proof = host_proof(held.of(scope), scope, &challenge, nonce);
            version::send(&mut writes, &Verdict::Admit { scope, proof }).await?;
            Ok((scope, join(Cursor::new(left).chain(reads), writes)))
        }
        _ => {
            version::send(
                &mut writes,
                &Verdict::Refuse {
                    why: NOT_ADMITTED.to_owned(),
                },
            )
            .await?;
            Err(Refused::Unauthenticated)
        }
    }
}
