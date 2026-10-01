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
//! staged file by ([`crate::secretshape::found`]), and so is one with a URL that carries a
//! password: memory is never where a secret goes. The reason names the line, never the value.
//! So is a file larger than [`LARGEST`] or not UTF-8. Each says why, so a file that is missing
//! from the offer is never merely missing.
//!
//! **Some files are offered with their box unticked**, and say why: one longer than
//! [`TICKED_UP_TO`], since every chat in the workspace reads all of it, and one holding an
//! invisible character ([`is_invisible`]) — a zero-width, bidirectional or tag character can
//! hide an instruction in text that looks harmless, and the preview draws each by its code
//! point.
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

/// One file of instructions and its text: what the preview shows, and — handed back — what the
/// operator said yes to. One type for both, so the yes names a file exactly as it was shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shown {
    /// The clone's name in the workspace.
    pub repo: String,
    /// The file's path inside the clone, `/`-separated.
    pub file: String,
    /// What the file holds; empty when it was left out before it was read.
    pub text: String,
}

/// One file of instructions, as found in one of a workspace's clones.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub shown: Shown,
    /// Whether it can be taken in.
    pub standing: Standing,
}

/// Whether a found file can be taken into memory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Standing {
    /// Offered: nothing in the workspace's memory holds it yet. `caution` is why its box
    /// starts unticked, when something about it deserves a second look first.
    Offered { caution: Option<String> },
    /// A memory in the workspace's journal already holds this text.
    InMemory,
    /// It cannot go into memory, and this says why.
    LeftOut(String),
}

