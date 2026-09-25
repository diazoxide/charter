//! The repos the operator's own forge login reaches, for the workspace repo picker (ADR 0055).
//!
//! **Asked live and never saved as a list.** Each operator's login reaches different repos,
//! and `inventory/repos.json` is tracked, so a listing written there is one operator's view
//! handed to everyone. The picker asks this each time it opens; what the operator then picks
//! is [`take`]n into the inventory, added beside what is there and never replacing it.

use std::path::Path;

use serde_json::Value;

use crate::forge;
use crate::inventory;

/// What the operator's logins reach, and what could not be asked.
#[derive(Debug, Default)]
pub struct Reachable {
    /// Inventory records, sorted by name. `stack` is `"unknown"`: probing every repo an
    /// operator can reach to draw a list is a call per repo, and the stack is descriptive.
    pub repos: Vec<Value>,
    /// One sentence per forge that did not answer — not logged in, or the listing failed —
    /// in the forge CLI's own words. The rest of the forges' repos are still listed.
    pub trouble: Vec<String>,
}

/// Every repo the plane's forges let this operator reach, under the owners it declares and
/// past its excludes.
///
/// `Err` only for a plane whose forges cannot be read at all; a forge that did not answer is
/// [`Reachable::trouble`], so one logged-out host does not hide another's repos.
pub fn reachable(root: &Path) -> Result<Reachable, String> {
    let cfg = forge::load_config(root)?;
    let mut out = Reachable::default();
    let mut batches = Vec::new();
    for (forge, owner, exclude) in forge::to_query(&cfg)? {
        if let Err(why) = forge.check_auth() {
            out.trouble.push(why.0);
            continue;
        }
        match forge.list_accessible(&owner) {
            Ok(projects) => batches.push(
                projects
                    .iter()
                    .filter(|p| {
                        let name = p.get("name").and_then(Value::as_str).unwrap_or_default();
                        !exclude.iter().any(|e| e == name)
                    })
                    .map(|p| inventory::record(&forge, p, "unknown"))
                    .collect::<Vec<_>>(),
            ),
            Err(why) => out.trouble.push(why.0),
        }
    }
    match inventory::merge(&batches) {
        Ok(repos) => out.repos = repos,
        Err(why) => out.trouble.push(why),
    }
    Ok(out)
}

/// Add the named repos to the inventory, so `clone` can find them — the ones it does not
/// already list, asked of the forge as this operator.
///
/// A repo the inventory already lists keeps its record: that one may carry a stack `discover`
/// probed, where this one would write `"unknown"` over it. A name nobody can reach is left
/// for `clone` to refuse in its own words.
pub fn take(root: &Path, names: &[String]) -> Result<(), String> {
    let cfg = forge::load_config(root).unwrap_or_default();
    let group = forge::group_of(&cfg, 0);
    let listed = inventory::listed(&inventory::load(root, &group)?);
    let wanted: Vec<&String> = names
        .iter()
        .filter(|n| inventory::find(&listed, n).is_none())
        .collect();
    if wanted.is_empty() {
        return Ok(());
    }
    let reached = reachable(root)?;
    let new: Vec<Value> = wanted
        .iter()
        .filter_map(|n| inventory::find(&reached.repos, n).cloned())
        .collect();
    if new.is_empty() {
        return Ok(());
    }
    inventory::add(root, &group, &new).map(|_| ())
}
