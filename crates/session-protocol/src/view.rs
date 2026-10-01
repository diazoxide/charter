//! A view: one terminal's bytes to one client, on a stream of its own.
//!
//! **On the wire,** the host opens the stream and writes a header (`0x01`, then the view's
//! number, its epoch and the snapshot's length, each a big-endian `u32`), the snapshot, and
//! then every byte the terminal writes after it, raw. The snapshot is what
//! `Engine::snapshot` makes: the bytes that draw the scrollback, the screen, the cursor and
//! the modes into a blank terminal. The client writes back only acknowledgements, each a
//! big-endian `u32`: how many more bytes it has drawn.
//!
//! **The host never waits on a view.** [`Feed::push`] queues and returns. A task per view
//! writes the queue to the stream.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, MutexGuard};

use bytes::Bytes;
use tokio::io::{AsyncReadExt, AsyncWriteExt, ReadHalf, WriteHalf};
use tokio::sync::{Notify, watch};
use tokio_util::sync::CancellationToken;

use crate::link::{LinkError, Stream};

/// What opens a view's stream.
pub const VIEW: u8 = 0x01;

/// The most a [`Reader`] hands over in one [`Chunk`].
const MOST_CHUNK_BYTES: usize = 16 * 1024;

/// How much one view may hold, and when it pauses.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// Bytes queued for the view and not yet written. Past it, the view has fallen behind.
    pub most_queued_bytes: usize,
    /// Bytes written and not yet acknowledged, past which the host stops writing.
    pub high_watermark: usize,
    /// Bytes not yet acknowledged, below which the host writes again.
    pub low_watermark: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            most_queued_bytes: 1 << 20,
            high_watermark: 64 << 10,
            low_watermark: 16 << 10,
        }
    }
}

/// What a client reads from a view.
#[derive(Debug, PartialEq, Eq)]
pub enum Chunk {
    /// Part of the snapshot. Every snapshot chunk comes before the first live one.
    Snapshot(Bytes),
    /// Bytes the terminal wrote after the snapshot.
    Live(Bytes),
}

/// The view could not keep up and was dropped (see [`Feed::push`]).
#[derive(Debug, thiserror::Error)]
#[error("the view fell behind and was dropped; attach it again with a fresh snapshot")]
pub struct FellBehind;

#[derive(Default)]
struct Queue {
    chunks: VecDeque<Bytes>,
    /// The bytes in `chunks`.
    bytes: usize,
    /// The [`Feed`] was dropped: write what is queued, then end the view.
    gone: bool,
}

struct Shared {
    queue: Mutex<Queue>,
    more: Notify,
    /// Cancelled when the view falls behind: the writer stops where it is.
    dropped: CancellationToken,
    most_queued: usize,
}

impl Shared {
    fn queue(&self) -> MutexGuard<'_, Queue> {
        self.queue
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// The host's end of a view: where the terminal's bytes go.
pub struct Feed {
    shared: Arc<Shared>,
}

impl Feed {
    /// Queue `bytes` for the view, without waiting.
    ///
    /// When the queue would pass [`Limits::most_queued_bytes`], the view has fallen behind:
    /// its queue is let go, its stream ends, and this and every later push answer
    /// [`FellBehind`]. The host then attaches the view again with a fresh snapshot and the next
    /// epoch, exactly as it attaches a new one.
    pub fn push(&self, bytes: Bytes) -> Result<(), FellBehind> {
        let mut queue = self.shared.queue();
        if self.shared.dropped.is_cancelled() {
            return Err(FellBehind);
        }
        if queue.bytes + bytes.len() > self.shared.most_queued {
            queue.chunks.clear();
            queue.bytes = 0;
            self.shared.dropped.cancel();
            return Err(FellBehind);
        }
        queue.bytes += bytes.len();
        queue.chunks.push_back(bytes);
        drop(queue);
        self.shared.more.notify_one();
        Ok(())
    }

    /// Bytes queued and not yet written: what this view holds of the host's memory, beside
    /// the at most [`Limits::high_watermark`] bytes it has written and the client not drawn.
    pub fn queued_bytes(&self) -> usize {
        self.shared.queue().bytes
    }

