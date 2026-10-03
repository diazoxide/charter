//! The harness capability card (HP-19): what each harness can do, read off its declaration
//! (FD-14) and the adapter charter ships for it. Every expected answer is written out.

use super::*;

/// A project with `harnesses/<file>` holding `text`.
fn project(files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join(harness_declaration::DIR)).unwrap();
    for (file, text) in files {
        std::fs::write(dir.path().join(harness_declaration::DIR).join(file), text).unwrap();
    }
    dir
}

/// A harness a project declares, answering two capabilities and leaving the rest unsaid.
const GEMINI: &str = r#"
name = "gemini"
title = "Gemini CLI"
program = "gemini"
tested = ">=0.9, <0.12"

[session]
resume = ["--resume={id}"]

[terminal]
ready_to_type = "raw-and-quiet"

[levels]
acp = ["gemini", "--experimental-acp"]

[capabilities]
reports_waiting = "no: it has no hook for a pending approval"
resumes_by_id = "yes"
"#;

fn card_of<'a>(cards: &'a [Card], name: &str) -> &'a Card {
    cards
        .iter()
        .find(|card| card.name == name)
        .unwrap_or_else(|| panic!("no card for {name}"))
}

fn ability<'a>(card: &'a Card, id: &str) -> &'a Ability {
    card.abilities
        .iter()
        .find(|ability| ability.id == id)
        .unwrap_or_else(|| panic!("{}'s card has no line for {id}", card.name))
}

/// Whether `card` has line `id`.
fn has(card: &Card, id: &str) -> bool {
    ability(card, id).answer.holds()
}

#[test]
fn every_harness_a_project_has_gets_a_card_the_built_ins_first() {
    let dir = project(&[("gemini.toml", GEMINI)]);

    let cards = read(dir.path());

    let labels: Vec<String> = cards.iter().map(Card::label).collect();
    assert_eq!(
        labels,
        [
            "What Claude Code can do here",
            "What opencode can do here",
            "What Codex can do here",
            "What Gemini CLI can do here",
        ]
    );
}

#[test]
fn a_card_says_each_capability_as_the_declaration_answers_it() {
    let cards = read(project(&[]).path());
    let opencode = card_of(&cards, "opencode");

    assert_eq!(ability(opencode, "reports_waiting").answer, Answer::Yes);
    assert_eq!(
        ability(opencode, "reports_its_process").answer,
        Answer::No("opencode names no variable for the process its plugin runs under".to_owned())
    );
    assert_eq!(
        ability(opencode, "reports_waiting").label,
        "Tells charter when it is waiting for you"
    );
}

#[test]
fn a_card_has_a_line_for_every_capability_charter_reads_even_one_left_unsaid() {
    let cards = read(project(&[("gemini.toml", GEMINI)]).path());
    let gemini = card_of(&cards, "gemini");

    for capability in harness_declaration::CAPABILITIES {
        let _ = ability(gemini, capability);
    }
    // Charter acts on this one only through an adapter, so a declared harness lacks it.
    assert_eq!(
        ability(gemini, "per_chat_plugins").answer,
        Answer::No(NO_ADAPTER.to_owned())
    );
    assert_eq!(ability(gemini, "resumes_by_id").answer, Answer::Yes);
}

#[test]
fn how_charter_follows_a_chat_comes_from_the_levels_its_declaration_offers() {
    let cards = read(project(&[("gemini.toml", GEMINI)]).path());

    let follows = |name: &str| {
        let card = card_of(&cards, name);
        (has(card, HOOKS), has(card, ACP))
    };
    assert_eq!(follows("claude"), (true, false));
    assert_eq!(follows("opencode"), (true, true));
    assert_eq!(follows("codex"), (true, false));
    // A project's harness has no adapter charter ships, so no hooks — and its own ACP agent.
    assert_eq!(follows("gemini"), (false, true));
}

#[test]
fn whether_charter_can_type_into_a_harness_comes_from_its_declared_terminal() {
    let cards = read(project(&[("gemini.toml", GEMINI)]).path());

    assert!(has(card_of(&cards, "claude"), READY_TO_TYPE));
    assert!(has(card_of(&cards, "codex"), READY_TO_TYPE));
    assert!(!has(card_of(&cards, "opencode"), READY_TO_TYPE));
    assert_eq!(
        ability(card_of(&cards, "opencode"), READY_TO_TYPE).answer,
        Answer::No("it gives no sign charter can read that it has finished starting".to_owned())
    );
}

