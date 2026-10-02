//! Work items: one type, with trackers as its backends (V3, FI4, ADR 0088).
//!
//! A work item is a view of a **Workspace**, not a concept of its own (X22, FI5). It is named by
//! its [`key::TrackerKey`]: the tracker's name and the item's own reference where it lives. A
//! forge's own identifier ([`crate::forge::backend::ForgeRef`]) travels beside the key in
//! [`WorkItem`], never inside it, and never into the project.
//!
//! - [`key`]: the key's grammar, normalised once and compared exactly.
//! - [`log`]: the work link log, `workspaces/<ws>/work/<device>.jsonl`, and its fold.
//! - [`list`]: a workspace's Work list, one item per key, its aliases collapsed.
//! - [`promote`]: a todo promoted to an issue, which closes the todo and leaves an alias.

pub mod key;
pub mod list;
pub mod log;
pub mod promote;

pub use key::TrackerKey;

use crate::forge::backend::ForgeRef;

/// One work item as a tracker answers it: its key, and the tracker's own identifier beside it.
///
/// The key is what the project and the audit name the item by. The [`ForgeRef`] is the forge's
/// node or global id, which a round trip uses so that it never re-derives one (ADR 0070 §1). A
/// todo has none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkItem {
    pub key: TrackerKey,
    pub forge_ref: Option<ForgeRef>,
    /// The item's page, for a person to open. Empty for a todo.
    pub url: String,
}
