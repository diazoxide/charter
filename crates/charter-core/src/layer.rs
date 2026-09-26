//! The rules charter's generated layer follows, whichever directory it is written into.
//!
//! Charter writes the plane's settings — its `env` (and so `$CHARTER_HARNESS`), its
//! `enabledPlugins` and its restrictive permission rules — into directories a harness reads
//! configuration from but which the plane's own files do not reach. There are **two such
//! targets**, and they differ only at the edges:
//!
//! | Target | Module | Extra |
//! |---|---|---|
//! | a checkout with a git root of its own | [`crate::guest`] | `.git/info/exclude`, the mirrored walk-up dirs, the machine-local document |
//! | a `workspaces/<ws>/` directory | [`crate::wslayer`] | the `.charter-structure` stamp |
//!
//! What they share is everything that decides **what charter may overwrite**, and that is
//! what lives here. It was `guest.rs`'s alone until M2.22 needed the second target;
//! copying it would have been two readers of one question, and this repository has paid
//! for that shape before — the Python's own `_harness_files`, `_layer_status` and
//! `_write_whole` each take a directory precisely so a checkout and a workspace cannot
//! drift about which files are charter's.
//!
//! # The ownership rule, in one sentence
//!
//! A path is charter's when a marker charter could have written records a digest the file
//! still has. Anything else is the operator's: never rewritten, never removed, and a wanted
//! path holding somebody else's file means the plane's rules are **not** in force there.
//!
//! # Why the record can hold a LIST
//!
//! Silence is not a verdict. Before charter rewrites a generated file it publishes that
//! file's entry as every digest the path may hold **while the write is under way** — the old
//! one and the new — and settles it to the single digest once the write has landed. A record
//! published only afterwards left, for any kill in between, the OLD digest beside the NEW
//! file: read back, that is somebody else's file, and the plane's rules quietly stop being in
//! force. So an entry is a string when it is settled and a list while it is pending, and
//! [`Record::recorded`] is the one reader of both.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::contain;

/// The sidecar recording what charter last generated in a directory, as
/// `{relative path: sha256 of the text charter wrote}`.
///
/// **A sidecar and not a key inside the vendor's JSON.** That file's schema belongs to the
/// harness; an unknown key in it is charter making a claim on somebody else's document, and
/// a validator that rejects unknown keys would turn charter's bookkeeping into a startup
/// failure.
pub const MARKER: &str = ".charter-generated";

/// The generated document carrying the plane's `env` (and so `$CHARTER_HARNESS`), its
/// `enabledPlugins`, and its shared ask/deny rules. Both targets want exactly this one.
pub const SETTINGS: &str = ".claude/settings.json";

/// The plane's own machine-local settings document, relative to the PLANE root.
pub const PLANE_LOCAL_SETTINGS: &str = ".claude/settings.local.json";

/// The plane settings keys that travel into a generated document, in this order.
///
/// Nothing else. `permissions` is the plane's decision about the plane's own root, and
/// copying a GRANT sideways into a directory nobody granted it in puts a permission in force
/// where no one clicked for it — which is why the restrictive half travels beside these keys
/// (see [`RESTRICTIVE`]) rather than among them.
const WORKSPACE_KEYS: [&str; 2] = ["enabledPlugins", "env"];

/// The `permissions` buckets that travel — never `allow`.
///
/// A restrictive rule is the opposite of a grant: `ask` adds a prompt, `deny` adds a refusal,
/// and neither can make anything run that would not have run anyway. Leaving them behind is
/// what puts a safety rule out of force in the one directory where the guarded command gets
/// typed.
const RESTRICTIVE: [&str; 2] = ["ask", "deny"];

/// The digest a marker records for one generated file.
///
/// Named and shared because the writer and the ownership test must not be two spellings of
/// "the same content": a marker written one way and read another reports every file charter
/// wrote as somebody else's, which is the direction that costs work.
pub fn digest(text: &str) -> String {
    use sha2::Digest;
    let mut hasher = sha2::Sha256::new();
    hasher.update(text.as_bytes());
    crate::extension::hex(&hasher.finalize())
}

/// Whether `key` is a name charter could have recorded: a relative path naming a file inside
/// the directory, and nothing else.
///
/// A key that is absolute, or walks up, is a key somebody else's content put there, and
/// acting on it is how a marker names a file OUTSIDE the tree as charter's to rewrite or
/// unlink.
pub fn key_ok(key: &str) -> bool {
    let path = Path::new(key);
    !key.is_empty()
        && path
            .components()
            .all(|c| matches!(c, std::path::Component::Normal(_)))
}

