//! git for a test's own fixtures: every repository a test creates is made unable to ask a
//! signer (charter-app#191).
//!
//! **Why a repository's own config, and not `GIT_CONFIG_GLOBAL` on the test's git.** The code
//! under test runs git through [`crate::worktree::git`], which clears the environment and keeps
//! `HOME` — so nothing a test sets in its own environment reaches the product's git, and the
//! product's git reads the developer's `~/.gitconfig`. In a fixture that is a `rebase` in
//! `charter save` signing the commits it replays. A repository's LOCAL config beats the global
//! one and is read by every git that touches the repository, the product's included, so that is
//! where `commit.gpgsign = false` goes.
//!
//! **Why through a template.** It needs no knowledge of where an `init` or a `clone` puts the
//! repository: git copies the template's `config` into whatever it creates. The template is
//! `tests/support/git-template`, shared with the integration tests' `support` module.
//!
//! The product's own git calls are untouched: this module exists only under `cfg(test)`.

use std::path::Path;

use crate::worktree::git;

/// The directory every fixture repository is created from.
pub(crate) const TEMPLATE: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/tests/support/git-template");

/// `args` for `git`, with `init.templateDir` set on the command line, so any repository they
/// create — by `init` or by `clone`, wherever it lands — is made from [`TEMPLATE`]. A verb that
/// creates nothing never reads it.
pub(crate) fn unsigned(args: &[&str]) -> Vec<String> {
    let mut out = vec!["-c".to_string(), format!("init.templateDir={TEMPLATE}")];
    out.extend(args.iter().map(|a| (*a).to_string()));
    out
}

/// git in `dir` for a test's own setup, through the same runner the product uses, with any
/// repository it creates unable to ask the developer's signer.
pub(crate) fn run(dir: &Path, args: &[&str]) -> git::Run {
    let argv = unsigned(args);
    let argv: Vec<&str> = argv.iter().map(String::as_str).collect();
    git::run_untimed(dir, &argv).expect("git runs")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    /// A `HOME` whose `.gitconfig` signs every commit and tag with a signer that fails and
    /// leaves a mark — the developer machine of charter-app#191, 1Password swapped for a script.
    fn signing_home(at: &Path) -> (std::path::PathBuf, std::path::PathBuf) {
        let home = at.join("home");
        std::fs::create_dir_all(&home).unwrap();
        let ran = at.join("signer-ran");
        let signer = stand_in::program(
            at,
            "gpg",
            &format!("#!/bin/sh\ntouch '{}'\nexit 1\n", ran.display()),
        );
        std::fs::write(
            home.join(".gitconfig"),
            format!(
                "[user]\n\tname = Dev\n\temail = dev@example.invalid\n\
                 [commit]\n\tgpgsign = true\n[tag]\n\tgpgsign = true\n\
                 [gpg]\n\tprogram = {}\n",
                signer.display()
            ),
        )
        .unwrap();
        (home, ran)
    }

    /// git as a developer with `home` runs it: that home's global config, no system config.
    fn as_developer(home: &Path, dir: &Path, args: &[String]) -> std::process::Output {
        let mut cmd = Command::new("git");
        cmd.current_dir(dir)
            .args(args)
            .env("HOME", home)
            .env_remove("GIT_CONFIG_GLOBAL")
            .env_remove("XDG_CONFIG_HOME")
            .env("GIT_CONFIG_NOSYSTEM", "1");
        crate::forklock::output(&mut cmd).expect("git runs")
    }

    fn words(args: &[&str]) -> Vec<String> {
        args.iter().map(|a| (*a).to_string()).collect()
    }

    #[test]
    fn the_signing_home_really_does_ask_its_signer_without_the_template() {
        // The control: without it, the next test could pass because the home never signed.
        let dir = tempfile::tempdir().unwrap();
        let top = dir.path().canonicalize().unwrap();
        let (home, ran) = signing_home(&top);
        let repo = top.join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        as_developer(&home, &repo, &words(&["init", "-q", "-b", "main", "."]));
        let done = as_developer(
            &home,
            &repo,
            &words(&["commit", "-q", "--allow-empty", "-m", "one"]),
        );
        assert!(
            !done.status.success(),
            "the signer refused, so the commit fails"
        );
        assert!(ran.exists(), "the signer was asked");
    }

    #[test]
    fn a_repo_a_fixture_inits_commits_and_tags_without_asking_the_developers_signer() {
        let dir = tempfile::tempdir().unwrap();
        let top = dir.path().canonicalize().unwrap();
        let (home, ran) = signing_home(&top);
        let repo = top.join("repo");
        std::fs::create_dir_all(&repo).unwrap();

        as_developer(&home, &repo, &unsigned(&["init", "-q", "-b", "main", "."]));
        let commit = as_developer(
            &home,
            &repo,
            &words(&["commit", "-q", "--allow-empty", "-m", "one"]),
        );
        let tag = as_developer(&home, &repo, &words(&["tag", "-a", "-m", "v1", "v1"]));

        assert!(commit.status.success(), "{commit:?}");
        assert!(tag.status.success(), "{tag:?}");
        assert!(!ran.exists(), "the signer was asked");
    }

    #[test]
    fn a_repo_a_fixture_clones_commits_without_asking_the_developers_signer() {
        let dir = tempfile::tempdir().unwrap();
        let top = dir.path().canonicalize().unwrap();
        let (home, ran) = signing_home(&top);
        let origin = top.join("origin.git");
        std::fs::create_dir_all(&origin).unwrap();
        // The origin is NOT made from the template: the clone has to get it on its own.
        as_developer(
            &home,
            &origin,
            &words(&["init", "-q", "--bare", "-b", "main"]),
        );

        as_developer(
            &home,
            &top,
            &unsigned(&["clone", "-q", "origin.git", "copy"]),
        );
        let commit = as_developer(
            &home,
            &top.join("copy"),
            &words(&["commit", "-q", "--allow-empty", "-m", "one"]),
        );

        assert!(commit.status.success(), "{commit:?}");
        assert!(!ran.exists(), "the signer was asked");
    }
}
