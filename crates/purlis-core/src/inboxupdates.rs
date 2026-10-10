//! **The Inbox's updates** (#1693, spec #1688, I-6, I-8, I-10): what happened that the person
//! may want to know and need not answer — a task that finished or failed, a doctor finding, a
//! chat that resumed, a sandbox change, a dispatch refused while nobody was there, a Smart close
//! that stopped — kept **per machine for a day** in purlis's data home and never in a project,
//! so it survives a relaunch, is never committed and is never sent.
//!
//! An update is not an ask: nothing waits on it. The window derives each one from the source
//! that already says it (the finished rows, the doctor's report, the chats a launch put back,
//! the sandbox's blocks, the away list, a Smart close's steps) and notes it here by a key of its
//! own, so the same update noted again is one update. What the person did with it is kept with
//! it: **read** (Mark all read) and **dismissed** (Dismiss, Dismiss all). A dismissed update is
//! kept, unseen, until its day is over, so its source saying it again does not bring it back.
//!
//! # Where and how
//!
//! `<data>/inbox/<project key>.json` (the key is the digest the sandbox's cache homes and the
//! network record are named by), one JSON array, rewritten whole under a lock on its folder,
//! 0600. A write lets go of what is older than [`KEPT_FOR_SECS`] and past [`AT_MOST_KEPT`].
//! A file this build cannot read reads as no updates and is written over: an update is
//! information, and losing a day of it loses nothing that waits.
//!
//! **What a chat named is data.** A chain names chats, and an update says what its source said
//! of them; each is held to a bound here ([`MOST_SAID`], [`MOST_NAME`], [`MOST_CHAIN`],
//! [`MOST_KEY`]), and the window draws it as text.

use std::io;
use std::path::{Path, PathBuf};

/// The updates' folder under purlis's data home.
pub const DIR: &str = "inbox";

/// How long an update is kept: a day (I-6).
pub const KEPT_FOR_SECS: u64 = 24 * 60 * 60;

/// The most updates one project keeps; the oldest go first.
pub const AT_MOST_KEPT: usize = 200;

/// The most characters an update says.
pub const MOST_SAID: usize = 400;

/// The most characters of one name in a chain.
pub const MOST_NAME: usize = 80;

/// The most names a chain keeps: the session first, the chat it is about last.
pub const MOST_CHAIN: usize = 8;

/// The most characters of a key.
pub const MOST_KEY: usize = 200;

/// What kind of thing happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    /// A task finished: done, cancelled, or ended by the person.
    TaskDone,
    /// A task came to nothing: failed, blocked, ended without a report, or did not start.
    TaskFailed,
    /// The doctor found something.
    Doctor,
    /// A chat came back: resumed, or started fresh in its place.
    Resumed,
    /// A chat's sandbox refused something that is no ask.
    Sandbox,
    /// A dispatch was refused while nobody was at its chat (#1507).
    RefusedAway,
    /// A Smart close stopped without its record (SI-8f).
    SmartClose,
}

/// One update, as it is kept.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Update {
    /// What tells it from every other update of its project: its source's own name for it.
    pub key: String,
    pub kind: Kind,
    /// When it happened, by its source's own time where it has one (seconds since 1970).
    pub at: u64,
    /// The chat it is about, while that number means one.
    #[serde(default)]
    pub session: Option<u32>,
    /// Who it is about, the session first: names, as data.
    #[serde(default)]
    pub chain: Vec<String>,
    /// What happened, in one line: data, never markup.
    pub says: String,
    /// The person has read it (Mark all read).
    #[serde(default)]
    pub read: bool,
    /// The person put it away (Dismiss).
    #[serde(default)]
    pub dismissed: bool,
}

/// What the person did with updates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Settle {
    /// Mark read: drawn as read, and kept.
    Read,
    /// Dismiss: not drawn again.
    Dismissed,
}

/// One machine's updates, under a data home.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Store {
    dir: PathBuf,
}

impl Store {
    /// The updates under the data home `data`.
    pub fn in_data(data: &Path) -> Self {
        Self {
            dir: data.join(DIR),
        }
    }

    /// The updates under this process's data home ([`crate::datahome::root`]), when there is
    /// one and it is not inside a project or a git work tree (ADR 0075 §6).
    pub fn here() -> Option<Self> {
        let data = crate::datahome::root()?;
        crate::datahome::refusal(&data)
            .is_none()
            .then(|| Self::in_data(&data))
    }

