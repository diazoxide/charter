//! The native transport's ETag store (ADR 0070 §3): what makes a repeated `GET` a conditional
//! request, so an unchanged answer is a `304` with no body: not counted against a GitHub
//! account's limit, and one request on GitLab, which counts every request ([`super::budget`]).
//!
//! **Per account, and the native transport's alone.** A store holds what one account's token
//! fetched, keyed by the request's path and query. Only [`super::http::Http`] reads or writes
//! one, and only a human in the window resolves to it, so an answer fetched with the human's
//! token is never handed to a chat's call or to a CLI call, which may be logged in as someone
//! else. It is **separate from FW-7's item cache**, which sits above the traits and holds
//! neutral items, not raw answers.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use sha2::{Digest, Sha256};

use super::backend::Account;

/// One stored answer: its ETag, the body, and the `Link` header that paginates it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Stored {
    pub etag: String,
    pub link: Option<String>,
    pub body: String,
}

/// An ETag store for one account.
pub trait EtagStore: Send + Sync {
    fn get(&self, key: &str) -> Option<Stored>;
    fn put(&self, key: &str, stored: Stored);
}

/// An [`EtagStore`] that lives as long as the process.
#[derive(Default)]
pub struct MemoryEtags(Mutex<HashMap<String, Stored>>);

impl EtagStore for MemoryEtags {
    fn get(&self, key: &str) -> Option<Stored> {
        self.0.lock().ok()?.get(key).cloned()
    }

    fn put(&self, key: &str, stored: Stored) {
        if let Ok(mut map) = self.0.lock() {
            map.insert(key.to_string(), stored);
        }
    }
}

/// An [`EtagStore`] in the machine tier: `<config>/forge-etags/native-<account>/`, one file
/// per request, named by the SHA-256 of its path and query. Machine, device-bound and
/// rebuildable (`docs/plane-format.md`). A file that cannot be read or written is a miss, never
/// an error.
pub struct EtagDir {
    dir: PathBuf,
}

/// Where every account's store lives under the machine store `config_root`. A chat's sandbox
/// denies it to read and to write (`sandbox::Denied`, the human-powers class): what is in it was
/// fetched with the human's sign-in token.
pub fn root(config_root: &Path) -> PathBuf {
    crate::machine::dir(config_root).join("forge-etags")
}

impl EtagDir {
    /// The store for what the native transport fetched as `account`, under the machine store
    /// `config_root`. The directory names the transport as well as the account, so nothing a
    /// different identity fetched can share it.
    pub fn for_native(config_root: &Path, account: &Account) -> EtagDir {
        EtagDir {
            dir: root(config_root).join(format!("native-{}", account.key())),
        }
    }

    fn file(&self, key: &str) -> PathBuf {
        let digest = Sha256::digest(key.as_bytes());
        let name: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        self.dir.join(format!("{name}.json"))
    }
}

impl EtagStore for EtagDir {
    fn get(&self, key: &str) -> Option<Stored> {
        let text = std::fs::read_to_string(self.file(key)).ok()?;
        serde_json::from_str(&text).ok()
    }

    fn put(&self, key: &str, stored: Stored) {
        let Ok(text) = serde_json::to_string(&stored) else {
            return;
        };
        let _ = write_private(&self.dir, &self.file(key), text.as_bytes());
    }
}

/// Write `bytes` to `file` in `dir` whole, the directory `0700` and the file `0600` on unix:
/// written beside it, then renamed over it.
pub(super) fn write_private(dir: &Path, file: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    builder.create(dir)?;
    let mut temp = tempfile::NamedTempFile::new_in(dir)?;
    std::io::Write::write_all(&mut temp, bytes)?;
    temp.persist(file).map_err(|e| e.error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::forge::Kind;

    #[test]
    fn an_account_key_is_one_safe_path_segment() {
        let account = Account {
            kind: Kind::GitHub,
            host: "ghe.example.com:8443".into(),
            login: "../me".into(),
        };
        assert_eq!(account.key(), "github-ghe.example.com_8443-.._me");
        let dir = EtagDir::for_native(Path::new("/c"), &account);
        assert_eq!(
            dir.dir,
            Path::new("/c/charter/forge-etags/native-github-ghe.example.com_8443-.._me")
        );
    }

    #[test]
    fn a_stored_answer_survives_the_store_being_opened_again() {
        let root = tempfile::tempdir().unwrap();
        let account = Account {
            kind: Kind::GitHub,
            host: "github.com".into(),
            login: "octocat".into(),
        };
        let stored = Stored {
            etag: "\"v1\"".into(),
            link: None,
            body: "[]".into(),
        };
        EtagDir::for_native(root.path(), &account).put("repos/o/r/pulls/1", stored.clone());
        assert_eq!(
            EtagDir::for_native(root.path(), &account).get("repos/o/r/pulls/1"),
            Some(stored)
        );
    }
}
