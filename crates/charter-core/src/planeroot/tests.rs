//! What A3 and A3b answer on a real repository, and what they answer where the Python raises.
//!
//! The recorded differential (`fixtures/corpora/planeroot-*`) is the arbiter of fidelity; these are
//! the rules stated once each, on a fixture small enough to read, plus the three inputs the
//! differential cannot arbitrate because the oracle raises on them (charter#1178).
//!
//! Every repository here is made through [`crate::testgit`]: the runner keeps `HOME`, and a signer
//! in the operator's global config would otherwise be asked to sign fixture commits.

use super::*;

fn git(dir: &std::path::Path, args: &[&str]) {
    let mut full = vec!["-c", "user.name=t", "-c", "user.email=t@t"];
    full.extend_from_slice(args);
    let r = crate::testgit::run(dir, &full);
    assert!(r.ok(), "git {args:?} failed: {}", r.err);
}

/// A plane root with an upstream, one unpushed commit, a tracked `README`, a branch `feature`,
/// a name that is both a tracked path and a branch (`both`), and repo aliases that move HEAD.
struct Fixture {
    _dir: tempfile::TempDir,
    base: String,
    root: String,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let base = crate::pypath::realpath(dir.path().to_str().unwrap());
    let up = format!("{base}/up.git");
    let root = format!("{base}/plane");
    std::fs::create_dir_all(&root).unwrap();
    let b = std::path::Path::new(&base);
    let r = std::path::Path::new(&root);
    git(b, &["init", "-q", "--bare", "-b", "main", &up]);
    git(r, &["init", "-q", "-b", "main", "."]);
    std::fs::write(r.join("README"), "r\n").unwrap();
    std::fs::write(r.join("both"), "b\n").unwrap();
    git(r, &["add", "README", "both"]);
    git(r, &["commit", "-q", "-m", "one"]);
    git(r, &["branch", "feature"]);
    git(r, &["branch", "both"]);
    git(r, &["remote", "add", "origin", &up]);
    git(r, &["push", "-q", "-u", "origin", "main"]);
    git(r, &["remote", "set-head", "origin", "main"]);
    git(r, &["commit", "-q", "--allow-empty", "-m", "unpushed"]);
    git(r, &["config", "alias.zzco", "checkout"]);
    git(r, &["config", "alias.zzck", "zzco"]);
    git(r, &["config", "alias.zzwipe", "reset --hard"]);
    git(r, &["config", "alias.zzsh", "!sh -c 'echo x'"]);
    git(r, &["config", "alias.zzgco", "!git checkout"]);
    Fixture {
        _dir: dir,
        base,
        root,
    }
}

fn branch(f: &Fixture, cmd: &str) -> Option<String> {
    plane_root_branch_reason(cmd, &f.root, &f.root)
}

fn reset(f: &Fixture, cmd: &str) -> Option<String> {
    plane_root_reset_reason(cmd, &f.root, &f.root)
}

#[test]
fn a_branch_switch_in_the_root_is_refused_and_the_remedy_is_not() {
    let f = fixture();
    let said = branch(&f, "git checkout feature").unwrap();
    assert!(
        said.starts_with("would switch to 'feature' in the PLANE ROOT. "),
        "{said}"
    );
    assert!(said.ends_with(
        "`git checkout main` — putting the root back on its default branch — is always allowed."
    ));
    assert_eq!(branch(&f, "git checkout main"), None);
    assert_eq!(branch(&f, "git switch -q main"), None);
    // A detach that names the default branch is not the remedy.
    assert!(
        branch(&f, "git checkout --detach main")
            .unwrap()
            .starts_with("would detach HEAD at 'main'")
    );
}

#[test]
fn a_file_restore_is_not_a_branch_move_and_an_ambiguous_name_is_refused() {
    let f = fixture();
    assert_eq!(branch(&f, "git checkout README"), None);
    assert_eq!(branch(&f, "git checkout -- anything"), None);
    assert_eq!(branch(&f, "git checkout feature README"), None);
    assert!(
        branch(&f, "git checkout both")
            .unwrap()
            .contains("AMBIGUOUS")
    );
    assert!(
        branch(&f, "git checkout nosuch")
            .unwrap()
            .contains("is not a path this tree tracks")
    );
    // `--orphan README` reads like a restore and creates a branch.
    assert!(
        branch(&f, "git checkout --orphan README")
            .unwrap()
            .starts_with("would create 'README'")
    );
    assert!(
        branch(&f, "git checkout -bREADME")
            .unwrap()
            .starts_with("would create 'README'")
    );
    assert!(
        branch(&f, "git checkout --track README")
            .unwrap()
            .contains("'--track'")
    );
}

