//! **What becomes of dispatch grants when a persona is removed, made again or renamed**
//! (#1504, ruling V100-61; ADR 0090 as amended).
//!
//! A dispatch grant is kept by name. So a name that changes hands must not carry what the
//! person allowed the persona that had it before.
//!
//! # A removed persona's grants are set aside
//!
//! [`persona_gone`] moves every grant of yours that names the persona, as asking persona or as
//! target, and its "any persona", out of the records that are in force and into
//! `dispatch_dormant` of `app/sandbox.json`. What this machine accepted of the project's grants
//! for that name is forgotten in the same write, so a pair the committed file still holds
//! waits for a yes again, as one a pull brought in does (D-1437-R1). **Nothing set aside is
//! read by [`crate::dispatchgrant::covers`]**: it is out of the record, not marked in it, so no
//! reader has to remember to skip it.
//!
//! It is called where purlis removes a persona, and again before purlis creates one, for the
//! name about to be taken: whatever still named it was an earlier persona's. [`sweep`] does
//! the same for every name the records hold that is no persona of the project now, and
//! Settings calls it each time it reads the table, so a persona removed by hand or by a pull
//! is set aside the next time the person looks.
//!
//! # Only the person brings one back
//!
//! [`revive`] puts one grant back in force, from Settings, where both names are personas of the
//! project now: the person's acknowledgement that the persona of that name today may have what
//! the earlier one had. [`remove`] takes it out for good. Nothing a chat sends reaches either.
//!
//! # A never stays in force
//!
//! A never is the one deny. Setting it aside would let through what the person refused, so it
//! is left as it is and holds for whichever persona has the name. Settings shows it and lifts
//! it.
//!
//! # A rename carries everything
//!
//! [`persona_renamed`] rewrites the name in every record: yours, "any persona", what you
//! accepted and declined of the project's, the nevers, and `[dispatch.grants]` of the committed
//! file. Whatever the new name still held from an earlier persona is set aside first, so the
//! renamed persona keeps its own and gains nothing.

use std::path::Path;

pub use crate::sandbox::local::Dormant;

/// The personas of the project at `root`, or `None` where they cannot be listed: then nothing
/// is known to be gone, and nothing is set aside.
pub fn personas_of(root: &Path) -> Option<Vec<String>> {
    crate::workspaces::Plane::open(root.to_path_buf())
        .personas()
        .ok()
}

/// The grants set aside in the project at `root`, oldest first.
pub fn list(root: &Path) -> Vec<Dormant> {
    crate::sandbox::local::dormant_dispatch(root)
}

/// **The persona `name` is gone, or its name is about to be taken by a new one**: every grant
/// of yours naming it is set aside, and what this machine accepted of the project's grants for
/// it is forgotten. Answers what was set aside.
pub fn persona_gone(root: &Path, name: &str) -> std::io::Result<Vec<Dormant>> {
    crate::sandbox::local::set_aside_dispatch(root, &|one| one == name)
}

/// **Sets aside every grant of yours that names no persona of the project now.** Nothing is
/// done where the personas cannot be listed. Answers what was set aside.
pub fn sweep(root: &Path) -> std::io::Result<Vec<Dormant>> {
    let Some(personas) = personas_of(root) else {
        return Ok(Vec::new());
    };
    crate::sandbox::local::set_aside_dispatch(root, &|one| !personas.iter().any(|p| p == one))
}

/// Why the grant set aside for `asking` to `target` cannot be put back, before anything is
/// audited or written; `Ok` where [`revive`] would put it back. `target` is a persona's name,
/// or [`crate::dispatchgrant::ANY`] for one set aside **as** "any persona"
/// ([`Dormant::any`]): a `*` that was written as a pair's target is nobody's name and is
/// never given back.
pub fn can_revive(root: &Path, asking: &str, target: &str) -> Result<(), String> {
    let any = target == crate::dispatchgrant::ANY;
    if !list(root)
        .iter()
        .any(|one| one.asking == asking && one.target == target && one.any == any)
    {
        return Err("purlis changed nothing: that grant is no longer there.".to_owned());
    }
    let Some(personas) = personas_of(root) else {
        return Err(
            "purlis could not list this project's personas, so nothing was changed.".to_owned(),
        );
    };
    let mut names = vec![asking];
    if !any {
        names.push(target);
    }
    for name in names {
        if !crate::personas::valid_name(name) || !personas.iter().any(|one| one == name) {
            return Err(format!(
                "This project has no persona named {}, so the grant stays set aside. Remove it, \
                 or make the persona first.",
                crate::shown::short(name)
            ));
        }
    }
    if !any {
        crate::dispatchgrant::Pair::new(asking, target)?;
    }
    Ok(())
}

/// **Puts the grant set aside for `asking` to `target` back in force** for you on this
/// machine: Settings' own action, the person's acknowledgement. `target` is a persona's name,
/// or [`crate::dispatchgrant::ANY`].
pub fn revive(root: &Path, asking: &str, target: &str) -> Result<(), String> {
    can_revive(root, asking, target)?;
    let any = target == crate::dispatchgrant::ANY;
    crate::sandbox::local::revive_dormant_dispatch(root, asking, target, any)
        .map(|_| ())
        .map_err(|why| format!("purlis could not keep the grant: {why}"))
}

/// **Removes the grant set aside for `asking` to `target`**: Settings' Remove. Answers whether
/// it was there.
pub fn remove(root: &Path, asking: &str, target: &str) -> Result<bool, String> {
    crate::sandbox::local::forget_dormant_dispatch(root, asking, target)
        .map_err(|why| format!("purlis could not remove it: {why}"))
}

/// **The persona `from` is now called `to`**: its grants, its "any persona" and its nevers are
/// rewritten in every record of the project at `root`, and in `[dispatch.grants]` of the
/// committed file.
///
/// For the code that renames a persona to call, once per rename. Whatever the records still
/// held for `to` from an earlier persona of that name is set aside first. Each record is
/// attempted whatever became of the others; the answer is every sentence of what could not be
/// rewritten, and those records still name `from`, which then names no persona and grants
/// nothing ([`sweep`] sets them aside).
pub fn persona_renamed(root: &Path, from: &str, to: &str) -> Result<(), Vec<String>> {
    for name in [from, to] {
        if !crate::personas::valid_name(name) {
            return Err(vec![format!(
                "{} is not a persona's name, so no dispatch grant was renamed.",
                crate::shown::short(name)
            )]);
        }
    }
    if from == to {
        return Ok(());
    }
    let mut refused = Vec::new();
    if let Err(why) = persona_gone(root, to)
        .and_then(|_| crate::sandbox::local::rename_dispatch_persona(root, from, to))
    {
        refused.push(format!(
            "purlis could not rename {from} in your dispatch grants on this machine ({why})."
        ));
    }
    if let Err(why) = crate::dispatchnever::rename(root, from, to) {
        refused.push(format!(
            "purlis could not rename {from} in the pairs you said never to ({why})."
        ));
    }
    if let Err(why) = crate::settings::dispatch::rename(root, from, to) {
        refused.push(why);
    }
    if refused.is_empty() {
        Ok(())
    } else {
        Err(refused)
    }
}

#[cfg(test)]
#[path = "dispatchdormant_tests.rs"]
mod tests;
