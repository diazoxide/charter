//! The generated persona-roster block inside the plane's own README — who exists, and who
//! work is actually routed to. A port of `charter/render.py`'s `personas_md` and
//! `splice_personas`.
//!
//! **Every column comes from COMMITTED state** — the persona definitions, their memory
//! files, the dispatch log. Not the dispatch records a machine keeps since a persona became a
//! chat of its own (#1452, #1460): those are this machine's, and `persona stats` reads them. Vault health is deliberately absent: vaults are gitignored and
//! per-engineer, so a column drawn from one would churn the README and conflict between
//! checkouts.
//!
//! **The plane's README is an operator's file.** charter owns the bytes between the two
//! markers and nothing else: [`splice`] replaces that span and returns `None` when either
//! marker is missing, so a hand-written README is never appended to by surprise. That is
//! the whole contract, and `docs generate` is tested against it.

use std::collections::BTreeMap;
use std::path::Path;

use crate::memstore;

/// The markers delimiting the generated block. `render.PERSONAS_BEGIN` / `PERSONAS_END`.
///
/// The begin marker is [`crate::names::PERSONAS_BEGIN`]: either spelling is found, and the
/// splice replaces the block, begin marker and all, with one under the spelling the plane
/// writes — charter's until the plane is migrated, since the README is committed (V93g, V93i).
/// `END` names no product and never changed.
pub const END: &str = "<!-- END personas -->";

/// How wide the dispatch bar is drawn. `render._bar`'s `width`.
const BAR: u64 = 12;

/// One roster row, before it is sorted and rendered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub name: String,
    /// Times work was dispatched to this persona, as the committed dispatch log counts it.
    pub dispatches: u64,
    /// How many memory files it holds.
    pub memories: usize,
    /// `tools:` from its frontmatter, stripped.
    pub tools: String,
    /// `activity:` from its frontmatter, stripped — the fallback when it declares no tools.
    pub activity: String,
}

/// A memory directory the roster could not list, with the errno.
///
/// `len(persona.memories(n))` raises rather than answering zero (charter #1084), and a
/// count charter did not take has no business being written into a committed file. The
/// caller names these and leaves the README alone.
pub type Unread = memstore::Unread;

/// The roster's rows, or the FIRST memory store that stopped them being taken.
///
/// Both halves of the answer, because the caller does two different things with them: the
/// rows render, and an unread store is named and the write abandoned.
///
/// It stops at the first such store rather than surveying every persona, because Python
/// does: `len(persona.memories(n))` raises out of the loop, so the sentence an operator
/// reads names one directory. A second one they cannot see yet is a second run away, and a
/// list of them is not what either charter prints.
pub fn rows(root: &Path, names: &[String], counts: &BTreeMap<String, u64>) -> (Vec<Row>, Unread) {
    let mut rows = Vec::new();
    let mut unread = Unread::new();
    for name in names {
        if !unread.is_empty() {
            break;
        }
        let meta: BTreeMap<String, String> = crate::personas::load(root, name)
            .unwrap_or_default()
            .into_iter()
            // `dict(pairs)`: a key written twice keeps the LAST value.
            .collect();
        let (files, could_not) = memstore::read_files(root, &memory_dir(root, name));
        unread.extend(could_not);
        rows.push(Row {
            name: name.clone(),
            dispatches: counts.get(name).copied().unwrap_or(0),
            memories: files.len(),
            tools: field(&meta, "tools"),
            activity: field(&meta, "activity"),
        });
    }
    // Most-dispatched first, ties by name — `rows.sort(key=lambda r: (-r["disp"], r["name"]))`.
    rows.sort_by(|a, b| b.dispatches.cmp(&a.dispatches).then(a.name.cmp(&b.name)));
    (rows, unread)
}

fn memory_dir(root: &Path, name: &str) -> std::path::PathBuf {
    root.join("personas").join(name).join("memory")
}

fn field(meta: &BTreeMap<String, String>, key: &str) -> String {
    memstore::py_strip(meta.get(key).map(String::as_str).unwrap_or_default()).to_string()
}

