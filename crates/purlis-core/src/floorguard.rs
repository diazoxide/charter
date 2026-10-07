//! Arm A4: the release floor — an unattended run may not publish (#299).
//!
//! A port of `charter/hooks.py`'s `_release_floor_reason`, `_unattended`, `UNATTENDED_MODE`,
//! `_PUBLISH_FORGE` and `_TAG_HARMLESS`. Everything under it is already standing:
//! [`crate::shellseg`] reads the line, [`crate::shellwrap::split_env`] names each segment's
//! program, [`crate::credguard::git_subcommand`] names git's verb and
//! [`crate::leakguard::is_charter`] recognises charter however it was spelled.
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
//! # Why this exists at all
//!
//! 0.46.0 taught `_ask` to fall back to `allow` under `bypassPermissions` so a workflow nudge
//! cannot hang an unattended run. That is right for a nudge, and it silently removed the only
//! thing standing between an autonomous agent and an irreversible release: the clone-commit
//! guard's `ask` used to floor the host's decision at a prompt in every permission mode. It
//! stopped things by accident. Now it allows them.
//!
//! **Deny, not ask**, deliberately: an unattended ask is now an allow, so an ask here would be
//! indistinguishable from no guard. Deny is also the only verdict that cannot hang.
//!
//! **Attended is untouched.** A guard that made releases harder for the operator is the cage the
//! plane-root guard warns about, and the fix people reach for then is to switch it off for good.
//!
//! # Two decisions the port made about the Python it came from
//!
//! * **The `_TAG_HARMLESS` arm clears its SEGMENT, not the command line** (#348). The Python
//!   `return None`d out of the whole walk, so `git tag -l` standing anywhere BEFORE a publish
//!   cleared the floor for it: `git tag -l && gh release create v1.0.0` was allowed unattended
//!   while `gh release create v1.0.0` alone was denied. That was a live fail-open (filed
//!   upstream as charter#1172), and every sibling guard already `continue`s past a segment that
//!   is not its business. This port `continue`s, which moved the recorded answer of the
//!   read-first row in `fixtures/corpora/shellseg-oracle.jsonl` on purpose (ADR 0046).
//! * **`is_charter` rather than `base == "charter"`**, so `purlis change land` (the name it
//!   ships as since RN-3), `edm change land` (the pre-rename binary) and `python3 -m charter
//!   change land` are the same command here as they are to the leak guard. Some spellings put
//!   charter's own NAME in `words` instead of in `prog`, which is why the leading name is
//!   dropped from `words` before the table is read. The table entry alone would have been a
//!   dead line: the lookup used to live under `elif base in ("gh", "glab")`.

mod merge;
mod token;

use crate::credguard::git_subcommand;
use crate::leakguard::{self, CHARTER_PROGS};
use crate::livesub;
use crate::shellseg;
use crate::shellwrap::{self, base_lower};

use std::sync::OnceLock;

use regex::Regex;

/// The `permission_mode` a host sets when there is nobody to answer a prompt —
/// `UNATTENDED_MODE`.
///
/// The field is the HOST's own; charter reads it and never holds a copy, because an autonomy flag
/// charter owned would be a second source of truth for a question the host already answers.
pub const UNATTENDED_MODE: &str = "bypassPermissions";

/// True when the host says there is nobody to answer a prompt — `_unattended`.
pub fn unattended(permission_mode: Option<&str>) -> bool {
    permission_mode == Some(UNATTENDED_MODE)
}

/// CLI `(program, noun, verb)` triples that publish or land code — `_PUBLISH_FORGE`.
///
/// `gh pr merge` and `gh release create` are no kind of `git`, so the git-write pattern never saw
/// them and nothing else did either — they were unguarded in every mode.
///
/// **`charter change land` is here.** That command merges one member of a cross-repo change,
/// which is the same act `gh pr merge` is: the floor already sits between *opening* a request and
/// *merging* one — `gh pr create` is deliberately absent — and a charter verb that merged would
/// be a documented way around a floor charter itself wrote. A cross-repo landing is N merges,
/// each individually revertible, so it is not treated as a release; the split is attended versus
/// unattended, exactly as it already is for `gh pr merge`.
pub const PUBLISH_FORGE: [(&str, &str, &str); 5] = [
    ("gh", "release", "create"),
    ("gh", "pr", "merge"),
    ("glab", "release", "create"),
    ("glab", "mr", "merge"),
    ("charter", "change", "land"),
];

