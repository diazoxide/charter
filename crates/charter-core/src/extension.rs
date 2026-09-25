//! The extension registry: what has contributed what to this window, and what the operator
//! agreed to.
//!
//! **The runtime exists, and it is not in this module.** Nothing here spawns a process, opens
//! a socket or evaluates anything. This module answers three questions and no others: what an
//! extension *is* — including which [capabilities](capability) it asks for (ADR 0053) — what
//! this machine has installed, and what the operator approved each one to contribute.
//!
//! **The executor is [`crate::executor`], and it was built against this list rather than
//! beside it** (ADR 0041 stage 2). It starts a program only after asking *this* module, at the
//! moment of starting, whether the extension is approved and whether what is on disk is still
//! what was approved — [`read_at`] re-taken over the whole tree, [`Loaded::standing`] compared.
//! Keeping the two apart is deliberate: the module that decides what a yes covers has no way to
//! run anything, so a defect here can never *be* an execution, only a refusal to allow one.
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
//! would run on a click, with even less in front of it. So a project cannot bring one with
//! it: nothing in a plane names an extension's directory, and this module's registry never reads
//! a plane. What a plane MAY say, since charter-app#253, is which of the extensions this machine
//! already approved it has on, and what it sets for them — `[extensions.<id>]`, read by
//! [`project`] and never by the registry, and never in place of the approval (ADR 0048).
//!
//! That also settles the collision above by construction: the two uses of the word can never
//! appear in one list, because only one of them can come out of a plane.
//!
//! # Where the record lives, and why ADR 0034 needs no amendment
//!
//! Beside [`crate::machine::FILE`], not inside it: `$CHARTER_CONFIG_HOME/charter/` (else
//! `$XDG_CONFIG_HOME`, else `~/.config`) holds `machine.json` and, next to it,
//! [`RECORD`]. ADR 0034's rule is "four things, and nothing else" and ADR 0040 already
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
//! `0600`/`0700` this record depends on has no expression there, and ADR 0031 settles
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
//! confine an extension below the operator.** A program the executor starts runs as the same
//! user, with the same filesystem and the same ability to `exec`; it is able to read
//! `.charter/vaults/`, write `machine.json` and edit `charter.local.toml` without asking
//! charter for anything. What this module records is **what charter will do on an extension's
//! behalf** — its own conduct — and a consent surface that implied a cage would manufacture
//! confidence charter cannot back. That is why the honest sentence is data owned by the core,
//! shipped to the window with the question, and pinned by a test.
//!
//! # What is *in* the vocabulary today
//!
//! A manifest's `version` is the protocol its program speaks, and its `capabilities` list is
//! what it asks charter to do for it beyond what follows ([`capability`], ADR 0053). A word in
//! that list this charter does not know refuses the whole manifest, by name. Badges and repo
//! columns ([`facts`]) are declared data too: charter draws them from a facts file in the
//! state directory and never starts the program to do it.
//!
//! Themes, panels (`crate::panel`, ADR 0043), and **views**. A theme and a panel are
//! declarative data against a closed vocabulary charter owns, charter chooses the consumer, and
//! charter parses and re-emits rather than interpolating — ADR 0041's four properties.
//!
//! A view ([`View`]) is the one that is not data. It is ADR 0041's second minimum capability —
//! *"a named palette command that, when the operator invokes it, sends one request to the
//! plugin and displays the text that comes back"* — with *the text* widened to the panel
//! vocabulary: the operator opens it, charter starts the extension's declared program
//! ([`Manifest::program`]) once, hands it what the view is [about](crate::panel::Subject), and
//! draws what comes back. [`crate::executor`] is what does the starting; this module is what it
//! asks first.
//!
//! # The fingerprint is over the DIRECTORY, not over the declared list (charter-app#152)
//!
//! Until #152 the fingerprint covered the manifest and every file the manifest *declared*, which
//! is ADR 0041's own wording — *"a hash of the executable and of every file it declares"* — and
//! is a gap in it rather than a deviation from it. **An extension could add or change an
//! undeclared sibling and the fingerprint said unchanged.** A program loads what it likes: a
//! `.dylib` beside it, a script it sources, a config it reads. None of those is declared, so
//! none of them was hashed, and [`FINGERPRINTED`]'s sentence — the one the operator reads before
//! saying yes — was false about exactly the files that matter once stage 2 starts a program.
//!
//! So [`tree`] hashes **every path below the extension's directory**: contents *and* the set of
//! paths, so a file added or removed changes the fingerprint and not only an edited one. The
//! operator ruled on 2026-09-22, and the reasoning is the constraint the rest of this section is
//! designed against.
//!
//! **One exclusion, and it is the manifest's own [`Manifest::state`] directory.** Without it, an
//! extension that keeps a cache or a log beside itself re-prompts at every launch, and **a
//! consent dialog people click through is worse than no dialog at all** — which is the same
//! sentence ADR 0041's amendment uses about a prompt that over-promises, pointed the other way.
//! The carve-out is therefore narrow by construction:
//!
//! - it is **one path segment**, so it is visible in a directory listing rather than buried;
//! - it is **named in the manifest**, which is itself hashed, so the carve-out cannot appear,
//!   move or widen without the operator being asked again;
//! - **nothing charter reads may be inside it** — a theme or a program declared under it is
//!   refused at [`parse`], so charter never opens a byte in there;
//! - **charter refuses to load an extension whose state directory holds a symlink or a file with
//!   an executable bit** ([`state_holds_no_code`]). That is the enforcement the exclusion has to
//!   carry: a state directory that may hold a `.dylib` or a script the program loads has given
//!   the whole property back.
//!
//! **And the limit of that enforcement, stated rather than left to be discovered.** No
//! filesystem predicate makes a file un-loadable as code: `dlopen` does not need the executable
//! bit on Linux, and `source` does not need it anywhere. What the check closes is the careless
//! case and the conventional one — a helper binary cached beside a log — and what it leaves open
//! is a program the operator approved that reads its own state directory as code. That last one
//! is the same class as a program that fetches a string and evaluates it, which no fingerprint
//! anywhere closes, and which ADR 0041 already refuses to pretend about.
//!
//! **A symlink inside the tree is hashed as a link and never followed.** Its own target string
//! is what goes into the hash, so re-pointing it asks again, a link out of the tree reads
//! nothing out there, and a loop cannot hang the walk because nothing is walked *through*.
//! Refusing links outright was the alternative and it is the wrong one: `node_modules/.bin/*` is
//! a tree of them, and "charter refuses my extension over a file I didn't write" is the reaction
//! the operator explicitly ruled against.
//!
//! **`.git` is hashed like anything else, and that is deliberate.** `.git/hooks/` holds
//! programs. An extension developer working inside a clone will be asked again after a fetch;
//! an operator running an installed extension will not, because nothing in it moves.

use std::collections::BTreeMap;
use std::io;
use std::path::{Component, Path, PathBuf};

pub mod briefing;
pub mod capability;
pub mod events;
pub mod facts;
pub mod project;

pub use capability::Capability;

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

/// The most any one file charter **keeps** may be — a theme's text, which is held in memory and
/// handed to the window. A theme is forty-odd hex strings.
///
/// It stopped being the bound on what charter *hashes* at charter-app#152: the tree is hashed in
/// chunks and nothing is held, so [`MOST_TREE_BYTES`] bounds that and a declared program is no
/// longer capped on its own. This is a bound on memory, which is why it belongs to the files that
/// are kept rather than to the files that are read.
const MOST_DECLARED_BYTES: u64 = 8 << 20;

/// The most files one extension may declare. Declaring is what puts a file on the screen and,
/// for a theme, what keeps its text in memory; the *hash* covers the whole directory either way
/// ([`MOST_TREE_ENTRIES`]), so this bounds the list the operator is shown rather than the work.
const MOST_DECLARED_FILES: usize = 64;

