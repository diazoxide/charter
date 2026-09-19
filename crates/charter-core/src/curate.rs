//! Memory curation — `charter/curate.py`: find what a store could lose, and apply only
//! what can be undone.
//!
//! Two tiers, and the line between them is the point. `--apply` performs only the SAFE and
//! REVERSIBLE: an exact duplicate is moved to `archive/` (not deleted), and a file the index
//! does not list is appended to it. Everything lossy or human-owned — merging near
//! duplicates, archiving by age, promoting a rule to the charter — is a PROPOSAL, printed
//! and never performed.
//!
//! Every write goes through `memstore`, whose gates are what keep an `--apply` inside the
//! store: a linked `archive/` refuses the move, a linked index refuses the append, and a
//! file charter will not read is not a memory to act on.

use std::path::Path;

use crate::memstore::{self, Found, Unread};

/// Durable-rule language worth promoting to a charter, and its weight — `curate._RULE_MARKERS`.
const RULE_MARKERS: [(&str, usize); 8] = [
    ("standing rule", 5),
    ("invariant", 3),
    ("the rule is", 3),
    ("must not", 2),
    ("always ", 2),
    ("never ", 2),
    (" must ", 1),
    ("do not ", 1),
];

/// Snapshot language: a memory saying any of these is never nominated.
const TRANSIENT: [&str; 5] = [
    "deployed state",
    " as of ",
    "deploy of ",
    "verified live",
    "re-verif",
];

fn rule_score(text: &str) -> usize {
    let low = text.to_lowercase();
    if TRANSIENT.iter().any(|t| low.contains(t)) {
        return 0;
    }
    RULE_MARKERS
        .iter()
        .map(|(marker, weight)| weight * low.matches(marker).count())
        .sum()
}

/// One store's findings — `curate.report`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Report {
    pub total: usize,
    /// Groups of filenames with one body between them, each sorted, the groups sorted.
    pub exact_dups: Vec<Vec<String>>,
    /// `(score rounded to two places, a, b)`.
    pub near_dups: Vec<(f64, String, String)>,
    /// `(filename, date, age in days)`, oldest first.
    pub stale: Vec<(String, String, i64)>,
    /// Indexed with no file behind it.
    pub orphans: Vec<String>,
    /// A file no index line names.
    pub missing: Vec<String>,
    /// `(filename, title, score)`, strongest first, at most ten.
    pub rules: Vec<(String, String, usize)>,
}

