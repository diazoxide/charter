//! **The fix registry** (FX-1, V91d, V91p): the doctor findings charter can fix itself, each
//! under one **fix id**, and the one entry point that applies a fix by that id.
//!
//! A row that charter can fix carries its id ([`super::Row::fix`]). `charter doctor --fix
//! [<id>]` and the Doctor dialog's Fix button both call [`apply`], so a fix is written once and
//! the terminal and the window can never disagree about what it does.
//!
//! # What is never a fix
//!
//! **Removing a git index lock** (V91p). Charter's rule is that it never removes a lock: a lock
//! that looks crashed can belong to a git that is still writing, and deleting it is how an
//! index is corrupted. The `index lock` row says how to check and leaves the removal to the
//! operator, so it carries no id, and no id here could name it.
//!
//! # What a fix answers
//!
//! What it changed, line by line, or why it refused ([`Fixed`]). A refusal is decided before
//! anything is written: a fix on a project this charter may not write (FR-24), or where there
//! is no project, writes nothing and says why. `plugin-install` is the one fix that writes no
//! project file, so it is the one that runs with no project too.
//!
//! # No fix removes or replaces your content
//!
//! Each id's doc below says exactly what it writes. A fix adds what is missing, or rewrites a
//! file charter generates and owns; it never deletes a line or a file you wrote.

use std::path::Path;

use crate::scaffold::Say;

pub mod identity;
pub mod rename_plane;

/// A fix charter can make, by the id every surface names it with.
///
/// Declared in the order `charter doctor --fix` applies them, which is also their sort order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FixId {
    /// `purlis migrate` (RN-5, V93f): moves this machine's local state to the purlis names — the
    /// config home's folder and the session host's folder in it, the data home, the app's log
    /// folder, and in this project and each one this machine remembers, `charter.local.toml`
    /// and `.charter/`. It writes no committed file: git is made to ignore the new names through
    /// the repository's own `info/exclude`. Every move is journalled, and `purlis migrate --undo`
    /// puts each one back. A name that is there under both spellings is left as it is, never
    /// merged. It refuses, moving nothing, while a running charter has a project open. Applied
    /// only by name ([`FixId::by_name_only`]): it moves this machine's folders, not one project's.
    RenameLocal,
    /// `charter plugin install`: makes charter's plugin load in the chats started outside the
    /// app. It writes this machine's harness configuration and no project file. It copies the
    /// plugin into charter's own folder and enables it in Claude Code's user `settings.json`.
    /// It adds charter's guard hook to Codex's `config.toml` and writes opencode's
    /// `plugin/charter.ts`, replacing that file only when charter wrote it. It turns off the
    /// retired `charter@charter` where your user settings enable it. Offered by the
    /// `plugin install`, `plugin` and `plugin files` rows. It refuses, writing nothing, where
    /// charter cannot tell where this machine's harnesses keep their configuration.
    PluginInstall,
    /// `charter reinit`: adds what the project is missing and never removes or replaces your
    /// content. It creates missing baseline folders and appends missing `.gitignore` lines. It
    /// also rewrites charter's own managed block in `.gitattributes` and merges charter's
    /// entries into `.claude/settings.json`. Offered by the `schema` row when a baseline folder
    /// is missing.
    Reinit,
    /// Appends the one line `/charter.local.toml` to `.gitignore`, creating the file when there
    /// is none. Every line already there is kept. Offered by the `harness profiles` row when
    /// git would commit `charter.local.toml`. It refuses a file git already tracks: an ignore
    /// line does not untrack it, and charter never runs `git rm` for you.
    LocalIgnore,
    /// Repairs each memory index: appends one link line to `MEMORY.md` for each memory file it
    /// does not list, the index repair `charter persona|workspace optimize --apply` makes. Every
    /// line already in the index is kept, and no memory is moved, merged or edited. Collapsing
    /// exact duplicates stays with `optimize --apply`, which you run when you mean it. Offered
    /// by the `memory indexes` row when a memory is unindexed.
    MemoryOptimize,
    /// `charter discover`: asks each forge `charter.toml` declares which repos it lists. It
    /// adds them to `inventory/repos.json`, keeping every repo already there. It rewrites
    /// charter's generated `docs/topology.md`, and the span between charter's markers in
    /// `README.md` when there is one. Offered by the `inventory` row when the inventory is
    /// empty and the project declares a forge. Applied only by name ([`FixId::by_name_only`]):
    /// it goes over the network, so bare `charter doctor --fix` never runs it.
    Discover,
    /// The git identity a commit is made with (FX-3): `user.name` and `user.email`, written to
    /// git's global config, the scope the `git identity` row's hint names. Offered by that row
    /// when either is unset. **It takes input** ([`FixId::takes_input`]): a name and an email,
    /// from the window's form or `--name`/`--email`, applied through [`identity::apply`].
    GitIdentity,
    /// **`rename-plane`** (RN-7, V93g): the project's committed files under purlis's names, in
    /// ONE commit. It renames `charter.toml` to `purlis.toml` and adds `requires = [{ feature =
    /// "purlis-names" }]` to it, so a build without that feature opens the project read-only.
    /// It renames `.charter-scan-allow.toml`, the managed blocks' markers (`.gitattributes`,
    /// `.gitignore`, the personas block in `README.md`, each generated agent), a committed
    /// `workspace.json`'s digest key, `.claude/settings.json`'s `CHARTER_HARNESS`, and personas'
    /// `charter:` skill references. Each `charter …` ask or deny rule gets its `purlis …` twin
    /// and is kept; hook commands keep `charter` for the window (D-RN7-11). It changes nothing
    /// else and removes none of your content. Applied only by name ([`FixId::by_name_only`]):
    /// it is a commit every teammate pulls. It refuses, writing nothing, from inside a chat, on
    /// a project with uncommitted changes, outside git, or with a file under both names
    /// ([`rename_plane`]).
    RenamePlane,
}

