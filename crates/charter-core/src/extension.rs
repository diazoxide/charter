//! The extension registry: what has contributed what to this window, and what the operator
//! agreed to.
//!
//! **This is charter ADR 0041's item 2 — "an extension registry with no executor" — and there
//! is no executor here.** Nothing in this module spawns a process, opens a socket or evaluates
//! anything. It answers three questions and no others: what an extension *is*, what this
//! machine has installed, and what the operator approved each one to contribute. The
//! subprocess runtime that 0041 recommends is a later stage and will be built *against* this
//! list rather than inventing one under pressure.
//!
//! # It is called an extension, and never a plugin
//!
//! ADR 0041 found the collision and it is not cosmetic. `layer::WORKSPACE_KEYS` is
//! `["enabledPlugins", "env"]`, and `enabledPlugins` is **Claude Code's** plugin list, which a
//! plane carries and which [`crate::machine::Contribution`] fingerprints and shows in the
//! first-open dialog. A charter extension would appear in that same dialog. A consent surface
//! that says *this project enables 3 plugins* above *this machine has 2 plugins installed*,
//! meaning two unrelated things, is a surface that has stopped being read. So charter's own
//! thing is an **extension**, here, in the record on disk, in the Tauri commands and in the
//! dialog.
//!
//! # An extension does not travel in a plane
//!
//! ADR 0022's argument, one level up. A harness profile is machine-local because a committed
//! file must not decide how a chat launches; an extension that contributed a palette command
//! would run on a click, with even less in front of it. So there is no `[extension.<name>]`
//! table in `charter.toml`, this module never reads a plane, and a project cannot bring one
//! with it. That also settles the collision above by construction: the two uses of the word
//! can never appear in one list, because only one of them can come out of a plane.
//!
//! # Where the record lives, and why ADR 0034 needs no amendment
//!
//! Beside [`crate::machine::FILE`], not inside it: `$CHARTER_CONFIG_HOME/charter/` (else
//! `$XDG_CONFIG_HOME`, else `~/.config`) holds `machine.json` and, next to it,
//! [`RECORD`]. charter ADR 0034's rule is "four things, and nothing else" and ADR 0040 already
//! argued it to five; the whole value of that limit is that the fifth had to be argued for,
//! and a sixth key appended to the same file without an argument would spend it. A separate
//! file in the same directory is a different claim: `machine.json` still holds exactly what
//! 0034 and 0040 say it holds, and the extension list stands or falls on its own.
//!
//! It is the same directory, so it inherits the same guards rather than a second copy of them
//! — `0700` on the directory (created at the mode, never chmod-ed into it),
//! `0600` on the file, the write landing on a `create_new` temp file that the containment walk
//! gated, and the `rename` over the name. [`crate::machine::config_root`] is what resolves the
//! directory, so the plane fence (charter-app#129, [`crate::fence::Act::Store`]) holds here
//! too without this module knowing about it.
//!
//! # Windows refuses rather than degrades
//!
//! `chmod 0o755` leaves a file at `0o666` on Windows (measured, charter-app#98), so the
//! `0600`/`0700` this record depends on has no expression there, and charter ADR 0031 settles
//! what to do: refuse. Every entry point that touches the record answers
//! [`io::ErrorKind::Unsupported`], exactly as [`crate::machine`] does, and charter keeps no
//! extension list at all on that platform. A world-writable list of "which programs the
//! operator approved" is not a degraded version of this record; it is the attack it exists to
//! stop.
//!
//! # Every unreadable state means ask
//!
//! [`crate::profiletrust`]'s opening rule, word for word, because this record decides the same
//! kind of thing. A missing file, a malformed one, an entry that is not a fingerprint, a link,
//! a FIFO, a planted giant: each reads as **no approval**, never as one. Treating silence as a
//! yes is the one state this module exists to keep out.
//!
//! The record is nonetheless **never overwritten when charter could not read it**
//! ([`approve`]), which is [`crate::machine::update`]'s rule and not `profiletrust`'s. The two
//! are not in tension and the split is deliberate: "could not read" must not become "approved"
//! (so the asking is fail-closed), and it must not become "your list is gone" either (so the
//! writing is fail-closed as well). `profiletrust` can clobber because losing its record costs
//! one extra prompt; losing this one costs the operator every extension they installed.
//!
//! # And it is not a boundary
//!
//! ADR 0041's honesty paragraph is the reason [`RUNS_AS_YOU`] is a constant in this file and
//! is carried into the dialog rather than written out again there. **A subprocess does not
//! confine an extension below the operator.** When the executor lands it will run as the same
//! user, with the same filesystem and the same ability to `exec`; it will be able to read
//! `.charter/vaults/`, write `machine.json` and edit `charter.local.toml` without asking
//! charter for anything. What this module records is **what charter will do on an extension's
//! behalf** — its own conduct — and a consent surface that implied a cage would manufacture
//! confidence charter cannot back. That is why the honest sentence is data owned by the core,
//! shipped to the window with the question, and pinned by a test.
//!
//! # What is *in* the vocabulary today
//!
//! Themes, and nothing else. A theme is declarative data against a closed vocabulary charter
//! owns (`app/src/theme/theme.ts`'s `TOKENS`), its values are hex and only hex, charter chooses
//! the consumer, and charter parses and re-emits rather than interpolating — ADR 0041's four
//! properties, all four of which the theme already had before this module existed. Wiring the
//! one extension point that has **nothing to isolate** through the registry is how the registry
//! is proved without an executor behind it.
//!
//! A manifest may also *declare* a program ([`Manifest::program`]). It is hashed into the
//! fingerprint, it is named in the prompt, and **nothing here runs it** — `charter_runs_nothing`
//! pins that. It is in the vocabulary now rather than later for one reason: ADR 0041's
//! difference from ADR 0035 is that *this fingerprints code*, and a fingerprint tested only over
//! JSON is a fingerprint nobody has seen do its job.

