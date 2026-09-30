//! The scan of an agent's commit before it is made (SQ-16): every line the commit ADDS, in any
//! repository a chat commits to, asked [`crate::secretshape::leaks`] — the same scanner a plane
//! save and a repo save use, with the rule a diff of code needs: vendor credential shapes and
//! personal data, and not the credential-assignment rule that refuses `password: String`.
//!
//! **What is read is the index being committed**, from inside git's own `pre-commit` hook
//! ([`crate::githooks`]), so `git commit -a` and `git commit <paths>` — which stage into a
//! temporary index git names in `GIT_INDEX_FILE` — are scanned as what they commit.
//!
//! **Only added lines.** A value already in the history, or one this commit removes, is not
//! something this commit publishes. A file git calls binary has no lines and is not read.
//!
//! **What is shown is masked** ([`crate::secretshape::masked`]): the kind, where it is, and a
//! short head of the value. Never the value.
//!
//! # The seam SQ-17 fills
//!
//! There is no allowlist and no way to mark a finding as a false positive here. SQ-17 adds a
//! per-repo allowlist as a filter over what [`staged`] answers; nothing else changes.

use std::path::Path;

use crate::secretshape;
use crate::worktree::git;

/// One thing a staged commit would publish.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// The file, as git names it from the top of the work tree.
    pub path: String,
    /// The 1-based line in the new version of the file.
    pub line: usize,
    /// What it looks like.
    pub kind: &'static str,
    /// The value, masked.
    pub masked: String,
}

/// Everything the staged diff in `repo` adds that looks like a secret or personal data, in the
/// order git lists it. `Err` is git failing to answer, which the caller refuses on: a commit
/// that could not be read is not a commit that was checked.
pub fn staged(repo: &Path) -> Result<Vec<Finding>, String> {
    let run = git::run_in_hook(
        repo,
        &[
            "-c",
            "core.quotePath=false",
            "diff",
            "--cached",
            "--no-color",
            "--no-ext-diff",
            "--no-textconv",
            "--src-prefix=a/",
            "--dst-prefix=b/",
            "-U0",
        ],
        git::READ,
    )
    .map_err(|gone| gone.to_string())?;
    if run.code != Some(0) {
        return Err(match run.code {
            None => "git took too long to show the staged diff".to_owned(),
            Some(_) => String::from_utf8_lossy(&run.err).trim().to_owned(),
        });
    }
    Ok(in_diff(&String::from_utf8_lossy(&run.out)))
}

/// What [`staged`] finds in a unified diff's text.
fn in_diff(diff: &str) -> Vec<Finding> {
    let mut found = Vec::new();
    let mut path: Option<String> = None;
    let mut at = 0usize;
    let mut in_hunk = false;
    for line in diff.lines() {
        if line.starts_with("diff --git ") {
            path = None;
            in_hunk = false;
        } else if !in_hunk && let Some(new) = line.strip_prefix("+++ ") {
            path = new.strip_prefix("b/").map(str::to_owned);
        } else if let Some(hunk) = line.strip_prefix("@@ ") {
            in_hunk = true;
            at = hunk
                .split_whitespace()
                .find_map(|part| part.strip_prefix('+'))
                .and_then(|new| new.split(',').next())
                .and_then(|start| start.parse().ok())
                .unwrap_or(0);
        } else if in_hunk && let Some(added) = line.strip_prefix('+') {
            if let Some(path) = &path {
                found.extend(secretshape::leaks(added).into_iter().map(|leak| Finding {
                    path: path.clone(),
                    line: at,
                    kind: leak.kind,
                    masked: secretshape::masked(&added[leak.span]),
                }));
            }
            at += 1;
        } else if in_hunk && line.starts_with(' ') {
            at += 1;
        }
    }
    found
}

/// The most findings a refusal lists; the rest are counted.
const LISTED: usize = 20;

/// What the commit is refused with, on standard error: every finding, masked.
pub fn refusal(found: &[Finding]) -> String {
    let mut out = String::from(
        "charter: commit refused — it would publish what looks like a secret or personal data:\n",
    );
    for finding in found.iter().take(LISTED) {
        out.push_str(&format!("  {}\n", one_line(finding)));
    }
    if found.len() > LISTED {
        out.push_str(&format!("  … and {} more\n", found.len() - LISTED));
    }
    out.push_str(
        "Take it out of the change and commit again. A credential belongs in a vault \
         (`charter secret`), and personal data does not belong in a repository.\n",
    );
    out
}

