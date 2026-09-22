//! CPython's `posixpath` and `fnmatch`, as the leak guard asks them.
//!
//! [`crate::leakguard`] is a port of `charter/hooks.py`'s `_leak_reason` and its closure, and
//! four of the answers in that closure are not the guard's own rules at all — they are
//! CPython's:
//!
//! * `os.path.normpath`, which is the whole of `_names_a_vault_path`'s second arm (`.charter//
//!   vaults/`, `.charter/./vaults/` and `a/b/../vaults` are the same path and the pattern only
//!   sees the text);
//! * `posixpath.join`, which is how a `cd` in an earlier segment and a wrapper's own `-C` reach
//!   a later operand;
//! * `Path.resolve()`, which is `os.path.realpath(strict=False)` and is what makes the
//!   guarded-state walk a question about ANCESTRY rather than about spelling;
//! * `fnmatch.fnmatch`, which decides whether `--exclude-dir=.charter*` really keeps a walker
//!   out — and therefore decides an ALLOW.
//!
//! Writing a second, plausible version of any of them is how this repository's guards have
//! acquired holes before, so each is ported to the letter and each is compared per case in
//! `tests/differential/shellseg.py`.
//!
//! # The direction each one fails in
//!
//! `fnmatch` is the one to read twice. In `_walk_into_guarded_state` a MATCH means *"this
//! directory is excluded, the walk does not reach the vault"* — an allow. So a matcher that
//! says yes where CPython says no is a **fail-open**, and that is why the class grammar below
//! is transcribed from `fnmatch._translate` rather than approximated with "`*`, `?` and a
//! bracket set". In `_glob_selects_inside` the same predicate decides a DENY, so the two call
//! sites pull in opposite directions and only fidelity satisfies both.
//!
//! # Why the pattern is not handed to the `regex` crate
//!
//! `fnmatch.translate` emits a CPython regular expression, and two of the things it emits are
//! not the same expression to the `regex` crate: `[]]` is a one-member class to CPython and a
//! parse error in Rust, and `\Z`/`\z` are spelled differently. Compiling the translated text
//! would therefore be a port of the translation PLUS a port of the difference between two
//! regex engines. What is ported here instead is the translation's *meaning*: the same
//! bracket-run scan, the same `-` chunking, the same empty and negated-empty answers, matched
//! directly.
//!
//! # The version this is a port of
//!
//! CPython 3.13/3.14's `fnmatch._translate`. `fnmatch` has been rewritten twice in recent
//! releases — 3.12 added the set-operation escaping, 3.13 the atomic-group join — so the
//! oracle's answer for an exotic pattern is a property of the interpreter running it. The
//! differential is what holds the two together, and a CI runner on an older interpreter would
//! report it as a divergence in `fnm` rather than as a silent difference.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// `os.path.isabs` on posix.
pub fn is_abs(path: &str) -> bool {
    path.starts_with('/')
}

/// `posixpath.join(a, b)` — the two-argument form, which is the only one the guard uses.
///
/// Not `Path::join`: Rust's would give `a/b` for an empty `a` where Python gives `b`, and would
/// not keep the single separator Python's `endswith(sep)` arm keeps.
pub fn join(a: &str, b: &str) -> String {
    if b.starts_with('/') {
        return b.to_string();
    }
    if a.is_empty() || a.ends_with('/') {
        return format!("{a}{b}");
    }
    format!("{a}/{b}")
}

