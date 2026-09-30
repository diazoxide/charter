//! The git hooks charter arms every chat with (SQ-16): git's own `pre-commit`, in whatever
//! repository the chat commits to — a workspace's repo, a piece, a clone the agent made itself
//! outside any plane.
//!
//! # How a chat is armed
//!
//! **One config pair in the chat's environment**, `core.hooksPath` through git's documented
//! `GIT_CONFIG_COUNT`/`GIT_CONFIG_KEY_<n>`/`GIT_CONFIG_VALUE_<n>` ([`GitHooks::env`]). Every
//! git the chat runs reads it, whatever the harness, whatever the repository, and nothing is
//! written into any repository's config. It is the one arming that follows a chat into a
//! repository charter never cloned, which a hook in `.git/hooks` or a `PreToolUse` guard
//! reading `git commit` off a command line cannot.
//!
//! The directory it names is the app's ([`GitHooks::write`], at every launch, like the shell
//! tab's shims of ADR 0062), under the app's data directory: a sandboxed chat (ADR 0067) can
//! read and run it and cannot rewrite it.
//!
//! # The repository's own hooks still run
//!
//! `core.hooksPath` replaces a repository's hooks directory, so a shim stands in for **every**
//! hook git runs, and each one runs the repository's own hook of the same name — after
//! charter's check, for `pre-commit`. Husky, the pre-commit framework and git-lfs keep working
//! in a chat exactly as they do in a terminal.
//!
//! **Which directory is the repository's own is git's answer, not charter's**: the shim asks
//! `git rev-parse --git-path hooks` with the chat's config pair taken away
//! (`GIT_CONFIG_COUNT=0`), which is `core.hooksPath` from the repository's own config files
//! when one is set and `<git dir>/hooks` when not. That is also why forwarding is in the shim
//! and not in `charter`: git runs a hook for every ref it moves — eleven for one `git commit`
//! — and a shell and one `git` per hook is the cheapest way to run nothing when the repository
//! has nothing there.
//!
//! # What it is not
//!
//! A guard against mistakes, like every other hook charter arms (`docs/hooks.md`): an agent can
//! still `git commit --no-verify`, or set `core.hooksPath` itself.

use std::io;
use std::path::{Path, PathBuf};

/// The CLI word the `pre-commit` shim runs: `charter git-hook pre-commit`.
pub const COMMAND: &str = "git-hook";

/// The hook charter checks a commit in.
pub const PRE_COMMIT: &str = "pre-commit";

/// Every hook `githooks(5)` names, each of which gets a shim so the repository's own still
/// runs. git-lfs, for one, lives in `pre-push`, `post-checkout`, `post-commit` and
/// `post-merge`.
pub const NAMES: [&str; 28] = [
    "applypatch-msg",
    "pre-applypatch",
    "post-applypatch",
    "pre-commit",
    "pre-merge-commit",
    "prepare-commit-msg",
    "commit-msg",
    "post-commit",
    "pre-rebase",
    "post-checkout",
    "post-merge",
    "pre-push",
    "pre-receive",
    "update",
    "proc-receive",
    "post-receive",
    "post-update",
    "reference-transaction",
    "push-to-checkout",
    "pre-auto-gc",
    "post-rewrite",
    "sendemail-validate",
    "fsmonitor-watchman",
    "p4-changelist",
    "p4-prepare-changelist",
    "p4-post-changelist",
    "p4-pre-submit",
    "post-index-change",
];

/// The key a chat's environment sets.
const KEY: &str = "core.hooksPath";

/// The app-owned directory git's hooks run from in every chat.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitHooks {
    root: PathBuf,
}