/// The most files and directories charter will look at inside one extension — hashed, or merely
/// checked inside the state directory.
///
/// **A directory walk is attacker-influenced input and needs its own bound** (charter-app#152).
/// The declared list was bounded by the manifest; a tree is bounded by whatever is on the disk,
/// so a hostile or merely enormous directory would otherwise stall the launch that draws the
/// window. Directories count as entries too, which is also what bounds the walk's depth without
/// a second number to keep in step.
///
/// Measured rather than guessed at: at this bound a re-hash costs the numbers in #152's PR body.
const MOST_TREE_ENTRIES: usize = 4096;

/// The most bytes charter will hash inside one extension.
///
/// [`MOST_TREE_ENTRIES`] bounds the opens and this bounds the reading, because one file can be
/// as large as the disk. Nothing is held: files are fed to the hasher in chunks, so this is a
/// bound on time and not on memory.
const MOST_TREE_BYTES: u64 = 64 << 20;

/// How many bytes charter reads at a time out of a file it is hashing.
const CHUNK: usize = 64 << 10;

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
///
/// **"every file in this extension's directory" is load-bearing and is charter-app#152's whole
/// point.** It used to say "this extension's files", which the operator reads as *all of them*
/// and which the code meant as *the declared ones*. The sentence is now true of what [`tree`]
/// does; the one thing it is not true of is the state directory, and that exception is said
/// where the operator consents — [`state_note`] — rather than left in a doc comment.
pub const FINGERPRINTED: &str = "charter has read every file in this extension's directory — \
     not only the ones it declares — and will ask again if any of them changes, or if one is \
     added or taken away. That catches an extension that changed under you. It is not a defence \
     against one written to deceive you, and it is not a boundary.";

/// The sentence charter says about the one directory it does not read, when there is one.
///
/// **It is on the screen because the exception is the operator's to weigh, not charter's.**
/// [`FINGERPRINTED`] says charter read everything; if that is true of everything *but* a
/// directory, the operator has to be told which directory and what charter still guarantees
/// about it — otherwise the wording has the same defect #152 opened over, one carve-out later.
pub fn state_note(state: &str) -> String {
    format!(
        "charter does not read '{state}/'. That is this extension's state directory: what is \
         written there does not make charter ask you again. What is written anywhere else in \
         this extension's directory does — and what its program writes outside that directory \
         charter does not see at all, because it runs as you do. charter refuses to load the \
         extension if '{state}/' holds a link or a program, so what is in there is data — but \
         charter cannot stop a program it has already read from treating its own data as code."
    )
}

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
    /// The protocol its program speaks, as the manifest's `version` says it (ADR 0053). A
    /// manifest from before capabilities says `1`, which is the only protocol there is yet.
    pub protocol: u32,
    /// The capabilities it asks charter for ([`capability`]), in the order it names them.
    /// Empty for a manifest that names none, which is every manifest written before the list
    /// existed.
    pub capabilities: Vec<Capability>,
    /// How charter names it in the record. One path segment, so that an id can never be a
    /// path — the record is keyed by it and a `../` here would be a key that means a file.
    pub id: String,
    /// What a person calls it.
    pub name: String,
    /// The themes it contributes.
    pub themes: Vec<Theme>,
    /// The panels it contributes to the window's side region (`crate::panel`).
    ///
    /// **Data, like a theme, and never a file.** A panel's whole body is in this manifest — so
    /// it is inside the manifest's own bytes, which are hashed into the fingerprint with
    /// everything else, and there is no second file for a later read to disagree with.
    pub panels: Vec<crate::panel::Panel>,
    /// The views it contributes: surfaces the operator opens, which charter fills by asking
    /// [`Self::program`]. Declaring one requires declaring the program.
    pub views: Vec<View>,
    /// A program it declares, relative to its own directory.
    ///
    /// **Declared, hashed, named in the prompt, and started only by [`crate::executor`]** —
    /// when the operator opens one of [`Self::views`], and only while [`Loaded::standing`] says
    /// the bytes on disk are the bytes he approved. A program declared with no view is never
    /// started at all: nothing would ask it anything.
    pub program: Option<String>,
    /// The one directory charter does not fingerprint, and the only place this extension may
    /// write without being asked about again (charter-app#152).
    ///
    /// **One segment, directly inside the extension's own directory.** A nested carve-out
    /// (`build/tmp/state`) would be a hole the operator cannot see from a directory listing, and
    /// the whole value of the exclusion being declared is that it is readable at a glance and
    /// hashed along with the rest of the manifest. Nothing this manifest declares may live under
    /// it, so charter opens nothing in there, and [`state_holds_no_code`] refuses the extension
    /// if what *is* in there is a link or an executable file.
    ///
    /// Absent is the ordinary case: an extension that never writes beside itself declares no
    /// state directory and has no exclusion at all.
    pub state: Option<String>,
    /// The settings a project may choose for it (charter-app#253, ADR 0048), handed to
    /// [`Self::program`] with each question. Declaring one requires declaring the program: a
    /// setting nothing reads is a control that does nothing.
    pub settings: Vec<Setting>,
    /// The status badges it declares (the `badges` capability, [`facts`]): drawn from its facts
    /// file, never by starting its program.
    pub badges: Vec<facts::DeclaredBadge>,
    /// The repo-table columns it declares (the `repo-columns` capability, [`facts`]).
    pub repo_columns: Vec<facts::DeclaredColumn>,
    /// The events it hears, and the folder it keeps in each workspace (the `events`
    /// capability, [`events`]). `None` for an extension that hears nothing.
    pub events: Option<events::Declared>,
    /// Its section of the session-start briefing (the `briefing` capability, [`briefing`]).
    pub briefing: Option<briefing::Declared>,
}

impl Manifest {
    /// Whether it hears `kind` ([`events`]).
    pub fn hears(&self, kind: events::Kind) -> bool {
        self.events
            .as_ref()
            .is_some_and(|declared| declared.hears.contains(&kind))
    }
}

/// One setting an extension declares: a key a project sets in `[extensions.<id>.settings]`.
///
/// **Declared in the manifest, so it is inside the fingerprint** and the operator is shown it
/// before saying yes. A project chooses a value; it cannot add a key, and a value that is not
/// one this declaration accepts is ignored with a sentence (`project::resolve`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Setting {
    /// One segment: letters, digits, '-' and '_'.
    pub key: String,
    /// What the form calls it.
    pub title: String,
    pub kind: SettingKind,
    /// What it is when no file sets it.
    pub default: SettingValue,
}

/// What a setting holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingKind {
    Bool,
    Text,
    /// One of these words, and nothing else.
    Choice(Vec<String>),
}

/// A setting's value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingValue {
    Bool(bool),
    Text(String),
}

impl SettingValue {
    /// As the program is handed it.
    pub fn to_json(&self) -> serde_json::Value {
        match self {
            Self::Bool(b) => (*b).into(),
            Self::Text(text) => text.as_str().into(),
        }
    }
}

/// The most settings one extension may declare. Each one is a row in two forms.
const MOST_SETTINGS: usize = 16;

/// The most a text setting may be, declared or set. A setting is a word or a line, not a file.
pub const MOST_SETTING_BYTES: usize = 200;

impl Setting {
    /// `value`, as found in a settings file, if this setting accepts it — or why not, as the end
    /// of a sentence that starts with what set it.
    pub fn accepts(&self, value: &toml::Value) -> Result<SettingValue, String> {
        match (&self.kind, value) {
            (SettingKind::Bool, toml::Value::Boolean(b)) => Ok(SettingValue::Bool(*b)),
            (SettingKind::Bool, _) => Err("and it is true or false".into()),
            (SettingKind::Text, toml::Value::String(text)) if text_ok(text) => {
                Ok(SettingValue::Text(text.clone()))
            }
            (SettingKind::Text, _) => Err(format!(
                "and it is one line of text of at most {MOST_SETTING_BYTES} bytes"
            )),
            (SettingKind::Choice(words), toml::Value::String(text))
                if words.iter().any(|word| word == text) =>
            {
                Ok(SettingValue::Text(text.clone()))
            }
            (SettingKind::Choice(words), _) => {
                Err(format!("and it is one of {}", words.join(", ")))
            }
        }
    }
}