/// What a marker vouches for, per relative path.
///
/// Entries are kept in the file's own order-insensitive map form; the one thing that matters
/// about the order is that it is *stable*, which `BTreeMap` gives and a `HashMap` does not.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Record {
    entries: BTreeMap<String, Vec<String>>,
    /// Whether each entry was read as a settled string rather than a pending list, so a
    /// republish writes back the shape it read.
    settled: BTreeMap<String, bool>,
}

impl Record {
    /// An empty record — a directory charter has written nothing in.
    pub fn new() -> Self {
        Self::default()
    }

    /// The digests this record vouches for at `rel`: one when settled, several while pending,
    /// none when it says nothing.
    ///
    /// The Python's `_recorded`. A value that is neither a string nor a list of strings
    /// vouches for nothing, which is the conservative direction: the file reads as somebody
    /// else's and is left exactly as it is.
    pub fn recorded(&self, rel: &str) -> &[String] {
        self.entries.get(rel).map_or(&[], Vec::as_slice)
    }

    /// The single digest this record SETTLES on at `rel`, or `None` when it is pending or
    /// silent.
    ///
    /// A pending entry is deliberately not an answer here. It lists what the path may hold
    /// while a write is under way, the new text among it; read as current, an approval a
    /// harness saved over an interrupted write would settle a file that never received the
    /// plane's new `deny`.
    pub fn settled(&self, rel: &str) -> Option<&str> {
        match self.entries.get(rel)?.as_slice() {
            [one] if self.settled.get(rel).copied().unwrap_or(false) => Some(one.as_str()),
            _ => None,
        }
    }

    /// Every path this record names, in path order.
    pub fn paths(&self) -> impl Iterator<Item = &String> {
        self.entries.keys()
    }

    /// Whether this record names `rel` at all.
    pub fn names(&self, rel: &str) -> bool {
        self.entries.contains_key(rel)
    }

    /// Whether it names nothing. A record that names nothing is removed rather than published.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Settle `rel` on one digest — what charter has just written and verified.
    pub fn settle(&mut self, rel: &str, hash: String) {
        self.entries.insert(rel.to_owned(), vec![hash]);
        self.settled.insert(rel.to_owned(), true);
    }

    /// Publish `rel` as PENDING: every digest the path may hold while the write is under way,
    /// which is whatever was recorded plus what is about to be written.
    ///
    /// The list is SORTED, and that is a divergence from the Python worth stating: there it is
    /// `list(set(...))`, whose order comes out of a randomised string hash, so the same plane
    /// and the same input can write two different files. A sorted list is the same content
    /// with an order a test can pin — and a pending entry only ever reaches the disk when the
    /// write it covers did not land, which is the one case a differential scenario can catch.
    pub fn pend(&mut self, rel: &str, hash: String) {
        let mut all = self.entries.get(rel).cloned().unwrap_or_default();
        if !all.contains(&hash) {
            all.push(hash);
        }
        all.sort();
        all.dedup();
        self.entries.insert(rel.to_owned(), all);
        self.settled.insert(rel.to_owned(), false);
    }

    /// Forget `rel` — a file charter withdrew, or one an entry vouches for that is not there.
    pub fn forget(&mut self, rel: &str) {
        self.entries.remove(rel);
        self.settled.remove(rel);
    }

    /// The document this record is published as: a string for a settled entry, a list for a
    /// pending one — the two shapes charter's own writer produces and its reader accepts.
    ///
    /// **Keys come out in PATH order**, which is the second divergence from the Python worth
    /// stating. There the marker is a `dict` read from the file, so its order is whatever the
    /// file on disk had, with new keys appended — and the file on disk can have any order at
    /// all, since an older charter or a hand edit wrote it. One order, fixed by the map, is
    /// what makes the same plane and the same input write the same bytes. Nothing reads a
    /// marker positionally: [`read_record`] keys it by path, and the single document a
    /// workspace directory generates today makes the two orders identical anyway.
    pub fn document(&self) -> serde_json::Value {
        let mut doc = serde_json::Map::new();
        for (rel, all) in &self.entries {
            let settled = self.settled.get(rel).copied().unwrap_or(false);
            let value = match all.as_slice() {
                [one] if settled => serde_json::Value::String(one.clone()),
                many => serde_json::Value::Array(
                    many.iter()
                        .map(|h| serde_json::Value::String(h.clone()))
                        .collect(),
                ),
            };
            doc.insert(rel.clone(), value);
        }
        serde_json::Value::Object(doc)
    }
}

