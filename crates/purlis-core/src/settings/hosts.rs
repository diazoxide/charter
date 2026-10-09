//! **A sandbox's hosts, as a Settings collection** (#1341, ADR 0067 §1 and §3 as amended):
//! `[sandbox] hosts` in either file, added and removed through the collection write seam
//! ([`super::collection`]). In `charter.toml` they are the **Project**'s, which every teammate
//! follows; in `charter.local.toml` they are **yours**, on this machine only.
//!
//! An entry is one host as [`crate::sandbox::hosts::Host::parse`] takes it, refused by field
//! (`host`) with its sentence. Every entry the file lists is drawn, a host purlis does not take
//! included (labelled with why), so it can be removed. Nothing uses a host, so a remove is never
//! refused for a user of it.
//!
//! **Yours are confirmed.** A host of `charter.local.toml` reaches your chats only once you
//! added or confirmed it here ([`crate::sandbox::hosts::personal`]): one the file holds that
//! you did not is listed "not yet confirmed", with Confirm ([`confirm`]).
//!
//! **Your own change is no news to you.** A write to the project's hosts made here is recorded
//! as seen on this machine ([`crate::sandbox::local::acknowledge_hosts`]) when nothing else was
//! waiting to be told, so the one-time Notice is a teammate's, not the writer's.

use std::path::Path;

use super::Step;
use super::Which;
use super::collection::{FieldRefusal, Listed, Refusal};
use crate::sandbox::TABLE;
use crate::sandbox::hosts::{Host, KEY};

