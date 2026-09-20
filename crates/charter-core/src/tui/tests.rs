//! What `charter/tui.py` promises, asserted against the same strings its own suite uses.

use super::*;

/// A workspace directory name charter never validated, which is how a wide glyph reaches a
/// column at all. Three CJK characters: six columns, three characters.
const CJK: &str = "日本語";
/// `e` plus U+0301 COMBINING ACUTE ACCENT: one column, two characters.
const COMBINED: &str = "e\u{0301}";

#[test]
fn a_column_is_what_python_says_it_is_and_not_a_character_count() {
    assert_eq!(width("charter"), 7);
    // The whole reason the tables exist: a character count answers 3 here and 2 below, and
    // both are wrong by the amount that shears a row.
    assert_eq!(width(CJK), 6);
    assert_eq!(CJK.chars().count(), 3);
    assert_eq!(width(COMBINED), 1);
    assert_eq!(COMBINED.chars().count(), 2);
    // Fullwidth forms are `F` and count two; halfwidth katakana is `H` and counts one.
    assert_eq!(width("Ａ"), 2);
    assert_eq!(width("ｱ"), 1);
    // East-asian AMBIGUOUS is one, which is what lets the box and the ellipsis be measured
    // at all — `…`, `│`, `⬢` are each one column to charter.
    assert_eq!(width("…"), 1);
    assert_eq!(width("│"), 1);
    assert_eq!(width("⬢"), 1);
    // SGR costs nothing.
    assert_eq!(width("\x1b[32mok\x1b[0m"), 2);
}

#[test]
fn sgr_is_the_only_escape_that_survives_and_the_rest_go_whole() {
    // charter's own markup, untouched.
    assert_eq!(sanitize("\x1b[32mok\x1b[0m"), "\x1b[32mok\x1b[0m");
    // An OSC title string goes WHOLE: dropping the introducer alone would print `0;pwned` as
    // ordinary text, which reads as data.
    assert_eq!(sanitize("a\x1b]0;pwned\x07b"), "ab");
    // Unterminated, to the end of the string, for the same reason.
    assert_eq!(sanitize("a\x1b]0;pwned"), "a");
    // Any other CSI — an erase-in-display — is a command, not content.
    assert_eq!(sanitize("a\x1b[2Jb"), "ab");
    // `ESC c` is a full terminal reset and is two characters.
    assert_eq!(sanitize("a\x1bcb"), "ab");
    // A stray introducer goes and the colour after it stays: the alternation's whole point.
    assert_eq!(sanitize("\x1b\x1b[32mok"), "\x1b[32mok");
    // A tab and a newline shear the columns below, so they keep their separation as a space
    // rather than vanishing: `a\tb` stays two words.
    assert_eq!(sanitize("a\tb\nc"), "a b c");
    // Other C0, DEL and C1 are removed outright.
    assert_eq!(sanitize("a\x00b\x07c\x7fd\u{9b}e"), "abcde");
    // Nothing to do is nothing done.
    assert_eq!(sanitize("plain ünïcode"), "plain ünïcode");
}

#[test]
fn width_measures_what_a_terminal_shows_not_what_it_was_handed() {
    // The measurement goes through `sanitize`, so an escape nobody thought of cannot buy
    // itself columns in the arithmetic.
    assert_eq!(width("a\x1b[2Jb"), 2);
    assert_eq!(width("a\x1b]0;a-very-long-title\x07b"), 2);
    assert_eq!(strip_ansi("\x1b[32mok\x1b[0m"), "ok");
}

#[test]
fn truncate_cuts_at_columns_carries_colour_and_closes_it() {
    // Fits: unchanged.
    assert_eq!(truncate("charter", 7), "charter");
    // Cut: the ellipsis takes one of the columns.
    assert_eq!(truncate("charter", 4), "cha…");
    assert_eq!(width(&truncate("charter", 4)), 4);
    // Zero columns is no line at all, not a bare ellipsis.
    assert_eq!(truncate("charter", 0), "");
    // A wide character is never split in half: at a budget of 4 the ellipsis takes one and
    // one more CJK glyph would take two of the three left — so only one fits.
    assert_eq!(truncate(CJK, 4), "日…");
    assert_eq!(truncate(CJK, 5), "日本…");
    // Escapes are carried verbatim at zero width and a reset is appended, so the style
    // cannot bleed into whatever is printed next.
    assert_eq!(truncate("\x1b[32mcharter\x1b[0m", 4), "\x1b[32mcha…\x1b[0m");
    // Sanitised BEFORE the fits-already path: this is charter #326, where an escape reached
    // the screen because nothing ever looked at a string that already fitted.
    assert_eq!(truncate("a\x1b[2Jb", 80), "ab");
}

#[test]
fn pad_fits_exactly_and_never_leaks_a_style_into_the_padding() {
    assert_eq!(pad("ab", 5, Align::Left), "ab   ");
    assert_eq!(pad("ab", 5, Align::Right), "   ab");
    assert_eq!(pad("ab", 5, Align::Center), " ab  ");
    // Long values are cut, not pushed: that is the difference from `{:<5}`.
    assert_eq!(pad("charter", 5, Align::Left), "char…");
    // Columns, not characters — a CJK name pads by three, not by six.
    assert_eq!(pad(CJK, 9, Align::Left), "日本語   ");
    // The spaces go OUTSIDE the colour span.
    assert_eq!(
        pad("\x1b[32mok\x1b[0m", 4, Align::Left),
        "\x1b[32mok\x1b[0m  "
    );
}

