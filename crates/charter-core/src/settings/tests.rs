use std::fs;
use std::path::Path;

use super::*;

/// A scratch plane: a `charter.toml`, and a git repository whose `.gitignore` carries the
/// line `charter init` writes, so the local file is ignored the way a real plane's is.
fn plane(shared: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("charter.toml"), shared).unwrap();
    crate::testgit::run(dir.path(), &["init", "-q"]);
    fs::write(dir.path().join(".gitignore"), "/charter.local.toml\n").unwrap();
    dir
}

fn text(root: &Path, file: &str) -> String {
    fs::read_to_string(root.join(file)).unwrap()
}

fn set(path: &[&str], value: Value) -> Edit {
    Edit {
        path: path.iter().map(|s| Step::Key((*s).to_owned())).collect(),
        value: Some(value),
    }
}

const COMMENTED: &str = "\
# The plane's own settings. Hand-edited; keep the comments.
schema = 1

[[forge]]
kind  = \"github\"   # where the repos live
owner = \"acme\"

# How far a memory travels.
[memory]
share = \"local\"  # local | commit | push

[workspace]
default = \"default\"
";

#[test]
fn changing_one_key_of_a_commented_file_changes_that_line_and_nothing_else() {
    let dir = plane(COMMENTED);
    let now = read(dir.path(), Which::Shared).unwrap();
    let after = edited(
        &now.text,
        &[set(&["memory", "share"], Value::Text("commit".into()))],
    )
    .unwrap();
    save(dir.path(), Which::Shared, Some(&now.text), &after).unwrap();

    let written = text(dir.path(), "charter.toml");
    let before: Vec<&str> = COMMENTED.lines().collect();
    let lines: Vec<&str> = written.lines().collect();
    assert_eq!(before.len(), lines.len(), "{written}");
    let changed: Vec<(&str, &str)> = before
        .iter()
        .zip(&lines)
        .filter(|(a, b)| a != b)
        .map(|(a, b)| (*a, *b))
        .collect();
    assert_eq!(
        changed,
        [(
            "share = \"local\"  # local | commit | push",
            "share = \"commit\"  # local | commit | push"
        )]
    );
}

#[test]
fn a_key_in_a_section_the_file_lacks_is_added_with_its_section_at_the_end() {
    let after = edited(
        COMMENTED,
        &[set(&["update", "channel"], Value::Text("dev".into()))],
    )
    .unwrap();
    assert!(after.starts_with(COMMENTED), "{after}");
    assert!(after.ends_with("[update]\nchannel = \"dev\"\n"), "{after}");
}

#[test]
fn a_forge_block_is_edited_by_its_place_in_the_file() {
    let after = edited(
        COMMENTED,
        &[Edit {
            path: vec![
                Step::Key("forge".into()),
                Step::Index(0),
                Step::Key("exclude".into()),
            ],
            value: Some(Value::List(vec!["old".into(), "archive".into()])),
        }],
    )
    .unwrap();
    assert!(
        after.contains("owner = \"acme\"\nexclude = [\"old\", \"archive\"]\n"),
        "{after}"
    );
}

#[test]
fn removing_a_key_takes_its_line_and_leaves_the_rest() {
    let after = edited(
        COMMENTED,
        &[Edit {
            path: vec![Step::Key("workspace".into()), Step::Key("default".into())],
            value: None,
        }],
    )
    .unwrap();
    assert!(!after.contains("default = \"default\""), "{after}");
    assert!(after.contains("# How far a memory travels."), "{after}");
}

#[test]
fn an_edit_through_a_key_that_is_not_a_table_is_refused_rather_than_guessed() {
    let err = edited(
        "memory = \"local\"\n",
        &[set(&["memory", "share"], Value::Text("commit".into()))],
    )
    .unwrap_err();
    assert!(err.contains("memory"), "{err}");
}

