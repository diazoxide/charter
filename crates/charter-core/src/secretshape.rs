//! Does this text look like it holds a credential? A port of `charter/hooks.py`'s
//! `_secret_kind` and what it needs.
//!
//! It answers with a KIND — `"JWT"`, `"AWS access key"` — and never with the value, because
//! every caller of it prints the answer. `charter save` is the caller ported here: a staged
//! memory or ref file whose text matches is a commit charter refuses to make, since a memory
//! is pushed to a shared repository and a secret in one is disclosed the moment it lands.
//!
//! **The rules are Python's, character for character**, so the two implementations refuse the
//! same files. Two of them are worth naming:
//!
//! - **The credential-assignment rule is a shape, not a dictionary.** `token: <six or more
//!   non-blank characters>` is a hit whatever the value is — which is why the exemption below
//!   exists at all.
//! - **A vault REFERENCE is not a credential.** `token: "vault:forge/gh"` names where a
//!   secret lives; refusing it would refuse charter's own documented remedy. The exemption is
//!   narrow on purpose: every slot of the reference must be a NAME, so a live token inlined
//!   where a reference was meant (`vault:forge/ghp_…`, or a 40-character hex run) is an
//!   ordinary assignment again.
//!
//! # Why the lookahead is gone and the match is still Python's
//!
//! Python's rule is `…[:=]\s*(?=['"]?\S{6,})(?P<value>[^\n]*)`. `regex` has no lookahead, and
//! rewriting it as "match, then test the value in code" is **not** equivalent: the rewritten
//! pattern consumes the whole line at a position Python rejected, so a real assignment later
//! on the same line would never be reached (`token: x  password: hunter2s`). So the length
//! test is folded into the pattern instead — `(?P<value>\S{6}[^\n]*)` — where a position that
//! fails it is not a match at all and the search carries on from the next character, exactly
//! as `finditer` does. `['"]?` is dropped with it because it never changed an answer: a
//! leading quote is itself non-blank, so the run it starts is at least as long as the run
//! after it, and `\S{6,}` was the whole test.
//!
//! # The three character classes `regex` and CPython disagree about, and what is done here
//!
//! "Character for character" was true of the *pattern text* and false of what it matched.
//! `\s`, `\S`, `\b` and `(?i)` all mean something different in `regex` than in CPython's `re`
//! on a `str`, and M3 found five live divergences across the three of them — **two of them
//! misses**, where this module answered `None` for a line the Python charter refuses, and
//! three false refusals. Each is closed below with a
//! predicate that was swept over all 1,112,064 non-surrogate codepoints against CPython
//! itself rather than argued from the documentation of either engine:
//!
//! - **`\s`**: CPython's is Unicode `White_Space` **plus U+001C–U+001F**, the four separator
//!   controls (and it is exactly `str.isspace()`, so `rstrip` carries the same four).
//!   `regex`'s is `White_Space` alone. [`crate::memstore::is_python_space`] already existed
//!   for this, with a docstring saying the difference is not cosmetic; this module called
//!   `\s` and `trim_end()` anyway. Closed by `SPACE`/`NOT_SPACE` and by `py_rstrip`.
//! - **`\b`**: `regex`'s word character is UTS#18's, which counts a combining mark, a
//!   variation selector and ZWJ as word characters; CPython's is `str.isalnum() or '_'`,
//!   which counts none of them. So `⚠️token: hunter2is` — U+FE0F is `Mn` — had **no word
//!   boundary** in front of `token` here and one in Python: charter refused the line and
//!   charter-app did not. The leading `\b` is therefore gone from the pattern and asked in
//!   code by `is_python_word`, whose class was measured equal to CPython's `\w`.
//! - **`(?i)`**: CPython folds through the simple lowercase mapping plus `_casefix`'s extra
//!   cases; `regex` uses Unicode simple case folding. Swept over every letter of the keyword
//!   set, the two differ for exactly three characters — U+0130 `İ` and U+0131 `ı` (both fold
//!   to `i` for CPython and neither for `regex`), and U+212A `K` and U+017F `ſ` (which both
//!   engines fold). So only `i` needs help, and only where the keywords spell one.
//!
//! The trailing `\b` is kept and is inert either way: what follows the keyword in this
//! pattern is `[\s\x1c-\x1f]*[:=]`, and every character either class can match is a non-word
//! character to both engines, so the boundary is present in both or the match fails in both.

