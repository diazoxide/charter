//! The per-file memory store: workspace memory, persona memory and todos all use it.
//!
//! A memory is one Markdown file named after its title, listed in a `MEMORY.md` index
//! beside it. The filename is part of the format — `ws todo done <slug>` closes a todo by
//! its file stem — so the slug rule is reproduced exactly rather than approximated.

/// The filename stem charter derives from a title.
///
/// Every run of characters outside `[a-z0-9]` becomes one `-`, the ends are trimmed, and
/// only then is the result cut to 48 characters — so a cut can leave a trailing `-`, and
/// charter keeps it.
pub fn slug(title: &str) -> String {
    let lowered = title.to_lowercase();
    let mut out = String::with_capacity(lowered.len());
    let mut in_run = false;
    for ch in lowered.chars() {
        if ch.is_ascii_lowercase() || ch.is_ascii_digit() {
            out.push(ch);
            in_run = false;
        } else if !in_run {
            out.push('-');
            in_run = true;
        }
    }
    let trimmed = out.trim_matches('-');
    // The cut is by characters and comes last, so it can leave the trailing `-` that
    // trimming had just removed from the end of the whole string.
    let cut: String = trimmed.chars().take(48).collect();
    if cut.is_empty() {
        "note".to_string()
    } else {
        cut
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Every expectation here came from charter itself: `from charter.memstore import slug`.

    #[test]
    fn a_title_becomes_its_lowercase_words_joined_by_single_dashes() {
        assert_eq!(slug("Hello World"), "hello-world");
        assert_eq!(slug("MiXeD CaSe 123"), "mixed-case-123");
        assert_eq!(
            slug("The API returns 418 on Mondays"),
            "the-api-returns-418-on-mondays"
        );
    }

    #[test]
    fn dashes_at_the_ends_are_trimmed_before_anything_else() {
        assert_eq!(slug("  --Trailing dashes--  "), "trailing-dashes");
    }

    #[test]
    fn a_title_with_nothing_sluggable_in_it_is_called_note() {
        assert_eq!(slug("!!!"), "note");
        assert_eq!(slug(""), "note");
    }

    #[test]
    fn every_non_ascii_letter_is_a_separator_not_a_letter() {
        // Python lowercases first, then replaces anything outside [a-z0-9]; the leading
        // run is trimmed, the inner ones are not.
        assert_eq!(slug("Ünïcödé fäncy"), "n-c-d-f-ncy");
    }

    #[test]
    fn the_cut_to_48_happens_after_trimming_so_it_can_leave_a_trailing_dash() {
        assert_eq!(slug(&"a".repeat(60)), "a".repeat(48));
        assert_eq!(slug(&"x ".repeat(30)), "x-".repeat(24));
    }
}
