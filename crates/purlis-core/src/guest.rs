//! Charter's guest layer: the plane's rules, agents and `$CHARTER_HARNESS`, written into a
//! checkout that has a git root of its own, and hidden in that checkout's own
//! `info/exclude`.
//!
//! A port of the parts of `charter/workspace.py` a worktree needs — `_guest_files`,
//! `wire_guest`, `_register_excludes` and the ownership rules around them — narrowed to the
//! one tree kind this milestone is about. It closes the gap ADR 0027 named and M1.4 recorded
//! as a todo: *a chat in a charter-cut worktree ran with no persona agents, none of the
//! plane's ask/deny rules and no `$CHARTER_HARNESS`.*
//!
//! # Why a checkout needs this at all
//!
//! Claude Code binds configuration to the directory a session launched in. Project settings
//! are read from that directory and the host does not walk up; agents and skills DO walk up,
//! but the walk **stops at the git root**. `workspaces/<ws>/.worktrees/<repo>/<piece>` is a
//! git root of its own, so a chat started there finds none of the plane's layer — not the
//! settings that carry `env` and the ask/deny rules, and not the persona agents either. The
//! `working-in-a-clone` skill states the same boundary in the operator's words.
//!
//! # Write, rather than refuse
//!
//! A refusal is right where charter cannot finish something alone and a pass would be a lie.
//! Here charter **can** finish alone — it cut the tree, and the layer is files in a directory
//! charter owns. So charter writes it, and keeps the refusal for the cases where the write does not land. What an
//! operator loses under a pure refusal is every worktree the app cuts, until they go and run
//! the Python; what they lose under a silent write is nothing, because a write that cannot
//! be hidden is not performed at all.
//!
//! # The repo's own `git status` is not charter's to dirty
//!
//! Charter is a guest. Every path it generates is registered in the checkout's
//! `info/exclude` — per-checkout, never committed, and the one file a guest may write — and
//! **the block is written before the first file is**. If the block cannot be written, no
//! file is written either and the layer is reported blocked. That is stronger than the
//! Python's (which withholds only the machine-local file), and it makes the guarantee
//! unconditional: nothing charter wrote can ever show in the operator's `git status`.
//!
//! For a **linked worktree** that file is not the worktree's own. Git treats `info/` as
//! shared, so a pattern written into `.git/worktrees/<id>/info/exclude` is read by nobody;
//! the file git reads is the common directory's, which `commondir` points at. So the block a
//! piece writes is the block its clone reads, and a line is therefore only ever **added** —
//! see [`block`].
//!
//! # What charter may overwrite, and what it may not
//!
//! A path is charter's when a marker charter could have written records the digest the file
//! still has. Anything else is the operator's: it is never rewritten, and a wanted path
//! holding somebody else's file means the plane's rules are **not** in force there, which is
//! a refusal and not a shrug. The one exception is a path the harness itself writes into —
//! `.claude/settings.local.json`, where "Yes, and don't ask again" lands — whose digest moves
//! without the file becoming anybody else's.
//!
//! A layer record (`.purlis-generated`, or `.charter-generated` from before the rename) git
//! **tracks** is not charter's record at all. Charter's own is
//! per-checkout and untracked, so a tracked one is content some cloned repository committed,
//! and trusting its digests is how a repo names charter's files as its own to redirect.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::layer::{self, digest, write_into, write_whole};
use crate::names;

/// The sidecar recording what charter last generated in a checkout, as
/// `{relative path: sha256 of the text charter wrote}`.
///
/// [`crate::layer`]'s, because a checkout and a workspace directory must not be able to
/// disagree about where charter's record lives.
pub const MARKER: &str = layer::MARKER;

/// The generated document carrying the plane's `env` (and so `$CHARTER_HARNESS`), its
/// `enabledPlugins`, and its shared ask/deny rules.
pub const SETTINGS: &str = layer::SETTINGS;

/// The generated document carrying the plane's **machine-local** ask/deny rules.
///
/// Only in a checkout: Claude Code reads this file at the git root as well as in the
/// session's own directory, so a directory inside the plane's repository already reads the
/// plane's copy and a checkout of its own reads nothing of it.
pub const LOCAL_SETTINGS: &str = ".claude/settings.local.json";

/// The generated paths the harness also writes into itself. Charter keeps them listed and
/// never rewrites one whose content has moved.
const COWRITTEN: [&str; 1] = [LOCAL_SETTINGS];

/// The plane-root directories a checkout of its own cuts a session off from, mirrored 1:1.
///
/// `CLAUDE.md` is deliberately absent: it walks up and is **not** git-bounded, so the plane's
/// own copy already reaches here, and a mirror would put the plane's instructions inside
/// somebody else's repository to be read as that repository's.
const WALKUP_DIRS: [&str; 2] = [".claude/agents", ".claude/skills"];

/// The delimiters of charter's managed block in a checkout's `info/exclude`.
///
/// Delimited rather than "charter's lines are the ones charter recognises": an operator's own
/// `/.claude/settings.json` line, written before charter ever arrived, is indistinguishable
/// from charter's by content alone, and a removal would take it with it.
///
/// These are the lines written. A block between either name's lines is charter's
/// ([`names::EXCLUDE_BEGIN`], [`names::EXCLUDE_END`]), and one charter wrote is rewritten in
/// place under these, never left beside a second block (V93i).
pub const EXCLUDE_BEGIN: &str = names::EXCLUDE_BEGIN.write;
pub const EXCLUDE_END: &str = names::EXCLUDE_END.write;

/// The lines inside the block, for the person who finds them in a repo they own.
const EXCLUDE_NOTE: [&str; 3] = [
    "# Files purlis generated in this checkout so a chat here gets the plane's layer.",
    "# Listed one by one — purlis hides only what it wrote, never a directory of yours.",
    "# This file is per-checkout and never committed; nothing your teammates clone is affected.",
];

/// Every temp [`write_whole`] writes through, as a pattern. Unanchored, because a temp is
/// written beside every file charter writes, and listed before the first one exists so a temp
/// a kill leaves behind stays hidden.
const TEMP_PATTERN: &str = ".purlis-generated.*.tmp";

/// Whether `rel` is the temp pattern under any name the temps have had: a block charter wrote
/// lists `.charter-generated.*.tmp`, and its line stays while a temp of that name is left.
fn is_temp_pattern(rel: &str) -> bool {
    names::GENERATED_TEMP_PREFIX
        .strip(rel)
        .is_some_and(|rest| rest == "*.tmp")
}

/// What charter found, or did, at one generated path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Charter wrote it where there was nothing.
    Created,
    /// Charter's own file, brought up to what the plane says now.
    ///
    /// Told apart from [`Status::Created`] because `charter workspace reinit` says which one
    /// happened, and charter's own report does: "wrote" and "refreshed" are different facts
    /// about a checkout an operator is looking at, and one word for both cannot be compared
    /// against the other implementation at all.
    Refreshed,
    /// It was already there, with the content charter's record names.
    Current,
    /// Somebody else's file is at that path. Never rewritten.
    Foreign,
    /// A path the harness also writes into, holding content charter did not write. Charter's
    /// to keep hidden, never charter's to rewrite.
    Theirs,
    /// Charter tried and the filesystem refused.
    Blocked,
    /// A generated path that is THERE and charter could not read, left exactly as it is.
    ///
    /// Never folded into [`Status::Foreign`]: every `foreign` sentence advises moving a file
    /// aside, and charter only failed to read this one — which may be the file the harness
    /// keeps its approvals in.
    Unreadable,
    /// A machine-local path charter would have written and did **not**, because the line that
    /// would hide it was left out of the shared block over a file of the operator's in
    /// another checkout reading the same exclude (charter#1072).
    ///
    /// Never [`Status::Blocked`]: nothing is in the way at that path, and `blocked`'s wording
    /// sends the operator looking for an obstruction that is not there. What stopped the
    /// write is that the file could not be hidden — and a machine-local file charter cannot
    /// hide is one `git add` from being committed into somebody else's repository.
    Withheld,
    /// Charter could not publish its record here **first**, so it wrote nothing and kept
    /// every exclude line it had — charter's ruling H.
    ///
    /// The window this closes: the write lands, the record does not, and the next launch
    /// reads the old record beside the new file, calls charter's own file somebody else's,
    /// and drops its line out of a shared exclude. Never [`Status::Blocked`] either: nothing
    /// is in the way at the generated path, and the repair is wherever the record goes.
    Unrecorded,
    /// A mirrored agent or skill purlis wrote here, still exactly as written, that the
    /// project no longer has: taken out, with its record entry and its line
    /// (`withdraw_mirrors`).
    Removed,
    /// The project has newer text for this path, and the file there is one the checkout's
    /// record names but the project's own note of what it offered does not (`vouched`).
    /// purlis cannot confirm it wrote that text, so it is left exactly as it is.
    ///
    /// Never [`Status::Refreshed`]: the record sits in the checkout, where a chat can write
    /// it, and on its own it would let a chat have purlis overwrite a file of the operator's
    /// that the chat itself may not touch. Never [`Status::Foreign`] either: it may well be
    /// purlis's own older copy, written before the project noted what it offered.
    Unconfirmed,
}

/// One generated path and what happened to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub rel: String,
    pub status: Status,
    /// Why, when the status is one that has a why. Empty otherwise.
    pub why: String,
}

/// Whether the block that hides charter's files is in place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Hidden {
    /// The block lists every path charter is about to own.
    InPlace,
    /// It does not, and so **nothing was written**. The second string is the repair.
    Blocked(String, String),
}

/// What happened to the block itself — charter's row for `.git/info/exclude`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Block {
    /// The file had no block of charter's and now has one.
    Created,
    /// It had one, and the lines in it changed.
    Refreshed,
    /// It had one, and it already said what this wire needs.
    Present,
    /// No block was written and none was needed: charter owns nothing in this checkout and
    /// is about to own nothing. A block naming only charter's own marker would be a write
    /// into a repository charter has nothing in.
    Untouched,
    /// The block is current, or was just written, and it **leaves out a line this checkout
    /// needs** — because that line would hide an untracked file of the operator's in another
    /// checkout reading the same exclude (charter#1072).
    ///
    /// It wins over `created`, `refreshed` and `present`, because all three mean "charter's
    /// files are hidden now", which is the one thing a line left out makes untrue. It does
    /// **not** win over blocked. [`unhidden`] says which path, whose file stopped it, and
    /// what clears it.
    Unhidden,
    /// Charter could not write the block on the pass that settles it, after the files were
    /// written. Reported so that "the layer is in force here" is never said over it.
    Blocked,
}

/// What one wire of a checkout did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wired {
    pub rows: Vec<Row>,
    pub hidden: Hidden,
    /// What happened to `.git/info/exclude`. charter reports it as a row of the checkout's
    /// own beside the files, so a reader can tell "your files are hidden now" from "they
    /// already were".
    pub block: Block,
}

/// Whether a row is one that stops the layer being in force.
///
/// One predicate, because "is this layer complete" and "what does the refusal name" must not
/// be able to disagree: a row the first counts and the second cannot find is a start that is
/// refused with an empty sentence.
/// Whether a row is one that stops the layer being in force.
///
/// [`Status::Withheld`] is deliberately NOT one of them, and that is charter's own answer:
/// the file withheld is the machine-local one, the plane's committed rules and the persona
/// agents are written and hidden, and the operator's own file is what is in the way. Charter
/// says so through `doctor` and `reinit` and lets the chat run; refusing it would take a
/// worktree away over a file somebody else put in a sibling checkout.
fn failed(status: Status) -> bool {
    matches!(
        status,
        Status::Foreign | Status::Blocked | Status::Unrecorded | Status::Unconfirmed
    )
}

impl Wired {
    /// Whether the plane's layer is in force in this tree now.
    ///
    /// Every wanted path is charter's and current, and the block hides them. `Theirs` counts:
    /// a machine-local file the harness rewrote is still a file whose rules the harness reads.
    pub fn complete(&self) -> bool {
        matches!(self.hidden, Hidden::InPlace)
            && self.block != Block::Blocked
            && !self.rows.iter().any(|r| failed(r.status))
    }

    /// The one sentence saying why the layer is not in force, or empty when it is.
    ///
    /// Names the path and the repair, because a refusal an operator cannot act on is a
    /// refusal they will route around.
    pub fn refusal(&self, tree: &Path) -> String {
        let shown = crate::shown::readable(&tree.display().to_string(), usize::MAX);
        if let Hidden::Blocked(why, fix) = &self.hidden {
            return format!(
                "purlis could not hide its own files in {shown} ({why}), so it wrote none of \
                 them — a chat there would run without the plane's ask/deny rules, its persona \
                 agents and $CHARTER_HARNESS. {fix}"
            );
        }
        let found = self.rows.iter().find(|r| failed(r.status));
        let Some(bad) = found else {
            if self.block == Block::Blocked {
                return format!(
                    "purlis wrote its files in {shown} and could not settle the block that \
                     hides them, so some of them show in that repository's own `git status`. \
                     Restore write access to its .git/info/exclude and run `purlis workspace \
                     reinit`."
                );
            }
            return String::new();
        };
        let rel = crate::shown::short(&bad.rel);
        match bad.status {
            Status::Foreign => format!(
                "{rel} in {shown} is a file purlis did not write, so the plane's layer is not \
                 in force there and purlis will not overwrite it. Move it aside and start the \
                 chat again, or start this chat in the clone."
            ),
            Status::Unconfirmed => format!(
                "the project has newer text for {rel} in {shown}, but purlis cannot confirm it \
                 wrote the file there now, so it is left as is and the plane's layer is not in \
                 force there. If it is purlis's own older copy, move it aside and start the \
                 chat again."
            ),
            Status::Unrecorded => format!(
                "purlis could not publish its record in {shown} first ({}), so it wrote \
                 nothing and kept every exclude line it had — a chat there would run without \
                 the plane's layer.",
                bad.why
            ),
            _ => format!(
                "purlis could not write {rel} in {shown} ({}), so a chat there would run \
                 without the plane's layer. Restore write access and start the chat again.",
                bad.why
            ),
        }
    }
}

/// What charter generates inside a guest checkout, as `{relative path: text}`.
///
/// Two questions kept apart, exactly as the Python keeps them. The **generated** half asks
/// what the plane's own settings say and renders it as a document for this checkout. The
/// **mirrored** half asks which of the plane's own paths a git boundary cuts off and copies
/// what is on disk. Neither can be derived from the other: charter cannot generate a plane's
/// personas, and it must not mirror a file it generated.
///
/// Empty for a plane with nothing to say — which is the honest rendering, because writing an
/// empty `{}` would look like a layer.
pub fn want(plane: &Path) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    if let Some(text) = layer::settings_document(plane) {
        out.insert(SETTINGS.to_owned(), text);
    }
    if let Some(text) = layer::local_settings_document(plane) {
        out.insert(LOCAL_SETTINGS.to_owned(), text);
    }
    out.extend(mirrored(plane));
    out
}

/// The plane's own `.claude/agents` and `.claude/skills`, as text, keyed by their path
/// relative to the plane.
///
/// A 1:1 mirror of what is on disk, and not a second generator: `.claude/agents/` is
/// generated from `personas/` by somebody else's generator, so re-deriving it here would
/// drift from it. A file that is not readable text is skipped — charter cannot copy a binary,
/// and it must not take the whole layer down with it.
fn mirrored(plane: &Path) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for sub in WALKUP_DIRS {
        let dir = plane.join(sub);
        // The path ITSELF as well as everything under it: a surface spelled as one FILE at a
        // repository root would otherwise mirror as nothing at all.
        if let Some(text) = layer::readable_text(plane, &dir) {
            out.insert(sub.to_owned(), text);
            continue;
        }
        for found in walk(&dir) {
            let Ok(rel) = found.strip_prefix(&dir) else {
                continue;
            };
            let Some(rel) = rel.to_str() else {
                continue;
            };
            if let Some(text) = layer::readable_text(plane, &found) {
                out.insert(format!("{sub}/{rel}"), text);
            }
        }
    }
    out
}

/// Every entry under `dir` that is not itself a real directory, depth first.
///
/// **The walk never descends through a link**, which is what makes it terminate: a
/// `.claude/agents/loop -> ..` would otherwise walk for ever. A link to a FILE is handed back
/// as a candidate rather than dropped, because the Python mirrors one — `personas/` generators
/// legitimately link an agent into place — and [`layer::readable_text`] is where it is decided,
/// against the plane it must resolve inside. Dropping it here instead would be a persona's
/// agent silently missing from every worktree, with nothing saying so.
fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(here) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&here) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            match path.symlink_metadata() {
                Ok(meta) if meta.file_type().is_dir() => stack.push(path),
                // Everything else — a file, or a link of any kind. A link to a directory
                // simply fails to read as text, which is the same answer as a binary.
                Ok(_) => out.push(path),
                Err(_) => {}
            }
        }
    }
    out
}

/// Whether `rel` is under one of the plane's mirrored roots ([`WALKUP_DIRS`]), the root
/// itself included: a surface spelled as one file mirrors as that path.
fn mirrored_path(rel: &str) -> bool {
    WALKUP_DIRS.iter().any(|sub| {
        rel.strip_prefix(sub)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
    })
}

/// Whether `record` names any mirrored path at all — the cheap question that decides whether
/// a plane with nothing to carry still has something to withdraw from `tree`.
fn names_a_mirror(record: &layer::Record) -> bool {
    record.paths().any(|rel| mirrored_path(rel))
}

/// The text at `rel` in `tree`, read only where nothing on the way to it, the file included,
/// is a link, and only when it is a plain file. `None` for anything else.
fn own_text(tree: &Path, rel: &str) -> Option<String> {
    use std::io::Read;
    let path = tree.join(rel);
    let mut open = crate::contain::open_no_link(tree, &path).ok()?;
    if !open.metadata().ok()?.is_file() {
        return None;
    }
    let mut text = String::new();
    open.by_ref()
        .take(crate::reopen::MAX_BYTES)
        .read_to_string(&mut text)
        .ok()?;
    Some(text)
}

