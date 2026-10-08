//! The record of the pairs a person said never to (#1503): a file of its own that no other
//! record's fault and no older build can empty, and that is never read as "no nevers" when it
//! does not read. Every expected answer is written out.

use std::path::Path;

use super::*;
use crate::dispatchgrant::{ChatPair, Covers, InForce, covers};
use crate::sandbox::policy::Locks;

fn one(asking: &str, target: &str) -> Vec<(String, String)> {
    vec![(asking.to_owned(), target.to_owned())]
}

/// Writes `text` as the file at `path`, making its folder.
fn by_hand(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().expect("a folder")).expect("made");
    std::fs::write(path, text).expect("written");
}

/// A grant made for the chat itself, which is the app's and goes with no file.
fn with_a_chat_grant(root: &Path) -> InForce {
    InForce::read(
        root,
        vec![ChatPair {
            asking: Some("steward".to_owned()),
            target: "devops".to_owned(),
        }],
    )
}

fn asked(grants: &InForce) -> Covers {
    covers(Some("steward"), "devops", grants, &Locks::none())
}

// ---- kept, read back and lifted -----------------------------------------------------------------

#[test]
fn no_record_is_no_nevers_and_one_is_kept_in_a_file_of_its_own() {
    let project = tempfile::tempdir().expect("a project");
    let root = project.path();
    assert_eq!(read(root), Nevers::Read(vec![]));
    add(root, "steward", "devops").expect("kept");
    add(root, "steward", "devops").expect("kept once");
    add(root, "qa", "prod").expect("kept");
    assert_eq!(
        std::fs::read_to_string(path(root)).expect("the record"),
        "{\n  \"never\": [\n    {\n      \"asking\": \"steward\",\n      \"target\": \
         \"devops\"\n    },\n    {\n      \"asking\": \"qa\",\n      \"target\": \"prod\"\n    \
         }\n  ]\n}\n"
    );
    assert!(path(root).ends_with("app/dispatch-never.json"));
    // Nothing of it is in this machine's other record, or in the project's file.
    assert!(!crate::sandbox::local::path(root).exists());
    assert!(!crate::names::manifest(root).exists());

    assert!(lift(root, "steward", "devops").expect("lifted"));
    assert!(!lift(root, "steward", "devops").expect("gone"));
    assert_eq!(read(root), Nevers::Read(one("qa", "prod")));
}

#[test]
fn a_key_this_build_does_not_know_is_kept_as_it_is() {
    let project = tempfile::tempdir().expect("a project");
    let root = project.path();
    by_hand(
        &path(root),
        r#"{"later": {"said": 1790000000}, "never": [{"asking": "steward", "target": "devops", "at": 5}]}"#,
    );
    assert_eq!(read(root), Nevers::Read(one("steward", "devops")));
    add(root, "qa", "prod").expect("kept");
    let kept: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path(root)).expect("the record"))
            .expect("json");
    assert_eq!(kept["later"], serde_json::json!({"said": 1_790_000_000}));
    assert_eq!(kept["never"].as_array().map(Vec::len), Some(2));
}

// ---- no other record's fault, and no older build, empties it (the review's probe) ---------------

#[test]
fn a_fault_in_this_machine_s_other_record_costs_no_never() {
    let project = tempfile::tempdir().expect("a project");
    let root = project.path();
    add(root, "steward", "devops").expect("kept");
    // One wrong-typed key beside the grants: that record reads as empty.
    by_hand(
        &crate::sandbox::local::path(root),
        r#"{"dispatch_mine": [{"asking": "steward", "target": "devops"}], "hosts_mine": "x"}"#,
    );

    // The never still stands, over a grant made for the chat itself.
    let grants = with_a_chat_grant(root);
    assert_eq!(grants.never, one("steward", "devops"));
    assert_eq!(asked(&grants), Covers::Never);

    // And the next write of that record, which starts it again, takes no never with it.
    crate::sandbox::local::count(root, crate::sandbox::local::Started::Sandboxed).expect("kept");
    crate::sandbox::local::grant_dispatch(root, "steward", "devops").expect("kept");
    assert_eq!(asked(&with_a_chat_grant(root)), Covers::Never);
    assert_eq!(read(root), Nevers::Read(one("steward", "devops")));
}

#[test]
fn a_build_that_does_not_know_of_nevers_rewrites_its_record_and_drops_none() {
    let project = tempfile::tempdir().expect("a project");
    let root = project.path();
    crate::sandbox::local::grant_dispatch(root, "steward", "devops").expect("kept");
    add(root, "steward", "devops").expect("kept");
    // An older build's whole view of `app/sandbox.json`, written back as it would write it:
    // the keys it knows, and nothing else.
    #[derive(serde::Serialize, serde::Deserialize)]
    struct Older {
        #[serde(default)]
        dispatch_mine: Vec<serde_json::Value>,
    }
    let record = crate::sandbox::local::path(root);
    let older: Older =
        serde_json::from_str(&std::fs::read_to_string(&record).expect("read")).expect("its shape");
    std::fs::write(&record, serde_json::to_string_pretty(&older).expect("json")).expect("written");

    // The grant it kept is still beaten by the never it never saw.
    let grants = InForce::read(root, Vec::new());
    assert_eq!(grants.you.len(), 1);
    assert_eq!(asked(&grants), Covers::Never);
}

