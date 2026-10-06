//! **Harness profiles, as a Settings collection** (ST-4, #1236): the `[harness.<name>]` tables
//! of the project's local settings file, added, removed and renamed through the collection
//! write seam ([`super::collection`]).
//!
//! A profile is what [`crate::profiles`] reads: a `kind`, a `command` (a list of arguments, no
//! shell) and an optional `env`. The Add form asks for the name, the kind and the command; the
//! environment is set on the profile's own page once it is there, one variable per line.
//!
//! **One set of rules.** A new entry is checked by the loader's own rules
//! ([`crate::profiles::refusals_of`] and [`crate::profiles::launch_refusals`]), each said under
//! the field it is about, in the words the next read would use — plus the two only a writer can
//! break: a name the file already has, and a value shaped like a secret, which a profile never
//! holds (V91m) and which is refused under its field with a pointer to the vault.
//!
//! **What uses a profile** ([`referrers`]) is a `[harness] default` that names it, in either
//! settings file — but only when the name would then name nothing: a table that replaces a
//! built-in (`[harness.claude]`) leaves the built-in standing, so the default still starts a
//! chat. A chat's record names its profile too, and is not a user here: a reopened chat whose
//! profile is gone is skipped by name (ADR 0022), never started on another.
//!
//! **Rename** (V91k) is allowed only while nothing uses the profile, and is refused naming who
//! does, as a remove is; renaming everywhere at once is a follow-up. A renamed profile is one no
//! approval names, so its first run asks again, as a new one's does ([`crate::profiletrust`]).

use std::path::Path;

use super::Step;
use super::Which;
use super::collection::{FieldRefusal, Listed, Referrer, Refusal};
use crate::doctor::SettingsGroup;
use crate::profiles;

/// The table every profile is under.
const HARNESS: &str = "harness";

/// The one key under `[harness]` that names the default rather than declaring a profile.
const DEFAULT: &str = "default";

/// One profile, as the Add form sends it: each field as typed, the command one argument per
/// entry.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub kind: String,
    pub command: Vec<String>,
}

/// The entry as it is written: trimmed, the command without blank arguments.
struct Clean {
    name: String,
    kind: String,
    command: Vec<String>,
}

fn clean(entry: &Entry) -> Clean {
    Clean {
        name: entry.name.trim().to_owned(),
        kind: entry.kind.trim().to_owned(),
        command: entry
            .command
            .iter()
            .map(|one| one.trim())
            .filter(|one| !one.is_empty())
            .map(str::to_owned)
            .collect(),
    }
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

/// The profile tables of `cfg`, in file order: each name and its table.
fn tables(cfg: &toml::Table) -> Vec<(&String, &toml::Table)> {
    let Some(toml::Value::Table(harness)) = cfg.get(HARNESS) else {
        return Vec::new();
    };
    harness
        .iter()
        .filter(|(name, _)| name.as_str() != DEFAULT)
        .filter_map(|(name, value)| value.as_table().map(|table| (name, table)))
        .collect()
}

/// **Every `[harness.<name>]` table of `text`, as the collection lists it** — in file order,
/// each with its identity, its name as its label and the Add form's values that would write it
/// again. A table the loader refuses is listed too: it is in the file, and Remove is how it goes.
/// `text` that is not TOML lists nothing.
///
/// **The identity** is opaque to the window and different for a table that was renamed or
/// changed, so a write sent for a profile that is no longer what it was is refused rather than
/// made to another.
pub fn listed(text: &str) -> Vec<Listed> {
    let Ok(cfg) = text.parse::<toml::Table>() else {
        return Vec::new();
    };
    tables(&cfg)
        .into_iter()
        .map(|(name, table)| {
            let text = |key: &str| {
                table
                    .get(key)
                    .and_then(toml::Value::as_str)
                    .unwrap_or_default()
                    .to_owned()
            };
            let command = table
                .get("command")
                .and_then(toml::Value::as_array)
                .map(|words| {
                    words
                        .iter()
                        .map(|w| w.as_str().unwrap_or_default())
                        .collect::<Vec<_>>()
                        .join("\n")
                })
                .unwrap_or_default();
            Listed {
                id: format!(
                    "profile:{}:{}",
                    fingerprint(name),
                    fingerprint(&table.to_string())
                ),
                label: crate::shown::short(name),
                keys: vec![Step::Key(HARNESS.to_owned()), Step::Key(name.clone())],
                values: vec![
                    ("name", name.clone()),
                    ("kind", text("kind")),
                    ("command", command),
                ],
            }
        })
        .collect()
}

/// The local file's name as the plane at `root` has it, for a sentence.
fn local_name(root: &Path) -> &'static str {
    Which::Local.file_at(root)
}

