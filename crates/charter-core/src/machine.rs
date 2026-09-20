//! The first thing charter keeps **outside** a plane.
//!
//! charter's founding rule is that the plane is the state: everything charter knows lives in
//! a `charter.toml` and the directories beside it, committed, and travelling with the clone.
//! This module is the one deliberate exception, and it exists because of a state the rule
//! cannot express — the app was started from **nowhere**. A double-clicked `.app` has `/` for
//! a working directory, so `plane::resolve` answers `NotFound` and the app has no plane, no
//! recents and no way to be given one. What the opener needs to know belongs to the machine
//! and to no plane on it, so it cannot be kept in one.
//!
//! **Three things, and nothing else:**
//!
//! - **the planes recently opened**, so the opener has something to offer;
//! - **whether the operator approved each one**, and *what it contributed when they did*;
//! - **which planes were open in which windows at the last quit**, so a cold launch restores
//!   the window set.
//!
//! **Never plane content.** A chat, a memory, a workspace, a persona, a profile: all of those
//! belong to a plane and stay in it. Each plane's own `.charter/app/reopen.json` still
//! restores *its* chats ([`crate::reopen`]) and this module does not touch it — the split is
//! "which planes were open" here, "what was open inside one" there.
//!
//! # Where it lives, per platform
//!
//! The conventional application-data directory, from the `dirs` crate, plus one directory of
//! charter's own ([`DIR`]):
//!
//! | platform | [`data_root`] | the store |
//! | --- | --- | --- |
//! | macOS | `~/Library/Application Support` | `~/Library/Application Support/charter/machine.json` |
//! | Linux | `$XDG_DATA_HOME`, else `~/.local/share` | `…/charter/machine.json` |
//! | Windows | `%APPDATA%` | refused — see below |
//!
//! **Data and not state**, though `$XDG_STATE_HOME`'s own wording ("recently used files, the
//! layout of windows") describes two of the three things here exactly. The third decides it:
//! this file records the operator's **approval** of a plane, and the XDG spec's contract for
//! state is that it may be discarded. A trust record that a cleaner may delete is a trust
//! record that silently becomes "ask again" — which is safe — or, worse, a store that is half
//! there. `$XDG_DATA_HOME` is the location whose contract matches what is kept. It is also
//! the only one of the three `dirs` answers for on every platform charter targets:
//! `dirs::state_dir` is `None` off Linux, so choosing it would mean charter writing its own
//! per-platform fallback table — the custom tooling this repo's priorities forbid where a
//! standard answer exists.
//!
//! **No environment variable moves this store.** `$CHARTER_HOME` moves a *plane's* state
//! directory and has no business here, and a new variable that relocated the trust record
//! would be a way to hand charter a store full of approvals the operator never gave. The
//! store's location is `dirs`' answer, and tests pass a root explicitly.
//!
//! # Windows refuses rather than degrades
//!
//! Every `chmod` call site in this repo is `#[cfg(unix)]` with nothing off it, and on Windows
//! `chmod 0o755` leaves a file at `0o666` (measured, charter-app#98). The `0600`/`0700` this
//! store depends on therefore has no Windows expression today, and ADR 0031 (charter) settles
//! what to do about that: **a guard that cannot be expressed refuses rather than degrades.**
//! So on any platform that is not unix every entry point here returns
//! [`io::ErrorKind::Unsupported`] and charter keeps no machine-level state at all. That is a
//! known quantity; a world-readable list of the operator's projects, and a trust record any
//! account on the machine can edit, is not.
//!
//! # Everything read back is attacker-influenced
//!
//! The file is `0600` in a `0700` directory, so on an honest machine only the operator writes
//! it. That is a reason to treat what comes back as untrusted, not a reason to trust it: the
//! store is the one charter file that is **not** in a plane's git history, so nothing else
//! vouches for it, and a plane path it names is a path charter is about to open a window on.
//! So on read every stored path is held to being absolute, free of `..` and free of a NUL,
//! and an entry that fails is **dropped with a reason** ([`Dropped`]) rather than raised. An
//! opener that shows one fewer row and says why is usable; an error dialog at launch, before
//! there is a window, is not.
//!
//! Whether a remembered path is still *there*, and still a plane, is a different question and
//! is deliberately **not** asked on the read path — see [`still_a_plane`].

use std::collections::BTreeMap;
use std::io;
use std::path::{Component, Path, PathBuf};

/// charter's own directory inside the app-data directory.
pub const DIR: &str = "charter";

/// The store, inside [`DIR`].
pub const FILE: &str = "machine.json";

/// The one version of this file charter writes and reads. Any other version reads as an empty
/// store: there is no migration that would be honest about an approval recorded under rules
/// this charter does not know.
pub const VERSION: u32 = 1;

/// The most this file may be. It is a bounded list of short entries, it is read whole, and it
/// is read before there is a window — so a planted giant here is a launch that never
/// finishes, exactly as it is for [`crate::reopen`]'s record.
pub const MAX_BYTES: u64 = 1 << 20;

/// How many planes are remembered. The list is bounded because the file is read at every
/// launch and nothing else prunes it.
///
/// **Falling off the end forgets the approval too**, because the approval is a field of the
/// entry (see [`Recent`]). That direction is the safe one: the plane is asked about again the
/// next time it is opened.
pub const MOST_RECENTS: usize = 64;

/// The conventional application-data directory for this platform, or `None` when there is no
/// home to put one in.
///
/// `dirs` rather than a hand-written table: it is the mature, standard answer, it is already
/// in this workspace's lockfile (Tauri depends on it), and the rules it encodes —
/// `$XDG_DATA_HOME` with its `~/.local/share` fallback, `~/Library/Application Support`,
/// `%APPDATA%` — are exactly the ones charter would otherwise be maintaining by hand.
pub fn data_root() -> Option<PathBuf> {
    dirs::data_dir()
}

/// charter's directory inside `data_root`.
pub fn dir(data_root: &Path) -> PathBuf {
    data_root.join(DIR)
}

/// The store's own path inside `data_root`.
pub fn file(data_root: &Path) -> PathBuf {
    dir(data_root).join(FILE)
}

/// Whether charter keeps machine-level state on this platform at all.
///
/// See the module docstring: on Windows the `0600`/`0700` this store depends on has no
/// expression, and ADR 0031 says such a guard refuses.
#[cfg(unix)]
fn supported() -> io::Result<()> {
    Ok(())
}

#[cfg(not(unix))]
fn supported() -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "charter keeps no machine-level store on this platform: the 0600 on the file and the \
         0700 on its directory have no expression here (charter-app#98), and a guard that \
         cannot be expressed refuses rather than degrades (charter ADR 0031)",
    ))
}

/// What a plane's committed settings hand to every harness started in it.
///
/// **Measured, and this is the whole reason trust is recorded at all.**
/// `layer::WORKSPACE_KEYS` is `["enabledPlugins", "env"]`: those two keys travel out of a
/// plane's own `.claude/settings.json` into the `.claude/settings.json` a harness reads. So
/// opening a stranger's plane lets its author choose the plugins that run and the environment
/// every chat is started under — `PATH`, `NODE_OPTIONS`, a base URL a harness talks to.
///
/// The two existing limits on that travel are not weakened here and are worth naming, because
/// they are why this type has two fields and not three: `permissions` travels only as `ask`
/// and `deny` and **never** as `allow` (`layer::RESTRICTIVE`), so it cannot make anything run
/// that would not have run anyway; and a harness profile is machine-local (ADR 0022), so a
/// plane cannot bring a command line with it. `enabledPlugins` and `env` are what is left,
/// and they are both grants.
///
/// **As the plane declares them, values included.** A change of *value* is the sharper half:
/// an `env` whose `PATH` gains a directory is a different grant from the one that was
/// approved, and a record of names alone could not see it. These values come out of a file
/// that is committed in the plane's own history, so recording them here moves nothing into
/// the machine store that was not already readable in the plane.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Contribution {
    /// Each plugin the plane enables, and what it enables it as.
    pub plugins: BTreeMap<String, String>,
    /// Each environment variable the plane sets, and its value.
    pub env: BTreeMap<String, String>,
}