#[test]
fn a_finished_line_has_no_trailing_space_even_behind_an_escape() {
    assert_eq!(finish("ab  "), "ab");
    assert_eq!(finish("ab \t"), "ab");
    // The one this exists for: a space hiding behind a trailing reset.
    assert_eq!(finish("ab \x1b[0m"), "ab\x1b[0m");
    // And behind several, repeatedly.
    assert_eq!(finish("ab \x1b[0m \x1b[32m"), "ab\x1b[0m\x1b[32m");
    // A space INSIDE the line is content and stays.
    assert_eq!(finish("a b"), "a b");
}

#[test]
fn a_width_is_a_positive_number_whichever_source_produced_it() {
    let env = |name: &str| match name {
        "COLUMNS" => Some("120".to_string()),
        _ => None,
    };
    assert_eq!(env_columns(&env), Some(120));
    assert_eq!(env_columns(&|_: &str| None::<String>), None);
    assert_eq!(env_columns(&|_: &str| Some("wide".to_string())), None);
    // `int("0")` parses happily, which is the whole of charter #594: what the variable SAYS
    // and whether that is a width are two questions, and only `judged` asks the second.
    assert_eq!(env_columns(&|_: &str| Some("0".to_string())), Some(0));

    // `$COLUMNS` wins when it is usable.
    assert_eq!(judged([Some(120), Some(80)], 80, 24), 120);
    // charter #594, both rungs: a source that answers ZERO has not answered. The env one was
    // guarded by a spelling and the tty one walked straight through it — a pty with no
    // window size reports zero, and charter drew every table one column wide.
    assert_eq!(judged([Some(0), Some(80)], 200, 24), 80);
    assert_eq!(judged([Some(0), Some(0)], 200, 24), 200);
    assert_eq!(judged([Some(-1), None], 200, 24), 200);
    assert_eq!(judged([None, None], 200, 24), 200);
    // The floor is the last word on all three paths, including the default's.
    assert_eq!(judged([Some(5), None], 200, 24), 24);
    assert_eq!(judged([None, Some(5)], 200, 24), 24);
    assert_eq!(judged([None, None], 5, 24), 24);
}

#[test]
fn a_column_is_sized_from_the_values_about_to_be_printed() {
    assert_eq!(column("name", ["ab", "abcdef"], 2, None), 8);
    // The header counts too, so a column is never narrower than its own head.
    assert_eq!(column("name-of-it", ["ab"], 2, None), 12);
    // A cap bounds the CONTENT and never the gap.
    assert_eq!(column("h", ["abcdefghij"], 2, Some(4)), 6);
    // Columns, not characters.
    assert_eq!(column("", [CJK], 0, None), 6);
}

#[test]
fn a_row_pads_its_fixed_cells_and_the_line_is_still_clamped() {
    let row = Node::Row {
        cells: vec![
            Cell::fixed("ab", 6, Align::Left),
            Cell::fixed("9", 3, Align::Right),
            Cell::new("tail"),
        ],
        gap: " ".to_string(),
    };
    assert_eq!(row.render(80), vec!["ab       9 tail"]);
    // Over budget, the assembled line is cut rather than wrapped.
    assert_eq!(row.render(10), vec!["ab       …"]);
}

#[test]
fn columns_share_what_is_left_and_blank_fill_the_shorter_side() {
    let node = Node::Columns {
        columns: vec![
            (Block::Lines(vec!["left".into()]), Some(10)),
            (Block::Lines(vec!["a".into(), "b".into(), "c".into()]), None),
        ],
        gap: "  ".to_string(),
    };
    let out = node.render(20);
    assert_eq!(
        out,
        vec![
            "left        a",
            // The left column is blank-filled, so the right column's rows stay in their own
            // column however tall each side is — ten columns of fixed width and the
            // two-column gap, whether or not the left side has a line to put there.
            "            b",
            "            c",
        ]
    );
}

#[test]
fn the_remainder_of_a_flex_share_goes_to_the_leftmost_columns() {
    // 20 columns, no gaps to pay for beyond the two singles, three flex columns: 20 - 2*1 =
    // 18 to share three ways is 6 each with 0 over. Narrow it by one and the leftmost pays.
    let cols = |w: usize| {
        Node::Columns {
            columns: vec![
                (Block::Lines(vec!["aaaaaaaa".into()]), None),
                (Block::Lines(vec!["bbbbbbbb".into()]), None),
                (Block::Lines(vec!["cccccccc".into()]), None),
            ],
            gap: " ".to_string(),
        }
        .render(w)
    };
    // 18 to share: 6, 6, 6.
    assert_eq!(cols(20), vec!["aaaaa… bbbbb… ccccc…"]);
    // 20 to share: 7, 7, 6 — the two leftmost take the remainder, the third does not. The
    // mutation this is here to catch is spending the remainder on every column or on none.
    assert_eq!(cols(22), vec!["aaaaaa… bbbbbb… ccccc…"]);
}

#[test]
fn a_stack_is_its_children_top_to_bottom_each_clamped_on_its_own() {
    let node = Node::stack(["one".to_string(), "two\nthree".to_string()]);
    assert_eq!(node.render(4), vec!["one", "two", "thr…"]);
}
