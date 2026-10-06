//! The forges collection on a scratch plane: add, read back, field refusals, remove, and the
//! refusal while something needs the block — through [`add`] and [`remove`] alone.

use std::fs;
use std::path::Path;

use super::*;
use crate::doctor::SettingsGroup;
use crate::settings::collection::Refusal;

const SHARED: &str = "\
# The plane's own settings.
schema = 1

[[forge]]
kind = \"github\"  # the team's org
owner = \"acme\"

[memory]
share = \"local\"
";

/// A scratch plane: `shared` as its `charter.toml`, in a git repository whose `.gitignore`
/// carries the line `charter init` writes.
fn plane(shared: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("charter.toml"), shared).unwrap();
    crate::testgit::run(dir.path(), &["init", "-q"]);
    fs::write(dir.path().join(".gitignore"), "/charter.local.toml\n").unwrap();
    dir
}

fn shared(root: &Path) -> String {
    fs::read_to_string(root.join("charter.toml")).unwrap()
}

fn entry(kind: &str, owner: &str, host: &str, exclude: &[&str]) -> Entry {
    Entry {
        kind: kind.to_owned(),
        owner: owner.to_owned(),
        host: host.to_owned(),
        exclude: exclude.iter().map(|one| (*one).to_owned()).collect(),
    }
}

/// The catalogue `discover` writes, with one repo per `(name, ssh_url)`.
fn catalogue(root: &Path, repos: &[(&str, &str)]) {
    let repos: Vec<serde_json::Value> = repos
        .iter()
        .map(|(name, url)| serde_json::json!({"name": name, "ssh_url": url, "forge": "gitlab"}))
        .collect();
    fs::create_dir_all(root.join("inventory")).unwrap();
    fs::write(
        root.join("inventory/repos.json"),
        serde_json::json!({"group": "acme", "count": repos.len(), "repos": repos}).to_string(),
    )
    .unwrap();
}

/// The identity of block `n` of `text`, as the window was handed it.
fn id(text: &str, n: usize) -> String {
    listed(text)[n].id.clone()
}

fn fields(refusal: &Refusal) -> Vec<&str> {
    refusal.fields.iter().map(|one| one.field).collect()
}

#[test]
fn an_added_forge_is_a_new_block_at_the_end_of_the_list_and_the_rest_of_the_file_is_kept() {
    let dir = plane(SHARED);
    add(
        dir.path(),
        Some(SHARED),
        &entry(
            "gitlab",
            "platform",
            "GitLab.Acme.dev",
            &["sandbox", "", "sandbox"],
        ),
    )
    .unwrap();

    let written = shared(dir.path());
    assert!(
        written.starts_with("# The plane's own settings.\nschema = 1\n"),
        "{written}"
    );
    assert!(
        written.contains("kind = \"github\"  # the team's org"),
        "{written}"
    );
    let cfg: toml::Table = written.parse().unwrap();
    let blocks = cfg["forge"].as_array().unwrap();
    assert_eq!(blocks.len(), 2, "{written}");
    let added = blocks[1].as_table().unwrap();
    assert_eq!(added["kind"].as_str(), Some("gitlab"));
    assert_eq!(added["owner"].as_str(), Some("platform"));
    // A host is matched against a remote's host, which is read lowercased.
    assert_eq!(added["host"].as_str(), Some("gitlab.acme.dev"));
    assert_eq!(
        added["exclude"].as_array().unwrap(),
        &vec![toml::Value::String("sandbox".into())]
    );
    // The two blocks are written together, before the table that followed them.
    assert!(
        written.find("platform").unwrap() < written.find("[memory]").unwrap(),
        "{written}"
    );
}

#[test]
fn a_forge_with_only_a_kind_writes_only_the_kind() {
    let dir = plane("schema = 1\n");
    add(
        dir.path(),
        Some("schema = 1\n"),
        &entry("github", " ", "", &[]),
    )
    .unwrap();
    assert_eq!(
        shared(dir.path()),
        "schema = 1\n\n[[forge]]\nkind = \"github\"\n"
    );
}