impl Standing {
    fn offered(&self) -> bool {
        matches!(self, Self::Offered { .. })
    }
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
            if one.standing.offered() && py_strip(&one.shown.text).is_empty() {
                continue;
            }
            if one.standing.offered() && kept.iter().any(|body| *body == py_strip(&one.shown.text))
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
/// **All or nothing.** Every one is checked before any is written: it must be a file [`found`]
/// offers now, holding exactly the text the preview showed, and anything else refuses the whole
/// import. A write that fails part way takes back the memories this import already wrote, so a
/// refusal always means nothing was added.
///
/// **Once.** A file chosen twice is written once, and the check and the writes are held under
/// a lock on the workspace's directory, so a second import of the same files — a double press,
/// or a second window — waits, then finds them already in memory.
pub fn import(
    plane: &Path,
    ws: &str,
    chosen: &[Shown],
    stamp: chrono::NaiveDateTime,
) -> Result<usize, String> {
    let workspace = Plane::open(plane)
        .workspace(ws)
        .map_err(|why| why.to_string())?;
    let mut unique: Vec<&Shown> = Vec::new();
    for one in chosen {
        if !unique.contains(&one) {
            unique.push(one);
        }
    }
    // Not the journal's own lock, which every memory write takes and a second `Lock::on` of
    // the same directory in this process would wait on forever (`rewrite::Lock`).
    let _held = crate::rewrite::Lock::on(workspace.dir());
    let now = found(plane, ws)?;
    for one in &unique {
        refusal(&now, one).map_or(Ok(()), Err)?;
    }
    let mut written: Vec<std::path::PathBuf> = Vec::new();
    for one in &unique {
        match workspace.remember_titled(&one.text, Some(&title(one)), stamp) {
            Ok(path) => written.push(path),
            Err(why) => {
                take_back(plane, &workspace.dir().join("memory"), &written);
                return Err(format!(
                    "charter could not add {}/{} to memory ({why}), so nothing was added.",
                    one.repo, one.file
                ));
            }
        }
    }
    Ok(written.len())
}

/// Why `one` cannot be written, given what [`found`] answers now; `None` when it can.
fn refusal(now: &[Found], one: &Shown) -> Option<String> {
    let Some(there) = now
        .iter()
        .find(|found| found.shown.repo == one.repo && found.shown.file == one.file)
    else {
        return Some(format!(
            "{}/{} is not one of the repo's instruction files charter found, so nothing was \
             added to memory.",
            one.repo, one.file
        ));
    };
    match &there.standing {
        Standing::Offered { .. } if there.shown.text == one.text => None,
        Standing::Offered { .. } => Some(format!(
            "{}/{} has changed since it was shown, so nothing was added to memory. Look at it \
             again, then add it.",
            one.repo, one.file
        )),
        Standing::InMemory => Some(format!(
            "{}/{} is already in memory, so nothing was added.",
            one.repo, one.file
        )),
        Standing::LeftOut(why) => Some(format!(
            "{}/{} is left out: {why}. Nothing was added to memory.",
            one.repo, one.file
        )),
    }
}

/// Removes the memories an import wrote before one of its writes failed, index lines and all.
/// Best effort: a memory that cannot be taken back is one the operator can see and delete.
fn take_back(plane: &Path, journal: &Path, written: &[std::path::PathBuf]) {
    for path in written {
        if let Some(name) = path.file_name() {
            let _ = crate::memstore::forget(plane, journal, &name.to_string_lossy());
        }
    }
}

/// A memory's title for an imported file: `CLAUDE.md from svc`.
fn title(one: &Shown) -> String {
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
///
/// **Read through the handle that was checked**: the file is opened `O_NOFOLLOW`, and its kind
/// and size are asked of that open file, so a link swapped in after the `lstat` is refused by
/// the open rather than followed.
fn read(repo: &str, clone: &Path, file: &str) -> Found {
    use std::io::Read;
    let left_out = |why: String| Found {
        shown: Shown {
            repo: repo.to_owned(),
            file: file.to_owned(),
            text: String::new(),
        },
        standing: Standing::LeftOut(why),
    };
    const LINK: &str = "it is a link, and charter reads only files that are in the repo itself";
    let path = clone.join(file);
    match std::fs::symlink_metadata(&path) {
        Ok(meta) if meta.file_type().is_symlink() => return left_out(LINK.to_owned()),
        Ok(_) => {}
        Err(why) => return left_out(format!("charter could not read it ({why})")),
    }
    let opened = crate::contain::nofollow(std::fs::OpenOptions::new().read(true)).open(&path);
    let handle = match opened {
        Ok(handle) => handle,
        // `ELOOP`: it became a link after the `lstat`.
        Err(why) if is_a_link(&why) => return left_out(LINK.to_owned()),
        Err(why) => return left_out(format!("charter could not read it ({why})")),
    };
    let meta = match handle.metadata() {
        Ok(meta) => meta,
        Err(why) => return left_out(format!("charter could not read it ({why})")),
    };
    if !meta.is_file() {
        return left_out("it is not a file".to_owned());
    }
    if meta.len() > LARGEST {
        return left_out(too_large());
    }
    let mut bytes = Vec::new();
    if let Err(why) = handle.take(LARGEST + 1).read_to_end(&mut bytes) {
        return left_out(format!("charter could not read it ({why})"));
    }
    if bytes.len() as u64 > LARGEST {
        return left_out(too_large());
    }
    let Ok(text) = String::from_utf8(bytes) else {
        return left_out("it is not text".to_owned());
    };
    if let Some(found) = crate::secretshape::found(&text) {
        return left_out(format!(
            "line {} holds what looks like a secret ({}), and a secret never goes into memory",
            found.line, found.kind
        ));
    }
    if let Some(line) = password_in_a_url(&text) {
        return left_out(format!(
            "line {line} has a URL that carries a password, and a secret never goes into memory"
        ));
    }
    let caution = [invisible(&text), past_ticking(&text)]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    Found {
        shown: Shown {
            repo: repo.to_owned(),
            file: file.to_owned(),
            text,
        },
        standing: Standing::Offered {
            caution: (!caution.is_empty()).then(|| caution.join("; ")),
        },
    }
}

/// Whether an `O_NOFOLLOW` open was refused because the last component is a link.
fn is_a_link(why: &std::io::Error) -> bool {
    #[cfg(unix)]
    {
        why.raw_os_error() == Some(rustix::io::Errno::LOOP.raw_os_error())
    }
    #[cfg(not(unix))]
    {
        let _ = why;
        false
    }
}

fn too_large() -> String {
    format!(
        "it is larger than {} KB, which is more than a memory should hold",
        LARGEST / 1024
    )
}

/// The largest file whose box starts ticked, in bytes. Every chat in the workspace reads its
/// memory at its start, so a longer file is shown and left for the operator to tick.
pub const TICKED_UP_TO: usize = 8 * 1024;

fn past_ticking(text: &str) -> Option<String> {
    (text.len() > TICKED_UP_TO).then(|| {
        format!(
            "it is longer than {} KB, and every chat here would read all of it, so it starts \
             unticked",
            TICKED_UP_TO / 1024
        )
    })
}

/// A character that changes what text means without being seen: zero-width characters, the
/// bidirectional controls, and the Unicode tag block, which can carry a whole hidden sentence.
/// The preview draws each as its code point; this unticks the file and says where the first is.
pub fn is_invisible(c: char) -> bool {
    matches!(
        c,
        '\u{00AD}'
            | '\u{061C}'
            | '\u{180E}'
            | '\u{200B}'..='\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2060}'..='\u{2064}'
            | '\u{2066}'..='\u{2069}'
            | '\u{FEFF}'
            | '\u{E0000}'..='\u{E007F}'
    )
}

fn invisible(text: &str) -> Option<String> {
    text.lines().enumerate().find_map(|(at, line)| {
        line.chars().find(|c| is_invisible(*c)).map(|c| {
            format!(
                "line {} holds an invisible character (U+{:04X}), drawn in the preview by its \
                 code point, so it starts unticked",
                at + 1,
                u32::from(c)
            )
        })
    })
}

/// The line of the first URL that carries a password (`scheme://user:password@host`).
fn password_in_a_url(text: &str) -> Option<usize> {
    static URL: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"[A-Za-z][A-Za-z0-9+.-]*://[^\s/@:]+:[^\s/@]+@")
            .expect("the pattern compiles")
    });
    URL.find(text)
        .map(|hit| 1 + text[..hit.start()].matches('\n').count())
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
                shown: Shown {
                    repo: "svc".into(),
                    file: "CLAUDE.md".into(),
                    text: "# Rules\n\nRun the tests first.\n".into(),
                },
                standing: Standing::Offered { caution: None },
            }]
        );
    }

    fn files(found: &[Found]) -> Vec<&str> {
        found.iter().map(|one| one.shown.file.as_str()).collect()
    }

    fn standing(found: &[Found], file: &str) -> Standing {
        found
            .iter()
            .find(|one| one.shown.file == file)
            .map(|one| one.standing.clone())
            .unwrap_or_else(|| panic!("{file} was not found"))
    }

    fn chosen(found: &[Found]) -> Vec<Shown> {
        found.iter().map(|one| one.shown.clone()).collect()
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
        assert!(
            found
                .iter()
                .all(|one| one.standing == Standing::Offered { caution: None })
        );
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
        let asked = [Shown {
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

    #[cfg(unix)]
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
        assert!(found[0].shown.text.is_empty(), "the link's target was read");
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

    fn caution(found: &[Found], file: &str) -> Option<String> {
        match standing(found, file) {
            Standing::Offered { caution } => caution,
            other => panic!("{file} is not offered: {other:?}"),
        }
    }

    fn left_out(found: &[Found], file: &str) -> String {
        match standing(found, file) {
            Standing::LeftOut(why) => why,
            other => panic!("{file} is not left out: {other:?}"),
        }
    }

    fn index_lines(root: &Path) -> usize {
        std::fs::read_to_string(root.join("workspaces/svc/memory/MEMORY.md"))
            .unwrap_or_default()
            .lines()
            .filter(|line| line.starts_with("- ["))
            .count()
    }

    #[test]
    fn a_file_past_the_ticking_size_is_offered_with_its_box_unticked() {
        let big = "Run the tests first.\n".repeat(500);
        let (_dir, root) = a_workspace(&[("CLAUDE.md", &big), ("AGENTS.md", "Short.\n")]);

        let found = found(&root, "svc").expect("read");

        let why = caution(&found, "CLAUDE.md").expect("a large file starts unticked");
        assert!(why.contains("8 KB"), "{why}");
        assert_eq!(caution(&found, "AGENTS.md"), None);
    }

    #[test]
    fn invisible_characters_are_named_with_their_line_and_untick_the_file() {
        let (_dir, root) = a_workspace(&[
            ("CLAUDE.md", "Be kind.\nRun\u{200b} the tests.\n"),
            ("AGENTS.md", "Fine.\n\u{202e}reversed\n"),
            (".cursor/rules/tag.mdc", "Tagged\u{e0041} text.\n"),
        ]);

        let found = found(&root, "svc").expect("read");

        for (file, code, line) in [
            ("CLAUDE.md", "U+200B", "line 2"),
            ("AGENTS.md", "U+202E", "line 2"),
            (".cursor/rules/tag.mdc", "U+E0041", "line 1"),
        ] {
            let why = caution(&found, file).unwrap_or_else(|| panic!("{file} is ticked"));
            assert!(why.contains(code) && why.contains(line), "{file}: {why}");
        }
    }

    #[test]
    fn a_secret_is_named_by_its_line_and_never_by_its_value() {
        let value = ["ghp", "_0123456789abcdefABCDEF0123456789abcd"].concat();
        let text = format!("# Tokens\n\ntoken = {value}\n");
        let (_dir, root) = a_workspace(&[("AGENTS.md", &text)]);

        let why = left_out(&found(&root, "svc").expect("read"), "AGENTS.md");

        assert!(why.contains("line 3"), "{why}");
        assert!(
            !why.contains(&value) && !why.contains("0123456789"),
            "{why}"
        );
    }

    #[test]
    fn a_url_that_carries_a_password_is_left_out_by_its_line() {
        let (_dir, root) = a_workspace(&[
            (
                "CLAUDE.md",
                "Connect with\n\npostgres://admin:hunter2@db.internal:5432/app\n",
            ),
            (
                "AGENTS.md",
                "Docs: https://example.com/a@b and git@github.com:o/r\n",
            ),
        ]);

        let found = found(&root, "svc").expect("read");

        let why = left_out(&found, "CLAUDE.md");
        assert!(why.contains("line 3") && why.contains("password"), "{why}");
        assert!(!why.contains("hunter2"), "{why}");
        assert_eq!(caution(&found, "AGENTS.md"), None);
    }

    #[cfg(unix)]
    #[test]
    fn a_write_that_fails_part_way_leaves_no_memory_behind() {
        let (_dir, root) = a_workspace(&[
            ("CLAUDE.md", "Run the tests first.\n"),
            ("AGENTS.md", "Agents: run make check.\n"),
        ]);
        let offered = chosen(&found(&root, "svc").expect("read"));
        // The name the second memory would take is a link out of the plane, which the store
        // refuses to write through: the first memory is written, the second fails.
        let memory = root.join("workspaces/svc/memory");
        std::fs::create_dir_all(&memory).expect("the journal");
        std::os::unix::fs::symlink(
            "/nonexistent-outside/agents.md",
            memory.join("20261001-093000-agents-md-from-svc.md"),
        )
        .expect("a planted link");

        let refused = import(&root, "svc", &offered, stamp()).expect_err("the second fails");

        assert!(refused.contains("AGENTS.md"), "{refused}");
        assert!(
            memories(&root).is_empty(),
            "the first memory was left behind"
        );
        assert_eq!(index_lines(&root), 0, "an index line was left behind");
    }

    #[test]
    fn a_file_chosen_twice_is_written_once() {
        let (_dir, root) = a_workspace(&[("CLAUDE.md", "Run the tests first.\n")]);
        let one = chosen(&found(&root, "svc").expect("read"));
        let twice = [one.clone(), one].concat();

        assert_eq!(import(&root, "svc", &twice, stamp()), Ok(1));
        assert_eq!(memories(&root).len(), 1);
    }

    #[test]
    fn two_imports_at_once_write_each_file_once() {
        let (_dir, root) = a_workspace(&[("CLAUDE.md", "Run the tests first.\n")]);
        let offered = chosen(&found(&root, "svc").expect("read"));
        let start = std::sync::Barrier::new(2);

        let answers: Vec<Result<usize, String>> = std::thread::scope(|scope| {
            let both: Vec<_> = (0..2)
                .map(|_| {
                    scope.spawn(|| {
                        start.wait();
                        import(&root, "svc", &offered, stamp())
                    })
                })
                .collect();
            both.into_iter()
                .map(|one| one.join().expect("an import"))
                .collect()
        });

        assert_eq!(memories(&root).len(), 1, "{answers:?}");
        assert_eq!(answers.iter().filter(|one| one.is_ok()).count(), 1);
    }
}
