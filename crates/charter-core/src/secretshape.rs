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
/// token cannot pass as a vault name, and that mark a bare token as a credential wherever it
/// stands ([`prefixed_token`]). Python's `_CREDENTIAL_PREFIXES` in its order, then the
/// documented prefixes it lacked, each from its provider's own documentation:
///
/// - GitHub, "GitHub's token formats" in
///   <https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/about-authentication-to-github>:
///   `ghp_` a personal access token (classic), `gho_` an OAuth access token, `ghu_` a GitHub
///   App user access token, `ghs_` a GitHub App installation access token, `ghr_` a GitHub App
///   refresh token — each the prefix and 36 characters — and `github_pat_` a fine-grained
///   personal access token.
/// - GitLab, "Token prefixes" in <https://docs.gitlab.com/security/tokens/>: `glpat-` a
///   personal, impersonation, project or group access token, `gloas-` an OAuth application
///   secret, `gldt-` a deploy token, `glrt-` and `glrtr-` a runner authentication token,
///   `glcbt-` a CI/CD job token, `glptt-` a trigger token, `glft-` a feed token, `glimt-` an
///   incoming mail token, `glagent-` an agent for Kubernetes token, `glwt-` a workspace token,
///   `glsoat-` a SCIM token, `glffct-` a feature flags client token.
/// - Slack, <https://docs.slack.dev/authentication/tokens>: `xoxb-` a bot token, `xoxp-` a
///   user token, `xapp-` an app-level token (a rotating one is `xoxe.xapp-`), `xwfp-` a
///   workflow token.
const CREDENTIAL_PREFIXES: [&str; 36] = [
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
    "gloas-",
    "gldt-",
    "glrt-",
    "glrtr-",
    "glcbt-",
    "glptt-",
    "glft-",
    "glimt-",
    "glagent-",
    "glwt-",
    "glsoat-",
    "glffct-",
    "xapp-",
    "xwfp-",
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

/// Which of [`checks`] a rule is, so a caller picks rules by what they are and not by how their
/// label is spelled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Rule {
    AgentMail,
    Jwt,
    PrivateKey,
    AwsKey,
    Assignment,
}

impl Rule {
    /// The rule's stable id, what an allowlist entry names.
    fn id(self) -> &'static str {
        match self {
            Self::AgentMail => "agentmail-key",
            Self::Jwt => "jwt",
            Self::PrivateKey => "private-key",
            Self::AwsKey => "aws-access-key",
            Self::Assignment => "credential-assignment",
        }
    }

    /// A credential and nothing else: what a code repository's save and a commit's scan ask,
    /// where the assignment rule would refuse `password: String` and a JWT is a test fixture.
    fn is_a_key(self) -> bool {
        matches!(self, Self::AgentMail | Self::PrivateKey | Self::AwsKey)
    }
}