    /// The file the project at `root` keeps its updates in.
    pub fn file(&self, root: &Path) -> PathBuf {
        self.dir
            .join(format!("{}.json", crate::sandbox::Homes::project_key(root)))
    }

    /// The project at `root`'s updates at `now`: kept, not dismissed, newest first.
    pub fn read(&self, root: &Path, now: u64) -> Vec<Update> {
        shown(&kept(self.everything(root), now))
    }

    /// Notes `noted` for the project at `root`: each whose key is not kept yet is added, unread;
    /// one already kept, dismissed or not, is left as it is. Answers [`Self::read`].
    pub fn note(&self, root: &Path, noted: Vec<Update>, now: u64) -> io::Result<Vec<Update>> {
        self.change(root, now, |all| {
            let mut changed = false;
            for one in noted {
                let one = bounded(one, now);
                if !within(one.at, now) || all.iter().any(|kept| kept.key == one.key) {
                    continue;
                }
                all.push(one);
                changed = true;
            }
            changed
        })
    }

    /// Settles the project at `root`'s updates named by `keys`, or every one where `keys` is
    /// `None`: read, or dismissed (and so read). Answers [`Self::read`].
    pub fn settle(
        &self,
        root: &Path,
        keys: Option<&[String]>,
        how: Settle,
        now: u64,
    ) -> io::Result<Vec<Update>> {
        self.change(root, now, |all| {
            let mut changed = false;
            for one in all.iter_mut().filter(|one| !one.dismissed) {
                if keys.is_some_and(|keys| !keys.contains(&one.key)) {
                    continue;
                }
                let was = (one.read, one.dismissed);
                one.read = true;
                one.dismissed |= how == Settle::Dismissed;
                changed |= was != (one.read, one.dismissed);
            }
            changed
        })
    }

    /// Every update the file holds, dismissed ones too; none where it cannot be read.
    fn everything(&self, root: &Path) -> Vec<Update> {
        std::fs::read_to_string(self.file(root))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    /// Reads, lets `change` edit, and writes back what is kept, under a lock on the folder.
    /// Written only where something changed or something was let go of.
    fn change(
        &self,
        root: &Path,
        now: u64,
        change: impl FnOnce(&mut Vec<Update>) -> bool,
    ) -> io::Result<Vec<Update>> {
        crate::rewrite::create_dir_all(&self.dir)?;
        let _held = crate::rewrite::Lock::on(&self.dir);
        let file = self.file(root);
        let was = self.everything(root);
        let before = was.len();
        let mut all = kept(was, now);
        let let_go = all.len() != before;
        let changed = change(&mut all);
        all = kept(all, now);
        if changed || let_go {
            let text = serde_json::to_string(&all).map_err(io::Error::other)?;
            crate::rewrite::replace(
                &self.dir,
                &file,
                text.as_bytes(),
                crate::rewrite::Mode::Private,
            )?;
        }
        Ok(shown(&all))
    }
}

/// Whether something that happened at `then` is still kept at `now`.
fn within(then: u64, now: u64) -> bool {
    then.saturating_add(KEPT_FOR_SECS) > now
}

/// What of `all` is still kept at `now`, oldest first, at most [`AT_MOST_KEPT`].
fn kept(mut all: Vec<Update>, now: u64) -> Vec<Update> {
    all.retain(|one| within(one.at, now));
    all.sort_by_key(|one| one.at);
    let over = all.len().saturating_sub(AT_MOST_KEPT);
    all.drain(..over);
    all
}

/// What of `all` is drawn: not dismissed, newest first.
fn shown(all: &[Update]) -> Vec<Update> {
    let mut out: Vec<Update> = all.iter().filter(|one| !one.dismissed).cloned().collect();
    out.sort_by(|one, other| other.at.cmp(&one.at).then_with(|| one.key.cmp(&other.key)));
    out
}

/// `text` held to `most` characters.
fn cut(text: &str, most: usize) -> String {
    text.chars().take(most).collect()
}

/// `one` as it is kept: new, unread, its words held to their bounds, and no later than `now`.
fn bounded(one: Update, now: u64) -> Update {
    Update {
        key: cut(&one.key, MOST_KEY),
        kind: one.kind,
        at: one.at.min(now),
        session: one.session,
        chain: one
            .chain
            .iter()
            .take(MOST_CHAIN)
            .map(|name| cut(name, MOST_NAME))
            .collect(),
        says: cut(&one.says, MOST_SAID),
        read: false,
        dismissed: false,
    }
}

#[cfg(test)]
#[path = "inboxupdates_tests.rs"]
mod tests;
