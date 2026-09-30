//! The Bash guard's arm that keeps a chat's commits going through charter's scan (SQ-16, ADR
//! 0074): a command line that would skip the hooks a chat's git is armed with.
//!
//! An agent's usual answer to a hook that failed is to run the commit again with
//! `--no-verify`. This refuses the spellings of that it can see:
//!
//! - `git commit --no-verify`, and `-n` on `git commit`, alone or in a bundle (`-an`);
//! - `git merge --no-verify`, which skips `pre-merge-commit` (`-n` on merge is `--no-stat`);
//! - `git -c core.hooksPath=…` and `git --config-env=core.hooksPath=…`, on any subcommand;
//! - `GIT_CONFIG_COUNT`, `GIT_CONFIG_PARAMETERS` or a `GIT_CONFIG_KEY_<n>`/`GIT_CONFIG_VALUE_<n>`
//!   set in front of a command or exported on the line, which replace the chat's pair.
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
const FIX: &str = "That would skip charter's scan of the commit for secrets and personal data. \
     Fix what the scan found and commit again; if it is not what it looks like, tell the \
     operator (allowlisting arrives with SQ-17). Do not use --no-verify.";

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

/// The spelling that would skip a chat's commit hooks, and the refusal, or `None`.
pub fn hook_skip_hit(cmd: &str) -> Option<(&'static str, String)> {
    let segments = shellseg::segment_argv(cmd);
    let exported = shellwrap::exported_env(&segments);
    for (toks, exported) in segments.iter().zip(exported) {
        let (prog, env, argv) = shellwrap::split_env(toks);
        let base = base_lower(&prog);
        let exports: Vec<&String> = if base == "export" {
            argv.iter().skip(1).collect()
        } else {
            Vec::new()
        };
        if env
            .iter()
            .chain(exported.iter())
            .chain(exports)
            .any(|a| replaces_the_chats_config(a))
        {
            return Some((
                "GIT_CONFIG_*",
                format!(
                    "setting git's config through its environment replaces the pair a chat's git runs charter's hooks with. {FIX}"
                ),
            ));
        }
        if base != "git" {
            continue;
        }
        let (globals, rest) = shellwrap::git_globals(&argv);
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
                        "`core.hooksPath` on git's command line replaces charter's hooks. {FIX}"
                    ),
                ));
            }
        }
        let Some((sub, args)) = rest.split_first() else {
            continue;
        };
        let skip = match sub.as_str() {
            "commit" => commit_skips_hooks(args),
            "merge" => args
                .iter()
                .any(|a| a == "--no-verify")
                .then_some("--no-verify"),
            _ => None,
        };
        if let Some(spelling) = skip {
            return Some((
                spelling,
                format!("`git {sub} {spelling}` skips the hooks a chat's commits run. {FIX}"),
            ));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::hook_skip_hit;

    fn refused(cmd: &str) -> Option<&'static str> {
        hook_skip_hit(cmd).map(|(spelling, _)| spelling)
    }

    #[test]
    fn a_commit_that_skips_its_hooks_is_refused_however_it_is_spelled() {
        for (cmd, spelling) in [
            ("git commit --no-verify -m x", "--no-verify"),
            ("git commit -n -m x", "-n"),
            ("git commit -anm x", "-n"),
            ("cd repo && git -C . commit -qn", "-n"),
            ("git merge --no-verify side", "--no-verify"),
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
            "git push --no-verify",
            "echo git commit --no-verify",
        ] {
            assert_eq!(refused(cmd), None, "{cmd}");
        }
    }

    #[test]
    fn the_refusal_says_what_to_do_instead() {
        let (_, why) = hook_skip_hit("git commit --no-verify").unwrap();
        assert!(why.contains("Do not use --no-verify"), "{why}");
        assert!(why.contains("SQ-17"), "{why}");
    }
}