/// The file, in the project's `<state>/app/`, that lists every text the project has offered
/// at a path it may later refresh or withdraw ([`note_offered`]): the mirrored agents and
/// skills and the generated settings a checkout gets, and the generated settings a workspace
/// folder gets ([`crate::wslayer`]).
const OFFERED: &str = "mirrors-offered.json";

/// The most digests [`OFFERED`] keeps for one path, newest last: every edit of an agent is a
/// new text, and a copy older than this many edits is left for the operator.
const MOST_OFFERED: usize = 32;

/// Where [`OFFERED`] is. Under the folder the sandbox denies every chat a write to
/// (`<state>/app`, by each spelling of the state folder), and NOT the checkout's own record,
/// which sits in the chat's own tree: a record a chat can write cannot be what proves a file
/// is purlis's to delete.
pub(crate) fn offered_path(plane: &Path) -> PathBuf {
    names::state(plane).join("app").join(OFFERED)
}

/// The paths [`OFFERED`] notes: every one purlis may refresh or withdraw a copy at, in a
/// checkout or in a workspace folder. The machine-local settings are noted for the refresh
/// alone; nothing withdraws them, since the harness writes there too.
fn offerable(rel: &str) -> bool {
    mirrored_path(rel) || rel == SETTINGS || rel == LOCAL_SETTINGS
}

/// Whether the project's own note of what it offered (`offered`, [`read_offered`]) holds the
/// text with digest `have` at exactly `rel`.
///
/// The one test behind both verbs that act on a file a record names — a refresh that
/// overwrites it and a withdraw that removes it — and asked alongside the record, never
/// instead of it. The record sits in the tree, where a chat may be able to write it, so on its
/// own it proves nothing; [`OFFERED`] sits where no chat can write. The key match is exact, so
/// a key spelled another way is never vouched for (#1583).
pub(crate) fn vouched(offered: &BTreeMap<String, Vec<String>>, rel: &str, have: &str) -> bool {
    offered
        .get(rel)
        .is_some_and(|all| all.iter().any(|d| d == have))
}

/// What [`OFFERED`] holds: `{path: [digest, …]}` for the [`offerable`] paths only. Absent,
/// unreadable, behind a link, or not that shape is nothing offered, which withdraws nothing.
pub(crate) fn read_offered(plane: &Path) -> BTreeMap<String, Vec<String>> {
    crate::contain::read_text_no_link(plane, &offered_path(plane))
        .map(|text| parse_offered(&text))
        .unwrap_or_default()
}

fn parse_offered(text: &str) -> BTreeMap<String, Vec<String>> {
    let Ok(serde_json::Value::Object(doc)) = serde_json::from_str::<serde_json::Value>(text) else {
        return BTreeMap::new();
    };
    doc.into_iter()
        .filter(|(rel, _)| layer::key_ok(rel) && offerable(rel))
        .filter_map(|(rel, digests)| {
            let digests: Vec<String> = digests
                .as_array()?
                .iter()
                .filter_map(|d| d.as_str().map(str::to_owned))
                .collect();
            Some((rel, digests))
        })
        .collect()
}

/// Note each [`offerable`] text `want` offers in [`OFFERED`], so a copy of it can be refreshed
/// once the project moves on, or withdrawn once it stops having it (#1583). Noted before
/// anything is written, which is what makes every text purlis writes one it can later confirm
/// ([`vouched`]); never seeded from a tree's record, which a chat may have written.
///
/// Written only when something is new, and under the folder's lock. A write that fails (a
/// sandboxed caller, a read-only project) notes nothing, and what is not noted is never
/// overwritten or withdrawn: the safe direction.
pub(crate) fn note_offered(plane: &Path, want: &BTreeMap<String, String>) {
    let offers: Vec<(&String, String)> = want
        .iter()
        .filter(|(rel, _)| offerable(rel))
        .map(|(rel, text)| (rel, digest(text)))
        .collect();
    let known = read_offered(plane);
    if offers
        .iter()
        .all(|(rel, d)| known.get(rel.as_str()).is_some_and(|all| all.contains(d)))
    {
        return;
    }
    let path = offered_path(plane);
    let Some(dir) = path.parent() else {
        return;
    };
    if crate::rewrite::create_dir_all(dir).is_err() {
        return;
    }
    let _ = crate::rewrite::update(dir, &path, |now| {
        let mut all = now.map(parse_offered).unwrap_or_default();
        for (rel, d) in &offers {
            let digests = all.entry((*rel).clone()).or_default();
            if !digests.contains(d) {
                digests.push(d.clone());
            }
            let over = digests.len().saturating_sub(MOST_OFFERED);
            digests.drain(..over);
        }
        Ok(Some(format!(
            "{}\n",
            serde_json::to_string_pretty(&all).unwrap_or_else(|_| "{}".to_owned())
        )))
    });
}

/// The mirrored agents and skills purlis wrote in `tree` that the project no longer has
/// (#1583): what [`withdraw_mirrors`] takes out.
///
/// READ ONLY. A path qualifies only when every one of these holds, and each is a reason a
/// file could be somebody else's:
///
/// - **the record names it, under a mirrored root.** A path the record does not name is never
///   purlis's, and the generated settings are not withdrawn here: a checkout's settings stay
///   the workspace layer's question.
/// - **the project does not want it, and its own file is proved gone.** A plane file purlis
///   could not read (refused, a link out of the project, not text) mirrors as nothing, and is
///   a file somebody is holding, not one the project stopped having. Only "not there" and "a
///   component is not a directory" prove it gone ([`crate::worktree::listing::exists`]).
/// - **the copy is still exactly as written.** Its text is read with no link on the way, the
///   file itself included, and must match a digest the record holds. A copy edited since is
///   the operator's.
/// - **the project once offered that very text at that path** (`offered`, [`OFFERED`]). The
///   record lives in the checkout, where a chat can write it, so it alone proves nothing: a
///   chat that cannot touch `.claude/agents` could otherwise name the operator's own file in
///   it, with that file's digest, and have purlis delete it.
/// - **git does not track it** (`tracked`). A copy somebody committed is that repository's
///   content now, and removing it would change a tracked file.
fn retired_mirrors(
    plane: &Path,
    tree: &Path,
    want: &BTreeMap<String, String>,
    record: &layer::Record,
    offered: &BTreeMap<String, Vec<String>>,
    tracked: &dyn Fn(&str) -> bool,
) -> Vec<String> {
    record
        .paths()
        .filter(|rel| mirrored_path(rel) && !want.contains_key(rel.as_str()))
        .filter(|rel| offered.contains_key(rel.as_str()))
        .filter(|rel| crate::worktree::listing::exists(&plane.join(rel.as_str())) == Some(false))
        .filter(|rel| {
            own_text(tree, rel).is_some_and(|text| {
                let d = digest(&text);
                record.recorded(rel).contains(&d) && vouched(offered, rel, &d)
            })
        })
        .filter(|rel| !tracked(rel))
        .cloned()
        .collect()
}

/// Take out each of `rels` ([`retired_mirrors`]) from `tree`, forget it in `record`, and
/// say so — `wslayer`'s withdraw, for a checkout.
///
/// The link check is asked again at the unlink, not carried from the classification: this is
/// the destructive verb, and it must not act on a stale answer. A file that will not go stays
/// in the record, and the next wire tries again. A directory the removal leaves empty goes too,
/// up to and never including `tree`.
///
/// An unwanted mirrored entry whose file is already PROVED gone vouches for nothing and is
/// forgotten too, so its exclude line leaves with it.
fn withdraw_mirrors(
    plane: &Path,
    tree: &Path,
    want: &BTreeMap<String, String>,
    record: &mut layer::Record,
    rels: Vec<String>,
) -> Vec<Row> {
    let mut rows = Vec::new();
    for rel in rels {
        let path = tree.join(&rel);
        if crate::contain::no_link_on_the_way(tree, &path).is_err() {
            continue;
        }
        if std::fs::remove_file(&path).is_err() {
            continue;
        }
        record.forget(&rel);
        if let Some(parent) = path.parent() {
            layer::prune_empty(parent.to_path_buf(), tree);
        }
        rows.push(Row {
            rel,
            status: Status::Removed,
            why: String::new(),
        });
    }
    let gone: Vec<String> = record
        .paths()
        .filter(|rel| mirrored_path(rel) && !want.contains_key(rel.as_str()))
        .filter(|rel| crate::worktree::listing::exists(&plane.join(rel.as_str())) == Some(false))
        .filter(|rel| crate::worktree::listing::exists(&tree.join(rel.as_str())) == Some(false))
        .cloned()
        .collect();
    for rel in gone {
        record.forget(&rel);
    }
    rows
}

/// Charter's record in `tree`, narrowed to the entries that are SETTLED: `{}` for one that
/// is absent, unreadable, not an object, or holding a key charter could not have written.
///
/// Read through [`layer::read_record`], so a checkout and a workspace directory parse one
/// file the same way. What is narrowed here is only which entries this module ACTS on: a
/// pending entry lists what a path may hold while a write is under way, and this module never
/// publishes one, so an entry in that shape came from the Python's own writer being killed
/// mid-write. Acting on it as if it were settled would rewrite a file whose content charter
/// cannot yet vouch for; dropping that one entry leaves the path reading as somebody else's,
/// which is the direction this module is deliberately wrong in.
///
/// **A pure read, with no subprocess in it.** Whether the repository *commits* a
/// layer record — which would make it somebody's content rather than charter's record
/// — is [`tracked`]'s question, and it is a fact about the REPOSITORY rather than about each
/// tree. Asking it here would put a `git ls-files` behind every sidebar row, and this is
/// called once per piece on every render.
pub fn marker_at(tree: &Path) -> BTreeMap<String, String> {
    let record = layer::read_record(tree);
    record
        .paths()
        .filter_map(|rel| {
            record
                .settled(rel)
                .map(|hash| (rel.clone(), hash.to_owned()))
        })
        .collect()
}

/// Whether git TRACKS `rel` in the checkout at `tree`.
///
/// Charter's own generated files are per-checkout and untracked. A tracked one is content
/// some cloned repository committed and says nothing about this tree. A call that could not
/// run at all is not evidence either way, so only a clear "tracked" counts — the direction
/// that drops a record rather than trusting one.
pub fn tracked(tree: &Path, rel: &str) -> bool {
    matches!(
        crate::worktree::git::run(
            tree,
            &["ls-files", "--error-unmatch", "--", rel],
            crate::worktree::git::READ,
        ),
        Ok(run) if run.ok()
    )
}

/// Write the plane's layer into the guest checkout `tree`, and hide it there.
///
/// **The block first, then the files, then the block again** — charter's `wire_guest`. The
/// first pass names every path charter is about to own, before a byte of it is written; the
/// second settles the block on what the record says afterwards, because a checkout left with
/// nothing of charter's loses its block altogether.
///
/// # Where this is stricter than charter, and why it stays that way
///
/// **A block charter could not write at all means nothing is written here.** charter
/// withholds only the machine-local file in that case and writes the shared settings and the
/// mirrored agents anyway. Measured against the pinned oracle, that is not a theoretical
/// difference: `workspace reinit` over a checkout whose `.git/info` is mode `0o500` exits 0
/// and leaves
///
/// ```text
/// $ git -C workspaces/beta/svc status --porcelain
/// ?? .purlis-generated
/// ?? .claude/
/// ```
///
/// — charter's own files showing in somebody else's repository, which is the noise the block
/// exists to prevent. This writes nothing, reports the checkout blocked, and
/// [`crate::start::layered_or_refusal`] refuses the chat, so nothing runs without the plane's rules and nothing
/// is left showing.
///
/// Both are coherent, in opposite directions, and the trade — a usable worktree with noise
/// against a clean repository and a refusal — is an ADR's to make. **There is no such record
/// yet, so this is NOT declared as a `Divergence` in the differential**: that instrument
/// refuses a `why` that cites no ADR and no spec decision, and inventing one would be the
/// difference-with-no-author it exists to prevent. charter-app#124 is the issue to close it;
/// until then the behaviour is held by this crate's own tests alone, and nothing would notice
/// if charter's half changed.
///
/// **A line left out over a file of yours is NOT that case**, and since M2.26 it is ported
/// exactly (charter#1072). Nothing has failed there: a sibling checkout reading the same
/// exclude holds an untracked file of the operator's at a path charter wants, so the line
/// that would hide charter's would hide theirs. charter keeps writing its own shared file —
/// visible in this checkout's `git status`, with the plane's rules in force — and withholds
/// only the machine-local one, whose exposure would be one `git add` from a commit. Folding
/// that into the refusal above would take a worktree away over a file in a directory nobody
/// asked charter to look at, which is being stricter by accident.
///
/// `tree` is the caller's to have confined already —
/// [`crate::worktree::confine::within_workspace`] for a piece. What this function confines is
/// each path **below** it, at the moment it is opened, because a committed `.claude` that is
/// a directory symlink would otherwise send the write wherever the link points.
///
/// # A mirror is two-way (#1583)
///
/// A mirrored agent or skill the project no longer has is taken out of the checkout again, as
/// `wslayer`'s withdraw does for a workspace folder — also where the project has nothing left
/// to carry. Only what `retired_mirrors` proves is purlis's own copy goes.
pub fn wire(plane: &Path, tree: &Path) -> Wired {
    // One listing per repository for the length of this wire, even when the caller did not
    // open a block: a wire asks the exclude's question twice over by construction.
    let _answers = crate::worktree::listing::answers();
    let want = want(plane);
    // Before anything is written: a copy is withdrawn later only if this says it was offered.
    note_offered(plane, &want);
    if want.is_empty() && !names_a_mirror(&layer::read_record(tree)) {
        // Nothing to write, nothing to hide and nothing to withdraw. Not a blocked layer and
        // not an incomplete one: a plane with no settings and no agents has no layer to carry.
        // A record that still names a mirrored agent or skill goes on below, so the copy of a
        // project's last agent is withdrawn as any other is (#1583).
        return Wired {
            rows: Vec::new(),
            hidden: Hidden::InPlace,
            block: Block::Untouched,
        };
    }
    // A layer record this repository COMMITS, under either name, is the one case where
    // charter cannot keep its promise. Charter's record is per-checkout and untracked, so a tracked one is
    // content somebody committed — and charter publishing its own over it would show as a
    // modified TRACKED file, which no `info/exclude` line can hide. So nothing is written and
    // the tree is reported blocked, rather than charter dirtying a repo it is a guest in.
    if let Some(committed) = commits_a_marker(tree) {
        return Wired {
            rows: Vec::new(),
            hidden: Hidden::Blocked(
                format!(
                    "this repository commits a {committed}, and purlis's own is per-checkout \
                     and never committed — writing over it would change a tracked file"
                ),
                format!(
                    "Stop committing {committed} in that repository, or start this chat \
                     somewhere purlis is not a guest."
                ),
            ),
            block: Block::Untouched,
        };
    }
    let record = layer::read_record(tree);
    // What the project noted offering, read once: a stale copy is refreshed only when this
    // vouches for it as well as the record, and a retired one withdrawn on the same terms.
    let offered = read_offered(plane);
    let plan: Vec<(String, Plan)> = want
        .iter()
        .map(|(rel, text)| (rel.clone(), planned(tree, rel, text, &record, &offered)))
        .collect();

    // What the FIRST pass names: what is missing, what charter still recognises as its own,
    // and a co-written path its record already holds. A wanted path holding somebody's own
    // file is never named, not even for the length of one call — hiding their untracked work
    // from their own `git status` is the failure the ownership rule exists to prevent.
    let mut first_pass: BTreeSet<String> = plan
        .iter()
        .filter(|(rel, what)| {
            *what == Plan::Create || (COWRITTEN.contains(&rel.as_str()) && record.names(rel))
        })
        .map(|(rel, _)| rel.clone())
        .collect();
    // The marker is left out here only because it is pushed after, and the second pass below
    // registers `charter_owned` again, whole. So `!= MARKER` against `== MARKER` changes
    // which of the two passes first lists a file charter already owns — which is to say what
    // the exclude holds for the moment between them inside this one call — and nothing a
    // caller can see afterwards: the same final block, the same rows, and the same worst of
    // the two passes (measured case by case in the PR that excluded it; `.cargo/mutants.toml`).
    first_pass.extend(
        charter_owned(tree, &record)
            .into_iter()
            .filter(|rel| rel.as_str() != MARKER),
    );

    let listed = !first_pass.is_empty();
    let (first, left) = if first_pass.is_empty() {
        (Wrote::Present, BTreeSet::new())
    } else {
        let mut rels: Vec<String> = first_pass.into_iter().collect();
        rels.push(MARKER.to_owned());
        register_excludes(plane, tree, &rels, false)
    };
    if listed && !matches!(first, Wrote::Blocked(_)) {
        // The block now lists the purlis name, so a record charter wrote under the old one
        // moves to it here, bytes unchanged, and is hidden on arrival. A move that fails leaves
        // it readable where it is.
        let _ = layer::carry_over(tree);
    }
    if let Wrote::Blocked(why) = &first {
        // The divergence this function's docs argue for: charter would write everything but
        // the machine-local file here. Nothing is written, and the chat is refused.
        return Wired {
            rows: Vec::new(),
            hidden: Hidden::Blocked(
                why.clone(),
                "Restore write access to that checkout's info/exclude and try again.".to_owned(),
            ),
            block: Block::Untouched,
        };
    }
    // A machine-local file whose line was left out over a file of yours is withheld too
    // (charter#1072): the same rule as an exclude that cannot be written, one path at a time.
    // The shared file and the mirrored agents are still written, so the plane's committed
    // rules reach this checkout.
    let withhold: BTreeSet<&str> = COWRITTEN
        .iter()
        .copied()
        .filter(|rel| left.contains(*rel))
        .collect();

    let mut rows = Vec::new();
    let mut writes: Vec<(String, Plan)> = Vec::new();
    let mut marker = record.clone();
    for (rel, what) in plan {
        match what {
            // Never `foreign` (charter's review rounds 2 and 3): every `foreign` sentence
            // advises removing a file charter only failed to read, which may hold the
            // harness's own approvals.
            Plan::Unreadable => rows.push(Row {
                rel,
                status: Status::Unreadable,
                why: String::new(),
            }),
            Plan::Foreign => rows.push(row(rel)),
            // Left exactly as it is. Its exclude line is what the record already gave it
            // (`charter_owned`), as before.
            Plan::Unconfirmed => rows.push(Row {
                rel,
                status: Status::Unconfirmed,
                why: String::new(),
            }),
            Plan::Current | Plan::HarnessEdited => {
                // A record that does not say what a current file holds is SETTLED on it:
                // pending over a write that finished, or settled by a launch that lost a race
                // to another, whose record then named text the file no longer held — and the
                // plane's next move would have called charter's own file somebody else's.
                if marker.names(&rel)
                    && let Some(text) = want.get(&rel)
                {
                    marker.settle(&rel, digest(text));
                }
                rows.push(Row {
                    rel,
                    status: Status::Current,
                    why: String::new(),
                });
            }
            // Never rewritten and never merged into: the harness keeps that file now.
            Plan::Theirs => rows.push(Row {
                rel,
                status: Status::Theirs,
                why: String::new(),
            }),
            Plan::Create | Plan::Refresh => {
                if withhold.contains(rel.as_str()) {
                    rows.push(Row {
                        rel,
                        status: Status::Withheld,
                        why: String::new(),
                    });
                } else {
                    writes.push((rel, what));
                }
            }
        }
    }

    // **Intent first**, charter's ruling H. Every file about to be written is published as
    // PENDING — the digests it may hold while the write is under way — before a byte of it
    // changes, so no kill between the write and its record can leave a file on disk that the
    // record calls somebody else's. Where that publish fails, NOTHING is written: a file
    // charter cannot record is a file whose line the next launch would drop.
    let mut published = marker.clone();
    let mut wrote_nothing = false;
    if !writes.is_empty() {
        let mut intent = marker.clone();
        for (rel, _) in &writes {
            if let Some(text) = want.get(rel) {
                intent.pend(rel, digest(text));
            }
        }
        match layer::publish_io(tree, &intent) {
            Err(refused) => {
                let why = reason(&refused);
                note_unrecorded(plane, tree, Some(&refused));
                for (rel, _) in &writes {
                    rows.push(Row {
                        rel: rel.clone(),
                        status: Status::Unrecorded,
                        why: why.clone(),
                    });
                }
                wrote_nothing = true;
            }
            Ok(()) => {
                marker = intent;
                // Compared with the record THIS pass published, never the one it read: a pass
                // that wrote a deleted file again ended on the record it read and skipped the
                // settle, so the pending entry it had just published stayed pending for good.
                published = marker.clone();
            }
        }
    }
    if !wrote_nothing {
        for (rel, what) in writes {
            let Some(text) = want.get(&rel) else { continue };
            match write_into(tree, &rel, text) {
                Err(why) => {
                    // The entry stays pending and the file holds what it held — writes are
                    // whole — and the next wire writes it again.
                    rows.push(Row {
                        rel,
                        status: Status::Blocked,
                        why,
                    });
                }
                Ok(()) => {
                    marker.settle(&rel, digest(text));
                    rows.push(Row {
                        rel,
                        status: if what == Plan::Create {
                            Status::Created
                        } else {
                            Status::Refreshed
                        },
                        why: String::new(),
                    });
                }
            }
        }
        // What the project stopped mirroring goes after the writes and before the record is
        // published, so the publish below carries both, and the second pass takes the lines
        // of what went (#1583).
        let retired = retired_mirrors(plane, tree, &want, &marker, &offered, &|rel| {
            tracked(tree, rel)
        });
        rows.extend(withdraw_mirrors(plane, tree, &want, &mut marker, retired));
        if marker != published {
            // Only when something changed: rewriting the record on every launch would move a
            // checkout's mtimes for a call that changed nothing.
            let refused = layer::publish_io(tree, &marker).err();
            // Every entry on disk is still the intent or the record before it, and each of
            // those accounts for what is there now — so the lines stay, and the row says why.
            // The first publish that succeeds forgets the note.
            note_unrecorded(plane, tree, refused.as_ref());
            if let Some(refused) = refused {
                rows.push(Row {
                    rel: MARKER.to_owned(),
                    status: Status::Unrecorded,
                    why: reason(&refused),
                });
            }
        }
    }

    // The second pass settles the block on what the record says NOW: a withdrawal takes its
    // line with it, and a checkout left with nothing of charter's loses the block altogether
    // — which reading the record from before the writes would skip.
    let (second, owned) = settle_block(plane, tree);
    // One row for the two passes, worst first: blocked whichever pass hit it, then a line
    // left out over a file of yours, then the pass that actually wrote. Two rows would report
    // one file twice.
    let block = match worst(&first, &second) {
        Wrote::Blocked(_) => Block::Blocked,
        Wrote::Unhidden => Block::Unhidden,
        Wrote::Created => Block::Created,
        Wrote::Refreshed => Block::Refreshed,
        // charter appends the exclude row when it owns something here or when the block was
        // not already right, and a checkout where every wanted path holds somebody else's
        // file has neither.
        Wrote::Present if owned.is_empty() => Block::Untouched,
        Wrote::Present => Block::Present,
    };
    Wired {
        rows,
        hidden: Hidden::InPlace,
        block,
    }
}