/// Python's `round()` — half to EVEN, not half away from zero.
///
/// The bar is the one arithmetic in this file, and the two rules disagree on exactly the
/// value a twelve-cell bar hits often: `round(6.5)` is 6 in Python and 7 in Rust. A README
/// is a committed file, so one cell of difference is a diff on every plane that regenerates
/// it with the other charter.
fn round_half_even(x: f64) -> u64 {
    let down = x.floor();
    let frac = x - down;
    // Exactly a half goes to whichever of the two neighbours is even; everything else goes
    // to the nearer one.
    let up = frac > 0.5 || (frac == 0.5 && (down as i64) % 2 != 0);
    let rounded = if up { down + 1.0 } else { down };
    rounded.max(0.0) as u64
}

/// The unicode bar for one row. `render._bar`.
///
/// Rendered from committed counts so every engineer sees the same one. A non-zero count
/// always fills at least one cell — a persona that was dispatched once must not draw as a
/// persona that never was, which is the distinction the ⚑ below is about.
pub fn bar(n: u64, peak: u64) -> String {
    if peak == 0 {
        return "░".repeat(BAR as usize);
    }
    let filled = if n == 0 {
        0
    } else {
        round_half_even(BAR as f64 * n as f64 / peak as f64).max(1)
    };
    let filled = filled.min(BAR) as usize;
    format!(
        "{}{}",
        "█".repeat(filled),
        "░".repeat(BAR as usize - filled)
    )
}

/// The whole generated block, markers included. `render.personas_md`, reworded (#1460).
///
/// **The committed log is all it counts.** A persona's chats since it stopped being a
/// sub-agent are counted from each machine's own dispatch records, which are never committed
/// and kept 30 days ([`crate::dispatchrecord::tally`]); counting them here would make this
/// committed block depend on which machine regenerated it last, and conflict between
/// checkouts. So the headline says what the log holds and sends a reader to `persona stats`
/// for the rest, and the share of "generic agents" the Python drew is gone: a helper is no
/// dispatch now, and the log stopped receiving those rows too.
pub fn block(plane: &Path, rows: &[Row]) -> String {
    let peak = rows.iter().map(|r| r.dispatches).max().unwrap_or(0);
    let total: u64 = rows.iter().map(|r| r.dispatches).sum();
    // The block is committed: it names the program as the plane spells it (D-RN11a-1).
    let program = crate::names::BINARY.writes_for(plane);
    let mut out: Vec<String> = vec![
        crate::names::PERSONAS_BEGIN.writes_for(plane).to_string(),
        String::new(),
        "## Personas — roster & routing health".to_string(),
        String::new(),
    ];
    let since = format!(
        "Dispatches since are counted by `{program} persona stats`, from each machine's own \
         records, which are never committed."
    );
    if total == 0 {
        out.push(format!(
            "_The committed dispatch log holds no dispatch to a persona._ {since}"
        ));
    } else {
        let dispatches = if total == 1 { "dispatch" } else { "dispatches" };
        out.push(format!(
            "**The committed dispatch log holds {total} {dispatches} to a persona**, all from \
             before a persona ran as its own chat. {since}"
        ));
    }
    out.push(String::new());
    out.push("| Persona | Dispatches | | Memory | Capability |".to_string());
    out.push("| --- | ---: | --- | ---: | --- |".to_string());
    let mut flagged = false;
    for r in rows {
        let cap = if !r.tools.is_empty() {
            format!("`{}`", r.tools)
        } else if !r.activity.is_empty() {
            format!("_{}_", r.activity)
        } else {
            "—".to_string()
        };
        // Only once there is a tally to be absent from: on a plane with no dispatches at
        // all, every persona would carry the mark and it would mean nothing.
        let flag = if r.dispatches == 0 && total > 0 {
            flagged = true;
            " ⚑"
        } else {
            ""
        };
        out.push(format!(
            "| `{}`{flag} | {} | `{}` | {} | {cap} |",
            r.name,
            r.dispatches,
            bar(r.dispatches, peak),
            r.memories
        ));
    }
    out.push(String::new());
    if flagged {
        out.push(
            "⚑ = **not in the log**: the committed log holds no dispatch to it. A persona \
             earns its place by carrying a capability a general-purpose chat can't have — a \
             credential/tool, or a domain narrow enough to name."
                .to_string(),
        );
        out.push(String::new());
    }
    out.push(format!(
        "Regenerate with `{program} docs generate`. Detail: `{program} persona stats` · \
         `docs/personas.md`."
    ));
    out.push(String::new());
    out.push(END.to_string());
    out.join("\n")
}