/// Charter's record in `base`: empty for one that is absent, unreadable, torn, not a JSON
/// object, or holding a key charter could not have written.
///
/// **A marker charter cannot have written is not charter's record**, and the WHOLE marker is
/// dropped rather than half of it. Charter writes a fresh one through the containment check,
/// so nothing that was really charter's is lost.
///
/// Whether a repository *commits* a `.charter-generated` — which would make it somebody's
/// content rather than charter's record — is a question about a CHECKOUT and lives in
/// [`crate::guest::tracked`]. A workspace directory is inside the plane's own repository and
/// `/workspaces/*/*` already ignores it, so there is nothing to ask there.
pub fn read_record(base: &Path) -> Record {
    let Ok(text) = std::fs::read_to_string(base.join(MARKER)) else {
        return Record::new();
    };
    parse_record(&text)
}

/// [`read_record`]'s body, for a text already in hand.
pub fn parse_record(text: &str) -> Record {
    let Ok(serde_json::Value::Object(doc)) = serde_json::from_str::<serde_json::Value>(text) else {
        return Record::new();
    };
    let mut out = Record::new();
    for (key, value) in doc {
        if !key_ok(&key) {
            return Record::new();
        }
        match value {
            serde_json::Value::String(hash) => out.settle(&key, hash),
            serde_json::Value::Array(items) => {
                // A list is a PENDING entry, and the strings in it are what it vouches for.
                // Anything else in the list vouches for nothing and is dropped, which is the
                // Python's `_recorded` verbatim: a value charter did not write cannot make a
                // file charter's.
                let all: Vec<String> = items
                    .into_iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect();
                out.entries.insert(key.clone(), all);
                out.settled.insert(key, false);
            }
            // A number, an object, `null`: not a digest charter could have recorded. The
            // entry vouches for nothing, so the path reads as somebody else's.
            _ => {}
        }
    }
    out
}

/// A plane file read as text, gated on the exact path the read opens.
///
/// The containment check is on the entry, not on the directory above it.
///
/// **Where a plane's data lands, not where the plane's data directories are.** `within_plane`
/// and **not** `contain::readable`: the latter asks whether a path lands in one of the plane's
/// DATA directories (`personas/`, `workspaces/`), which `.claude/settings.json` is not. Asking
/// it here would refuse the whole layer.
///
/// # Why this is not a `read_to_string` (charter-app#112)
///
/// It was one, and [`crate::machine::Contribution::of`] calls it against a **stranger's**
/// plane's `.claude/settings.json` to draw the approval dialog — a directory the operator has
/// just pointed at and has not yet trusted, read before the app has a window. So the three
/// hazards every other reader of a plane file in this crate already pays for apply here with
/// nothing to click on:
///
/// * **a FIFO is not a link**, so a containment check waves it through and `read_to_string`
///   on one never returns. `O_NONBLOCK` — which arrives with [`contain::open_no_link`] — is
///   what makes the open return, and [`crate::reopen::refuse_unusable`] is what makes charter
///   decline what it returned;
/// * **a planted giant** is read whole into memory before anything can reject it, and a
///   sparse multi-gigabyte file packs small in a clone. Bounded at
///   [`crate::reopen::MAX_BYTES`], which is Python's own `contain.MAX_BYTES` — and Python
///   refuses the same two shapes with `NOT_A_FILE` and `TOO_LARGE`, so this is one divergence
///   from the oracle closed rather than opened;
/// * **the gate and the open were two different resolutions of one name.** `within_plane`
///   resolved the path to decide, and `read_to_string` resolved it again to read. A link
///   planted between the two was followed — ADR 0028's measured window, 1881 escapes in
///   20,000 reads of the record before `open_no_link` closed it.
///
/// **The path that is OPENED is the resolved one**, and that is the whole of the third fix. A
/// link this function must still follow is followed *here*, by [`contain::resolved`], and what
/// the kernel is then handed is the place it landed — checked with `strip_prefix` inside
/// [`contain::no_link_on_the_way`] and opened `O_NOFOLLOW`. So the object the gate judged and
/// the object the read gets cannot be two files.
///
/// **And a contained link is still followed, deliberately.** `personas/` generators link an
/// agent into `.claude/agents/`, Python's `contain._refused` follows a link whose target is
/// inside the plane's data, and `a_plane_file_linked_from_inside_the_plane_is_still_mirrored`
/// pins it. `open_no_link` against the *unresolved* name would refuse every one of those and
/// take a persona's agent out of every worktree — which is why the resolution happens first
/// rather than the gate being pointed at the name.
pub fn readable_text(plane: &Path, path: &Path) -> Option<String> {
    use std::io::Read;

    // Both ends, for the reason `within_plane` gives: on macOS a temp plane is under
    // `/var/folders/…`, itself a link to `/private/var/…`, so a resolved path compared
    // against an unresolved root refuses everything. `None` is "charter cannot say where this
    // lands", which is a no rather than a yes.
    let root = contain::resolved(plane)?;
    let lands = contain::resolved(path)?;
    // The containment test is `no_link_on_the_way`'s own `strip_prefix`, which is the same
    // question `within_plane` asked a line earlier — asked once, of the path being opened.
    let mut open = contain::open_no_link(&root, &lands).ok()?;
    // `fstat` of the descriptor the read will use, never of the name: the two cannot be
    // handed different files.
    let found = open.metadata().ok()?;
    if crate::reopen::refuse_unusable(&lands, &found).is_err() {
        return None;
    }
    let mut text = String::new();
    // Bounded again on the way in: `refuse_unusable` asked how big it was, and a writer that
    // appends between the `fstat` and the read would otherwise still be unbounded.
    open.by_ref()
        .take(crate::reopen::MAX_BYTES)
        .read_to_string(&mut text)
        .ok()?;
    Some(text)
}

