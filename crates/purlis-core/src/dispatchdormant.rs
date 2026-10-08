//! **What becomes of dispatch grants when a persona goes away and a persona of that name is
//! there again** (#1504, ruling V100-61; ADR 0090 as amended).
//!
//! A dispatch grant is kept by name. Two rules keep a name that changes hands from carrying
//! what the person allowed the persona that had it before.
//!
//! # 1. A grant is in force only while both personas exist
//!
//! Checked where a dispatch is judged, with nothing moved: the app refuses a dispatch to a
//! name that is no persona, and one from a chat whose persona is no persona of the project
//! now, before any grant is read. That holds for a named pair, for "any persona" and for the
//! project's grants alike. So a branch without the persona, or a checkout half done, takes
//! nothing out of any record: the grants are simply not in force while it is away, and are
//! back when it is.
//!
//! # 2. Away, then there again, asks once
//!
//! - **The absence mark** ([`Known::away`], in `dispatch_known` of `app/sandbox.json`) is
//!   written when a judged dispatch ([`judged`]) or a read of Settings ([`noticed`]) finds a
//!   name a grant holds that is no persona. It is the only thing either writes about a name
//!   that is away. purlis's own `persona remove` writes it too ([`persona_removed`]).
//! - **The same definition coming back lifts it unasked.** With the mark is the hash of the
//!   persona's definition as it was when a dispatch was last judged with it there. A marked
//!   name that is a persona again with those very bytes is the same persona (a branch
//!   switched away and back), and [`judged`] lifts the mark and says nothing.
//! - **Anything else coming back is set aside.** A marked name that is a persona again with
//!   another definition, or with none known, has its grants moved out of the records in force
//!   ([`crate::sandbox::local::set_aside_dispatch`]) the next time a dispatch is judged. What
//!   this machine accepted of the project's grants for the name is set aside with them.
//!   purlis's own `persona create` does the same before it makes the persona
//!   ([`persona_gone`]), mark or none.
//! - **One acknowledgement per name gives them back** ([`give_back`]): Settings' Give back,
//!   the person's word that the persona of that name today may have what the earlier one had.
//!   [`remove`] takes one out for good. Nothing a chat sends reaches either.
//!
//! **Reading Settings moves nothing.** [`noticed`] writes the mark and no more; [`state`]
//! works out, without writing, which names are away and which are back and waiting.
//!
//! # A never stays in force
//!
//! A never is the one deny. Setting it aside would let through what the person refused, so it
//! holds for whichever persona has the name, and Settings says where it was said of an earlier
//! one.
//!
//! # What this does not do
//!
//! - **An absence nothing observed still inherits.** A persona removed and another made under
//!   its name, by hand or in one pull, with no dispatch judged and no read of Settings in
//!   between, leaves no mark, and the new persona has the old one's grants. Only an identity
//!   recorded in the persona's definition closes that; purlis records none today.
//! - **It is not a boundary against a chat.** A chat that may write the project's `personas/`
//!   can edit a persona in place, which no rule about names sees. This keeps the table true
//!   when a name is honestly reused.
//! - **purlis has no persona rename.** A persona renamed by moving its folder is, to this
//!   module, one that went away and another that appeared.

use std::path::Path;

pub use crate::sandbox::local::{Accepted, Dormant, Known, SetAside};

/// The personas of the project at `root`, or `None` where they cannot be listed: then nothing
/// is known to be gone, nothing is marked and nothing is set aside.
pub fn personas_of(root: &Path) -> Option<Vec<String>> {
    crate::workspaces::Plane::open(root.to_path_buf())
        .personas()
        .ok()
}

/// The SHA-256 of the definition of the persona `name`, in hex; `None` where it has none
/// that reads.
pub fn definition_hash(root: &Path, name: &str) -> Option<String> {
    use sha2::Digest;
    if !crate::personas::valid_name(name) {
        return None;
    }
    let bytes = std::fs::read(crate::personas::def_path(root, name)).ok()?;
    Some(
        sha2::Sha256::digest(&bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
    )
}

/// The grants set aside in the project at `root`, oldest first.
pub fn list(root: &Path) -> Vec<Dormant> {
    crate::sandbox::local::dormant_dispatch(root)
}

/// Which names the records of dispatch hold that are not plainly a persona's now.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct State {
    /// Names a grant holds that are no persona of the project now: such a grant is not in
    /// force.
    pub away: Vec<String>,
    /// Names that are a persona again after being seen gone, with a definition that is not
    /// the one known: the grants that name them are not in force, and are set aside the next
    /// time a dispatch is judged.
    pub returned: Vec<String>,
    /// Names the person can give something back to: every [`State::returned`] name, and each
    /// persona that has grants or acceptances set aside for it.
    pub back: Vec<String>,
}