use std::collections::BTreeMap;
use std::io;
use std::path::{Component, Path, PathBuf};

/// The registry, inside [`crate::machine::DIR`] and beside [`crate::machine::FILE`].
pub const RECORD: &str = "extensions.json";

/// The one file that makes a directory an extension.
pub const MANIFEST: &str = "charter-extension.json";

/// The one version of the record charter writes and reads. Any other version reads as an
/// unreadable record — which asks about everything and refuses to clobber — because there is
/// no migration that would be honest about an approval recorded under rules this charter does
/// not know.
pub const VERSION: u32 = 1;

/// The themes charter itself contributes. They are compiled into the window's bundle
/// (`app/src/theme/charter-dark.json`, `charter-light.json`) and are in the survey so that
/// "what has contributed what" has one answer rather than one per source.
///
/// `app/src/theme/registry.test.ts` reads this array out of this file and fails if the window
/// ships a built-in it does not name. Two lists that must agree, with the agreement tested,
/// because the alternative is a registry that quietly stops being the whole answer.
pub const BUILT_IN_THEMES: [&str; 2] = ["charter-dark", "charter-light"];

/// The most a manifest may be. It is one small object, it is read whole, and it is read on the
/// path that draws the window — so a planted giant is a launch that never finishes.
const MOST_MANIFEST_BYTES: u64 = 64 << 10;

/// The most any one declared file may be. A theme is forty-odd hex strings; a program is a
/// program, and this is the bound on what charter will *hash*, not on what it would run.
const MOST_DECLARED_BYTES: u64 = 8 << 20;

/// The most files one extension may declare. The fingerprint reads every one of them at every
/// launch (ADR 0041's named cost), so the launch's cost is bounded by the record rather than by
/// whatever a manifest asks for.
const MOST_DECLARED_FILES: usize = 64;

/// The most the record may be, for the reason [`crate::machine::MAX_BYTES`] has one.
const MOST_RECORD_BYTES: u64 = 1 << 20;

/// What [`slurp`] says about a file that is not there, as a constant rather than as a literal
/// written twice.
///
/// [`read`] has to tell "no record at all" — an ordinary machine with no extensions — apart from
/// "charter could not read the record", because the first is fine and the second must approve
/// nothing AND refuse to clobber. It tells them apart by this sentence. Spelling it in two
/// places would mean an edit to the wording silently turned every fresh machine's missing
/// record into an unreadable one, and [`approve`] would then refuse forever with a message
/// about a file that was never there.
const NOT_THERE: &str = "is not there";

/// The sentence charter says about what an extension can reach, which is the most important
/// string in this module.
///
/// It is **here** and not in the dialog, so that the window cannot say something kinder than
/// the core knows to be true, and so that changing it is a change to a file with tests on it.
/// Its content is ADR 0041's honesty paragraph, which the operator was shown before he ruled on
/// 2026-09-22 to ship without a sandbox: the grants are charter's conduct, not a cage.
pub const RUNS_AS_YOU: &str = "charter does not confine an extension. It runs as you do, with \
     your files, your network and your ability to start programs — nothing charter has stops \
     one reading your vaults, writing charter's own settings, or changing what your next \
     launch runs. What is listed above is what this extension DECLARES, not what it is \
     LIMITED to.";

/// The sentence charter says about the fingerprint, which is the second most important one.
///
/// ADR 0035's record says the same thing about itself and `profiletrust` says it at full
/// volume: what the ask closes is the accident and the careless change, not an author who set
/// out to deceive. Said on screen rather than left for whoever first assumes the dialog was a
/// guarantee.
pub const FINGERPRINTED: &str = "charter has read this extension's files and will ask again if \
     any of them change. That catches an extension that changed under you. It is not a \
     defence against one written to deceive you, and it is not a boundary.";

/// One theme an extension contributes: what to call it, and the file it is in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Theme {
    /// The name the operator picks it by.
    pub name: String,
    /// Where it is, relative to the extension's own directory.
    pub file: String,
}

/// What an extension says it is, as its manifest declares it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    /// How charter names it in the record. One path segment, so that an id can never be a
    /// path — the record is keyed by it and a `../` here would be a key that means a file.
    pub id: String,
    /// What a person calls it.
    pub name: String,
    /// The themes it contributes.
    pub themes: Vec<Theme>,
    /// A program it declares, relative to its own directory.
    ///
    /// **Declared, hashed, named in the prompt, and never run.** There is no executor in this
    /// stage; when there is one it will find the declaration already fingerprinted and already
    /// consented to, rather than inventing both at the moment it first needs them.
    pub program: Option<String>,
}

