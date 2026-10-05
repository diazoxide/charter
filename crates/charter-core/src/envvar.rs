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
