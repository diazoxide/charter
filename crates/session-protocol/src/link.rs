//! A link: one ordered byte stream, negotiated, then multiplexed into a control lane and any
//! number of streams.
//!
//! **The multiplexer is Yamux** (the `yamux` crate, which libp2p and Substrate run in
//! production). It is the standard multiplexer for exactly this case: many independent
//! streams over one reliable, ordered connection that is not QUIC, with **per-stream credit**:
//! each stream has its own receive window, a sender stops when it has spent the window, and the
//! receiver grants more as it reads. A pane that stops reading stops only its own stream. It
//! asks nothing of the transport, so the same link runs over a unix socket, a pipe pair, a
//! child's stdio, and inside the Noise channel ADR 0078 §3 puts around a runner's connector,
//! unchanged. Yamux has no stream priorities, so the link schedules what it writes: the control
//! lane's frames go first (`crate::priority`).
//!
//! **The control lane** is the first stream, which the client opens. It carries
//! length-delimited frames (tokio-util's `LengthDelimitedCodec`: a big-endian `u32` length,
//! then that many bytes), each at most [`MOST_CONTROL_FRAME_BYTES`]. What a frame means is the
//! layer above this one (the session protocol's commands and events, FD-26); this layer
//! delivers each frame whole and in order.
//!
//! **Every other stream** is opened by either end with [`Link::open`] and taken by the other
//! with [`Link::accept`]. Yamux opens a stream lazily: the other end learns of it with its
//! first bytes, so whoever opens a stream writes first. The views of [`crate::view`] open with
//! their header, and the control lane with [`CONTROL_LANE`].
//!
//! **A link owns its connection.** It is driven by a task on the current tokio runtime, and it
//! ends when the [`Link`] is dropped or the transport closes, taking every stream with it.

use std::collections::VecDeque;
use std::future::poll_fn;
use std::sync::{Arc, OnceLock};
use std::task::Poll;

use bytes::Bytes;
use futures::{SinkExt, StreamExt};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::{mpsc, oneshot};
use tokio_util::codec::{Framed, LengthDelimitedCodec};
use tokio_util::compat::{FuturesAsyncReadCompatExt, TokioAsyncReadCompatExt};

use crate::auth::{self, Credential, Credentials, Scope};
use crate::priority::Prioritized;
use crate::version::{self, Refused, Speaks, Version};

/// The largest frame the control lane carries. A command or an event is small; a frame this
/// size is a peer that is wrong, and it is refused rather than buffered.
pub const MOST_CONTROL_FRAME_BYTES: usize = 1024 * 1024;

/// Streams the other end opened that nobody has accepted yet. Past this, a new stream is
/// refused: dropped, which resets it for the opener.
pub const MOST_UNACCEPTED_STREAMS: usize = 32;

/// The byte the client writes first on the control lane, which is what makes the host see it,
/// and what the host checks before it believes the first stream is the lane.
pub const CONTROL_LANE: u8 = 0x00;

