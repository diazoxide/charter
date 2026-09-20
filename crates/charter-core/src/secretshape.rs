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

/// The kinds, in the order they are asked. Python's `_SECRET_CHECKS`: the label of the FIRST
/// rule that hits anywhere in the text wins, not the earliest hit in the text.
const CHECKS: [(&str, &str); 5] = [
    ("AgentMail key", r"am_us_[A-Za-z0-9]{4,}"),
    ("JWT", r"eyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}"),
    ("private key (PEM)", r"-----BEGIN [A-Z ]*PRIVATE KEY-----"),
    ("AWS access key", r"AKIA[0-9A-Z]{12,}"),
    (
        "credential assignment",
        r"(?i)\b(?:password|passwd|api[_-]?key|apikey|secret|token)\b\s*[:=]\s*(?P<value>\S{6}[^\n]*)",
    ),
];

fn compiled() -> &'static [(&'static str, Regex)] {
    static ONCE: OnceLock<Vec<(&'static str, Regex)>> = OnceLock::new();
    ONCE.get_or_init(|| {
        CHECKS
            .iter()
            .map(|(label, pattern)| {
                (
                    *label,
                    Regex::new(pattern).expect("a pattern this module wrote"),
                )
            })
            .collect()
    })
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
    for (label, rx) in compiled() {
        for caps in rx.captures_iter(text) {
            match caps.name("value") {
                // `rstrip` before the reference test, as Python does: the value runs to the
                // end of the line and a reference does not include the spaces after it.
                Some(value) if names_where_a_credential_lives(value.as_str().trim_end()) => {
                    continue;
                }
                _ => return Some(label),
            }
        }
    }
    None
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

    #[test]
    fn the_first_rule_that_hits_names_the_kind_wherever_in_the_text_it_is() {
        // Python loops rules outside and matches inside, so a JWT at the end of a file still
        // outranks a credential assignment at the top.
        let text = "password: hunter2is\n\neyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIn0\n";
        assert_eq!(secret_kind(text), Some("JWT"));
    }
}