/// `os.path.normpath` on posix — `posixpath.normpath`, collapsing `//`, `/./` and `a/b/..`.
///
/// CPython runs this in C (`posix._path_normpath`) and the pure-Python fallback beside it is
/// what is ported; the two are the same function. The one piece that is not obvious is that
/// POSIX gives **exactly two** leading slashes an implementation-defined meaning and three or
/// more the meaning of one, so `//a` keeps its pair and `///a` does not.
///
/// A leading `..` on a RELATIVE path is kept (`../x` is a real path); on an absolute path it is
/// dropped, because `/..` is `/`.
pub fn normpath(path: &str) -> String {
    if path.is_empty() {
        return ".".to_string();
    }
    let initial_slashes = if path.starts_with("//") && !path.starts_with("///") {
        2
    } else if path.starts_with('/') {
        1
    } else {
        0
    };
    let mut comps: Vec<&str> = Vec::new();
    for comp in path.split('/') {
        if comp.is_empty() || comp == "." {
            continue;
        }
        if comp != ".."
            || (initial_slashes == 0 && comps.is_empty())
            || comps.last().is_some_and(|c| *c == "..")
        {
            comps.push(comp);
        } else if !comps.is_empty() {
            comps.pop();
        }
    }
    let mut out = String::new();
    for _ in 0..initial_slashes {
        out.push('/');
    }
    out.push_str(&comps.join("/"));
    if out.is_empty() { ".".to_string() } else { out }
}

/// `os.getcwd()` as `realpath` uses it: already absolute and already symlink-free.
///
/// A cwd that cannot be read is the one case Python cannot reach (it raises), so the fallback
/// is `/` — which keeps every later answer a path rather than a panic in a security guard.
fn getcwd() -> String {
    std::env::current_dir()
        .ok()
        .and_then(|p| p.to_str().map(str::to_string))
        .unwrap_or_else(|| "/".to_string())
}