/// An extension charter has read off the disk, with the fingerprint of the bytes it read.
///
/// **The fingerprint is over the same read that produced the declarations**, which is
/// charter-app#123's shape applied before that defect can be copied: what the dialog shows and
/// what [`approve`] records cannot be two different reads of a file that changed in between.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Extension {
    /// Where it is. The directory the operator chose, absolute.
    pub path: PathBuf,
    /// Its manifest, as read.
    pub manifest: Manifest,
    /// sha256, hex, over the manifest and every file it declares. See [`read_parts`].
    pub fingerprint: String,
    /// Each declared theme's file, and the text that was hashed into
    /// [`Self::fingerprint`] — keyed by the file name the manifest declared.
    ///
    /// **Kept rather than re-read, and that is the whole reason this field exists.** The text
    /// the window draws with has to be the text the fingerprint was taken over, or the two are
    /// charter-app#123 one level down: an approval of bytes, and then a second read that could
    /// pick up different ones. Holding them costs at most [`MOST_DECLARED_FILES`] theme files;
    /// a declared program's bytes are *not* kept, because nothing consumes them.
    pub theme_text: BTreeMap<String, String>,
}

impl Extension {
    /// Its id, which is what the record is keyed by.
    pub fn id(&self) -> &str {
        &self.manifest.id
    }

    /// The text of one declared theme, as it was when the fingerprint was taken.
    ///
    /// It is handed to the window **as text**, unparsed. The theme vocabulary belongs to
    /// `app/src/theme/theme.ts` — it is that module's `TOKENS` that is the closed vocabulary
    /// charter owns, and its `load` that parses and re-emits rather than interpolating (ADR
    /// 0041's properties 1 and 4). A second parser here would be a second vocabulary, and the
    /// two would drift.
    pub fn theme_text(&self, which: &Theme) -> Option<&str> {
        self.theme_text.get(&which.file).map(String::as_str)
    }
}

/// Whether charter needs to ask about an extension before it contributes anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Standing {
    /// The operator approved these exact bytes at this exact path.
    Approved,
    /// Nothing has approved it.
    New,
    /// It was approved, and what is on disk now is not what was approved.
    Changed,
}

impl Standing {
    /// Whether charter may let it contribute without asking.
    pub fn may_contribute(self) -> bool {
        matches!(self, Self::Approved)
    }

    /// The word a prompt or a refusal says.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Approved => "approved",
            Self::New => "new",
            Self::Changed => "changed",
        }
    }
}

/// One entry in the record: where an extension is, and the fingerprint the operator approved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The directory, as the operator gave it.
    pub path: PathBuf,
    /// The fingerprint they approved, or `None` for an extension charter knows about and has
    /// never been told to trust.
    pub approved: Option<String>,
}

/// Every extension this machine knows about, keyed by id.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Registry {
    /// Sorted by id, so the record charter writes does not depend on the order it read.
    pub entries: BTreeMap<String, Entry>,
}

/// The record, and what reading it cost.
#[derive(Debug, Clone, Default)]
pub struct Loaded {
    /// What the record said. **Empty when [`Self::unreadable`] is set**, so that an
    /// unreadable record approves nothing.
    pub registry: Registry,
    /// Why charter could not read it, when it could not. A record that is missing is not
    /// unreadable — it is a machine with no extensions — so this stays `None` for that.
    pub unreadable: Option<String>,
    /// Entries charter dropped rather than raised on, with the reason for each.
    pub dropped: Vec<String>,
}

impl Loaded {
    /// What the record says about `id`, if anything.
    pub fn entry(&self, id: &str) -> Option<&Entry> {
        self.registry.entries.get(id)
    }

    /// Whether `found` may contribute without asking, and why not when it may not.
    ///
    /// **The path is part of the approval, not only the fingerprint.** An extension re-installed
    /// from a different directory is a different extension that kept a name, and the operator
    /// approved a thing at a place. This is [`crate::machine::still_a_plane`]'s reasoning about
    /// a remembered path, one level in.
    pub fn standing(&self, found: &Extension) -> Standing {
        match self.entry(found.id()) {
            None => Standing::New,
            Some(entry) => match &entry.approved {
                None => Standing::New,
                Some(_) if entry.path != found.path => Standing::Changed,
                Some(was) if was == &found.fingerprint => Standing::Approved,
                Some(_) => Standing::Changed,
            },
        }
    }
}

// ---------------------------------------------------------------------------------------
// Reading an extension off the disk
// ---------------------------------------------------------------------------------------

/// Read the extension in `dir`, or say why charter will not.
///
/// The error is a sentence for the operator, as [`crate::machine::still_a_plane`]'s is: an
/// extension charter cannot read is a row that says why, never a dialog at launch and never a
/// silent absence.
///
/// **Every refusal below is a state that would otherwise read as "nothing declared", which
/// reads as "safe".** That is the direction the whole module leans away from: an extension
/// whose manifest charter could not read contributes nothing *and says so*, rather than
/// contributing nothing quietly and being counted as fine.
pub fn read_at(dir: &Path) -> Result<Extension, String> {
    if !dir.is_absolute() {
        return Err(format!(
            "'{}' is not an absolute path, and charter records an extension by where it is",
            dir.display()
        ));
    }
    // `symlink_metadata`, not `is_dir`: the latter is true of a link TO a directory, and the
    // approval would then be recorded against a name that can be re-pointed at anything
    // afterwards without the fingerprint noticing — the file contents would not have changed,
    // because they would be a different file's.
    match std::fs::symlink_metadata(dir) {
        Err(_) => {
            return Err(format!("'{}' is not there", dir.display()));
        }
        Ok(found) if found.file_type().is_symlink() => {
            return Err(format!(
                "'{}' is a symlink, and charter reads an extension from the path it was given",
                dir.display()
            ));
        }
        Ok(found) if !found.is_dir() => {
            return Err(format!("'{}' is not a directory", dir.display()));
        }
        Ok(_) => {}
    }

    let manifest_at = dir.join(MANIFEST);
    let text = slurp(dir, &manifest_at, MOST_MANIFEST_BYTES)
        .map_err(|why| format!("'{}' {why}", manifest_at.display()))?;
    let manifest = parse(&text).map_err(|why| format!("'{}' {why}", manifest_at.display()))?;

    // The fingerprint is taken over the bytes just read and the bytes about to be read, the
    // declarations handed back come out of the same read, and the theme text the window will
    // draw with is kept from it rather than fetched again. Two reads is charter-app#123.
    let (fingerprint, theme_text) = read_parts(dir, text.as_bytes(), &manifest)?;

    Ok(Extension {
        path: dir.to_path_buf(),
        manifest,
        fingerprint,
        theme_text,
    })
}

