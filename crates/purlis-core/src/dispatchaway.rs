//! **The dispatches refused while nobody was there** (#1507, spec #1483; decision V100-29):
//! what a chat nobody is at was refused for lack of a standing grant, kept so the person reads
//! of it when they are back and can allow the pair for the next run.
//!
//! # What is kept, and what is not
//!
//! One entry for each asking persona, target persona and workspace: who wanted whom, where,
//! the name of the latest task, when it was last refused and how many times. **Never one a
//! refusal**: a chat that asks in a loop raises a count, not a list ([`keep`]).
//!
//! **Nothing of the brief.** The brief is a chat's words, and this record is read back into a
//! row that offers a standing grant. The only text of a chat's in an entry is the task's name,
//! which the app held to a task name's rule before it got here, kept to [`MOST_TASK_CHARS`],
//! and dropped where it holds anything purlis would not draw.
//!
//! **Only a refusal a person's grant would mend** ([`pair_kept`]): no grant between two
//! personas. A never, a policy lock, a limit, the loop rule, a profile that asks nobody, a
//! chat with no sandbox and a record of nevers that does not read are each refused for a
//! reason no Allow lifts, and none is kept.
//!
//! # It decides nothing
//!
//! **Nothing here is read when a dispatch is decided.** An entry is not a grant, not a
//! question waiting on a chat, and not a dispatch held to be started: the chat was refused and
//! told so, and what it asked for is gone. The one thing an entry does is let the person make
//! the ordinary standing grant for that pair with one press, which the app checks again at the
//! press against the personas, the policy and the nevers as they stand then.
//!
//! # Bounded
//!
//! At most [`MOST`] entries a project. Past that a new pair is not kept ([`Kept::Full`]) and a
//! pair already there still counts. An entry is dropped [`KEPT_SECS`] after its latest
//! refusal, at the next write and from every [`list`].
//!
//! # Where it is, and who writes it
//!
//! `app/dispatches/refused-while-away.json` in the project's state folder: the one folder of
//! the app's a sandboxed chat can neither read nor write ([`crate::dispatchrecord`], ADR 0067
//! §5 class 2), so a chat cannot read which pairs are about to be put to the person, and
//! cannot add one. The app writes it. A chat that runs without the sandbox runs as the person
//! and can write it, as it can write a grant itself; what [`list`] answers is still only what
//! the app would have written ([`sound`]), and the app's own checks at the press are what a
//! grant is made on.
//!
//! The file is not a dispatch's record: it is not named by a ULID, so
//! [`crate::dispatchrecord::list`] and the retention sweep pass it by.
//!
//! # The shape
//!
//! ```json
//! {"v": 1, "refused": [{"asking": "steward", "target": "devops", "workspace": "ide",
//!   "task": "deploy", "latest": 1791400000, "times": 3}]}
//! ```
//!
//! **A file that does not read lists nothing and is written over**: every entry is an offer,
//! so losing one loses no refusal and no grant. A file of another version lists nothing and is
//! left as it is.

use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::dispatchunattended::Refusal;

/// The record's version: a file of another lists nothing and is left as it is.
pub const VERSION: u32 = 1;

/// The file's name in the dispatch store ([`crate::dispatchrecord::dir`]).
pub const FILE_NAME: &str = "refused-while-away.json";

/// The most entries a project keeps.
pub const MOST: usize = 50;

/// How long an entry is kept after its latest refusal: 30 days, as a dispatch's record is.
pub const KEPT_SECS: u64 = 30 * 24 * 60 * 60;

/// The most of a task's name an entry keeps, in characters.
pub const MOST_TASK_CHARS: usize = 80;

/// How far past now an entry's time may read before it is taken for one the app did not write.
const CLOCK_ROOM_SECS: u64 = 24 * 60 * 60;

/// The file in the project at `root`.
pub fn path(root: &Path) -> PathBuf {
    crate::dispatchrecord::dir(root).join(FILE_NAME)
}

/// One pair refused while nobody was there, in one workspace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Refused {
    /// The persona the asking chat ran with, by the app's record of it.
    pub asking: String,
    /// The persona it asked for: one of the project's.
    pub target: String,
    /// The workspace the asking chat worked in; none for the project's root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
    /// The name of the task last refused, where it had one purlis draws.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task: Option<String>,
    /// When it was last refused, in seconds since 1970.
    pub latest: u64,
    /// How many times it was refused.
    pub times: u32,
}

impl Refused {
    fn is(&self, asking: &str, target: &str, workspace: Option<&str>) -> bool {
        self.asking == asking && self.target == target && self.workspace.as_deref() == workspace
    }
}