use std::sync::OnceLock;

use regex::Regex;

/// One NAME inside a vault reference: a vault, a key, an item, a field, a path segment.
const REFERENCE_SLOT: &str = r"[A-Za-z0-9_][A-Za-z0-9._-]*";

/// The most characters ALL the names of one reference may hold together and still be read as
/// names. Python's `_REFERENCE_NAMES_MAX`, and the reasoning is there: vault and key names are
/// short words, and the credentials this rule exists for are long random runs.
const REFERENCE_NAMES_MAX: usize = 32;

/// Prefixes that mark a name as a credential whatever its length, so a short or truncated
/// token cannot pass as a vault name. Python's `_CREDENTIAL_PREFIXES`, in its order.
const CREDENTIAL_PREFIXES: [&str; 22] = [
    "ghp_",
    "gho_",
    "ghu_",
    "ghs_",
    "ghr_",
    "github_pat_",
    "glpat-",
    "sk_live_",
    "sk_test_",
    "rk_live_",
    "sk-",
    "xoxa-",
    "xoxb-",
    "xoxp-",
    "xoxr-",
    "xoxs-",
    "AIza",
    "pypi-",
    "npm_",
    "hf_",
    "AKIA",
    "ASIA",
];

/// CPython's `\s`, spelled for `regex`: Unicode `White_Space` and the four separator
/// controls it adds. Measured equal to `re`'s `\s` and to `str.isspace()` over every
/// non-surrogate codepoint.
const SPACE: &str = r"[\s\x1c-\x1f]";

/// CPython's `\S`.
const NOT_SPACE: &str = r"[^\s\x1c-\x1f]";

/// `i` as CPython's `(?i)` reads it: the letter, and the two Turkic spellings `regex`'s
/// simple case folding leaves out. Under `(?i)` this class is `{i, I, ı, İ}`, which is
/// exactly what `re.IGNORECASE` matches for `i`.
const I_CLASS: &str = "[iıİ]";

