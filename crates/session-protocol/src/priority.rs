//! The control lane's priority: a scheduler between the multiplexer and the transport that
//! sends the control lane's frames ahead of every terminal byte still waiting (ADR 0068 §4,
//! amended again by FD-4).
//!
//! **Why it is needed.** Yamux has no stream priorities. It takes one frame from each stream
//! with something to send, in turn, and writes one frame at a time. So a control frame waits
//! behind a frame of every busy stream: fifty terminals' 16 KiB frames on a 1 MB/s link, a
//! connector's stdio to a remote host, is about 800 ms. What the transport already holds it
//! cannot jump, and that is the one wait left.
//!
//! **How.** The multiplexer writes into this end instead of the transport. It takes the
//! bytes as they come, up to its bounds, cuts the bytes into Yamux's frames (a 12-byte header: version, type, flags,
//! stream, length; then `length` bytes of body for a data frame, none for the others), and
//! queues each frame. A task writes the frames to the transport, the control lane's first.
//!
//! **Nothing on the wire changes.** Every frame is the multiplexer's own, whole, and the frames
//! of one stream keep their order; only frames of different streams are reordered, which Yamux
//! allows, since each stream has its own window and its own sequence. So no version is bumped:
//! a peer that reads Yamux reads this, and a peer that does not schedule reads the same.
//!
//! **Which frames go first.** The control lane's: the stream the client opens first, which
//! Yamux numbers 1 ([`CONTROL_LANE_STREAM`]), and which the host checks is stream 1 before it
//! takes it as the lane. With them go the session's pings and pongs on stream 0, as Yamux
//! itself puts a pong ahead of the streams, so a round trip Yamux measures is not this queue's.
//! Every other stream takes its turn, a frame each, and a go-away goes last.
//!
//! **Bounded, and what that costs.** Two bounds: [`MOST_QUEUED_BYTES`] for the streams'
//! frames, and [`MOST_LANE_BYTES`] for the lane's, pings and pongs included. Each frame counts
//! at least 64 bytes against its bound, what a queued frame holds of the heap, so a flood of
//! 12-byte pongs is bounded in memory and not only in bytes on the wire. Past either one,
//! this end takes no next frame of any kind, a header-only frame included, until the writing
//! task has made room: a queue passes its bound by at most the one frame already begun. That
//! is the transport's back-pressure, passed on: Yamux stops writing, and since it has one
//! frame on its way at a time, it stops reading too once a pong or a window update waits. So
//! a peer that grants credit and never reads, or floods pings, cannot grow either queue. And
//! past a bound **everything waits, the control lane too**: Yamux cannot hand over the lane's
//! frame while another is stuck. In use the bounds are not reached: a data frame is sent only
//! with the other end's credit for its stream, and each view's bytes are held to its
//! watermark, so what waits here is what the views have already been allowed in flight.

use std::collections::{HashMap, VecDeque};
use std::io;
use std::pin::Pin;
use std::sync::{Arc, Mutex, MutexGuard};
use std::task::{Context, Poll, Waker};

use bytes::{Bytes, BytesMut};
use tokio::io::{AsyncWrite, AsyncWriteExt};
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

/// The Yamux stream the control lane is: the client's first, and Yamux numbers a client's
/// streams from 1.
pub(crate) const CONTROL_LANE_STREAM: u32 = 1;

/// Bytes of frames other than the control lane's, pings and pongs that may wait here: every
/// stream a link may carry, each a view's high watermark (64 KiB) ahead. Past it, the
/// multiplexer waits, whatever it writes next.
pub(crate) const MOST_QUEUED_BYTES: usize = crate::link::MOST_STREAMS * 64 * 1024;

/// How long a dropped end keeps writing what it had queued, as a closed socket lingers: what
/// the multiplexer was handed before the link was dropped still reaches a peer that reads it,
/// and a peer that does not read holds the transport no longer than this.
pub(crate) const LINGER: std::time::Duration = std::time::Duration::from_secs(10);