#[test]
fn reading_a_plane_says_which_file_and_whether_local_exists_yet() {
    let dir = plane(COMMENTED);
    let shared = read(dir.path(), Which::Shared).unwrap();
    assert_eq!(shared.file, "charter.toml");
    assert!(shared.exists);
    assert_eq!(shared.text, COMMENTED);
    let local = read(dir.path(), Which::Local).unwrap();
    assert_eq!(local.file, "charter.local.toml");
    assert!(!local.exists);
    assert_eq!(local.text, "");
}

#[test]
fn a_shared_file_that_is_not_toml_is_refused_in_the_words_charter_reads_it_with() {
    let dir = plane(COMMENTED);
    let bad = "[memory\nshare = 1\n";
    let why = refusals(dir.path(), Which::Shared, bad);
    let doctor = crate::doctor::Config::parse(&dir.path().join("charter.toml"), bad.into());
    let crate::doctor::Config::Malformed(expected) = doctor else {
        panic!("the doctor reads it as malformed")
    };
    assert_eq!(why, [expected]);
    let err = save(dir.path(), Which::Shared, Some(COMMENTED), bad).unwrap_err();
    assert_eq!(err, why);
    assert_eq!(
        text(dir.path(), "charter.toml"),
        COMMENTED,
        "nothing was written"
    );
}

#[test]
fn a_schema_this_charter_cannot_place_is_refused() {
    let dir = plane(COMMENTED);
    let why = refusals(dir.path(), Which::Shared, "schema = 2\n");
    assert_eq!(why.len(), 1);
    assert!(
        why[0].ends_with(
            "declares schema 2, but this charter understands 1. Upgrade charter: update the app."
        ),
        "{why:?}"
    );
}

#[test]
fn a_forge_that_does_not_resolve_is_refused_as_the_doctor_says_it() {
    let dir = plane(COMMENTED);
    let why = refusals(
        dir.path(),
        Which::Shared,
        "[[forge]]\nkind = \"bitbucket\"\n",
    );
    assert_eq!(why.len(), 1, "{why:?}");
    assert!(
        why[0].starts_with("1 [[forge]] block(s) failed to resolve — [[forge]] block 0:"),
        "{why:?}"
    );
}

#[test]
fn a_profile_in_the_committed_file_is_refused_with_the_pointer_to_the_local_one() {
    let dir = plane(COMMENTED);
    let why = refusals(
        dir.path(),
        Which::Shared,
        "[harness.work]\nkind = \"claude\"\ncommand = [\"claude\"]\n",
    );
    assert_eq!(why.len(), 1, "{why:?}");
    assert!(
        why[0].starts_with("[harness.work] is in charter.toml, which is committed"),
        "{why:?}"
    );
}

#[test]
fn a_harness_default_naming_nothing_is_refused_but_one_the_local_file_declares_stands() {
    let dir = plane(COMMENTED);
    let why = refusals(dir.path(), Which::Shared, "[harness]\ndefault = \"work\"\n");
    assert_eq!(why.len(), 1, "{why:?}");
    assert!(
        why[0].starts_with("[harness] default = \"work\" is not a harness charter can launch"),
        "{why:?}"
    );
    fs::write(
        dir.path().join("charter.local.toml"),
        "[harness.work]\nkind = \"claude\"\ncommand = [\"claude\"]\n",
    )
    .unwrap();
    assert_eq!(
        refusals(dir.path(), Which::Shared, "[harness]\ndefault = \"work\"\n"),
        Vec::<String>::new()
    );
}

#[test]
fn a_local_profile_charter_would_refuse_is_refused_in_its_own_sentence() {
    let dir = plane(COMMENTED);
    let why = refusals(
        dir.path(),
        Which::Local,
        "[harness.work]\nkind = \"claude\"\ncommand = \"claude --x\"\n",
    );
    assert_eq!(
        why,
        [
            "profile 'work' has no usable command — command is a list of arguments, \
          [\"claude\"], never a shell string, because no shell runs it. Write it as a list."
        ]
    );
}

#[test]
fn a_local_file_holding_anything_but_harness_is_refused() {
    let dir = plane(COMMENTED);
    let why = refusals(dir.path(), Which::Local, "[memory]\nshare = \"push\"\n");
    assert_eq!(why.len(), 1, "{why:?}");
    assert!(
        why[0].starts_with("[memory] in charter.local.toml is not read"),
        "{why:?}"
    );
}