impl FixId {
    /// Every fix, in the order `charter doctor --fix` applies them.
    pub const ALL: [FixId; 8] = [
        FixId::RenameLocal,
        FixId::PluginInstall,
        FixId::Reinit,
        FixId::LocalIgnore,
        FixId::MemoryOptimize,
        FixId::Discover,
        FixId::GitIdentity,
        FixId::RenamePlane,
    ];

    /// The id, as `charter doctor --fix <id>`, `--json` and the window spell it.
    pub const fn id(self) -> &'static str {
        match self {
            Self::RenameLocal => "rename-local",
            Self::PluginInstall => "plugin-install",
            Self::Reinit => "reinit",
            Self::LocalIgnore => "local-ignore",
            Self::MemoryOptimize => "memory-optimize",
            Self::Discover => "discover",
            Self::GitIdentity => "git-identity",
            Self::RenamePlane => "rename-plane",
        }
    }

    /// Whether this fix runs only when it is named — `charter doctor --fix <id>`, or its Fix
    /// button — and never from a bare `charter doctor --fix` (D-FX2-9). The fixes bare `--fix`
    /// applies are local and additive; `discover` asks a forge over the network and writes the
    /// inventory and the docs, so it waits to be asked. `rename-plane` makes a commit every
    /// teammate pulls, so it is never automatic (V93g). `rename-local` moves this machine's
    /// folders, not the project's, so it waits to be asked too (D-RN5-5).
    pub const fn by_name_only(self) -> bool {
        matches!(self, Self::Discover | Self::RenamePlane | Self::RenameLocal)
    }

    /// Whether this fix needs the operator's input before it can be applied (FX-3, D-FX3-1).
    /// Such a fix is applied through its own entry point with that input — the window draws a
    /// form for it, the CLI takes flags — and [`apply`] by its id alone refuses, saying what to
    /// give.
    pub const fn takes_input(self) -> bool {
        matches!(self, Self::GitIdentity)
    }

    /// The fix an id names, or `None` for an id that names no fix.
    pub fn parse(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|fix| fix.id() == id)
    }
}

impl std::fmt::Display for FixId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.id())
    }
}

/// What applying a fix came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fixed {
    /// The fix ran. `said` is what it changed, in the words its command prints, including
    /// "nothing to do" when nothing needed changing. `complete` is false when part of it could
    /// not be done; `said` names that part.
    Ran { said: Vec<String>, complete: bool },
    /// Refused before anything was written, and why.
    Refused(String),
}

impl Fixed {
    /// Whether the fix did everything it was asked to.
    pub fn complete(&self) -> bool {
        matches!(self, Self::Ran { complete: true, .. })
    }

    /// Every line, as a terminal prints it.
    pub fn lines(&self) -> Vec<String> {
        match self {
            Self::Ran { said, .. } => said.clone(),
            Self::Refused(why) => vec![format!("✗ refused: {why}")],
        }
    }
}