/// Whether the repository at `tree` commits a layer record, and under which name. Charter's own is
/// per-checkout and untracked, so writing over a tracked one would change a tracked file,
/// which no `info/exclude` line can hide. [`wire`] reports the tree blocked over it, and
/// [`wire_for_chat`] writes no guidance.
///
/// Under any name the record has had: one committed as `.charter-generated` would otherwise be
/// read as charter's record, or removed as a leftover — a tracked file either way.
fn commits_a_marker(tree: &Path) -> Option<&'static str> {
    names::GENERATED_SIDECAR
        .spellings()
        .find(|name| tree.join(name).exists() && tracked(tree, name))
}

/// The block's settling pass: charter's block in `tree`'s exclude, written again from what
/// `tree`'s record says now. `(what it wrote, what the record owns)`.
///
/// After the files, never before: a withdrawal takes its line with it, a line added for a
/// file that was then not written leaves again (its path is confirmed absent), and a checkout
/// left with nothing of charter's loses the block altogether.
fn settle_block(plane: &Path, tree: &Path) -> (Wrote, Vec<String>) {
    let owned = charter_owned(tree, &layer::read_record(tree));
    let (wrote, _) = register_excludes(plane, tree, &owned, false);
    (wrote, owned)
}

/// The one project-instructions file charter writes: a chat's guidance, in its own worktree
/// alone (ADR 0085).
pub const AGENTS_MD: &str = "AGENTS.md";

/// What became of a chat's `AGENTS.md` ([`wire_for_chat`]). None of these refuses a chat: the
/// file is guidance, and a chat without it still has its hook briefing (ADR 0085 §4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Guidance {
    /// Charter wrote it, where there was none or where its own was.
    Written,
    /// Charter's own was there, already saying this.
    Current,
    /// The repository tracks an `AGENTS.md`: charter writes nothing.
    Tracked,
    /// An `AGENTS.md` charter did not write is there (or one it cannot read): the operator's,
    /// left exactly as it is.
    Theirs,
    /// Not written, because it could not be hidden: the exclude cannot be written, or its line
    /// would hide an untracked file of the operator's in a checkout that reads the same
    /// exclude (charter#1072). A generated file that shows is one `git add` from a commit.
    Withheld(String),
    /// Not written: the record or the file could not be written. No line is left behind.
    Blocked(String),
}

impl Guidance {
    /// The one sentence the window shows for this at the chat's start, or none for an outcome
    /// that is nothing to report. The reason is the operator's to act on, so it is never
    /// dropped (a withheld or blocked file is otherwise invisible).
    pub fn notice(&self) -> Option<String> {
        match self {
            Self::Written | Self::Current | Self::Tracked | Self::Theirs => None,
            Self::Withheld(why) | Self::Blocked(why) => Some(format!(
                "purlis did not write this chat's {AGENTS_MD}: {why} (ADR 0085)."
            )),
        }
    }
}

/// [`wire`], and then, for a chat starting in its own worktree, its `AGENTS.md` (ADR 0085).
///
/// **The layer first, and the two never mixed.** The layer is a guard, and [`Wired`] says
/// whether a chat may start; the guidance is not, and nothing that happens to it changes that
/// answer. The guidance is written **only where the layer is complete**: a start that is
/// refused writes nothing a chat would have read.
///
/// **Only into a piece.** `tree` is asked here ([`crate::worktree::locate`]), not trusted: a
/// shared clone, where two personas would overwrite each other's file, or any tree that is not
/// a piece of this plane, gets `None` and nothing written. `guidance` is
/// `briefing::agents_md`'s text, or `None` for a chat with nothing to be told.
pub fn wire_for_chat(
    plane: &Path,
    tree: &Path,
    guidance: Option<&str>,
) -> (Wired, Option<Guidance>) {
    let wired = wire(plane, tree);
    let a_piece = crate::worktree::locate(plane, tree).is_some_and(|found| {
        crate::worktree::path_for(plane, &found.workspace, &found.repo, &found.piece)
            .ok()
            .and_then(|path| crate::contain::resolved(&path))
            == crate::contain::resolved(tree)
    });
    let guided = match guidance {
        Some(text) if a_piece && wired.complete() => Some(guide(plane, tree, text)),
        _ => None,
    };
    (wired, guided)
}

/// Write `text` as `tree`'s `AGENTS.md`, under the guest layer's ownership rule, or step aside.
///
/// Held under [`Held`], so two chats starting in one piece at once take turns: interleaved,
/// one could settle its record over the other's file, and the file would read as somebody
/// else's for good.
fn guide(plane: &Path, tree: &Path, text: &str) -> Guidance {
    let _held = Held::on(tree);
    let _answers = crate::worktree::listing::answers();
    if tracked(tree, AGENTS_MD) {
        return Guidance::Tracked;
    }
    if let Some(committed) = commits_a_marker(tree) {
        return Guidance::Withheld(format!(
            "this repository commits a {committed}, and purlis's record is never committed"
        ));
    }
    let record = layer::read_record(tree);
    // No offers: `planned` goes by the record alone for this one file.
    let plan = planned(tree, AGENTS_MD, text, &record, &BTreeMap::new());
    match plan {
        Plan::Foreign
        | Plan::Unreadable
        | Plan::Theirs
        | Plan::HarnessEdited
        | Plan::Unconfirmed => {
            return Guidance::Theirs;
        }
        Plan::Current | Plan::Create | Plan::Refresh => {}
    }
    // The line first, before a byte of the file exists, and with it every line this tree
    // already needs: the block is written whole.
    let mut rels = charter_owned(tree, &record);
    rels.push(AGENTS_MD.to_owned());
    if !rels.iter().any(|r| r == MARKER) {
        rels.push(MARKER.to_owned());
    }
    let (first, left) = register_excludes(plane, tree, &rels, false);
    if !matches!(first, Wrote::Blocked(_)) {
        // As in `wire`: hidden first, then moved.
        let _ = layer::carry_over(tree);
    }
    if let Wrote::Blocked(why) = first {
        return Guidance::Withheld(format!(
            "its line could not be written to the exclude ({why})"
        ));
    }
    if left.contains(AGENTS_MD) {
        return Guidance::Withheld(left_out_why(tree));
    }
    if plan == Plan::Current {
        return Guidance::Current;
    }
    let outcome = write_guidance(tree, text, &record);
    // Always, whatever the write did: a line added above for a file that was then not written
    // leaves again here, because its path is confirmed absent.
    settle_block(plane, tree);
    outcome
}

/// Intent, file, settled record — [`wire`]'s order, for the one file.
fn write_guidance(tree: &Path, text: &str, record: &layer::Record) -> Guidance {
    let mut intent = record.clone();
    intent.pend(AGENTS_MD, digest(text));
    if let Err(refused) = layer::publish_io(tree, &intent) {
        return Guidance::Blocked(format!(
            "its record could not be written ({})",
            reason(&refused)
        ));
    }
    if let Err(why) = write_into(tree, AGENTS_MD, text) {
        // The entry stays pending over a file that holds what it held, and the next start
        // writes it again.
        return Guidance::Blocked(format!("the file could not be written ({why})"));
    }
    let mut settled = intent;
    settled.settle(AGENTS_MD, digest(text));
    match layer::publish_io(tree, &settled) {
        Err(refused) => Guidance::Blocked(format!(
            "its record could not be settled ({})",
            reason(&refused)
        )),
        Ok(()) => Guidance::Written,
    }
}

/// Why the line for `tree`'s `AGENTS.md` was left out: the untracked file of the operator's,
/// in a checkout that reads the same exclude, that it would have hidden (charter#1072).
fn left_out_why(tree: &Path) -> String {
    let theirs = exclude_file(tree)
        .and_then(|exclude| crate::worktree::listing::live_trees(tree, &exclude).0)
        .unwrap_or_default()
        .into_iter()
        .filter(|t| crate::contain::resolved(t) != crate::contain::resolved(tree))
        .find(|t| yours_untracked(t, AGENTS_MD))
        .map(|t| t.join(AGENTS_MD).display().to_string());
    match theirs {
        Some(theirs) => format!(
            "{theirs} is an untracked file purlis did not write, and the line that would hide \
             purlis's would hide it too — commit or move it, and the next chat here gets one"
        ),
        None => "the line that would hide it would hide an untracked file of yours in another \
                 checkout of this repository"
            .to_owned(),
    }
}

/// An exclusive `flock` on `tree`'s own git directory for as long as it is held: one chat's
/// [`guide`] at a time in one piece.
///
/// **On the git directory, and no file of its own**: for a piece that is
/// `.git/worktrees/<id>/`, which is that piece's alone and outside its working tree, so the
/// lock is no store and nothing shows in `git status`. **Best effort**, as `machine.rs`'s is:
/// a lock that cannot be taken costs only the protection it adds.
struct Held(Option<std::fs::File>);

impl Held {
    fn on(tree: &Path) -> Self {
        let Some(dir) = git_dir(tree) else {
            return Self(None);
        };
        let Ok(file) = std::fs::File::open(&dir) else {
            return Self(None);
        };
        #[cfg(unix)]
        match rustix::fs::flock(&file, rustix::fs::FlockOperation::LockExclusive) {
            Ok(()) => Self(Some(file)),
            Err(_) => Self(None),
        }
        #[cfg(not(unix))]
        {
            drop(file);
            Self(None)
        }
    }
}

impl Drop for Held {
    fn drop(&mut self) {
        if let Some(file) = self.0.take() {
            #[cfg(unix)]
            let _ = rustix::fs::flock(&file, rustix::fs::FlockOperation::Unlock);
            drop(file);
        }
    }
}

/// What V35's check found: each `AGENTS.md` charter's line hides and charter did not write,
/// and each place it could not look. Doubt is never silence: a check that could not finish
/// says so, and a caller reports it as a warning ([`Self::said`]).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HiddenAgentsMd {
    /// The hidden files, by path, sorted.
    pub found: Vec<PathBuf>,
    /// Why some checkout or file could not be checked, each naming what clears it.
    pub unsure: Vec<String>,
}

impl HiddenAgentsMd {
    /// Nothing found and nothing in doubt.
    pub fn is_empty(&self) -> bool {
        self.found.is_empty() && self.unsure.is_empty()
    }

    /// Add another check's answer to this one.
    pub fn extend(&mut self, other: HiddenAgentsMd) {
        self.found.extend(other.found);
        self.unsure.extend(other.unsure);
        self.found.sort();
        self.found.dedup();
        self.unsure.dedup();
    }

    /// **The one wording** of V35, for `charter doctor`, the window's notice at a chat's
    /// start and the chat's briefing alike, with `shown` spelling each path for its surface.
    /// `None` when there is nothing to say.
    pub fn said(&self, shown: impl Fn(&Path) -> String) -> Option<String> {
        if self.is_empty() {
            return None;
        }
        let mut parts = Vec::new();
        if !self.found.is_empty() {
            let named = self
                .found
                .iter()
                .map(|p| shown(p))
                .collect::<Vec<_>>()
                .join(", ");
            parts.push(format!(
                "{named}: not written by purlis, and hidden from git status by the /{AGENTS_MD} \
                 line purlis keeps in that repository's info/exclude for a chat's own worktree. \
                 While it is hidden it can go uncommitted unseen, and a checkout that brings in a \
                 tracked {AGENTS_MD} replaces it — commit it or move it aside (ADR 0085)."
            ));
        }
        if !self.unsure.is_empty() {
            parts.push(format!(
                "purlis could not check every {AGENTS_MD} its line may hide: {}.",
                self.unsure.join("; ")
            ));
        }
        Some(parts.join(" "))
    }
}

/// Every `AGENTS.md` that charter's line in the exclude `tree` reads hides, and that charter
/// did not write: in `tree` and in every checkout that reads the same exclude (V35).
///
/// Git has no per-worktree exclude (ADR 0085 §5), so the line that hides a piece's
/// `AGENTS.md` hides one at the root of the clone and of every sibling piece too. A file the
/// operator makes there afterwards is hidden from their own `git status`, and a checkout that
/// brings in a tracked one replaces it without a word. Empty where charter's block has no
/// such line. READ ONLY.
pub fn hidden_agents_md(tree: &Path) -> HiddenAgentsMd {
    let _answers = crate::worktree::listing::answers();
    let mut out = HiddenAgentsMd::default();
    let Some(exclude) = exclude_file(tree) else {
        return out;
    };
    let text = std::fs::read_to_string(&exclude).unwrap_or_default();
    if !already(&text).contains(AGENTS_MD) {
        return out;
    }
    let (trees, doubt) = crate::worktree::listing::live_trees(tree, &exclude);
    let trees = trees.unwrap_or_else(|| {
        out.unsure.push(doubt);
        vec![tree.to_path_buf()]
    });
    hidden_in(trees, &mut out);
    out.found.sort();
    out.found.dedup();
    out
}

