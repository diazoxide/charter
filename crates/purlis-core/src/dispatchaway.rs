//! **The dispatches refused while nobody was there** (#1507, spec #1483; decision V100-29):
//! what a chat nobody is at was refused for lack of a standing grant, kept so the person reads
//! of it when they are back and can allow the pair for the next run.
//!
//! # What is kept, and what is not
//!
//! One entry for each asking persona, target persona and workspace: who wanted whom, where,
//! when it was first and last refused and how many times. **Never one a refusal**: a chat that
//! asks in a loop raises a count, not a list ([`keep`]).
//!
//! **Nothing a chat wrote.** Not the brief and not the task's name: both are a chat's words,
//! and this record is read back into a row that offers a standing grant. Every field of an
//! entry is the app's own: the two personas and the workspace by its record of the asking
//! chat and the project, the times by its clock, the count by its counting.
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
//! # An order a chat cannot move
//!
//! [`list`] is in the order pairs were **first** refused, oldest first. A repeat changes an
//! entry's count and its latest time and nothing about where it is listed, so a chat asking
//! again never moves a row, its own or another's.
//!
//! # Dismissed holds
//!
//! [`dismiss`] keeps the entry and marks it with the time. While it is marked, the entry is
//! not listed, further refusals for it are counted without a word ([`Kept::Quiet`]), and it
//! still holds its place under the cap, so a chat that asks in a loop cannot put back what
//! the person put away. It is listed again by a refusal that comes [`QUIET_SECS`] or more
//! after the dismissal, and by nothing else.
//!
//! # Bounded
//!
//! At most [`MOST`] entries a project, dismissed ones included. Past that a new pair is not
//! kept ([`Kept::Full`]) and a pair already there still counts. An entry is dropped
//! [`KEPT_SECS`] after its latest refusal, at the next write and from every read.
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
//!   "first": 1791300000, "latest": 1791400000, "times": 3, "dismissed": 1791400100}]}
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

/// How long a dismissal holds: a refusal this long or longer after it lists the entry again.
/// Seven days.
pub const QUIET_SECS: u64 = 7 * 24 * 60 * 60;

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
    /// The workspace the refused task would have worked in (#1505: the one a dispatch names
    /// or moves into, else the asking chat's own); none for the project's root. It is what
    /// the refusal was judged for, and what **Allow from now on** is limited to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
    /// When it was first refused, in seconds since 1970: what the list is ordered by.
    pub first: u64,
    /// When it was last refused, in seconds since 1970.
    pub latest: u64,
    /// How many times it was refused.
    pub times: u32,
    /// When the person dismissed it, where they did: not listed while this stands.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dismissed: Option<u64>,
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
    /// The pair was there for that workspace: its count went up and its latest time moved.
    Again,
    /// The person dismissed it and the quiet period has not passed: counted, and not listed.
    Quiet,
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

/// What the event log's record of a grant or a never says it came from (`from`), where the
/// person made it on an entry of this record: [`crate::eventlog::Recorder::dispatch_grant_from`].
pub const AUDITED_FROM: &str = "away";

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
/// personas' names, a workspace that draws on one line within its bound, a count of at least
/// one, and times that are in order and not ahead of the clock. Refused, never tidied.
pub fn sound(entry: &Refused, now: u64) -> bool {
    a_pair(&entry.asking, &entry.target)
        && entry
            .workspace
            .as_deref()
            .is_none_or(|name| drawn(name, MOST_WORKSPACE_CHARS))
        && entry.times >= 1
        && entry.first <= entry.latest
        && entry.latest <= now.saturating_add(CLOCK_ROOM_SECS)
        && entry
            .dismissed
            .is_none_or(|when| when <= now.saturating_add(CLOCK_ROOM_SECS))
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

/// **Every entry kept for the project at `root`**, as it stands at `now`, dismissed ones
/// included: only entries the app would have written ([`sound`]), none past [`KEPT_SECS`], in
/// the order they were first refused. Nothing for no file, for one that does not read and for
/// one of another version.
pub fn kept(root: &Path, now: u64) -> Vec<Refused> {
    let Some(Held::Read(mut entries)) = text_of(root).map(|text| parsed(&text, now)) else {
        return Vec::new();
    };
    // By first refusal, oldest first, and the file's order among entries of one second: an
    // order no later refusal moves.
    entries.sort_by_key(|one| one.first);
    entries
}

/// **What was refused while nobody was there in the project at `root`, as the person is
/// shown it** at `now`: [`kept`], less what they dismissed.
pub fn list(root: &Path, now: u64) -> Vec<Refused> {
    kept(root, now)
        .into_iter()
        .filter(|one| one.dismissed.is_none())
        .collect()
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
/// at `at`, from `workspace`. One entry a pair and workspace: a second refusal raises its count
/// and moves its latest time, and where the person dismissed it less than [`QUIET_SECS`] ago it
/// does so without listing it ([`Kept::Quiet`]). The app's to call, with its own record of the
/// chat: no word of the request is taken.
pub fn keep(
    root: &Path,
    asking: &str,
    target: &str,
    workspace: Option<&str>,
    at: u64,
) -> io::Result<Kept> {
    if !a_pair(asking, target) {
        return Ok(Kept::NotAPair);
    }
    let workspace = workspace.filter(|name| drawn(name, MOST_WORKSPACE_CHARS));
    let mut kept = Kept::Full;
    change(root, at, |entries| {
        if let Some(there) = entries
            .iter_mut()
            .find(|one| one.is(asking, target, workspace))
        {
            there.times = there.times.saturating_add(1);
            there.latest = at;
            kept = match there.dismissed {
                Some(when) if at.saturating_sub(when) < QUIET_SECS => Kept::Quiet,
                _ => {
                    there.dismissed = None;
                    Kept::Again
                }
            };
            return true;
        }
        if entries.len() >= MOST {
            return false;
        }
        entries.push(Refused {
            asking: asking.to_owned(),
            target: target.to_owned(),
            workspace: workspace.map(str::to_owned),
            first: at,
            latest: at,
            times: 1,
            dismissed: None,
        });
        kept = Kept::New;
        true
    })?;
    Ok(kept)
}

/// **Puts the entry for `asking` to `target` in `workspace` away** at `now`: the person's
/// Dismiss. The entry stays, marked, so further refusals for it are counted and not listed
/// until one comes [`QUIET_SECS`] or more after `now`. Answers whether there was one listed.
/// Grants nothing.
pub fn dismiss(
    root: &Path,
    asking: &str,
    target: &str,
    workspace: Option<&str>,
    now: u64,
) -> io::Result<bool> {
    let mut was = false;
    change(root, now, |entries| {
        if let Some(there) = entries
            .iter_mut()
            .find(|one| one.is(asking, target, workspace) && one.dismissed.is_none())
        {
            there.dismissed = Some(now);
            was = true;
        }
        was
    })?;
    Ok(was)
}

/// **Takes every entry for `asking` to `target` away**, whatever its workspace: the pair was
/// granted, or the person said never to it. Dismissed ones go too. Answers how many there were.
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

/// **Takes the entry for `asking` to `target` in `workspace` away**, and no other: the pair
/// was granted for work in that workspace alone (#1505), so what was refused elsewhere still
/// waits for an answer of its own. A dismissed one goes too. Answers whether there was one.
pub fn forget_in(
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

#[cfg(test)]
#[path = "dispatchaway_tests.rs"]
mod tests;
