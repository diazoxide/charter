//! The product's own environment variables, read and handed on under both of their names
//! during the rename's window (#1253, RN-2d, V93k).
//!
//! `PURLIS_<X>` is the variable; `CHARTER_<X>` is read when it is absent. Which spellings
//! there are, and which wins, is [`crate::names::ENV_PREFIX`]'s to say ([`Name::pick_with`]);
//! this module only asks it. Every read of a product variable goes through [`lookup`] — or
//! [`var_os`] and [`var`], which are [`lookup`] over this process's environment — so a name
//! can be given in either spelling and the answer is the same.
//!
//! **A chat gets both** ([`twinned`]): a hook, a plugin or a script a harness runs may still
//! read the old name, and one that was taught the new one finds it too. Every other program
//! the product starts gets what the product wrote, which is the purlis name, and reads it here.
//!
//! **Precedence is by presence**, as for a file: a `PURLIS_<X>` that is set, even to nothing,
//! wins, and an empty value means what an empty value of `CHARTER_<X>` meant to its reader.
//!
//! [`Name::pick_with`]: crate::names::Name::pick_with

use std::collections::HashMap;
use std::ffi::{OsStr, OsString};

use crate::names::ENV_PREFIX;

/// What follows the product prefix in `name`, in any spelling (`ROOT` of `CHARTER_ROOT`), or
/// `None` for a variable that is not the product's.
pub fn rest(name: &str) -> Option<&str> {
    ENV_PREFIX.strip(name).filter(|rest| !rest.is_empty())
}

/// `name` spelled the way the product writes it: `CHARTER_ROOT` is `PURLIS_ROOT`. Any other
/// variable is itself.
pub fn canonical(name: &str) -> String {
    match rest(name) {
        Some(rest) => format!("{}{rest}", ENV_PREFIX.write),
        None => name.to_owned(),
    }
}

/// Every spelling of `name`, the purlis one first; just `name` for a variable that is not the
/// product's.
pub fn spellings(name: &str) -> Vec<String> {
    match rest(name) {
        Some(rest) => ENV_PREFIX
            .spellings()
            .map(|p| format!("{p}{rest}"))
            .collect(),
        None => vec![name.to_owned()],
    }
}

/// Whether `a` and `b` are one variable, under whichever of its names each is spelled.
pub fn same(a: &str, b: &str) -> bool {
    canonical(a) == canonical(b)
}

/// `name` from `env`, under the spelling [`ENV_PREFIX`] picks: the purlis one when it is set,
/// else the first old one that is. `name` may be given in any spelling.
pub fn lookup<T>(name: &str, env: impl Fn(&str) -> Option<T>) -> Option<T> {
    let Some(rest) = rest(name) else {
        return env(name);
    };
    let picked = ENV_PREFIX.pick_with(rest, |full| env(full).is_some());
    env(&picked.name)
}

/// [`lookup`] in this process's environment.
pub fn var_os(name: &str) -> Option<OsString> {
    lookup(name, |n: &str| std::env::var_os(n))
}

/// [`var_os`], as text; a value that is not UTF-8 reads as unset, as `std::env::var(..).ok()`.
pub fn var(name: &str) -> Option<String> {
    var_os(name).and_then(|value| value.into_string().ok())
}

/// The variables that choose what a command acts on — a project, a path, a persona, the
/// machine's approvals — by what follows the prefix (D-RN2d-8).
///
/// For these, two names with two values are never silently read under one of them: the
/// caller refuses ([`disagreement`]). Inside a chat both carry the same value, so the case is
/// a `CHARTER_<X>=…` typed in front of a command — which, read under the purlis name, would
/// act on the chat's own project instead of the one asked for. Every other variable (logs,
/// knobs, the footer) is read under the purlis name when the two differ (V93e).
///
/// `SESSION_ID` is not here: it keys a chat's own state rather than naming a scope, and an
/// opencode shim an older build installed sets only its old name in a tool's shell.
pub const SELECTING: [&str; 8] = [
    "ROOT",
    "HOME",
    "WORKSPACE",
    "WORKTREES",
    "PLANE_ROOT_SESSION",
    "PERSONA",
    "CONFIG_HOME",
    "DATA_HOME",
];

/// One of [`SELECTING`] set under two names with two values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Disagreement {
    /// The purlis name, the one to keep.
    pub purlis: String,
    /// The old name that says something else.
    pub old: String,
}

impl std::fmt::Display for Disagreement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{purlis} and {old} disagree — set only {purlis} ({old} is the old name).",
            purlis = self.purlis,
            old = self.old
        )
    }
}

/// The first of [`SELECTING`] whose names disagree in `env`: both set, to different values.
/// Equal values, or one name alone, are no disagreement.
pub fn disagreement_in(env: impl Fn(&str) -> Option<OsString>) -> Option<Disagreement> {
    SELECTING.iter().find_map(|rest| {
        let mut names = ENV_PREFIX
            .spellings()
            .map(|prefix| format!("{prefix}{rest}"));
        let purlis = names.next()?;
        let value = env(&purlis)?;
        names
            .find(|old| env(old).is_some_and(|other| other != value))
            .map(|old| Disagreement { purlis, old })
    })
}

/// [`disagreement_in`] this process's environment.
pub fn disagreement() -> Option<Disagreement> {
    disagreement_in(|name: &str| std::env::var_os(name))
}

/// Sets `name` on `command` under every one of its names, so an old name the child would
/// inherit says the same thing ([`disagreement`]).
pub fn set_on(command: &mut std::process::Command, name: &str, value: impl AsRef<OsStr>) {
    for spelling in spellings(name) {
        command.env(spelling, value.as_ref());
    }
}

/// A chat's environment with each product variable under both of its names (V93k).
///
/// Every other pair is kept, in its place. A product variable keeps the place it was first
/// named at, under its purlis name then its old one, with the LAST value it was given — a later
/// pair wins, as it would on a [`std::process::Command`]. So the caller lists what it inherited
/// first and what it sets after; [`crate::chatenv::inherited`] has already let an inherited
/// `PURLIS_<X>` win over an inherited `CHARTER_<X>`. Applying it twice changes nothing.
pub fn twinned(env: Vec<(OsString, OsString)>) -> Vec<(OsString, OsString)> {
    let mut last: HashMap<String, OsString> = HashMap::new();
    for (name, value) in &env {
        if let Some(rest) = name.to_str().and_then(rest) {
            last.insert(rest.to_owned(), value.clone());
        }
    }
    let mut out = Vec::with_capacity(env.len() + last.len());
    for (name, value) in env {
        let Some(rest) = name.to_str().and_then(rest).map(str::to_owned) else {
            out.push((name, value));
            continue;
        };
        if let Some(value) = last.remove(&rest) {
            for prefix in ENV_PREFIX.spellings() {
                out.push((OsString::from(format!("{prefix}{rest}")), value.clone()));
            }
        }
    }
    out
}

/// Of `env`, as inherited, every product variable that a purlis spelling of the same variable
/// beside it outranks: an inherited `CHARTER_<X>` is dropped when `PURLIS_<X>` is inherited too.
pub fn outranked(env: &[(OsString, OsString)]) -> impl Fn(&OsStr) -> bool + '_ {
    move |name: &OsStr| {
        let Some(text) = name.to_str() else {
            return false;
        };
        let canonical = canonical(text);
        canonical != text
            && env
                .iter()
                .any(|(other, _)| other.as_os_str() == canonical.as_str())
    }
}

#[cfg(test)]
#[path = "envvar_tests.rs"]
mod tests;
