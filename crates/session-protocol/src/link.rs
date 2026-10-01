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
//! unchanged. Yamux has no stream priorities; how the control lane still gets through a busy
//! terminal is in [`crate::view`]: every terminal's bytes in flight are bounded, so the lane
//! waits behind a bounded amount, never an unbounded one.
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
use std::task::Poll;

use bytes::Bytes;
use futures::{SinkExt, StreamExt};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::{mpsc, oneshot};
use tokio_util::codec::{Framed, LengthDelimitedCodec};
use tokio_util::compat::{FuturesAsyncReadCompatExt, TokioAsyncReadCompatExt};

use crate::version::{self, Refused, Speaks, Version};

/// The largest frame the control lane carries. A command or an event is small; a frame this
/// size is a peer that is wrong, and it is refused rather than buffered.
pub const MOST_CONTROL_FRAME_BYTES: usize = 1024 * 1024;

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
    /// The client did not open the control lane within the handshake's deadline.
    #[error("the client did not open the control lane within {:?}", crate::version::HANDSHAKE_TIMEOUT)]
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
    control: Control,
    open: mpsc::UnboundedSender<Opening>,
    inbound: mpsc::UnboundedReceiver<yamux::Stream>,
}

/// The control lane: frames, whole and in order, both ways.
pub struct Control(Framed<Stream, LengthDelimitedCodec>);

impl Link {
    /// The version both ends agreed on.
    pub fn version(&self) -> Version {
        self.version
    }

    pub fn control(&mut self) -> &mut Control {
        &mut self.control
    }

    /// Open a stream. The other end sees it once this end writes to it.
    pub async fn open(&mut self) -> Result<Stream, LinkError> {
        open_on(&self.open).await
    }

    /// A handle that opens streams on this link from any task, as each chat's view does.
    pub fn opener(&self) -> Opener {
        Opener(self.open.clone())
    }

    /// The next stream the other end opened.
    pub async fn accept(&mut self) -> Result<Stream, LinkError> {
        self.inbound
            .recv()
            .await
            .map(|s| s.compat())
            .ok_or(LinkError::Closed)
    }

    /// Hand the streams the other end opens to a task of their own. After this, [`Link::accept`]
    /// answers [`LinkError::Closed`].
    pub fn acceptor(&mut self) -> Acceptor {
        let (_, closed) = mpsc::unbounded_channel();
        Acceptor(std::mem::replace(&mut self.inbound, closed))
    }
}

/// Opens streams on a link from any task. It keeps the link's driver running while it lives.
#[derive(Clone)]
pub struct Opener(mpsc::UnboundedSender<Opening>);

impl Opener {
    /// Open a stream. The other end sees it once this end writes to it.
    pub async fn open(&self) -> Result<Stream, LinkError> {
        open_on(&self.0).await
    }
}

/// Takes the streams the other end opens, apart from the [`Link`].
pub struct Acceptor(mpsc::UnboundedReceiver<yamux::Stream>);

impl Acceptor {
    /// The next stream the other end opened.
    pub async fn accept(&mut self) -> Result<Stream, LinkError> {
        self.0
            .recv()
            .await
            .map(|s| s.compat())
            .ok_or(LinkError::Closed)
    }
}

async fn open_on(open: &mpsc::UnboundedSender<Opening>) -> Result<Stream, LinkError> {
    let (reply, opened) = oneshot::channel();
    open.send(reply).map_err(|_| LinkError::Closed)?;
    Ok(opened.await.map_err(|_| LinkError::Closed)??.compat())
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

/// The client end: negotiate with what `speaks` names, start the multiplexer and open the
/// control lane.
pub async fn connect<S: AsyncRead + AsyncWrite + Unpin + Send + 'static>(
    mut io: S,
    speaks: Speaks,
) -> Result<Link, LinkError> {
    let version = version::offer(&mut io, &speaks).await?;
    let (open, inbound) = drive(io, yamux::Mode::Client);
    let (reply, opened) = oneshot::channel();
    open.send(reply).map_err(|_| LinkError::Closed)?;
    let mut lane = opened.await.map_err(|_| LinkError::Closed)??.compat();
    lane.write_all(&[CONTROL_LANE]).await?;
    lane.flush().await?;
    Ok(Link {
        version,
        control: lane_framed(lane),
        open,
        inbound,
    })
}

/// The host end: answer with what `speaks` names, start the multiplexer and take the control
/// lane the client opens.
pub async fn serve<S: AsyncRead + AsyncWrite + Unpin + Send + 'static>(
    mut io: S,
    speaks: Speaks,
) -> Result<Link, LinkError> {
    let version = version::answer(&mut io, &speaks).await?;
    let (open, mut inbound) = drive(io, yamux::Mode::Server);
    let lane = tokio::time::timeout(version::HANDSHAKE_TIMEOUT, async {
        let mut lane = inbound.recv().await.ok_or(LinkError::Closed)?.compat();
        if lane.read_u8().await? != CONTROL_LANE {
            return Err(LinkError::NoControlLane);
        }
        Ok(lane)
    })
    .await
    .unwrap_or(Err(LinkError::TimedOut))?;
    Ok(Link {
        version,
        control: lane_framed(lane),
        open,
        inbound,
    })
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
) -> (
    mpsc::UnboundedSender<Opening>,
    mpsc::UnboundedReceiver<yamux::Stream>,
) {
    let mut connection = yamux::Connection::new(io.compat(), config(), mode);
    let (open, mut opens) = mpsc::unbounded_channel::<Opening>();
    let (hand_over, inbound) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        let mut waiting: VecDeque<Opening> = VecDeque::new();
        let mut asks_closed = false;
        let _ended = poll_fn(|cx| {
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
                        let _ = hand_over.send(stream);
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
    });
    (open, inbound)
}

fn config() -> yamux::Config {
    yamux::Config::default()
}