/// What **Move aside…** renames the operator's `AGENTS.md` to, before a number is needed:
/// a name charter's `/AGENTS.md` exclude line does not match, so the file shows in `git status`
/// again (NO-4).
const ASIDE: &str = "AGENTS.aside";

/// The operator's `AGENTS.md` at the top of `branch` (V35): there, a plain file with one name,
/// untracked, not one charter's record vouches for, and **hidden by charter's own exclude line
/// and by no other rule** — one [`hidden_agents_md`] names, whose ignore git traces to that line
/// ([`hidden_by_charters_line`]). Refused, in a sentence, otherwise: the core decides which
/// files these actions reach, never the window. The branch is placed by name
/// ([`crate::files::place`]): no link, nothing outside the branch, never git's own.
fn their_agents_md(plane: &Path, branch: crate::files::Branch<'_>) -> Result<PathBuf, String> {
    let file = crate::files::place(plane, branch, AGENTS_MD)
        .map_err(|refused| refused.to_string())?
        .absolute;
    let shown = file.display();
    if !file.is_file() {
        return Err(format!("{shown} is not a file"));
    }
    // One name only: a second name would let this file carry another file's contents.
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        let names = std::fs::symlink_metadata(&file)
            .map_err(|e| format!("purlis cannot read {shown}: {e}"))?
            .nlink();
        if names > 1 {
            return Err(format!(
                "{shown} has more than one name on disk, so purlis does not hand it on"
            ));
        }
    }
    let tree = file.parent().unwrap_or(plane);
    if tracked(tree, AGENTS_MD) {
        return Err(format!(
            "the repository tracks {shown}: it is the repository's, and changing it is a commit"
        ));
    }
    let on_disk =
        std::fs::read_to_string(&file).map_err(|e| format!("purlis cannot read {shown}: {e}"))?;
    if layer::read_record(tree)
        .recorded(AGENTS_MD)
        .contains(&digest(&on_disk))
    {
        return Err(format!(
            "purlis wrote {shown}, and writes it again at a chat's next start there: it is not \
             yours to move aside"
        ));
    }
    let at = crate::contain::resolved(&file);
    let named = at.is_some()
        && hidden_agents_md(tree)
            .found
            .iter()
            .any(|found| crate::contain::resolved(found) == at);
    if !named || !hidden_by_charters_line(tree) {
        return Err(format!(
            "{shown} is not hidden by purlis's own exclude line alone, so it is yours to open \
             or move as you would any file"
        ));
    }
    Ok(file)
}

/// Whether the ignore rule git applies to `tree`'s `AGENTS.md` is charter's own `/AGENTS.md`
/// line, inside charter's block of the exclude file `tree` reads. Asked of git itself
/// (`check-ignore -v`), which names the rule that wins: where another rule also matches and
/// takes precedence, it is that rule's, and this is `false`. A git that cannot answer is `false`.
fn hidden_by_charters_line(tree: &Path) -> bool {
    let Some(exclude) = exclude_file(tree) else {
        return false;
    };
    let Ok(run) = crate::worktree::git::run(
        tree,
        &["check-ignore", "-v", "--no-index", "--", AGENTS_MD],
        crate::worktree::git::READ,
    ) else {
        return false;
    };
    if !run.ok() {
        return false;
    }
    // `<source>:<line>:<pattern>\t<path>`. The source can hold a colon, so it is split from
    // the right: the pattern is `/AGENTS.md`, and the line number sits just before it.
    let Some((rule, _path)) = run.line().split_once('\t') else {
        return false;
    };
    let Some((rest, pattern)) = rule.rsplit_once(':') else {
        return false;
    };
    let Some((source, number)) = rest.rsplit_once(':') else {
        return false;
    };
    let Ok(number) = number.parse::<usize>() else {
        return false;
    };
    let source = Path::new(source);
    let source = if source.is_absolute() {
        source.to_path_buf()
    } else {
        tree.join(source)
    };
    let exclude_at = crate::contain::resolved(&exclude);
    if pattern != format!("/{AGENTS_MD}")
        || exclude_at.is_none()
        || crate::contain::resolved(&source) != exclude_at
    {
        return false;
    }
    let text = std::fs::read_to_string(&exclude).unwrap_or_default();
    let lines: Vec<&str> = text.lines().collect();
    // git counts from 1; the block's lines are `begin + 1 .. after` counted from 0.
    span(&lines).is_some_and(|(begin, after)| number > begin + 1 && number <= after)
}

/// **Open file** for the operator's `AGENTS.md` at the top of `branch` (NO-4): what to launch to
/// show it in their editor.
///
/// Charter's own exclude line hides this file, so the light editor's rule — only what git does
/// not ignore opens ([`crate::files::in_your_editor`]) — would refuse it. That rule keeps an
/// ignored secret out of a review; this file is hidden by charter, not by the operator, and
/// only it opens here, only when it is theirs.
pub fn their_agents_md_in_your_editor(
    plane: &Path,
    branch: crate::files::Branch<'_>,
    editor: crate::youreditor::Editor,
    var: &dyn Fn(&str) -> Option<String>,
) -> Result<crate::youreditor::Launch, String> {
    let file = their_agents_md(plane, branch)?;
    crate::youreditor::launch(editor, &file, 1, var).map_err(|refused| refused.to_string())
}

/// **Move aside…** (NO-4): the operator's `AGENTS.md` at the top of `branch` renamed to
/// `AGENTS.aside.md`, or `AGENTS.aside-2.md` and on when that is taken, **never over anything**.
/// Answers the name it now has. Only on the operator's press, after the window asked: charter
/// never moves a file of theirs by itself (V91c). Refused, touching nothing, for a file that
/// is not theirs ([`their_agents_md`]).
pub fn move_agents_md_aside(
    plane: &Path,
    branch: crate::files::Branch<'_>,
) -> Result<String, String> {
    let file = their_agents_md(plane, branch)?;
    let tree = file.parent().unwrap_or(plane);
    for n in 1..=99 {
        let name = if n == 1 {
            format!("{ASIDE}.md")
        } else {
            format!("{ASIDE}-{n}.md")
        };
        match rename_new(&file, &tree.join(&name)) {
            Ok(true) => return Ok(name),
            Ok(false) => continue,
            Err(e) => {
                return Err(format!(
                    "purlis could not move {} aside: {e}",
                    file.display()
                ));
            }
        }
    }
    Err(format!(
        "every name from {ASIDE}.md to {ASIDE}-99.md is taken beside {}, so nothing was moved",
        file.display()
    ))
}

/// `from` renamed to `to`, never over anything: `Ok(false)`, moving nothing, when something is
/// at `to`. The kernel's no-replace rename where there is one, as `held.rs`'s is; a filesystem
/// without it is renamed onto after a look.
pub(crate) fn rename_new(from: &Path, to: &Path) -> std::io::Result<bool> {
    #[cfg(any(
        target_os = "linux",
        target_os = "android",
        target_os = "macos",
        target_os = "ios"
    ))]
    match rustix::fs::renameat_with(
        rustix::fs::CWD,
        from,
        rustix::fs::CWD,
        to,
        rustix::fs::RenameFlags::NOREPLACE,
    ) {
        Ok(()) => return Ok(true),
        Err(rustix::io::Errno::EXIST) => return Ok(false),
        Err(rustix::io::Errno::INVAL | rustix::io::Errno::NOSYS | rustix::io::Errno::NOTSUP) => {}
        Err(e) => return Err(e.into()),
    }
    if std::fs::symlink_metadata(to).is_ok() {
        return Ok(false);
    }
    std::fs::rename(from, to).map(|()| true)
}

/// [`hidden_agents_md`]'s question, asked of each of `trees`: whether its `AGENTS.md` is one
/// charter's line hides and charter did not write, or one it cannot tell about.
fn hidden_in(trees: Vec<PathBuf>, out: &mut HiddenAgentsMd) {
    for t in trees {
        let path = t.join(AGENTS_MD);
        match crate::worktree::listing::exists(&path) {
            Some(false) => continue,
            None => {
                out.unsure.push(format!(
                    "{} cannot be checked — restoring read access clears this",
                    path.display()
                ));
                continue;
            }
            Some(true) => {}
        }
        if tracked(&t, AGENTS_MD) {
            continue;
        }
        let record = layer::read_record(&t);
        match std::fs::read_to_string(&path) {
            Ok(on_disk) if record.recorded(AGENTS_MD).contains(&digest(&on_disk)) => {}
            Ok(_) => out.found.push(path),
            // Charter's ownership rule keeps the line for a file it cannot read; that is not
            // evidence the file is charter's, so it is a doubt here.
            Err(_) => out.unsure.push(format!(
                "{} cannot be read, so purlis cannot tell whether it wrote it — restoring read \
                 access clears this",
                path.display()
            )),
        }
    }
}

/// A row for `rel`, starting at the state no write changes: somebody else's file.
///
/// Foreign is the default on purpose. A `Row` built as `Current` and then left unset by a
/// branch that forgot to write would say the plane's rules are in force where they are not,
/// and that is the one direction this module must not be wrong in.
fn row(rel: String) -> Row {
    Row {
        rel,
        status: Status::Foreign,
        why: String::new(),
    }
}

/// What charter will do with one wanted path — charter's `_layer_status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Plan {
    /// Not there at all: charter's to write. charter's `missing`.
    Create,
    /// There with the content charter's record names, and the plane has moved on: charter's
    /// own file to bring up to date. charter's `stale`.
    Refresh,
    Current,
    /// A path the harness writes into too, whose content matches neither what the plane wants
    /// nor a digest charter recorded — but charter's last write IS still what the plane wants,
    /// so the harness only added to it and every rule charter mirrored is in it.
    /// charter's `harness-edited`, which is CURRENT: writing charter's text over it would
    /// throw away the approvals the harness saved there.
    HarnessEdited,
    /// The same path once the plane has moved on since, or where charter never wrote it.
    /// charter's `harness-behind`: charter will not merge into a file the harness keeps, so
    /// the plane's newer rules are not in it.
    Theirs,
    /// There, and charter could not read it.
    Unreadable,
    /// Somebody else's file.
    Foreign,
    /// What [`Plan::Refresh`] would be on the record's word alone, where the project's own
    /// note of what it offered does not vouch for the text there ([`vouched`]): left as is.
    Unconfirmed,
}

/// `offered` is the project's note of what it offered ([`read_offered`]). A file the record
/// vouches for is [`Plan::Refresh`] only when this vouches for the same text at the same path;
/// otherwise it is [`Plan::Unconfirmed`] and stays as it is.
///
/// **Except [`AGENTS_MD`]**, a chat's own guidance, which goes by the record alone: a chat can
/// write that file in its own worktree itself, so a record naming it gives a chat nothing it
/// does not already have, and its per-chat texts are not ones the project notes.
fn planned(
    tree: &Path,
    rel: &str,
    text: &str,
    record: &layer::Record,
    offered: &BTreeMap<String, Vec<String>>,
) -> Plan {
    let path = tree.join(rel);
    // `listing::exists`, never a bare `symlink_metadata().is_err()`: that reads EACCES as
    // "not there", and charter would then WRITE over a path it was not allowed to look at.
    // Only ENOENT and ENOTDIR prove a path gone; a dangling link is THERE, and reading it as
    // absent would write through it.
    if crate::worktree::listing::exists(&path) == Some(false) {
        return Plan::Create;
    }
    if !layer::inside(tree, &path) {
        // A committed link whose target, or whose parent, leaves the checkout. Charter
        // neither reads through it — the digest on the far end is not evidence the file is
        // charter's — nor writes through it. `foreign` is the state left exactly as it is.
        return Plan::Foreign;
    }
    let Ok(on_disk) = std::fs::read_to_string(&path) else {
        return Plan::Unreadable;
    };
    if on_disk == text {
        return Plan::Current;
    }
    // Only content a record LISTS is charter's to overwrite, pending or settled — and only
    // when the project noted offering that very text here too, because the record alone sits
    // where a chat can write it (#1583).
    let have = digest(&on_disk);
    if record.recorded(rel).contains(&have) {
        return if rel == AGENTS_MD || vouched(offered, rel, &have) {
            Plan::Refresh
        } else {
            Plan::Unconfirmed
        };
    }
    if COWRITTEN.contains(&rel) {
        // `harness-edited` only over a SETTLED record of exactly what the plane wants now. A
        // pending entry lists what the file may hold while a write is under way, the new text
        // among it: read as current, an approval the harness saved over an interrupted write
        // would settle a file that never received the plane's new `deny`.
        return if record.settled(rel) == Some(digest(text).as_str()) {
            Plan::HarnessEdited
        } else {
            Plan::Theirs
        };
    }
    Plan::Foreign
}

/// The real git directory of the checkout at `root`, or `None` when there is none.
///
/// `<root>/.git` is a DIRECTORY in a clone and a FILE reading `gitdir: <path>` in a linked
/// worktree. Treating the second as a directory does not fail loudly — `create_dir_all` would
/// happily make `.git/info/` beside the `.git` file's parent, and git reads none of it.
///
/// **Public because it is what makes a directory a CHECKOUT**, and charter asks exactly this
/// of every workspace child (`_children`): a `.git` file whose `gitdir:` charter cannot reach
/// is not a checkout, so it is passed over rather than reported as one charter failed to
/// wire. Measured on the differential: a linked worktree whose admin directory is unreadable
/// was reported `blocked` here and passed over by charter, one row apart.
pub fn git_dir(root: &Path) -> Option<PathBuf> {
    let dot = root.join(".git");
    if dot.is_dir() {
        return Some(dot);
    }
    let text = std::fs::read_to_string(&dot).ok()?;
    let line = text.lines().find_map(|l| l.strip_prefix("gitdir:"))?;
    let named = PathBuf::from(line.trim());
    let dir = if named.is_absolute() {
        named
    } else {
        root.join(named)
    };
    dir.is_dir().then_some(dir)
}

/// The `info/exclude` git actually READS for the checkout at `root`.
///
/// The COMMON directory's, which for a linked worktree is not its own gitdir. Git treats
/// `info/` as shared, so a pattern written to `.git/worktrees/<id>/info/exclude` is read by
/// nobody: measured on git 2.x, the file stays listed as untracked there while the identical
/// pattern in the main repository's `.git/info/exclude` hides it. `commondir` is the pointer
/// git itself leaves for this, holding a path relative to the worktree's gitdir.
pub fn exclude_file(root: &Path) -> Option<PathBuf> {
    let mut dir = git_dir(root)?;
    if let Ok(common) = std::fs::read_to_string(dir.join("commondir")) {
        let common = common.trim();
        if !common.is_empty() {
            // Joining answers both spellings: an absolute `common` replaces the base.
            dir = normalise(&dir.join(common));
        }
    }
    Some(dir.join("info").join("exclude"))
}