#[test]
fn whether_a_harness_opens_sandboxed_comes_from_the_adapter_charter_ships() {
    let cards = read(project(&[("gemini.toml", GEMINI)]).path());

    for card in &cards {
        let compiled = Harness::of_kind(&card.name)
            .filter(|_| card.origin == Origin::BuiltIn)
            .is_some_and(|harness| crate::sandbox::compiler(harness).is_some());
        assert_eq!(
            has(card, SANDBOX),
            compiled,
            "{}'s card disagrees with its adapter about the sandbox",
            card.name
        );
    }
    assert!(has(card_of(&cards, "claude"), SANDBOX));
    assert_eq!(
        ability(card_of(&cards, "gemini"), SANDBOX).answer,
        Answer::No(NO_ADAPTER.to_owned())
    );
}

#[test]
fn a_control_off_for_a_missing_capability_says_the_cards_line_and_label() {
    let cards = read(project(&[("gemini.toml", GEMINI)]).path());

    assert_eq!(card_of(&cards, "claude").lacks(READY_TO_TYPE), None);
    assert_eq!(
        card_of(&cards, "opencode").lacks(READY_TO_TYPE).as_deref(),
        Some(
            "opencode cannot have a prompt typed in for you, because charter cannot tell when it \
             has finished starting. See What opencode can do here."
        )
    );
    assert_eq!(
        card_of(&cards, "codex").lacks("reports_waiting").as_deref(),
        Some(
            "Codex does not tell charter when it is waiting, so its chats will not show needs \
             you. See What Codex can do here."
        )
    );
    assert_eq!(
        card_of(&cards, "gemini")
            .lacks("per_chat_plugins")
            .as_deref(),
        Some(
            "Gemini CLI cannot take plugins for one chat alone, so the plugins chosen for this \
             project are not handed to it. See What Gemini CLI can do here."
        )
    );
    // Silence never reads as yes: a declaration that does not say how it resumes.
    let quiet = read(project(&[("quiet.toml", "name = \"quiet\"\nprogram = \"quiet\"\n")]).path());
    assert_eq!(
        card_of(&quiet, "quiet").lacks("resumes_by_id").as_deref(),
        Some(
            "quiet cannot pick a conversation back up by its id, so a chat opened again starts a \
             new conversation, as far as charter knows: its declaration does not say. See What \
             quiet can do here."
        )
    );
    assert_eq!(card_of(&cards, "codex").lacks("no_such_line"), None);
}

#[test]
fn the_lines_are_what_the_harness_lacks_in_the_cards_order() {
    let cards = read(project(&[]).path());

    assert_eq!(
        card_of(&cards, "claude").lines(),
        [
            "Claude Code has no ACP agent charter can talk to, so charter can hand it work only in \
          a terminal."
        ]
    );
    assert_eq!(
        card_of(&cards, "opencode").lines(),
        [
            "opencode does not tell charter which process it reports from, so a chat keeps the \
             first conversation it reports and a later one is not followed.",
            "opencode says nothing until your first prompt, so a new chat looks idle until then.",
            "opencode finds a conversation by its id from any folder, not by the folder it was \
             started in.",
            "opencode cannot take plugins for one chat alone, so the plugins chosen for this \
             project are not handed to it.",
            "opencode cannot have a prompt typed in for you, because charter cannot tell when it \
             has finished starting.",
            "opencode cannot be sandboxed by charter yet, so a project that sandboxes its chats \
             opens none on it.",
        ]
    );
}

/// The words ADR 0072 §3 keeps off the first-hour surfaces — the picker, a chat's header, the
/// palette's rows — where the card's lines are said, and "level", which ADR 0073 §6 keeps off
/// the card. `app/src/firstHour.ts` is the window's copy of the list.
const OUTSIDE_THE_FIRST_HOUR: [&str; 17] = [
    "plane",
    "piece",
    "worktree",
    "run",
    "device",
    "vault",
    "mode",
    "harness",
    "profile",
    "extension",
    "capability",
    "curation",
    "change",
    "member",
    "inventory",
    "strip",
    "level",
];