/// A forge CLI's `(noun, verb)`.
type ForgeAct = (&'static str, &'static str);

/// Each [`PUBLISH_FORGE`] row of `gh` beside the same act's row for `glab`, as `(noun, verb)`
/// (#1068). Every forge row is in a pair; its test fails on one that is not.
#[cfg_attr(not(test), allow(dead_code))]
const PUBLISH_FORGE_PAIRS: [(ForgeAct, ForgeAct); 2] = [
    (("release", "create"), ("release", "create")),
    (("pr", "merge"), ("mr", "merge")),
];

/// `git tag` flags that only READ or act locally — `_TAG_HARMLESS`.
///
/// An autonomous run legitimately needs to know what the tags are, and deleting a local tag
/// publishes nothing.
pub const TAG_HARMLESS: [&str; 11] = [
    "-l",
    "--list",
    "-d",
    "--delete",
    "-n",
    "--contains",
    "--points-at",
    "--merged",
    "--no-merged",
    "-v",
    "--verify",
];

/// The remedy, identical for every trigger — the opening of every denial this guard returns.
pub const RELEASE_FLOOR_FIX: &str = "Publishing is on purlis's floor: a run with nobody watching may not cut a release. \
     `bypassPermissions` means *stop asking me*, not *stop knowing things*. Re-run this step \
     **attended**, or have a person do it. ";

/// `v?\d+\.\d+(?:\.\d+)?[\w.-]*`, full-match — what a version tag looks like.
///
/// Two CPython classes are spelled out rather than borrowed:
///
/// * **`\d`** is `\p{Nd}` on a `str` — every Unicode decimal digit — and the `regex` crate's is
///   the same `\p{Nd}`, so it is left as written and the fuzz alphabet carries Unicode digits to
///   measure it.
/// * **`\w`** is NOT the same. CPython's is `str.isalnum() or '_'`; the crate's is UTS#18's,
///   which counts a combining mark, a variation selector and ZWJ. #92 swept all 1,112,064
///   non-surrogate codepoints and found `[\p{L}\p{N}_]` is CPython's, with zero disagreements in
///   either direction, so that is what is written here.
///
/// `re.fullmatch` does NOT allow a trailing newline the way `$` does, so this is `\A…\z`.
fn version_tag_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"\Av?\d+\.\d+(?:\.\d+)?[\p{L}\p{N}_.\-]*\z").expect("the pattern compiles")
    })
}

/// Deny a publish when the host says nobody is watching — `_release_floor_reason`.
///
/// Keyed on TAGGING rather than on a `v*` shape. Shape-matching is narrower and reads more
/// correct — `release.yml` triggers on `v*` — but it is walked past by naming the tag
/// `release-1`, and tagging attended costs nothing. The asymmetry favours bluntness: a false stop
/// costs one re-run, a false pass costs a version number that can never be reused.
///
/// `unattended` is the host's answer, passed in rather than read: the Python takes the whole hook
/// payload and asks [`unattended`] of its `permission_mode`, and the core holds no globals.
pub fn release_floor_reason(cmd: &str, unattended: bool) -> Option<String> {
    floor(cmd, unattended, 0)
}

/// How many commands deep the floor reads a command inside a command — a shell's script, an
/// alias's expansion, the words fed to a shell's stdin (#866).
///
/// Past it the line is refused as one the floor cannot read. No real command nests this deep,
/// and without a bound a line nested thousands deep would exhaust the stack: a hook that dies
/// without answering is an allow. The bound also keeps the cost linear in the line, since each
/// level reads a shorter string than the one around it.
pub const MAX_DEPTH: usize = crate::guardcaps::MAX_COMMAND_DEPTH;