/// A path with its `.` and `..` folded lexically. Not `canonicalize`: `commondir` is git's own
/// `../..`, and resolving would also follow symlinks charter was not asked to follow.
fn normalise(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Which of guest checkout `tree`'s files charter left out of its `info/exclude` block, whose
/// file of yours stopped each, and what clears it — READ ONLY, empty when nothing was
/// (charter#1072).
///
/// [`unaccounted`]'s other half. That one names a line kept without proof; this names a line
/// charter would not ADD, because a clone and its worktrees share the file and the line would
/// hide an untracked file of yours in one of the others. Charter's own shared file stays
/// written and shows in `tree`'s `git status` — the plane's ask/deny rules stay in force
/// there — and `reinit` says so through this.
///
/// The machine-local files charter would write into `tree` and has not are asked about too,
/// so a file [`wire`] withholds is never called `missing` anywhere, which reads as "`reinit`
/// writes it" — the one thing `reinit` will not do while your file is there.
pub fn unhidden(plane: &Path, tree: &Path) -> Vec<String> {
    let _answers = crate::worktree::listing::answers();
    let Some(path) = exclude_file(tree) else {
        return Vec::new();
    };
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let record = layer::read_record(tree);
    let want = want(plane);
    let mut rels = charter_owned(tree, &record);
    for (rel, body) in &want {
        // Only `Create` is asked, which no offer changes.
        if COWRITTEN.contains(&rel.as_str())
            && planned(tree, rel, body, &record, &BTreeMap::new()) == Plan::Create
        {
            rels.push(rel.clone());
        }
    }
    shared_rels(plane, tree, &rels, &text, &path, false)
        .shown
        .into_values()
        .collect()
}

/// Why charter's block in guest checkout `tree`'s `info/exclude` keeps a line it cannot prove
/// is still needed — READ ONLY, and empty when every line is accounted for.
///
/// charter's ruling G, second half: keep every line, and let the report say what could not be
/// accounted for. A block kept silently is a block nobody can tell from one that is right.
pub fn unaccounted(plane: &Path, tree: &Path) -> Vec<String> {
    let _answers = crate::worktree::listing::answers();
    let Some(path) = exclude_file(tree) else {
        return Vec::new();
    };
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let rels = charter_owned(tree, &layer::read_record(tree));
    shared_rels(plane, tree, &rels, &text, &path, false).unaccounted
}

// --------------------------------------------------------------------------------------- //
// a record charter could not publish                                                        //
// --------------------------------------------------------------------------------------- //

/// Where the reason `tree`'s record could not be published is kept — charter's
/// `_unrecorded_note`.
///
/// In charter's own state directory and not beside the checkout: the checkout is the place
/// that refused a write, so it is the last place a note about that refusal can go.
fn unrecorded_note(plane: &Path, tree: &Path) -> PathBuf {
    let real = crate::contain::resolved(tree).unwrap_or_else(|| tree.to_path_buf());
    let key: String = digest(&real.display().to_string())
        .chars()
        .take(32)
        .collect();
    crate::plane::state_dir(plane)
        .join("unrecorded")
        .join(format!("{key}.json"))
}

/// Keep the errno a publish into `tree` failed with, or forget it once one succeeded.
///
/// charter took this reason from `os.access`, which passes a read-only filesystem, a full
/// disk and a quota alike — each a publish that fails in a directory whose mode bits allow
/// it. Best effort: a note that cannot be kept costs the reason, never the launch.
fn note_unrecorded(plane: &Path, tree: &Path, refused: Option<&std::io::Error>) {
    let note = unrecorded_note(plane, tree);
    let Some(refused) = refused else {
        let _ = std::fs::remove_file(&note);
        return;
    };
    let Some(parent) = note.parent() else { return };
    if std::fs::create_dir_all(parent).is_err() {
        return;
    }
    let doc = serde_json::json!({
        "errno": errno_name(refused),
        "says": strerror(refused),
    });
    // `json.dumps`' own defaults, which is what charter writes: one line, `", "` and `": "`.
    let _ = layer::write_whole(
        &note,
        &(crate::pyjson::dumps(&doc, None, ", ", ": ") + "\n"),
    );
}

/// `"EACCES: Permission denied"` for one refusal — what the note holds, joined.
fn reason(refused: &std::io::Error) -> String {
    format!("{}: {}", errno_name(refused), strerror(refused))
}

/// Why charter could not publish guest checkout `tree`'s record — `"EACCES: Permission
/// denied"` — or `""` when no failed publish is on record. READ ONLY.
pub fn unrecorded_reason(plane: &Path, tree: &Path) -> String {
    let Ok(text) = std::fs::read_to_string(unrecorded_note(plane, tree)) else {
        return String::new();
    };
    let Ok(doc) = serde_json::from_str::<serde_json::Value>(&text) else {
        return String::new();
    };
    let (Some(code), Some(says)) = (doc.get("errno"), doc.get("says")) else {
        return String::new();
    };
    format!(
        "{}: {}",
        crate::forge::py_str(code),
        crate::forge::py_str(says)
    )
}

/// What clears a record publish that failed, by the errno it failed with.
///
/// "Restore write access" was charter's advice for every one of them, and a full disk or a
/// read-only mount has no write access to restore. An errno not named here gets no guessed
/// cause.
const UNRECORDED_FIXES: [(&str, &str); 3] = [
    ("EACCES", "restore write access to {where}"),
    ("ENOSPC", "free space on the disk that holds {where}"),
    (
        "EROFS",
        "remount the read-only filesystem that holds {where} read-write",
    ),
];

/// What clears guest checkout `tree`'s failed record publish, worded for `where_` and ending
/// with its errno — `""` when no failed publish is on record. READ ONLY.
///
/// One wording for the chat, `doctor` and `reinit`, so no two of them can hand a reader
/// different remedies for one refusal.
pub fn unrecorded_fix(plane: &Path, tree: &Path, where_: &str) -> String {
    let reason = unrecorded_reason(plane, tree);
    if reason.is_empty() {
        return String::new();
    }
    // By PREFIX, never by splitting the reason: its first ":" always follows the errno's
    // name, and a `strerror` may hold one of its own.
    let fix = UNRECORDED_FIXES
        .iter()
        .find(|(name, _)| reason.starts_with(&format!("{name}:")))
        .map_or("fix what stops writes to {where}", |(_, fix)| *fix);
    format!("{} ({reason})", fix.replace("{where}", where_))
}

/// The errno's NAME, as Python's `errno.errorcode` spells it.
///
/// Only the values POSIX fixes at the same number on macOS and Linux are named; above 34 the
/// two platforms diverge, and a wrong name here would be a wrong repair printed at an
/// operator. Anything else is its number, which is what charter prints for an errno its own
/// table does not hold.
fn errno_name(e: &std::io::Error) -> String {
    // Under a write's rewording, the errno it failed with (#1345).
    let Some(code) = crate::rewrite::os_cause(e).raw_os_error() else {
        return "None".to_owned();
    };
    match code {
        1 => "EPERM".to_owned(),
        2 => "ENOENT".to_owned(),
        5 => "EIO".to_owned(),
        9 => "EBADF".to_owned(),
        12 => "ENOMEM".to_owned(),
        13 => "EACCES".to_owned(),
        16 => "EBUSY".to_owned(),
        17 => "EEXIST".to_owned(),
        20 => "ENOTDIR".to_owned(),
        21 => "EISDIR".to_owned(),
        22 => "EINVAL".to_owned(),
        23 => "ENFILE".to_owned(),
        24 => "EMFILE".to_owned(),
        27 => "EFBIG".to_owned(),
        28 => "ENOSPC".to_owned(),
        30 => "EROFS".to_owned(),
        31 => "EMLINK".to_owned(),
        other => other.to_string(),
    }
}

/// The OS's own sentence for the failure, without the `(os error N)` Rust appends.
///
/// Rust has no `strerror`, and the number is already in [`errno_name`]; printing it twice is
/// what charter does not do. The suffix is Rust's own fixed format, so taking it off is
/// reading this crate's output rather than guessing at the OS's.
fn strerror(e: &std::io::Error) -> String {
    let said = crate::rewrite::os_cause(e).to_string();
    match said.rfind(" (os error ") {
        Some(at) if said.ends_with(')') => said[..at].to_owned(),
        _ => said,
    }
}

// --------------------------------------------------------------------------------------- //
// the block, and the bookkeeping behind every line in it                                    //
// --------------------------------------------------------------------------------------- //

/// What writing charter's block did — charter's `_register_excludes` status.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Wrote {
    Created,
    Refreshed,
    Present,
    /// The block is right, and it leaves out a line this checkout needs (charter#1072).
    Unhidden,
    /// Nothing was written, and why.
    Blocked(String),
}

impl Wrote {
    /// Where this outcome sits in the report's order — blocked, then unhidden, then a write,
    /// then present. Lower is worse.
    fn rank(&self) -> u8 {
        match self {
            Wrote::Blocked(_) => 0,
            Wrote::Unhidden => 1,
            Wrote::Created => 2,
            Wrote::Refreshed => 3,
            Wrote::Present => 4,
        }
    }
}

/// The worse of two passes over one block. charter's
/// `next((s for s in ("blocked", "unhidden", "created", "refreshed") if s in (first, second)))`.
fn worst(first: &Wrote, second: &Wrote) -> Wrote {
    if first.rank() <= second.rank() {
        first.clone()
    } else {
        second.clone()
    }
}

/// What charter's block in a checkout must list, why a line it holds could not be let go, and
/// why a line the checkout needs was left out. charter's `_Block`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Lines {
    /// The block's lines, in the order it writes them.
    pub rels: Vec<String>,
    /// Why a line charter could not prove unneeded was kept. `doctor` prints these, and each
    /// one names what clears it.
    pub unaccounted: Vec<String>,
    /// `{rel: why}` for each line left OUT: whose file stopped it, and what clears it.
    pub shown: BTreeMap<String, String>,
}

/// Write charter's block into `tree`'s `info/exclude` — charter's `_register_excludes`.
///
/// `(what it did, the paths it left out)`. The left-out set is what [`wire`] withholds a
/// machine-local file over.
///
/// `rels` is what THIS tree needs; the block written is [`shared_rels`]' — never one tree's
/// list alone, because a clone and its linked worktrees read one file. Written whole: killed
/// after its truncate, an in-place write lost every line of the operator's own.
fn register_excludes(
    plane: &Path,
    tree: &Path,
    rels: &[String],
    leaving: bool,
) -> (Wrote, BTreeSet<String>) {
    let Some(path) = exclude_file(tree) else {
        return (
            Wrote::Blocked(format!(
                "{} has no git directory purlis can find",
                crate::shown::short(&tree.display().to_string())
            )),
            BTreeSet::new(),
        );
    };
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return (Wrote::Blocked(e.to_string()), BTreeSet::new()),
    };
    let lines = shared_rels(plane, tree, rels, &text, &path, leaving);
    let left: BTreeSet<String> = lines.shown.keys().cloned().collect();
    let new = replace_block(&text, &rendered(&lines.rels));
    // `unhidden` over `created`, `refreshed` and `present`, never over `blocked`: those three
    // all mean "charter's files are hidden now", which is the one thing a line left out makes
    // untrue.
    let done = (!lines.shown.is_empty()).then_some(Wrote::Unhidden);
    if new == text {
        return (done.unwrap_or(Wrote::Present), left);
    }
    // Which of the two writes this is, read off the text BEFORE it changes: a block that was
    // not there is `created` and one whose lines moved is `refreshed`, which is the word
    // charter's own report uses and the only thing that tells an operator whether their
    // files have just become hidden or already were.
    let had = text
        .lines()
        .any(|line| names::EXCLUDE_BEGIN.recognises(line));
    if let Some(parent) = path.parent()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        return (Wrote::Blocked(e.to_string()), left);
    }
    if let Err(why) = write_whole(&path, &new) {
        return (Wrote::Blocked(why), left);
    }
    (
        done.unwrap_or(if had {
            Wrote::Refreshed
        } else {
            Wrote::Created
        }),
        left,
    )
}

/// What charter's block in `exclude` (holding `text`) must list once `tree` needs `rels` —
/// charter's `_shared_rels`, and the one place every line in the block is decided.
///
/// # One block, every tree's files
///
/// A clone and its linked worktrees read ONE `info/exclude`, the common git directory's, and
/// each tree rewriting charter's block there alone is how, in charter's own history, a
/// worktree wired after its clone wrote the block without the line for the clone's
/// harness-edited machine-local file — the plane's private rules plus the operator's grants —
/// and removing a workspace that held a worktree of another workspace's clone emptied that
/// clone's block outright.
///
/// # A line stays while its path is there
///
/// In `tree`, or in any other checkout charter wires that reads this exclude ([`wired`]),
/// whatever any record says. charter's rounds 2 to 4 let a line go once a record said the
/// file was no longer charter's, and each round found another way a record says that wrongly
/// — a lost marker, a torn one, a kill between a write and its record, and last two launches
/// racing to settle one file, which dropped the line for charter's own
/// `.claude/settings.json` into somebody else's repository. Hidden while it is there is the
/// safe direction, and a reversible one.
///
/// `rels` only ever ADDS a line: what charter is about to write, or wrote and still
/// recognises ([`charter_owned`]). charter never adds one for a file it did not write.
///
/// **A line leaves only on certainty** (charter's ruling G): its path confirmed absent in
/// every such checkout, and not in `rels`. It stays while, in any of them, the path cannot be
/// checked, and every line stays while the listing cannot be trusted — both say why in
/// `unaccounted`, and each reason names what clears it.
///
/// **A line is never ADDED over a file of yours** (charter#1072). The line for `tree`'s file
/// hides that path in every tree reading this exclude, so where another of them holds an
/// untracked file there that charter did not write ([`yours_untracked`]), the line is left
/// out: `tree` keeps charter's file, showing in its own `git status`, and `shown` says whose
/// file stopped it and what clears it. **Added only** — a line already in the block is not
/// re-asked, because git answers "ignored" for a path that line hides, whoever's file it is.
///
/// `leaving` is an unwire's: a checkout charter is leaving keeps no line for a file of its
/// own the harness does not also write into.
fn shared_rels(
    plane: &Path,
    tree: &Path,
    rels: &[String],
    text: &str,
    exclude: &Path,
    leaving: bool,
) -> Lines {
    use crate::worktree::listing;

    let current = already(text);
    let mut need: BTreeSet<String> = rels.iter().cloned().collect();
    if !need.is_empty() {
        // Listed whenever anything is, so it is in the block before the first temp exists,
        // and otherwise while a temp is left beside a listed path.
        need.insert(TEMP_PATTERN.to_owned());
    }
    let (trees, doubt) = listing::live_trees(tree, exclude);
    let here = crate::contain::resolved(tree);
    let others: Vec<PathBuf> = trees
        .iter()
        .flatten()
        .filter(|t| crate::contain::resolved(t.as_path()) != here)
        .cloned()
        .collect();
    // Every other tree git lists, wired or not, reads this exclude — a main checkout outside
    // the plane as surely as a piece does. But a line only STAYS for a checkout charter
    // wires, which is what keeps the promise that removing the workspace holding a linked
    // worktree takes charter's block out of that repository.
    let mut wired_here: Vec<PathBuf> = vec![tree.to_path_buf()];
    wired_here.extend(others.iter().filter(|t| wired(plane, t.as_path())).cloned());

    let mut shown: BTreeMap<String, String> = BTreeMap::new();
    // Never the marker: an untracked `.purlis-generated` is charter's even where charter
    // cannot read it, and `charter_owned` vouches for none there. The temp pattern names no
    // file that is ever found. While the list cannot be trusted there is no tree to ask, and
    // a line is added as before — `unaccounted` already names that doubt.
    let asking: Vec<String> = need
        .iter()
        .filter(|rel| !current.contains(*rel) && rel.as_str() != MARKER)
        .cloned()
        .collect();
    for rel in asking {
        let Some(mine) = others.iter().find(|t| yours_untracked(t.as_path(), &rel)) else {
            continue;
        };
        need.remove(&rel);
        // A machine-local file is withheld rather than left showing, so its sentence says
        // "not written", and what the next `reinit` then does is write it.
        let local = COWRITTEN.contains(&rel.as_str());
        let theirs = mine.join(&rel).display().to_string();
        shown.insert(
            rel.clone(),
            format!(
                "purlis's {rel} is not {} there, because {theirs} is an untracked file purlis \
                 did not write and the line hiding purlis's would hide it too, through the {} \
                 both checkouts read{} — commit or move {theirs}, and the next `purlis \
                 workspace reinit` {} purlis's",
                if local { "written" } else { "hidden" },
                exclude.display(),
                if local {
                    " — and a machine-local file purlis cannot hide is one `git add` from \
                     being committed"
                } else {
                    ""
                },
                if local { "writes and hides" } else { "hides" },
            ),
        );
    }

    // The temp pattern's own parent is the checkout root, and the root is scanned with the
    // rest: leaving that line out of this set left the root unscanned whenever it was the
    // block's last.
    let beside: BTreeSet<PathBuf> = current
        .iter()
        .map(|rel| {
            let parent = Path::new(rel).parent().unwrap_or(Path::new(""));
            parent.to_path_buf()
        })
        .collect();
    let mut why: Vec<String> = Vec::new();
    let leaving_now: Vec<String> = current.difference(&need).cloned().collect();
    for rel in leaving_now {
        for t in &wired_here {
            let there = if is_temp_pattern(&rel) {
                temps_left(beside.iter().map(|d| t.join(d)))
            } else if leaving && t.as_path() == tree && !COWRITTEN.contains(&rel.as_str()) {
                continue;
            } else {
                listing::exists(&t.join(&rel))
            };
            if there == Some(false) {
                continue;
            }
            if there.is_none() {
                why.push(format!(
                    "{} cannot be checked — restoring read access clears this",
                    t.join(&rel).display()
                ));
            }
            need.insert(rel.clone());
            break;
        }
    }
    if trees.is_none() && current.difference(&need).next().is_some() {
        why.push(doubt);
        need.extend(current.iter().cloned());
    }
    let mut ordered: Vec<String> = need
        .iter()
        .filter(|rel| rel.as_str() != MARKER && rel.as_str() != TEMP_PATTERN)
        .cloned()
        .collect();
    if need.contains(MARKER) {
        ordered.push(MARKER.to_owned());
    }
    if need.contains(TEMP_PATTERN) {
        ordered.push(TEMP_PATTERN.to_owned());
    }
    Lines {
        rels: ordered,
        unaccounted: why,
        shown,
    }
}

/// The paths under `tree` charter generated and STILL recognises, the marker included —
/// charter's `_charter_owned`, and what `tree` needs charter's block to list.
///
/// A list of FILES rather than a `.claude/` glob: a guest repo may have its own `.claude/`,
/// and hiding a directory would hide the operator's files inside it from their own
/// `git status` — charter silently making somebody's untracked work invisible in their own
/// repo is a worse failure than the untracked noise this exists to prevent.
///
/// A path whose content no record lists is left out, so charter never adds a line for a file
/// somebody else wrote. Leaving it out takes no line away: a line for a path that is there
/// stays ([`shared_rels`]), so a file charter wrote and somebody then rewrote stays hidden,
/// is never overwritten, and is reported `foreign`.
///
/// **Except a path the harness also writes into**, listed for as long as the record names it
/// whatever its digest: the harness saving an approval moves the digest without making the
/// file anybody else's.
///
/// Empty when charter has generated nothing here, so a checkout with an all-foreign layer
/// gets no block at all.
fn charter_owned(tree: &Path, record: &layer::Record) -> Vec<String> {
    if record.is_empty() {
        return Vec::new();
    }
    let mut out: Vec<String> = Vec::new();
    for rel in record.paths() {
        if !COWRITTEN.contains(&rel.as_str()) {
            match std::fs::read_to_string(tree.join(rel)) {
                Ok(text) if !record.recorded(rel).contains(&digest(&text)) => continue,
                // Gone, refused or not text: none of those proves the file is somebody
                // else's now (ruling G), and a path listed here adds a line at most.
                _ => {}
            }
        }
        out.push(rel.clone());
    }
    out.push(MARKER.to_owned());
    out
}

/// Whether checkout `t` is one charter wires: directly inside a workspace of this plane, or
/// holding a charter marker — another plane's included, and one that cannot be checked
/// counted. charter's `_wired`.
///
/// A line stays while its path is there IN A CHECKOUT CHARTER WIRES. A main checkout charter
/// never wired keeps no line by holding a `.claude/settings.local.json` of its own, which is
/// what keeps the promise that removing the workspace holding its linked worktree takes
/// charter's block out of that repository.
fn wired(plane: &Path, t: &Path) -> bool {
    let workspaces = crate::contain::resolved(&plane.join("workspaces"));
    let here = crate::contain::resolved(t);
    if let (Some(root), Some(here)) = (workspaces, here.as_ref())
        && here.parent().and_then(Path::parent) == Some(root.as_path())
    {
        return true;
    }
    names::GENERATED_SIDECAR
        .spellings()
        .any(|name| crate::worktree::listing::exists(&t.join(name)) != Some(false))
}