impl Contribution {
    /// What `plane` contributes right now.
    ///
    /// An unreadable, absent or malformed `.claude/settings.json` contributes **nothing**,
    /// which is the same answer a plane with no settings gives. That is deliberate in both
    /// directions: a plane that contributes nothing is never asked about, and a plane whose
    /// settings charter could not read is not credited with whatever they might say.
    pub fn of(plane: &Path) -> Self {
        let Some(settings) = crate::layer::plane_settings(plane, crate::layer::SETTINGS) else {
            return Self::default();
        };
        Self {
            plugins: settings
                .get("enabledPlugins")
                .map(plugins_of)
                .unwrap_or_default(),
            env: settings
                .get("env")
                .and_then(serde_json::Value::as_object)
                .map(|map| map.iter().map(|(k, v)| (k.clone(), word(v))).collect())
                .unwrap_or_default(),
        }
    }

    /// Every way `now` differs from what this recorded, in a stable order.
    pub fn against(&self, now: &Self) -> Vec<Change> {
        let mut out = Vec::new();
        diff(
            &self.plugins,
            &now.plugins,
            Change::PluginAdded,
            Change::PluginRemoved,
            Change::PluginChanged,
            &mut out,
        );
        diff(
            &self.env,
            &now.env,
            Change::EnvAdded,
            Change::EnvRemoved,
            Change::EnvChanged,
            &mut out,
        );
        out
    }
}

/// `enabledPlugins` as a name-to-value map, whatever shape the key is in.
///
/// An object is the shape a settings file uses (`{"name@market": true}`), and an array of
/// names is the other one anyone writes by hand. Anything else — a string, a number, `null` —
/// is keyed by its own rendering, so a plane that changes it is still **noticed** rather than
/// quietly read as contributing nothing. A reader that assumed a shape would be the crash a
/// launch cannot have.
fn plugins_of(value: &serde_json::Value) -> BTreeMap<String, String> {
    match value {
        serde_json::Value::Object(map) => map.iter().map(|(k, v)| (k.clone(), word(v))).collect(),
        serde_json::Value::Array(items) => items.iter().map(|v| (word(v), String::new())).collect(),
        other => [(word(other), String::new())].into_iter().collect(),
    }
}

/// A JSON value as one comparable word: a string as itself, anything else as its compact
/// rendering.
fn word(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

fn diff(
    was: &BTreeMap<String, String>,
    now: &BTreeMap<String, String>,
    added: fn(String) -> Change,
    removed: fn(String) -> Change,
    changed: fn(String) -> Change,
    out: &mut Vec<Change>,
) {
    for (name, value) in now {
        match was.get(name) {
            None => out.push(added(name.clone())),
            Some(before) if before != value => out.push(changed(name.clone())),
            Some(_) => {}
        }
    }
    for name in was.keys() {
        if !now.contains_key(name) {
            out.push(removed(name.clone()));
        }
    }
}

/// One way a plane's contribution differs from the one that was approved.
///
/// **The name only, never the value.** The caller holds both contributions and can show
/// whatever it needs; a variant carrying a value would put the contents of every plane's
/// `env` into every string a refusal, a log line or a report is built from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    PluginAdded(String),
    PluginRemoved(String),
    PluginChanged(String),
    EnvAdded(String),
    EnvRemoved(String),
    EnvChanged(String),
}

impl Change {
    /// Whether this change can make something run that the approval did not cover.
    ///
    /// An addition or a changed value can; a removal cannot. That asymmetry is the same one
    /// `layer::RESTRICTIVE` already makes about `ask`/`deny`, and it is what [`Consent`]
    /// turns into "ask again" or "just say so".
    pub fn is_a_grant(&self) -> bool {
        match self {
            Self::PluginAdded(_)
            | Self::PluginChanged(_)
            | Self::EnvAdded(_)
            | Self::EnvChanged(_) => true,
            Self::PluginRemoved(_) | Self::EnvRemoved(_) => false,
        }
    }

    /// The name this change is about.
    pub fn name(&self) -> &str {
        match self {
            Self::PluginAdded(name)
            | Self::PluginRemoved(name)
            | Self::PluginChanged(name)
            | Self::EnvAdded(name)
            | Self::EnvRemoved(name)
            | Self::EnvChanged(name) => name,
        }
    }
}

impl std::fmt::Display for Change {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (what, how) = match self {
            Self::PluginAdded(_) => ("plugin", "is new"),
            Self::PluginRemoved(_) => ("plugin", "is gone"),
            Self::PluginChanged(_) => ("plugin", "is enabled differently"),
            Self::EnvAdded(_) => ("environment variable", "is new"),
            Self::EnvRemoved(_) => ("environment variable", "is gone"),
            Self::EnvChanged(_) => ("environment variable", "has a different value"),
        };
        write!(f, "the {what} {} {how}", self.name())
    }
}

/// Whether a plane may be opened without asking, and why not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Consent {
    /// Nothing has recorded this plane. Ask.
    New,
    /// It was approved, and it contributes exactly what was approved.
    Unchanged,
    /// It was approved, and it now contributes **more** — a plugin, an environment variable,
    /// or a different value for one it already had. Ask again, and say what is new.
    Grew(Vec<Change>),
    /// It was approved, and it contributes strictly **less**. Say so; do not ask.
    Shrank(Vec<Change>),
}

impl Consent {
    /// Whether the operator has to be asked before this plane is opened.
    ///
    /// **A change re-asks; a withdrawal only reports.** The argument is the blast radius of
    /// what actually changed, and it is `layer::RESTRICTIVE`'s argument applied one level up.
    ///
    /// *Why an addition re-asks.* An approval is consent to a **contribution**, not to a
    /// path. `enabledPlugins` is code that will run inside the operator's harness and `env`
    /// decides what that harness talks to — so a plane that gains either after it was
    /// approved has been handed a grant nobody looked at. Reporting it and opening anyway
    /// would put the notice in the one place an operator has already decided not to read: a
    /// window that opened successfully. [`crate::profiletrust`] settled the identical
    /// question for a profile's command line — `Approval::Changed` asks — and a plugin is
    /// strictly more than a command line, so a weaker rule here would be the same decision
    /// made twice with different answers, which is the defect this repo keeps finding.
    ///
    /// *Why a withdrawal does not.* Removing a plugin or an environment variable cannot make
    /// anything run that would not have run under the approval already given; it can only
    /// make less run. A prompt that never carries risk is a prompt an operator learns to
    /// answer yes to without reading, which spends the attention the real question needs.
    /// So a shrinkage is reported — the store still records what changed — and nothing stops.
    ///
    /// *Mixed.* One addition among ten removals is [`Consent::Grew`] and asks: the question
    /// is whether anything new was granted, never how much was given back.
    pub fn must_ask(&self) -> bool {
        matches!(self, Self::New | Self::Grew(_))
    }

    /// What changed, for a caller that wants to say so. Empty unless something did.
    pub fn changes(&self) -> &[Change] {
        match self {
            Self::New | Self::Unchanged => &[],
            Self::Grew(changes) | Self::Shrank(changes) => changes,
        }
    }
}

/// What the operator approved about one plane, and when.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trust {
    /// Seconds since the epoch.
    pub approved: u64,
    /// What the plane contributed at that moment — not merely that it was approved. The
    /// difference is the whole of [`Consent`].
    pub contributed: Contribution,
}

/// One plane the operator has opened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recent {
    /// Absolute, `..`-free, as it was when it was opened. Whether anything is still there is
    /// [`still_a_plane`]'s question.
    pub plane: PathBuf,
    /// Seconds since the epoch.
    pub opened: u64,
    /// The approval, where there is one.
    ///
    /// **A field of the entry, so there are never two lists to keep in step.** Forgetting a
    /// plane forgets its approval with it, which is both the simple implementation and the
    /// safe direction: the plane is asked about again.
    pub trust: Option<Trust>,
}

