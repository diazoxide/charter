//! The plane's own files and its state folder during the window (RN-2a, V93e).
//!
//! [`Name::file_in`] and [`Name::dir_in`] answer "which spelling is there"; a caller that is about
//! to READ or WRITE needs one path, and these give it:
//!
//! * the purlis spelling when it is there — it wins, whatever else is;
//! * else the old spelling that is there;
//! * else the newest OLD spelling, though nothing is there yet.
//!
//! **The last rule is what keeps state whole.** A plane nothing has migrated keeps writing the
//! names it always had, so a first write never starts a second file beside the one an older
//! build — or a teammate — still reads. Messages that name a file this way name the one the
//! operator has; some labels still give the old name whatever the plane has (issue 1277).
//! Only the migrations (`rename-local`, `rename-plane`) create a purlis spelling; from then on
//! it is there, so it is read and written.
//!
//! **The state folder is the exception (D-RN2a-7).** Its presence is not trusted: anything that
//! can make a folder in the project root could otherwise move charter's state, approvals and
//! vault registry into a folder of its choosing. So `.purlis/` is the state folder only when it
//! is the ONE there (rename-local moved `.charter/` away), or when both are there and this
//! machine's rename-local record says this project moved ([`moved_by_rename_local`]). Otherwise
//! it is `.charter/`. The doctor's `renamed leftovers` row names both either way.

use std::path::{Path, PathBuf};

use super::{LOCAL_SETTINGS, Name, PLANE_MANIFEST, SCAN_ALLOW, STATE_DIR};

impl Name {
    /// The spelling of this name to read and write in `dir`, judged by `is` on each joined path
    /// (see the module): the purlis one when it is there, else the newest old one that is, else
    /// the newest old one.
    pub fn spelling_at(&self, dir: &Path, is: impl Fn(&Path) -> bool) -> &'static str {
        self.spellings()
            .find(|name| is(&dir.join(name)))
            .unwrap_or_else(|| self.newest_old())
    }

    /// The spelling of this name to read and write in `dir`, as a FILE (see the module).
    pub fn file_at(&self, dir: &Path) -> PathBuf {
        dir.join(self.spelling_at(dir, Path::is_file))
    }

    /// The spelling of this name to read and write in `dir`, as a FOLDER (see the module).
    pub fn folder_at(&self, dir: &Path) -> PathBuf {
        dir.join(self.spelling_at(dir, Path::is_dir))
    }

    /// Whether `name` — one path component, or a path relative to where this name lives — is
    /// any spelling of this name. For guards: a check that knows only the old spelling misses the
    /// purlis one, which WINS when it is there.
    pub fn is(&self, name: impl AsRef<Path>) -> bool {
        name.as_ref().to_str().is_some_and(|s| self.recognises(s))
    }
}

/// The plane manifest of the plane at `root`: `purlis.toml` or `charter.toml`.
pub fn manifest(root: &Path) -> PathBuf {
    PLANE_MANIFEST.file_at(root)
}

/// [`manifest`]'s file name alone: `purlis.toml` or `charter.toml`.
pub fn manifest_name(root: &Path) -> &'static str {
    PLANE_MANIFEST.spelling_at(root, Path::is_file)
}

/// Whether `dir` holds a plane manifest under either name. A folder of that name is no manifest.
pub fn has_manifest(dir: &Path) -> bool {
    manifest(dir).is_file()
}

/// The machine-local settings file of the plane at `root`: `purlis.local.toml` or
/// `charter.local.toml`.
pub fn local_settings(root: &Path) -> PathBuf {
    LOCAL_SETTINGS.file_at(root)
}

/// The scan allowlist at the top of the repository at `root`.
pub fn scan_allow(root: &Path) -> PathBuf {
    SCAN_ALLOW.file_at(root)
}

/// The state folder in `dir`: `.purlis/` or `.charter/`, by the module's rule for it
/// (D-RN2a-7). **Not** `$CHARTER_HOME`, which only `plane::state_dir` honours: a caller that
/// joins the folder to a root keeps doing exactly that.
pub fn state(dir: &Path) -> PathBuf {
    dir.join(state_name(dir))
}

/// [`state`]'s last component alone, for a walk that opens the folder one step at a time.
pub fn state_name(dir: &Path) -> &'static str {
    state_name_with(dir, moved_by_rename_local)
}

/// [`state_name`], with the machine's rename-local record asked through `moved`: the seam a test
/// drives without a config home, and the one question asked only when both folders are there.
pub fn state_name_with(dir: &Path, moved: impl FnOnce(&Path) -> bool) -> &'static str {
    let purlis = dir.join(STATE_DIR.write).is_dir();
    let old = STATE_DIR
        .reads
        .iter()
        .chain(STATE_DIR.history)
        .copied()
        .find(|name| dir.join(name).is_dir());
    match (purlis, old) {
        (true, None) => STATE_DIR.write,
        (true, Some(old)) => {
            if moved(dir) {
                STATE_DIR.write
            } else {
                old
            }
        }
        (false, Some(old)) => old,
        (false, None) => STATE_DIR.newest_old(),
    }
}

/// Where this machine's rename-local record lists the projects whose state folder it moved to
/// `.purlis/`, inside charter's directory in the config home: one project root per line, as it
/// resolves. **RN-5 writes it**; nothing here does. Kept in the config home, never the project,
/// because a record a chat could write would be no better than the folder it vouches for.
pub const STATE_MOVED_RECORD: &str = "rename-local/state-moved";

/// Whether this machine's rename-local record says the project at `dir` moved its state folder
/// to `.purlis/`. No config home, no record, or no line for `dir`: it did not.
pub fn moved_by_rename_local(dir: &Path) -> bool {
    let Some(config) = crate::machine::rooted(
        std::env::var_os(crate::machine::HOME_VAR),
        std::env::var_os("XDG_CONFIG_HOME"),
        dirs::home_dir(),
    ) else {
        return false;
    };
    // A fenced (test) build never reads a store outside its fence, and a record it cannot read
    // is no record: the state stays `.charter/`, which is the side to be wrong on.
    #[cfg(feature = "fenced")]
    if !crate::fence::inside(&config, &crate::fence::fence()) {
        return false;
    }
    let record = crate::machine::dir(&config).join(STATE_MOVED_RECORD);
    listed_in(&record, dir)
}

/// Whether the record at `record` has a line naming `dir`, as spelled or as it resolves.
pub fn listed_in(record: &Path, dir: &Path) -> bool {
    let Ok(text) = std::fs::read_to_string(record) else {
        return false;
    };
    let spellings = crate::machine::spellings_of(dir);
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .any(|line| spellings.iter().any(|known| Path::new(line) == known))
}

/// Every spelling of the state folder without its leading dot, as a regex alternation
/// (`purlis|charter`): for a guard that matches the folder in a command's TEXT. A guard that
/// knew only `.charter` would wave a read of `.purlis/vaults` through.
pub fn state_alternation() -> String {
    STATE_DIR
        .spellings()
        .map(|name| regex::escape(name.trim_start_matches('.')))
        .collect::<Vec<_>>()
        .join("|")
}

#[cfg(test)]
#[path = "window_tests.rs"]
mod tests;
