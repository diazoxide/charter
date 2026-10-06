//! The Bash guard's arm that keeps a chat's commits going through charter's scan (SQ-16, ADR
//! 0074): a command line that would skip the hooks a chat's git is armed with.
//!
//! An agent's usual answer to a hook that failed is to run the commit again with
//! `--no-verify`. This refuses the spellings of that it can see:
//!
//! - `git commit --no-verify`, and `-n` on `git commit`, alone or in a bundle (`-an`);
//! - `git merge --no-verify`, which skips `pre-merge-commit` (`-n` on merge is `--no-stat`);
//! - `git push --no-verify`, which skips `pre-push`, the same scan of what a push sends (SQ-7;
//!   `-n` on push is `--dry-run`), and on `merge` and `push` each prefix of `--no-verify` git
//!   takes as it (`--no-veri` and longer);
//! - `git send-pack`, the plumbing under `git push`, which runs no `pre-push`;
//! - `git -c core.hooksPath=…` and `git --config-env=core.hooksPath=…`, on any subcommand;
//! - `GIT_CONFIG_COUNT`, `GIT_CONFIG_PARAMETERS` or a `GIT_CONFIG_KEY_<n>`/`GIT_CONFIG_VALUE_<n>`
//!   set in front of a command or exported on the line, which replace the chat's pair.
//! - `git revert` of a commit that changed the scan's allowlist, and `git merge --ff-only` to a
//!   commit whose allowlist differs from `HEAD`'s (SQ-17): the two ways onto a changed allowlist
//!   without a `pre-commit` that the command line names cheaply. git is asked what each target
//!   changes, with a short deadline, and a git that cannot say lets the command through.
//!
//! **Ungated**, like the leak guard: a chat commits in repositories outside any plane.
//!
//! # What it is not
//!
//! A mistake guard. It reads the command line, so `sh -c '…'`, a script, an alias in the
//! repository's config or a variable set earlier in the session are outside it, as they are for
//! every arm (`shellwrap::exported_env` states the boundary). Holding a chat to its hooks for
//! certain is the sandbox's work (ADR 0067), not this arm's; ADR 0074 records that follow-up.

use crate::shellseg;
use crate::shellwrap::{self, base_lower};

/// What every refusal of this arm ends with.
const FIX: &str = "That would skip purlis's scan of what it publishes for secrets and personal \
     data. Fix what the scan found and try again. If a finding is not what it looks like, \
     `purlis scan --explain` names the entry that would let it through; tell the operator, \
     who commits it to .charter-scan-allow.toml. Do not use --no-verify.";

/// The variables that replace a chat's git config pair when a command sets them.
fn replaces_the_chats_config(assignment: &str) -> bool {
    let name = assignment.split('=').next().unwrap_or("");
    name == "GIT_CONFIG_COUNT"
        || name == "GIT_CONFIG_PARAMETERS"
        || name.starts_with("GIT_CONFIG_KEY_")
        || name.starts_with("GIT_CONFIG_VALUE_")
}

/// Whether a `-c`/`--config-env` value names `core.hooksPath`, whatever its case.
fn names_hooks_path(setting: &str) -> bool {
    setting
        .split('=')
        .next()
        .is_some_and(|key| key.eq_ignore_ascii_case("core.hookspath"))
}

/// `git commit`'s short options that take a value, so `-m -n` is a message and not a flag.
const COMMIT_VALUE_SHORTS: &str = "mFcCt";

/// Whether `git commit`'s arguments ask it to skip its hooks.
fn commit_skips_hooks(args: &[String]) -> Option<&'static str> {
    let mut i = 0;
    while i < args.len() {
        let arg = args[i].as_str();
        if arg == "--" {
            return None;
        }
        if arg == "--no-verify" {
            return Some("--no-verify");
        }
        if let Some(bundle) = arg.strip_prefix('-').filter(|b| !b.starts_with('-')) {
            for (at, flag) in bundle.char_indices() {
                if flag == 'n' {
                    return Some("-n");
                }
                if COMMIT_VALUE_SHORTS.contains(flag) {
                    // The rest of the bundle is the value, or the next token is.
                    if at + flag.len_utf8() == bundle.len() {
                        i += 1;
                    }
                    break;
                }
            }
        }
        i += 1;
    }
    None
}

/// Whether a `merge` or `push` argument is `--no-verify` as git's option parser takes it: the
/// whole word, or any prefix of it long enough to name it alone (`--no-veri`; `--no-ver` is
/// also the start of `--no-verbose`, which git refuses as ambiguous). `commit` is read by
/// [`commit_skips_hooks`].
fn skips_verify(arg: &str) -> bool {
    const WORD: &str = "--no-verify";
    arg.len() >= "--no-veri".len() && WORD.starts_with(arg)
}

