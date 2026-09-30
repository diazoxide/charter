//! A chat's commit, scanned before it is made (SQ-16): a real repository outside any plane, git
//! armed the way the app arms a chat, the `charter` this build made, and a real socket.
//!
//! The acceptance line: a planted key and a planted email in an agent diff are refused before
//! commit, on a repo outside the plane.
#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Output;
use std::sync::{Mutex, mpsc};
use std::time::Duration;

use charter_core::githooks::GitHooks;
use charter_core::hookwire::{CHAT_ENV, CommitRefused, Hearing, Listener, SOCKET_ENV, TOKEN_ENV};

const CHARTER: &str = env!("CARGO_BIN_EXE_charter");

fn key() -> String {
    ["ghp", "_0123456789abcdefABCDEF"].concat()
}

/// A repository outside any plane, a chat's armed environment, and an app listening.
///
/// **The reading is dropped first**: it wakes its own thread by connecting to the socket, which
/// has to still be there.
struct Chat {
    _reading: charter_core::hookwire::Reading,
    heard: mpsc::Receiver<CommitRefused>,
    env: Vec<(std::ffi::OsString, std::ffi::OsString)>,
    repo: PathBuf,
    dir: tempfile::TempDir,
}

impl Chat {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        let hooks = GitHooks::at(root.join("app").join("git-hooks"));
        hooks.write(Path::new(CHARTER)).unwrap();
        let socket = root.join("app").join("hooks.sock");
        let listener = Listener::bind(&root, &socket).expect("a socket");
        let token = listener.tokens().issue(7).expect("a token");
        let (tx, heard) = mpsc::channel();
        let tx = Mutex::new(tx);
        let reading = listener.hear(Hearing {
            each: Box::new(|_| {}),
            answer: Box::new(|_, _| panic!("no ask")),
            noticed: Box::new(|_| {}),
            saved: Box::new(|_| {}),
            refused: Box::new(move |refused| tx.lock().unwrap().send(refused).unwrap()),
        });
        let env = hooks.arm(vec![
            (SOCKET_ENV.into(), socket.clone().into_os_string()),
            (CHAT_ENV.into(), "7".into()),
            (TOKEN_ENV.into(), token.expose().into()),
        ]);
        let repo = root.join("elsewhere").join("app");
        std::fs::create_dir_all(&repo).unwrap();
        let chat = Self {
            dir,
            repo,
            env,
            heard,
            _reading: reading,
        };
        chat.git(&["init", "-q", "-b", "main", "."]);
        std::fs::write(chat.repo.join("README.md"), "one\n").unwrap();
        chat.git(&["add", "README.md"]);
        assert!(chat.git(&["commit", "-q", "-m", "one"]).status.success());
        chat
    }

    /// git as the chat runs it: the developer's config shut out, the chat's own environment in.
    fn git(&self, args: &[&str]) -> Output {
        self.git_as(args, true)
    }

    /// git as the operator runs it in their own terminal, which charter does not arm.
    fn operator_git(&self, args: &[&str]) -> Output {
        self.git_as(args, false)
    }

    fn git_as(&self, args: &[&str], chat: bool) -> Output {
        let mut cmd = std::process::Command::new("git");
        cmd.arg("-C").arg(&self.repo).args(args);
        cmd.env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_AUTHOR_NAME", "agent")
            .env("GIT_AUTHOR_EMAIL", "agent@example.invalid")
            .env("GIT_COMMITTER_NAME", "agent")
            .env("GIT_COMMITTER_EMAIL", "agent@example.invalid")
            .env("HOME", self.dir.path());
        if chat {
            for (k, v) in &self.env {
                cmd.env(k, v);
            }
        }
        charter_core::forklock::output(&mut cmd).unwrap()
    }

    fn head(&self) -> String {
        String::from_utf8_lossy(&self.git(&["rev-parse", "HEAD"]).stdout).into_owned()
    }

    /// The repository's own hook `name`, which writes a mark when git runs it.
    fn own_hook(&self, name: &str) -> PathBuf {
        let mark = self.dir.path().join(format!("{name} ran"));
        let hook = self.repo.join(".git").join("hooks").join(name);
        std::fs::write(&hook, format!("#!/bin/sh\ntouch '{}'\n", mark.display())).unwrap();
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
        mark
    }
}

#[test]
fn a_planted_key_and_email_are_refused_before_commit_on_a_repo_outside_the_plane() {
    let chat = Chat::new();
    let before = chat.head();
    std::fs::write(
        chat.repo.join("config.py"),
        format!("TOKEN = '{}'\nOWNER = 'ada@lovelace.dev'\n", key()),
    )
    .unwrap();
    chat.git(&["add", "config.py"]);

    let ran = chat.git(&["commit", "-m", "add config"]);

    assert!(!ran.status.success(), "{ran:?}");
    assert_eq!(chat.head(), before, "nothing was committed");
    let said = String::from_utf8_lossy(&ran.stderr);
    assert!(
        said.contains("config.py:1") && said.contains("config.py:2"),
        "{said}"
    );
    assert!(
        said.contains("ghp_****") && said.contains("ad****"),
        "{said}"
    );
    assert!(
        !said.contains(&key()) && !said.contains("ada@lovelace.dev"),
        "{said}"
    );

    let heard = chat.heard.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(heard.chat, 7);
    assert!(heard.commit_refused.contains("config.py:1"), "{heard:?}");
    assert!(heard.commit_refused.contains("and 1 more"), "{heard:?}");
    assert!(!heard.commit_refused.contains(&key()), "{heard:?}");
}

