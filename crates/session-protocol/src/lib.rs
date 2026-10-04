//! charter's session protocol, the transport layer: what `charterd` and its clients speak
//! over any ordered byte stream (FD-4, ADR 0068 §4).
//!
//! The transport is whatever the client has: a unix socket to `charterd.sock`, a pair of
//! pipes, or a connector's stdio (`ssh`, `docker exec`, `kubectl exec`), and inside a
//! runner's Noise channel (ADR 0078 §3), unchanged. Nothing here assumes a local socket.
//!
//! Four layers, each in its own module, and one check before them:
//!
//! 0. `local` (unix only): on `charterd.sock`, a connection from another uid is closed before
//!    a byte of it is read, and only what passed is a `local::SameUser`, the one thing the
//!    host's `link::serve` takes (FD-6).
//! 1. [`version`]: before anything else, the two ends agree on one version, and refuse when
//!    they share no major, or when the other end has not finished its half within
//!    [`version::HANDSHAKE_TIMEOUT`], one deadline over this step and the two after it. It
//!    fails closed.
//! 2. [`auth`]: admission is mutual. The client proves, over the host's fresh challenge, that it
//!    holds one client scope's credential, and the host proves, over the client's fresh nonce,
//!    that it holds it too; either end refuses the other otherwise. The credential never
//!    crosses the wire. There is no anonymous scope, and a [`link::Link`] exists only once
//!    both ends have admitted (FD-6, ADR 0068 §5).
//! 3. [`link`]: the stream is multiplexed with Yamux into a **control lane** (length-delimited
//!    frames, the commands and events) and any number of streams, each with **its own credit
//!    flow control**, so a pane that stops reading stops only itself. The control lane is a
//!    **priority lane**: its frames are sent ahead of every stream's still waiting.
//! 4. [`view`]: a terminal's bytes to one client: a header, the snapshot, then the live bytes,
//!    raw. The host holds a view to two bounds: it writes no more than a **high watermark**
//!    ahead of what the client has drawn (xterm.js's flow control, with acknowledgements), and
//!    it queues no more than a bound in bytes. A view that falls behind that bound is dropped,
//!    and the host's [`view::Attacher`] attaches it again with a fresh snapshot at the next
//!    epoch, after a growing pause when its client drew nothing. A client that acknowledges
//!    more than it was sent loses the view.
//!
//! **Why these pieces, and not others.** ADR 0068 asked for an existing multiplexer rather than
//! a new one. Yamux is the standard one for a single reliable, ordered connection that is not
//! QUIC (HTTP/2 needs HTTP semantics and has dropped stream priority; QUIC needs UDP, which a
//! connector's stdio is not; SSH channels would make the protocol depend on SSH). The `yamux`
//! crate is what libp2p and Substrate run, and it has per-stream windows. The control lane's
//! framing is tokio-util's `LengthDelimitedCodec`, the standard one for the runtime the app
//! already uses.
//!
//! **What carries the control lane past a busy terminal** (ADR 0068 §4, amended again by FD-4).
//! Yamux has no stream priorities, so the link schedules its frames itself: the multiplexer
//! writes into `priority`, which sends the control lane's frames ahead of every terminal's
//! still waiting, and lets the other streams take turns, a frame each. Nothing on the wire
//! changes. What the transport already holds is the one wait left, and it is bounded, since
//! nothing ahead of it is unbounded: every stream's bytes in flight are held to its window,
//! every view's to its watermark, the streams on a link to [`link::MOST_STREAMS`]. The tests
//! show it on a deterministic link simulator in virtual time: a control frame through fifty busy
//! streams on a 1 MB/s link in 73 ms, where the multiplexer alone took 908 ms, and a needs-you
//! through fifty busy chats on a 150 ms, 2% loss link within #643's 1 s plus the round trip.
//! A keystroke through ten flooding panes over a unix socket is `charter-session-bench`'s
//! (ADR 0086's L1 row).
//!
//! **What the control lane's frames mean** is the last two modules (FD-26, ADR 0068 §4):
//!
//! 5. [`session`]: the session protocol, small, public and versioned, with the compatibility
//!    promise; `tests/fixtures/session-protocol.jsonl` holds it in CI.
//! 6. [`ui`]: the UI RPC beside it, the app's own commands for its own build and the `local-ui`
//!    scope only, excluded from that promise.
//!
//! **What this crate does not decide.** Where a view's bytes and snapshot come from, and what
//! each command does, is the host's (FD-5): `Engine::snapshot`, the session's output, and a
//! [`session::Host`]. What each scope may call is [`grants`]' one table (FD-27), which
//! [`session::serve`] checks before any host hears of a command.

pub mod auth;
pub mod grants;
pub mod link;
#[cfg(unix)]
pub mod local;
mod priority;
pub mod session;
pub mod ui;
pub mod version;
pub mod view;
