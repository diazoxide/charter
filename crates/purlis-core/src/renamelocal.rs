//! `rename-local` (RN-5, V93f): this machine's local state moved to the purlis names, every
//! move journalled, and `purlis migrate --undo` to put each one back.
//!
//! **What moves**, each only when the old name is there and the purlis one is not:
//!
//! * the config home's folder, `charter/` → `purlis/` ([`crate::machine::dir`]);
//! * the session host's folder inside it, `charterd/` → `purlisd/` (V93a);
//! * the data home's folder, unless `$PURLIS_DATA_HOME` names it ([`crate::datahome`]);
//! * the app's log folder, unless `$PURLIS_LOG_DIR` names it ([`crate::applog`]);
//! * in each project this machine remembers (and the one a doctor fix is run in):
//!   `charter.local.toml` → `purlis.local.toml`, and `.charter/` → `.purlis/`.
//!
//! * in each of those projects, the keychain items of its keyring vaults and of its identity
//!   records, copied from `charter/…` to `purlis/…` and read back before anything reads them
//!   there ([`keychain`], RN-6, V93h). The old items are kept. The copy never lets the Keychain
//!   ask: a vault whose items would ask waits ([`Waiting`]) until the person finishes it
//!   ([`finish`], #1306).
//!
//! And **the harness plugin** (RN-8, #1266), where `local.plugin` says which charter and bundle
//! to install it from: every harness charter's plugin is installed for is installed again under
//! the purlis names and taken out from under the old ones (`plugin_install::move_ids`) — the
//! Claude Code copy lives in the config home's folder, so its registration has to follow the
//! move — the opencode shim becomes `purlis.ts`, and the Codex guard runs this charter. That
//! step rewrites files rather than renaming them, so its journal entry says only that it was
//! made, and the undo installs under the old names again, after every move is put back.
//!
//! Nothing else: committed project files are RN-7's.
//!
//! **A move is a rename, so it is whole or not at all.** Each is written to the journal before it
//! is made, and the journal is what undo replays backwards. A rename that fails leaves the old
//! name exactly where it was, and every reader still finds it there, because every reader picks
//! the purlis spelling only when it exists (`names::Name::folder_at`). A move whose two names are
//! both there is never made: rename-local never merges two folders.
//!
//! **What is trusted once it exists.** A project's `.purlis/` beside a `.charter/` is trusted only
//! when the state-moved record lists the project (D-RN2a-7). The machine's own folders have no
//! such record: `machine::dir`, `datahome` and `applog` read the purlis spelling of the config
//! home, the session host's folder, the data home and the log folder the moment it exists
//! (`Name::folder_at`). The config home is the anchor everything else is vouched for by, so no
//! record could vouch for it; the defence there is the sandbox, which denies a chat writing any
//! spelling of the config home, there or not (`sandbox`, RN-5).
//!
//! **A project's state folder is ignored before it exists.** `.purlis/` and
//! `purlis.local.toml` are added to the repository's own `info/exclude` before the move (#1277):
//! the committed `.gitignore` is the project's and RN-7's, and still names only `/.charter/`.
//!
//! **The state-moved record** (`names::STATE_MOVED_RECORD`) lists each project whose state
//! folder moved, as it resolves, so `names::state` trusts `.purlis/` even when an older build
//! makes a second `.charter/` beside it. It lives in the config home's folder, so it moves with
//! that folder and is found through [`crate::machine::dir`] before the move and after.
//!
//! **Nothing moves under a running charter** (D-RN5-11). A move under one would split its state
//! between two names for good. [`busy`] says what counts as running, conservatively; a run that
//! finds anything moves nothing and says to quit every charter first, and the app tries again at
//! its next launch.
//!
//! **An undo is finished or it is pending.** `Undone` is journalled only when every step was put
//! back; until then the steps left stay pending, a second `--undo` finishes them, and the app's
//! launch moves nothing while one is pending, so an older build is never locked out of the names
//! it reads. Every entry is checked against the moves this machine's rename-local can make
//! before any is replayed, and a journal holding any other is refused whole.

pub mod busy;
pub mod keychain;

use std::collections::BTreeMap;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::names::{
    BUNDLE_ID, CONFIG_HOME, DAEMON_DIR, DATA_HOME, LOCAL_SETTINGS, STATE_DIR, STATE_MOVED_RECORD,
};

/// The journal, inside charter's directory in the config home, beside the state-moved record.
pub const JOURNAL: &str = "rename-local/journal.jsonl";

/// The command that puts every move back, as messages name it.
pub const UNDO_COMMAND: &str = "purlis migrate --undo";

/// Where this machine keeps the names rename-local moves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Local {
    /// The folder the config home's folder sits in (`~/.config`).
    pub config_root: PathBuf,
    /// The folder the data home's folder sits in; `None` when a variable names the data home.
    pub data_base: Option<PathBuf>,
    /// The app's log folder under its old and its purlis identifier; `None` when a variable
    /// names the log folder.
    pub logs: Option<Logs>,
    /// Projects to migrate besides the ones this machine remembers.
    pub planes: Vec<PathBuf>,
    /// The identifier of the app this runs in, at its launch: its own single-instance lock and
    /// socket are its own, not a second app's ([`busy::Instances::of`]). `None` in a terminal.
    pub own_app: Option<String>,
    /// The harnesses' folders, the charter a hook runs and the bundled plugin, for the plugin's
    /// step. With a bundle the plugin is installed under the purlis names; without one, Claude
    /// Code's registration is pointed at the copy where it moved (D-RN8-13). `None` only where
    /// the harnesses' folders cannot be told. Its `charter_dir` is not read: the step asks
    /// [`crate::machine::dir`] once the config home has moved.
    pub plugin: Option<crate::plugin_install::Machine>,
}

