//! **Work that reports back has one route, taught in the same words everywhere** (#1515).
//!
//! A chat told "try to dispatch devops" ran a reporting handoff, twice: the handoff skill
//! taught `--report` as the way to get an answer and `--persona` for another persona, while
//! the persona skill taught `purlis dispatch`. Two routes for one job, and a chat picks either.
//!
//! So the route is one sentence, [`purlis_core::handoff::ONE_ROUTE`], and every page a chat
//! learns it from carries that sentence word for word: the handoff skill, the persona skill
//! and `purlis docs show handoff`. A chat's SessionStart briefing and the handoff command's
//! result carry the constant itself (their own tests hold them), and the command's help is
//! held to it in `purlis-cli`.
//!
//! And the handoff skill offers no flag that asks for an answer.

use std::path::{Path, PathBuf};

use purlis_core::handoff::ONE_ROUTE;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn skill(name: &str) -> String {
    let path = repo()
        .join("app/src-tauri/plugin/skills")
        .join(name)
        .join("SKILL.md");
    std::fs::read_to_string(&path).unwrap_or_else(|why| panic!("{}: {why}", path.display()))
}

fn handoff_page() -> String {
    let path = repo().join("crates/purlis-core/docs/handoff.md");
    std::fs::read_to_string(&path).unwrap_or_else(|why| panic!("{}: {why}", path.display()))
}

/// `text` as one line of words: a page wraps its lines and indents a list item's.
fn words(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[test]
fn the_skills_and_the_handoff_page_say_the_route_in_the_same_words() {
    purlis_core::unsteered!();
    for (what, text) in [
        ("the handoff skill", skill("handoff")),
        ("the persona skill", skill("persona")),
        ("docs/handoff.md", handoff_page()),
    ] {
        assert!(
            words(&text).contains(&words(ONE_ROUTE)),
            "{what} does not say: {ONE_ROUTE}"
        );
    }
}

#[test]
fn the_handoff_skill_offers_no_flag_that_asks_for_an_answer() {
    purlis_core::unsteered!();
    let handoff = skill("handoff");
    assert!(
        !handoff.contains("--report"),
        "the handoff skill still names --report"
    );
    assert!(
        !handoff.contains("handoff report"),
        "the handoff skill still teaches a handoff's report back"
    );
    // What it points to instead, for this chat's own persona and for another.
    let said = words(&handoff);
    for route in [
        "purlis dispatch --name \"<task>\"",
        "`--to <persona>`",
        "`--in workspace:<name>`",
    ] {
        assert!(said.contains(route), "the handoff skill names {route}");
    }
}

/// A chat an older build opened owing a report reads this skill too, and its own first message
/// says the chat that asked wants an answer. So the skill does not tell every handed-off chat
/// that nobody is waiting: it sends the chat to the lines under its stamp, and says nobody is
/// waiting only where none of them asks.
#[test]
fn the_handoff_skill_tells_a_handed_off_chat_to_do_what_its_own_first_message_says() {
    purlis_core::unsteered!();
    let said = words(&skill("handoff"));
    let asked = "If one says the chat that handed this off wants an answer, do what that line says";
    let (before, after) = said
        .split_once(asked)
        .expect("the sentence for a chat that owes a report");
    assert!(before.ends_with("**Read the lines under the stamp.** "));
    assert!(after.contains("Otherwise nobody is waiting on a report"));
    // Nowhere does it say so with no condition in front.
    assert_eq!(said.matches("obody is waiting on a report").count(), 1);
}

/// The description is what selects the skill, and it agrees with the body: a handoff may name
/// another persona whose work it is, so what the description rules out is work this chat needs
/// an answer from, whoever's it is.
#[test]
fn the_handoff_skills_description_rules_out_answers_and_not_another_persona() {
    purlis_core::unsteered!();
    let handoff = skill("handoff");
    let description = handoff
        .lines()
        .find(|line| line.starts_with("description: "))
        .expect("a description");
    assert!(
        description.contains(
            "Not for work this chat needs an answer from, its own persona's or another's"
        ),
        "{description}"
    );
    assert!(
        !description.contains("not for giving work to another persona"),
        "{description}"
    );
    assert!(handoff.contains("A handoff may name another persona (`--persona`)"));
}

/// The persona skill is where "dispatch to devops" is answered, so it names the command for
/// it and says which command it is not.
#[test]
fn the_persona_skill_names_the_dispatch_and_not_a_handoff_to_a_persona() {
    purlis_core::unsteered!();
    let said = words(&skill("persona"));
    assert!(said.contains("purlis dispatch --to <persona> --name"));
    assert!(said.contains("never `purlis handoff --persona devops`"));
}

/// The page's own command line offers no `--report`, and its section on the flag says what
/// becomes of one.
#[test]
fn the_handoff_page_shows_no_report_flag_on_the_command_and_says_what_becomes_of_one() {
    purlis_core::unsteered!();
    let page = handoff_page();
    let synopsis = page
        .lines()
        .find(|line| line.starts_with("purlis handoff --name \"<short task>\""))
        .expect("the command's own line");
    assert!(!synopsis.contains("--report"), "{synopsis}");
    assert!(
        words(&page).contains("carried out as a task"),
        "the page says what a --report handoff becomes"
    );
}