/// Whether `text` may be a text setting: short, and nothing in it draws as nothing.
fn text_ok(text: &str) -> bool {
    text.len() <= MOST_SETTING_BYTES && !text.contains(crate::panel::undrawable)
}

/// The settings a manifest's top-level `settings` declares, or why charter will not read them.
fn settings_of(value: &serde_json::Value, program: Option<&str>) -> Result<Vec<Setting>, String> {
    let list = value
        .as_array()
        .ok_or("has a 'settings' that is not an array")?;
    if list.is_empty() {
        return Ok(Vec::new());
    }
    if program.is_none() {
        return Err(
            "declares settings and no program ('runs') to hand them to, so a project \
                    would be choosing values nothing reads"
                .into(),
        );
    }
    if list.len() > MOST_SETTINGS {
        return Err(format!(
            "declares {} settings, and charter draws at most {MOST_SETTINGS} for one extension",
            list.len()
        ));
    }
    let mut out: Vec<Setting> = Vec::with_capacity(list.len());
    for (at, raw) in list.iter().enumerate() {
        let object = raw
            .as_object()
            .ok_or_else(|| format!("declares a setting at {at} that is not an object"))?;
        for key in object.keys() {
            if !SETTING_KEYS.contains(&key.as_str()) {
                return Err(format!(
                    "declares a setting at {at} carrying {key:?}, which is not part of what a \
                     setting may say — a setting is {}",
                    SETTING_KEYS.join(", ")
                ));
            }
        }
        let key = object
            .get("key")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| format!("declares a setting at {at} with no key"))?;
        if !project::key_ok(key) {
            return Err(format!(
                "declares the setting {key:?}, and a setting's key is letters, digits, '-' and \
                 '_', starting with a letter or a digit"
            ));
        }
        if out.iter().any(|seen| seen.key == key) {
            return Err(format!("declares two settings called {key:?}"));
        }
        let title = object
            .get("title")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|title| !title.is_empty())
            .unwrap_or(key);
        if !text_ok(title) {
            return Err(format!(
                "declares the setting {key:?} with a title charter will not draw: it is longer \
                 than {MOST_SETTING_BYTES} bytes or holds a control or invisible formatting \
                 character"
            ));
        }
        let kind = match object.get("type").and_then(serde_json::Value::as_str) {
            Some("bool") => SettingKind::Bool,
            Some("text") => SettingKind::Text,
            Some("choice") => {
                let words: Vec<String> = object
                    .get("choices")
                    .and_then(serde_json::Value::as_array)
                    .map(|words| {
                        words
                            .iter()
                            .filter_map(serde_json::Value::as_str)
                            .map(str::to_owned)
                            .collect()
                    })
                    .unwrap_or_default();
                if words.is_empty() || words.iter().any(|word| word.is_empty() || !text_ok(word)) {
                    return Err(format!(
                        "declares the choice setting {key:?} without a list of words to choose \
                         from"
                    ));
                }
                SettingKind::Choice(words)
            }
            _ => {
                return Err(format!(
                    "declares the setting {key:?} with no type charter knows — a setting's type \
                     is bool, text or choice"
                ));
            }
        };
        let mut setting = Setting {
            key: key.to_owned(),
            title: title.to_owned(),
            default: match &kind {
                SettingKind::Bool => SettingValue::Bool(false),
                SettingKind::Text => SettingValue::Text(String::new()),
                SettingKind::Choice(words) => SettingValue::Text(words[0].clone()),
            },
            kind,
        };
        if let Some(default) = object.get("default") {
            let as_toml = match default {
                serde_json::Value::Bool(b) => toml::Value::Boolean(*b),
                serde_json::Value::String(text) => toml::Value::String(text.clone()),
                _ => {
                    return Err(format!(
                        "declares the setting {key:?} with a default that is neither true, false \
                         nor text"
                    ));
                }
            };
            setting.default = setting.accepts(&as_toml).map_err(|why| {
                format!("declares the setting {key:?} with a default it would not accept, {why}")
            })?;
        }
        out.push(setting);
    }
    Ok(out)
}

/// The keys a declared setting may carry.
const SETTING_KEYS: [&str; 5] = ["key", "title", "type", "choices", "default"];

/// One view an extension contributes: a surface the operator opens, filled by its program.
///
/// **Its id, a title, and what it is about. Nothing that says where.** charter decides where a
/// view about [`crate::panel::Subject::Personas`] is offered — on the personas panel's heading
/// and in a persona's card — and the view cannot ask for a region, a size or a key. That is ADR
/// 0043's property 3 for a panel, carried to the one thing an extension contributes that runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct View {
    /// One segment, unique within the extension. It is what the window asks for by name.
    pub id: String,
    /// What the button and the surface say.
    pub title: String,
    /// What it is about, which is also what charter hands its program ([`crate::handed`]).
    pub about: crate::panel::Subject,
}

/// The most views one extension may contribute. Each one is a button charter puts on one of
/// its own surfaces, and a heading with twenty buttons on it is a heading nobody reads.
const MOST_VIEWS: usize = 8;

/// The keys a declared view may carry. Anything else refuses the extension, for the reason
/// `panel::declared` gives.
const VIEW_KEYS: [&str; 3] = ["id", "title", "about"];

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
    /// sha256, hex, over every path below [`Self::path`] except the state directory. See
    /// [`tree`].
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
    let (text, manifest) = read_manifest(dir)?;

    // Before the walk, because the walk does not descend into it and this is the only thing
    // that answers for what is in there (charter-app#152).
    let mut budget = MOST_TREE_ENTRIES;
    if let Some(state) = &manifest.state {
        state_holds_no_code(dir, state, &mut budget)?;
    }

    // The fingerprint is taken over the bytes just read and the bytes about to be read, the
    // declarations handed back come out of the same read, and the theme text the window will
    // draw with is kept from it rather than fetched again. Two reads is charter-app#123.
    let (fingerprint, theme_text) = tree(dir, text.as_bytes(), &manifest, budget)?;

    Ok(Extension {
        path: dir.to_path_buf(),
        manifest,
        fingerprint,
        theme_text,
    })
}

/// The manifest in `dir` and nothing else: [`read_at`]'s checks on the directory and the one
/// file, without the walk and without a fingerprint.
///
/// **Never evidence of approval** — there is no fingerprint to compare. It is for a caller that
/// has to know what an extension declares *before* it pays for the walk and will take
/// [`read_at`] afterwards anyway: the executor reads which view it was asked about here, builds
/// the question, and only then re-takes the fingerprint, so that the fingerprint is as close to
/// the program starting as it can be (`crate::executor`).
pub fn manifest_at(dir: &Path) -> Result<Manifest, String> {
    read_manifest(dir).map(|(_, manifest)| manifest)
}

/// The directory's checks and the manifest's read, shared by [`read_at`] and [`manifest_at`] so
/// that the two cannot disagree about what a manifest is.
fn read_manifest(dir: &Path) -> Result<(String, Manifest), String> {
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

    let at = dir.join(MANIFEST);
    let text =
        slurp(dir, &at, MOST_MANIFEST_BYTES).map_err(|why| format!("'{}' {why}", at.display()))?;
    let manifest = parse(&text).map_err(|why| format!("'{}' {why}", at.display()))?;
    Ok((text, manifest))
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
    let (mut open, _) = plain_file(root, path, Some(most))?;
    let mut text = String::new();
    io::Read::read_to_string(&mut open, &mut text)
        .map_err(|why| format!("could not be read: {why}"))?;
    Ok(text)
}