/// Whether a check's pattern needs CPython's word boundary in front of its match, which
/// [`secret_kind`] asks rather than the engine. Only the keyword rule has one.
type Check = (&'static str, String, bool);

/// The kinds, in the order they are asked. Python's `_SECRET_CHECKS`: the label of the FIRST
/// rule that hits anywhere in the text wins, not the earliest hit in the text.
fn checks() -> [Check; 5] {
    [
        ("AgentMail key", r"am_us_[A-Za-z0-9]{4,}".to_owned(), false),
        (
            "JWT",
            r"eyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}".to_owned(),
            false,
        ),
        (
            "private key (PEM)",
            r"-----BEGIN [A-Z ]*PRIVATE KEY-----".to_owned(),
            false,
        ),
        ("AWS access key", r"AKIA[0-9A-Z]{12,}".to_owned(), false),
        (
            "credential assignment",
            // Built by concatenation, never with a line continuation: a raw string does not
            // process `\` at end of line, so a "continued" pattern carries a literal
            // backslash-newline into the regex. `reference()` below learned the same thing.
            [
                r"(?i)(?:password|passwd|ap".to_owned(),
                format!("{I_CLASS}[_-]?key|ap{I_CLASS}key"),
                r"|secret|token)\b".to_owned(),
                format!("{SPACE}*[:=]{SPACE}*"),
                format!(r"(?P<value>{NOT_SPACE}{{6}}[^\n]*)"),
            ]
            .concat(),
            true,
        ),
    ]
}

fn compiled() -> &'static [(&'static str, Regex, bool)] {
    static ONCE: OnceLock<Vec<(&'static str, Regex, bool)>> = OnceLock::new();
    ONCE.get_or_init(|| {
        checks()
            .into_iter()
            .map(|(label, pattern, boundary)| {
                (
                    label,
                    Regex::new(&pattern).expect("a pattern this module wrote"),
                    boundary,
                )
            })
            .collect()
    })
}

/// Is `c` a word character to CPython's `re` on a `str`?
///
/// `str.isalnum() or c == '_'`, which a sweep of all 1,112,064 non-surrogate codepoints
/// found to be exactly `[\p{L}\p{N}_]` — zero disagreements in either direction. That is
/// why this is a general-category test and not `char::is_alphanumeric`, whose `Alphabetic`
/// half also holds `Other_Alphabetic` combining marks that CPython does not count.
fn is_python_word(c: char) -> bool {
    static ONCE: OnceLock<Regex> = OnceLock::new();
    let word =
        ONCE.get_or_init(|| Regex::new(r"^[\p{L}\p{N}_]$").expect("a pattern this module wrote"));
    let mut buffer = [0u8; 4];
    word.is_match(c.encode_utf8(&mut buffer))
}

/// CPython's `\b` immediately before `at`, where what follows is known to be a word
/// character (every keyword starts with an ASCII letter).
fn python_boundary_before(text: &str, at: usize) -> bool {
    text[..at]
        .chars()
        .next_back()
        .is_none_or(|c| !is_python_word(c))
}

/// The next character boundary after `at`, so a rejected position can be stepped over
/// without splitting a codepoint — `regex`'s `captures_at` takes byte offsets and a search
/// resumed inside one is not a search.
fn after_one_char(text: &str, at: usize) -> usize {
    at + text[at..].chars().next().map_or(1, char::len_utf8)
}

fn reference() -> &'static Regex {
    static ONCE: OnceLock<Regex> = OnceLock::new();
    ONCE.get_or_init(|| {
        // Built by concatenation rather than with a line continuation: a raw string does not
        // process `\` at end of line, so a "continued" pattern would carry a literal
        // backslash-newline into the regex.
        let slot = REFERENCE_SLOT;
        let alternatives = [
            format!(r"vault:(?P<vault>{slot})/(?P<key>{slot})"),
            format!(r"charter secret get (?P<cli_vault>{slot}) (?P<cli_key>{slot})"),
            format!(r"op://(?P<op_vault>{slot})/(?P<op_item>{slot})/(?P<op_field>{slot})"),
            format!(r"vault://(?P<path>{slot}(?:/{slot})*)#(?P<field>{slot})"),
        ]
        .join("|");
        Regex::new(&format!(
            "{}(?:{alternatives}){}",
            r#"\A['"`]?"#, r#"['"`]?\z"#
        ))
        .expect("a pattern this module wrote")
    })
}

/// Whether `value` is exactly a vault reference whose slots hold names, not a credential.
/// Python's `_names_where_a_credential_lives`.
fn names_where_a_credential_lives(value: &str) -> bool {
    let Some(caps) = reference().captures(value) else {
        return false;
    };
    let mut slots: Vec<&str> = [
        "vault",
        "key",
        "cli_vault",
        "cli_key",
        "op_vault",
        "op_item",
        "op_field",
        "field",
    ]
    .into_iter()
    .filter_map(|name| caps.name(name).map(|m| m.as_str()))
    .collect();
    if let Some(path) = caps.name("path") {
        slots.extend(path.as_str().split('/'));
    }
    slots.iter().map(|s| s.len()).sum::<usize>() <= REFERENCE_NAMES_MAX
        && !slots
            .iter()
            .any(|s| CREDENTIAL_PREFIXES.iter().any(|p| s.starts_with(p)))
}