/// The spelling that would skip a chat's commit hooks, and the refusal, or `None`.
pub fn hook_skip_hit(cmd: &str, cwd: &str) -> Option<(&'static str, String)> {
    let segments = shellseg::segment_argv(cmd);
    let exported = shellwrap::exported_env_shared(&segments);
    // The list only grows, so each export is looked at once, in the first segment that
    // inherits it: a line of exports stays linear.
    let mut looked = 0usize;
    for (i, toks) in segments.iter().enumerate() {
        let inherited = exported.before(i);
        let fresh = &inherited[looked.min(inherited.len())..];
        looked = inherited.len();
        let (prog, env, argv) = shellwrap::split_env(toks);
        let base = base_lower(&prog);
        let exports: Vec<&String> = if base == "export" {
            argv.iter().skip(1).collect()
        } else {
            Vec::new()
        };
        if env
            .iter()
            .chain(fresh.iter())
            .chain(exports)
            .any(|a| replaces_the_chats_config(a))
        {
            return Some((
                "GIT_CONFIG_*",
                format!(
                    "setting git's config through its environment replaces the pair a chat's git runs purlis's hooks with. {FIX}"
                ),
            ));
        }
        if base != "git" {
            continue;
        }
        let (globals, rest) = shellwrap::git_globals(&argv);
        // Where this git runs: the chat's directory, moved by each `-C` in front of the
        // subcommand, as git reads them.
        let mut dir = std::path::PathBuf::from(cwd);
        let mut c = globals.iter();
        while let Some(opt) = c.next() {
            if opt == "-C"
                && let Some(to) = c.next()
            {
                dir = dir.join(to);
            }
        }
        let mut g = globals.iter();
        while let Some(opt) = g.next() {
            let setting = match opt.as_str() {
                "-c" | "--config-env" => g.next().map(String::as_str),
                other => other
                    .strip_prefix("--config-env=")
                    .or_else(|| other.strip_prefix("-c")),
            };
            if setting.is_some_and(names_hooks_path) {
                return Some((
                    "core.hooksPath",
                    format!(
                        "`core.hooksPath` on git's command line replaces purlis's hooks. {FIX}"
                    ),
                ));
            }
        }
        let Some((sub, args)) = rest.split_first() else {
            continue;
        };
        let skip = match sub.as_str() {
            "commit" => commit_skips_hooks(args),
            // `-n` on merge is `--no-stat`, and on push `--dry-run`.
            "merge" | "push" => args
                .iter()
                .any(|a| skips_verify(a))
                .then_some("--no-verify"),
            // The plumbing under `git push`, which runs no `pre-push` at all.
            "send-pack" => Some("send-pack"),
            _ => None,
        };
        if sub == "send-pack" {
            return Some((
                "send-pack",
                format!(
                    "`git send-pack` pushes without the `pre-push` hook a chat's pushes run. \
                     Use `git push`. {FIX}"
                ),
            ));
        }
        if let Some(spelling) = skip {
            let what = if sub == "push" { "pushes" } else { "commits" };
            return Some((
                spelling,
                format!("`git {sub} {spelling}` skips the hooks a chat's {what} run. {FIX}"),
            ));
        }
        if !cwd.is_empty() && moves_the_allowlist(&dir, sub, args) {
            return Some((
                "allowlist",
                format!(
                    "`git {sub}` here would change {}, the scan's allowlist, without the \
                     `pre-commit` that refuses a chat's change to it. Only the operator changes \
                     that file. {FIX}",
                    crate::scanallow::FILE
                ),
            ));
        }
    }
    None
}

/// How long the guard waits for git to say what a revert or a merge would change. A guard runs
/// before every Bash call, so it is short, and a git that does not answer in time lets the
/// command through: this is a mistake guard (ADR 0074).
const ASKING_GIT: std::time::Duration = std::time::Duration::from_secs(3);