impl GitHooks {
    /// The hooks under `root`, which the app owns.
    pub fn at(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// The directory itself.
    pub fn dir(&self) -> &Path {
        &self.root
    }

    /// What arms a chat: `core.hooksPath` pointed at [`GitHooks::dir`], as git's own
    /// environment config. It takes index 0 and a count of one, so a `GIT_CONFIG_COUNT` the
    /// operator passes a chat through `[chat_env] pass` is replaced by this one.
    pub fn env(&self) -> Vec<(String, String)> {
        vec![
            ("GIT_CONFIG_COUNT".to_owned(), "1".to_owned()),
            ("GIT_CONFIG_KEY_0".to_owned(), KEY.to_owned()),
            (
                "GIT_CONFIG_VALUE_0".to_owned(),
                self.root.display().to_string(),
            ),
        ]
    }

    /// Writes a shim for every hook in [`NAMES`], running `charter`. Called at every launch,
    /// so a shim always runs the `charter` of the app that is running. Each is written beside
    /// itself and renamed into place, as the shell tab's shims are.
    #[cfg(unix)]
    pub fn write(&self, charter: &Path) -> io::Result<()> {
        use std::os::unix::fs::{DirBuilderExt, PermissionsExt};

        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&self.root)?;
        std::fs::set_permissions(&self.root, std::fs::Permissions::from_mode(0o700))?;
        for name in NAMES {
            put(&self.root.join(name), &shim(charter, &self.root, name))?;
        }
        Ok(())
    }

    /// Off unix a shim would be a POSIX shell script no git there runs, so nothing is written
    /// and the caller arms no chat with it.
    #[cfg(not(unix))]
    pub fn write(&self, _charter: &Path) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "charter's git hooks are POSIX shell scripts, and this platform has no answer yet",
        ))
    }
}

/// `text` at `path`, executable, replacing whatever was there in one step.
#[cfg(unix)]
fn put(path: &Path, text: &str) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let beside = path.with_extension(format!("charter-{}", std::process::id()));
    std::fs::write(&beside, text)?;
    std::fs::set_permissions(&beside, std::fs::Permissions::from_mode(0o755))?;
    std::fs::rename(&beside, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&beside);
    })
}

/// `text` as one POSIX shell word, whatever is in it.
fn quoted(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}

