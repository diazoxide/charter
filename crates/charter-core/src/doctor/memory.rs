//! `memory indexes`: every memory base's `MEMORY.md` agrees with the files beside it.
//!
//! A dangling link makes recall surface a hit nobody can read; an unindexed file is a memory
//! the index — and so the SessionStart digest — never mentions. Both arrive without any
//! concurrency bug: `MEMORY.md` is append-heavy and edited by many hands, and a merge that
//! takes one side drops the other's line while its file survives.
//!
//! WARN, never FAIL: drift is hygiene, not "you cannot work".

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use super::fsx::{self, Unread};
use super::{Doctor, Row, Status};

/// An index this long is worth curating. Not a truncation — the SessionStart digest is
/// bounded anyway — a nudge.
const INDEX_LINES_WARN: usize = 150;

const INDEX: &str = crate::memstore::INDEX;

/// `persona.list_personas`: legacy flat `personas/<name>.md` files, and directories holding a
/// `persona.md` — never `_shared` or any other name starting with `_`.
pub(super) fn list_personas(root: &Path) -> std::io::Result<Vec<String>> {
    let dir = root.join("personas");
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut names = BTreeSet::new();
    for entry in std::fs::read_dir(&dir)? {
        let path = entry?.path();
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if let Some(cut) = name.strip_suffix(".md") {
            // `Path.stem`: a name that is only a leading dot and `md` has no suffix to cut.
            let stem = if cut.is_empty() {
                name.clone()
            } else {
                cut.to_owned()
            };
            if stem.to_lowercase() != "readme" {
                names.insert(stem);
            }
        }
        if path.is_dir() && !name.starts_with('_') && path.join("persona.md").exists() {
            names.insert(name);
        }
    }
    Ok(names.into_iter().collect())
}

/// `memstore.index_refusal`: why charter will not touch a base's `MEMORY.md`. The write-side
/// rule, deliberately: an index that is merely absent is a fresh base, not a defect, while a
/// DANGLING link out of the plane is absent and hostile at once.
fn index_refusal(root: &Path, mem_dir: &Path) -> Option<String> {
    fsx::dir_refusal(root, mem_dir, "write")
        .or_else(|| fsx::write_refusal(root, &mem_dir.join(INDEX)))
}

/// Which kind of base a label is, for the command that repairs it — `charter persona
/// optimize` never touches a workspace, so a hint naming it for one fixes nothing.
fn kind(label: &str) -> &'static str {
    if label.starts_with("ws:") {
        "workspace"
    } else {
        "persona"
    }
}

/// `…, …` over the first `n`, and `, …` when there were more.
fn first(items: &[String], n: usize, sep: &str) -> String {
    let mut out = items.iter().take(n).cloned().collect::<Vec<_>>().join(sep);
    if items.len() > n {
        out.push_str(", …");
    }
    out
}

/// A memory base: its label in the row (`steward`, `_shared`, `ws:alpha`) and its directory.
type Base = (String, PathBuf);

/// Every memory base, labelled — each persona's, the shared namespace, each workspace's —
/// and the directories under `workspaces/` whose kind could not be told. `Err` when a
/// directory the list comes from could not be listed at all.
fn memory_bases(root: &Path) -> Result<(Vec<Base>, Vec<Unread>), String> {
    let personas = root.join("personas");
    let mut bases: Vec<Base> = list_personas(root)
        .map_err(|e| fsx::py_os_error(&e, &personas))?
        .into_iter()
        .map(|name| {
            let dir = personas.join(&name).join("memory");
            (name, dir)
        })
        .collect();
    bases.push((
        crate::personas::SHARED.to_owned(),
        personas.join(crate::personas::SHARED).join("memory"),
    ));
    let (names, unread) =
        fsx::read_workspaces(root).map_err(|e| fsx::py_os_error(&e, &root.join("workspaces")))?;
    for name in names {
        let dir = root.join("workspaces").join(&name).join("memory");
        bases.push((format!("ws:{name}"), dir));
    }
    Ok((bases, unread))
}