/// [`slurp`]'s open, alone: the gated open and the checks on the descriptor it returned, with
/// the length the read may rely on.
///
/// Split out for [`tree`], which reads in chunks rather than whole and cannot go through a
/// `String` — an extension's directory holds binaries, and a `.dylib` is not UTF-8.
fn plain_file(root: &Path, path: &Path, most: Option<u64>) -> Result<(std::fs::File, u64), String> {
    let open = crate::contain::open_no_link(root, path).map_err(|why| match why.kind() {
        io::ErrorKind::NotFound => NOT_THERE.to_owned(),
        _ => format!("could not be opened: {why}"),
    })?;
    // A FIFO here is not a slow read, it is a permanent one, and this runs on the path that
    // draws the window — so the type is asked before a byte is taken. It is asked of the
    // DESCRIPTOR, which is the file the read will use: the gate is on the exact path that was
    // opened, never on a `stat` of it a moment earlier.
    let found = open
        .metadata()
        .map_err(|why| format!("could not be read: {why}"))?;
    if !found.file_type().is_file() {
        return Err("is not a plain file, and charter reads an extension from nothing else".into());
    }
    if let Some(most) = most
        && found.len() > most
    {
        return Err(format!(
            "is {} bytes, and charter reads no more than {most} here",
            found.len()
        ));
    }
    Ok((open, found.len()))
}

/// A manifest's text, as a manifest, or why it is not one.
fn parse(text: &str) -> Result<Manifest, String> {
    let doc: serde_json::Value =
        serde_json::from_str(text).map_err(|why| format!("is not JSON: {why}"))?;
    let doc = doc.as_object().ok_or("is not a JSON object")?;

    // **`version` is the protocol the extension speaks** (ADR 0053), and not the record's
    // [`VERSION`]: the two were the same number by coincidence, and they move apart the day a
    // capability changes what a request or an answer holds. charter keeps answering every
    // protocol up to its own, so a manifest written for an older charter keeps loading.
    let protocol = match doc.get("version").and_then(serde_json::Value::as_u64) {
        Some(found) if (1..=u64::from(crate::executor::PROTOCOL)).contains(&found) => {
            u32::try_from(found).map_err(|_| "has a version charter cannot hold".to_owned())?
        }
        Some(found) => {
            let reads = match crate::executor::PROTOCOL {
                1 => "version 1".to_owned(),
                newest => format!("versions 1 to {newest}"),
            };
            return Err(format!(
                "is version {found}, and this charter reads {reads}"
            ));
        }
        None => return Err("says no version, so charter cannot say what it means".into()),
    };
    // Before anything it would be asked to do: a capability this charter does not know refuses
    // the whole manifest, so nothing below is ever loaded for less than it asked for.
    let capabilities = match doc.get("capabilities") {
        None => Vec::new(),
        Some(value) => capability::declared(value)?,
    };

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
    // The name is what the consent dialog asks about, so it is held to what a view's title is
    // held to: nothing in it may draw as nothing, or turn the words beside it around.
    if name.contains(crate::panel::undrawable) {
        return Err(format!(
            "has a name holding a control or invisible formatting character ({:?}), which \
             charter will not draw where it asks you about the extension",
            name
        ));
    }

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

    // **The second word in the vocabulary, and the one this charter grew for**
    // (`crate::panel`). A panel is declarative data against a closed vocabulary charter owns,
    // exactly as a theme is — no file is named, nothing is evaluated, and a row that tried to
    // carry a charter verb is refused by name (`panel::NO_VERB`).
    //
    // It is parsed here, from these bytes, so that what the fingerprint was taken over and what
    // the operator is shown are one read (charter-app#123's shape). Note that a panel declares
    // no FILE: it does not touch `MOST_DECLARED_FILES` below, and it is bounded by
    // `panel::MOST_PANELS` and `panel::MOST_ROWS` instead.
    let panels = match contributes.get("panels") {
        None => Vec::new(),
        Some(value) => crate::panel::declared(value, id)?,
    };

    // **The one word that runs something.** Parsed here, from these bytes, for the reason
    // panels are: the fingerprint and the declarations have to be one read.
    let views = match contributes.get("views") {
        None => Vec::new(),
        Some(value) => views_of(value, program.as_deref())?,
    };

    // **A capability's shape is in `contributes` under its own word, and the two come as a
    // pair** (ADR 0053): a shape whose capability is not listed would be a contribution the
    // prompt never named, and a listed capability that declares nothing is a yes to nothing.
    for capability in Capability::known() {
        if !capability.has_shape() {
            continue;
        }
        let word = capability.as_str();
        match (
            capabilities.contains(&capability),
            contributes.contains_key(word),
        ) {
            (false, true) => {
                return Err(format!(
                    "declares 'contributes.{word}' without asking for the capability \"{word}\" \
                     in 'capabilities', so you would be approving it without being told"
                ));
            }
            (true, false) => {
                return Err(format!(
                    "asks for the capability \"{word}\" and declares no 'contributes.{word}', \
                     so there is nothing it would do"
                ));
            }
            _ => {}
        }
    }
    let badges = match contributes.get(Capability::Badges.as_str()) {
        None => Vec::new(),
        Some(value) => facts::badges_of(value)?,
    };
    let repo_columns = match contributes.get(Capability::RepoColumns.as_str()) {
        None => Vec::new(),
        Some(value) => facts::columns_of(value)?,
    };
    // A capability that is a request kind of its own is refused in a manifest that speaks a
    // protocol without it (ADR 0053): its program would be asked a question it was not written
    // to read.
    for capability in &capabilities {
        let since = capability.since_protocol();
        if protocol < since {
            return Err(format!(
                "asks for the capability \"{}\" and speaks protocol {protocol}, which does not \
                 have it — a manifest that asks for it says \"version\": {since} or later",
                capability.as_str()
            ));
        }
    }
    let events = match contributes.get(Capability::Events.as_str()) {
        None => None,
        Some(value) => Some(events::declared_of(value)?),
    };
    let briefing = match contributes.get(Capability::Briefing.as_str()) {
        None => None,
        Some(value) => Some(briefing::declared_of(value)?),
    };
    // Both are questions to the program, so each needs one to ask — for the reason a view
    // does: a contribution that can never do anything is one the operator consented to and
    // did not get.
    if (events.is_some() || briefing.is_some()) && program.is_none() {
        return Err(
            "hears events or adds a briefing section, and declares no program ('runs') to ask"
                .into(),
        );
    }

    if themes.is_empty()
        && panels.is_empty()
        && views.is_empty()
        && program.is_none()
        && badges.is_empty()
        && repo_columns.is_empty()
    {
        return Err("declares no contributions, so there is nothing to consent to".into());
    }
    if themes.len() + usize::from(program.is_some()) > MOST_DECLARED_FILES {
        return Err(format!(
            "declares more than {MOST_DECLARED_FILES} files, and charter puts every one of them \
             in front of the operator"
        ));
    }

    // The one carve-out in the fingerprint (charter-app#152), and every rule on it is here
    // rather than at the walk: this is where the string arrives, and a carve-out charter
    // accepted and then worked around at the walk would be two answers to one question.
    let state = match doc.get("state") {
        None => None,
        Some(value) => {
            let named = value
                .as_str()
                .ok_or("names a 'state' that is not a directory")?;
            if !crate::contain::segment_ok(named) {
                return Err(format!(
                    "names the state directory {named:?}, and a state directory is one plain \
                     name directly inside the extension — charter will not carve a hole out of \
                     its fingerprint that cannot be seen in a directory listing"
                ));
            }
            if named == MANIFEST {
                return Err(format!(
                    "names {MANIFEST} as its state directory, which is the file charter reads it \
                     from"
                ));
            }
            // Nothing charter opens may be inside the one directory charter does not read. The
            // alternative is an extension whose theme or program is exempt from the fingerprint
            // by declaration, which is #152 with the hole moved rather than closed.
            for file in themes
                .iter()
                .map(|theme| theme.file.as_str())
                .chain(program.as_deref())
            {
                if Path::new(file)
                    .components()
                    .next()
                    .is_some_and(|first| first.as_os_str() == named)
                {
                    return Err(format!(
                        "declares {file:?} inside its state directory {named:?}, which charter \
                         does not read — so charter would be asked to show the operator a file \
                         it never hashed"
                    ));
                }
            }
            Some(named.to_owned())
        }
    };

    // The facts file lives in the state directory, so a badge or a column with none has
    // nowhere to be filled from.
    if (!badges.is_empty() || !repo_columns.is_empty()) && state.is_none() {
        return Err(format!(
            "declares badges or repo columns and names no state directory ('state'), which is \
             where its facts file ({}) is",
            facts::FILE
        ));
    }

    let settings = match doc.get("settings") {
        None => Vec::new(),
        Some(value) => settings_of(value, program.as_deref())?,
    };

    Ok(Manifest {
        protocol,
        capabilities,
        id: id.to_owned(),
        name,
        themes,
        panels,
        views,
        program,
        state,
        settings,
        badges,
        repo_columns,
        events,
        briefing,
    })
}