/// What [`keep`] did with a refusal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kept {
    /// A new entry.
    New,
    /// The pair was there for that workspace: its count went up and its time moved.
    Again,
    /// The record is at its most and the pair is not in it: nothing was kept.
    Full,
    /// Not two different personas' names: nothing was kept.
    NotAPair,
}

impl Kept {
    /// Whether the person will read of it.
    pub fn listed(self) -> bool {
        matches!(self, Self::New | Self::Again)
    }
}

/// **The pair a refusal is kept for**, asking persona then target, or `None` for one that is
/// not kept: only [`Refusal::Missing`] between two different personas, which a standing grant
/// for that pair mends. A chat on no persona has no pair such a grant could name.
pub fn pair_kept(refusal: &Refusal) -> Option<(&str, &str)> {
    let Refusal::Missing(missing) = refusal else {
        return None;
    };
    let asking = missing.asking.as_deref()?;
    a_pair(asking, &missing.target).then_some((asking, missing.target.as_str()))
}

/// What the asking chat is told after the refusal's own sentence, where the refusal was kept.
pub const THE_PERSON_WILL_SEE_IT: &str =
    "purlis kept that this was refused, and the person will see it when they are back.";

/// `refusal`, the sentence a chat is refused with, and that the person will see it.
pub fn told(refusal: &str) -> String {
    format!("{refusal} {THE_PERSON_WILL_SEE_IT}")
}

fn a_pair(asking: &str, target: &str) -> bool {
    asking != target && crate::personas::valid_name(asking) && crate::personas::valid_name(target)
}

/// A text purlis would draw on one line.
fn drawn(text: &str, most_chars: usize) -> bool {
    !text.trim().is_empty()
        && text.chars().count() <= most_chars
        && !text.contains(crate::panel::undrawable)
}

/// The most of a workspace's name an entry keeps, in characters.
const MOST_WORKSPACE_CHARS: usize = 128;

/// **Whether `entry` is one the app would have written**, read at `now`: two different
/// personas' names, a workspace and a task that draw on one line within their bounds, a count
/// of at least one, and a time that is not ahead of the clock. Refused, never tidied.
pub fn sound(entry: &Refused, now: u64) -> bool {
    a_pair(&entry.asking, &entry.target)
        && entry
            .workspace
            .as_deref()
            .is_none_or(|name| drawn(name, MOST_WORKSPACE_CHARS))
        && entry
            .task
            .as_deref()
            .is_none_or(|name| drawn(name, MOST_TASK_CHARS))
        && entry.times >= 1
        && entry.latest <= now.saturating_add(CLOCK_ROOM_SECS)
}

fn expired(entry: &Refused, now: u64) -> bool {
    now.saturating_sub(entry.latest) >= KEPT_SECS
}

/// What a file's text holds.
enum Held {
    /// The entries that are sound and not expired at `now`, one a pair and workspace, in the
    /// file's order. Empty for a text that is not this record's shape.
    Read(Vec<Refused>),
    /// A record of another version.
    AnotherVersion,
}

fn parsed(text: &str, now: u64) -> Held {
    let Ok(serde_json::Value::Object(whole)) = serde_json::from_str::<serde_json::Value>(text)
    else {
        return Held::Read(Vec::new());
    };
    match whole.get("v").and_then(serde_json::Value::as_u64) {
        Some(v) if v == u64::from(VERSION) => {}
        Some(_) => return Held::AnotherVersion,
        None => return Held::Read(Vec::new()),
    }
    let mut kept: Vec<Refused> = Vec::new();
    let entries = whole
        .get("refused")
        .and_then(serde_json::Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    for entry in entries {
        let Ok(entry) = serde_json::from_value::<Refused>(entry.clone()) else {
            continue;
        };
        let twice = kept
            .iter()
            .any(|one| one.is(&entry.asking, &entry.target, entry.workspace.as_deref()));
        if sound(&entry, now) && !expired(&entry, now) && !twice && kept.len() < MOST {
            kept.push(entry);
        }
    }
    Held::Read(kept)
}

fn another_version() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "the record of dispatches refused while nobody was there is of another version of \
         purlis, so this one leaves it as it is",
    )
}

/// The text of the file, never through a link and never of anything but a file.
fn text_of(root: &Path) -> Option<String> {
    let path = path(root);
    let there = std::fs::symlink_metadata(&path).ok()?;
    there
        .is_file()
        .then(|| std::fs::read_to_string(&path).ok())
        .flatten()
}

