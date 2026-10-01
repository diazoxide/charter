//! What may name a piece and a branch, and what a chat's name becomes.
//!
//! The slug here is a CONVENIENCE. The containment is `piece_name_ok` and the confinement
//! check at the point of use — a test pins that the slug can never hand them a name they
//! would have to refuse, so the two cannot drift into a gap.

use crate::contain;

/// Can `name` name a piece — one directory under `.worktrees/<repo>/`?
///
/// Charter's alphabet, and it must START alphanumeric: a piece name reaches `git worktree
/// add` as a path, and a path beginning with `-` is an OPTION by the time git parses argv.
/// A leading `.` would also hide the directory from every listing that skips dotfiles.
pub fn piece_name_ok(name: &str) -> bool {
    contain::segment_ok(name)
        && name.starts_with(|c: char| c.is_ascii_alphanumeric())
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
}

/// Why a branch name is not one charter will hand to git.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum BadBranch {
    #[error("a branch name may not be empty")]
    Empty,
    #[error(
        "a branch name may not start with '-': git would read it as an option rather than as \
         a name"
    )]
    LooksLikeAFlag,
    #[error("a branch name may not hold a NUL")]
    Nul,
}

/// The rules charter enforces on a branch name — and only those.
///
/// **Deliberately three rules and not a ref grammar.** Git has `check-ref-format`, it is the
/// definition, and reimplementing it here would drift from the git the operator runs. What
/// git cannot protect against is the first rule: by the time git sees the name it is argv, so
/// `--upload-pack=…` is an option and not a bad ref.
pub fn branch_name_ok(name: &str) -> Result<(), BadBranch> {
    if name.is_empty() {
        return Err(BadBranch::Empty);
    }
    if name.starts_with('-') {
        return Err(BadBranch::LooksLikeAFlag);
    }
    if name.contains('\0') {
        return Err(BadBranch::Nul);
    }
    Ok(())
}

/// Every ref charter names to git, fully qualified.
///
/// `@` is a legal branch name, and `git merge --ff-only @` resolves HEAD rather than the
/// branch: measured on git 2.50.1 it prints "Already up to date", exits 0 and leaves HEAD
/// where it was — a merge charter would report as successful that landed nothing. `--` does
/// not help. Qualification does.
pub fn as_ref(branch: &str) -> String {
    format!("refs/heads/{branch}")
}

/// Whether `name` is one git keeps for itself or reads as something else, so charter never
/// mints it as a branch, though git's grammar would take some of them.
///
/// - `HEAD`, in any case: git refuses it as a branch, and on a case-insensitive filesystem
///   (macOS's default) `head` and `HEAD` are one file, so a branch or folder called `head`
///   is read as the repository's own HEAD.
/// - `*_HEAD`, in any case: `FETCH_HEAD`, `ORIG_HEAD`, `MERGE_HEAD`, `CHERRY_PICK_HEAD` and
///   the ones git adds next are files beside HEAD, for the same reason.
/// - Forty hex digits or more: where a branch is taken as a revision, git reads it as an
///   object name first.
pub fn reserved(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower == "head"
        || lower.ends_with("_head")
        || (name.len() >= 40 && name.chars().all(|c| c.is_ascii_hexdigit()))
}

/// The longest piece name charter will mint from a chat's name.
const MAX_SLUG: usize = 40;

