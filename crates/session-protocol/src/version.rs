//! Version negotiation: the first thing on every stream, before the multiplexer starts.
//!
//! **The exchange.** The client sends a hello naming every version it speaks, one per major
//! with the highest minor it has of that major. The host answers with the highest major both
//! speak, at the lower of the two minors, or refuses and says which majors it speaks. Then the
//! client is admitted as one client scope ([`crate::auth`]), and only then do both sides start
//! the multiplexer at that version. A host speaks its own major and the one before
//! it, which is how a client and a host one version apart always talk (ADR 0068 §4, N−1).
//!
//! **It fails closed.** No shared major is a refusal on both ends, never a guess. A client
//! does not believe an answer naming a version it never offered. A stream that does not open
//! with [`MAGIC`] is not this protocol and is refused before a byte of it is parsed, and so is
//! a hello longer than [`MOST_HELLO_BYTES`].
//!
//! **On the wire,** each message is [`MAGIC`], then one frame of tokio-util's
//! `LengthDelimitedCodec` with a big-endian `u16` length, holding JSON. JSON, because this is the one message that must stay readable to every future
//! version, and to a person reading a capture. Fields a version does not know are ignored, so
//! a later minor may add some.
//!
//! The magic is PNG's trick: a first byte with the high bit set, then `\r\n`, `\x1a` and `\n`,
//! so a stream that strips the eighth bit, rewrites line endings or stops at a DOS end-of-file
//! fails at the first message instead of later and stranger.

use std::io::Cursor;

use bytes::{Bytes, BytesMut};
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::io::{
    AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, Chain, Join, ReadHalf, WriteHalf, join,
    split,
};
use tokio_util::codec::{FramedRead, FramedWrite, LengthDelimitedCodec, LengthDelimitedCodecError};

/// The stream after the handshake. Reading the answer may read past it, into what the other
/// end sent next; those bytes come first.
pub type Negotiated<S> = Join<Chain<Cursor<BytesMut>, ReadHalf<S>>, WriteHalf<S>>;

/// What every message of the negotiation opens with.
pub const MAGIC: &[u8; 8] = b"\x89CSP\r\n\x1a\n";

/// How long either end waits for the other's half of the handshake. [`crate::link`] holds one
/// deadline of this length over the whole of it: the version, the admission and the control
/// lane together.
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
    #[error("the other end is not speaking purlis's session protocol")]
    NotThisProtocol,
    /// A hello or an answer longer than [`MOST_HELLO_BYTES`].
    #[error("a negotiation message longer than {most} bytes")]
    TooLong { most: usize },
    /// A message that is not the shape this version reads.
    #[error("a negotiation message that cannot be read: {0}")]
    Malformed(String),
    /// The host refused this client's credential, and said why ([`crate::auth`]).
    #[error("the host did not admit this client: {0}")]
    NotAdmitted(String),
    /// The host admitted this client without proving it holds the scope's credential, so it
    /// is not the host that minted it ([`crate::auth`]).
    #[error(
        "the other end admitted this client without proving it is the host that minted its credential"
    )]
    HostUnproven,
    /// The client's peer runs inside a chat, or could not be shown not to, and no scope on
    /// `charterd.sock` is ever a chat's (FD-27, V16a; [`crate::local`]). Said to the client
    /// whatever its proof was.
    #[error("{0}")]
    InsideAChat(String),
    /// The client presented no credential that admits it as a scope ([`crate::auth`]).
    #[error("the client presented no credential that admits it")]
    Unauthenticated,
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

/// The client's side: offer what `speaks` names, and return the version the host agreed to,
/// with the stream to go on with. A host that has not answered within [`HANDSHAKE_TIMEOUT`] is
/// refused.
pub async fn offer<S: AsyncRead + AsyncWrite>(
    io: S,
    speaks: &Speaks,
) -> Result<(Version, Negotiated<S>), Refused> {
    within_the_deadline(offer_unbounded(io, speaks)).await
}

/// [`offer`] with no deadline of its own, for a caller that holds one over the whole handshake
/// ([`crate::link::connect`]).
pub(crate) async fn offer_unbounded<S: AsyncRead + AsyncWrite>(
    io: S,
    speaks: &Speaks,
) -> Result<(Version, Negotiated<S>), Refused> {
    let (mut reads, mut writes) = split(io);
    let (version, left) = offer_now(&mut reads, &mut writes, speaks).await?;
    Ok((version, join(Cursor::new(left).chain(reads), writes)))
}

