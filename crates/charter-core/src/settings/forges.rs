//! **Forges, as a Settings collection** (ST-3, #1227): `[[forge]]` blocks in `charter.toml`,
//! added and removed through the collection write seam ([`super::collection`]).
//!
//! A block is what `docs/plane-format.md` documents: `kind` (`gitlab` or `github`), `owner` (or
//! `group`, which wins when a block has both), an optional bare `host`, and an optional
//! `exclude` list. A new block is written the way `charter init` writes one — `kind`, `owner`,
//! `host` — with `exclude` after, and only the keys that say something.
//!
//! **What uses a block** ([`referrers`]) is what would stop working without it. A block makes its
//! `host` a forge charter knows ([`crate::forge::known_in`]); removing it matters only when that
//! host is then unknown, or known as another kind — a block at `github.com` or `gitlab.com`, or
//! at a host another block also declares, is needed by nothing, since the host stays known. For
//! each host the removal would lose:
//!
//! - every repo `inventory/repos.json` catalogues on it (its clone's credential and git policy
//!   are that forge's);
//! - every `[repos.<name>] mode` that opens a request, for a repo catalogued on it;
//! - the project's own `[plane] mode`, when it opens a request and the project's origin is on it.
//!
//! What a block's `owner` and `exclude` say is only what `discover` lists next; nothing already
//! listed depends on it, so neither stops a removal.

use std::collections::BTreeMap;
use std::path::Path;

use super::Which;
use super::collection::{FieldRefusal, Referrer, Refusal};
use crate::forge::{self, Forge, Kind};
use crate::worktree::git;

/// The Settings group a save policy is changed in: the deep link a referrer carries.
const SAVING: &str = "project.saving";

/// One forge, as the Add form sends it: each field as typed. An empty `kind` is the default
/// kind, an empty `host` that kind's own host, and an empty `owner` none.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Entry {
    pub kind: String,
    pub owner: String,
    pub host: String,
    pub exclude: Vec<String>,
}

/// The entry as it is written: trimmed, the host lowercased (a remote's host is read lowercased,
/// so a capital in a declared host would never match one), the exclude list without blanks or
/// repeats.
struct Clean {
    kind: String,
    owner: String,
    host: String,
    exclude: Vec<String>,
}

fn clean(entry: &Entry) -> Clean {
    let kind = entry.kind.trim();
    let mut exclude: Vec<String> = Vec::new();
    for one in entry.exclude.iter().map(|one| one.trim()) {
        if !one.is_empty() && !exclude.iter().any(|kept| kept == one) {
            exclude.push(one.to_owned());
        }
    }
    Clean {
        kind: if kind.is_empty() {
            forge::DEFAULT_KIND.word().to_owned()
        } else {
            kind.to_owned()
        },
        owner: entry.owner.trim().to_owned(),
        host: entry.host.trim().to_ascii_lowercase(),
        exclude,
    }
}

/// One block of `cfg`'s `[[forge]]`, as the readers resolve it: the forge, and its owner.
fn declared(cfg: &toml::Table) -> Vec<Option<(Forge, String)>> {
    let Some(toml::Value::Array(blocks)) = cfg.get("forge") else {
        return Vec::new();
    };
    blocks
        .iter()
        .map(|block| {
            let block = block.as_table()?;
            let text = |key: &str| block.get(key).and_then(toml::Value::as_str);
            let kind = text("kind")
                .filter(|kind| !kind.is_empty())
                .unwrap_or(forge::DEFAULT_KIND.word());
            let forge = Forge::build(kind, text("host")).ok()?;
            let owner = text("group")
                .filter(|group| !group.is_empty())
                .or_else(|| text("owner"))
                .unwrap_or_default();
            Some((forge, owner.to_owned()))
        })
        .collect()
}