/// Whether checkout `t` holds a file at `rel` that charter did not write and git would show as
/// untracked there — one a line for `rel` in the exclude `t` reads would hide (charter#1072).
///
/// Charter's own is what `t`'s record vouches for ([`charter_owned`]). **Only untracked
/// counts**: a tracked file stays in `git status` whatever the exclude lists, and one git
/// already ignores is hidden with or without charter's line. A git that cannot answer is not
/// taken for "untracked", for the same reason a path that cannot be checked is not taken for
/// "there": a git that cannot read `t` shows nothing there to lose from view.
fn yours_untracked(t: &Path, rel: &str) -> bool {
    if crate::worktree::listing::exists(&t.join(rel)) != Some(true) {
        return false;
    }
    if charter_owned(t, &layer::read_record(t))
        .iter()
        .any(|r| r.as_str() == rel)
    {
        return false;
    }
    // One `git status` per path per block: a launch wires every checkout twice over, and each
    // pass would ask again.
    crate::worktree::listing::untracked(t, rel, || path_state(t, rel) == State::Committable)
}

/// Whether a file matching [`TEMP_PATTERN`] sits in any of `dirs` — `None` when one of them
/// cannot be listed and none that could be holds one. charter's `_temps_left`.
///
/// A temp a kill left is charter's own write, and may be a copy of the plane's machine-local
/// rules, so its line stays while one is there. A directory that is certainly not there holds
/// none.
fn temps_left(dirs: impl Iterator<Item = PathBuf>) -> Option<bool> {
    let mut unsure = false;
    for dir in dirs {
        match std::fs::read_dir(&dir) {
            Ok(entries) => {
                for entry in entries.flatten() {
                    let name = entry.file_name();
                    let name = name.to_string_lossy();
                    if names::GENERATED_TEMP_PREFIX.strip(&name).is_some() && name.ends_with(".tmp")
                    {
                        return Some(true);
                    }
                }
            }
            Err(e) if crate::worktree::listing::gone(&e) => continue,
            Err(_) => unsure = true,
        }
    }
    if unsure { None } else { Some(false) }
}

/// What git says about one path — charter's `util.git_path_state`, narrowed to the answers
/// this module acts on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    /// git would commit it: untracked, and not ignored.
    Committable,
    /// Tracked, ignored, not a repository, or git would not say. **An unknown is not a pass**
    /// — a probe whose failure reads as its benign answer is a check that fails open.
    Other,
}

/// One `git status` for one path.
///
/// `--no-optional-locks` because a plain `status` refreshes the index and takes `index.lock`
/// when it can, and this runs per path per tree on every launch — which is how a concurrent
/// `charter save` was broken (charter#917). It goes after `-C`, which git accepts (measured,
/// git 2.50.1). `--untracked-files=all` overrides an operator's `status.showUntrackedFiles=no`,
/// which would otherwise hide `??`; `--ignored=matching` is what tells an ignored file from a
/// tracked one. `LC_ALL=C` is [`crate::worktree::git`]'s already.
fn path_state(tree: &Path, rel: &str) -> State {
    let Ok(run) = crate::worktree::git::run(
        tree,
        &[
            "--no-optional-locks",
            "status",
            "--porcelain=v1",
            "--ignored=matching",
            "--untracked-files=all",
            "--",
            rel,
        ],
        crate::worktree::git::READ,
    ) else {
        return State::Other;
    };
    if !run.ok() {
        return State::Other;
    }
    let lines: Vec<&str> = run.out.lines().collect();
    let mut untracked = false;
    for line in &lines {
        let code: String = line.chars().take(2).collect();
        if code == "??" {
            untracked = true;
        } else if code == "!!" {
            continue;
        } else {
            // A tracked status in either column, or a line this cannot read: the next commit
            // carries the file whatever else is printed, so it is not a file a line would
            // hide from view.
            return State::Other;
        }
    }
    if lines.is_empty() {
        return State::Other;
    }
    if untracked {
        State::Committable
    } else {
        State::Other
    }
}

/// What charter's block in `text` lists now — the temp pattern among them, in either
/// spelling.
fn already(text: &str) -> BTreeSet<String> {
    let lines: Vec<&str> = text.lines().collect();
    let Some((begin, after)) = span(&lines) else {
        return BTreeSet::new();
    };
    // `begin + 1` skips the begin marker, which the filter below would drop anyway: it starts
    // with `#`, neither `/` nor the temp glob. So `begin * 1`, which keeps it in the slice, is
    // an equivalent mutant, excluded in `.cargo/mutants.toml` on that ground.
    lines[begin + 1..after]
        .iter()
        .filter(|line| line.starts_with('/') || is_temp_pattern(line))
        .map(|line| line.trim_start_matches('/').to_owned())
        .collect()
}

/// **The paths purlis itself hides in the checkout at `tree`**: what its block of the exclude
/// file that checkout reads lists now, the layer it wrote there (a chat's `AGENTS.md` among
/// them) and its temp pattern. Empty where there is no block or the file cannot be read.
///
/// For a caller that lists what git ignores in a chat's folder and must tell what the chat
/// left from what purlis put there (#1453).
pub fn hidden_by_purlis(tree: &Path) -> BTreeSet<String> {
    exclude_file(tree)
        .and_then(|file| std::fs::read_to_string(file).ok())
        .map(|text| already(&text))
        .unwrap_or_default()
}

/// Whether `path`, a path of a checkout as `git status` prints it, is one of `hidden`
/// ([`hidden_by_purlis`]): listed there itself, or a temp purlis writes beside a file of its
/// own, whatever folder it is in.
pub fn is_purlis_own(hidden: &BTreeSet<String>, path: &str) -> bool {
    let file = path.rsplit('/').next().unwrap_or(path);
    hidden.contains(path)
        || (hidden.iter().any(|rel| is_temp_pattern(rel))
            && names::GENERATED_TEMP_PREFIX
                .strip(file)
                .is_some_and(|rest| rest.ends_with(".tmp")))
}

/// The block listing `rels`, **in the order given** — [`shared_rels`] decides both, the
/// temp-file pattern among them, which is written unanchored. charter's `_exclude_block`.
///
/// Empty for an empty list: a checkout charter owns nothing in gets no block at all, and
/// [`replace_block`] then removes the one it had.
fn rendered(rels: &[String]) -> String {
    if rels.is_empty() {
        return String::new();
    }
    let mut lines: Vec<String> = vec![EXCLUDE_BEGIN.to_owned()];
    lines.extend(EXCLUDE_NOTE.iter().map(|l| (*l).to_owned()));
    for rel in rels {
        lines.push(if is_temp_pattern(rel) {
            rel.clone()
        } else {
            format!("/{rel}")
        });
    }
    lines.push(EXCLUDE_END.to_owned());
    lines.join("\n") + "\n"
}

/// `(begin, after)` — the index of charter's begin marker, and the index of the first line
/// that is not charter's — or `None` when the block is not there.
///
/// An UNTERMINATED block (a begin marker with no end) runs to the end of the file and is
/// replaced whole. The alternative is appending a second block below it, which is the
/// duplication this exists to prevent, wearing a crash for a hat.
///
/// One function for both readers of it — the rewrite and the reading of what the block holds
/// now — because two spellings of where the block ends are two answers to which lines are
/// charter's.
fn span(lines: &[&str]) -> Option<(usize, usize)> {
    let begin = lines
        .iter()
        .position(|l| names::EXCLUDE_BEGIN.recognises(l))?;
    let after = lines[begin + 1..]
        .iter()
        .position(|l| names::EXCLUDE_END.recognises(l))
        .map(|k| begin + k + 2)
        .unwrap_or(lines.len());
    Some((begin, after))
}

