//! The key that orders charter versions, as PEP 440 orders them.
//!
//! `charter/update.py`'s `version_key`, and the whole of it: every comparison of two charter
//! versions in Python goes through that one function so they cannot disagree, and the same
//! rule applies here — [`news`](crate::news) is the only caller today and the pin, the
//! updater and any version row a later milestone grows are the ones after it.
//!
//! **Why it is not a string compare, and not a split on dots.** The function this replaces in
//! Python kept the digits of each dot-separated part and dropped the rest, so `0.60.0rc1`
//! became `(0, 60, 1)` and a release candidate compared NEWER than its release (charter
//! #1050). Nothing had published a pre-release, so no comparison had yet met one; the day one
//! did, the update command, the pin, the status line's arrow and the session-start check
//! would all have had it backwards.
//!
//! **Anything that is not a version sorts below every version and ties with every other one.**
//! That is [`VersionKey::NOT_A_VERSION`], and it is an `Option`'s `None`, which Rust already
//! orders below every `Some`. Below, so that a cache or a pin holding junk never reads as
//! newer; tied, so no caller finds a direction between two strings that have none.
//!
//! **A local label (`+dev`, `+local`) does not move a version**, which is not PEP 440's own
//! order. `channel.build_label` appends one to the SAME wheel's number to say where the build
//! came from, not that it is a later release. The label is matched — a version carrying one is
//! still a version — and then never read.
//!
//! The grammar is the regular expression in PEP 440's Appendix B, and this is a hand parser
//! for it rather than a regex, because charter-core carries no regex engine. The alternation
//! order and the greediness of every optional group are reproduced deliberately: `parse`
//! enumerates each group's candidates in the order a backtracking engine would try them and
//! takes the first that consumes the whole string, which is leftmost-first matching written
//! out. `tests/version_key_matches_charter.rs` checks the result against charter's own answer
//! for every spelling in PEP 440's own examples.

/// How a charter version orders against another.
///
/// `None` is [`VersionKey::NOT_A_VERSION`]. Rust orders `None` below every `Some` and equal to
/// itself, which is exactly Python's empty tuple.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct VersionKey(Option<Key>);

impl VersionKey {
    /// What every string that is not a version keys as.
    pub const NOT_A_VERSION: VersionKey = VersionKey(None);

    /// Is this a version at all?
    pub fn is_version(&self) -> bool {
        self.0.is_some()
    }
}

/// The ordered fields, in the order Python's tuple puts them.
///
/// Derived `Ord` compares field by field in declaration order, which is what a tuple compare
/// is — so the declaration order here IS the sort and cannot be changed without changing it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Key {
    epoch: u64,
    release: Vec<u64>,
    /// Python's `(-1,)`, `(phase, n)` or `(3,)`, as a pair.
    ///
    /// The one-element tuples never tie with a two-element one at the first position — `-1`
    /// and `3` are outside the phase range `0..=2` — so widening them to pairs cannot change
    /// an answer, and it makes the type one shape instead of three.
    pre: (i32, u64),
    /// `-1` for absent, so `X < X.post0`, as Python's `-1 if post is None else post`.
    post: i64,
    /// `(0, n)` for a development release and `(1, 0)` otherwise, so `X.dev0 < X`.
    dev: (u8, u64),
}

/// The key that orders charter versions.
///
/// Numbers compare as numbers (0.10.0 is newer than 0.2.0), and trailing zeros do not count
/// (`0.60` is `0.60.0`). Around a release, PEP 440's order:
/// `X.devN < XaN.devN < XaN < XbN < XrcN < X < X.postN.devN < X.postN`.
pub fn version_key(v: &str) -> VersionKey {
    VersionKey(parse(v))
}

