//! `git worktree list --porcelain`, read.
//!
//! A documented machine format that git holds stable, which is why charter depends on it —
//! spec decision 12 is about harness output, prose written for a person whose shape nobody
//! promised. Python charter has parsed this since the feature existed
//! (`charter/worktree.py:parse_porcelain`), and the two must agree.

use std::path::PathBuf;

/// One worktree, as git reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub path: PathBuf,
    /// The branch, with `refs/heads/` removed. `None` when detached or bare.
    pub branch: Option<String>,
    pub detached: bool,
    /// The reason git gives, or an empty string when it gives none. `None` when the tree is
    /// really there.
    ///
    /// **Callers MUST check this before treating `path` as a directory that exists.**
    pub prunable: Option<String>,
    pub bare: bool,
}

pub fn parse(text: &str) -> Vec<Row> {
    let mut out = Vec::new();
    let mut cur: Option<Row> = None;
    for line in text.lines() {
        if line.trim().is_empty() {
            out.extend(cur.take());
            continue;
        }
        let (key, value) = match line.split_once(' ') {
            Some((k, v)) => (k, v),
            None => (line, ""),
        };
        if key == "worktree" {
            out.extend(cur.take());
            cur = Some(Row {
                path: PathBuf::from(value),
                branch: None,
                detached: false,
                prunable: None,
                bare: false,
            });
            continue;
        }
        let Some(row) = cur.as_mut() else { continue };
        match key {
            // Only the prefix is stripped: a branch name legitimately holds slashes, so
            // splitting on the last one would drop everything before it.
            "branch" => row.branch = Some(value.trim_start_matches("refs/heads/").to_string()),
            "detached" => row.detached = true,
            "bare" => row.bare = true,
            "prunable" => row.prunable = Some(value.to_string()),
            _ => {}
        }
    }
    // git's last record is followed by a blank line, but a truncated read is not a reason to
    // lose it.
    out.extend(cur);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_branch_with_slashes_in_it_survives_being_read() {
        let rows = parse("worktree /a/b\nHEAD abc\nbranch refs/heads/feature/spi-schema\n\n");
        assert_eq!(rows[0].branch.as_deref(), Some("feature/spi-schema"));
    }

    #[test]
    fn a_worktree_whose_directory_is_gone_is_read_as_prunable_with_its_reason() {
        let rows = parse(
            "worktree /a/gone\nHEAD abc\nbranch refs/heads/x\n\
             prunable gitdir file points to non-existent location\n\n",
        );
        assert_eq!(
            rows[0].prunable.as_deref(),
            Some("gitdir file points to non-existent location")
        );
    }

    #[test]
    fn a_bare_repositorys_own_entry_is_not_read_as_a_tree() {
        // Its `path` is a git directory, not a checkout: treating it as a tree with no `.git`
        // is charter #942, review round 3.
        let rows = parse("worktree /a/bare.git\nbare\n\n");
        assert!(rows[0].bare);
        assert!(rows[0].branch.is_none());
    }

    #[test]
    fn a_detached_head_has_no_branch_and_says_so() {
        let rows = parse("worktree /a/b\nHEAD abc\ndetached\n\n");
        assert!(rows[0].detached);
        assert!(rows[0].branch.is_none());
    }

    #[test]
    fn a_final_record_with_no_trailing_blank_line_is_still_read() {
        let rows = parse("worktree /a/b\nHEAD abc\nbranch refs/heads/x\n");
        assert_eq!(rows.len(), 1);
    }

    #[test]
    fn a_prunable_line_with_no_reason_is_still_prunable() {
        let rows = parse("worktree /a/gone\nHEAD abc\nprunable\n\n");
        assert_eq!(rows[0].prunable.as_deref(), Some(""));
    }
}