    /// Whether the view fell behind and was dropped.
    pub fn fell_behind(&self) -> bool {
        self.shared.dropped.is_cancelled()
    }
}

impl Drop for Feed {
    fn drop(&mut self) {
        self.shared.queue().gone = true;
        self.shared.more.notify_one();
    }
}

/// Start a view on `stream`, a stream this end just opened: write the header and the
/// snapshot, then whatever is pushed to the [`Feed`] it returns, never more than
/// `limits.high_watermark` ahead of what the client has acknowledged.
pub fn start(stream: Stream, view: u32, epoch: u32, snapshot: Bytes, limits: Limits) -> Feed {
    let shared = Arc::new(Shared {
        queue: Mutex::new(Queue::default()),
        more: Notify::new(),
        dropped: CancellationToken::new(),
        most_queued: limits.most_queued_bytes,
    });
    let pump = Arc::clone(&shared);
    tokio::spawn(async move {
        let (acks, out) = tokio::io::split(stream);
        let (acked, drawn) = watch::channel(0u64);
        let counting = tokio::spawn(count_acks(acks, acked));
        let mut writer = Gated {
            out,
            sent: 0,
            drawn,
            limits,
        };
        tokio::select! {
            _ = writer.write(view, epoch, &snapshot, &pump) => {}
            () = pump.dropped.cancelled() => {}
        }
        let _ = writer.out.shutdown().await;
        counting.abort();
    });
    Feed { shared }
}

/// Read the client's acknowledgements, and keep the running total of bytes drawn.
async fn count_acks(mut acks: ReadHalf<Stream>, drawn: watch::Sender<u64>) {
    while let Ok(n) = acks.read_u32().await {
        drawn.send_modify(|total| *total += u64::from(n));
    }
}

/// The writing half of a view, held to the watermarks.
struct Gated {
    out: WriteHalf<Stream>,
    sent: u64,
    drawn: watch::Receiver<u64>,
    limits: Limits,
}

impl Gated {
    async fn write(
        &mut self,
        view: u32,
        epoch: u32,
        snapshot: &Bytes,
        shared: &Shared,
    ) -> std::io::Result<()> {
        let length = u32::try_from(snapshot.len()).map_err(|_| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "a snapshot over 4 GiB")
        })?;
        let mut header = Vec::with_capacity(13);
        header.push(VIEW);
        header.extend_from_slice(&view.to_be_bytes());
        header.extend_from_slice(&epoch.to_be_bytes());
        header.extend_from_slice(&length.to_be_bytes());
        self.out.write_all(&header).await?;
        self.send(snapshot).await?;
        loop {
            let next = {
                let mut queue = shared.queue();
                match queue.chunks.pop_front() {
                    Some(chunk) => {
                        queue.bytes -= chunk.len();
                        Some(chunk)
                    }
                    None if queue.gone => return Ok(()),
                    None => None,
                }
            };
            match next {
                Some(chunk) => self.send(&chunk).await?,
                None => shared.more.notified().await,
            }
        }
    }

    /// Write `bytes` in pieces, waiting before each one while the client is a high watermark
    /// behind, until it is back below the low one.
    async fn send(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        let high = self.limits.high_watermark as u64;
        let low = self.limits.low_watermark as u64;
        for piece in bytes.chunks(MOST_CHUNK_BYTES) {
            if self.sent - *self.drawn.borrow() >= high {
                let sent = self.sent;
                self.out.flush().await?;
                if self
                    .drawn
                    .wait_for(|drawn| sent - (*drawn).min(sent) <= low)
                    .await
                    .is_err()
                {
                    return Err(std::io::ErrorKind::BrokenPipe.into());
                }
            }
            self.out.write_all(piece).await?;
            self.sent += piece.len() as u64;
        }
        self.out.flush().await
    }
}

/// The client's end of a view.
pub struct Reader {
    view: u32,
    epoch: u32,
    snapshot_left: usize,
    input: ReadHalf<Stream>,
    acks: WriteHalf<Stream>,
}

impl Reader {
    /// The view's number, as the host gave it.
    pub fn view(&self) -> u32 {
        self.view
    }

    /// Which snapshot of the view this is. A view attached again after it fell behind comes
    /// back with the same number and a higher epoch, and the client resets its terminal.
    pub fn epoch(&self) -> u32 {
        self.epoch
    }

    /// The next chunk, or `None` when the host has ended the view.
    pub async fn next(&mut self) -> Option<Result<Chunk, LinkError>> {
        let most = if self.snapshot_left > 0 {
            self.snapshot_left.min(MOST_CHUNK_BYTES)
        } else {
            MOST_CHUNK_BYTES
        };
        let mut buffer = vec![0u8; most];
        match self.input.read(&mut buffer).await {
            Ok(0) => None,
            Ok(n) => {
                buffer.truncate(n);
                let bytes = Bytes::from(buffer);
                if self.snapshot_left > 0 {
                    self.snapshot_left -= n;
                    Some(Ok(Chunk::Snapshot(bytes)))
                } else {
                    Some(Ok(Chunk::Live(bytes)))
                }
            }
            Err(e) => Some(Err(e.into())),
        }
    }

    /// Tell the host `bytes` more have been drawn. A client acks what its terminal has
    /// finished writing, xterm's write callback, not what it has merely read.
    pub async fn ack(&mut self, bytes: usize) -> Result<(), LinkError> {
        let mut left = bytes;
        while left > 0 {
            let n = left.min(u32::MAX as usize);
            self.acks.write_all(&(n as u32).to_be_bytes()).await?;
            left -= n;
        }
        self.acks.flush().await?;
        Ok(())
    }
}

/// Take a view the host opened: read its header.
pub async fn accept(stream: Stream) -> Result<Reader, LinkError> {
    let (mut input, acks) = tokio::io::split(stream);
    let kind = input.read_u8().await?;
    if kind != VIEW {
        return Err(LinkError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("a stream that opens with {kind:#04x} is not a view"),
        )));
    }
    let view = input.read_u32().await?;
    let epoch = input.read_u32().await?;
    let snapshot_left = input.read_u32().await? as usize;
    Ok(Reader {
        view,
        epoch,
        snapshot_left,
        input,
        acks,
    })
}