/// Apply the fix `id` to the project at `root`.
///
/// The one entry point: the CLI and the window both call it. It refuses, writing nothing, where
/// there is no project and on a project this charter may not write. `plugin-install` installs
/// for this machine as this process finds it, naming this binary in the hooks: what
/// `charter plugin install` does. The window names its own `charter` with [`apply_for`].
pub fn apply(root: &Path, id: FixId) -> Fixed {
    if id == FixId::PluginInstall {
        return match this_machine(None) {
            Ok(machine) => apply_for(root, id, &machine),
            Err(why) => Fixed::Refused(why),
        };
    }
    applied(root, id, None)
}

/// [`apply`], installing `plugin-install` on `machine`: the window's way in, since the `charter`
/// its hooks run is the one beside the app and not the app itself.
pub fn apply_for(root: &Path, id: FixId, machine: &crate::plugin_install::Machine) -> Fixed {
    applied(root, id, Some(machine))
}

fn applied(root: &Path, id: FixId, machine: Option<&crate::plugin_install::Machine>) -> Fixed {
    // A fix that takes input has nothing to write without it, project or not: said first.
    if id.takes_input() {
        return needs_input(id);
    }
    if id == FixId::PluginInstall {
        return match machine {
            Some(machine) => plugin_install(machine),
            None => Fixed::Refused("charter cannot tell this machine's harnesses".to_owned()),
        };
    }
    if id == FixId::RenameLocal {
        return rename_local(root);
    }
    if let Some(why) = refusal(root) {
        return Fixed::Refused(why);
    }
    match id {
        FixId::Reinit => ran(crate::scaffold::reinit(&crate::plane::Place {
            root: root.to_path_buf(),
            is_plane: true,
        })),
        FixId::LocalIgnore => local_ignore(root),
        FixId::MemoryOptimize => memory_optimize(root),
        FixId::Discover => discover(root),
        FixId::GitIdentity => needs_input(id),
        FixId::RenamePlane => rename_plane::apply(root),
        FixId::PluginInstall | FixId::RenameLocal => unreachable!("answered above"),
    }
}

/// This machine as `charter plugin install` finds it: this binary, by its resolved path, and
/// the plugin `plugin_from` names, else the one shipped beside this binary. The CLI's
/// `plugin install` and this registry's `plugin-install` both build it here, so the two install
/// the same copy.
pub fn this_machine(plugin_from: Option<&Path>) -> Result<crate::plugin_install::Machine, String> {
    let binary = std::env::current_exe()
        .and_then(|p| p.canonicalize())
        .map_err(|e| format!("cannot tell where this charter is, so no hook could name it: {e}"))?;
    let bundle = match plugin_from {
        Some(dir) => Some(
            dir.canonicalize()
                .map_err(|e| format!("--plugin-from {}: {e}", dir.display()))?,
        ),
        None => crate::plugin_install::bundle_beside(&binary),
    };
    crate::plugin_install::Machine::from_env(binary, bundle)
}

fn plugin_install(machine: &crate::plugin_install::Machine) -> Fixed {
    use crate::plugin_install as install;
    let outcomes = install::run(machine, install::Verb::Install, &[], false);
    Fixed::Ran {
        said: install::render(&outcomes, false)
            .lines()
            .map(str::to_owned)
            .collect(),
        complete: !install::failed(&outcomes),
    }
}

/// `rename-local`: this machine's local state, with the project at `root` among the projects
/// moved when it is one. A project this charter may not write is left as it is by the move
/// itself, which asks each project.
fn rename_local(root: &Path) -> Fixed {
    let here: Vec<std::path::PathBuf> = crate::names::has_manifest(root)
        .then(|| root.to_path_buf())
        .into_iter()
        .collect();
    let Some(local) = crate::renamelocal::Local::of_this_machine(&here) else {
        return Fixed::Refused(
            "charter cannot tell where this machine's config home is, so there is nowhere to \
             journal a move; nothing was moved"
                .to_owned(),
        );
    };
    moved(crate::renamelocal::run(
        &local,
        &crate::renamelocal::Seams::real(),
    ))
}

/// A [`crate::renamelocal::Moved`] as a fix's answer.
pub fn moved(moved: crate::renamelocal::Moved) -> Fixed {
    match moved.refused {
        Some(why) => Fixed::Refused(why),
        None => Fixed::Ran {
            said: moved.said,
            complete: moved.complete,
        },
    }
}

