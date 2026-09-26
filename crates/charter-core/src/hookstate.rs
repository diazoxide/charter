//! The plane's machine-local state as the hooks write it — `config.STATE_DIR` and the
//! `config.*_for` writers beneath `charter/hooks.py`.
//!
//! Every hook that remembers something between two tool calls writes it under the state
//! directory: the persona tool gate's per-session ceiling, the dispatch in-flight records, the
//! memory-cadence counter, the one-shot nudge markers. They share three rules, which is why
//! they share this module:
//!
//! - **The directory is [`crate::plane::state_dir`]**, which honours `$CHARTER_HOME` exactly as
//!   `config.STATE_DIR` does — the leak guard reads the same one, and a gate that wrote its
//!   ceiling somewhere the guard does not look would be two answers to "where is charter's
//!   state".
//! - **Directories at 0700 and files at 0600**, whatever the umask: a marker another account
//!   can create is a marker that account gets to set, and the ceiling decides what runs
//!   without a prompt.
//! - **No link on the way.** These are charter's own paths, created by charter, so a symlink
//!   anywhere under the trust root has no honest use.
//!
//! Every writer returns `io::Result` and every caller in a hook drops it: bookkeeping may never
//! fail a turn.

use std::io;
use std::path::{Path, PathBuf};

/// The directory a state directory's paths are gated from: the plane when `state` is inside
/// it, so a `.charter/` that is itself a link is refused — and `state` itself when
/// `$CHARTER_HOME` puts it elsewhere, since then the plane is no place to walk from and the
/// directory is the operator's own choice.
pub fn trust_root<'a>(root: &'a Path, state: &'a Path) -> &'a Path {
    if state.starts_with(root) { root } else { state }
}

/// A plane's state directory, and the directory its paths are trusted below.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct State {
    dir: PathBuf,
    trust: PathBuf,
}

impl State {
    /// The state directory of the plane at `root`.
    pub fn of(root: &Path) -> Self {
        let dir = crate::plane::state_dir(root);
        let trust = trust_root(root, &dir).to_path_buf();
        Self { dir, trust }
    }

    /// The directory this state's paths are gated from ([`trust_root`]).
    pub fn trust(&self) -> &Path {
        &self.trust
    }

    /// `config.STATE_DIR`.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// `config.SESSIONS_DIR`.
    pub fn sessions(&self) -> PathBuf {
        self.dir.join("sessions")
    }

    /// `config.PERSONA_STATE_DIR`.
    pub fn persona_state(&self) -> PathBuf {
        self.dir.join("persona-state")
    }

    /// `config.VAULTS_DIR`.
    pub fn vaults(&self) -> PathBuf {
        self.dir.join("vaults")
    }

    /// A state file's text, read as it is written: never through a link at the file or on
    /// the way from the trust root, and only a plain file (#440). `None` for a file that is
    /// missing, refused, or not UTF-8 — every hook reader already treats "cannot read it" as
    /// "nothing recorded".
    pub fn read_text(&self, path: &Path) -> Option<String> {
        crate::contain::read_text_no_link(&self.trust, path).ok()
    }

    /// [`State::read_text`] as the IO answer, for a reader that tells "not there" (a record
    /// never made) from "there but refused" (a record it must not trust).
    pub fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        crate::contain::read_no_link(&self.trust, path)
    }

    /// `config.private_mkdir`.
    pub fn mkdir(&self, dir: &Path) -> io::Result<()> {
        crate::plane::private_dir(&self.trust, dir)
    }

    /// `config.write_for` — the whole content, at 0600, the parent made private first.
    pub fn write(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            self.mkdir(parent)?;
        }
        crate::plane::write_private(&self.trust, path, bytes)
    }

    /// `config.replace_for` — whole or not at all, at 0600, the parent made private first:
    /// [`crate::rewrite::replace`], gated from the trust root.
    pub fn replace(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        let Some(parent) = path.parent() else {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "no parent"));
        };
        self.mkdir(parent)?;
        crate::rewrite::replace(&self.trust, path, bytes, crate::rewrite::Mode::Private)
    }

    /// `config.touch_for` — create at 0600 if absent, and bump the mtime either way.
    pub fn touch(&self, path: &Path) -> io::Result<()> {
        use std::io::Write;
        if let Some(parent) = path.parent() {
            self.mkdir(parent)?;
        }
        crate::contain::no_link_on_the_way(&self.trust, path)?;
        let mut options = std::fs::OpenOptions::new();
        options.append(true).create(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = crate::contain::nofollow(&mut options).open(path)?;
        file.flush()?;
        file.set_modified(std::time::SystemTime::now())
    }

    /// Remove a file of charter's own, when it is there. `missing_ok=True`.
    pub fn remove(&self, path: &Path) -> io::Result<()> {
        crate::contain::no_link_on_the_way(&self.trust, path)?;
        match std::fs::remove_file(path) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            other => other,
        }
    }
}