/// The KIND of secret `text` appears to hold, or `None`.
///
/// Every match of a rule that carries a value is asked whether that value merely NAMES where a
/// credential lives, so one exempt assignment does not excuse the next one on the line below.
pub fn secret_kind(text: &str) -> Option<&'static str> {
    for (label, rx, boundary) in compiled() {
        // `finditer`'s own walk, written out, because one position has to be REJECTED and
        // resumed from: a match whose leading word boundary is `regex`'s and not CPython's
        // is not a match at all, and Python's search carries on from the next character —
        // not from the end of the text this engine happened to consume. Resuming at the end
        // instead would step over a real assignment later on the same line, which is the
        // same defect the folded lookahead above exists to avoid.
        let mut at = 0;
        while at <= text.len() {
            let Some(caps) = rx.captures_at(text, at) else {
                break;
            };
            let whole = caps.get(0).expect("group 0 is always set on a match");
            if *boundary && !python_boundary_before(text, whole.start()) {
                at = after_one_char(text, whole.start());
                continue;
            }
            match caps.name("value") {
                // `rstrip` before the reference test, as Python does: the value runs to the
                // end of the line and a reference does not include the spaces after it —
                // and `rstrip` takes the four separator controls `trim_end` leaves behind,
                // which used to make a plain `vault:forge/gh␜` read as a credential.
                Some(value)
                    if names_where_a_credential_lives(crate::memstore::py_rstrip(
                        value.as_str(),
                    )) =>
                {
                    // Every match here is at least six characters, so it can never be
                    // empty and this can never fail to advance.
                    at = whole.end();
                }
                _ => return Some(label),
            }
        }
    }
    None
}

