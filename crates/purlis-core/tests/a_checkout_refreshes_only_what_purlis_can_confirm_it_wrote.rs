//! #1583: a launch brings a checkout's generated or mirrored file up to the project's newer
//! text only when purlis can confirm it wrote the text there now. The checkout's own record
//! is not enough on its own: the project's app state must have noted offering that very text
//! at that path too.
//!
//! These need real git (a checkout's own `info/exclude`), so they first run on CI.

mod support;

use purlis_core::{guest, layer, wslayer};

const AGENT: &str = ".claude/agents/steward.md";
const SETTINGS: &str = ".claude/settings.json";
const LOCAL: &str = ".claude/settings.local.json";

#[test]
fn a_forged_record_naming_the_operators_own_files_leaves_them_byte_for_byte() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("svc");
    f.give_the_plane_a_layer();
    // The operator's own files, at the paths the project's layer offers, never written by
    // purlis and never offered by the project.
    let mine = [
        (
            SETTINGS,
            "{\"permissions\": {\"deny\": [\"Bash(curl *)\"]}}\n",
        ),
        (LOCAL, "{\"permissions\": {\"deny\": [\"Bash(ssh *)\"]}}\n"),
        (AGENT, "# my own steward\n"),
    ];
    std::fs::create_dir_all(f.clone.join(".claude/agents")).unwrap();
    for (rel, text) in mine {
        std::fs::write(f.clone.join(rel), text).unwrap();
    }
    // A record in the checkout, which a chat there could write, naming each with its digest.
    let forged: serde_json::Map<String, serde_json::Value> = mine
        .iter()
        .map(|(rel, text)| ((*rel).to_owned(), layer::digest(text).into()))
        .collect();
    std::fs::write(
        f.clone.join(layer::MARKER),
        serde_json::to_string(&forged).unwrap(),
    )
    .unwrap();

    let wired = guest::wire(&f.plane, &f.clone);

    for (rel, text) in mine {
        assert_eq!(
            std::fs::read(f.clone.join(rel)).unwrap(),
            text.as_bytes(),
            "{rel} was overwritten: {wired:?}"
        );
        let row = wired
            .rows
            .iter()
            .find(|r| r.rel == rel)
            .expect("a row for it");
        assert_eq!(row.status, guest::Status::Unconfirmed, "{wired:?}");
    }
    assert!(
        !wired.complete(),
        "the project's newer layer is not in force there"
    );
    let refusal = wired.refusal(&f.clone);
    assert!(refusal.contains("cannot confirm"), "{refusal}");

    // And a workspace's own wire reports the same rows, for `reinit` to name.
    let rows = wslayer::wire(&f.plane, &f.workspace());
    assert!(
        rows.iter()
            .any(|r| r.rel == format!("svc/{SETTINGS}") && r.did == wslayer::Did::Unconfirmed),
        "{rows:?}"
    );
    for (rel, text) in mine {
        assert_eq!(std::fs::read_to_string(f.clone.join(rel)).unwrap(), text);
    }
}

#[test]
fn a_copy_purlis_wrote_and_noted_is_still_brought_up_to_date() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("svc");
    f.give_the_plane_a_layer();
    guest::wire(&f.plane, &f.clone);
    assert!(f.clone.join(AGENT).exists(), "mirrored first");

    std::fs::write(
        f.plane.join(SETTINGS),
        "{\"env\": {\"CHARTER_HARNESS\": \"claude-code\"}, \
         \"permissions\": {\"deny\": [\"Bash(curl *)\"]}}\n",
    )
    .unwrap();
    std::fs::write(
        f.plane.join(LOCAL),
        "{\"permissions\": {\"deny\": [\"Bash(ssh *)\"]}}\n",
    )
    .unwrap();
    std::fs::write(f.plane.join(AGENT), "# steward\n\nMoved on.\n").unwrap();
    let wired = guest::wire(&f.plane, &f.clone);

    for rel in [SETTINGS, LOCAL, AGENT] {
        let row = wired
            .rows
            .iter()
            .find(|r| r.rel == rel)
            .expect("a row for it");
        assert_eq!(row.status, guest::Status::Refreshed, "{wired:?}");
    }
    assert!(wired.complete(), "{wired:?}");
    assert!(
        std::fs::read_to_string(f.clone.join(SETTINGS))
            .unwrap()
            .contains("Bash(curl *)")
    );
    assert!(
        std::fs::read_to_string(f.clone.join(LOCAL))
            .unwrap()
            .contains("Bash(ssh *)")
    );
    assert_eq!(
        std::fs::read_to_string(f.clone.join(AGENT)).unwrap(),
        "# steward\n\nMoved on.\n"
    );
}
