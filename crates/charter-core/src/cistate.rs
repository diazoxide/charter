//! What a forge last said about a branch, **read from a cache and never fetched**.
//!
//! # Why the app does not ask the forge
//!
//! Three reasons, and the first settles it on its own.
//!
//! 1. The app never calls Python (spec decision 14). `charter gl-refresh` — the detached
//!    Python process that fills this cache — is not something a shipped path may run, and
//!    there is no Rust forge client yet. So at this milestone charter can *read* CI state
//!    and cannot *fetch* it, and saying so is better than a panel that is silently empty.
//! 2. A panel that fetched would hold the window for as long as `gh` or `glab` takes, which
//!    on a cold token is seconds. The window's budget for a workspace switch is 100 ms.
//! 3. A fetch on a render path puts a forge token in the process that draws the window.
//!    Python deliberately does not do that either: the refresher holds the token, the
//!    renderer reads a file (`charter/glstate.py`).
//!
//! # The file is another process's, so every value in it is checked
//!
//! `.charter/cache/glstate.json` is an unsigned, world-writable-by-the-operator JSON file
//! that some *other* program writes, and its values reach a panel. Python learned this the
//! hard way — charter #326, where the change id was the one forge field reaching a line a
//! terminal interprets — and now coerces `change` on read as well as on write. This does the
//! same, and extends it: the CI word must be one of the seven the forges are pinned to, and
//! the sigil must be one of the two charter knows. Nothing else from the file is ever shown.
//!
//! # What this does NOT claim to fix
//!
//! Python writes `ci: null` both for "the branch has no pipeline" and for "the call failed"
//! (`glstate.state_for_repo` catches every exception and still stamps `ts`). That
//! information is not in the file, so it cannot be read out of it. An entry inside the
//! window with no CI word therefore says exactly that — the last refresh recorded none —
//! and is kept distinct from "nobody has looked", which is the part the file *can* answer.

use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::Value;

use crate::contain;

/// The cache the forge refresher writes, relative to the plane root.
pub const CACHE: &str = ".charter/cache/glstate.json";

/// How long an entry is served for: `charter/glstate.py:21` `DISPLAY_TTL`.
pub const DISPLAY: Duration = Duration::from_secs(7200);

/// Past this an entry is old enough that a refresh is due: `REFRESH_TTL`.
///
/// It is what the panel says about an answer's age AND what [`crate::glstate`] spawns a
/// refresh over — one number, because a panel saying "a refresh is due" beside a policy that
/// would not start one tells the operator two stories about one file.
pub const REFRESH: Duration = Duration::from_secs(300);

/// The only words a CI cell may hold: `charter/forge/base.py:CI_STATES`. Both forges map
/// their own vocabulary onto these, so anything else in the file was not written by a
/// charter that agrees with this one.
pub const CI_STATES: [&str; 7] = [
    "success", "failed", "running", "pending", "manual", "canceled", "skipped",
];

/// The two sigils a forge names a change with: `#` for GitHub, `!` for GitLab.
const SIGILS: [char; 2] = ['#', '!'];

/// The most this file may be before charter stops reading it. Python's own bound on a file
/// it is meant to read is 1 MiB (`contain.MAX_BYTES`); at roughly 120 bytes an entry that is
/// some eight thousand checkouts, and a bigger file is a defect rather than a limit to raise.
const MAX_BYTES: u64 = 1_048_576;

/// charter would not read the cache, and the panel says so instead of showing nothing.
#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum NotRead {
    #[error(
        "{path} is reached through a symlink, and charter's own path may not be: the read \
         would land somewhere charter did not check"
    )]
    ThroughALink { path: String },
    #[error("{path} is {kind}, not a file charter will read")]
    NotAFile { path: String, kind: &'static str },
    #[error("{path} is {bytes} bytes, past the {MAX_BYTES} charter will read of a cache")]
    TooBig { path: String, bytes: u64 },
    #[error("{path} could not be read ({why})")]
    Unreadable { path: String, why: String },
    #[error("{path} is not the object of entries charter writes there ({why})")]
    NotEntries { path: String, why: String },
}

/// The forge cache, read once for a whole listing.
#[derive(Debug, Clone, Default)]
pub struct Cache {
    entries: serde_json::Map<String, Value>,
    /// Every key by where it LANDS, for the miss that is not really a miss.
    ///
    /// Built once and only after a key has already been missed on its spelling — which is
    /// the ordinary case, because the refresher and the app usually walk the plane the same
    /// way. Resolving every key costs a `readlink` per component per entry, and this file is
    /// never pruned, so it is not paid unless it is needed.
    resolved: std::cell::OnceCell<std::collections::HashMap<std::path::PathBuf, String>>,
    /// Seconds since the epoch when the file was read, so every row in one listing ages
    /// against the same instant.
    now: u64,
}

/// What the cache had to say about one checkout's branch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reading {
    /// An entry charter will serve. `state` is absent when the refresh recorded no pipeline.
    Fetched {
        state: Option<String>,
        change: Option<u64>,
        sigil: Option<char>,
        seconds_ago: u64,
    },
    /// Nothing to serve, and why — never a blank cell.
    NotFetched(String),
}