/// Read one file under `root`, refusing everything that is not a plain, bounded file reached
/// without a link.
///
/// **The gate is on the exact path that is opened, never on its parent.**
/// [`crate::contain::open_no_link`] walks every component below `root` refusing a symlink, and
/// `O_NOFOLLOW` answers for the last one at the instant of the open rather than a `stat`
/// earlier. The `fstat` afterwards is of the descriptor the read will use, so the file charter
/// checked and the file charter reads cannot be two files.
fn slurp(root: &Path, path: &Path, most: u64) -> Result<String, String> {
    let mut open = crate::contain::open_no_link(root, path).map_err(|why| match why.kind() {
        io::ErrorKind::NotFound => NOT_THERE.to_owned(),
        _ => format!("could not be opened: {why}"),
    })?;
    // A FIFO here is not a slow read, it is a permanent one, and this runs on the path that
    // draws the window — so the type is asked before a byte is taken.
    let found = open
        .metadata()
        .map_err(|why| format!("could not be read: {why}"))?;
    if !found.file_type().is_file() {
        return Err("is not a plain file, and charter reads an extension from nothing else".into());
    }
    if found.len() > most {
        return Err(format!(
            "is {} bytes, and charter reads no more than {most} here",
            found.len()
        ));
    }
    let mut text = String::new();
    io::Read::read_to_string(&mut open, &mut text)
        .map_err(|why| format!("could not be read: {why}"))?;
    Ok(text)
}

/// A manifest's text, as a manifest, or why it is not one.
fn parse(text: &str) -> Result<Manifest, String> {
    let doc: serde_json::Value =
        serde_json::from_str(text).map_err(|why| format!("is not JSON: {why}"))?;
    let doc = doc.as_object().ok_or("is not a JSON object")?;

    match doc.get("version").and_then(serde_json::Value::as_u64) {
        Some(found) if found == u64::from(VERSION) => {}
        Some(found) => {
            return Err(format!(
                "is version {found}, and this charter reads version {VERSION}"
            ));
        }
        None => return Err("says no version, so charter cannot say what it means".into()),
    }

    let id = doc
        .get("id")
        .and_then(serde_json::Value::as_str)
        .ok_or("has no id")?;
    // The record is keyed by this, so an id that is a path is a key that is a file.
    if !crate::contain::segment_ok(id) || !id.chars().all(ok_in_an_id) {
        return Err(format!(
            "has the id {id:?}, and an extension's id is letters, digits, '-', '_' and '.', \
             starting with a letter or a digit"
        ));
    }
    if !id.starts_with(|c: char| c.is_ascii_alphanumeric()) {
        return Err(format!(
            "has the id {id:?}, which does not start with a letter or a digit"
        ));
    }
    let name = doc
        .get("name")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or(id)
        .to_owned();

    let contributes = match doc.get("contributes") {
        None => return Err("declares no contributions, so there is nothing to consent to".into()),
        Some(value) => value
            .as_object()
            .ok_or("has a 'contributes' that is not an object")?,
    };

    let mut themes = Vec::new();
    for (at, raw) in contributes
        .get("themes")
        .map(array)
        .unwrap_or_default()
        .iter()
        .enumerate()
    {
        let raw = raw
            .as_object()
            .ok_or_else(|| format!("declares a theme at {at} that is not an object"))?;
        let file = raw
            .get("file")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| format!("declares a theme at {at} with no file"))?;
        declarable(file).map_err(|why| format!("declares the theme file {file:?}, which {why}"))?;
        let name = raw
            .get("name")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .unwrap_or(file)
            .to_owned();
        themes.push(Theme {
            name,
            file: file.to_owned(),
        });
    }

    let program = match contributes.get("runs") {
        None => None,
        Some(value) => {
            let file = value
                .as_str()
                .ok_or("declares a 'runs' that is not a path")?;
            declarable(file)
                .map_err(|why| format!("declares the program {file:?}, which {why}"))?;
            Some(file.to_owned())
        }
    };

    if themes.is_empty() && program.is_none() {
        return Err("declares no contributions, so there is nothing to consent to".into());
    }
    if themes.len() + usize::from(program.is_some()) > MOST_DECLARED_FILES {
        return Err(format!(
            "declares more than {MOST_DECLARED_FILES} files, and charter hashes every one of \
             them at every launch"
        ));
    }
    Ok(Manifest {
        id: id.to_owned(),
        name,
        themes,
        program,
    })
}

fn ok_in_an_id(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.'
}

fn array(value: &serde_json::Value) -> Vec<serde_json::Value> {
    value.as_array().cloned().unwrap_or_default()
}