/// The app's log folder under each identifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Logs {
    pub old: PathBuf,
    pub new: PathBuf,
}

impl Local {
    /// This machine, as this process's environment names it, with `planes` besides the ones it
    /// remembers. `None` when there is no config home to keep a journal in.
    pub fn of_this_machine(planes: &[PathBuf]) -> Option<Self> {
        let config_root = crate::machine::config_root()?;
        let data_base = crate::datahome::base_in(&crate::envvar::var);
        if let Some(base) = &data_base {
            crate::fence::hold(crate::fence::Act::Store, base);
        }
        let logs = match crate::envvar::var_os("PURLIS_LOG_DIR") {
            Some(named) if !named.is_empty() => None,
            _ => {
                let old = crate::applog::log_dir_named(BUNDLE_ID.reads[0]);
                let new = crate::applog::log_dir_named(BUNDLE_ID.write);
                old.zip(new).map(|(old, new)| Logs { old, new })
            }
        };
        if let Some(logs) = &logs {
            crate::fence::hold(crate::fence::Act::Store, &logs.old);
        }
        Some(Self {
            config_root,
            data_base,
            logs,
            planes: planes.to_vec(),
            own_app: None,
            // The harnesses' folders, with no bundle: enough to keep Claude Code's registration
            // pointing at the copy when the config home moves. A caller with the app's plugin
            // hands it in instead.
            plugin: std::env::current_exe()
                .ok()
                .and_then(|exe| crate::plugin_install::Machine::from_env(exe, None).ok()),
        })
    }

    fn home(&self) -> PathBuf {
        crate::machine::dir(&self.config_root)
    }

    fn journal(&self) -> PathBuf {
        self.home().join(JOURNAL)
    }
}

/// What a run or an undo came to.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Moved {
    /// One line per step, marked `✓`, `✗` or `–`.
    pub said: Vec<String>,
    /// Whether every step it tried was done.
    pub complete: bool,
    /// Why nothing was tried at all.
    pub refused: Option<String>,
    /// Whether anything on disk changed.
    pub changed: bool,
    /// The vaults and identity records left on the old prefix because reading their items
    /// would have asked the person (#1306): [`finish`] moves them when the person asks.
    pub waiting: Vec<Waiting>,
}

/// A keyring vault, or a vault's identity record, that waits to move: it still reads its items
/// under `charter/…`, where they keep working, because reading them would make the system ask
/// the person for each (#1306).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Waiting {
    /// The project it is in, as it resolves.
    pub plane: PathBuf,
    /// The vault's name.
    pub vault: String,
    /// Whether it is the vault's identity record, rather than its keyring items.
    pub identity: bool,
    /// How many items it has: how many times the system may ask when it is finished.
    pub items: usize,
    /// Whether it already reads under `purlis/…` and waits to be held again by the app under
    /// its new identity (RN-9), rather than to be copied there.
    pub hold: bool,
}

impl Moved {
    fn new() -> Self {
        Self {
            complete: true,
            ..Self::default()
        }
    }

    fn refused(why: String) -> Self {
        Self {
            refused: Some(why),
            ..Self::default()
        }
    }

    fn done(&mut self, line: String) {
        self.changed = true;
        self.said.push(format!("✓ {line}"));
    }

    fn failed(&mut self, line: String) {
        self.complete = false;
        self.said.push(format!("✗ {line}"));
    }

    fn note(&mut self, line: String) {
        self.said.push(format!("– {line}"));
    }

    fn waits(&mut self, waiting: Waiting, line: String) {
        self.note(line);
        self.waiting.push(waiting);
    }
}

/// Why something is running, asked with the app's own identifier (or `None`) and the projects in
/// play ([`busy::why`]).
pub type Busy = dyn Fn(Option<&str>, &[PathBuf]) -> Option<String>;

/// How a run touches the world: a seam for the tests, [`Seams::real`] otherwise.
pub struct Seams<'a> {
    /// Renames `from` to `to`.
    pub rename: &'a dyn Fn(&Path, &Path) -> io::Result<()>,
    /// Why something that may hold charter's folders is running, given the projects in play, or
    /// `None` ([`busy::why`]). The config home's lock ([`busy::LOCK`]) is asked as well,
    /// whatever this says.
    pub busy: &'a Busy,
    /// The keychain a project's items are copied in, or `None` where reading them could ask
    /// the person ([`keychain::real`]).
    pub keyring: &'a keychain::Keyring,
}

fn real_rename(from: &Path, to: &Path) -> io::Result<()> {
    std::fs::rename(from, to)
}

impl Seams<'static> {
    /// The file system, and [`busy::why`].
    pub fn real() -> Self {
        Self {
            rename: &real_rename,
            busy: &busy::why,
            keyring: &keychain::real,
        }
    }
}