#[test]
fn an_alias_is_followed_to_the_command_it_runs() {
    let f = fixture();
    assert!(branch(&f, "git zzco feature").is_some());
    assert!(branch(&f, "git zzck feature").is_some());
    assert!(branch(&f, "git zzgco feature").is_some());
    assert!(branch(&f, "git -c alias.zzin=checkout zzin feature").is_some());
    assert_eq!(branch(&f, "git zzsh feature"), None);
    assert_eq!(branch(&f, "git zznotanalias feature"), None);
}

#[test]
fn only_the_root_is_guarded_however_it_is_reached() {
    let f = fixture();
    let elsewhere = format!("{}/elsewhere", f.base);
    std::fs::create_dir_all(&elsewhere).unwrap();
    assert_eq!(
        plane_root_branch_reason("git checkout feature", &elsewhere, &f.root),
        None
    );
    let via_c = format!("git -C {} checkout feature", f.root);
    assert!(plane_root_branch_reason(&via_c, &elsewhere, &f.root).is_some());
    let via_gd = format!("git --git-dir={}/.git/refs/.. checkout feature", f.root);
    assert!(plane_root_branch_reason(&via_gd, &elsewhere, &f.root).is_some());
    let via_env = format!("export GIT_DIR={}/.git && git checkout feature", f.root);
    assert!(plane_root_branch_reason(&via_env, &elsewhere, &f.root).is_some());
    // #183: the workflow the denial recommends.
    let cd = format!("cd {elsewhere} && git checkout -b x");
    assert_eq!(branch(&f, &cd), None);
}

/// A `cd` that fails leaves the shell where it was (#345). Only `&&` stops the list when it
/// fails, so only a `cd` joined by `&&` may be taken to have moved the commands after it — and
/// only until the `&&` chain ends.
#[test]
fn a_cd_that_fails_leaves_the_later_commands_in_the_root() {
    let f = fixture();
    let nowhere = format!("{}/nowhere", f.base);
    for sep in [";", "||", "&", "\n"] {
        let cmd = format!("cd {nowhere} {sep} git checkout feature");
        assert!(branch(&f, &cmd).is_some(), "{cmd:?}");
        let cmd = format!("cd {nowhere} {sep} git reset --hard origin/main");
        assert!(reset(&f, &cmd).is_some(), "{cmd:?}");
    }
    // `&&` stops the list: the one spelling that moves the later command for certain.
    assert_eq!(
        branch(&f, &format!("cd {nowhere} && git checkout feature")),
        None
    );
    // ...but only as far as the `&&` chain runs: a failed `cd` skips `true` and not what
    // follows the `;` or the `||`.
    for tail in ["; git checkout feature", "|| git checkout feature"] {
        let cmd = format!("cd {nowhere} && true {tail}");
        assert!(branch(&f, &cmd).is_some(), "{cmd:?}");
    }
}

/// Only the SHELL's own `cd` moves the shell: one run in a subshell, a pipeline, or as a
/// program (`env cd`, `/usr/bin/cd`, and `CD`, which a case-insensitive filesystem finds as
/// `/usr/bin/cd`) leaves the next command in the root.
#[test]
fn a_cd_that_does_not_move_the_shell_does_not_move_the_guard() {
    let f = fixture();
    let elsewhere = format!("{}/elsewhere", f.base);
    std::fs::create_dir_all(&elsewhere).unwrap();
    for cmd in [
        format!("(cd {elsewhere} && true); git checkout feature"),
        format!("(cd {elsewhere}) && git checkout feature"),
        format!("true | cd {elsewhere} && git checkout feature"),
        format!("env cd {elsewhere} && git checkout feature"),
        format!("/usr/bin/cd {elsewhere} && git checkout feature"),
        format!("CD {elsewhere} && git checkout feature"),
        format!("! cd {elsewhere} && git checkout feature"),
    ] {
        assert!(branch(&f, &cmd).is_some(), "{cmd:?}");
    }
    // The shell's own `cd`, however it is spelled, still moves it.
    for cmd in [
        format!("builtin cd {elsewhere} && git checkout feature"),
        format!("command cd {elsewhere} && git checkout feature"),
        format!("FOO=1 cd {elsewhere} && git checkout feature"),
        format!("pushd {elsewhere} && git checkout feature"),
    ] {
        assert_eq!(branch(&f, &cmd), None, "{cmd:?}");
    }
}

