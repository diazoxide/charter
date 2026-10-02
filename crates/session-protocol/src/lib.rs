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
//! 2. [`auth`]: the client is admitted as one client scope, by a proof over a fresh challenge
//!    that it holds that scope's credential, or refused. The credential never crosses the wire. There is no anonymous scope, and a [`link::Link`] exists only once admitted
//!    (FD-6, ADR 0068 §5).
//! 3. [`link`]: the stream is multiplexed with Yamux into a **control lane** (length-delimited
//!    frames, the commands and events) and any number of streams, each with **its own credit
//!    flow control**, so a pane that stops reading stops only itself.
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
//! **What carries the control lane past a busy terminal** (ADR 0068, amended by FD-4). Yamux
//! has no stream priorities. The lane gets through because nothing ahead of it is unbounded:
//! every stream's bytes in flight are held to its window, every view's to its watermark, the
//! streams on a link to [`link::MOST_STREAMS`], and Yamux sends data in frames of at most
//! 16 KiB, so a control frame interleaves with them. The tests print what that gives as
//! evidence (ADR 0086 keeps wall-clock budgets out of `cargo test`): a keystroke through ten
//! panes flooding about 100 to 270 MB/s at a p95 of a few milliseconds, and a needs-you through
//! fifty busy chats on a shaped 150 ms link in well under a second.
//!
//! **What this crate does not decide.** What a control frame means (the session protocol's
//! commands and events, and the UI RPC beside them) is FD-26's split and LV-2a's crate. Where
//! a view's bytes and snapshot come from is the host's (FD-5): `Engine::snapshot` and the
//! session's output.

pub mod auth;
pub mod link;
#[cfg(unix)]
pub mod local;
pub mod version;
pub mod view;
