//! `charter.local.toml` is gitignored, and charter does not trust its own line.
//!
//! The reason it cannot: an ignore rule does not apply to a tracked path. `git add -f`
//! commits the file, a fresh clone materialises it, and every machine that pulls runs the
//! commands in it — on a click, with no prompt in between. So while git tracks the file, or
//! WOULD commit it, every profile it declares is refused by name.
//!
//! An answer git could not give refuses too: an unknown is not a pass (ADR 0009).

use std::fs;
use std::path::Path;

use charter_core::profiles;

mod support;

fn git(dir: &Path, args: &[&str]) {
    let out = charter_core::forklock::output(
        support::unsigned()
            .args(args)
            .current_dir(dir)
            // Hermetic: the operator's own global config is not this test's business, and it
            // broke the suite once — a `commit.gpgsign` pointing at a 1Password signer failed
            // the commit with "failed to fill whole buffer" and reddened two tests that are
            // about git's answer, not about signing.
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@e")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@e"),
    )
    .expect("git runs");
    assert!(out.status.success(), "git {args:?}: {out:?}");
}

/// A plane that is a git repository, with a local file and whatever `.gitignore` is given.
fn repo(ignore: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("charter.toml"), "").unwrap();
    fs::write(
        dir.path().join(profiles::LOCAL_FILE),
        "[harness.work]\nkind = \"claude\"\ncommand = [\"claude\"]\n",
    )
    .unwrap();
    if !ignore.is_empty() {
        fs::write(dir.path().join(".gitignore"), ignore).unwrap();
    }
    git(dir.path(), &["init", "-q"]);
    dir
}

#[test]
fn a_plane_that_is_not_a_git_repository_has_nothing_to_commit_to_and_passes() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join(profiles::LOCAL_FILE), "").unwrap();

    assert!(profiles::ignore_check(dir.path()).passes());
}

#[test]
fn a_plane_with_no_local_file_declares_nothing_and_passes() {
    let dir = repo("/charter.local.toml\n");
    fs::remove_file(dir.path().join(profiles::LOCAL_FILE)).unwrap();

    assert!(profiles::ignore_check(dir.path()).passes());
}

#[test]
fn an_ignored_untracked_local_file_is_the_state_the_feature_wants_and_passes() {
    let dir = repo("/charter.local.toml\n");

    assert!(profiles::ignore_check(dir.path()).passes());
}

#[test]
fn a_local_file_git_would_commit_refuses_every_profile_in_it() {
    // No `.gitignore` line: git reports it as `??`, which means the next `git add .` takes
    // it, and then every clone has it.
    let dir = repo("");

    let check = profiles::ignore_check(dir.path());

    assert_eq!(
        check.reason,
        "git would commit charter.local.toml, so the profiles in it are refused until it is \
         ignored — charter reinit adds /charter.local.toml to .gitignore."
    );
    assert_eq!(check.fix, "charter reinit");
}

#[test]
fn a_tracked_local_file_is_refused_and_told_that_reinit_alone_will_not_fix_it() {
    // One fix per state: `charter reinit` adds the ignore line, and an ignore rule does not
    // apply to a path git already tracks.
    let dir = repo("/charter.local.toml\n");
    git(dir.path(), &["add", "-f", profiles::LOCAL_FILE]);
    git(dir.path(), &["commit", "-qm", "forced"]);

    let check = profiles::ignore_check(dir.path());

    assert_eq!(
        check.reason,
        "git tracks charter.local.toml, so the profiles in it would reach every clone of \
         this plane — charter refuses them until it is untracked: git rm --cached \
         charter.local.toml, commit that removal, then charter reinit."
    );
    assert_eq!(
        check.fix,
        "git rm --cached charter.local.toml, commit that removal, then charter reinit"
    );
}

#[test]
fn a_file_staged_for_removal_but_not_yet_committed_is_still_tracked() {
    // `git rm --cached` leaves `D ` beside `!!` until the removal is committed, and the file
    // is in HEAD until then — so a clone still carries it.
    let dir = repo("/charter.local.toml\n");
    git(dir.path(), &["add", "-f", profiles::LOCAL_FILE]);
    git(dir.path(), &["commit", "-qm", "forced"]);
    git(dir.path(), &["rm", "-q", "--cached", profiles::LOCAL_FILE]);

    assert!(
        profiles::ignore_check(dir.path())
            .reason
            .starts_with("git tracks charter.local.toml,"),
        "a removal that is not committed read as untracked"
    );
}

#[test]
fn an_answer_git_could_not_give_refuses_because_an_unknown_is_not_a_pass() {
    // ADR 0009. There is no git on the PATH this check is given, so it cannot look — which
    // is not the same as looking and finding the file ignored.
    let dir = repo("/charter.local.toml\n");

    let check = profiles::ignore_check_with(dir.path(), Path::new("/definitely/not/git"));

    assert!(
        check
            .reason
            .starts_with("git could not say whether charter.local.toml is ignored (")
            && check.reason.ends_with(
                "), so the profiles in it are refused — an unknown is not a pass. Run git status \
             --ignored -- charter.local.toml in the plane to see what git says."
            ),
        "{:?}",
        check.reason
    );
    assert!(check.fix.starts_with(
        "run git status --ignored -- charter.local.toml in the plane by hand; git said: "
    ));
}

