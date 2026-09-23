//! What the contribution contract admits, and — mostly — what it refuses.
//!
//! Every refusal below is a state that would otherwise read as "nothing declared", which reads
//! as "safe". That is the direction [`super`] leans away from, so the refusals outnumber the
//! acceptances here on purpose.

use super::*;

/// The smallest manifest fragment that declares one usable panel.
fn declaring(json: &str) -> Result<Vec<Panel>, String> {
    let value: serde_json::Value = serde_json::from_str(json).expect("the test's own JSON");
    declared(&value, "acme")
}

/// The rows of the first (and, in these tests, only) list block.
fn rows(panel: &Panel) -> &[Row] {
    match &panel.blocks[0] {
        Block::List { rows, .. } => rows,
        Block::Note { .. } | Block::Chart(_) => panic!("the panel's first block is not a list"),
    }
}

#[test]
fn a_panel_is_a_title_an_ordering_and_a_body() {
    // The whole of the minimum, and the shape everything else in this file is a refusal of.
    let panels = declaring(
        r#"[{
            "id": "reviews",
            "title": "Reviews",
            "order": 20,
            "mark": "note",
            "rows": [
                { "key": "a", "text": "Land the panel contract", "note": "2026-09-23" },
                { "key": "b", "text": "Short", "detail": "The whole of it, in the card." }
            ],
            "empty": { "headline": "Nothing to review", "body": "It is all merged." }
        }]"#,
    )
    .expect("a panel charter draws");

    let [panel] = &panels[..] else {
        panic!("one panel, {panels:?}")
    };
    assert_eq!(panel.title, "Reviews");
    assert_eq!(panel.order, 20);
    assert_eq!(panel.mark, Mark::Note);
    assert_eq!(panel.from, By::Extension("acme".into()));
    assert_eq!(rows(panel).len(), 2);
    assert_eq!(rows(panel)[0].note.as_deref(), Some("2026-09-23"));
    assert_eq!(
        rows(panel)[1].detail,
        Some(Detail::Text("The whole of it, in the card.".into()))
    );
}

#[test]
fn a_contributed_panel_may_not_run_a_charter_verb() {
    // **The most important refusal in this file.** A row that ran a catalogue offer on a click
    // would be the first thing in charter that executes on an extension's say-so, through a
    // path with no hook, no prompt and no grant — charter ADR 0041's second door, opened by a
    // panel. The asymmetry it creates is charter's own panels' and is written down in
    // `panel.rs`'s header rather than smoothed over.
    let why = declaring(
        r#"[{ "id": "p", "title": "P",
              "rows": [{ "key": "a", "text": "Delete the workspace",
                         "runs": "workspace.delete:alpha" }] }]"#,
    )
    .expect_err("a declared row may not carry a verb");

    assert!(why.contains("may not put one on a row"), "{why}");
    assert!(why.contains("data charter draws"), "{why}");
}

#[test]
fn an_empty_states_button_is_refused_for_the_same_reason() {
    // The other half of the same hole: a panel with no rows that offers a way out is an action,
    // and an action is a verb. Refused by name so that the two are one rule rather than one
    // rule and one oversight.
    let why = declaring(
        r#"[{ "id": "p", "title": "P", "empty": { "headline": "None", "offer": "chat.new" } }]"#,
    )
    .expect_err("a declared empty state may not carry a verb");

    assert!(why.contains("may not put one on a row"), "{why}");
}

