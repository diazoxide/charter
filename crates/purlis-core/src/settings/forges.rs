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

use super::Step;
use super::Which;
use super::collection::{FieldRefusal, Listed, Referrer, Refusal};
use crate::doctor::SettingsGroup;
use crate::forge::{self, Forge, Kind};
use crate::worktree::git;

/// The Settings group a save policy is changed in: the deep link a referrer carries.
const SAVING: SettingsGroup = SettingsGroup::Saving;

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
            return out;
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
    // A kind's own host is known whatever the blocks say: a block of another kind there would
    // retype it for every repo on it.
    if out.is_empty()
        && let Some(known) = forge::known_in(cfg).get(&new.host)
        && known.kind != new.kind
    {
        out.push(FieldRefusal {
            field: "host",
            why: format!(
                "{} is {}'s own host: one host is one forge",
                new.host,
                known.kind.display()
            ),
        });
    }
    out
}

/// A stable fingerprint of `text` (FNV-1a), for an entry's identity.
fn fingerprint(text: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{hash:016x}")
}

/// **Every `[[forge]]` block of `text`, as the collection lists it** — in file order, each with
/// its identity, its label and the Add form's values that would write it again. `text` that is
/// not TOML, or whose `forge` is not a list of tables, lists nothing.
///
/// **The identity** is `forge:<place>:<fingerprint of the block>` (D-ST3-j): opaque to the
/// window, and different for a block that moved or changed, so a remove sent for a block that is
/// no longer where and what it was is refused rather than taking its neighbour.
pub fn listed(text: &str) -> Vec<Listed> {
    let Ok(cfg) = text.parse::<toml::Table>() else {
        return Vec::new();
    };
    let Some(toml::Value::Array(blocks)) = cfg.get("forge") else {
        return Vec::new();
    };
    blocks
        .iter()
        .enumerate()
        .filter_map(|(at, block)| {
            let table = block.as_table()?;
            let text = |key: &str| {
                table
                    .get(key)
                    .and_then(toml::Value::as_str)
                    .unwrap_or_default()
                    .trim()
                    .to_owned()
            };
            let kind = Some(text("kind"))
                .filter(|kind| !kind.is_empty())
                .unwrap_or_else(|| forge::DEFAULT_KIND.word().to_owned());
            let owner = Some(text("group"))
                .filter(|group| !group.is_empty())
                .unwrap_or_else(|| text("owner"));
            let host = text("host");
            let shown_host = Kind::parse(&kind)
                .filter(|_| host.is_empty())
                .map_or_else(|| host.clone(), |kind| kind.default_host().to_owned());
            let exclude = forge::exclude_of(&cfg, at).join("\n");
            let n = at + 1;
            Some(Listed {
                id: format!("forge:{at}:{}", fingerprint(&block.to_string())),
                label: if owner.is_empty() {
                    format!("Forge {n}: {kind} at {shown_host}")
                } else {
                    format!("Forge {n}: {kind} {owner} at {shown_host}")
                },
                keys: vec![Step::Key("forge".to_owned()), Step::Index(at)],
                values: vec![
                    ("kind", kind),
                    ("owner", owner),
                    ("host", host),
                    ("exclude", exclude),
                ],
            })
        })
        .collect()
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
/// `base` — answering the new entry's identity ([`listed`]), or saying, by field and for the
/// whole write, why nothing was written.
pub fn add(root: &Path, base: Option<&str>, entry: &Entry) -> Result<String, Refusal> {
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
    super::save(root, Which::Shared, base, &after).map_err(Refusal::file)?;
    listed(&after)
        .pop()
        .map(|one| one.id)
        .ok_or_else(|| Refusal::file(vec!["the forge was written, and is not read back".into()]))
}

/// **Removes the `[[forge]]` block called `id`** ([`listed`]) from `charter.toml` at `root`, read
/// by the caller as `base` — unless something uses it ([`referrers`]), when nothing is written
/// and each user is named. Answers the block as the Add form would write it again: what an
/// Undo adds back.
pub fn remove(root: &Path, base: Option<&str>, id: &str) -> Result<Entry, Refusal> {
    super::unchanged(root, Which::Shared, base).map_err(Refusal::file)?;
    let text = base.unwrap_or_default();
    let Some((index, entry)) = listed(text)
        .into_iter()
        .enumerate()
        .find(|(_, one)| one.id == id)
    else {
        return Err(Refusal::file(vec![
            "That forge is not in charter.toml as it was shown, so nothing was removed. Read \
             the file again, then remove it again."
                .to_owned(),
        ]));
    };
    let index = match entry.keys.get(1) {
        Some(Step::Index(at)) => *at,
        _ => index,
    };
    let after = removed(text, index)?;
    let referrers = referrers(root, text, &after);
    if !referrers.is_empty() {
        return Err(Refusal {
            referrers,
            ..Refusal::default()
        });
    }
    super::save(root, Which::Shared, base, &after).map_err(Refusal::file)?;
    let value = |field: &str| {
        entry
            .values
            .iter()
            .find(|(key, _)| *key == field)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    Ok(Entry {
        kind: value("kind"),
        owner: value("owner"),
        host: value("host"),
        exclude: value("exclude").lines().map(str::to_owned).collect(),
    })
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
            follows: false,
        });
        let mode = settings.repo(name).mode;
        if mode.value.opens_a_pr() {
            policies.push(Referrer {
                what: format!(
                    "[repos.{name}] mode = \"{}\" in {} opens a request on {host}.",
                    mode.value.as_str(),
                    mode.source
                        .file()
                        .unwrap_or(crate::profiles::COMMITTED_FILE),
                ),
                group: Some(SAVING),
                follows: false,
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
                    plane
                        .source
                        .file()
                        .unwrap_or(crate::profiles::COMMITTED_FILE),
                ),
                group: Some(SAVING),
                follows: false,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests;