/// A Yamux frame's header.
const HEADER: usize = 12;

/// The type of a Yamux data frame, the only one with a body.
const DATA: u8 = 0;

/// The type of a Yamux ping or pong, on the session's stream 0.
const PING: u8 = 2;

/// The flag that makes a ping a pong.
#[cfg(test)]
const ACK: u16 = 2;

/// Bytes of the control lane's frames, pings and pongs that may wait here: two of its largest
/// frames. Past it, the multiplexer waits, whatever it writes next.
pub(crate) const MOST_LANE_BYTES: usize = 2 * crate::link::MOST_CONTROL_FRAME_BYTES;

/// What one queued frame is counted as, at least: its `Bytes`, its allocation and its slot in a
/// queue hold this much of the heap whatever its length.
const LEAST_FRAME_COST: usize = 64;

/// What a queued frame is counted as against its bound.
fn cost(frame: &Bytes) -> usize {
    frame.len().max(LEAST_FRAME_COST)
}

/// The type of a Yamux go-away frame, which ends the session.
const GO_AWAY: u8 = 3;

#[derive(Default)]
struct Queues {
    /// The control lane's frames, sent first.
    urgent: VecDeque<Bytes>,
    /// Every other frame, by stream, each stream's in the order the multiplexer wrote them.
    bulk: HashMap<u32, VecDeque<Bytes>>,
    /// The streams in `bulk`, in the order they take their turns: one frame each.
    turns: VecDeque<u32>,
    /// A frame that ends the session (Yamux's go-away), sent once nothing else is queued.
    last: VecDeque<Bytes>,
    /// What `bulk` and `last` hold, each frame counted at [`cost`].
    bulk_bytes: usize,
    /// What `urgent` holds, each frame counted at [`cost`].
    lane_bytes: usize,
    /// The multiplexer waiting for `bulk` to have room.
    room: Option<Waker>,
    /// The multiplexer shut its end: write what is queued, then shut the transport.
    closing: bool,
    /// The transport is shut, or failed, and why.
    done: Option<Result<(), io::ErrorKind>>,
    /// The multiplexer waiting for `done`.
    shut: Option<Waker>,
}

impl Queues {
    /// Past either bound: the multiplexer waits.
    fn full(&self) -> bool {
        self.bulk_bytes >= MOST_QUEUED_BYTES || self.lane_bytes >= MOST_LANE_BYTES
    }

    /// The next frame of a stream other than the control lane: one from the stream whose turn
    /// it is, which then goes to the back if it has more; a go-away only once nothing else is
    /// left.
    fn next_turn(&mut self) -> Option<Bytes> {
        let frame = match self.turns.pop_front() {
            Some(stream) => {
                let waiting = self.bulk.get_mut(&stream)?;
                let frame = waiting.pop_front()?;
                if waiting.is_empty() {
                    self.bulk.remove(&stream);
                } else {
                    self.turns.push_back(stream);
                }
                frame
            }
            None => self.last.pop_front()?,
        };
        self.bulk_bytes -= cost(&frame);
        Some(frame)
    }
}

struct Shared {
    queues: Mutex<Queues>,
    more: Notify,
}

