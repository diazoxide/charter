//! **Every store `docs/plane-format.md` names says which storage tier it is in** (ADR 0069).
//!
//! There are four tiers: **Plane** (committed), **Clone state** (`.charter/` and the other
//! per-clone files git does not carry), **Machine** (outside every plane, each store either
//! `syncable` or `device-bound`) and **Keyring**. A store's tier decides whether FR-10's backup
//! carries it, whether a second machine may receive it, and whether deleting it costs anything
//! but a rebuild. A store that lands without one has had none of those questions asked, so this
//! test fails until it has.
//!
//! What it reads, and why that is enough:
//!
//! - **Every `###` and `####` heading that names a path in backticks** between "Finding the
//!   plane" and the appendix is a store, and its section carries a `**Tier:**` line. That is the
//!   document's own convention: every file it records has a heading of that shape. The few
//!   backticked headings that name a key, a group or a rule instead are listed in [`NOT_STORES`]
//!   with the reason, so adding one is a decision somebody makes in a diff.
//! - **Every table whose first column is `Path`** has a `Tier` column, because a table of paths
//!   is a list of stores (`workspace-rename.json` is recorded only in one).
//! - **Every tier said anywhere is a tier the ADR defines**, spelled as [`check`] reads it.

use std::path::Path;

/// Backticked headings that do not name a store, each with the reason.
const NOT_STORES: &[(&str, &str)] = &[
    (
        "`[frame]`",
        "keys inside `charter.toml`, whose tier is its own",
    ),
    ("`[[frame.component]]`", "a table inside `charter.toml`"),
    (
        "`settings` — a workspace's layer",
        "a key inside `workspace.json`",
    ),
    (
        "`[memory] share`",
        "a setting, and how it moves persona files between tiers",
    ),
    (
        "Generated harness layer in `workspaces/<ws>/`",
        "a group heading: each file under it carries its own tier",
    ),
    (
        "`.charter/…` — active-workspace pointers",
        "its table's rows carry their tiers",
    ),
];

/// The first word of a tier line: the tier, or `None` for a path charter records but does not
/// own (a harness's own file, the operator's checkout).
const TIERS: &[&str] = &["Plane", "Clone state", "Machine", "Keyring", "None"];

/// What may follow the tier, comma-separated.
const MARKS: &[&str] = &[
    "syncable",
    "device-bound",
    "rebuildable",
    "transient",
    "legacy",
    "Clone state when LOCAL",
];

/// The problem with one tier, or `None` when it reads. `said` is everything after `**Tier:**`,
/// up to the dash that starts the reason.
fn check(said: &str) -> Option<String> {
    let value = said
        .split(" — ")
        .next()
        .unwrap_or_default()
        .trim()
        .trim_end_matches('.');
    let mut parts = value.split(", ").map(str::trim);
    let tier = parts.next().unwrap_or_default();
    if !TIERS.contains(&tier) {
        return Some(format!("`{tier}` is not a tier (one of {TIERS:?})"));
    }
    let marks: Vec<&str> = parts.collect();
    if let Some(odd) = marks.iter().find(|mark| !MARKS.contains(mark)) {
        return Some(format!("`{odd}` is not a mark (one of {MARKS:?})"));
    }
    let placed = marks
        .iter()
        .filter(|mark| ["syncable", "device-bound"].contains(mark))
        .count();
    match tier {
        "Machine" if placed != 1 => {
            Some("a Machine store is exactly one of `syncable` or `device-bound`".into())
        }
        "Machine" => None,
        "None" | "Keyring" if !marks.is_empty() => Some(format!("`{tier}` takes no marks")),
        _ if placed > 0 => Some("only a Machine store is `syncable` or `device-bound`".into()),
        _ if marks.contains(&"Clone state when LOCAL") && tier != "Plane" => {
            Some("only a Plane store can be Clone state when its workspace is LOCAL".into())
        }
        _ => None,
    }
}

/// One `**Tier:**` value on a line, if the line has one.
fn tier_on(line: &str) -> Option<&str> {
    line.split_once("**Tier:**").map(|(_, rest)| rest.trim())
}

fn cells(row: &str) -> Vec<&str> {
    row.trim()
        .trim_matches('|')
        .split('|')
        .map(str::trim)
        .collect()
}