#[test]
fn a_refusing_check_moves_every_declared_profile_to_refused_and_leaves_the_built_ins() {
    // Each of the three sentences says "the profiles in it are refused", so a surface that
    // asked must show them refused — not as ordinary rows with a warning underneath.
    let dir = repo("");
    let set = profiles::current(dir.path());
    assert!(
        set.get("work").is_some(),
        "the profile reads fine on its own"
    );

    let guarded = profiles::with_ignore_check(set, &profiles::ignore_check(dir.path()));

    assert!(guarded.get("work").is_none());
    assert_eq!(
        guarded
            .profiles()
            .iter()
            .map(|p| p.name.as_str())
            .collect::<Vec<_>>(),
        ["claude", "opencode", "codex"]
    );
    assert!(
        guarded
            .refused
            .iter()
            .any(|r| r.name == "work" && r.reason.starts_with("git would commit"))
    );
}

#[test]
fn a_replacement_refused_by_the_git_check_does_not_let_its_built_in_stand_in() {
    // Ruling 19: the operator said how `claude` runs here, and the built-in standing in
    // would run the command they replaced.
    let dir = repo("");
    fs::write(
        dir.path().join(profiles::LOCAL_FILE),
        "[harness.claude]\nkind = \"claude\"\ncommand = [\"~/.local/bin/claude\"]\n",
    )
    .unwrap();

    let guarded = profiles::with_ignore_check(
        profiles::current(dir.path()),
        &profiles::ignore_check(dir.path()),
    );

    assert_eq!(
        guarded
            .profiles()
            .iter()
            .map(|p| p.name.as_str())
            .collect::<Vec<_>>(),
        ["opencode", "codex"],
        "the built-in `claude` stood in for the refused replacement"
    );
}

/// A stand-in `git` in `dir` that runs `body` whatever it is asked.
///
/// Through `stand_in::program`, never `fs::write` here: these tests run it the instant it is
/// written, and written from this process that lost to `ETXTBSY` on a green branch
/// (charter-app#81). The reasoning is in that crate.
fn a_git_that(dir: &Path, name: &str, body: &str) -> std::path::PathBuf {
    stand_in::program(dir, name, &format!("#!/bin/sh\n{body}\n"))
}

#[test]
fn a_git_that_refuses_the_repository_is_not_taken_for_one_that_found_none() {
    // Only git's own "not a git repository" at exit 128 is a plane with nothing to commit
    // to. Exit 128 is also what git answers when it refuses to look — a repository owned by
    // someone else, `safe.directory` unset — and that is an unknown, not a pass. Turning the
    // `&&` into `||` (nightly run 35430920851) passed the suite and waved every declared
    // profile through on a plane git would not read.
    let dir = repo("");
    let refusing = a_git_that(
        dir.path(),
        "git-that-refuses",
        "echo \"fatal: detected dubious ownership in repository at '$PWD'\" >&2\nexit 128",
    );

    let check = profiles::ignore_check_with(dir.path(), &refusing);

    assert!(!check.passes(), "a refusing git read as no repository");
    assert!(
        check.reason.starts_with(
            "git could not say whether charter.local.toml is ignored (fatal: \
                          detected dubious ownership"
        ),
        "{:?}",
        check.reason
    );
}

#[test]
fn a_status_line_charter_cannot_read_is_an_unknown_and_not_a_tracked_file() {
    // A status code outside the ones that mean "tracked" is git saying something charter has
    // not measured. It refuses either way — but as an UNKNOWN, with git's own line quoted
    // and the fix that goes with it, not as "git tracks charter.local.toml" and a
    // `git rm --cached` that would not help. The `&&` beside the tracked codes survived
    // being turned into `||` (the same run).
    let dir = repo("");
    let odd = a_git_that(
        dir.path(),
        "git-that-says-zz",
        "echo 'ZZ charter.local.toml'",
    );

    let check = profiles::ignore_check_with(dir.path(), &odd);

    assert_eq!(
        check.reason,
        "git could not say whether charter.local.toml is ignored (git status printed \"ZZ \
         charter.local.toml\"), so the profiles in it are refused — an unknown is not a pass. \
         Run git status --ignored -- charter.local.toml in the plane to see what git says."
    );
}

#[test]
fn a_git_that_never_answers_refuses_too_because_a_hang_is_not_a_pass() {
    // The unknown branch was only ever driven by a git that EXITS. A git that hangs is the
    // one that would turn "an unknown is not a pass" into a pass, and it was unexercised —
    // found in review. It waits two seconds rather than the real thirty: at thirty it was half
    // of charter-core's test time, paid again for every mutant the nightly tests. And the git
    // "hangs" for twenty seconds, not for ever: long enough that only a deadline answers
    // first, short enough that a check which stopped keeping its deadline FAILS here inside a
    // mutation run's timeout instead of outliving it as a TIMEOUT.
    let dir = repo("/charter.local.toml\n");
    let hanging = a_git_that(dir.path(), "git-that-hangs", "sleep 20");

    let check =
        profiles::ignore_check_within(dir.path(), &hanging, std::time::Duration::from_secs(2));

    assert!(
        check
            .reason
            .contains("git could not say whether charter.local.toml is ignored"),
        "{:?}",
        check.reason
    );
    assert!(
        check.reason.contains("did not answer within 2 seconds"),
        "{:?}",
        check.reason
    );
}