/// The views a manifest's `contributes.views` declares, or why charter will not read them.
///
/// **A view with no program is refused rather than drawn as a button that does nothing**, and
/// a view about a subject charter does not publish is refused rather than drawn as one that is
/// handed nothing: each would be a contribution the operator consented to and then did not get.
fn views_of(value: &serde_json::Value, program: Option<&str>) -> Result<Vec<View>, String> {
    let list = value
        .as_array()
        .ok_or("has a 'contributes.views' that is not an array")?;
    if list.is_empty() {
        return Ok(Vec::new());
    }
    if program.is_none() {
        return Err(
            "declares a view and no program ('runs') to answer it, so charter would \
             draw a button that can never do anything"
                .into(),
        );
    }
    if list.len() > MOST_VIEWS {
        return Err(format!(
            "declares {} views, and charter offers at most {MOST_VIEWS} from one extension",
            list.len()
        ));
    }
    let mut views: Vec<View> = Vec::with_capacity(list.len());
    for (at, raw) in list.iter().enumerate() {
        let object = raw
            .as_object()
            .ok_or_else(|| format!("declares a view at {at} that is not an object"))?;
        for key in object.keys() {
            if !VIEW_KEYS.contains(&key.as_str()) {
                return Err(format!(
                    "declares a view at {at} carrying {key:?}, which is not part of what a view \
                     may say — a view is {}",
                    VIEW_KEYS.join(", ")
                ));
            }
        }
        let id = object
            .get("id")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| format!("declares a view at {at} with no id"))?;
        if !crate::contain::segment_ok(id)
            || !id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
            || !id.starts_with(|c: char| c.is_ascii_alphanumeric())
        {
            return Err(format!(
                "declares the view id {id:?}, and a view's id is letters, digits, '-' and '_', \
                 starting with a letter or a digit"
            ));
        }
        if views.iter().any(|seen| seen.id == id) {
            return Err(format!(
                "declares two views called {id:?}, and a view's id is how charter asks for it"
            ));
        }
        let title = object
            .get("title")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|title| !title.is_empty())
            .ok_or_else(|| format!("declares the view {id:?} with no title"))?;
        // `panel::undrawable`, not `is_control`: a title is drawn with ` · <extension id>`
        // after it, and a bidirectional override in the title would draw that name backwards.
        if title.len() > 200 || title.contains(crate::panel::undrawable) {
            return Err(format!(
                "declares the view {id:?} with a title charter will not draw on a button: it is \
                 longer than 200 bytes or holds a control or invisible formatting character"
            ));
        }
        let about = object
            .get("about")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| format!("declares the view {id:?} without saying what it is about"))?;
        let about = crate::panel::Subject::parse(about).ok_or_else(|| {
            let every: Vec<&str> = crate::panel::Subject::every()
                .iter()
                .map(|it| it.as_str())
                .collect();
            format!(
                "declares the view {id:?} about {about:?}, and charter has something to hand a \
                 view about {} and nothing else",
                every.join(", ")
            )
        })?;
        views.push(View {
            id: id.to_owned(),
            title: title.to_owned(),
            about,
        });
    }
    Ok(views)
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
    // The manifest is already a part of the digest, taken over the bytes that were parsed rather
    // than a second read of the same name (charter-app#123, and [`tree`]'s manifest arm). A
    // manifest that declared ITSELF as a theme would ask for those bytes back as theme text,
    // which the tree walk does not keep — and the theme would then be dropped without a word,
    // which looks exactly like a theme that did nothing wrong.
    if file == MANIFEST {
        return Err("is the manifest itself, which charter already reads as the manifest".into());
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

/// Which scheme the fingerprint was taken under, inside the hash.
///
/// `1` was the manifest and the declared list; `2` is the directory tree (charter-app#152).
/// It is in the digest's first part so that **widening what is hashed re-asks rather than
/// silently matching**, and it is a number of its own rather than [`VERSION`] because the
/// manifest's version is a promise to extension authors about a file format and this is a
/// promise to the operator about what a yes covered. The two must be able to move apart.
///
/// Every extension approved under scheme 1 is asked about once more, and that is correct: the
/// question being asked is not the one that was answered.
const FINGERPRINT_SCHEME: u32 = 2;

/// What one entry of an extension's directory is, to the hash.
///
/// **A symlink is its own kind and is never followed** (charter-app#152). Its target *string*
/// is what is hashed, so re-pointing it asks again, a link out of the tree reads nothing out
/// there, and a loop cannot hang a walk that never walks *through* anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Dir,
    File,
    Link,
}

impl Kind {
    /// The letter this kind contributes to a part's name. Fixed width, so a path that begins
    /// with a space or a letter cannot be read as a different kind.
    fn letter(self) -> char {
        match self {
            Self::Dir => 'd',
            Self::File => 'f',
            Self::Link => 'l',
        }
    }
}

/// One path below an extension's directory, before it is read.
///
/// Named for what it is rather than "entry", because [`Entry`] in this module is a row of the
/// record and the two are a sentence apart everywhere they are both mentioned.
#[derive(Debug, Clone)]
struct Found {
    /// Relative to the extension's own directory, `/`-separated on every platform, so the hash
    /// does not change with the separator the operator's machine writes.
    rel: String,
    kind: Kind,
    /// Whether any executable bit is set. **In the hash**, so `chmod +x` on a file that was
    /// already there is a change the operator is asked about — it is the smallest edit that
    /// turns data into something a shell will run.
    runnable: bool,
}

impl Found {
    /// The name this entry goes into the digest under: kind, executable bit, path — the first
    /// three characters fixed, so nothing about a path can be read as one of the other two.
    fn part_name(&self) -> String {
        let runnable = if self.runnable { 'x' } else { '-' };
        format!("{}{runnable} {}", self.kind.letter(), self.rel)
    }
}

