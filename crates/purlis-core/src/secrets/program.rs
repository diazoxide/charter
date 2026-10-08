//! Where purlis looks for a provider's program: `op` for a 1Password vault, `op` or `vault`
//! for a reference. One lookup, [`Ctx::program`], asked by every provider, by the pin taken
//! when a token is stored, and by `purlis doctor`.
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
//! **A program where a chat may write is never run** (D-1516-9). A provider's program is handed
//! the vault's identity, so a file a chat could have written is refused as a harness in such a
//! place is ([`crate::sandbox::program::checked`], whose test of "where a chat can write" this
//! asks). The file is judged as it was found and as the disk names it, so a link in a
//! directory no chat writes to a file a chat does write is refused too, and what runs is the
//! file the disk names. A copy further along the search that no chat can write is used; with
//! none, the refusal names the file that was passed over.
//!
//! **Where a chat may write is the sandbox's own record** ([`Ctx::chat_writes`]): the project,
//! the folders the person let every chat of it write, its cache home, every harness's own
//! homes and the temp directories; and, for a read the app makes for one chat, that chat's own
//! folder and what its sandbox was compiled to let it write ([`Ctx::chat`]). A read with no
//! chat behind it is held to what the project's sandbox would grant any chat, so the answer
//! does not depend on who asks.
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
    /// The file to run: the one the disk names, with every link resolved.
    pub path: PathBuf,
    /// Where the search found it, which may be a link to [`Self::path`]. What a pin records:
    /// an installer's link keeps its name across an upgrade and the file behind it does not.
    pub found: PathBuf,
    pub route: Route,
}

/// Why no provider's program is run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotRun {
    /// No directory holds one.
    NotFound(NotFound),
    /// The only one found is where a chat may write.
    Writable {
        /// The file, as it was found or as the disk names it: whichever spelling lies where a
        /// chat may write.
        path: PathBuf,
        /// Every directory searched, in order.
        looked: Vec<PathBuf>,
    },
}

impl NotRun {
    /// Every directory that was searched, in order.
    pub fn looked(&self) -> &[PathBuf] {
        match self {
            Self::NotFound(not) => &not.looked,
            Self::Writable { looked, .. } => looked,
        }
    }
}

/// Every directory searched for a provider's program, in order.
///
/// [`programs::search_dirs_from`]'s list for `path` and `home`, less the machine-wide
/// directories `path` does not name when `machine_wide` is off.
pub fn places_from(path: Option<&OsStr>, home: Option<&Path>, machine_wide: bool) -> Vec<Place> {
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
    programs::search_dirs_from(path, home)
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
        .collect()
}

