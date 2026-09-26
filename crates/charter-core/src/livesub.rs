//! Would the SHELL run a command substitution on this line? — `_live_substitution`.
//!
//! A port of `charter/hooks.py`'s `_live_substitution` and the four scanners it walks with:
//! `_ansi_c_end`, `_double_quoted_substitution`, `_heredoc_substitution` and
//! `_heredoc_bodies`. It is the whole judgement A5 ([`crate::proseguard::forge_substitution_hit`])
//! and A6 ([`crate::proseguard::charter_substitution_hit`]) share, and it is its own module for
//! the reason the Python gives for not folding it into either guard: a second copy of a shell
//! quoting walk is a second thing to be wrong about.
//!
//! # Where this is called from
//!
//! [`crate::toolgate`] — `charter/hooks.py:pretooluse`'s eight refusals, in its order — and
//! through it `charter hook pretooluse` (M3.1 stage 6).
//!
//! Until that stage the switch was closed: `main.rs`'s `is_a_tool_hook` answered every word in
//! the `pretooluse`/`posttooluse` namespace with exit 2 — *block* — because a PARTIAL guard on
//! the switch turns fail-closed into allow-everything-except-the-arm-that-is-ported. It moved
//! once all eight arms were standing, and never earlier. The other eight words in that
//! namespace still block, for the same reason they always did.
//! # Why this cannot be [`crate::shellseg`]
//!
//! The Python says it and the port is bound by the same fact. `segment_argv_parsed` opens with
//! [`crate::shellseg::unbacktick`], which rewrites a backtick inside single quotes anyway
//! because the distinction does not matter to the guards it serves — and it is the only
//! distinction that matters here. [`crate::shellseg::Tok`] gets one step closer, recording
//! whether a token was quoted AT ALL, and stops exactly short: ``'…`x`…'`` and ``"…`x`…"`` are
//! both "not bare", and only one of them runs `x`.
//!
//! So this is a plain four-state walk of POSIX 2.2 quoting answering one question, and it stops
//! at the FIRST live substitution — it never has to be right about anything after it. Nesting,
//! operator boundaries, argument attribution and word splitting are all outside what it decides.
//!
//! # Known divergences from a shell, each in the direction of denying MORE
//!
//! * `$((…))` arithmetic reads as `$(` ([`SUBSTITUTIONS`]);
//! * an unterminated quote or heredoc leaves the rest of the string literal, which is what a
//!   shell does with the whole command — it refuses to run it;
//! * a substitution's own contents are never scanned, because the verdict is already in.
//!
//! # One thing is REUSED rather than rewritten, and one is not
//!
//! The heredoc HEADER is [`crate::heredoc::heredoc_header`] — stage 2's port of the same
//! `_heredoc_header` the Python calls here, the same function object, so the two readings of a
//! `<<` opener cannot drift. The heredoc BODY scan is not `_live_substitution` and must not be:
//! a body has no quoting rules but the backslash, so ``'`x`'`` inside one still runs `x`.
//! Reusing the outer walk there would apply single-quote protection where a shell offers none —
//! the fail-OPEN direction, on the exact path the working rule steers agents onto.

use crate::heredoc::{self, Line};
use crate::shellseg::{dollar_opens_quote, past_continuations, spliced_end};

/// The two spellings of command substitution — `_SUBSTITUTIONS`.
///
/// `$(` covers `$((` arithmetic too, which is not a substitution — a false DENY on
/// `--body "$((1+2))"`, and the direction to be wrong in. Separating them would mean deciding
/// `$((x) )` from `$( (x) )`, and a parser that gets that wrong fails OPEN.
///
/// The frozen Python's table, recorded with every corpus row, so bash 5.3's third spelling is
/// [`FUNSUB`] beside it rather than a new entry in it.
pub const SUBSTITUTIONS: [&str; 2] = ["`", "$("];

