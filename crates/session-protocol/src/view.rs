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

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::Duration;

use bytes::Bytes;
use tokio::io::{AsyncReadExt, AsyncWriteExt, ReadHalf, WriteHalf};
use tokio::sync::{Notify, watch};
use tokio_util::sync::CancellationToken;

use crate::link::{LinkError, Opener, Stream};

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

/// Why the host closed a view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Closed {
    /// The view could not keep up and was dropped (see [`Feed::push`]).
    #[error("the view fell behind and was dropped; attach it again with a fresh snapshot")]
    FellBehind,
    /// The client acknowledged bytes it was never sent.
    #[error("the client acknowledged more than it was sent")]
    OverAcknowledged,
}

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
    /// Cancelled when the host closes the view: the writer stops where it is.
    dropped: CancellationToken,
    /// Why, once it has.
    why: OnceLock<Closed>,
    /// Bytes handed to the stream so far, header aside: what the client may acknowledge.
    sent: AtomicU64,
    /// Bytes the client has acknowledged drawing.
    drawn: AtomicU64,
    most_queued: usize,
}

impl Shared {
    fn queue(&self) -> MutexGuard<'_, Queue> {
        self.queue
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Close the view: the first reason given is the one kept.
    fn close(&self, why: Closed) -> Closed {
        let why = *self.why.get_or_init(|| why);
        self.dropped.cancel();
        why
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
    /// [`Closed::FellBehind`]. The host then attaches the view again with a fresh snapshot and the next
    /// epoch, exactly as it attaches a new one.
    pub fn push(&self, bytes: Bytes) -> Result<(), Closed> {
        let mut queue = self.shared.queue();
        if let Some(why) = self.shared.why.get() {
            return Err(*why);
        }
        if queue.bytes + bytes.len() > self.shared.most_queued {
            queue.chunks.clear();
            queue.bytes = 0;
            return Err(self.shared.close(Closed::FellBehind));
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

    /// Bytes written to the client and not yet acknowledged as drawn: at most the high
    /// watermark, plus the one piece being written.
    pub fn in_flight_bytes(&self) -> usize {
        let sent = self.shared.sent.load(Ordering::Acquire);
        let drawn = self.shared.drawn.load(Ordering::Acquire);
        usize::try_from(sent.saturating_sub(drawn)).unwrap_or(usize::MAX)
    }

    /// Why the host closed the view, once it has.
    pub fn closed(&self) -> Option<Closed> {
        self.shared.why.get().copied()
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
fn start(
    stream: Stream,
    view: ViewId,
    epoch: Epoch,
    snapshot: Bytes,
    limits: Limits,
) -> (Feed, Arc<Shared>) {
    let shared = Arc::new(Shared {
        queue: Mutex::new(Queue::default()),
        more: Notify::new(),
        dropped: CancellationToken::new(),
        why: OnceLock::new(),
        sent: AtomicU64::new(0),
        drawn: AtomicU64::new(0),
        most_queued: limits.most_queued_bytes,
    });
    let pump = Arc::clone(&shared);
    tokio::spawn(async move {
        let (acks, out) = tokio::io::split(stream);
        let (acked, drawn) = watch::channel(0u64);
        let counting = tokio::spawn(count_acks(acks, acked, Arc::clone(&pump)));
        let mut writer = Gated {
            out,
            sent: 0,
            drawn,
            limits,
            shared: Arc::clone(&pump),
        };
        tokio::select! {
            _ = writer.write(view, epoch, &snapshot, &pump) => {}
            () = pump.dropped.cancelled() => {}
        }
        let _ = writer.out.shutdown().await;
        counting.abort();
    });
    (
        Feed {
            shared: Arc::clone(&shared),
        },
        shared,
    )
}

/// Read the client's acknowledgements, and keep the running total of bytes drawn. A client
/// that acknowledges more than it was sent is wrong about the view, and loses it.
async fn count_acks(mut acks: ReadHalf<Stream>, drawn: watch::Sender<u64>, shared: Arc<Shared>) {
    let mut total: u64 = 0;
    while let Ok(n) = acks.read_u32().await {
        match total.checked_add(u64::from(n)) {
            Some(more) if more <= shared.sent.load(Ordering::Acquire) => {
                total = more;
                shared.drawn.store(total, Ordering::Release);
                drawn.send_replace(total);
            }
            _ => {
                shared.close(Closed::OverAcknowledged);
                return;
            }
        }
    }
}

/// The writing half of a view, held to the watermarks.
struct Gated {
    out: WriteHalf<Stream>,
    sent: u64,
    drawn: watch::Receiver<u64>,
    limits: Limits,
    shared: Arc<Shared>,
}

impl Gated {
    async fn write(
        &mut self,
        view: ViewId,
        epoch: Epoch,
        snapshot: &Bytes,
        shared: &Shared,
    ) -> std::io::Result<()> {
        let length = u32::try_from(snapshot.len()).map_err(|_| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "a snapshot over 4 GiB")
        })?;
        let mut header = Vec::with_capacity(13);
        header.push(VIEW);
        header.extend_from_slice(&view.0.to_be_bytes());
        header.extend_from_slice(&epoch.0.to_be_bytes());
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
            if self.sent.saturating_sub(*self.drawn.borrow()) >= high {
                let sent = self.sent;
                self.out.flush().await?;
                if self
                    .drawn
                    .wait_for(|drawn| sent.saturating_sub(*drawn) <= low)
                    .await
                    .is_err()
                {
                    return Err(std::io::ErrorKind::BrokenPipe.into());
                }
            }
            // Counted before it is written, so an acknowledgement of it can never arrive first.
            self.sent += piece.len() as u64;
            self.shared.sent.store(self.sent, Ordering::Release);
            self.out.write_all(piece).await?;
        }
        self.out.flush().await
    }
}

/// The client's end of a view.
pub struct Reader {
    view: ViewId,
    epoch: Epoch,
    snapshot_left: usize,
    input: ReadHalf<Stream>,
    acks: WriteHalf<Stream>,
}

impl Reader {
    /// The view's number, as the host gave it.
    pub fn view(&self) -> ViewId {
        self.view
    }

    /// Which snapshot of the view this is. A view attached again after it fell behind comes
    /// back with the same number and a higher epoch, and the client resets its terminal.
    pub fn epoch(&self) -> Epoch {
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
async fn accept(stream: Stream) -> Result<Reader, LinkError> {
    let (mut input, acks) = tokio::io::split(stream);
    let kind = input.read_u8().await?;
    if kind != VIEW {
        return Err(LinkError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("a stream that opens with {kind:#04x} is not a view"),
        )));
    }
    let view = ViewId(input.read_u32().await?);
    let epoch = Epoch(input.read_u32().await?);
    let snapshot_left = input.read_u32().await? as usize;
    Ok(Reader {
        view,
        epoch,
        snapshot_left,
        input,
        acks,
    })
}

/// A view's number on a link: which terminal it shows. The layer above says which chat that is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ViewId(pub u32);

/// Which attachment of a view this is. Every re-attach, after a view fell behind, is the next
/// one, and a client resets its terminal for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Epoch(pub u32);

/// The pause before re-attaching a view whose client drew nothing before it fell behind. It
/// doubles with each such attachment in a row, up to [`MOST_BACKOFF`], and a client that
/// draws anything starts again from none.
pub const FIRST_BACKOFF: Duration = Duration::from_millis(250);

/// The longest pause before a re-attach.
pub const MOST_BACKOFF: Duration = Duration::from_secs(8);

/// What the host remembers of a view's last attachment.
struct Attached {
    epoch: Epoch,
    /// Attachments in a row that fell behind with nothing drawn.
    stalled: u32,
    last: Option<Arc<Shared>>,
}

/// The host's views on one link: it numbers each view's attachments, and paces the
/// re-attachment of a client that never draws.
pub struct Attacher {
    opener: Opener,
    limits: Limits,
    views: Mutex<HashMap<ViewId, Attached>>,
}

impl Attacher {
    pub fn new(opener: Opener, limits: Limits) -> Self {
        Attacher {
            opener,
            limits,
            views: Mutex::new(HashMap::new()),
        }
    }

    /// Attach `view`, new or again, with `snapshot`: open its stream, at the next epoch, after
    /// the pause its record calls for.
    pub async fn attach(&self, view: ViewId, snapshot: Bytes) -> Result<Feed, LinkError> {
        let (epoch, pause) = {
            let mut views = self
                .views
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let next = match views.get(&view) {
                None => Attached {
                    epoch: Epoch(1),
                    stalled: 0,
                    last: None,
                },
                Some(before) => {
                    let epoch = before.epoch.0.checked_add(1).map(Epoch).ok_or_else(|| {
                        LinkError::Io(std::io::Error::other("a view attached 4 billion times"))
                    })?;
                    let stalled = match &before.last {
                        Some(last)
                            if last.why.get() == Some(&Closed::FellBehind)
                                && last.drawn.load(Ordering::Acquire) == 0 =>
                        {
                            before.stalled.saturating_add(1)
                        }
                        _ => 0,
                    };
                    Attached {
                        epoch,
                        stalled,
                        last: None,
                    }
                }
            };
            let pause = match next.stalled {
                0 => Duration::ZERO,
                n => FIRST_BACKOFF
                    .saturating_mul(1 << (n - 1).min(16))
                    .min(MOST_BACKOFF),
            };
            let epoch = next.epoch;
            views.insert(view, next);
            (epoch, pause)
        };
        tokio::time::sleep(pause).await;
        let stream = self.opener.open().await?;
        let (feed, shared) = start(stream, view, epoch, snapshot, self.limits);
        if let Some(attached) = self
            .views
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get_mut(&view)
            .filter(|attached| attached.epoch == epoch)
        {
            attached.last = Some(shared);
        }
        Ok(feed)
    }
}

/// The client's views on one link: it takes each view the host opens, and refuses an
/// attachment that is not newer than the last it saw of that view.
#[derive(Default)]
pub struct Viewer {
    seen: HashMap<ViewId, Epoch>,
}

impl Viewer {
    /// Take a view the host opened.
    pub async fn accept(&mut self, stream: Stream) -> Result<Reader, LinkError> {
        let reader = accept(stream).await?;
        if let Some(last) = self.seen.get(&reader.view)
            && reader.epoch <= *last
        {
            return Err(LinkError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "view {} at epoch {}, which is not newer than {}",
                    reader.view.0, reader.epoch.0, last.0
                ),
            )));
        }
        self.seen.insert(reader.view, reader.epoch);
        Ok(reader)
    }
}