/// The sha256, hex, of everything an extension is: **every path below its directory except its
/// state directory**, contents and names alike.
///
/// **ADR 0041's difference from ADR 0035, widened by charter-app#152, and it costs something.**
/// 0035 fingerprints *configuration* — two settings keys read out of a file. This fingerprints
/// **code**, and since #152 it fingerprints the code an extension did not declare as well: a
/// `.dylib` beside the program, a script it sources, a file added after approval. An
/// extension's path is not its contents, an extension that rewrites itself after approval is
/// the attack the whole record is about, and *the declared list was the attacker's to choose*.
/// The cost is named here rather than discovered when the app gets slower: it is bounded by
/// [`MOST_TREE_ENTRIES`] and [`MOST_TREE_BYTES`], and it is measured in #152's PR body.
///
/// **The set of paths is hashed, not only the contents.** Every entry contributes a part whose
/// *name* is its kind, its executable bit and its relative path, so a file that is added or
/// taken away changes the fingerprint exactly as an edited one does. A directory is a part with
/// no bytes, so an empty directory appearing is a change too.
///
/// **The parts are length-framed** ([`Framed`]) and sorted by path, so the fingerprint does not
/// depend on the order the filesystem happened to hand entries back. Since #152 the framing is
/// load-bearing in the way [`digest`]'s docstring says it was only waiting to be: the parts are
/// now paths nothing else names, so `f- a/b` holding `c` and `f- a` holding `b/c`-ish bytes are
/// exactly the pair the lengths keep apart.
///
/// It hands back the theme text alongside the digest for the reason [`Extension::theme_text`]
/// gives: the bytes the window draws with must be the bytes that were hashed, and a second read
/// to fetch them would reopen charter-app#123 one level down.
fn tree(
    dir: &Path,
    manifest_bytes: &[u8],
    manifest: &Manifest,
    budget: usize,
) -> Result<(String, BTreeMap<String, String>), String> {
    let found = survey_tree(dir, manifest.state.as_deref(), budget)?;

    let mut framed = Framed::new();
    // A domain tag first, so this digest can never equal one taken over the same bytes for
    // another purpose, and the scheme with it.
    framed.part("charter-extension", &FINGERPRINT_SCHEME.to_be_bytes());
    // **What a yes to a program covers, inside the hash** (ADR 0041 stage 2). Until the
    // executor existed the prompt said of a declared program *"this charter has no extension
    // runtime and does not start it"*, and an operator who approved one approved THAT sentence.
    // Starting the program on that yes would be running something he said yes to under words
    // that are no longer true, so an extension that declares a program carries the executor's
    // protocol in its fingerprint: every one approved before this existed reads as changed and
    // is asked about again, with the prompt that says it will run. A theme-only extension is
    // not touched — nothing about what its yes covered has moved.
    //
    // **The protocol the manifest declares, not the executor's newest** (ADR 0053, from
    // charter-app#343): charter asks a program in the protocol it declared, so what a yes to it
    // covered moves only when the extension moves it — and a charter that learns protocol 3
    // re-asks nobody about an extension still speaking 1. For protocol 1 these are the bytes
    // the executor's own number was hashed as, so no extension approved before is asked again.
    if manifest.program.is_some() {
        framed.part("charter-starts-it", &manifest.protocol.to_be_bytes());
    }

    let wanted: BTreeMap<&str, ()> = manifest
        .themes
        .iter()
        .map(|theme| (theme.file.as_str(), ()))
        .collect();
    let declared: Vec<&str> = wanted
        .keys()
        .copied()
        .chain(manifest.program.as_deref())
        .collect();

    let mut theme_text = BTreeMap::new();
    let mut seen_the_manifest = false;
    let mut spent = 0u64;
    for entry in &found {
        let at = dir.join(&entry.rel);
        match entry.kind {
            Kind::Dir => framed.part(&entry.part_name(), &[]),
            Kind::Link => {
                // Read, never followed. `read_link` answers about the last component only, so
                // what goes into the hash is the link's own target string.
                let points = std::fs::read_link(&at)
                    .map_err(|why| format!("'{}' could not be read: {why}", at.display()))?;
                framed.part(&entry.part_name(), points.as_os_str().as_encoded_bytes());
            }
            Kind::File => {
                let is_the_manifest = entry.rel == MANIFEST;
                if is_the_manifest {
                    // **The bytes already read, not a second read of the same name.** The
                    // declarations handed back and the fingerprint have to come from one read
                    // or they are charter-app#123: a manifest rewritten between the parse and
                    // the walk would be consented to without having been shown.
                    seen_the_manifest = true;
                    framed.part(&entry.part_name(), manifest_bytes);
                    continue;
                }
                let keeping = wanted.contains_key(entry.rel.as_str());
                let kept = hash_one(dir, &at, entry, &mut framed, &mut spent, keeping)?;
                if let Some(text) = kept {
                    theme_text.insert(entry.rel.clone(), text);
                }
            }
        }
    }

    if !seen_the_manifest {
        return Err(format!(
            "'{}' went away while charter was reading the extension, so charter cannot say what \
             it just read",
            dir.join(MANIFEST).display()
        ));
    }
    // A declared file that is not in the tree is not "hashed as absent": putting it there
    // afterwards would contribute something nobody was asked about. The walk is what tells the
    // difference, because a declared file that turned out to be a link or a directory was
    // recorded as one and was never opened as a file.
    for file in declared {
        // "not there" and "there, and not a file" are two different things to tell the
        // operator, and `NOT_THERE`'s wording is the one every other absence in this module
        // uses. Nothing stranger than a link or a directory reaches this arm: the walk refuses
        // a FIFO, a socket or a device outright, because charter cannot fingerprint one.
        match found.iter().find(|entry| entry.rel == file) {
            Some(entry) if entry.kind == Kind::File => {}
            Some(_) => {
                return Err(format!(
                    "'{}' is declared by this extension and is not a plain file, and charter \
                     reads an extension from nothing else",
                    dir.join(file).display()
                ));
            }
            None => {
                return Err(format!("'{}' {NOT_THERE}", dir.join(file).display()));
            }
        }
    }

    Ok((framed.finish(), theme_text))
}

/// Hash one plain file, in chunks, and hand back its text when it is one charter keeps.
///
/// **Chunks and not a whole read**, so [`MOST_TREE_BYTES`] bounds time rather than memory: an
/// extension holding one file as large as the disk is refused by the budget without charter
/// having held it. The length is taken from the DESCRIPTOR and is what goes into the framing,
/// and a file that turns out to be a different length is refused rather than framed as a length
/// it did not have.
fn hash_one(
    dir: &Path,
    at: &Path,
    entry: &Found,
    framed: &mut Framed,
    spent: &mut u64,
    keeping: bool,
) -> Result<Option<String>, String> {
    // Every refusal below names the file it is about, and the byte budget names the extension:
    // "this file is not a plain file" and "this extension is too big to hash" are answers to
    // two different questions and the operator is owed the right one.
    let about = |why: String| format!("'{}' {why}", at.display());
    let (mut open, len) = plain_file(dir, at, None).map_err(about)?;
    *spent = spent.saturating_add(len);
    if *spent > MOST_TREE_BYTES {
        return Err(too_many_bytes(dir));
    }
    if keeping && len > MOST_DECLARED_BYTES {
        return Err(about(format!(
            "is {len} bytes, and charter holds no more than {MOST_DECLARED_BYTES} of a file it \
             draws with"
        )));
    }
    framed.begin(&entry.part_name(), len);
    let mut kept = keeping.then(Vec::new);
    let mut buffer = vec![0u8; CHUNK];
    let mut left = len;
    while left > 0 {
        let want = usize::try_from(left.min(CHUNK as u64)).unwrap_or(CHUNK);
        let got = io::Read::read(&mut open, &mut buffer[..want])
            .map_err(|why| about(format!("could not be read: {why}")))?;
        if got == 0 {
            return Err(about(SHRANK.to_owned()));
        }
        framed.feed(&buffer[..got]);
        if let Some(kept) = &mut kept {
            kept.extend_from_slice(&buffer[..got]);
        }
        left -= got as u64;
    }
    // The file it framed and the file it read have to be the same length, or the framing is a
    // claim about bytes that were never hashed.
    if io::Read::read(&mut open, &mut buffer[..1])
        .map_err(|why| about(format!("could not be read: {why}")))?
        != 0
    {
        return Err(about(SHRANK.to_owned()));
    }
    match kept {
        None => Ok(None),
        Some(bytes) => String::from_utf8(bytes)
            .map(Some)
            .map_err(|_| about("is drawn as text and is not valid UTF-8".to_owned())),
    }
}