/// `text` as TOML a form can edit, or the whole write's refusal.
fn document(root: &Path, text: &str) -> Result<toml_edit::DocumentMut, Refusal> {
    text.parse().map_err(|e: toml_edit::TomlError| {
        Refusal::file(vec![format!(
            "{} is not valid TOML ({}), so a form cannot change it — fix it under Edit as TOML",
            local_name(root),
            crate::shown::short(e.message())
        )])
    })
}

/// The `[harness]` table of `doc`, made when there is none — or the refusal when `harness` is
/// written some other way (an inline table, a value).
fn harness_of<'d>(
    root: &Path,
    doc: &'d mut toml_edit::DocumentMut,
    what: &str,
) -> Result<&'d mut toml_edit::Table, Refusal> {
    if doc.get(HARNESS).is_none() {
        let mut table = toml_edit::Table::new();
        table.set_implicit(true);
        doc.insert(HARNESS, toml_edit::Item::Table(table));
    }
    match doc.get_mut(HARNESS) {
        Some(toml_edit::Item::Table(table)) => Ok(table),
        _ => Err(Refusal::file(vec![format!(
            "harness in {} is not written as [harness.<name>] tables, so a form cannot {what} \
             a profile — {what} it under Edit as TOML",
            local_name(root)
        )])),
    }
}

/// A value pasted into `field` that is shaped like a secret (V91m), refused by the secret's
/// KIND and never its text: every other refusal of that field would quote it back.
fn secret_in(field: &'static str, value: &str) -> Option<FieldRefusal> {
    crate::secretshape::secret_kind(value).map(|kind| FieldRefusal {
        field,
        why: format!(
            "This looks like a secret ({kind}), and a profile never holds one: anything a \
             profile gives its harness reaches the model's own shell. Keep the secret in a \
             vault, and log in inside the harness."
        ),
    })
}

/// Why `name` may not be the name of a profile the file does not yet hold: the loader's name
/// rules, then a name the file already has.
fn name_refusals(cfg: &toml::Table, name: &str, but: Option<&str>) -> Vec<FieldRefusal> {
    if let Some(secret) = secret_in("name", name) {
        return vec![secret];
    }
    let probe = toml::Value::Table(
        [
            (
                "kind".to_owned(),
                toml::Value::from(profiles::KINDS[0].word),
            ),
            (
                "command".to_owned(),
                toml::Value::Array(vec![toml::Value::from(profiles::KINDS[0].binary)]),
            ),
        ]
        .into_iter()
        .collect(),
    );
    let mut out: Vec<FieldRefusal> = profiles::refusals_of(name, &probe, &Default::default())
        .into_iter()
        .chain(profiles::launch_refusals(name, &[]))
        .filter(|(field, _)| *field == "name")
        .map(|(field, why)| FieldRefusal { field, why })
        .collect();
    if out.is_empty() && tables(cfg).iter().any(|(held, _)| held.as_str() == name) {
        out.push(FieldRefusal {
            field: "name",
            why: match but {
                Some(was) if was == name => format!("The profile is already called {name}."),
                _ => format!(
                    "[harness.{}] is already a profile in this file: a name is one profile. \
                     Pick another name, or change that profile on its own page.",
                    crate::shown::short(name)
                ),
            },
        });
    }
    out
}

/// Why each field of `entry` may not be added to `cfg`. Empty when it may.
///
/// A name or a kind shaped like a secret is refused by its kind alone, first: the rules' own
/// sentences quote the name (every one of them) and the kind, and a secret is never said back.
fn check(root: &Path, cfg: &toml::Table, entry: &Clean) -> Vec<FieldRefusal> {
    let secrets: Vec<FieldRefusal> = [("name", &entry.name), ("kind", &entry.kind)]
        .into_iter()
        .filter_map(|(field, value)| secret_in(field, value))
        .collect();
    if secrets.iter().any(|one| one.field == "name") {
        return secrets;
    }
    let mut out = name_refusals(cfg, &entry.name, None);
    if entry.name.is_empty() {
        out = vec![FieldRefusal {
            field: "name",
            why: "A profile needs a name: letters, digits, '_' and '-'.".to_owned(),
        }];
    }
    let table = toml::Value::Table(
        [
            ("kind".to_owned(), toml::Value::from(entry.kind.as_str())),
            (
                "command".to_owned(),
                toml::Value::Array(
                    entry
                        .command
                        .iter()
                        .map(|w| toml::Value::from(w.as_str()))
                        .collect(),
                ),
            ),
        ]
        .into_iter()
        .collect(),
    );
    let declared = crate::harness_declaration::read(root);
    out.extend(
        profiles::refusals_of(&entry.name, &table, &declared)
            .into_iter()
            .chain(profiles::launch_refusals(&entry.name, &entry.command))
            .filter(|(field, _)| *field != "name")
            .filter(|(field, _)| *field != "kind" || secrets.is_empty())
            .map(|(field, why)| FieldRefusal { field, why }),
    );
    out.extend(secrets);
    // A secret pasted where an argument goes: said by its kind, never its text.
    if !out.iter().any(|one| one.field == "command")
        && let Some(secret) = entry
            .command
            .iter()
            .find_map(|word| secret_in("command", word))
    {
        out.push(secret);
    }
    out
}