/// The shim for hook `name`: charter's check first, for `pre-commit`, then the repository's own
/// hook of that name, with git's arguments and standard streams, as `exec` so its exit status
/// is the hook's.
///
/// When the `charter` that wrote it has gone, a `pre-commit` refuses — a commit nothing
/// scanned is not one charter lets through.
fn shim(charter: &Path, hooks: &Path, name: &str) -> String {
    let charter = quoted(&charter.display().to_string());
    let hooks = quoted(&hooks.display().to_string());
    let check = if name == PRE_COMMIT {
        format!(
            r#"if [ ! -x "$charter" ]; then
  echo "charter: this commit could not be scanned for secrets — charter's binary is gone; restart the chat from the app" >&2
  exit 1
fi
"$charter" {COMMAND} {name} || exit 1
"#
        )
    } else {
        String::new()
    };
    format!(
        r#"#!/bin/sh
# Written by charter at every launch, for its chats (SQ-16). git runs this in place of the
# repository's own `{name}`; this runs that one after charter's check. Edits here are
# overwritten.
charter={charter}
hooks={hooks}
{check}# The repository's own hooks directory: git's answer without the chat's config pair.
own=$(GIT_CONFIG_COUNT=0 git rev-parse --git-path hooks 2>/dev/null) || exit 0
case "$own" in
  /*) ;;
  *) own="$PWD/$own" ;;
esac
[ -f "$own/{name}" ] && [ -x "$own/{name}" ] || exit 0
# A repository whose own config names this directory has no hooks but these.
[ "$(cd "$own" && pwd -P)" = "$(cd "$hooks" && pwd -P)" ] && exit 0
exec "$own/{name}" "$@"
"#
    )
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::testgit;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn a_chat_is_armed_by_one_config_pair_naming_the_hooks_directory() {
        let hooks = GitHooks::at("/app/data/git-hooks");
        assert_eq!(
            hooks.env(),
            vec![
                ("GIT_CONFIG_COUNT".to_owned(), "1".to_owned()),
                ("GIT_CONFIG_KEY_0".to_owned(), "core.hooksPath".to_owned()),
                (
                    "GIT_CONFIG_VALUE_0".to_owned(),
                    "/app/data/git-hooks".to_owned()
                ),
            ]
        );
    }

    fn script(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, format!("#!/bin/sh\n{text}\n")).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    /// A repository, charter's hooks written for a stand-in `charter` that runs `says`, and git
    /// as the armed chat runs it.
    struct Armed {
        root: PathBuf,
        repo: PathBuf,
        hooks: GitHooks,
        _dir: tempfile::TempDir,
    }

    impl Armed {
        fn new(says: &str) -> Self {
            let dir = tempfile::tempdir().unwrap();
            let root = std::fs::canonicalize(dir.path()).unwrap();
            script(&root.join("charter"), says);
            let hooks = GitHooks::at(root.join("git-hooks"));
            hooks.write(&root.join("charter")).unwrap();
            let repo = root.join("repo");
            std::fs::create_dir(&repo).unwrap();
            testgit::run(&repo, &["init", "-q", "-b", "main", "."]);
            Self {
                root,
                repo,
                hooks,
                _dir: dir,
            }
        }

        fn commit(&self) -> std::process::Output {
            std::fs::write(self.repo.join("f"), "x\n").unwrap();
            let git = |args: &[&str]| {
                let mut cmd = std::process::Command::new("git");
                cmd.arg("-C").arg(&self.repo).args(args);
                cmd.env("GIT_CONFIG_GLOBAL", "/dev/null")
                    .env("GIT_CONFIG_NOSYSTEM", "1")
                    .env("GIT_AUTHOR_NAME", "a")
                    .env("GIT_AUTHOR_EMAIL", "a@example.invalid")
                    .env("GIT_COMMITTER_NAME", "a")
                    .env("GIT_COMMITTER_EMAIL", "a@example.invalid");
                for (k, v) in self.hooks.env() {
                    cmd.env(k, v);
                }
                crate::forklock::output(&mut cmd).unwrap()
            };
            git(&["add", "f"]);
            git(&["commit", "-q", "-m", "x"])
        }

        fn mark(&self, name: &str) -> PathBuf {
            self.root.join(format!("{name} ran"))
        }
    }

    #[test]
    fn every_hook_git_runs_has_an_executable_shim() {
        let armed = Armed::new("exit 0");
        for name in NAMES {
            let mode = std::fs::metadata(armed.hooks.dir().join(name))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o111, 0o111, "{name} is executable");
        }
    }

    #[test]
    fn a_commit_runs_charters_check_and_then_the_repositorys_own_hooks_from_its_hooks_path() {
        let armed = Armed::new("");
        let said = armed.root.join("said");
        script(
            &armed.root.join("charter"),
            &format!("printf '%s\\n' \"$@\" > '{}'", said.display()),
        );
        testgit::run(&armed.repo, &["config", "core.hooksPath", ".husky"]);
        for name in ["pre-commit", "commit-msg"] {
            script(
                &armed.repo.join(".husky").join(name),
                &format!("touch '{}'", armed.mark(name).display()),
            );
        }

        let ran = armed.commit();

        assert!(ran.status.success(), "{ran:?}");
        assert_eq!(
            std::fs::read_to_string(&said).unwrap(),
            "git-hook\npre-commit\n"
        );
        assert!(armed.mark("pre-commit").exists());
        assert!(armed.mark("commit-msg").exists());
    }

    #[test]
    fn a_refusal_from_charter_stops_the_commit_before_the_repositorys_own_pre_commit() {
        let armed = Armed::new("echo refused >&2; exit 1");
        script(
            &armed.repo.join(".git/hooks/pre-commit"),
            &format!("touch '{}'", armed.mark("pre-commit").display()),
        );

        let ran = armed.commit();

        assert!(!ran.status.success());
        assert!(String::from_utf8_lossy(&ran.stderr).contains("refused"));
        assert!(!armed.mark("pre-commit").exists());
    }

    #[test]
    fn a_pre_commit_whose_charter_has_gone_refuses_the_commit() {
        let armed = Armed::new("exit 0");
        std::fs::remove_file(armed.root.join("charter")).unwrap();

        let ran = armed.commit();

        assert!(!ran.status.success());
        assert!(String::from_utf8_lossy(&ran.stderr).contains("could not be scanned"));
    }

    #[test]
    fn a_repository_whose_own_config_names_charters_hooks_runs_each_once() {
        let armed = Armed::new("exit 0");
        let dir = armed.hooks.dir().display().to_string();
        testgit::run(&armed.repo, &["config", "core.hooksPath", &dir]);

        assert!(armed.commit().status.success());
    }
}
