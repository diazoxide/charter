//! The instructions a repo carries for AI agents, taken into its workspace's memory (FR-18a,
//! #612).
//!
//! A repo that already works with an agent has told it how: `CLAUDE.md`, `AGENTS.md`, and
//! Cursor's `.cursor/rules/`. W10 asks for them to be brought in on the first run, with a
//! preview before anything is written, so every chat in the workspace starts with them —
//! whichever harness runs it, and not only the one whose file it is.
//!
//! Two calls, and the second only on the preview's yes:
//!
//! - [`found`]: every such file in the workspace's clones, with its whole text and whether it
//!   can go into memory. Reads, and writes nothing.
//! - [`import`]: the files the operator ticked, written into the workspace's memory. It is
//!   handed back the text the preview showed and writes a file only while it still holds
//!   exactly that; one that changed, or that is no longer offered, refuses the whole import and
//!   nothing is written, because the yes was to the preview.
//!
//! # What is read, and from where
//!
//! **The workspace's clones, never the repo the operator picked**: the clone is charter's copy,
//! so nothing here touches the operator's repo, and nothing here writes into the clone either.
//! Only `CLAUDE.md` and `AGENTS.md` at a clone's top, and the `.md` and `.mdc` files under its
//! `.cursor/rules/`, at any depth up to [`RULES_DEPTH`].
//!
//! **A link is never followed.** A committed `CLAUDE.md -> ~/.aws/credentials` travels with
//! the repo, and following it would copy a file from outside the repo into memory, which a
//! LIVE workspace commits and pushes. A link is listed as left out, and its target is not read.
//! A `.cursor` or `rules` directory that is a link is not walked at all.
//!
//! **A file that looks like it holds a secret is left out**, by the rule a plane save refuses a
//! staged file by ([`crate::secretshape::found`]): memory is never where a secret goes. So is
//! one larger than [`LARGEST`] or not UTF-8. Each says why, so a file that is missing from the
//! offer is never merely missing.
//!
//! # Where it is written
//!
//! Each file is one memory in the workspace's journal (`workspaces/<ws>/memory/`), titled
//! `<file> from <repo>`, its text as it is. That store's tier is already named — Plane when the
//! workspace is LIVE, Clone state when LOCAL (`docs/plane-format.md`) — and nothing here keeps
//! a store of its own: a file is "already in memory" when a memory in the journal holds the
//! same text, which is read from the journal itself.

use std::path::Path;

use crate::memstore::py_strip;
use crate::workspaces::Plane;

/// The largest file taken into memory, in bytes. A memory is read whole into a chat's
/// briefing and search; a rules file past this is a document, not an instruction.
pub const LARGEST: u64 = 64 * 1024;

/// How deep under `.cursor/rules/` a rule is looked for.
pub const RULES_DEPTH: usize = 4;

/// The files at a clone's top that are instructions, in the order they are offered.
const AT_TOP: [&str; 2] = ["CLAUDE.md", "AGENTS.md"];

/// Cursor's rules directory, from a clone's top.
const RULES: [&str; 2] = [".cursor", "rules"];

/// One file of instructions, as found in one of a workspace's clones.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    /// The clone's name in the workspace.
    pub repo: String,
    /// The file's path inside the clone, `/`-separated.
    pub file: String,
    /// What the file holds; empty when it was left out before it was read.
    pub text: String,
    /// Whether it can be taken in.
    pub standing: Standing,
}

/// Whether a found file can be taken into memory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Standing {
    /// Offered: nothing in the workspace's memory holds it yet.
    Offered,
    /// A memory in the workspace's journal already holds this text.
    InMemory,
    /// It cannot go into memory, and this says why.
    LeftOut(String),
}

/// One file the operator ticked, with the text the preview showed them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chosen {
    pub repo: String,
    pub file: String,
    pub text: String,
}

