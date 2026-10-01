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
//! # What is let through (SQ-17)
//!
//! [`checked`] is [`staged`] filtered through the allowlist ([`crate::scanallow`]): charter's
//! own entries, and the repository's file as it is at `HEAD`. It also says whether the commit
//! changes that file, which a chat's commit may not.

use std::path::Path;

use crate::scanallow::{self, Allowlist, Entry, Seen};
use crate::secretshape;
use crate::worktree::git;

/// One thing a staged commit would publish.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// The file, as git names it from the top of the work tree.
    pub path: String,
    /// The 1-based line in the new version of the file.
    pub line: usize,
    /// The rule that found it, by the id an allowlist entry names it with.
    pub rule: &'static str,
    /// What it looks like.
    pub kind: &'static str,
    /// The value, masked.
    pub masked: String,
    /// The value's fingerprint ([`scanallow::fingerprint`]), which an entry can name it by.
    pub fingerprint: String,
}

/// A staged commit, scanned and filtered through its allowlist.
#[derive(Debug, Clone, Default)]
pub struct Scan {
    /// What the commit is refused for.
    pub refused: Vec<Finding>,
    /// What an entry let through, and the entry.
    pub allowed: Vec<(Finding, Entry)>,
    /// Entries of the repository's allowlist that allow nothing, and why.
    pub problems: Vec<String>,
    /// Whether the commit changes the allowlist file itself, which only a commit made outside a
    /// chat may.
    pub changes_the_allowlist: bool,
}

/// [`staged`], filtered through charter's own entries and the repository's allowlist as it is at
/// `HEAD` — never the working tree's or the index's, so an entry written and not yet committed
/// allows nothing ([`crate::scanallow`]).
pub fn checked(repo: &Path) -> Result<Scan, String> {
    let found = staged(repo)?;
    let at_head = |args: &[&str]| git::run_in_hook(repo, args, git::READ).ok();
    let file = at_head(&["show", &format!("HEAD:{}", scanallow::FILE)])
        .filter(|run| run.code == Some(0))
        .map(|run| Allowlist::parse(&String::from_utf8_lossy(&run.out)))
        .unwrap_or_default();
    let allowlist = Allowlist::of(file);
    let changed = at_head(&[
        "-c",
        "core.quotePath=false",
        "diff",
        "--cached",
        "--no-renames",
        "--name-only",
        "-z",
    ])
    .filter(|run| run.code == Some(0))
    .ok_or("git could not name the files the commit changes")?;
    let mut scan = Scan {
        problems: allowlist.problems.clone(),
        changes_the_allowlist: String::from_utf8_lossy(&changed.out)
            .split('\0')
            .any(scanallow::is_the_file),
        ..Scan::default()
    };
    for finding in found {
        let seen = Seen {
            rule: finding.rule,
            path: &finding.path,
            fingerprint: &finding.fingerprint,
        };
        match allowlist.allowing(seen) {
            Some(entry) => {
                let entry = entry.clone();
                scan.allowed.push((finding, entry));
            }
            None => scan.refused.push(finding),
        }
    }
    Ok(scan)
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
    in_diff(&String::from_utf8_lossy(&run.out))
}

/// What [`staged`] finds in a unified diff's text, or why the text could not be read — which the
/// caller refuses the commit on, since a file whose name could not be read is a file nobody
/// scanned.
fn in_diff(diff: &str) -> Result<Vec<Finding>, String> {
    let mut found = Vec::new();
    let mut path: Option<String> = None;
    let mut at = 0usize;
    let mut in_hunk = false;
    for line in diff.lines() {
        if line.starts_with("diff --git ") {
            path = None;
            in_hunk = false;
        } else if !in_hunk && let Some(new) = line.strip_prefix("+++ ") {
            path = new_path(new)?;
        } else if let Some(hunk) = line.strip_prefix("@@ ") {
            in_hunk = true;
            at = hunk
                .split_whitespace()
                .find_map(|part| part.strip_prefix('+'))
                .and_then(|new| new.split(',').next())
                .and_then(|start| start.parse().ok())
                .ok_or_else(|| format!("a hunk header git wrote could not be read: {line:?}"))?;
        } else if in_hunk && let Some(added) = line.strip_prefix('+') {
            let Some(path) = &path else {
                return Err("git showed added lines with no file named for them".to_owned());
            };
            found.extend(secretshape::leaks(added).into_iter().map(|leak| Finding {
                path: path.clone(),
                line: at,
                rule: leak.rule,
                kind: leak.kind,
                masked: secretshape::masked(&added[leak.span.clone()]),
                fingerprint: scanallow::fingerprint(&added[leak.span]),
            }));
            at += 1;
        } else if in_hunk && line.starts_with(' ') {
            at += 1;
        }
    }
    Ok(found)
}