/// One window, and the planes it had open as tabs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Window {
    /// Left to right, as the tabs were.
    pub planes: Vec<PathBuf>,
    /// Which tab was in front. Always a valid index into `planes`, which is not something a
    /// file can promise — see [`read`].
    pub active: usize,
}

/// Everything charter keeps outside a plane.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Store {
    /// Most recently opened first.
    pub recents: Vec<Recent>,
    pub windows: Vec<Window>,
}

impl Store {
    /// What is remembered about `plane`, if anything.
    pub fn recent(&self, plane: &Path) -> Option<&Recent> {
        self.recents.iter().find(|entry| entry.plane == plane)
    }

    /// Put `plane` at the front of the list, keeping whatever was approved about it.
    ///
    /// **An open is not an approval**, so an existing [`Trust`] is carried over untouched and
    /// a plane that had none still has none. Opening a plane a hundred times must not turn
    /// into consent.
    pub fn remember(&mut self, plane: &Path, when: u64) {
        let entry = match self.recents.iter().position(|e| e.plane == plane) {
            Some(at) => {
                let mut entry = self.recents.remove(at);
                entry.opened = when;
                entry
            }
            None => Recent {
                plane: plane.to_path_buf(),
                opened: when,
                trust: None,
            },
        };
        self.recents.insert(0, entry);
        self.recents.truncate(MOST_RECENTS);
    }

    /// Record that the operator approved `plane` while it contributed `contributed`.
    pub fn approve(&mut self, plane: &Path, when: u64, contributed: Contribution) {
        self.remember(plane, when);
        if let Some(entry) = self.recents.first_mut() {
            entry.trust = Some(Trust {
                approved: when,
                contributed,
            });
        }
    }

    /// Drop `plane` from the list, and with it any approval.
    pub fn forget(&mut self, plane: &Path) {
        self.recents.retain(|entry| entry.plane != plane);
        for window in &mut self.windows {
            window.planes.retain(|open| open != plane);
        }
        self.windows.retain(|window| !window.planes.is_empty());
        for window in &mut self.windows {
            window.active = window.active.min(window.planes.len() - 1);
        }
    }

    /// Whether `plane` may be opened without asking, given what it contributes now.
    pub fn consent(&self, plane: &Path, contributes: &Contribution) -> Consent {
        let Some(trust) = self.recent(plane).and_then(|entry| entry.trust.as_ref()) else {
            return Consent::New;
        };
        let changes = trust.contributed.against(contributes);
        if changes.is_empty() {
            Consent::Unchanged
        } else if changes.iter().any(Change::is_a_grant) {
            Consent::Grew(changes)
        } else {
            Consent::Shrank(changes)
        }
    }
}

/// Something the store held that charter would not take back.
///
/// Every one of these is a **drop with a reason**, never an error a launch has to handle: an
/// opener that shows one fewer row and can say why is usable, and a dialog before there is a
/// window is not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Dropped {
    /// The file parsed as something, and that something is not this store.
    TheStore(String),
    /// One remembered plane.
    Recent { plane: String, why: String },
    /// One window, or one tab in one.
    Window { plane: String, why: String },
}

impl std::fmt::Display for Dropped {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TheStore(why) => write!(f, "charter's machine store {why}"),
            Self::Recent { plane, why } => write!(f, "the remembered plane '{plane}' {why}"),
            Self::Window { plane, why } => write!(f, "the open plane '{plane}' {why}"),
        }
    }
}

/// What a read of the store came back with.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Loaded {
    pub store: Store,
    /// What was in the file and is not in the store, each with its reason.
    pub dropped: Vec<Dropped>,
    /// Why the file could not be read **at all** — a link, a FIFO, a giant, no permission, or
    /// a platform charter keeps no store on.
    ///
    /// This is not the same as a file that parsed badly, and [`update`] treats the two
    /// differently: content charter could read and did not understand is replaced, and a file
    /// charter could not read is left exactly where it is.
    pub unreadable: Option<String>,
}

/// Everything charter kept outside a plane, with whatever it would not take back named.
///
/// **This never fails**, because every caller is a launch. A missing file is a first launch;
/// anything else is an empty store plus a reason.
///
/// The read is gated the way every read of charter's own state is (charter ADR 0028):
///
/// - [`crate::contain::open_no_link`] against `data_root`, so a link at `charter/` or at the
///   file itself cannot make this answer out of somebody else's file — and so the last
///   component's answer is the **kernel's, at the instant of the open**, rather than
///   charter's a moment earlier;
/// - the plain-file and size questions asked of the **open descriptor**, which no swap can
///   get between, exactly as [`crate::reopen::read_or_refusal`] asks them;
/// - `O_NONBLOCK`, which comes with the same open, because a FIFO is not a link and reading
///   one blocks for ever — here, at a cold launch, before there is a window or a tray.
pub fn read(data_root: &Path) -> Loaded {
    let text = match read_text(data_root) {
        Ok(None) => return Loaded::default(),
        Ok(Some(text)) => text,
        Err(why) => {
            return Loaded {
                unreadable: Some(why.to_string()),
                ..Loaded::default()
            };
        }
    };
    let mut dropped = Vec::new();
    let store = match parse(&text) {
        Ok(doc) => load(&doc, &mut dropped),
        Err(why) => {
            dropped.push(Dropped::TheStore(why));
            Store::default()
        }
    };
    Loaded {
        store,
        dropped,
        unreadable: None,
    }
}

/// The text as a store document of **this** version, or why it is not one.
///
/// Read as a `serde_json::Value` and asked about by hand, the way
/// [`crate::profiletrust`] reads its record, rather than deserialised into a shape: every
/// level of this file can be anything, one bad entry must cost one row and not the list,
/// and this crate turns `arbitrary_precision` on — which changes what a `Value` is and is a
/// reason not to route a typed read back through one.
fn parse(text: &str) -> Result<serde_json::Value, String> {
    let doc: serde_json::Value =
        serde_json::from_str(text).map_err(|why| format!("is not a store charter wrote: {why}"))?;
    if !doc.is_object() {
        return Err("is not a store charter wrote: it is not an object".to_owned());
    }
    match doc.get("version").and_then(serde_json::Value::as_u64) {
        Some(found) if found == u64::from(VERSION) => Ok(doc),
        Some(found) => Err(format!(
            "is version {found}, and this charter writes version {VERSION}"
        )),
        None => Err("says no version, so charter cannot say what it means".to_owned()),
    }
}

/// The file's bytes, `None` for no file at all, an error for a file charter will not read.
fn read_text(data_root: &Path) -> io::Result<Option<String>> {
    supported()?;
    let target = file(data_root);
    let mut open = match crate::contain::open_no_link(data_root, &target) {
        Ok(open) => open,
        Err(gone) if gone.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(refused) => return Err(refused),
    };
    // `fstat` of the descriptor the read will use, never of the name: the two cannot be
    // handed two different files.
    let found = open.metadata()?;
    if !found.file_type().is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "{} is not a plain file, and charter reads its machine store from nothing else",
                target.display()
            ),
        ));
    }
    if found.len() > MAX_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "{} is {} bytes, and charter's machine store is never larger than {MAX_BYTES}",
                target.display(),
                found.len()
            ),
        ));
    }
    let mut text = String::new();
    io::Read::read_to_string(&mut open, &mut text)?;
    Ok(Some(text))
}

/// Read the store, change it, and write it back.
///
/// **A store charter could not read is never overwritten.** The difference between "this file
/// says nothing charter understands" and "charter could not read this file" is the difference
/// between content worth replacing and a path that is compromised or a disk that is failing —
/// and clobbering the second destroys the operator's list and their approvals to fix nothing.
/// So the first is replaced and the second refuses, loudly, with the reason attached.
pub fn update(data_root: &Path, change: impl FnOnce(&mut Store)) -> io::Result<Loaded> {
    let mut loaded = read(data_root);
    if let Some(why) = &loaded.unreadable {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("charter will not overwrite a machine store it could not read: {why}"),
        ));
    }
    change(&mut loaded.store);
    write(data_root, &loaded.store)?;
    Ok(loaded)
}

