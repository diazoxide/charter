//! Charter's generated layer inside a `workspaces/<ws>/` DIRECTORY, and that directory's
//! structure stamp.
//!
//! The other half of [`crate::layer`]'s two targets. A chat launched in `workspaces/<ws>/`
//! reads Claude Code's project settings from that directory and the host does not walk up, so
//! without this it runs with none of the plane's ask/deny rules, none of its `enabledPlugins`
//! and no `$CHARTER_HARNESS` — charter#850 exactly. Agents and skills DO arrive, because
//! those walk up and a workspace directory is not a git boundary: half a layer, and the half
//! that was missing is the half that runs.
//!
//! # What this target has that a checkout does not
//!
//! The `.charter-structure` stamp — the durable upgrade anchor. A workspace created by an
//! older charter can lack files a newer one expects, so the layout version it was built to is
//! stamped in a small local file; a workspace whose stamp is missing or older, or that is
//! missing a baseline file, reads as stale and is flagged until `charter workspace reinit`
//! heals it.
//!
//! # What a checkout has that this target does not
//!
//! **No `.git/info/exclude` entry, and that is measured rather than assumed.**
//! `/workspaces/*/*` is already in the plane's `.gitignore` and the managed LIVE block
//! un-ignores four named paths, none of them `.claude/`. Nothing generated here can reach a
//! commit, so there is no block to write and — the consequence that matters — no
//! `withheld` and no `unrecorded` row: those two exist in the Python because a checkout's
//! write is not made at all when its exclude line or its record cannot be published first. A
//! record that cannot be published here costs only the record, and the layer still lands.
//!
//! **No machine-local document.** Claude Code reads `.claude/settings.local.json` at the git
//! root as well as in the session's own directory, and a workspace directory is inside the
//! plane's own repository, so the plane's copy already reaches it. It is therefore never
//! wanted here — and since it is also the only path the harness itself writes into
//! ([`crate::guest`]'s `COWRITTEN`), the `harness-edited` and `harness-behind` states cannot
//! be reached in a workspace directory at all. See [`Found`] and [`Did`], which carry only
//! the states this target has.
//!
//! # Deliberate divergences from the Python, both in the refusing direction
//!
//! 1. **A symlink anywhere on the way is refused, wherever it points.** The Python's
//!    `_inside` resolves and allows a link that lands back inside the workspace;
//!    [`contain::no_link_on_the_way`] refuses every one. This is [`crate::guest`]'s rule
//!    already, and it can only ever report `foreign` where the Python would have rewritten a
//!    file through a link of the operator's.
//! 2. **The plane's `.claude/settings.json` is read only when it resolves inside the plane.**
//!    The Python reads it wherever a link points. Same direction: charter mirrors less, never
//!    more.
//!
//! Neither is reachable from a fixture plane, so both are stated here rather than waved
//! through in a differential scenario.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::contain;
use crate::layer::{self, Record};

/// The layout version [`scaffold`] produces. Bump it when the baseline grows, and every
/// workspace an older charter made flags itself for `charter workspace reinit`.
///
/// v2: memory is a per-file DB (a `MEMORY.md` index), not a lone `notes.md`.
/// v3: the managed `.gitignore` block shares `changes/` (not its log).
/// v4: the workspace carries charter's harness layer (charter#850).
/// v5: every workspace has a `workspace.json`, from birth (charter#884).
pub const STRUCTURE_VERSION: u32 = 5;

/// Where that version is stamped.
pub const STRUCTURE_MARKER: &str = ".charter-structure";

/// The pre-rename spelling, migrated in place the first time it is read.
///
/// Renaming the marker without moving it would silently reset every existing workspace to
/// v0: the new name is not there, so a fully up-to-date workspace reads as stale. Harmless
/// (reinit is additive) but wrong, noisy, and on a LIVE workspace it manufactures a commit.
pub const LEGACY_STRUCTURE_MARKER: &str = ".edm-structure";

/// The baseline files every workspace should have, in the order `reinit` names them.
pub const BASELINE: [&str; 4] = [
    "workspace.md",
    "workspace.json",
    "memory/MEMORY.md",
    "refs/README.md",
];

/// The top-level directory segments charter may generate a file under.
///
/// The boundary the DESTRUCTIVE side answers to, independent of any marker's digests: a
/// marker key naming a path outside these — `README.md`, `keep.txt` — names a file charter
/// never wrote and must never withdraw, whatever digest a planted marker records for it.
///
/// These are every registered harness's in-repo surface roots, as
/// `harness/registry.inherited_paths` gives them: Claude Code's `.claude/agents` and
/// `.claude/skills`, opencode's `.opencode/agent`, Codex's `.codex/skills`. Listed as the
/// roots rather than derived at runtime because charter-app has no harness registry of its
/// own yet — and stated here so the day it grows one, this is the call site to move.
const GENERATED_ROOTS: [&str; 3] = [".claude", ".opencode", ".codex"];

/// What charter found at one wanted path — READ ONLY. The Python's `_layer_status`.
///
/// `harness-edited` and `harness-behind` are absent on purpose: both require a path the
/// harness also writes into, and the only such path is a checkout's
/// `.claude/settings.local.json`, which a workspace directory never wants. See the module
/// docs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Found {
    /// There, holding exactly what the plane wants.
    Ok,
    /// Not there at all.
    Missing,
    /// Charter's own file, holding content charter's record vouches for, that the plane has
    /// moved on from.
    Stale,
    /// Somebody else's file — content charter cannot vouch for against the record, or a path
    /// reached through a symlink. Never repaired and never overwritten.
    ///
    /// The restraint ADR 0015 settles for an unstamped shim: charter cannot tell a file it
    /// wrote before the marker existed from one somebody rewrote, and guessing wrong in that
    /// direction destroys work.
    Foreign,
    /// There, and charter could not read it. Never folded into [`Found::Foreign`]: every
    /// `foreign` sentence advises moving a file aside, and charter only failed to read this
    /// one.
    Unreadable,
    /// A file charter generated, still exactly as written, that nothing generates any more.
    Unwanted,
}

/// What charter DID at one path. The Python's `wire_harnesses`.
///
/// `withheld` and `unrecorded` are absent for the reason the module docs give: both are a
/// checkout's, where a write whose exclude line or whose record cannot be published is not
/// made at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Did {
    /// Written where there was nothing.
    Created,
    /// Charter's own file, brought up to what the plane says now.
    Refreshed,
    /// Already current. `reinit` prints nothing for these.
    Present,
    /// Charter generated it, nothing generates it now, and it was removed.
    Removed,
    /// Somebody else's file, left exactly as found.
    Foreign,
    /// Charter tried to write and the filesystem refused — or the whole directory resolves
    /// out of the plane.
    Blocked,
    /// A generated path charter cannot read, left exactly as it is.
    Unreadable,
}

impl Did {
    /// Whether this row is a repair `reinit` counts.
    pub fn is_repair(self) -> bool {
        matches!(self, Did::Created | Did::Refreshed | Did::Removed)
    }

    /// Whether this row names a state `reinit` cannot clear, so the closing line may not say
    /// "nothing to do" over it.
    pub fn is_unresolved(self) -> bool {
        matches!(self, Did::Foreign | Did::Blocked | Did::Unreadable)
    }
}

