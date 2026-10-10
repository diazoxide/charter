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
fn a_local_file_that_is_not_toml_is_refused_and_nothing_is_written() {
    let dir = plane(COMMENTED);
    let before = "[harness]\ndefault = \"claude\"\n";
    save(dir.path(), Which::Local, None, before).unwrap();
    let bad = "[harness\ndefault = \"claude\"\n";
    let err = save(dir.path(), Which::Local, Some(before), bad).unwrap_err();
    assert!(
        err.iter().any(|why| why.contains("TOML")),
        "the parser's reason is said: {err:?}"
    );
    assert_eq!(
        text(dir.path(), "charter.local.toml"),
        before,
        "nothing was written"
    );
}

/// The raw editor (SE-19) is how a file that does not parse is mended, so a save that leaves it
/// still not parsing is refused rather than waved through as what the file already held: a
/// half-mended file is never written.
#[test]
fn a_file_that_does_not_parse_is_never_saved_as_text_that_still_does_not() {
    for which in [Which::Shared, Which::Local] {
        let dir = plane(COMMENTED);
        let broken = "[memory\nshare = 1\n";
        fs::write(which.path(dir.path()), broken).unwrap();
        let err = save(dir.path(), which, Some(broken), broken).unwrap_err();
        assert!(!err.is_empty(), "{which:?}");
        assert_eq!(
            fs::read_to_string(which.path(dir.path())).unwrap(),
            broken,
            "{which:?}: nothing was written"
        );
    }
}