/// [`release_floor_reason`] at `depth` commands in.
pub(crate) fn floor(cmd: &str, unattended: bool, depth: usize) -> Option<String> {
    if !unattended {
        return None;
    }
    let fix = RELEASE_FLOOR_FIX;
    if depth > MAX_DEPTH {
        return Some(format!("{fix}{}", merge::UNREADABLE));
    }
    let inner_floor = |inner: &str| floor(inner, unattended, depth + 1);
    let segments = shellseg::segment_argv(cmd);
    // A shell that reads its script from stdin is fed by the rest of the line (#866).
    let a_shell_reads_stdin = segments
        .iter()
        .any(|toks| shellwrap::reads_script_from_stdin(toks));
    // A substitution the shell runs, wherever it stands, even inside double quotes, read from
    // the line as written: the words a substitution's own quotes split are not its body (#866).
    if livesub::may_substitute(cmd) && livesub::live_substitution(cmd).is_some() {
        for body in substitution_bodies(cmd) {
            if let Some(said) = inner_floor(&body) {
                return Some(said);
            }
        }
    }
    for toks in &segments {
        // Each segment is split once, and every reading below shares it: at every level the
        // floor reads, a segment is as long as the line, so a second split is a second line
        // (#1355).
        let (prog, env, argv) = shellwrap::split_env(toks);
        // A string a shell runs is read as the command it is (#866).
        let scripts = shellwrap::shell_scripts_of(&prog, &argv)
            .into_iter()
            .chain(shellwrap::here_string_script(toks));
        let fed = if a_shell_reads_stdin && !shellwrap::reads_script_from_stdin(toks) {
            fed_lines(toks)
        } else {
            Vec::new()
        };
        for inner in scripts.chain(fed) {
            if let Some(said) = inner_floor(&inner) {
                return Some(said);
            }
        }
        if let Some(why) = token::keychain_read_reason(&prog, &argv) {
            return Some(format!("{fix}{why}"));
        }
        // `GIT tag v1` is a tag — see A2's fold.
        let base = base_lower(&prog);
        let args: &[String] = argv.get(1..).unwrap_or(&[]);
        let words: Vec<&String> = args.iter().filter(|a| !a.starts_with('-')).collect();
        if base == "git" || base.starts_with("git-credential-") {
            let sub = git_subcommand(args);
            // A git alias defined on the line is read as what it stands for (#866).
            match merge::git_alias(args, &env, sub.as_deref()) {
                merge::GitAlias::Resolved(inner) => {
                    if let Some(said) = inner_floor(&inner) {
                        return Some(said);
                    }
                }
                merge::GitAlias::Unreadable => return Some(format!("{fix}{}", merge::UNREADABLE)),
                merge::GitAlias::None => {}
            }
            if sub.as_deref() == Some("tag") {
                // `git tag` alone lists; a bare name is a CREATION — the choke point, since a tag
                // that does not exist locally cannot be pushed.
                //
                // A harmless flag clears THIS segment and the walk carries on (#348): a read
                // standing before a publish must not clear the floor for the publish.
                if args.iter().any(|a| TAG_HARMLESS.contains(&a.as_str())) {
                    continue;
                }
                if words.len() > 1 {
                    return Some(format!(
                        "{fix}Creating a tag is the first step of a publish."
                    ));
                }
            }
            if sub.as_deref() == Some("push") {
                if args.iter().any(|a| a == "--tags" || a == "--follow-tags") {
                    return Some(format!("{fix}`--tags`/`--follow-tags` pushes tags."));
                }
                // Defence in depth: a tag that already existed locally.
                if args.iter().any(|a| a.starts_with("refs/tags/")) {
                    return Some(format!("{fix}This pushes a tag ref."));
                }
                if words.iter().skip(1).any(|w| version_tag_re().is_match(w)) {
                    return Some(format!("{fix}This pushes what looks like a version tag."));
                }
            }
            // A push option that sets auto-merge, an alias that would publish, or a command that
            // prints the forge token (#866).
            let why = token::git_reason(&base, args, sub.as_deref())
                .or_else(|| merge::git_reason(args, &env, sub.as_deref(), unattended, depth));
            if let Some(why) = why {
                return Some(format!("{fix}{why}"));
            }
        } else if base == "gh" || base == "glab" || leakguard::is_charter(&prog, args) {
            // **The reader had to widen with the set.** `PUBLISH_FORGE`'s charter row would be a
            // tuple nothing could reach if this branch were still `base in ("gh", "glab")`.
            let forge = base == "gh" || base == "glab";
            // Every name of charter's own is read as the one its table row is keyed on, so
            // `purlis change land` and `edm change land` are `charter change land` (RN-3).
            let name = if forge {
                base.clone()
            } else {
                crate::cliname::ALIAS.to_string()
            };
            // `edm change land` and `python3 -m charter change land` put charter's own NAME in
            // `words` instead of in `prog` — and `-m` drops out with the other flags — so the
            // pair sits one place further along and is put back on the same footing here. A
            // forge CLI's words are read past its repository flag (#866).
            let words: Vec<&str> = if forge {
                merge::forge_words(args).into_iter().map(|w| w.1).collect()
            } else {
                match words.first() {
                    Some(w) if CHARTER_PROGS.contains(&w.to_lowercase().as_str()) => &words[1..],
                    _ => &words[..],
                }
                .iter()
                .map(|w| w.as_str())
                .collect()
            };
            if let [noun, verb, ..] = words[..] {
                // A CLI's own alias of a verb is that verb (#866).
                let held = merge::canonical_verb(&name, noun, verb);
                if PUBLISH_FORGE.contains(&(name.as_str(), noun, held)) {
                    return Some(format!(
                        "{fix}`{name} {noun} {verb}` publishes or lands code."
                    ));
                }
            }
            if forge {
                let why = token::forge_reason(args)
                    .or_else(|| merge::forge_reason(&name, args, cmd, unattended, depth));
                if let Some(why) = why {
                    return Some(format!("{fix}{why}"));
                }
            }
        }
    }
    None
}