#[test]
fn an_empty_kind_is_the_default_kind_written_out() {
    let dir = plane("schema = 1\n");
    add(
        dir.path(),
        Some("schema = 1\n"),
        &entry("", "acme", "", &[]),
    )
    .unwrap();
    let cfg: toml::Table = shared(dir.path()).parse().unwrap();
    assert_eq!(cfg["forge"][0]["kind"].as_str(), Some("gitlab"));
}

#[test]
fn a_kind_and_a_host_charter_cannot_use_are_refused_by_field_in_the_readers_words() {
    let dir = plane(SHARED);
    let refusal = add(
        dir.path(),
        Some(SHARED),
        &entry("bitbucket", "acme", "https://git.acme.dev/", &[]),
    )
    .unwrap_err();
    assert_eq!(fields(&refusal), ["kind", "host"]);
    assert_eq!(
        refusal.fields[0].why,
        "unknown forge kind 'bitbucket' — known kinds: github, gitlab"
    );
    assert!(
        refusal.fields[1]
            .why
            .starts_with("host 'https://git.acme.dev/' is not a hostname"),
        "{}",
        refusal.fields[1].why
    );
    assert_eq!(shared(dir.path()), SHARED, "nothing is written");
}

#[test]
fn a_host_another_block_declares_as_another_kind_is_refused_on_the_host() {
    let dir = plane(SHARED);
    let refusal = add(
        dir.path(),
        Some(SHARED),
        &entry("gitlab", "acme", "github.com", &[]),
    )
    .unwrap_err();
    assert_eq!(fields(&refusal), ["host"]);
    assert_eq!(
        refusal.fields[0].why,
        "github.com is already a GitHub forge in [[forge]] block 1: one host is one forge"
    );
}

#[test]
fn the_same_forge_and_owner_twice_is_refused_on_the_owner() {
    let dir = plane(SHARED);
    let refusal = add(dir.path(), Some(SHARED), &entry("github", "acme", "", &[])).unwrap_err();
    assert_eq!(fields(&refusal), ["owner"]);
    assert_eq!(
        refusal.fields[0].why,
        "[[forge]] block 1 already lists acme on github.com"
    );
}

#[test]
fn a_file_changed_since_it_was_read_is_refused_as_a_whole_and_kept() {
    let dir = plane(SHARED);
    let refusal = add(
        dir.path(),
        Some("schema = 1\n"),
        &entry("gitlab", "x", "", &[]),
    )
    .unwrap_err();
    assert!(refusal.fields.is_empty());
    assert!(
        refusal.file[0].starts_with("charter.toml changed on disk since this tab read it"),
        "{:?}",
        refusal.file
    );
    assert_eq!(shared(dir.path()), SHARED);
}

#[test]
fn a_secret_in_an_owner_is_refused_as_every_save_refuses_it() {
    let dir = plane(SHARED);
    let token = "AKIAIOSFODNN7EXAMPLE";
    let refusal = add(dir.path(), Some(SHARED), &entry("gitlab", token, "", &[])).unwrap_err();
    assert!(
        refusal
            .file
            .iter()
            .any(|why| why.contains("looks like it holds a secret")),
        "{:?}",
        refusal.file
    );
    assert!(!shared(dir.path()).contains(token));
}

#[test]
fn forge_written_as_something_other_than_blocks_is_left_to_edit_as_toml() {
    let text = "schema = 1\nforge = [{ kind = \"github\" }]\n";
    let dir = plane(text);
    let refusal = add(dir.path(), Some(text), &entry("gitlab", "x", "", &[])).unwrap_err();
    assert_eq!(
        refusal.file,
        [
            "forge in charter.toml is not written as [[forge]] blocks, so a form cannot add one — \
          add it under Edit as TOML"
        ]
    );
}