/// Write the store, creating charter's directory at `0700` if it is not there.
///
/// **Crash-safe by replacement, and the object being replaced is an inode** (the lesson
/// charter-app#82 left about `ETXTBSY`: reason about the inode, never about the path). The
/// bytes go to a file beside the store, are flushed to the disk, and are then `rename`d over
/// the name. A launch reading at that moment holds a descriptor on the old inode and reads
/// the whole of the old store; a launch opening afterwards opens the new one. Neither can see
/// half of one, and a process killed between the two leaves the previous store intact with a
/// stray temp file beside it.
///
/// **The gate is on the path the write actually lands on**, which is the temp file and not
/// the store. Guarding the destination of a rename while the bytes go somewhere unguarded is
/// the mistake this repo has had six review rounds on, and the one that put a whole
/// `reopen.json` outside a plane through a committed `reopen.json.writing -> elsewhere`.
///
/// `rename` is not gated and does not need to be: it replaces a **name**, so a symlink
/// sitting at the store's path is replaced rather than written through.
pub fn write(data_root: &Path, store: &Store) -> io::Result<()> {
    supported()?;
    let dir = private_dir(data_root)?;
    let target = dir.join(FILE);
    // A pid AND a per-call tag, as `profiletrust::write_private` does: the pid separates two
    // processes, the tag separates two writers inside one — Tauri runs commands on a thread
    // pool — and a pid the kernel has recycled.
    let temp = dir.join(format!(
        "{FILE}.{}.{}.writing",
        std::process::id(),
        crate::workspaces::scratch_tag()
    ));
    let text = serde_json::to_string_pretty(&OnDisk::from(store))
        .expect("the store is plain data serde can always write");
    write_through(data_root, &target, &temp, (text + "\n").as_bytes())
}

/// [`write`]'s body, with the temp file named by the caller.
///
/// The seam exists so a test can **plant its link at the path that is actually opened**. A
/// test that plants one at the store's own path passes against code that writes through an
/// unguarded temp file, which is precisely the defect the gate here exists to stop, so a test
/// that cannot name the temp file proves nothing about it.
fn write_through(data_root: &Path, target: &Path, temp: &Path, bytes: &[u8]) -> io::Result<()> {
    // The walk, against the app-data root: this is what refuses a `charter/` that is a link
    // out of it, at the moment the create happens rather than at some earlier check.
    crate::contain::no_link_on_the_way(data_root, temp)?;
    let mut options = std::fs::OpenOptions::new();
    // `create_new`, so an existing file at the temp path is refused rather than written
    // through — and, on any POSIX system, so is a symlink sitting there (`O_CREAT|O_EXCL`
    // fails on one). `O_NOFOLLOW` comes from `contain::nofollow` as well; on this path it is
    // the second of two answers to the same question, which is said here so nobody credits
    // it with a refusal `O_EXCL` already made.
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        // The mode is set on the TEMP file because a rename carries the source's mode onto
        // the target, not the other way round — and `OpenOptions::mode` applies only when the
        // call creates the inode, which `create_new` guarantees it does.
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    // The open is outside what is cleaned up below, deliberately: a `create_new` that fails
    // failed because something was ALREADY at that path, and unlinking that something is
    // charter deleting a file it did not make.
    let mut out = crate::contain::nofollow(&mut options).open(temp)?;
    let result = io::Write::write_all(&mut out, bytes)
        // The bytes reach the disk before the name changes. This does not make the rename
        // itself durable — that would need the directory synced too — so what it buys is
        // narrow and worth stating: a store whose name is the new one is never a file of
        // zeroes.
        .and_then(|()| out.sync_all())
        .and_then(|()| std::fs::rename(temp, target));
    if result.is_err() {
        // This call created it, so this call takes it away.
        let _ = std::fs::remove_file(temp);
    }
    result
}

/// charter's directory under `data_root`, private to the operator.
///
/// **Tightened even when it is already there**, which is where this differs from
/// [`crate::profiletrust::private_dir`]'s rule of leaving an existing directory exactly as it
/// was found. That rule exists because `$CHARTER_HOME` can point a plane's state directory at
/// a home or a team share the operator chose, and charter has no business re-moding it. No
/// such escape hatch reaches here: this directory is charter's own, at a path charter alone
/// decides, holding a list of every project the operator opens and their approvals of each.
///
/// The tightening is best-effort, as every `chmod` in this crate is: a filesystem with fixed
/// permissions (exFAT, many network mounts) cannot hold a mode, and refusing to keep state to
/// protect a mode the filesystem was never going to keep helps nobody. The mode that the
/// guard rests on is the one set at **creation**, which is not best-effort.
fn private_dir(data_root: &Path) -> io::Result<PathBuf> {
    // The app-data directory itself is made without a mode: `~/.local/share` belongs to the
    // operator and to every application, and charter creating it at 0700 would quietly
    // re-mode a directory that is not its own. `0700` starts at charter's own level.
    std::fs::create_dir_all(data_root)?;
    let dir = dir(data_root);
    // Refuses a symlink at the directory and creates it at 0700, which is exactly what is
    // wanted here — one implementation, because two drift.
    crate::profiletrust::private_dir(&dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
    }
    Ok(dir)
}

/// Whether a remembered path still names a plane, or why it should be dropped.
///
/// **Deliberately not asked by [`read`], and that is a decision rather than an omission.**
/// Every question here is a `stat`, and a remembered plane can be on a network mount, an
/// unplugged external disk or an automounted share. A cold launch that stats sixty-four of
/// them before it can draw the opener is a launch that hangs on the one that is gone, with
/// nothing on screen to say so. So the read is lexical and total, and the disk is asked per
/// row, by whoever is about to show or open one.
///
/// A path that is a **symlink now** is dropped rather than followed. It may be honest — a
/// project moved to another volume and linked back — but the approval recorded against it was
/// recorded for what the path pointed at then, and charter cannot tell the two apart. The
/// repair is in the operator's hands and costs one dialog: open it again by its real path,
/// which asks about it again. (A link *above* the plane — a symlinked `$HOME`, `/tmp` on
/// macOS — is ordinary and is not asked about, exactly as `contain` does not ask about the
/// components above the root it is given.)
pub fn still_a_plane(plane: &Path) -> Result<(), String> {
    let found = std::fs::symlink_metadata(plane).map_err(|_| "is no longer there".to_owned())?;
    if found.file_type().is_symlink() {
        return Err(
            "is a symlink now, and charter opens a plane by the path that was approved".to_owned(),
        );
    }
    if !found.is_dir() {
        return Err("is not a directory any more".to_owned());
    }
    if !plane.join(crate::plane::MANIFEST).is_file() {
        return Err(format!(
            "is not a plane any more: it holds no {}",
            crate::plane::MANIFEST
        ));
    }
    Ok(())
}

/// Whether a path off the file may be used, and why not.
///
/// Lexical only, and total: these are the questions that can be asked of a **string** off a
/// file charter did not write this run.
fn usable(raw: &str) -> Result<PathBuf, String> {
    if raw.is_empty() {
        return Err("is empty".to_owned());
    }
    // A NUL terminates the string inside the C library, so the path charter checked and the
    // path the kernel opened would be two different strings.
    if raw.contains('\0') {
        return Err("holds a NUL, so the kernel would see a shorter path".to_owned());
    }
    let path = PathBuf::from(raw);
    if !path.is_absolute() {
        return Err(
            "is not absolute, and a relative path resolves against a working directory that is \
             '/' for an app nobody launched from a terminal"
                .to_owned(),
        );
    }
    if path.components().any(|part| part == Component::ParentDir) {
        return Err("walks up through '..', and charter's own paths never do".to_owned());
    }
    Ok(path)
}