/// `Path.resolve()` — `os.path.realpath(strict=False)`, ported from CPython 3.14's `posixpath`.
///
/// **Non-strict, and that is the whole point.** A component that does not exist is not an
/// error: the walk keeps going and appends the name. `std::fs::canonicalize` refuses such a
/// path, so using it here would turn every operand naming a file that is not there into an
/// `OSError` — which `_walk_into_guarded_state` answers with `continue`, i.e. with an ALLOW.
///
/// Three details are load-bearing and each is CPython's, not an invention:
///
/// * `..` is resolved **lexically against the already-resolved prefix**
///   (`path[:path.rindex(sep)] or sep`), not by asking the filesystem for a parent. Since the
///   prefix is symlink-free by construction, that is the same answer the kernel would give —
///   and it is what makes `.charter/vaults/../..` land where the shell would put it.
/// * a symlink LOOP is not an error either: the path resolved so far is used and the walk goes
///   on, so a planted loop cannot make the guard throw.
/// * an absolute symlink target **resets** the resolved path to `/`.
///
/// `seen` caches a fully resolved symlink so a repeated traversal is not repeated work, and it
/// is also how the loop is detected: an entry present but unresolved means the walk is inside
/// its own target.
pub fn realpath(filename: &str) -> String {
    // The stack of unresolved parts, in reverse. `None` is CPython's marker meaning "the entry
    // below me is a symlink whose target is now fully resolved".
    let mut rest: Vec<Option<String>> = filename
        .split('/')
        .rev()
        .map(|s| Some(s.to_string()))
        .collect();
    let mut part_count = rest.len();
    let mut path = if filename.starts_with('/') {
        "/".to_string()
    } else {
        getcwd()
    };
    let mut seen: HashMap<String, Option<String>> = HashMap::new();

    while part_count > 0 {
        // `part_count` counts a SUBSET of `rest`: every real path part, but neither the `None`
        // marker nor the symlink path directly beneath it. So a positive count always has an
        // entry to pop — CPython relies on the same fact, and its `name = rest.pop()` raises
        // `IndexError` if it is ever wrong.
        //
        // Stated here because the `break` below is otherwise a SECOND, silent stop condition,
        // and a silent one made three mutations to this loop's counter impossible for any test
        // to catch. The nightly reported all three as MISSED on 2026-09-21 — `while part_count
        // >= 0` (always true on a `usize`), `part_count += 1`, and `part_count /= 1` — and none
        // of them is a no-op: each one means the loop no longer stops when the parts run out.
        // They survived only because the `break` absorbed the overrun and returned the same
        // answer anyway. A sweep of 540,000 generated symlink farms confirmed it: zero
        // divergences for all three, against 54,004 for a control mutation known to be real.
        //
        // `debug_assert`, and deliberately NOT `expect`. In tests it fires on the first call
        // any of those three mutants makes, which is what kills them. In a shipped guard it is
        // gone, and the `break` stands, because a PANIC here is not the safe direction: a
        // crashed `pretooluse` exits 1 and a harness reads that as ALLOW — the same reasoning
        // that makes a symlink loop return a path instead of raising (see the loop test).
        debug_assert!(
            !rest.is_empty(),
            "part_count counts entries of `rest`, so a positive count has one to pop"
        );
        let Some(popped) = rest.pop() else { break };
        let Some(name) = popped else {
            // A resolved symlink target: the entry below is the symlink path it belongs to.
            if let Some(Some(link)) = rest.pop() {
                seen.insert(link, Some(path.clone()));
            }
            continue;
        };
        part_count -= 1;
        if name.is_empty() || name == "." {
            continue;
        }
        if name == ".." {
            path = match path.rfind('/') {
                Some(0) | None => "/".to_string(),
                Some(i) => path[..i].to_string(),
            };
            continue;
        }
        let newpath = if path == "/" {
            format!("/{name}")
        } else {
            format!("{path}/{name}")
        };
        // `symlink_metadata` is `os.lstat`: it does NOT follow the last component.
        let target = match std::fs::symlink_metadata(&newpath) {
            Err(_) => None, // the ignored `OSError`: the component is simply not there
            Ok(md) => {
                if !md.file_type().is_symlink() {
                    path = newpath;
                    continue;
                }
                match seen.get(&newpath) {
                    Some(Some(cached)) => {
                        path = cached.clone();
                        continue;
                    }
                    Some(None) => {
                        // Seen but unresolved: this is a loop. Non-strict keeps the path.
                        //
                        // **The differential cannot check this arm**, and that is a fact about
                        // the oracle rather than about the rule: on CPython 3.11 and 3.12
                        // `Path.resolve()` raises a `RuntimeError` for a loop and
                        // `_walk_into_guarded_state` catches only `OSError`, so the Python has
                        // no answer to record (charter#1166). The fixture plane therefore plants
                        // no loop, and `a_symlink_loop_is_an_answer_and_not_a_hang` below is
                        // what holds this line — mutated to `path.clone()` and watched go red.
                        path = newpath;
                        continue;
                    }
                    None => std::fs::read_link(&newpath)
                        .ok()
                        .and_then(|p| p.to_str().map(str::to_string)),
                }
            }
        };
        match target {
            None => path = newpath, // an error occurred and was ignored
            Some(target) => {
                if target.starts_with('/') {
                    path = "/".to_string();
                }
                seen.insert(newpath.clone(), None);
                rest.push(Some(newpath));
                rest.push(None);
                let parts: Vec<&str> = target.split('/').collect();
                part_count += parts.len();
                for p in parts.into_iter().rev() {
                    rest.push(Some(p.to_string()));
                }
            }
        }
    }
    path
}

/// [`realpath`] over a [`Path`], for the callers that hold one. A path that is not UTF-8 comes
/// back unchanged: charter's own state directory is one it created, and inventing a lossy
/// spelling of somebody else's bytes is how a comparison starts answering about a different
/// file.
pub fn realpath_of(path: &Path) -> PathBuf {
    match path.to_str() {
        Some(s) => PathBuf::from(realpath(s)),
        None => path.to_path_buf(),
    }
}

// --------------------------------------------------------------------------- //
// `fnmatch`                                                                     //
// --------------------------------------------------------------------------- //

/// One piece of a translated shell pattern.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Item {
    /// `*` — any run of characters, newline included (CPython compiles with `(?s:…)`).
    Star,
    /// `?` — exactly one character.
    Any,
    /// A literal character.
    Lit(char),
    /// `[…]`, as members and ranges, with CPython's negation.
    Class {
        negated: bool,
        singles: Vec<char>,
        ranges: Vec<(char, char)>,
    },
    /// CPython's `(?!)`: an empty range, which matches nothing at all — so the whole pattern
    /// matches nothing. `[b-a]` collapses to this.
    Never,
}