/// The collection's name in each file, as the window sends it back.
pub fn collection(which: Which) -> &'static str {
    match which {
        Which::Shared => "hosts",
        Which::Local => "myHosts",
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

/// The entries of `text`'s `[sandbox] hosts`, as written: none where the file is not TOML or
/// `hosts` is not a list.
fn written(text: &str) -> Vec<toml::Value> {
    text.parse::<toml::Table>()
        .ok()
        .and_then(|top| top.get(TABLE)?.get(KEY)?.as_array().cloned())
        .unwrap_or_default()
}

/// **Every entry of `text`'s `[sandbox] hosts`**, in file order: the host as purlis spells it,
/// or as written with why it reaches nothing. The identity is `host:<place>:<fingerprint>`, so a
/// remove sent for an entry that moved or changed is refused rather than taking another.
pub fn listed(text: &str) -> Vec<Listed> {
    written(text)
        .iter()
        .enumerate()
        .map(|(at, entry)| {
            let as_written = entry
                .as_str()
                .map_or_else(|| entry.to_string(), str::to_owned);
            let label = match entry.as_str().map(Host::parse) {
                Some(Ok(host)) => host.to_string(),
                Some(Err(why)) => format!("{as_written} (reaches nothing: {why})"),
                None => format!("{as_written} (reaches nothing: a host is written as text)"),
            };
            Listed {
                id: format!("host:{at}:{}", fingerprint(&entry.to_string())),
                label,
                keys: vec![Step::Key(TABLE.to_owned()), Step::Key(KEY.to_owned())],
                values: vec![("host", as_written)],
            }
        })
        .collect()
}

/// `text` as TOML a form can edit, or the whole write's refusal.
fn document(which: Which, text: &str) -> Result<toml_edit::DocumentMut, Refusal> {
    text.parse().map_err(|e: toml_edit::TomlError| {
        Refusal::file(vec![format!(
            "{} is not valid TOML ({}), so a form cannot change it — fix it under Edit as TOML",
            which.file(),
            crate::shown::short(e.message())
        )])
    })
}

fn not_a_list(which: Which) -> Refusal {
    Refusal::file(vec![format!(
        "{TABLE}.{KEY} in {} is not written as a list, so a form cannot change it — change it \
         under Edit as TOML",
        which.file()
    )])
}

/// The hosts list of `doc`, made where there is none.
fn list_in(
    which: Which,
    doc: &mut toml_edit::DocumentMut,
) -> Result<&mut toml_edit::Array, Refusal> {
    let table = doc
        .entry(TABLE)
        .or_insert_with(|| toml_edit::Item::Table(toml_edit::Table::new()));
    let Some(table) = table.as_table_like_mut() else {
        return Err(Refusal::file(vec![format!(
            "{TABLE} in {} is not a table, so a form cannot add a host to it — change it under \
             Edit as TOML",
            which.file()
        )]));
    };
    let list = table
        .entry(KEY)
        .or_insert(toml_edit::value(toml_edit::Array::new()));
    list.as_array_mut().ok_or_else(|| not_a_list(which))
}

/// `text`, as `which`, with `host` last in its `[sandbox] hosts`: every other line kept, and the
/// list written in one spacing however its entries were written.
fn with(which: Which, text: &str, host: &Host) -> Result<String, Refusal> {
    let mut doc = document(which, text)?;
    let list = list_in(which, &mut doc)?;
    list.push(host.to_string());
    list.fmt();
    Ok(doc.to_string())
}

/// `text`, as `which`, without entry `at` of its `[sandbox] hosts`: the key goes with its last
/// host, and this machine's `[sandbox]` table with it once it is empty.
fn without(which: Which, text: &str, at: usize) -> Result<String, Refusal> {
    let mut doc = document(which, text)?;
    let list = list_in(which, &mut doc)?;
    if at >= list.len() {
        return Err(not_a_list(which));
    }
    list.remove(at);
    list.fmt();
    if list.is_empty()
        && let Some(table) = doc.get_mut(TABLE).and_then(|it| it.as_table_like_mut())
    {
        table.remove(KEY);
        if which == Which::Local && table.is_empty() {
            doc.remove(TABLE);
        }
    }
    Ok(doc.to_string())
}

/// **Adds the host `typed`** to `which`'s `[sandbox] hosts` at `root`, read by the caller as
/// `base` — answering the new entry's identity, or why nothing was written: the host's own
/// refusal under the field, one an administrator's policy does not allow at this level
/// ([`crate::sandbox::policy::Locks::refuses`], #1423), one already listed here, or (for yours)
/// one the project already lets every chat reach.
pub fn add(root: &Path, which: Which, base: Option<&str>, typed: &str) -> Result<String, Refusal> {
    added(root, which, base, typed).map_err(|refusal| refusal.named_at(root))
}

fn added(root: &Path, which: Which, base: Option<&str>, typed: &str) -> Result<String, Refusal> {
    super::unchanged(root, which, base).map_err(Refusal::file)?;
    let text = base.unwrap_or_default();
    let field = |why: String| Refusal {
        fields: vec![FieldRefusal { field: "host", why }],
        ..Refusal::default()
    };
    let host = Host::parse(typed).map_err(field)?;
    // An administrator's policy (#1423): a host it locks out at this level would be written and
    // then reach nothing, so it is refused here, with the policy's own sentence.
    if let Some(why) =
        crate::sandbox::policy::Locks::of(root).refuses(&crate::sandbox::hosts::Granted {
            host: host.clone(),
            level: match which {
                Which::Shared => crate::sandbox::hosts::Level::Project,
                Which::Local => crate::sandbox::hosts::Level::You,
            },
        })
    {
        return Err(field(why));
    }
    let here = |text: &str| crate::sandbox::hosts::of_table(text.parse().ok().as_ref());
    if here(text).contains(&host) {
        let confirmed = crate::sandbox::local::confirmed_hosts(root);
        return Err(field(
            if which == Which::Local && !confirmed.contains(&host.to_string()) {
                format!(
                    "{host} is already listed here, not yet confirmed: press Confirm beside it."
                )
            } else {
                format!("{host} is already listed here.")
            },
        ));
    }
    if which == Which::Local {
        let project = super::on_disk(root, Which::Shared)
            .map(|(_, text)| here(&text))
            .unwrap_or_default();
        if project.contains(&host) {
            return Err(field(format!(
                "{host} is already one of the project's hosts, so every chat here reaches it."
            )));
        }
    }
    let pending = which == Which::Shared && crate::sandbox::local::hosts_changed(root).is_some();
    let after = with(which, text, &host)?;
    super::save(root, which, base, &after).map_err(Refusal::file)?;
    seen_by_you(root, which, pending, &after);
    // Added here, by you: confirmed on this machine.
    if which == Which::Local {
        crate::sandbox::local::confirm_host(root, &host.to_string()).map_err(|e| {
            Refusal::file(vec![format!(
                "{host} was written, and could not be recorded as yours ({}), so it reaches \
                 nothing yet. Press Confirm beside it.",
                crate::shown::short(&e.to_string())
            )])
        })?;
    }
    listed(&after)
        .pop()
        .map(|one| one.id)
        .ok_or_else(|| Refusal::file(vec!["the host was written, and is not read back".into()]))
}

/// **Removes the entry called `id`** ([`listed`]) from `which`'s `[sandbox] hosts` at `root`,
/// read by the caller as `base`, answering it as written: what an Undo adds back. The key goes
/// when its last host does, and this machine's `[sandbox]` table with it once it is empty.
pub fn remove(root: &Path, which: Which, base: Option<&str>, id: &str) -> Result<String, Refusal> {
    removed(root, which, base, id).map_err(|refusal| refusal.named_at(root))
}

fn removed(root: &Path, which: Which, base: Option<&str>, id: &str) -> Result<String, Refusal> {
    super::unchanged(root, which, base).map_err(Refusal::file)?;
    let text = base.unwrap_or_default();
    let Some((at, entry)) = listed(text)
        .into_iter()
        .enumerate()
        .find(|(_, one)| one.id == id)
    else {
        return Err(Refusal::file(vec![format!(
            "That host is not in {} as it was shown, so nothing was removed. Read the file \
             again, then remove it again.",
            which.file()
        )]));
    };
    let pending = which == Which::Shared && crate::sandbox::local::hosts_changed(root).is_some();
    let after = without(which, text, at)?;
    super::save(root, which, base, &after).map_err(Refusal::file)?;
    seen_by_you(root, which, pending, &after);
    let written = entry
        .values
        .into_iter()
        .find(|(field, _)| *field == "host")
        .map(|(_, value)| value)
        .unwrap_or_default();
    if which == Which::Local
        && let Ok(host) = Host::parse(&written)
        && !here_hosts(&after).contains(&host)
    {
        // Best effort: a host left on the record reaches nothing while the file lacks it.
        let _ = crate::sandbox::local::unconfirm_host(root, &host.to_string());
    }
    Ok(written)
}

fn here_hosts(text: &str) -> Vec<Host> {
    crate::sandbox::hosts::of_table(text.parse().ok().as_ref())
}

/// **Confirms your own host called `id`** ([`listed`]) in `charter.local.toml` at `root`, read
/// by the caller as `base` (#1341): from now on it reaches your chats here. Only Settings calls
/// this; a host that is not one is refused with why, and nothing is written to the file.
pub fn confirm(root: &Path, base: Option<&str>, id: &str) -> Result<String, Refusal> {
    confirmed(root, base, id).map_err(|refusal| refusal.named_at(root))
}

fn confirmed(root: &Path, base: Option<&str>, id: &str) -> Result<String, Refusal> {
    super::unchanged(root, Which::Local, base).map_err(Refusal::file)?;
    let text = base.unwrap_or_default();
    let Some(entry) = listed(text).into_iter().find(|one| one.id == id) else {
        return Err(Refusal::file(vec![format!(
            "That host is not in {} as it was shown, so nothing was confirmed. Read the file \
             again, then confirm it again.",
            crate::profiles::LOCAL_FILE
        )]));
    };
    let written = entry
        .values
        .iter()
        .find(|(field, _)| *field == "host")
        .map(|(_, value)| value.clone())
        .unwrap_or_default();
    let host = Host::parse(&written).map_err(|why| Refusal::file(vec![why]))?;
    crate::sandbox::local::confirm_host(root, &host.to_string()).map_err(|e| {
        Refusal::file(vec![format!(
            "{host} could not be recorded as yours ({}), so nothing was confirmed.",
            crate::shown::short(&e.to_string())
        )])
    })?;
    Ok(entry.id)
}

/// [`listed`], with your own hosts that you have not confirmed on this machine labelled so,
/// and each entry's `confirmed` value (`yes` or `no`) for the window's Confirm.
pub fn listed_at(root: &Path, which: Which, text: &str) -> Vec<Listed> {
    let mut entries = listed(text);
    if which != Which::Local {
        return entries;
    }
    let confirmed = crate::sandbox::local::confirmed_hosts(root);
    for one in &mut entries {
        let written = one
            .values
            .iter()
            .find(|(field, _)| *field == "host")
            .map(|(_, value)| value.clone())
            .unwrap_or_default();
        let mine = Host::parse(&written).is_ok_and(|host| confirmed.contains(&host.to_string()));
        if !mine && Host::parse(&written).is_ok() {
            one.label = format!("{} (not yet confirmed)", one.label);
        }
        one.values
            .push(("confirmed", if mine { "yes" } else { "no" }.to_owned()));
    }
    entries
}

/// A change to the project's hosts made here is one this machine has seen — unless another was
/// still waiting to be told (`pending`), which the Notice then tells with it. Recorded as
/// `written` says them, never as a later read of the disk
/// ([`crate::sandbox::local::hosts_seen_by_you`]).
fn seen_by_you(root: &Path, which: Which, pending: bool, written: &str) {
    if which == Which::Shared {
        crate::sandbox::local::hosts_seen_by_you(root, pending, written);
    }
}

/// One refusal as a sentence: each field's, then the file's.
fn said(refusal: Refusal) -> String {
    refusal
        .fields
        .into_iter()
        .map(|one| one.why)
        .chain(refusal.file)
        .collect::<Vec<_>>()
        .join(" ")
}

/// The text of `which` at `root` as it is on disk now: what a grant from a Notice writes
/// against (`None`: not there).
fn now(root: &Path, which: Which) -> Result<Option<String>, String> {
    super::on_disk(root, which).map(|(there, text)| there.then_some(text))
}

/// **Grants `host` from a block's Notice** (#1342): [`add`] against `which` as it is on disk
/// now, so the project's hosts or yours gain it by the same rules Settings' Add keeps (yours
/// confirmed on this machine as it is added). Answers why not, in a sentence.
pub fn grant(root: &Path, which: Which, host: &Host) -> Result<(), String> {
    let base = now(root, which)?;
    add(root, which, base.as_deref(), &host.to_string())
        .map(|_| ())
        .map_err(said)
}

/// **Revokes `host`** at `which`'s level (#1348): [`remove`] of every entry naming it, against
/// the file as it is on disk now. Answers why not, in a sentence.
pub fn revoke(root: &Path, which: Which, host: &Host) -> Result<(), String> {
    loop {
        let base = now(root, which)?;
        let text = base.clone().unwrap_or_default();
        let Some(entry) = listed(&text).into_iter().find(|one| {
            one.values.iter().any(|(field, value)| {
                *field == "host" && Host::parse(value).ok().as_ref() == Some(host)
            })
        }) else {
            return Ok(());
        };
        remove(root, which, base.as_deref(), &entry.id).map_err(said)?;
    }
}

#[cfg(test)]
#[path = "hosts_tests.rs"]
mod tests;