/// **Where each name the grants hold stands**, worked out and not written; `None` where the
/// project's personas cannot be listed. `also` is names held by grants kept elsewhere (the
/// app's, made for one chat).
pub fn state(root: &Path, also: &[String]) -> Option<State> {
    let personas = personas_of(root)?;
    let there = |name: &str| personas.iter().any(|one| one == name);
    let known = crate::sandbox::local::dispatch_known(root);
    let mut out = State::default();
    for name in named(root, also) {
        if !there(&name) {
            out.away.push(name);
        } else if known
            .iter()
            .any(|one| one.name == name && one.away && !same(root, one))
        {
            out.returned.push(name);
        }
    }
    out.back.clone_from(&out.returned);
    let set_aside_for = list(root).into_iter().map(|one| one.was).chain(
        crate::sandbox::local::accepted_aside_dispatch(root)
            .into_iter()
            .map(|one| one.was),
    );
    for was in set_aside_for {
        if !was.is_empty() && there(&was) && !out.back.contains(&was) {
            out.back.push(was);
        }
    }
    Some(out)
}

/// Every name a grant holds: this machine's records, and `also`.
fn named(root: &Path, also: &[String]) -> Vec<String> {
    let mut names = crate::sandbox::local::dispatch_granted_names(root);
    for name in also {
        if name != crate::dispatchgrant::ANY && !names.contains(name) {
            names.push(name.clone());
        }
    }
    names
}

/// Whether the persona of `known`'s name has, now, the definition that is known of it.
fn same(root: &Path, known: &Known) -> bool {
    !known.hash.is_empty() && definition_hash(root, &known.name).as_deref() == Some(&known.hash)
}

/// **Settings read the table**: each name a grant holds that is no persona now gets its
/// absence mark. **That is all a read writes**: no grant moves, nothing this machine accepted
/// is dropped, and a name that is back is left for [`judged`] or the person.
pub fn noticed(root: &Path, also: &[String]) -> std::io::Result<()> {
    let Some(state) = state(root, also) else {
        return Ok(());
    };
    crate::sandbox::local::mark_dispatch_away(root, &state.away, false)
}

/// What [`judged`] did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Judged {
    /// What was set aside in this machine's records.
    pub aside: SetAside,
    /// The names that were seen gone and are another persona's now: the app ends the grants
    /// it made for one chat that name them.
    pub changed_hands: Vec<String>,
}

/// **A dispatch is about to be judged**: the records are brought up to what the project's
/// personas are now, before any grant is read. `also` is names held by grants kept elsewhere
/// (the app's, made for one chat).
///
/// - A name a grant holds that is no persona gets its absence mark.
/// - A marked name that is a persona again with the definition known of it has the mark
///   lifted.
/// - A marked name that is a persona again otherwise has its grants set aside, and the mark
///   lifted: Settings shows them, and one Give back for the name returns them.
/// - A name that is there and unmarked has its definition's hash kept up to date.
///
/// Nothing is done where the personas cannot be listed.
pub fn judged(root: &Path, also: &[String]) -> std::io::Result<Judged> {
    let Some(personas) = personas_of(root) else {
        return Ok(Judged::default());
    };
    let there = |name: &str| personas.iter().any(|one| one == name);
    let known = crate::sandbox::local::dispatch_known(root);
    let mut away = Vec::new();
    let mut seen = Vec::new();
    let mut changed_hands = Vec::new();
    for name in named(root, also) {
        match known.iter().find(|one| one.name == name) {
            _ if !there(&name) => away.push(name),
            Some(one) if one.away && !same(root, one) => changed_hands.push(name),
            _ => {
                let now = definition_hash(root, &name).unwrap_or_default();
                seen.push((name, now));
            }
        }
    }
    crate::sandbox::local::mark_dispatch_away(root, &away, false)?;
    let aside = crate::sandbox::local::set_aside_dispatch(root, &|name| {
        changed_hands.iter().any(|one| one == name)
    })?;
    // What came back under a marked name is known from here on as what it is now, so it is
    // not set aside twice; what the person then grants it is its own.
    seen.extend(changed_hands.iter().map(|name| {
        (
            name.clone(),
            definition_hash(root, name).unwrap_or_default(),
        )
    }));
    crate::sandbox::local::know_dispatch_personas(root, &seen)?;
    Ok(Judged {
        aside,
        changed_hands,
    })
}