/// One path and what happened to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub rel: String,
    pub did: Did,
}

/// What charter generates inside a workspace directory, as `{relative path: text}`.
///
/// One document today: the plane's own `.claude/settings.json`, filtered to the keys that
/// travel. Empty for a plane with nothing to say — the honest rendering, because writing an
/// empty `{}` would look like a layer.
pub fn want(plane: &Path) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    if let Some(text) = layer::settings_document(plane) {
        out.insert(layer::SETTINGS.to_owned(), text);
    }
    out
}

/// Whether `dir` is a workspace directory of THIS plane that charter may write into.
///
/// A `workspaces/<ws>` committed as a symlink out of the plane would otherwise plant the
/// whole baseline wherever it points, so the directory is refused **as a whole** — one
/// decision, rather than a race between several.
fn writable_directory(plane: &Path, dir: &Path) -> bool {
    let workspaces = plane.join("workspaces");
    let (Some(root), Some(here)) = (contain::resolved(&workspaces), contain::resolved(dir)) else {
        return false;
    };
    here != root && here.starts_with(&root)
}

/// `(relative path, what charter found)` for every path the layer wants — READ ONLY.
///
/// **Regenerate and compare**, never a stored diff: the generator's own output drifts as the
/// plane's settings move, so what "current" means is whatever the generator says today.
///
/// **Read only, because `doctor` calls this from a hook.** A check that writes is not a
/// check: it would report every workspace healthy by having just healed it.
pub fn status(
    plane: &Path,
    dir: &Path,
    want_all: &BTreeMap<String, String>,
) -> Vec<(String, Found)> {
    if !writable_directory(plane, dir) {
        // Charter reads no layer through a directory that resolves out of the plane, and
        // writes none. The row names the directory rather than the tree behind the link.
        return vec![(layer::MARKER.to_owned(), Found::Foreign)];
    }
    let record = layer::read_record(dir);
    let mut rows = found(plane, dir, want_all, &record);
    rows.extend(
        unwanted(plane, dir, want_all, &record)
            .into_iter()
            .map(|rel| (rel, Found::Unwanted)),
    );
    rows
}

/// [`status`]'s comparison for the wanted paths alone, against a record already in hand.
fn found(
    _plane: &Path,
    dir: &Path,
    want_all: &BTreeMap<String, String>,
    record: &Record,
) -> Vec<(String, Found)> {
    let mut rows = Vec::new();
    for (rel, want) in want_all {
        let path = dir.join(rel);
        // `symlink_metadata`, so a DANGLING link is not read as "absent" and written through.
        // Only a path that is certainly not there is `missing`; one the filesystem will not
        // answer for goes on to the read and is `unreadable` there.
        if matches!(path.symlink_metadata(), Err(ref e) if crate::memstore::is_absent(e)) {
            rows.push((rel.clone(), Found::Missing));
            continue;
        }
        if contain::no_link_on_the_way(dir, path.parent().unwrap_or(dir)).is_err() {
            rows.push((rel.clone(), Found::Foreign));
            continue;
        }
        let Ok(have) = std::fs::read_to_string(&path) else {
            rows.push((rel.clone(), Found::Unreadable));
            continue;
        };
        if &have == want {
            rows.push((rel.clone(), Found::Ok));
        } else if record.recorded(rel).contains(&layer::digest(&have)) {
            // Only content a record LISTS is charter's to overwrite, pending or settled.
            rows.push((rel.clone(), Found::Stale));
        } else {
            rows.push((rel.clone(), Found::Foreign));
        }
    }
    rows
}

/// Files charter generated here, still exactly as written, that nothing generates now.
///
/// READ ONLY — [`withdraw`] removes them and [`status`] reports them, and both ask here so
/// the two cannot disagree about which files those are.
///
/// **Only a path under one of charter's generated roots** ([`GENERATED_ROOTS`]): a marker
/// charter trusts can still name a file charter never wrote, and withdrawing on its word
/// deletes the operator's own file.
///
/// **Only while the file still matches a digest charter recorded** — a file the operator has
/// since edited is not charter's to delete, and unreadable counts as not charter's for the
/// same reason.
///
/// **Never a held path**: a plane file charter cannot READ is not a plane that stopped
/// declaring something, and withdrawing over it took every workspace's `enabledPlugins`,
/// `env` and `deny` away over a typo.
fn unwanted(
    plane: &Path,
    dir: &Path,
    want_all: &BTreeMap<String, String>,
    record: &Record,
) -> Vec<String> {
    let _held = layer::held(plane);
    let mut _roots: BTreeSet<&str> = GENERATED_ROOTS.into_iter().collect();
    for rel in want_all.keys() {
        _roots.insert(root_of(rel));
    }
    let mut out = Vec::new();
    // In the record's own path order, which `BTreeMap` fixes. A set's iteration order would
    // be whatever a hash gives, and `reinit`'s report must not inherit that.
    for rel in record.paths() {
        if want_all.contains_key(rel) {
            continue;
        }
        let path = dir.join(rel);
        let Ok(have) = std::fs::read_to_string(&path) else {
            continue;
        };
        if record.recorded(rel).contains(&layer::digest(&have)) {
            out.push(rel.clone());
        }
    }
    out
}

/// The SHALLOWEST directory above `path` whose own check meets `code` — else `path` itself,
/// whose check already met it.
///
/// A check through a symlink fails at the first component that fails, not at the path it asked
/// about: with a workspace's `refs/` a loop, `refs/README.md` answers ELOOP, and "fix the
/// symlink loop at refs/README.md" sends the reader to a file that is no link at all. A
/// directory that answers anything but `code` is not taken for the cause, so a filesystem that
/// changed in between costs precision and never names a path that did not fail that way.
pub fn stopped_at(path: &Path, code: Option<i32>) -> PathBuf {
    let mut parents: Vec<&Path> = path.ancestors().skip(1).collect();
    parents.reverse();
    for up in parents {
        if matches!(up.metadata(), Err(e) if e.raw_os_error() == code && !crate::memstore::is_absent(&e))
        {
            return up.to_path_buf();
        }
    }
    path.to_path_buf()
}

/// Whether `path` resolves inside `root`, with BOTH ends resolved.
fn resolves_inside(path: &Path, root: &Path) -> bool {
    match (contain::resolved(path), contain::resolved(root)) {
        (Some(here), Some(root)) => here.starts_with(&root),
        _ => false,
    }
}

/// The top-level segment of a relative path — the whole of it when there is no slash.
fn root_of(rel: &str) -> &str {
    rel.split('/').next().unwrap_or(rel)
}