/// What a segment could print into a shell reading stdin: its words as one line, with and
/// without the producer's own leading options (`echo -n`, `printf --`) and, for `printf`, its
/// format; and each word that is a line of its own.
pub(crate) fn fed_lines(toks: &[String]) -> Vec<String> {
    let (prog, _env, argv) = shellwrap::split_env(toks);
    let words = argv.get(1..).unwrap_or(&[]);
    let past_options = words
        .iter()
        .position(|w| !w.starts_with('-'))
        .map_or(&[][..], |at| &words[at..]);
    let mut lines = vec![words.join(" "), past_options.join(" ")];
    if base_lower(&prog) == "printf" {
        lines.push(past_options.get(1..).unwrap_or(&[]).join(" "));
    }
    lines.extend(words.iter().filter(|w| w.contains(' ')).cloned());
    lines.dedup();
    lines
}

/// The bodies of the outermost command and process substitutions in a line: `$(…)`, `` `…` ``,
/// `<(…)` and `>(…)`. Quoting is not read, so a substitution in single quotes is read too, which
/// only ever refuses more; the caller asks only of a line where the shell runs one. Unbalanced, a
/// body runs to the end of the line. A nested one is found when its body is read in turn.
fn substitution_bodies(line: &str) -> Vec<String> {
    let chars: Vec<char> = line.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let opens = matches!(chars[i], '$' | '<' | '>')
            && chars.get(i + 1) == Some(&'(')
            && !(chars[i] == '$' && chars.get(i + 2) == Some(&'('));
        if opens {
            let mut depth = 1;
            let mut j = i + 2;
            while j < chars.len() && depth > 0 {
                match chars[j] {
                    '(' => depth += 1,
                    ')' => depth -= 1,
                    _ => {}
                }
                j += 1;
            }
            let end = if depth == 0 { j - 1 } else { j };
            out.push(chars[i + 2..end].iter().collect());
            i = j;
            continue;
        }
        if chars[i] == '`' {
            let close = chars[i + 1..].iter().position(|c| *c == '`');
            let end = close.map_or(chars.len(), |at| i + 1 + at);
            out.push(chars[i + 1..end].iter().collect());
            i = end + 1;
            continue;
        }
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #1068: a publishing act floored through one forge CLI is floored through the other.
    #[test]
    fn each_publish_forge_row_has_its_counterpart() {
        let row =
            |cli: &str, (noun, verb): (&str, &str)| PUBLISH_FORGE.contains(&(cli, noun, verb));
        for (gh, glab) in PUBLISH_FORGE_PAIRS {
            assert!(row("gh", gh) && row("glab", glab), "{gh:?} / {glab:?}");
        }
        for (cli, noun, verb) in PUBLISH_FORGE.iter().filter(|(c, ..)| *c != "charter") {
            assert!(
                PUBLISH_FORGE_PAIRS.iter().any(|(gh, glab)| {
                    (*cli == "gh" && *gh == (*noun, *verb))
                        || (*cli == "glab" && *glab == (*noun, *verb))
                }),
                "{cli} {noun} {verb} has no counterpart in PUBLISH_FORGE_PAIRS"
            );
        }
    }

    fn deny(cmd: &str) -> Option<String> {
        release_floor_reason(cmd, true)
    }

    /// Attended is untouched, whatever the command is.
    #[test]
    fn attended_is_untouched() {
        for cmd in [
            "git tag v1.2.3",
            "gh release create v1",
            "git push --tags",
            "charter change land",
        ] {
            assert_eq!(release_floor_reason(cmd, false), None, "{cmd}");
        }
    }

    /// The host's own field, and nothing charter holds a copy of.
    #[test]
    fn the_mode_is_the_hosts_word() {
        assert!(unattended(Some(UNATTENDED_MODE)));
        assert!(!unattended(Some("default")));
        assert!(!unattended(None));
    }

    /// Creating a tag is the choke point; listing and deleting are not.
    #[test]
    fn creating_a_tag_is_refused_and_reading_one_is_not() {
        assert!(deny("git tag v1.2.3").is_some());
        assert!(deny("git tag release-1").is_some());
        assert_eq!(deny("git tag"), None);
        assert_eq!(deny("git tag -l"), None);
        assert_eq!(deny("git tag --list 'v*'"), None);
        assert_eq!(deny("git tag -d v1"), None);
    }

    /// **A harmless tag flag clears only its own segment** (#348). Listing the tags to work out
    /// the next version and then cutting the release on the same Bash call is the ordinary way
    /// that script is written, and the read must not clear the floor for the publish after it.
    #[test]
    fn a_harmless_tag_flag_clears_only_its_own_segment() {
        for cmd in [
            "git tag -l && gh release create v1.0.0",
            "gh release create v1.0.0 && git tag -l",
            "git tag -l && git tag v1.0.0",
            "git tag -l >/dev/null && git push --tags",
            "git tag -l && charter change land",
            "git tag -d v0 ; gh pr merge 12",
            "git tag --list 'v*' | tail -1 && git tag v2",
        ] {
            assert!(deny(cmd).is_some(), "{cmd}");
        }
        // The read alone is still a read.
        assert_eq!(deny("git tag -l"), None);
        assert_eq!(deny("git tag -l && git tag -d v1"), None);
        assert_eq!(deny("git tag -l && git log --oneline"), None);
    }

    /// A push carrying tags, in each of its three spellings.
    #[test]
    fn a_push_that_carries_a_tag_is_refused() {
        assert!(deny("git push --tags").is_some());
        assert!(deny("git push --follow-tags origin main").is_some());
        assert!(deny("git push origin refs/tags/v1").is_some());
        assert!(deny("git push origin v1.2.3").is_some());
        assert!(deny("git push origin 1.2").is_some());
        assert_eq!(deny("git push origin main"), None);
        // The version scan skips `words[0]`, which is the subcommand itself.
        assert_eq!(deny("git push"), None);
    }

    /// CPython's `\w` counts a combining mark and the crate's does not. The class is written out
    /// as `[\p{L}\p{N}_]`, which #92 measured to be CPython's over every codepoint.
    #[test]
    fn the_version_shape_uses_cpythons_word_class() {
        // A variation selector is a `\w` to the `regex` crate and not to CPython, so a tag ending
        // in one must NOT match — CPython's `fullmatch` fails on it.
        assert!(!version_tag_re().is_match("v1.2.3\u{fe0f}"));
        assert!(version_tag_re().is_match("v1.2.3-rc1"));
        // A Unicode decimal digit is `\d` in both engines.
        assert!(version_tag_re().is_match("v\u{0663}.\u{0664}"));
        // `fullmatch` allows no trailing newline, unlike `$`.
        assert!(!version_tag_re().is_match("v1.2\n"));
    }

    /// The forge table, and the branch that reaches it.
    #[test]
    fn a_forge_publish_is_refused_and_opening_a_request_is_not() {
        assert!(deny("gh release create v1").is_some());
        assert!(deny("gh pr merge 12").is_some());
        assert!(deny("glab mr merge 12").is_some());
        assert!(deny("glab release create v1").is_some());
        assert_eq!(deny("gh pr create --title x"), None);
        assert_eq!(deny("gh issue create --title x"), None);
    }

    /// charter's own landing verb, in every spelling `is_charter` knows.
    #[test]
    fn charters_own_landing_verb_is_on_the_floor() {
        assert!(deny("charter change land").is_some());
        assert!(deny("edm change land").is_some());
        assert!(deny("python3 -m charter change land").is_some());
        assert!(deny("/usr/local/bin/charter change land").is_some());
        assert_eq!(deny("charter change show"), None);
        assert_eq!(deny("charter change list"), None);
    }

    /// The denial names the command it refused, which is what makes the floor arguable rather
    /// than mysterious.
    #[test]
    fn the_denial_names_the_command() {
        let said = deny("gh pr merge 12").expect("a denial");
        assert!(said.starts_with(RELEASE_FLOOR_FIX));
        assert!(
            said.contains("`gh pr merge` publishes or lands code."),
            "{said}"
        );
    }
}