/// What the app's needs-you item says about a refusal in `repo`: the repository's name and the
/// first finding, masked, with how many more there are.
pub fn summary(repo: &Path, found: &[Finding]) -> String {
    let name = repo.file_name().map_or_else(
        || repo.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    );
    let first = found.first().map(one_line).unwrap_or_default();
    match found.len() {
        0 | 1 => format!("commit refused in {name}: {first}"),
        n => format!("commit refused in {name}: {first} (and {} more)", n - 1),
    }
}

/// One finding as a line: where, what, and the value masked.
pub fn one_line(finding: &Finding) -> String {
    format!(
        "{}:{}  {}  {}",
        finding.path, finding.line, finding.kind, finding.masked
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testgit;

    fn repo() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let repo = std::fs::canonicalize(dir.path()).unwrap();
        testgit::run(&repo, &["init", "-q", "-b", "main", "."]);
        testgit::run(&repo, &["config", "user.name", "t"]);
        testgit::run(&repo, &["config", "user.email", "t@example.invalid"]);
        (dir, repo)
    }

    fn key() -> String {
        ["ghp", "_0123456789abcdefABCDEF"].concat()
    }

    #[test]
    fn a_planted_key_and_email_in_the_staged_diff_are_found_masked_with_path_and_line() {
        let (_dir, repo) = repo();
        std::fs::write(repo.join("app.py"), "print('hi')\n").unwrap();
        testgit::run(&repo, &["add", "app.py"]);
        testgit::run(&repo, &["commit", "-q", "-m", "one"]);
        std::fs::write(
            repo.join("app.py"),
            format!(
                "print('hi')\nTOKEN = '{}'\nOWNER = 'ada@lovelace.dev'\n",
                key()
            ),
        )
        .unwrap();
        testgit::run(&repo, &["add", "app.py"]);

        let found = staged(&repo).unwrap();
        assert_eq!(
            found,
            vec![
                Finding {
                    path: "app.py".into(),
                    line: 2,
                    kind: "a token by its forge's prefix",
                    masked: "ghp_**********************".into(),
                },
                Finding {
                    path: "app.py".into(),
                    line: 3,
                    kind: "an email address",
                    masked: "ad**************".into(),
                },
            ]
        );
        let said = refusal(&found);
        assert!(
            !said.contains(&key()) && !said.contains("ada@lovelace.dev"),
            "{said}"
        );
        assert!(
            said.contains("app.py:2") && said.contains("app.py:3"),
            "{said}"
        );
    }

    #[test]
    fn only_added_lines_count_and_a_clean_diff_finds_nothing() {
        let (_dir, repo) = repo();
        // Already committed, and removed: neither is this commit publishing it.
        std::fs::write(repo.join("old.txt"), format!("{}\nkeep\n", key())).unwrap();
        std::fs::write(repo.join("owners"), "ada@lovelace.dev\n").unwrap();
        testgit::run(&repo, &["add", "."]);
        testgit::run(&repo, &["commit", "-q", "-m", "one"]);
        std::fs::write(repo.join("old.txt"), "keep\n").unwrap();
        std::fs::write(repo.join("owners"), "ada@lovelace.dev\nplain\n").unwrap();
        testgit::run(&repo, &["add", "."]);
        assert_eq!(staged(&repo).unwrap(), Vec::new());
    }

    #[test]
    fn a_new_file_and_a_binary_file_are_read_as_git_shows_them() {
        let (_dir, repo) = repo();
        std::fs::write(repo.join("new.env"), format!("\n\nX={}\n", key())).unwrap();
        std::fs::write(repo.join("blob.bin"), b"\0\0ada@lovelace.dev\0").unwrap();
        testgit::run(&repo, &["add", "."]);
        let found = staged(&repo).unwrap();
        assert_eq!(
            found
                .iter()
                .map(|f| (f.path.as_str(), f.line))
                .collect::<Vec<_>>(),
            vec![("new.env", 3)]
        );
    }
}
