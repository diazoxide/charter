//! Admission: every connection to `charterd` is one client scope, proved by that scope's
//! credential, before anything else it says is read (FD-6, ADR 0068 §5).
//!
//! **The exchange** comes right after the version is agreed and before the multiplexer
//! starts. The client sends the scope it asks to be and that scope's credential; the host
//! admits it as that scope or refuses it, and either way says so. Only an admitted connection
//! becomes a [`crate::link::Link`], and the link knows which scope it is, so what a scope may
//! do (FD-27) is decided against a fact the host checked, never against a claim.
//!
//! **It fails closed.** There is no anonymous scope, not even for listing. A frame that names
//! no scope, a scope this end does not know, no credential, a wrong one, or another scope's,
//! is refused, and so is a client that has not finished within
//! [`crate::version::HANDSHAKE_TIMEOUT`]. Credentials are compared in constant time.
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
//! **On the wire,** both messages are framed as the version's are ([`crate::version::MAGIC`],
//! then one length-delimited frame of JSON): `{"scope":"terminal","credential":"<hex>"}`, then
//! `{"admit":"terminal"}` or `{"refuse":{"why":"…"}}`.

use std::io::Cursor;
use std::path::Path;

use bytes::BytesMut;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, join, split};

use crate::version::{self, Negotiated, Refused};

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

/// One scope's credential: 32 random bytes, as hex. Never written out by `Debug`.
#[derive(Clone, PartialEq, Eq)]
pub struct Credential(String);

impl Credential {
    /// A fresh one, from the operating system's generator.
    pub fn mint() -> std::io::Result<Self> {
        let mut bytes = [0u8; A_CREDENTIAL_IS];
        getrandom::fill(&mut bytes).map_err(std::io::Error::other)?;
        Ok(Credential(
            bytes.iter().map(|byte| format!("{byte:02x}")).collect(),
        ))
    }

    /// `scope`'s credential, as the host that is running minted it into `dir`. A missing file,
    /// and one that does not hold a credential, are errors, never an empty credential.
    pub fn read(dir: &Path, scope: Scope) -> std::io::Result<Self> {
        let text = std::fs::read_to_string(dir.join(scope.word()))?;
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
    /// is made `0700` and must be a directory of this user's, never a link. A file already
    /// there is replaced whole, by a rename, so a client never reads half of one.
    #[cfg(unix)]
    pub fn mint_into(dir: &Path) -> std::io::Result<Self> {
        use std::io::Write;

        private_directory(dir)?;
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

    /// Whether `presented` is `scope`'s credential, compared in constant time, so how long a
    /// wrong one takes to refuse says nothing about how much of it was right.
    pub fn admits(&self, scope: Scope, presented: &Credential) -> bool {
        self.admits_text(scope, presented.expose())
    }

    fn admits_text(&self, scope: Scope, presented: &str) -> bool {
        use subtle::ConstantTimeEq;
        bool::from(self.of(scope).0.as_bytes().ct_eq(presented.as_bytes()))
    }
}

/// Makes `dir` this user's alone: created at `0700` as it is made, and when it is already
/// there, refused unless it is a directory (never a link to one), then set to `0700`, which
/// fails for a directory another user owns. `hookwire`'s directory for the hook socket is made
/// the same way.
#[cfg(unix)]
fn private_directory(dir: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};

    if let Some(above) = dir.parent() {
        std::fs::create_dir_all(above)?;
    }
    match std::fs::DirBuilder::new().mode(0o700).create(dir) {
        Ok(()) => return Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(err) => return Err(err),
    }
    if !std::fs::symlink_metadata(dir)?.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            format!("{} is not a directory", dir.display()),
        ));
    }
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
}

/// What the client sends. Every field is optional on the host's side, so a frame that lacks
/// one is answered with a refusal rather than a closed stream.
#[derive(Serialize, Deserialize)]
struct Presented {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    scope: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    credential: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Verdict {
    Admit(Scope),
    Refuse { why: String },
}

/// What the host says to every refused credential, whatever was wrong with it, so a refusal
/// says nothing about which part was right.
const NOT_ADMITTED: &str = "a connection to charterd names one client scope and presents the credential the running host minted for it";

/// The client's side: ask to be `scope`, with its `credential`, and return the stream to go on
/// with once the host has admitted it.
pub async fn present<S: AsyncRead + AsyncWrite>(
    io: S,
    scope: Scope,
    credential: &Credential,
) -> Result<Negotiated<S>, Refused> {
    let (mut reads, mut writes) = split(io);
    let left = version::within_the_deadline(async {
        version::send(
            &mut writes,
            &Presented {
                scope: Some(scope.word().to_owned()),
                credential: Some(credential.expose().to_owned()),
            },
        )
        .await?;
        let (verdict, left): (Verdict, BytesMut) = version::receive(&mut reads).await?;
        match verdict {
            Verdict::Admit(admitted) if admitted == scope => Ok(left),
            Verdict::Admit(admitted) => Err(Refused::Malformed(format!(
                "the host admitted this client as {admitted}, which it never asked to be"
            ))),
            Verdict::Refuse { why } => Err(Refused::NotAdmitted(why)),
        }
    })
    .await?;
    Ok(join(Cursor::new(left).chain(reads), writes))
}

/// The host's side: read what the client presents, and admit it as the scope it names when
/// the credential is that scope's, returning the scope and the stream to go on with. Anything
/// else is refused, and the client is told so.
pub async fn admit<S: AsyncRead + AsyncWrite>(
    io: S,
    held: &Credentials,
) -> Result<(Scope, Negotiated<S>), Refused> {
    let (mut reads, mut writes) = split(io);
    let (scope, left) = version::within_the_deadline(async {
        let (presented, left): (Presented, BytesMut) = version::receive(&mut reads).await?;
        let scope = presented.scope.as_deref().and_then(Scope::from_word);
        match (scope, presented.credential.as_deref()) {
            (Some(scope), Some(credential)) if held.admits_text(scope, credential) => {
                version::send(&mut writes, &Verdict::Admit(scope)).await?;
                Ok((scope, left))
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
    })
    .await?;
    Ok((scope, join(Cursor::new(left).chain(reads), writes)))
}