/// The file's contents held to what a store may be, with every refusal recorded.
///
/// Every value is asked about rather than assumed: a `recents` that is a number, an entry
/// that is a string, a `trust` that is a list. Each costs the one row it is, because a store
/// is read at a cold launch and one bad row must not be a lost list.
fn load(doc: &serde_json::Value, dropped: &mut Vec<Dropped>) -> Store {
    let mut recents: Vec<Recent> = Vec::new();
    for raw in array(doc.get("recents")) {
        let named = raw
            .get("plane")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let why = if named.is_empty() && !raw.is_object() {
            Err("is not an entry charter wrote".to_owned())
        } else {
            usable(&named)
        };
        let plane = match why {
            Ok(plane) => plane,
            Err(why) => {
                dropped.push(Dropped::Recent { plane: named, why });
                continue;
            }
        };
        if recents.iter().any(|kept| kept.plane == plane) {
            dropped.push(Dropped::Recent {
                plane: named,
                why: "is in the list twice, and the later entry is the stale one".to_owned(),
            });
            continue;
        }
        if recents.len() >= MOST_RECENTS {
            dropped.push(Dropped::Recent {
                plane: named,
                why: format!("is past the {MOST_RECENTS} planes charter remembers"),
            });
            continue;
        }
        recents.push(Recent {
            plane,
            opened: number(raw.get("opened")),
            // A `trust` that is not one is NO trust, so the plane is asked about again.
            // There is no shape of malformed approval that it is safe to read as a yes.
            trust: raw.get("trust").and_then(trust_of),
        });
    }

    let mut windows = Vec::new();
    for raw in array(doc.get("windows")) {
        let mut planes = Vec::new();
        for named in array(raw.get("planes")) {
            let named = named.as_str().unwrap_or_default().to_owned();
            match usable(&named) {
                Ok(plane) => planes.push(plane),
                Err(why) => dropped.push(Dropped::Window { plane: named, why }),
            }
        }
        if planes.is_empty() {
            continue;
        }
        windows.push(Window {
            // Clamped rather than dropped: an index past the end is what a dropped tab
            // leaves behind, and a window that opens on the wrong tab is a smaller wrong
            // than a window that does not open.
            active: (number(raw.get("active")) as usize).min(planes.len() - 1),
            planes,
        });
    }

    Store { recents, windows }
}

/// One approval off the file, or `None` for anything that is not one.
fn trust_of(raw: &serde_json::Value) -> Option<Trust> {
    if !raw.is_object() {
        return None;
    }
    Some(Trust {
        approved: number(raw.get("approved")),
        contributed: Contribution {
            plugins: words(raw.get("plugins")),
            env: words(raw.get("env")),
        },
    })
}