/// Remove what charter generated here and no longer generates.
///
/// A generated file only ever ARRIVED until the layer started carrying the plane's `ask` and
/// `deny` rules. Now a plane that drops its last rule stops wanting a whole generated file —
/// so without this, a restriction could be put in force in every workspace and never lifted
/// from any of them. `charter guard` has no remove verb; hand-editing the plane's settings is
/// how a rule goes, and a mirror that is one-way turns that edit into a lie.
fn withdraw(
    plane: &Path,
    dir: &Path,
    want_all: &BTreeMap<String, String>,
    record: &mut Record,
) -> Vec<Row> {
    let mut rows = Vec::new();
    for rel in unwanted(plane, dir, want_all, record) {
        let path = dir.join(rel.as_str());
        // Re-checked at the moment of the unlink, not carried from the classification: this
        // is the destructive verb, and it is the one that must not act on a stale answer.
        if contain::no_link_on_the_way(dir, &path).is_err() {
            continue;
        }
        if std::fs::remove_file(&path).is_err() {
            // Gone since it was read, or held by a directory that will not let go. It stays
            // in the record and stays reported `unwanted`, and the next wire tries again.
            continue;
        }
        record.forget(&rel);
        if let Some(parent) = path.parent() {
            layer::prune_empty(parent.to_path_buf(), dir);
        }
        rows.push(Row {
            rel,
            did: Did::Removed,
        });
    }
    // An entry whose file is PROVED gone vouches for nothing, and is forgotten. Proved:
    // `lexists` answered False for a refused `lstat` too, and one EACCES during a launch
    // forgot `settings.json` for good.
    let gone: Vec<String> = record
        .paths()
        .filter(|rel| {
            matches!(dir.join(rel.as_str()).symlink_metadata(), Err(ref e) if crate::memstore::is_absent(e))
        })
        .cloned()
        .collect();
    for rel in gone {
        record.forget(&rel);
    }
    rows
}

/// Materialise the harness layer into the workspace directory `dir`.
///
/// **Refresh, not create-once.** A file whose digest is in the record is charter's, and
/// charter brings its own files up to date; a file whose digest is not is the operator's and
/// is left exactly as found. Create-once was right about the operator's files and wrong about
/// charter's: a document generated by an old version survived every upgrade afterwards while
/// `doctor` reported the tree wired (ADR 0015).
///
/// Called from [`scaffold`], so a launch gets it. `charter workspace reinit` is the repair.
///
/// # What this does NOT do
///
/// It does not descend into the guest CHECKOUTS inside the workspace — a clone, or a linked
/// worktree, each a git root of its own that cuts a chat off from the plane far more sharply
/// than this directory does. [`crate::guest::wire`] is that machinery and it exists; what has
/// no port is the Python's row vocabulary for it (`unhidden`, `unlisted`, `unrecorded`,
/// `withheld`) and the `.git/info/exclude` bookkeeping those rows report on. [`checkouts`]
/// names them so `reinit` can say so out loud rather than print "up to date" over an unwired
/// clone.
pub fn wire(plane: &Path, dir: &Path) -> Vec<Row> {
    let want_all = want(plane);
    let mut record = layer::read_record(dir);
    let before = record.clone();
    let mut rows = withdraw(plane, dir, &want_all, &mut record);
    let mut writes: Vec<(String, Found)> = Vec::new();
    let mut published = before;
    for (rel, what) in found(plane, dir, &want_all, &record) {
        match what {
            Found::Unreadable => rows.push(Row {
                rel,
                did: Did::Unreadable,
            }),
            Found::Foreign => rows.push(Row {
                rel,
                did: Did::Foreign,
            }),
            Found::Ok => {
                // A record that does not say what an `ok` file holds is SETTLED on it:
                // pending over a write that finished, or settled by a launch that lost a race
                // to another, whose record then named text the file no longer held — and the
                // plane's next move would have called charter's own file foreign.
                if record.names(&rel)
                    && let Some(text) = want_all.get(&rel)
                {
                    record.settle(&rel, layer::digest(text));
                }
                rows.push(Row {
                    rel,
                    did: Did::Present,
                });
            }
            // Already tried above; there is nothing here to write.
            Found::Unwanted => {}
            Found::Missing | Found::Stale => writes.push((rel, what)),
        }
    }
    if !writes.is_empty() {
        // **Intent first.** Every file about to be written is published as PENDING — the
        // digests it may hold while the write is under way — before a byte of it changes, so
        // no kill between the write and its record can leave a file on disk that the record
        // calls somebody else's.
        //
        // Unlike a checkout, a workspace directory does not abandon the write when this
        // publish fails: there is no exclude block whose line the next launch would drop, so
        // a record that cannot be published costs only the record and the layer still lands.
        let mut intent = record.clone();
        for (rel, _) in &writes {
            if let Some(text) = want_all.get(rel) {
                intent.pend(rel, layer::digest(text));
            }
        }
        let _ = layer::publish(dir, &intent);
        record = intent;
        // What the settle below is compared with: the record THIS pass published, never the
        // one it read. Compared with the record read at the start, a pass that wrote a deleted
        // file again ended on that very record and skipped the settle, so the pending entry it
        // had just published stayed pending for good.
        published = record.clone();
    }
    for (rel, what) in writes {
        let Some(text) = want_all.get(&rel) else {
            continue;
        };
        match layer::write_into(dir, &rel, text) {
            Err(_) => {
                // The entry stays pending and the file holds what it held — writes are whole —
                // and the next wire writes it again.
                rows.push(Row {
                    rel,
                    did: Did::Blocked,
                });
            }
            Ok(()) => {
                record.settle(&rel, layer::digest(text));
                rows.push(Row {
                    rel,
                    did: if what == Found::Missing {
                        Did::Created
                    } else {
                        Did::Refreshed
                    },
                });
            }
        }
    }
    if record != published {
        // Only when something changed. Rewriting the record on every launch would move a
        // workspace's mtimes for a call that changed nothing.
        let _ = layer::publish(dir, &record);
    }
    // NOT sorted: the order is the Python's — what was withdrawn, then what was found in
    // path order, then what was written. `reinit` prints these one by one, so an order of
    // this port's own would be a different report for one plane.
    rows
}

/// Every checkout directly inside the workspace directory that charter is a GUEST in — a
/// clone or a linked worktree, each a git root of its own.
///
/// Named so `reinit` can report what it did not wire. `.git` is a DIRECTORY in a clone and a
/// FILE reading `gitdir: <path>` in a linked worktree, and both count: this asks "where would
/// a chat's cwd be cut off from the plane's layer", and a worktree's root is a git boundary
/// exactly as a clone's is.
pub fn checkouts(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.join(".git").exists())
        .collect();
    out.sort();
    out
}

// --------------------------------------------------------------------------------------- //
// the structure stamp                                                                       //
// --------------------------------------------------------------------------------------- //

/// The stamp's path, migrating a pre-rename `.edm-structure` the first time it is read.
///
/// Rename rather than re-stamp, so a genuinely older marker keeps its own version instead of
/// being claimed as current.
pub fn structure_marker(dir: &Path) -> PathBuf {
    let new = dir.join(STRUCTURE_MARKER);
    let legacy = dir.join(LEGACY_STRUCTURE_MARKER);
    let (here, there) = (
        new.symlink_metadata().is_ok(),
        legacy.symlink_metadata().is_ok(),
    );
    if !here && there && std::fs::rename(&legacy, &new).is_err() {
        // Unreadable, or across devices: still read the old one.
        return legacy;
    }
    if here && there {
        // Both present: the new one already won.
        let _ = std::fs::remove_file(&legacy);
    }
    new
}