/// A chat's name as a piece name, or `None` when nothing legal survives.
///
/// `None` is not a failure to swallow: a piece name is a directory and a branch the operator
/// has to live with, so nothing here invents one. A writing chat whose name slugs to nothing is
/// given a numbered `chat-<n>` by [`crate::chatpiece`] instead.
pub fn slug(name: &str) -> Option<String> {
    let mut out = String::with_capacity(name.len().min(MAX_SLUG));
    let mut pending_dash = false;
    for c in name.chars() {
        // `..` is never part of a ref name (`git check-ref-format`), so a run of dots is one:
        // `v1..v2` reads `v1.v2`.
        if c == '.' && !pending_dash && out.ends_with('.') {
            continue;
        }
        let keep = if c.is_ascii_alphanumeric() {
            Some(c.to_ascii_lowercase())
        } else if (c == '_' || c == '.') && !out.is_empty() {
            // Kept only once something alphanumeric has been written. `_wip` would otherwise
            // slug to `_wip`, which `piece_name_ok` refuses — a name the UI has already shown
            // the operator, rejected after the fact.
            Some(c)
        } else {
            None
        };
        match keep {
            // A separator is remembered rather than written, so a run of them collapses and a
            // trailing one is never emitted at all.
            None => pending_dash = !out.is_empty(),
            Some(c) => {
                if pending_dash && out.len() < MAX_SLUG {
                    out.push('-');
                    pending_dash = false;
                }
                if out.len() >= MAX_SLUG {
                    break;
                }
                out.push(c);
            }
        }
    }
    // A trailing `.` or `_` can survive the loop; a piece name ending in one is legal but
    // ugly, and `.lock` endings are refused by git anyway.
    let mut out = out.trim_end_matches(['.', '_', '-']).to_string();
    // A ref may not end `.lock` (git's lock files), however many times it says so.
    while let Some(kept) = out.strip_suffix(".lock") {
        out = kept.trim_end_matches(['.', '_', '-']).to_string();
    }
    // A name git keeps for itself, or reads as an object, is no name to mint. `None` sends a
    // writing chat to `chat-<n>` rather than to a refusal (GL-1 review B1).
    if out.is_empty() || reserved(&out) {
        return None;
    }
    // And a chat called "NUL" must not slug to `nul`, which `add` refuses because Windows
    // reads it as the null device (charter-app#96). Only a device stem can reach here — the
    // loop above emits `[a-z0-9._-]` and the trim takes the strippable endings off — and
    // every device name is four characters or fewer, so the suffix never crowds `MAX_SLUG`.
    if contain::mintable(&out).is_err() {
        // `-chat`, not `-piece`: the window shows this name as a branch (ADR 0072 §3).
        return Some(format!("{out}-chat"));
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_piece_name_is_one_contained_entry_that_is_never_an_argument() {
        assert!(piece_name_ok("fix-login"));
        assert!(piece_name_ok("m1.4"));
        assert!(piece_name_ok("a_b"));
        for name in [
            "",
            ".",
            "..",
            "a/b",
            "a\\b",
            "/abs",
            "C:x",
            ".hidden",
            "-force",
            "--force",
            "_wip",
            "a b",
            "é",
            "alpha\0evil",
        ] {
            assert!(!piece_name_ok(name), "{name:?} must not name a piece");
        }
    }

    #[test]
    fn a_branch_name_that_would_be_an_argument_is_refused_before_git_sees_it() {
        assert_eq!(branch_name_ok("-force"), Err(BadBranch::LooksLikeAFlag));
        assert_eq!(
            branch_name_ok("--upload-pack=evil"),
            Err(BadBranch::LooksLikeAFlag)
        );
        assert_eq!(branch_name_ok(""), Err(BadBranch::Empty));
        assert_eq!(branch_name_ok("a\0b"), Err(BadBranch::Nul));
        // Everything else is git's own ref grammar, checked by git, not here.
        assert_eq!(branch_name_ok("feature/spi-schema"), Ok(()));
        assert_eq!(branch_name_ok("@"), Ok(()));
    }

    #[test]
    fn every_ref_charter_names_is_qualified() {
        assert_eq!(as_ref("@"), "refs/heads/@");
        assert_eq!(as_ref("feature/x"), "refs/heads/feature/x");
    }

    #[test]
    fn a_chat_name_becomes_a_piece_name_or_charter_asks_for_another() {
        assert_eq!(slug("Fix login").as_deref(), Some("fix-login"));
        assert_eq!(slug("🔥 hotfix").as_deref(), Some("hotfix"));
        assert_eq!(slug("../../etc/passwd").as_deref(), Some("etc-passwd"));
        assert_eq!(slug("  --force  ").as_deref(), Some("force"));
        assert_eq!(slug("_wip").as_deref(), Some("wip"));
        assert_eq!(slug("__scratch__").as_deref(), Some("scratch"));
        assert_eq!(slug(".hidden").as_deref(), Some("hidden"));
        assert_eq!(slug(&"z".repeat(80)).as_deref(), Some(&*"z".repeat(40)));
        for nothing in ["🔥🔥🔥", "...", "___", "", "   ", "---"] {
            assert_eq!(slug(nothing), None, "{nothing:?} leaves no legal name");
        }
    }

    /// Whether git itself takes `name` as a new branch: `check-ref-format --branch`, the
    /// definition, not a copy of its rules.
    fn git_takes(name: &str) -> bool {
        let dir = tempfile::tempdir().unwrap();
        let ran = crate::testgit::run(dir.path(), &["check-ref-format", "--branch", name]);
        ran.ok() && ran.line() == name
    }

    /// Names git refuses, or that a case-insensitive filesystem reads as one of git's own files
    /// (GL-1 review B1). A chat's Name must never turn into one of these.
    const GITS_EDGES: &[&str] = &[
        "v1..v2 diff",
        "notes.lock",
        "notes.lock.lock",
        "a..lock",
        "HEAD",
        "Head",
        "head",
        "FETCH_HEAD",
        "orig_head",
        "Merge_Head",
        "cherry-pick_head",
        "my_head",
        "da39a3ee5e6b4b0d3255bfef95601890afd80709",
        "DA39A3EE5E6B4B0D3255BFEF95601890AFD80709 and more",
        "@",
        "@{-1}",
        "x.",
        "1..2",
    ];

    #[test]
    fn every_slug_it_produces_is_a_branch_git_itself_takes() {
        for raw in GITS_EDGES
            .iter()
            .chain(["Fix login", "release 1.2", "v1.lock-in"].iter())
        {
            if let Some(name) = slug(raw) {
                assert!(
                    git_takes(&name),
                    "{raw:?} slugged to {name:?}, which git refuses"
                );
                assert!(
                    !reserved(&name),
                    "{raw:?} slugged to {name:?}, which git reserves"
                );
            }
        }
    }

    #[test]
    fn a_name_git_reserves_or_reads_as_an_object_slugs_to_nothing() {
        for raw in [
            "HEAD",
            "Head",
            "fetch_head",
            "ORIG_HEAD",
            "Merge_Head",
            "x_head",
        ] {
            assert_eq!(slug(raw), None, "{raw:?}");
        }
        assert_eq!(slug("da39a3ee5e6b4b0d3255bfef95601890afd80709"), None);
        // All hex at forty or more reads to git as an object name wherever a branch is taken
        // as a revision, whatever it was meant as.
        assert_eq!(slug(&"a".repeat(40)), None);
        assert_eq!(slug(&"a".repeat(39)).as_deref(), Some(&*"a".repeat(39)));
    }

    #[test]
    fn a_dotted_name_keeps_its_words_and_loses_what_git_refuses() {
        assert_eq!(slug("v1..v2 diff").as_deref(), Some("v1.v2-diff"));
        assert_eq!(slug("notes.lock").as_deref(), Some("notes"));
        assert_eq!(slug("notes.lock.lock").as_deref(), Some("notes"));
        assert_eq!(slug("v1.lock-in").as_deref(), Some("v1.lock-in"));
    }

    #[test]
    fn every_slug_it_produces_is_a_name_the_gate_accepts() {
        // The slug is a convenience; the gate is the containment. This pins that the
        // convenience can never hand the gate something it would have to refuse — including
        // the leading `_` and `.` cases, whose absence made an earlier version of this test
        // pass while proving nothing.
        for raw in [
            "Fix login",
            "🔥 hotfix",
            "../../etc/passwd",
            "  --force  ",
            "_wip",
            "__scratch__",
            ".hidden",
            "...trailing",
            "-lead",
            "a/b/c",
            "1",
            "_1_",
            &"z".repeat(200),
            "ünïcödé",
            "a.b.c",
        ] {
            if let Some(s) = slug(raw) {
                assert!(
                    piece_name_ok(&s),
                    "slug({raw:?}) = {s:?} must be a legal piece name"
                );
            }
        }
    }

    #[test]
    fn a_chat_named_after_a_device_still_slugs_to_a_name_that_travels() {
        // charter-app#96's half of the rule above: `worktree::add` refuses a piece name that
        // means another directory on the next machine, so the convenience must never hand
        // the operator one — a chat called "NUL" would otherwise slug to `nul`, be shown to
        // them as the piece name, and be refused after the fact.
        for raw in ["NUL", "nul", "Con", "aux", "COM1", "lpt9", "prn", "alpha."] {
            let Some(s) = slug(raw) else { continue };
            assert!(
                contain::mintable(&s).is_ok(),
                "slug({raw:?}) = {s:?} is a name `add` would have to refuse"
            );
        }
    }
}
