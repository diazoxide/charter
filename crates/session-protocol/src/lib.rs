//! charter's session protocol, the transport layer: what `charterd` and its clients speak
//! over any ordered byte stream (FD-4, ADR 0068 §4).
//!
//! The transport is whatever the client has: a unix socket to `charterd.sock`, a pair of
//! pipes, or a connector's stdio (`ssh`, `docker exec`, `kubectl exec`), and inside a
//! runner's Noise channel (ADR 0078 §3), unchanged. Nothing here assumes a local socket.
//!
//! Three layers, each in its own module:
//!
//! 1. [`version`]: before anything else, the two ends agree on one version, and refuse when
//!    they share no major. It fails closed.
//! 2. [`link`]: the stream is multiplexed with Yamux into a **control lane** (length-delimited
//!    frames, the commands and events) and any number of streams, each with **its own credit
//!    flow control**, so a pane that stops reading stops only itself.
//! 3. [`view`]: a terminal's bytes to one client: a header, the snapshot, then the live bytes,
//!    raw. The host holds a view to two bounds: it writes no more than a **high watermark**
//!    ahead of what the client has drawn (xterm.js's flow control, with acknowledgements), and
//!    it queues no more than a bound in bytes. A view that falls behind that bound is dropped,
//!    and the host attaches it again with a fresh snapshot.
//!
//! **Why these pieces, and not others.** ADR 0068 asked for an existing multiplexer rather than
//! a new one. Yamux is the standard one for a single reliable, ordered connection that is not
//! QUIC (HTTP/2 needs HTTP semantics and has dropped stream priority; QUIC needs UDP, which a
//! connector's stdio is not; SSH channels would make the protocol depend on SSH). The `yamux`
//! crate is what libp2p and Substrate run, and it has per-stream windows. The control lane's
//! framing is tokio-util's `LengthDelimitedCodec`, the standard one for the runtime the app
//! already uses.
//!
//! **What carries the control lane past a busy terminal.** Yamux has no stream priorities. The
//! lane gets through because nothing ahead of it is unbounded: every stream's bytes in flight
//! are held to its window and every view's to its watermark, and Yamux sends data in frames of
//! at most 16 KiB, so a control frame interleaves with them. The tests measure it: ten panes
//! flooding about 150 MB/s leave a keystroke in an eleventh at a few milliseconds, and fifty
//! chats over a shaped 150 ms, 2% loss, 10 MB/s link show each needs-you in well under a
//! second.
//!
//! **What this crate does not decide.** What a control frame means (the session protocol's
//! commands and events, and the UI RPC beside them) is FD-26's split and LV-2a's crate. Where
//! a view's bytes and snapshot come from is the host's (FD-5): `Engine::snapshot` and the
//! session's output.

pub mod link;
pub mod version;
pub mod view;