/// Why a link could not start, or stopped.
#[derive(Debug, thiserror::Error)]
pub enum LinkError {
    #[error(transparent)]
    Refused(#[from] Refused),
    /// The first stream the client opened is not the control lane.
    #[error("the client's first stream is not the control lane")]
    NoControlLane,
    #[error("the link is closed")]
    Closed,
    /// The multiplexer ended the link, and this is why: the transport closed, or the other end
    /// sent something that is not Yamux.
    #[error("the link ended: {0}")]
    Ended(String),
    /// The other end did not finish the handshake (the version, the admission and the control
    /// lane) within its one deadline.
    #[error(
        "the other end did not finish the handshake within {:?}",
        crate::version::HANDSHAKE_TIMEOUT
    )]
    TimedOut,
    #[error(transparent)]
    Mux(#[from] yamux::ConnectionError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// One stream of a link, as tokio reads and writes it. It closes for writing on `shutdown`.
pub type Stream = tokio_util::compat::Compat<yamux::Stream>;

type Opening = oneshot::Sender<yamux::Result<yamux::Stream>>;

/// A negotiated, multiplexed link.
pub struct Link {
    version: Version,
    scope: Scope,
    control: Control,
    opener: Opener,
    inbound: Acceptor,
}

/// Why the driver stopped, once it has: what every handle answers after.
type Ending = Arc<OnceLock<String>>;

fn ended(why: &Ending) -> LinkError {
    match why.get() {
        Some(why) => LinkError::Ended(why.clone()),
        None => LinkError::Closed,
    }
}

/// The control lane: frames, whole and in order, both ways.
pub struct Control(Framed<Stream, LengthDelimitedCodec>);

impl Link {
    /// The version both ends agreed on.
    pub fn version(&self) -> Version {
        self.version
    }

    /// The client scope this link was admitted as ([`crate::auth`]).
    pub fn scope(&self) -> Scope {
        self.scope
    }

    pub fn control(&mut self) -> &mut Control {
        &mut self.control
    }

    /// Open a stream. The other end sees it once this end writes to it.
    pub async fn open(&mut self) -> Result<Stream, LinkError> {
        self.opener.open().await
    }

    /// A handle that opens streams on this link from any task, as each chat's view does.
    pub fn opener(&self) -> Opener {
        self.opener.clone()
    }

    /// The next stream the other end opened.
    pub async fn accept(&mut self) -> Result<Stream, LinkError> {
        self.inbound.accept().await
    }

    /// Streams the other end opened that nobody has accepted yet, at most
    /// [`MOST_UNACCEPTED_STREAMS`].
    pub fn unaccepted(&self) -> usize {
        self.inbound.streams.len()
    }

    /// Hand the streams the other end opens to a task of their own. After this, [`Link::accept`]
    /// answers [`LinkError::Closed`].
    pub fn acceptor(&mut self) -> Acceptor {
        let (_, none) = mpsc::channel(1);
        let taken = Acceptor {
            streams: none,
            why: Arc::default(),
        };
        std::mem::replace(&mut self.inbound, taken)
    }
}

/// Opens streams on a link from any task. It keeps the link's driver running while it lives.
#[derive(Clone)]
pub struct Opener {
    asks: mpsc::UnboundedSender<Opening>,
    why: Ending,
}

impl Opener {
    /// Open a stream. The other end sees it once this end writes to it.
    pub async fn open(&self) -> Result<Stream, LinkError> {
        let (reply, opened) = oneshot::channel();
        self.asks.send(reply).map_err(|_| ended(&self.why))?;
        Ok(opened.await.map_err(|_| ended(&self.why))??.compat())
    }
}

/// Takes the streams the other end opens, apart from the [`Link`].
pub struct Acceptor {
    streams: mpsc::Receiver<yamux::Stream>,
    why: Ending,
}

impl Acceptor {
    /// The next stream the other end opened.
    pub async fn accept(&mut self) -> Result<Stream, LinkError> {
        match self.streams.recv().await {
            Some(stream) => Ok(stream.compat()),
            None => Err(ended(&self.why)),
        }
    }
}

impl Control {
    /// Send one frame. A frame over [`MOST_CONTROL_FRAME_BYTES`] is refused here, and the lane
    /// stays usable.
    pub async fn send(&mut self, frame: Bytes) -> Result<(), LinkError> {
        if frame.len() > MOST_CONTROL_FRAME_BYTES {
            return Err(LinkError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!(
                    "a control frame of {} bytes, over {MOST_CONTROL_FRAME_BYTES}",
                    frame.len()
                ),
            )));
        }
        self.0.send(frame).await?;
        Ok(())
    }

    /// The next frame, or `None` when the lane is closed.
    pub async fn next(&mut self) -> Option<Result<Bytes, LinkError>> {
        self.0
            .next()
            .await
            .map(|frame| frame.map(|f| f.freeze()).map_err(LinkError::from))
    }
}

/// The client end: negotiate with what `speaks` names, be admitted as `scope` with its
/// `credential`, start the multiplexer and open the control lane, all within one
/// [`version::HANDSHAKE_TIMEOUT`].
pub async fn connect<S: AsyncRead + AsyncWrite + Unpin + Send + 'static>(
    io: S,
    speaks: Speaks,
    scope: Scope,
    credential: &Credential,
) -> Result<Link, LinkError> {
    within_the_handshake(async {
        let (version, io) = version::offer_unbounded(io, &speaks).await?;
        let io = auth::present_unbounded(io, scope, credential).await?;
        let (opener, inbound) = drive(io, yamux::Mode::Client);
        let mut lane = opener.open().await?;
        lane.write_all(&[CONTROL_LANE]).await?;
        lane.flush().await?;
        Ok(Link {
            version,
            scope,
            control: lane_framed(lane),
            opener,
            inbound,
        })
    })
    .await
}