#[test]
fn every_line_is_in_the_first_hours_words() {
    for words in &LINES {
        for said in [words.label, words.without] {
            let lower = said.to_ascii_lowercase();
            for word in lower.split(|c: char| !c.is_ascii_alphanumeric() && c != '-') {
                let singular = word.strip_suffix('s').unwrap_or(word);
                assert!(
                    !OUTSIDE_THE_FIRST_HOUR.contains(&word)
                        && !OUTSIDE_THE_FIRST_HOUR.contains(&singular),
                    "{:?} says {word:?}, which the first hour never says: {said}",
                    words.id
                );
            }
        }
    }
}

#[test]
fn the_card_tab_draws_its_label_where_it_comes_from_and_a_row_per_line() {
    let codex = built_in_card(Harness::Codex);

    let blocks = codex.blocks();

    assert!(
        matches!(&blocks[0], Block::Note { text, .. } if text == "What Codex can do here"),
        "the card opens with its label: {blocks:?}"
    );
    let Block::Facts(facts) = &blocks[1] else {
        panic!("then where it comes from: {blocks:?}")
    };
    let fact = |label: &str| {
        facts
            .iter()
            .find(|fact| fact.label == label)
            .map(|fact| fact.value.as_str())
    };
    assert_eq!(fact("Program"), Some("codex"));
    assert_eq!(fact("Declared"), Some("shipped with charter"));
    assert_eq!(fact("Measured on"), Some("0.147.0"));
    let Block::List { rows, .. } = &blocks[2] else {
        panic!("then a row per line: {blocks:?}")
    };
    let waiting = rows
        .iter()
        .find(|row| row.key == "reports_waiting")
        .expect("a row for reports_waiting");
    assert_eq!(waiting.text, "Tells charter when it is waiting for you");
    assert_eq!(waiting.note.as_deref(), Some("no"));
    assert_eq!(
        waiting.detail,
        Some(Detail::Text(
            "Codex does not tell charter when it is waiting, so its chats will not show needs \
             you. Why: Codex never says when it stops mid-turn for your approval."
                .to_owned()
        ))
    );
    assert_eq!(rows.len(), LINES.len());
    assert!(
        matches!(&blocks[3], Block::Note { text, .. } if text.starts_with("Codex says nothing")),
        "a Codex card says what its chats cannot tell charter: {blocks:?}"
    );
}

#[test]
fn a_harness_a_project_declares_says_where_and_has_no_sentence_of_charters() {
    let cards = read(project(&[("gemini.toml", GEMINI)]).path());
    let gemini = card_of(&cards, "gemini");

    assert_eq!(gemini.unreported, None);
    let blocks = gemini.blocks();
    let Block::Facts(facts) = &blocks[1] else {
        panic!("facts second: {blocks:?}")
    };
    assert!(facts.iter().any(|fact| fact.label == "Declared"
        && fact.value == "by this project, in harnesses/gemini.toml"));
    let Block::List { rows, .. } = &blocks[2] else {
        panic!("rows third: {blocks:?}")
    };
    let plugins = rows
        .iter()
        .find(|row| row.key == "per_chat_plugins")
        .unwrap();
    assert_eq!(plugins.note.as_deref(), Some("no"));
    assert_eq!(blocks.len(), 3);
}

#[test]
fn every_line_the_card_draws_is_one_charter_reads_and_each_capability_has_one() {
    let ids: Vec<&str> = LINES.iter().map(|words| words.id).collect();
    for capability in harness_declaration::CAPABILITIES {
        assert!(ids.contains(&capability), "no line for {capability}");
    }
    assert_eq!(
        ids.len(),
        harness_declaration::CAPABILITIES.len() + 4,
        "a line that is neither a capability nor hooks, ACP, typing or the sandbox: {ids:?}"
    );
}

/// A harness a project declares answering yes to everything it can be asked.
const SAYS_YES: &str = r#"
name = "eager"
program = "eager"

[session]
resume = ["--resume={id}"]

[terminal]
ready_to_type = "raw-and-quiet"

[levels]
acp = ["eager", "acp"]