#[test]
fn a_removed_forge_takes_its_block_and_nothing_else() {
    let text = "schema = 1\n\n[[forge]]\nkind = \"github\"\nowner = \"acme\"\n\n[[forge]]\nkind = \"gitlab\"\nowner = \"ops\"\n\n[memory]\nshare = \"local\"\n";
    let dir = plane(text);
    remove(dir.path(), Some(text), &id(text, 0)).unwrap();
    assert_eq!(
        shared(dir.path()),
        "schema = 1\n\n[[forge]]\nkind = \"gitlab\"\nowner = \"ops\"\n\n[memory]\nshare = \"local\"\n"
    );
}

#[test]
fn removing_the_last_forge_leaves_no_forge_key() {
    let dir = plane(SHARED);
    remove(dir.path(), Some(SHARED), &id(SHARED, 0)).unwrap();
    let written = shared(dir.path());
    assert!(!written.contains("forge"), "{written}");
    assert!(written.contains("[memory]"), "{written}");
}

#[test]
fn an_identity_the_file_it_was_read_from_does_not_hold_is_refused_and_nothing_goes() {
    let two = "schema = 1\n\n[[forge]]\nkind = \"github\"\nowner = \"acme\"\n\n[[forge]]\nkind = \"github\"\nowner = \"beta\"\n";
    let dir = plane(two);
    // Shown when acme was the first block and beta the second; acme is gone since.
    let beta = id(two, 1);
    let one = "schema = 1\n\n[[forge]]\nkind = \"github\"\nowner = \"beta\"\n";
    fs::write(dir.path().join("charter.toml"), one).unwrap();
    let first_now = id(one, 0);
    assert_ne!(beta, first_now, "a block that moved is not the same entry");
    let refusal = remove(dir.path(), Some(one), &beta).unwrap_err();
    assert!(
        refusal.file[0].starts_with("That forge is not in charter.toml as it was shown"),
        "{refusal:?}"
    );
    assert_eq!(shared(dir.path()), one);
}

#[test]
fn every_block_is_listed_with_its_identity_label_and_the_values_that_write_it_again() {
    let shown = listed(SELF_HOSTED);
    let labels: Vec<&str> = shown.iter().map(|one| one.label.as_str()).collect();
    assert_eq!(
        labels,
        [
            "Forge 1: gitlab platform at git.acme.dev",
            "Forge 2: github acme at github.com"
        ]
    );
    assert_eq!(
        shown[0].values,
        [
            ("kind", "gitlab".to_owned()),
            ("owner", "platform".to_owned()),
            ("host", "git.acme.dev".to_owned()),
            ("exclude", String::new()),
        ]
    );
    assert!(shown[0].id.starts_with("forge:0:"), "{}", shown[0].id);
}

#[test]
fn add_answers_the_new_entrys_identity_and_remove_the_entry_it_took() {
    let dir = plane(SHARED);
    let new = add(
        dir.path(),
        Some(SHARED),
        &entry("gitlab", "ops", "git.ops.dev", &["a", "b"]),
    )
    .unwrap();
    let after = shared(dir.path());
    assert_eq!(new, id(&after, 1));
    let took = remove(dir.path(), Some(&after), &new).unwrap();
    assert_eq!(took, entry("gitlab", "ops", "git.ops.dev", &["a", "b"]));
    assert_eq!(shared(dir.path()), SHARED);
}

#[test]
fn a_kinds_own_host_is_refused_as_another_kind_even_with_no_block_for_it() {
    let dir = plane("schema = 1\n");
    let refusal = add(
        dir.path(),
        Some("schema = 1\n"),
        &entry("gitlab", "acme", "github.com", &[]),
    )
    .unwrap_err();
    assert_eq!(fields(&refusal), ["host"]);
    assert_eq!(
        refusal.fields[0].why,
        "github.com is GitHub's own host: one host is one forge"
    );
}