/// Whether a manifest may name this file: a relative path of ordinary segments, below the
/// extension's own directory.
///
/// `contain::open_no_link` would refuse most of these at the open, a moment later. It is
/// checked here anyway and the duplication is deliberate: this is the string that goes **into
/// the fingerprint and onto the screen**, so a path the operator would be shown must be one
/// charter has already said is a path, rather than one that reads plausibly and fails later.
fn declarable(file: &str) -> Result<(), String> {
    if file.is_empty() {
        return Err("is empty".into());
    }
    if file.contains('\0') {
        return Err("holds a NUL, which ends the string inside the C library".into());
    }
    let path = Path::new(file);
    if path.is_absolute() {
        return Err("is absolute, and an extension declares its own files by relative path".into());
    }
    for step in path.components() {
        match step {
            Component::Normal(segment) => {
                let Some(segment) = segment.to_str() else {
                    return Err("is not valid UTF-8".into());
                };
                if !crate::contain::segment_ok(segment) {
                    return Err("holds a segment charter will not open".into());
                }
            }
            Component::CurDir => return Err("holds a '.', which names nothing".into()),
            Component::ParentDir => {
                // Deliberately NOT the words `contain::no_link_on_the_way` uses for the same
                // shape ("walks up out of {root}"). Both guards refuse this path and the walk
                // refuses it a moment later, so a test asserting on the shared phrase passed
                // with this check deleted — measured, and the reason this sentence is its own.
                return Err("names a parent directory, which a manifest may not".into());
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err("is rooted, and an extension declares its own files".into());
            }
        }
    }
    Ok(())
}

/// The sha256, hex, of everything an extension is.
///
/// **ADR 0041's difference from ADR 0035, and it costs something.** 0035 fingerprints
/// *configuration* — two settings keys read out of a file. This fingerprints **code**: the
/// manifest, every theme it declares, and the program it declares, read whole, at every
/// launch. An extension's path is not its contents, and an extension that rewrites itself
/// after approval is the attack the whole record is about. The cost is named here rather than
/// discovered when the app gets slower: it is bounded by [`MOST_DECLARED_FILES`] files of
/// [`MOST_DECLARED_BYTES`] each.
///
/// **The parts are length-framed** ([`digest`]), and the honest note about that is there rather
/// than here: through this function the framing is not currently load-bearing, because the
/// manifest is itself a part and it names every file the later parts carry. It is defence for
/// the first part the manifest does not name. The parts are visited in the order the manifest
/// declares them, and that order is inside the hash for the same reason.
///
/// It hands back the theme text alongside the digest for the reason [`Extension::theme_text`]
/// gives: the bytes the window draws with must be the bytes that were hashed, and a second read
/// to fetch them would reopen charter-app#123 one level down.
fn read_parts(
    dir: &Path,
    manifest_bytes: &[u8],
    manifest: &Manifest,
) -> Result<(String, BTreeMap<String, String>), String> {
    let mut read: Vec<(String, String)> = Vec::new();
    let mut theme_text = BTreeMap::new();
    let declared = manifest
        .themes
        .iter()
        .map(|theme| (theme.file.as_str(), true))
        .chain(manifest.program.as_deref().map(|file| (file, false)));
    for (file, is_a_theme) in declared {
        let at = dir.join(file);
        let text = slurp(dir, &at, MOST_DECLARED_BYTES)
            .map_err(|why| format!("'{}' {why}", at.display()))?;
        if is_a_theme {
            theme_text.insert(file.to_owned(), text.clone());
        }
        read.push((file.to_owned(), text));
    }

    // A domain tag first, so this digest can never equal one taken over the same bytes for
    // another purpose, and the version with it, so widening what is hashed re-asks rather than
    // silently matching.
    let tag = VERSION.to_be_bytes();
    let mut parts: Vec<(&str, &[u8])> =
        vec![("charter-extension", &tag), (MANIFEST, manifest_bytes)];
    parts.extend(
        read.iter()
            .map(|(file, text)| (file.as_str(), text.as_bytes())),
    );
    Ok((digest(&parts), theme_text))
}

/// sha256, hex, over a list of named parts — **length-framed**, so no two different lists can
/// hash the same.
///
/// Each part goes in as its name's length, its name, its bytes' length and its bytes.
/// Concatenating name and content without the lengths would let a part named `a` holding `bc`
/// collide with one named `ab` holding `c`, and a fingerprint with a collision in it is a
/// fingerprint that can be changed under the operator without asking again.
///
/// **It is a seam, and the seam is why the framing is testable at all.** Through
/// [`read_parts`] the framing is currently unreachable: the manifest's own bytes are the
/// second part and they *name every file the later parts carry*, so any pair of extensions
/// that would collide already differ in the manifest. That makes the framing defence for a
/// part the manifest does not name — and the day one is added, nobody will notice it became
/// load-bearing. `a_name_and_its_contents_cannot_run_together` drives this function directly
/// rather than through an extension, so dropping the framing reddens a test today.
fn digest(parts: &[(&str, &[u8])]) -> String {
    use sha2::Digest as _;
    let mut hasher = sha2::Sha256::new();
    for (name, bytes) in parts {
        hasher.update((name.len() as u64).to_be_bytes());
        hasher.update(name.as_bytes());
        hasher.update((bytes.len() as u64).to_be_bytes());
        hasher.update(bytes);
    }
    hex(&hasher.finalize())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

// ---------------------------------------------------------------------------------------
// The record
// ---------------------------------------------------------------------------------------

/// The record's own path inside `config_root`.
pub fn file(config_root: &Path) -> PathBuf {
    crate::machine::dir(config_root).join(RECORD)
}

/// Whether charter keeps an extension record on this platform at all. See the module
/// docstring, and [`crate::machine`]'s, and charter ADR 0031.
#[cfg(unix)]
fn supported() -> io::Result<()> {
    Ok(())
}

#[cfg(not(unix))]
fn supported() -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "charter keeps no extension record on this platform: the 0600 on the file and the 0700 \
         on its directory have no expression here (charter-app#98), and a guard that cannot be \
         expressed refuses rather than degrades (charter ADR 0031)",
    ))
}