/// Where a `cd` goes is read the way the shell reads it: `pushd` is a `cd`, `~` is `$HOME`, a
/// `..` after a symlink is taken logically as well as physically, a wrapper's chdir flag moves
/// git, and a destination the guard cannot read (`$VAR`, a glob, `cd -`, `popd`) may be the root.
#[test]
fn a_cd_is_followed_to_where_the_shell_goes() {
    let f = fixture();
    let elsewhere = format!("{}/elsewhere", f.base);
    std::fs::create_dir_all(&elsewhere).unwrap();
    let from_elsewhere = |cmd: &str| plane_root_branch_reason(cmd, &elsewhere, &f.root);
    for cmd in [
        format!("pushd {} && git checkout feature", f.root),
        "cd $PLANE && git checkout feature".to_string(),
        format!("cd {}/pla* && git checkout feature", f.base),
        format!("cd {} && cd - && git checkout feature", f.root),
        "popd && git checkout feature".to_string(),
        format!("env -C {} git checkout feature", f.root),
        format!("sudo --chdir={} git checkout feature", f.root),
    ] {
        assert!(from_elsewhere(&cmd).is_some(), "{cmd:?}");
    }
    // `cd` reads `a/link/..` logically: back in `a`, wherever the link points.
    #[cfg(unix)]
    {
        let link = format!("{}/away", f.root);
        std::os::unix::fs::symlink(&elsewhere, &link).unwrap();
        assert!(from_elsewhere(&format!("cd {link}/.. && git checkout feature")).is_some());
    }
    // `~` is the home directory: reach the root from it through `..`.
    if let Ok(home) = std::env::var("HOME")
        && home.starts_with('/')
    {
        let ups = "../".repeat(home.trim_end_matches('/').matches('/').count());
        let cmd = format!("cd ~/{ups}{} && git checkout feature", &f.root[1..]);
        assert!(from_elsewhere(&cmd).is_some(), "{cmd:?}");
    }
}

/// A `cd` the line itself can redirect is not taken on trust: a `pushd -n` that moves nothing, a
/// `~` or `CDPATH` the line sets, a `cd` the line redefines or disables, and zsh's two-operand
/// `cd old new` all leave the later command possibly in the root.
#[test]
fn a_cd_the_line_can_redirect_is_not_taken_on_trust() {
    let f = fixture();
    let clone = format!("{}/workspaces/w/clone", f.root);
    std::fs::create_dir_all(&clone).unwrap();
    git(
        std::path::Path::new(&clone),
        &["init", "-q", "-b", "main", "."],
    );
    for cmd in [
        "pushd -n workspaces/w/clone && git checkout feature".to_string(),
        format!("HOME={}; cd ~ && git checkout feature", f.root),
        format!("HOME={} cd && git checkout feature", f.root),
        format!(
            "CDPATH={}; cd workspaces/w/clone && git checkout feature",
            f.base
        ),
        "cd(){ :;}; cd workspaces/w/clone && git checkout feature".to_string(),
        "function cd { :; }; cd workspaces/w/clone && git checkout feature".to_string(),
        "enable -n cd; cd workspaces/w/clone && git checkout feature".to_string(),
        format!("cd {0} {0} && git checkout feature", f.base),
    ] {
        assert!(branch(&f, &cmd).is_some(), "{cmd:?}");
    }
    // The workflow the denial recommends still runs.
    assert_eq!(
        branch(&f, "cd workspaces/w/clone && git checkout -b x"),
        None
    );
}

/// git is recognised by what runs, not how it is spelled: on APFS and NTFS `GIT` runs git (#346).
#[test]
fn a_git_spelled_in_capitals_is_git() {
    let f = fixture();
    for cmd in [
        "GIT checkout feature",
        "Git checkout feature",
        "/usr/bin/GIT checkout feature",
    ] {
        assert!(branch(&f, cmd).is_some(), "{cmd}");
    }
    assert!(reset(&f, "GIT reset --hard origin/main").is_some());
    assert!(reset(&f, "Git zzwipe origin/main").is_some());
    // The shell takes the quotes off before it runs the word, and the walk reads the word it runs.
    assert!(branch(&f, "g''it checkout feature").is_some());
    assert!(reset(&f, "g''it reset --hard origin/main").is_some());
    assert!(reset(&f, "G\\IT reset --hard origin/main").is_some());
}

/// An alias defined in one case and used in another is the same alias: git folds the key.
#[test]
fn an_inline_alias_is_found_in_any_case() {
    let f = fixture();
    for cmd in [
        "git -c alias.ZZIN=checkout zzin feature",
        "git -c alias.zzin=checkout ZZIN feature",
        "git -c alias.ZzIn=checkout zZiN feature",
        "git -c 'alias.zzb=!GIT checkout' zzb feature",
    ] {
        assert!(branch(&f, cmd).is_some(), "{cmd}");
    }
}