/// `(version, blocker)` for the structure stamp — the ONE read of it.
///
/// Opened `O_NOFOLLOW | O_NONBLOCK` and judged by the descriptor: `read_to_string` opened a
/// FIFO at that name and waited for a writer that never came, so one stray local file froze
/// `reinit` and every status-line render in the workspace. A link is not read through either,
/// wherever it points: charter never writes the stamp through one, so a version read through
/// one is not charter's.
pub fn stamp(dir: &Path) -> (u32, Option<Blocker>) {
    let marker = structure_marker(dir);
    let Ok(file) = std::fs::File::open(&marker) else {
        // Absent, a link (`O_NOFOLLOW`'s ELOOP), or unanswered.
        return (0, in_the_way(dir, &marker));
    };
    let Ok(meta) = file.metadata() else {
        return (0, None);
    };
    if !meta.is_file() {
        return (
            0,
            Some(Blocker {
                path: marker,
                why: if meta.is_dir() {
                    Why::IsADirectory
                } else {
                    Why::NotARegularFile
                },
            }),
        );
    }
    // A bounded read, not `read_to_string`: the stamp is a few bytes, and a regular file at
    // that name that is not one is not charter's to swallow whole.
    use std::io::Read;
    let mut text = String::new();
    if file.take(4096).read_to_string(&mut text).is_err() {
        return (0, None);
    }
    (text.trim().parse().unwrap_or(0), None)
}

/// Write the stamp, never through a link.
///
/// `O_NOFOLLOW` refuses a link wherever it points, `O_NONBLOCK` a FIFO nobody reads instead of
/// hanging the launch, and a directory refuses itself. The stamp is local, so a refused stamp
/// costs only the stamp: the workspace re-flags stale, and nothing is written into a tree
/// charter did not choose.
pub fn write_stamp(dir: &Path) -> std::io::Result<()> {
    use std::io::Write;
    let marker = structure_marker(dir);
    let mut file = contain::create_no_link(dir, &marker)?;
    file.write_all(format!("{STRUCTURE_VERSION}\n").as_bytes())
}

/// Why a path of the layout is not the thing the layout needs there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Why {
    /// A symlink whose target is not there. Charter writes nothing through it.
    Dangling,
    /// Something that is not a directory, where a directory goes.
    NotADirectory,
    /// A symlink charter will not follow — where a file belongs, wherever it points, or
    /// where a directory belongs and it does not land on one inside the plane.
    IsALink,
    /// A directory where a file goes.
    IsADirectory,
    /// Any other file that is not a regular one — a FIFO, a socket, a device.
    NotARegularFile,
}

/// The path that stands where the layout needs something else, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Blocker {
    pub path: PathBuf,
    pub why: Why,
}

/// The path from `base` down to `p` that stands where the layout needs something else — or
/// `None` when nothing does.
///
/// "Gone" is read as "create it", and a create through a link to nowhere, or under a file,
/// raises out of `workspace reinit`. So "gone because something stands where its directory
/// goes" is told apart here, ONCE, for `reinit` to name and for [`scaffold`] to write nothing
/// under: the two cannot disagree about one path.
///
/// A link where a FILE belongs is in the way wherever it points — git stores a symlink as a
/// symlink, so a teammate's commit can put one at `refs/README.md`, and a plain write through
/// it creates a file wherever it points. A link where a DIRECTORY belongs is in the way unless
/// it lands on a directory inside the plane's data.
///
/// Nothing ABOVE `base` is asked: the plane root may be a link for reasons nobody committed
/// (`/var` is one on macOS), and every caller has already reached `base` through it.
pub fn in_the_way(base: &Path, p: &Path) -> Option<Blocker> {
    let Ok(below) = p.strip_prefix(base) else {
        return None;
    };
    let mut walk = vec![base.to_path_buf()];
    let steps: Vec<_> = below.components().collect();
    for step in steps.iter().take(steps.len().saturating_sub(1)) {
        let mut next = walk.last().expect("seeded with base").clone();
        next.push(step);
        walk.push(next);
    }
    walk.push(p.to_path_buf());
    let last = walk.len() - 1;
    for (n, q) in walk.iter().enumerate() {
        let Ok(link) = q.symlink_metadata().map(|m| m.file_type().is_symlink()) else {
            // ONE clause. Absent is what a create makes, and unanswered is not charter's to
            // call in the way either; neither belongs here.
            return None;
        };
        if n == last {
            return None;
        }
        let Ok(meta) = q.metadata() else {
            // The name is THERE — the `symlink_metadata` above answered for it — and resolves
            // to nothing. A create under it raises, so it is in the way rather than gone.
            return Some(Blocker {
                path: q.clone(),
                why: Why::Dangling,
            });
        };
        if !meta.is_dir() {
            return Some(Blocker {
                path: q.clone(),
                why: if link {
                    Why::IsALink
                } else {
                    Why::NotADirectory
                },
            });
        }
        // A directory of the layout that IS a link answers to the same containment every
        // write here does — and to a DIFFERENT tree depending on which directory it is, which
        // is the Python's split verbatim. The workspace directory itself must resolve inside
        // `workspaces/`, because that is the test `scaffold` and the layer refuse the whole
        // directory by; anything beneath it must resolve inside the workspace.
        //
        // Both ends are RESOLVED before they are compared. On macOS `/var` is itself a link,
        // so a lexical `starts_with` against an unresolved base calls every link in a
        // `$TMPDIR` plane an escape.
        let inside = if n == 0 {
            base.parent().unwrap_or(base)
        } else {
            base
        };
        if link && !resolves_inside(q, inside) {
            return Some(Blocker {
                path: q.clone(),
                why: Why::IsALink,
            });
        }
    }
    None
}

/// What one baseline path answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Baseline {
    pub rel: &'static str,
    pub path: PathBuf,
    /// `Some(true)`/`Some(false)` when the filesystem answered, `None` when it would not.
    pub there: Option<bool>,
    /// The errno behind a `there` of `None`, which is what words the repair: a symlink loop
    /// names the link, because no permission bit is in its way.
    pub code: Option<i32>,
    /// What stands where this path's directory — or the path itself — goes.
    pub blocker: Option<Blocker>,
}

impl Baseline {
    /// Whether [`scaffold`] may write this path: it is certainly not there, and nothing is in
    /// the way.
    fn clear(&self) -> bool {
        self.there == Some(false) && self.blocker.is_none()
    }
}

/// `{rel: what it answered}` for every baseline path — the ONE classification
/// [`structure_status`] reports and [`scaffold`] writes by.
pub fn baseline(dir: &Path) -> Vec<Baseline> {
    BASELINE
        .into_iter()
        .map(|rel| {
            let path = dir.join(rel);
            // Through the link (`metadata`, not `symlink_metadata`): the question "is
            // `workspace.json` there" means the manifest behind the link, and a dangling link
            // is a manifest that is not there.
            let (there, code) = match path.metadata() {
                Ok(_) => (Some(true), None),
                Err(ref e) if crate::memstore::is_absent(e) => (Some(false), None),
                Err(e) => (None, e.raw_os_error()),
            };
            // Asked of what answered "there" as well as of what answered "gone": a link at a
            // baseline name to a file that IS there answers "there" through the link, and is
            // in the way all the same.
            let blocker = there.and_then(|_| in_the_way(dir, &path));
            Baseline {
                rel,
                path,
                there,
                code,
                blocker,
            }
        })
        .collect()
}