#[test]
fn a_local_file_may_hold_extensions_as_well_as_harness() {
    // charter-app#253: an extension is turned on or off, and configured, for this machine's
    // use of the project in the Local file, which overrides the Shared one key by key.
    let dir = plane(COMMENTED);
    let why = refusals(
        dir.path(),
        Which::Local,
        "[extensions.stats]\nenabled = false\n[extensions.stats.settings]\nwindow = \"7d\"\n",
    );
    assert_eq!(why, Vec::<String>::new());
}

#[test]
fn a_local_file_may_hold_plane_and_repos_as_well() {
    // charter-app#292, ADR 0051: how far a save goes is overridable per machine, key by key.
    let dir = plane(COMMENTED);
    let why = refusals(
        dir.path(),
        Which::Local,
        "[plane]\nmode = \"commit\"\nautosave = false\n\n[repos.charter-app]\nmode = \"push\"\n",
    );
    assert_eq!(why, Vec::<String>::new());
}

#[test]
fn plane_and_repos_values_charter_would_not_read_are_refused_in_either_file() {
    let dir = plane(COMMENTED);
    for which in [Which::Shared, Which::Local] {
        let file = which.file();
        let why = refusals(
            dir.path(),
            which,
            "[plane]\nmode = \"yolo\"\nsign = \"yes\"\nautosave_after = \"soon\"\n\
             branch = \"has space\"\ncolour = \"red\"\n\n\
             [repos.charter-app]\nsave_branch = \"x\"\n",
        );
        assert_eq!(
            why,
            [
                format!(
                    "plane.mode in {file} is not a mode — one of off, commit, push, pr, pr-merge"
                ),
                format!("plane.sign in {file} is not true or false"),
                format!(
                    "plane.autosave_after in {file} is not a quiet period — a whole number of \
                     seconds or minutes, like \"30s\" or \"2m\""
                ),
                format!("plane.branch in {file} is not a branch name git would accept"),
                format!(
                    "plane.colour in {file} is not read — [plane] holds mode, branch, \
                     save_branch, sign, autosave, autosave_after and worktrees"
                ),
                format!(
                    "repos.charter-app.save_branch in {file} is not read — [repos.<name>] holds \
                     mode, branch, sign, autosave and autosave_after"
                ),
            ],
            "{which:?}"
        );
    }
}

#[test]
fn worktrees_in_the_local_file_is_refused_with_the_way_to_set_it_per_machine() {
    let dir = plane(COMMENTED);
    let why = refusals(dir.path(), Which::Local, "[plane]\nworktrees = \"../wt\"\n");
    assert_eq!(
        why,
        [
            "plane.worktrees in charter.local.toml is not read — it belongs in charter.toml, and \
          $CHARTER_WORKTREES sets it for this machine alone"
        ]
    );
}

#[test]
fn an_extensions_table_charter_would_not_read_is_refused_in_either_file() {
    let dir = plane(COMMENTED);
    for which in [Which::Shared, Which::Local] {
        let why = refusals(dir.path(), which, "[extensions.stats]\nenabled = \"yes\"\n");
        assert_eq!(
            why,
            [format!(
                "extensions.stats.enabled in {} is not true or false",
                which.file()
            )],
            "{which:?}"
        );
    }
}

#[test]
fn a_local_file_that_does_not_exist_is_created_on_the_first_save() {
    let dir = plane(COMMENTED);
    let body = "[harness]\ndefault = \"claude\"\n";
    save(dir.path(), Which::Local, None, body).unwrap();
    assert_eq!(text(dir.path(), "charter.local.toml"), body);
}

#[cfg(unix)]
#[test]
fn a_new_local_file_is_readable_by_this_user_only() {
    use std::os::unix::fs::PermissionsExt;
    let dir = plane(COMMENTED);
    save(dir.path(), Which::Local, None, "").unwrap();
    let mode = fs::metadata(dir.path().join("charter.local.toml"))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);
}