/// bash 5.3's substitution that runs in the current shell: `${ cmd; }`, and `${| cmd; }`, which
/// substitutes `$REPLY` — `${` followed by a blank, a newline or `|` ([`funsub_at`]). `${VAR}`
/// never is, and an older bash refuses the whole command as a bad substitution, so reading it as
/// live costs nothing where it does not run.
pub const FUNSUB: &str = "${";

/// Whether `cmd` may hold a live substitution at all — the cheap test a hot-path guard asks
/// before [`live_substitution`].
///
/// It must never answer no where the walk would answer yes, so it looks for each spelling with a
/// backslash-newline allowed after the `$`: the shell removes that pair before it reads the line,
/// and `"$\<newline>(x)"` runs `x`.
pub fn may_substitute(cmd: &str) -> bool {
    cmd.contains('`') || cmd.contains("$(") || cmd.contains("${") || cmd.contains("$\\\n")
}

/// Whether a bash 5.3 `${ …; }` or `${| …; }` substitution opens at `i`. See [`FUNSUB`].
pub fn funsub_at(chars: &[char], i: usize) -> bool {
    spliced_end(chars, i, "${").is_some_and(|j| {
        matches!(
            chars.get(past_continuations(chars, j)),
            Some(' ' | '\t' | '\n' | '|')
        )
    })
}

/// The live substitution that opens at `i`, if one does, read as [`spliced_end`] reads.
fn substitution_at(chars: &[char], i: usize) -> Option<&'static str> {
    match chars.get(i) {
        Some('`') => Some("`"),
        Some('$') if spliced_end(chars, i, "$(").is_some() => Some("$("),
        Some('$') if funsub_at(chars, i) => Some(FUNSUB),
        _ => None,
    }
}

/// Index just past the `'` closing a `$'…'` (ANSI-C) quotation opened at `i` — `_ansi_c_end`.
///
/// Its own scanner because a backslash escapes there and does not inside ordinary single quotes:
/// `$'a\'b'` ends at the LAST quote, and reading it as a plain `'…'` ends it at the middle one.
/// Getting that wrong consumes more of the line as quoted than a shell would, which hides a
/// later live backtick — the fail-OPEN direction.
///
/// `i` and the answer are CHARACTER indices, as every offset in this module is: the Python walks
/// a `str`, and a byte index would part company with it on the first non-ASCII character.
///
/// A backslash-newline here is NOT removed — bash keeps both inside `$'…'` — and the backslash
/// rule already steps over the pair.
pub fn ansi_c_end(chars: &[char], mut i: usize) -> usize {
    let n = chars.len();
    while i < n {
        match chars[i] {
            '\\' => i += 2,
            '\'' => return i + 1,
            _ => i += 1,
        }
    }
    n
}

/// `(the substitution opened inside this double-quoted run, index past its close)` —
/// `_double_quoted_substitution`.
///
/// The one state that matters and the reason #703 happened: a backtick is INERT inside single
/// quotes and LIVE inside double quotes, and an apostrophe inside double quotes is an ordinary
/// character rather than the start of a quotation (``"it's `x`"`` runs `x` — checked against
/// bash, not remembered).
///
/// **A backslash here consumes the next character unconditionally, and POSIX's shorter
/// double-quote escape set is deliberately not modelled.** The Python's docstring records the
/// measurement: the three characters this scanner acts on are `$`, `` ` `` and `"`, and all
/// three are in the shell's escape set, so a backslash before a significant character is
/// skipped either way and one before an insignificant character cannot change a verdict by
/// swallowing it. Both spellings were run over 597,871 strings and agreed on all.
pub fn double_quoted_substitution(chars: &[char], mut i: usize) -> (Option<&'static str>, usize) {
    let n = chars.len();
    while i < n {
        if let Some(hit) = substitution_at(chars, i) {
            return (Some(hit), i);
        }
        match chars[i] {
            '\\' => i += 2,
            '"' => return (None, i + 1),
            _ => i += 1,
        }
    }
    // Unterminated: a shell errors, nothing runs.
    (None, n)
}