/// A rule, its label, its pattern, and whether the pattern needs CPython's word boundary in
/// front of its match, which [`secret_kind`] asks rather than the engine. Only the keyword
/// rule has one.
type Check = (Rule, &'static str, String, bool);

/// The kinds, in the order they are asked. Python's `_SECRET_CHECKS`: the label of the FIRST
/// rule that hits anywhere in the text wins, not the earliest hit in the text.
fn checks() -> [Check; 5] {
    [
        (
            Rule::AgentMail,
            "AgentMail key",
            r"am_us_[A-Za-z0-9]{4,}".to_owned(),
            false,
        ),
        (
            Rule::Jwt,
            "JWT",
            r"eyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}".to_owned(),
            false,
        ),
        (
            Rule::PrivateKey,
            "private key (PEM)",
            r"-----BEGIN [A-Z ]*PRIVATE KEY-----".to_owned(),
            false,
        ),
        (
            Rule::AwsKey,
            "AWS access key",
            r"AKIA[0-9A-Z]{12,}".to_owned(),
            false,
        ),
        (
            Rule::Assignment,
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

fn compiled() -> &'static [(Rule, &'static str, Regex, bool)] {
    static ONCE: OnceLock<Vec<(Rule, &'static str, Regex, bool)>> = OnceLock::new();
    ONCE.get_or_init(|| {
        checks()
            .into_iter()
            .map(|(rule, label, pattern, boundary)| {
                (
                    rule,
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
            // Either name the command line answers to (RN-3).
            format!(r"(?:charter|purlis) secret get (?P<cli_vault>{slot}) (?P<cli_key>{slot})"),
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
/// A bare token behind one of [`CREDENTIAL_PREFIXES`] is asked last, so every guard built on
/// this answer — a settings save, a written memory, a session record, a handoff brief — refuses
/// the same tokens [`found`] does, and every kind the Python rules named stays the one named.
pub fn secret_kind(text: &str) -> Option<&'static str> {
    secret_at(text)
        .or_else(|| token_at(text))
        .map(|(label, _)| label)
}

/// [`secret_kind`], with the byte offset of the match that decided it — the one walk both
/// answer from, so the kind and where it is can never disagree.
fn secret_at(text: &str) -> Option<(&'static str, usize)> {
    for (_, label, rx, boundary) in compiled() {
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
                _ => return Some((label, whole.start())),
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
    token_at(text).map(|(label, _)| label)
}

/// [`token_kind`], with the byte offset of the match that decided it.
fn token_at(text: &str) -> Option<(&'static str, usize)> {
    for (rule, label, rx, _) in compiled() {
        if rule.is_a_key()
            && let Some(hit) = rx.find(text)
        {
            return Some((label, hit.start()));
        }
    }
    let token = prefixed_token();
    token
        .captures(text)
        .and_then(|caps| caps.name("token"))
        .map(|hit| (FORGE_TOKEN.1, hit.start()))
}

/// A credential [`found`] in a text: its kind, and the 1-based line it starts on. Never the
/// value, since every caller prints what it gets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Found {
    pub kind: &'static str,
    pub line: usize,
}

/// The widest question this module answers: [`secret_kind`]'s rules, then [`token_kind`]'s
/// prefix rule for a bare token no assignment names. What a plane save refuses in any staged
/// file, and what a report redacts a line for.
pub fn found(text: &str) -> Option<Found> {
    let (kind, at) = secret_at(text).or_else(|| token_at(text))?;
    let line = 1 + text.as_bytes()[..at]
        .iter()
        .filter(|b| **b == b'\n')
        .count();
    Some(Found { kind, line })
}

/// One thing a commit's added line would publish: its kind, and where on the line it is.
///
/// The span is for the caller to MASK ([`masked`]) and never to print as it is: every caller
/// shows what it gets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Leak {
    /// The rule's stable id, what an allowlist entry names (`email`, `forge-token`, …).
    pub rule: &'static str,
    /// What it looks like, in words.
    pub kind: &'static str,
    pub span: std::ops::Range<usize>,
}

/// The forge-prefix rule, by its id and its words.
const FORGE_TOKEN: (&str, &str) = ("forge-token", "a token by its forge's prefix");

/// Every rule [`leaks`] asks, by the id an allowlist entry names it with and the words a
/// refusal uses for it, in the order they are asked. `charter scan --explain` prints this.
pub fn leak_rules() -> Vec<(&'static str, &'static str)> {
    let keys = compiled()
        .iter()
        .filter(|(rule, ..)| rule.is_a_key())
        .map(|(rule, kind, ..)| (rule.id(), *kind));
    let vendors = gitleaks().iter().map(|(id, kind, _)| (*id, *kind));
    let personal = pii().iter().map(|(id, kind, ..)| (*id, *kind));
    keys.chain(std::iter::once(FORGE_TOKEN))
        .chain(vendors)
        .chain(personal)
        .collect()
}

/// Whether rule `id` finds personal data — a value a hash of it can be reversed from by
/// guessing, so an allowlist never names one by its fingerprint.
pub fn is_personal(id: &str) -> bool {
    pii().iter().any(|(rule, ..)| *rule == id)
}

/// Credential shapes from gitleaks' default rules that neither [`CREDENTIAL_PREFIXES`] nor the
/// PEM, AWS and AgentMail rules already cover. Each is anchored on a vendor's own prefix, so
/// ordinary code does not match one; gitleaks' generic and entropy-only rules are left out for
/// the reason [`token_kind`] leaves the credential-assignment rule out.
const GITLEAKS: [(&str, &str, &str); 9] = [
    (
        "slack-webhook",
        "Slack webhook",
        r"\bhooks\.slack\.com/(?:services|workflows|triggers)/[A-Za-z0-9+/]{43,56}",
    ),
    ("sendgrid-key", "SendGrid key", r"\bSG\.[A-Za-z0-9=_.-]{66}"),
    (
        "digitalocean-token",
        "DigitalOcean token",
        r"\bdo[por]_v1_[a-f0-9]{64}",
    ),
    (
        "shopify-token",
        "Shopify token",
        r"\bshp(?:at|ca|pa|ss)_[a-fA-F0-9]{32}",
    ),
    ("linear-key", "Linear key", r"\blin_api_[A-Za-z0-9]{40}"),
    (
        "postman-key",
        "Postman key",
        r"\bPMAK-[a-fA-F0-9]{24}-[a-fA-F0-9]{34}",
    ),
    (
        "doppler-token",
        "Doppler token",
        r"\bdp\.pt\.[A-Za-z0-9]{43}",
    ),
    (
        "databricks-token",
        "Databricks token",
        r"\bdapi[a-f0-9]{32}",
    ),
    (
        "grafana-token",
        "Grafana token",
        r"\b(?:glc_[A-Za-z0-9+/]{32,400}={0,2}|glsa_[A-Za-z0-9]{32}_[A-Fa-f0-9]{8})",
    ),
];

/// How a PII rule's candidate is checked beyond its pattern.
#[derive(Clone, Copy)]
enum Personal {
    Email,
    Card,
    Ssn,
}

type PiiRule = (&'static str, &'static str, Regex, Personal);

fn pii() -> &'static [PiiRule] {
    static ONCE: OnceLock<Vec<PiiRule>> = OnceLock::new();
    ONCE.get_or_init(|| {
        [
            (
                "email",
                "an email address",
                r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)*\.[A-Za-z]{2,}\b",
                Personal::Email,
            ),
            (
                "card-number",
                "a card number",
                r"\b(?:\d[ -]?){12,18}\d\b",
                Personal::Card,
            ),
            (
                "us-ssn",
                "an ID number (US SSN)",
                r"\b\d{3}-\d{2}-\d{4}\b",
                Personal::Ssn,
            ),
        ]
        .into_iter()
        .map(|(id, kind, pattern, check)| {
            (
                id,
                kind,
                Regex::new(pattern).expect("a pattern this module wrote"),
                check,
            )
        })
        .collect()
    })
}

fn gitleaks() -> &'static [(&'static str, &'static str, Regex)] {
    static ONCE: OnceLock<Vec<(&'static str, &'static str, Regex)>> = OnceLock::new();
    ONCE.get_or_init(|| {
        GITLEAKS
            .iter()
            .map(|(id, kind, pattern)| {
                (
                    *id,
                    *kind,
                    Regex::new(pattern).expect("a pattern this module wrote"),
                )
            })
            .collect()
    })
}

fn prefixed_token() -> &'static Regex {
    static TOKEN: OnceLock<Regex> = OnceLock::new();
    TOKEN.get_or_init(|| {
        let prefixes: Vec<String> = CREDENTIAL_PREFIXES
            .iter()
            .map(|p| regex::escape(p))
            .collect();
        // The token is its own group: the class in front of it consumes the character before
        // it, which is the newline when a token opens its line.
        Regex::new(&format!(
            r"(?:^|[^A-Za-z0-9_-])(?P<token>(?:{})[A-Za-z0-9_-]{{16,}})",
            prefixes.join("|")
        ))
        .expect("a pattern this module wrote")
    })
}

/// An address that names nobody: a documentation or private-use domain (RFC 2606, RFC 6761,
/// `.internal`, `.local`, `localhost.localdomain`), a no-reply sender, `git@<host>`, which is an
/// SSH remote and not a mailbox, or an image's scale suffix (`logo@2x.png`).
fn names_nobody(address: &str) -> bool {
    let (local, domain) = address.rsplit_once('@').unwrap_or(("", address));
    let (local, domain) = (local.to_ascii_lowercase(), domain.to_ascii_lowercase());
    let reserved = [
        "example.com",
        "example.net",
        "example.org",
        "noreply.github.com",
    ]
    .iter()
    .any(|d| domain == *d || domain.ends_with(&format!(".{d}")));
    let tld = domain.rsplit('.').next().unwrap_or("");
    reserved
        || matches!(
            tld,
            "test" | "invalid" | "localhost" | "localdomain" | "example" | "local" | "internal"
        )
        || local.starts_with("noreply")
        || local.starts_with("no-reply")
        || local == "git"
        || a_scale_suffix(&domain)
}

/// `2x.png`, `3x.webp`, `1.5x.jpg`: what follows the `@` in an image asset's name. Only an
/// image's extension, so `ada@2x.dev` is still somebody's address.
fn a_scale_suffix(domain: &str) -> bool {
    static ONCE: OnceLock<Regex> = OnceLock::new();
    ONCE.get_or_init(|| {
        Regex::new(r"^[0-9]+(?:\.[0-9]+)?x\.(?:png|jpe?g|gif|webp|avif|svg|ico|bmp|tiff?|heic)$")
            .expect("a pattern this module wrote")
    })
    .is_match(domain)
}

/// A card number: 13 to 19 digits under a known issuer prefix, passing the Luhn check.
fn a_card(candidate: &str) -> bool {
    let digits: Vec<u32> = candidate.chars().filter_map(|c| c.to_digit(10)).collect();
    let n = digits.len();
    let head = |k: usize| digits.iter().take(k).fold(0, |acc, d| acc * 10 + d);
    let issuer = match digits.first() {
        Some(4) => matches!(n, 13 | 16 | 19),
        Some(3) => n == 15 && matches!(head(2), 34 | 37),
        Some(5) => n == 16 && (51..=55).contains(&head(2)),
        Some(2) => n == 16 && (2221..=2720).contains(&head(4)),
        Some(6) => (16..=19).contains(&n) && (head(4) == 6011 || head(2) == 65),
        _ => false,
    };
    let luhn = digits
        .iter()
        .rev()
        .enumerate()
        .map(|(i, d)| match (i % 2, d * 2) {
            (0, _) => *d,
            (_, doubled) if doubled > 9 => doubled - 9,
            (_, doubled) => doubled,
        })
        .sum::<u32>()
        % 10
        == 0;
    issuer && luhn
}

/// A US SSN the Social Security Administration could have issued: no area 000, 666 or 9xx, no
/// group 00, no serial 0000.
fn an_ssn(candidate: &str) -> bool {
    let mut parts = candidate.split('-');
    let (Some(area), Some(group), Some(serial)) = (parts.next(), parts.next(), parts.next()) else {
        return false;
    };
    area != "000" && area != "666" && !area.starts_with('9') && group != "00" && serial != "0000"
}

/// Everything on one line a commit would publish: [`token_kind`]'s shapes, gitleaks' vendor
/// shapes ([`GITLEAKS`]), and personal data — an email address, a card number, an ID number.
/// In the order they sit on the line; where two overlap, the earlier one is kept.
///
/// What an agent's diff is checked with before it is committed (SQ-16). **Not** the plane
/// save's rule ([`found`]): that one refuses `password: String`, which is ordinary code.
pub fn leaks(line: &str) -> Vec<Leak> {
    let mut all = every_leak(line);
    all.sort_by_key(|leak| (leak.span.start, std::cmp::Reverse(leak.span.end)));
    let mut kept: Vec<Leak> = Vec::new();
    for leak in all {
        if kept
            .last()
            .is_none_or(|last| leak.span.start >= last.span.end)
        {
            kept.push(leak);
        }
    }
    kept
}

/// Every stretch of `line` that any of [`leaks`]' rules matches, overlapping ones joined into
/// one, in order, each with the kind of the rule that starts it. For a caller that masks:
/// [`leaks`] keeps the earlier of two overlapping hits whole, which can leave the tail of the
/// later one outside every span.
pub fn leak_spans(line: &str) -> Vec<(std::ops::Range<usize>, &'static str)> {
    joined(every_leak(line))
}

/// `all`'s spans in order, each run of overlapping ones joined into one.
fn joined(mut all: Vec<Leak>) -> Vec<(std::ops::Range<usize>, &'static str)> {
    all.sort_by_key(|leak| (leak.span.start, std::cmp::Reverse(leak.span.end)));
    let mut joined: Vec<(std::ops::Range<usize>, &'static str)> = Vec::new();
    for leak in all {
        match joined.last_mut() {
            Some((span, _)) if leak.span.start < span.end => span.end = span.end.max(leak.span.end),
            _ => joined.push((leak.span, leak.kind)),
        }
    }
    joined
}

/// Every hit of every rule [`leaks`] runs, overlaps and all, in no order.
fn every_leak(line: &str) -> Vec<Leak> {
    let mut all: Vec<Leak> = Vec::new();
    for (rule, kind, rx, _) in compiled() {
        if rule.is_a_key() {
            all.extend(rx.find_iter(line).map(|hit| Leak {
                rule: rule.id(),
                kind,
                span: hit.range(),
            }));
        }
    }
    all.extend(
        prefixed_token()
            .captures_iter(line)
            .filter_map(|caps| caps.name("token"))
            .map(|hit| Leak {
                rule: FORGE_TOKEN.0,
                kind: FORGE_TOKEN.1,
                span: hit.range(),
            }),
    );
    for (id, kind, rx) in gitleaks() {
        all.extend(rx.find_iter(line).map(|hit| Leak {
            rule: id,
            kind,
            span: hit.range(),
        }));
    }
    for (id, kind, rx, check) in pii() {
        all.extend(
            rx.find_iter(line)
                .filter(|hit| match check {
                    Personal::Email => !names_nobody(hit.as_str()),
                    Personal::Card => a_card(hit.as_str()),
                    Personal::Ssn => an_ssn(hit.as_str()),
                })
                .map(|hit| Leak {
                    rule: id,
                    kind,
                    span: hit.range(),
                }),
        );
    }
    all
}

/// The most characters of a masked value drawn; a longer one says its length instead.
const MASK_WIDTH: usize = 32;

/// `value` with all but a short head replaced by `*`, for a caller that has to say WHICH
/// value it refused without saying what it is. A value under twelve characters keeps nothing.
pub fn masked(value: &str) -> String {
    let n = value.chars().count();
    let keep = if n < 12 { 0 } else { (n / 6).min(4) };
    let shown = n.min(MASK_WIDTH);
    let mut out: String = value.chars().take(keep).collect();
    out.push_str(&"*".repeat(shown - keep));
    if n > MASK_WIDTH {
        out.push_str(&format!("… ({n} characters)"));
    }
    out
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
            // The name the command line ships as since RN-3.
            "token: 'purlis secret get forge token'",
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

#[cfg(test)]
mod found_tests {
    use super::{Found, found};

    #[test]
    fn a_bare_token_in_prose_is_found_by_its_prefix() {
        let text = [
            "# notes\n\nthe deploy uses ghp",
            "_0123456789abcdefABCDEF now\n",
        ]
        .concat();
        assert_eq!(
            found(&text),
            Some(Found {
                kind: "a token by its forge's prefix",
                line: 3
            })
        );
    }

    #[test]
    fn the_line_is_where_the_value_starts_even_at_the_start_of_a_line() {
        // The token rule's leading class consumes the newline before a token that opens its
        // line; the line named is the token's, not the one above it.
        let text = ["one\ntwo\nglpat", "-0123456789abcdefABCD\n"].concat();
        assert_eq!(found(&text).map(|f| f.line), Some(3));
        let text = "one\n\n\npassword: hunter2is\n";
        assert_eq!(
            found(text),
            Some(Found {
                kind: "credential assignment",
                line: 4
            })
        );
        let key = ["a\n-----BEGIN RSA ", "PRIVATE KEY-----\n"].concat();
        assert_eq!(found(&key).map(|f| f.line), Some(2));
    }

    #[test]
    fn the_broad_rules_are_asked_first_and_their_kind_is_the_one_named() {
        // `secret_kind`'s answer wins where it has one, so a file both rules match is named
        // the way every other refusal names it.
        let text = ["token: ghp", "_0123456789abcdefABCDEF\n"].concat();
        assert_eq!(found(&text).map(|f| f.kind), Some("credential assignment"));
    }

    #[test]
    fn ordinary_text_and_a_vault_reference_are_not_found() {
        for text in [
            "a note about the deploy token, without one",
            "token: vault:forge/gh",
            "fn mask_ghp_prefix() {}",
            "",
        ] {
            assert_eq!(found(text), None, "{text}");
        }
    }
}

#[cfg(test)]
mod leak_tests {
    use super::{leaks, masked};

    /// Each line's kinds, in the order they sit on the line.
    fn kinds(line: &str) -> Vec<&'static str> {
        leaks(line).into_iter().map(|leak| leak.kind).collect()
    }

    #[test]
    fn a_key_and_an_email_on_an_added_line_are_each_named_with_where_they_are() {
        let line = [
            "let k = \"ghp",
            "_0123456789abcdefABCDEF\"; // ada@lovelace.dev",
        ]
        .concat();
        let found = leaks(&line);
        assert_eq!(
            found.iter().map(|l| l.kind).collect::<Vec<_>>(),
            vec!["a token by its forge's prefix", "an email address"]
        );
        assert_eq!(
            &line[found[0].span.clone()],
            ["ghp", "_0123456789abcdefABCDEF"].concat()
        );
        assert_eq!(&line[found[1].span.clone()], "ada@lovelace.dev");
    }

    #[test]
    fn the_gitleaks_shapes_a_forge_prefix_does_not_cover_are_found() {
        for (line, kind) in [
            (
                [
                    "https://hooks.slack.com/services/T0000000",
                    "0/B00000000/XXXXXXXXXXXXXXXXXXXXXXXX",
                ]
                .concat(),
                "Slack webhook",
            ),
            (
                [
                    "SG.abcdefghijklmnopqrstuv",
                    ".abcdefghijklmnopqrstuvwxyz0123456789ABCDEFG",
                ]
                .concat(),
                "SendGrid key",
            ),
            (["dop_v1_", &"a1".repeat(32)].concat(), "DigitalOcean token"),
            (["shpat_", &"a1".repeat(16)].concat(), "Shopify token"),
            (["lin_api_", &"A".repeat(40)].concat(), "Linear key"),
            (
                ["PMAK-", &"a".repeat(24), "-", &"b".repeat(34)].concat(),
                "Postman key",
            ),
            (
                ["-----BEGIN RSA ", "PRIVATE KEY-----"].concat(),
                "private key (PEM)",
            ),
            (["AKIA", "IOSFODNN7EXAMPLE"].concat(), "AWS access key"),
        ] {
            assert_eq!(kinds(&line), vec![kind], "{line}");
        }
    }

    #[test]
    fn every_leak_names_the_rule_an_allowlist_entry_would_name() {
        let line = [
            "let k = \"ghp",
            "_0123456789abcdefABCDEF\"; // ada@lovelace.dev",
        ]
        .concat();
        let rules: Vec<&str> = leaks(&line).into_iter().map(|l| l.rule).collect();
        assert_eq!(rules, ["forge-token", "email"]);
        let ids: Vec<&str> = super::leak_rules().iter().map(|(id, _)| *id).collect();
        assert_eq!(ids.len(), 16);
        let mut unique = ids.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(ids.len(), unique.len(), "every rule id is its own");
    }

    #[test]
    fn a_card_number_is_found_only_when_it_passes_luhn() {
        assert_eq!(kinds("card 4111 1111 1111 1111 ok"), vec!["a card number"]);
        assert_eq!(kinds("card 5500-0000-0000-0004"), vec!["a card number"]);
        assert_eq!(kinds("card 4111 1111 1111 1112"), Vec::<&str>::new());
        // A timestamp or an id is digits and nothing else.
        assert_eq!(
            kinds("at 1727712000123 ms, id 9876543210123456"),
            Vec::<&str>::new()
        );
    }

    #[test]
    fn a_social_security_number_is_found_and_an_impossible_one_is_not() {
        assert_eq!(kinds("ssn: 123-45-6789"), vec!["an ID number (US SSN)"]);
        for line in [
            "000-12-3456",
            "666-12-3456",
            "900-12-3456",
            "123-00-4567",
            "123-45-0000",
        ] {
            assert_eq!(kinds(line), Vec::<&str>::new(), "{line}");
        }
    }

    #[test]
    fn a_real_domain_is_still_an_email_even_beside_a_placeholder() {
        assert_eq!(
            kinds("a@example.com and ada@lovelace.dev"),
            vec!["an email address"]
        );
        assert_eq!(kinds("ada@2x.dev"), vec!["an email address"]);
    }

    #[test]
    fn a_placeholder_or_noreply_address_is_not_somebodys_email() {
        for line in [
            "user@example.com",
            "a@b.example.org",
            "x@service.test",
            "x@host.invalid",
            "root@localhost",
            "noreply@anthropic.com",
            "no-reply@github.com",
            "1234+someone@users.noreply.github.com",
            "logo@2x.png",
            "<img src=\"icon@3x.webp\">",
            "hero@1.5x.jpg",
            "admin@localhost.localdomain",
            "svc@db.internal",
            "me@printer.local",
            "x@y.test",
            "x@docs.example",
            "x@nowhere.invalid",
        ] {
            assert_eq!(kinds(line), Vec::<&str>::new(), "{line}");
        }
    }

    #[test]
    fn ordinary_code_is_not_a_leak() {
        for line in [
            "struct Login { password: String, token: Option<String> }",
            "api_key = os.environ['API_KEY']",
            "let version = \"1.2.3-4\";",
            "@media (max-width: 600px) {",
            "import foo from '@scope/pkg@1.2.3';",
            "git clone git@github.com:acme/app.git",
        ] {
            assert_eq!(kinds(line), Vec::<&str>::new(), "{line}");
        }
    }

    #[test]
    fn a_masked_value_keeps_a_short_head_and_hides_the_rest() {
        assert_eq!(masked("ada@lovelace.dev"), "ad**************");
        assert_eq!(
            masked(&["ghp", "_0123456789abcdefABCDEF"].concat()),
            "ghp_**********************"
        );
        assert_eq!(masked("abc"), "***");
        // A long value keeps its head and says how long it was rather than drawing every star.
        let long = "x".repeat(200);
        assert_eq!(
            masked(&long),
            format!("xxxx{}… (200 characters)", "*".repeat(28))
        );
    }
}

#[cfg(test)]
mod joined_tests {
    use super::{Leak, joined};

    fn hit(kind: &'static str, span: std::ops::Range<usize>) -> Leak {
        Leak {
            rule: kind,
            kind,
            span,
        }
    }

    #[test]
    fn overlapping_hits_join_into_one_span_that_covers_both() {
        // The earlier hit is shorter than the later one it overlaps: `leaks` keeps only the
        // first, and its tail, 10..14, would go unmasked.
        let spans = joined(vec![
            hit("b", 6..14),
            hit("a", 2..10),
            hit("c", 20..25),
            hit("d", 25..30),
        ]);

        assert_eq!(spans, [(2..14, "a"), (20..25, "c"), (25..30, "d")]);
    }
}

#[cfg(test)]
mod documented_token_tests {
    use super::{found, leaks, secret_kind, token_kind};

    /// A made-up token body: `n` characters of the token alphabet, built here so no
    /// token-shaped literal sits in the source.
    fn fake_body(n: usize) -> String {
        "Ab1".chars().cycle().take(n).collect()
    }

    /// Every documented token format a bare token is recognised by, each as one made-up token.
    fn documented_tokens() -> Vec<String> {
        let classic = fake_body(36);
        let mut tokens: Vec<String> = ["ghp_", "gho_", "ghu_", "ghs_", "ghr_"]
            .iter()
            .map(|prefix| [prefix, classic.as_str()].concat())
            .collect();
        tokens.push(["github_pat_", &fake_body(22), "_", &fake_body(59)].concat());
        tokens.extend(
            [
                "glpat-", "gloas-", "gldt-", "glrt-", "glrtr-", "glcbt-", "glptt-", "glft-",
                "glimt-", "glagent-", "glwt-", "glsoat-", "glffct-",
            ]
            .iter()
            .map(|prefix| [prefix, fake_body(20).as_str()].concat()),
        );
        tokens.push(["xapp-1-", &fake_body(40)].concat());
        tokens.push(["xwfp-", &fake_body(40)].concat());
        tokens
    }

    #[test]
    fn a_bare_token_of_every_documented_format_is_a_secret_to_every_guard() {
        for token in documented_tokens() {
            let line = format!("the deploy uses {token} now");
            let head: String = token.chars().take(8).collect();
            assert_eq!(
                secret_kind(&line),
                Some("a token by its forge's prefix"),
                "{head}…"
            );
            assert!(token_kind(&line).is_some(), "{head}…");
            assert!(found(&line).is_some(), "{head}…");
            assert!(!leaks(&line).is_empty(), "{head}…");
        }
    }

    #[test]
    fn a_bare_token_in_a_settings_document_is_a_secret() {
        let token = ["glpat-", &fake_body(20)].concat();
        let text = format!("{{\n  \"remote\": \"{token}\"\n}}");
        assert_eq!(secret_kind(&text), Some("a token by its forge's prefix"));
    }

    #[test]
    fn a_name_that_only_starts_like_a_token_is_not_a_secret() {
        for text in [
            "git switch ghp-fix",
            "ghp_fix",
            "the glpat- prefix",
            "gldt-docs",
            "github_pat_ is a prefix",
            "xapp-short",
            "token",
            "a note about forge tokens",
            "fn read_ghp_token_from_env() {}",
        ] {
            assert_eq!(secret_kind(text), None, "{text}");
            assert_eq!(token_kind(text), None, "{text}");
        }
    }
}