// ---- a record that does not read is never "no nevers" -------------------------------------------

/// Every way the file is there and is not the shape.
const UNREAD: [&str; 8] = [
    "",
    "not json",
    "[]",
    r#"{"never": "steward"}"#,
    r#"{"never": [3]}"#,
    r#"{"never": [{"asking": "steward"}]}"#,
    r#"{"never": [{"asking": "steward", "target": 7}]}"#,
    r#"{"never": [{"asking": "qa", "target": "prod"}, "steward -> devops"]}"#,
];

#[test]
fn a_record_that_does_not_read_lets_no_grant_cover_any_pair_of_two_personas() {
    use crate::dispatchunattended::{Answer, Refusal, covers as unattended};
    for broken in UNREAD {
        let project = tempfile::tempdir().expect("a project");
        let root = project.path();
        // Granted in every way there is, with no never ever said.
        crate::sandbox::local::grant_dispatch(root, "steward", "devops").expect("kept");
        crate::dispatchgrant::allow_any(root, "steward", crate::sandbox::grant::Level::You)
            .expect("kept");
        by_hand(&path(root), broken);

        assert_eq!(read(root), Nevers::Unread, "{broken:?}");
        let grants = with_a_chat_grant(root);
        assert!(grants.never_unread, "{broken:?}");
        // The person is asked: not covered by the chat's grant, theirs, or "any persona".
        assert_eq!(asked(&grants), Covers::Unread, "{broken:?}");
        assert_eq!(
            covers(Some("steward"), "qa", &grants, &Locks::none()),
            Covers::Unread
        );
        assert_eq!(
            covers(None, "devops", &grants, &Locks::none()),
            Covers::Unread
        );
        // A chat's own persona needs no grant, so none is missing for it.
        assert_eq!(
            covers(Some("steward"), "steward", &grants, &Locks::none()),
            Covers::Covered
        );
        // A chat nobody is at is refused, in a sentence that says why.
        let answer = unattended(Some("steward"), "devops", &grants, &Locks::none(), true);
        assert_eq!(answer, Answer::Refused(Refusal::NeversUnread), "{broken:?}");
    }
    assert_eq!(
        crate::dispatchunattended::Refusal::NeversUnread.say(),
        "purlis could not read the list of pairs the person said never to on this machine, so \
         no dispatch grant counts until it reads, and nobody is here to ask. Nothing was \
         started. Do this work without another persona, or say in what you leave behind that \
         it is waiting; a person mends the list on this machine."
    );
}

#[test]
fn a_write_refuses_a_record_it_could_not_read_and_leaves_its_bytes() {
    for broken in UNREAD {
        let project = tempfile::tempdir().expect("a project");
        let root = project.path();
        by_hand(&path(root), broken);
        // Named by where it is in the project, whichever name the state folder goes by.
        let state = crate::names::state(root);
        let folder = state.file_name().expect("a name").to_string_lossy();
        let said = format!(
            "purlis could not read the list of pairs you said never to \
             ({folder}/app/dispatch-never.json in this project), so it changed nothing there \
             and no dispatch grant counts until it reads. Fix that file, or delete it to say \
             never to nothing."
        );
        assert_eq!(
            add(root, "qa", "prod").expect_err("refused").to_string(),
            said,
            "{broken:?}"
        );
        assert_eq!(
            lift(root, "steward", "devops")
                .expect_err("refused")
                .to_string(),
            said
        );
        assert_eq!(
            std::fs::read_to_string(path(root)).expect("still there"),
            broken
        );
    }
}