/// The KIND of credential `text` holds by a rule that code does not trip, or `None` — what a
/// workspace repo's save refuses (ADR 0051), where [`secret_kind`]'s credential-assignment rule
/// would refuse `password: String`.
///
/// Only shapes that are a credential and nothing else: a PEM private key block, an AWS access
/// key, an AgentMail key, and a token behind one of [`CREDENTIAL_PREFIXES`] with at least
/// sixteen token characters after it. A JWT is left out: test fixtures carry them by the
/// hundred, and a signed token is not a key to anything by itself.
pub fn token_kind(text: &str) -> Option<&'static str> {
    for (label, rx, _) in compiled() {
        if matches!(
            *label,
            "private key (PEM)" | "AWS access key" | "AgentMail key"
        ) && rx.is_match(text)
        {
            return Some(label);
        }
    }
    static TOKEN: OnceLock<Regex> = OnceLock::new();
    let token = TOKEN.get_or_init(|| {
        let prefixes: Vec<String> = CREDENTIAL_PREFIXES
            .iter()
            .map(|p| regex::escape(p))
            .collect();
        Regex::new(&format!(
            r"(?:^|[^A-Za-z0-9_-])(?:{})[A-Za-z0-9_-]{{16,}}",
            prefixes.join("|")
        ))
        .expect("a pattern this module wrote")
    });
    token
        .is_match(text)
        .then_some("a token by its forge's prefix")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_kind_is_recognised_and_named_without_its_value() {
        for (text, kind) in [
            ("key: am_us_abcd1234", "AgentMail key"),
            ("jwt eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIn0", "JWT"),
            ("-----BEGIN OPENSSH PRIVATE KEY-----", "private key (PEM)"),
            ("aws AKIAIOSFODNN7EXAMPLE", "AWS access key"),
            ("password: hunter2is", "credential assignment"),
            ("API_KEY=abcdefgh", "credential assignment"),
            ("  Token : 'sixchars'", "credential assignment"),
        ] {
            assert_eq!(secret_kind(text), Some(kind), "{text}");
        }
    }

    #[test]
    fn ordinary_prose_and_a_short_value_are_not_a_secret() {
        for text in [
            "a note about the deploy token, without one",
            "token: x",
            "password:",
            "# secret = tbd",
            "",
        ] {
            assert_eq!(secret_kind(text), None, "{text}");
        }
    }

    #[test]
    fn a_vault_reference_is_a_name_and_a_token_inside_one_is_not() {
        for named in [
            "token: vault:forge/gh",
            "token: \"vault:forge/gh\"",
            "token: 'charter secret get forge token'",
            "token: op://plane/forge/token",
            "token: vault://plane/forge#token",
        ] {
            assert_eq!(secret_kind(named), None, "{named}");
        }
        for inlined in [
            // A live token in the slot a name belongs in — the accident the exemption must
            // not cover.
            "token: vault:forge/ghp_0123456789abcdef",
            "token: vault:forge/0123456789abcdef0123456789abcdef01234567",
            "token: vault://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/bbbbbbbbbbbbbbbbbbbbbbbbbbbbbb#c",
        ] {
            assert_eq!(
                secret_kind(inlined),
                Some("credential assignment"),
                "{inlined}"
            );
        }
    }

    #[test]
    fn an_exempt_assignment_does_not_excuse_the_next_one() {
        // Python asks EVERY match, and `continue` is what makes that true. A port that
        // returned `None` on the first exempt match would pass every test above.
        let text = "token: vault:forge/gh\npassword: hunter2is\n";
        assert_eq!(secret_kind(text), Some("credential assignment"));
    }

    #[test]
    fn a_rejected_position_does_not_swallow_the_rest_of_its_line() {
        // The whole reason the lookahead was folded into the pattern rather than tested
        // afterwards. `token: x` fails the six-character rule; `password: hunter2s` on the
        // same line must still be found.
        assert_eq!(
            secret_kind("token: x  password: hunter2s"),
            Some("credential assignment")
        );
    }

    // ----------------------------------------------------------------------------------
    // the four character classes CPython and `regex` disagree about
    //
    // Every expectation below is the PYTHON charter's answer, taken by running
    // `charter.hooks._secret_kind` on the same literal. Two of them were misses here — a
    // line the Python charter refuses and charter-app allowed into a memory file — and two
    // were false refusals.
    // ----------------------------------------------------------------------------------

    #[test]
    fn a_combining_mark_before_the_keyword_does_not_hide_it() {
        // `regex`'s word character is UTS#18's and counts a mark, a variation selector and
        // ZWJ; CPython's does not, so the boundary Python sees in front of `token` was
        // missing here and the whole line passed. U+FE0F is the realistic one: it is the
        // second half of every emoji presentation sequence an agent types into a heading.
        for text in [
            "\u{301}token: hunter2is",
            "\u{200d}token: hunter2is",
            "\u{200c}token: hunter2is",
            "\u{26a0}\u{fe0f}token: hunter2is",
            // `Mc`, and `Other_Alphabetic`. This is the one row `char::is_alphanumeric`
            // still gets wrong — its `Alphabetic` half counts a mark with that property
            // and CPython's `isalnum` does not — so it is what holds the predicate to a
            // general-category test rather than to the convenient one.
            "\u{903}token: hunter2is",
        ] {
            assert_eq!(
                secret_kind(text),
                Some("credential assignment"),
                "{text:?} is a credential assignment to charter"
            );
        }
    }

    #[test]
    fn the_turkic_spellings_of_i_are_the_letter_i_here_as_they_are_to_python() {
        // CPython folds U+0130 and U+0131 to `i`; `regex`'s simple case folding does not,
        // so `apıkey: …` was not an api key here and is one to charter.
        for text in ["ap\u{131}key: hunter2is", "AP\u{130}KEY: hunter2is"] {
            assert_eq!(secret_kind(text), Some("credential assignment"), "{text:?}");
        }
        // And the two `regex` DOES fold, which are not a divergence and are pinned so a
        // narrower class cannot be substituted for `(?i)` without this going red.
        for text in ["\u{17f}ecret: hunter2is", "to\u{212a}en: hunter2is"] {
            assert_eq!(secret_kind(text), Some("credential assignment"), "{text:?}");
        }
    }

    #[test]
    fn the_four_separator_controls_are_blank_here_as_they_are_to_python() {
        // CPython's `\s` holds U+001C–U+001F and `regex`'s does not, so charter ate them as
        // whitespace and this module counted them toward its six non-blank characters —
        // refusing two lines charter allows.
        for text in ["token:\u{1c}\u{1c}\u{1c}\u{1c}ab", "token: ab\u{1c}cdefgh"] {
            assert_eq!(secret_kind(text), None, "{text:?}");
        }
    }

    #[test]
    fn a_vault_reference_with_a_separator_control_after_it_is_still_a_reference() {
        // `rstrip` takes the four; `trim_end` leaves them, so the value no longer matched
        // the reference pattern end to end and charter's own documented remedy was refused.
        assert_eq!(secret_kind("token: vault:forge/gh\u{1c}"), None);
        // And one that is NOT trailing still ends the reference, in both.
        assert_eq!(
            secret_kind("token: vault:forge/gh\u{1d}x"),
            Some("credential assignment")
        );
    }

    #[test]
    fn a_number_that_is_not_a_digit_is_a_word_character_here_as_it_is_to_python() {
        // `²`, `½` and `①` are `No`. CPython counts them as word characters, so there is no
        // boundary in front of `token` and no match; `char::is_alphanumeric` agrees with
        // CPython here only because `is_numeric` covers `No` — which is why the predicate
        // is a general-category test rather than an ad-hoc list.
        for text in [
            "\u{b2}token: hunter2is",
            "\u{bd}token: hunter2is",
            "\u{2460}token: hunter2is",
        ] {
            assert_eq!(secret_kind(text), None, "{text:?}");
        }
    }

    #[test]
    fn rejecting_a_position_does_not_step_over_a_real_assignment_later_on_the_line() {
        // The hazard the hand-written walk introduces, and the reason it resumes one
        // CHARACTER on rather than at the end of what the engine consumed. `xtoken:` is
        // rejected for its boundary; the `password:` after it is charter's answer.
        assert_eq!(
            secret_kind("xtoken: hunter2is  password: hunter2is"),
            Some("credential assignment")
        );
        // And the same with a multi-byte character at the rejected position, which is
        // where a byte-at-a-time resume would split a codepoint and panic.
        assert_eq!(
            secret_kind("\u{2460}token: hunter2is  password: hunter2is"),
            Some("credential assignment")
        );
    }

    #[test]
    fn the_first_rule_that_hits_names_the_kind_wherever_in_the_text_it_is() {
        // Python loops rules outside and matches inside, so a JWT at the end of a file still
        // outranks a credential assignment at the top.
        let text = "password: hunter2is\n\neyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIn0\n";
        assert_eq!(secret_kind(text), Some("JWT"));
    }
}

#[cfg(test)]
mod token_tests {
    use super::token_kind;

    #[test]
    fn a_token_or_a_private_key_is_named_and_ordinary_code_is_not() {
        let key = ["-----BEGIN OPENSSH ", "PRIVATE KEY-----\nabc\n"].concat();
        assert_eq!(token_kind(&key), Some("private key (PEM)"));
        let gh = ["let t = \"ghp", "_0123456789abcdefABCDEF\";"].concat();
        assert_eq!(token_kind(&gh), Some("a token by its forge's prefix"));
        for code in [
            "struct Login { password: String, token: Option<String> }",
            "let key = \"sk-short\";",
            "fn mask_ghp_prefix() {}",
            "api_key = os.environ['API_KEY']",
        ] {
            assert_eq!(token_kind(code), None, "{code}");
        }
    }
}