/// The host end, on a connection to `charterd.sock` whose peer is this user
/// ([`crate::local::Listener::accept`]): answer with what `speaks` names, admit the client as
/// the scope whose credential in `held` it proves to hold or refuse it, start the multiplexer
/// and take the control lane the client opens, all within one [`version::HANDSHAKE_TIMEOUT`].
///
/// It takes a [`crate::local::SameUser`] and nothing else, so a connection that skipped the
/// uid check cannot be served. A peer inside one of `chats` is refused whatever it proves, and
/// so is one whose ancestry cannot be read (FD-27, V16a, [`crate::local`]). A runner's link
/// (ADR 0078 §3) is admitted by its Noise identity, not by these credentials, and is served by
/// [`serve_as_a_device`].
#[cfg(unix)]
pub async fn serve(
    io: crate::local::SameUser,
    speaks: Speaks,
    held: &Credentials,
    chats: &Arc<impl crate::local::Chats>,
) -> Result<Link, LinkError> {
    let pid = io.pid();
    let chats = Arc::clone(chats);
    within_the_handshake(async {
        let inside = match crate::local::inside_a_chat(pid, chats).await {
            Ok(None) => None,
            Ok(Some(chat)) => Some(format!(
                "this connection comes from inside a chat (process {pid}, under chat program \
                 {chat}), and a chat never holds a person's scope: ask in the chat, and answer \
                 in charter's window"
            )),
            Err(why) => Some(format!(
                "whether this connection comes from inside a chat could not be read ({why}), so \
                 it is refused as if it did"
            )),
        };
        serve_unbounded(io.into_stream(), speaks, held, inside).await
    })
    .await
}

/// [`serve`] over any ordered byte stream, for the crate's own tests, which drive the host over
/// pipes, a child's stdio and in-process streams. It skips the uid check, so it exists only
/// with the `any-stream` feature, which only this crate's dev-dependencies turn on.
#[cfg(feature = "any-stream")]
pub async fn serve_any<S: AsyncRead + AsyncWrite + Unpin + Send + 'static>(
    io: S,
    speaks: Speaks,
    held: &Credentials,
) -> Result<Link, LinkError> {
    within_the_handshake(serve_unbounded(io, speaks, held, None)).await
}

/// A peer device a link's Noise `XX` handshake proved against the keys pinned at pairing (ADR
/// 0078 §3): what admits a link as [`Scope::RemoteLink`], since no credential file does.
///
/// **Nothing in a build of the host makes one yet.** RR-13 builds that handshake, and its
/// result is the only thing that should. Until then `remote-link` is admitted nowhere: it fails
/// closed. The crate's own tests stand in for the handshake with `ProvenDevice::stand_in`,
/// which exists only with the `any-stream` feature.
#[derive(Debug)]
pub struct ProvenDevice(());

#[cfg(feature = "any-stream")]
impl ProvenDevice {
    /// A device taken as proved, for the crate's tests only.
    pub fn stand_in() -> ProvenDevice {
        ProvenDevice(())
    }
}

/// The host end of a link from a peer device that `device` proved: answer with what `speaks`
/// names, start the multiplexer and take the control lane, all within one
/// [`version::HANDSHAKE_TIMEOUT`]. The link is [`Scope::RemoteLink`], whatever the device says.
pub async fn serve_as_a_device<S: AsyncRead + AsyncWrite + Unpin + Send + 'static>(
    io: S,
    speaks: Speaks,
    device: ProvenDevice,
) -> Result<Link, LinkError> {
    let ProvenDevice(()) = device;
    within_the_handshake(async {
        let (version, io) = version::answer_unbounded(io, &speaks).await?;
        take_the_lane(version, Scope::RemoteLink, io).await
    })
    .await
}

/// The device's end of [`serve_as_a_device`], for the crate's tests: negotiate and open the
/// control lane, with no credential, since the stream stands for one a handshake has already
/// proved.
#[cfg(feature = "any-stream")]
pub async fn connect_as_a_device<S: AsyncRead + AsyncWrite + Unpin + Send + 'static>(
    io: S,
    speaks: Speaks,
) -> Result<Link, LinkError> {
    within_the_handshake(async {
        let (version, io) = version::offer_unbounded(io, &speaks).await?;
        let (opener, inbound) = drive(io, yamux::Mode::Client);
        let mut lane = opener.open().await?;
        lane.write_all(&[CONTROL_LANE]).await?;
        lane.flush().await?;
        Ok(Link {
            version,
            scope: Scope::RemoteLink,
            control: lane_framed(lane),
            opener,
            inbound,
        })
    })
    .await
}

/// The host's last step of a handshake: start the multiplexer and take the control lane the
/// client opens, for a link admitted as `scope`.
async fn take_the_lane<S: AsyncRead + AsyncWrite + Unpin + Send + 'static>(
    version: Version,
    scope: Scope,
    io: S,
) -> Result<Link, LinkError> {
    let (opener, mut inbound) = drive(io, yamux::Mode::Server);
    let mut lane = inbound.accept().await?;
    if lane.read_u8().await? != CONTROL_LANE {
        return Err(LinkError::NoControlLane);
    }
    Ok(Link {
        version,
        scope,
        control: lane_framed(lane),
        opener,
        inbound,
    })
}