/// One journal line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "kebab-case")]
enum Entry {
    /// `from` is about to be renamed `to`.
    Move {
        what: String,
        from: PathBuf,
        to: PathBuf,
    },
    /// `plane` is about to be added to the state-moved record.
    Record { plane: PathBuf },
    /// `text` is about to be appended to `file`, which was `len` bytes long (`None`: absent).
    Append {
        file: PathBuf,
        len: Option<u64>,
        text: String,
    },
    /// The folder `path` is about to be made.
    Made { path: PathBuf },
    /// The keychain item `service`/`account` is about to be written by the keychain copy: an
    /// item there that differs from its original later is this copy's, out of date, and not
    /// one planted (RN-6). Undo leaves it.
    Copied { service: String, account: String },
    /// The keyring vault `vault` of `plane` is about to read its items under `to` instead of
    /// `from`, every one copied and read back; `keys` is each key's `updated` then.
    Switched {
        plane: PathBuf,
        vault: String,
        from: String,
        to: String,
        keys: BTreeMap<String, String>,
    },
    /// The identity record of vault `vault` in `plane`, with item ids `ids` by source, is about
    /// to be read under the purlis base, every item copied and read back.
    Rebased {
        plane: PathBuf,
        vault: String,
        ids: BTreeMap<String, String>,
    },
    /// The harness plugin is about to be installed under the purlis names and taken out from
    /// under the old ones; its undo installs it under the old names again.
    Plugin,
    /// Every entry before this one was undone.
    Undone,
    /// An undo began and is not finished: the launch moves nothing while this is the last word.
    Undoing,
}

fn private_dirs(dir: &Path) -> io::Result<()> {
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    builder.create(dir)
}

fn write_entry(local: &Local, entry: &Entry) -> io::Result<()> {
    let journal = local.journal();
    if let Some(parent) = journal.parent() {
        private_dirs(parent)?;
    }
    let mut options = std::fs::OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options.open(&journal)?;
    let mut line = serde_json::to_string(entry).map_err(io::Error::other)?;
    line.push('\n');
    file.write_all(line.as_bytes())?;
    file.sync_all()
}

fn read_journal(local: &Local) -> Vec<Entry> {
    std::fs::read_to_string(local.journal())
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

/// Whether the last thing done here was an undo: then nothing moves at launch until the operator
/// asks again (`purlis migrate`, or the `rename-local` fix).
pub fn undone(local: &Local) -> bool {
    matches!(
        read_journal(local).last(),
        Some(Entry::Undone | Entry::Undoing)
    )
}

fn there(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok()
}

fn real_dir(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|meta| meta.is_dir())
}

fn real_file(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|meta| meta.is_file())
}

/// The projects to migrate: this machine's remembered ones and `local.planes`, as each resolves,
/// once each, and only those still a project.
fn planes_of(local: &Local, more: &[PathBuf]) -> Vec<PathBuf> {
    let store = crate::machine::read(&local.config_root).store;
    let mut planes: Vec<PathBuf> = Vec::new();
    let named = store
        .recents
        .iter()
        .map(|recent| recent.plane.clone())
        .chain(local.planes.iter().cloned())
        .chain(more.iter().cloned());
    for plane in named {
        let Ok(real) = std::fs::canonicalize(&plane) else {
            continue;
        };
        if crate::names::has_manifest(&real) && !planes.contains(&real) {
            planes.push(real);
        }
    }
    planes
}

fn refused_while_running(why: &str, again: &str) -> String {
    format!(
        "{why}; nothing was moved. To move this machine's names, {}, then run `{again}` (the \
         app tries again at its next launch).",
        busy::QUIT_FIRST
    )
}

/// The exclusive lock and the busy check every run and undo starts with.
fn quiet(
    local: &Local,
    seams: &Seams,
    planes: &[PathBuf],
    again: &str,
) -> Result<crate::filelock::Held, String> {
    let lock =
        busy::exclusive(&local.config_root).map_err(|why| refused_while_running(&why, again))?;
    if let Some(why) = (seams.busy)(local.own_app.as_deref(), planes) {
        return Err(refused_while_running(&why, again));
    }
    Ok(lock)
}

/// Move this machine's local state to the purlis names (see the module).
pub fn run(local: &Local, seams: &Seams) -> Moved {
    let planes = planes_of(local, &[]);
    let _lock = match quiet(local, seams, &planes, "purlis migrate") {
        Ok(lock) => lock,
        Err(why) => return Moved::refused(why),
    };
    let mut moved = Moved::new();
    reconcile_record(local);

    let root = &local.config_root;
    move_one(
        local,
        seams,
        &mut moved,
        "the config home",
        &root.join(CONFIG_HOME.reads[0]),
        &root.join(CONFIG_HOME.write),
    );
    let home = local.home();
    move_one(
        local,
        seams,
        &mut moved,
        "the session host's folder",
        &home.join(DAEMON_DIR.reads[0]),
        &home.join(DAEMON_DIR.write),
    );
    if let Some(base) = &local.data_base {
        move_one(
            local,
            seams,
            &mut moved,
            "the data home",
            &base.join(DATA_HOME.reads[0]),
            &base.join(DATA_HOME.write),
        );
    }
    if let Some(logs) = &local.logs {
        move_logs(local, seams, &mut moved, logs);
    }
    for plane in &planes {
        move_plane(local, seams, &mut moved, plane);
    }
    // The app, at its first launch under an identity its keychain items were not held to (RN-9,
    // `dev.charter.app` → `dev.purlis.app`), holds the items under the purlis prefix again,
    // with the Keychain's dialogs off: one that would ask waits for the person, as the copy's
    // do. Before the copy, whose items the app makes fresh and so holds already.
    if let Some(own) = local.own_app.as_deref()
        && keychain::held_to(local).as_deref() != Some(own)
    {
        let mut done = keychain::done_for(local, own);
        let waited = moved.waiting.len();
        let mut whole = true;
        for plane in &planes {
            whole &= keychain::hold_again(
                seams,
                &mut moved,
                plane,
                own,
                keychain::Asking::Never,
                None,
                &mut done,
            );
        }
        let written = if whole && moved.waiting.len() == waited {
            keychain::record_held_to(local, own)
        } else {
            keychain::record_done(local, own, &done)
        };
        if let Err(e) = written {
            moved.failed(format!(
                "what is held again by {own} could not be recorded ({e}), so the app looks \
                 again at its next launch"
            ));
        }
    }
    // After every move, so the journal is read where it is now and each project's state folder
    // is the one it reads.
    let written = keychain::written(&read_journal(local));
    for plane in &planes {
        keychain::copy_plane(
            local,
            seams,
            &mut moved,
            plane,
            &written,
            keychain::Asking::Never,
            None,
        );
    }
    if let Some(m) = &local.plugin {
        move_plugin(local, &mut moved, m);
    }
    if moved.said.is_empty() {
        moved.note(
            "nothing to move: this machine's local state already has the purlis names".into(),
        );
    } else if moved.changed {
        moved.note(format!(
            "every move is journalled; `{UNDO_COMMAND}` puts them back"
        ));
    }
    moved
}

