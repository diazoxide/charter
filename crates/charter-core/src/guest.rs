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
        Status::Foreign | Status::Blocked | Status::Unrecorded
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
                "charter could not hide its own files in {shown} ({why}), so it wrote none of \
                 them — a chat there would run without the plane's ask/deny rules, its persona \
                 agents and $CHARTER_HARNESS. {fix}"
            );
        }
        let found = self.rows.iter().find(|r| failed(r.status));
        let Some(bad) = found else {
            if self.block == Block::Blocked {
                return format!(
                    "charter wrote its files in {shown} and could not settle the block that \
                     hides them, so some of them show in that repository's own `git status`. \
                     Restore write access to its .git/info/exclude and run `charter workspace \
                     reinit`."
                );
            }
            return String::new();
        };
        let rel = crate::shown::short(&bad.rel);
        match bad.status {
            Status::Foreign => format!(
                "{rel} in {shown} is a file charter did not write, so the plane's layer is not \
                 in force there and charter will not overwrite it. Move it aside and start the \
                 chat again, or start this chat in the clone."
            ),
            Status::Unrecorded => format!(
                "charter could not publish its record in {shown} first ({}), so it wrote \
                 nothing and kept every exclude line it had — a chat there would run without \
                 the plane's layer.",
                bad.why
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
/// ?? .charter-generated
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
pub fn wire(plane: &Path, tree: &Path) -> Wired {
    // One listing per repository for the length of this wire, even when the caller did not
    // open a block: a wire asks the exclude's question twice over by construction.
    let _answers = crate::worktree::listing::answers();
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
    let record = layer::read_record(tree);
    let plan: Vec<(String, Plan)> = want
        .iter()
        .map(|(rel, text)| (rel.clone(), planned(tree, rel, text, &record)))
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

    let (first, left) = if first_pass.is_empty() {
        (Wrote::Present, BTreeSet::new())
    } else {
        let mut rels: Vec<String> = first_pass.into_iter().collect();
        rels.push(MARKER.to_owned());
        register_excludes(plane, tree, &rels, false)
    };
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
    let owned = charter_owned(tree, &layer::read_record(tree));
    let (second, _) = register_excludes(plane, tree, &owned, false);
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
}

fn planned(tree: &Path, rel: &str, text: &str, record: &layer::Record) -> Plan {
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
    // Only content a record LISTS is charter's to overwrite, pending or settled.
    if record.recorded(rel).contains(&digest(&on_disk)) {
        return Plan::Refresh;
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
        if COWRITTEN.contains(&rel.as_str()) && planned(tree, rel, body, &record) == Plan::Create {
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
    let Some(code) = e.raw_os_error() else {
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
    let said = e.to_string();
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
                "{} has no git directory charter can find",
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
    let had = text.lines().any(|line| line == EXCLUDE_BEGIN);
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
    // Never the marker: an untracked `.charter-generated` is charter's even where charter
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
                "charter's {rel} is not {} there, because {theirs} is an untracked file charter \
                 did not write and the line hiding charter's would hide it too, through the {} \
                 both checkouts read{} — commit or move {theirs}, and the next `charter \
                 workspace reinit` {} charter's",
                if local { "written" } else { "hidden" },
                exclude.display(),
                if local {
                    " — and a machine-local file charter cannot hide is one `git add` from \
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
            let there = if rel == TEMP_PATTERN {
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
    crate::worktree::listing::exists(&t.join(MARKER)) != Some(false)
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
                    if name.starts_with(".charter-generated.") && name.ends_with(".tmp") {
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
        .filter(|line| line.starts_with('/') || **line == TEMP_PATTERN)
        .map(|line| line.trim_start_matches('/').to_owned())
        .collect()
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
        lines.push(if rel == TEMP_PATTERN {
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
                "/.charter-generated",
                ".charter-generated.*.tmp",
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
                "/.charter-generated",
                ".charter-generated.*.tmp",
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
            planned(&tree, LOCAL_SETTINGS, want, &current),
            Plan::HarnessEdited
        );

        // The plane has moved on since: charter will not merge into a file the harness
        // keeps, so the plane's newer rules are NOT in force there and the row says so.
        let mut behind = layer::Record::new();
        behind.settle(LOCAL_SETTINGS, digest("something older"));
        assert_eq!(planned(&tree, LOCAL_SETTINGS, want, &behind), Plan::Theirs);

        // A pending entry is not a settled one: an approval saved over an interrupted write
        // must not settle a file that never received the plane's new `deny`.
        let mut pending = layer::Record::new();
        pending.pend(LOCAL_SETTINGS, digest(want));
        assert_eq!(planned(&tree, LOCAL_SETTINGS, want, &pending), Plan::Theirs);
    }

    #[test]
    fn a_generated_path_that_cannot_be_read_is_never_taken_for_one_charter_may_write() {
        use std::os::unix::fs::PermissionsExt;

        let (_dir, _plane, tree) = plane_with("svc");
        let claude = tree.join(".claude");
        std::fs::create_dir_all(&claude).unwrap();
        std::fs::write(tree.join(SETTINGS), "theirs\n").unwrap();
        std::fs::set_permissions(&claude, std::fs::Permissions::from_mode(0o000)).unwrap();

        let what = planned(&tree, SETTINGS, "{}\n", &layer::Record::new());
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
            planned(&tree, SETTINGS, "{}\n", &layer::Record::new()),
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
                    "charter wrote its files in /w/alpha/.worktrees/app/fix and could not settle \
                     the block"
                )
        );

        let unrecorded = row_of(".charter-generated", Status::Unrecorded, "disk full");
        assert_eq!(
            layer_of(vec![fine.clone(), unrecorded], Block::Present).refusal(tree),
            "charter could not publish its record in /w/alpha/.worktrees/app/fix first (disk \
             full), so it wrote nothing and kept every exclude line it had — a chat there would \
             run without the plane's layer."
        );
        let blocked = row_of(".claude/settings.json", Status::Blocked, "read-only");
        assert_eq!(
            layer_of(vec![blocked], Block::Present).refusal(tree),
            "charter could not write .claude/settings.json in /w/alpha/.worktrees/app/fix \
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
        let run = crate::worktree::git::run(dir, args, crate::worktree::git::READ).unwrap();
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
}