/// The plane's settings document at `rel`, read as JSON — `None` when there is none or it
/// will not parse.
///
/// **Read from the plane's own file rather than composed from charter's constants**, and that
/// is what makes this a sync rather than a second generator. A plane whose operator edited one
/// of these keys by hand would otherwise get charter's default mirrored into every target,
/// silently reverting a deliberate choice in a new place.
pub fn plane_settings(plane: &Path, rel: &str) -> Option<serde_json::Value> {
    let path = plane.join(rel);
    let text = readable_text(plane, &path)?;
    let doc: serde_json::Value = serde_json::from_str(&text).ok()?;
    doc.is_object().then_some(doc)
}

/// `{ask, deny}` out of a settings document, empty buckets dropped.
///
/// Every level is checked for its shape before it is read: `permissions` can be a string and
/// `ask` can be a number in a file a chat could have written, and a reader that assumed would
/// be a crash in a launch.
///
/// Empty buckets are dropped so a plane whose `permissions` holds nothing but grants
/// contributes no `permissions` key at all — an empty block in the generated file would read
/// as policy.
pub fn restrictive(doc: &serde_json::Value) -> serde_json::Map<String, serde_json::Value> {
    let mut out = serde_json::Map::new();
    let Some(block) = doc
        .get("permissions")
        .and_then(serde_json::Value::as_object)
    else {
        return out;
    };
    for bucket in RESTRICTIVE {
        let rules: Vec<serde_json::Value> = block
            .get(bucket)
            .and_then(serde_json::Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter(|rule| rule.is_string())
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        if !rules.is_empty() {
            out.insert(bucket.to_owned(), serde_json::Value::Array(rules));
        }
    }
    out
}

/// The generated `.claude/settings.json`: the plane's `enabledPlugins` and `env`, and its
/// shared ask/deny rules. `None` for a plane with nothing to say.
///
/// The key order is the Python's, because the file is compared byte for byte against it:
/// `enabledPlugins`, `env`, then `permissions` last.
///
/// `None` rather than `{}`: writing an empty document would look like a layer, and the honest
/// rendering of "there is nothing to mirror" is no file and no marker entry at all.
pub fn settings_document(plane: &Path) -> Option<String> {
    let settings = plane_settings(plane, SETTINGS)?;
    let mut doc = serde_json::Map::new();
    for key in WORKSPACE_KEYS {
        let Some(value) = settings.get(key) else {
            continue;
        };
        // The retired Python charter's plugin is left behind: no file charter writes enables
        // it (#374). A plane that still enables it is its own file, which `charter doctor`
        // names.
        if key == "enabledPlugins"
            && let serde_json::Value::Object(plugins) = value
        {
            let kept: serde_json::Map<String, serde_json::Value> = plugins
                .iter()
                .filter(|(id, _)| id.as_str() != crate::plugin::SUPERSEDED)
                .map(|(id, on)| (id.clone(), on.clone()))
                .collect();
            if !kept.is_empty() {
                doc.insert(key.to_owned(), serde_json::Value::Object(kept));
            }
            continue;
        }
        doc.insert(key.to_owned(), value.clone());
    }
    let rules = restrictive(&settings);
    if !rules.is_empty() {
        doc.insert("permissions".to_owned(), serde_json::Value::Object(rules));
    }
    if doc.is_empty() {
        return None;
    }
    Some(crate::pyjson::dumps_indent2(&serde_json::Value::Object(
        doc,
    )))
}

/// The generated `.claude/settings.local.json`: the plane's machine-local ask/deny rules and
/// nothing else. **A checkout's only** — see [`crate::guest`].
pub fn local_settings_document(plane: &Path) -> Option<String> {
    let local = plane_settings(plane, PLANE_LOCAL_SETTINGS)?;
    let rules = restrictive(&local);
    if rules.is_empty() {
        return None;
    }
    let mut doc = serde_json::Map::new();
    doc.insert("permissions".to_owned(), serde_json::Value::Object(rules));
    Some(crate::pyjson::dumps_indent2(&serde_json::Value::Object(
        doc,
    )))
}

/// Whether a plane file charter mirrors is one it currently cannot read — the Python's
/// `held_files`.
///
/// A plane whose `.claude/settings.json` is there but will not parse is a file somebody is
/// holding, not a plane that stopped declaring anything. Charter keeps what is already on
/// disk at the generated path: it neither rewrites it nor withdraws it. Withdrawing over a
/// typo took every workspace's `enabledPlugins`, `env` and `deny` away.
pub fn held(plane: &Path) -> bool {
    let path = plane.join(SETTINGS);
    match readable_text(plane, &path) {
        // There, and it does not parse as an object: held.
        Some(text) => {
            !serde_json::from_str::<serde_json::Value>(&text).is_ok_and(|doc| doc.is_object())
        }
        // Not there, or charter may not read it: nothing is held. `settings_document` answers
        // `None` for both, and a plane with no settings declares nothing rather than holding
        // something.
        None => path.symlink_metadata().is_ok(),
    }
}

/// Write `text` at `rel` inside `base`, whole.
///
/// **The containment check is on the exact path being opened**, before the parent directories
/// are made and again after. The first call is the one that matters for a committed `.claude`
/// that is a DANGLING link out of the tree: without it, `create_dir_all` follows the link and
/// creates the target — a directory charter made outside every plane, before any containment
/// check ever ran. The second is for the component swapped for a link between the two, which
/// narrows the window [`contain`] documents as structural.
pub fn write_into(base: &Path, rel: &str, text: &str) -> Result<(), String> {
    let path = base.join(rel);
    let parent = path.parent().ok_or_else(|| "no parent".to_owned())?;
    let linked = || {
        Err("it is reached through a symlink, and charter will not write through one".to_owned())
    };
    if contain::no_link_on_the_way(base, &path).is_err() {
        return linked();
    }
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    if contain::no_link_on_the_way(base, &path).is_err() {
        return linked();
    }
    write_whole(&path, text)
}

/// Replace the file at `path` with `text` whole — [`crate::rewrite::replace`]: a temp beside
/// it, fsynced, one rename, the directory fsynced — or leave it exactly as it was.
///
/// Written in place, a kill mid-write left a `settings.json` at 68 or 0 bytes, which reads as
/// somebody else's edit: the plane's new `deny` never arrived there, and `doctor` said all
/// current.
///
/// The temp is `.charter-generated.<name>.<pid>.<tag>.tmp` ([`crate::rewrite::TEMP_PREFIX`]) —
/// beside the target, so the rename is a rename, and private to this writer.
pub fn write_whole(path: &Path, text: &str) -> Result<(), String> {
    write_whole_io(path, text).map_err(|e| e.to_string())
}

/// [`write_whole`], keeping the errno.
///
/// Two spellings of one write, because one caller needs what the other throws away: charter
/// words what clears a record it could not publish **by the errno it failed with** — a full
/// disk and a read-only mount are not "restore write access" — and an `io::Error` rendered to
/// a string has already lost the number that decides which sentence to print.
pub fn write_whole_io(path: &Path, text: &str) -> std::io::Result<()> {
    // Gated from the file's own directory: every caller has already walked the path above it,
    // so this adds the one answer they could not hold — a link AT the file is refused rather
    // than replaced. `Kept`: a generated file keeps the mode it has.
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::other("no parent"))?;
    crate::rewrite::replace(parent, path, text.as_bytes(), crate::rewrite::Mode::Kept)
}