#[test]
fn a_commit_all_is_scanned_as_what_it_commits() {
    let chat = Chat::new();
    let before = chat.head();
    std::fs::write(chat.repo.join("README.md"), format!("one\n{}\n", key())).unwrap();

    let ran = chat.git(&["commit", "-a", "-m", "tracked change"]);

    assert!(!ran.status.success(), "{ran:?}");
    assert_eq!(chat.head(), before);
    assert!(String::from_utf8_lossy(&ran.stderr).contains("README.md:2"));
}

#[test]
fn a_clean_commit_goes_through_and_the_repositorys_own_hooks_still_run() {
    let chat = Chat::new();
    let pre = chat.own_hook("pre-commit");
    let msg = chat.own_hook("commit-msg");
    std::fs::write(chat.repo.join("notes.md"), "nothing to see\n").unwrap();
    chat.git(&["add", "notes.md"]);

    let ran = chat.git(&["commit", "-q", "-m", "notes"]);

    assert!(ran.status.success(), "{ran:?}");
    assert!(pre.exists(), "the repository's own pre-commit ran");
    assert!(msg.exists(), "the repository's own commit-msg ran");
    assert!(chat.heard.recv_timeout(Duration::from_millis(300)).is_err());
}

#[test]
fn a_refused_commit_does_not_run_the_repositorys_own_pre_commit() {
    let chat = Chat::new();
    let pre = chat.own_hook("pre-commit");
    std::fs::write(chat.repo.join("owners"), "ada@lovelace.dev\n").unwrap();
    chat.git(&["add", "owners"]);

    assert!(!chat.git(&["commit", "-q", "-m", "owners"]).status.success());
    assert!(!pre.exists());
}

#[test]
fn the_repositorys_own_hook_decides_when_it_refuses() {
    let chat = Chat::new();
    let hook = chat.repo.join(".git").join("hooks").join("pre-commit");
    std::fs::write(&hook, "#!/bin/sh\necho 'lint failed' >&2\nexit 3\n").unwrap();
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::write(chat.repo.join("notes.md"), "fine\n").unwrap();
    chat.git(&["add", "notes.md"]);

    let ran = chat.git(&["commit", "-q", "-m", "notes"]);

    assert!(!ran.status.success());
    assert!(String::from_utf8_lossy(&ran.stderr).contains("lint failed"));
}

const DOCS_EMAIL: &str = "[[allow]]\nrule = \"email\"\npaths = [\"docs/**\"]\nreason = \"the docs name their authors\"\n";

#[test]
fn an_allowlisted_fixture_passes_once_the_operator_has_committed_the_entry() {
    let chat = Chat::new();
    std::fs::write(chat.repo.join(".charter-scan-allow.toml"), DOCS_EMAIL).unwrap();
    chat.operator_git(&["add", ".charter-scan-allow.toml"]);
    assert!(
        chat.operator_git(&["commit", "-q", "-m", "allow the docs' authors"])
            .status
            .success()
    );
    std::fs::create_dir(chat.repo.join("docs")).unwrap();
    std::fs::write(chat.repo.join("docs/intro.md"), "By ada@lovelace.dev\n").unwrap();
    chat.git(&["add", "docs/intro.md"]);

    let ran = chat.git(&["commit", "-q", "-m", "docs"]);

    assert!(ran.status.success(), "{ran:?}");
}

#[test]
fn a_chat_cannot_commit_an_allowlist_entry_for_its_own_finding() {
    let chat = Chat::new();
    let before = chat.head();
    std::fs::write(chat.repo.join(".charter-scan-allow.toml"), DOCS_EMAIL).unwrap();
    std::fs::create_dir(chat.repo.join("docs")).unwrap();
    std::fs::write(chat.repo.join("docs/intro.md"), "By ada@lovelace.dev\n").unwrap();
    chat.git(&["add", "."]);

    let ran = chat.git(&["commit", "-q", "-m", "allow myself"]);

    assert!(!ran.status.success());
    assert_eq!(chat.head(), before);
    let said = String::from_utf8_lossy(&ran.stderr);
    assert!(said.contains("only a commit made outside a chat"), "{said}");
    let heard = chat.heard.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(heard.commit_refused.contains("allowlist"), "{heard:?}");

    // Nor on its own, in a commit that finds nothing else.
    chat.git(&["reset", "-q"]);
    chat.git(&["add", ".charter-scan-allow.toml"]);
    assert!(
        !chat
            .git(&["commit", "-q", "-m", "only the file"])
            .status
            .success()
    );
}

#[test]
fn charter_scan_explain_names_the_rule_and_the_entry_that_would_let_it_through() {
    let chat = Chat::new();
    std::fs::write(chat.repo.join("notes.md"), "ada@lovelace.dev\n").unwrap();
    chat.git(&["add", "notes.md"]);

    let mut cmd = std::process::Command::new(CHARTER);
    cmd.args(["scan", "--explain"]).current_dir(&chat.repo);
    cmd.env("GIT_CONFIG_GLOBAL", "/dev/null");
    let ran = charter_core::forklock::output(&mut cmd).unwrap();

    let said = String::from_utf8_lossy(&ran.stdout);
    assert_eq!(ran.status.code(), Some(1), "{said}");
    assert!(said.contains("notes.md:1"), "{said}");
    assert!(said.contains("rule: email"), "{said}");
    assert!(
        said.contains("[[allow]]") && said.contains("paths = [\"notes.md\"]"),
        "{said}"
    );
    assert!(said.contains("fingerprint = \"sha256:"), "{said}");
    assert!(!said.contains("ada@lovelace.dev"), "{said}");
}