/// The first live substitution in an EXPANDING heredoc body, or `None` —
/// `_heredoc_substitution`.
///
/// A body has no quoting rules but the backslash — ``'`x`'`` inside one still runs `x`, checked
/// against bash — so this is deliberately not [`live_substitution`]. Reusing that walk would
/// apply single-quote protection where a shell offers none, which is the fail-OPEN direction on
/// the exact path the working rule steers agents onto.
pub fn heredoc_substitution(body: &str) -> Option<&'static str> {
    let chars: Vec<char> = body.chars().collect();
    let n = chars.len();
    let mut i = 0;
    while i < n {
        if let Some(hit) = substitution_at(&chars, i) {
            return Some(hit);
        }
        match chars[i] {
            '\\' => i += 2,
            _ => i += 1,
        }
    }
    None
}

/// One heredoc still waiting for its body: `(delimiter, it expands, `<<-` strips tabs)`.
type Pending = (String, bool, bool);

/// Consume the bodies of the heredocs `pending` on the line that just ended at `i` —
/// `_heredoc_bodies`.
///
/// In order, because a line may open several (``cat <<'A' <<B``) and their bodies follow in the
/// order the headers appeared — checked against bash, since getting the order wrong would read
/// an expanding body as a literal one.
pub fn heredoc_bodies(
    chars: &[char],
    mut i: usize,
    pending: &[Pending],
) -> (Option<&'static str>, usize) {
    let n = chars.len();
    for (delim, expands, strip) in pending {
        let mut lines: Vec<String> = Vec::new();
        while i < n {
            // `text.find("\n", i)`, in characters.
            let end = chars[i..].iter().position(|&c| c == '\n').map(|k| i + k);
            let line: String = match end {
                None => chars[i..].iter().collect(),
                Some(e) => chars[i..e].iter().collect(),
            };
            i = match end {
                None => n,
                Some(e) => e + 1,
            };
            // `line.lstrip("\t")` — TABS only, which is all `<<-` strips.
            let compared = if *strip {
                line.trim_start_matches('\t')
            } else {
                line.as_str()
            };
            if compared == delim {
                break;
            }
            lines.push(line);
        }
        if *expands && let Some(hit) = heredoc_substitution(&lines.join("\n")) {
            return (Some(hit), i);
        }
    }
    (None, i)
}