#[cfg(unix)]
#[test]
fn saving_an_existing_file_keeps_its_mode() {
    use std::os::unix::fs::PermissionsExt;
    let dir = plane(COMMENTED);
    let path = dir.path().join("charter.toml");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    save(dir.path(), Which::Shared, Some(COMMENTED), "schema = 1\n").unwrap();
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o644
    );
}

#[test]
fn a_local_file_git_would_commit_is_never_written() {
    let dir = plane(COMMENTED);
    fs::write(dir.path().join(".gitignore"), "").unwrap();
    let err = save(dir.path(), Which::Local, None, "").unwrap_err();
    assert_eq!(
        err,
        [
            "git would commit charter.local.toml, so the profiles in it are refused until it is \
          ignored — charter reinit adds /charter.local.toml to .gitignore."
        ]
    );
    assert!(
        !dir.path().join("charter.local.toml").exists(),
        "nothing was created"
    );
}

#[test]
fn a_local_file_git_tracks_is_never_written_even_where_gitignore_names_it() {
    let dir = plane(COMMENTED);
    fs::write(dir.path().join("charter.local.toml"), "").unwrap();
    crate::testgit::run(dir.path(), &["add", "-f", "charter.local.toml"]);
    let err = save(dir.path(), Which::Local, Some(""), "[harness]\n").unwrap_err();
    assert_eq!(err.len(), 1, "{err:?}");
    assert!(
        err[0].starts_with("git tracks charter.local.toml"),
        "{err:?}"
    );
}

#[test]
fn a_plane_that_is_not_a_repository_has_nothing_to_commit_the_local_file_to() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("charter.toml"), "").unwrap();
    save(dir.path(), Which::Local, None, "[harness]\n").unwrap();
}

#[test]
fn a_secret_shaped_value_is_refused_by_its_kind_and_never_quoted() {
    let dir = plane(COMMENTED);
    let token = "AKIAIOSFODNN7EXAMPLE";
    let body = format!("[workspace]\ndefault = \"{token}\"\n");
    let why = refusals(dir.path(), Which::Shared, &body);
    assert_eq!(why.len(), 1, "{why:?}");
    assert!(why[0].contains("AWS access key"), "{why:?}");
    assert!(why[0].contains("vault"), "points at vaults: {why:?}");
    assert!(!why[0].contains(token), "the value is never said back");
    let local = refusals(dir.path(), Which::Local, &format!("# {token}\n"));
    assert_eq!(
        local.len(),
        1,
        "a comment is part of the file too: {local:?}"
    );
}

#[test]
fn a_file_changed_on_disk_since_it_was_read_is_not_overwritten() {
    let dir = plane(COMMENTED);
    fs::write(dir.path().join("charter.toml"), "schema = 1\n").unwrap();
    let err = save(dir.path(), Which::Shared, Some(COMMENTED), COMMENTED).unwrap_err();
    assert_eq!(
        err,
        [
            "charter.toml changed on disk since this tab read it, so nothing was saved. Read it \
          again, then make the change again."
        ]
    );
    assert_eq!(text(dir.path(), "charter.toml"), "schema = 1\n");
}

#[test]
fn a_local_file_created_by_somebody_else_since_it_was_read_is_not_overwritten() {
    let dir = plane(COMMENTED);
    fs::write(dir.path().join("charter.local.toml"), "[harness]\n").unwrap();
    let err = save(dir.path(), Which::Local, None, "").unwrap_err();
    assert!(
        err[0].starts_with("charter.local.toml changed on disk"),
        "{err:?}"
    );
}

#[cfg(unix)]
#[test]
fn a_local_file_that_is_a_link_is_not_written_through() {
    let dir = plane(COMMENTED);
    let elsewhere = tempfile::tempdir().unwrap();
    let target = elsewhere.path().join("x.toml");
    fs::write(&target, "").unwrap();
    std::os::unix::fs::symlink(&target, dir.path().join("charter.local.toml")).unwrap();
    assert!(save(dir.path(), Which::Local, Some(""), "[harness]\n").is_err());
    assert_eq!(fs::read_to_string(&target).unwrap(), "");
}