/// `readme` with the marked block replaced, or `None` when the markers are not both there
/// in that order. `render.splice_personas`.
///
/// The FIRST of each marker, as Python's `str.find` takes them, and the span between them
/// is the only thing that changes: everything before `BEGIN` and everything after `END` is
/// handed back byte for byte. A README that carries no block is left entirely alone —
/// appending one would be charter writing into a file it was not invited into.
pub fn splice(readme: &str, block: &str) -> Option<String> {
    let (i, j) = span(readme)?;
    Some(format!(
        "{}{block}{}",
        &readme[..i],
        &readme[j + END.len()..]
    ))
}

/// Whether `readme` carries a block at all — the question `splice` asks FIRST.
///
/// Separate because the order is behaviour: Python's `splice_personas` returns before it
/// calls `personas_md`, so a plane with no block never reads a persona, never counts a
/// memory, and cannot be stopped by a memory store it could not list.
pub fn has_block(readme: &str) -> bool {
    span(readme).is_some()
}

/// `(begin, end)` byte offsets of the block, or `None` — the FIRST of each marker, as
/// Python's `str.find` takes them.
fn span(readme: &str) -> Option<(usize, usize)> {
    let i = crate::names::PERSONAS_BEGIN
        .spellings()
        .filter_map(|marker| readme.find(marker))
        .min()?;
    let j = readme.find(END)?;
    (j >= i).then_some((i, j))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(name: &str, dispatches: u64) -> Row {
        Row {
            name: name.into(),
            dispatches,
            memories: 0,
            tools: String::new(),
            activity: String::new(),
        }
    }

    #[test]
    fn a_bar_is_filled_from_the_peak_and_rounds_the_way_python_rounds() {
        assert_eq!(bar(0, 0), "░░░░░░░░░░░░", "no dispatches anywhere");
        assert_eq!(bar(4, 4), "████████████");
        assert_eq!(bar(0, 4), "░░░░░░░░░░░░", "never dispatched draws empty");
        // One dispatch out of forty is 0.3 cells; it still draws one, because "dispatched
        // once" and "never dispatched" are the two states this column exists to separate.
        assert_eq!(bar(1, 40), "█░░░░░░░░░░░");
        // 12 * 13 / 24 = 6.5 — Python's `round` gives 6, Rust's `f64::round` gives 7.
        assert_eq!(bar(13, 24), "██████░░░░░░");
    }

    #[test]
    fn the_headline_says_what_the_committed_log_counts_and_where_the_rest_is_counted() {
        // #1460: the log stopped moving when a persona stopped being a sub-agent, so the block
        // says what it holds, and never a share of "generic agents" or "sub-agents".
        let rows = [row("devops", 2), row("steward", 0)];
        let block = block(Path::new("/nonexistent-plane"), &rows);

        assert!(
            block.contains(
                "**The committed dispatch log holds 2 dispatches to a persona**, all from before \
                 a persona ran as its own chat."
            ),
            "{block}"
        );
        assert!(block.contains("`charter persona stats`"), "{block}");
        let one = super::block(Path::new("/nonexistent-plane"), &[row("devops", 1)]);
        assert!(one.contains("holds 1 dispatch to a persona**"), "{one}");
        assert!(block.contains("| `steward` ⚑ | 0 |"), "{block}");
        assert!(block.contains("⚑ = **not in the log**"), "{block}");
        for retired in ["generic", "sub-agent", "```mermaid"] {
            assert!(!block.contains(retired), "{retired}: {block}");
        }
    }

    #[test]
    fn a_plane_that_has_dispatched_nothing_flags_nobody() {
        // Every persona is at zero, so the mark would be on every row and would say nothing
        // about any of them.
        let block = block(
            Path::new("/nonexistent-plane"),
            &[row("devops", 0), row("steward", 0)],
        );

        assert!(
            block.contains("_The committed dispatch log holds no dispatch to a persona._"),
            "{block}"
        );
        assert!(!block.contains('⚑'), "{block}");
        assert!(!block.contains("```mermaid"), "{block}");
    }

    #[test]
    fn a_capability_is_the_tools_then_the_activity_then_a_dash() {
        let mut with_tools = row("a", 0);
        with_tools.tools = "Bash, Read".into();
        let mut with_activity = row("b", 0);
        with_activity.activity = "ships releases".into();
        let block = block(
            Path::new("/nonexistent-plane"),
            &[with_tools, with_activity, row("c", 0)],
        );

        assert!(block.contains("| `Bash, Read` |"), "{block}");
        assert!(block.contains("| _ships releases_ |"), "{block}");
        assert!(block.contains("| — |"), "{block}");
    }

    const BEGIN: &str = crate::names::PERSONAS_BEGIN.write;

    #[test]
    fn only_the_span_between_the_markers_is_ever_rewritten() {
        let readme = format!("# My plane\n\nMine.\n\n{BEGIN}\nOLD\n{END}\n\n## Also mine\n");
        let spliced = splice(&readme, "NEW").expect("both markers, in order");

        assert_eq!(spliced, "# My plane\n\nMine.\n\nNEW\n\n## Also mine\n");
    }

    #[test]
    fn a_block_charter_opened_is_found_and_rewritten_in_place_under_the_purlis_marker() {
        let old = "<!-- BEGIN personas — GENERATED by `charter docs`; do not edit by hand. -->";
        let readme = format!("# My plane\n\n{old}\nOLD\n{END}\n\n## Also mine\n");
        assert!(has_block(&readme));
        let block = format!("{BEGIN}\nNEW\n{END}");
        let spliced = splice(&readme, &block).expect("the old block is recognised");
        assert_eq!(spliced, format!("# My plane\n\n{block}\n\n## Also mine\n"));
        assert!(!spliced.contains("charter docs"), "{spliced}");
        assert_eq!(spliced.matches("<!-- BEGIN personas").count(), 1);
        assert!(has_block(&spliced), "the purlis block is recognised");
        assert!(BEGIN.contains("`purlis docs`"), "{BEGIN}");

        let unmigrated = super::block(Path::new("/nonexistent-plane"), &[]);
        assert!(unmigrated.starts_with(old), "{unmigrated}");
    }

    #[test]
    fn a_readme_without_the_block_is_left_exactly_as_it_is() {
        assert_eq!(splice("# My plane\n", "NEW"), None);
        assert_eq!(splice(&format!("{BEGIN}\n"), "NEW"), None, "no end marker");
        assert_eq!(splice(&format!("{END}\n"), "NEW"), None, "no begin marker");
        // End before begin is not a block either; splicing it would delete the file's middle.
        assert_eq!(splice(&format!("{END}\nx\n{BEGIN}\n"), "NEW"), None);
    }

    #[test]
    fn a_rows_count_comes_from_the_plane_and_a_store_it_cannot_list_is_named() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(root.join("charter.toml"), "").unwrap();
        let mem = root.join("personas/devops/memory");
        std::fs::create_dir_all(&mem).unwrap();
        std::fs::write(mem.join("MEMORY.md"), "# index\n").unwrap();
        std::fs::write(mem.join("a-fact.md"), "# a fact\n").unwrap();
        std::fs::write(
            root.join("personas/devops/persona.md"),
            "---\nname: devops\ntools: Bash\n---\n\n# DevOps\n",
        )
        .unwrap();

        let mut counts = BTreeMap::new();
        counts.insert("devops".to_string(), 3);
        let (rows, unread) = rows(root, &["devops".to_string()], &counts);

        assert!(unread.is_empty());
        assert_eq!(rows[0].memories, 1, "the index is not a memory");
        assert_eq!(rows[0].dispatches, 3);
        assert_eq!(rows[0].tools, "Bash");
    }
}