async fn serve_unbounded<S: AsyncRead + AsyncWrite + Unpin + Send + 'static>(
    io: S,
    speaks: Speaks,
    held: &Credentials,
    inside_a_chat: Option<String>,
) -> Result<Link, LinkError> {
    let (version, io) = version::answer_unbounded(io, &speaks).await?;
    let (scope, io) = auth::admit_unbounded(io, held, inside_a_chat).await?;
    take_the_lane(version, scope, io).await
}

/// One deadline over the whole handshake, so a peer that takes nearly the whole of it at each
/// step cannot hold the other end for several of them.
async fn within_the_handshake(
    handshake: impl Future<Output = Result<Link, LinkError>>,
) -> Result<Link, LinkError> {
    tokio::time::timeout(version::HANDSHAKE_TIMEOUT, handshake)
        .await
        .unwrap_or(Err(LinkError::TimedOut))
}

fn lane_framed(lane: Stream) -> Control {
    Control(
        LengthDelimitedCodec::builder()
            .max_frame_length(MOST_CONTROL_FRAME_BYTES)
            .new_framed(lane),
    )
}

/// Start the task that drives the connection: it opens what this end asks for, hands over
/// what the other end opens, and moves every byte. It ends when the link is dropped or the
/// transport closes.
fn drive<S: AsyncRead + AsyncWrite + Unpin + Send + 'static>(
    io: S,
    mode: yamux::Mode,
) -> (Opener, Acceptor) {
    // The multiplexer writes through the control lane's scheduler, and reads the transport
    // as it is (`crate::priority`).
    let (reads, writes) = tokio::io::split(io);
    let io = tokio::io::join(reads, Prioritized::start(writes));
    let mut connection = yamux::Connection::new(io.compat(), config(), mode);
    let (asks, mut opens) = mpsc::unbounded_channel::<Opening>();
    let (hand_over, streams) = mpsc::channel(MOST_UNACCEPTED_STREAMS);
    let why: Ending = Arc::default();
    let record = Arc::clone(&why);
    tokio::spawn(async move {
        let mut waiting: VecDeque<Opening> = VecDeque::new();
        let mut asks_closed = false;
        let ended: yamux::Result<()> = poll_fn(|cx| {
            loop {
                let mut moved = false;
                if !asks_closed {
                    match opens.poll_recv(cx) {
                        Poll::Ready(Some(ask)) => {
                            waiting.push_back(ask);
                            moved = true;
                        }
                        Poll::Ready(None) => asks_closed = true,
                        Poll::Pending => {}
                    }
                }
                while !waiting.is_empty() {
                    match connection.poll_new_outbound(cx) {
                        Poll::Ready(stream) => {
                            if let Some(ask) = waiting.pop_front() {
                                let _ = ask.send(stream);
                            }
                            moved = true;
                        }
                        Poll::Pending => break,
                    }
                }
                match connection.poll_next_inbound(cx) {
                    Poll::Ready(Some(Ok(stream))) => {
                        // Full: nobody is taking streams as fast as they come, and this one is
                        // refused. Dropping it resets it for the end that opened it.
                        let _ = hand_over.try_send(stream);
                        moved = true;
                    }
                    Poll::Ready(Some(Err(e))) => return Poll::Ready(Err(e)),
                    Poll::Ready(None) => return Poll::Ready(Ok(())),
                    Poll::Pending => {}
                }
                if asks_closed && hand_over.is_closed() {
                    return Poll::Ready(Ok(()));
                }
                if !moved {
                    return Poll::Pending;
                }
            }
        })
        .await;
        // Kept before the channels close, so whoever finds them closed can say why.
        let _ = record.set(match ended {
            Ok(()) => "the transport closed".to_owned(),
            Err(e) => e.to_string(),
        });
        drop(hand_over);
        drop(opens);
    });
    (
        Opener {
            asks,
            why: Arc::clone(&why),
        },
        Acceptor { streams, why },
    )
}

/// Streams one link carries at once, open by either end. A host serves about 200 chats per
/// machine (Q1), each with one view per client, and the rest is room.
pub const MOST_STREAMS: usize = 256;

/// The multiplexer's limits, set rather than left to its defaults (512 streams and a 1 GiB
/// receive window across them). Each stream gets Yamux's own initial window of 256 KiB, and the
/// whole link's receive windows together are capped at that times [`MOST_STREAMS`], 64 MiB:
/// the most a peer can make this end buffer, however it opens and writes.
fn config() -> yamux::Config {
    let mut config = yamux::Config::default();
    config.set_max_num_streams(MOST_STREAMS);
    config.set_max_connection_receive_window(Some(MOST_STREAMS * yamux::DEFAULT_CREDIT as usize));
    config
}