/// Read the record. **Never raises**, because this is on the path that draws the window.
///
/// Everything that is not a record charter can read comes back as
/// [`Loaded::unreadable`] with an empty registry — so nothing is approved and everything asks
/// — and [`approve`] then refuses to write over it.
pub fn read(config_root: &Path) -> Loaded {
    let refused = |why: String| Loaded {
        registry: Registry::default(),
        unreadable: Some(why),
        dropped: Vec::new(),
    };
    if let Err(why) = supported() {
        return refused(why.to_string());
    }
    let target = file(config_root);
    let text = match slurp(config_root, &target, MOST_RECORD_BYTES) {
        Ok(text) => text,
        // No record is a machine with no extensions, and is the ordinary state. It is the one
        // absence that is not an error, and it is told apart from the others by `NotFound`
        // rather than by a second `stat` that could answer about a different file.
        Err(why) if why == NOT_THERE => return Loaded::default(),
        Err(why) => return refused(format!("'{}' {why}", target.display())),
    };
    let doc: serde_json::Value = match serde_json::from_str(&text) {
        Ok(doc) => doc,
        Err(why) => return refused(format!("'{}' is not JSON: {why}", target.display())),
    };
    let Some(doc) = doc.as_object() else {
        return refused(format!("'{}' is not a JSON object", target.display()));
    };
    match doc.get("version").and_then(serde_json::Value::as_u64) {
        Some(found) if found == u64::from(VERSION) => {}
        Some(found) => {
            return refused(format!(
                "'{}' is version {found}, and this charter reads version {VERSION}",
                target.display()
            ));
        }
        None => {
            return refused(format!(
                "'{}' says no version, so charter cannot say what it means",
                target.display()
            ));
        }
    }

    let mut registry = Registry::default();
    let mut dropped = Vec::new();
    let listed = doc
        .get("extensions")
        .and_then(serde_json::Value::as_object)
        .cloned()
        .unwrap_or_default();
    for (id, raw) in listed {
        match usable(&id, &raw) {
            Ok(entry) => {
                registry.entries.insert(id, entry);
            }
            // Dropped and reported, never raised: a record with one bad row still holds the
            // operator's other extensions, and a launch that refuses them all because of one
            // is a launch that punishes the wrong thing.
            Err(why) => dropped.push(format!("{id}: {why}")),
        }
    }
    Loaded {
        registry,
        unreadable: None,
        dropped,
    }
}

/// One row of the record, or why it is dropped.
///
/// An `approved` that is not a fingerprint — a number, an object, a truncated hex string, the
/// word `true` — is **not** an approval and is not an error either: the row keeps its path and
/// loses its yes, so charter asks about it again. That is `profiletrust`'s "an entry that is
/// not a fingerprint reads as no record", and the state it keeps out is a record whose
/// `"approved": true` grants everything.
fn usable(id: &str, raw: &serde_json::Value) -> Result<Entry, String> {
    if !crate::contain::segment_ok(id) || !id.chars().all(ok_in_an_id) {
        return Err("is not an extension id".into());
    }
    let row = raw.as_object().ok_or("is not an object")?;
    let path = row
        .get("path")
        .and_then(serde_json::Value::as_str)
        .ok_or("has no path")?;
    if path.is_empty() || path.contains('\0') {
        return Err("has a path charter will not open".into());
    }
    let path = PathBuf::from(path);
    if !path.is_absolute() || path.components().any(|step| step == Component::ParentDir) {
        return Err("has a path that is not absolute and free of '..'".into());
    }
    let approved = row
        .get("approved")
        .and_then(serde_json::Value::as_str)
        .filter(|hash| hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit()))
        .map(str::to_owned);
    Ok(Entry { path, approved })
}

/// The record as charter writes it.
fn as_json(registry: &Registry) -> serde_json::Value {
    let mut rows = serde_json::Map::new();
    for (id, entry) in &registry.entries {
        let mut row = serde_json::Map::new();
        row.insert(
            "path".into(),
            serde_json::Value::String(entry.path.display().to_string()),
        );
        row.insert(
            "approved".into(),
            match &entry.approved {
                Some(hash) => serde_json::Value::String(hash.clone()),
                None => serde_json::Value::Null,
            },
        );
        rows.insert(id.clone(), serde_json::Value::Object(row));
    }
    let mut doc = serde_json::Map::new();
    doc.insert("version".into(), serde_json::Value::from(VERSION));
    doc.insert("extensions".into(), serde_json::Value::Object(rows));
    serde_json::Value::Object(doc)
}

/// Read the record, change it, and write it back.
///
/// **A record charter could not read is never overwritten** — [`crate::machine::update`]'s
/// rule, for the reason in the module docstring: the difference between "this says nothing
/// charter understands" and "charter could not read this" is the difference between content
/// worth replacing and a path that is compromised, and clobbering the second destroys every
/// extension the operator installed to fix nothing.
fn update(config_root: &Path, change: impl FnOnce(&mut Registry)) -> io::Result<()> {
    supported()?;
    let mut loaded = read(config_root);
    if let Some(why) = &loaded.unreadable {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("charter will not overwrite an extension record it could not read: {why}"),
        ));
    }
    change(&mut loaded.registry);
    write(config_root, &loaded.registry)
}

