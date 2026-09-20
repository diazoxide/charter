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
//! [`crate::wiring`] refuses a chat whose harness config folder charter cannot finish wiring,
//! and that is right *there*: a Codex home needs a person to approve hooks inside a session,
//! so charter cannot finish it alone and a pass would be a lie. Here charter **can** finish
//! alone — it cut the tree, and the layer is files in a directory charter owns. So charter
//! writes it, and keeps the refusal for the cases where the write does not land. What an
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
//! A `.charter-generated` git **tracks** is not charter's record at all. Charter's own is
//! per-checkout and untracked, so a tracked one is content some cloned repository committed,
//! and trusting its digests is how a repo names charter's files as its own to redirect.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::layer::{self, digest, write_into, write_whole};

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
pub const EXCLUDE_BEGIN: &str = "# >>> charter (generated layer — `charter workspace reinit`) >>>";
pub const EXCLUDE_END: &str = "# <<< charter <<<";

/// The lines inside the block, for the person who finds them in a repo they own.
const EXCLUDE_NOTE: [&str; 3] = [
    "# Files charter generated in this checkout so a chat here gets the plane's layer.",
    "# Listed one by one — charter hides only what it wrote, never a directory of yours.",
    "# This file is per-checkout and never committed; nothing your teammates clone is affected.",
];

/// Every temp [`write_whole`] writes through, as a pattern. Unanchored, because a temp is
/// written beside every file charter writes, and listed before the first one exists so a temp
/// a kill leaves behind stays hidden.
const TEMP_PATTERN: &str = ".charter-generated.*.tmp";

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
fn failed(status: Status) -> bool {
    matches!(status, Status::Foreign | Status::Blocked)
}

impl Wired {
    /// Whether the plane's layer is in force in this tree now.
    ///
    /// Every wanted path is charter's and current, and the block hides them. `Theirs` counts:
    /// a machine-local file the harness rewrote is still a file whose rules the harness reads.
    pub fn complete(&self) -> bool {
        matches!(self.hidden, Hidden::InPlace) && !self.rows.iter().any(|r| failed(r.status))
    }