/// Every problem in `text`, one sentence each.
fn problems(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut in_scope = false;
    let mut fenced = false;
    // The heading being read, if it names a store and no tier has been seen under it yet.
    let mut owed: Option<(usize, String)> = None;
    // The column that holds the tier in the table being read, if one is.
    let mut table: Option<Option<usize>> = None;
    let lines: Vec<&str> = text.lines().collect();

    for (at, line) in lines.iter().enumerate() {
        let number = at + 1;
        if line.starts_with("```") {
            fenced = !fenced;
            continue;
        }
        if fenced {
            continue;
        }
        if line.starts_with("## ") {
            if let Some((heading_at, heading)) = owed.take() {
                found.push(format!(
                    "line {heading_at}: {heading} has no **Tier:** line"
                ));
            }
            in_scope = line.starts_with("## Finding the plane")
                || (in_scope && !line.starts_with("## Appendix"));
            continue;
        }
        if !in_scope {
            continue;
        }
        if line.starts_with("### ") || line.starts_with("#### ") {
            if let Some((heading_at, heading)) = owed.take() {
                found.push(format!(
                    "line {heading_at}: {heading} has no **Tier:** line"
                ));
            }
            let heading = line.trim_start_matches('#').trim();
            let names_a_path = heading.contains('`');
            let exempt = NOT_STORES
                .iter()
                .any(|(start, _)| heading.starts_with(start));
            if names_a_path && !exempt {
                owed = Some((number, heading.to_string()));
            }
            continue;
        }
        if line.trim_start().starts_with('|') {
            let row = cells(line);
            match table {
                None => {
                    let tier_column = row.iter().position(|cell| *cell == "Tier");
                    if row.first() == Some(&"Path") && tier_column.is_none() {
                        found.push(format!(
                            "line {number}: a table of paths has no Tier column"
                        ));
                    }
                    table = Some(tier_column);
                }
                Some(Some(column)) if !row.iter().all(|cell| cell.starts_with("---")) => {
                    let said = row.get(column).copied().unwrap_or_default();
                    if let Some(problem) = check(said) {
                        found.push(format!("line {number}: {problem}"));
                    }
                }
                Some(_) => {}
            }
            continue;
        }
        table = None;
        if let Some(said) = tier_on(line) {
            owed = None;
            if let Some(problem) = check(said) {
                found.push(format!("line {number}: {problem}"));
            }
        }
    }
    if let Some((heading_at, heading)) = owed {
        found.push(format!(
            "line {heading_at}: {heading} has no **Tier:** line"
        ));
    }
    found
}

#[test]
fn every_store_the_plane_format_names_says_its_tier() {
    charter_core::unsteered!();
    let doc = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/plane-format.md");
    let text = std::fs::read_to_string(&doc).expect("docs/plane-format.md is readable");
    let found = problems(&text);
    assert!(
        found.is_empty(),
        "docs/plane-format.md names a store without a tier it can read (ADR 0069):\n{}",
        found.join("\n")
    );
    let tiers = text.matches("**Tier:**").count();
    assert!(
        tiers > 100,
        "only {tiers} tier lines: the walk read the wrong file"
    );
}

#[test]
fn a_new_file_without_a_tier_is_named() {
    charter_core::unsteered!();
    let text = "## Finding the plane\n\n### `a.json`\n\n- **Tier:** Plane\n\n\
                ### `b.json`\n\n- **Status:** stable\n\n## Appendix\n";
    let found = problems(text);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("`b.json`"), "{found:?}");
}

#[test]
fn a_table_of_paths_carries_a_tier_per_row() {
    charter_core::unsteered!();
    let without = "## Finding the plane\n\n| Path | What |\n|---|---|\n| `x` | y |\n";
    assert_eq!(problems(without).len(), 1);
    let with = "## Finding the plane\n\n| Path | Tier |\n|---|---|\n| `x` | Machine |\n";
    let found = problems(with);
    assert!(
        found.len() == 1 && found[0].contains("syncable"),
        "{found:?}"
    );
}

#[test]
fn a_tier_is_one_the_adr_defines() {
    charter_core::unsteered!();
    assert_eq!(check("Plane — committed"), None);
    assert_eq!(
        check("Plane, Clone state when LOCAL — the LIVE block"),
        None
    );
    assert_eq!(check("Clone state, rebuildable"), None);
    assert_eq!(check("Machine, device-bound, rebuildable"), None);
    assert_eq!(check("Keyring"), None);
    assert!(check("App data").is_some());
    assert!(check("Machine").is_some());
    assert!(check("Machine, syncable, device-bound").is_some());
    assert!(check("Clone state, syncable").is_some());
    assert!(check("Keyring, rebuildable").is_some());
    assert!(check("Clone state, Clone state when LOCAL").is_some());
    assert!(check("Clone state, cached").is_some());
}