#[test]
fn a_key_charter_did_not_publish_is_refused_rather_than_ignored() {
    // Property 1: the vocabulary is closed and charter decides it. A key charter ignored is a
    // key the operator was not shown when he consented — and a manifest written against a later
    // charter is better refused with the word in the message than approved for a contribution
    // half of which does nothing.
    let why = declaring(r#"[{ "id": "p", "title": "P", "side": "left" }]"#)
        .expect_err("an unpublished key is refused");

    assert!(why.contains("\"side\""), "{why}");
    assert!(why.contains("not part of what a panel may say"), "{why}");
}

#[test]
fn a_panel_cannot_say_where_it_goes() {
    // Property 3: charter chooses the consumer. `side`, above, is the obvious one; these are
    // the ones somebody reasonable proposes next. None of them is in the vocabulary, so all of
    // them land on the same refusal — which is the point of the vocabulary being a list.
    for key in ["region", "width", "class", "style", "html", "css"] {
        let why =
            declaring(&format!(r#"[{{ "id": "p", "title": "P", "{key}": "x" }}]"#)).unwrap_err();
        assert!(
            why.contains(key),
            "{key} was not named in its own refusal: {why}"
        );
    }
}

#[test]
fn a_mark_is_a_word_charter_published_and_never_a_file() {
    // ADR 0041's third crossing is *a reference to a file*, and an icon is the shape that
    // invites one. The refusal lists the vocabulary so the author is not left guessing.
    let why = declaring(r#"[{ "id": "p", "title": "P", "mark": "./logo.svg" }]"#)
        .expect_err("a mark that is a path is refused");

    assert!(why.contains("charter's marks are"), "{why}");
    assert!(why.contains("it does not bring one"), "{why}");
}

#[test]
fn a_declared_card_is_text_and_never_a_read_of_the_plane() {
    // `Detail::Persona` costs a `persona_details` call against the plane. A declared panel gets
    // `Detail::Text`, which reads nothing — the same asymmetry `runs` draws, one notch smaller.
    let panels = declaring(
        r#"[{ "id": "p", "title": "P",
              "rows": [{ "key": "a", "text": "t", "detail": "steward" }] }]"#,
    )
    .expect("a declared card");

    assert_eq!(
        rows(&panels[0])[0].detail,
        Some(Detail::Text("steward".into())),
        "a declared detail is the row's own words, never a name charter would go and read"
    );
}

#[test]
fn a_control_character_is_refused_rather_than_stripped() {
    // There is no injection to filter — a row is a text node — so this is not about escaping.
    // What a control character does is make two different strings look like one on screen,
    // which is the deception half of ADR 0041's concern and the half no escaping helps with.
    // Built rather than written, because JSON's own `` escape is how one arrives: a raw
    // control byte does not survive `serde_json`, so the state worth pinning is the one that
    // parses cleanly and then wants drawing.
    let value = serde_json::json!([{ "id": "p", "title": "Re\u{8}views" }]);
    let why = declared(&value, "acme").expect_err("a control character is refused");

    assert!(why.contains("control character"), "{why}");
}

#[test]
fn a_panels_id_is_a_segment_so_a_key_can_never_be_a_path() {
    for id in ["../elsewhere", "a/b", ".hidden", "", "-leading"] {
        let why = declaring(&format!(r#"[{{ "id": "{id}", "title": "P" }}]"#))
            .expect_err(&format!("{id:?} is refused"));
        assert!(why.contains("id"), "{id:?}: {why}");
    }
}

#[test]
fn two_extensions_cannot_collide_and_neither_can_one_that_calls_itself_charter() {
    // `Panel::key` is why. An extension id and a panel id are both one segment and neither can
    // hold a '/', so the namespaces cannot be made to overlap — including by an extension that
    // takes charter's own name, which nothing forbids.
    let mine = Panel {
        id: "todos".into(),
        title: "Todos".into(),
        order: 0,
        mark: Mark::Todo,
        blocks: Vec::new(),
        from: By::Charter,
        about: None,
    };
    let theirs = Panel {
        from: By::Extension("charter".into()),
        ..mine.clone()
    };

    assert_eq!(mine.key(), "charter/todos");
    assert_eq!(theirs.key(), "ext/charter/todos");
    assert_ne!(mine.key(), theirs.key());
}

#[test]
fn charters_own_panels_sort_before_a_strangers_at_the_same_order() {
    // A hint and not a position. Two extensions that both said `10` get a stable order rather
    // than the order the disk was read in, and charter's own are not pushed down the region by
    // an extension that declared `order: -1`… which it can, and which is what `order` means.
    let at = |order: i32, from: By, id: &str| Panel {
        id: id.into(),
        title: id.into(),
        order,
        mark: Mark::Dot,
        blocks: Vec::new(),
        from,
        about: None,
    };
    let mut panels = vec![
        at(10, By::Extension("zeta".into()), "z"),
        at(10, By::Charter, "personas"),
        at(10, By::Extension("acme".into()), "a"),
        at(0, By::Extension("acme".into()), "first"),
    ];

    Panel::sort(&mut panels);

    let keys: Vec<String> = panels.iter().map(Panel::key).collect();
    assert_eq!(
        keys,
        [
            "ext/acme/first",
            "charter/personas",
            "ext/acme/a",
            "ext/zeta/z"
        ]
    );
}

#[test]
fn a_panel_an_extension_declares_is_one_line_in_the_prompt() {
    // ADR 0041's consent rule: what the operator says yes to is what he was shown. A panel is
    // shown by what it is called and how much of his region it wants.
    let panels = declaring(
        r#"[{ "id": "p", "title": "Reviews",
              "rows": [{ "key": "a", "text": "one" }, { "key": "b", "text": "two" }] }]"#,
    )
    .expect("a panel");

    assert_eq!(
        declares(&panels[0]),
        "a panel, “Reviews” — 2 rows charter draws in the window's side region"
    );
}

#[test]
fn more_panels_than_the_region_can_hold_are_refused() {
    // A bound is what keeps "an extension contributed a panel" from becoming "an extension took
    // the region" — the one ADR 0038 says must never compete with the needs-you queue.
    let many: Vec<String> = (0..=MOST_PANELS)
        .map(|n| format!(r#"{{ "id": "p{n}", "title": "P{n}" }}"#))
        .collect();
    let why = declaring(&format!("[{}]", many.join(","))).expect_err("too many panels");

    assert!(why.contains(&format!("at most {MOST_PANELS}")), "{why}");
}

#[test]
fn two_panels_with_one_id_are_refused_and_so_are_two_rows_with_one_key() {
    // Both are the window's handles. Two things answering to one handle is a card that opens
    // over the wrong row, which is a defect nobody reports as one.
    let why = declaring(r#"[{ "id": "p", "title": "A" }, { "id": "p", "title": "B" }]"#)
        .expect_err("two panels, one id");
    assert!(why.contains("two panels called"), "{why}");

    let why = declaring(
        r#"[{ "id": "p", "title": "P",
              "rows": [{ "key": "a", "text": "one" }, { "key": "a", "text": "two" }] }]"#,
    )
    .expect_err("two rows, one key");
    assert!(why.contains("two rows called"), "{why}");
}

// -------------------------------------------------------------------------------------
// What a program answers (ADR 0041 stage 2): the same vocabulary, plus a chart
// -------------------------------------------------------------------------------------

fn answering(json: &str) -> Result<Vec<Block>, String> {
    let value: serde_json::Value = serde_json::from_str(json).expect("the test's own JSON");
    answered(&value)
}

#[test]
fn an_answer_is_a_list_a_note_and_a_chart() {
    let blocks = answering(
        r#"[{"kind":"note","text":"42 memories"},
            {"kind":"chart","title":"Per persona","shape":"columns","unit":"memories",
             "points":[{"label":"steward","value":30,"note":"71%"},{"label":"release","value":12}]},
            {"kind":"list","rows":[{"key":"steward","text":"steward","note":"today"}]}]"#,
    )
    .expect("an answer");

    assert_eq!(blocks.len(), 3);
    assert_eq!(
        blocks[1],
        Block::Chart(Chart {
            title: "Per persona".into(),
            shape: Shape::Columns,
            unit: Some("memories".into()),
            points: vec![
                Point {
                    label: "steward".into(),
                    value: 30,
                    note: Some("71%".into())
                },
                Point {
                    label: "release".into(),
                    value: 12,
                    note: None
                },
            ],
        })
    );
}

#[test]
fn a_chart_cannot_be_declared_in_a_manifest() {
    // ADR 0043's reason, which still holds for a manifest: a declared chart is numbers written
    // down at install time. `declared` has no way to carry one, and a panel that tried is
    // refused by name rather than having the key dropped.
    let refused = declaring(r#"[{"id":"p","title":"P","chart":{"title":"x","points":[]}}]"#)
        .expect_err("a frozen chart was declared");
    assert!(refused.contains("\"chart\""), "{refused}");
}

#[test]
fn an_answered_row_may_not_carry_a_verb_either() {
    let refused =
        answering(r#"[{"kind":"list","rows":[{"key":"a","text":"a","runs":"chat.new"}]}]"#)
            .expect_err("a verb in an answer");
    assert!(refused.contains(NO_VERB), "{refused}");
}

#[test]
fn an_answered_empty_state_may_not_offer_a_verb() {
    let refused =
        answering(r#"[{"kind":"list","rows":[],"empty":{"headline":"x","offer":"chat.new"}}]"#)
            .expect_err("a verb in an empty state");
    assert!(refused.contains(NO_VERB), "{refused}");
}

#[test]
fn a_chart_that_names_a_colour_is_refused_rather_than_painted() {
    let refused = answering(r#"[{"kind":"chart","title":"x","colour":"red","points":[]}]"#)
        .expect_err("a colour reached a chart");
    assert!(refused.contains("\"colour\""), "{refused}");
}

#[test]
fn a_point_that_is_not_a_whole_count_is_refused_rather_than_rounded() {
    for value in ["-1", "1.5", "\"7\"", "4294967296", "null"] {
        let refused = answering(&format!(
            r#"[{{"kind":"chart","title":"x","points":[{{"label":"a","value":{value}}}]}}]"#
        ))
        .expect_err(value);
        assert!(refused.contains("value"), "{value}: {refused}");
    }
}

#[test]
fn a_chart_shape_charter_does_not_draw_is_refused_with_the_ones_it_does() {
    let refused = answering(r#"[{"kind":"chart","title":"x","shape":"pie","points":[]}]"#)
        .expect_err("a pie");
    assert!(refused.contains("bars, columns"), "{refused}");
}

#[test]
fn a_block_kind_charter_does_not_draw_is_refused() {
    let refused =
        answering(r#"[{"kind":"html","html":"<b>"}]"#).expect_err("markup reached the window");
    assert!(refused.contains("\"html\""), "{refused}");
}

#[test]
fn an_answer_is_bounded_in_blocks_and_in_points() {
    let many_blocks = format!(
        "[{}]",
        vec![r#"{"kind":"note","text":"x"}"#; MOST_BLOCKS + 1].join(",")
    );
    assert!(
        answering(&many_blocks).is_err(),
        "too many blocks were drawn"
    );

    let points: Vec<String> = (0..=MOST_POINTS)
        .map(|n| format!(r#"{{"label":"p{n}","value":{n}}}"#))
        .collect();
    let many_points = format!(
        r#"[{{"kind":"chart","title":"x","points":[{}]}}]"#,
        points.join(",")
    );
    assert!(
        answering(&many_points).is_err(),
        "too many points were drawn"
    );
}

#[test]
fn a_format_character_that_draws_as_nothing_is_refused_wherever_an_answer_puts_words() {
    // `is_control` is `Cc` only. The bidirectional overrides and isolates, the zero-width
    // characters and the byte-order mark are `Cf`: they draw nothing, and the overrides turn the
    // words after them around — so a row reading "fine" could draw as anything its author liked.
    for invisible in [
        '\u{202A}', '\u{202E}', '\u{2066}', '\u{2069}', '\u{200B}', '\u{200F}', '\u{2060}',
        '\u{FEFF}', '\u{00AD}', '\u{2028}',
    ] {
        let text = format!("fine{invisible}enif");
        for answer in [
            serde_json::json!([{ "kind": "note", "text": text }]),
            serde_json::json!([{ "kind": "list", "rows": [{ "key": "k", "text": text }] }]),
            serde_json::json!([{ "kind": "list", "rows": [{ "key": text, "text": "t" }] }]),
            serde_json::json!([{ "kind": "chart", "title": "c",
                                 "points": [{ "label": text, "value": 1 }] }]),
        ] {
            assert!(
                answered(&answer).is_err(),
                "U+{:04X} was drawn: {answer}",
                u32::from(invisible)
            );
        }
    }
}

#[test]
fn ordinary_words_with_spaces_and_accents_are_still_drawn() {
    // The refusal is of what draws as nothing, not of anything outside ASCII: an accent, an
    // em-dash and a no-break space are content.
    let blocks = answering(r#"[{"kind":"note","text":"café — 3\u00a0memories"}]"#)
        .expect("ordinary words are drawn");
    assert_eq!(blocks.len(), 1);
}

#[test]
fn a_control_character_in_a_label_is_refused() {
    let refused =
        answering(r#"[{"kind":"chart","title":"x","points":[{"label":"a\u0007b","value":1}]}]"#)
            .expect_err("a bell in a label");
    assert!(refused.contains("control character"), "{refused}");
}