/// Why each field of `entry` may not be added to `cfg`. Empty when it may.
fn check(cfg: &toml::Table, entry: &Clean) -> Vec<FieldRefusal> {
    let mut out = Vec::new();
    let kind = Kind::parse(&entry.kind);
    if kind.is_none()
        && let Err(why) = Forge::build(&entry.kind, None)
    {
        out.push(FieldRefusal { field: "kind", why });
    }
    let host = (!entry.host.is_empty()).then_some(entry.host.as_str());
    if let Some(host) = host
        && !forge::host_ok(host)
        && let Err(why) = Forge::build(forge::DEFAULT_KIND.word(), Some(host))
    {
        out.push(FieldRefusal { field: "host", why });
    }
    let (Some(kind), true) = (kind, out.is_empty()) else {
        return out;
    };
    let new = host.map_or_else(
        || Forge::default_of(kind),
        |host| Forge {
            kind,
            host: host.to_owned(),
        },
    );
    for (at, block) in declared(cfg).into_iter().enumerate() {
        let Some((forge, owner)) = block else {
            continue;
        };
        let n = at + 1;
        if forge.host == new.host && forge.kind != new.kind {
            out.push(FieldRefusal {
                field: "host",
                why: format!(
                    "{} is already a {} forge in [[forge]] block {n}: one host is one forge",
                    new.host,
                    forge.kind.display()
                ),
            });
            break;
        }
        if forge == new && owner == entry.owner {
            out.push(FieldRefusal {
                field: "owner",
                why: format!(
                    "[[forge]] block {n} already lists {} on {}",
                    if owner.is_empty() { "no owner" } else { &owner },
                    new.host
                ),
            });
            break;
        }
    }
    out
}

/// `text` as TOML a form can edit, or the whole write's refusal.
fn document(text: &str) -> Result<toml_edit::DocumentMut, Refusal> {
    text.parse().map_err(|e: toml_edit::TomlError| {
        Refusal::file(vec![format!(
            "charter.toml is not valid TOML ({}), so a form cannot change it — fix it under \
             Edit as TOML",
            crate::shown::short(e.message())
        )])
    })
}

/// `text` with `entry` as a new block after the last one.
fn added(text: &str, entry: &Clean) -> Result<String, Refusal> {
    let mut doc = document(text)?;
    let mut block = toml_edit::Table::new();
    block.insert("kind", toml_edit::value(entry.kind.as_str()));
    for (key, value) in [("owner", &entry.owner), ("host", &entry.host)] {
        if !value.is_empty() {
            block.insert(key, toml_edit::value(value.as_str()));
        }
    }
    if !entry.exclude.is_empty() {
        let list: toml_edit::Array = entry.exclude.iter().map(String::as_str).collect();
        block.insert("exclude", toml_edit::value(list));
    }
    match doc.get_mut("forge") {
        None => {
            let mut blocks = toml_edit::ArrayOfTables::new();
            blocks.push(block);
            doc.insert("forge", toml_edit::Item::ArrayOfTables(blocks));
        }
        Some(toml_edit::Item::ArrayOfTables(blocks)) => {
            // At the last block's place: tables are written in order of place, a tie in the
            // order they are found, so the new block follows the last one and comes before
            // whatever table followed it.
            if let Some(last) = blocks.iter().filter_map(toml_edit::Table::position).max() {
                block.set_position(Some(last));
            }
            blocks.push(block);
        }
        Some(_) => return Err(not_blocks("add")),
    }
    Ok(doc.to_string())
}

fn not_blocks(what: &str) -> Refusal {
    Refusal::file(vec![format!(
        "forge in charter.toml is not written as [[forge]] blocks, so a form cannot {what} one — \
         {what} it under Edit as TOML"
    )])
}

/// `text` without block `index` — and without the `forge` key once no block is left.
fn removed(text: &str, index: usize) -> Result<String, Refusal> {
    let mut doc = document(text)?;
    let gone = || Refusal::file(vec![format!("[[forge]] block {} is not there", index + 1)]);
    match doc.get_mut("forge") {
        Some(toml_edit::Item::ArrayOfTables(blocks)) => {
            if index >= blocks.len() {
                return Err(gone());
            }
            blocks.remove(index);
            if blocks.is_empty() {
                doc.remove("forge");
            }
        }
        None => return Err(gone()),
        Some(_) => return Err(not_blocks("remove")),
    }
    Ok(doc.to_string())
}