/// Every file of instructions in the clones of workspace `ws` of the plane at `plane`, clone
/// by clone in name order: `CLAUDE.md`, `AGENTS.md`, then the rules in path order. A file
/// holding nothing but whitespace is not listed. Writes nothing.
pub fn found(plane: &Path, ws: &str) -> Result<Vec<Found>, String> {
    let workspace = Plane::open(plane)
        .workspace(ws)
        .map_err(|why| why.to_string())?;
    let clones = crate::repos::clones(plane, ws).map_err(|why| why.to_string())?;
    // An unreadable journal holds nothing to compare with: every file is offered, and the
    // write, which asks the same questions, is where a real failure is said.
    let kept: Vec<String> = workspace
        .memories()
        .unwrap_or_default()
        .into_iter()
        .map(|entry| entry.body)
        .collect();
    let mut out = Vec::new();
    for repo in clones.repos {
        for file in candidates(&repo.path) {
            let mut one = read(&repo.name, &repo.path, &file);
            if one.standing == Standing::Offered && py_strip(&one.text).is_empty() {
                continue;
            }
            if one.standing == Standing::Offered
                && kept.iter().any(|body| *body == py_strip(&one.text))
            {
                one.standing = Standing::InMemory;
            }
            out.push(one);
        }
    }
    Ok(out)
}

/// Writes each of `chosen` into workspace `ws`'s memory, and answers how many were written.
///
/// Every one is checked before any is written: it must be a file [`found`] offers now, holding
/// exactly the text the preview showed. Anything else refuses the whole import and writes
/// nothing.
pub fn import(
    plane: &Path,
    ws: &str,
    chosen: &[Chosen],
    stamp: chrono::NaiveDateTime,
) -> Result<usize, String> {
    let now = found(plane, ws)?;
    for one in chosen {
        let Some(there) = now
            .iter()
            .find(|found| found.repo == one.repo && found.file == one.file)
        else {
            return Err(format!(
                "{}/{} is not one of the repo's instruction files charter found, so nothing was \
                 added to memory.",
                one.repo, one.file
            ));
        };
        match &there.standing {
            Standing::Offered => {}
            Standing::InMemory => {
                return Err(format!(
                    "{}/{} is already in memory, so nothing was added.",
                    one.repo, one.file
                ));
            }
            Standing::LeftOut(why) => {
                return Err(format!(
                    "{}/{} is left out: {why}. Nothing was added to memory.",
                    one.repo, one.file
                ));
            }
        }
        if there.text != one.text {
            return Err(format!(
                "{}/{} has changed since it was shown, so nothing was added to memory. Look at \
                 it again, then add it.",
                one.repo, one.file
            ));
        }
    }
    let workspace = Plane::open(plane)
        .workspace(ws)
        .map_err(|why| why.to_string())?;
    for one in chosen {
        workspace
            .remember_titled(&one.text, Some(&title(one)), stamp)
            .map_err(|why| {
                format!(
                    "charter could not add {}/{} to memory ({why}).",
                    one.repo, one.file
                )
            })?;
    }
    Ok(chosen.len())
}

/// A memory's title for an imported file: `CLAUDE.md from svc`.
fn title(one: &Chosen) -> String {
    format!("{} from {}", one.file, one.repo)
}

/// The instruction files in the clone at `clone`, `/`-separated from its top, in the order
/// they are offered. Every entry is asked about without following a link.
fn candidates(clone: &Path) -> Vec<String> {
    let mut out: Vec<String> = AT_TOP
        .iter()
        .filter(|name| std::fs::symlink_metadata(clone.join(name)).is_ok())
        .map(|name| (*name).to_owned())
        .collect();
    let mut rules = Vec::new();
    let mut dir = clone.to_path_buf();
    for part in RULES {
        dir.push(part);
        if !is_real_dir(&dir) {
            return out;
        }
    }
    walk(&dir, &RULES.join("/"), RULES_DEPTH, &mut rules);
    rules.sort();
    out.extend(rules);
    out
}

/// A directory that is itself a directory, not a link to one.
fn is_real_dir(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_dir())
}

/// The rules under `dir`, which is `shown` from the clone's top, down to `depth` more levels.
/// A link to a directory is not walked; a link named like a rule is listed, to be left out.
fn walk(dir: &Path, shown: &str, depth: usize, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            if depth > 1 {
                walk(&entry.path(), &format!("{shown}/{name}"), depth - 1, out);
            }
        } else if name.ends_with(".md") || name.ends_with(".mdc") {
            out.push(format!("{shown}/{name}"));
        }
    }
}