/// Is the workspace's on-disk layout current?
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Structure {
    /// No baseline file missing AND the stamp up to date.
    pub ok: bool,
    /// Baseline paths that are certainly not there with nothing in the way — the ones
    /// [`scaffold`] adds.
    pub missing: Vec<&'static str>,
    /// Baseline paths the filesystem would not answer for. Not `missing`: scaffolding over a
    /// path charter cannot see is a write into something charter cannot see.
    pub unreadable: Vec<Baseline>,
    /// Baseline paths, and the stamp, with something in the way.
    pub in_the_way: Vec<(String, Blocker)>,
    pub version: u32,
    pub target: u32,
}

/// [`Structure`] for the workspace directory `dir`.
pub fn structure_status(dir: &Path) -> Structure {
    let seen = baseline(dir);
    let missing: Vec<&'static str> = seen.iter().filter(|b| b.clear()).map(|b| b.rel).collect();
    let (version, stamp_blocker) = stamp(dir);
    let mut blocked: Vec<(String, Blocker)> = seen
        .iter()
        .filter_map(|b| b.blocker.clone().map(|w| (b.rel.to_owned(), w)))
        .collect();
    // The stamp is a row of `in_the_way` too, never of `missing`: an absent stamp is the
    // version line's to report, and one that is not a regular file is what `reinit` names.
    if let Some(why) = stamp_blocker {
        blocked.push((STRUCTURE_MARKER.to_owned(), why));
    }
    Structure {
        ok: missing.is_empty() && version >= STRUCTURE_VERSION,
        missing,
        unreadable: seen.iter().filter(|b| b.there.is_none()).cloned().collect(),
        in_the_way: blocked,
        version,
        target: STRUCTURE_VERSION,
    }
}

/// Whether an existing workspace's structure is stale.
pub fn needs_reinit(dir: &Path) -> bool {
    dir.exists() && !structure_status(dir).ok
}

