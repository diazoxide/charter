//! The `sandbox` row: where the project turned the sandbox on (ADR 0067), how many new chats
//! on this machine started without it — the opt-out rate SD-2's outcome bar is measured by
//! (V12). A local count, shown here and in Settings and never sent (ruling V78 d).

use super::{Config, Doctor, Row};

/// The row, only where the project turned the sandbox on, or an administrator's policy
/// requires it on this machine: any other project prints the rows it always printed.
pub(super) fn sandbox(d: &Doctor) -> Option<Row> {
    const NAME: &str = "sandbox";
    let cfg = match &d.config {
        Config::Read(cfg) => cfg,
        Config::Malformed(_) | Config::Refused(_) => return None,
    };
    // The project's own, or the one an administrator's policy requires here (D-1423-1): one
    // row either way, which says when it is policy's.
    let locks = crate::sandbox::policy::Locks::of(&d.root);
    let said = crate::sandbox::Said::of(Some(cfg));
    let its_own = said.policy.is_some();
    said.in_force(&locks)?;
    Some(Row::ok(
        NAME,
        format!(
            "on{} — {}",
            if its_own { "" } else { ", required by policy" },
            crate::sandbox::local::tally(&d.root).said()
        ),
    ))
}

/// The `sandbox blocks` row (#1338): what chats' sandboxes blocked in this project on this
/// machine over the last seven days, counted per operation, as the app heard each one
/// ([`crate::sandboxblock`]). Only where something was blocked: a project with none prints the
/// rows it always printed.
///
/// **A block of purlis's own operation is a purlis bug**, so any makes the row a warning that
/// says how to report it. Every other block is the chat's own work meeting the sandbox: counted,
/// not a fault.
pub(super) fn blocks(d: &Doctor) -> Option<Row> {
    blocks_at(d, now())
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

pub(super) fn blocks_at(d: &Doctor, now: u64) -> Option<Row> {
    const NAME: &str = "sandbox blocks";
    let counts = crate::sandboxblock::counts(&d.root, now);
    if counts.is_empty() {
        return None;
    }
    let said: Vec<String> = counts
        .iter()
        .map(|count| {
            let ours = if count.ours > 0 {
                format!(" ({} purlis's own)", count.ours)
            } else {
                String::new()
            };
            format!("{} {}{ours}", count.operation.word(), count.blocks)
        })
        .collect();
    let detail = format!("{} in the last 7 days", said.join(", "));
    if counts.iter().any(|count| count.ours > 0) {
        Some(Row::warn(
            NAME,
            detail,
            "A block of purlis's own operation is a purlis bug. The chat's tab offers Report, \
             which shows a draft naming only the operation, the kind of path and the versions, \
             and sends nothing until you press File report.",
        ))
    } else {
        Some(Row::ok(NAME, detail))
    }
}
