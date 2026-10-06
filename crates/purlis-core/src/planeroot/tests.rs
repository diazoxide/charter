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

/// `git` written with ANSI-C escapes, as a locale string, or split by a backslash-newline is
/// the `git` the shell runs, and the reset guard's `git` filter lets each of them through to the
/// walk that decides.
#[test]
fn a_git_the_shell_spells_out_of_quoting_is_git() {
    let f = fixture();
    for cmd in [
        "$'\\x67it' checkout feature",
        "$'\\147\\151\\164' checkout feature",
        "$\"git\" checkout feature",
        "g\\\nit checkout feature",
    ] {
        assert!(branch(&f, cmd).is_some(), "{cmd:?}");
    }
    assert!(reset(&f, "$'\\x67it' reset --hard origin/main").is_some());
    assert!(reset(&f, "$\\\n'\\x67it' reset --hard origin/main").is_some());
    // A `!` alias is handed to `sh -c`, which decodes the same quoting.
    assert!(branch(&f, "git -c \"alias.zzq=!$'\\x67it' checkout\" zzq feature").is_some());
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

/// A plane root, a session directory inside it, and a scratch clone of the root OUTSIDE it — the
/// layout of #1323. With `linked`, each repository keeps its git directory beside it and has a
/// `.git` FILE naming it (`git clone --separate-git-dir`), the layout a scratch clone often has.
struct Scratch {
    _dir: tempfile::TempDir,
    root: String,
    session: String,
    clone: String,
}

fn scratch(linked: bool) -> Scratch {
    scratch_with(linked, linked)
}

/// [`scratch`] with the root's and the clone's layouts chosen apart.
fn scratch_with(root_linked: bool, clone_linked: bool) -> Scratch {
    let dir = tempfile::tempdir().unwrap();
    let base = crate::pypath::realpath(dir.path().to_str().unwrap());
    let root = format!("{base}/plane");
    let clone = format!("{base}/scratch/clone");
    let session = format!("{root}/workspaces/w");
    std::fs::create_dir_all(format!("{base}/scratch")).unwrap();
    let b = std::path::Path::new(&base);
    let r = std::path::Path::new(&root);
    let beside = |repo: &str| format!("--separate-git-dir={repo}.gitdir");
    let mut init = vec!["init", "-q", "-b", "main"];
    let root_gd = beside(&root);
    if root_linked {
        init.push(&root_gd);
    }
    init.push(&root);
    git(b, &init);
    std::fs::write(r.join("README"), "r\n").unwrap();
    git(r, &["add", "README"]);
    git(r, &["commit", "-q", "-m", "one"]);
    git(r, &["branch", "feature"]);
    std::fs::create_dir_all(&session).unwrap();
    let mut cl = vec!["clone", "-q"];
    let clone_gd = beside(&clone);
    if clone_linked {
        cl.push(&clone_gd);
    }
    cl.extend([root.as_str(), clone.as_str()]);
    git(b, &cl);
    Scratch {
        _dir: dir,
        root,
        session,
        clone,
    }
}

/// `~/` followed by the climb from `$HOME` to `/` and then `abs` — the path `abs` spelled from
/// the home directory — or `None` where the test's `HOME` is not absolute.
fn from_home(abs: &str) -> Option<String> {
    let home = std::env::var("HOME").ok().filter(|h| h.starts_with('/'))?;
    let ups = "../".repeat(home.trim_end_matches('/').matches('/').count());
    Some(format!("~/{ups}{}", &abs[1..]))
}

/// #1323: the guard follows a command to the repository it really acts on. `git -C <clone>` and
/// a `cd <clone> &&` earlier in the line act in the clone, from a session standing in the root;
/// the same two spellings aimed at the root are refused from a session standing in the clone.
fn a_command_aimed_at_another_clone_acts_there(linked: bool) {
    let s = scratch(linked);
    let (root, clone) = (&s.root, &s.clone);
    for cmd in [
        format!("git -C {clone} switch -c x"),
        format!("git -C {clone} checkout -b x"),
        format!("git -C {clone} checkout --detach"),
        format!("cd {clone} && git checkout -b x"),
        format!("cd {clone} && git switch -c x"),
        format!("cd {clone} && git fetch -q && git checkout -q -b x 2>&1 | tail -3"),
    ] {
        assert_eq!(
            plane_root_branch_reason(&cmd, &s.session, root),
            None,
            "{cmd:?}"
        );
    }
    for cmd in [
        format!("git -C {root} switch -c x"),
        format!("cd {root} && git checkout -b x"),
        format!("git -C {clone} status && git -C {root} checkout -b x"),
        format!("cd {root}/workspaces/w && git checkout -b x"),
    ] {
        assert!(
            plane_root_branch_reason(&cmd, clone, root).is_some(),
            "{cmd:?}"
        );
    }
}

#[test]
fn a_command_aimed_at_another_clone_acts_there_with_a_git_directory() {
    a_command_aimed_at_another_clone_acts_there(false);
}

#[test]
fn a_command_aimed_at_another_clone_acts_there_with_a_git_file() {
    a_command_aimed_at_another_clone_acts_there(true);
}

/// A `~` in a directory git is pointed at is read the way the shell reads it, as a `cd`'s is:
/// `$HOME`, and the literal name as well, since a quoted `~` stays one.
fn a_home_relative_directory_is_where_the_shell_sends_git(linked: bool) {
    let s = scratch(linked);
    let (Some(to_clone), Some(to_root)) = (from_home(&s.clone), from_home(&s.root)) else {
        return;
    };
    for cmd in [
        format!("git -C {to_clone} switch -c x"),
        format!("cd {to_clone} && git checkout -b x"),
    ] {
        assert_eq!(
            plane_root_branch_reason(&cmd, &s.session, &s.root),
            None,
            "{cmd:?}"
        );
    }
    for cmd in [
        format!("git -C {to_root} switch -c x"),
        format!("GIT_WORK_TREE={to_root} git checkout -b x"),
        format!("git --git-dir {to_root}/.git checkout -b x"),
        format!("env -C {to_root} git checkout -b x"),
        format!("HOME=/elsewhere git -C {to_clone} switch -c x"),
    ] {
        assert!(
            plane_root_branch_reason(&cmd, &s.clone, &s.root).is_some(),
            "{cmd:?}"
        );
    }
}

#[test]
fn a_home_relative_directory_is_where_the_shell_sends_git_with_a_git_directory() {
    a_home_relative_directory_is_where_the_shell_sends_git(false);
}

#[test]
fn a_home_relative_directory_is_where_the_shell_sends_git_with_a_git_file() {
    a_home_relative_directory_is_where_the_shell_sends_git(true);
}

/// A directory git is pointed at that the guard cannot name — the shell expands it later — may
/// be the root, so it is refused from anywhere, a clone of the guard's own included. Fail
/// closed: the guard cannot tell, so it answers as if it were the root.
fn a_directory_the_guard_cannot_name_may_be_the_root(linked: bool) {
    let s = scratch(linked);
    for cmd in [
        "git -C \"$PLANE\" switch -c x".to_string(),
        "git -C $PLANE/ checkout -b x".to_string(),
        "git -C ~operator/plane checkout -b x".to_string(),
        format!("git -C {}/pla* checkout -b x", &s.root[..s.root.len() - 6]),
        "git -C {a,b} checkout -b x".to_string(),
        "git --git-dir=$P/.git checkout -b x".to_string(),
        "git --work-tree \"$P\" checkout -b x".to_string(),
        "GIT_DIR=$P/.git git checkout -b x".to_string(),
        "export GIT_WORK_TREE=$P && git checkout -b x".to_string(),
        "env -C \"$P\" git checkout -b x".to_string(),
    ] {
        assert!(
            plane_root_branch_reason(&cmd, &s.clone, &s.root).is_some(),
            "{cmd:?}"
        );
    }
    // What it cannot name only matters where it names a directory.
    assert_eq!(
        plane_root_branch_reason("git -c x.y=$V switch -c x", &s.clone, &s.root),
        None
    );
}

#[test]
fn a_directory_the_guard_cannot_name_may_be_the_root_with_a_git_directory() {
    a_directory_the_guard_cannot_name_may_be_the_root(false);
}

#[test]
fn a_directory_the_guard_cannot_name_may_be_the_root_with_a_git_file() {
    a_directory_the_guard_cannot_name_may_be_the_root(true);
}

/// A `~` the shell leaves alone names a directory called `~`: quoted (`'~'`, `\~`), after an
/// assignment's `=` under `env` (zsh), and in an attached wrapper flag. The reader takes the
/// quoting off, so such a `~` arrives looking like `$HOME`. From a clone holding a `~` that
/// links to the root, each of these acts on the root and is refused (#1323).
#[cfg(unix)]
fn a_tilde_the_shell_leaves_alone_is_a_directory_named_tilde(linked: bool) {
    let s = scratch(linked);
    std::os::unix::fs::symlink(&s.root, format!("{}/~", s.clone)).unwrap();
    for cmd in [
        "git -C '~' checkout -b x",
        "git -C \\~ switch -c x",
        "env GIT_DIR=~/.git git checkout -b x",
        "env --chdir=~ git checkout -b x",
        "env -C '~' git checkout -b x",
        "sudo -D~ git checkout -b x",
    ] {
        assert!(
            plane_root_branch_reason(cmd, &s.clone, &s.root).is_some(),
            "{cmd:?}"
        );
    }
}

#[cfg(unix)]
#[test]
fn a_tilde_the_shell_leaves_alone_is_a_directory_named_tilde_with_a_git_directory() {
    a_tilde_the_shell_leaves_alone_is_a_directory_named_tilde(false);
}

#[cfg(unix)]
#[test]
fn a_tilde_the_shell_leaves_alone_is_a_directory_named_tilde_with_a_git_file() {
    a_tilde_the_shell_leaves_alone_is_a_directory_named_tilde(true);
}

/// A root whose git directory lives beside it (a `.git` file) is reached by naming that
/// directory: `--git-dir=<it>` moves the root's HEAD from anywhere.
#[test]
fn the_roots_own_git_directory_is_the_root_wherever_it_lives() {
    let s = scratch(true);
    let gd = format!("{}.gitdir", s.root);
    for cmd in [
        format!("git --git-dir={gd} checkout -b x"),
        format!("git --git-dir {gd} --work-tree {} switch -c x", s.clone),
        format!("GIT_DIR={gd} git checkout -b x"),
    ] {
        assert!(
            plane_root_branch_reason(&cmd, &s.clone, &s.root).is_some(),
            "{cmd:?}"
        );
    }
}

/// A command refused only because a directory it is sent to cannot be named says so, rather
/// than claiming what it would do in the root; one that reaches the root by name keeps its own
/// sentence.
fn a_directory_the_guard_cannot_name_is_refused_as_unnamed(linked: bool) {
    let s = scratch(linked);
    for cmd in [
        "git -C \"$WT\" switch -c x",
        "cd \"$X\" && git checkout -b x",
        "cd - && git checkout feature",
    ] {
        let said = plane_root_branch_reason(cmd, &s.clone, &s.root).expect(cmd);
        assert!(
            said.starts_with("cannot tell which repository this `git "),
            "{cmd:?}: {said}"
        );
        assert!(said.contains("Spell the path out"), "{said}");
    }
    let said =
        plane_root_branch_reason(&format!("git -C {} switch -c x", s.root), &s.clone, &s.root)
            .unwrap();
    assert!(
        said.starts_with("would create 'x' in the PLANE ROOT"),
        "{said}"
    );
}

#[test]
fn a_directory_the_guard_cannot_name_is_refused_as_unnamed_with_a_git_directory() {
    a_directory_the_guard_cannot_name_is_refused_as_unnamed(false);
}

#[test]
fn a_directory_the_guard_cannot_name_is_refused_as_unnamed_with_a_git_file() {
    a_directory_the_guard_cannot_name_is_refused_as_unnamed(true);
}

/// The layout the live refusals came from: the root has a `.git` directory, and the scratch clone
/// outside it has a `.git` FILE naming a git directory outside both. Pointed at by name, the clone
/// is the clone; pointed at through a variable, it is refused as a directory the guard cannot
/// name, and the denial asks for the path spelled out.
fn a_clone_whose_git_is_a_file_elsewhere(root_linked: bool) {
    let s = scratch_with(root_linked, true);
    let clone = &s.clone;
    for cmd in [
        format!("git -C {clone} switch -c x"),
        format!("git -C {clone} checkout -b x"),
        format!("cd {clone} && git switch -c x"),
    ] {
        assert_eq!(
            plane_root_branch_reason(&cmd, &s.session, &s.root),
            None,
            "{cmd:?}"
        );
    }
    let said = plane_root_branch_reason("git -C \"$C\" switch -c x", &s.session, &s.root).unwrap();
    assert!(said.contains("Spell the path out"), "{said}");
}

#[test]
fn a_clone_whose_git_is_a_file_elsewhere_beside_a_root_with_a_git_directory() {
    a_clone_whose_git_is_a_file_elsewhere(false);
}

#[test]
fn a_clone_whose_git_is_a_file_elsewhere_beside_a_root_with_a_git_file() {
    a_clone_whose_git_is_a_file_elsewhere(true);
}

/// Whether a separated `~` is a directory named `~` is read from the line's quoting, never from
/// the filesystem before the line runs: a `~` the same line makes, quoted or escaped, is the
/// directory git is sent to, and from inside the root that directory is in the root.
fn a_quoted_tilde_the_same_line_makes_is_read_literally(linked: bool) {
    let s = scratch(linked);
    for cmd in [
        "mkdir \\~ && git -C \\~ switch -c x",
        "mkdir -p '~/a' && git -C '~/a' switch -c x",
        "git -C \"~/clone\" checkout -b x",
        "git -C ~'/a' checkout -b x",
    ] {
        assert!(
            plane_root_branch_reason(cmd, &s.session, &s.root).is_some(),
            "{cmd:?}"
        );
    }
}

#[test]
fn a_quoted_tilde_the_same_line_makes_is_read_literally_with_a_git_directory() {
    a_quoted_tilde_the_same_line_makes_is_read_literally(false);
}

#[test]
fn a_quoted_tilde_the_same_line_makes_is_read_literally_with_a_git_file() {
    a_quoted_tilde_the_same_line_makes_is_read_literally(true);
}

/// Reading a long run of `-C`s costs what the run is long, not twice as much per `-C`: the hook
/// answers on a deadline, and a guard that timed out would let the command through.
#[test]
fn a_long_run_of_directory_options_is_read_in_time() {
    let s = scratch(true);
    let cmd = format!("git -C '~' {}-C {} switch -c x", "-C . ".repeat(64), s.root);
    let started = std::time::Instant::now();
    assert!(plane_root_branch_reason(&cmd, &s.clone, &s.root).is_some());
    let took = started.elapsed();
    assert!(took < std::time::Duration::from_secs(2), "{took:?}");
}

/// ANSI-C quoting spells a `~` without one (`$'\x7e'`, `$'\176'`), and a `cd` to a quoted `~`
/// goes to the directory named `~` as well as `$HOME`. From a clone holding a `~` that links to
/// the root, and from inside the root after the line makes one, each of these is refused.
#[cfg(unix)]
fn a_tilde_spelled_without_one_or_reached_by_cd_is_read_literally(linked: bool) {
    let s = scratch(linked);
    std::os::unix::fs::symlink(&s.root, format!("{}/~", s.clone)).unwrap();
    for cmd in [
        "git -C $'\\x7e' checkout -b x",
        "git -C $'\\176' switch -c x",
        "cd \\~ && git switch -c x",
        "cd $'\\x7e' && git checkout -b x",
    ] {
        assert!(
            plane_root_branch_reason(cmd, &s.clone, &s.root).is_some(),
            "{cmd:?}"
        );
    }
    for cmd in [
        "mkdir $'\\x7e' && git -C $'\\x7e' switch -c x",
        "mkdir \\~ && cd \\~ && git switch -c x",
        "mkdir $'\\x7e' && cd $'\\x7e' && git switch -c x",
        "mkdir '~' && cd '~' && git switch -c x",
    ] {
        assert!(
            plane_root_branch_reason(cmd, &s.session, &s.root).is_some(),
            "{cmd:?}"
        );
    }
}

#[cfg(unix)]
#[test]
fn a_tilde_spelled_without_one_or_reached_by_cd_is_read_literally_with_a_git_directory() {
    a_tilde_spelled_without_one_or_reached_by_cd_is_read_literally(false);
}

#[cfg(unix)]
#[test]
fn a_tilde_spelled_without_one_or_reached_by_cd_is_read_literally_with_a_git_file() {
    a_tilde_spelled_without_one_or_reached_by_cd_is_read_literally(true);
}

/// An unquoted command substitution among git's own options — `$(…)` or backticks — names a
/// directory only the shell knows, and the reader spreads its words through git's argv (#1354).
/// From a clone, a branch move or a reset after one is refused as unnamed: neither the directory
/// nor the options after it can be read, and the words do not move the subcommand out of sight.
fn a_substitution_among_gits_options_is_unnamed(linked: bool) {
    let s = scratch(linked);
    let root = &s.root;
    let parent = &root[..root.rfind('/').unwrap()];
    let r = std::path::Path::new(root);
    // An upstream the root is a commit ahead of, so a reset there would lose one.
    git(
        std::path::Path::new(parent),
        &["init", "-q", "--bare", "-b", "main", "up.git"],
    );
    git(r, &["remote", "add", "origin", &format!("{parent}/up.git")]);
    git(r, &["push", "-q", "-u", "origin", "main"]);
    git(r, &["commit", "-q", "--allow-empty", "-m", "unpushed"]);
    let unnamed = "cannot tell which repository this `git ";
    for (open, close) in [("$(", ")"), ("`", "`")] {
        let sub = |inner: &str| format!("{open}{inner}{close}");
        let echo = sub(&format!("echo {root}"));
        for cmd in [
            format!("git -C {echo} checkout -b x"),
            format!("git -C {echo}/ switch -c x"),
            format!(
                "git -C {} checkout -b x",
                sub("git rev-parse --show-toplevel")
            ),
            format!("git --git-dir={echo}/.git checkout -b x"),
            format!("git --git-dir {echo}/.git switch -c x"),
            format!("git --work-tree={echo} checkout -b x"),
            format!("git --work-tree {echo} checkout --detach"),
            format!("git -c x.y={} -C {root} checkout -b x", sub("echo 1")),
            format!("GIT_DIR={echo}/.git git checkout -b x"),
            format!("env -C {echo} git switch -c x"),
        ] {
            let said = plane_root_branch_reason(&cmd, &s.clone, root).expect(&cmd);
            assert!(said.starts_with(unnamed), "{cmd:?}: {said}");
        }
        for cmd in [
            format!("git -C {echo} reset --hard HEAD~1"),
            format!("git --git-dir={echo}/.git reset --hard HEAD~1"),
            format!("git --work-tree {echo} reset --hard HEAD~1"),
        ] {
            let said = plane_root_reset_reason(&cmd, &s.clone, root).expect(&cmd);
            assert!(said.starts_with(unnamed), "{cmd:?}: {said}");
        }
        // A substitution after the subcommand is an argument, and the clone stays the clone.
        let after = format!(
            "git -C {} checkout -b x {}",
            s.clone,
            sub("git rev-parse HEAD")
        );
        assert_eq!(plane_root_branch_reason(&after, &s.session, root), None);
    }
}

#[test]
fn a_substitution_among_gits_options_is_unnamed_with_a_git_directory() {
    a_substitution_among_gits_options_is_unnamed(false);
}

#[test]
fn a_substitution_among_gits_options_is_unnamed_with_a_git_file() {
    a_substitution_among_gits_options_is_unnamed(true);
}

/// An alias after a substitution among git's options is followed as one in its place would be:
/// inline (`-c alias.co=checkout`, before or after the substitution) or in the root's config.
/// And a program only the shell can name (`$G`, `$(which git)`) may be git: a branch move or
/// reset after it is refused as unnamed (#1354).
fn an_alias_or_a_variable_git_after_a_substitution_is_followed(linked: bool) {
    let s = scratch(linked);
    let root = &s.root;
    git(
        std::path::Path::new(root),
        &["config", "alias.zzsw", "switch"],
    );
    let unnamed = "cannot tell which repository this `git ";
    for (open, close) in [("$(", ")"), ("`", "`")] {
        let echo = format!("{open}echo {root}{close}");
        for cmd in [
            format!("git -c alias.co=checkout -C {echo} co -b x"),
            format!("git -C {echo} -c alias.co=checkout co -b x"),
            format!("git -C {echo} zzsw -c x"),
            format!("git --git-dir={echo}/.git zzsw -c x"),
            format!("{open}which git{close} -C {root} switch -c x"),
        ] {
            let said = plane_root_branch_reason(&cmd, &s.clone, root).expect(&cmd);
            assert!(said.starts_with(unnamed), "{cmd:?}: {said}");
        }
        // An alias that resolves to no branch move stays allowed.
        let st = format!("git -c alias.st=status -C {echo} st");
        assert_eq!(plane_root_branch_reason(&st, &s.clone, root), None);
    }
    for cmd in [
        "G=git; $G checkout -b x",
        "$GIT switch -c x",
        "\"$G\" checkout --detach",
    ] {
        let said = plane_root_branch_reason(cmd, &s.session, root).expect(cmd);
        assert!(said.starts_with(unnamed), "{cmd:?}: {said}");
    }
    assert_eq!(
        plane_root_branch_reason("G=git; $G status", &s.session, root),
        None
    );
}

#[test]
fn an_alias_or_a_variable_git_after_a_substitution_is_followed_with_a_git_directory() {
    an_alias_or_a_variable_git_after_a_substitution_is_followed(false);
}

#[test]
fn an_alias_or_a_variable_git_after_a_substitution_is_followed_with_a_git_file() {
    an_alias_or_a_variable_git_after_a_substitution_is_followed(true);
}

/// Following aliases after a substitution costs one git question per command line, however
/// many segments and alias words it holds: the hook answers on a deadline (#1354).
#[test]
fn aliases_after_substitutions_are_read_once_per_line() {
    let s = scratch(true);
    let root = &s.root;
    git(
        std::path::Path::new(root),
        &["config", "alias.st", "status"],
    );
    let one = format!("git -C $(echo {root}) {}", "st ".repeat(40));
    let cmd = vec![one; 200].join("; ");
    let started = std::time::Instant::now();
    assert_eq!(plane_root_branch_reason(&cmd, &s.clone, root), None);
    let took = started.elapsed();
    assert!(took < std::time::Duration::from_secs(3), "{took:?}");
    // Past the budget of different aliases to follow, what git runs is unread, and refused.
    let mut words = String::new();
    for n in 0..=MAX_CHECKOUT_OPERANDS {
        let name = format!("alias.st{n}");
        git(std::path::Path::new(root), &["config", &name, "status"]);
        words.push_str(&format!("st{n} "));
    }
    let many = format!("git -C $(echo {root}) {words}");
    let said = plane_root_branch_reason(&many, &s.clone, root).unwrap();
    assert!(
        said.starts_with("cannot tell what this `git` command does"),
        "{said}"
    );
}

/// Where git cannot be asked which words are aliases — here, an alias body that is not UTF-8 —
/// a branch move hidden behind a substitution is not taken to be absent: it is refused (#1354,
/// as an operand git cannot be asked about is, #438).
fn an_alias_book_git_cannot_read_keeps_the_refusal(linked: bool) {
    use std::io::Write;
    let s = scratch(linked);
    let config = if linked {
        format!("{}.gitdir/config", s.root)
    } else {
        format!("{}/.git/config", s.root)
    };
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(config)
        .unwrap();
    f.write_all(b"[alias]\n\tzzbad = checkout \xff\n").unwrap();
    let cmd = format!("git -C $(echo {}) zzco -b x", s.root);
    let said = plane_root_branch_reason(&cmd, &s.clone, &s.root).unwrap();
    assert!(
        said.starts_with("cannot tell what this `git` command does"),
        "{said}"
    );
}

#[test]
fn an_alias_book_git_cannot_read_keeps_the_refusal_with_a_git_directory() {
    an_alias_book_git_cannot_read_keeps_the_refusal(false);
}

#[test]
fn an_alias_book_git_cannot_read_keeps_the_refusal_with_a_git_file() {
    an_alias_book_git_cannot_read_keeps_the_refusal(true);
}

/// An alias redefined on the line after a word that named it is a new alias: what the word
/// after the redefinition runs is followed again, not answered from the first (#1354).
#[test]
fn an_alias_redefined_after_a_substitution_is_followed_again() {
    let s = scratch(true);
    let cmd = format!(
        "git -c alias.zz=status -C $(echo {}; : zz) -c alias.zz=checkout zz -b x",
        s.root
    );
    assert!(plane_root_branch_reason(&cmd, &s.clone, &s.root).is_some());
}

/// Thousands of inline alias definitions after a substitution are read in one pass, not once
/// per word: the hook answers on a deadline (#1354).
#[test]
fn many_inline_aliases_after_a_substitution_are_read_in_time() {
    let s = scratch(true);
    let cmd = format!(
        "git -C $(echo {}) {}",
        s.root,
        "-c alias.zz=status zz ".repeat(6000)
    );
    let started = std::time::Instant::now();
    assert_eq!(plane_root_branch_reason(&cmd, &s.clone, &s.root), None);
    let took = started.elapsed();
    assert!(took < std::time::Duration::from_secs(3), "{took:?}");
}

/// A chain of aliases longer than the guard follows is refused, not taken to end where the
/// guard stopped looking: written out, and after a substitution (#1354).
fn an_alias_chain_past_the_hop_limit_is_refused(linked: bool) {
    let s = scratch(linked);
    let r = std::path::Path::new(&s.root);
    for (name, body) in [
        ("e1", "e2"),
        ("e2", "e3"),
        ("e3", "e4"),
        ("e4", "e5"),
        ("e5", "checkout"),
    ] {
        git(r, &["config", &format!("alias.{name}"), body]);
    }
    let said =
        plane_root_branch_reason(&format!("git -C {} e1 -b x", s.root), &s.clone, &s.root).unwrap();
    assert!(said.starts_with("cannot tell what `git e1` does"), "{said}");
    let said = plane_root_branch_reason(
        &format!("git -C $(echo {}) e1 -b x", s.root),
        &s.clone,
        &s.root,
    )
    .unwrap();
    assert!(
        said.starts_with("cannot tell what this `git` command does"),
        "{said}"
    );
    // Four hops still reach the end.
    let said =
        plane_root_branch_reason(&format!("git -C {} e2 -b x", s.root), &s.clone, &s.root).unwrap();
    assert!(said.starts_with("would create 'x'"), "{said}");
}

#[test]
fn an_alias_chain_past_the_hop_limit_is_refused_with_a_git_directory() {
    an_alias_chain_past_the_hop_limit_is_refused(false);
}

#[test]
fn an_alias_chain_past_the_hop_limit_is_refused_with_a_git_file() {
    an_alias_chain_past_the_hop_limit_is_refused(true);
}