/// `re.sub(r"[^A-Za-z0-9._-]", "", value)` — how charter makes an id a filename.
pub fn safe(value: &str) -> String {
    value
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
        .collect()
}

/// `session.current(explicit)`: the payload's id if it has one, else `$CHARTER_SESSION_ID`,
/// else `$CLAUDE_CODE_SESSION_ID` — stripped and made a filename, `None` when nothing is left.
pub fn session(explicit: Option<&str>, env: &dyn Fn(&str) -> Option<String>) -> Option<String> {
    let raw = explicit
        .filter(|v| !v.is_empty())
        .map(str::to_owned)
        .or_else(|| env(crate::active::SESSION_ID_ENV).filter(|v| !v.is_empty()))
        // The last rung needs no empty-filter of its own: an empty value is an empty id below,
        // which is `None` either way — a filter here was a mutant nothing could see (#311).
        .or_else(|| env(crate::active::CONVERSATION_ENV))?;
    let id = safe(crate::memstore::py_strip(&raw));
    (!id.is_empty()).then_some(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_payloads_id_outranks_the_environments() {
        let env = |name: &str| (name == "CHARTER_SESSION_ID").then(|| "chat-7".to_string());
        assert_eq!(session(Some("abc"), &env).as_deref(), Some("abc"));
        assert_eq!(session(None, &env).as_deref(), Some("chat-7"));
        assert_eq!(session(Some(""), &env).as_deref(), Some("chat-7"));
        assert_eq!(session(Some(" ../x "), &|_| None).as_deref(), Some("..x"));
        assert_eq!(session(Some("///"), &|_| None), None);
    }

    #[test]
    fn removing_what_is_not_there_is_fine_and_what_cannot_be_removed_is_not() {
        let dir = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        let state = State {
            dir: root.join(".charter"),
            trust: root.clone(),
        };
        std::fs::create_dir_all(state.sessions()).unwrap();
        assert!(state.remove(&state.sessions().join("never-was")).is_ok());
        // A directory is not a file `remove_file` can take: that error is not swallowed.
        assert!(state.remove(&state.sessions()).is_err());
        assert!(state.sessions().is_dir());
    }

    #[test]
    fn each_of_charters_state_directories_is_where_config_puts_it() {
        let state = State {
            dir: PathBuf::from("/plane/.charter"),
            trust: PathBuf::from("/plane"),
        };
        assert_eq!(state.sessions(), Path::new("/plane/.charter/sessions"));
        assert_eq!(
            state.persona_state(),
            Path::new("/plane/.charter/persona-state")
        );
        assert_eq!(state.vaults(), Path::new("/plane/.charter/vaults"));
    }

    #[cfg(unix)]
    #[test]
    fn what_it_writes_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        let state = State {
            dir: root.join(".charter"),
            trust: root.clone(),
        };
        let file = state.sessions().join("s.tools");
        state.replace(&file, b"{}").unwrap();
        state.touch(&state.sessions().join("s.gate")).unwrap();
        let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&file), 0o600);
        assert_eq!(mode(&state.sessions().join("s.gate")), 0o600);
        assert_eq!(mode(&state.sessions()), 0o700);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "{}");
        state.remove(&file).unwrap();
        state.remove(&file).unwrap();
        assert!(!file.exists());
    }

    /// #440: state is read as it is written — never through a link at the file or on the way.
    #[cfg(unix)]
    #[test]
    fn state_is_never_read_through_a_link() {
        let dir = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        let state = State {
            dir: root.join(".charter"),
            trust: root.clone(),
        };
        let file = state.sessions().join("s.memnudge");
        state.write(&file, b"3").unwrap();
        assert_eq!(state.read_text(&file).as_deref(), Some("3"));

        let elsewhere = tempfile::tempdir().unwrap();
        std::fs::write(elsewhere.path().join("theirs"), b"7").unwrap();
        std::fs::remove_file(&file).unwrap();
        std::os::unix::fs::symlink(elsewhere.path().join("theirs"), &file).unwrap();
        assert_eq!(
            state.read_text(&file),
            None,
            "read through a link at the file"
        );

        std::fs::remove_dir_all(root.join(".charter")).unwrap();
        std::fs::create_dir_all(elsewhere.path().join("sessions")).unwrap();
        std::fs::write(elsewhere.path().join("sessions/s.memnudge"), b"7").unwrap();
        std::os::unix::fs::symlink(elsewhere.path(), root.join(".charter")).unwrap();
        assert_eq!(
            state.read_text(&file),
            None,
            "read through a linked .charter"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_link_on_the_way_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        let elsewhere = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(elsewhere.path(), root.join(".charter")).unwrap();
        let state = State {
            dir: root.join(".charter"),
            trust: root.clone(),
        };
        assert!(state.write(&state.sessions().join("x"), b"1").is_err());
        assert!(
            std::fs::read_dir(elsewhere.path())
                .unwrap()
                .next()
                .is_none()
        );
    }
}
