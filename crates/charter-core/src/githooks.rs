//! The git hooks charter arms every chat with (SQ-16, ADR 0074): git's own `pre-commit` and
//! `pre-merge-commit`, in whatever repository the chat commits to — a workspace's repo, a piece,
//! a clone the agent made itself outside any plane.
//!
//! # How a chat is armed
//!
//! **One config pair in the chat's environment**, `core.hooksPath` through git's documented
//! `GIT_CONFIG_COUNT`/`GIT_CONFIG_KEY_<n>`/`GIT_CONFIG_VALUE_<n>` ([`GitHooks::arm`]). Every
//! git the chat runs reads it, whatever the harness, whatever the repository, and nothing is
//! written into any repository's config. It is the one arming that follows a chat into a
//! repository charter never cloned, which a hook in `.git/hooks` or a `PreToolUse` guard
//! reading `git commit` off a command line cannot.
//!
//! **It is added after whatever pairs the chat already has**: a `GIT_CONFIG_COUNT` the operator
//! passes through `[chat_env] pass`, or a profile sets, keeps its pairs, and charter's is the
//! next index with the count raised by one.
//!
//! The directory it names is the app's ([`GitHooks::write`], at every launch, like the shell
//! tab's shims of ADR 0062), under the app's data directory: a sandboxed chat (ADR 0067) can
//! read and run it and cannot rewrite it.
//!
//! # The repository's own hooks still run
//!
//! `core.hooksPath` replaces a repository's hooks directory, so a shim stands in for each hook
//! in [`NAMES`], and each one runs the repository's own hook of the same name — after charter's
//! check, for the two in [`CHECKED`], and after charter stamps the message with the chat's
//! provenance trailers, for [`COMMIT_MSG`] (GL-8, [`crate::provenance`]). Husky, the pre-commit framework and git-lfs keep working
//! in a chat exactly as they do in a terminal.
//!
//! **Which directory is the repository's own is git's answer, not charter's**: the shim asks
//! `git rev-parse --git-path hooks` with charter's own pair renamed out of the way for that one
//! call, which is `core.hooksPath` from the repository's config (or any other pair) when one is
//! set and `<git dir>/hooks` when not. When git cannot answer, the shim says so and fails:
//! a repository's `pre-push` or `commit-msg` that silently did not run is worse than a hook
//! that stopped.
//!
//! # What it is not
//!
//! A guard against mistakes, like every other hook charter arms: an agent can set
//! `core.hooksPath` itself or pass `--no-verify`. The Bash guard refuses both spellings it can
//! see ([`crate::commitguard`]); the boundary is ADR 0067's sandbox, not this.

use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};

/// The CLI word the checking shims run: `charter git-hook <name>`.
pub const COMMAND: &str = "git-hook";

/// The hook a chat's own commit runs, and the only one that refuses a change to the scan's
/// allowlist (SQ-17): a merge brings the file as it was committed on the other side.
pub const PRE_COMMIT: &str = "pre-commit";

/// The hooks charter checks a commit in: an ordinary commit, and a merge commit, which git
/// never runs `pre-commit` for.
pub const CHECKED: [&str; 2] = [PRE_COMMIT, PRE_MERGE_COMMIT];

/// The hook git runs before it makes a merge commit, which never runs `pre-commit`.
pub const PRE_MERGE_COMMIT: &str = "pre-merge-commit";

/// The hook that hands charter the message git is about to commit, so an agent's own commit
/// carries its provenance trailers (GL-8, V67, ADR 0074 amended). It never refuses a commit: a
/// message charter could not stamp, or a `charter` that has gone, commits as it was written.
pub const COMMIT_MSG: &str = "commit-msg";

/// The hooks that get a shim, so the repository's own still runs. Every hook `githooks(5)`
/// names but three, which run on every index or ref change and cost a shell and a `git` each
/// time: `post-index-change`, `reference-transaction` and `fsmonitor-watchman`. A
/// repository's own hook of one of those three names does not run in a chat (ADR 0074).
/// `fsmonitor-watchman` is named by `core.fsmonitor`'s path in practice, not looked up here.
pub const NAMES: [&str; 25] = [
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
    "push-to-checkout",
    "pre-auto-gc",
    "post-rewrite",
    "sendemail-validate",
    "p4-changelist",
    "p4-prepare-changelist",
    "p4-post-changelist",
    "p4-pre-submit",
];

/// The key a chat's environment sets.
const KEY: &str = "core.hooksPath";