/// **Adds `entry` as a new `[[forge]]` block** to `charter.toml` at `root`, read by the caller as
/// `base` — or says, by field and for the whole write, why nothing was written.
pub fn add(root: &Path, base: Option<&str>, entry: &Entry) -> Result<(), Refusal> {
    super::unchanged(root, Which::Shared, base).map_err(Refusal::file)?;
    let text = base.unwrap_or_default();
    let cfg: toml::Table = text.parse().unwrap_or_default();
    let entry = clean(entry);
    let fields = check(&cfg, &entry);
    if !fields.is_empty() {
        return Err(Refusal {
            fields,
            ..Refusal::default()
        });
    }
    let after = added(text, &entry)?;
    super::save(root, Which::Shared, base, &after).map_err(Refusal::file)
}

/// **Removes `[[forge]]` block `index`** (from 0) of `charter.toml` at `root`, read by the caller
/// as `base` — unless something uses it ([`referrers`]), when nothing is written and each user
/// is named.
pub fn remove(root: &Path, base: Option<&str>, index: usize) -> Result<(), Refusal> {
    super::unchanged(root, Which::Shared, base).map_err(Refusal::file)?;
    let text = base.unwrap_or_default();
    let after = removed(text, index)?;
    let referrers = referrers(root, text, &after);
    if !referrers.is_empty() {
        return Err(Refusal {
            referrers,
            ..Refusal::default()
        });
    }
    super::save(root, Which::Shared, base, &after).map_err(Refusal::file)
}

/// Who uses what `before` declares and `after` does not: see the module's rule.
fn referrers(root: &Path, before: &str, after: &str) -> Vec<Referrer> {
    let known = |text: &str| forge::known_in(&text.parse::<toml::Table>().unwrap_or_default());
    let (was, now) = (known(before), known(after));
    let lost: BTreeMap<&String, &Forge> = was
        .iter()
        .filter(|(host, forge)| now.get(*host) != Some(*forge))
        .collect();
    if lost.is_empty() {
        return Vec::new();
    }
    let settings = crate::planesave::Settings::read(root);
    let mut out = Vec::new();
    let catalogued = crate::inventory::load(root, "")
        .map(|doc| crate::inventory::listed(&doc))
        .unwrap_or_default();
    let mut policies = Vec::new();
    for record in &catalogued {
        let text = |key: &str| record.get(key).and_then(serde_json::Value::as_str);
        let (Some(name), Some(url)) = (
            text("name"),
            text("ssh_url")
                .filter(|url| !url.is_empty())
                .or_else(|| text("web_url")),
        ) else {
            continue;
        };
        let host = forge::host_of(url);
        if !lost.contains_key(&host) {
            continue;
        }
        out.push(Referrer {
            what: format!("The repo {name} (inventory/repos.json) is on {host}."),
            group: None,
        });
        let mode = settings.repo(name).mode;
        if mode.value.opens_a_pr() {
            policies.push(Referrer {
                what: format!(
                    "[repos.{name}] mode = \"{}\" in {} opens a request on {host}.",
                    mode.value.as_str(),
                    mode.source.file().unwrap_or("charter.toml"),
                ),
                group: Some(SAVING),
            });
        }
    }
    out.extend(policies);
    let plane = &settings.plane.mode;
    if let Some(mode) = plane.value.filter(|mode| mode.opens_a_pr()) {
        let origin = git::run(root, &["remote", "get-url", "origin"], git::READ)
            .map(|run| forge::host_of(run.out.trim()))
            .unwrap_or_default();
        if lost.contains_key(&origin) {
            out.push(Referrer {
                what: format!(
                    "[plane] mode = \"{}\" in {} opens a request on {origin}, where this \
                     project's origin is.",
                    mode.as_str(),
                    plane.source.file().unwrap_or("charter.toml"),
                ),
                group: Some(SAVING),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests;