/// The root is recognised by WHAT it is, not how its path is spelled (#346): a re-cased path on
/// a case-insensitive filesystem and macOS's `/System/Volumes/Data` firmlink both name it, and so
/// does any directory git would discover the root's repository from — a subdirectory of it, or
/// one not made yet — while a repository of its own inside it is not the root.
#[test]
fn the_root_is_recognised_by_what_it_is_not_how_it_is_spelled() {
    let f = fixture();
    let elsewhere = format!("{}/elsewhere", f.base);
    std::fs::create_dir_all(&elsewhere).unwrap();
    let from_elsewhere = |cmd: &str| plane_root_branch_reason(cmd, &elsewhere, &f.root);

    let recased = format!("{}/PLANE", f.base);
    if std::fs::metadata(&recased).is_ok() {
        assert!(from_elsewhere(&format!("git -C {recased} checkout feature")).is_some());
        assert!(from_elsewhere(&format!("cd {recased} && git checkout feature")).is_some());
    }
    let firmlinked = format!("/System/Volumes/Data{}", f.root);
    if std::fs::metadata(&firmlinked).is_ok() {
        assert!(from_elsewhere(&format!("git -C {firmlinked} checkout feature")).is_some());
    }

    std::fs::create_dir_all(format!("{}/docs", f.root)).unwrap();
    for cmd in [
        format!("git -C {}/docs checkout feature", f.root),
        format!("cd {}/docs && git checkout feature", f.root),
        format!(
            "mkdir {0}/new && cd {0}/new && git checkout feature",
            f.root
        ),
        format!("git -C {}/docs reset --hard origin/main", f.root),
    ] {
        let said = if cmd.contains("reset") {
            plane_root_reset_reason(&cmd, &elsewhere, &f.root)
        } else {
            from_elsewhere(&cmd)
        };
        assert!(said.is_some(), "{cmd:?}");
    }

    let clone = format!("{}/workspaces/w/clone", f.root);
    std::fs::create_dir_all(&clone).unwrap();
    git(
        std::path::Path::new(&clone),
        &["init", "-q", "-b", "main", "."],
    );
    assert_eq!(
        branch(&f, "cd workspaces/w/clone && git checkout -b x"),
        None
    );
    assert_eq!(
        from_elsewhere(&format!("git -C {clone} checkout -b x")),
        None
    );
}

#[test]
fn a_clone_whose_config_names_the_root_as_its_work_tree_is_the_root() {
    let f = fixture();
    let clone = format!("{}/ws/clone", f.base);
    std::fs::create_dir_all(format!("{clone}/.git")).unwrap();
    std::fs::write(
        format!("{clone}/.git/config"),
        format!("[core]\n\tworktree = {}\n", f.root),
    )
    .unwrap();
    assert!(plane_root_branch_reason("git checkout feature", &clone, &f.root).is_some());
}

#[test]
fn a_reset_that_destroys_an_unpushed_commit_is_refused_and_one_that_does_not_is_not() {
    let f = fixture();
    let said = reset(&f, "git reset --hard origin/main").unwrap();
    assert!(
        said.starts_with("would delete 1 commit from the PLANE ROOT that is not on origin/main,"),
        "{said}"
    );
    assert!(said.contains(&format!(
        "`git -C {} log --oneline '@{{upstream}}..HEAD'`",
        f.root
    )));
    assert!(reset(&f, "git zzwipe origin/main").is_some());
    assert_eq!(reset(&f, "git reset --soft origin/main"), None);
    assert_eq!(reset(&f, "git reset --hard HEAD"), None);
    assert_eq!(reset(&f, "git reset --hard"), None);
    assert_eq!(reset(&f, "git reset --hard origin/main -- README"), None);
    assert_eq!(reset(&f, "echo reset --hard origin/main"), None);
}

#[test]
fn the_default_branch_is_the_remotes_answer_first() {
    let f = fixture();
    assert_eq!(default_branch(&f.root).as_deref(), Some("main"));
    let bare = format!("{}/bare", f.base);
    std::fs::create_dir_all(&bare).unwrap();
    git(
        std::path::Path::new(&bare),
        &["init", "-q", "-b", "dev", "."],
    );
    assert_eq!(default_branch(&bare), None);
}