/// Write the record: `0600`, in charter's `0700` directory, atomically by rename.
///
/// This is [`crate::machine::write`]'s body with this record's name, and the sharp parts are
/// the same because they are the same directory: the mode goes on the **temp** file (a rename
/// carries the source's mode onto the target, never the other way round), the containment walk
/// gates the **temp** path because that is where the bytes actually land, and the `rename`
/// needs no gate because it replaces a *name* — a symlink sitting at the record's path is
/// replaced rather than written through.
pub fn write(config_root: &Path, registry: &Registry) -> io::Result<()> {
    supported()?;
    let dir = crate::machine::dir(config_root);
    std::fs::create_dir_all(config_root)?;
    crate::profiletrust::private_dir(&dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
    }
    let target = dir.join(RECORD);
    // A pid AND a per-call tag, as `machine::write` does: the pid separates two processes, the
    // tag separates two writers inside one — Tauri runs commands on a thread pool — and a pid
    // the kernel has recycled.
    let temp = dir.join(format!(
        "{RECORD}.{}.{}.writing",
        std::process::id(),
        crate::workspaces::scratch_tag()
    ));
    let text = crate::pyjson::dumps_indent2(&as_json(registry)) + "\n";
    write_through(config_root, &target, &temp, text.as_bytes())
}

/// [`write`]'s body with the temp file named by the caller.
///
/// The seam exists so a test can **plant its link at the path that is actually opened**.
/// `machine::write_through` has the same one for the same reason: a test that plants a link at
/// the record's own path passes against code that writes through an unguarded temp file, which
/// is precisely the defect the gate exists to stop.
fn write_through(config_root: &Path, target: &Path, temp: &Path, bytes: &[u8]) -> io::Result<()> {
    crate::contain::no_link_on_the_way(config_root, temp)?;
    let mut options = std::fs::OpenOptions::new();
    // `create_new`, so an existing file at the temp path is refused rather than written
    // through — and, on any POSIX system, so is a symlink sitting there (`O_CREAT|O_EXCL`
    // fails on one).
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut out = crate::contain::nofollow(&mut options).open(temp)?;
    let result = io::Write::write_all(&mut out, bytes)
        .and_then(|()| out.sync_all())
        .and_then(|()| std::fs::rename(temp, target));
    if result.is_err() {
        // This call created it, so this call takes it away.
        let _ = std::fs::remove_file(temp);
    }
    result
}

/// Put an extension in the record, unapproved, and hand back what was read.
///
/// **Installing is not consenting.** ADR 0041 rejects `charter plugin install <name>` as a
/// consent step for ADR 0022's reason — *a chat can run a command as easily as it can edit a
/// file* — so this writes the row with no approval on it and the yes comes from the prompt and
/// from nothing else. An extension already in the record keeps its approval only if its
/// fingerprint and its path are unchanged; anything else drops the yes, which makes the next
/// launch ask.
///
/// By path, by the operator, from nowhere. There is no fetch by name here and there is no
/// registry service to fetch from: the moment charter resolves an extension name over the
/// network it owns a supply chain.
pub fn install(config_root: &Path, dir: &Path) -> io::Result<Extension> {
    supported()?;
    let found = read_at(dir).map_err(|why| io::Error::new(io::ErrorKind::InvalidData, why))?;
    let loaded = read(config_root);
    let keep = matches!(loaded.standing(&found), Standing::Approved);
    let approved = keep
        .then(|| loaded.entry(found.id()).and_then(|e| e.approved.clone()))
        .flatten();
    let id = found.id().to_owned();
    let path = found.path.clone();
    update(config_root, |registry| {
        registry.entries.insert(id, Entry { path, approved });
    })?;
    Ok(found)
}

/// Record the operator's yes for exactly the bytes they were shown.
///
/// `fingerprint` is carried back from the prompt rather than recomputed here, which is
/// charter-app#123's fix applied before the defect can be copied: the contribution shown in the
/// dialog and the record written afterwards are **one read**, so a write between them cannot be
/// consented to without having been shown. A fingerprint that is not one is refused rather than
/// stored, because a stored non-fingerprint is a row that can never match and would ask forever
/// — or, if a later reader were laxer, a row that matches everything.
pub fn approve(config_root: &Path, id: &str, at: &Path, fingerprint: &str) -> io::Result<()> {
    supported()?;
    if fingerprint.len() != 64 || !fingerprint.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "an approval is recorded against a sha256, and that is not one",
        ));
    }
    if !crate::contain::segment_ok(id) || !id.chars().all(ok_in_an_id) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{id:?} is not an extension id"),
        ));
    }
    let entry = Entry {
        path: at.to_path_buf(),
        approved: Some(fingerprint.to_owned()),
    };
    update(config_root, |registry| {
        registry.entries.insert(id.to_owned(), entry);
    })
}

/// Take an extension out of the record.
///
/// Its own data is not charter's to delete and is not in charter's store to begin with (ADR
/// 0041: *an extension is re-installable by name; its state is its own problem*). What this
/// removes is the path and the approval, which is exactly what ADR 0034's test says deleting
/// the store must cost — the arrangement and the approvals, and nothing else.
pub fn forget(config_root: &Path, id: &str) -> io::Result<()> {
    update(config_root, |registry| {
        registry.entries.remove(id);
    })
}

// ---------------------------------------------------------------------------------------
// The survey: what has contributed what to this window
// ---------------------------------------------------------------------------------------

