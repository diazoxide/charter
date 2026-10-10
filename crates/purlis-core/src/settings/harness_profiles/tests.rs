//! The harness profiles collection on a scratch plane: add, read back, field refusals from the
//! profile rules, remove, rename while nothing uses a profile, and the refusals that name who
//! does — through [`add`], [`remove`] and [`rename`] alone.

use std::fs;
use std::path::Path;

use super::*;
use crate::doctor::SettingsGroup;
use crate::names::{LOCAL_SETTINGS, PLANE_MANIFEST};
use crate::profiletrust::{self, Approval};
use crate::settings::collection::{FieldRefusal, Refusal};

const LOCAL: &str = "\
# This machine's profiles.
[harness.claude-work]
kind = \"claude\"  # the work account
command = [\"claude\"]
env = { CLAUDE_CONFIG_DIR = \"~/.claude-work\" }

[plane]
mode = \"off\"
";

/// The spelling a plane on an older build has its files under, and the purlis one.
const OLD: bool = true;
const PURLIS: bool = false;

/// A scratch plane in a git repository that ignores its local file, with `local` as that file
/// (none for `None`) and `shared` as its manifest, both under the old names or the purlis ones.
fn plane_named(old: bool, shared: &str, local: Option<&str>) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let (manifest, settings) = if old {
        (PLANE_MANIFEST.newest_old(), LOCAL_SETTINGS.newest_old())
    } else {
        (PLANE_MANIFEST.write, LOCAL_SETTINGS.write)
    };
    fs::write(dir.path().join(manifest), shared).unwrap();
    if let Some(local) = local {
        fs::write(dir.path().join(settings), local).unwrap();
    }
    crate::testgit::run(dir.path(), &["init", "-q"]);
    fs::write(
        dir.path().join(".gitignore"),
        format!(
            "/{}\n/{}\n",
            LOCAL_SETTINGS.newest_old(),
            LOCAL_SETTINGS.write
        ),
    )
    .unwrap();
    dir
}

fn plane(local: Option<&str>) -> tempfile::TempDir {
    plane_named(OLD, "schema = 1\n", local)
}

/// The local file as it stands, wherever the plane has it.
fn local(root: &Path) -> String {
    fs::read_to_string(crate::names::local_settings(root)).unwrap_or_default()
}

fn entry(name: &str, kind: &str, command: &[&str]) -> Entry {
    Entry {
        name: name.to_owned(),
        kind: kind.to_owned(),
        command: command.iter().map(|one| (*one).to_owned()).collect(),
    }
}

/// The identity of the profile `name` in `text`, as the window was handed it.
fn id(text: &str, name: &str) -> String {
    listed(text)
        .into_iter()
        .find(|one| one.label == name)
        .map(|one| one.id)
        .unwrap_or_else(|| panic!("{name} is listed"))
}

fn fields(refusal: &Refusal) -> Vec<&str> {
    refusal.fields.iter().map(|one| one.field).collect()
}

#[test]
fn every_profile_table_is_listed_by_name_and_the_default_is_not() {
    let text = "[harness]\ndefault = \"claude-work\"\n\n[harness.claude-work]\nkind = \"claude\"\n\
                command = [\"claude\"]\n\n[harness.cx]\nkind = \"codex\"\ncommand = [\"codex\", \"--x\"]\n";
    let all = listed(text);
    let names: Vec<&str> = all.iter().map(|one| one.label.as_str()).collect();
    assert_eq!(names, ["claude-work", "cx"]);
    assert_eq!(
        all[1].keys,
        [Step::Key("harness".into()), Step::Key("cx".into())]
    );
    assert_eq!(
        all[1].values,
        [
            ("name", "cx".to_owned()),
            ("kind", "codex".to_owned()),
            ("command", "codex\n--x".to_owned()),
        ]
    );
    assert_ne!(all[0].id, all[1].id);
    assert!(listed("not = [toml").is_empty());
}

#[test]
fn an_added_profile_is_a_new_table_and_the_rest_of_the_file_is_kept() {
    let dir = plane(Some(LOCAL));
    let added = add(
        dir.path(),
        Some(LOCAL),
        &entry(
            " codex-alt ",
            "codex",
            &["~/bin/codex", "", "--profile=alt"],
        ),
    )
    .unwrap();
    let now = local(dir.path());
    // After the last profile, before the table that followed it.
    assert_eq!(
        now,
        LOCAL.replace(
            "\n[plane]",
            "\n[harness.codex-alt]\nkind = \"codex\"\ncommand = [\"~/bin/codex\", \"--profile=alt\"]\n\n[plane]"
        )
    );
    let cfg: toml::Table = now.parse().unwrap();
    let table = &cfg["harness"]["codex-alt"];
    assert_eq!(table["kind"].as_str(), Some("codex"));
    assert_eq!(
        table["command"].as_array().unwrap(),
        &vec![
            toml::Value::from("~/bin/codex"),
            toml::Value::from("--profile=alt")
        ]
    );
    assert_eq!(added, id(&now, "codex-alt"));
    // The loader reads it as a profile of this machine.
    let set = crate::profiles::derive(dir.path());
    assert!(set.get("codex-alt").is_some(), "{:?}", set.refused);
}

#[test]
fn the_first_profile_creates_the_local_file() {
    let dir = plane(None);
    add(dir.path(), None, &entry("work", "claude", &["claude"])).unwrap();
    assert!(local(dir.path()).contains("[harness.work]"));
}

#[test]
fn a_write_lands_in_the_file_the_plane_has_under_its_purlis_name() {
    let dir = plane_named(PURLIS, "schema = 1\n", Some(LOCAL));
    add(
        dir.path(),
        Some(LOCAL),
        &entry("work", "claude", &["claude"]),
    )
    .unwrap();
    let there = fs::read_to_string(dir.path().join(LOCAL_SETTINGS.write)).unwrap();
    assert!(there.contains("[harness.work]"));
    assert!(!dir.path().join(LOCAL_SETTINGS.newest_old()).exists());
}

#[test]
fn each_field_is_refused_in_the_profile_rules_own_words_and_nothing_is_written() {
    let dir = plane(Some(LOCAL));
    let refused = |e: Entry| add(dir.path(), Some(LOCAL), &e).unwrap_err();

    let bad = refused(entry("a.b", "svn", &[]));
    assert_eq!(fields(&bad), ["name", "kind", "command"]);
    assert!(bad.fields[0].why.contains("letters, digits"), "{bad:?}");
    assert!(
        bad.fields[1]
            .why
            .contains("one of: claude, opencode, codex")
    );
    assert!(bad.fields[2].why.contains("never a shell string"));

    assert_eq!(fields(&refused(entry("", "claude", &["claude"]))), ["name"]);
    let default = refused(entry("default", "claude", &["claude"]));
    assert!(default.fields[0].why.contains("cannot be named 'default'"));
    let word = refused(entry("doctor", "claude", &["claude"]));
    assert!(word.fields[0].why.contains("`purlis doctor`"), "{word:?}");
    let itself = refused(entry("me", "claude", &["purlis", "harness"]));
    assert_eq!(fields(&itself), ["command"]);
    assert!(itself.fields[0].why.contains("runs purlis itself"));

    let twice = refused(entry("claude-work", "claude", &["claude"]));
    assert_eq!(fields(&twice), ["name"]);
    assert!(twice.fields[0].why.contains("already"), "{twice:?}");

    assert_eq!(local(dir.path()), LOCAL);
}

#[test]
fn a_pasted_secret_is_refused_under_its_field_and_points_to_the_vault() {
    let dir = plane(Some(LOCAL));
    let token = format!("ghp_{}", "a1B2c3D4e5F6g7H8i9J0k1L2m3N4o5P6q7R8");
    let refusal = add(
        dir.path(),
        Some(LOCAL),
        &entry("work", "claude", &["claude", "--token", &token]),
    )
    .unwrap_err();
    assert_eq!(fields(&refusal), ["command"]);
    let why = &refusal.fields[0].why;
    assert!(why.contains("vault"), "{why}");
    assert!(!why.contains(&token), "a secret is never said back: {why}");
    assert_eq!(local(dir.path()), LOCAL);
}

#[test]
fn a_secret_pasted_through_its_escapes_is_refused_as_the_save_would_refuse_it() {
    let dir = plane(Some(LOCAL));
    let tail = &crate::secretshape::escaped::token()[1..];
    let spelled = format!("\\u0067{tail}");
    let refusal = add(
        dir.path(),
        Some(LOCAL),
        &entry("work", "claude", &["claude", "--token", &spelled]),
    )
    .unwrap_err();
    assert_eq!(fields(&refusal), ["command"]);
    assert!(!refusal.fields[0].why.contains(tail));
    assert_eq!(local(dir.path()), LOCAL);
}

#[test]
fn a_file_changed_since_it_was_read_is_not_written() {
    let dir = plane(Some(LOCAL));
    let refusal = add(
        dir.path(),
        Some("[harness]\n"),
        &entry("work", "claude", &["claude"]),
    )
    .unwrap_err();
    assert!(!refusal.file.is_empty());
    assert_eq!(local(dir.path()), LOCAL);
}

#[test]
fn a_removed_profile_leaves_the_file_and_answers_what_would_add_it_again() {
    let dir = plane(Some(LOCAL));
    let took = remove(dir.path(), Some(LOCAL), &id(LOCAL, "claude-work")).unwrap();
    assert_eq!(took, entry("claude-work", "claude", &["claude"]));
    let now = local(dir.path());
    assert!(!now.contains("claude-work"), "{now}");
    assert!(now.contains("[plane]"));
}

#[test]
fn a_profile_the_default_harness_names_is_not_removed_and_each_user_is_named() {
    let shared = "schema = 1\n[harness]\ndefault = \"claude-work\"\n";
    let text = format!("[harness]\ndefault = \"claude-work\"\n\n{LOCAL}");
    let dir = plane_named(OLD, shared, Some(&text));
    let refusal = remove(dir.path(), Some(&text), &id(&text, "claude-work")).unwrap_err();
    assert_eq!(refusal.referrers.len(), 2, "{refusal:?}");
    for (one, file) in refusal
        .referrers
        .iter()
        .zip([PLANE_MANIFEST.newest_old(), LOCAL_SETTINGS.newest_old()])
    {
        assert!(one.what.contains("[harness] default"), "{one:?}");
        assert!(one.what.contains(file), "{one:?}");
        assert_eq!(one.group, Some(SettingsGroup::Harness));
    }
    assert_eq!(local(dir.path()), text);
}

#[test]
fn a_profile_that_replaces_a_built_in_is_removed_while_the_default_names_it() {
    // The built-in of that name still stands once the table is gone, so the default still names
    // a profile: nothing is left dangling.
    let text = "[harness]\ndefault = \"claude\"\n\n[harness.claude]\nkind = \"claude\"\n\
                command = [\"/opt/claude\"]\n";
    let dir = plane(Some(text));
    remove(dir.path(), Some(text), &id(text, "claude")).unwrap();
    assert!(!local(dir.path()).contains("[harness.claude]"));
}

#[test]
fn a_remove_sent_for_a_profile_no_longer_as_shown_is_refused() {
    let dir = plane(Some(LOCAL));
    let stale = id(
        "[harness.claude-work]\nkind = \"codex\"\ncommand = [\"x\"]\n",
        "claude-work",
    );
    let refusal = remove(dir.path(), Some(LOCAL), &stale).unwrap_err();
    assert!(!refusal.file.is_empty());
    assert_eq!(local(dir.path()), LOCAL);
}

#[test]
fn an_unused_profile_is_renamed_in_place_with_its_keys_and_comment() {
    let dir = plane(Some(LOCAL));
    let renamed = rename(dir.path(), Some(LOCAL), &id(LOCAL, "claude-work"), "work").unwrap();
    let now = local(dir.path());
    assert_eq!(
        now,
        LOCAL.replace("[harness.claude-work]", "[harness.work]")
    );
    assert_eq!(renamed, id(&now, "work"));
}

#[test]
fn a_profile_something_uses_is_not_renamed_and_each_user_is_named() {
    let text = format!("[harness]\ndefault = \"claude-work\"\n\n{LOCAL}");
    let dir = plane(Some(&text));
    let refusal = rename(dir.path(), Some(&text), &id(&text, "claude-work"), "work").unwrap_err();
    assert_eq!(refusal.referrers.len(), 1, "{refusal:?}");
    assert_eq!(refusal.referrers[0].group, Some(SettingsGroup::Harness));
    assert_eq!(local(dir.path()), text);
}

#[test]
fn a_new_name_is_refused_by_the_same_rules_as_an_added_one() {
    let text = format!("{LOCAL}\n[harness.other]\nkind = \"codex\"\ncommand = [\"codex\"]\n");
    let dir = plane(Some(&text));
    let at = id(&text, "claude-work");
    for (to, says) in [
        ("a.b", "letters, digits"),
        ("default", "'default'"),
        ("status", "`purlis status`"),
        ("other", "already"),
        ("claude-work", "already"),
    ] {
        let refusal = rename(dir.path(), Some(&text), &at, to).unwrap_err();
        assert_eq!(fields(&refusal), ["name"], "{to}");
        assert!(refusal.fields[0].why.contains(says), "{to}: {refusal:?}");
    }
    assert_eq!(local(dir.path()), text);
}

#[test]
fn an_added_profile_still_asks_before_its_first_run_and_a_renamed_one_asks_again() {
    let dir = plane(None);
    add(dir.path(), None, &entry("work", "claude", &["claude"])).unwrap();
    let profile = |name: &str| {
        crate::profiles::derive(dir.path())
            .get(name)
            .cloned()
            .unwrap()
    };
    let work = profile("work");
    assert_eq!(
        profiletrust::approval_needed(dir.path(), &work),
        Some(Approval::New)
    );
    profiletrust::approve(dir.path(), &work, &profiletrust::shown(dir.path(), &work)).unwrap();
    assert_eq!(profiletrust::approval_needed(dir.path(), &work), None);

    let text = local(dir.path());
    rename(dir.path(), Some(&text), &id(&text, "work"), "job").unwrap();
    assert_eq!(
        profiletrust::approval_needed(dir.path(), &profile("job")),
        Some(Approval::New),
        "a name nothing approved asks, whatever it runs"
    );
}

/// A JWT: secret-shaped, and not a name the rules accept, so a name refusal would quote it.
fn jwt() -> String {
    format!(
        "eyJhbGciOiJIUzI1NiJ9.{}.{}",
        "eyJzdWIiOiIxMjM0NTY3ODkwIn0", "dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U"
    )
}

#[test]
fn a_secret_pasted_as_the_name_or_the_kind_is_refused_by_its_kind_and_never_said_back() {
    let dir = plane(Some(LOCAL));
    let token = format!("ghp_{}", "a1B2c3D4e5F6g7H8i9J0k1L2m3N4o5P6q7R8");
    for (e, field, secret) in [
        (entry(&jwt(), "claude", &["claude"]), "name", jwt()),
        (entry(&token, "claude", &["claude"]), "name", token.clone()),
        (entry("work", &token, &["claude"]), "kind", token.clone()),
    ] {
        let refusal = add(dir.path(), Some(LOCAL), &e).unwrap_err();
        let said: Vec<&FieldRefusal> = refusal
            .fields
            .iter()
            .filter(|one| one.field == field)
            .collect();
        assert_eq!(said.len(), 1, "{field}: {refusal:?}");
        assert!(said[0].why.contains("vault"), "{field}: {refusal:?}");
        assert!(
            !format!("{refusal:?}").contains(&secret),
            "{field}: a secret is never said back: {refusal:?}"
        );
    }
    assert_eq!(local(dir.path()), LOCAL);
}

#[test]
fn a_secret_pasted_as_a_new_name_is_refused_by_its_kind_and_never_said_back() {
    let dir = plane(Some(LOCAL));
    let refusal = rename(dir.path(), Some(LOCAL), &id(LOCAL, "claude-work"), &jwt()).unwrap_err();
    assert_eq!(fields(&refusal), ["name"]);
    assert!(refusal.fields[0].why.contains("vault"));
    assert!(!format!("{refusal:?}").contains(&jwt()));
    assert_eq!(local(dir.path()), LOCAL);
}

#[test]
fn an_inline_profile_keeps_its_place_on_a_rename_and_on_the_rename_back() {
    let text = "[harness]\ndefault = \"claude\"\nfirst = { kind = \"claude\", command = [\"claude\"] }\n\
                last = { kind = \"codex\", command = [\"codex\"] }\n";
    let dir = plane(Some(text));
    let renamed = rename(dir.path(), Some(text), &id(text, "first"), "work").unwrap();
    let now = local(dir.path());
    assert_eq!(now, text.replace("first = ", "work = "));
    rename(dir.path(), Some(&now), &renamed, "first").unwrap();
    assert_eq!(local(dir.path()), text);
}

#[test]
fn a_quoted_name_keeps_its_spelling_on_a_rename_and_on_the_rename_back() {
    let text = "[harness.\"work\"]\nkind = \"claude\"\ncommand = [\"claude\"]\n\n\
                [harness.'alt']  # literal\nkind = \"codex\"\ncommand = [\"codex\"]\n";
    let dir = plane(Some(text));
    let renamed = rename(dir.path(), Some(text), &id(text, "work"), "job").unwrap();
    let now = local(dir.path());
    assert_eq!(now, text.replace("\"work\"", "\"job\""));
    rename(dir.path(), Some(&now), &renamed, "work").unwrap();
    let now = local(dir.path());
    assert_eq!(now, text);
    let renamed = rename(dir.path(), Some(&now), &id(&now, "alt"), "other").unwrap();
    assert_eq!(local(dir.path()), text.replace("'alt'", "'other'"));
    let now = local(dir.path());
    rename(dir.path(), Some(&now), &renamed, "alt").unwrap();
    assert_eq!(local(dir.path()), text);
}

#[test]
fn a_refused_rename_says_which_users_a_rename_everywhere_changes_too() {
    // The refusal is the preview (#1380): each user, and whether renaming everywhere takes it
    // along — a default in this machine's own file does; the project's, every teammate's, not.
    let shared = "schema = 1\n[harness]\ndefault = \"claude-work\"\n";
    let text = format!("[harness]\ndefault = \"claude-work\"\n\n{LOCAL}");
    let dir = plane_named(OLD, shared, Some(&text));
    let refusal = rename(dir.path(), Some(&text), &id(&text, "claude-work"), "work").unwrap_err();
    let follows: Vec<bool> = refusal.referrers.iter().map(|one| one.follows).collect();
    assert_eq!(follows, [false, true], "{refusal:?}");
    assert_eq!(local(dir.path()), text);
}

#[test]
fn a_rename_everywhere_renames_the_profile_and_this_files_default_in_one_write() {
    let text = format!("[harness]\ndefault = 'claude-work'  # mine\n\n{LOCAL}");
    let dir = plane(Some(&text));
    let renamed =
        rename_everywhere(dir.path(), Some(&text), &id(&text, "claude-work"), "work").unwrap();
    let now = local(dir.path());
    assert_eq!(
        now,
        text.replace("'claude-work'", "'work'")
            .replace("[harness.claude-work]", "[harness.work]")
    );
    assert_eq!(renamed, id(&now, "work"));
    // Its Undo, the rename back everywhere, gives the text it started from.
    rename_everywhere(dir.path(), Some(&now), &renamed, "claude-work").unwrap();
    assert_eq!(local(dir.path()), text);
}

#[test]
fn a_rename_everywhere_keeps_a_triple_quoted_default_as_written_and_undoes_to_it() {
    for written in ["'''claude-work'''", "\"\"\"claude-work\"\"\""] {
        let text = format!("[harness]\ndefault = {written} # mine\n\n{LOCAL}");
        let dir = plane(Some(&text));
        let renamed =
            rename_everywhere(dir.path(), Some(&text), &id(&text, "claude-work"), "work").unwrap();
        let now = local(dir.path());
        assert_eq!(
            now,
            text.replace(written, &written.replace("claude-work", "work"))
                .replace("[harness.claude-work]", "[harness.work]")
        );
        rename_everywhere(dir.path(), Some(&now), &renamed, "claude-work").unwrap();
        assert_eq!(local(dir.path()), text);
    }
}

#[test]
fn a_rename_everywhere_of_a_profile_nothing_uses_is_a_rename() {
    let dir = plane(Some(LOCAL));
    rename_everywhere(dir.path(), Some(LOCAL), &id(LOCAL, "claude-work"), "work").unwrap();
    assert_eq!(
        local(dir.path()),
        LOCAL.replace("[harness.claude-work]", "[harness.work]")
    );
}

#[test]
fn a_rename_everywhere_the_projects_default_names_writes_nothing_and_names_it() {
    // charter.toml is every teammate's: a rename on this machine never rewrites it, so the
    // profile stays as it was, this file's default with it — nothing half-renamed.
    let shared = "schema = 1\n[harness]\ndefault = \"claude-work\"\n";
    let text = format!("[harness]\ndefault = \"claude-work\"\n\n{LOCAL}");
    let dir = plane_named(OLD, shared, Some(&text));
    let refusal =
        rename_everywhere(dir.path(), Some(&text), &id(&text, "claude-work"), "work").unwrap_err();
    assert_eq!(refusal.referrers.len(), 2, "{refusal:?}");
    assert!(!refusal.referrers[0].follows);
    assert!(
        refusal.referrers[0]
            .what
            .contains(PLANE_MANIFEST.newest_old())
    );
    assert_eq!(local(dir.path()), text);
}

#[test]
fn a_rename_everywhere_still_refuses_a_name_the_rules_refuse() {
    let text = format!("[harness]\ndefault = \"claude-work\"\n\n{LOCAL}");
    let dir = plane(Some(&text));
    let refusal =
        rename_everywhere(dir.path(), Some(&text), &id(&text, "claude-work"), "a.b").unwrap_err();
    assert_eq!(fields(&refusal), ["name"]);
    assert_eq!(local(dir.path()), text);
}

#[test]
fn a_default_that_is_not_the_profiles_plain_name_refuses_the_whole_write() {
    // D-1380-3: what cannot be rewritten as a name refuses the write, never half of it.
    let dir = plane(None);
    for text in [
        "[harness]\ndefault = 3\n",
        "[harness]\ndefault = \"other\"\n",
        "[harness]\n",
    ] {
        let refusal = defaulted(dir.path(), text, "claude-work", "work").unwrap_err();
        assert!(
            refusal.file[0].contains("is not written as a name a form can change"),
            "{refusal:?}"
        );
    }
}

/// Writes the persona `name` with `front` as its definition's frontmatter.
fn persona(root: &Path, name: &str, front: &str) {
    let dir = root.join("personas").join(name);
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("persona.md"),
        format!("---\n{front}\n---\nA role.\n"),
    )
    .unwrap();
}

