//! Version negotiation: the first thing on every stream, before the multiplexer starts.
//!
//! **The exchange.** The client sends a hello naming every version it speaks, one per major
//! with the highest minor it has of that major. The host answers with the highest major both
//! speak, at the lower of the two minors, or refuses and says which majors it speaks. Then both
//! sides start the multiplexer at that version. A host speaks its own major and the one before
//! it, which is how a client and a host one version apart always talk (ADR 0068 §4, N−1).
//!
//! **It fails closed.** No shared major is a refusal on both ends, never a guess. A client
//! does not believe an answer naming a version it never offered. A stream that does not open
//! with [`MAGIC`] is not this protocol and is refused before a byte of it is parsed, and so is
//! a hello longer than [`MOST_HELLO_BYTES`].
//!
//! **On the wire,** each message is [`MAGIC`], a big-endian `u16` length, and that many bytes
//! of JSON. JSON, because this is the one message that must stay readable to every future
//! version, and to a person reading a capture. Fields a version does not know are ignored, so
//! a later minor may add some.
//!
//! The magic is PNG's trick: a first byte with the high bit set, then `\r\n`, `\x1a` and `\n`,
//! so a stream that strips the eighth bit, rewrites line endings or stops at a DOS end-of-file
//! fails at the first message instead of later and stranger.

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// What every message of the negotiation opens with.
pub const MAGIC: &[u8; 8] = b"\x89CSP\r\n\x1a\n";

/// How long either end waits for the other's half of the handshake.
pub const HANDSHAKE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// The longest hello or answer either end reads. A real one is a few dozen bytes.
pub const MOST_HELLO_BYTES: usize = 4096;

/// One version of the protocol. Minors within a major only add; a new major is a break.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Version {
    pub major: u16,
    pub minor: u16,
}

/// The versions one end speaks: for each major, the highest minor it has.
#[derive(Debug, Clone)]
pub struct Speaks(Vec<Version>);

impl Speaks {
    /// Highest major first, one entry per major (the highest minor given for it wins).
    pub fn new(versions: impl IntoIterator<Item = Version>) -> Self {
        let mut all: Vec<Version> = versions.into_iter().collect();
        all.sort_by(|a, b| b.major.cmp(&a.major).then(b.minor.cmp(&a.minor)));
        all.dedup_by_key(|v| v.major);
        Speaks(all)
    }

    /// The majors, highest first.
    pub fn majors(&self) -> Vec<u16> {
        self.0.iter().map(|v| v.major).collect()
    }

    fn minor_of(&self, major: u16) -> Option<u16> {
        self.0.iter().find(|v| v.major == major).map(|v| v.minor)
    }
}