/// Read `.charter/cache/glstate.json`. A plane nothing has refreshed has an empty cache,
/// which is an answer and not a refusal.
pub fn read(plane: &Path) -> Result<Cache, NotRead> {
    let path = plane.join(CACHE);
    let named = path.display().to_string();
    // `.charter/` is not one of the plane's data directories, so `contain::readable` is the
    // wrong gate here — it would refuse every read of this file. The right one is the walk
    // `reopen` and `hookwire` already use for charter's own paths under `.charter/`, and it
    // is asked of the **file**, not of `cache/`: a link at `glstate.json` redirects the read
    // exactly as one at the directory does, and a check on the parent cannot see it.
    contain::no_link_on_the_way(plane, &path).map_err(|_| NotRead::ThroughALink {
        path: named.clone(),
    })?;
    let found = match std::fs::symlink_metadata(&path) {
        Ok(found) => found,
        // Nothing has ever refreshed this plane. An answer, not a refusal.
        Err(why) if why.kind() == std::io::ErrorKind::NotFound => return Ok(Cache::default()),
        Err(why) => {
            return Err(NotRead::Unreadable {
                path: named,
                why: why.to_string(),
            });
        }
    };
    if !found.is_file() {
        // A FIFO on this path would block the read for ever and a device would never end,
        // so the kind is decided from the `lstat` charter already has rather than by opening.
        return Err(NotRead::NotAFile {
            path: named,
            kind: kind_of(&found),
        });
    }
    if found.len() > MAX_BYTES {
        return Err(NotRead::TooBig {
            path: named,
            bytes: found.len(),
        });
    }
    let text = std::fs::read_to_string(&path).map_err(|why| NotRead::Unreadable {
        path: named.clone(),
        why: why.to_string(),
    })?;
    let doc: Value = serde_json::from_str(&text).map_err(|why| NotRead::NotEntries {
        path: named.clone(),
        why: why.to_string(),
    })?;
    let entries = doc
        .as_object()
        .cloned()
        .ok_or_else(|| NotRead::NotEntries {
            path: named,
            why: "the document is not an object".into(),
        })?;
    Ok(Cache {
        entries,
        resolved: std::cell::OnceCell::new(),
        now: now(),
    })
}

/// What `lstat` found, in the word a refusal uses.
fn kind_of(found: &std::fs::Metadata) -> &'static str {
    let kind = found.file_type();
    if kind.is_dir() {
        "a directory"
    } else if kind.is_symlink() {
        "a symlink"
    } else {
        "not a regular file"
    }
}

impl Cache {
    /// Whether the file held nothing at all — no refresher has ever run on this plane.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// What the cache says about `tree`, given the branch it is actually on now.
    pub fn about(&self, tree: &Path, branch: &str) -> Reading {
        let Some(entry) = self.entry_for(tree) else {
            return Reading::NotFetched("nothing has fetched this checkout".into());
        };
        // The entry names the branch it was fetched for, and a checkout that has moved since
        // is a different question. Serving the old answer under the new branch is how a red
        // pipeline reads as somebody else's — Python drops the entry for the same reason
        // (`glstate.read_for`).
        match entry.get("branch").and_then(Value::as_str) {
            Some(was) if was == branch => {}
            Some(_) => {
                return Reading::NotFetched(format!(
                    "the last fetch was for a different branch, and this checkout is on \
                     '{branch}'"
                ));
            }
            None => return Reading::NotFetched("the cached entry names no branch".into()),
        }
        let seconds_ago = self.age_of(entry);
        if seconds_ago > DISPLAY.as_secs() {
            return Reading::NotFetched(format!(
                "last fetched {}, past the two hours a cached answer is served for",
                ago(seconds_ago)
            ));
        }
        let state = entry.get("ci").filter(|ci| !ci.is_null());
        let known = state
            .and_then(Value::as_str)
            .filter(|word| CI_STATES.contains(word))
            .map(str::to_string);
        // A word charter does not know is a word charter does not print. The seven states
        // are what both forges are pinned to, so anything else was written by something that
        // does not agree with this charter — and an escape sequence in that cell would be
        // drawn by whatever renders the panel.
        if state.is_some() && known.is_none() {
            return Reading::NotFetched(
                "the cached entry names a CI state charter does not know".into(),
            );
        }
        Reading::Fetched {
            state: known,
            change: change_of(entry.get("change").or_else(|| entry.get("mr"))),
            sigil: sigil_of(entry.get("sigil")),
            seconds_ago,
        }
    }