/// `text` with `entry` as a new `[harness.<name>]` table after the last profile.
fn added(root: &Path, text: &str, entry: &Clean) -> Result<String, Refusal> {
    let mut doc = document(root, text)?;
    let harness = harness_of(root, &mut doc, "add")?;
    let mut table = toml_edit::Table::new();
    table.insert("kind", toml_edit::value(entry.kind.as_str()));
    let command: toml_edit::Array = entry.command.iter().map(String::as_str).collect();
    table.insert("command", toml_edit::value(command));
    // At the last profile's place, or `[harness]`'s own: tables are written in order of place, a
    // tie in the order they are found, so the new one follows the profiles before whatever
    // table followed them.
    let last = harness
        .iter()
        .filter_map(|(_, item)| item.as_table().and_then(toml_edit::Table::position))
        .chain(harness.position())
        .max();
    if let Some(last) = last {
        table.set_position(Some(last));
        // A blank line before it, as between the tables it joins.
        table.decor_mut().set_prefix("\n");
    }
    harness.insert(&entry.name, toml_edit::Item::Table(table));
    Ok(doc.to_string())
}

/// `text` without the profile `name`, and without an implicit `[harness]` it leaves empty.
fn removed(root: &Path, text: &str, name: &str) -> Result<String, Refusal> {
    let mut doc = document(root, text)?;
    let harness = harness_of(root, &mut doc, "remove")?;
    if harness.remove(name).is_none() {
        return Err(gone(name));
    }
    if harness.is_empty() && harness.is_implicit() {
        doc.remove(HARNESS);
    }
    Ok(doc.to_string())
}

/// `text` with the profile `from` called `to`, where it was, as it was written: at its place
/// among `[harness]`'s keys (which is where an inline profile is written), its table at its
/// place among the file's tables, and its name quoted as it was quoted — so a rename back gives
/// the text it started from.
fn renamed(root: &Path, text: &str, from: &str, to: &str) -> Result<String, Refusal> {
    let mut doc = document(root, text)?;
    let harness = harness_of(root, &mut doc, "rename")?;
    if !harness.contains_key(from) {
        return Err(gone(from));
    }
    // Every key out and back in its order: a table keeps no place for a key put in later.
    let names: Vec<String> = harness.iter().map(|(name, _)| name.to_owned()).collect();
    let mut kept = Vec::new();
    for name in &names {
        if let Some(one) = harness.remove_entry(name) {
            kept.push(one);
        }
    }
    for (key, item) in kept {
        if key.get() == from {
            harness.insert_formatted(&spelled_as(&key, to), item);
        } else {
            harness.insert_formatted(&key, item);
        }
    }
    Ok(doc.to_string())
}

/// `to` as a key spelled as `was` is: quoted the same way, with the same space around it.
fn spelled_as(was: &toml_edit::Key, to: &str) -> toml_edit::Key {
    let raw = was
        .as_repr()
        .and_then(|repr| repr.as_raw().as_str())
        .unwrap_or_default();
    let mut key = toml_edit::Key::new(to);
    // `to` passed the name rules (letters, digits, '_' and '-'), so quoting it is wrapping it.
    if let Some(quote) = raw.chars().next().filter(|c| *c == '"' || *c == '\'')
        && let Ok(quoted) = format!("{quote}{to}{quote}").parse::<toml_edit::Key>()
    {
        key = quoted;
    }
    *key.leaf_decor_mut() = was.leaf_decor().clone();
    *key.dotted_decor_mut() = was.dotted_decor().clone();
    key
}

fn gone(name: &str) -> Refusal {
    Refusal::file(vec![format!(
        "[harness.{}] is not there",
        crate::shown::short(name)
    )])
}

/// The profile `id` names in `text`: its key, and its listing.
fn found(text: &str, id: &str) -> Result<(String, Listed), Refusal> {
    listed(text)
        .into_iter()
        .find(|one| one.id == id)
        .and_then(|one| match one.keys.get(1) {
            Some(Step::Key(name)) => Some((name.clone(), one)),
            _ => None,
        })
        .ok_or_else(|| {
            Refusal::file(vec![
                "That profile is not in the file as it was shown, so nothing was changed. Read \
                 the file again, then try again."
                    .to_owned(),
            ])
        })
}