#[test]
fn undoing_an_add_is_its_remove_and_is_refused_once_a_repo_is_catalogued_on_the_host() {
    let dir = plane(SHARED);
    let new = add(
        dir.path(),
        Some(SHARED),
        &entry("gitlab", "ops", "git.ops.dev", &[]),
    )
    .unwrap();
    let after = shared(dir.path());
    // Outside the window: discover lists a repo on the new forge.
    catalogue(dir.path(), &[("tools", "git@git.ops.dev:ops/tools.git")]);
    let refusal = remove(dir.path(), Some(&after), &new).unwrap_err();
    assert_eq!(
        refusal.referrers[0].what,
        "The repo tools (inventory/repos.json) is on git.ops.dev."
    );
    assert_eq!(shared(dir.path()), after);
}

const SELF_HOSTED: &str = "\
schema = 1

[[forge]]
kind = \"gitlab\"
group = \"platform\"
host = \"git.acme.dev\"

[[forge]]
kind = \"github\"
owner = \"acme\"
";

#[test]
fn removing_a_self_hosted_forge_a_catalogued_repo_is_on_is_refused_naming_the_repo() {
    let dir = plane(SELF_HOSTED);
    catalogue(
        dir.path(),
        &[
            ("billing", "git@git.acme.dev:platform/billing.git"),
            ("site", "git@github.com:acme/site.git"),
        ],
    );
    let refusal = remove(dir.path(), Some(SELF_HOSTED), &id(SELF_HOSTED, 0)).unwrap_err();
    assert_eq!(refusal.referrers.len(), 1, "{refusal:?}");
    assert_eq!(
        refusal.referrers[0].what,
        "The repo billing (inventory/repos.json) is on git.acme.dev."
    );
    assert_eq!(refusal.referrers[0].group, None);
    assert_eq!(shared(dir.path()), SELF_HOSTED, "nothing is written");
}

#[test]
fn a_pr_mode_on_a_repo_that_needs_the_forge_is_named_with_its_settings_group() {
    let dir = plane(SELF_HOSTED);
    catalogue(
        dir.path(),
        &[("billing", "https://git.acme.dev/platform/billing.git")],
    );
    fs::write(
        dir.path().join("charter.local.toml"),
        "[repos.billing]\nmode = \"pr\"\n",
    )
    .unwrap();
    let refusal = remove(dir.path(), Some(SELF_HOSTED), &id(SELF_HOSTED, 0)).unwrap_err();
    let named: Vec<(&str, Option<SettingsGroup>)> = refusal
        .referrers
        .iter()
        .map(|one| (one.what.as_str(), one.group))
        .collect();
    assert_eq!(
        named,
        [
            (
                "The repo billing (inventory/repos.json) is on git.acme.dev.",
                None
            ),
            (
                "[repos.billing] mode = \"pr\" in charter.local.toml opens a request on git.acme.dev.",
                Some(SettingsGroup::Saving)
            ),
        ]
    );
}

#[test]
fn the_planes_own_pr_mode_on_the_forge_is_named() {
    let text = format!("{SELF_HOSTED}\n[plane]\nmode = \"pr-merge\"\n");
    let dir = plane(&text);
    crate::testgit::run(
        dir.path(),
        &[
            "remote",
            "add",
            "origin",
            "git@git.acme.dev:platform/plane.git",
        ],
    );
    let refusal = remove(dir.path(), Some(&text), &id(&text, 0)).unwrap_err();
    assert_eq!(
        refusal
            .referrers
            .iter()
            .map(|one| (one.what.as_str(), one.group))
            .collect::<Vec<_>>(),
        [(
            "[plane] mode = \"pr-merge\" in charter.toml opens a request on git.acme.dev, where \
             this project's origin is.",
            Some(SettingsGroup::Saving)
        )]
    );
}