/// `path` as the disk names it, as far as it exists.
fn real(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

/// `$HOME` in `ctx`'s environment, when it is an absolute path.
fn home_of(ctx: &Ctx) -> Option<PathBuf> {
    ctx.env
        .get("HOME")
        .map(PathBuf::from)
        .filter(|home| home.is_absolute())
}

#[cfg(test)]
thread_local! {
    /// A test's seam, as [`crate::sandbox::grant`]'s is: a fixture's stand-in program is made in
    /// a temp folder, which the rule refuses as it must. A test of the rest turns the temp
    /// directories off for its own thread ([`stand_ins_live_in_temp_folders`]);
    /// the rule has tests of its own.
    pub(crate) static TEMP_COUNTS: std::cell::Cell<bool> = const { std::cell::Cell::new(true) };
}

/// Turns the temp directories off, for this thread, as a place a chat may write: called by a
/// fixture that makes a stand-in program in one.
#[cfg(test)]
pub(crate) fn stand_ins_live_in_temp_folders() {
    TEMP_COUNTS.with(|counts| counts.set(false));
}

/// Whether the temp directories count as a place a chat may write: always, in a build that
/// ships.
#[cfg(test)]
fn temp_counts() -> bool {
    TEMP_COUNTS.with(std::cell::Cell::get)
}

/// See the shipped arm. A fenced build that is not this crate's own tests is another crate's
/// test, or the command a test runs: its fixtures hold their stand-in programs in a temp
/// folder, in a process no thread-local seam reaches. Compile time only, as the fence is.
#[cfg(all(not(test), feature = "fenced"))]
fn temp_counts() -> bool {
    false
}

#[cfg(all(not(test), not(feature = "fenced")))]
fn temp_counts() -> bool {
    true
}

impl Ctx {
    /// This context, reading for the chat whose folder is `folder` and whose sandbox was
    /// compiled to `confines`: what that chat may write is added to where no provider's program
    /// is run from. The app's record of the chat, never anything the chat sent.
    pub fn chat(mut self, confines: &crate::sandbox::Confines, folder: Option<&Path>) -> Self {
        self.chat_writes.extend(folder.map(Path::to_path_buf));
        self.chat_writes.extend(confines.writable.iter().cloned());
        self.chat_writes.extend(
            confines
                .widened
                .caches
                .iter()
                .flat_map(|caches| caches.writable()),
        );
        self
    }

    /// Every directory this context searches for a provider's program, in order.
    pub fn program_places(&self) -> Vec<Place> {
        let path = self.env.get("PATH");
        places_from(
            path.as_deref().map(OsStr::new),
            home_of(self).as_deref(),
            !crate::fence::FENCED,
        )
    }

    /// Where a chat may write, each folder as named and as the disk names it: the project, the
    /// folders the person let every chat of it write, its cache home, every harness's own homes
    /// and the temp directories, and what [`Self::chat`] added for the chat this reads for.
    pub fn chat_writes(&self) -> Vec<(PathBuf, PathBuf)> {
        let machine = crate::sandbox::Machine {
            env: self.env.clone(),
            home: home_of(self),
            os: crate::sandbox::Os::this(),
        };
        let mut homes = crate::sandbox::Homes::of(&machine);
        homes.codex_project = crate::sandbox::Homes::codex_project(&machine, &self.root);
        let mut named = vec![self.root.clone()];
        named.extend(self.chat_writes.iter().cloned());
        named.extend(crate::sandbox::local::granted_writes(&self.root));
        named.extend(crate::sandbox::caches::root_of(&machine, &self.root));
        named.extend(crate::sandbox::grant::harness_homes(&machine, &homes));
        if temp_counts() {
            let tmpdir: Vec<(String, String)> = self
                .env
                .get("TMPDIR")
                .map(|dir| ("TMPDIR".to_owned(), dir))
                .into_iter()
                .collect();
            named.extend(crate::sandbox::program::temp_roots(&tmpdir));
        }
        named
            .into_iter()
            .filter(|dir| dir.is_absolute())
            .map(|dir| {
                let real = real(&dir);
                (dir, real)
            })
            .collect()
    }

    /// The provider's program `name`: the first one in the search that is nowhere a chat may
    /// write, as the file the disk names, with the route it was found by. Or why none is run:
    /// none was found, or the only one found is where a chat may write.
    pub fn program(&self, name: &str) -> Result<Found, NotRun> {
        let places = self.program_places();
        let writes = self.chat_writes();
        let mut passed_over: Option<PathBuf> = None;
        for place in &places {
            let Some(found) = programs::find(name, std::slice::from_ref(&place.dir)) else {
                continue;
            };
            let real = real(&found);
            let writable = [&found, &real]
                .into_iter()
                .find(|path| crate::sandbox::program::writable(path, &writes))
                .cloned();
            match writable {
                Some(path) => {
                    passed_over.get_or_insert(path);
                }
                None => {
                    return Ok(Found {
                        path: real,
                        found,
                        route: place.route,
                    });
                }
            }
        }
        let looked: Vec<PathBuf> = places.into_iter().map(|place| place.dir).collect();
        Err(match passed_over {
            Some(path) => NotRun::Writable { path, looked },
            None => NotRun::NotFound(NotFound {
                program: name.to_owned(),
                looked,
            }),
        })
    }

    /// The refusal for `why`: `what` is the program as a person names it ("the 1Password CLI
    /// ('op')") and `install` what to do where there is none. Every path is whole.
    pub fn not_run(&self, what: &str, install: &str, why: &NotRun) -> String {
        match why {
            NotRun::NotFound(_) => format!(
                "purlis could not find {what}. {install} {}",
                self.looked_in(why.looked())
            ),
            NotRun::Writable { path, .. } => format!(
                "purlis found {what} only where a chat can write: {}, so it was not run. Keep \
                 the program outside the project and outside what a chat may write. {}",
                crate::shown::readable(&path.display().to_string(), usize::MAX),
                self.looked_in(why.looked())
            ),
        }
    }

    /// The end of a refusal: every directory that was searched, whole, and where to link a
    /// program that is installed somewhere else.
    pub fn looked_in(&self, looked: &[PathBuf]) -> String {
        if looked.is_empty() {
            return "It had no directory to look in: this process has no PATH and no home \
                    directory."
                .to_owned();
        }
        let listed = NotFound {
            program: String::new(),
            looked: looked.to_vec(),
        }
        .looked_in();
        match home_of(self) {
            Some(home) => format!(
                "It looked in: {listed}. If it is installed somewhere else, put a link to it \
                 in {}.",
                crate::shown::readable(
                    &home.join(programs::USER_BIN[0]).display().to_string(),
                    usize::MAX
                )
            ),
            None => format!("It looked in: {listed}."),
        }
    }
}

#[cfg(test)]
#[path = "program_tests.rs"]
mod tests;