/// Create a workspace's baseline structure: `memory/` (the per-file DB and its index),
/// `refs/`, the `workspace.md` charter, the `workspace.json` manifest, the harness layer,
/// and the structure-version stamp. Idempotent and additive.
///
/// **Writes nothing when the workspace directory itself resolves outside the plane.** Below
/// that, each baseline path is judged by [`baseline`] and the stamp by the kernel: nothing is
/// written for a path that could not be checked or has something in the way, so `reinit`
/// names what this refuses rather than raising out of it.
pub fn scaffold(
    plane: &crate::workspaces::Plane,
    name: &str,
    now: chrono::DateTime<chrono::Utc>,
    author: &str,
) -> Vec<Row> {
    let Ok(workspace) = plane.workspace(name) else {
        return Vec::new();
    };
    let dir = workspace.dir().to_path_buf();
    if !writable_directory(plane.root(), &dir) {
        return Vec::new();
    }
    let clear: BTreeMap<&str, bool> = baseline(&dir)
        .into_iter()
        .map(|b| (b.rel, b.clear()))
        .collect();
    let is_clear = |rel: &str| clear.get(rel).copied().unwrap_or(false);
    if is_clear("memory/MEMORY.md") {
        let _ = workspace.scaffold_memory();
    }
    if is_clear("refs/README.md") {
        let refs = dir.join("refs");
        if std::fs::create_dir_all(&refs).is_ok() {
            // Created only where it is certainly absent, and through the containment gate at
            // the exact path opened — the check above is a `stat`, and a link planted between
            // the two is followed by a plain write.
            let readme = refs.join("README.md");
            if let Ok(mut file) = contain::create_no_link(&dir, &readme) {
                use std::io::Write;
                let _ = file.write_all(
                    format!(
                        "# {name} — task references\n\nDrop docs, links, and snippets for \
                         this task here (local, gitignored).\n"
                    )
                    .as_bytes(),
                );
            }
        }
    }
    if is_clear("workspace.md") {
        let _ = workspace.scaffold_charter();
    }
    if is_clear("workspace.json") {
        let _ = workspace.scaffold_manifest(now, author);
    }
    let layer = wire(plane.root(), &dir);
    // The stamp is LOCAL, so a refused write costs only the stamp: the workspace re-flags
    // stale and nothing is written into a tree charter did not choose. `reinit` reads it back
    // rather than trusting this call, because a swallowed failure printed "added structure
    // v0 → v5" over a stamp that was never written.
    let _ = write_stamp(&dir);
    layer
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A plane with one workspace directory and a settings file worth mirroring.
    fn plane() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("charter.toml"), "").unwrap();
        std::fs::create_dir_all(dir.path().join(".claude")).unwrap();
        std::fs::write(
            dir.path().join(layer::SETTINGS),
            r#"{"env":{"CHARTER_HARNESS":"claude-code"},
                "permissions":{"ask":["Bash(terraform apply *)"]}}"#,
        )
        .unwrap();
        let ws = dir.path().join("workspaces").join("alpha");
        std::fs::create_dir_all(&ws).unwrap();
        (dir, ws)
    }

    /// Run the scaffold against a workspace of the test plane, as `ensure` does.
    fn scaffold_into(plane: &tempfile::TempDir, name: &str) {
        let opened = crate::workspaces::Plane::open(plane.path());
        scaffold(
            &opened,
            name,
            "2026-05-04T11:32:17Z".parse().unwrap(),
            "fixture",
        );
    }

    fn rows(rows: &[Row]) -> Vec<(&str, Did)> {
        rows.iter().map(|r| (r.rel.as_str(), r.did)).collect()
    }

    #[test]
    fn a_workspace_with_no_layer_gets_one_and_a_record_that_names_it() {
        let (plane, ws) = plane();
        assert_eq!(
            rows(&wire(plane.path(), &ws)),
            [(layer::SETTINGS, Did::Created)]
        );
        let text = std::fs::read_to_string(ws.join(layer::SETTINGS)).unwrap();
        assert!(text.contains("terraform apply"), "{text}");
        assert!(text.contains("CHARTER_HARNESS"), "{text}");
        let record = layer::read_record(&ws);
        assert_eq!(
            record.settled(layer::SETTINGS),
            Some(layer::digest(&text).as_str())
        );
    }

    #[test]
    fn a_second_wire_changes_nothing_and_says_so() {
        let (plane, ws) = plane();
        wire(plane.path(), &ws);
        let marker_before = std::fs::metadata(ws.join(layer::MARKER))
            .unwrap()
            .modified()
            .unwrap();
        assert_eq!(
            rows(&wire(plane.path(), &ws)),
            [(layer::SETTINGS, Did::Present)]
        );
        // The record is republished only when something changed.
        assert_eq!(
            std::fs::metadata(ws.join(layer::MARKER))
                .unwrap()
                .modified()
                .unwrap(),
            marker_before
        );
    }

    #[test]
    fn a_plane_that_moved_refreshes_charters_own_file() {
        let (plane, ws) = plane();
        wire(plane.path(), &ws);
        std::fs::write(
            plane.path().join(layer::SETTINGS),
            r#"{"permissions":{"deny":["Bash(rm -rf *)"]}}"#,
        )
        .unwrap();
        assert_eq!(
            rows(&wire(plane.path(), &ws)),
            [(layer::SETTINGS, Did::Refreshed)]
        );
        let text = std::fs::read_to_string(ws.join(layer::SETTINGS)).unwrap();
        assert!(text.contains("rm -rf"), "{text}");
    }

    #[test]
    fn a_file_charter_did_not_write_is_left_exactly_as_it_is() {
        let (plane, ws) = plane();
        std::fs::create_dir_all(ws.join(".claude")).unwrap();
        std::fs::write(ws.join(layer::SETTINGS), "MINE\n").unwrap();
        assert_eq!(
            rows(&wire(plane.path(), &ws)),
            [(layer::SETTINGS, Did::Foreign)]
        );
        assert_eq!(
            std::fs::read_to_string(ws.join(layer::SETTINGS)).unwrap(),
            "MINE\n"
        );
        // And charter vouches for nothing it did not write.
        assert!(!ws.join(layer::MARKER).exists());
    }

    #[test]
    fn a_plane_that_stops_declaring_anything_withdraws_the_file_it_generated() {
        let (plane, ws) = plane();
        assert_eq!(
            rows(&wire(plane.path(), &ws)),
            [(layer::SETTINGS, Did::Created)]
        );
        // Every key that travels, gone. Not an unreadable file: a plane that is HOLDING its
        // settings is the other case, and it withdraws nothing.
        std::fs::write(plane.path().join(layer::SETTINGS), r#"{"hooks":{}}"#).unwrap();
        assert_eq!(
            rows(&wire(plane.path(), &ws)),
            [(layer::SETTINGS, Did::Removed)]
        );
        assert!(!ws.join(layer::SETTINGS).exists());
        // The `.claude/` it lived in goes too — charter standing in a directory it has
        // nothing in.
        assert!(!ws.join(".claude").exists());
        // And the record, which now names nothing.
        assert!(!ws.join(layer::MARKER).exists());
    }

    #[test]
    fn a_plane_holding_an_unparseable_settings_file_withdraws_nothing() {
        let (plane, ws) = plane();
        wire(plane.path(), &ws);
        let kept = std::fs::read_to_string(ws.join(layer::SETTINGS)).unwrap();
        std::fs::write(plane.path().join(layer::SETTINGS), "{ not json").unwrap();
        // NO ROW AT ALL, which is the Python's answer too and worth being exact about: a plane
        // that is HOLDING its settings wants nothing, so there is nothing to compare — and the
        // record's entry is skipped rather than withdrawn. A `removed` row here would be every
        // workspace's `enabledPlugins`, `env` and `deny` taken away over a typo.
        assert_eq!(rows(&wire(plane.path(), &ws)), []);
        assert_eq!(
            std::fs::read_to_string(ws.join(layer::SETTINGS)).unwrap(),
            kept,
            "charter kept the last good copy"
        );
        // And the record still names it, so the day the plane parses again the file reads as
        // charter's own rather than as somebody else's.
        assert_eq!(
            layer::read_record(&ws).settled(layer::SETTINGS),
            Some(layer::digest(&kept).as_str())
        );
    }

    #[test]
    fn a_record_naming_a_file_outside_charters_generated_roots_withdraws_nothing() {
        let (plane, ws) = plane();
        std::fs::write(ws.join("README.md"), "the operator's\n").unwrap();
        // A marker charter trusts can still name a file charter never wrote.
        std::fs::write(
            ws.join(layer::MARKER),
            serde_json::to_string(&serde_json::json!({
                "README.md": layer::digest("the operator's\n"),
            }))
            .unwrap(),
        )
        .unwrap();
        let did = wire(plane.path(), &ws);
        assert!(
            !did.iter().any(|r| r.rel == "README.md"),
            "{did:?} must not touch a file outside charter's generated roots"
        );
        assert!(ws.join("README.md").exists());
    }

    #[test]
    fn a_record_whose_key_leaves_the_workspace_is_not_charters_record() {
        let (plane, ws) = plane();
        let outside = plane.path().parent().unwrap().join("victim.json");
        std::fs::write(&outside, "PRECIOUS\n").unwrap();
        std::fs::write(
            ws.join(layer::MARKER),
            serde_json::to_string(&serde_json::json!({
                "../../../victim.json": layer::digest("PRECIOUS\n"),
            }))
            .unwrap(),
        )
        .unwrap();
        wire(plane.path(), &ws);
        assert_eq!(std::fs::read_to_string(&outside).unwrap(), "PRECIOUS\n");
    }

    #[cfg(unix)]
    #[test]
    fn a_workspace_directory_that_is_a_link_out_of_the_plane_is_refused_whole() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("charter.toml"), "").unwrap();
        std::fs::create_dir_all(dir.path().join(".claude")).unwrap();
        std::fs::write(dir.path().join(layer::SETTINGS), r#"{"env":{"A":"1"}}"#).unwrap();
        let outside = dir.path().parent().unwrap().join("elsewhere-ws");
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::create_dir_all(dir.path().join("workspaces")).unwrap();
        let ws = dir.path().join("workspaces").join("escape");
        std::os::unix::fs::symlink(&outside, &ws).unwrap();
        assert_eq!(
            rows(&wire(dir.path(), &ws)),
            [(layer::MARKER, Did::Blocked)]
        );
        assert!(!outside.join(".claude").exists());
        assert!(!outside.join(layer::MARKER).exists());
        let _ = std::fs::remove_dir_all(&outside);
    }

    #[cfg(unix)]
    #[test]
    fn a_settings_path_reached_through_a_link_is_foreign_and_never_written_through() {
        let (plane, ws) = plane();
        // Charter's own layer first, so there IS a record — then the file is replaced by a
        // link out of the plane pointing at content that record still vouches for, while the
        // plane moves on. Without the containment check the row reads `stale`, which is
        // charter's own file to rewrite, and the write lands wherever the link points.
        //
        // The weaker shape of this test — a link to content nothing vouches for — passes with
        // the check DELETED, because unrecorded content reads `foreign` on its own. This one
        // does not: it is the digest, not the path, that would let the write through.
        wire(plane.path(), &ws);
        let mine = std::fs::read_to_string(ws.join(layer::SETTINGS)).unwrap();
        let outside = plane.path().parent().unwrap().join("victim-settings.json");
        std::fs::write(&outside, &mine).unwrap();
        std::fs::remove_file(ws.join(layer::SETTINGS)).unwrap();
        // Planted at the EXACT path the write opens, which is the only place a test of this
        // proves anything: a link one directory up is caught by a gate on the parent, and a
        // gate on the parent is the mistake this repository keeps shipping.
        std::os::unix::fs::symlink(&outside, ws.join(layer::SETTINGS)).unwrap();
        std::fs::write(
            plane.path().join(layer::SETTINGS),
            r#"{"permissions":{"deny":["Bash(rm -rf *)"]}}"#,
        )
        .unwrap();

        assert_eq!(
            status(plane.path(), &ws, &want(plane.path())),
            [(layer::SETTINGS.to_string(), Found::Foreign)],
            "a path charter reaches through a link is not one its record can vouch for"
        );
        assert_eq!(
            rows(&wire(plane.path(), &ws)),
            [(layer::SETTINGS, Did::Foreign)]
        );
        assert_eq!(
            std::fs::read_to_string(&outside).unwrap(),
            mine,
            "charter wrote the plane's new rules through a link out of the plane"
        );
        let _ = std::fs::remove_file(&outside);
    }

    #[cfg(unix)]
    #[test]
    fn a_withdrawal_is_never_performed_through_a_link_out_of_the_workspace() {
        let (plane, ws) = plane();
        wire(plane.path(), &ws);
        let mine = std::fs::read_to_string(ws.join(layer::SETTINGS)).unwrap();
        // The same shape one verb further on: the record vouches for what the link POINTS at,
        // so without the check the withdraw finds a digest it trusts and unlinks a path of the
        // operator's. `remove_file` goes at the link node, so what is lost is their link.
        let outside = plane.path().parent().unwrap().join("victim-withdrawn.json");
        std::fs::write(&outside, &mine).unwrap();
        std::fs::remove_file(ws.join(layer::SETTINGS)).unwrap();
        std::os::unix::fs::symlink(&outside, ws.join(layer::SETTINGS)).unwrap();
        // And the plane stops declaring anything, so this path is now unwanted.
        std::fs::write(plane.path().join(layer::SETTINGS), r#"{"hooks":{}}"#).unwrap();

        assert!(
            !status(plane.path(), &ws, &want(plane.path()))
                .iter()
                .any(|(_, found)| *found == Found::Unwanted),
            "a path reached through a link is not charter's to withdraw"
        );
        assert!(
            !wire(plane.path(), &ws)
                .iter()
                .any(|row| row.did == Did::Removed)
        );
        assert!(
            ws.join(layer::SETTINGS).symlink_metadata().is_ok(),
            "charter removed a link the operator put there"
        );
        let _ = std::fs::remove_file(&outside);
    }

    #[cfg(unix)]
    #[test]
    fn a_claude_directory_that_is_a_link_out_is_refused_at_the_component_it_is_on() {
        let (plane, ws) = plane();
        let outside = plane.path().parent().unwrap().join("victim-claude");
        std::fs::create_dir_all(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, ws.join(".claude")).unwrap();
        let did = wire(plane.path(), &ws);
        // `missing` at the leaf, so the write is attempted and the gate on the WAY is what
        // has to stop it.
        assert_eq!(rows(&did), [(layer::SETTINGS, Did::Blocked)]);
        assert!(!outside.join("settings.json").exists());
        let _ = std::fs::remove_dir_all(&outside);
    }

    #[cfg(unix)]
    #[test]
    fn a_pending_record_left_by_a_blocked_write_still_vouches_for_the_old_file() {
        use std::os::unix::fs::PermissionsExt;

        let (plane, ws) = plane();
        wire(plane.path(), &ws);
        let first = std::fs::read_to_string(ws.join(layer::SETTINGS)).unwrap();
        // The plane moves, so charter's own file is `stale` and charter is about to rewrite
        // it. The write is then made to fail with the FILE STILL THERE, which is the only
        // shape that tests this: a path that is GONE is one the record forgets by design, and
        // a link on the way makes the row `foreign` before any write is attempted.
        std::fs::write(
            plane.path().join(layer::SETTINGS),
            r#"{"permissions":{"deny":["Bash(rm -rf *)"]}}"#,
        )
        .unwrap();
        let claude = ws.join(".claude");
        std::fs::set_permissions(&claude, std::fs::Permissions::from_mode(0o500)).unwrap();
        // `r-x`: the file is still readable and still `stat`-able, and the temp `write_whole`
        // writes beside it cannot be created. Root ignores the bits, so the state is asserted
        // rather than assumed — a test that quietly did nothing here would report every guard
        // below it green.
        let probe = std::fs::File::create_new(claude.join("probe"));
        assert!(
            probe.is_err(),
            "this test needs a user the mode bits apply to; it cannot run as root"
        );

        assert_eq!(
            rows(&wire(plane.path(), &ws)),
            [(layer::SETTINGS, Did::Blocked)]
        );

        // The entry vouches for BOTH: what the file holds, and what the write would have put
        // there. Silence is not a verdict — a record published only AFTER the write would
        // leave the old digest beside a file that may already hold the new text, and charter's
        // own file then reads as somebody else's for ever.
        let record = layer::read_record(&ws);
        let wanted = layer::digest(&want(plane.path())[layer::SETTINGS]);
        assert!(
            record
                .recorded(layer::SETTINGS)
                .contains(&layer::digest(&first))
        );
        assert!(record.recorded(layer::SETTINGS).contains(&wanted));
        assert_eq!(
            record.settled(layer::SETTINGS),
            None,
            "a pending entry is not a verdict about what the file holds"
        );
        assert_eq!(
            std::fs::read_to_string(ws.join(layer::SETTINGS)).unwrap(),
            first,
            "a write that did not land left the file exactly as it was"
        );

        // And once the write can land, the file charter last wrote is still charter's own to
        // refresh rather than somebody else's to leave alone.
        std::fs::set_permissions(&claude, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(
            rows(&wire(plane.path(), &ws)),
            [(layer::SETTINGS, Did::Refreshed)]
        );
        assert_eq!(
            layer::read_record(&ws).settled(layer::SETTINGS),
            Some(wanted.as_str()),
            "and the record settles on what is now there"
        );
    }

    #[test]
    fn the_read_only_status_reports_what_a_wire_would_do_and_writes_nothing() {
        let (plane, ws) = plane();
        // Missing, before anything is written.
        assert_eq!(
            status(plane.path(), &ws, &want(plane.path())),
            [(layer::SETTINGS.to_string(), Found::Missing)]
        );
        // And it is READ ONLY — `doctor` calls it from a hook, and a check that heals reports
        // every workspace healthy by having just healed it.
        assert!(!ws.join(layer::SETTINGS).exists());
        assert!(!ws.join(layer::MARKER).exists());

        wire(plane.path(), &ws);
        assert_eq!(
            status(plane.path(), &ws, &want(plane.path())),
            [(layer::SETTINGS.to_string(), Found::Ok)]
        );

        // The plane moves: charter's own file, still holding what charter's record names.
        std::fs::write(
            plane.path().join(layer::SETTINGS),
            r#"{"permissions":{"deny":["Bash(rm -rf *)"]}}"#,
        )
        .unwrap();
        assert_eq!(
            status(plane.path(), &ws, &want(plane.path())),
            [(layer::SETTINGS.to_string(), Found::Stale)]
        );

        // The plane stops declaring anything: a file nothing generates any more.
        std::fs::write(plane.path().join(layer::SETTINGS), r#"{"hooks":{}}"#).unwrap();
        assert_eq!(
            status(plane.path(), &ws, &want(plane.path())),
            [(layer::SETTINGS.to_string(), Found::Unwanted)]
        );
        assert!(
            ws.join(layer::SETTINGS).exists(),
            "reporting a withdrawal is not performing one"
        );
    }

    #[test]
    fn a_generated_path_charter_cannot_read_is_unreadable_and_never_foreign() {
        let (plane, ws) = plane();
        wire(plane.path(), &ws);
        // A directory at the generated path: there, and not readable as text. Not `foreign`,
        // whose sentence advises moving a file aside — charter only failed to read this one.
        std::fs::remove_file(ws.join(layer::SETTINGS)).unwrap();
        std::fs::create_dir(ws.join(layer::SETTINGS)).unwrap();
        assert_eq!(
            status(plane.path(), &ws, &want(plane.path())),
            [(layer::SETTINGS.to_string(), Found::Unreadable)]
        );
        assert_eq!(
            rows(&wire(plane.path(), &ws)),
            [(layer::SETTINGS, Did::Unreadable)]
        );
        assert!(ws.join(layer::SETTINGS).is_dir(), "left exactly as it is");
    }

    #[test]
    fn every_status_this_target_can_reach_is_named_by_a_test() {
        // The enumeration the ticket asks for, as an assertion rather than a comment: each
        // of these is produced by a test above, and the ones a workspace directory CANNOT
        // reach are named in the module docs with the reason.
        let reachable = [
            Did::Created,
            Did::Refreshed,
            Did::Present,
            Did::Removed,
            Did::Foreign,
            Did::Blocked,
            Did::Unreadable,
        ];
        assert_eq!(reachable.len(), 7);
        assert_eq!(reachable.iter().filter(|d| d.is_repair()).count(), 3);
        assert_eq!(reachable.iter().filter(|d| d.is_unresolved()).count(), 3);
    }

    #[test]
    fn a_fresh_stamp_reads_back_the_version_it_was_written_with() {
        let (_plane, ws) = plane();
        assert_eq!(stamp(&ws), (0, None));
        write_stamp(&ws).unwrap();
        assert_eq!(stamp(&ws), (STRUCTURE_VERSION, None));
    }

    #[cfg(unix)]
    #[test]
    fn a_stamp_that_is_a_link_is_neither_read_through_nor_written_through() {
        let (plane, ws) = plane();
        let outside = plane.path().parent().unwrap().join("victim-stamp");
        std::fs::write(&outside, "99\n").unwrap();
        std::os::unix::fs::symlink(&outside, ws.join(STRUCTURE_MARKER)).unwrap();
        let (version, blocker) = stamp(&ws);
        assert_eq!(version, 0);
        assert_eq!(blocker.map(|b| b.why), Some(Why::IsALink));
        assert!(write_stamp(&ws).is_err());
        assert_eq!(std::fs::read_to_string(&outside).unwrap(), "99\n");
        let _ = std::fs::remove_file(&outside);
    }

    #[test]
    fn a_stamp_that_is_a_directory_is_named_rather_than_written_over() {
        let (_plane, ws) = plane();
        std::fs::create_dir_all(ws.join(STRUCTURE_MARKER)).unwrap();
        let (version, blocker) = stamp(&ws);
        assert_eq!(version, 0);
        assert_eq!(blocker.map(|b| b.why), Some(Why::IsADirectory));
        assert!(write_stamp(&ws).is_err());
    }

    #[test]
    fn a_legacy_stamp_is_renamed_rather_than_restamped() {
        let (_plane, ws) = plane();
        std::fs::write(ws.join(LEGACY_STRUCTURE_MARKER), "3\n").unwrap();
        // The version it really is, not the one charter is shipping.
        assert_eq!(stamp(&ws).0, 3);
        assert!(ws.join(STRUCTURE_MARKER).exists());
        assert!(!ws.join(LEGACY_STRUCTURE_MARKER).exists());
    }

    #[test]
    fn a_baseline_file_under_something_that_is_not_a_directory_is_in_the_way_not_missing() {
        let (_plane, ws) = plane();
        std::fs::write(ws.join("refs"), "not a directory\n").unwrap();
        let status = structure_status(&ws);
        assert!(!status.missing.contains(&"refs/README.md"));
        let (rel, blocker) = status
            .in_the_way
            .iter()
            .find(|(rel, _)| rel == "refs/README.md")
            .expect("named in_the_way");
        assert_eq!(rel, "refs/README.md");
        assert_eq!(blocker.why, Why::NotADirectory);
    }

    #[cfg(unix)]
    #[test]
    fn a_baseline_file_that_is_a_dangling_link_is_in_the_way_and_never_written_through() {
        let (plane, ws) = plane();
        let never = plane.path().parent().unwrap().join("never-made.md");
        std::os::unix::fs::symlink(&never, ws.join("workspace.md")).unwrap();
        let status = structure_status(&ws);
        // A link where a FILE belongs is in the way wherever it points.
        assert_eq!(
            status
                .in_the_way
                .iter()
                .find(|(rel, _)| rel == "workspace.md")
                .map(|(_, b)| b.why),
            Some(Why::IsALink)
        );
        assert!(!status.missing.contains(&"workspace.md"));
        assert!(!never.exists());
    }

    #[cfg(unix)]
    #[test]
    fn a_baseline_directory_that_is_a_link_out_of_the_workspace_is_in_the_way() {
        let (plane, ws) = plane();
        let outside = plane.path().parent().unwrap().join("victim-refs");
        std::fs::create_dir_all(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, ws.join("refs")).unwrap();
        let status = structure_status(&ws);
        let (_rel, blocker) = status
            .in_the_way
            .iter()
            .find(|(rel, _)| rel == "refs/README.md")
            .expect("named in_the_way");
        // The link is named, not the file under it: that is where the repair is.
        assert_eq!(blocker.why, Why::IsALink);
        assert_eq!(blocker.path, ws.join("refs"));
        assert!(!status.missing.contains(&"refs/README.md"));
        // And nothing is written out there.
        scaffold_into(&plane, "alpha");
        assert!(!outside.join("README.md").exists());
        let _ = std::fs::remove_dir_all(&outside);
    }

    #[cfg(unix)]
    #[test]
    fn a_baseline_directory_that_is_a_link_back_inside_the_workspace_is_not_in_the_way() {
        let (_plane, ws) = plane();
        // The other side of the same test, and the reason both ends are resolved before they
        // are compared: on macOS `$TMPDIR` is itself reached through a link, so a lexical
        // comparison calls every honest link in a temp plane an escape.
        std::fs::create_dir_all(ws.join("real-refs")).unwrap();
        std::os::unix::fs::symlink(ws.join("real-refs"), ws.join("refs")).unwrap();
        let status = structure_status(&ws);
        assert!(
            !status
                .in_the_way
                .iter()
                .any(|(rel, _)| rel == "refs/README.md"),
            "{status:?}"
        );
        assert!(status.missing.contains(&"refs/README.md"));
    }

    #[test]
    fn a_workspace_with_the_whole_baseline_and_the_stamp_reads_current() {
        let (_plane, ws) = plane();
        for rel in BASELINE {
            let path = ws.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, "x\n").unwrap();
        }
        write_stamp(&ws).unwrap();
        let status = structure_status(&ws);
        assert!(status.ok, "{status:?}");
        assert!(status.missing.is_empty());
        assert_eq!(status.version, STRUCTURE_VERSION);
    }

    #[test]
    fn an_older_stamp_is_stale_even_with_the_whole_baseline() {
        let (_plane, ws) = plane();
        for rel in BASELINE {
            let path = ws.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, "x\n").unwrap();
        }
        std::fs::write(ws.join(STRUCTURE_MARKER), "4\n").unwrap();
        assert!(!structure_status(&ws).ok);
        assert!(needs_reinit(&ws));
    }
}