/// The bracket-run scan of `fnmatch._translate`, to the letter.
///
/// Returned as items rather than as a regular expression for the reason this module's header
/// gives. The three answers a careless version gets wrong, each of which is a pattern somebody
/// really types at `--exclude-dir`:
///
/// * an unclosed `[` is a LITERAL `[`, not a class and not an error;
/// * `[]a]` is the class `{']', 'a'}` — the run scan steps over a `]` that is the first
///   character, so the class is not empty and the pattern is not two literals;
/// * `[b-a]` matches NOTHING, where a matcher that merely skipped the impossible range would
///   match `b`, `-` and `a`.
fn translate(pat: &str) -> Vec<Item> {
    let p: Vec<char> = pat.chars().collect();
    let n = p.len();
    let mut out: Vec<Item> = Vec::new();
    let mut i = 0usize;
    while i < n {
        let c = p[i];
        i += 1;
        match c {
            '*' => {
                out.push(Item::Star);
                // CPython compresses a run of `*` into one. **Inert here** — measured, zero
                // answers over the fuzz and over the recording — because two adjacent `Star`s
                // match exactly what one does in [`matches_from`]: the second only re-records
                // the backtrack point at the same position. It is kept because it is CPython's
                // and because it bounds the item list.
                while i < n && p[i] == '*' {
                    i += 1;
                }
            }
            '?' => out.push(Item::Any),
            '[' => {
                let mut j = i;
                if j < n && p[j] == '!' {
                    j += 1;
                }
                if j < n && p[j] == ']' {
                    j += 1;
                }
                while j < n && p[j] != ']' {
                    j += 1;
                }
                if j >= n {
                    out.push(Item::Lit('['));
                } else {
                    out.push(class_item(&p, &mut i, j));
                    i = j + 1;
                }
            }
            _ => out.push(Item::Lit(c)),
        }
    }
    out
}