#[test]
fn a_forge_at_a_kinds_own_host_is_needed_by_nothing_since_that_host_is_known_without_it() {
    let dir = plane(SELF_HOSTED);
    catalogue(dir.path(), &[("site", "git@github.com:acme/site.git")]);
    remove(dir.path(), Some(SELF_HOSTED), &id(SELF_HOSTED, 1)).unwrap();
    assert!(!shared(dir.path()).contains("github"));
}

#[test]
fn a_host_another_block_still_declares_is_needed_by_nothing() {
    let text = format!(
        "{SELF_HOSTED}\n[[forge]]\nkind = \"gitlab\"\ngroup = \"data\"\nhost = \"git.acme.dev\"\n"
    );
    let dir = plane(&text);
    catalogue(
        dir.path(),
        &[("billing", "git@git.acme.dev:platform/billing.git")],
    );
    remove(dir.path(), Some(&text), &id(&text, 0)).unwrap();
}

#[test]
fn adding_a_forge_and_removing_it_again_leaves_the_file_as_it_was() {
    let text = "schema = 1\n\n[[forge]]\nkind = \"github\"\nowner = \"acme\"\n\n[memory]\nshare = \"local\"\n\n[persona]\ndefault = \"steward\"\n";
    let dir = plane(text);
    add(
        dir.path(),
        Some(text),
        &entry("gitlab", "ops", "git.ops.invalid", &[]),
    )
    .unwrap();
    let added = shared(dir.path());
    assert!(
        added.find("git.ops.invalid").unwrap() < added.find("[memory]").unwrap(),
        "{added}"
    );
    remove(dir.path(), Some(&added), &id(&added, 1)).unwrap();
    assert_eq!(shared(dir.path()), text);
}

/// The review's file: a comment, block 0 holding both `group` and `owner` (so `group` is the
/// plane's primary group, `config.GROUP`), then a second block.
const PRIMARY: &str = "\
schema = 1

# the team
[[forge]]
kind = \"gitlab\"
group = \"acme\"
owner = \"old\"

[[forge]]
kind = \"github\"
owner = \"beta\"
";

#[test]
fn undoing_a_remove_puts_the_text_back_exactly_and_block_0_is_still_the_primary_group() {
    let dir = plane(PRIMARY);
    remove(dir.path(), Some(PRIMARY), &id(PRIMARY, 0)).unwrap();
    let after = shared(dir.path());
    assert_eq!(crate::forge::group_of(&after.parse().unwrap(), 0), "beta");

    // The Undo of a remove (D-ST3-i as amended): the text before, against the text it left.
    crate::settings::save(dir.path(), Which::Shared, Some(&after), PRIMARY).unwrap();

    assert_eq!(shared(dir.path()), PRIMARY);
    assert_eq!(crate::forge::group_of(&PRIMARY.parse().unwrap(), 0), "acme");
}

#[test]
fn undoing_the_remove_of_one_of_two_identical_blocks_goes_through() {
    let twins = "schema = 1\n\n[[forge]]\nkind = \"github\"\nowner = \"acme\"\n\n[[forge]]\nkind = \"github\"\nowner = \"acme\"\n";
    let dir = plane(twins);
    remove(dir.path(), Some(twins), &id(twins, 1)).unwrap();
    let after = shared(dir.path());
    crate::settings::save(dir.path(), Which::Shared, Some(&after), twins).unwrap();
    assert_eq!(shared(dir.path()), twins);
}

#[test]
fn undoing_a_remove_is_refused_once_the_file_moved() {
    let dir = plane(PRIMARY);
    remove(dir.path(), Some(PRIMARY), &id(PRIMARY, 0)).unwrap();
    let after = shared(dir.path());
    fs::write(
        dir.path().join("charter.toml"),
        format!("{after}\n# edited\n"),
    )
    .unwrap();
    let why = crate::settings::save(dir.path(), Which::Shared, Some(&after), PRIMARY).unwrap_err();
    assert!(why[0].contains("changed on disk"), "{why:?}");
}