[capabilities]
reports_its_process = "yes"
reports_its_start_before_the_first_prompt = "yes"
keeps_conversations_by_directory = "yes"
reports_waiting = "yes"
resumes_by_id = "yes"
per_chat_plugins = "yes"
"#;

/// A declared harness's answer for `id`, from [`SAYS_YES`].
fn eager(id: &str) -> Answer {
    let cards = read(project(&[("eager.toml", SAYS_YES)]).path());
    ability(card_of(&cards, "eager"), id).answer.clone()
}

#[test]
fn a_declared_harness_has_no_hooks_charter_arms_whatever_it_says() {
    assert_eq!(eager(HOOKS), Answer::No(NO_ADAPTER.to_owned()));
}

#[test]
fn a_declared_harness_does_not_say_which_process_it_reports_from_whatever_it_says() {
    assert_eq!(
        eager("reports_its_process"),
        Answer::No(NO_ADAPTER.to_owned())
    );
}

#[test]
fn a_declared_harness_does_not_report_its_start_whatever_it_says() {
    assert_eq!(
        eager("reports_its_start_before_the_first_prompt"),
        Answer::No(NO_ADAPTER.to_owned())
    );
}

#[test]
fn a_declared_harness_does_not_keep_conversations_by_folder_for_charter_whatever_it_says() {
    assert_eq!(
        eager("keeps_conversations_by_directory"),
        Answer::No(NO_ADAPTER.to_owned())
    );
}

#[test]
fn a_declared_harness_does_not_tell_charter_it_is_waiting_whatever_it_says() {
    assert_eq!(eager("reports_waiting"), Answer::No(NO_ADAPTER.to_owned()));
}

#[test]
fn a_declared_harness_takes_no_plugins_for_one_chat_whatever_it_says() {
    assert_eq!(eager("per_chat_plugins"), Answer::No(NO_ADAPTER.to_owned()));
}

#[test]
fn a_declared_harness_is_never_typed_into_whatever_its_terminal_says() {
    assert_eq!(eager(READY_TO_TYPE), Answer::No(NO_ADAPTER.to_owned()));
}

#[test]
fn a_declared_harness_is_never_sandboxed() {
    assert_eq!(eager(SANDBOX), Answer::No(NO_ADAPTER.to_owned()));
}

#[test]
fn a_declared_harness_keeps_what_charter_does_from_a_declaration_alone() {
    // Resuming by its `[session] resume`, and the ACP agent its `[levels] acp` names.
    assert_eq!(eager("resumes_by_id"), Answer::Yes);
    assert_eq!(eager(ACP), Answer::Yes);
}

#[test]
fn every_line_a_card_says_on_a_first_hour_surface_is_in_its_words() {
    // What the picker, a chat's header and an off control actually say, for every built-in
    // and two declared harnesses.
    let cards = read(project(&[("gemini.toml", GEMINI), ("eager.toml", SAYS_YES)]).path());
    assert_eq!(cards.len(), 5);
    for card in &cards {
        let mut said = card.lines();
        said.extend(LINES.iter().filter_map(|words| card.lacks(words.id)));
        said.push(card.label());
        for sentence in said {
            let lower = sentence.to_ascii_lowercase();
            for word in lower.split(|c: char| !c.is_ascii_alphanumeric() && c != '-') {
                let singular = word.strip_suffix('s').unwrap_or(word);
                assert!(
                    !OUTSIDE_THE_FIRST_HOUR.contains(&word)
                        && !OUTSIDE_THE_FIRST_HOUR.contains(&singular),
                    "{}'s card says {word:?}, which the first hour never says: {sentence}",
                    card.name
                );
            }
        }
    }
}

#[test]
fn a_card_draws_a_declarations_words_as_one_clipped_line() {
    // The reader refuses anything else; the card holds every declaration to it all the same.
    let mut declaration = harness_declaration::builtin("claude").unwrap().clone();
    declaration.title = format!("Gem\u{202e}{}", "i".repeat(100));
    declaration.tested = Some("1.0\n2.0".to_owned());

    let card = of(&declaration);

    assert!(!card.title.contains('\u{202e}'), "{}", card.title);
    assert_eq!(
        card.title.chars().count(),
        harness_declaration::MOST_TITLE + 1
    );
    assert_eq!(card.tested.as_deref(), Some("1.0\\x0a2.0"));
}
