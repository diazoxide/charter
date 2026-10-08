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
    crate::sandbox::local::acknowledge_dispatch_any(root, asking).map_err(|why| {
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

// ---- a grant limited to one workspace (#1505) ------------------------------------------------

use crate::dispatchwithin::{IN, Limited, TO, Within};

/// Whether `entry`, one item of an asking persona's list, is the limited grant to `target` in
/// `workspace`: a table of exactly those two keys.
fn is_limited(entry: &toml_edit::Value, target: &str, workspace: &str) -> bool {
    entry.as_inline_table().is_some_and(|table| {
        table.len() == 2
            && table.get(TO).and_then(toml_edit::Value::as_str) == Some(target)
            && table.get(IN).and_then(toml_edit::Value::as_str) == Some(workspace)
    })
}

/// Whether `entry` is a limited grant to `target`, in whichever workspace: a table of
/// exactly the two keys this build reads. **A table with a key more is not one**: it is a
/// later build's, this build grants nothing by it, and no edit here takes it out.
fn is_limited_to(entry: &toml_edit::Value, target: &str) -> bool {
    entry.as_inline_table().is_some_and(|table| {
        table.len() == 2
            && table.get(TO).and_then(toml_edit::Value::as_str) == Some(target)
            && table.get(IN).and_then(toml_edit::Value::as_str).is_some()
    })
}

/// A limited grant as the file writes it: `{ to = "devops", in = "runners" }`.
fn limited_entry(target: &str, workspace: &str) -> toml_edit::Value {
    let mut table = toml_edit::InlineTable::new();
    table.insert(TO, target.into());
    table.insert(IN, workspace.into());
    toml_edit::Value::InlineTable(table)
}

/// **`text` with the list of `asking` in `[dispatch.grants]` changed by `how`**, every other
/// line kept: the tables and the list are made where there are none and `create` says so, and
/// taken out again once they are empty. `how` answers whether it changed the list; the same
/// text where it did not, or where there was no list to change.
fn with_list(
    text: &str,
    asking: &str,
    create: bool,
    how: impl FnOnce(&mut toml_edit::Array) -> bool,
) -> Result<String, String> {
    let mut doc = document(text)?;
    if !create
        && doc
            .get(TABLE)
            .and_then(|dispatch| dispatch.get(KEY))
            .and_then(|grants| grants.get(asking))
            .is_none()
    {
        return Ok(text.to_owned());
    }
    let dispatch = doc.entry(TABLE).or_insert_with(|| {
        let mut table = toml_edit::Table::new();
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
    if !how(targets) {
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

/// **`text` with `one` granted**: `{ to = "<target>", in = "<workspace>" }` added last to the
/// asking persona's list. The same text where it is granted already.
pub fn with_in(text: &str, one: &Limited) -> Result<String, String> {
    with_list(text, &one.asking, true, |targets| {
        if targets
            .iter()
            .any(|entry| is_limited(entry, &one.target, &one.workspace))
        {
            return false;
        }
        targets.push_formatted(limited_entry(&one.target, &one.workspace));
        true
    })
}

/// **`text` without `one`**. The same text where it was not granted. A grant of the same pair
/// that holds in any workspace, or in another, stays.
pub fn without_in(text: &str, one: &Limited) -> Result<String, String> {
    with_list(text, &one.asking, false, |targets| {
        let before = targets.len();
        targets.retain(|entry| !is_limited(entry, &one.target, &one.workspace));
        targets.len() != before
    })
}

/// **`text` with the grant of `asking` to `target` (`"*"`: any persona) moved from holding
/// `from` to holding `to`**, in one edit. `Err` where the file does not hold it as `from`
/// says. Widening to any workspace takes the pair's other limited entries with it: the one
/// that holds everywhere covers them.
pub fn with_within(
    text: &str,
    asking: &str,
    target: &str,
    from: &Within,
    to: &Within,
) -> Result<String, String> {
    let mut there = false;
    let after = with_list(text, asking, false, |targets| {
        let before = targets.len();
        match from {
            Within::Any => targets.retain(|entry| entry.as_str() != Some(target)),
            Within::Workspace(workspace) => {
                targets.retain(|entry| !is_limited(entry, target, workspace));
            }
        }
        there = targets.len() != before;
        if !there {
            return false;
        }
        match to {
            Within::Any => {
                targets.retain(|entry| !is_limited_to(entry, target));
                targets.push(target);
            }
            Within::Workspace(workspace) => {
                if !targets
                    .iter()
                    .any(|entry| is_limited(entry, target, workspace))
                {
                    targets.push_formatted(limited_entry(target, workspace));
                }
            }
        }
        true
    })?;
    if there {
        Ok(after)
    } else {
        Err("purlis changed nothing: the project no longer has that grant.".to_owned())
    }
}

/// **Whether [`grant_in`] of `one` would be written**, asked before the grant is audited.
pub fn can_grant_in(root: &Path, one: &Limited) -> Result<(), String> {
    with_in(&on_disk(root)?, one).map(|_| ())
}

/// **Writes `one` into the project's file at `root`, and no more**: the committed half of
/// [`grant_in`]. Refused, with nothing written, where its workspace is not one of the
/// project's now. Where the file holds it already (a teammate's) nothing is written.
pub fn write_in(root: &Path, one: &Limited) -> Result<(), String> {
    if !crate::dispatchwithin::Seen::read(root).is_there(&one.workspace) {
        return Err(crate::dispatchwithin::not_there_said(&one.workspace));
    }
    write(root, |text| with_in(text, one))
}

/// **Grants `one` for everyone in the project at `root`**: the grant Notice's Allow at the
/// project level, for the workspace the task works in. Either way it is accepted on this
/// machine, so it is in force here. A caller that must tell "not written" from "written, and
/// not accepted here" calls [`write_in`] and [`crate::dispatchwithin::accept`] itself.
pub fn grant_in(root: &Path, one: &Limited) -> Result<(), String> {
    write_in(root, one)?;
    crate::dispatchwithin::accept(root, one).map_err(|why| written_not_accepted(&why))
}

/// What is said where a grant is in the project's file and this machine could not record
/// its own acceptance of it.
pub fn written_not_accepted(why: &str) -> String {
    format!(
        "The grant is in {}, and it covers nothing on this machine yet. {why} Accept it in \
         {}.",
        Which::Shared.file(),
        crate::dispatchgrant::SETTINGS
    )
}

/// **Revokes the project's limited grant `one`**: Settings' Remove for everyone.
pub fn revoke_in(root: &Path, one: &Limited) -> Result<(), String> {
    write(root, |text| without_in(text, one))?;
    // Best effort: an acceptance left behind covers nothing once the file lacks the grant.
    let _ = crate::dispatchwithin::unaccept(root, one);
    Ok(())
}

/// **Whether [`set_within`] would be written**, asked before it is audited.
pub fn can_set_within(
    root: &Path,
    asking: &str,
    target: &str,
    from: &Within,
    to: &Within,
) -> Result<(), String> {
    with_within(&on_disk(root)?, asking, target, from, to).map(|_| ())
}

/// **Writes the move of the project's grant of `asking` to `target` (`"*"`: any persona)
/// from holding `from` to holding `to` into the project's file, and no more**: the committed
/// half of [`set_within`]. Refused, with nothing written, where `to` is a workspace that is
/// not one of the project's now.
pub fn write_within(
    root: &Path,
    asking: &str,
    target: &str,
    from: &Within,
    to: &Within,
) -> Result<(), String> {
    if let Within::Workspace(workspace) = to
        && !crate::dispatchwithin::Seen::read(root).is_there(workspace)
    {
        return Err(crate::dispatchwithin::not_there_said(workspace));
    }
    write(root, |text| with_within(text, asking, target, from, to))
}

/// **Has this machine follow a project grant that [`write_within`] just moved**: what it
/// accepted of the grant as it was is dropped, and the grant as it is now is accepted here,
/// since the person at this machine just wrote it. `Err` where the acceptance could not be
/// recorded: the file holds the change all the same.
pub fn follow_within(
    root: &Path,
    asking: &str,
    target: &str,
    from: &Within,
    to: &Within,
) -> Result<(), String> {
    let any = target == crate::dispatchgrant::ANY;
    // Best effort, as a revoke's is: what is left accepted covers nothing the file lacks.
    match from {
        Within::Workspace(workspace) => {
            if let Ok(one) = Limited::new(asking, target, workspace) {
                let _ = crate::dispatchwithin::unaccept(root, &one);
            }
        }
        Within::Any if any => {
            let _ = crate::sandbox::local::forget_dispatch_any(root, asking);
        }
        Within::Any => {
            if let Ok(pair) = Pair::new(asking, target) {
                let _ = crate::dispatchgrant::forget_pair(root, &pair);
            }
        }
    }
    match to {
        Within::Workspace(workspace) => {
            let one = Limited::new(asking, target, workspace)?;
            crate::dispatchwithin::accept(root, &one).map_err(|why| written_not_accepted(&why))
        }
        Within::Any if any => crate::sandbox::local::acknowledge_dispatch_any(root, asking)
            .map_err(|why| written_not_accepted(&format!("({why})"))),
        Within::Any => {
            let pair = Pair::new(asking, target)?;
            crate::dispatchgrant::acknowledge_pair(root, &pair)
                .map_err(|why| written_not_accepted(&format!("({why})")))
        }
    }
}

/// **Moves the project's grant of `asking` to `target` (`"*"`: any persona) from holding
/// `from` to holding `to`, for everyone**: Settings' change of a project grant's workspace,
/// one edit of the committed file ([`write_within`]), which this machine then follows
/// ([`follow_within`]).
pub fn set_within(
    root: &Path,
    asking: &str,
    target: &str,
    from: &Within,
    to: &Within,
) -> Result<(), String> {
    write_within(root, asking, target, from, to)?;
    follow_within(root, asking, target, from, to)
}

#[cfg(test)]
#[path = "dispatch_tests.rs"]
mod tests;