impl Shared {
    fn queues(&self) -> MutexGuard<'_, Queues> {
        self.queues
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// The multiplexer's end: what it writes to as it would to the transport.
pub(crate) struct Prioritized {
    shared: Arc<Shared>,
    /// The frame being cut, header first.
    frame: BytesMut,
    /// The whole frame's length, once its header is in.
    length: Option<usize>,
    /// The stream of the frame being cut, and its type, once its header is in.
    stream: u32,
    kind: u8,
    /// Cancelled when this end is dropped: the task writes what is queued and shuts the
    /// transport, for at most [`LINGER`].
    stop: CancellationToken,
}

impl Prioritized {
    /// Write the frames this end is given to `out`, the control lane's first, from a task of
    /// its own. Once this end is dropped, the task writes what is queued and shuts `out`, or
    /// gives up after [`LINGER`].
    pub(crate) fn start<W: AsyncWrite + Unpin + Send + 'static>(out: W) -> Prioritized {
        let shared = Arc::new(Shared {
            queues: Mutex::new(Queues::default()),
            more: Notify::new(),
        });
        let stop = CancellationToken::new();
        let writing = Arc::clone(&shared);
        let stopped = stop.clone();
        tokio::spawn(async move {
            tokio::select! {
                () = write_out(out, &writing) => {}
                () = async {
                    stopped.cancelled().await;
                    tokio::time::sleep(LINGER).await;
                } => {}
            }
        });
        Prioritized {
            shared,
            frame: BytesMut::with_capacity(HEADER),
            length: None,
            stream: 0,
            kind: DATA,
            stop,
        }
    }

    fn failed(queues: &Queues) -> Option<io::Error> {
        match queues.done {
            Some(Err(kind)) => Some(kind.into()),
            Some(Ok(())) => Some(io::ErrorKind::BrokenPipe.into()),
            None => None,
        }
    }
}

impl Drop for Prioritized {
    fn drop(&mut self) {
        self.shared.queues().closing = true;
        self.shared.more.notify_one();
        self.stop.cancel();
    }
}

impl AsyncWrite for Prioritized {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = &mut *self;
        if let Some(e) = Self::failed(&this.shared.queues()) {
            return Poll::Ready(Err(e));
        }
        let mut taken = 0;
        while taken < buf.len() {
            let Some(length) = this.length else {
                // Before a frame's first byte, the bound: past it, nothing more is taken, of any
                // stream, until the writing task makes room. Once begun, a frame is taken whole,
                // so a queue passes its bound by at most one frame.
                if this.frame.is_empty() {
                    let mut queues = this.shared.queues();
                    if queues.full() {
                        if taken > 0 {
                            break;
                        }
                        queues.room = Some(cx.waker().clone());
                        return Poll::Pending;
                    }
                }
                let n = (HEADER - this.frame.len()).min(buf.len() - taken);
                this.frame.extend_from_slice(&buf[taken..taken + n]);
                taken += n;
                if this.frame.len() == HEADER {
                    let body = if this.frame[1] == DATA {
                        u32::from_be_bytes([
                            this.frame[8],
                            this.frame[9],
                            this.frame[10],
                            this.frame[11],
                        ]) as usize
                    } else {
                        0
                    };
                    let stream = u32::from_be_bytes([
                        this.frame[4],
                        this.frame[5],
                        this.frame[6],
                        this.frame[7],
                    ]);
                    this.stream = stream;
                    this.kind = this.frame[1];
                    this.length = Some(HEADER + body);
                    this.frame.reserve(body);
                    this.queue_if_whole();
                }
                continue;
            };
            let n = (length - this.frame.len()).min(buf.len() - taken);
            this.frame.extend_from_slice(&buf[taken..taken + n]);
            taken += n;
            this.queue_if_whole();
        }
        Poll::Ready(Ok(taken))
    }

    /// Every byte written is queued, and the task writes the queue on, flushing whenever it
    /// runs dry: there is nothing more to flush here.
    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.shared.queues().done {
            Some(Err(kind)) => Poll::Ready(Err(kind.into())),
            _ => Poll::Ready(Ok(())),
        }
    }

    /// Write what is queued, then shut the transport.
    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let mut queues = self.shared.queues();
        match queues.done {
            Some(Ok(())) => Poll::Ready(Ok(())),
            Some(Err(kind)) => Poll::Ready(Err(kind.into())),
            None => {
                queues.closing = true;
                queues.shut = Some(cx.waker().clone());
                drop(queues);
                self.shared.more.notify_one();
                Poll::Pending
            }
        }
    }
}