/// Why no version was agreed. Every one of them ends the stream.
#[derive(Debug, thiserror::Error)]
pub enum Refused {
    /// This end shares no major with the peer.
    #[error("no major version in common: this end speaks {ours:?}, the other end {theirs:?}")]
    NoSharedMajor { ours: Vec<u16>, theirs: Vec<u16> },
    /// The host refused this client, and said what it speaks.
    #[error("the host speaks major versions {speaks:?} and none of this client's")]
    ByPeer { speaks: Vec<u16> },
    /// The host accepted a major this client never offered.
    #[error("the host answered with major version {major}, which this client never offered")]
    NotOffered { major: u16 },
    /// The stream does not open with [`MAGIC`].
    #[error("the other end is not speaking charter's session protocol")]
    NotThisProtocol,
    /// A hello or an answer longer than [`MOST_HELLO_BYTES`].
    #[error("a negotiation message of {length} bytes, longer than {MOST_HELLO_BYTES}")]
    TooLong { length: usize },
    /// A message that is not the shape this version reads.
    #[error("a negotiation message that cannot be read: {0}")]
    Malformed(String),
    /// The other end did not finish its half of the handshake within [`HANDSHAKE_TIMEOUT`].
    #[error("the other end did not finish the handshake within {HANDSHAKE_TIMEOUT:?}")]
    TimedOut,
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

#[derive(Serialize, Deserialize)]
struct Hello {
    versions: Vec<Version>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Answer {
    Accept(Version),
    Refuse { speaks: Vec<u16> },
}

/// The client's side: offer what `speaks` names, and return the version the host agreed to.
/// A host that has not answered within [`HANDSHAKE_TIMEOUT`] is refused.
pub async fn offer<S: AsyncRead + AsyncWrite + Unpin>(
    io: &mut S,
    speaks: &Speaks,
) -> Result<Version, Refused> {
    within_the_deadline(offer_now(io, speaks)).await
}

/// The host's side: read a client's hello, and agree to the highest major both speak or
/// refuse it. A client that has not sent its whole hello within [`HANDSHAKE_TIMEOUT`] is
/// refused.
pub async fn answer<S: AsyncRead + AsyncWrite + Unpin>(
    io: &mut S,
    speaks: &Speaks,
) -> Result<Version, Refused> {
    within_the_deadline(answer_now(io, speaks)).await
}

async fn within_the_deadline<T>(
    handshake: impl Future<Output = Result<T, Refused>>,
) -> Result<T, Refused> {
    tokio::time::timeout(HANDSHAKE_TIMEOUT, handshake)
        .await
        .unwrap_or(Err(Refused::TimedOut))
}

async fn offer_now<S: AsyncRead + AsyncWrite + Unpin>(
    io: &mut S,
    speaks: &Speaks,
) -> Result<Version, Refused> {
    send(
        io,
        &Hello {
            versions: speaks.0.clone(),
        },
    )
    .await?;
    match receive::<_, Answer>(io).await? {
        Answer::Accept(version) => match speaks.minor_of(version.major) {
            Some(minor) if version.minor <= minor => Ok(version),
            Some(_) => Err(Refused::Malformed(format!(
                "the host agreed to minor {} of major {}, past this client's",
                version.minor, version.major
            ))),
            None => Err(Refused::NotOffered {
                major: version.major,
            }),
        },
        Answer::Refuse { speaks } => Err(Refused::ByPeer { speaks }),
    }
}

async fn answer_now<S: AsyncRead + AsyncWrite + Unpin>(
    io: &mut S,
    speaks: &Speaks,
) -> Result<Version, Refused> {
    let hello: Hello = receive(io).await?;
    let theirs = Speaks::new(hello.versions);
    let agreed = speaks.0.iter().find_map(|ours| {
        theirs.minor_of(ours.major).map(|minor| Version {
            major: ours.major,
            minor: minor.min(ours.minor),
        })
    });
    match agreed {
        Some(version) => {
            send(io, &Answer::Accept(version)).await?;
            Ok(version)
        }
        None => {
            send(
                io,
                &Answer::Refuse {
                    speaks: speaks.majors(),
                },
            )
            .await?;
            Err(Refused::NoSharedMajor {
                ours: speaks.majors(),
                theirs: theirs.majors(),
            })
        }
    }
}

async fn send<S: AsyncWrite + Unpin, T: Serialize>(io: &mut S, message: &T) -> Result<(), Refused> {
    let body = serde_json::to_vec(message).map_err(|e| Refused::Malformed(e.to_string()))?;
    let length = u16::try_from(body.len())
        .ok()
        .filter(|&n| usize::from(n) <= MOST_HELLO_BYTES)
        .ok_or(Refused::TooLong { length: body.len() })?;
    let mut frame = Vec::with_capacity(MAGIC.len() + 2 + body.len());
    frame.extend_from_slice(MAGIC);
    frame.extend_from_slice(&length.to_be_bytes());
    frame.extend_from_slice(&body);
    io.write_all(&frame).await?;
    io.flush().await?;
    Ok(())
}

async fn receive<S: AsyncRead + Unpin, T: for<'de> Deserialize<'de>>(
    io: &mut S,
) -> Result<T, Refused> {
    let mut magic = [0u8; MAGIC.len()];
    match io.read_exact(&mut magic).await {
        Ok(_) if &magic == MAGIC => {}
        Ok(_) => return Err(Refused::NotThisProtocol),
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
            return Err(Refused::NotThisProtocol);
        }
        Err(e) => return Err(e.into()),
    }
    let length = usize::from(io.read_u16().await?);
    if length > MOST_HELLO_BYTES {
        return Err(Refused::TooLong { length });
    }
    let mut body = vec![0u8; length];
    io.read_exact(&mut body).await?;
    serde_json::from_slice(&body).map_err(|e| Refused::Malformed(e.to_string()))
}