/// What charter says about a file that moved under it while it was being hashed.
const SHRANK: &str = "changed while charter was reading it, so charter cannot say what it read";

/// The refusal when an extension is past a bound, saying **which** bound and what to do.
///
/// One sentence of advice for both, because there is only one thing to do about either: an
/// extension's directory is a directory holding an extension. A refusal that named a number and
/// stopped there would be the launch telling the operator that something is too big without
/// telling them what "too big" is measured against.
fn too_much(dir: &Path, past: &str) -> String {
    format!(
        "'{}' holds more than {past}, and charter reads all of it at every launch to see \
         whether the extension changed. Point charter at a directory holding the extension and \
         nothing else: a build tree, a virtualenv or a clone of something large belongs outside \
         it.",
        dir.display()
    )
}

/// [`too_much`] for the entry bound.
fn too_many_entries(dir: &Path) -> String {
    too_much(dir, &format!("{MOST_TREE_ENTRIES} files and directories"))
}

/// [`too_much`] for the byte bound.
fn too_many_bytes(dir: &Path) -> String {
    too_much(dir, &format!("{MOST_TREE_BYTES} bytes charter would hash"))
}

/// Every path below `dir` except `state`, sorted, with nothing read yet.
///
/// **The walk descends only into directories it saw with its own `symlink_metadata`**, and it
/// follows nothing: a symlink is recorded as one and is not a way in, which is what makes a
/// loop impossible and a link out of the tree unreadable. That the gate is then re-taken on the
/// exact path each file is *opened* by ([`plain_file`]) rather than on this walk's answer is
/// ADR 0028's rule and the reason the two are separate passes.
fn survey_tree(dir: &Path, state: Option<&str>, budget: usize) -> Result<Vec<Found>, String> {
    let mut found: Vec<Found> = Vec::new();
    let mut left = budget;
    // Relative directories still to look inside, the extension's own first.
    let mut todo = vec![String::new()];
    while let Some(rel) = todo.pop() {
        let at = if rel.is_empty() {
            dir.to_path_buf()
        } else {
            dir.join(&rel)
        };
        // A directory charter is about to list is a path like any other, so the containment
        // walk answers about it first. It is a `stat` and not a handle — ADR 0028 — but a
        // directory swapped for a link between the two is refused at the leaf regardless,
        // because every file below is opened with `O_NOFOLLOW` through the same gate.
        crate::contain::no_link_on_the_way(dir, &at)
            .map_err(|why| format!("'{}' {why}", at.display()))?;
        let listing = std::fs::read_dir(&at)
            .map_err(|why| format!("'{}' could not be listed: {why}", at.display()))?;
        for step in listing {
            let step =
                step.map_err(|why| format!("'{}' could not be listed: {why}", at.display()))?;
            let Some(name) = step.file_name().to_str().map(str::to_owned) else {
                return Err(format!(
                    "'{}' holds a name that is not valid UTF-8, and charter records what it \
                     hashed by name",
                    at.display()
                ));
            };
            let next = if rel.is_empty() {
                name.clone()
            } else {
                format!("{rel}/{name}")
            };
            // The one exclusion, and only at the top: `state` is one segment, so a `cache`
            // nested inside the tree is an ordinary directory and is hashed.
            if rel.is_empty() && state == Some(name.as_str()) {
                continue;
            }
            if left == 0 {
                return Err(too_many_entries(dir));
            }
            left -= 1;
            let found_it = step
                .path()
                .symlink_metadata()
                .map_err(|why| format!("'{}' could not be read: {why}", step.path().display()))?;
            let kind = if found_it.file_type().is_symlink() {
                Kind::Link
            } else if found_it.is_dir() {
                todo.push(next.clone());
                Kind::Dir
            } else if found_it.is_file() {
                Kind::File
            } else {
                return Err(format!(
                    "'{}' is not a file, a directory or a link, and charter cannot fingerprint \
                     what it cannot read. Take it out of the extension, or move it into the \
                     extension's state directory.",
                    step.path().display()
                ));
            };
            found.push(Found {
                rel: next,
                kind,
                runnable: runnable(&found_it),
            });
        }
    }
    // Sorted by path, so what the filesystem happened to hand back is not inside the hash.
    found.sort_by(|one, two| one.rel.cmp(&two.rel));
    Ok(found)
}

/// Whether anything may run this, as the filesystem answers it.
#[cfg(unix)]
fn runnable(found: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    found.permissions().mode() & 0o111 != 0
}

/// Windows has no mode bits charter could read this off, and this module refuses on Windows
/// anyway (ADR 0031, the module docstring, [`supported`]). `false` keeps the hash defined
/// rather than guessed at.
#[cfg(not(unix))]
fn runnable(_found: &std::fs::Metadata) -> bool {
    false
}

/// Refuse an extension whose state directory holds anything that could be loaded as code.
///
/// **This is the enforcement the carve-out has to carry** (charter-app#152). The state
/// directory is the one place [`tree`] does not look, so anything in it is a file the operator
/// will never be asked about — and a state directory that may hold a `.dylib` or a script the
/// program sources has given back the whole property the tree hash exists for.
///
/// What it refuses: a symlink (which would name code from anywhere without naming it here), and
/// any file with an executable bit. What it costs: one `symlink_metadata` per entry and not one
/// byte read, so a cache of ten thousand files is checked without being hashed and without
/// re-prompting anybody.
///
/// **Its limit, which belongs beside it and not in a later apology.** No filesystem predicate
/// makes a file un-loadable as code: `dlopen` does not need the executable bit on Linux and
/// `source` needs it nowhere. This closes the careless case and the conventional one. What it
/// leaves open is a program the operator approved choosing to read its own state as code, which
/// is the same class as one that fetches a string and evaluates it — and [`state_note`] says so
/// on the screen rather than here alone.
fn state_holds_no_code(dir: &Path, state: &str, budget: &mut usize) -> Result<(), String> {
    let root = dir.join(state);
    match std::fs::symlink_metadata(&root) {
        // Not there is the ordinary first launch: an extension that has not written anything
        // yet. The exclusion does not depend on the directory existing, so that the first write
        // is not itself a change.
        Err(_) => return Ok(()),
        Ok(found) if found.file_type().is_symlink() => {
            return Err(format!(
                "'{}' is this extension's state directory and is a symlink. charter does not \
                 read what is in there, so a link would put the one place this extension may \
                 write wherever it points. Make it a real directory.",
                root.display()
            ));
        }
        Ok(found) if !found.is_dir() => {
            return Err(format!(
                "'{}' is declared as this extension's state directory and is not a directory",
                root.display()
            ));
        }
        Ok(_) => {}
    }

    let mut todo = vec![root];
    while let Some(at) = todo.pop() {
        let listing = std::fs::read_dir(&at)
            .map_err(|why| format!("'{}' could not be listed: {why}", at.display()))?;
        for step in listing {
            let step =
                step.map_err(|why| format!("'{}' could not be listed: {why}", at.display()))?;
            if *budget == 0 {
                return Err(too_many_entries(dir));
            }
            *budget -= 1;
            let path = step.path();
            let found = path
                .symlink_metadata()
                .map_err(|why| format!("'{}' could not be read: {why}", path.display()))?;
            if found.file_type().is_symlink() {
                return Err(format!(
                    "'{}' is a symlink inside the state directory '{state}', which charter does \
                     not fingerprint — so it names code the operator would never be asked \
                     about. Move what it points at into the extension, where charter reads it.",
                    path.display()
                ));
            }
            if found.is_dir() {
                todo.push(path);
                continue;
            }
            if runnable(&found) {
                return Err(format!(
                    "'{}' can be run and is inside the state directory '{state}', which charter \
                     does not fingerprint — so it is a program the operator would never be \
                     asked about. Move it out of '{state}', where charter reads it and asks.",
                    path.display()
                ));
            }
        }
    }
    Ok(())
}