/// A plain rename, a rename everywhere and a remove of `claude-work` in `text`, each refused:
/// what each refusal names, and the file left as it was.
fn every_change_refused(root: &Path, text: &str) -> Vec<Vec<Referrer>> {
    let at = id(text, "claude-work");
    let refusals = [
        rename(root, Some(text), &at, "work").unwrap_err(),
        rename_everywhere(root, Some(text), &at, "work").unwrap_err(),
        remove(root, Some(text), &at).unwrap_err(),
    ];
    assert_eq!(local(root), text, "nothing is written");
    refusals.into_iter().map(|one| one.referrers).collect()
}

#[test]
fn a_persona_that_names_the_profile_refuses_every_rename_and_the_remove_naming_it() {
    // #1380: a persona's `profile:` is its own file's, which no rename here rewrites; #1241:
    // it is changed in the persona's tab, which the referrer says.
    let dir = plane(Some(LOCAL));
    persona(dir.path(), "devops", "name: devops\nprofile: claude-work");
    persona(dir.path(), "writer", "name: writer\nprofile: codex");
    for referrers in every_change_refused(dir.path(), LOCAL) {
        assert_eq!(
            referrers,
            [Referrer {
                what: "profile: claude-work in personas/devops/persona.md starts the chats \
                       dispatched to devops on it."
                    .to_owned(),
                group: None,
                follows: false,
                elsewhere: Some(Elsewhere::Persona("devops".to_owned())),
            }]
        );
    }
}