    /// The entry for one checkout, by the path it is keyed under **or by where that path
    /// lands**.
    ///
    /// The refresher keys by the path as IT walked the plane (`cache[str(d)]`), and two
    /// programs reach the same checkout under two spellings all the time: `/tmp` is a link to
    /// `/private/tmp` on macOS, `$CHARTER_ROOT` may be set to either, and a plane reached
    /// through a link into `workspaces/` is a third. Comparing the strings alone reads every
    /// one of those as "nobody has fetched this" — for ever, silently, and no fixture can
    /// catch it, because a fixture writes the key the app is about to build.
    ///
    /// So the spelling is tried first, and a miss falls back to where both sides LAND, using
    /// the plane's own resolver (`contain::resolved`) rather than a second idea of what a
    /// resolved path is. A path charter cannot place resolves to `None` and stays a miss.
    fn entry_for(&self, tree: &Path) -> Option<&serde_json::Map<String, Value>> {
        let spelled = tree.display().to_string();
        if let Some(found) = self
            .entries
            .get(spelled.as_str())
            .and_then(Value::as_object)
        {
            return Some(found);
        }
        let lands = contain::resolved(tree)?;
        let key = self.by_where_they_land().get(&lands)?;
        self.entries.get(key.as_str()).and_then(Value::as_object)
    }

    /// Every key by where it lands, worked out once for the whole listing.
    fn by_where_they_land(&self) -> &std::collections::HashMap<std::path::PathBuf, String> {
        self.resolved.get_or_init(|| {
            self.entries
                .keys()
                .filter_map(|key| Some((contain::resolved(Path::new(key))?, key.clone())))
                .collect()
        })
    }

    /// How long ago this entry was written. An entry with no `ts`, or one stamped in the
    /// future, is treated as written at the epoch — which puts it past `DISPLAY` and out.
    fn age_of(&self, entry: &serde_json::Map<String, Value>) -> u64 {
        let stamped = entry
            .get("ts")
            .and_then(Value::as_f64)
            .filter(|ts| ts.is_finite() && *ts >= 0.0)
            .unwrap_or(0.0) as u64;
        self.now.saturating_sub(stamped)
    }
}

/// The change id, taken only when it is a whole number above zero.
///
/// `charter/glstate.py:_change_or_none`, applied on read for the reason Python applies it on
/// read as well as on write: the cache is an unsigned file another process writes, and an
/// entry written by a charter with a bug renders for two hours after the upgrade that fixed
/// it. `mr` is the name an older charter wrote the same field under.
fn change_of(value: Option<&Value>) -> Option<u64> {
    match value? {
        // `as_u64` is already false for a bool and for a float, which is the whole check:
        // Python needs `isinstance(v, bool)` because `int(True)` is 1, and serde does not.
        Value::Number(number) => number.as_u64().filter(|change| *change > 0),
        _ => None,
    }
}

/// The sigil, taken only when it is exactly one of the two characters a forge uses.
fn sigil_of(value: Option<&Value>) -> Option<char> {
    let mut chars = value?.as_str()?.chars();
    let one = chars.next()?;
    (chars.next().is_none() && SIGILS.contains(&one)).then_some(one)
}

/// Seconds since the epoch, or 0 on a clock charter cannot read — which makes every entry
/// look freshly written rather than making the panel blank.
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_secs())
        .unwrap_or(0)
}

/// An age a person reads, in the coarsest unit that still says something.
pub fn ago(seconds: u64) -> String {
    match seconds {
        0..=89 => format!("{seconds} seconds ago"),
        90..=5399 => format!("{} minutes ago", (seconds + 30) / 60),
        _ => format!("{} hours ago", (seconds + 1800) / 3600),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_whole_number_above_zero_is_a_change() {
        let taken = |text: &str| {
            let value: Value = serde_json::from_str(text).unwrap();
            change_of(Some(&value))
        };

        assert_eq!(taken("41"), Some(41));
        assert_eq!(taken("0"), None);
        assert_eq!(taken("-3"), None);
        assert_eq!(taken("true"), None);
        assert_eq!(taken("12.5"), None);
        assert_eq!(taken("\"12\""), None);
        assert_eq!(taken("\"\\u001b[2J\""), None);
        assert_eq!(change_of(None), None);
    }

    #[test]
    fn only_the_two_characters_a_forge_names_a_change_with_are_a_sigil() {
        let taken = |text: &str| {
            let value: Value = serde_json::from_str(text).unwrap();
            sigil_of(Some(&value))
        };

        assert_eq!(taken("\"#\""), Some('#'));
        assert_eq!(taken("\"!\""), Some('!'));
        assert_eq!(taken("\"\""), None);
        assert_eq!(taken("\"##\""), None);
        assert_eq!(taken("\"\\u001b[2J\""), None);
        assert_eq!(taken("7"), None);
        // ONE character that is not a sigil. Without this the allowlist is held by nothing:
        // every other case above is refused by the length check alone, so a version that
        // took any single character passed the whole test. A mutation found that.
        assert_eq!(taken("\"x\""), None);
        assert_eq!(taken("\"\\u0007\""), None);
    }

    #[test]
    fn an_age_is_said_in_the_coarsest_unit_that_still_says_something() {
        assert_eq!(ago(0), "0 seconds ago");
        assert_eq!(ago(89), "89 seconds ago");
        assert_eq!(ago(90), "2 minutes ago");
        assert_eq!(ago(300), "5 minutes ago");
        assert_eq!(ago(5400), "2 hours ago");
    }
}