/// Rename `from` to `to` when `from` is there and `to` is not, journalled first.
fn move_one(
    local: &Local,
    seams: &Seams,
    moved: &mut Moved,
    what: &str,
    from: &Path,
    to: &Path,
) -> bool {
    if !there(from) {
        return false;
    }
    if there(to) {
        moved.failed(format!(
            "{what}: {} and {} are both there; rename-local never merges two, so both are left as \
             they are. Keep the one you want, move the other away, and run it again.",
            from.display(),
            to.display()
        ));
        return false;
    }
    let entry = Entry::Move {
        what: what.to_owned(),
        from: from.to_path_buf(),
        to: to.to_path_buf(),
    };
    if let Err(e) = write_entry(local, &entry) {
        moved.failed(format!(
            "{what}: the journal could not be written ({e}), so {} was not moved",
            from.display()
        ));
        return false;
    }
    match (seams.rename)(from, to) {
        Ok(()) => {
            moved.done(format!("{what}: {} → {}", from.display(), to.display()));
            true
        }
        Err(e) => {
            moved.failed(format!(
                "{what}: {} could not be moved ({e}); it stays where it is and is still read",
                from.display()
            ));
            false
        }
    }
}

/// The harness plugin under the purlis names (see the module), from `m` with charter's
/// directory where it is now. Without a bundle to install from, Claude Code's registration is
/// still pointed at the copy where it moved ([`crate::plugin_install::repoint`], D-RN8-13), and
/// what cannot be is a failed line naming `purlis plugin install`: never a guard silently gone.
fn move_plugin(local: &Local, moved: &mut Moved, m: &crate::plugin_install::Machine) {
    use crate::plugin_install as install;
    let mut m = m.clone();
    m.charter_dir = local.home();
    let wanted = if m.bundle.is_some() {
        install::adapters().any(|a| {
            a.home(&m).is_dir()
                && matches!(a.installed(&m), Ok(true))
                && a.install(&m).is_ok_and(|plan| plan.changes())
        })
    } else {
        match install::repoint(&m, true) {
            None => false,
            Some(probe) if probe.plan.is_err() => {
                report(moved, &[probe]);
                return;
            }
            Some(_) => true,
        }
    };
    if !wanted {
        return;
    }
    if let Err(e) = write_entry(local, &Entry::Plugin) {
        moved.failed(format!(
            "the harness plugin: the journal could not be written ({e}), so it was left as it is"
        ));
        return;
    }
    if m.bundle.is_some() {
        report(moved, &install::move_ids(&m, &install::NOW));
    } else {
        report(moved, &Vec::from_iter(install::repoint(&m, false)));
    }
}

/// The plugin's step put back: installed under the names it had before when `local.plugin`
/// has a bundle, once every move is undone and charter's directory is the old folder again;
/// without one, Claude Code's registration pointed at the copy where it moved back to.
fn unmove_plugin(local: &Local, moved: &mut Moved) {
    use crate::plugin_install as install;
    let Some(m) = &local.plugin else {
        moved.failed(
            "the harness plugin: this purlis cannot tell where the harnesses keep their \
             settings, so Claude Code may still name the plugin's old folder; run `purlis \
             plugin install`"
                .to_owned(),
        );
        return;
    };
    let mut m = m.clone();
    m.charter_dir = local.home();
    if m.bundle.is_some() {
        report(moved, &install::move_ids(&m, &install::BEFORE));
    } else {
        report(moved, &Vec::from_iter(install::repoint(&m, false)));
    }
}

/// Each harness's line, done or failed.
fn report(moved: &mut Moved, outcomes: &[crate::plugin_install::Outcome]) {
    for (ok, line) in crate::plugin_install::said(outcomes) {
        if ok {
            moved.done(line);
        } else {
            moved.failed(line);
        }
    }
}

fn move_logs(local: &Local, seams: &Seams, moved: &mut Moved, logs: &Logs) {
    if !there(&logs.old) || there(&logs.new) {
        // Nothing to move, or both there: `move_one` says the second.
        move_one(local, seams, moved, "the log folder", &logs.old, &logs.new);
        return;
    }
    // On Linux the log folder is `<identifier>/logs`, and the new identifier's folder may not be
    // there yet: made first, and journalled, so undo takes it away again.
    if let Some(parent) = logs.new.parent()
        && !there(parent)
    {
        let entry = Entry::Made {
            path: parent.to_path_buf(),
        };
        if let Err(e) = write_entry(local, &entry).and_then(|()| std::fs::create_dir(parent)) {
            moved.failed(format!(
                "the log folder: {} could not be made ({e}), so the logs stay in {}",
                parent.display(),
                logs.old.display()
            ));
            return;
        }
    }
    move_one(local, seams, moved, "the log folder", &logs.old, &logs.new);
}

