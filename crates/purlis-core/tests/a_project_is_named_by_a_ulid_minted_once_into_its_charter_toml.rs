//! A project is named by a stable id: a ULID minted once into its own `charter.toml`, as
//! `[project] id`, so every clone and every device names it the same, and the session protocol
//! names it by that and never by a path (V76, ADR 0068 as amended by FD-26).

use purlis_core::scaffold::planefile;

fn project(text: &str) -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("charter.toml"), text).unwrap();
    root
}

#[test]
fn a_project_without_an_id_is_given_one_and_keeps_every_byte_it_had() {
    purlis_core::unsteered!();
    let text = "schema = 1\n\n# the team's forge\n[[forge]]\nkind = \"github\"\n";
    let root = project(text);
    assert_eq!(planefile::project_id(root.path()), None);

    let id = planefile::ensure_project_id(root.path()).unwrap();
    assert!(ulid::Ulid::from_string(&id).is_ok(), "{id} is a ULID");
    let written = std::fs::read_to_string(root.path().join("charter.toml")).unwrap();
    assert_eq!(written, format!("{text}\n[project]\nid = \"{id}\"\n"));
    assert_eq!(planefile::project_id(root.path()), Some(id));
}

#[test]
fn an_id_once_minted_is_the_one_every_later_ask_gets() {
    purlis_core::unsteered!();
    let root = project("schema = 1\n");
    let first = planefile::ensure_project_id(root.path()).unwrap();
    let before = std::fs::read(root.path().join("charter.toml")).unwrap();
    assert_eq!(planefile::ensure_project_id(root.path()).unwrap(), first);
    assert_eq!(
        std::fs::read(root.path().join("charter.toml")).unwrap(),
        before,
        "an id already there is not written again"
    );
}

#[test]
fn an_id_written_by_hand_in_lowercase_reads_in_its_canonical_spelling() {
    purlis_core::unsteered!();
    let root = project("[project]\nid = \"01j9prjct00000000000000000\"\n");
    assert_eq!(
        planefile::project_id(root.path()).as_deref(),
        Some("01J9PRJCT00000000000000000")
    );
}

#[test]
fn an_id_that_is_not_a_ulid_is_refused_and_never_written_over() {
    purlis_core::unsteered!();
    let text = "[project]\nid = \"my-project\"\n";
    let root = project(text);
    assert_eq!(planefile::project_id(root.path()), None);
    assert!(planefile::ensure_project_id(root.path()).is_err());
    assert_eq!(
        std::fs::read_to_string(root.path().join("charter.toml")).unwrap(),
        text
    );
}

#[test]
fn a_directory_that_is_not_a_project_gets_no_id() {
    purlis_core::unsteered!();
    let root = tempfile::tempdir().unwrap();
    assert!(planefile::ensure_project_id(root.path()).is_err());
    assert!(!root.path().join("charter.toml").exists());
}

#[test]
fn many_asking_at_once_all_get_the_one_id() {
    purlis_core::unsteered!();
    let root = project("schema = 1\n");
    let path = root.path().to_owned();
    let ids: Vec<String> = std::thread::scope(|scope| {
        let asks: Vec<_> = (0..8)
            .map(|_| scope.spawn(|| planefile::ensure_project_id(&path).unwrap()))
            .collect();
        asks.into_iter().map(|ask| ask.join().unwrap()).collect()
    });
    assert!(ids.windows(2).all(|pair| pair[0] == pair[1]), "{ids:?}");
    let written = std::fs::read_to_string(root.path().join("charter.toml")).unwrap();
    assert_eq!(written.matches("[project]").count(), 1, "{written}");
}

/// Asks for the id of a project whose `charter.toml` is `text`, and holds the file to what it
/// must be after: valid TOML that still says everything `text` said, and the id that came back.
fn minted_into(text: &str) -> String {
    let root = project(text);
    let id = planefile::ensure_project_id(root.path())
        .unwrap_or_else(|e| panic!("no id for {text:?}: {e}"));
    let written = std::fs::read_to_string(root.path().join("charter.toml")).unwrap();
    let after: toml::Table = written
        .parse()
        .unwrap_or_else(|e| panic!("{written:?} is not TOML: {e}"));
    assert_eq!(
        after["project"]["id"].as_str(),
        Some(id.as_str()),
        "{written}"
    );
    let before: toml::Table = text.parse().unwrap();
    for (key, value) in &before {
        if key == "project" {
            for (inner, was) in value.as_table().unwrap() {
                assert_eq!(&after["project"][inner], was, "{written}");
            }
        } else {
            assert_eq!(&after[key], value, "{written}");
        }
    }
    assert_eq!(planefile::project_id(root.path()), Some(id.clone()));
    id
}

#[test]
fn a_project_header_with_a_comment_gets_its_id_inside_it() {
    purlis_core::unsteered!();
    minted_into("schema = 1\n\n[project] # who we are\nname = \"login\"\n");
}

#[test]
fn a_project_header_with_spaces_inside_gets_its_id_inside_it() {
    purlis_core::unsteered!();
    minted_into("schema = 1\n\n[ project ]\nname = \"login\"\n");
}

#[test]
fn a_quoted_project_header_gets_its_id_inside_it() {
    purlis_core::unsteered!();
    minted_into("schema = 1\n\n[\"project\"]\nname = \"login\"\n");
}

#[test]
fn an_inline_project_table_gets_its_id_inside_it() {
    purlis_core::unsteered!();
    minted_into("schema = 1\nproject = { name = \"login\" }\n");
}

#[test]
fn a_project_table_made_by_a_dotted_key_gets_its_id_inside_it() {
    purlis_core::unsteered!();
    minted_into("schema = 1\nproject.name = \"login\"\n");
}

#[test]
fn a_project_table_made_by_a_subtable_gets_its_id_inside_it() {
    purlis_core::unsteered!();
    minted_into("schema = 1\n\n[project.links]\nhome = \"https://example.com\"\n");
}

#[test]
fn a_project_that_is_not_a_table_is_refused_and_left_as_it_is() {
    purlis_core::unsteered!();
    let text = "schema = 1\nproject = \"login\"\n";
    let root = project(text);
    assert!(planefile::ensure_project_id(root.path()).is_err());
    assert_eq!(
        std::fs::read_to_string(root.path().join("charter.toml")).unwrap(),
        text
    );
}