/// Whether `git <sub> <args>` in `dir` would move `HEAD` onto a change to the scan's allowlist
/// without a `pre-commit` (SQ-17): a `revert` of a commit that changed the file, or a
/// `merge --ff-only` to a commit whose file differs from `HEAD`'s. The two ways past the hook
/// that the command line names cheaply; the rest are ADR 0074's stated gap.
fn moves_the_allowlist(dir: &std::path::Path, sub: &str, args: &[String]) -> bool {
    let targets: Vec<&str> = args
        .iter()
        .filter(|a| !a.starts_with('-'))
        .map(String::as_str)
        .collect();
    let names_the_file = |args: &[&str]| {
        crate::worktree::git::run_as_session(dir, args, ASKING_GIT)
            .ok()
            .filter(|run| run.code == Some(0))
            .is_some_and(|run| {
                String::from_utf8_lossy(&run.out)
                    .lines()
                    .any(crate::scanallow::is_the_file)
            })
    };
    match sub {
        "revert" => targets.iter().any(|commit| {
            names_the_file(&[
                "diff-tree",
                // A repository's first commit has no parent to compare with, and without this
                // git names none of the files it added.
                "--root",
                "--no-commit-id",
                "--name-only",
                "--no-renames",
                "-r",
                commit,
            ])
        }),
        "merge" if args.iter().any(|a| a == "--ff-only") => targets
            .iter()
            .any(|to| names_the_file(&["diff", "--name-only", "--no-renames", "HEAD", to])),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::hook_skip_hit;

    fn refused(cmd: &str) -> Option<&'static str> {
        hook_skip_hit(cmd, "").map(|(spelling, _)| spelling)
    }

    #[test]
    fn a_commit_that_skips_its_hooks_is_refused_however_it_is_spelled() {
        for (cmd, spelling) in [
            ("git commit --no-verify -m x", "--no-verify"),
            ("git commit -n -m x", "-n"),
            ("git commit -anm x", "-n"),
            ("cd repo && git -C . commit -qn", "-n"),
            ("git merge --no-verify side", "--no-verify"),
            ("git push --no-verify origin main", "--no-verify"),
            ("git push --no-verif origin main", "--no-verify"),
            ("git push --no-veri", "--no-verify"),
            ("git merge --no-verif side", "--no-verify"),
            (
                "git send-pack https://forge.invalid/o/r.git main",
                "send-pack",
            ),
            (
                "git -c core.hooksPath=/dev/null commit -m x",
                "core.hooksPath",
            ),
            ("git -c CORE.HOOKSPATH= commit -m x", "core.hooksPath"),
            ("git --config-env=core.hooksPath=H commit", "core.hooksPath"),
            ("GIT_CONFIG_COUNT=0 git commit -m x", "GIT_CONFIG_*"),
            ("env GIT_CONFIG_PARAMETERS=x git commit", "GIT_CONFIG_*"),
            ("export GIT_CONFIG_COUNT=0; git commit -m x", "GIT_CONFIG_*"),
        ] {
            assert_eq!(refused(cmd), Some(spelling), "{cmd}");
        }
    }

    #[test]
    fn an_ordinary_commit_or_merge_is_not_refused() {
        for cmd in [
            "git commit -m 'no -n here'",
            "git commit -m -n",
            "git commit -F msg.txt -a",
            "git merge -n side",
            "git -c user.name=x commit -m y",
            "git log -n 3",
            "git push -n origin main",
            "git push --no-ver origin main",
            "git push --no-verbose origin main",
            "echo git commit --no-verify",
        ] {
            assert_eq!(refused(cmd), None, "{cmd}");
        }
    }

    #[test]
    fn the_refusal_says_what_to_do_instead() {
        let (_, why) = hook_skip_hit("git commit --no-verify", "").unwrap();
        assert!(why.contains("Do not use --no-verify"), "{why}");
        assert!(why.contains("purlis scan --explain"), "{why}");
    }

    /// A repository whose `HEAD` has an allowlist, a branch `wider` that widens it, and the
    /// commit that made it at `HEAD`.
    fn with_an_allowlist() -> (tempfile::TempDir, String) {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        let run = |args: &[&str]| crate::testgit::run(repo, args);
        run(&["init", "-q", "-b", "main", "."]);
        run(&["config", "user.name", "t"]);
        run(&["config", "user.email", "t@example.invalid"]);
        std::fs::write(repo.join(crate::scanallow::FILE), "# narrow\n").unwrap();
        run(&["add", "."]);
        run(&["commit", "-q", "-m", "narrow"]);
        run(&["checkout", "-q", "-b", "wider"]);
        std::fs::write(repo.join(crate::scanallow::FILE), "# wider\n").unwrap();
        run(&["commit", "-q", "-am", "wider"]);
        run(&["checkout", "-q", "main"]);
        std::fs::write(repo.join("other.txt"), "x\n").unwrap();
        run(&["add", "other.txt"]);
        run(&["commit", "-q", "-m", "unrelated"]);
        let cwd = repo.display().to_string();
        (dir, cwd)
    }

    #[test]
    fn a_revert_or_fast_forward_onto_an_allowlist_change_is_refused_and_others_are_not() {
        let (_dir, cwd) = with_an_allowlist();
        let hit = |cmd: &str| hook_skip_hit(cmd, &cwd).map(|(spelling, _)| spelling);

        assert_eq!(hit("git merge --ff-only wider"), Some("allowlist"));
        assert_eq!(hit("git revert --no-edit HEAD~1"), Some("allowlist"));
        assert_eq!(
            hit("git revert --no-edit HEAD"),
            None,
            "an unrelated commit"
        );
        assert_eq!(
            hit("git merge wider"),
            None,
            "a merge runs pre-merge-commit"
        );
        assert_eq!(hit("git log --ff-only wider"), None);
    }
}