/// The six characters Python's `str.strip(" \t\n\r\f\v")` takes off the ends here.
///
/// Named one by one rather than `trim()`, and `charter/update.py` says why: a bare strip would
/// also take a trailing U+2028 off, which would make a version out of a string no installer
/// accepts as one. The same paragraph is why the Python regex carries `re.ASCII` — without it
/// `re.IGNORECASE` folds the KELVIN SIGN to a `k` in a local label. Lowercasing here is
/// `to_ascii_lowercase`, which leaves every non-ASCII codepoint alone to fail the match.
fn pep440_strip(v: &str) -> &str {
    v.trim_matches(|c| matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{c}' | '\u{b}'))
}

fn parse(v: &str) -> Option<Key> {
    let lowered = pep440_strip(v).to_ascii_lowercase();
    let b = lowered.as_bytes();
    let mut i = 0;
    if b.first() == Some(&b'v') {
        i = 1;
    }
    // `(?:([0-9]+)!)?` — digits only count as an epoch when the `!` is actually there, so a
    // plain `1.0` does not lose its first component to this group.
    let mut epoch = 0;
    if let Some((n, j)) = digits(b, i)
        && b.get(j) == Some(&b'!')
    {
        epoch = n;
        i = j + 1;
    }
    // `([0-9]+(?:\.[0-9]+)*)` — required, and never backtracked: nothing else in the grammar
    // begins with a dot followed by digits, so the greedy read can never have to give one back.
    let (first, mut j) = digits(b, i)?;
    let mut release = vec![first];
    while b.get(j) == Some(&b'.') {
        let Some((n, k)) = digits(b, j + 1) else {
            break;
        };
        release.push(n);
        j = k;
    }
    i = j;

    // The four optional groups, each enumerated in the order a backtracking engine would try
    // its alternatives. The first combination that consumes the whole string is the match,
    // which is leftmost-first semantics spelled out as nested loops.
    for (after_pre, pre) in pre_options(b, i) {
        for (after_post, post) in post_options(b, after_pre) {
            for (after_dev, dev) in dev_options(b, after_post) {
                for end in local_options(b, after_dev) {
                    if end != b.len() {
                        continue;
                    }
                    // Cloned rather than moved: this is inside three loops that may still
                    // iterate, and the release is the one part of the answer built before them.
                    return Some(build(epoch, release.clone(), pre, post, dev));
                }
            }
        }
    }
    None
}

fn build(
    epoch: u64,
    mut release: Vec<u64>,
    pre: Option<(&'static str, u64)>,
    post: Option<u64>,
    dev: Option<u64>,
) -> Key {
    while release.last() == Some(&0) {
        release.pop();
    }
    let pre = match pre {
        Some((phase, n)) => (phase_rank(phase), n),
        // `X.devN` comes before `X`'s first alpha, not after its rc — but only when there is
        // no post-release, where `X.post1.dev2` is still an ordinary release's descendant.
        None if post.is_none() && dev.is_some() => (-1, 0),
        // A release, or its post-release, comes after every one of its phases.
        None => (3, 0),
    };
    Key {
        epoch,
        release,
        pre,
        post: post.map_or(-1, |n| n as i64),
        dev: dev.map_or((1, 0), |n| (0, n)),
    }
}

/// PEP 440's spellings of the three pre-release phases, in the order the phases come.
fn phase_rank(literal: &str) -> i32 {
    match literal {
        "a" | "alpha" => 0,
        "b" | "beta" => 1,
        _ => 2, // c, pre, preview, rc
    }
}

/// `(?:[-_.]?(alpha|a|beta|b|preview|pre|c|rc)[-_.]?([0-9]+)?)?`
///
/// The alternation order is PEP 440's own and is load-bearing: `preview` has to be tried
/// before `pre`, and `alpha` before `a`, or the longer spelling would match its own prefix and
/// leave the rest of itself behind.
fn pre_options(b: &[u8], i: usize) -> Vec<(usize, Option<(&'static str, u64)>)> {
    const PHASES: [&str; 8] = ["alpha", "a", "beta", "b", "preview", "pre", "c", "rc"];
    let mut out = Vec::new();
    for start in separator_options(b, i) {
        for literal in PHASES {
            let Some(after) = literal_at(b, start, literal) else {
                continue;
            };
            for after_sep in separator_options(b, after) {
                if let Some((n, end)) = digits(b, after_sep) {
                    out.push((end, Some((literal, n))));
                }
                out.push((after_sep, Some((literal, 0))));
            }
        }
    }
    out.push((i, None));
    out
}

/// `(?:-([0-9]+)|[-_.]?(post|rev|r)[-_.]?([0-9]+)?)?`
fn post_options(b: &[u8], i: usize) -> Vec<(usize, Option<u64>)> {
    const SPELLINGS: [&str; 3] = ["post", "rev", "r"];
    let mut out = Vec::new();
    // PEP 440's implicit post-release, `1.0-1`, and the first branch of the alternation.
    if b.get(i) == Some(&b'-')
        && let Some((n, end)) = digits(b, i + 1)
    {
        out.push((end, Some(n)));
    }
    for start in separator_options(b, i) {
        for literal in SPELLINGS {
            let Some(after) = literal_at(b, start, literal) else {
                continue;
            };
            for after_sep in separator_options(b, after) {
                if let Some((n, end)) = digits(b, after_sep) {
                    out.push((end, Some(n)));
                }
                out.push((after_sep, Some(0)));
            }
        }
    }
    out.push((i, None));
    out
}

/// `(?:[-_.]?dev[-_.]?([0-9]+)?)?`
fn dev_options(b: &[u8], i: usize) -> Vec<(usize, Option<u64>)> {
    let mut out = Vec::new();
    for start in separator_options(b, i) {
        let Some(after) = literal_at(b, start, "dev") else {
            continue;
        };
        for after_sep in separator_options(b, after) {
            if let Some((n, end)) = digits(b, after_sep) {
                out.push((end, Some(n)));
            }
            out.push((after_sep, Some(0)));
        }
    }
    out.push((i, None));
    out
}

/// `(?:\+[a-z0-9]+(?:[-_.][a-z0-9]+)*)?`, matched and then never read — see the module note on
/// why a local label does not move a version.
///
/// One candidate plus the skip, because the label can only be the last thing in the string: a
/// shorter read of a greedy `+` would leave characters no later group can match, so maximal
/// munch is the only end that can succeed.
fn local_options(b: &[u8], i: usize) -> Vec<usize> {
    let mut out = Vec::new();
    if b.get(i) == Some(&b'+') {
        let mut j = i + 1;
        let start = j;
        while j < b.len() && b[j].is_ascii_alphanumeric() {
            j += 1;
        }
        if j > start {
            loop {
                let sep = j;
                if b.get(sep).is_some_and(|c| matches!(c, b'-' | b'_' | b'.')) {
                    let mut k = sep + 1;
                    while k < b.len() && b[k].is_ascii_alphanumeric() {
                        k += 1;
                    }
                    if k > sep + 1 {
                        j = k;
                        continue;
                    }
                }
                break;
            }
            out.push(j);
        }
    }
    out.push(i);
    out
}

/// `[-_.]?`, greedy: consuming the separator is tried before skipping it.
fn separator_options(b: &[u8], i: usize) -> Vec<usize> {
    if b.get(i).is_some_and(|c| matches!(c, b'-' | b'_' | b'.')) {
        vec![i + 1, i]
    } else {
        vec![i]
    }
}

fn literal_at(b: &[u8], i: usize, literal: &str) -> Option<usize> {
    let end = i + literal.len();
    (b.len() >= end && &b[i..end] == literal.as_bytes()).then_some(end)
}

/// `[0-9]+` at `i`, as a number and the index after it.
///
/// **Saturating, and that is a departure worth naming.** Python's integers are unbounded, so a
/// release component of forty digits compares exactly there and clamps to `u64::MAX` here. The
/// only way to see it is to compare two versions whose components both exceed 18 quintillion
/// and differ above that; charter's own versions are three small numbers, and PyPI would not
/// take the other kind. Clamping rather than refusing, because refusing would make such a
/// string sort BELOW every version, which is a louder wrong answer than tying with another
/// absurd one.
fn digits(b: &[u8], i: usize) -> Option<(u64, usize)> {
    let mut j = i;
    while j < b.len() && b[j].is_ascii_digit() {
        j += 1;
    }
    if j == i {
        return None;
    }
    let mut n: u64 = 0;
    for &c in &b[i..j] {
        n = n.saturating_mul(10).saturating_add(u64::from(c - b'0'));
    }
    Some((n, j))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn newer(a: &str, b: &str) {
        assert!(
            version_key(a) > version_key(b),
            "{a} should sort above {b}: {:?} vs {:?}",
            version_key(a),
            version_key(b)
        );
    }

    fn same(a: &str, b: &str) {
        assert_eq!(version_key(a), version_key(b), "{a} should tie with {b}");
    }

    #[test]
    fn numbers_compare_as_numbers() {
        newer("0.10.0", "0.2.0");
        newer("0.62.1", "0.62.0");
        newer("1.0", "0.99.99");
    }

    #[test]
    fn trailing_zeros_do_not_count() {
        same("0.60", "0.60.0");
        same("0.60.0.0", "0.60");
        same("1", "1.0.0");
    }

    #[test]
    fn a_pre_release_comes_before_its_release() {
        // charter #1050, in one line: the defect this replaced had this backwards.
        newer("0.60.0", "0.60.0rc1");
        newer("0.60.0rc1", "0.60.0b2");
        newer("0.60.0b2", "0.60.0a9");
        newer("0.60.0a1", "0.60.0.dev5");
        newer("0.60.0.post1", "0.60.0");
        newer("0.60.0.post1", "0.60.0.post1.dev2");
    }

    #[test]
    fn every_spelling_of_one_phase_is_one_answer() {
        same("1.0a1", "1.0alpha1");
        same("1.0b1", "1.0beta1");
        same("1.0rc1", "1.0c1");
        same("1.0rc1", "1.0pre1");
        same("1.0rc1", "1.0preview1");
        same("1.0-a1", "1.0_a.1");
        same("1.0.post1", "1.0rev1");
        same("1.0.post1", "1.0r1");
        same("1.0-1", "1.0.post1");
    }

    #[test]
    fn an_absent_number_is_zero() {
        same("1.0a", "1.0a0");
        same("1.0.post", "1.0.post0");
        same("1.0.dev", "1.0.dev0");
    }

    #[test]
    fn a_local_label_does_not_move_a_version() {
        same("0.61.0+dev", "0.61.0");
        same("0.61.0+dev.9d18d55", "0.61.0");
        same("0.61.0+a-b_c", "0.61.0");
    }

    #[test]
    fn an_epoch_outranks_everything_below_it() {
        newer("1!0.1", "99.99.99");
        same("0!1.0", "1.0");
    }

    #[test]
    fn the_tag_name_and_the_case_are_both_read() {
        same("v0.62.0", "0.62.0");
        same("0.62.0RC1", "0.62.0rc1");
        same(" 0.62.0\n", "0.62.0");
    }

    #[test]
    fn anything_that_is_not_a_version_sorts_below_every_version_and_ties() {
        for junk in [
            "",
            "unreleased",
            "build7",
            "main",
            "0.62.0 extra",
            "0.62.0\u{2028}",
        ] {
            assert!(
                !version_key(junk).is_version(),
                "{junk} parsed as a version"
            );
            assert!(
                version_key("0.0.1") > version_key(junk),
                "{junk} outranked 0.0.1"
            );
            assert_eq!(version_key(junk), VersionKey::NOT_A_VERSION);
        }
        // `build7` is the one the function this replaced read as `(7,)` — newer than
        // everything charter has published.
        same("build7", "unreleased");
    }

    #[test]
    fn a_kelvin_sign_is_not_a_k() {
        // `re.ASCII` in charter, `to_ascii_lowercase` here: the fold must not reach outside
        // ASCII, or a local label spelled with U+212A would parse.
        assert!(!version_key("1.0+\u{212a}").is_version());
    }
}