/// The host's side: read a client's hello, and agree to the highest major both speak or
/// refuse it, returning the stream to go on with. A client that has not sent its whole hello
/// within [`HANDSHAKE_TIMEOUT`] is refused.
pub async fn answer<S: AsyncRead + AsyncWrite>(
    io: S,
    speaks: &Speaks,
) -> Result<(Version, Negotiated<S>), Refused> {
    within_the_deadline(answer_unbounded(io, speaks)).await
}

/// [`answer`] with no deadline of its own, for a caller that holds one over the whole
/// handshake ([`crate::link::serve`]).
pub(crate) async fn answer_unbounded<S: AsyncRead + AsyncWrite>(
    io: S,
    speaks: &Speaks,
) -> Result<(Version, Negotiated<S>), Refused> {
    let (mut reads, mut writes) = split(io);
    let (version, left) = answer_now(&mut reads, &mut writes, speaks).await?;
    Ok((version, join(Cursor::new(left).chain(reads), writes)))
}

pub(crate) async fn within_the_deadline<T>(
    handshake: impl Future<Output = Result<T, Refused>>,
) -> Result<T, Refused> {
    tokio::time::timeout(HANDSHAKE_TIMEOUT, handshake)
        .await
        .unwrap_or(Err(Refused::TimedOut))
}

async fn offer_now<S: AsyncRead + AsyncWrite>(
    reads: &mut ReadHalf<S>,
    writes: &mut WriteHalf<S>,
    speaks: &Speaks,
) -> Result<(Version, BytesMut), Refused> {
    send(
        writes,
        &Hello {
            versions: speaks.0.clone(),
        },
    )
    .await?;
    let (answer, left) = receive::<_, Answer>(reads).await?;
    match answer {
        Answer::Accept(version) => match speaks.minor_of(version.major) {
            Some(minor) if version.minor <= minor => Ok((version, left)),
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

async fn answer_now<S: AsyncRead + AsyncWrite>(
    reads: &mut ReadHalf<S>,
    writes: &mut WriteHalf<S>,
    speaks: &Speaks,
) -> Result<(Version, BytesMut), Refused> {
    let (hello, left): (Hello, _) = receive(reads).await?;
    let theirs = Speaks::new(hello.versions);
    let agreed = speaks.0.iter().find_map(|ours| {
        theirs.minor_of(ours.major).map(|minor| Version {
            major: ours.major,
            minor: minor.min(ours.minor),
        })
    });
    match agreed {
        Some(version) => {
            send(writes, &Answer::Accept(version)).await?;
            Ok((version, left))
        }
        None => {
            send(
                writes,
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

/// The codec both messages are framed with.
fn codec() -> LengthDelimitedCodec {
    LengthDelimitedCodec::builder()
        .length_field_type::<u16>()
        .max_frame_length(MOST_HELLO_BYTES)
        .new_codec()
}

pub(crate) async fn send<W: AsyncWrite + Unpin, T: Serialize>(
    io: &mut W,
    message: &T,
) -> Result<(), Refused> {
    let body = serde_json::to_vec(message).map_err(|e| Refused::Malformed(e.to_string()))?;
    if body.len() > MOST_HELLO_BYTES {
        return Err(Refused::TooLong {
            most: MOST_HELLO_BYTES,
        });
    }
    io.write_all(MAGIC).await?;
    FramedWrite::new(&mut *io, codec())
        .send(Bytes::from(body))
        .await?;
    Ok(())
}

/// Read one message, returning it and whatever was read past it.
pub(crate) async fn receive<R: AsyncRead + Unpin, T: for<'de> Deserialize<'de>>(
    io: &mut R,
) -> Result<(T, BytesMut), Refused> {
    let mut magic = [0u8; MAGIC.len()];
    match io.read_exact(&mut magic).await {
        Ok(_) if &magic == MAGIC => {}
        Ok(_) => return Err(Refused::NotThisProtocol),
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
            return Err(Refused::NotThisProtocol);
        }
        Err(e) => return Err(e.into()),
    }
    let mut frames = FramedRead::new(&mut *io, codec());
    let body = match frames.next().await {
        Some(Ok(body)) => body,
        Some(Err(e))
            if e.get_ref()
                .is_some_and(|inner| inner.is::<LengthDelimitedCodecError>()) =>
        {
            return Err(Refused::TooLong {
                most: MOST_HELLO_BYTES,
            });
        }
        Some(Err(e)) => return Err(e.into()),
        None => return Err(Refused::NotThisProtocol),
    };
    let left = frames.read_buffer_mut().split();
    let message = serde_json::from_slice(&body).map_err(|e| Refused::Malformed(e.to_string()))?;
    Ok((message, left))
}