/// Publish `record` at `base`, or remove the marker when it names nothing.
///
/// Whole, for [`write_whole`]'s reason: truncated in place, a launch reading this marker while
/// another wire wrote it could read a prefix or nothing.
pub fn publish(base: &Path, record: &Record) -> Result<(), String> {
    publish_io(base, record).map_err(|e| e.to_string())
}

/// [`publish`], keeping the errno — see [`write_whole_io`] for why one caller needs it.
pub fn publish_io(base: &Path, record: &Record) -> std::io::Result<()> {
    let path = base.join(MARKER);
    if record.is_empty() {
        // `remove_file` never follows a symlink, so a hostile marker LINK is removed at the
        // link node, never through it — no containment needed on this branch.
        return match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
        };
    }
    if contain::no_link_on_the_way(base, &path).is_err() {
        return Err(std::io::Error::other(
            "the record is reached through a symlink",
        ));
    }
    write_whole_io(&path, &crate::pyjson::dumps_indent2(&record.document()))
}

/// Remove `dir` and its parents up to (never including) `stop`, while they are empty AND
/// resolve inside `stop`.
///
/// A `.claude/` left standing after its last generated file is withdrawn is charter still
/// visible in a directory it no longer has anything in.
///
/// The containment test also stops the walk AT `stop`: a real directory's parent never
/// resolves inside it, and a `stop` that is itself a link is refused by `remove_dir` (ENOTDIR).
pub fn prune_empty(mut dir: PathBuf, stop: &Path) {
    while contain::no_link_on_the_way(stop, &dir).is_ok() && inside(stop, &dir) {
        if std::fs::remove_dir(&dir).is_err() {
            return;
        }
        match dir.parent() {
            Some(up) => dir = up.to_path_buf(),
            None => return,
        }
    }
}