#[test]
fn what_the_core_says_about_the_files_as_they_stand_comes_with_the_read() {
    let dir = plane("[[forge]]\nkind = \"bitbucket\"\n");
    let shared = read(dir.path(), Which::Shared).unwrap();
    assert_eq!(shared.refusals.len(), 1, "{:?}", shared.refusals);
}

#[test]
fn every_value_is_found_by_its_path_and_a_forge_block_by_its_place() {
    let found = fields(COMMENTED).unwrap();
    let paths: Vec<(String, Found)> = found
        .into_iter()
        .map(|(path, value)| (dotted(&path), value))
        .collect();
    assert_eq!(
        paths,
        [
            ("schema".to_owned(), Found::Value(Value::Integer(1))),
            (
                "forge[0].kind".to_owned(),
                Found::Value(Value::Text("github".into()))
            ),
            (
                "forge[0].owner".to_owned(),
                Found::Value(Value::Text("acme".into()))
            ),
            (
                "memory.share".to_owned(),
                Found::Value(Value::Text("local".into()))
            ),
            (
                "workspace.default".to_owned(),
                Found::Value(Value::Text("default".into()))
            ),
        ]
    );
    assert_eq!(fields("[memory\n"), None);
    assert_eq!(
        fields("ratio = 0.5\n").unwrap(),
        [(vec![Step::Key("ratio".into())], Found::Other("0.5".into()))]
    );
}

#[test]
fn a_profiles_env_written_inline_is_edited_inline() {
    let body = "[harness.work]\nkind = \"claude\"\ncommand = [\"claude\"]\nenv = { CLAUDE_CONFIG_DIR = \"~/.a\", LANG = \"C\" }\n";
    let after = edited(
        body,
        &[
            set(
                &["harness", "work", "env", "CLAUDE_CONFIG_DIR"],
                Value::Text("~/.b".into()),
            ),
            Edit {
                path: ["harness", "work", "env", "LANG"]
                    .iter()
                    .map(|s| Step::Key((*s).to_owned()))
                    .collect(),
                value: None,
            },
        ],
    )
    .unwrap();
    // The space before `}` was the removed entry's own, and went with it.
    assert_eq!(
        after,
        "[harness.work]\nkind = \"claude\"\ncommand = [\"claude\"]\nenv = { CLAUDE_CONFIG_DIR = \"~/.b\"}\n"
    );
}

#[test]
fn a_problem_the_file_already_has_does_not_stop_an_unrelated_edit_but_a_new_one_is_refused() {
    let broken = "[[forge]]\nkind = \"bitbucket\"\n\n[memory]\nshare = \"local\"\n";
    let dir = plane(broken);
    let after = edited(
        broken,
        &[set(&["memory", "share"], Value::Text("push".into()))],
    )
    .unwrap();
    save(dir.path(), Which::Shared, Some(broken), &after).unwrap();

    let worse = format!("{after}\n[[forge]]\nkind = \"sourcehut\"\n");
    let err = save(dir.path(), Which::Shared, Some(&after), &worse).unwrap_err();
    assert_eq!(err.len(), 1, "{err:?}");
    assert!(
        err[0].starts_with("2 [[forge]] block(s) failed to resolve"),
        "{err:?}"
    );
}

#[test]
fn a_secret_the_file_already_holds_still_stops_every_save() {
    let held = "# password = hunter2hunter2\n";
    let dir = plane(held);
    let err = save(
        dir.path(),
        Which::Shared,
        Some(held),
        &format!("{held}schema = 1\n"),
    )
    .unwrap_err();
    assert!(err[0].contains("credential assignment"), "{err:?}");
}

#[test]
fn a_value_is_never_written_over_a_table() {
    let err = edited(COMMENTED, &[set(&["memory"], Value::Text("push".into()))]).unwrap_err();
    assert!(err.contains("memory is a table"), "{err}");
    let err = edited(COMMENTED, &[set(&["forge"], Value::Text("x".into()))]).unwrap_err();
    assert!(err.contains("forge is a table"), "{err}");
}