/// The spelling of the first command substitution in `cmd` **that the shell would run**, or
/// `None` — `_live_substitution`.
///
/// Every rule below was verified against a real `bash` by the Python's author: that the
/// double-quoted backtick expands and the single-quoted one does not, that ``\` `` inside double
/// quotes is inert, that an apostrophe inside double quotes opens nothing, that `<<EOF` expands
/// and `<<'EOF'`, `<<-'EOF'` and `<<\EOF` do not, that quotes inside an expanding body are
/// literal, and that ``<<<"…`x`…"`` expands (a here-STRING is an ordinary double-quoted word, so
/// it is not treated as a heredoc and needs no special case).
///
/// Two more were checked against GNU bash 3.2.57, 5.2.21 and 5.3 when they were added: a
/// backslash-newline is removed before any of this is read, unquoted, inside `"…"` and in an
/// expanding body alike (`$\<newline>(x)` runs `x`; [`spliced_end`]), but not inside `'…'` or
/// `$'…'`; and bash 5.3 runs `${ x; }` ([`FUNSUB`]).
///
/// The return is a `&'static str` rather than a `String` because the answer is one of exactly
/// three fixed spellings — it is the SHAPE that is reported, never a character of the command
/// line, which is the same rule [`crate::credguard::single_credential_hit`]'s shape field keeps
/// and for the same reason: this value reaches a trace file that outlives the conversation.
pub fn live_substitution(cmd: &str) -> Option<&'static str> {
    // A [`Line`] rather than a bare `Vec<char>`, and the cost is stated rather than hidden:
    // `Line::of` also builds a quote map that nothing here reads, because
    // [`heredoc::heredoc_header`] answers from the characters alone. One extra O(n) pass per call
    // buys the SAME header reading stage 2 ported and the differential already compares, instead
    // of a second one to keep in step — and this walk is already O(n) with a `Vec<char>` of its
    // own either way. A second copy of that reading is the #555 shape this file exists to avoid.
    let line = Line::of(cmd);
    let chars = line.chars();
    let n = chars.len();
    let mut i = 0usize;
    let mut pending: Vec<Pending> = Vec::new();
    while i < n {
        let c = chars[i];
        if c == '\\' {
            // The next character is literal, whatever it is.
            i += 2;
        } else if c == '\'' {
            let end = chars[i + 1..]
                .iter()
                .position(|&x| x == '\'')
                .map(|k| i + 1 + k);
            i = match end {
                None => n,
                Some(e) => e + 1,
            };
        } else if c == '"' {
            let (hit, next) = double_quoted_substitution(chars, i + 1);
            i = next;
            if let Some(hit) = hit {
                return Some(hit);
            }
        } else if let Some(hit) = substitution_at(chars, i) {
            return Some(hit);
        } else if let Some(open) =
            spliced_end(chars, i, "$'").filter(|_| dollar_opens_quote(chars, i))
        {
            // A `$'…'` the shell reads, not the tail of `$$` — its own backslash rule ends it.
            i = ansi_c_end(chars, open);
        } else if c == '<' && starts_with(chars, i, "<<<") {
            // A here-STRING, not a heredoc: its word is an ordinary one and the loop must judge
            // it as such — ``<<<"a `x` b"`` runs `x`. All THREE characters are stepped over
            // together, because advancing by one leaves a `<<` for the next iteration to read as
            // a heredoc header whose "delimiter" is the quoted word — which classified the live
            // substitution inside it as an inert body.
            i += 3;
        } else if c == '<' && starts_with(chars, i, "<<") {
            match heredoc::heredoc_header(&line, i) {
                // **`i += 1` here changes no answer, and that is proved rather than measured.**
                // The mutation sweep reported it INERT over 20,000 cases and over the whole
                // recording, and the reason it is a no-op rather than an evidence gap is short:
                // reaching this arm means the `<<<` test above failed, so `chars[i + 2]` is not
                // `<`; the character at `i + 1` is therefore a `<` that begins neither `<<` nor
                // `<<<`, so the next iteration falls through to the final `else` and advances to
                // `i + 2` anyway. Written as `+= 2` because that is what the Python writes and
                // what the construct means, not because the walk needs it.
                None => i += 2,
                Some(h) => {
                    i = h.end;
                    pending.push((h.delim, h.expands, h.dash));
                }
            }
        } else if c == '\n' && !pending.is_empty() {
            let (hit, next) = heredoc_bodies(chars, i + 1, &pending);
            i = next;
            pending.clear();
            if let Some(hit) = hit {
                return Some(hit);
            }
        } else {
            i += 1;
        }
    }
    None
}