pub(super) fn memory_indexes(d: &Doctor) -> Row {
    const NAME: &str = "memory indexes";
    let root = d.root.as_path();
    let (bases, mut unread) = match memory_bases(root) {
        Ok(found) => found,
        Err(why) => return Row::not_checked(NAME, why),
    };

    let (mut dangling, mut unindexed) = (0usize, 0usize);
    let mut worst: Vec<String> = Vec::new();
    let mut large: Vec<String> = Vec::new();
    let mut unindexed_kinds: BTreeSet<&str> = BTreeSet::new();
    let mut large_kinds: BTreeSet<&str> = BTreeSet::new();
    let mut refused: Vec<String> = Vec::new();
    let mut unread_bases = 0usize;
    for (label, mem_dir) in &bases {
        // Not there THROUGH a link is not nothing there: a `memory/` that is a link to nothing
        // is asked about as itself, so the refusal below meets the base it is for.
        let (mut there, mut code) = fsx::existence(mem_dir, true);
        if there == Some(false) {
            (there, code) = fsx::existence(mem_dir, false);
        }
        match there {
            None => {
                unread.push((mem_dir.clone(), code));
                unread_bases += 1;
                continue;
            }
            Some(false) => continue,
            Some(true) => {}
        }
        // LISTED before anything is read from it: a directory charter may not list is not
        // an empty one, and drift described over it would describe a store nobody listed.
        // `memstore`'s listing, which is what `recall` and `optimize` read the store with.
        let (files, missed) = crate::memstore::read_files(root, mem_dir);
        if !missed.is_empty() {
            unread.extend(missed);
            unread_bases += 1;
            continue;
        }
        // Asked FIRST: a refused index answers "nothing is listed", which an empty base
        // answers too, and drift numbers over it would point a repair at a file charter is
        // declining to touch (#349).
        if let Some(why) = index_refusal(root, mem_dir) {
            refused.push(format!("{label}: {why}"));
            continue;
        }
        // `memstore::index_drift`, the one comparison — `charter persona optimize` repairs
        // what it names, and a row describing different drift from the command that fixes it
        // is a row that sends the operator to a repair for something else.
        let Ok((lost, unlinked)) = crate::memstore::index_drift(root, mem_dir) else {
            unread.push((mem_dir.clone(), None));
            unread_bases += 1;
            continue;
        };
        if !lost.is_empty() || !unlinked.is_empty() {
            dangling += lost.len();
            unindexed += unlinked.len();
            if !unlinked.is_empty() {
                unindexed_kinds.insert(kind(label));
            }
            worst.push(format!(
                "{label} ({} dangling, {} unindexed)",
                lost.len(),
                unlinked.len()
            ));
        }
        if files.len() >= INDEX_LINES_WARN {
            large.push(format!("{label} ({} entries)", files.len()));
            large_kinds.insert(kind(label));
        }
    }

    let row = |status: Status, detail: String, hint: String| {
        let row = Row {
            name: NAME.to_owned(),
            status,
            detail,
            hint,
        };
        fsx::beside_unread(root, row, &unread)
    };
    if !refused.is_empty() {
        return row(
            Status::Warn,
            format!("{} index(es) charter will not touch", refused.len()),
            format!(
                "{}  → this is a defect in a committed file: replace the link with a real \
                 MEMORY.md",
                first(&refused, 2, "; ")
            ),
        );
    }
    if worst.is_empty() && large.is_empty() {
        return row(
            Status::Ok,
            format!("{} base(s) consistent", bases.len() - unread_bases),
            String::new(),
        );
    }
    let mut hint = first(&worst, 4, ", ");
    if unindexed > 0 {
        for k in &unindexed_kinds {
            hint.push_str(&format!(
                "  → charter {k} optimize --all --apply  (links unindexed files)"
            ));
        }
    }
    if dangling > 0 {
        hint.push_str(
            "  → a dangling link is proposal-only: prune it, or write the memory it names",
        );
    }
    if !large.is_empty() {
        if !hint.is_empty() {
            hint.push_str("  ");
        }
        hint.push_str("large: ");
        hint.push_str(&first(&large, 4, ", "));
        for k in &large_kinds {
            hint.push_str(&format!("  → charter {k} optimize <name>"));
        }
        hint.push_str("  (curate; growth is not a defect)");
    }
    if worst.is_empty() {
        return row(
            Status::Warn,
            format!("{} large index(es)", large.len()),
            hint,
        );
    }
    row(
        Status::Warn,
        format!("{dangling} dangling, {unindexed} unindexed"),
        hint,
    )
}