fn move_plane(local: &Local, seams: &Seams, moved: &mut Moved, plane: &Path) {
    let settings = (
        plane.join(LOCAL_SETTINGS.reads[0]),
        plane.join(LOCAL_SETTINGS.write),
    );
    let state = (plane.join(STATE_DIR.reads[0]), plane.join(STATE_DIR.write));
    // A link is never moved: it could point anywhere, and the purlis name would then be wherever
    // it points (D-RN2a-7). Only a real file and a real folder are.
    let mut settings_moves = real_file(&settings.0);
    let mut state_moves = real_dir(&state.0);
    if !settings_moves && !state_moves {
        return;
    }
    if let crate::compat::Compat::ReadOnly(why) = crate::compat::read(plane) {
        moved.note(format!(
            "{}: left as it is, because this purlis may not write it ({why})",
            plane.display()
        ));
        return;
    }
    for (moves, (from, to)) in [(&mut settings_moves, &settings), (&mut state_moves, &state)] {
        if *moves && there(to) {
            *moves = false;
            moved.failed(format!(
                "{}: {} and {} are both there; rename-local never merges them or trusts what the \
                 purlis one holds, so both are left as they are",
                plane.display(),
                from.file_name().unwrap_or_default().to_string_lossy(),
                to.file_name().unwrap_or_default().to_string_lossy(),
            ));
        }
    }
    if !settings_moves && !state_moves {
        return;
    }
    // Ignored before it exists (#1277): a `.purlis/` git would carry, even for a moment, is the
    // project's state one `git add -A` from a commit.
    let mut names = Vec::new();
    if state_moves {
        names.push(format!("{}/", STATE_DIR.write));
    }
    if settings_moves {
        names.push(LOCAL_SETTINGS.write.to_owned());
    }
    if let Err(e) = ignore(local, plane, &names) {
        moved.failed(format!(
            "{}: git could not be made to ignore {} ({e}), so nothing in it was moved",
            plane.display(),
            names.join(" and ")
        ));
        return;
    }
    if settings_moves {
        move_one(
            local,
            seams,
            moved,
            &format!("{}'s local settings", plane.display()),
            &settings.0,
            &settings.1,
        );
    }
    if state_moves {
        if let Err(e) = record(local, plane) {
            moved.failed(format!(
                "{}: the state-moved record could not be written ({e}), so {} was not moved",
                plane.display(),
                STATE_DIR.reads[0]
            ));
            return;
        }
        let went = move_one(
            local,
            seams,
            moved,
            &format!("{}'s state folder", plane.display()),
            &state.0,
            &state.1,
        );
        // A record for a folder that did not move would vouch for a `.purlis/` nobody made.
        if !went {
            unrecord(local, moved, plane);
        }
    }
}

/// Take `plane` off the state-moved record.
fn unrecord(local: &Local, moved: &mut Moved, plane: &Path) {
    let file = record_file(local);
    let Ok(text) = std::fs::read_to_string(&file) else {
        return;
    };
    let lines: Vec<String> = text
        .lines()
        .filter(|line| Path::new(line.trim()) != plane)
        .map(str::to_owned)
        .collect();
    if let Err(e) = write_record(&file, &lines) {
        moved.failed(format!(
            "{}: the state-moved record could not be rewritten ({e})",
            plane.display()
        ));
    }
}

/// Drop every line of the state-moved record whose project has no `.purlis/` folder: a run that
/// died between writing the line and the rename left it, and it would vouch for a `.purlis/`
/// nobody made. Healing, not a move, so it is not journalled.
fn reconcile_record(local: &Local) {
    let file = record_file(local);
    let Ok(text) = std::fs::read_to_string(&file) else {
        return;
    };
    let kept: Vec<String> = text
        .lines()
        .filter(|line| real_dir(&Path::new(line.trim()).join(STATE_DIR.write)))
        .map(str::to_owned)
        .collect();
    if kept.len() != text.lines().count() {
        let _ = write_record(&file, &kept);
    }
}

/// The record's path, in whichever config home folder is the one now.
fn record_file(local: &Local) -> PathBuf {
    local.home().join(STATE_MOVED_RECORD)
}

/// Add `plane` (already resolved) to the state-moved record, journalled first, atomically.
fn record(local: &Local, plane: &Path) -> io::Result<()> {
    let file = record_file(local);
    let text = std::fs::read_to_string(&file).unwrap_or_default();
    if text.lines().any(|line| Path::new(line.trim()) == plane) {
        return Ok(());
    }
    write_entry(
        local,
        &Entry::Record {
            plane: plane.to_path_buf(),
        },
    )?;
    let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
    lines.push(plane.to_string_lossy().into_owned());
    write_record(&file, &lines)
}

fn write_record(file: &Path, lines: &[String]) -> io::Result<()> {
    let parent = file.parent().unwrap_or(Path::new("."));
    private_dirs(parent)?;
    if lines.is_empty() {
        return match std::fs::remove_file(file) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        };
    }
    let mut text = lines.join("\n");
    text.push('\n');
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    temp.write_all(text.as_bytes())?;
    temp.as_file().sync_all()?;
    temp.persist(file).map_err(|e| e.error)?;
    Ok(())
}