/// sha256 over a list of named parts — **length-framed**, so no two different lists can hash
/// the same.
///
/// Each part goes in as its name's length, its name, its bytes' length and its bytes.
/// Concatenating name and content without the lengths would let a part named `a` holding `bc`
/// collide with one named `ab` holding `c`, and a fingerprint with a collision in it is a
/// fingerprint that can be changed under the operator without asking again.
///
/// **It takes a part at a time** so that [`tree`] can feed a file through it in chunks: the
/// length is known from the descriptor before a byte is read, which is what lets the framing
/// stay honest without the file being held.
struct Framed(sha2::Sha256);

impl Framed {
    fn new() -> Self {
        use sha2::Digest as _;
        Self(sha2::Sha256::new())
    }

    /// One whole part.
    fn part(&mut self, name: &str, bytes: &[u8]) {
        self.begin(name, bytes.len() as u64);
        self.feed(bytes);
    }

    /// A part's name and the length of the bytes that follow, before they are read.
    fn begin(&mut self, name: &str, len: u64) {
        use sha2::Digest as _;
        self.0.update((name.len() as u64).to_be_bytes());
        self.0.update(name.as_bytes());
        self.0.update(len.to_be_bytes());
    }

    /// Some of a part's bytes.
    fn feed(&mut self, bytes: &[u8]) {
        use sha2::Digest as _;
        self.0.update(bytes);
    }

    fn finish(self) -> String {
        use sha2::Digest as _;
        hex(&self.0.finalize())
    }
}

/// [`Framed`] over a list, as a function — the seam the framing is tested through.
///
/// **It is a seam, and the seam is why the framing is testable at all.** Through [`tree`] a
/// collision would need two different path sets whose framed encodings agree, which is the
/// thing the lengths make impossible and which therefore cannot be demonstrated from the
/// outside. `a_name_and_its_contents_cannot_run_together` drives this function directly, so
/// dropping the framing reddens a test today rather than the day something relies on it.
///
/// It exists **for** that test and is compiled only for it: a second caller would be a second
/// answer to "how are parts framed", which is the drift [`Framed`] is one struct to prevent.
#[cfg(test)]
fn digest(parts: &[(&str, &[u8])]) -> String {
    let mut framed = Framed::new();
    for (name, bytes) in parts {
        framed.part(name, bytes);
    }
    framed.finish()
}

/// Lowercase hex, two digits a byte: how every sha256 charter records or prints is spelled.
///
/// One spelling for the crate, since `sha2` 0.11's digests no longer format as `{:x}`.
pub(crate) fn hex(bytes: &[u8]) -> String {
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
/// docstring, and [`crate::machine`]'s, and ADR 0031.
///
/// On unix this IS `Ok(())`, so cargo-mutants' one mutation of it changes only the refusal
/// below, which no unix build compiles; `.cargo/mutants.toml` excludes it for that reason.
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
         expressed refuses rather than degrades (ADR 0031)",
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

    /// The panels it is contributing right now, on the same terms and for the same reason: an
    /// extension that is new, changed or unreadable contributes nothing, and the registry doing
    /// that one job is what a panel contribution rests on entirely.
    pub fn panels_in_force(&self) -> &[crate::panel::Panel] {
        match (&self.found, self.standing.may_contribute()) {
            (Some(found), true) => &found.manifest.panels,
            _ => &[],
        }
    }

    /// The views it is offering right now, on the same terms.
    ///
    /// **This decides what buttons the window draws, and it does not decide what runs.** A
    /// survey is taken once per window; the extension can change after it. So the button this
    /// puts on screen is a question the operator can ask, and [`crate::executor`] re-reads the
    /// record and re-takes the fingerprint when he asks it — a view that was in force at the
    /// survey and changed since is refused at the press, not run on the survey's word.
    pub fn views_in_force(&self) -> &[View] {
        match (&self.found, self.standing.may_contribute()) {
            (Some(found), true) => &found.manifest.views,
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
/// It reads the disk — every installed extension's whole directory, to re-take the fingerprint
/// — because an approval is of bytes and the bytes are what may have changed. That is ADR
/// 0041's named cost of fingerprinting code rather than configuration, widened by
/// charter-app#152 from the declared list to the tree, and it is bounded by
/// [`MOST_TREE_ENTRIES`] and [`MOST_TREE_BYTES`] per extension.
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
    /// [`state_note`], when this extension declares a state directory, and `None` when it
    /// declares none — which is the ordinary case and has no exception to state.
    ///
    /// **The exception goes where the operator consents, not only into a doc comment**
    /// (charter-app#152). [`FINGERPRINTED`] says charter read every file in the directory; if
    /// that is true of everything but one directory, the one directory is the operator's to
    /// weigh.
    pub state_note: Option<String>,
}

/// The question to ask about `found`.
pub fn prompt(found: &Extension, standing: Standing) -> Prompt {
    // **Each capability first, one line each** (ADR 0053): what the extension asks charter to
    // do for it is the part of the yes that grows, so it is what the operator reads first.
    // `declares` is also what the Extensions list shows under each row, so the list and the
    // dialog name the same capabilities from the one place.
    let mut declares: Vec<String> = found
        .manifest
        .capabilities
        .iter()
        .map(|capability| capability.asks())
        .collect();
    // What each capability with a shape declares, right after the capabilities themselves.
    declares.extend(facts::declares(&found.manifest));
    declares.extend(events::declares(&found.manifest));
    declares.extend(briefing::declares(&found.manifest));
    declares.extend(
        found
            .manifest
            .themes
            .iter()
            .map(|theme| format!("a theme, “{}”", theme.name)),
    );
    declares.extend(found.manifest.panels.iter().map(crate::panel::declares));
    declares.extend(found.manifest.views.iter().map(|view| {
        format!(
            "a view, “{}” — when you open it, charter starts this extension's program and hands \
             it {}",
            view.title,
            crate::handed::what(view.about)
        )
    }));
    if !found.manifest.settings.is_empty() {
        // What a project's files can hand the program is part of what the yes covers: a
        // committed file choosing a value the program then acts on is a channel the operator is
        // owed a line about (ADR 0048).
        let titles: Vec<&str> = found
            .manifest
            .settings
            .iter()
            .map(|setting| setting.title.as_str())
            .collect();
        declares.push(format!(
            "settings a project may choose, handed to its program with each question: {}",
            titles.join(", ")
        ));
    }
    if let Some(program) = &found.manifest.program {
        // **Named as a program charter starts, with what bounds charter puts on it and what it
        // does not.** ADR 0041's amendment: the prompt says what charter will do, and says that
        // it is conduct and not a cage — [`RUNS_AS_YOU`] carries the second half, below.
        let manifest = &found.manifest;
        declares.push(
            if manifest.views.is_empty() && manifest.events.is_none() && manifest.briefing.is_none()
            {
                format!(
                    "a program, {program} — it declares no view, hears no event and adds no \
                     briefing section, so nothing ever asks charter to start it"
                )
            } else if manifest.events.is_none() && manifest.briefing.is_none() {
                format!("a program, {program} — {}", crate::executor::HOW_IT_RUNS)
            } else {
                format!(
                    "a program, {program} — {}",
                    crate::executor::how_it_runs(manifest)
                )
            },
        );
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
        state_note: found.manifest.state.as_deref().map(state_note),
    }
}

#[cfg(test)]
mod tests;
