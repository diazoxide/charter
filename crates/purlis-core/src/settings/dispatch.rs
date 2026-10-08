//! **The project's dispatch grants, as the committed file keeps them** (#1437, spec #1434):
//! `[dispatch.grants]`, each asking persona with the personas its chats may dispatch to.
//! Written through the seam a project host is ([`super::save`]): against the file as it is on
//! disk, every other line kept, and refused where the file changed underneath or would no
//! longer read.
//!
//! Only the grant Notice's "Allow for everyone in this project" and Settings' Revoke write it,
//! on a person's press. A chat never does: the file is a later-code name a sandboxed chat is
//! denied writing, and a brokered write refuses any change under `[dispatch]`
//! ([`crate::brokered::guard`]).
//!
//! **Your own change is no news to you, and is in force at once**: a pair granted here is
//! acknowledged on this machine as it is written, and one revoked here is taken off what was
//! acknowledged, so the one-time Notice is a teammate's ([`crate::dispatchgrant::changed`]).
//! A teammate's pair covers nothing here until it is allowed here (D-1437-R1).

use std::path::Path;

use super::Which;
use crate::dispatchgrant::{KEY, Pair, TABLE};

/// Why a form cannot change `[dispatch.grants]` as the file writes it.
fn not_editable(what: &str) -> String {
    format!(
        "{what} in {} is not written in a form purlis edits, so nothing was changed. Change it \
         under Edit as TOML.",
        Which::Shared.file()
    )
}

/// `text` as TOML a form can edit.
fn document(text: &str) -> Result<toml_edit::DocumentMut, String> {
    text.parse().map_err(|e: toml_edit::TomlError| {
        format!(
            "{} is not valid TOML ({}), so nothing was changed. Fix it under Edit as TOML.",
            Which::Shared.file(),
            crate::shown::short(e.message())
        )
    })
}

/// **`text`, the project file, with `pair` granted**: `[dispatch.grants]` made where there is
/// none, the target added last to the asking persona's list, and every other line kept. The
/// same text where it is granted already.
pub fn with(text: &str, pair: &Pair) -> Result<String, String> {
    with_target(text, &pair.asking, &pair.target)
}

/// **`text` with `asking` granted any persona** (#1503): `"*"` added to its list, as [`with`]
/// adds a target. Settings' explicit grant, and nothing a Notice's answer writes.
pub fn with_any(text: &str, asking: &str) -> Result<String, String> {
    with_target(text, &any_asking(asking)?, crate::dispatchgrant::ANY)
}

/// **`text` without `asking`'s grant of any persona**: as [`without`] takes a target out.
pub fn without_any(text: &str, asking: &str) -> Result<String, String> {
    without_target(text, &any_asking(asking)?, crate::dispatchgrant::ANY)
}

/// `asking` as a persona's name, or why "any persona" is not kept for it.
fn any_asking(asking: &str) -> Result<String, String> {
    if crate::personas::valid_name(asking) {
        Ok(asking.to_owned())
    } else {
        Err(format!(
            "{} is not a persona's name, so purlis keeps no dispatch grant for it.",
            crate::shown::short(asking)
        ))
    }
}

/// [`with`], for a target as the file spells it.
fn with_target(text: &str, asking: &str, target: &str) -> Result<String, String> {
    let mut doc = document(text)?;
    let dispatch = doc.entry(TABLE).or_insert_with(|| {
        let mut table = toml_edit::Table::new();
        // Written as `[dispatch.grants]`, with no bare `[dispatch]` line above it.
        table.set_implicit(true);
        toml_edit::Item::Table(table)
    });
    let dispatch = dispatch
        .as_table_like_mut()
        .ok_or_else(|| not_editable(TABLE))?;
    let grants = dispatch
        .entry(KEY)
        .or_insert_with(|| toml_edit::Item::Table(toml_edit::Table::new()));
    let grants = grants
        .as_table_like_mut()
        .ok_or_else(|| not_editable(&format!("{TABLE}.{KEY}")))?;
    let targets = grants
        .entry(asking)
        .or_insert(toml_edit::value(toml_edit::Array::new()));
    let targets = targets
        .as_array_mut()
        .ok_or_else(|| not_editable(&format!("{TABLE}.{KEY}.{asking}")))?;
    if !targets.iter().any(|one| one.as_str() == Some(target)) {
        targets.push(target);
        targets.fmt();
    }
    Ok(doc.to_string())
}

/// **`text` without `pair`**: every entry naming the target goes from the asking persona's
/// list, the persona's key with its last target, and the tables once they are empty. The same
/// text where it was not granted.
pub fn without(text: &str, pair: &Pair) -> Result<String, String> {
    without_target(text, &pair.asking, &pair.target)
}