#[test]
fn a_schema_this_charter_cannot_place_is_refused() {
    let dir = plane(COMMENTED);
    let why = refusals(dir.path(), Which::Shared, "schema = 3\n");
    assert_eq!(why.len(), 1);
    assert!(
        why[0].ends_with(
            "declares schema 3, but this purlis understands 2. Upgrade purlis: update the app."
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
        why[0].starts_with("[harness] default = \"work\" is not a harness purlis can launch"),
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

/// #1197: `[chat_env]` is this machine's and read from the Local file alone, so the Shared
/// file refuses it in the reader's words rather than letting a save or a move write it unread.
#[test]
fn chat_env_in_the_shared_file_is_refused_as_not_read() {
    // No git and no file on disk: the Shared file's refusals read the text alone.
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(
        refusals(
            dir.path(),
            Which::Shared,
            "schema = 1\n[chat_env]\npass = [\"GOPATH\"]\n"
        ),
        [
            "[chat_env] in charter.toml is not read — it names what this machine passes to a \
             chat, so it is read from charter.local.toml alone. Put [chat_env] in \
             charter.local.toml."
        ]
    );
    assert_eq!(
        refusals(dir.path(), Which::Shared, "schema = 1\n"),
        Vec::<String>::new()
    );
}

/// ST-1 (#1225): the default persona and workspace are pickers over what is here, and a value
/// set by hand that names nothing is the core's to say, as `[harness] default`'s is.
#[test]
fn a_persona_default_naming_no_persona_is_refused_and_one_that_is_here_stands() {
    let dir = plane("schema = 1\n");
    let why = refusals(
        dir.path(),
        Which::Shared,
        "[persona]\ndefault = \"ghost\"\n",
    );
    assert_eq!(why.len(), 1, "{why:?}");
    assert!(
        why[0].starts_with("[persona] default = \"ghost\" names no persona in this project"),
        "{why:?}"
    );
    fs::create_dir_all(dir.path().join("personas/ghost")).unwrap();
    fs::write(
        dir.path().join("personas/ghost/persona.md"),
        "---\nrole: Ghost\n---\n",
    )
    .unwrap();
    assert_eq!(
        refusals(
            dir.path(),
            Which::Shared,
            "[persona]\ndefault = \"ghost\"\n"
        ),
        Vec::<String>::new()
    );
}

/// A declared `[workspace] default` is a workspace charter treats as there and makes where it
/// is first used (`wscmd::select`'s `there`), so one not made yet is no refusal (D-ST1-1, amended).
#[test]
fn a_workspace_default_declared_before_it_is_made_stands() {
    let dir = plane("schema = 1\n");
    for named in ["later", "default"] {
        assert_eq!(
            refusals(
                dir.path(),
                Which::Shared,
                &format!("[workspace]\ndefault = \"{named}\"\n"),
            ),
            Vec::<String>::new()
        );
    }
}

#[test]
fn a_default_set_by_hand_to_nothing_does_not_stop_another_edit_but_a_new_one_is_refused() {
    let dir = plane("schema = 1\n[persona]\ndefault = \"ghost\"\n");
    write_one(
        dir.path(),
        &["memory", "share"],
        Some(Value::Text("local".into())),
    )
    .unwrap();
    let refused = write_one(
        dir.path(),
        &["persona", "default"],
        Some(Value::Text("nobody".into())),
    )
    .unwrap_err();
    assert!(
        refused[0].starts_with("[persona] default = \"nobody\""),
        "{refused:?}"
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
                     save_branch, sign, autosave, autosave_after, assisted_by and worktrees"
                ),
                format!(
                    "repos.charter-app.save_branch in {file} is not read — [repos.<name>] holds \
                     mode, branch, sign, autosave, autosave_after and assisted_by"
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
fn a_local_file_may_hold_harness_plugins() {
    // charter-app#274: a harness's own plugins are turned on or off for this machine's use of
    // the project in the Local file, which overrides the Shared one plugin by plugin.
    let dir = plane(COMMENTED);
    let why = refusals(
        dir.path(),
        Which::Local,
        "[harness_plugins.claude]\n\"figma@official\" = false\n",
    );
    assert_eq!(why, Vec::<String>::new());
}

#[test]
fn neither_file_may_turn_charters_own_plugin_off() {
    let dir = plane(COMMENTED);
    for which in [Which::Shared, Which::Local] {
        let why = refusals(
            dir.path(),
            which,
            "[harness_plugins.claude]\n\"purlis@inline\" = false\n",
        );
        assert_eq!(
            why,
            [format!(
                "harness_plugins.claude.\"purlis@inline\" in {} cannot be false: \
                 purlis@inline is always on: it is purlis's own plugin, and it carries \
                 purlis's hooks and the Bash guard",
                which.file()
            )],
            "{which:?}"
        );
    }
}

#[test]
fn the_shared_file_may_turn_the_sandbox_on_and_never_off() {
    // ADR 0067: a committed file may restrict what a chat is confined to, never loosen it.
    let dir = plane(COMMENTED);
    assert_eq!(
        refusals(dir.path(), Which::Shared, "[sandbox]\nmode = \"on\"\n"),
        Vec::<String>::new()
    );
    assert_eq!(
        refusals(dir.path(), Which::Shared, "[sandbox]\nmode = \"off\"\n"),
        [
            "sandbox.mode in charter.toml cannot be \"off\": a committed file may turn the \
          sandbox on and never off — only a person turns it off, for one chat; so the \
          sandbox is on"
        ]
    );
}

#[test]
fn the_local_file_says_nothing_about_the_sandbox() {
    // An ignored file must not change plane policy with no trace in git, and whether chats run
    // sandboxed is plane policy: the local file's `[sandbox]` holds this machine's hosts and
    // only those (#1341), so a mode there is refused.
    let dir = plane(COMMENTED);
    let why = refusals(dir.path(), Which::Local, "[sandbox]\nmode = \"on\"\n");
    assert_eq!(why.len(), 1, "{why:?}");
    assert!(
        why[0].starts_with("sandbox.mode in charter.local.toml is not read"),
        "{why:?}"
    );
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
            "git would commit charter.local.toml, so purlis reads nothing in it until it is \
          ignored — purlis doctor --fix local-ignore adds /charter.local.toml to .gitignore."
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

#[test]
fn either_file_may_pick_a_theme() {
    // charter-app#273: a project's theme, Local over Shared.
    let dir = plane(COMMENTED);
    for which in [Which::Shared, Which::Local] {
        for value in ["charter-light", "system", "solarized/Solarized Dark"] {
            let why = refusals(dir.path(), which, &format!("[theme]\nuse = \"{value}\"\n"));
            assert_eq!(why, Vec::<String>::new(), "{which:?} {value}");
        }
    }
}

#[test]
fn a_theme_charter_would_not_read_is_refused_in_either_file() {
    let dir = plane(COMMENTED);
    for which in [Which::Shared, Which::Local] {
        let file = which.file();
        let why = refusals(
            dir.path(),
            which,
            "[theme]\nuse = \"purple\"\nfont = \"x\"\n",
        );
        assert_eq!(
            why,
            [
                format!(
                    "theme.use in {file} is \"purple\", which is not charter-dark, \
                     charter-light, system or <extension>/<theme>"
                ),
                format!(
                    "theme.font in {file} is not read — [theme] holds use and icons and nothing \
                     else"
                ),
            ],
            "{which:?}"
        );
        let why = refusals(dir.path(), which, "theme = \"system\"\n");
        assert_eq!(
            why,
            [format!(
                "theme in {file} is not a table — write [theme] with use = \"<theme>\""
            )],
            "{which:?}"
        );
    }
}

/// #357: the settings tab and `charter persona default` both rewrite charter.toml. One
/// starting while the other is between its check and its rename must not be overwritten by
/// it: the tab's save holds the plane's lock from the "changed on disk?" check to the rename.
#[test]
fn a_save_and_a_persona_default_at_once_both_land() {
    let dir = plane("schema = 1\n");
    let manifest = dir.path().join("charter.toml");
    let other = manifest.clone();
    let second: std::rc::Rc<std::cell::RefCell<Option<std::thread::JoinHandle<()>>>> =
        Default::default();
    let started = second.clone();
    let _hook = crate::rewrite::hook::set(move |_, _| {
        if started.borrow().is_none() {
            let path = other.clone();
            let (done, finished) = std::sync::mpsc::channel();
            *started.borrow_mut() = Some(std::thread::spawn(move || {
                crate::personacmd::set_key(&path, "persona", "default", Some("ops")).unwrap();
                let _ = done.send(());
            }));
            let _ = finished.recv_timeout(std::time::Duration::from_millis(300));
        }
        Ok(())
    });

    save(
        dir.path(),
        Which::Shared,
        Some("schema = 1\n"),
        "schema = 1\n\n[memory]\nshare = \"local\"\n",
    )
    .unwrap();
    second
        .borrow_mut()
        .take()
        .expect("the hook ran")
        .join()
        .unwrap();

    let now = text(dir.path(), "charter.toml");
    assert!(now.contains("[memory]\nshare = \"local\"\n"), "{now}");
    assert!(now.contains("[persona]\ndefault = \"ops\"\n"), "{now}");
}

/// A file a person wrote by hand: comments, a key no charter reads, and a table no form covers.
const HAND_WRITTEN: &str = "\
# The team's settings. Hand-edited; keep the comments.
schema = 1

[plane]
mode = \"push\"     # how far a save goes
autosave = true

# Not charter's: a key a person keeps here for their own tools.
[tools]
lint = \"strict\"

[[forge]]
kind = \"github\"
owner = \"acme\"
";

/// One Setting's write in the Settings tab (SE-17, V89e): the one key it names, through the
/// same `edited` and `save` every write takes.
fn write_one(root: &Path, path: &[&str], value: Option<Value>) -> Result<(), Vec<String>> {
    let now = read(root, Which::Shared).unwrap();
    let edit = Edit {
        path: path.iter().map(|s| Step::Key((*s).to_owned())).collect(),
        value,
    };
    let after = edited(&now.text, &[edit]).map_err(|why| vec![why])?;
    save(root, Which::Shared, Some(&now.text), &after)
}

#[test]
fn a_setting_written_on_its_own_changes_its_line_and_keeps_every_other_key_and_comment() {
    let dir = plane(HAND_WRITTEN);

    write_one(
        dir.path(),
        &["plane", "mode"],
        Some(Value::Text("commit".into())),
    )
    .unwrap();

    assert_eq!(
        text(dir.path(), "charter.toml"),
        HAND_WRITTEN.replace(
            "mode = \"push\"     # how far a save goes",
            "mode = \"commit\"     # how far a save goes"
        )
    );
}

#[test]
fn writing_the_value_a_setting_had_before_puts_the_file_back_as_it_was() {
    // Undo (V89e) is the previous value written the same way.
    let dir = plane(HAND_WRITTEN);
    write_one(dir.path(), &["plane", "autosave"], Some(Value::Bool(false))).unwrap();

    write_one(dir.path(), &["plane", "autosave"], Some(Value::Bool(true))).unwrap();

    assert_eq!(text(dir.path(), "charter.toml"), HAND_WRITTEN);
}

#[test]
fn a_setting_given_a_value_charter_would_not_read_is_refused_and_nothing_is_written() {
    let dir = plane(HAND_WRITTEN);

    let refused = write_one(
        dir.path(),
        &["plane", "mode"],
        Some(Value::Text("sideways".into())),
    )
    .unwrap_err();

    assert_eq!(
        refused,
        ["plane.mode in charter.toml is not a mode — one of off, commit, push, pr, pr-merge"]
    );
    assert_eq!(text(dir.path(), "charter.toml"), HAND_WRITTEN);
}

// ---------------------------------------------------------------------------------------
// Moving a value between the two files (SE-18, V89d)
// ---------------------------------------------------------------------------------------

fn path_of(keys: &[&str]) -> Vec<Step> {
    keys.iter()
        .map(|key| Step::Key((*key).to_owned()))
        .collect()
}

const SAVING: &str = "\
# the team's
[plane]
mode = \"push\"   # how far a save goes
sign = true
";

#[test]
fn a_value_moved_to_this_machine_leaves_charter_toml_and_lands_in_charter_local_toml() {
    let dir = plane(SAVING);
    move_keys(
        dir.path(),
        Which::Local,
        Some(SAVING),
        None,
        &[path_of(&["plane", "mode"])],
    )
    .unwrap();
    assert_eq!(
        text(dir.path(), "charter.toml"),
        "# the team's\n[plane]\nsign = true\n"
    );
    let local = text(dir.path(), "charter.local.toml");
    assert_eq!(
        fields(&local).unwrap(),
        [(
            path_of(&["plane", "mode"]),
            Found::Value(Value::Text("push".into()))
        )]
    );
}

#[test]
fn a_value_moved_back_to_shared_leaves_charter_local_toml_and_lands_in_charter_toml() {
    let dir = plane("[plane]\nsign = true\n");
    let local = "[plane]\nmode = \"pr\"\n\n[harness]\ndefault = \"work\"\n";
    fs::write(dir.path().join("charter.local.toml"), local).unwrap();
    move_keys(
        dir.path(),
        Which::Shared,
        Some(local),
        Some("[plane]\nsign = true\n"),
        &[path_of(&["plane", "mode"])],
    )
    .unwrap();
    assert_eq!(
        fields(&text(dir.path(), "charter.toml")).unwrap(),
        [
            (path_of(&["plane", "sign"]), Found::Value(Value::Bool(true))),
            (
                path_of(&["plane", "mode"]),
                Found::Value(Value::Text("pr".into()))
            ),
        ]
    );
    // A table the move emptied goes with it.
    let local = text(dir.path(), "charter.local.toml");
    assert!(!local.contains("[plane]"), "{local}");
    assert_eq!(
        fields(&local).unwrap(),
        [(
            path_of(&["harness", "default"]),
            Found::Value(Value::Text("work".into()))
        )]
    );
}

#[test]
fn moving_a_value_over_one_the_other_file_holds_replaces_it() {
    let dir = plane(SAVING);
    let local = "[plane]\nmode = \"commit\"\n";
    fs::write(dir.path().join("charter.local.toml"), local).unwrap();
    move_keys(
        dir.path(),
        Which::Shared,
        Some(local),
        Some(SAVING),
        &[path_of(&["plane", "mode"])],
    )
    .unwrap();
    assert_eq!(
        text(dir.path(), "charter.toml"),
        "# the team's\n[plane]\nmode = \"commit\"   # how far a save goes\nsign = true\n"
    );
}

#[test]
fn a_move_the_other_file_would_refuse_writes_neither_file() {
    // [sandbox] mode is only the Shared file's (the local file's [sandbox] holds hosts alone,
    // #1341): moving it to this machine is refused, and the sandbox stays on in charter.toml.
    let shared = "[sandbox]\nmode = \"on\"\n";
    let dir = plane(shared);
    let err = move_keys(
        dir.path(),
        Which::Local,
        Some(shared),
        None,
        &[path_of(&["sandbox", "mode"])],
    )
    .unwrap_err();
    assert!(
        err[0].starts_with("sandbox.mode in charter.local.toml is not read"),
        "{err:?}"
    );
    assert_eq!(text(dir.path(), "charter.toml"), shared);
    assert!(!dir.path().join("charter.local.toml").exists());
}

#[test]
fn a_move_to_a_local_file_git_would_commit_writes_neither_file() {
    let dir = plane(SAVING);
    fs::write(dir.path().join(".gitignore"), "").unwrap();
    let err = move_keys(
        dir.path(),
        Which::Local,
        Some(SAVING),
        None,
        &[path_of(&["plane", "mode"])],
    )
    .unwrap_err();
    assert!(err[0].starts_with("git would commit"), "{err:?}");
    assert_eq!(text(dir.path(), "charter.toml"), SAVING);
    assert!(!dir.path().join("charter.local.toml").exists());
}

#[test]
fn a_move_when_either_file_changed_since_it_was_read_writes_neither_file() {
    let dir = plane(SAVING);
    fs::write(dir.path().join("charter.local.toml"), "[plane]\n").unwrap();
    let err = move_keys(
        dir.path(),
        Which::Local,
        Some(SAVING),
        None,
        &[path_of(&["plane", "mode"])],
    )
    .unwrap_err();
    assert!(
        err[0].starts_with("charter.local.toml changed on disk"),
        "{err:?}"
    );
    assert_eq!(text(dir.path(), "charter.toml"), SAVING);
    assert_eq!(text(dir.path(), "charter.local.toml"), "[plane]\n");
}

#[test]
fn moving_a_value_the_file_does_not_hold_is_refused() {
    let dir = plane(SAVING);
    let err = move_keys(
        dir.path(),
        Which::Local,
        Some(SAVING),
        None,
        &[path_of(&["plane", "branch"])],
    )
    .unwrap_err();
    assert_eq!(
        err,
        ["plane.branch is not in charter.toml, so there is nothing to move."]
    );
    assert!(!dir.path().join("charter.local.toml").exists());
}

#[cfg(unix)]
#[test]
fn a_move_whose_second_write_fails_puts_the_first_file_back() {
    use std::os::unix::fs::PermissionsExt;
    // The file the value comes out of is written last; a read-only one is not written, and
    // the file the value went into is put back as it was.
    let dir = plane(SAVING);
    let local = "[harness]\ndefault = \"work\"\n";
    fs::write(dir.path().join("charter.local.toml"), local).unwrap();
    let shared = dir.path().join("charter.toml");
    fs::set_permissions(&shared, fs::Permissions::from_mode(0o444)).unwrap();
    let err = move_keys(
        dir.path(),
        Which::Local,
        Some(SAVING),
        Some(local),
        &[path_of(&["plane", "mode"])],
    )
    .unwrap_err();
    assert!(
        err[0].starts_with("charter.toml could not be written"),
        "{err:?}"
    );
    assert_eq!(text(dir.path(), "charter.toml"), SAVING);
    assert_eq!(text(dir.path(), "charter.local.toml"), local);
}

#[cfg(unix)]
#[test]
fn a_failed_move_into_a_new_local_file_leaves_no_local_file() {
    use std::os::unix::fs::PermissionsExt;
    let dir = plane(SAVING);
    let shared = dir.path().join("charter.toml");
    fs::set_permissions(&shared, fs::Permissions::from_mode(0o444)).unwrap();
    move_keys(
        dir.path(),
        Which::Local,
        Some(SAVING),
        None,
        &[path_of(&["plane", "mode"])],
    )
    .unwrap_err();
    assert_eq!(text(dir.path(), "charter.toml"), SAVING);
    assert!(!dir.path().join("charter.local.toml").exists());
}

#[test]
fn a_local_harness_default_naming_nothing_is_refused_in_the_doctors_words() {
    // The Local file is asked about its `[harness] default` alone, and asked all the same.
    let dir = plane(COMMENTED);
    let why = refusals(dir.path(), Which::Local, "[harness]\ndefault = \"work\"\n");
    assert_eq!(why.len(), 1, "{why:?}");
    assert!(
        why[0].starts_with("[harness] default = \"work\" is not a harness purlis can launch"),
        "{why:?}"
    );
}

#[test]
fn a_list_a_form_can_write_is_text_only_and_any_other_list_is_left_to_the_raw_view() {
    let found = fields("none = []\nnames = [\"a\", \"b\"]\nports = [1, 2]\n").unwrap();
    let paths: Vec<(String, Found)> = found
        .into_iter()
        .map(|(path, value)| (dotted(&path), value))
        .collect();
    assert_eq!(
        paths,
        [
            ("none".to_owned(), Found::Value(Value::List(Vec::new()))),
            (
                "names".to_owned(),
                Found::Value(Value::List(vec!["a".into(), "b".into()]))
            ),
            ("ports".to_owned(), Found::Other("[1, 2]".into())),
        ]
    );
}

#[test]
fn a_value_is_never_written_over_an_inline_table() {
    let body = "[harness.work]\nkind = \"claude\"\nenv = { LANG = \"C\" }\n";
    let err = edited(
        body,
        &[set(&["harness", "work", "env"], Value::Text("x".into()))],
    )
    .unwrap_err();
    assert!(err.contains("harness.work.env is a table"), "{err}");
}

#[test]
fn a_settings_file_that_is_there_but_cannot_be_read_is_said_and_not_shown_as_absent() {
    // A directory where the file belongs: there, and not a file charter can read.
    let dir = plane(COMMENTED);
    fs::create_dir(dir.path().join("charter.local.toml")).unwrap();
    let err = read(dir.path(), Which::Local).unwrap_err();
    assert!(
        err.starts_with("charter.local.toml could not be read"),
        "{err}"
    );
}

// ---- the rename window (RN-2a, V93e) ------------------------------------------------ //

/// A plane whose two settings files go by `manifest` and `local`, both ignored the way a plane
/// of either name ignores its local file.
fn plane_named(manifest: &str, local: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join(manifest), "schema = 1\n").unwrap();
    fs::write(dir.path().join(local), "").unwrap();
    crate::testgit::run(dir.path(), &["init", "-q"]);
    fs::write(
        dir.path().join(".gitignore"),
        "/charter.local.toml\n/purlis.local.toml\n",
    )
    .unwrap();
    dir
}

#[test]
fn a_save_lands_in_the_settings_file_the_plane_has_and_never_starts_a_second() {
    for (manifest, local, other_manifest, other_local) in [
        (
            "charter.toml",
            "charter.local.toml",
            "purlis.toml",
            "purlis.local.toml",
        ),
        (
            "purlis.toml",
            "purlis.local.toml",
            "charter.toml",
            "charter.local.toml",
        ),
    ] {
        let dir = plane_named(manifest, local);
        let root = dir.path();
        let shared = "schema = 1\n\n[workspace]\ndefault = \"ide\"\n";
        save(root, Which::Shared, Some("schema = 1\n"), shared).unwrap();
        let body = "[harness]\ndefault = \"claude\"\n";
        save(root, Which::Local, Some(""), body).unwrap();

        assert_eq!(text(root, manifest), shared);
        assert_eq!(text(root, local), body);
        assert!(!root.join(other_manifest).exists(), "{other_manifest}");
        assert!(!root.join(other_local).exists(), "{other_local}");
        assert_eq!(read(root, Which::Shared).unwrap().file, manifest);
        assert_eq!(read(root, Which::Local).unwrap().file, local);
    }
}

#[test]
fn with_both_names_the_purlis_file_is_read_and_written_and_the_old_one_left_alone() {
    let dir = plane_named("charter.toml", "charter.local.toml");
    let root = dir.path();
    fs::write(root.join("purlis.toml"), "schema = 1\n").unwrap();
    fs::write(root.join("charter.toml"), "schema = 1\n# old\n").unwrap();

    assert_eq!(read(root, Which::Shared).unwrap().text, "schema = 1\n");
    let shared = "schema = 1\n\n[workspace]\ndefault = \"ide\"\n";
    save(root, Which::Shared, Some("schema = 1\n"), shared).unwrap();

    assert_eq!(text(root, "purlis.toml"), shared);
    assert_eq!(text(root, "charter.toml"), "schema = 1\n# old\n");
}

#[test]
fn a_secret_is_refused_as_the_file_reads_it_not_only_as_typed() {
    // `\u0041` is `A`, and `\u0077` is `w`: what charter reads is the secret, so it is refused
    // the way Edit as JSON refuses a manifest's and the project save refuses a staged file.
    let dir = plane(COMMENTED);
    for body in [
        "[workspace]\ndefault = \"\\u0041KIAIOSFODNN7EXAMPLE\"\n",
        "[extensions.stats.settings]\n\"pass\\u0077ord\" = \"hunter2hunter2\"\n",
    ] {
        let why = refusals(dir.path(), Which::Shared, body);
        assert_eq!(why.len(), 1, "{body}: {why:?}");
        assert!(why[0].contains("vault"), "{body}: {why:?}");
        let err = save(dir.path(), Which::Shared, Some(COMMENTED), body).unwrap_err();
        assert!(err.iter().any(|one| one == &why[0]), "{err:?}");
    }
    // Every spelling the project save refuses (#1304): a text that parses is refused for the
    // secret it spells, and one that does not is refused as no TOML.
    for shape in crate::secretshape::escaped::shapes() {
        let body = &shape.text;
        let err = save(dir.path(), Which::Shared, Some(COMMENTED), body).unwrap_err();
        if body.parse::<toml::Table>().is_ok() {
            let named = secret_refusal(Which::Shared, shape.kind);
            assert!(err.contains(&named), "{body}: {err:?}");
        }
    }
    assert_eq!(
        fs::read_to_string(dir.path().join("charter.toml")).unwrap(),
        COMMENTED
    );
}

#[test]
fn a_file_with_escapes_and_no_secret_is_not_refused_for_one() {
    let dir = plane(COMMENTED);
    for (_, body) in crate::secretshape::escaped::clean() {
        let why = refusals(dir.path(), Which::Shared, &body);
        assert!(
            !why.iter().any(|one| one.contains("holds a secret")),
            "{body}: {why:?}"
        );
    }
}

/// #1340: a refusal Settings shows names the files the project uses, under their new names too.
#[test]
fn a_refusal_names_the_files_the_project_uses() {
    let said =
        "sandbox.egress in charter.toml names x; sandbox.mode in charter.local.toml is not read";
    assert_eq!(
        named_as(said, "purlis.toml", "purlis.local.toml"),
        "sandbox.egress in purlis.toml names x; sandbox.mode in purlis.local.toml is not read"
    );
    assert_eq!(named_as(said, "charter.toml", "charter.local.toml"), said);
}

/// #1340: a name a person wrote that only holds a file's name is quoted as they wrote it.
#[test]
fn a_host_or_path_that_holds_a_files_name_is_left_as_written() {
    let said = "sandbox.hosts in charter.toml names charter.toml.example.com, and \
                /opt/mycharter.toml and x-charter.local.toml are not read. See charter.toml.";
    assert_eq!(
        named_as(said, "purlis.toml", "purlis.local.toml"),
        "sandbox.hosts in purlis.toml names charter.toml.example.com, and \
         /opt/mycharter.toml and x-charter.local.toml are not read. See purlis.toml."
    );
    assert_eq!(
        named_as(
            "/home/dev/plane/charter.toml: unreadable",
            "purlis.toml",
            "purlis.local.toml"
        ),
        "/home/dev/plane/purlis.toml: unreadable"
    );
}

/// **Each standing refusal says the key it is about** (#1292): the window links a refusal to its
/// setting by this, never by reading the sentence. An extension id with dots in it is one step
/// of the key, which no reading of `extensions.my.ext.enabled in …` could tell.
#[test]
fn a_standing_refusal_carries_the_key_it_is_about_with_a_dotted_extension_id_as_one_step() {
    let dir = tempfile::tempdir().unwrap();
    let shared = "schema = 1\n\n[extensions.\"my.ext\"]\nenabled = \"yes\"\n";
    let refusals = standing(dir.path(), Which::Shared, shared);
    let about: Vec<_> = refusals
        .iter()
        .filter(|one| one.why.contains("is not true or false"))
        .collect();
    assert_eq!(about.len(), 1, "{refusals:?}");
    assert_eq!(
        about[0].key,
        Some(vec![
            "extensions".to_owned(),
            "my.ext".to_owned(),
            "enabled".to_owned()
        ])
    );
}

/// A `[table] key = …` sentence is about that key, and a sentence about the whole file is about
/// none (#1292).
#[test]
fn a_tables_default_is_its_key_and_a_whole_file_refusal_has_none() {
    let dir = tempfile::tempdir().unwrap();
    let shared = "schema = 1\n\n[persona]\ndefault = \"ghost\"\n";
    let refusals = standing(dir.path(), Which::Shared, shared);
    let ghost = refusals
        .iter()
        .find(|one| one.why.starts_with("[persona] default = \"ghost\""))
        .unwrap_or_else(|| panic!("{refusals:?}"));
    assert_eq!(
        ghost.key,
        Some(vec!["persona".to_owned(), "default".to_owned()])
    );

    let broken = standing(dir.path(), Which::Shared, "[memory\nshare = 1\n");
    assert_eq!(broken.len(), 1, "{broken:?}");
    assert_eq!(broken[0].key, None);
}

/// The refusal whose sentence holds `said`, among `refusals`, and the key it carries.
fn key_of(refusals: &[Refusal], said: &str) -> Option<Vec<String>> {
    let found: Vec<_> = refusals
        .iter()
        .filter(|one| one.why.contains(said))
        .collect();
    assert_eq!(found.len(), 1, "{said:?} in {refusals:#?}");
    found[0].key.clone()
}

fn keys(k: &[&str]) -> Option<Vec<String>> {
    Some(k.iter().map(|s| (*s).to_owned()).collect())
}

/// **Every reader of the committed file hands its key over** (#1292): harness plugins, the
/// theme, the sandbox, the dispatch limits, the plane's saves, and the doctor's findings, each
/// with the key its sentence is about, so none is read back from the sentence.
#[test]
fn each_reader_of_the_committed_file_gives_the_key_of_its_refusal() {
    let dir = tempfile::tempdir().unwrap();
    let shared = "schema = 1\n\
                  [harness]\ndefault = \"ghost\"\n\
                  [harness.mine]\nkind = \"claude\"\ncommand = [\"claude\"]\n\
                  [harness_plugins.nope]\n\
                  [harness_plugins.codex]\n\"one\" = 3\n\
                  [theme]\nuse = 3\nfoo = 1\n\
                  [sandbox]\nmode = \"off\"\negress = \"web\"\n\
                  [sandbox.personas.ghost]\nhosts = []\n\
                  [dispatch]\ndepth = \"deep\"\n\
                  [dispatch.personas.devops]\nbogus = 1\n\
                  [dispatch.grants]\nsteward = 3\n\
                  [plane]\nmode = \"sideways\"\nworktrees = \"/far/away\"\n\
                  [repos.app]\nsign = 1\n\
                  [[forge]]\nkind = \"nope\"\n";
    let refusals = standing(dir.path(), Which::Shared, shared);
    let at = |said: &str| key_of(&refusals, said);
    assert_eq!(
        at("[harness] default = \"ghost\""),
        keys(&["harness", "default"])
    );
    assert_eq!(at("[harness.mine] is in"), keys(&["harness", "mine"]));
    assert_eq!(
        at("[harness_plugins.nope]"),
        keys(&["harness_plugins", "nope"])
    );
    assert_eq!(
        at("harness_plugins.codex.one in"),
        keys(&["harness_plugins", "codex", "one"])
    );
    assert_eq!(at("theme.use in"), keys(&["theme", "use"]));
    assert_eq!(at("theme.foo in"), keys(&["theme", "foo"]));
    assert_eq!(at("sandbox.mode in"), keys(&["sandbox", "mode"]));
    assert_eq!(at("sandbox.egress in"), keys(&["sandbox", "egress"]));
    assert_eq!(
        at("sandbox.personas.ghost in"),
        keys(&["sandbox", "personas", "ghost"])
    );
    assert_eq!(at("dispatch.depth in"), keys(&["dispatch", "depth"]));
    assert_eq!(
        at("dispatch.personas.devops.bogus in"),
        keys(&["dispatch", "personas", "devops", "bogus"])
    );
    assert_eq!(at("dispatch.grants.steward"), keys(&["dispatch", "grants"]));
    assert_eq!(at("plane.mode in"), keys(&["plane", "mode"]));
    assert_eq!(
        at("[plane] worktrees points outside"),
        keys(&["plane", "worktrees"])
    );
    assert_eq!(at("repos.app.sign in"), keys(&["repos", "app", "sign"]));
    assert_eq!(at("[[forge]] block(s) failed"), keys(&["forge"]));
}

/// **This machine's file hands its keys over too** (#1292), and **a refused profile gives its
/// own table**, so the window links it to that profile's page (`project.harness.profile.<name>`).
#[test]
fn each_reader_of_the_local_file_gives_the_key_of_its_refusal_and_a_profile_its_table() {
    let dir = tempfile::tempdir().unwrap();
    let local = "[harness.bad]\nkind = \"nope\"\ncommand = [\"x\"]\n\
                 [harness.mine.sub]\nx = 1\n\
                 [chat_env]\npass = 3\nother = 1\n\
                 [sandbox]\nmode = \"on\"\n\
                 [dispatch.profiles]\ndevops = [\"work\"]\n\
                 [bogus]\nx = 1\n";
    let refusals = standing(dir.path(), Which::Local, local);
    let at = |said: &str| key_of(&refusals, said);
    assert_eq!(at("'bad' has kind"), keys(&["harness", "bad", "kind"]));
    assert_eq!(
        at("[harness.mine] holds a table sub"),
        keys(&["harness", "mine", "sub"])
    );
    assert_eq!(at("[chat_env].pass in"), keys(&["chat_env", "pass"]));
    assert_eq!(at("[chat_env].other in"), keys(&["chat_env", "other"]));
    assert_eq!(at("sandbox.mode in"), keys(&["sandbox", "mode"]));
    assert_eq!(at("dispatch.profiles in"), keys(&["dispatch", "profiles"]));
    assert_eq!(at("[bogus] in"), keys(&["bogus"]));
}

/// A profile refused for running purlis itself gives the key it is about: its command (ST-4).
#[test]
fn a_profile_that_runs_purlis_gives_its_command() {
    let dir = tempfile::tempdir().unwrap();
    let local = "[harness.mine]\nkind = \"claude\"\ncommand = [\"purlis\"]\n";
    let refusals = standing(dir.path(), Which::Local, local);
    assert_eq!(
        key_of(&refusals, "runs purlis itself"),
        keys(&["harness", "mine", "command"])
    );
}
