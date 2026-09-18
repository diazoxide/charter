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

/// The longest piece name charter will mint from a chat's name.
const MAX_SLUG: usize = 40;

/// A chat's name as a piece name, or `None` when nothing legal survives.
///
/// `None` is not a failure to swallow: charter asks for a name rather than inventing one,
/// because a piece name is a directory and a branch the operator has to live with.
pub fn slug(name: &str) -> Option<String> {
    let mut out = String::with_capacity(name.len().min(MAX_SLUG));
    let mut pending_dash = false;
    for c in name.chars() {
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
    let out = out.trim_end_matches(['.', '_', '-']).to_string();
    if out.is_empty() { None } else { Some(out) }
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
        assert_eq!(slug(&"a".repeat(80)).as_deref(), Some(&*"a".repeat(40)));
        for nothing in ["🔥🔥🔥", "...", "___", "", "   ", "---"] {
            assert_eq!(slug(nothing), None, "{nothing:?} leaves no legal name");
        }
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
}