/// **What was refused while nobody was there in the project at `root`**, as it stands at
/// `now`: newest first, only entries the app would have written ([`sound`]), none past
/// [`KEPT_SECS`]. Nothing for no file, for one that does not read and for one of another
/// version.
pub fn list(root: &Path, now: u64) -> Vec<Refused> {
    let Some(Held::Read(mut entries)) = text_of(root).map(|text| parsed(&text, now)) else {
        return Vec::new();
    };
    // Newest first, and the file's order among entries of one second.
    entries.reverse();
    entries.sort_by_key(|one| std::cmp::Reverse(one.latest));
    entries
}

/// Reads the record at `now`, changes its entries with `how` and writes it back where `how`
/// says it changed, under purlis's lock on the store. A file of another version is refused and
/// left; one that does not read is written over.
fn change(root: &Path, now: u64, how: impl FnOnce(&mut Vec<Refused>) -> bool) -> io::Result<()> {
    let path = path(root);
    let dir = crate::dispatchrecord::dir(root);
    crate::rewrite::create_dir_all(&dir)?;
    let _held = crate::rewrite::Lock::on(&dir);
    let mut entries = match std::fs::symlink_metadata(&path) {
        Err(none) if none.kind() == io::ErrorKind::NotFound => Vec::new(),
        Err(other) => return Err(other),
        // A link or a folder under that name is not this record, and is not written through.
        Ok(there) if !there.is_file() => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("{} is not a file", path.display()),
            ));
        }
        Ok(_) => match std::fs::read_to_string(&path).map(|text| parsed(&text, now)) {
            Ok(Held::AnotherVersion) => return Err(another_version()),
            Ok(Held::Read(entries)) => entries,
            // Not text: nothing in it is an entry.
            Err(_) => Vec::new(),
        },
    };
    if !how(&mut entries) {
        return Ok(());
    }
    let text = serde_json::to_string_pretty(&serde_json::json!({
        "v": VERSION,
        "refused": entries,
    }))
    .map_err(io::Error::other)?;
    crate::rewrite::replace(
        root,
        &path,
        format!("{text}\n").as_bytes(),
        crate::rewrite::Mode::Private,
    )
}

/// **Keeps that a chat nobody is at, running as `asking`, was refused a dispatch to `target`**
/// at `at`, from `workspace`, for the task named `task`. One entry a pair and workspace: a
/// second refusal raises its count, moves its time and names the later task. The app's to
/// call, with its own record of the chat: never a word of the request but the task's name,
/// which is dropped here where purlis would not draw it.
pub fn keep(
    root: &Path,
    asking: &str,
    target: &str,
    workspace: Option<&str>,
    task: Option<&str>,
    at: u64,
) -> io::Result<Kept> {
    if !a_pair(asking, target) {
        return Ok(Kept::NotAPair);
    }
    let workspace = workspace.filter(|name| drawn(name, MOST_WORKSPACE_CHARS));
    let task = task
        .map(|name| name.chars().take(MOST_TASK_CHARS).collect::<String>())
        .filter(|name| drawn(name, MOST_TASK_CHARS));
    let mut kept = Kept::Full;
    change(root, at, |entries| {
        if let Some(there) = entries
            .iter_mut()
            .find(|one| one.is(asking, target, workspace))
        {
            there.times = there.times.saturating_add(1);
            there.latest = at;
            there.task = task;
            kept = Kept::Again;
            return true;
        }
        if entries.len() >= MOST {
            return false;
        }
        entries.push(Refused {
            asking: asking.to_owned(),
            target: target.to_owned(),
            workspace: workspace.map(str::to_owned),
            task,
            latest: at,
            times: 1,
        });
        kept = Kept::New;
        true
    })?;
    Ok(kept)
}

/// **Takes the entry for `asking` to `target` in `workspace` away**: the person's Dismiss.
/// Answers whether there was one. Grants nothing.
pub fn dismiss(
    root: &Path,
    asking: &str,
    target: &str,
    workspace: Option<&str>,
    now: u64,
) -> io::Result<bool> {
    let mut was = false;
    change(root, now, |entries| {
        let before = entries.len();
        entries.retain(|one| !one.is(asking, target, workspace));
        was = entries.len() != before;
        was
    })?;
    Ok(was)
}

/// **Takes every entry for `asking` to `target` away**, whatever its workspace: the pair was
/// granted, or the person said never to it. Answers how many there were.
pub fn forget_pair(root: &Path, asking: &str, target: &str, now: u64) -> io::Result<usize> {
    let mut were = 0;
    change(root, now, |entries| {
        let before = entries.len();
        entries.retain(|one| !(one.asking == asking && one.target == target));
        were = before - entries.len();
        were > 0
    })?;
    Ok(were)
}

#[cfg(test)]
#[path = "dispatchaway_tests.rs"]
mod tests;