/// A JSON array, or nothing at all — never an error.
fn array(value: Option<&serde_json::Value>) -> &[serde_json::Value] {
    value
        .and_then(serde_json::Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
}

/// A JSON number as seconds or an index, or zero. A value that is not a number is zero: it is
/// an ordering hint and a tab index, and neither is worth dropping a row over.
fn number(value: Option<&serde_json::Value>) -> u64 {
    value
        .and_then(serde_json::Value::as_u64)
        .unwrap_or_default()
}

/// A JSON object of strings. A value that is not a string is dropped, which reads as the
/// plane contributing one thing fewer than it did — and therefore as [`Consent::Grew`] the
/// next time it is compared, which asks.
fn words(value: Option<&serde_json::Value>) -> BTreeMap<String, String> {
    value
        .and_then(serde_json::Value::as_object)
        .map(|map| {
            map.iter()
                .filter_map(|(name, value)| {
                    value.as_str().map(|value| (name.clone(), value.to_owned()))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The store as JSON, and the only place this file's field names are written down.
///
/// Writing is a derive and reading is by hand, deliberately: what charter writes is always
/// the same shape, and what it reads is whatever is on the disk.
#[derive(serde::Serialize)]
struct OnDisk {
    version: u32,
    /// When it was written, in seconds since the epoch. Nothing reads it; it is here because
    /// a file nobody can date is one nobody can debug.
    at: u64,
    recents: Vec<RecentOnDisk>,
    windows: Vec<WindowOnDisk>,
}

#[derive(serde::Serialize)]
struct RecentOnDisk {
    plane: String,
    opened: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    trust: Option<TrustOnDisk>,
}

#[derive(serde::Serialize)]
struct TrustOnDisk {
    approved: u64,
    plugins: BTreeMap<String, String>,
    env: BTreeMap<String, String>,
}

#[derive(serde::Serialize)]
struct WindowOnDisk {
    planes: Vec<String>,
    active: usize,
}

impl From<&Store> for OnDisk {
    fn from(store: &Store) -> Self {
        Self {
            version: VERSION,
            at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|since| since.as_secs())
                .unwrap_or_default(),
            recents: store
                .recents
                .iter()
                .take(MOST_RECENTS)
                // `to_str` and never `display`, which SUBSTITUTES for a byte that is not
                // UTF-8. JSON holds a string, so a path that is not one cannot be written
                // here honestly — and writing the lossy rendering would remember a path that
                // is not the one that was opened, and later open it.
                .filter_map(|entry| {
                    Some(RecentOnDisk {
                        plane: entry.plane.to_str()?.to_owned(),
                        opened: entry.opened,
                        trust: entry.trust.as_ref().map(|trust| TrustOnDisk {
                            approved: trust.approved,
                            plugins: trust.contributed.plugins.clone(),
                            env: trust.contributed.env.clone(),
                        }),
                    })
                })
                .collect(),
            windows: store
                .windows
                .iter()
                .map(|window| WindowOnDisk {
                    planes: window
                        .planes
                        .iter()
                        .filter_map(|plane| Some(plane.to_str()?.to_owned()))
                        .collect(),
                    active: window.active,
                })
                .collect(),
        }
    }
}

/// The store keeps nothing off unix, so its tests are unix's. What the other platforms do
/// instead is one refusal, and [`supported`] is where it is said.
#[cfg(all(test, unix))]
mod tests {
    use std::os::unix::fs::PermissionsExt;
    use std::time::Duration;

    use super::*;

    /// The app-data directory a test writes its store under.
    fn machine() -> tempfile::TempDir {
        tempfile::tempdir().expect("a temp app-data directory")
    }

    fn a_plane(at: &Path) -> PathBuf {
        std::fs::create_dir_all(at).unwrap();
        std::fs::write(at.join(crate::plane::MANIFEST), "").unwrap();
        at.to_path_buf()
    }

    fn mode_of(path: &Path) -> u32 {
        std::fs::symlink_metadata(path)
            .unwrap()
            .permissions()
            .mode()
            & 0o777
    }

    fn one_plane() -> Store {
        let mut store = Store::default();
        store.remember(
            Path::new("/Users/aharon/IdeaProjects/charter"),
            1_758_000_000,
        );
        store
    }

    // ---------------------------------------------------------------- what it is for

    #[test]
    fn what_was_written_is_what_the_next_launch_reads() {
        let machine = machine();
        let mut store = one_plane();
        store.approve(
            Path::new("/Users/aharon/work/other"),
            1_758_000_100,
            Contribution {
                plugins: [("market@repo".to_owned(), "true".to_owned())]
                    .into_iter()
                    .collect(),
                env: [("PATH".to_owned(), "/usr/bin".to_owned())]
                    .into_iter()
                    .collect(),
            },
        );
        store.windows = vec![Window {
            planes: vec![
                PathBuf::from("/Users/aharon/work/other"),
                PathBuf::from("/Users/aharon/IdeaProjects/charter"),
            ],
            active: 1,
        }];

        write(machine.path(), &store).unwrap();
        let back = read(machine.path());

        assert_eq!(back.store, store);
        assert_eq!(back.dropped, Vec::new());
        assert_eq!(back.unreadable, None);
    }

    #[test]
    fn a_machine_with_no_store_at_all_is_simply_a_first_launch() {
        let machine = machine();

        let back = read(machine.path());

        assert_eq!(back.store, Store::default());
        assert_eq!(back.unreadable, None, "a missing store is not a refusal");
    }

    #[test]
    fn the_most_recently_opened_plane_is_first_and_an_open_is_not_an_approval() {
        let mut store = Store::default();
        let first = Path::new("/planes/first");
        let second = Path::new("/planes/second");

        store.approve(first, 1, Contribution::default());
        store.remember(second, 2);
        store.remember(first, 3);

        assert_eq!(store.recents[0].plane, first);
        assert_eq!(store.recents[0].opened, 3);
        assert_eq!(store.recents[1].plane, second);
        assert!(
            store.recents[0].trust.is_some(),
            "re-opening an approved plane kept its approval"
        );
        assert!(
            store.recents[1].trust.is_none(),
            "opening a plane a second time must not become consent to it"
        );
    }

    #[test]
    fn the_list_is_bounded_so_a_file_read_at_every_launch_cannot_grow_for_ever() {
        let mut store = Store::default();
        for n in 0..MOST_RECENTS + 10 {
            store.remember(&PathBuf::from(format!("/planes/{n}")), n as u64);
        }

        assert_eq!(store.recents.len(), MOST_RECENTS);
        assert_eq!(
            store.recents[0].plane,
            PathBuf::from(format!("/planes/{}", MOST_RECENTS + 9)),
            "the newest is kept"
        );
    }

    #[test]
    fn forgetting_a_plane_forgets_the_approval_and_closes_its_tab() {
        let mut store = Store::default();
        let gone = Path::new("/planes/gone");
        store.approve(gone, 1, Contribution::default());
        store.remember(Path::new("/planes/kept"), 2);
        store.windows = vec![Window {
            planes: vec![PathBuf::from("/planes/kept"), gone.to_path_buf()],
            active: 1,
        }];

        store.forget(gone);

        assert!(store.recent(gone).is_none());
        assert_eq!(store.windows[0].planes, vec![PathBuf::from("/planes/kept")]);
        assert_eq!(store.windows[0].active, 0, "the active tab is still a tab");
        assert_eq!(
            store.consent(gone, &Contribution::default()),
            Consent::New,
            "a forgotten plane is asked about again"
        );
    }

    // ---------------------------------------------------------------- trust

    fn a_contribution(plugins: &[(&str, &str)], env: &[(&str, &str)]) -> Contribution {
        Contribution {
            plugins: plugins
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect(),
            env: env
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect(),
        }
    }

    #[test]
    fn a_plane_nobody_approved_is_asked_about() {
        let store = Store::default();

        let consent = store.consent(Path::new("/planes/new"), &Contribution::default());

        assert_eq!(consent, Consent::New);
        assert!(consent.must_ask());
    }

    #[test]
    fn an_approved_plane_that_contributes_what_it_did_opens_without_a_question() {
        let mut store = Store::default();
        let plane = Path::new("/planes/known");
        let same = a_contribution(&[("a@m", "true")], &[("PATH", "/usr/bin")]);
        store.approve(plane, 1, same.clone());

        let consent = store.consent(plane, &same);

        assert_eq!(consent, Consent::Unchanged);
        assert!(!consent.must_ask());
    }

    #[test]
    fn a_plane_that_added_a_plugin_after_it_was_approved_is_asked_about_again() {
        let mut store = Store::default();
        let plane = Path::new("/planes/known");
        store.approve(plane, 1, a_contribution(&[("a@m", "true")], &[]));

        let consent = store.consent(
            plane,
            &a_contribution(&[("a@m", "true"), ("evil@m", "true")], &[]),
        );

        assert!(consent.must_ask(), "a new plugin ran without being seen");
        assert_eq!(
            consent.changes(),
            [Change::PluginAdded("evil@m".to_owned())]
        );
    }

    #[test]
    fn a_plane_that_added_an_environment_variable_is_asked_about_again() {
        let mut store = Store::default();
        let plane = Path::new("/planes/known");
        store.approve(plane, 1, a_contribution(&[], &[("PATH", "/usr/bin")]));

        let consent = store.consent(
            plane,
            &a_contribution(&[], &[("PATH", "/usr/bin"), ("NODE_OPTIONS", "-r ./evil")]),
        );

        assert!(consent.must_ask());
        assert_eq!(
            consent.changes(),
            [Change::EnvAdded("NODE_OPTIONS".to_owned())]
        );
    }

    #[test]
    fn a_plane_that_changed_what_an_environment_variable_says_is_asked_about_again() {
        // The sharper half, and the reason the VALUE is recorded and not only the name: the
        // key set is identical here.
        let mut store = Store::default();
        let plane = Path::new("/planes/known");
        store.approve(plane, 1, a_contribution(&[], &[("PATH", "/usr/bin")]));

        let consent = store.consent(
            plane,
            &a_contribution(&[], &[("PATH", "/tmp/evil:/usr/bin")]),
        );

        assert!(consent.must_ask(), "a changed PATH is a grant nobody saw");
        assert_eq!(consent.changes(), [Change::EnvChanged("PATH".to_owned())]);
    }

    #[test]
    fn a_plane_that_takes_a_grant_back_says_so_and_does_not_ask() {
        let mut store = Store::default();
        let plane = Path::new("/planes/known");
        store.approve(
            plane,
            1,
            a_contribution(&[("a@m", "true")], &[("PATH", "/usr/bin")]),
        );

        let consent = store.consent(plane, &Contribution::default());

        assert!(
            !consent.must_ask(),
            "withdrawing a grant cannot make anything run, so asking only spends attention"
        );
        assert_eq!(
            consent,
            Consent::Shrank(vec![
                Change::PluginRemoved("a@m".to_owned()),
                Change::EnvRemoved("PATH".to_owned()),
            ])
        );
    }

    #[test]
    fn one_addition_among_removals_still_asks() {
        let mut store = Store::default();
        let plane = Path::new("/planes/known");
        store.approve(plane, 1, a_contribution(&[("a@m", "1"), ("b@m", "1")], &[]));

        let consent = store.consent(plane, &a_contribution(&[("c@m", "1")], &[]));

        assert!(
            consent.must_ask(),
            "the question is whether anything NEW was granted"
        );
    }

    #[test]
    fn what_a_plane_contributes_is_read_out_of_the_settings_that_travel() {
        let held = machine();
        let plane = a_plane(&held.path().join("plane"));
        std::fs::create_dir_all(plane.join(".claude")).unwrap();
        std::fs::write(
            plane.join(crate::layer::SETTINGS),
            r#"{
              "enabledPlugins": {"market@repo": true},
              "env": {"PATH": "/tmp/evil"},
              "permissions": {"allow": ["Bash(rm:*)"], "deny": ["Read(./secrets)"]}
            }"#,
        )
        .unwrap();

        let contributes = Contribution::of(&plane);

        assert_eq!(
            contributes,
            a_contribution(&[("market@repo", "true")], &[("PATH", "/tmp/evil")]),
            "exactly the two keys layer::WORKSPACE_KEYS carries into a harness's settings"
        );
    }

    #[test]
    fn a_plane_whose_settings_charter_cannot_read_contributes_nothing() {
        let held = machine();
        let plane = a_plane(&held.path().join("plane"));

        assert_eq!(Contribution::of(&plane), Contribution::default());
    }

    #[test]
    fn a_plugins_key_of_any_shape_is_still_compared_rather_than_read_as_nothing() {
        let as_array = plugins_of(&serde_json::json!(["a@m", "b@m"]));
        let as_object = plugins_of(&serde_json::json!({"a@m": true}));
        let as_nonsense = plugins_of(&serde_json::json!(7));

        assert_eq!(as_array.keys().collect::<Vec<_>>(), ["a@m", "b@m"]);
        assert_eq!(as_object.get("a@m"), Some(&"true".to_owned()));
        assert_eq!(as_nonsense.len(), 1, "{as_nonsense:?} was read as nothing");
    }

    // ---------------------------------------------------------------- the mode

    #[test]
    fn the_store_is_0600_in_a_0700_directory() {
        let machine = machine();

        write(machine.path(), &one_plane()).unwrap();

        assert_eq!(mode_of(&file(machine.path())), 0o600);
        assert_eq!(mode_of(&dir(machine.path())), 0o700);
    }

    #[test]
    fn the_mode_is_charters_and_not_whatever_the_umask_left() {
        // Without this the test above passes on a machine whose umask happens to be 077 even
        // with every mode dropped. A control made the ordinary way, in the same directory,
        // under the same umask: if the two ever match, this fails rather than quietly
        // vouching for nothing.
        let machine = machine();
        write(machine.path(), &one_plane()).unwrap();
        let control = dir(machine.path()).join("control");
        std::fs::write(&control, b"x").unwrap();

        assert_ne!(
            mode_of(&file(machine.path())),
            mode_of(&control),
            "charter's mode and the umask's are the same, so this suite proves nothing \
             about the mode: run it under a umask that is not 077"
        );
    }

    #[test]
    fn a_directory_that_was_already_there_at_0755_is_tightened() {
        // charter's own directory, at a path charter alone decides — unlike a plane's state
        // directory, which `$CHARTER_HOME` may point at a share.
        let machine = machine();
        std::fs::create_dir_all(dir(machine.path())).unwrap();
        std::fs::set_permissions(dir(machine.path()), std::fs::Permissions::from_mode(0o755))
            .unwrap();

        write(machine.path(), &one_plane()).unwrap();

        assert_eq!(mode_of(&dir(machine.path())), 0o700);
    }

    #[test]
    fn the_app_data_directory_itself_is_not_re_moded_by_charter() {
        // `~/.local/share` belongs to the operator and to every application on the machine.
        let held = machine();
        let data = held.path().join("share");
        std::fs::create_dir_all(&data).unwrap();
        std::fs::set_permissions(&data, std::fs::Permissions::from_mode(0o755)).unwrap();

        write(&data, &one_plane()).unwrap();

        assert_eq!(mode_of(&data), 0o755);
    }

    // ---------------------------------------------------------------- the gate

    #[test]
    fn a_link_at_the_store_itself_is_not_read_through() {
        let machine = machine();
        let elsewhere = machine.path().join("elsewhere.json");
        std::fs::write(&elsewhere, br#"{"version":1,"at":0,"recents":[]}"#).unwrap();
        std::fs::create_dir_all(dir(machine.path())).unwrap();
        std::os::unix::fs::symlink(&elsewhere, file(machine.path())).unwrap();

        let back = read(machine.path());

        assert!(
            back.unreadable.is_some(),
            "a linked store was read as this machine's"
        );
    }

    #[test]
    fn a_link_at_charters_own_directory_is_not_read_through() {
        let machine = machine();
        let elsewhere = machine.path().join("elsewhere");
        std::fs::create_dir_all(&elsewhere).unwrap();
        std::fs::write(
            elsewhere.join(FILE),
            br#"{"version":1,"at":0,"recents":[{"plane":"/planted","opened":0}]}"#,
        )
        .unwrap();
        std::os::unix::fs::symlink(&elsewhere, dir(machine.path())).unwrap();

        let back = read(machine.path());

        assert!(back.unreadable.is_some(), "{back:?}");
        assert_eq!(back.store, Store::default());
    }

    #[test]
    fn the_write_lands_on_a_path_that_is_gated_and_not_beside_one() {
        // The mistake this repo has had six review rounds on: the store's own path is
        // guarded while the bytes go to a temp file that is not. So the link is planted at
        // the path that is actually opened.
        let machine = machine();
        let captured = machine.path().join("captured.json");
        let dir = private_dir(machine.path()).unwrap();
        let temp = dir.join("machine.json.writing");
        std::os::unix::fs::symlink(&captured, &temp).unwrap();

        let refused = write_through(
            machine.path(),
            &dir.join(FILE),
            &temp,
            b"{\"version\":1,\"at\":0}",
        );

        assert!(refused.is_err(), "the temp path was written through");
        assert!(
            !captured.exists(),
            "the store was written outside charter's own directory"
        );
    }

    #[test]
    fn the_write_asks_about_the_directory_again_at_the_moment_of_the_create() {
        // `private_dir` refuses a linked `charter/` a moment earlier, and this is the only
        // thing between a directory swapped in AFTER that answer and the bytes. `O_NOFOLLOW`
        // cannot hold this: it answers about the last component and a directory above it is
        // not one. So `write_through` is called directly, with the link already there.
        let machine = machine();
        let elsewhere = machine.path().join("elsewhere");
        std::fs::create_dir_all(&elsewhere).unwrap();
        std::os::unix::fs::symlink(&elsewhere, dir(machine.path())).unwrap();
        let temp = dir(machine.path()).join("machine.json.writing");

        let refused = write_through(machine.path(), &file(machine.path()), &temp, b"{}");

        assert!(
            refused.is_err(),
            "the bytes went through a linked directory"
        );
        assert!(
            !elsewhere.join("machine.json.writing").exists(),
            "the store was written outside the app-data directory"
        );
    }

    #[test]
    fn a_write_through_a_linked_directory_is_refused_at_the_moment_of_the_create() {
        let machine = machine();
        let elsewhere = machine.path().join("elsewhere");
        std::fs::create_dir_all(&elsewhere).unwrap();
        std::os::unix::fs::symlink(&elsewhere, dir(machine.path())).unwrap();

        let refused = write(machine.path(), &one_plane());

        assert!(refused.is_err(), "the store was written through a link");
        assert!(!elsewhere.join(FILE).exists());
    }

    #[test]
    fn a_store_that_is_not_a_plain_file_is_refused_instead_of_read_for_ever() {
        // A FIFO is not a link, so a link check waves it through, and `read_to_string` on one
        // never returns — at a cold launch, before there is a window to close.
        let machine = machine();
        std::fs::create_dir_all(dir(machine.path())).unwrap();
        let made =
            crate::forklock::status(std::process::Command::new("mkfifo").arg(file(machine.path())))
                .expect("mkfifo runs");
        assert!(made.success(), "the test needs a fifo to plant");

        // In a thread, because the whole point is that the unguarded version never returns.
        let (say, heard) = std::sync::mpsc::channel();
        let asked = machine.path().to_path_buf();
        std::thread::spawn(move || say.send(read(&asked).unreadable));
        let answered = heard
            .recv_timeout(Duration::from_secs(5))
            .expect("reading a fifo store must not block a launch");

        assert!(answered.is_some(), "a fifo store was accepted");
    }

    #[test]
    fn a_store_too_large_to_be_one_is_refused_rather_than_read_whole() {
        let machine = machine();
        std::fs::create_dir_all(dir(machine.path())).unwrap();
        let planted = std::fs::File::create(file(machine.path())).unwrap();
        planted.set_len(MAX_BYTES + 1).unwrap();

        let back = read(machine.path());

        assert!(
            back.unreadable
                .is_some_and(|why| why.contains("never larger than")),
            "an oversized store was read whole"
        );
    }

    #[test]
    fn a_store_charter_could_not_read_is_never_overwritten() {
        let machine = machine();
        let captured = machine.path().join("captured.json");
        std::fs::write(&captured, b"the operator's other file").unwrap();
        std::fs::create_dir_all(dir(machine.path())).unwrap();
        std::os::unix::fs::symlink(&captured, file(machine.path())).unwrap();

        let refused = update(machine.path(), |store| {
            store.remember(Path::new("/planes/new"), 1)
        });

        assert!(refused.is_err(), "an unreadable store was clobbered");
        assert_eq!(
            std::fs::read_to_string(&captured).unwrap(),
            "the operator's other file",
            "the file behind the link was rewritten"
        );
    }

    #[test]
    fn a_store_charter_could_read_and_did_not_understand_is_replaced() {
        // The other half of the rule above, and the reason the two are told apart: content
        // that says nothing is worth replacing, a path charter cannot read is not.
        let machine = machine();
        std::fs::create_dir_all(dir(machine.path())).unwrap();
        std::fs::write(file(machine.path()), b"not json at all").unwrap();

        let done = update(machine.path(), |store| {
            store.remember(Path::new("/planes/new"), 1)
        })
        .expect("a malformed store is replaced, not a wall");

        assert!(matches!(done.dropped.as_slice(), [Dropped::TheStore(_)]));
        assert_eq!(read(machine.path()).store.recents.len(), 1);
    }

    #[test]
    fn a_store_of_another_version_is_not_guessed_at() {
        let machine = machine();
        std::fs::create_dir_all(dir(machine.path())).unwrap();
        std::fs::write(
            file(machine.path()),
            br#"{"version":99,"at":0,"recents":[{"plane":"/planes/old","opened":1}]}"#,
        )
        .unwrap();

        let back = read(machine.path());

        assert_eq!(back.store, Store::default());
        assert!(matches!(back.dropped.as_slice(), [Dropped::TheStore(_)]));
    }

    // ---------------------------------------------------------------- entries off the file

    #[test]
    fn a_remembered_path_that_is_not_absolute_is_dropped_with_a_reason() {
        let machine = machine();
        std::fs::create_dir_all(dir(machine.path())).unwrap();
        std::fs::write(
            file(machine.path()),
            br#"{"version":1,"at":0,"recents":[
                 {"plane":"relative/plane","opened":1},
                 {"plane":"/planes/good","opened":2}]}"#,
        )
        .unwrap();

        let back = read(machine.path());

        assert_eq!(back.store.recents.len(), 1, "{:?}", back.store.recents);
        assert_eq!(back.store.recents[0].plane, PathBuf::from("/planes/good"));
        assert!(
            back.dropped
                .iter()
                .any(|d| d.to_string().contains("relative/plane")),
            "a dropped row must say which one and why: {:?}",
            back.dropped
        );
    }

    #[test]
    fn a_remembered_path_that_walks_up_is_dropped() {
        let machine = machine();
        std::fs::create_dir_all(dir(machine.path())).unwrap();
        std::fs::write(
            file(machine.path()),
            br#"{"version":1,"at":0,"recents":[{"plane":"/planes/../../etc","opened":1}]}"#,
        )
        .unwrap();

        assert_eq!(read(machine.path()).store.recents, Vec::new());
    }

    #[test]
    fn one_malformed_entry_costs_one_row_and_not_the_list() {
        let machine = machine();
        std::fs::create_dir_all(dir(machine.path())).unwrap();
        std::fs::write(
            file(machine.path()),
            br#"{"version":1,"at":0,"recents":[
                 7,
                 {"plane":"/planes/good","opened":2}]}"#,
        )
        .unwrap();

        let back = read(machine.path());

        assert_eq!(back.store.recents.len(), 1);
        assert_eq!(back.dropped.len(), 1);
    }

    #[test]
    fn a_window_that_points_past_its_own_tabs_still_opens() {
        let machine = machine();
        std::fs::create_dir_all(dir(machine.path())).unwrap();
        std::fs::write(
            file(machine.path()),
            br#"{"version":1,"at":0,"recents":[],"windows":[
                 {"planes":["relative","/planes/good"],"active":1}]}"#,
        )
        .unwrap();

        let back = read(machine.path());

        assert_eq!(back.store.windows.len(), 1);
        assert_eq!(
            back.store.windows[0].planes,
            [PathBuf::from("/planes/good")]
        );
        assert_eq!(back.store.windows[0].active, 0);
    }

    #[test]
    fn a_plane_listed_twice_is_remembered_once() {
        let machine = machine();
        std::fs::create_dir_all(dir(machine.path())).unwrap();
        std::fs::write(
            file(machine.path()),
            br#"{"version":1,"at":0,"recents":[
                 {"plane":"/planes/a","opened":9},
                 {"plane":"/planes/a","opened":1}]}"#,
        )
        .unwrap();

        let back = read(machine.path());

        assert_eq!(back.store.recents.len(), 1);
        assert_eq!(
            back.store.recents[0].opened, 9,
            "the first one is the newest"
        );
    }

    #[test]
    fn a_file_holding_more_planes_than_charter_remembers_is_cut_to_the_bound() {
        let machine = machine();
        std::fs::create_dir_all(dir(machine.path())).unwrap();
        let rows: Vec<String> = (0..MOST_RECENTS + 5)
            .map(|n| format!(r#"{{"plane":"/planes/{n}","opened":{n}}}"#))
            .collect();
        std::fs::write(
            file(machine.path()),
            format!(r#"{{"version":1,"at":0,"recents":[{}]}}"#, rows.join(",")),
        )
        .unwrap();

        let back = read(machine.path());

        assert_eq!(back.store.recents.len(), MOST_RECENTS);
        assert_eq!(back.dropped.len(), 5);
    }

    // ---------------------------------------------------------------- the disk question

    #[test]
    fn a_path_that_is_not_utf_8_is_not_remembered_as_a_lossy_one() {
        // `display()` substitutes U+FFFD for a byte that is not UTF-8, so writing that
        // rendering would remember a path that is not the one the operator opened — and
        // later open it. JSON holds a string; a path that is not one is simply not kept.
        use std::os::unix::ffi::OsStrExt;
        let machine = machine();
        let mut store = Store::default();
        store.remember(
            Path::new(std::ffi::OsStr::from_bytes(b"/planes/not\xffutf8")),
            1,
        );
        store.remember(Path::new("/planes/ordinary"), 2);

        write(machine.path(), &store).unwrap();
        let back = read(machine.path());

        assert_eq!(
            back.store.recents.len(),
            1,
            "{:?} — a lossy path was written",
            back.store.recents
        );
        assert_eq!(
            back.store.recents[0].plane,
            PathBuf::from("/planes/ordinary")
        );
    }

    #[test]
    fn a_remembered_plane_that_is_still_a_plane_is_usable() {
        let held = machine();
        let plane = a_plane(&held.path().join("plane"));

        assert_eq!(still_a_plane(&plane), Ok(()));
    }

    #[test]
    fn a_remembered_plane_that_moved_or_stopped_being_one_is_dropped_with_a_reason() {
        let held = machine();
        let gone = held.path().join("gone");
        let not_a_plane = held.path().join("ordinary");
        std::fs::create_dir_all(&not_a_plane).unwrap();
        let a_file = held.path().join("file");
        std::fs::write(&a_file, b"x").unwrap();
        let linked = held.path().join("linked");
        std::os::unix::fs::symlink(a_plane(&held.path().join("real")), &linked).unwrap();

        assert!(still_a_plane(&gone).is_err_and(|why| why.contains("no longer there")));
        assert!(still_a_plane(&not_a_plane).is_err_and(|why| why.contains("not a plane")));
        assert!(still_a_plane(&a_file).is_err_and(|why| why.contains("not a directory")));
        assert!(still_a_plane(&linked).is_err_and(|why| why.contains("symlink")));
    }

    #[test]
    fn reading_the_store_never_touches_the_planes_it_names() {
        // The reason `still_a_plane` is a separate question: a cold launch that stats sixty
        // paths hangs on the first dead network mount, with nothing drawn to say so.
        let machine = machine();
        let mut store = Store::default();
        store.remember(Path::new("/planes/nothing-is-here"), 1);
        write(machine.path(), &store).unwrap();

        let back = read(machine.path());

        assert_eq!(
            back.store.recents.len(),
            1,
            "a path that does not exist is still remembered; whether to show it is a \
             separate question asked per row"
        );
    }

    #[test]
    fn unix_has_an_expression_for_the_mode_so_nothing_here_refuses() {
        // The other side of it is not testable from here: off unix the whole store refuses
        // (charter ADR 0031, charter-app#98) and this module's tests do not compile at all.
        // What that platform does is one sentence, and `supported` is where it is said.
        assert!(supported().is_ok());
    }
}