#[test]
fn the_subject_list_only_grows_and_resolves_against_the_shell_directory() {
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    let got = git_target(
        "/nowhere/a",
        &s(&["-C", "..", "-C", "b", "--work-tree", "w"]),
        &[],
    );
    assert_eq!(got, ["/nowhere/a/../b", "/nowhere/a/../b/w"]);
    let got = git_target("/nowhere", &s(&["--git-dir=g"]), &s(&["GIT_WORK_TREE=/t"]));
    assert_eq!(got, ["/nowhere", "/t", "/nowhere/g", "/nowhere/g/.."]);
    assert_eq!(git_target("", &[], &[]), ["."]);
}

#[test]
fn an_option_is_placed_by_its_name_not_its_spelling() {
    assert_eq!(checkout_opt_kind("-bREADME"), OptKind::Create);
    assert_eq!(checkout_opt_kind("-qbREADME"), OptKind::Create);
    assert_eq!(checkout_opt_kind("-fq"), OptKind::Restore);
    assert_eq!(checkout_opt_kind("-dq"), OptKind::Detach);
    assert_eq!(checkout_opt_kind("--orphan=x"), OptKind::Create);
    assert_eq!(checkout_opt_kind("--no-orphan"), OptKind::Create);
    assert_eq!(checkout_opt_kind("--no-quiet"), OptKind::Restore);
    assert_eq!(checkout_opt_kind("--track"), OptKind::Unknown);
    assert_eq!(checkout_opt_kind("-t"), OptKind::Unknown);
}

// ---- where the Python RAISES (charter#1178), pinned here because the differential cannot
// arbitrate an input one side has no answer for.

/// A symlink loop among the subjects: CPython 3.11/3.12 raises `RuntimeError` out of the walk,
/// which is an ALLOW for the whole line. This resolves it as 3.14 does — not the root — and
/// goes on to the later segment, which it refuses.
#[cfg(unix)]
#[test]
fn a_symlink_loop_among_the_subjects_does_not_stop_the_walk() {
    let f = fixture();
    let lp = format!("{}/loop", f.base);
    std::os::unix::fs::symlink(&lp, &lp).unwrap();
    let cmd = format!("git -C {lp} status; git checkout feature");
    assert!(branch(&f, &cmd).is_some());
    let cmd = format!("git --work-tree={lp} status; git reset --hard origin/main");
    assert!(reset(&f, &cmd).is_some());
}

/// A NUL in `-C`, `--work-tree` or `--git-dir`: the Python raises `ValueError`. Here it is a
/// path that is not the root, and the later segment is still judged.
#[test]
fn a_nul_in_a_subject_does_not_stop_the_walk() {
    let f = fixture();
    for opt in ["-C a\u{0}b", "--work-tree=a\u{0}b", "--git-dir=a\u{0}b"] {
        let cmd = format!("git {opt} status; git checkout feature");
        assert!(branch(&f, &cmd).is_some(), "{opt}");
        let cmd = format!("git {opt} status; git reset --hard origin/main");
        assert!(reset(&f, &cmd).is_some(), "{opt}");
    }
}

/// An operand holding a NUL cannot be put to git: `Unknown`, which keeps the refusal.
#[test]
fn an_operand_git_cannot_be_asked_about_keeps_the_refusal() {
    let f = fixture();
    assert_eq!(
        checkout_operand_kind(&f.root, "a\u{0}b"),
        OperandKind::Unknown
    );
    assert!(
        branch(&f, "git checkout 'a\u{0}b'")
            .unwrap()
            .contains("could not ask git")
    );
}

/// An alias body that is not UTF-8: the Python's strict decode raises `ValueError`, which
/// `_resolve_git_alias` answers by standing aside — so the lossy reading that WOULD resolve it
/// to `checkout` must not happen here either, or the two part company.
#[test]
fn an_alias_body_that_is_not_utf8_is_not_read() {
    let f = fixture();
    let config = format!("{}/.git/config", f.root);
    let mut text = std::fs::read(&config).unwrap();
    text.extend_from_slice(b"[alias]\n\tzznu = checkout \xff\n");
    std::fs::write(&config, text).unwrap();
    assert_eq!(resolve_git_alias(&f.root, "zznu", &[], &[]).0, "zznu");
    assert_eq!(branch(&f, "git zznu feature"), None);
}

#[test]
fn python_int_reads_what_rev_list_writes_and_nothing_else() {
    assert_eq!(py_int("12"), Some(12));
    assert_eq!(py_int("1_0"), Some(10));
    assert_eq!(py_int("-3"), Some(-3));
    assert_eq!(py_int("_1"), None);
    assert_eq!(py_int("1__0"), None);
    assert_eq!(py_int("x"), None);
    assert_eq!(py_int(""), None);
}