fn local_ignore(root: &Path) -> Fixed {
    if crate::scaffold::gitignore_is_a_link(root) {
        return Fixed::Refused(
            "this project's .gitignore is a symbolic link, which git does not read, so a line \
             added to it would change nothing; nothing was written. Replace the link with a \
             real .gitignore, then run the fix again."
                .to_owned(),
        );
    }
    let check = crate::profiles::ignore_check(root);
    if check.passes() {
        return Fixed::Ran {
            said: vec![format!(
                "✓ git does not carry {} — nothing to do.",
                crate::names::LOCAL_SETTINGS.spelling_at(root, Path::is_file)
            )],
            complete: true,
        };
    }
    if !check.ignorable {
        // Tracked, or git could not say: an ignore line is no cure for either, and untracking
        // a file is the operator's step, never charter's.
        return Fixed::Refused(format!(
            "an ignore line does not cure this, so nothing was written. {}",
            check.reason
        ));
    }
    match ran(crate::scaffold::ignore_local_profiles(root)) {
        // Asked again: a later `!` line, or a rule elsewhere, can keep git carrying the file
        // whatever this line says.
        Fixed::Ran { mut said, complete } => {
            let still = crate::profiles::ignore_check(root);
            if !still.passes() {
                said.push(format!("✗ {}", still.reason));
            }
            Fixed::Ran {
                said,
                complete: complete && still.passes(),
            }
        }
        refused => refused,
    }
}

fn memory_optimize(root: &Path) -> Fixed {
    let bases = super::memory::unindexed_bases(root);
    if bases.is_empty() {
        return Fixed::Ran {
            said: vec!["✓ every memory is in its index — nothing to do.".to_owned()],
            complete: true,
        };
    }
    let mut said: Vec<String> = Vec::new();
    let mut read = true;
    for (label, dir) in &bases {
        match crate::curate::link_unindexed(root, dir) {
            // A line that says the index was not repaired is a failure, and is marked as one.
            Ok(actions) => said.extend(actions.iter().map(|a| {
                let mark = if a.starts_with("index NOT repaired") {
                    "✗"
                } else {
                    "✓"
                };
                format!("{mark} {label}: {a}")
            })),
            Err(unread) => {
                read = false;
                for (path, code) in &unread {
                    said.push(format!(
                        "✗ {}",
                        crate::memstore::cannot_check(root, path, *code)
                    ));
                }
            }
        }
    }
    // Complete only when nothing is left unindexed: an index that could not be written says so
    // in `said` without failing the call.
    Fixed::Ran {
        said,
        complete: read && super::memory::unindexed_bases(root).is_empty(),
    }
}

fn discover(root: &Path) -> Fixed {
    let mut said: Vec<String> = Vec::new();
    let mut failed: Vec<String> = Vec::new();
    let mut sink = |line: crate::repocmd::Say| {
        use crate::repocmd::Say;
        if matches!(line, Say::Plain(_) | Say::Fail(_)) {
            failed.push(line.to_string());
        }
        said.push(line.to_string());
    };
    let code = crate::repocmd::discover::discover(
        root,
        crate::repocmd::discover::Options::default(),
        &mut sink,
    );
    // Discover saves nothing until every forge has answered, so a failure wrote nothing.
    if code != 0 {
        return Fixed::Refused(failed.join(" "));
    }
    Fixed::Ran {
        said,
        complete: true,
    }
}

/// The refusal of a fix that [takes input](FixId::takes_input), asked for by its id alone.
fn needs_input(id: FixId) -> Fixed {
    Fixed::Refused(match id {
        FixId::GitIdentity => identity::NEEDS_INPUT.to_owned(),
        other => format!("{other} needs input this caller did not give"),
    })
}

/// Why no fix may write at `root`, or `None` when one may.
fn refusal(root: &Path) -> Option<String> {
    // A `charter.toml` that is a link out of the project: reinit's own containment gate refuses
    // it with the words that name the link, and writes nothing.
    if crate::scaffold::manifest_escapes(root) {
        return None;
    }
    if !crate::names::has_manifest(root) {
        return Some(format!(
            "no project at {} (it has no charter.toml), so there is nothing to fix",
            super::fsx::path_field(root)
        ));
    }
    match crate::compat::read(root) {
        crate::compat::Compat::Writable => None,
        crate::compat::Compat::ReadOnly(why) => Some(why.to_string()),
    }
}

fn ran(outcome: crate::scaffold::Outcome) -> Fixed {
    // A command that wrote nothing and only said why is a refusal, whatever it was named.
    let refused = outcome.code != 0 && outcome.said.iter().all(|s| matches!(s, Say::Err(_)));
    if refused {
        let why: Vec<&str> = outcome
            .said
            .iter()
            .filter_map(|s| match s {
                Say::Err(text) => Some(text.as_str()),
                _ => None,
            })
            .collect();
        return Fixed::Refused(why.join(" "));
    }
    Fixed::Ran {
        said: outcome.said.iter().map(Say::marked).collect(),
        complete: outcome.code == 0,
    }
}
