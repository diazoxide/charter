//! **Who last committed a line of the project's file** (#1464): what Settings' lists of grants
//! say of a grant the project carries ("committed by …"), for the dispatch grants and for the
//! Granted list alike.
//!
//! # One read a page
//!
//! A list asks for every line it draws at once ([`last_touching`]): git's history of the file is
//! read **once**, newest commit first, and each line is answered by the first commit whose diff
//! adds or removes a line it matches. A page of many grants is one `git log`, never one a grant.
//!
//! # Through the hardened runner
//!
//! git runs as [`crate::worktree::git::run`] runs it (#1415): a constructed environment, no
//! program a repository's config names (no external diff, no text conversion), and the
//! project's `.git` read by purlis where it is a file. Bounded by [`crate::worktree::git::READ`].
//! A history git cannot read answers nothing for every line: the list then says a grant is not
//! committed yet, which is what it says of one nobody committed.
//!
//! # A name is a commit's word
//!
//! Who committed is whatever a commit's author field says, and anyone who can push to the
//! project writes that. It is drawn as plain text, **bounded** ([`MOST_NAME_CHARS`]) and with
//! no control, zero-width or direction-changing character ([`bounded`]), so a name can neither
//! fill the row nor turn the rest of the line around.

use std::path::Path;

use crate::worktree::git;

/// The most characters of a committer's name a list draws; a longer one is cut, and says so
/// with `…`.
pub const MOST_NAME_CHARS: usize = 64;

/// Who committed a line, and when.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Committed {
    /// The commit's author, [`bounded`].
    pub by: String,
    /// When, in seconds since 1970.
    pub at: u64,
}

/// What separates one commit from the next in the history read: NUL, which git keeps out of
/// an author name and a text diff's lines both.
const COMMIT: char = '\0';

/// **For each of `wanted`, who last committed a line of the project's file it matches**, in
/// the project at `root`: the newest commit whose diff adds or removes such a line, or `None`
/// where no commit did (not committed yet), or the history could not be read. One `git log`
/// for all of them.
pub fn last_touching<F: Fn(&str) -> bool>(root: &Path, wanted: &[F]) -> Vec<Option<Committed>> {
    if wanted.is_empty() {
        return Vec::new();
    }
    let manifest = crate::names::manifest(root);
    let Some(file) = manifest.file_name().and_then(|name| name.to_str()) else {
        return vec![None; wanted.len()];
    };
    let format = "--format=%x00%an%x09%at".to_owned();
    let read = git::run(
        root,
        &[
            "log",
            &format,
            "--patch",
            "--unified=0",
            "--no-color",
            "--no-ext-diff",
            "--no-textconv",
            "--no-renames",
            "--",
            file,
        ],
        git::READ,
    );
    match read {
        Ok(run) if run.ok() => answered(&run.out, wanted),
        _ => vec![None; wanted.len()],
    }
}

/// [`last_touching`]'s reading of `log`, git's history of the file with each commit's patch.
fn answered<F: Fn(&str) -> bool>(log: &str, wanted: &[F]) -> Vec<Option<Committed>> {
    let mut found: Vec<Option<Committed>> = vec![None; wanted.len()];
    for commit in log.split(COMMIT).filter(|one| !one.is_empty()) {
        if found.iter().all(Option::is_some) {
            break;
        }
        let mut lines = commit.lines();
        // The time is the last field: a name may hold a tab, and is not split on it.
        let Some((by, at)) = lines.next().and_then(|head| head.rsplit_once('\t')) else {
            continue;
        };
        let Ok(at) = at.trim().parse::<u64>() else {
            continue;
        };
        let changed: Vec<&str> = lines
            .filter(|line| !line.starts_with("+++") && !line.starts_with("---"))
            .filter_map(|line| line.strip_prefix('+').or_else(|| line.strip_prefix('-')))
            .collect();
        for (slot, matches) in found.iter_mut().zip(wanted) {
            if slot.is_none() && changed.iter().any(|line| matches(line)) {
                *slot = Some(Committed {
                    by: bounded(by),
                    at,
                });
            }
        }
    }
    found
}