/// Whether `path` resolves strictly inside `base` — never `base` itself, which is what stops
/// [`prune_empty`] at the root it is pruning under.
///
/// charter's `_inside`, and the test [`crate::guest`] asks of a generated path BEFORE it
/// reads one: a committed `.claude` that is a directory link out of the checkout would
/// otherwise have its far end read as charter's own file, and the row would say the plane's
/// rules are in force through a link charter will not write through.
pub(crate) fn inside(base: &Path, path: &Path) -> bool {
    let (Some(root), Some(here)) = (contain::resolved(base), contain::resolved(path)) else {
        return false;
    };
    here != root && here.starts_with(&root)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The digest a marker records is plain lowercase-hex SHA-256, pinned to values computed
    /// outside this crate (`hashlib.sha256(...).hexdigest()` in Python): every marker already
    /// on disk was written this way, and a dependency bump must not respell it.
    #[test]
    fn a_marker_digest_is_the_known_sha256_of_the_text() {
        assert_eq!(
            digest(""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            digest("abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            digest("charter wrote this\n"),
            "98c856e3f0b5aedddd335e4ed9992ecc8639aad2a112b902b2d4090291d685ef"
        );
    }

    #[test]
    fn a_settled_entry_reads_back_as_the_one_digest_it_names() {
        let record = parse_record(r#"{"a.json": "abc"}"#);
        assert_eq!(record.recorded("a.json"), ["abc".to_string()]);
        assert_eq!(record.settled("a.json"), Some("abc"));
    }

    #[test]
    fn a_pending_entry_vouches_for_every_digest_and_settles_on_none() {
        let record = parse_record(r#"{"a.json": ["abc", "def"]}"#);
        assert_eq!(
            record.recorded("a.json"),
            ["abc".to_string(), "def".to_string()]
        );
        // The whole of ruling H: a pending entry lists what the path MAY hold while a write
        // is under way. Read as settled, an approval saved over an interrupted write would
        // settle a file that never received the plane's new rules.
        assert_eq!(record.settled("a.json"), None);
    }

    #[test]
    fn a_pending_list_of_one_is_still_pending() {
        let record = parse_record(r#"{"a.json": ["abc"]}"#);
        assert_eq!(record.recorded("a.json"), ["abc".to_string()]);
        assert_eq!(record.settled("a.json"), None);
    }

    #[test]
    fn a_key_that_leaves_the_directory_drops_the_whole_record() {
        for key in ["../victim", "/etc/passwd", "a/../../out", ""] {
            let text =
                serde_json::to_string(&serde_json::json!({key: "abc", "ok.json": "def"})).unwrap();
            let record = parse_record(&text);
            assert!(
                record.is_empty(),
                "a marker holding the key {key:?} must vouch for nothing at all"
            );
        }
    }

    #[test]
    fn a_value_that_is_no_digest_vouches_for_nothing_without_dropping_the_record() {
        let record = parse_record(r#"{"a.json": 7, "b.json": "abc"}"#);
        assert_eq!(record.recorded("a.json"), [] as [String; 0]);
        assert_eq!(record.settled("b.json"), Some("abc"));
    }

    #[test]
    fn a_marker_that_is_not_an_object_vouches_for_nothing() {
        for text in ["[]", "7", "null", "not json at all", ""] {
            assert!(parse_record(text).is_empty(), "{text:?}");
        }
    }

    #[test]
    fn a_pending_entry_is_published_as_a_list_and_a_settled_one_as_a_string() {
        let mut record = Record::new();
        record.settle("a.json", "abc".into());
        record.pend("b.json", "def".into());
        assert_eq!(
            record.document(),
            serde_json::json!({"a.json": "abc", "b.json": ["def"]})
        );
    }

    #[test]
    fn pending_lists_are_sorted_so_the_same_input_writes_the_same_file() {
        let mut one = Record::new();
        one.settle("a.json", "zzz".into());
        one.pend("a.json", "aaa".into());
        let mut two = Record::new();
        two.settle("a.json", "aaa".into());
        two.pend("a.json", "zzz".into());
        assert_eq!(one.document(), two.document());
        assert_eq!(
            one.document(),
            serde_json::json!({"a.json": ["aaa", "zzz"]})
        );
    }

    #[test]
    fn a_plane_with_no_settings_generates_no_document() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(settings_document(dir.path()), None);
    }

    #[test]
    fn only_the_two_keys_and_the_restrictive_buckets_travel() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".claude")).unwrap();
        std::fs::write(
            dir.path().join(SETTINGS),
            r#"{"env":{"CHARTER_HARNESS":"claude-code"},"enabledPlugins":{"p":true},
                "hooks":{"PreToolUse":[]},
                "permissions":{"allow":["Bash(rm *)"],"ask":["Bash(x *)"],"deny":["Bash(y *)"]}}"#,
        )
        .unwrap();
        let doc = settings_document(dir.path()).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&doc).unwrap();
        assert_eq!(
            parsed,
            serde_json::json!({
                "enabledPlugins": {"p": true},
                "env": {"CHARTER_HARNESS": "claude-code"},
                "permissions": {"ask": ["Bash(x *)"], "deny": ["Bash(y *)"]},
            })
        );
        // `allow` is the one bucket that can put something in force where nobody clicked for
        // it, and `hooks` is not charter's to copy sideways.
        assert!(!doc.contains("allow"), "{doc}");
        assert!(!doc.contains("hooks"), "{doc}");
        // The order the file is compared byte for byte against.
        let plugins = doc.find("enabledPlugins").unwrap();
        let env = doc.find("\"env\"").unwrap();
        let perms = doc.find("permissions").unwrap();
        assert!(plugins < env && env < perms, "{doc}");
    }

    #[test]
    fn a_plane_settings_file_that_is_a_link_out_of_the_plane_is_not_read() {
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(
            outside.path().join("settings.json"),
            r#"{"env":{"SECRET":"1"}}"#,
        )
        .unwrap();
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".claude")).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(
            outside.path().join("settings.json"),
            dir.path().join(SETTINGS),
        )
        .unwrap();
        assert_eq!(settings_document(dir.path()), None);
    }

    #[test]
    fn a_write_through_a_directory_link_out_of_the_tree_is_refused() {
        let outside = tempfile::tempdir().unwrap();
        let dir = tempfile::tempdir().unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(outside.path(), dir.path().join(".claude")).unwrap();
        // The EXACT path opened is `<dir>/.claude/settings.json`, and the link is at
        // `.claude` — a component ON THE WAY. Gating the parent alone would pass this.
        let refused = write_into(dir.path(), SETTINGS, "{}\n");
        assert!(refused.is_err(), "{refused:?}");
        assert!(
            !outside.path().join("settings.json").exists(),
            "charter wrote through the link"
        );
    }

    #[test]
    fn a_write_through_a_leaf_link_out_of_the_tree_is_refused() {
        let outside = tempfile::tempdir().unwrap();
        let victim = outside.path().join("settings.json");
        std::fs::write(&victim, "PRECIOUS\n").unwrap();
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".claude")).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&victim, dir.path().join(SETTINGS)).unwrap();
        assert!(write_into(dir.path(), SETTINGS, "{}\n").is_err());
        assert_eq!(std::fs::read_to_string(&victim).unwrap(), "PRECIOUS\n");
    }

    #[test]
    fn a_dangling_directory_link_is_not_created_by_the_write() {
        let outside = tempfile::tempdir().unwrap();
        let never = outside.path().join("never-made");
        let dir = tempfile::tempdir().unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&never, dir.path().join(".claude")).unwrap();
        assert!(write_into(dir.path(), SETTINGS, "{}\n").is_err());
        assert!(!never.exists(), "charter made a directory outside the tree");
    }

    /// The check BEFORE `create_dir_all` runs, which the test above does NOT pin — measured
    /// by deleting it and watching this one go red while that one stayed green.
    ///
    /// For a DANGLING link `create_dir_all` fails on its own (`mkdir` answers EEXIST, and the
    /// name does not resolve to a directory), so the second check never has to speak. The
    /// shape that needs the first check is a link to a directory that IS there and a rel with
    /// a directory of its own to make: without it, `create_dir_all` walks the link and makes
    /// that directory inside somebody else's tree before any containment test has run, and
    /// the refusal afterwards no longer undoes it.
    #[cfg(unix)]
    #[test]
    fn no_directory_is_made_outside_the_tree_on_the_way_to_a_refused_write() {
        let outside = tempfile::tempdir().unwrap();
        let dir = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), dir.path().join(".claude")).unwrap();

        assert!(write_into(dir.path(), ".claude/agents/steward.md", "mine\n").is_err());

        assert!(
            !outside.path().join("agents").exists(),
            "charter made a directory inside somebody else's tree on the way to a write it \
             then refused"
        );
        assert!(!outside.path().join("agents").join("steward.md").exists());
    }

    #[test]
    fn a_marker_reached_through_a_link_is_not_published_through() {
        let outside = tempfile::tempdir().unwrap();
        let victim = outside.path().join("record.json");
        std::fs::write(&victim, "PRECIOUS\n").unwrap();
        let dir = tempfile::tempdir().unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&victim, dir.path().join(MARKER)).unwrap();
        let mut record = Record::new();
        record.settle("a.json", "abc".into());
        assert!(publish(dir.path(), &record).is_err());
        assert_eq!(std::fs::read_to_string(&victim).unwrap(), "PRECIOUS\n");
    }

    #[test]
    fn a_record_that_names_nothing_removes_the_marker_at_the_link_node() {
        let outside = tempfile::tempdir().unwrap();
        let victim = outside.path().join("record.json");
        std::fs::write(&victim, "PRECIOUS\n").unwrap();
        let dir = tempfile::tempdir().unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&victim, dir.path().join(MARKER)).unwrap();
        assert!(publish(dir.path(), &Record::new()).is_ok());
        assert!(dir.path().join(MARKER).symlink_metadata().is_err());
        // Removed AT the link node, never through it.
        assert_eq!(std::fs::read_to_string(&victim).unwrap(), "PRECIOUS\n");
    }

    #[test]
    fn a_plane_whose_settings_will_not_parse_is_holding_them() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".claude")).unwrap();
        std::fs::write(dir.path().join(SETTINGS), "{ not json").unwrap();
        assert!(held(dir.path()));
        assert_eq!(settings_document(dir.path()), None);
    }

    #[test]
    fn a_plane_with_no_settings_file_holds_nothing() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!held(dir.path()));
    }

    #[test]
    fn prune_stops_at_the_directory_it_is_pruning_under() {
        let dir = tempfile::tempdir().unwrap();
        let deep = dir.path().join(".claude").join("agents");
        std::fs::create_dir_all(&deep).unwrap();
        prune_empty(deep, dir.path());
        assert!(!dir.path().join(".claude").exists());
        // Never the root itself, whatever the walk does above it.
        assert!(dir.path().exists());
    }

    #[test]
    fn prune_leaves_a_directory_that_still_holds_something() {
        let dir = tempfile::tempdir().unwrap();
        let deep = dir.path().join(".claude").join("agents");
        std::fs::create_dir_all(&deep).unwrap();
        std::fs::write(dir.path().join(".claude").join("keep.md"), "mine\n").unwrap();
        prune_empty(deep.clone(), dir.path());
        assert!(!deep.exists());
        assert!(dir.path().join(".claude").join("keep.md").exists());
    }
}