/// The body of `fnmatch._translate`'s `[` arm: `stuff = pat[i:j]`, the `-` chunking, the empty
/// and negated-empty answers, and the class itself.
///
/// `i` is taken by reference because CPython's own loop MOVES it while collecting chunks, and
/// the chunk boundaries are what decide which `-` is a range and which is a literal. The caller
/// resets it to `j + 1` afterwards, exactly as CPython does.
///
/// The escaped text CPython would put between the brackets is built FIRST and read second,
/// rather than the ranges being reconstructed from the chunks directly. That is not a detour:
/// the join puts a `-` between every pair of chunks, so a chunk the merge emptied turns its
/// range back into a literal `-` (`[b-a-c]` is `[-c]`, one literal hyphen and one `c`), and
/// reading the joined text is the only way that stays true without a second table of cases.
///
/// # Two rules here are INERT, and they are written down rather than removed
///
/// Both were mutated and measured against the oracle over 20,000 generated cases and over the
/// whole recorded corpus: **zero answers changed either way**. They are kept because they are
/// CPython's, and a port that quietly drops a rule it cannot see the effect of is a port whose
/// next reader cannot tell a simplification from an omission.
///
/// * **The impossible-range merge.** Removing it leaves `[b-a]` as a range whose low end is
///   above its high end, and [`class_holds`] skips such a range — so the class matches nothing
///   either way, which is exactly what CPython's `(?!)` means. The merge also drops the two
///   endpoint characters, and those characters were consumed BY the range, so nothing else
///   loses a member.
/// * **The `-` chunking itself.** CPython chunks because it is building regex TEXT and has to
///   decide which `-` to escape; [`parse_class`] decides the same thing directly — a `-` with a
///   member on each side is a range and a `-` at either end is a member — so running the
///   chunker or not produces the same class. Both the empty (`(?!)`) and the negated-empty
///   (`.`) answers survive the chunker's absence for the same reason.
fn class_item(p: &[char], i: &mut usize, j: usize) -> Item {
    let start = *i;
    let raw = &p[start..j];
    let stuff: String = if !raw.contains(&'-') {
        raw.iter().collect::<String>().replace('\\', "\\\\")
    } else {
        let mut chunks: Vec<Vec<char>> = Vec::new();
        let mut k = if p[start] == '!' {
            start + 2
        } else {
            start + 1
        };
        while let Some(at) = (k.min(j)..j).find(|&x| p[x] == '-') {
            chunks.push(p[*i..at].to_vec());
            *i = at + 1;
            k = at + 3;
        }
        let chunk = p[*i..j].to_vec();
        if chunk.is_empty() {
            // CPython's `chunks[-1] += '-'`: a trailing `-` is a literal member. `chunks` cannot
            // be empty here — an empty tail means the loop appended at least once.
            if let Some(last) = chunks.last_mut() {
                last.push('-');
            }
        } else {
            chunks.push(chunk);
        }
        // "Remove empty ranges -- invalid in RE": where a range's low end is above its high end,
        // CPython MERGES the two chunks and drops both endpoints, which is what makes `[b-a]`
        // collapse to nothing rather than to three literals.
        let mut k = chunks.len();
        while k > 1 {
            k -= 1;
            let lo = chunks[k - 1].last().copied();
            let hi = chunks[k].first().copied();
            if let (Some(lo), Some(hi)) = (lo, hi)
                && lo > hi
            {
                let mut merged = chunks[k - 1].clone();
                merged.pop();
                merged.extend_from_slice(&chunks[k][1..]);
                chunks[k - 1] = merged;
                chunks.remove(k);
            }
        }
        chunks
            .iter()
            .map(|c| {
                c.iter()
                    .collect::<String>()
                    .replace('\\', "\\\\")
                    .replace('-', "\\-")
            })
            .collect::<Vec<_>>()
            .join("-")
    };

    if stuff.is_empty() {
        return Item::Never; // CPython's `(?!)`
    }
    if stuff == "!" {
        return Item::Any; // a negated EMPTY range matches any character
    }
    // CPython escapes `&`, `~` and `|` so a set operation cannot be spelled by accident; `\&` is
    // the literal `&` either way, which is what the reader below gives it.
    let mut text: Vec<char> = stuff.chars().collect();
    let mut negated = false;
    match text[0] {
        '!' => {
            negated = true;
            text.remove(0);
        }
        // `[^…]` and `[[…]`: CPython puts a backslash in front so the character is a MEMBER and
        // not a negation or a nested set. **Inert here** — measured, zero answers — because the
        // reader below has no `^` negation (fnmatch's is `!`) and no nested sets, so both
        // characters are already members. Kept so the text this builds is the text CPython
        // builds, which is what makes the two readable side by side.
        '^' | '[' => text.insert(0, '\\'),
        _ => {}
    }
    let (singles, ranges) = parse_class(&text);
    Item::Class {
        negated,
        singles,
        ranges,
    }
}

/// CPython's `re` reading of the text between `[` and `]`, over the alphabet
/// `fnmatch._translate` can emit: `\X` is the literal X, a `-` with a member on each side is a
/// range, and a `-` at either end is a member of its own.
fn parse_class(text: &[char]) -> (Vec<char>, Vec<(char, char)>) {
    let mut singles: Vec<char> = Vec::new();
    let mut ranges: Vec<(char, char)> = Vec::new();
    let n = text.len();
    // One member, escaped or not, and where it ends.
    let member = |at: usize| -> Option<(char, usize)> {
        if at >= n {
            None
        } else if text[at] == '\\' && at + 1 < n {
            Some((text[at + 1], at + 2))
        } else {
            Some((text[at], at + 1))
        }
    };
    let mut i = 0usize;
    while let Some((lo, next)) = member(i) {
        // A `-` is a range only with a member on BOTH sides: `[a-]` and `[-a]` are literals.
        if next < n
            && text[next] == '-'
            && let Some((hi, after)) = member(next + 1)
        {
            ranges.push((lo, hi));
            i = after;
            continue;
        }
        singles.push(lo);
        i = next;
    }
    (singles, ranges)
}

