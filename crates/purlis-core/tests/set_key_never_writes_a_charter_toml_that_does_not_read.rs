//! `planefile::set_key` (`charter persona default`'s write) edits `charter.toml` as text, to keep
//! every byte the operator wrote, as the Python charter did. Where that text edit would leave a
//! file that does not parse, or that does not say what was set, it is refused and the file is
//! left exactly as it was.

use purlis_core::scaffold::planefile;

fn project(text: &str) -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("charter.toml"), text).unwrap();
    root
}

fn refused_and_untouched(text: &str) {
    let root = project(text);
    assert!(
        planefile::set_key(root.path(), "persona", "default", "steward").is_err(),
        "{text:?}"
    );
    assert_eq!(
        std::fs::read_to_string(root.path().join("charter.toml")).unwrap(),
        text
    );
}

#[test]
fn a_persona_that_is_not_a_table_is_refused() {
    purlis_core::unsteered!();
    refused_and_untouched("schema = 1\npersona = \"steward\"\n");
}

#[test]
fn a_persona_table_the_text_edit_cannot_see_is_refused_rather_than_doubled() {
    purlis_core::unsteered!();
    refused_and_untouched("schema = 1\npersona = { default = \"ops\" }\n");
    refused_and_untouched("schema = 1\n\n[persona] # front door\ndefault = \"ops\"\n");
}

#[test]
fn an_ordinary_file_is_still_edited_in_place_byte_for_byte() {
    purlis_core::unsteered!();
    let root = project("schema = 1\n\n[persona]\ndefault = \"ops\"\n");
    planefile::set_key(root.path(), "persona", "default", "steward").unwrap();
    assert_eq!(
        std::fs::read_to_string(root.path().join("charter.toml")).unwrap(),
        "schema = 1\n\n[persona]\ndefault = \"steward\"\n"
    );
}

/// `charter persona default`'s own write (`personacmd::set_key`), the same text edit with a
/// clear beside the set.
mod persona_default {
    use purlis_core::personacmd::set_key;

    fn refused_and_untouched(text: &str, value: Option<&str>) {
        let root = super::project(text);
        let path = root.path().join("charter.toml");
        assert!(
            set_key(&path, "persona", "default", value).is_err(),
            "{text:?}"
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
    }

    #[test]
    fn a_set_that_would_double_a_table_or_shadow_a_value_is_refused() {
        purlis_core::unsteered!();
        refused_and_untouched("persona = \"steward\"\n", Some("ops"));
        refused_and_untouched("persona = { default = \"ops\" }\n", Some("steward"));
        refused_and_untouched(
            "[persona] # front door\ndefault = \"ops\"\n",
            Some("steward"),
        );
        refused_and_untouched("persona.default = \"ops\"\n", Some("steward"));
    }

    #[test]
    fn an_ordinary_set_and_clear_still_edit_in_place() {
        purlis_core::unsteered!();
        let root = super::project("schema = 1\n\n[persona]\ndefault = \"ops\"\n");
        let path = root.path().join("charter.toml");
        set_key(&path, "persona", "default", Some("steward")).unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "schema = 1\n\n[persona]\ndefault = \"steward\"\n"
        );
        set_key(&path, "persona", "default", None).unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "schema = 1\n\n[persona]\n"
        );
    }
}
