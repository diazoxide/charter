//! A deterministic link simulator: two in-process ends joined by a link with a one-way delay, a
//! rate, a buffer and per-packet loss, in tokio's virtual time (FD-4, #643).
//!
//! **Deterministic.** A test runs it on a current-thread runtime with the clock paused
//! (`#[tokio::test(start_paused = true)]`): time moves only when every task waits, so a slow or
//! loaded machine runs the same schedule as a fast one, and the loss is drawn from a seeded
//! generator. A delay this link adds is a delay the test can assert on, which wall-clock
//! budgets in plain `cargo test` may not be (ADR 0086). The ends' own work takes no time here.
//!
//! **The model.** Each direction is its own link. A chunk read from the sender is serialized at
//! `bytes_per_second`, after whatever is still on the wire ahead of it, and delivered
//! `one_way` later. Loss is drawn per `packet` bytes at one in `loss_one_in`; a chunk with a
//! lost packet waits one more `retransmit` (fast retransmit, RFC 5681), and everything behind
//! it waits for it, since the stream is ordered. At most `buffer` bytes are between the
//! sender's write and the receiver's read, past which the sender's writes wait, as on a socket
//! or a pipe. There is **no congestion control**: no slow start, no window halving after a
//! loss. Kernel netem over TCP is SC-21's job (#828).

use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream, ReadHalf, WriteHalf, duplex};
use tokio::sync::{Semaphore, mpsc};
use tokio::time::{Instant, sleep_until};

/// One direction's shape; both directions get the same one.
#[derive(Debug, Clone, Copy)]
pub struct Shape {
    pub one_way: Duration,
    pub bytes_per_second: f64,
    /// Bytes between the sender's write and the receiver's read, at most.
    pub buffer: usize,
    /// One packet in this many is lost; 0 loses none.
    pub loss_one_in: u64,
    pub packet: usize,
    /// What a lost packet adds: one round trip, for fast retransmit.
    pub retransmit: Duration,
}

/// The most the simulator reads from a sender at once.
const MOST_CHUNK: usize = 16 * 1024;

/// What each end's own pipe into the link holds: part of [`Shape::buffer`]'s budget in effect,
/// so it is kept small.
pub const END_PIPE: usize = 4 * 1024;

/// A small, seeded generator, so a failing run is the same run again.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }
}

/// Two ends joined by the link: the first is the client's, the second the host's.
pub fn link(shape: Shape, seed: u64) -> (DuplexStream, DuplexStream) {
    let (client, client_far) = duplex(END_PIPE);
    let (host, host_far) = duplex(END_PIPE);
    let (client_reads, client_writes) = tokio::io::split(client_far);
    let (host_reads, host_writes) = tokio::io::split(host_far);
    tokio::spawn(direction(client_reads, host_writes, shape, seed));
    tokio::spawn(direction(
        host_reads,
        client_writes,
        shape,
        seed.wrapping_add(1),
    ));
    (client, host)
}

async fn direction(
    mut from: ReadHalf<DuplexStream>,
    mut to: WriteHalf<DuplexStream>,
    shape: Shape,
    seed: u64,
) {
    let room = Arc::new(Semaphore::new(shape.buffer));
    let (queue, mut due) = mpsc::unbounded_channel::<(Instant, Bytes)>();
    let freed = Arc::clone(&room);
    tokio::spawn(async move {
        while let Some((at, bytes)) = due.recv().await {
            sleep_until(at).await;
            if to.write_all(&bytes).await.is_err() {
                return;
            }
            freed.add_permits(bytes.len());
        }
    });
    let mut random = Lcg(seed);
    let mut free_at = Instant::now();
    let mut last_at = Instant::now();
    let mut buffer = vec![0u8; MOST_CHUNK.min(shape.buffer)];
    loop {
        // Room for a whole chunk first, so the sender waits once the link is full.
        let Ok(permit) = room.acquire_many(buffer.len() as u32).await else {
            return;
        };
        let n = match from.read(&mut buffer).await {
            Ok(0) | Err(_) => return,
            Ok(n) => n,
        };
        permit.forget();
        room.add_permits(buffer.len() - n);
        let now = Instant::now();
        free_at = free_at.max(now) + Duration::from_secs_f64(n as f64 / shape.bytes_per_second);
        let mut at = free_at + shape.one_way;
        if shape.loss_one_in > 0
            && (0..n.div_ceil(shape.packet))
                .any(|_| random.next().is_multiple_of(shape.loss_one_in))
        {
            at += shape.retransmit;
        }
        // Ordered: nothing is delivered before a chunk sent ahead of it.
        at = at.max(last_at);
        last_at = at;
        if queue
            .send((at, Bytes::copy_from_slice(&buffer[..n])))
            .is_err()
        {
            return;
        }
    }
}