fn name_of(found: &Found) -> String {
    found
        .path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

/// `round(x, 2)` as Python rounds: to the nearest representable two-place value, ties to
/// even on the exact binary value — which is what formatting to two places gives.
pub fn round2(x: f64) -> f64 {
    format!("{x:.2}").parse().unwrap_or(x)
}

/// A float as Python's `str()` writes it: the shortest repr, and `1.0` rather than `1`.
pub fn py_float(x: f64) -> String {
    let s = format!("{x}");
    if s.contains('.') || s.contains('e') || s.contains("inf") || s.contains("NaN") {
        s
    } else {
        format!("{s}.0")
    }
}

/// What curating `dir` would find. `Err` with what could not be looked at when the store
/// itself could not be listed — charter names it and curates the next one (#1084).
pub fn report(
    root: &Path,
    dir: &Path,
    stale_days: i64,
    near_threshold: f64,
    today: chrono::NaiveDate,
) -> Result<Report, Unread> {
    let (ents, unread) = memstore::read_entries(root, dir);
    if !unread.is_empty() {
        return Err(unread);
    }

    // Exact duplicates: one normalised body, grouped in listing order.
    let mut by_body: Vec<(String, Vec<String>)> = Vec::new();
    for found in &ents {
        let key = memstore::body(&found.text);
        match by_body.iter_mut().find(|(k, _)| *k == key) {
            Some((_, names)) => names.push(name_of(found)),
            None => by_body.push((key, vec![name_of(found)])),
        }
    }
    let mut exact: Vec<Vec<String>> = by_body
        .into_iter()
        .filter(|(_, names)| names.len() > 1)
        .map(|(_, mut names)| {
            names.sort();
            names
        })
        .collect();
    exact.sort();

    // Near duplicates, less the pairs tier 1 already handles — the FIRST two of each exact
    // group, which is the pair charter excludes.
    let exact_pairs: Vec<[&String; 2]> = exact.iter().map(|g| [&g[0], &g[1]]).collect();
    let mut near = Vec::new();
    for (score, i, j) in memstore::duplicates(&ents, near_threshold) {
        let (a, b) = (name_of(&ents[i]), name_of(&ents[j]));
        let excluded = exact_pairs
            .iter()
            .any(|[x, y]| (**x == a && **y == b) || (**x == b && **y == a));
        if !excluded {
            near.push((round2(score), a, b));
        }
    }

    // Stale — a proposal, never automatic: age is not obsolescence.
    let mut stale = Vec::new();
    for found in &ents {
        if let Some(date) = memstore::memory_date(&found.text, &name_of(found)) {
            let age = (today - date).num_days();
            if age > stale_days {
                stale.push((name_of(found), date.format("%Y-%m-%d").to_string(), age));
            }
        }
    }
    stale.sort_by(|a, b| b.2.cmp(&a.2));

    let (orphans, missing) = memstore::index_drift(root, dir)?;

    let mut rules: Vec<(String, String, usize)> = ents
        .iter()
        .map(|f| (name_of(f), f.title.clone(), rule_score(&f.text)))
        .filter(|r| r.2 >= 2)
        .collect();
    rules.sort_by(|a, b| b.2.cmp(&a.2));
    rules.truncate(10);

    Ok(Report {
        total: ents.len(),
        exact_dups: exact,
        near_dups: near,
        stale,
        orphans,
        missing,
        rules,
    })
}

/// Apply ONLY the safe, reversible ops and say what was done — `curate.apply_safe`.
///
/// Each exact-duplicate group keeps its first file and archives the rest; then every file
/// the index does not list is linked. A refused index is reported into the log rather than
/// raised out of a half-finished batch — and reported, not swallowed, because a linked index
/// would turn this into N appends into whatever it points at.
pub fn apply_safe(
    root: &Path,
    dir: &Path,
    today: chrono::NaiveDate,
) -> Result<Vec<String>, Unread> {
    let mut actions = Vec::new();
    let rep = report(root, dir, 90, 0.5, today)?;
    for group in &rep.exact_dups {
        for dup in &group[1..] {
            if memstore::archive(root, dir, dup).is_some() {
                actions.push(format!(
                    "archived exact-duplicate {dup} (kept {})",
                    group[0]
                ));
            }
        }
    }
    let missing = report(root, dir, 90, 0.5, today)?.missing;
    if !missing.is_empty() {
        let index = dir.join(memstore::INDEX);
        let (ents, _) = memstore::read_entries(root, dir);
        let mut refused = None;
        for found in &ents {
            if missing.contains(&name_of(found))
                && let Err(e) = memstore::index_append(root, &index, &name_of(found), &found.title)
            {
                refused = Some(e);
                break;
            }
        }
        actions.push(match refused {
            None => format!(
                "repaired index: linked {} unindexed memory(ies)",
                missing.len()
            ),
            Some(e) => format!("index NOT repaired — {e}"),
        });
    }
    Ok(actions)
}

/// What [`apply_safe`] WOULD do, for the read-only report. A read-only report that hides a
/// change the tool will make is worse than none: every branch mirrors one above.
pub fn pending_auto(rep: &Report) -> Vec<String> {
    let mut out = Vec::new();
    if !rep.exact_dups.is_empty() {
        let redundant: usize = rep.exact_dups.iter().map(|g| g.len() - 1).sum();
        out.push(format!(
            "collapse {} exact-duplicate group(s) — archives {redundant} redundant copy(ies), reversible",
            rep.exact_dups.len()
        ));
    }
    if !rep.missing.is_empty() {
        let mut shown = rep
            .missing
            .iter()
            .take(3)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        if rep.missing.len() > 3 {
            shown.push_str(", …");
        }
        out.push(format!(
            "repair index: link {} memory(ies) that exist but aren't listed ({shown})",
            rep.missing.len()
        ));
    }
    out
}

/// The tier-2 proposals, for a person to decide — `curate.proposals`.
pub fn proposals(rep: &Report) -> Vec<String> {
    let mut out = Vec::new();
    for (score, a, b) in &rep.near_dups {
        out.push(format!(
            "merge near-duplicates ({}): {a} + {b} → one canonical memory?",
            py_float(*score)
        ));
    }
    if let Some(oldest) = rep.stale.first() {
        out.push(format!(
            "archive {} stale memory(ies) (age-based, oldest {}d — review first: age ≠ obsolete)?",
            rep.stale.len(),
            oldest.2
        ));
    }
    for (name, title, score) in &rep.rules {
        let cut: String = title.chars().take(60).collect();
        out.push(format!(
            "promote to charter? [{name}] \"{cut}\" (rule-signal {score})"
        ));
    }
    if !rep.orphans.is_empty() {
        let names = rep
            .orphans
            .iter()
            .take(5)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        out.push(format!(
            "index lists {} missing file(s) — stale links to prune: {names}",
            rep.orphans.len()
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_score_is_rounded_to_two_places_as_python_rounds_it() {
        assert_eq!(py_float(round2(2.0 / 3.0)), "0.67");
        assert_eq!(py_float(round2(1.0)), "1.0");
        assert_eq!(py_float(round2(0.5)), "0.5");
        // 0.125 is exact in binary, and Python's round goes to the even neighbour.
        assert_eq!(py_float(round2(0.125)), "0.12");
        assert_eq!(py_float(round2(0.375)), "0.38");
    }

    #[test]
    fn a_snapshot_is_never_a_rule_however_imperative_it_sounds() {
        assert_eq!(
            rule_score("Standing rule: never deploy. Deployed state is fine."),
            0
        );
        assert_eq!(
            rule_score("This is a standing rule. You must not deploy."),
            8
        );
    }
}