/// One installed extension, as the registry answers about it.
#[derive(Debug, Clone)]
pub struct Surveyed {
    /// Its id, which is the key in the record even when the directory is gone.
    pub id: String,
    /// Where the record says it is.
    pub path: PathBuf,
    /// What is on disk there now, when charter could read it.
    pub found: Option<Extension>,
    /// Why charter could not read it, when it could not. A row with this set contributes
    /// nothing and **says so**, which is the state this module leans away from leaving silent.
    pub refused: Option<String>,
    /// Whether it may contribute without asking. An extension charter could not read is
    /// [`Standing::New`] — there is nothing to compare, so there is nothing approved.
    pub standing: Standing,
}

impl Surveyed {
    /// The themes it is contributing to this window right now: none unless it is approved.
    pub fn themes_in_force(&self) -> &[Theme] {
        match (&self.found, self.standing.may_contribute()) {
            (Some(found), true) => &found.manifest.themes,
            _ => &[],
        }
    }
}

/// What has contributed what to this window.
#[derive(Debug, Clone)]
pub struct Survey {
    /// charter's own themes, which are compiled into the window and are never asked about.
    pub built_in_themes: Vec<String>,
    /// Every extension the record names, in id order.
    pub installed: Vec<Surveyed>,
    /// Why the record could not be read, when it could not — in which case `installed` is
    /// empty and nothing an extension declares is in force.
    pub unreadable: Option<String>,
    /// Rows the record held and charter dropped, with the reason for each.
    pub dropped: Vec<String>,
}

/// Answer the one question ADR 0041 says to build before anything else: *what has contributed
/// what to this window.*
///
/// It reads the disk — every installed extension's manifest and every file it declares, to
/// re-take the fingerprint — because an approval is of bytes and the bytes are what may have
/// changed. That is ADR 0041's named cost of fingerprinting code rather than configuration,
/// and it is bounded by [`MOST_DECLARED_FILES`] and [`MOST_DECLARED_BYTES`] per extension.
pub fn survey(config_root: &Path) -> Survey {
    let loaded = read(config_root);
    let mut installed = Vec::new();
    for (id, entry) in &loaded.registry.entries {
        match read_at(&entry.path) {
            Ok(found) => {
                // The id the record is keyed by and the id the manifest declares must agree.
                // They can stop agreeing without anybody lying — a directory is moved, a
                // manifest is edited — and if they did not have to agree, a record row could
                // carry an approval for one extension to a directory holding another.
                let standing = if found.id() == id {
                    loaded.standing(&found)
                } else {
                    Standing::Changed
                };
                installed.push(Surveyed {
                    id: id.clone(),
                    path: entry.path.clone(),
                    found: Some(found),
                    refused: None,
                    standing,
                });
            }
            Err(why) => installed.push(Surveyed {
                id: id.clone(),
                path: entry.path.clone(),
                found: None,
                refused: Some(why),
                standing: Standing::New,
            }),
        }
    }
    Survey {
        built_in_themes: BUILT_IN_THEMES
            .iter()
            .map(|&name| name.to_owned())
            .collect(),
        installed,
        unreadable: loaded.unreadable,
        dropped: loaded.dropped,
    }
}

// ---------------------------------------------------------------------------------------
// The consent surface
// ---------------------------------------------------------------------------------------

/// The question charter asks before an extension contributes anything, and the honest limits
/// on the answer.
///
/// **The words are the core's, not the window's.** A dialog that wrote its own sentence about
/// what an extension can reach could drift kinder than the truth one refactor at a time, and
/// the truth here is uncomfortable enough that drift is the likely direction. So
/// [`RUNS_AS_YOU`] and [`FINGERPRINTED`] travel with the question and the window renders what
/// it is given.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prompt {
    /// Its id.
    pub id: String,
    /// What to call it, as its manifest does.
    pub name: String,
    /// Where it is. Shown and not merely carried: an extension is approved at a path.
    pub path: String,
    /// Every contribution, in charter's own words, one line each.
    pub declares: Vec<String>,
    /// The fingerprint being consented to, which goes back to [`approve`] untouched.
    pub fingerprint: String,
    /// Whether this is a first ask rather than a re-ask, so the dialog can say which.
    pub first: bool,
    /// [`RUNS_AS_YOU`]. Carried rather than imported so that the window cannot render a
    /// different sentence from the one the core stands behind.
    pub runs_as_you: String,
    /// [`FINGERPRINTED`].
    pub fingerprint_note: String,
}

/// The question to ask about `found`.
pub fn prompt(found: &Extension, standing: Standing) -> Prompt {
    let mut declares: Vec<String> = found
        .manifest
        .themes
        .iter()
        .map(|theme| format!("a theme, “{}”", theme.name))
        .collect();
    if let Some(program) = &found.manifest.program {
        // Named, and named as not running. An extension that declares a program and is
        // approved by a charter with no executor must not leave the operator believing they
        // approved it running — nor believing they refused it, when the next charter will run
        // it on the same approval.
        declares.push(format!(
            "a program, {program} — this charter has no extension runtime and does not start it"
        ));
    }
    Prompt {
        id: found.id().to_owned(),
        name: found.manifest.name.clone(),
        path: found.path.display().to_string(),
        declares,
        fingerprint: found.fingerprint.clone(),
        first: standing != Standing::Changed,
        runs_as_you: RUNS_AS_YOU.to_owned(),
        fingerprint_note: FINGERPRINTED.to_owned(),
    }
}

#[cfg(test)]
mod tests;
