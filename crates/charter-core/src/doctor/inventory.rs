//! `inventory`: can this plane clone anything?
//!
//! Asked of [`crate::inventory::repos`] rather than of the file's own count, because a plane
//! can clone its own repo without ever running `discover` — the root's `origin` says what it
//! is. So `discover` is optional, and a plane that can reach the repo it was made for is
//! missing nothing.
//!
//! Warning regardless would be the permanently-yellow preflight this file keeps arguing
//! against, and the nag is expensive in its own right: on a personal account `discover`
//! enumerates every repo the owner has and writes that listing into a tracked
//! `inventory/repos.json`. Telling someone to publish sixty repos to silence a row about the
//! one they already have is worse advice than saying nothing.

use super::{Doctor, Row};
use crate::forge;

pub(super) fn inventory(d: &Doctor) -> Row {
    const NAME: &str = "inventory";
    let empty = toml::Table::new();
    let cfg = d.config.table().unwrap_or(&empty);
    let group = forge::group_of(cfg, 0);
    let exclude = forge::exclude_of(cfg, 0);
    // Python reads the file with a bare `json.loads` and lets a malformed one take the whole
    // command down; this says which file it could not read and carries on with the rest.
    let doc = match crate::inventory::load(&d.root, &group) {
        Ok(doc) => doc,
        Err(why) => return Row::not_checked(NAME, why),
    };
    if let Some(count) = doc.get("count").filter(|v| forge::truthy(v)) {
        return Row::ok(NAME, format!("{} repos mapped", forge::py_str(count)));
    }
    if !crate::inventory::repos(&d.root, &doc, &exclude).is_empty() {
        return Row::ok(
            NAME,
            "not built — this plane's own repo is clonable without it",
        );
    }
    Row::warn(
        NAME,
        "empty, and this plane's own repo could not be derived",
        "Run: charter discover  (builds inventory/repos.json).",
    )
}
