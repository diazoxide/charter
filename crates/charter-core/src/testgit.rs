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
//! The template also points `core.excludesFile` at `/dev/null`, so the developer's global
//! excludes file does not decide what a fixture calls ignored (charter-app#262).
//!
//! The product's own git calls are untouched: this module exists only under `cfg(test)`.

use std::path::{Path, PathBuf};

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

/// A `HOME` whose `.gitconfig` signs every commit and tag with a signer that fails and
/// leaves a mark — the developer machine of charter-app#191, 1Password swapped for a script.
/// Answers the home and the file the signer touches when it is asked.
pub(crate) fn signing_home(at: &Path) -> (PathBuf, PathBuf) {
    home_signing_through(at, |ran| {
        format!("#!/bin/sh\ntouch '{}'\nexit 1\n", ran.display())
    })
}

/// The same HOME with a signer that SIGNS: it answers git the way gpg does — the status line
/// git looks for on stderr, an armoured signature on stdout — and appends one line to the
/// mark per call, so a test can count how many commits were signed.
pub(crate) fn home_that_signs(at: &Path) -> (PathBuf, PathBuf) {
    home_signing_through(at, |ran| {
        format!(
            "#!/bin/sh\ncat >/dev/null\necho signed >> '{}'\n\
             printf '\\n[GNUPG:] SIG_CREATED D 1 8 00 0 FIXTURE\\n' >&2\n\
             printf -- '-----BEGIN PGP SIGNATURE-----\\n\\nZml4dHVyZQ==\\n-----END PGP SIGNATURE-----\\n'\n",
            ran.display()
        )
    })
}

fn home_signing_through(at: &Path, signer: impl Fn(&Path) -> String) -> (PathBuf, PathBuf) {
    let home = at.join("home");
    std::fs::create_dir_all(&home).unwrap();
    let ran = at.join("signer-ran");
    let signer = stand_in::program(at, "gpg", &signer(&ran));
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

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

    /// A `HOME` whose global excludes file — git's default one, `~/.config/git/ignore` — hides
    /// the machine-local Claude settings, as the operator's does (charter-app#262).
    fn excluding_home(at: &Path) -> std::path::PathBuf {
        let home = at.join("home");
        std::fs::create_dir_all(home.join(".config/git")).unwrap();
        std::fs::write(
            home.join(".config/git/ignore"),
            "**/.claude/settings.local.json\n",
        )
        .unwrap();
        home
    }

    /// Whether git in `repo`, run as the developer with `home`, calls `path` ignored.
    fn ignored(home: &Path, repo: &Path, path: &str) -> bool {
        as_developer(home, repo, &words(&["check-ignore", "-q", path]))
            .status
            .success()
    }

    #[test]
    fn the_excluding_home_really_does_hide_the_file_without_the_template() {
        // The control: without it, the next test could pass because the home hid nothing.
        let dir = tempfile::tempdir().unwrap();
        let top = dir.path().canonicalize().unwrap();
        let home = excluding_home(&top);
        let repo = top.join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        as_developer(&home, &repo, &words(&["init", "-q", "-b", "main", "."]));

        assert!(ignored(&home, &repo, ".claude/settings.local.json"));
    }

    #[test]
    fn a_repo_a_fixture_inits_or_clones_never_reads_the_developers_global_excludes() {
        let dir = tempfile::tempdir().unwrap();
        let top = dir.path().canonicalize().unwrap();
        let home = excluding_home(&top);
        let repo = top.join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        as_developer(&home, &repo, &unsigned(&["init", "-q", "-b", "main", "."]));
        as_developer(&home, &top, &unsigned(&["clone", "-q", "repo", "copy"]));

        for tree in [&repo, &top.join("copy")] {
            assert!(
                !ignored(&home, tree, ".claude/settings.local.json"),
                "{} read the developer's excludes",
                tree.display()
            );
        }
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