/// `text.startswith(s, i)` in characters.
fn starts_with(chars: &[char], i: usize, s: &str) -> bool {
    let n = s.chars().count();
    i + n <= chars.len() && chars[i..i + n].iter().copied().eq(s.chars())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The #703 defect itself: a backtick meant as a markdown code span, inside double quotes.
    #[test]
    fn a_double_quoted_backtick_is_live_and_a_single_quoted_one_is_not() {
        assert_eq!(
            live_substitution("gh issue create --body \"a `env` b\""),
            Some("`")
        );
        assert_eq!(
            live_substitution("gh issue create --body 'a `env` b'"),
            None
        );
    }

    /// An apostrophe inside double quotes opens nothing — the state that makes the row above
    /// decidable at all.
    #[test]
    fn an_apostrophe_inside_double_quotes_is_an_ordinary_character() {
        assert_eq!(live_substitution("x \"it's `y`\""), Some("`"));
    }

    /// A here-STRING is an ordinary double-quoted word. Stepping one character instead of three
    /// leaves a `<<` behind, which then reads as a heredoc whose delimiter is the quoted word —
    /// and the live substitution inside it becomes an inert body.
    #[test]
    fn a_here_string_is_not_a_heredoc() {
        assert_eq!(live_substitution("cat <<<\"a `x` b\""), Some("`"));
    }

    /// An UNQUOTED heredoc body expands; a quoted delimiter in any spelling stops it.
    #[test]
    fn a_heredoc_body_expands_only_when_its_delimiter_is_unquoted() {
        assert_eq!(live_substitution("cat <<EOF\na `x` b\nEOF\n"), Some("`"));
        assert_eq!(live_substitution("cat <<'EOF'\na `x` b\nEOF\n"), None);
        assert_eq!(live_substitution("cat <<\"EOF\"\na `x` b\nEOF\n"), None);
        assert_eq!(live_substitution("cat <<\\EOF\na `x` b\nEOF\n"), None);
        assert_eq!(live_substitution("cat <<EO'F'\na `x` b\nEOF\n"), None);
    }

    /// Quotes inside an expanding body are literal, so the body scan is NOT this walk.
    #[test]
    fn a_quote_inside_an_expanding_body_does_not_protect_a_backtick() {
        assert_eq!(live_substitution("cat <<EOF\n'`x`'\nEOF\n"), Some("`"));
    }

    /// Two heredocs on one line take their bodies in header order.
    #[test]
    fn two_heredocs_take_their_bodies_in_the_order_the_headers_appeared() {
        assert_eq!(
            live_substitution("cat <<'A' <<B\n`x`\nA\n`y`\nB\n"),
            Some("`")
        );
        assert_eq!(
            live_substitution("cat <<B <<'A'\n`y`\nB\n`x`\nA\n"),
            Some("`")
        );
        // …and the LITERAL one's body really is skipped rather than merely reached second.
        assert_eq!(live_substitution("cat <<'A' <<'B'\n`x`\nA\n`y`\nB\n"), None);
    }

    /// `<<-` strips leading TABS only, never spaces — the terminator rule the shared persona
    /// memory records as having shipped wrong twice.
    #[test]
    fn a_dash_heredoc_strips_tabs_and_not_spaces() {
        assert_eq!(live_substitution("cat <<-EOF\n`x`\n\tEOF\n"), Some("`"));
        // A space before the terminator is NOT a terminator, so the body runs on and the
        // backtick after it is still inside an expanding body.
        assert_eq!(live_substitution("cat <<-'EOF'\nq\n EOF\n`x`\nEOF\n"), None);
    }

    /// `$'…'` has its own escape rule: `$'a\'b'` ends at the LAST quote. Reading it as a plain
    /// `'…'` ends it at the middle one and leaves the rest of the line looking quoted, which
    /// hides the backtick after it.
    #[test]
    fn an_ansi_c_quotation_ends_where_its_own_backslash_rule_says() {
        assert_eq!(live_substitution(r"x $'a\'b' `y`"), Some("`"));
        assert_eq!(live_substitution(r"x $'a\'`y`b'"), None);
    }

    /// A bare `$VAR` is not `$'`, and reading it as one skips to the next single quote — over a
    /// live substitution. The second half of that condition is the whole rule; the Python's
    /// deletion sweep found it by mutation.
    #[test]
    fn a_bare_dollar_is_not_an_ansi_c_quotation() {
        assert_eq!(live_substitution("echo $VAR 'q' `x`"), Some("`"));
    }

    /// Arithmetic reads as a substitution — a known divergence, in the deny direction.
    #[test]
    fn arithmetic_reads_as_a_substitution() {
        assert_eq!(live_substitution("echo \"$((1+2))\""), Some("$("));
    }

    /// An unterminated quote leaves the rest literal, which is what a shell does with the whole
    /// command: it refuses to run it.
    #[test]
    fn an_unterminated_quote_leaves_the_rest_literal() {
        assert_eq!(live_substitution("echo 'a `x`"), None);
        assert_eq!(live_substitution("echo \"a `x`"), Some("`"));
    }

    /// A backslash consumes the next character wherever it stands.
    #[test]
    fn a_backslash_makes_the_next_character_literal() {
        assert_eq!(live_substitution(r"echo \`x\`"), None);
        assert_eq!(live_substitution(r#"echo "\`x\`""#), None);
    }

    /// Every offset here is a CHARACTER offset. A byte index would part company with the oracle
    /// on the first astral character before the substitution.
    #[test]
    fn the_walk_counts_characters_and_not_bytes() {
        assert_eq!(live_substitution("echo 𝄞𝄞𝄞 `x`"), Some("`"));
        assert_eq!(live_substitution("echo '𝄞𝄞𝄞 `x`'"), None);
    }

    /// The shell removes a backslash-newline before it reads the line, inside `"…"` as outside
    /// and in an expanding heredoc body, so a `$(` split by one still runs (checked against GNU
    /// bash 3.2.57, 5.2.21 and 5.3). Inside `'…'` and a quoted heredoc body the pair stays.
    #[test]
    fn a_substitution_split_by_a_backslash_newline_is_still_live() {
        for cmd in [
            "x \"$\\\n(y)\"",
            "x $\\\n(y)",
            "x \"$\\\n\\\n(y)\"",
            "cat <<EOF\n$\\\n(y)\nEOF\n",
            // The delimiter the shell reads is `EOF`, unquoted, so the body expands.
            "cat <<E\\\nOF\n$(y)\nEOF\n",
        ] {
            assert_eq!(live_substitution(cmd), Some("$("), "{cmd:?}");
        }
        assert_eq!(live_substitution("x '$\\\n(y)'"), None);
        assert_eq!(live_substitution("cat <<'EOF'\n$\\\n(y)\nEOF\n"), None);
        // A backtick is one character, so nothing can split it, and an escaped one is inert.
        assert_eq!(live_substitution("x \"a\\\n`y`\""), Some("`"));
        assert!(may_substitute("x \"$\\\n(y)\""));
    }

    /// `$\<newline>'…'` is an ANSI-C quotation too, and its own backslash rule decides where it
    /// ends: read as a plain `'…'` it ends early and hides the backtick after it.
    #[test]
    fn an_ansi_c_quotation_split_from_its_dollar_still_ends_where_bash_ends_it() {
        assert_eq!(live_substitution("x $\\\n'a\\'b' `y`"), Some("`"));
    }

    /// `$$'…'` is the PID and a PLAIN single-quoted string, not an ANSI-C one; `$$$'…'` is the
    /// PID and an ANSI-C string. Reading the second `$` of `$$` as opening `$'` gave the
    /// quotation the wrong end and could hide a live backtick after it (checked against bash).
    #[test]
    fn a_dollar_dollar_does_not_open_an_ansi_c_quotation() {
        // Even run: plain single quote, so the `\'` closes at the middle quote and the backtick
        // after it is live.
        assert_eq!(live_substitution(r"echo $$'a\'`x`'"), Some("`"));
        // Odd run: ANSI-C, whose `\'` does not close, so the backtick is inside it and inert.
        assert_eq!(live_substitution(r"echo $$$'a\'`x`'"), None);
    }

    /// bash 5.3 runs `${ cmd; }` and `${| cmd; }`; `${VAR}` is a parameter.
    #[test]
    fn a_bash_5_3_substitution_is_live() {
        for cmd in [
            "x \"${ y; }\"",
            "x ${|y; }",
            "x ${\ty; }",
            "x ${\ny\n}",
            "x \"$\\\n{ y; }\"",
        ] {
            assert_eq!(live_substitution(cmd), Some(FUNSUB), "{cmd:?}");
        }
        assert_eq!(live_substitution("x \"${HOME}\" ${a:-b}"), None);
        assert_eq!(live_substitution("x '${ y; }'"), None);
    }
}