    /// The one sentence saying why the layer is not in force, or empty when it is.
    ///
    /// Names the path and the repair, because a refusal an operator cannot act on is a
    /// refusal they will route around.
    pub fn refusal(&self, tree: &Path) -> String {
        let shown = crate::shown::readable(&tree.display().to_string(), usize::MAX);
        if let Hidden::Blocked(why, fix) = &self.hidden {
            return format!(
                "charter could not hide its own files in {shown} ({why}), so it wrote none of \
                 them — a chat there would run without the plane's ask/deny rules, its persona \
                 agents and $CHARTER_HARNESS. {fix}"
            );
        }
        let found = self.rows.iter().find(|r| failed(r.status));
        let Some(bad) = found else {
            return String::new();
        };
        let rel = crate::shown::short(&bad.rel);
        match bad.status {
            Status::Foreign => format!(
                "{rel} in {shown} is a file charter did not write, so the plane's layer is not \
                 in force there and charter will not overwrite it. Move it aside and start the \
                 chat again, or start this chat in the clone."
            ),
            _ => format!(
                "charter could not write {rel} in {shown} ({}), so a chat there would run \
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
/// `.charter-generated` — which would make it somebody's content rather than charter's record
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
/// **The block first, then the files.** Nothing is written until every path charter is about
/// to own is hidden, so a checkout whose `info/exclude` charter cannot write gets no files at
/// all rather than untracked noise in somebody else's `git status`.
///
/// `tree` is the caller's to have confined already —
/// [`crate::worktree::confine::within_workspace`] for a piece. What this function confines is
/// each path **below** it, at the moment it is opened, because a committed `.claude` that is
/// a directory symlink would otherwise send the write wherever the link points.
pub fn wire(plane: &Path, tree: &Path) -> Wired {
    let want = want(plane);
    if want.is_empty() {
        // Nothing to write and nothing to hide. Not a blocked layer and not an incomplete
        // one: a plane with no settings and no agents has no layer to carry.
        return Wired {
            rows: Vec::new(),
            hidden: Hidden::InPlace,
            block: Block::Untouched,
        };
    }
    // A `.charter-generated` this repository COMMITS is the one case where charter cannot
    // keep its promise. Charter's record is per-checkout and untracked, so a tracked one is
    // content somebody committed — and charter publishing its own over it would show as a
    // modified TRACKED file, which no `info/exclude` line can hide. So nothing is written and
    // the tree is reported blocked, rather than charter dirtying a repo it is a guest in.
    if tree.join(MARKER).exists() && tracked(tree, MARKER) {
        return Wired {
            rows: Vec::new(),
            hidden: Hidden::Blocked(
                format!(
                    "this repository commits a {MARKER}, and charter's own is per-checkout and \
                     never committed — writing over it would change a tracked file"
                ),
                format!(
                    "Stop committing {MARKER} in that repository, or start this chat somewhere \
                     charter is not a guest."
                ),
            ),
            block: Block::Untouched,
        };
    }
    let record = marker_at(tree);
    let mut plan: Vec<(String, Plan)> = Vec::new();
    for (rel, text) in &want {
        plan.push((rel.clone(), planned(tree, rel, text, &record)));
    }

    // Every path charter would own once this pass is done, plus what its record already
    // names: a line for a file charter wrote before is kept, so a second tree wiring this
    // same shared exclude cannot drop it.
    let mut hide: BTreeSet<String> = plan
        .iter()
        .filter(|(_, p)| !matches!(p, Plan::Foreign))
        .map(|(rel, _)| rel.clone())
        .collect();
    hide.extend(record.keys().cloned());
    if hide.is_empty() {
        // Charter owns nothing here and is about to own nothing: every wanted path holds
        // somebody else's file. A block naming only charter's own marker would then be a
        // write into a repository charter has nothing in — so there is none, and the rows
        // say why a chat is refused.
        let rows = plan.into_iter().map(|(rel, _)| row(rel)).collect();
        return Wired {
            rows,
            hidden: Hidden::InPlace,
            block: Block::Untouched,
        };
    }
    hide.insert(MARKER.to_owned());

    let hid = match block(tree, &hide) {
        Ok(hid) => hid,
        Err(why) => {
            return Wired {
                rows: Vec::new(),
                hidden: Hidden::Blocked(
                    why,
                    "Restore write access to that checkout's info/exclude and try again."
                        .to_owned(),
                ),
                block: Block::Untouched,
            };
        }
    };

    let mut rows = Vec::new();
    let mut wrote: BTreeMap<String, String> = BTreeMap::new();
    for (rel, what) in plan {
        let text = want.get(&rel).cloned().unwrap_or_default();
        let mut done = row(rel.clone());
        match what {
            Plan::Current => {
                wrote.insert(rel, digest(&text));
                done.status = Status::Current;
            }
            Plan::Theirs => {
                // Kept in the record so its line stays: the harness saving an approval moves
                // the digest without making the file anybody else's.
                if let Some(had) = record.get(&rel) {
                    wrote.insert(rel, had.clone());
                }
                done.status = Status::Theirs;
            }
            // `row` already says foreign, which is the state no write changes.
            Plan::Foreign => {}
            Plan::Create | Plan::Refresh => match write_into(tree, &rel, &text) {
                Ok(()) => {
                    wrote.insert(rel, digest(&text));
                    done.status = if what == Plan::Create {
                        Status::Created
                    } else {
                        Status::Refreshed
                    };
                }
                Err(why) => {
                    done.status = Status::Blocked;
                    done.why = why;
                }
            },
        }
        rows.push(done);
    }

    // Anything the record named that this plane no longer wants keeps its digest, so a line
    // for a file that is still on disk is not dropped by a plane that stopped generating it.
    for (rel, hash) in &record {
        wrote.entry(rel.clone()).or_insert_with(|| hash.clone());
    }
    if let Err(why) = publish(tree, &wrote) {
        // A record charter could not publish is a layer charter cannot vouch for next time:
        // every file it just wrote would read as somebody else's on the next wire. Reported
        // as a blocked row, which is a refusal, rather than left to be discovered then.
        let mut lost = row(MARKER.to_owned());
        lost.status = Status::Blocked;
        lost.why = why;
        rows.push(lost);
    }
    Wired {
        rows,
        hidden: Hidden::InPlace,
        block: hid,
    }
}

/// Publish charter's record at `tree`, or remove it when it names nothing.
///
/// Every entry this module writes is SETTLED: a checkout's write is not made at all unless
/// its exclude line is in place first, so there is no window here for the pending shape
/// [`layer::Record`] carries for the other target.
fn publish(tree: &Path, wrote: &BTreeMap<String, String>) -> Result<(), String> {
    let mut record = layer::Record::new();
    for (rel, hash) in wrote {
        record.settle(rel, hash.clone());
    }
    layer::publish(tree, &record)
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

/// What charter will do with one wanted path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Plan {
    /// Not there at all: charter's to write.
    Create,
    /// There with the content charter's record names, and the plane has moved on: charter's
    /// own file to bring up to date.
    Refresh,
    Current,
    /// The harness's own edit of a file charter generated.
    Theirs,
    /// Somebody else's file.
    Foreign,
}

fn planned(tree: &Path, rel: &str, text: &str, record: &BTreeMap<String, String>) -> Plan {
    let path = tree.join(rel);
    let Ok(on_disk) = std::fs::read_to_string(&path) else {
        // Absent, a directory, unreadable, or not text. `symlink_metadata` tells the first
        // from the rest: a path that is THERE and charter cannot read is not one charter may
        // replace, and reading a dangling link as "absent" would write through it.
        return if path.symlink_metadata().is_err() {
            Plan::Create
        } else {
            Plan::Foreign
        };
    };
    if on_disk == text {
        return Plan::Current;
    }
    match record.get(rel) {
        // Charter wrote it and nobody has touched it since: the plane moved, so charter
        // rewrites its own file.
        Some(had) if *had == digest(&on_disk) => Plan::Refresh,
        // A path the harness writes into itself, that charter's record still names.
        Some(_) if COWRITTEN.contains(&rel) => Plan::Theirs,
        _ => Plan::Foreign,
    }
}

/// The real git directory of the checkout at `root`, or `None` when there is none.
///
/// `<root>/.git` is a DIRECTORY in a clone and a FILE reading `gitdir: <path>` in a linked
/// worktree. Treating the second as a directory does not fail loudly — `create_dir_all` would
/// happily make `.git/info/` beside the `.git` file's parent, and git reads none of it.
fn git_dir(root: &Path) -> Option<PathBuf> {
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

/// Put every path in `rels` into charter's block in the checkout's own `info/exclude`.
///
/// **A line is only ever added.** A clone and its linked worktrees read ONE `info/exclude`,
/// so a piece that rewrote the block to its own list alone would drop the clone's lines — and
/// measured in the Python's own history, that dropped the line for charter's
/// `.claude/settings.json`, the plane's rules and `env`, into somebody else's repository.
/// Removing a line belongs to an unwire, which is not this milestone's.
///
/// The block is replaced rather than appended, which is the whole of the idempotence
/// requirement: appending would duplicate every line on the second wire, and a wire runs on
/// every launch, so the operator's `info/exclude` would grow without bound while their
/// `git status` stayed clean.
fn block(tree: &Path, rels: &BTreeSet<String>) -> Result<Block, String> {
    let Some(path) = exclude_file(tree) else {
        return Err(format!(
            "{} has no git directory charter can find",
            crate::shown::short(&tree.display().to_string())
        ));
    };
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e.to_string()),
    };
    let mut need: BTreeSet<String> = already(&text);
    need.extend(rels.iter().cloned());
    need.insert(MARKER.to_owned());
    need.insert(TEMP_PATTERN.to_owned());

    let new = replace_block(&text, &rendered(&need));
    if new == text {
        return Ok(Block::Present);
    }
    // Which of the two writes this is, read off the text BEFORE it changes: a block that was
    // not there is `created` and one whose lines moved is `refreshed`, which is the word
    // charter's own report uses and the only thing that tells an operator whether their
    // files have just become hidden or already were.
    let had = text.lines().any(|line| line == EXCLUDE_BEGIN);
    std::fs::create_dir_all(path.parent().expect("info/exclude has a parent"))
        .map_err(|e| e.to_string())?;
    write_whole(&path, &new)?;
    Ok(if had {
        Block::Refreshed
    } else {
        Block::Created
    })
}

/// What charter's block in `text` lists now — the temp pattern among them, in either
/// spelling.
fn already(text: &str) -> BTreeSet<String> {
    let lines: Vec<&str> = text.lines().collect();
    let Some((begin, after)) = span(&lines) else {
        return BTreeSet::new();
    };
    lines[begin + 1..after]
        .iter()
        .filter(|line| line.starts_with('/') || **line == TEMP_PATTERN)
        .map(|line| line.trim_start_matches('/').to_owned())
        .collect()
}

/// The block listing `rels`: everything sorted, then the marker, then the temp pattern.
fn rendered(rels: &BTreeSet<String>) -> String {
    let mut lines: Vec<String> = vec![EXCLUDE_BEGIN.to_owned()];
    lines.extend(EXCLUDE_NOTE.iter().map(|l| (*l).to_owned()));
    for rel in rels {
        if rel != MARKER && rel != TEMP_PATTERN {
            lines.push(format!("/{rel}"));
        }
    }
    lines.push(format!("/{MARKER}"));
    lines.push(TEMP_PATTERN.to_owned());
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
    let begin = lines.iter().position(|l| *l == EXCLUDE_BEGIN)?;
    let after = lines[begin + 1..]
        .iter()
        .position(|l| *l == EXCLUDE_END)
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

    #[test]
    fn the_block_lists_the_marker_and_the_temp_pattern_last() {
        // The order the Python writes, which is what a plane wired by both implementations
        // has to see: everything sorted, then the marker, then the unanchored temp glob.
        let mut rels = BTreeSet::new();
        rels.insert(SETTINGS.to_owned());
        rels.insert(".claude/agents/steward.md".to_owned());

        let block = rendered(&rels);
        let lines: Vec<&str> = block.lines().collect();

        assert_eq!(lines[0], EXCLUDE_BEGIN);
        assert_eq!(
            lines[4..].to_vec(),
            vec![
                "/.claude/agents/steward.md",
                "/.claude/settings.json",
                "/.charter-generated",
                ".charter-generated.*.tmp",
                EXCLUDE_END,
            ]
        );
    }

    #[test]
    fn replacing_the_block_twice_leaves_one_block() {
        let mut rels = BTreeSet::new();
        rels.insert(SETTINGS.to_owned());
        let theirs = "# mine\n/build\n";

        let once = replace_block(theirs, &rendered(&rels));
        let twice = replace_block(&once, &rendered(&rels));

        assert_eq!(once, twice, "a wire runs on every launch");
        assert_eq!(once.matches(EXCLUDE_BEGIN).count(), 1);
        assert!(once.starts_with("# mine\n/build\n"), "{once}");
    }

    #[test]
    fn an_unterminated_block_is_replaced_rather_than_doubled() {
        let mut rels = BTreeSet::new();
        rels.insert(SETTINGS.to_owned());
        let torn = format!("# mine\n{EXCLUDE_BEGIN}\n/.claude/settings.json\n");

        let fixed = replace_block(&torn, &rendered(&rels));

        assert_eq!(fixed.matches(EXCLUDE_BEGIN).count(), 1, "{fixed}");
        assert!(fixed.contains(EXCLUDE_END), "{fixed}");
    }

    #[test]
    fn a_file_with_nothing_of_charters_is_handed_back_unchanged_when_there_is_nothing_to_add() {
        let theirs = "# mine\n/build\n";

        assert_eq!(replace_block(theirs, ""), theirs);
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
}