/// Whether one character is in a translated class.
///
/// A range whose low end is above its high end matches nothing. CPython never compiles one —
/// the chunk merge above removes it, and `re.compile` would raise if one got through — so this
/// arm answers for a case the merge cannot reach rather than for one it can.
fn class_holds(negated: bool, singles: &[char], ranges: &[(char, char)], c: char) -> bool {
    let inside = singles.contains(&c)
        || ranges
            .iter()
            .any(|&(lo, hi)| lo <= hi && lo <= c && c <= hi);
    inside != negated
}

/// `fnmatch.fnmatch(name, pat)` on posix, where `os.path.normcase` is the identity.
///
/// The match is CPython's over CODE POINTS, so the classes and the positions are the ones `str`
/// indexing gives — the whole of this port compares character positions, never bytes.
///
/// A `*` matches anything including `/`: `fnmatch` is not `glob`, and both callers rely on that
/// (`--exclude-dir='.char*'` keeps a walker out of `.charter`).
pub fn fnmatch(name: &str, pat: &str) -> bool {
    let items = translate(pat);
    // `(?!)` anywhere in a concatenation makes the whole expression unmatchable.
    if items.contains(&Item::Never) {
        return false;
    }
    let chars: Vec<char> = name.chars().collect();
    matches_from(&items, &chars)
}

/// The classic glob match: greedy `*` with backtracking.
///
/// CPython spells the same thing as `(?>.*?fixed)…` atomic groups for speed. The two agree
/// because every interior fixed run in a translated pattern is both preceded and followed by
/// `.*`, which is the condition under which the earliest match of a fixed run is never worse
/// than a later one.
fn matches_from(items: &[Item], name: &[char]) -> bool {
    let (mut ii, mut ni) = (0usize, 0usize);
    let (mut star, mut resume) = (None::<usize>, 0usize);
    while ni < name.len() {
        match items.get(ii) {
            Some(Item::Star) => {
                star = Some(ii);
                resume = ni;
                ii += 1;
            }
            Some(it) if one_matches(it, name[ni]) => {
                ii += 1;
                ni += 1;
            }
            _ => match star {
                Some(s) => {
                    ii = s + 1;
                    resume += 1;
                    ni = resume;
                }
                None => return false,
            },
        }
    }
    items[ii..].iter().all(|it| *it == Item::Star)
}

