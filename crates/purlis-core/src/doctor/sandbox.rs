//! The `sandbox` row: where the project turned the sandbox on (ADR 0067), how many new chats
//! on this machine started without it — the opt-out rate SD-2's outcome bar is measured by
//! (V12). A local count, shown here and in Settings and never sent (ruling V78 d).

use super::{Config, Doctor, Row};

/// The row, only where the project turned the sandbox on: a project that has not prints the
/// rows it always printed.
pub(super) fn sandbox(d: &Doctor) -> Option<Row> {
    const NAME: &str = "sandbox";
    let cfg = match &d.config {
        Config::Read(cfg) => cfg,
        Config::Malformed(_) | Config::Refused(_) => return None,
    };
    crate::sandbox::Said::of(Some(cfg)).policy?;
    Some(Row::ok(
        NAME,
        format!("on — {}", crate::sandbox::local::tally(&d.root).said()),
    ))
}