/// `text` with charter's block replaced by `block`.
fn replace_block(text: &str, block: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let new: Vec<&str> = block.lines().collect();
    let out: Vec<&str> = match span(&lines) {
        // Nothing of charter's here and nothing to add: the file is handed back BYTE FOR
        // BYTE rather than re-joined, because re-joining normalises a missing trailing
        // newline — a write into somebody's repository for no reason at all.
        None if new.is_empty() => return text.to_owned(),
        None => [lines.as_slice(), new.as_slice()].concat(),
        Some((begin, after)) => [&lines[..begin], new.as_slice(), &lines[after..]].concat(),
    };
    if out.is_empty() {
        String::new()
    } else {
        out.join("\n") + "\n"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The list [`shared_rels`] would hand [`rendered`] for `rels`: sorted, then the marker,
    /// then the unanchored temp glob — the order charter writes, which is what a plane wired
    /// by both implementations has to see.
    fn ordered(rels: &[&str]) -> Vec<String> {
        let mut out: Vec<String> = rels.iter().map(|r| (*r).to_owned()).collect();
        out.sort();
        out.push(MARKER.to_owned());
        out.push(TEMP_PATTERN.to_owned());
        out
    }

    /// The pattern hides the temps `rewrite::replace` actually writes, so a write killed
    /// before its rename leaves nothing in a guest checkout's `git status`.
    #[test]
    fn the_temp_pattern_is_the_one_every_whole_file_write_names_its_temp_by() {
        assert_eq!(
            TEMP_PATTERN,
            format!("{}*.tmp", crate::rewrite::TEMP_PREFIX)
        );
    }

    #[test]
    fn the_block_lists_the_marker_and_the_temp_pattern_last() {
        let block = rendered(&ordered(&[SETTINGS, ".claude/agents/steward.md"]));
        let lines: Vec<&str> = block.lines().collect();

        assert_eq!(lines[0], EXCLUDE_BEGIN);
        assert_eq!(
            lines[4..].to_vec(),
            vec![
                "/.claude/agents/steward.md",
                "/.claude/settings.json",
                "/.purlis-generated",
                ".purlis-generated.*.tmp",
                EXCLUDE_END,
            ]
        );
    }

    #[test]
    fn the_block_is_written_in_the_order_it_is_given_and_never_re_sorted() {
        // The whole of M2.26's change to this function: `_shared_rels` decides both which
        // lines and which order, so a second sort here would put the marker back among the
        // paths for any plane whose generated names sort after `.charter-generated`.
        let block = rendered(&[
            "zz/last.md".to_owned(),
            MARKER.to_owned(),
            TEMP_PATTERN.to_owned(),
        ]);

        let lines: Vec<&str> = block.lines().collect();
        assert_eq!(
            lines[4..].to_vec(),
            vec![
                "/zz/last.md",
                "/.purlis-generated",
                ".purlis-generated.*.tmp",
                EXCLUDE_END,
            ]
        );
    }

    #[test]
    fn a_block_listing_nothing_is_no_block_at_all() {
        // A checkout charter owns nothing in gets no block, and the one it had is removed —
        // which is what makes `unwire` the same code path as a wire that wants nothing.
        assert_eq!(rendered(&[]), "");
        let had = replace_block("# mine\n", &rendered(&ordered(&[SETTINGS])));
        assert!(had.contains(EXCLUDE_BEGIN));

        assert_eq!(replace_block(&had, &rendered(&[])), "# mine\n");
    }

    #[test]
    fn replacing_the_block_twice_leaves_one_block() {
        let rels = ordered(&[SETTINGS]);
        let theirs = "# mine\n/build\n";

        let once = replace_block(theirs, &rendered(&rels));
        let twice = replace_block(&once, &rendered(&rels));

        assert_eq!(once, twice, "a wire runs on every launch");
        assert_eq!(once.matches(EXCLUDE_BEGIN).count(), 1);
        assert!(once.starts_with("# mine\n/build\n"), "{once}");
    }

    #[test]
    fn an_unterminated_block_is_replaced_rather_than_doubled() {
        let torn = format!("# mine\n{EXCLUDE_BEGIN}\n/.claude/settings.json\n");

        let fixed = replace_block(&torn, &rendered(&ordered(&[SETTINGS])));

        assert_eq!(fixed.matches(EXCLUDE_BEGIN).count(), 1, "{fixed}");
        assert!(fixed.contains(EXCLUDE_END), "{fixed}");
    }

    /// A block exactly as charter wrote it before the rename, between somebody's lines.
    const CHARTERS_BLOCK: &str = "# mine\n\
        # >>> charter (generated layer — `charter workspace reinit`) >>>\n\
        # a note\n\
        /.claude/settings.json\n\
        /.charter-generated\n\
        .charter-generated.*.tmp\n\
        # <<< charter <<<\n\
        /build\n";

    #[test]
    fn a_block_charter_wrote_is_read_back_line_for_line() {
        let held = already(CHARTERS_BLOCK);
        assert_eq!(
            held,
            BTreeSet::from([
                SETTINGS.to_owned(),
                ".charter-generated".to_owned(),
                ".charter-generated.*.tmp".to_owned(),
            ])
        );
        assert!(is_temp_pattern(".charter-generated.*.tmp"));
        assert!(is_temp_pattern(TEMP_PATTERN));
        assert!(!is_temp_pattern(".charter-generated"));
    }

    #[test]
    fn a_block_charter_wrote_is_rewritten_in_place_under_the_purlis_markers() {
        let fixed = replace_block(CHARTERS_BLOCK, &rendered(&ordered(&[SETTINGS])));
        assert_eq!(
            fixed,
            format!("# mine\n{}/build\n", rendered(&ordered(&[SETTINGS]))),
            "where it stood, once"
        );
        assert!(!fixed.contains("# >>> charter"), "{fixed}");
        assert_eq!(fixed.matches("generated layer").count(), 1, "{fixed}");
        assert_eq!(already(&fixed), already(&rendered(&ordered(&[SETTINGS]))));
    }

    #[test]
    fn a_block_opened_under_one_name_and_closed_under_the_other_is_one_block() {
        let mixed = CHARTERS_BLOCK.replace("# <<< purlis <<<", EXCLUDE_END);
        let fixed = replace_block(&mixed, &rendered(&ordered(&[SETTINGS])));
        assert!(fixed.ends_with("/build\n"), "{fixed}");
        assert_eq!(fixed.matches("generated layer").count(), 1, "{fixed}");
    }

    #[test]
    fn a_file_with_nothing_of_charters_is_handed_back_unchanged_when_there_is_nothing_to_add() {
        let theirs = "# mine\n/build\n";

        assert_eq!(replace_block(theirs, ""), theirs);
    }

    #[test]
    fn what_the_block_holds_now_is_read_back_in_either_spelling_of_the_temp_glob() {
        let text = format!(
            "{EXCLUDE_BEGIN}\n# note\n/.claude/settings.json\n/{MARKER}\n{TEMP_PATTERN}\n\
             {EXCLUDE_END}\n"
        );

        let held = already(&text);

        assert!(held.contains(SETTINGS));
        assert!(held.contains(MARKER));
        assert!(held.contains(TEMP_PATTERN), "{held:?}");
        assert!(!held.contains("# note"));
    }

    #[test]
    fn what_clears_a_record_charter_could_not_publish_is_worded_for_its_errno() {
        // charter's list, and the reason it is a list: "restore write access" was the advice
        // for every one of them, and a full disk or a read-only mount has no write access to
        // restore.
        let dir = tempfile::tempdir().unwrap();
        let plane = dir.path();
        let tree = plane.join("workspaces").join("beta").join("svc");
        std::fs::create_dir_all(&tree).unwrap();

        for (code, expect) in [
            (13, "restore write access to that checkout"),
            (28, "free space on the disk that holds that checkout"),
            (
                30,
                "remount the read-only filesystem that holds that checkout",
            ),
            (21, "fix what stops writes to that checkout"),
        ] {
            note_unrecorded(plane, &tree, Some(&std::io::Error::from_raw_os_error(code)));
            let said = unrecorded_fix(plane, &tree, "that checkout");
            assert!(said.starts_with(expect), "errno {code}: {said}");
            assert!(said.ends_with(')'), "the errno itself is in it: {said}");
        }

        // And the first publish that succeeds forgets it.
        note_unrecorded(plane, &tree, None);
        assert_eq!(unrecorded_fix(plane, &tree, "that checkout"), "");
        assert_eq!(unrecorded_reason(plane, &tree), "");
    }

    #[test]
    fn a_publish_a_sandbox_refused_is_still_recorded_as_eperm() {
        // #1345 rewords an EPERM write to name its file; the note keeps the errno under it.
        let dir = tempfile::tempdir().unwrap();
        let plane = dir.path();
        let tree = plane.join("workspaces").join("beta").join("svc");
        std::fs::create_dir_all(&tree).unwrap();
        let refused = crate::rewrite::refused_write(
            std::io::Error::from_raw_os_error(1),
            &tree.join(".git/info/exclude"),
            true,
        );

        note_unrecorded(plane, &tree, Some(&refused));

        let reason = unrecorded_reason(plane, &tree);
        assert!(reason.starts_with("EPERM: "), "{reason}");
        assert!(
            !reason.contains("sandbox"),
            "the OS's own sentence: {reason}"
        );
    }

    #[test]
    fn a_refusal_is_named_by_its_errno_and_never_by_the_number_twice() {
        let e = std::io::Error::from_raw_os_error(13);
        assert_eq!(errno_name(&e), "EACCES");
        assert!(!strerror(&e).contains("os error"), "{}", strerror(&e));
        assert_eq!(reason(&e), format!("EACCES: {}", strerror(&e)));
    }

    /// A plane with a workspace directory, and a checkout at `workspaces/beta/<name>` with
    /// no git in it — enough for every question [`shared_rels`] asks that is not git's.
    fn plane_with(name: &str) -> (tempfile::TempDir, PathBuf, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let plane = dir.path().join("plane");
        let tree = plane.join("workspaces").join("beta").join(name);
        std::fs::create_dir_all(tree.join(".git").join("info")).unwrap();
        (dir, plane, tree)
    }

    #[cfg(unix)]
    #[test]
    fn an_agents_md_in_a_tree_charter_cannot_look_into_is_a_doubt() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let tree = dir.path().join("p2");
        std::fs::create_dir_all(&tree).unwrap();
        std::fs::set_permissions(&tree, std::fs::Permissions::from_mode(0o600)).unwrap();

        let mut out = HiddenAgentsMd::default();
        hidden_in(vec![tree.clone()], &mut out);
        std::fs::set_permissions(&tree, std::fs::Permissions::from_mode(0o755)).unwrap();

        assert!(out.found.is_empty());
        assert!(
            out.unsure
                .iter()
                .any(|why| why.contains(&tree.join(AGENTS_MD).display().to_string())),
            "{out:?}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_chats_agents_md_waits_for_another_start_in_the_same_piece() {
        let dir = tempfile::tempdir().unwrap();
        let plane = std::fs::canonicalize(dir.path()).unwrap();
        let tree = plane.join("tree");
        std::fs::create_dir_all(&tree).unwrap();
        crate::testgit::run(&tree, &["init", "-q", "-b", "main"]);
        let held = Held::on(&tree);
        assert!(
            held.0.is_some(),
            "the lock is taken on the tree's git directory"
        );
        let done = std::sync::atomic::AtomicBool::new(false);

        std::thread::scope(|s| {
            s.spawn(|| {
                guide(&plane, &tree, "x\n");
                done.store(true, std::sync::atomic::Ordering::SeqCst);
            });
            std::thread::sleep(std::time::Duration::from_millis(400));
            assert!(
                !done.load(std::sync::atomic::Ordering::SeqCst),
                "a second start writes nothing while the first holds the piece"
            );
            drop(held);
        });

        assert!(done.load(std::sync::atomic::Ordering::SeqCst));
        assert_eq!(
            std::fs::read_to_string(tree.join(AGENTS_MD)).unwrap(),
            "x\n"
        );
    }

    #[test]
    fn a_checkout_inside_a_workspace_of_this_plane_is_one_charter_wires() {
        let (dir, plane, tree) = plane_with("svc");

        assert!(wired(&plane, &tree));
        // A checkout somewhere else is not — unless it holds a marker of charter's, which is
        // how another plane's checkout counts.
        let outside = dir.path().join("elsewhere");
        std::fs::create_dir_all(&outside).unwrap();
        assert!(!wired(&plane, &outside));
        std::fs::write(outside.join(MARKER), "{}").unwrap();
        assert!(wired(&plane, &outside));
    }

    #[test]
    fn a_line_is_left_out_rather_than_hide_an_untracked_file_of_yours_next_door() {
        // charter#1072, at the one decision that makes it: the line for THIS tree's path
        // would hide that path in every tree reading the exclude, so it is not added — and
        // `shown` names whose file stopped it and what clears it.
        //
        // `others` here is decided by the listing, which a directory with no git in it
        // cannot give, so the sibling is planted as a real worktree registration in the
        // integration tests. What this pins is the SENTENCE and the ordering rule around it.
        let (_dir, plane, tree) = plane_with("svc");
        let exclude = tree.join(".git").join("info").join("exclude");

        let lines = shared_rels(
            &plane,
            &tree,
            &[SETTINGS.to_owned(), MARKER.to_owned()],
            "",
            &exclude,
            false,
        );

        // With no sibling to ask about, every wanted line is in — the marker and the temp
        // glob last, which is the order charter writes.
        assert_eq!(
            lines.rels,
            vec![
                SETTINGS.to_owned(),
                MARKER.to_owned(),
                TEMP_PATTERN.to_owned()
            ]
        );
        assert!(lines.shown.is_empty());
        assert!(lines.unaccounted.is_empty());
    }

    #[test]
    fn asking_for_nothing_lists_no_temp_glob_and_keeps_a_line_whose_file_is_there() {
        // Two rules at once, and both are charter's. `need` empty adds no temp pattern; and
        // a line already in the block STAYS while its path is there, whatever any record
        // says — which is what stops a second tree's wire dropping the first tree's line.
        let (_dir, plane, tree) = plane_with("svc");
        let exclude = tree.join(".git").join("info").join("exclude");
        std::fs::create_dir_all(tree.join(".claude")).unwrap();
        std::fs::write(tree.join(SETTINGS), "{}\n").unwrap();
        std::fs::write(tree.join(MARKER), "{}\n").unwrap();
        let text = rendered(&[SETTINGS.to_owned(), MARKER.to_owned()]);

        let lines = shared_rels(&plane, &tree, &[], &text, &exclude, false);

        assert_eq!(lines.rels, vec![SETTINGS.to_owned(), MARKER.to_owned()]);
        assert!(
            !lines.rels.contains(&TEMP_PATTERN.to_owned()),
            "an empty ask adds no temp glob: {lines:?}"
        );
    }

    #[test]
    fn a_line_leaves_only_once_its_path_is_proved_gone() {
        let (_dir, plane, tree) = plane_with("svc");
        let exclude = tree.join(".git").join("info").join("exclude");
        // The block names a path that is not there, in a checkout charter wires, and nothing
        // asks for it: the one shape a line may be dropped in.
        let text = rendered(&[SETTINGS.to_owned(), MARKER.to_owned()]);

        let lines = shared_rels(&plane, &tree, &[], &text, &exclude, false);

        assert!(!lines.rels.contains(&SETTINGS.to_owned()), "{lines:?}");
        assert!(lines.unaccounted.is_empty(), "{lines:?}");
    }

    #[test]
    fn a_path_that_cannot_be_checked_keeps_its_line_and_says_what_clears_it() {
        use std::os::unix::fs::PermissionsExt;

        let (_dir, plane, tree) = plane_with("svc");
        let exclude = tree.join(".git").join("info").join("exclude");
        let claude = tree.join(".claude");
        std::fs::create_dir_all(&claude).unwrap();
        std::fs::set_permissions(&claude, std::fs::Permissions::from_mode(0o000)).unwrap();
        let text = rendered(&[SETTINGS.to_owned(), MARKER.to_owned()]);

        let lines = shared_rels(&plane, &tree, &[], &text, &exclude, false);
        std::fs::set_permissions(&claude, std::fs::Permissions::from_mode(0o755)).unwrap();

        assert!(lines.rels.contains(&SETTINGS.to_owned()), "{lines:?}");
        assert_eq!(lines.unaccounted.len(), 1, "{lines:?}");
        assert!(
            lines.unaccounted[0].contains("restoring read access clears this"),
            "{lines:?}"
        );
    }

    #[test]
    fn charter_owns_a_file_its_record_still_recognises_and_nothing_else() {
        let (_dir, _plane, tree) = plane_with("svc");
        std::fs::create_dir_all(tree.join(".claude")).unwrap();
        std::fs::write(tree.join(SETTINGS), "mine\n").unwrap();
        let mut record = layer::Record::new();
        record.settle(SETTINGS, digest("mine\n"));

        assert_eq!(
            charter_owned(&tree, &record),
            vec![SETTINGS.to_owned(), MARKER.to_owned()]
        );

        // Rewritten by somebody: charter adds no line for it. The line it already has stays,
        // which is `shared_rels`' rule and not this one's.
        std::fs::write(tree.join(SETTINGS), "theirs\n").unwrap();
        assert_eq!(charter_owned(&tree, &record), vec![MARKER.to_owned()]);

        // A machine-local path stays listed whatever its digest: the harness saving an
        // approval moves it without making the file anybody else's.
        let mut local = layer::Record::new();
        local.settle(LOCAL_SETTINGS, digest("whatever charter wrote"));
        std::fs::write(tree.join(LOCAL_SETTINGS), "the harness added this\n").unwrap();
        assert_eq!(
            charter_owned(&tree, &local),
            vec![LOCAL_SETTINGS.to_owned(), MARKER.to_owned()]
        );

        // And a record that names nothing owns nothing — not even the marker, so a checkout
        // with an all-foreign layer gets no block at all.
        assert!(charter_owned(&tree, &layer::Record::new()).is_empty());
    }

    #[test]
    fn a_temp_a_kill_left_behind_keeps_the_glob_listed() {
        let (_dir, plane, tree) = plane_with("svc");
        let exclude = tree.join(".git").join("info").join("exclude");
        std::fs::create_dir_all(tree.join(".claude")).unwrap();
        std::fs::write(
            tree.join(".claude").join(".charter-generated.9.ab.tmp"),
            "x",
        )
        .unwrap();
        let text = rendered(&[SETTINGS.to_owned(), TEMP_PATTERN.to_owned()]);

        let lines = shared_rels(&plane, &tree, &[], &text, &exclude, false);

        assert!(lines.rels.contains(&TEMP_PATTERN.to_owned()), "{lines:?}");
    }

    #[test]
    fn a_wanted_path_the_harness_keeps_is_told_from_one_it_only_added_to() {
        let (_dir, _plane, tree) = plane_with("svc");
        std::fs::create_dir_all(tree.join(".claude")).unwrap();
        let want = "{\"deny\":[]}\n";
        std::fs::write(tree.join(LOCAL_SETTINGS), "the harness rewrote this\n").unwrap();

        // charter's last write IS what the plane wants now: the harness only added to it,
        // every rule charter mirrored is still in it, and charter leaves it alone.
        let mut current = layer::Record::new();
        current.settle(LOCAL_SETTINGS, digest(want));
        assert_eq!(
            planned(&tree, LOCAL_SETTINGS, want, &current, &BTreeMap::new()),
            Plan::HarnessEdited
        );

        // The plane has moved on since: charter will not merge into a file the harness
        // keeps, so the plane's newer rules are NOT in force there and the row says so.
        let mut behind = layer::Record::new();
        behind.settle(LOCAL_SETTINGS, digest("something older"));
        assert_eq!(
            planned(&tree, LOCAL_SETTINGS, want, &behind, &BTreeMap::new()),
            Plan::Theirs
        );

        // A pending entry is not a settled one: an approval saved over an interrupted write
        // must not settle a file that never received the plane's new `deny`.
        let mut pending = layer::Record::new();
        pending.pend(LOCAL_SETTINGS, digest(want));
        assert_eq!(
            planned(&tree, LOCAL_SETTINGS, want, &pending, &BTreeMap::new()),
            Plan::Theirs
        );
    }

    #[test]
    fn a_file_only_the_checkouts_record_vouches_for_is_never_refreshed() {
        // The record sits in the checkout, where a chat can write it. One naming the
        // operator's own file, with that file's digest, must not make it purlis's to
        // overwrite: the project's ledger must have noted that very text at that path too.
        let (_dir, _plane, tree) = plane_with("svc");
        std::fs::create_dir_all(tree.join(".claude/agents")).unwrap();
        let want = "{\"permissions\":{\"ask\":[]}}\n";
        for (rel, mine) in [
            (
                SETTINGS,
                "{\"permissions\":{\"deny\":[\"Bash(curl *)\"]}}\n",
            ),
            (
                LOCAL_SETTINGS,
                "{\"permissions\":{\"deny\":[\"Bash(ssh *)\"]}}\n",
            ),
            (AGENT, "# my own\n"),
        ] {
            std::fs::write(tree.join(rel), mine).unwrap();
            let mut forged = layer::Record::new();
            forged.settle(rel, digest(mine));

            assert_eq!(
                planned(&tree, rel, want, &forged, &BTreeMap::new()),
                Plan::Unconfirmed,
                "{rel}"
            );
            // Noted at another path, or another text at this one: still not this file.
            let elsewhere = BTreeMap::from([("x".to_owned(), vec![digest(mine)])]);
            assert_eq!(
                planned(&tree, rel, want, &forged, &elsewhere),
                Plan::Unconfirmed
            );
            let other = BTreeMap::from([(rel.to_owned(), vec![digest("older\n")])]);
            assert_eq!(
                planned(&tree, rel, want, &forged, &other),
                Plan::Unconfirmed
            );

            // The genuine case: both vouch, so purlis's own stale copy is brought up to date.
            let noted = BTreeMap::from([(rel.to_owned(), vec![digest(mine)])]);
            assert_eq!(planned(&tree, rel, want, &forged, &noted), Plan::Refresh);
        }
        assert!(
            failed(Status::Unconfirmed),
            "the newer layer is not in force there"
        );
    }

    #[test]
    fn a_generated_path_that_cannot_be_read_is_never_taken_for_one_charter_may_write() {
        use std::os::unix::fs::PermissionsExt;

        let (_dir, _plane, tree) = plane_with("svc");
        let claude = tree.join(".claude");
        std::fs::create_dir_all(&claude).unwrap();
        std::fs::write(tree.join(SETTINGS), "theirs\n").unwrap();
        std::fs::set_permissions(&claude, std::fs::Permissions::from_mode(0o000)).unwrap();

        let what = planned(
            &tree,
            SETTINGS,
            "{}\n",
            &layer::Record::new(),
            &BTreeMap::new(),
        );
        std::fs::set_permissions(&claude, std::fs::Permissions::from_mode(0o755)).unwrap();

        // NOT `Create`: reading EACCES as "not there" is how charter would come to write over
        // a path it was never allowed to look at.
        assert_ne!(what, Plan::Create);
    }

    #[test]
    fn a_generated_path_reached_through_a_link_out_of_the_checkout_is_never_read_as_charters() {
        let (dir, _plane, tree) = plane_with("svc");
        let outside = dir.path().join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("settings.json"), "{}\n").unwrap();
        std::fs::create_dir_all(tree.join(".claude")).unwrap();
        std::os::unix::fs::symlink(outside.join("settings.json"), tree.join(SETTINGS)).unwrap();

        // The bytes on the far end match what the plane wants exactly, so a reader that
        // followed the link would call it `current` and report the plane's rules in force
        // through a link charter will not write through.
        assert_eq!(
            planned(
                &tree,
                SETTINGS,
                "{}\n",
                &layer::Record::new(),
                &BTreeMap::new()
            ),
            Plan::Foreign
        );
    }

    #[test]
    fn a_checkout_charter_is_leaving_keeps_no_line_for_a_file_of_its_own() {
        // `leaving` is an unwire's, and it is the one branch of `shared_rels` no command
        // reaches yet — [`crate::wscmd`]'s gap list names `unwire` as unported. It is tested
        // here rather than left for the day it is, because a guard nothing drives is a guard
        // nobody can tell from one that is wrong.
        //
        // The rule: a checkout charter is LEAVING keeps no line for a file of its own, even
        // though the file is still there — the whole point of leaving is that the operator
        // gets their `git status` back. A co-written file is the exception and is checked
        // below: the harness saves into a file it finds already ignored without adding an
        // ignore of its own, so its line stays.
        let (_dir, plane, tree) = plane_with("svc");
        let exclude = tree.join(".git").join("info").join("exclude");
        std::fs::create_dir_all(tree.join(".claude")).unwrap();
        std::fs::write(tree.join(SETTINGS), "{}\n").unwrap();
        std::fs::write(tree.join(LOCAL_SETTINGS), "{}\n").unwrap();
        let text = rendered(&[SETTINGS.to_owned(), LOCAL_SETTINGS.to_owned()]);

        let staying = shared_rels(&plane, &tree, &[], &text, &exclude, false);
        let going = shared_rels(&plane, &tree, &[], &text, &exclude, true);

        assert_eq!(
            staying.rels,
            vec![SETTINGS.to_owned(), LOCAL_SETTINGS.to_owned()],
            "both files are there, so both lines stay"
        );
        assert_eq!(
            going.rels,
            vec![LOCAL_SETTINGS.to_owned()],
            "on the way out charter's own line goes and the co-written one stays"
        );
    }

    #[test]
    fn a_line_kept_without_proof_is_named_where_a_reader_looks() {
        // charter's ruling G, second half: keep every line, and let the report say what could
        // not be accounted for. A block kept silently is a block nobody can tell from one
        // that is right — which is the only thing standing between "charter still hides this"
        // and "charter has forgotten why".
        use std::os::unix::fs::PermissionsExt;

        let (_dir, plane, tree) = plane_with("svc");
        let exclude = tree.join(".git").join("info").join("exclude");
        // The block names a path charter owns nothing at any more, and the path cannot be
        // checked: not proved gone, so the line stays — and says why.
        std::fs::write(
            &exclude,
            rendered(&[SETTINGS.to_owned(), MARKER.to_owned()]),
        )
        .unwrap();
        let claude = tree.join(".claude");
        std::fs::create_dir_all(&claude).unwrap();
        std::fs::set_permissions(&claude, std::fs::Permissions::from_mode(0o000)).unwrap();

        let said = unaccounted(&plane, &tree);
        std::fs::set_permissions(&claude, std::fs::Permissions::from_mode(0o755)).unwrap();

        assert_eq!(said.len(), 1, "{said:?}");
        assert!(said[0].contains(SETTINGS), "{said:?}");
        assert!(
            said[0].contains("restoring read access clears this"),
            "{said:?}"
        );
    }

    #[test]
    fn a_marker_key_that_leaves_the_checkout_is_not_one_charter_could_have_written() {
        use crate::layer::key_ok;

        assert!(key_ok(".claude/settings.json"));
        assert!(key_ok(MARKER));
        for bad in [
            "/etc/passwd",
            "../outside",
            ".claude/../../outside",
            "",
            "./x",
        ] {
            assert!(!key_ok(bad), "{bad}");
        }
    }

    fn layer_of(rows: Vec<Row>, block: Block) -> Wired {
        Wired {
            rows,
            hidden: Hidden::InPlace,
            block,
        }
    }

    fn row_of(rel: &str, status: Status, why: &str) -> Row {
        Row {
            rel: rel.into(),
            status,
            why: why.into(),
        }
    }

    #[test]
    fn a_refusal_names_what_stopped_the_layer_and_nothing_when_nothing_did() {
        let tree = Path::new("/w/alpha/.worktrees/app/fix");
        let fine = row_of(".claude/settings.json", Status::Current, "");

        assert_eq!(
            layer_of(vec![fine.clone()], Block::Present).refusal(tree),
            ""
        );
        assert!(
            layer_of(vec![fine.clone()], Block::Blocked)
                .refusal(tree)
                .starts_with(
                    "purlis wrote its files in /w/alpha/.worktrees/app/fix and could not settle \
                     the block"
                )
        );

        let unrecorded = row_of(".charter-generated", Status::Unrecorded, "disk full");
        assert_eq!(
            layer_of(vec![fine.clone(), unrecorded], Block::Present).refusal(tree),
            "purlis could not publish its record in /w/alpha/.worktrees/app/fix first (disk \
             full), so it wrote nothing and kept every exclude line it had — a chat there would \
             run without the plane's layer."
        );
        let blocked = row_of(".claude/settings.json", Status::Blocked, "read-only");
        assert_eq!(
            layer_of(vec![blocked], Block::Present).refusal(tree),
            "purlis could not write .claude/settings.json in /w/alpha/.worktrees/app/fix \
             (read-only), so a chat there would run without the plane's layer. Restore write \
             access and start the chat again."
        );
    }

    #[test]
    fn a_file_with_no_block_and_nothing_to_add_is_handed_back_byte_for_byte() {
        // Re-joining would add the newline this file does not end with — a write into
        // somebody's repository for no reason at all.
        assert_eq!(replace_block("# mine", ""), "# mine");
        assert_eq!(replace_block("# mine\r\n/build", ""), "# mine\r\n/build");
    }

    #[test]
    fn the_os_sentence_loses_only_the_suffix_rust_appends() {
        assert_eq!(
            strerror(&std::io::Error::from_raw_os_error(2)),
            "No such file or directory"
        );
        // Not Rust's `(os error N)` suffix at the end: nothing is cut.
        let said = std::io::Error::other("a (os error 5) b");
        assert_eq!(strerror(&said), "a (os error 5) b");
    }

    #[test]
    fn the_worse_of_two_passes_is_the_one_the_row_reports() {
        let blocked = Wrote::Blocked("no".into());
        for (a, b, want) in [
            (&blocked, &Wrote::Unhidden, &blocked),
            (&Wrote::Unhidden, &blocked, &blocked),
            (&Wrote::Created, &Wrote::Unhidden, &Wrote::Unhidden),
            (&Wrote::Refreshed, &Wrote::Created, &Wrote::Created),
            (&Wrote::Present, &Wrote::Refreshed, &Wrote::Refreshed),
            (&Wrote::Refreshed, &Wrote::Present, &Wrote::Refreshed),
            (&Wrote::Present, &Wrote::Present, &Wrote::Present),
        ] {
            assert_eq!(&worst(a, b), want, "{a:?} and {b:?}");
        }
    }

    #[test]
    fn a_temp_charter_left_is_found_and_a_directory_that_is_gone_holds_none() {
        let dir = tempfile::tempdir().unwrap();
        let with = dir.path().join("with");
        let without = dir.path().join("without");
        std::fs::create_dir_all(&with).unwrap();
        std::fs::create_dir_all(&without).unwrap();
        std::fs::write(with.join(".charter-generated.123.tmp"), "").unwrap();
        // Half the pattern each is not the pattern.
        std::fs::write(without.join(".charter-generated.123"), "").unwrap();
        std::fs::write(without.join("other.tmp"), "").unwrap();
        let gone = dir.path().join("gone");

        assert_eq!(temps_left([without.clone(), with].into_iter()), Some(true));
        assert_eq!(temps_left([without.clone(), gone].into_iter()), Some(false));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let shut = dir.path().join("shut");
            std::fs::create_dir_all(&shut).unwrap();
            std::fs::set_permissions(&shut, std::fs::Permissions::from_mode(0o000)).unwrap();
            let listed = std::fs::read_dir(&shut).is_ok();
            let found = temps_left([without, shut.clone()].into_iter());
            std::fs::set_permissions(&shut, std::fs::Permissions::from_mode(0o755)).unwrap();
            // Root reads through a mode of 000, and the question then does not arise.
            if !listed {
                assert_eq!(
                    found, None,
                    "a directory that cannot be listed is not an empty one"
                );
            }
        }
    }

    fn git_in(dir: &Path, args: &[&str]) {
        let run = crate::testgit::run(dir, args);
        assert!(run.ok(), "git {args:?}: {}", run.err);
    }

    #[test]
    fn only_a_path_git_would_commit_is_committable() {
        let dir = tempfile::tempdir().unwrap();
        let tree = std::fs::canonicalize(dir.path()).unwrap();
        git_in(&tree, &["init", "-q", "."]);
        std::fs::write(tree.join(".gitignore"), "ignored*\n").unwrap();
        std::fs::write(tree.join("tracked"), "one\n").unwrap();
        git_in(&tree, &["add", ".gitignore", "tracked"]);
        std::fs::write(tree.join("tracked"), "two\n").unwrap();
        std::fs::write(tree.join("loose"), "").unwrap();
        std::fs::write(tree.join("ignored-file"), "").unwrap();
        std::fs::create_dir_all(tree.join("mixed")).unwrap();
        std::fs::write(tree.join("mixed/loose"), "").unwrap();
        std::fs::write(tree.join("mixed/ignored-too"), "").unwrap();

        assert_eq!(path_state(&tree, "loose"), State::Committable);
        assert_eq!(path_state(&tree, "tracked"), State::Other);
        assert_eq!(path_state(&tree, "ignored-file"), State::Other);
        assert_eq!(path_state(&tree, "absent"), State::Other);
        // An ignored file beside an untracked one does not hide the untracked one.
        assert_eq!(path_state(&tree, "mixed"), State::Committable);
        // Not a repository at all: git fails, and a failure is not a pass.
        let bare = tempfile::tempdir().unwrap();
        std::fs::write(bare.path().join("loose"), "").unwrap();
        assert_eq!(path_state(bare.path(), "loose"), State::Other);

        assert!(yours_untracked(&tree, "loose"));
        assert!(!yours_untracked(&tree, "absent"), "nothing there to hide");
        assert!(
            !yours_untracked(&tree, "tracked"),
            "a tracked file shows anyway"
        );
    }

    #[test]
    fn a_link_to_a_directory_is_handed_back_and_never_walked_into() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("a/b")).unwrap();
        std::fs::write(root.join("a/b/deep.md"), "").unwrap();
        std::fs::write(root.join("top.md"), "").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink("..", root.join("a/loop")).unwrap();

        let mut found: Vec<String> = walk(root)
            .into_iter()
            .map(|p| p.strip_prefix(root).unwrap().display().to_string())
            .collect();
        found.sort();

        #[cfg(unix)]
        assert_eq!(found, ["a/b/deep.md", "a/loop", "top.md"]);
        #[cfg(not(unix))]
        assert_eq!(found, ["a/b/deep.md", "top.md"]);
    }

    // -----------------------------------------------------------------------------------
    // #1583: a checkout withdraws the mirrored agents and skills the project stopped having
    // -----------------------------------------------------------------------------------

    const AGENT: &str = ".claude/agents/ops.md";

    /// A plane and a checkout in which purlis wrote `rel` holding `text`, as its record says.
    fn mirrored_once(
        rel: &str,
        text: &str,
    ) -> (tempfile::TempDir, PathBuf, PathBuf, layer::Record) {
        let (dir, plane, tree) = plane_with("svc");
        std::fs::create_dir_all(&plane).unwrap();
        write_into(&tree, rel, text).unwrap();
        let mut record = layer::Record::new();
        record.settle(rel, digest(text));
        (dir, plane, tree, record)
    }

    fn untracked(_: &str) -> bool {
        false
    }

    /// Every text `record` names, as the project's offers: the record purlis itself wrote.
    fn offered_as(record: &layer::Record) -> BTreeMap<String, Vec<String>> {
        record
            .paths()
            .map(|rel| (rel.clone(), record.recorded(rel).to_vec()))
            .collect()
    }

    #[test]
    fn a_record_naming_a_file_the_project_never_offered_withdraws_nothing() {
        // The record is in the checkout, where a chat can write it: one naming the operator's
        // own agent, with that file's digest, must not get purlis to delete it.
        let (_dir, plane, tree, mut record) = mirrored_once(AGENT, "# mine\n");
        let want = BTreeMap::new();

        let retired = retired_mirrors(&plane, &tree, &want, &record, &BTreeMap::new(), &untracked);
        assert!(retired.is_empty(), "{retired:?}");

        // Offered at that path, but another text: still not this file.
        let other = BTreeMap::from([(AGENT.to_owned(), vec![digest("# ops\n")])]);
        assert!(retired_mirrors(&plane, &tree, &want, &record, &other, &untracked).is_empty());
        withdraw_mirrors(&plane, &tree, &want, &mut record, Vec::new());
        assert_eq!(
            std::fs::read_to_string(tree.join(AGENT)).unwrap(),
            "# mine\n"
        );
    }

    #[test]
    fn what_the_project_offers_is_noted_once_and_kept_to_the_newest() {
        let dir = tempfile::tempdir().unwrap();
        let plane = dir.path();
        let want = |text: &str| {
            BTreeMap::from([
                (AGENT.to_owned(), text.to_owned()),
                (SETTINGS.to_owned(), "{}\n".to_owned()),
                (LOCAL_SETTINGS.to_owned(), "{}\n".to_owned()),
            ])
        };

        note_offered(plane, &want("# ops\n"));
        let noted = read_offered(plane);
        assert_eq!(
            noted,
            BTreeMap::from([
                (AGENT.to_owned(), vec![digest("# ops\n")]),
                (SETTINGS.to_owned(), vec![digest("{}\n")]),
                (LOCAL_SETTINGS.to_owned(), vec![digest("{}\n")]),
            ]),
            "every generated and mirrored text is noted, so a refresh can be confirmed"
        );
        let written = std::fs::metadata(offered_path(plane))
            .unwrap()
            .modified()
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        note_offered(plane, &want("# ops\n"));
        assert_eq!(
            std::fs::metadata(offered_path(plane))
                .unwrap()
                .modified()
                .unwrap(),
            written,
            "nothing new, nothing written"
        );

        for n in 0..MOST_OFFERED + 3 {
            note_offered(plane, &want(&format!("# ops {n}\n")));
        }
        let kept = &read_offered(plane)[AGENT];
        assert_eq!(kept.len(), MOST_OFFERED);
        assert_eq!(
            kept.last(),
            Some(&digest(&format!("# ops {}\n", MOST_OFFERED + 2)))
        );
        assert!(!kept.contains(&digest("# ops\n")), "the oldest went first");

        // A file of another shape, or naming what is no mirror, offers nothing.
        assert!(parse_offered("[1]").is_empty());
        assert!(parse_offered(r#"{"../x": ["a"], ".claude/notes/x.md": ["a"]}"#).is_empty());
    }

    #[test]
    fn a_mirror_the_project_no_longer_has_is_withdrawn_with_its_entry_and_its_empty_folders() {
        let (_dir, plane, tree, mut record) = mirrored_once(AGENT, "# ops\n");
        let want = BTreeMap::new();

        let retired = retired_mirrors(
            &plane,
            &tree,
            &want,
            &record,
            &offered_as(&record),
            &untracked,
        );
        assert_eq!(retired, [AGENT.to_owned()]);
        let rows = withdraw_mirrors(&plane, &tree, &want, &mut record, retired);

        assert_eq!(rows, [row_of(AGENT, Status::Removed, "")]);
        assert!(!tree.join(AGENT).exists());
        assert!(!record.names(AGENT), "the entry goes with the file");
        assert!(
            !tree.join(".claude").exists(),
            "a folder left empty goes too: {:?}",
            std::fs::read_dir(&tree)
                .unwrap()
                .flatten()
                .map(|e| e.path())
                .collect::<Vec<_>>()
        );
        assert!(tree.join(".git").exists(), "and nothing above it");
    }

    #[test]
    fn a_file_the_record_does_not_name_is_never_withdrawn_and_keeps_its_folder() {
        let (_dir, plane, tree, mut record) = mirrored_once(AGENT, "# ops\n");
        std::fs::write(tree.join(".claude/agents/mine.md"), "# mine\n").unwrap();
        let want = BTreeMap::new();

        let retired = retired_mirrors(
            &plane,
            &tree,
            &want,
            &record,
            &offered_as(&record),
            &untracked,
        );
        withdraw_mirrors(&plane, &tree, &want, &mut record, retired);

        assert!(!tree.join(AGENT).exists());
        assert_eq!(
            std::fs::read_to_string(tree.join(".claude/agents/mine.md")).unwrap(),
            "# mine\n"
        );
    }

    #[test]
    fn a_copy_edited_since_purlis_wrote_it_is_the_operators_and_stays() {
        let (_dir, plane, tree, mut record) = mirrored_once(AGENT, "# ops\n");
        std::fs::write(tree.join(AGENT), "# ops, with my notes\n").unwrap();
        let want = BTreeMap::new();

        let retired = retired_mirrors(
            &plane,
            &tree,
            &want,
            &record,
            &offered_as(&record),
            &untracked,
        );
        assert!(retired.is_empty(), "{retired:?}");
        withdraw_mirrors(&plane, &tree, &want, &mut record, retired);

        assert_eq!(
            std::fs::read_to_string(tree.join(AGENT)).unwrap(),
            "# ops, with my notes\n"
        );
        assert!(record.names(AGENT), "and its entry is not purlis's to drop");
    }

    #[test]
    fn a_copy_git_tracks_or_the_project_still_wants_stays() {
        let (_dir, plane, tree, record) = mirrored_once(AGENT, "# ops\n");

        let tracked = |rel: &str| rel == AGENT;
        assert!(
            retired_mirrors(
                &plane,
                &tree,
                &BTreeMap::new(),
                &record,
                &offered_as(&record),
                &tracked
            )
            .is_empty()
        );

        let want = BTreeMap::from([(AGENT.to_owned(), "# ops, newer\n".to_owned())]);
        assert!(
            retired_mirrors(
                &plane,
                &tree,
                &want,
                &record,
                &offered_as(&record),
                &untracked
            )
            .is_empty()
        );
    }

    #[test]
    fn a_project_file_purlis_could_not_read_is_held_and_its_copy_stays() {
        // Not text: `mirrored` skips it, so it is not wanted, and it is still THERE. A file
        // somebody is holding is not one the project stopped having.
        let (_dir, plane, tree, record) = mirrored_once(AGENT, "# ops\n");
        std::fs::create_dir_all(plane.join(".claude/agents")).unwrap();
        std::fs::write(plane.join(AGENT), [0xff_u8, 0xfe, 0x00]).unwrap();
        assert!(!want(&plane).contains_key(AGENT));

        assert!(
            retired_mirrors(
                &plane,
                &tree,
                &BTreeMap::new(),
                &record,
                &offered_as(&record),
                &untracked
            )
            .is_empty()
        );
    }

    #[test]
    fn the_generated_settings_are_not_a_mirror_and_are_never_withdrawn_here() {
        let (_dir, plane, tree, record) = mirrored_once(SETTINGS, "{}\n");

        assert!(
            retired_mirrors(
                &plane,
                &tree,
                &BTreeMap::new(),
                &record,
                &offered_as(&record),
                &untracked
            )
            .is_empty()
        );
        assert!(!names_a_mirror(&record));
        assert!(names_a_mirror(&{
            let mut r = layer::Record::new();
            r.settle(".claude/skills", digest("x"));
            r
        }));
        assert!(!mirrored_path(".claude/agentsx/a.md"));
    }

    #[cfg(unix)]
    #[test]
    fn a_mirror_reached_through_a_link_is_never_read_or_removed() {
        // The copy's digest is right on the far end of the link, which is exactly why it is
        // no evidence: a committed `.claude` that is a link out of the checkout, and a file
        // that is itself a link, both stay, and so does what they point at.
        let (dir, plane, tree, mut record) = mirrored_once(AGENT, "# ops\n");
        let outside = dir.path().join("outside");
        std::fs::create_dir_all(outside.join("agents")).unwrap();
        std::fs::write(outside.join("agents/ops.md"), "# ops\n").unwrap();
        std::fs::remove_dir_all(tree.join(".claude")).unwrap();
        std::os::unix::fs::symlink(&outside, tree.join(".claude")).unwrap();
        let want = BTreeMap::new();

        let retired = retired_mirrors(
            &plane,
            &tree,
            &want,
            &record,
            &offered_as(&record),
            &untracked,
        );
        assert!(retired.is_empty(), "{retired:?}");
        // And the unlink asks again, whatever it is handed.
        let rows = withdraw_mirrors(&plane, &tree, &want, &mut record, vec![AGENT.to_owned()]);
        assert!(rows.is_empty(), "{rows:?}");
        assert!(outside.join("agents/ops.md").exists());

        std::fs::remove_file(tree.join(".claude")).unwrap();
        std::fs::create_dir_all(tree.join(".claude/agents")).unwrap();
        std::os::unix::fs::symlink(outside.join("agents/ops.md"), tree.join(AGENT)).unwrap();
        assert!(
            retired_mirrors(
                &plane,
                &tree,
                &want,
                &record,
                &offered_as(&record),
                &untracked
            )
            .is_empty()
        );
        let rows = withdraw_mirrors(&plane, &tree, &want, &mut record, vec![AGENT.to_owned()]);
        assert!(rows.is_empty(), "{rows:?}");
        assert!(
            tree.join(AGENT).symlink_metadata().is_ok(),
            "the link stays"
        );
        assert!(outside.join("agents/ops.md").exists());
    }

    #[test]
    fn an_unwanted_mirror_already_gone_is_forgotten_so_its_line_leaves() {
        let (_dir, plane, tree, mut record) = mirrored_once(AGENT, "# ops\n");
        std::fs::remove_file(tree.join(AGENT)).unwrap();

        let rows = withdraw_mirrors(&plane, &tree, &BTreeMap::new(), &mut record, Vec::new());

        assert!(rows.is_empty());
        assert!(!record.names(AGENT));
    }
}
