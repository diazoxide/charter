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

/// A fix charter can make, by the id every surface names it with.
///
/// Declared in the order `charter doctor --fix` applies them, which is also their sort order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FixId {
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
    /// `charter persona optimize --all --apply` and `charter workspace optimize --all --apply`,
    /// for each kind of memory that has a file its `MEMORY.md` does not list. It appends a link
    /// line to `MEMORY.md` for each such file. It moves each extra copy of a memory that is an
    /// exact duplicate of another into `memory/archive/`, where `unarchive` brings it back. It
    /// deletes nothing and edits no memory's text; everything else stays a proposal. Offered
    /// by the `memory indexes` row when a memory is unindexed.
    MemoryOptimize,
    /// `charter discover`: asks each forge `charter.toml` declares which repos it lists. It
    /// adds them to `inventory/repos.json`, keeping every repo already there. It rewrites
    /// charter's generated `docs/topology.md`, and the span between charter's markers in
    /// `README.md` when there is one. Offered by the `inventory` row when the inventory is
    /// empty and the project declares a forge.
    Discover,
}

impl FixId {
    /// Every fix, in the order `charter doctor --fix` applies them.
    pub const ALL: [FixId; 5] = [
        FixId::PluginInstall,
        FixId::Reinit,
        FixId::LocalIgnore,
        FixId::MemoryOptimize,
        FixId::Discover,
    ];

    /// The id, as `charter doctor --fix <id>`, `--json` and the window spell it.
    pub const fn id(self) -> &'static str {
        match self {
            Self::PluginInstall => "plugin-install",
            Self::Reinit => "reinit",
            Self::LocalIgnore => "local-ignore",
            Self::MemoryOptimize => "memory-optimize",
            Self::Discover => "discover",
        }
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
        return match this_machine() {
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
    if id == FixId::PluginInstall {
        return match machine {
            Some(machine) => plugin_install(machine),
            None => Fixed::Refused("charter cannot tell this machine's harnesses".to_owned()),
        };
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
        FixId::PluginInstall => unreachable!("answered above"),
    }
}

/// This machine as `charter plugin install` finds it: this binary, by its resolved path, and
/// the plugin shipped beside it.
fn this_machine() -> Result<crate::plugin_install::Machine, String> {
    let binary = std::env::current_exe()
        .and_then(|p| p.canonicalize())
        .map_err(|e| format!("cannot tell where this charter is, so no hook could name it: {e}"))?;
    let bundle = crate::plugin_install::bundle_beside(&binary);
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

fn local_ignore(root: &Path) -> Fixed {
    let check = crate::profiles::ignore_check(root);
    if check.passes() {
        return Fixed::Ran {
            said: vec!["✓ git does not carry charter.local.toml — nothing to do.".to_owned()],
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
    let kinds = super::memory::unindexed_kinds(root);
    if kinds.is_empty() {
        return Fixed::Ran {
            said: vec!["✓ every memory is in its index — nothing to do.".to_owned()],
            complete: true,
        };
    }
    let today = chrono::Local::now().date_naive();
    let mut said: Vec<String> = Vec::new();
    let mut code = 0;
    let mut sink = |line: crate::repocmd::Say| {
        said.extend(line.to_string().lines().map(str::to_owned));
    };
    if kinds.contains("persona") {
        let ask = crate::personaverbs::upkeep::Optimize {
            name: None,
            all: true,
            apply: true,
            stale_days: STALE_DAYS,
            today,
        };
        code |= crate::personaverbs::upkeep::optimize(root, &ask, &mut || {}, &mut sink);
    }
    if kinds.contains("workspace") {
        let names = crate::recall::read_workspaces(root)
            .map(|(names, _)| names)
            .unwrap_or_default();
        code |= crate::curate::optimize_workspaces(
            root,
            &names,
            true,
            STALE_DAYS,
            today,
            &mut || {},
            &mut sink,
        );
    }
    // Complete only when nothing is left unindexed: a store whose index could not be written
    // says so in `said` and still answers 0.
    let left = super::memory::unindexed_kinds(root);
    Fixed::Ran {
        said: said.into_iter().filter(|line| !line.is_empty()).collect(),
        complete: code == 0 && left.is_empty(),
    }
}

/// The age past which `optimize` proposes a memory for review: the commands' default.
const STALE_DAYS: i64 = 90;

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

/// Why no fix may write at `root`, or `None` when one may.
fn refusal(root: &Path) -> Option<String> {
    // A `charter.toml` that is a link out of the project: reinit's own containment gate refuses
    // it with the words that name the link, and writes nothing.
    if crate::scaffold::manifest_escapes(root) {
        return None;
    }
    if !root.join(crate::plane::MANIFEST).is_file() {
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