/// The repository's `info/exclude` for the project at `plane`, and the project's path inside
/// that repository (`sub/` for a project below the top, empty at the top). `None` when the
/// project is in no git repository, which carries nothing.
fn exclude_of(plane: &Path) -> Option<(PathBuf, String)> {
    let ask = |args: &[&str]| -> Option<String> {
        let mut git = std::process::Command::new("git");
        git.arg("-C")
            .arg(plane)
            .args(args)
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE");
        let out = crate::forklock::output(&mut git).ok()?;
        out.status.success().then(|| {
            String::from_utf8_lossy(&out.stdout)
                .trim_end_matches('\n')
                .to_owned()
        })
    };
    let exclude = PathBuf::from(ask(&["rev-parse", "--git-path", "info/exclude"])?);
    let prefix = ask(&["rev-parse", "--show-prefix"])?;
    let exclude = if exclude.is_absolute() {
        exclude
    } else {
        plane.join(exclude)
    };
    Some((exclude, prefix))
}

/// Make git ignore each of `names` at the top of `plane`, through the repository's own
/// `info/exclude`: appended, journalled first, and only the lines it lacks.
fn ignore(local: &Local, plane: &Path, names: &[String]) -> io::Result<()> {
    let Some((exclude, prefix)) = exclude_of(plane) else {
        return Ok(());
    };
    let before = match std::fs::read(&exclude) {
        Ok(bytes) => Some(bytes),
        Err(e) if e.kind() == io::ErrorKind::NotFound => None,
        Err(e) => return Err(e),
    };
    let had = String::from_utf8_lossy(before.as_deref().unwrap_or_default()).into_owned();
    let lines: Vec<String> = names
        .iter()
        .map(|name| format!("/{prefix}{name}"))
        .filter(|line| !had.lines().any(|there| there.trim() == line))
        .collect();
    if lines.is_empty() {
        return Ok(());
    }
    let mut text = String::new();
    if !had.is_empty() && !had.ends_with('\n') {
        text.push('\n');
    }
    for line in &lines {
        text.push_str(line);
        text.push('\n');
    }
    if let Some(info) = exclude.parent()
        && !there(info)
    {
        write_entry(
            local,
            &Entry::Made {
                path: info.to_path_buf(),
            },
        )?;
        std::fs::create_dir(info)?;
    }
    write_entry(
        local,
        &Entry::Append {
            file: exclude.clone(),
            len: before.as_ref().map(|bytes| bytes.len() as u64),
            text: text.clone(),
        },
    )?;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&exclude)?;
    file.write_all(text.as_bytes())?;
    file.sync_all()
}

/// Put back every move since the last finished undo, newest first (see the module).
pub fn undo(local: &Local, seams: &Seams) -> Moved {
    let journal = read_journal(local);
    let start = journal
        .iter()
        .rposition(|entry| *entry == Entry::Undone)
        .map_or(0, |at| at + 1);
    let pending: Vec<&Entry> = journal[start..]
        .iter()
        .filter(|entry| **entry != Entry::Undoing)
        .collect();
    if pending.is_empty() {
        let mut moved = Moved::new();
        moved.note(
            "nothing to undo: rename-local has moved nothing since it was last undone".into(),
        );
        return moved;
    }
    let planes = planes_in(local, &pending);
    if let Err(why) = only_our_moves(local, &planes, &pending) {
        return Moved::refused(format!(
            "the journal holds a step rename-local never makes ({why}); nothing was put back"
        ));
    }
    let _lock = match quiet(local, seams, &planes, UNDO_COMMAND) {
        Ok(lock) => lock,
        Err(why) => return Moved::refused(why),
    };
    let mut moved = Moved::new();
    // Said first, so that from here until the undo is finished the launch moves nothing back.
    if let Err(e) = write_entry(local, &Entry::Undoing) {
        return Moved::refused(format!(
            "the journal could not be written ({e}); nothing was put back"
        ));
    }
    for entry in pending.iter().rev() {
        undo_one(local, seams, &mut moved, entry);
    }
    // Last, whatever its place in the journal: it is installed into charter's directory, which
    // is only where it ends up once the config home is back.
    if pending.iter().any(|entry| **entry == Entry::Plugin) {
        unmove_plugin(local, &mut moved);
    } else if let Some(m) = &local.plugin {
        // No plugin step to put back, but a copy installed since the move lives in the config
        // home that just moved back, so its registration is pointed at it (D-RN8-13). It needs
        // no bundle and does nothing where no registered folder is gone.
        let mut m = m.clone();
        m.charter_dir = local.home();
        report(
            &mut moved,
            &Vec::from_iter(crate::plugin_install::repoint(&m, false)),
        );
    }
    if !moved.complete {
        moved.note(format!(
            "the steps not put back are still pending; `{UNDO_COMMAND}` finishes them, and until \
             then the app moves nothing at launch"
        ));
        return moved;
    }
    // In whichever config home folder is the one now: the old one, once its move is undone.
    if let Err(e) = write_entry(local, &Entry::Undone) {
        moved.failed(format!(
            "the journal could not record the finished undo ({e}); `{UNDO_COMMAND}` again \
             records it"
        ));
    } else {
        moved.note(
            "the app leaves the old names alone from now on; `purlis migrate` moves them again"
                .into(),
        );
    }
    moved
}

/// Every project a pending journal names: remembered ones, `local.planes`, recorded ones and
/// the folders its project moves were made in.
fn planes_in(local: &Local, pending: &[&Entry]) -> Vec<PathBuf> {
    let named: Vec<PathBuf> = pending
        .iter()
        .filter_map(|entry| match entry {
            Entry::Record { plane } => Some(plane.clone()),
            Entry::Move { from, .. } if in_plane(from).is_some() => {
                from.parent().map(Path::to_path_buf)
            }
            Entry::Switched { plane, .. } | Entry::Rebased { plane, .. } => Some(plane.clone()),
            _ => None,
        })
        .collect();
    planes_of(local, &named)
}