/// **Adds `entry` as a new `[harness.<name>]` table** to the local settings file at `root`,
/// read by the caller as `base` — answering the new entry's identity ([`listed`]), or saying,
/// by field and for the whole write, why nothing was written. Its first run still asks for
/// approval, as any profile nothing has approved does.
pub fn add(root: &Path, base: Option<&str>, entry: &Entry) -> Result<String, Refusal> {
    super::unchanged(root, Which::Local, base).map_err(Refusal::file)?;
    let text = base.unwrap_or_default();
    let cfg: toml::Table = text.parse().unwrap_or_default();
    let entry = clean(entry);
    let fields = check(root, &cfg, &entry);
    if !fields.is_empty() {
        return Err(Refusal {
            fields,
            ..Refusal::default()
        });
    }
    let after = added(root, text, &entry)?;
    super::save(root, Which::Local, base, &after).map_err(Refusal::file)?;
    listed(&after)
        .into_iter()
        .find(|one| one.keys.get(1) == Some(&Step::Key(entry.name.clone())))
        .map(|one| one.id)
        .ok_or_else(|| Refusal::file(vec!["the profile was written, and is not read back".into()]))
}

/// **Removes the profile called `id`** ([`listed`]) from the local settings file at `root`,
/// read by the caller as `base` — unless something uses it ([`referrers`]), when nothing is
/// written and each user is named. Answers the profile as the Add form would write it again.
pub fn remove(root: &Path, base: Option<&str>, id: &str) -> Result<Entry, Refusal> {
    super::unchanged(root, Which::Local, base).map_err(Refusal::file)?;
    let text = base.unwrap_or_default();
    let (name, entry) = found(text, id)?;
    let after = removed(root, text, &name)?;
    let users = referrers(root, &name, text, &after);
    if !users.is_empty() {
        return Err(Refusal {
            referrers: users,
            ..Refusal::default()
        });
    }
    super::save(root, Which::Local, base, &after).map_err(Refusal::file)?;
    let value = |field: &str| {
        entry
            .values
            .iter()
            .find(|(key, _)| *key == field)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    Ok(Entry {
        name: value("name"),
        kind: value("kind"),
        command: value("command").lines().map(str::to_owned).collect(),
    })
}

/// **Renames the profile called `id`** ([`listed`]) to `to` in the local settings file at
/// `root`, read by the caller as `base`, keeping its place, keys and comments (V91k) — refused
/// by field for a name the rules refuse, and naming each user while something uses it.
/// Answers the renamed entry's identity, which is what its Undo renames back.
pub fn rename(root: &Path, base: Option<&str>, id: &str, to: &str) -> Result<String, Refusal> {
    super::unchanged(root, Which::Local, base).map_err(Refusal::file)?;
    let text = base.unwrap_or_default();
    let (name, _) = found(text, id)?;
    let to = to.trim();
    let cfg: toml::Table = text.parse().unwrap_or_default();
    let fields = name_refusals(&cfg, to, Some(&name));
    if !fields.is_empty() {
        return Err(Refusal {
            fields,
            ..Refusal::default()
        });
    }
    let after = renamed(root, text, &name, to)?;
    let users = referrers(root, &name, text, &after);
    if !users.is_empty() {
        return Err(Refusal {
            referrers: users,
            ..Refusal::default()
        });
    }
    super::save(root, Which::Local, base, &after).map_err(Refusal::file)?;
    listed(&after)
        .into_iter()
        .find(|one| one.keys.get(1) == Some(&Step::Key(to.to_owned())))
        .map(|one| one.id)
        .ok_or_else(|| Refusal::file(vec!["the profile was renamed, and is not read back".into()]))
}

/// Who uses the profile `name` that the local file `before` declares and `after` does not:
/// each `[harness] default` naming it, when `after` leaves that name no profile at all.
fn referrers(root: &Path, name: &str, before: &str, after: &str) -> Vec<Referrer> {
    let committed = std::fs::read_to_string(crate::names::manifest(root)).ok();
    let declared = crate::harness_declaration::read(root);
    let set = |local: &str| {
        profiles::derive_declared(committed.as_deref(), Ok(Some(local.to_owned())), &declared)
    };
    if set(before).get(name).is_none() || set(after).get(name).is_some() {
        return Vec::new();
    }
    let names_it = |text: Option<&str>| {
        text.and_then(|text| text.parse::<toml::Table>().ok())
            .and_then(|cfg| {
                cfg.get(HARNESS)?
                    .as_table()?
                    .get(DEFAULT)?
                    .as_str()
                    .map(str::to_owned)
            })
            .is_some_and(|named| named == name)
    };
    let mut out = Vec::new();
    for (which, text) in [
        (Which::Shared, committed.as_deref()),
        (Which::Local, Some(before)),
    ] {
        if names_it(text) {
            out.push(Referrer {
                what: format!(
                    "[harness] default = \"{}\" in {} starts new chats on it.",
                    crate::shown::short(name),
                    which.file_at(root)
                ),
                group: Some(SettingsGroup::Harness),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests;