/// `file` in the clone at `clone`, read if it can go into memory, or left out with why.
fn read(repo: &str, clone: &Path, file: &str) -> Found {
    let left_out = |why: String| Found {
        repo: repo.to_owned(),
        file: file.to_owned(),
        text: String::new(),
        standing: Standing::LeftOut(why),
    };
    let path = clone.join(file);
    let meta = match std::fs::symlink_metadata(&path) {
        Ok(meta) => meta,
        Err(why) => return left_out(format!("charter could not read it ({why})")),
    };
    if meta.file_type().is_symlink() {
        return left_out(
            "it is a link, and charter reads only files that are in the repo itself".to_owned(),
        );
    }
    if !meta.is_file() {
        return left_out("it is not a file".to_owned());
    }
    if meta.len() > LARGEST {
        return left_out(format!(
            "it is larger than {} KB, which is more than a memory should hold",
            LARGEST / 1024
        ));
    }
    let text = match std::fs::read(&path).map(String::from_utf8) {
        Ok(Ok(text)) => text,
        Ok(Err(_)) => return left_out("it is not text".to_owned()),
        Err(why) => return left_out(format!("charter could not read it ({why})")),
    };
    if let Some(found) = crate::secretshape::found(&text) {
        return left_out(format!(
            "it holds what looks like a secret ({}), and a secret never goes into memory",
            found.kind
        ));
    }
    Found {
        repo: repo.to_owned(),
        file: file.to_owned(),
        text,
        standing: Standing::Offered,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// A plane with workspace `svc` holding a clone `svc`, with `files` committed in it.
    fn a_workspace(files: &[(&str, &str)]) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("a directory");
        let root =
            crate::firstrun::ensure_local_plane(&dir.path().join("cfg")).expect("a local plane");
        let repo = dir.path().join("svc");
        std::fs::create_dir_all(&repo).expect("the repo's directory");
        for (file, text) in files {
            let at = repo.join(file);
            std::fs::create_dir_all(at.parent().expect("a parent")).expect("its directory");
            std::fs::write(at, text).expect("a file");
        }
        for argv in [
            vec!["init", "-q", "-b", "main", "."],
            vec!["config", "user.email", "t@e.invalid"],
            vec!["config", "user.name", "t"],
            vec!["add", "-A"],
            vec!["commit", "-q", "--allow-empty", "-m", "first"],
        ] {
            assert!(crate::testgit::run(&repo, &argv).ok(), "git {argv:?}");
        }
        crate::firstrun::take_in(&root, &repo).expect("taken in");
        (dir, root)
    }

    #[test]
    fn a_claude_md_in_the_repo_is_offered_as_it_is() {
        let (_dir, root) = a_workspace(&[("CLAUDE.md", "# Rules\n\nRun the tests first.\n")]);

        let found = found(&root, "svc").expect("the workspace is read");

        assert_eq!(
            found,
            vec![Found {
                repo: "svc".into(),
                file: "CLAUDE.md".into(),
                text: "# Rules\n\nRun the tests first.\n".into(),
                standing: Standing::Offered,
            }]
        );
    }

    fn files(found: &[Found]) -> Vec<&str> {
        found.iter().map(|one| one.file.as_str()).collect()
    }

    fn standing(found: &[Found], file: &str) -> Standing {
        found
            .iter()
            .find(|one| one.file == file)
            .map(|one| one.standing.clone())
            .unwrap_or_else(|| panic!("{file} was not found"))
    }

    fn chosen(found: &[Found]) -> Vec<Chosen> {
        found
            .iter()
            .map(|one| Chosen {
                repo: one.repo.clone(),
                file: one.file.clone(),
                text: one.text.clone(),
            })
            .collect()
    }

    fn stamp() -> chrono::NaiveDateTime {
        chrono::NaiveDate::from_ymd_opt(2026, 10, 1)
            .and_then(|d| d.and_hms_opt(9, 30, 0))
            .expect("a time")
    }

    fn memories(root: &Path) -> Vec<crate::workspaces::Entry> {
        crate::workspaces::Plane::open(root)
            .workspace("svc")
            .expect("the workspace")
            .memories()
            .unwrap_or_default()
    }

    #[test]
    fn claude_md_agents_md_and_every_cursor_rule_are_found_and_nothing_else() {
        let (_dir, root) = a_workspace(&[
            ("AGENTS.md", "Agents: run make check.\n"),
            ("CLAUDE.md", "Claude: run make check.\n"),
            (
                ".cursor/rules/style.mdc",
                "---\nalwaysApply: true\n---\nUse tabs.\n",
            ),
            (".cursor/rules/api/errors.md", "Errors are values.\n"),
            (".cursor/rules/notes.txt", "not a rule\n"),
            ("README.md", "# svc\n"),
            ("docs/CLAUDE.md", "not at the top\n"),
        ]);

        let found = found(&root, "svc").expect("the workspace is read");

        assert_eq!(
            files(&found),
            vec![
                "CLAUDE.md",
                "AGENTS.md",
                ".cursor/rules/api/errors.md",
                ".cursor/rules/style.mdc",
            ]
        );
        assert!(found.iter().all(|one| one.standing == Standing::Offered));
    }

    #[test]
    fn a_repo_with_none_offers_nothing() {
        let (_dir, root) = a_workspace(&[("README.md", "# svc\n"), ("CLAUDE.md", "  \n\n")]);

        assert_eq!(found(&root, "svc").expect("read"), Vec::new());
    }

    #[test]
    fn each_file_type_imports_into_the_workspaces_memory_as_it_is() {
        let (_dir, root) = a_workspace(&[
            ("CLAUDE.md", "# Rules\n\nRun the tests first.\n"),
            ("AGENTS.md", "Agents: run make check.\n"),
            (".cursor/rules/style.mdc", "Use tabs.\n"),
        ]);
        let offered = found(&root, "svc").expect("read");

        let written = import(&root, "svc", &chosen(&offered), stamp()).expect("imported");

        assert_eq!(written, 3);
        // One memory each, in the journal's own order (its filenames, all stamped this second).
        let kept: std::collections::BTreeSet<(String, String)> = memories(&root)
            .into_iter()
            .map(|entry| (entry.title, entry.body))
            .collect();
        let expected: std::collections::BTreeSet<(String, String)> = [
            ("CLAUDE.md from svc", "# Rules\n\nRun the tests first."),
            ("AGENTS.md from svc", "Agents: run make check."),
            (".cursor/rules/style.mdc from svc", "Use tabs."),
        ]
        .into_iter()
        .map(|(title, body)| (title.to_owned(), body.to_owned()))
        .collect();
        assert_eq!(kept, expected);
    }

    #[test]
    fn a_file_already_in_memory_is_not_offered_again() {
        let (_dir, root) = a_workspace(&[("CLAUDE.md", "Run the tests first.\n")]);
        let offered = found(&root, "svc").expect("read");
        import(&root, "svc", &chosen(&offered), stamp()).expect("imported");

        let again = found(&root, "svc").expect("read again");

        assert_eq!(standing(&again, "CLAUDE.md"), Standing::InMemory);
        let refused = import(&root, "svc", &chosen(&again), stamp()).expect_err("not twice");
        assert!(refused.contains("already in memory"), "{refused}");
        assert_eq!(memories(&root).len(), 1);
    }

    #[test]
    fn nothing_is_written_unless_it_is_what_the_preview_showed() {
        let (_dir, root) = a_workspace(&[
            ("CLAUDE.md", "Run the tests first.\n"),
            ("AGENTS.md", "Agents: run make check.\n"),
        ]);
        let mut shown = chosen(&found(&root, "svc").expect("read"));
        shown[1].text = "Something the preview never showed.\n".into();

        let refused = import(&root, "svc", &shown, stamp()).expect_err("refused");

        assert!(refused.contains("changed since"), "{refused}");
        // Not even the one that did match: the yes was to the whole preview.
        assert!(memories(&root).is_empty());
    }

    #[test]
    fn a_file_that_was_not_found_is_refused() {
        let (_dir, root) = a_workspace(&[("CLAUDE.md", "Run the tests first.\n")]);
        let asked = [Chosen {
            repo: "svc".into(),
            file: "../../../../etc/hosts".into(),
            text: "127.0.0.1 localhost\n".into(),
        }];

        let refused = import(&root, "svc", &asked, stamp()).expect_err("refused");

        assert!(refused.contains("not one of"), "{refused}");
        assert!(memories(&root).is_empty());
    }

    #[test]
    fn a_file_that_looks_like_it_holds_a_secret_is_left_out_and_cannot_be_imported() {
        let token = ["token = ghp", "_0123456789abcdefABCDEF0123456789abcd\n"].concat();
        let (_dir, root) = a_workspace(&[("AGENTS.md", &token)]);

        let found = found(&root, "svc").expect("read");

        match standing(&found, "AGENTS.md") {
            Standing::LeftOut(why) => {
                assert!(why.contains("a secret never goes into memory"), "{why}")
            }
            other => panic!("offered a secret: {other:?}"),
        }
        let refused = import(&root, "svc", &chosen(&found), stamp()).expect_err("refused");
        assert!(refused.contains("left out"), "{refused}");
        assert!(memories(&root).is_empty());
    }

    #[test]
    fn a_link_is_left_out_rather_than_followed() {
        let outside = tempfile::tempdir().expect("somewhere else");
        std::fs::write(outside.path().join("secret.md"), "not the repo's\n").expect("a file");
        let (_dir, root) = a_workspace(&[("README.md", "# svc\n")]);
        let clone = root.join("workspaces/svc/svc");
        std::os::unix::fs::symlink(outside.path().join("secret.md"), clone.join("CLAUDE.md"))
            .expect("a link");
        std::os::unix::fs::symlink(outside.path(), clone.join(".cursor")).expect("a link");

        let found = found(&root, "svc").expect("read");

        assert_eq!(files(&found), vec!["CLAUDE.md"]);
        match standing(&found, "CLAUDE.md") {
            Standing::LeftOut(why) => assert!(why.contains("link"), "{why}"),
            other => panic!("a link was followed: {other:?}"),
        }
        assert!(found[0].text.is_empty(), "the link's target was read");
    }

    #[test]
    fn a_file_too_large_for_memory_is_left_out() {
        let big = "Run the tests first.\n".repeat(4000);
        let (_dir, root) = a_workspace(&[("CLAUDE.md", &big)]);

        let found = found(&root, "svc").expect("read");

        match standing(&found, "CLAUDE.md") {
            Standing::LeftOut(why) => assert!(why.contains("larger than"), "{why}"),
            other => panic!("offered: {other:?}"),
        }
    }

    #[test]
    fn importing_writes_nothing_into_the_repo_or_its_clone() {
        let (dir, root) = a_workspace(&[("CLAUDE.md", "Run the tests first.\n")]);
        let repo = dir.path().join("svc");
        let clone = root.join("workspaces/svc/svc");
        let listing = |at: &Path| {
            let mut names: Vec<String> = std::fs::read_dir(at)
                .expect("listed")
                .map(|e| {
                    e.expect("an entry")
                        .file_name()
                        .to_string_lossy()
                        .into_owned()
                })
                .collect();
            names.sort();
            names
        };
        let (repo_before, clone_before) = (listing(&repo), listing(&clone));

        import(
            &root,
            "svc",
            &chosen(&found(&root, "svc").expect("read")),
            stamp(),
        )
        .expect("imported");

        assert_eq!(listing(&repo), repo_before);
        assert_eq!(listing(&clone), clone_before);
        let status = crate::forklock::output(
            std::process::Command::new("git")
                .arg("-C")
                .arg(&clone)
                .args(["status", "--porcelain"]),
        )
        .expect("git runs");
        assert!(String::from_utf8_lossy(&status.stdout).trim().is_empty());
    }
}