/// The old and purlis spelling of a project's own name, when `from` is one: its local settings
/// or its state folder.
fn in_plane(from: &Path) -> Option<&'static str> {
    let name = from.file_name()?;
    [LOCAL_SETTINGS, STATE_DIR]
        .into_iter()
        .find(|entry| name == entry.reads[0])
        .map(|entry| entry.write)
}

/// Whether every entry is one rename-local on this machine makes (S3 of the review, a forged
/// journal): the pairs [`run`] moves, the exclude files of the projects in play with the lines it
/// adds, and the folders it makes. `Err` names the first that is not.
fn only_our_moves(local: &Local, planes: &[PathBuf], pending: &[&Entry]) -> Result<(), String> {
    let root = &local.config_root;
    let mut pairs: Vec<(PathBuf, PathBuf)> = vec![(
        root.join(CONFIG_HOME.reads[0]),
        root.join(CONFIG_HOME.write),
    )];
    for home in crate::machine::dirs_spelled(root) {
        pairs.push((home.join(DAEMON_DIR.reads[0]), home.join(DAEMON_DIR.write)));
    }
    if let Some(base) = &local.data_base {
        pairs.push((base.join(DATA_HOME.reads[0]), base.join(DATA_HOME.write)));
    }
    let mut made: Vec<PathBuf> = Vec::new();
    if let Some(logs) = &local.logs {
        pairs.push((logs.old.clone(), logs.new.clone()));
        made.extend(logs.new.parent().map(Path::to_path_buf));
    }
    let mut excludes: Vec<(PathBuf, Vec<String>)> = Vec::new();
    for plane in planes {
        for (old, new) in [
            (LOCAL_SETTINGS.reads[0], LOCAL_SETTINGS.write),
            (STATE_DIR.reads[0], STATE_DIR.write),
        ] {
            pairs.push((plane.join(old), plane.join(new)));
        }
        if let Some((file, prefix)) = exclude_of(plane) {
            made.extend(file.parent().map(Path::to_path_buf));
            let lines = vec![
                format!("/{prefix}{}/", STATE_DIR.write),
                format!("/{prefix}{}", LOCAL_SETTINGS.write),
            ];
            excludes.push((file, lines));
        }
    }
    for entry in pending {
        let ours = match entry {
            Entry::Move { from, to, .. } => pairs.iter().any(|(a, b)| a == from && b == to),
            Entry::Record { plane } => plane.is_absolute(),
            Entry::Append { file, text, .. } => excludes.iter().any(|(ours, lines)| {
                ours == file
                    && text
                        .lines()
                        .all(|line| line.is_empty() || lines.iter().any(|l| l == line))
            }),
            Entry::Made { path } => made.contains(path),
            Entry::Copied { .. } | Entry::Switched { .. } | Entry::Rebased { .. } => {
                keychain::ours(entry)
            }
            Entry::Plugin | Entry::Undone | Entry::Undoing => true,
        };
        if !ours {
            return Err(serde_json::to_string(entry).unwrap_or_else(|_| format!("{entry:?}")));
        }
    }
    Ok(())
}

fn undo_one(local: &Local, seams: &Seams, moved: &mut Moved, entry: &Entry) {
    match entry {
        Entry::Move { what, from, to } => {
            if !there(to) {
                // Never made, or already back.
                return;
            }
            if there(from) {
                moved.failed(format!(
                    "{what}: {} was made again since it moved, so {} was not moved back. Nothing \
                     in either is lost: move {} out of the way, then run `{UNDO_COMMAND}` again",
                    from.display(),
                    to.display(),
                    from.display()
                ));
                return;
            }
            match (seams.rename)(to, from) {
                Ok(()) => moved.done(format!("{what}: {} → {}", to.display(), from.display())),
                Err(e) => moved.failed(format!(
                    "{what}: {} could not be moved back ({e}); it stays where it is and is still read",
                    to.display()
                )),
            }
        }
        Entry::Record { plane } => unrecord(local, moved, plane),
        Entry::Append { file, len, text } => {
            let Ok(now) = std::fs::read(file) else {
                return;
            };
            let before = len.unwrap_or(0) as usize;
            // Still its length before: the append was never made (a run that died between
            // journalling it and writing it), or it was put back already.
            if len.is_some() && now.len() == before {
                return;
            }
            let ours = now.len() == before + text.len() && now.ends_with(text.as_bytes());
            if !ours {
                moved.failed(format!(
                    "{} changed since rename-local added to it, so its lines are left in place",
                    file.display()
                ));
                return;
            }
            let put_back = match len {
                None => std::fs::remove_file(file),
                Some(_) => std::fs::write(file, &now[..before]),
            };
            match put_back {
                Ok(()) => moved.done(format!("{}: put back as it was", file.display())),
                Err(e) => moved.failed(format!("{} could not be put back ({e})", file.display())),
            }
        }
        Entry::Made { path } => {
            // Only while it is empty: anything in it was put there since.
            if std::fs::remove_dir(path).is_ok() {
                moved.changed = true;
            }
        }
        // The copy is kept, as the original was (D-RN6-2).
        Entry::Copied { .. } => {}
        Entry::Switched {
            plane,
            vault,
            from,
            to,
            keys,
        } => keychain::switch_back(seams, moved, plane, vault, from, to, keys),
        Entry::Rebased { plane, vault, ids } => keychain::rebase_back(moved, plane, vault, ids),
        Entry::Plugin | Entry::Undone | Entry::Undoing => {}
    }
}

