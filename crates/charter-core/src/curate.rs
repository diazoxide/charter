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
    stale.sort_by_key(|s| std::cmp::Reverse(s.2));

    let (orphans, missing) = memstore::index_drift(root, dir)?;

    let mut rules: Vec<(String, String, usize)> = ents
        .iter()
        .map(|f| (name_of(f), f.title.clone(), rule_score(&f.text)))
        .filter(|r| r.2 >= 2)
        .collect();
    rules.sort_by_key(|r| std::cmp::Reverse(r.2));
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

/// One store an `optimize` curates: the name its heading shows, where it is, and — for a
/// persona's — the share of its memories that say they were checked.
pub struct Store {
    pub label: String,
    pub dir: std::path::PathBuf,
    pub verified_pct: Option<Option<i64>>,
}

/// How an `optimize` runs, and the words that differ between `workspace optimize` and
/// `persona optimize`.
pub struct Optimizing {
    /// Perform the safe ops, rather than name what they would be.
    pub apply: bool,
    /// Age in days past which a memory is proposed for archival.
    pub stale_days: i64,
    pub today: chrono::NaiveDate,
    /// The heading over the proposals.
    pub proposals: &'static str,
    /// Said when `--apply` found nothing safe to do.
    pub tidy: &'static str,
}

/// `workspace optimize` and `persona optimize`, over each of `stores` in order: the report,
/// then the safe ops with `apply` or what they would be without it, then the proposals.
/// `changed` is called once per store `--apply` changed. The exit code is 1 when a store
/// could not be looked at — the others are still curated, and that one is named.
pub fn optimize(
    root: &Path,
    stores: &[Store],
    how: &Optimizing,
    changed: &mut dyn FnMut(),
    say: crate::repocmd::Sink,
) -> u8 {
    use crate::repocmd::Say;
    let (apply, stale_days, today) = (how.apply, how.stale_days, how.today);
    let mut actions_total = 0;
    let mut unread: Unread = Vec::new();
    let missed = |missing: Unread, unread: &mut Unread, say: crate::repocmd::Sink| {
        for (path, code) in &missing {
            say(Say::Fail(memstore::cannot_check(root, path, *code)));
        }
        unread.extend(missing);
    };
    for store in stores {
        if !store.dir.exists() {
            continue;
        }
        let rep = match report(root, &store.dir, stale_days, 0.5, today) {
            Ok(rep) => rep,
            Err(missing) => {
                missed(missing, &mut unread, say);
                continue;
            }
        };
        if rep.total == 0 {
            continue;
        }
        let verified = match store.verified_pct {
            Some(pct) => format!("{}% verified · ", pct.unwrap_or(0)),
            None => String::new(),
        };
        say(Say::Out(format!(
            "\n◆ {}  ({} memories · {verified}{} exact-dup group(s) · {} near-dup pair(s) · {} \
             stale)",
            store.label,
            rep.total,
            rep.exact_dups.len(),
            rep.near_dups.len(),
            rep.stale.len()
        )));
        let rep = if apply {
            let actions = match apply_safe(root, &store.dir, today) {
                Ok(actions) => actions,
                Err(missing) => {
                    missed(missing, &mut unread, say);
                    continue;
                }
            };
            for action in &actions {
                say(Say::Done(format!("  auto: {action}")));
            }
            actions_total += actions.len();
            if !actions.is_empty() {
                changed();
            }
            match report(root, &store.dir, stale_days, 0.5, today) {
                Ok(rep) => rep,
                Err(missing) => {
                    missed(missing, &mut unread, say);
                    continue;
                }
            }
        } else {
            let pending = pending_auto(&rep);
            if !pending.is_empty() {
                say(Say::Out("  would auto-apply (re-run with --apply):".into()));
                for p in pending {
                    say(Say::Out(format!("    + {p}")));
                }
            }
            rep
        };
        let proposals = proposals(&rep);
        if !proposals.is_empty() {
            say(Say::Out(how.proposals.to_string()));
            for p in proposals {
                say(Say::Out(format!("    ? {p}")));
            }
        } else if apply {
            say(Say::Info("  clean — nothing to propose.".into()));
        }
    }
    if !apply {
        say(Say::Info(
            "\nRead-only. Re-run with --apply to auto-apply the safe/reversible ops (exact-dup \
             collapse + index repair); proposals always stay manual."
                .into(),
        ));
    } else if actions_total == 0 {
        say(Say::Info(how.tidy.to_string()));
    }
    if unread.is_empty() { 0 } else { 1 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn a_score_is_rounded_to_two_places_as_python_rounds_it() {
        assert_eq!(py_float(round2(2.0 / 3.0)), "0.67");
        assert_eq!(py_float(round2(1.0)), "1.0");
        assert_eq!(py_float(round2(0.5)), "0.5");
        // 0.125 is exact in binary, and Python's round goes to the even neighbour.
        assert_eq!(py_float(round2(0.125)), "0.12");
        assert_eq!(py_float(round2(0.375)), "0.38");
        // `str(float('inf'))` is `inf`, with nothing appended.
        assert_eq!(py_float(f64::INFINITY), "inf");
        assert_eq!(py_float(f64::NEG_INFINITY), "-inf");
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

    fn day(text: &str) -> chrono::NaiveDate {
        text.parse().unwrap()
    }

    /// A store with a bit of everything curation looks for, and the plane it sits in.
    ///
    /// Every expectation below it was read off Python's `curate.report` and
    /// `curate.apply_safe` run over the same files on the same day.
    fn store() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        std::fs::write(root.join("charter.toml"), "schema = 1\n").unwrap();
        let mem = root.join("personas/p/memory");
        std::fs::create_dir_all(&mem).unwrap();
        for (name, text) in [
            // `a` and `b` are one body under two titles; `c` is near both of them.
            (
                "20250601-a.md",
                "# Widget build\nThe widget build passes on every runner\n",
            ),
            (
                "20250601-b.md",
                "# Widget build copy\nThe widget build passes on every runner\n",
            ),
            (
                "20250601-c.md",
                "# Widget build\nThe widget build passes on every runner today\n",
            ),
            // Exactly ninety days old on the day asked, and a strong rule.
            (
                "d.md",
                "# Dated\n_2025-06-03 · x_\nA standing rule: never guess.\n",
            ),
            // A rule signal of exactly two, which is enough, and of one, which is not.
            ("e.md", "# Habit\nAlways run the suite first.\n"),
            ("f.md", "# Caution\nDo not panic.\n"),
        ] {
            std::fs::write(mem.join(name), text).unwrap();
        }
        std::fs::write(
            mem.join(memstore::INDEX),
            "# Memory Index\n\n- [Widget build](20250601-a.md)\n- [Gone](gone.md)\n",
        )
        .unwrap();
        (dir, root, mem)
    }

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn a_report_finds_every_kind_of_candidate_the_python_report_finds() {
        let (_d, root, mem) = store();

        let rep = report(&root, &mem, 90, 0.5, day("2025-09-01")).unwrap();

        assert_eq!(
            rep,
            Report {
                total: 6,
                exact_dups: vec![names(&["20250601-a.md", "20250601-b.md"])],
                // The exact pair is tier 1's, and not proposed again as a near one.
                near_dups: vec![
                    (0.83, "20250601-a.md".into(), "20250601-c.md".into()),
                    (0.71, "20250601-b.md".into(), "20250601-c.md".into()),
                ],
                // Ninety-two days is past ninety; ninety is not.
                stale: vec![
                    ("20250601-a.md".into(), "2025-06-01".into(), 92),
                    ("20250601-b.md".into(), "2025-06-01".into(), 92),
                    ("20250601-c.md".into(), "2025-06-01".into(), 92),
                ],
                orphans: names(&["gone.md"]),
                missing: names(&["20250601-b.md", "20250601-c.md", "d.md", "e.md", "f.md"]),
                rules: vec![
                    ("d.md".into(), "Dated".into(), 7),
                    ("e.md".into(), "Habit".into(), 2),
                ],
            }
        );
    }

    #[test]
    fn only_the_exact_pair_itself_is_left_out_of_the_near_duplicates() {
        // `b` is near both halves of the exact pair `c`/`d`. The pair tier 1 handles is
        // dropped from the proposals; a pair that merely shares a name with it is not. Read
        // off Python's `curate.report` over the same three files.
        let (_d, root, _) = store();
        let mem = root.join("personas/q/memory");
        std::fs::create_dir_all(&mem).unwrap();
        for (name, text) in [
            (
                "b.md",
                "# B\nThe widget build passes on every runner today\n",
            ),
            ("c.md", "# C\nThe widget build passes on every runner\n"),
            ("d.md", "# D\nThe widget build passes on every runner\n"),
        ] {
            std::fs::write(mem.join(name), text).unwrap();
        }

        let rep = report(&root, &mem, 90, 0.5, day("2025-09-01")).unwrap();

        assert_eq!(rep.exact_dups, vec![names(&["c.md", "d.md"])]);
        assert_eq!(
            rep.near_dups,
            vec![
                (0.83, "b.md".into(), "c.md".into()),
                (0.83, "b.md".into(), "d.md".into()),
            ]
        );
    }

    #[test]
    fn a_store_that_cannot_be_listed_is_named_rather_than_reported_empty() {
        let (_d, root, mem) = store();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&mem, std::fs::Permissions::from_mode(0o000)).unwrap();
            let listed = std::fs::read_dir(&mem).is_ok();
            let got = report(&root, &mem, 90, 0.5, day("2025-09-01"));
            let applied = apply_safe(&root, &mem, day("2025-09-01"));
            std::fs::set_permissions(&mem, std::fs::Permissions::from_mode(0o755)).unwrap();
            // Root reads through a mode of 000, and the question then does not arise.
            if !listed {
                assert_eq!(got.unwrap_err().len(), 1);
                assert_eq!(applied.unwrap_err().len(), 1);
            }
        }
        let _ = (root, mem);
    }

    #[test]
    fn applying_archives_the_redundant_copy_and_links_what_the_index_misses_once() {
        let (_d, root, mem) = store();

        assert_eq!(
            apply_safe(&root, &mem, day("2025-09-01")).unwrap(),
            [
                "archived exact-duplicate 20250601-b.md (kept 20250601-a.md)",
                "repaired index: linked 4 unindexed memory(ies)",
            ]
        );
        assert!(mem.join("archive/20250601-b.md").is_file());
        let index = std::fs::read_to_string(mem.join(memstore::INDEX)).unwrap();
        assert!(
            index.ends_with(
                "- [Widget build](20250601-c.md)\n- [Dated](d.md)\n- [Habit](e.md)\n\
                 - [Caution](f.md)\n"
            ),
            "{index}"
        );

        assert_eq!(
            apply_safe(&root, &mem, day("2025-09-01")).unwrap(),
            Vec::<String>::new(),
            "nothing left that is safe to do"
        );
    }

    #[cfg(unix)]
    #[test]
    fn an_index_that_leaves_the_plane_is_reported_and_never_appended_to() {
        let (_d, root, mem) = store();
        let outside = tempfile::tempdir().unwrap();
        let target = outside.path().join("elsewhere.md");
        std::fs::write(&target, "untouched\n").unwrap();
        std::fs::remove_file(mem.join(memstore::INDEX)).unwrap();
        std::os::unix::fs::symlink(&target, mem.join(memstore::INDEX)).unwrap();

        let actions = apply_safe(&root, &mem, day("2025-09-01")).unwrap();

        assert_eq!(actions.len(), 2, "{actions:?}");
        assert!(
            actions[1].starts_with("index NOT repaired — "),
            "{actions:?}"
        );
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "untouched\n");
    }

    #[test]
    fn what_apply_would_do_is_said_before_it_is_done() {
        let mut rep = Report {
            exact_dups: vec![names(&["a.md", "b.md", "c.md"]), names(&["d.md", "e.md"])],
            missing: names(&["m1.md", "m2.md", "m3.md"]),
            ..Report::default()
        };
        assert_eq!(
            pending_auto(&rep),
            [
                "collapse 2 exact-duplicate group(s) — archives 3 redundant copy(ies), reversible",
                "repair index: link 3 memory(ies) that exist but aren't listed \
                 (m1.md, m2.md, m3.md)",
            ]
        );
        rep.missing.push("m4.md".into());
        rep.exact_dups.clear();
        assert_eq!(
            pending_auto(&rep),
            [
                "repair index: link 4 memory(ies) that exist but aren't listed \
              (m1.md, m2.md, m3.md, …)"
            ]
        );
        assert_eq!(pending_auto(&Report::default()), Vec::<String>::new());
    }

    #[test]
    fn what_needs_a_person_is_proposed_in_pythons_words() {
        let long = "x".repeat(70);
        let rep = Report {
            near_dups: vec![
                (0.83, "a.md".into(), "c.md".into()),
                (1.0, "x.md".into(), "y.md".into()),
            ],
            stale: vec![
                ("old.md".into(), "2020-01-01".into(), 400),
                ("less.md".into(), "2021-01-01".into(), 300),
            ],
            rules: vec![("r.md".into(), long.clone(), 7)],
            orphans: names(&["g1.md", "g2.md", "g3.md", "g4.md", "g5.md", "g6.md"]),
            ..Report::default()
        };
        assert_eq!(
            proposals(&rep),
            [
                "merge near-duplicates (0.83): a.md + c.md → one canonical memory?".to_string(),
                "merge near-duplicates (1.0): x.md + y.md → one canonical memory?".to_string(),
                "archive 2 stale memory(ies) (age-based, oldest 400d — review first: age ≠ \
                 obsolete)?"
                    .to_string(),
                format!(
                    "promote to charter? [r.md] \"{}\" (rule-signal 7)",
                    &long[..60]
                ),
                "index lists 6 missing file(s) — stale links to prune: g1.md, g2.md, g3.md, \
                 g4.md, g5.md"
                    .to_string(),
            ]
        );
        assert_eq!(proposals(&Report::default()), Vec::<String>::new());
    }
}