/// [`without`], for a target as the file spells it.
fn without_target(text: &str, asking: &str, target: &str) -> Result<String, String> {
    let mut doc = document(text)?;
    let Some(dispatch) = doc.get_mut(TABLE) else {
        return Ok(text.to_owned());
    };
    let dispatch = dispatch
        .as_table_like_mut()
        .ok_or_else(|| not_editable(TABLE))?;
    let Some(grants) = dispatch.get_mut(KEY) else {
        return Ok(text.to_owned());
    };
    let grants = grants
        .as_table_like_mut()
        .ok_or_else(|| not_editable(&format!("{TABLE}.{KEY}")))?;
    let Some(targets) = grants.get_mut(asking) else {
        return Ok(text.to_owned());
    };
    let targets = targets
        .as_array_mut()
        .ok_or_else(|| not_editable(&format!("{TABLE}.{KEY}.{asking}")))?;
    let before = targets.len();
    targets.retain(|one| one.as_str() != Some(target));
    if targets.len() == before {
        return Ok(text.to_owned());
    }
    targets.fmt();
    if targets.is_empty() {
        grants.remove(asking);
    }
    if grants.is_empty() {
        dispatch.remove(KEY);
    }
    if dispatch.is_empty() {
        doc.remove(TABLE);
    }
    Ok(doc.to_string())
}

/// The project file at `root` as it is on disk, or why there is none to edit.
fn on_disk(root: &Path) -> Result<String, String> {
    let (there, text) = super::on_disk(root, Which::Shared)?;
    if !there {
        return Err(format!(
            "{} is not there, so nothing was changed.",
            Which::Shared.file()
        ));
    }
    Ok(text)
}

/// **Whether [`grant`] of `pair` would be written**, asked before the grant is audited: the
/// file is there and is one a form can edit. Nothing is written.
pub fn can_grant(root: &Path, pair: &Pair) -> Result<(), String> {
    with(&on_disk(root)?, pair).map(|_| ())
}

/// Writes `how`'s edit of the project file at `root`, as it is on disk now. A text the edit
/// leaves as it is is not written.
fn write(root: &Path, how: impl FnOnce(&str) -> Result<String, String>) -> Result<(), String> {
    let text = on_disk(root)?;
    let after = how(&text)?;
    if after == text {
        return Ok(());
    }
    super::save(root, Which::Shared, Some(&text), &after).map_err(|why| why.join(" "))
}

/// **Grants `pair` for everyone in the project at `root`**: the grant Notice's Allow at the
/// project level. Answers why not, in a sentence, and then nothing was written. Where the file
/// holds the pair already (a teammate's, not yet allowed here) nothing is written. Either way
/// the pair is acknowledged on this machine, so it is in force here and is no news here.
pub fn grant(root: &Path, pair: &Pair) -> Result<(), String> {
    write(root, |text| with(text, pair))?;
    crate::dispatchgrant::acknowledge_pair(root, pair).map_err(|why| {
        format!(
            "The grant is in {}, and purlis could not record it as allowed on this machine \
             ({}), so it covers nothing here yet. Allow it again.",
            Which::Shared.file(),
            crate::shown::short(&why.to_string())
        )
    })
}

/// **Revokes the project's grant of `pair`**: Settings' Revoke, a change to the committed file
/// which teammates follow like any other, and no news on this machine.
pub fn revoke(root: &Path, pair: &Pair) -> Result<(), String> {
    write(root, |text| without(text, pair))?;
    // Best effort: a pair left acknowledged covers nothing once the file lacks it.
    let _ = crate::dispatchgrant::forget_pair(root, pair);
    Ok(())
}

/// **Whether [`grant_any`] for `asking` would be written**, asked before it is audited.
pub fn can_grant_any(root: &Path, asking: &str) -> Result<(), String> {
    with_any(&on_disk(root)?, asking).map(|_| ())
}

/// **Lets `asking`'s chats dispatch to any persona, for everyone in the project at `root`**
/// (#1503): Settings' explicit grant. Where the file holds it already (a teammate's) nothing
/// is written. Either way it is accepted on this machine, so it is in force here.
pub fn grant_any(root: &Path, asking: &str) -> Result<(), String> {
    write(root, |text| with_any(text, asking))?;
    crate::dispatchgrant::acknowledge_any(root, asking).map_err(|why| {
        format!(
            "The grant is in {}, and purlis could not record it as allowed on this machine \
             ({}), so it covers nothing here yet. Allow it again.",
            Which::Shared.file(),
            crate::shown::short(&why.to_string())
        )
    })
}

/// **Revokes the project's grant of any persona for `asking`**: Settings' Revoke.
pub fn revoke_any(root: &Path, asking: &str) -> Result<(), String> {
    write(root, |text| without_any(text, asking))?;
    // Best effort: an acceptance left behind covers nothing once the file lacks the grant.
    let _ = crate::sandbox::local::forget_dispatch_any(root, asking);
    Ok(())
}

#[cfg(test)]
#[path = "dispatch_tests.rs"]
mod tests;