/// Finish moving the vaults and records a run left `waiting` (#1306), on the person's word: the
/// same copy, with the Keychain's dialogs on, so the system asks once for each item, and only
/// for those. Anything else on the old prefix is left as the run left it.
///
/// **It takes no lock**: the app it runs in holds the config home for its life, so no run or
/// undo of a terminal starts meanwhile, and the switch itself refuses a vault whose keys changed
/// while it was copied. After an undo it moves nothing, as the launch does not.
pub fn finish(local: &Local, seams: &Seams, waiting: &[Waiting]) -> Moved {
    if undone(local) {
        return Moved::refused(
            "the last thing done here was an undo, so nothing moves until `purlis migrate` is \
             run"
            .into(),
        );
    }
    let mut moved = Moved::new();
    let written = keychain::written(&read_journal(local));
    let mut planes: Vec<&Path> = Vec::new();
    for waits in waiting {
        if !planes.contains(&waits.plane.as_path()) {
            planes.push(&waits.plane);
        }
    }
    for plane in &planes {
        keychain::copy_plane(
            local,
            seams,
            &mut moved,
            plane,
            &written,
            keychain::Asking::Allowed,
            Some(waiting),
        );
    }
    // The items that wait to be held again by this app (RN-9), the same way: asked for once
    // each, and only those.
    if let Some(own) = local.own_app.as_deref()
        && waiting.iter().any(|waits| waits.hold)
    {
        let mut done = keychain::done_for(local, own);
        for plane in &planes {
            keychain::hold_again(
                seams,
                &mut moved,
                plane,
                own,
                keychain::Asking::Allowed,
                Some(waiting),
                &mut done,
            );
        }
        if let Err(e) = keychain::record_done(local, own, &done) {
            moved.failed(format!(
                "what is held again by {own} could not be recorded ({e}); the next launch \
                 looks again"
            ));
        }
    }
    // What the person may finish again: each that is still on the old prefix, or not held
    // again yet, for whatever reason (an item the person did not allow is one). Only what
    // waited is offered: the press never brings back anything else.
    moved.waiting = waiting
        .iter()
        .filter(|waits| {
            if waits.hold {
                keychain::hold_still_waits(local, waits)
            } else {
                keychain::still_waits(waits)
            }
        })
        .cloned()
        .collect();
    if moved.changed {
        moved.note(format!(
            "every move is journalled; `{UNDO_COMMAND}` puts them back"
        ));
    }
    moved
}

/// The app's launch, by the app whose identifier is `identifier`: [`run`] on this machine, unless the last thing done was an undo (finished
/// or not), or the environment names the project twice over (the app refuses that launch
/// itself). `None` when nothing was said; what was said is for the app's log.
///
/// **The log folder is left where it is** (D-RN5-12): the app has its own log file open in it
/// by now, and a move under the open file would have the next day's file made under the old
/// name again. [`logs_at_launch`] moved it already, before the file was opened; `purlis migrate`
/// and the `rename-local` fix move it too, with the app closed.
///
/// `plugin` is what the plugin's step installs from (the app's own `charter` and bundled plugin,
/// or no bundle to only keep the registration pointing at the moved copy), or `None` to leave
/// the harnesses alone, as a fenced build does.
pub fn at_launch(
    identifier: &str,
    plugin: Option<crate::plugin_install::Machine>,
) -> Option<Moved> {
    if crate::envvar::disagreement().is_some() {
        return None;
    }
    let mut local = Local::of_this_machine(&[])?;
    local.logs = None;
    local.own_app = Some(identifier.to_owned());
    local.plugin = plugin;
    if undone(&local) {
        return None;
    }
    let moved = run(&local, &Seams::real());
    (moved.changed || !moved.complete || moved.refused.is_some() || !moved.waiting.is_empty())
        .then_some(moved)
}

/// The app's log folder moved to its purlis identifier's name at the app's launch (RN-9, V93e),
/// **before the app opens its log file** in it: the first thing `run()` does after its panic
/// hook, so the file this launch writes is already under the new name. `None` when there is
/// nothing to move (no old folder, or both there: `purlis migrate` says that one), the last
/// thing done was an undo, or the environment names the project twice over.
///
/// Asked as a terminal asks, with no identifier of its own: it runs before the single-instance
/// handoff, so a socket or lock under the purlis identifier is a second app's, never this one's.
/// What was said is for the app's log, once it is open.
pub fn logs_at_launch() -> Option<Moved> {
    if crate::envvar::disagreement().is_some() {
        return None;
    }
    logs_at_launch_with(&Local::of_this_machine(&[])?, &Seams::real())
}

/// [`logs_at_launch`] on `local`, through `seams`.
pub fn logs_at_launch_with(local: &Local, seams: &Seams) -> Option<Moved> {
    let logs = local.logs.as_ref()?;
    if !there(&logs.old) || there(&logs.new) || undone(local) {
        return None;
    }
    let _lock = match quiet(local, seams, &[], "purlis migrate") {
        Ok(lock) => lock,
        Err(why) => return Some(Moved::refused(why)),
    };
    let mut moved = Moved::new();
    move_logs(local, seams, &mut moved, logs);
    if moved.changed {
        moved.note(format!(
            "every move is journalled; `{UNDO_COMMAND}` puts them back"
        ));
    }
    Some(moved)
}