/// **`name` as a list draws it**: with no character that draws as nothing (a control, a
/// zero-width or direction-changing one: the house's one table, [`crate::shown`]), and at
/// most [`MOST_NAME_CHARS`] characters, a cut one ending in `…`.
pub fn bounded(name: &str) -> String {
    let kept: Vec<char> = name
        .chars()
        .filter(|ch| *ch == ' ' || !crate::shown::invisible(*ch))
        .collect::<String>()
        .trim()
        .chars()
        .collect();
    if kept.len() <= MOST_NAME_CHARS {
        return kept.into_iter().collect();
    }
    let mut cut: String = kept.into_iter().take(MOST_NAME_CHARS).collect();
    cut.push('…');
    cut
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOG: &str = "\0Ada\t300\n\ndiff --git a/purlis.toml b/purlis.toml\n--- \
                       a/purlis.toml\n+++ b/purlis.toml\n@@ -3 +3 @@\n-steward = [\"devops\"]\n+\
                       steward = [\"devops\", \"qa\"]\n\0Grace\t200\n\ndiff --git \
                       a/purlis.toml b/purlis.toml\n@@ -1,0 +2 @@\n+allow = [\"forge.invalid\"]\n\
                       \0Linus\t100\n\n@@ -0,0 +1 @@\n+steward = [\"devops\"]\n";

    fn names(found: &[Option<Committed>]) -> Vec<Option<(&str, u64)>> {
        found
            .iter()
            .map(|one| one.as_ref().map(|one| (one.by.as_str(), one.at)))
            .collect()
    }

    #[test]
    fn each_line_is_answered_by_the_newest_commit_that_added_or_removed_it_from_one_read() {
        let qa = |line: &str| line.contains("\"qa\"");
        let devops = |line: &str| line.contains("\"devops\"");
        let host = |line: &str| line.contains("\"forge.invalid\"");
        let never = |line: &str| line.contains("\"prod\"");
        let wanted: [&dyn Fn(&str) -> bool; 4] = [&qa, &devops, &host, &never];

        assert_eq!(
            names(&answered(LOG, &wanted)),
            [
                Some(("Ada", 300)),
                // A line removed is a line the commit touched, as git's -G reads it.
                Some(("Ada", 300)),
                Some(("Grace", 200)),
                None,
            ]
        );
    }

    #[test]
    fn a_file_header_is_not_a_line_of_the_file() {
        // `+++ b/purlis.toml` names the file; a matcher for the file's name finds nothing.
        let named = |line: &str| line.contains("purlis.toml");
        let wanted: [&dyn Fn(&str) -> bool; 1] = [&named];
        assert_eq!(names(&answered(LOG, &wanted)), [None]);
    }

    #[test]
    fn a_name_holding_a_tab_or_a_record_mark_is_credited_with_its_own_commit() {
        // #1464 review: a name with a tab, or U+001E, must not move the credit to an older
        // commit.
        let log = "\0Ada\tthe\u{1e}second\t300\n+steward = [\"devops\"]\n\0Linus\t100\n+steward \
                   = [\"devops\"]\n";
        let devops = |line: &str| line.contains("\"devops\"");
        let wanted: [&dyn Fn(&str) -> bool; 1] = [&devops];
        let found = answered(log, &wanted);
        assert_eq!(found[0].as_ref().map(|one| one.at), Some(300));
        assert!(
            found[0]
                .as_ref()
                .is_some_and(|one| one.by.starts_with("Ada"))
        );
    }

    #[test]
    fn a_commit_whose_head_does_not_read_answers_nothing_and_the_next_one_is_read() {
        let log = "\0no tab here\n+steward = [\"devops\"]\n\0Ada\tsoon\n+steward = \
                   [\"devops\"]\n\0Linus\t100\n+steward = [\"devops\"]\n";
        let devops = |line: &str| line.contains("\"devops\"");
        let wanted: [&dyn Fn(&str) -> bool; 1] = [&devops];
        assert_eq!(names(&answered(log, &wanted)), [Some(("Linus", 100))]);
    }

    #[test]
    fn a_name_is_drawn_bounded_and_with_nothing_that_turns_the_line_around() {
        assert_eq!(bounded("  Ada Lovelace "), "Ada Lovelace");
        assert_eq!(bounded("Ada\u{202e}ecalevoL\u{200b}\u{7}"), "AdaecalevoL");
        assert_eq!(bounded("Aarón"), "Aarón");
        let long = "x".repeat(MOST_NAME_CHARS + 10);
        let drawn = bounded(&long);
        assert_eq!(drawn.chars().count(), MOST_NAME_CHARS + 1);
        assert!(drawn.ends_with('…'));
        assert_eq!(
            bounded(&"y".repeat(MOST_NAME_CHARS)),
            "y".repeat(MOST_NAME_CHARS)
        );
    }

    #[test]
    fn nothing_wanted_asks_git_nothing() {
        // No repository here at all: with nothing wanted, git is not run.
        let dir = tempfile::tempdir().expect("a folder");
        let nothing: &[fn(&str) -> bool] = &[];
        assert!(last_touching(dir.path(), nothing).is_empty());
    }
}