impl Prioritized {
    /// Queue the frame being cut once it is whole.
    fn queue_if_whole(&mut self) {
        if self.length != Some(self.frame.len()) {
            return;
        }
        let frame = std::mem::replace(&mut self.frame, BytesMut::with_capacity(HEADER)).freeze();
        self.length = None;
        let mut guard = self.shared.queues();
        let queues = &mut *guard;
        if self.stream == CONTROL_LANE_STREAM || (self.stream == 0 && self.kind == PING) {
            queues.lane_bytes += cost(&frame);
            queues.urgent.push_back(frame);
        } else {
            queues.bulk_bytes += cost(&frame);
            if self.kind == GO_AWAY {
                queues.last.push_back(frame);
            } else {
                let waiting = queues.bulk.entry(self.stream).or_default();
                if waiting.is_empty() {
                    queues.turns.push_back(self.stream);
                }
                waiting.push_back(frame);
            }
        }
        drop(guard);
        self.shared.more.notify_one();
    }
}

/// The writing task: the control lane's frames first, then the rest in order; a flush whenever
/// the queue runs dry and after each control frame; the transport shut when the multiplexer
/// shuts its end.
async fn write_out<W: AsyncWrite + Unpin>(mut out: W, shared: &Shared) {
    enum Next {
        Lane(Bytes),
        Bulk(Bytes),
        Shut,
        Dry,
    }
    let ended = loop {
        let next = {
            let mut queues = shared.queues();
            let next = if let Some(frame) = queues.urgent.pop_front() {
                queues.lane_bytes -= cost(&frame);
                Some(Next::Lane(frame))
            } else {
                queues.next_turn().map(Next::Bulk)
            };
            if next.is_some()
                && !queues.full()
                && let Some(waker) = queues.room.take()
            {
                waker.wake();
            }
            if let Some(next) = next {
                next
            } else if queues.closing {
                Next::Shut
            } else {
                Next::Dry
            }
        };
        let wrote = match next {
            Next::Lane(frame) => match out.write_all(&frame).await {
                Ok(()) => out.flush().await,
                Err(e) => Err(e),
            },
            Next::Bulk(frame) => out.write_all(&frame).await,
            Next::Shut => break out.shutdown().await,
            Next::Dry => match out.flush().await {
                Ok(()) => {
                    shared.more.notified().await;
                    Ok(())
                }
                Err(e) => Err(e),
            },
        };
        if let Err(e) = wrote {
            break Err(e);
        }
    };
    let mut queues = shared.queues();
    queues.done = Some(ended.map_err(|e| e.kind()));
    for waker in [queues.room.take(), queues.shut.take()]
        .into_iter()
        .flatten()
    {
        waker.wake();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncReadExt;

    fn frame(stream: u32, body: &[u8]) -> Vec<u8> {
        let mut f = vec![0, DATA, 0, 0];
        f.extend_from_slice(&stream.to_be_bytes());
        f.extend_from_slice(&(body.len() as u32).to_be_bytes());
        f.extend_from_slice(body);
        f
    }

    fn window_update(stream: u32, credit: u32) -> Vec<u8> {
        let mut f = vec![0, 1, 0, 0];
        f.extend_from_slice(&stream.to_be_bytes());
        f.extend_from_slice(&credit.to_be_bytes());
        f
    }

    /// Frames written while the transport is not read come out with the control lane's first,
    /// whole, and every stream's own in its order; a header-only frame has no body.
    #[tokio::test]
    async fn the_control_lanes_frames_go_out_first_and_every_frame_whole() {
        let (near, mut far) = tokio::io::duplex(16);
        let mut ours = Prioritized::start(near);
        let bulk = [
            frame(3, &[3; 40]),
            window_update(5, 1000),
            frame(5, &[5; 40]),
            frame(3, &[4; 40]),
        ];
        // The first bulk frame is already on its way when the rest are queued.
        let mut written = Vec::new();
        for f in &bulk {
            written.extend_from_slice(f);
        }
        let lane = [frame(1, b"needs-you 1"), frame(1, b"needs-you 2")];
        // Byte by byte, as the multiplexer may hand them over.
        for b in &written {
            ours.write_all(std::slice::from_ref(b)).await.unwrap();
        }
        tokio::task::yield_now().await;
        for f in &lane {
            ours.write_all(f).await.unwrap();
        }
        let total = written.len() + lane.iter().map(Vec::len).sum::<usize>();
        let mut seen = vec![0u8; total];
        far.read_exact(&mut seen).await.unwrap();

        let mut cut = Vec::new();
        let mut at = 0;
        while at < seen.len() {
            let body = if seen[at + 1] == DATA {
                u32::from_be_bytes(seen[at + 8..at + 12].try_into().unwrap()) as usize
            } else {
                0
            };
            cut.push(seen[at..at + HEADER + body].to_vec());
            at += HEADER + body;
        }
        let first_lane = cut.iter().position(|f| *f == lane[0]).unwrap();
        let second_lane = cut.iter().position(|f| *f == lane[1]).unwrap();
        assert!(first_lane < second_lane, "the lane's own order is kept");
        assert!(
            first_lane <= 1,
            "the lane's frame waited behind more than the frame being written: {first_lane}"
        );
        let rest: Vec<_> = cut.into_iter().filter(|f| !lane.contains(f)).collect();
        for stream in [3u8, 5] {
            let of = |frames: &[Vec<u8>]| -> Vec<Vec<u8>> {
                frames.iter().filter(|f| f[7] == stream).cloned().collect()
            };
            assert_eq!(of(&rest), of(&bulk), "each stream keeps its order");
        }
    }

    /// An end whose transport is never read, with [`MOST_QUEUED_BYTES`] of a terminal's
    /// frames queued.
    async fn full() -> (Prioritized, tokio::io::DuplexStream) {
        let (near, far) = tokio::io::duplex(16);
        let mut ours = Prioritized::start(near);
        let body = vec![9u8; 16 * 1024];
        // The first frame is taken off the queue, to be written to a transport nobody reads.
        ours.write_all(&frame(3, &body)).await.unwrap();
        tokio::task::yield_now().await;
        let mut queued = 0;
        while queued < MOST_QUEUED_BYTES {
            ours.write_all(&frame(3, &body)).await.unwrap();
            queued += HEADER + body.len();
        }
        (ours, far)
    }

    /// Past the bound, a terminal's frame waits for room.
    #[tokio::test]
    async fn past_the_bound_a_terminals_frame_waits() {
        let (mut ours, _far) = full().await;
        let waited = tokio::time::timeout(
            std::time::Duration::from_millis(50),
            ours.write_all(&frame(5, &[5; 1024])),
        )
        .await;
        assert!(
            waited.is_err(),
            "a terminal's frame was taken past the bound"
        );
    }

    /// Past the bound everything waits, the control lane's frame too: the multiplexer has one
    /// frame on its way at a time, so a frame that waits holds back every frame behind it.
    #[tokio::test]
    async fn past_the_bound_everything_waits_the_control_lane_too() {
        let (mut ours, _far) = full().await;
        let waited = tokio::time::timeout(
            std::time::Duration::from_millis(50),
            ours.write_all(&frame(CONTROL_LANE_STREAM, b"needs-you")),
        )
        .await;
        assert!(waited.is_err(), "the lane's frame was taken past the bound");
    }

    fn ping(flags: u16) -> Vec<u8> {
        let mut f = vec![0, PING];
        f.extend_from_slice(&flags.to_be_bytes());
        f.extend_from_slice(&0u32.to_be_bytes());
        f.extend_from_slice(&7u32.to_be_bytes());
        f
    }

    /// Writes `frames` to an end whose transport is never read, and says whether each was taken
    /// before a write waited.
    async fn taken_before_waiting(
        ours: &mut Prioritized,
        frames: impl Iterator<Item = Vec<u8>>,
    ) -> bool {
        for f in frames {
            let took =
                tokio::time::timeout(std::time::Duration::from_millis(50), ours.write_all(&f))
                    .await;
            if took.is_err() {
                return false;
            }
        }
        true
    }

    /// The control lane has a bound of its own: a peer that grants the lane credit and never
    /// reads cannot make it queue without limit.
    #[tokio::test]
    async fn the_control_lane_has_a_bound_of_its_own() {
        let (near, _far) = tokio::io::duplex(16);
        let mut ours = Prioritized::start(near);
        let body = vec![1u8; 16 * 1024];
        let flood =
            (0..(2 * MOST_LANE_BYTES / body.len())).map(|_| frame(CONTROL_LANE_STREAM, &body));
        assert!(
            !taken_before_waiting(&mut ours, flood).await,
            "twice the lane's bound was queued for a transport nobody reads"
        );
    }

    /// A frame with no body is held to the bound too: pongs, window updates.
    #[tokio::test]
    async fn frames_with_no_body_are_held_to_the_bound() {
        let (mut ours, _far) = full().await;
        let updates = (0..1000).map(|_| window_update(5, 1000));
        assert!(
            !taken_before_waiting(&mut ours, updates).await,
            "window updates were taken past the bound"
        );
        let (near, _far) = tokio::io::duplex(16);
        let mut ours = Prioritized::start(near);
        let pongs = (0..(2 * MOST_LANE_BYTES / HEADER)).map(|_| ping(ACK));
        assert!(
            !taken_before_waiting(&mut ours, pongs).await,
            "pongs were queued past the lane's bound"
        );
    }

    /// Pings and pongs go with the lane, so a round trip Yamux measures is not this queue's.
    #[tokio::test]
    async fn pings_and_pongs_go_with_the_control_lane() {
        let (near, mut far) = tokio::io::duplex(16);
        let mut ours = Prioritized::start(near);
        let busy: Vec<_> = (0..6u8).map(|n| frame(3, &[n; 40])).collect();
        for f in &busy {
            ours.write_all(f).await.unwrap();
        }
        tokio::task::yield_now().await;
        let pong = ping(ACK);
        ours.write_all(&pong).await.unwrap();
        let total = busy.iter().map(Vec::len).sum::<usize>() + pong.len();
        let mut seen = vec![0u8; total];
        far.read_exact(&mut seen).await.unwrap();
        let at = cut(&seen).iter().position(|f| *f == pong).unwrap();
        assert!(at <= 1, "the pong waited behind {at} frames");
    }

    /// Shutting the multiplexer's end writes what is queued, then shuts the transport.
    #[tokio::test]
    async fn a_shutdown_writes_what_is_queued_then_ends_the_transport() {
        let (near, mut far) = tokio::io::duplex(16);
        let mut ours = Prioritized::start(near);
        let sent = [frame(3, &[3; 100]), frame(CONTROL_LANE_STREAM, b"bye")].concat();
        ours.write_all(&sent).await.unwrap();
        let (shut, seen) = tokio::join!(ours.shutdown(), async {
            let mut seen = Vec::new();
            far.read_to_end(&mut seen).await.unwrap();
            seen
        });
        shut.unwrap();
        assert_eq!(seen.len(), sent.len());
    }

    /// Once the transport fails, the multiplexer hears it on its next write.
    #[tokio::test]
    async fn a_failed_transport_fails_the_next_write() {
        let (near, far) = tokio::io::duplex(16);
        let mut ours = Prioritized::start(near);
        drop(far);
        ours.write_all(&frame(3, &[3; 100])).await.unwrap();
        tokio::task::yield_now().await;
        let failed = ours.write_all(&frame(3, &[3; 100])).await;
        assert!(
            failed.is_err(),
            "a write after the transport failed was taken"
        );
    }

    /// Cut a byte stream back into frames.
    fn cut(seen: &[u8]) -> Vec<Vec<u8>> {
        let mut cut = Vec::new();
        let mut at = 0;
        while at < seen.len() {
            let body = if seen[at + 1] == DATA {
                u32::from_be_bytes(seen[at + 8..at + 12].try_into().unwrap()) as usize
            } else {
                0
            };
            cut.push(seen[at..at + HEADER + body].to_vec());
            at += HEADER + body;
        }
        cut
    }

    /// The other streams take turns, a frame each, as the multiplexer's own streams do: a
    /// stream with one frame to send waits behind one frame of each busy stream, not behind
    /// everything a busy stream has queued.
    #[tokio::test]
    async fn the_other_streams_take_turns() {
        let (near, mut far) = tokio::io::duplex(16);
        let mut ours = Prioritized::start(near);
        let busy: Vec<_> = (0..6u8).map(|n| frame(3, &[n; 40])).collect();
        for f in &busy {
            ours.write_all(f).await.unwrap();
        }
        let echo = frame(5, b"typed");
        ours.write_all(&echo).await.unwrap();
        let total = busy.iter().map(Vec::len).sum::<usize>() + echo.len();
        let mut seen = vec![0u8; total];
        far.read_exact(&mut seen).await.unwrap();
        let cut = cut(&seen);
        let at = cut.iter().position(|f| *f == echo).unwrap();
        assert!(
            at <= 1,
            "the echo waited behind {at} frames of a busy stream"
        );
        let rest: Vec<_> = cut.into_iter().filter(|f| *f != echo).collect();
        assert_eq!(rest, busy, "the busy stream keeps its order");
    }

    /// A dropped end still writes what it was handed, then ends the transport.
    #[tokio::test]
    async fn a_dropped_end_writes_what_it_was_handed() {
        let (near, mut far) = tokio::io::duplex(16);
        let mut ours = Prioritized::start(near);
        let sent = [frame(3, &[3; 100]), frame(CONTROL_LANE_STREAM, b"last")].concat();
        ours.write_all(&sent).await.unwrap();
        drop(ours);
        let mut seen = Vec::new();
        far.read_to_end(&mut seen).await.unwrap();
        let (mut seen, mut sent) = (cut(&seen), cut(&sent));
        seen.sort();
        sent.sort();
        assert_eq!(seen, sent);
    }

    /// A dropped end whose transport is never read gives the transport up after [`LINGER`].
    #[tokio::test(start_paused = true)]
    async fn a_dropped_end_lets_go_of_a_transport_nobody_reads() {
        let (near, mut far) = tokio::io::duplex(16);
        let mut ours = Prioritized::start(near);
        ours.write_all(&frame(3, &[3; 1000])).await.unwrap();
        drop(ours);
        tokio::time::sleep(LINGER + std::time::Duration::from_millis(1)).await;
        let mut seen = Vec::new();
        far.read_to_end(&mut seen).await.unwrap();
        assert!(
            seen.len() < 1000,
            "the whole frame went through a pipe nobody read"
        );
    }

    /// A small frame is counted at what it holds of the heap, not its bytes on the wire, so a
    /// flood of 12-byte pongs fills the lane's bound in as many frames as its memory allows.
    #[tokio::test]
    async fn a_small_frame_counts_at_what_it_holds_of_the_heap() {
        let (near, _far) = tokio::io::duplex(16);
        let mut ours = Prioritized::start(near);
        let pong = ping(ACK);
        let mut taken = 0usize;
        while tokio::time::timeout(std::time::Duration::from_millis(50), ours.write_all(&pong))
            .await
            .is_ok()
        {
            taken += 1;
        }
        // A 12-byte frame holds about 64 bytes of heap: its allocation, its `Bytes`, its slot.
        let most = MOST_LANE_BYTES / 64 + 2;
        assert!(
            taken <= most,
            "{taken} pongs were queued, past the {most} the lane's bound holds in memory"
        );
    }
}
