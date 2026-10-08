//! Where purlis looks for a provider's program: `op` for a 1Password vault, `op` or `vault`
//! for a reference. One lookup, [`Ctx::program`], asked by every provider and by `purlis doctor`.
//!
//! **#1516.** A provider used to be looked for on its process's own `PATH` and nowhere else. An
//! app started from the Dock is given `/usr/bin:/bin:/usr/sbin:/sbin`, so the app that resolves
//! a sandboxed chat's `secret exec` ([`super::brokered`]) answered "not on PATH" for an `op` in
//! `/opt/homebrew/bin`, which the chat's own shell found.
//!
//! **The directories are [`crate::programs`]'s**, the list that finds a harness and a forge's
//! CLI: the process's `PATH`, then the directories a person's installers use under their home,
//! then the machine-wide ones. That module says why the list is fixed and no login shell is
//! asked, and it holds here for the same reasons.
//!
//! **Whose `PATH` and whose home.** The [`Ctx`]'s: the process that resolves the value. For a
//! sandboxed chat that is the app, whose environment the chat never sets. What a chat sends
//! with its ask is the child's environment and is never read here.
//!
//! **A directory a chat may write is searched last**, wherever `PATH` names it: the project,
//! the temp directories and a harness's own homes ([`chat_may_write`]). A provider's program is
//! handed the vault's identity, so one a chat could have written never runs in place of one the
//! person installed.
//!
//! **A fenced build leaves the machine-wide directories out** unless its `PATH` names them, as
//! it keeps a stub for the keyring: no test may run the `op` this machine has installed.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use super::Ctx;
use crate::programs::{self, NotFound};

/// How a directory came to be searched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    /// Only this process's own `PATH` names it: a purlis started with another `PATH` does not
    /// search it.
    Path,
    /// One of the directories purlis searches whatever `PATH` says.
    Always,
}

/// The programs `vault`'s provider runs to read it: `op` for a 1Password vault, the CLIs its
/// references resolve through for a reference vault, none for the others.
pub fn needed_by(ctx: &Ctx, vault: &super::registry::Vault) -> Vec<&'static str> {
    match vault.provider.as_str() {
        "1password" => vec!["op"],
        "reference" => super::reference::clis(ctx, vault),
        _ => Vec::new(),
    }
}

/// One directory of the search, and how it came to be in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Place {
    pub dir: PathBuf,
    pub route: Route,
}

/// A provider's program, and how it was found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub path: PathBuf,
    pub route: Route,
}

/// Every directory searched for a provider's program, in order.
///
/// [`programs::search_dirs_from`]'s list for `path` and `home`, less the machine-wide
/// directories `path` does not name when `machine_wide` is off, with every directory
/// `chat_may_write` answers for moved after the rest. Each half keeps its own order.
pub fn places_from(
    path: Option<&OsStr>,
    home: Option<&Path>,
    machine_wide: bool,
    chat_may_write: &dyn Fn(&Path) -> bool,
) -> Vec<Place> {
    let inherited: Vec<PathBuf> = path
        .map(|path| programs::searchable(path).collect())
        .unwrap_or_default();
    let machine_wide_dir = |dir: &Path| {
        programs::SYSTEM_BIN
            .iter()
            .any(|fixed| Path::new(fixed) == dir)
    };
    // What is searched with no `PATH` at all: a program found there is found however this
    // process was started.
    let always: Vec<PathBuf> = programs::search_dirs_from(None, home)
        .into_iter()
        .filter(|dir| machine_wide || !machine_wide_dir(dir))
        .collect();
    let (first, last): (Vec<Place>, Vec<Place>) = programs::search_dirs_from(path, home)
        .into_iter()
        .filter(|dir| inherited.contains(dir) || always.contains(dir))
        .map(|dir| Place {
            route: if always.contains(&dir) {
                Route::Always
            } else {
                Route::Path
            },
            dir,
        })
        .partition(|place| !chat_may_write(&place.dir));
    first.into_iter().chain(last).collect()
}

/// `path` as the disk names it, as far as it exists: a directory spelled through a link is
/// compared by where it is.
fn real(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

/// The folders a chat in `ctx`'s project may write: the project, the temp directories, and the
/// homes a harness keeps its own files under. Each in both spellings, as named and as the disk
/// names it.
pub fn chat_may_write(ctx: &Ctx) -> Vec<PathBuf> {
    let home = home_of(ctx);
    let tmpdir: Vec<(String, String)> = ctx
        .env
        .get("TMPDIR")
        .map(|dir| ("TMPDIR".to_owned(), dir))
        .into_iter()
        .collect();
    let machine = crate::sandbox::Machine {
        env: ctx.env.clone(),
        home,
        os: crate::sandbox::Os::this(),
    };
    let homes = crate::sandbox::Homes::of(&machine);
    let mut named = crate::sandbox::program::temp_roots(&tmpdir);
    named.extend(
        [
            homes.data,
            homes.state,
            homes.config,
            homes.cache,
            homes.codex,
            crate::sandbox::Homes::codex_project(&machine, &ctx.root),
        ]
        .into_iter()
        .flatten(),
    );
    named.push(ctx.root.clone());
    let mut all: Vec<PathBuf> = Vec::new();
    for dir in named {
        for spelling in [real(&dir), dir] {
            if spelling.is_absolute() && !all.contains(&spelling) {
                all.push(spelling);
            }
        }
    }
    all
}

/// `$HOME` in `ctx`'s environment, when it is an absolute path.
fn home_of(ctx: &Ctx) -> Option<PathBuf> {
    ctx.env
        .get("HOME")
        .map(PathBuf::from)
        .filter(|home| home.is_absolute())
}

impl Ctx {
    /// Every directory this context searches for a provider's program, in order.
    pub fn program_places(&self) -> Vec<Place> {
        let writable = chat_may_write(self);
        let path = self.env.get("PATH");
        places_from(
            path.as_deref().map(OsStr::new),
            home_of(self).as_deref(),
            !crate::fence::FENCED,
            &|dir| {
                let real = real(dir);
                writable
                    .iter()
                    .any(|grant| dir.starts_with(grant) || real.starts_with(grant))
            },
        )
    }

    /// The provider's program `name` as an absolute path with the route it was found by, or
    /// every directory that was searched.
    pub fn program(&self, name: &str) -> Result<Found, NotFound> {
        let places = self.program_places();
        let dirs: Vec<PathBuf> = places.iter().map(|place| place.dir.clone()).collect();
        let Some(path) = programs::find(name, &dirs) else {
            return Err(NotFound {
                program: name.to_owned(),
                looked: dirs,
            });
        };
        let route = places
            .iter()
            .find(|place| path.parent() == Some(place.dir.as_path()))
            .map_or(Route::Always, |place| place.route);
        Ok(Found { path, route })
    }
}

impl Ctx {
    /// The end of a not-found refusal: every directory that was searched, whole, and where to
    /// link a program that is installed somewhere else.
    pub fn looked_in(&self, not: &NotFound) -> String {
        if not.looked.is_empty() {
            return "It had no directory to look in: this process has no PATH and no home \
                    directory."
                .to_owned();
        }
        let looked = format!("It looked in: {}.", not.looked_in());
        match home_of(self) {
            Some(home) => format!(
                "{looked} If it is installed somewhere else, put a link to it in {}.",
                crate::shown::readable(
                    &home.join(programs::USER_BIN[0]).display().to_string(),
                    usize::MAX
                )
            ),
            None => looked,
        }
    }
}

#[cfg(test)]
#[path = "program_tests.rs"]
mod tests;