/// The file a `+++ ` header names: `b/<path>`, or git's C-quoted `"b/<path>"` for a name
/// holding a quote, a backslash, a control character or (with `core.quotePath` off) nothing
/// else; `None` for `/dev/null`, a deletion. Anything else is an error.
///
/// git ends a name that holds a space with a tab (`diff.c`, so `patch` can find where the name
/// stops); that one tab is not part of the name.
fn new_path(header: &str) -> Result<Option<String>, String> {
    let header = header.strip_suffix('\t').unwrap_or(header);
    if header == "/dev/null" {
        return Ok(None);
    }
    let name = if header.starts_with('"') {
        unquoted(header)
            .ok_or_else(|| format!("a file name git quoted could not be read: {header:?}"))?
    } else {
        header.to_owned()
    };
    name.strip_prefix("b/")
        .map(|path| Some(path.to_owned()))
        .ok_or_else(|| format!("a file header git wrote could not be read: {header:?}"))
}

/// git's C-quoted name, unquoted (`quote.c`'s `unquote_c_style`): `\"`, `\\`, `\a` `\b` `\t`
/// `\n` `\v` `\f` `\r`, and three octal digits for any other byte. `None` for anything that is
/// not exactly one such quoted string.
fn unquoted(quoted: &str) -> Option<String> {
    let inner = quoted.strip_prefix('"')?.strip_suffix('"')?;
    let mut bytes = Vec::with_capacity(inner.len());
    let mut rest = inner.as_bytes().iter().copied();
    while let Some(byte) = rest.next() {
        match byte {
            b'"' => return None,
            b'\\' => {
                let escaped = rest.next()?;
                bytes.push(match escaped {
                    b'"' => b'"',
                    b'\\' => b'\\',
                    b'a' => 0x07,
                    b'b' => 0x08,
                    b't' => b'\t',
                    b'n' => b'\n',
                    b'v' => 0x0b,
                    b'f' => 0x0c,
                    b'r' => b'\r',
                    b'0'..=b'3' => {
                        let (two, three) = (rest.next()?, rest.next()?);
                        let digit = |d: u8| (b'0'..=b'7').contains(&d).then(|| d - b'0');
                        (escaped - b'0') * 64 + digit(two)? * 8 + digit(three)?
                    }
                    _ => return None,
                });
            }
            other => bytes.push(other),
        }
    }
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

/// What a chat's commit that changes the allowlist is refused with.
pub const ALLOWLIST_REFUSAL: &str = "charter: commit refused — it changes .charter-scan-allow.toml, the \
     scan's allowlist, which only a commit made outside a chat may change. Take the file out of \
     the commit (`git restore --staged .charter-scan-allow.toml`); the operator reviews and \
     commits an entry. Do not use --no-verify; the operator has been told.\n";

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
         (`charter secret`), and personal data does not belong in a repository. If a finding is \
         not what it looks like, `charter scan --explain` names its rule and the entry that \
         would let it through; tell the operator, who adds it to .charter-scan-allow.toml in a \
         commit of their own. Do not use --no-verify; the operator has been told.\n",
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
                    rule: "forge-token",
                    kind: "a token by its forge's prefix",
                    masked: "ghp_**********************".into(),
                    fingerprint: scanallow::fingerprint(&key()),
                },
                Finding {
                    path: "app.py".into(),
                    line: 3,
                    rule: "email",
                    kind: "an email address",
                    masked: "ad**************".into(),
                    fingerprint: scanallow::fingerprint("ada@lovelace.dev"),
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

    #[test]
    fn a_file_whose_name_git_quotes_is_scanned_under_its_own_name() {
        let (_dir, repo) = repo();
        for name in [
            "say \"hi\".txt",
            "tab\there.txt",
            "back\\slash.txt",
            "plain space.txt",
        ] {
            std::fs::write(repo.join(name), format!("{}\n", key())).unwrap();
        }
        testgit::run(&repo, &["add", "."]);

        let mut named: Vec<String> = staged(&repo).unwrap().into_iter().map(|f| f.path).collect();
        named.sort();

        assert_eq!(
            named,
            [
                "back\\slash.txt",
                "plain space.txt",
                "say \"hi\".txt",
                "tab\there.txt"
            ]
        );
    }

    #[test]
    fn a_header_that_cannot_be_read_refuses_rather_than_skips() {
        for diff in [
            "diff --git a/x b/x\n+++ \"b/unterminated\n@@ -0,0 +1 @@\n+x\n",
            "diff --git a/x b/x\n+++ elsewhere/x\n@@ -0,0 +1 @@\n+x\n",
            "diff --git a/x b/x\n+++ b/x\n@@ nonsense @@\n+x\n",
            "diff --git a/x b/x\n@@ -0,0 +1 @@\n+x\n",
        ] {
            assert!(in_diff(diff).is_err(), "{diff:?}");
        }
        assert_eq!(
            unquoted(r#""b/a\"b\\c\td\303\251""#).as_deref(),
            Some("b/a\"b\\c\tdé")
        );
    }

    /// Commits `.charter-scan-allow.toml` with `text` at `HEAD`, as the operator would.
    fn allowlist_at_head(repo: &std::path::Path, text: &str) {
        std::fs::write(repo.join(scanallow::FILE), text).unwrap();
        testgit::run(repo, &["add", scanallow::FILE]);
        testgit::run(repo, &["commit", "-q", "-m", "allowlist"]);
    }

    const DOCS_EMAIL: &str = "[[allow]]\nrule = \"email\"\npaths = [\"docs/**\"]\nreason = \"the docs name their authors\"\n";

    #[test]
    fn an_allowlisted_fixture_passes_and_says_which_entry_let_it() {
        let (_dir, repo) = repo();
        allowlist_at_head(&repo, DOCS_EMAIL);
        std::fs::create_dir(repo.join("docs")).unwrap();
        std::fs::write(repo.join("docs/intro.md"), "Written by ada@lovelace.dev\n").unwrap();
        testgit::run(&repo, &["add", "docs/intro.md"]);

        let scan = checked(&repo).unwrap();

        assert!(scan.refused.is_empty(), "{:?}", scan.refused);
        assert_eq!(scan.allowed.len(), 1);
        assert_eq!(scan.allowed[0].1.origin, scanallow::Origin::File(1));
        assert!(!scan.changes_the_allowlist);
    }

    #[test]
    fn an_entry_written_and_not_committed_allows_nothing() {
        let (_dir, repo) = repo();
        std::fs::write(repo.join("README.md"), "one\n").unwrap();
        testgit::run(&repo, &["add", "README.md"]);
        testgit::run(&repo, &["commit", "-q", "-m", "one"]);
        // In the working tree only: what an agent could write and not commit.
        std::fs::write(repo.join(scanallow::FILE), DOCS_EMAIL).unwrap();
        std::fs::create_dir(repo.join("docs")).unwrap();
        std::fs::write(repo.join("docs/intro.md"), "ada@lovelace.dev\n").unwrap();
        testgit::run(&repo, &["add", "docs/intro.md"]);

        let scan = checked(&repo).unwrap();

        assert_eq!(scan.refused.len(), 1);
        // And staging it is the change a chat's commit may not make.
        testgit::run(&repo, &["add", scanallow::FILE]);
        assert!(checked(&repo).unwrap().changes_the_allowlist);
    }

    #[test]
    fn an_author_in_a_manifest_is_let_through_with_no_allowlist_at_all() {
        let (_dir, repo) = repo();
        std::fs::write(
            repo.join("Cargo.toml"),
            "[package]\nauthors = [\"Ada <ada@lovelace.dev>\"]\n",
        )
        .unwrap();
        std::fs::write(
            repo.join("src.rs"),
            format!("// ada@lovelace.dev {}\n", key()),
        )
        .unwrap();
        testgit::run(&repo, &["add", "."]);

        let scan = checked(&repo).unwrap();

        assert_eq!(
            scan.allowed
                .iter()
                .map(|(f, _)| f.path.as_str())
                .collect::<Vec<_>>(),
            ["Cargo.toml"]
        );
        assert_eq!(
            scan.refused.iter().map(|f| f.rule).collect::<Vec<_>>(),
            ["email", "forge-token"]
        );
    }

    #[test]
    fn a_broken_allowlist_says_so_and_allows_nothing() {
        let (_dir, repo) = repo();
        allowlist_at_head(&repo, "[[allow]]\nrule = \"email\"\n");
        std::fs::write(repo.join("notes.md"), "ada@lovelace.dev\n").unwrap();
        testgit::run(&repo, &["add", "notes.md"]);

        let scan = checked(&repo).unwrap();

        assert_eq!(scan.refused.len(), 1);
        assert!(scan.problems[0].contains("reason"), "{:?}", scan.problems);
    }

    #[test]
    fn moving_the_allowlist_away_is_a_change_to_it() {
        let (_dir, repo) = repo();
        allowlist_at_head(&repo, DOCS_EMAIL);
        testgit::run(&repo, &["mv", scanallow::FILE, "elsewhere.toml"]);

        assert!(checked(&repo).unwrap().changes_the_allowlist);
    }
}