/// **purlis is about to create the persona `name`**, which is no persona now: every grant of
/// yours naming it is set aside, with what this machine accepted of the project's grants for
/// it, whether or not the name was ever seen gone. Answers what was set aside. **An error
/// here must stop the creation**: the persona would inherit what could not be set aside.
pub fn persona_gone(root: &Path, name: &str) -> std::io::Result<SetAside> {
    let aside = crate::sandbox::local::set_aside_dispatch(root, &|one| one == name)?;
    if !aside.is_empty() {
        // Not away, and nothing known of it yet: the first dispatch judged with it there
        // records what it is.
        crate::sandbox::local::know_dispatch_personas(root, &[(name.to_owned(), String::new())])?;
    }
    Ok(aside)
}

/// **purlis removed the persona `name`**: where a grant names it, it is marked gone and what
/// was known of its definition is forgotten, so nothing that comes back under the name is
/// taken for it. Nothing is moved: the grants are not in force while the name is no persona.
/// Answers whether any grant names it.
pub fn persona_removed(root: &Path, name: &str) -> std::io::Result<bool> {
    let named = crate::sandbox::local::dispatch_granted_names(root)
        .iter()
        .any(|one| one == name);
    if named {
        crate::sandbox::local::mark_dispatch_away(root, &[name.to_owned()], true)?;
    }
    Ok(named)
}

/// Why nothing can be given back to `name`, before anything is audited or written; `Ok`
/// where [`give_back`] would act.
pub fn can_give_back(root: &Path, name: &str) -> Result<(), String> {
    let Some(state) = state(root, &[]) else {
        return Err(
            "purlis could not list this project's personas, so nothing was changed.".to_owned(),
        );
    };
    if state.back.iter().any(|one| one == name) {
        return Ok(());
    }
    if state.away.iter().any(|one| one == name) || list(root).iter().any(|one| one.was == name) {
        return Err(format!(
            "This project has no persona named {}, so its grants stay as they are. Make the \
             persona first, or remove them.",
            crate::shown::short(name)
        ));
    }
    Err("purlis changed nothing: nothing is waiting to be given back to that persona.".to_owned())
}

/// **Gives the persona `name` back what an earlier persona of that name had**: Settings' own
/// action, one acknowledgement for the name. Its absence mark is lifted, so grants that were
/// never moved are in force again; each grant set aside for it whose other persona exists is
/// in force again for you on this machine; and what this machine had accepted of the
/// project's grants for it is accepted again where the project's file still holds it. A grant
/// set aside whose other persona is no persona stays set aside. Answers what came back.
pub fn give_back(root: &Path, name: &str) -> Result<SetAside, String> {
    can_give_back(root, name)?;
    let personas = personas_of(root).unwrap_or_default();
    let there = |one: &str| crate::personas::valid_name(one) && personas.iter().any(|p| p == one);
    let ok = |one: &Dormant| {
        there(&one.asking)
            && if one.any {
                one.target == crate::dispatchgrant::ANY
            } else {
                there(&one.target) && one.asking != one.target
            }
    };
    let holds = |said: &str| {
        said.split_once(" -> ").is_some_and(|(asking, target)| {
            crate::dispatchgrant::project_grants(root, asking, target)
        })
    };
    let hash = definition_hash(root, name).unwrap_or_default();
    crate::sandbox::local::give_back_dispatch(root, name, &hash, &ok, &holds)
        .map_err(|why| format!("purlis could not keep the grants: {why}"))
}

/// **Removes the grant set aside for `asking` to `target`**: Settings' Remove. `any` says
/// which is meant, so one press takes one entry. Answers whether it was there.
pub fn remove(root: &Path, asking: &str, target: &str, any: bool) -> Result<bool, String> {
    crate::sandbox::local::forget_dormant_dispatch(root, asking, target, any)
        .map_err(|why| format!("purlis could not remove it: {why}"))
}

#[cfg(test)]
#[path = "dispatchdormant_tests.rs"]
mod tests;
