//! `shown::readable` against the Python charter's `contain.readable`, value by value.
//!
//! These expectations were taken FROM the oracle, by running it
//! (`PYTHONPATH=. python3 -c "from charter import contain; ..."`), not written from reading
//! it. A refusal sentence quotes a profile name a chat can write, so a value that reaches a
//! report line without this is a value that can repaint the line it is printed on.

use charter_core::shown;

#[test]
fn an_ordinary_name_comes_back_exactly_as_it_was() {
    charter_core::unsteered!();
    assert_eq!(shown::short("claude"), "claude");
}

#[test]
fn a_carriage_return_is_shown_as_an_escape_and_never_redraws_the_line() {
    charter_core::unsteered!();
    // Ruling 35: `charter.local.toml` is a file a chat can write, and a CR in a command
    // would otherwise take the cursor back to the start of the row it was printed on.
    assert_eq!(shown::short("a\rb"), "a\\u000db");
}

#[test]
fn a_codepoint_outside_printable_ascii_is_escaped_whatever_it_is() {
    charter_core::unsteered!();
    // The rule is a complement, so an accented letter and a control character are the same
    // question. Python: 'café' -> 'caf\\u00e9'.
    assert_eq!(shown::short("café"), "caf\\u00e9");
}

#[test]
fn an_astral_codepoint_gets_the_eight_digit_form_so_no_two_values_read_alike() {
    charter_core::unsteered!();
    // `ὠ0` would be five hex digits, which U+1F60 followed by `0` also spells.
    assert_eq!(shown::short("😀"), "\\U0001f600");
}

#[test]
fn a_real_backslash_is_doubled_so_the_escapes_stay_reversible() {
    charter_core::unsteered!();
    // Without this, a name holding the six characters `ㅤ` reads as one holding U+3164.
    assert_eq!(shown::short("a\\b"), "a\\\\b");
}

#[test]
fn a_value_that_renders_as_nothing_is_shown_as_the_blank_marker() {
    charter_core::unsteered!();
    // Decided rather than enumerated: after the escape the string is U+0020..U+007E, where
    // the space is the only character that renders as nothing.
    assert_eq!(shown::short(""), "\"\"");
    assert_eq!(shown::short("   "), "\"\"");
}

#[test]
fn a_value_past_the_budget_is_clipped_with_ascii_dots() {
    charter_core::unsteered!();
    // ASCII dots and not `…`, so "what comes back is printable ASCII" holds for a clipped
    // value too. Python clips to the limit and then appends, so the line is limit + 3.
    let long = "x".repeat(200);

    let out = shown::short(&long);

    assert_eq!(out, "x".repeat(160) + "...");
}

#[test]
fn the_budget_counts_the_escape_and_not_the_value() {
    charter_core::unsteered!();
    // One astral codepoint becomes ten characters, so a budget of twenty holds exactly two
    // of them. Taken from the oracle, which refuted the count this test first carried.
    assert_eq!(
        shown::readable(&"😀".repeat(10), 20),
        "\\U0001f600\\U0001f600..."
    );
}

#[test]
fn spaces_that_reach_the_clip_are_not_the_blank_marker_because_the_dots_are_not_blank() {
    charter_core::unsteered!();
    // The blank test runs AFTER the clip, and that ORDER decides this case: the appended
    // dots are themselves visible, so the clipped value no longer renders as nothing. Also
    // taken from the oracle, which refuted the blank this test first expected.
    assert_eq!(
        shown::readable(&" ".repeat(200), 160),
        " ".repeat(160) + "..."
    );
}