const COUNT: &str = "GIT_CONFIG_COUNT";

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

    /// `env`, a chat's environment as it will be started, armed: `core.hooksPath` pointed at
    /// [`GitHooks::dir`] as the next `GIT_CONFIG_KEY_<n>`/`GIT_CONFIG_VALUE_<n>` after the
    /// pairs `env` already has, and `GIT_CONFIG_COUNT` raised to include it.
    ///
    /// Where `env` names a variable twice, the LAST is the one a program is started with, and
    /// the one read here. Every entry for the count and for charter's index is replaced by one,
    /// so the answer holds each name once and does not depend on order. A count that is not a
    /// number is one git would refuse anyway; charter's pair then stands alone at index 0.
    pub fn arm(&self, env: Vec<(OsString, OsString)>) -> Vec<(OsString, OsString)> {
        let n: usize = env
            .iter()
            .rev()
            .find(|(name, _)| name == COUNT)
            .and_then(|(_, value)| value.to_str()?.trim().parse().ok())
            .unwrap_or(0);
        let key = format!("GIT_CONFIG_KEY_{n}");
        let value = format!("GIT_CONFIG_VALUE_{n}");
        let mut out: Vec<(OsString, OsString)> = env
            .into_iter()
            .filter(|(name, _)| name != COUNT && *name != *key && *name != *value)
            .collect();
        out.push((COUNT.into(), (n + 1).to_string().into()));
        out.push((key.into(), KEY.into()));
        out.push((value.into(), self.root.clone().into_os_string()));
        out
    }

    /// Writes a shim for every hook in [`NAMES`], running `charter`, and removes any other file
    /// the directory holds — a shim an earlier build wrote for a hook this one does not shim.
    /// Called at every launch, so a shim always runs the `charter` of the app that is running.
    /// Each is written beside itself and renamed into place, as the shell tab's shims are.
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
        for entry in std::fs::read_dir(&self.root)? {
            let entry = entry?;
            let name = entry.file_name();
            if !NAMES.iter().any(|kept| name == *kept) {
                let _ = std::fs::remove_file(entry.path());
            }
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

/// The shim for hook `name`: charter's check first, for a hook in [`CHECKED`], then the
/// repository's own hook of that name, with git's arguments and standard streams, as `exec` so
/// its exit status is the hook's.
///
/// When the `charter` that wrote it has gone, a checked hook refuses — a commit nothing scanned
/// is not one charter lets through. [`COMMIT_MSG`] runs `charter` with git's arguments and never
/// refuses: a message it could not stamp is committed as it was written.
fn shim(charter: &Path, hooks: &Path, name: &str) -> String {
    let charter = quoted(&charter.display().to_string());
    let hooks = quoted(&hooks.display().to_string());
    let check = if CHECKED.contains(&name) {
        format!(
            r#"if [ ! -x "$charter" ]; then
  echo "charter: this commit could not be scanned for secrets — charter's binary is gone; restart the chat from the app" >&2
  exit 1
fi
"$charter" {COMMAND} {name} || exit 1
"#
        )
    } else if name == COMMIT_MSG {
        format!(
            r#"[ -x "$charter" ] && "$charter" {COMMAND} {name} "$@"
"#
        )
    } else {
        String::new()
    };
    format!(
        r#"#!/bin/sh
# Written by charter at every launch, for its chats (SQ-16, ADR 0074). git runs this in place
# of the repository's own `{name}`; this runs that one after charter's check. Edits here are
# overwritten.
charter={charter}
hooks={hooks}
{check}# The repository's own hooks directory is git's answer with charter's pair set aside: the
# pair whose key is core.hooksPath and whose value is this directory, renamed for one call.
n=${{GIT_CONFIG_COUNT:-0}}
i=0
ours=
while [ "$i" -lt "$n" ] 2>/dev/null; do
  eval "k=\${{GIT_CONFIG_KEY_$i-}} v=\${{GIT_CONFIG_VALUE_$i-}}"
  if [ "$k" = core.hooksPath ] && [ "$v" = "$hooks" ]; then ours=$i; fi
  i=$((i + 1))
done
if [ -n "$ours" ]; then
  own=$(eval "GIT_CONFIG_KEY_$ours=charter.disarmed git rev-parse --git-path hooks")
else
  own=$(git rev-parse --git-path hooks)
fi || {{
  echo "charter: could not find this repository's own hooks, so its own {name} did not run" >&2
  exit 1
}}
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

    fn pairs(env: &[(&str, &str)]) -> Vec<(OsString, OsString)> {
        env.iter()
            .map(|(k, v)| ((*k).into(), (*v).into()))
            .collect()
    }

    fn sorted(mut env: Vec<(OsString, OsString)>) -> Vec<(OsString, OsString)> {
        env.sort();
        env
    }

    #[test]
    fn a_chat_with_no_git_config_of_its_own_is_armed_with_one_pair_at_index_zero() {
        let hooks = GitHooks::at("/app/data/git-hooks");
        assert_eq!(
            sorted(hooks.arm(pairs(&[("PATH", "/bin")]))),
            sorted(pairs(&[
                ("PATH", "/bin"),
                ("GIT_CONFIG_COUNT", "1"),
                ("GIT_CONFIG_KEY_0", "core.hooksPath"),
                ("GIT_CONFIG_VALUE_0", "/app/data/git-hooks"),
            ]))
        );
    }

    #[test]
    fn a_pair_the_operator_passes_keeps_its_index_and_charters_comes_after_it() {
        let hooks = GitHooks::at("/app/data/git-hooks");
        let armed = hooks.arm(pairs(&[
            ("GIT_CONFIG_COUNT", "1"),
            ("GIT_CONFIG_KEY_0", "user.signingKey"),
            ("GIT_CONFIG_VALUE_0", "ABC"),
        ]));
        assert_eq!(
            sorted(armed),
            sorted(pairs(&[
                ("GIT_CONFIG_COUNT", "2"),
                ("GIT_CONFIG_KEY_0", "user.signingKey"),
                ("GIT_CONFIG_VALUE_0", "ABC"),
                ("GIT_CONFIG_KEY_1", "core.hooksPath"),
                ("GIT_CONFIG_VALUE_1", "/app/data/git-hooks"),
            ]))
        );
    }

    #[test]
    fn the_count_a_chat_is_started_with_is_the_last_one_named_and_each_name_is_left_once() {
        let hooks = GitHooks::at("/h");
        let armed = hooks.arm(pairs(&[
            ("GIT_CONFIG_COUNT", "5"),
            ("GIT_CONFIG_KEY_0", "a.b"),
            ("GIT_CONFIG_VALUE_0", "1"),
            // The profile's own, after the app's: this is the one the program gets.
            ("GIT_CONFIG_COUNT", "1"),
            ("GIT_CONFIG_KEY_1", "stale"),
        ]));
        let names: Vec<&OsString> = armed.iter().map(|(k, _)| k).collect();
        let mut unique = names.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(names.len(), unique.len(), "{armed:?}");
        assert!(armed.contains(&("GIT_CONFIG_COUNT".into(), "2".into())));
        assert!(armed.contains(&("GIT_CONFIG_KEY_1".into(), "core.hooksPath".into())));
        assert!(armed.contains(&("GIT_CONFIG_VALUE_1".into(), "/h".into())));
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
        /// What the chat had before charter armed it.
        before: Vec<(OsString, OsString)>,
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
                before: Vec::new(),
                _dir: dir,
            }
        }

        fn commit(&self) -> std::process::Output {
            std::fs::write(self.repo.join("f"), "x\n").unwrap();
            self.git(&["add", "f"]);
            self.git(&["commit", "-q", "-m", "x"])
        }

        fn git(&self, args: &[&str]) -> std::process::Output {
            {
                let mut cmd = std::process::Command::new("git");
                cmd.arg("-C").arg(&self.repo).args(args);
                cmd.env("GIT_CONFIG_GLOBAL", "/dev/null")
                    .env("GIT_CONFIG_NOSYSTEM", "1")
                    .env("GIT_AUTHOR_NAME", "a")
                    .env("GIT_AUTHOR_EMAIL", "a@example.invalid")
                    .env("GIT_COMMITTER_NAME", "a")
                    .env("GIT_COMMITTER_EMAIL", "a@example.invalid");
                for (k, v) in self.hooks.arm(self.before.clone()) {
                    cmd.env(k, v);
                }
                crate::forklock::output(&mut cmd).unwrap()
            }
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
            &format!("printf '%s\\n' \"$@\" >> '{}'", said.display()),
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
            "git-hook\npre-commit\ngit-hook\ncommit-msg\n.git/COMMIT_EDITMSG\n"
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

    #[test]
    fn an_operators_own_pair_still_names_the_repositorys_hooks_under_charters() {
        let mut armed = Armed::new("exit 0");
        armed.before = pairs(&[
            ("GIT_CONFIG_COUNT", "1"),
            ("GIT_CONFIG_KEY_0", "core.hooksPath"),
            ("GIT_CONFIG_VALUE_0", ".husky"),
        ]);
        script(
            &armed.repo.join(".husky").join("commit-msg"),
            &format!("touch '{}'", armed.mark("commit-msg").display()),
        );

        let ran = armed.commit();

        assert!(ran.status.success(), "{ran:?}");
        assert!(armed.mark("commit-msg").exists());
    }

    #[test]
    fn a_merge_commit_is_checked_too() {
        let armed = Armed::new("");
        let said = armed.root.join("said");
        script(
            &armed.root.join("charter"),
            &format!("printf '%s\\n' \"$@\" >> '{}'", said.display()),
        );
        assert!(armed.commit().status.success());
        armed.git(&["checkout", "-q", "-b", "side"]);
        std::fs::write(armed.repo.join("g"), "y\n").unwrap();
        armed.git(&["add", "g"]);
        armed.git(&["commit", "-q", "-m", "side"]);
        armed.git(&["checkout", "-q", "main"]);
        std::fs::remove_file(&said).unwrap();

        let ran = armed.git(&["merge", "-q", "--no-ff", "-m", "merge", "side"]);

        assert!(ran.status.success(), "{ran:?}");
        assert!(
            std::fs::read_to_string(&said)
                .unwrap()
                .starts_with("git-hook\npre-merge-commit\n"),
            "the check, then the message"
        );
    }

    #[test]
    fn a_shim_that_cannot_find_the_repositorys_own_hooks_says_so_and_fails() {
        let armed = Armed::new("exit 0");
        let nowhere = armed.root.join("not-a-repo");
        std::fs::create_dir(&nowhere).unwrap();
        let mut cmd = std::process::Command::new(armed.hooks.dir().join("pre-push"));
        cmd.current_dir(&nowhere)
            .env("GIT_CEILING_DIRECTORIES", &armed.root)
            .env("GIT_CONFIG_GLOBAL", "/dev/null");

        let ran = crate::forklock::output(&mut cmd).unwrap();

        assert!(!ran.status.success());
        assert!(
            String::from_utf8_lossy(&ran.stderr).contains("its own pre-push did not run"),
            "{ran:?}"
        );
    }

    #[test]
    fn a_shim_an_earlier_build_wrote_for_a_hook_no_longer_shimmed_is_removed() {
        let armed = Armed::new("exit 0");
        let stale = armed.hooks.dir().join("reference-transaction");
        std::fs::write(&stale, "#!/bin/sh\n").unwrap();

        armed.hooks.write(&armed.root.join("charter")).unwrap();

        assert!(!stale.exists());
        assert!(armed.hooks.dir().join("pre-push").exists());
    }

    #[test]
    fn charter_writes_the_message_before_the_repositorys_own_commit_msg_reads_it() {
        let armed = Armed::new(
            "[ \"$2\" = commit-msg ] && printf '\\nAssisted-by: claude\\n' >> \"$3\"; exit 0",
        );
        let seen = armed.root.join("seen");
        script(
            &armed.repo.join(".git/hooks/commit-msg"),
            &format!("cp \"$1\" '{}'", seen.display()),
        );

        let ran = armed.commit();

        assert!(ran.status.success(), "{ran:?}");
        assert!(
            std::fs::read_to_string(&seen)
                .unwrap()
                .contains("Assisted-by: claude")
        );
        let body = armed.git(&["log", "-1", "--format=%B"]);
        assert_eq!(
            String::from_utf8_lossy(&body.stdout),
            "x\n\nAssisted-by: claude\n\n"
        );
    }

    #[test]
    fn a_message_charter_could_not_stamp_is_still_committed() {
        let armed = Armed::new("[ \"$2\" = commit-msg ] && exit 1; exit 0");

        let ran = armed.commit();

        assert!(ran.status.success(), "{ran:?}");
    }

    #[test]
    fn a_commit_msg_whose_charter_has_gone_still_commits_and_the_repositorys_own_runs() {
        let armed = Armed::new("exit 0");
        script(
            &armed.repo.join(".git/hooks/commit-msg"),
            &format!("touch '{}'", armed.mark("commit-msg").display()),
        );
        armed.commit();
        std::fs::remove_file(armed.root.join("charter")).unwrap();
        std::fs::remove_file(armed.mark("commit-msg")).unwrap();
        std::fs::write(armed.repo.join("f"), "y\n").unwrap();
        armed.git(&["add", "f"]);

        // pre-commit refuses a commit nothing scanned, so this one asks git to skip the check
        // and runs the message hook alone, as `git commit --no-verify` would not: by hand.
        let mut cmd = std::process::Command::new(armed.hooks.dir().join("commit-msg"));
        std::fs::write(armed.root.join("msg"), "y\n").unwrap();
        cmd.current_dir(&armed.repo).arg(armed.root.join("msg"));
        let ran = crate::forklock::output(&mut cmd).unwrap();

        assert!(ran.status.success(), "{ran:?}");
        assert!(armed.mark("commit-msg").exists());
    }
}