#[test]
fn a_dispatch_list_that_names_the_profile_refuses_every_rename_and_the_remove_naming_it() {
    // #1380: `[dispatch.profiles]` is in the project's file, every teammate's.
    let shared = "schema = 1\n[dispatch.profiles]\ndevops = [\"codex\", \"claude-work\"]\n\
                  writer = [\"codex\"]\n";
    let dir = plane_named(OLD, shared, Some(LOCAL));
    for referrers in every_change_refused(dir.path(), LOCAL) {
        assert_eq!(
            referrers,
            [Referrer {
                what: format!(
                    "[dispatch.profiles] devops in {} lets chats dispatched to devops start \
                     on it.",
                    PLANE_MANIFEST.newest_old()
                ),
                group: None,
                follows: false,
                elsewhere: None,
            }]
        );
    }
}

#[test]
fn a_profile_no_persona_and_no_dispatch_list_names_is_renamed() {
    let shared = "schema = 1\n[dispatch.profiles]\nwriter = [\"codex\"]\n";
    let dir = plane_named(OLD, shared, Some(LOCAL));
    persona(dir.path(), "writer", "name: writer\nprofile: codex");
    rename(dir.path(), Some(LOCAL), &id(LOCAL, "claude-work"), "work").unwrap();
    assert_eq!(
        local(dir.path()),
        LOCAL.replace("[harness.claude-work]", "[harness.work]")
    );
}
