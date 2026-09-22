//! Arm A4: the release floor — an unattended run may not publish (#299).
//!
//! A port of `charter/hooks.py`'s `_release_floor_reason`, `_unattended`, `UNATTENDED_MODE`,
//! `_PUBLISH_FORGE` and `_TAG_HARMLESS`. Everything under it is already standing:
//! [`crate::shellseg`] reads the line, [`crate::shellwrap::split_env`] names each segment's
//! program, [`crate::credguard::git_subcommand`] names git's verb and
//! [`crate::leakguard::is_charter`] recognises charter however it was spelled.
//!
//! # Nothing calls this yet, on purpose
//!
//! `charter hook pretooluse` is ONE switch. `main.rs`'s `is_a_tool_hook` answers every word in
//! the `pretooluse`/`posttooluse` namespace with exit 2 — *block* — and
//! `crates/charter-cli/tests/hook.rs` pins that. **This is stage 4 of six and is wired to
//! nothing**; the switch moves in the last PR and nowhere earlier.
//!
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
//! # Two shapes of the same trap, and why the port keeps both
//!
//! * **The `_TAG_HARMLESS` arm returns for the WHOLE command**, not for its segment. `git tag -l`
//!   standing anywhere BEFORE a publish clears the floor for the publish — measured against the
//!   oracle, `git tag -l && gh release create v1.0.0` is allowed unattended while
//!   `gh release create v1.0.0` alone is denied, and so is the tag creation the arm exists to
//!   stop. That is a live fail-open and it is filed as **charter#1172**; the fix there is
//!   `continue` rather than `return None`. It is reproduced here exactly, because the frozen
//!   Python is the differential's oracle and a port that is right where the oracle is wrong
//!   fails its own test. The corpus carries both orders — a read first and a publish first —
//!   so the upstream fix shows up as a divergence rather than silently.
//! * **`is_charter` rather than `base == "charter"`**, so `edm change land` (the pre-rename
//!   binary) and `python3 -m charter change land` are the same command here as they are to the
//!   leak guard. Both put charter's own NAME in `words` instead of in `prog`, which is why the
//!   leading name is dropped from `words` before the table is read. The table entry alone would
//!   have been a dead line: the lookup used to live under `elif base in ("gh", "glab")`.

use crate::credguard::git_subcommand;
use crate::leakguard::{self, CHARTER_PROGS};
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
pub const RELEASE_FLOOR_FIX: &str = "Publishing is on charter's floor: a run with nobody watching may not cut a release. \
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
    if !unattended {
        return None;
    }
    let fix = RELEASE_FLOOR_FIX;
    for toks in shellseg::segment_argv(cmd) {
        let (prog, _env, argv) = shellwrap::split_env(&toks);
        // `GIT tag v1` is a tag — see A2's fold.
        let base = base_lower(&prog);
        let args: Vec<String> = argv.iter().skip(1).cloned().collect();
        let words: Vec<String> = args
            .iter()
            .filter(|a| !a.starts_with('-'))
            .cloned()
            .collect();
        if base == "git" {
            let sub = git_subcommand(&args);
            if sub.as_deref() == Some("tag") {
                // `git tag` alone lists; a bare name is a CREATION — the choke point, since a tag
                // that does not exist locally cannot be pushed.
                //
                // **This returns for the whole COMMAND LINE, not for this segment.** That is the
                // Python's own control flow, reproduced rather than corrected.
                if args.iter().any(|a| TAG_HARMLESS.contains(&a.as_str())) {
                    return None;
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
        } else if base == "gh" || base == "glab" || leakguard::is_charter(&prog, &args) {
            // **The reader had to widen with the set.** `PUBLISH_FORGE`'s charter row would be a
            // tuple nothing could reach if this branch were still `base in ("gh", "glab")`.
            let name = if base == "gh" || base == "glab" {
                base.clone()
            } else {
                CHARTER_PROGS[0].to_string()
            };
            // `edm change land` and `python3 -m charter change land` put charter's own NAME in
            // `words` instead of in `prog` — and `-m` drops out with the other flags — so the
            // pair sits one place further along and is put back on the same footing here.
            let words: &[String] = match words.first() {
                Some(w) if CHARTER_PROGS.contains(&w.to_lowercase().as_str()) => &words[1..],
                _ => &words[..],
            };
            if words.len() >= 2
                && PUBLISH_FORGE.contains(&(name.as_str(), words[0].as_str(), words[1].as_str()))
            {
                return Some(format!(
                    "{fix}`{name} {} {}` publishes or lands code.",
                    words[0], words[1]
                ));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

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

    /// **A harmless tag flag clears the WHOLE command line, not its segment.** The Python
    /// `return None`s out of the loop, and a port that narrowed it to `continue` would deny a
    /// command the oracle allows — the differential's job is to catch exactly that, so the
    /// behaviour is pinned here too.
    #[test]
    fn a_harmless_tag_flag_clears_the_whole_command() {
        assert!(deny("gh release create v1").is_some());
        assert_eq!(deny("git tag -l && gh release create v1"), None);
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