#[cfg(unix)]
#[test]
fn a_record_that_is_a_link_or_a_folder_is_not_read_and_not_written_through() {
    let project = tempfile::tempdir().expect("a project");
    let root = project.path();
    let elsewhere = project.path().join("elsewhere.json");
    std::fs::write(&elsewhere, r#"{"never": []}"#).expect("written");
    std::fs::create_dir_all(path(root).parent().expect("a folder")).expect("made");
    std::os::unix::fs::symlink(&elsewhere, path(root)).expect("a link");
    assert_eq!(read(root), Nevers::Unread);
    assert!(add(root, "steward", "devops").is_err());
    assert_eq!(
        std::fs::read_to_string(&elsewhere).expect("read"),
        r#"{"never": []}"#
    );

    std::fs::remove_file(path(root)).expect("the link goes");
    std::fs::create_dir(path(root)).expect("a folder in its place");
    assert_eq!(read(root), Nevers::Unread);
    assert!(lift(root, "steward", "devops").is_err());
}

#[test]
fn deleting_the_record_is_how_a_person_says_never_to_nothing() {
    let project = tempfile::tempdir().expect("a project");
    let root = project.path();
    crate::sandbox::local::grant_dispatch(root, "steward", "devops").expect("kept");
    by_hand(&path(root), "not json");
    assert_eq!(asked(&InForce::read(root, Vec::new())), Covers::Unread);
    std::fs::remove_file(path(root)).expect("deleted");
    assert_eq!(asked(&InForce::read(root, Vec::new())), Covers::Covered);
}

// ---- what an earlier build left in the other record ---------------------------------------------

#[test]
fn the_move_keeps_every_pair_once_and_every_other_key_of_the_record_it_leaves() {
    let project = tempfile::tempdir().expect("a project");
    let root = project.path();
    let record = crate::sandbox::local::path(root);
    by_hand(
        &record,
        r#"{"offer": "taken", "dispatch_mine": [{"asking": "steward", "target": "devops"}],
            "dispatch_never": [{"asking": "steward", "target": "devops"}, {"asking": "qa", "target": "prod"}]}"#,
    );
    by_hand(
        &path(root),
        r#"{"never": [{"asking": "qa", "target": "prod"}]}"#,
    );

    let grants = InForce::read(root, Vec::new());

    assert_eq!(
        grants.never,
        vec![
            ("qa".to_owned(), "prod".to_owned()),
            ("steward".to_owned(), "devops".to_owned())
        ]
    );
    assert_eq!(asked(&grants), Covers::Never);
    let left: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&record).expect("read")).expect("json");
    assert_eq!(left.get("dispatch_never"), None);
    assert_eq!(left["offer"], "taken");
    assert_eq!(left["dispatch_mine"].as_array().map(Vec::len), Some(1));
    // Read again, nothing is left to move and nothing is doubled.
    assert_eq!(InForce::read(root, Vec::new()).never.len(), 2);
}

#[test]
fn a_never_left_in_a_record_that_does_not_otherwise_read_is_moved_all_the_same() {
    let project = tempfile::tempdir().expect("a project");
    let root = project.path();
    by_hand(
        &crate::sandbox::local::path(root),
        r#"{"hosts_mine": "x", "dispatch_never": [{"asking": "steward", "target": "devops"}]}"#,
    );
    assert_eq!(asked(&with_a_chat_grant(root)), Covers::Never);
    assert_eq!(read(root), Nevers::Read(one("steward", "devops")));
}

#[test]
fn a_write_of_the_other_record_before_the_move_drops_no_never() {
    // A chat starts, and is counted, before anything has read the nevers.
    for record in [
        r#"{"dispatch_never": [{"asking": "steward", "target": "devops"}]}"#,
        // One that reads as empty and is started again by that write.
        r#"{"hosts_mine": "x", "dispatch_never": [{"asking": "steward", "target": "devops"}]}"#,
    ] {
        let project = tempfile::tempdir().expect("a project");
        let root = project.path();
        by_hand(&crate::sandbox::local::path(root), record);
        crate::sandbox::local::count(root, crate::sandbox::local::Started::Sandboxed)
            .expect("counted");
        assert_eq!(
            read(root),
            Nevers::Read(one("steward", "devops")),
            "{record}"
        );
    }
}

#[test]
fn what_an_earlier_build_left_that_is_no_list_of_pairs_is_unread_and_is_left_where_it_is() {
    let project = tempfile::tempdir().expect("a project");
    let root = project.path();
    let record = crate::sandbox::local::path(root);
    let left = r#"{"dispatch_never": [3]}"#;
    by_hand(&record, left);
    assert_eq!(read(root), Nevers::Unread);
    assert_eq!(asked(&with_a_chat_grant(root)), Covers::Unread);
    assert_eq!(std::fs::read_to_string(&record).expect("read"), left);
    assert!(add(root, "qa", "prod").is_err());
}

#[test]
fn nevers_that_cannot_be_moved_into_a_record_that_does_not_read_are_not_lost() {
    let project = tempfile::tempdir().expect("a project");
    let root = project.path();
    let record = crate::sandbox::local::path(root);
    by_hand(
        &record,
        r#"{"dispatch_never": [{"asking": "steward", "target": "devops"}]}"#,
    );
    by_hand(&path(root), "not json");
    assert_eq!(read(root), Nevers::Unread);
    // Still where the earlier build left them, for when the record reads again.
    assert!(
        std::fs::read_to_string(&record)
            .expect("read")
            .contains("dispatch_never")
    );
    std::fs::remove_file(path(root)).expect("mended");
    assert_eq!(read(root), Nevers::Read(one("steward", "devops")));
}