fn one_matches(item: &Item, c: char) -> bool {
    match item {
        Item::Any => true,
        Item::Lit(l) => *l == c,
        Item::Class {
            negated,
            singles,
            ranges,
        } => class_holds(*negated, singles, ranges, c),
        Item::Star | Item::Never => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    /// The corners of `fnmatch._translate` a matcher that only knows `*`, `?` and a bracket set
    /// gets wrong, each with the answer CPython really gives.
    ///
    /// Read from the interpreter, not argued: the same table was swept against CPython 3.11.14,
    /// 3.12.13 and 3.14.4 over 113,320 name/pattern pairs — every 1-, 2- and 3-character pattern
    /// over `*?[]!-^\ab` plus 100,000 generated ones — with zero divergences on all three. That
    /// matters because `fnmatch` was rewritten in 3.12 and again in 3.13, so the oracle's answer
    /// here is a property of whichever interpreter `uv` resolves.
    #[test]
    fn the_class_grammar_is_cpythons() {
        for (name, pat, want) in [
            // An unclosed `[` is a LITERAL `[`, not a class and not an error.
            ("[", "[", true),
            ("a", "[", false),
            // `[]` is two literals: the run scan steps over a `]` that is the first character,
            // then finds no closing one at all.
            ("[]", "[]", true),
            // ...and `[]a]` really is the class {']', 'a'}.
            ("]", "[]a]", true),
            ("a", "[]a]", true),
            ("b", "[]a]", false),
            // An impossible range collapses to "matches NOTHING" — not to three literals.
            ("b", "[b-a]", false),
            ("-", "[b-a]", false),
            ("a", "[b-a]", false),
            // ...and the collapse turns the neighbouring `-` back into a member.
            ("-", "[b-a-c]", true),
            ("c", "[b-a-c]", true),
            // A negated EMPTY range matches any character.
            ("q", "[!b-a]", true),
            // A `-` at either end of a class is a member, not a range.
            ("-", "[a-]", true),
            ("-", "[-a]", true),
            // A leading `^` is a MEMBER here, because `!` is fnmatch's negation.
            ("^", "[^a]", true),
            ("b", "[^a]", false),
            ("a", "[!a]", false),
            ("b", "[!a]", true),
            // The set-operation characters are literals.
            ("&", "[&&]", true),
            ("|", "[|~]", true),
            // `*` matches a `/` — `fnmatch` is not `glob`, and both callers rely on it.
            ("a/b", "a*b", true),
            (".charter", ".char*", true),
            // A run of `*` is one `*`, and one `*` may match nothing.
            ("ab", "a***b", true),
            ("ab", "a**b", true),
            ("axb", "a**b", true),
            ("", "**", true),
            // `?` is exactly one character, and the astral plane is one character.
            ("\u{1d7ce}", "?", true),
            ("İ", "?", true),
            ("", "?", false),
        ] {
            assert_eq!(fnmatch(name, pat), want, "fnmatch({name:?}, {pat:?})");
        }
    }

    /// `posixpath.normpath`'s three answers a lexical collapse gets wrong.
    #[test]
    fn normpath_is_posixpaths() {
        for (path, want) in [
            ("", "."),
            (".", "."),
            ("//", "//"), // POSIX gives exactly two leading slashes their own meaning
            ("///", "/"), // ...and three or more the meaning of one
            ("//a", "//a"),
            ("///a", "/a"),
            ("/..", "/"),
            ("..", ".."), // a leading `..` on a RELATIVE path is a real path
            ("../..", "../.."),
            ("a/..", "."),
            ("a/../..", ".."),
            (".charter//vaults/", ".charter/vaults"),
            (".charter/./vaults", ".charter/vaults"),
            (".charter/vaults/../..", "."),
        ] {
            assert_eq!(normpath(path), want, "normpath({path:?})");
        }
    }

    /// `posixpath.join`'s two rules, which `Path::join` does not have.
    #[test]
    fn join_is_posixpaths() {
        for (a, b, want) in [
            ("", "x", "x"),
            ("a", "", "a/"),
            ("a/", "b", "a/b"),
            ("a", "b", "a/b"),
            ("a", "/b", "/b"),
            ("", "", ""),
            ("/", "x", "/x"),
        ] {
            assert_eq!(join(a, b), want, "join({a:?}, {b:?})");
        }
    }

    /// A symlink LOOP is answered rather than raised — and the differential cannot check that,
    /// so it is checked here.
    ///
    /// `_walk_into_guarded_state` resolves with `Path.resolve()`, and on CPython 3.11 and 3.12
    /// `pathlib`'s `check_eloop` turns the kernel's `ELOOP` into a **`RuntimeError`**, which its
    /// `except OSError` does not catch: the oracle raises out of `pretooluse`, and a crashed
    /// `pretooluse` exits 1, which a harness reads as *allow* (charter#1166). CPython 3.14
    /// returns the path, as `os.path.realpath(strict=False)` does on all three, and that is what
    /// this port does — the fail-CLOSED direction, since an answer can be refused and a crash
    /// cannot. A differential cannot arbitrate an input one side has no answer for, so the
    /// fixture plane plants no loop and this test does.
    #[test]
    fn a_symlink_loop_is_an_answer_and_not_a_hang() {
        let tmp = tempfile::tempdir().expect("a temporary directory");
        let root = tmp.path().canonicalize().expect("the root resolves");
        // A self-referential link, and a two-step cycle, which are the two shapes a filesystem
        // can hold.
        std::os::unix::fs::symlink("lp", root.join("lp")).expect("symlink");
        std::os::unix::fs::symlink("b", root.join("a")).expect("symlink");
        std::os::unix::fs::symlink("a", root.join("b")).expect("symlink");
        for name in ["lp", "a", "b"] {
            let asked = root.join(name).to_string_lossy().into_owned();
            let got = realpath(&asked);
            assert_eq!(got, asked, "a loop resolves to the path that closed it");
        }
        // ...and a loop in the MIDDLE of a path still answers, with the rest appended.
        let deep = root.join("lp").join("x").to_string_lossy().into_owned();
        assert_eq!(realpath(&deep), deep);
    }

    /// The walk stops when the PARTS run out, which is not when the stack does.
    ///
    /// `realpath` keeps two kinds of thing on one stack: the path parts still to resolve, and,
    /// for every symlink it expands, a two-entry marker recording where that link landed.
    /// `part_count` counts only the first kind. So a path whose LAST component is a symlink
    /// finishes with a marker pair still on the stack and the loop leaving it there — and
    /// since nothing after the loop reads `seen`, walking off the end instead changed no
    /// answer at all. That is why the nightly reported three mutations to this counter as
    /// MISSED on 2026-09-21 (`> 0` to `>= 0`, `-= 1` to `+= 1`, `-= 1` to `/= 1`): a second,
    /// silent `break` on an empty stack was absorbing every one of them. A sweep of 540,000
    /// generated symlink farms found zero divergences for all three, against 54,004 for a
    /// control mutation known to be real.
    ///
    /// They are catchable now because the loop states that invariant in a `debug_assert`
    /// instead of letting a `break` paper over it. Each of the three was put back into the
    /// source and this test was watched go red. The assertion is compiled out of a shipped
    /// guard on purpose — a panic in `pretooluse` is read as ALLOW, so the `break` is still
    /// what runs there.
    #[test]
    fn the_walk_stops_when_the_parts_run_out_and_not_when_the_stack_does() {
        let tmp = tempfile::tempdir().expect("a temporary directory");
        let root = tmp.path().canonicalize().expect("the root resolves");
        std::fs::create_dir(root.join("real")).expect("a directory to land in");
        std::fs::write(root.join("real").join("f"), b"x").expect("a file in it");
        let real = root.join("real").to_string_lossy().into_owned();

        // A link as the LAST component: the parts run out with its marker pair unread.
        std::os::unix::fs::symlink("real", root.join("one")).expect("symlink");
        let one = root.join("one").to_string_lossy().into_owned();
        assert_eq!(
            realpath(&one),
            real,
            "a trailing link resolves to its target"
        );

        // ...and a chain of them, so more than one marker pair is left behind.
        std::os::unix::fs::symlink("one", root.join("two")).expect("symlink");
        std::os::unix::fs::symlink("two", root.join("three")).expect("symlink");
        let three = root.join("three").to_string_lossy().into_owned();
        assert_eq!(realpath(&three), real, "a chain of trailing links resolves");

        // A link in the MIDDLE, where the counter still has parts to stop for afterwards.
        let through = root.join("three").join("f").to_string_lossy().into_owned();
        let target = root.join("real").join("f").to_string_lossy().into_owned();
        assert_eq!(
            realpath(&through),
            target,
            "the rest of the path follows the link"
        );
    }

    proptest! {
        /// A matcher that panics is a matcher that does not answer, and the thing above it reads
        /// "did not answer" as an error rather than as a refusal. The class grammar indexes a
        /// pattern in four places; this is the property that says none of them can walk off it.
        #[test]
        fn no_pattern_makes_the_matcher_panic(
            name in ".{0,40}",
            pat in "[*?\\[\\]!^\\\\a-c0-9.\\-/&|~]{0,14}",
        ) {
            let _ = fnmatch(&name, &pat);
        }

        /// `normpath` is CPython's fixed point: swept over 50,780 paths against the interpreter,
        /// normalising a normalised path changes nothing. The guard asks it of every operand, so
        /// a second pass has to be free.
        #[test]
        fn normpath_is_its_own_fixed_point(p in "[a-z./]{0,40}") {
            prop_assert_eq!(normpath(&normpath(&p)), normpath(&p));
        }
    }
}
