//! `ask rules` and `handoff gate`: the plane's force-prompt rules, read from the harness's own
//! files — the ones `charter guard` writes (#364).
//!
//! Both read the settings of the directory the doctor runs in, because Claude Code reads a
//! session's `.claude/settings.json` from the session's own directory and does not walk up
//! (`session root` says which directory that is). opencode's rule lives in the plane's
//! `opencode.json`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use super::{Doctor, Row, canonical, memory};
use crate::guardcmd;
use crate::scaffold::settings;

/// A settings file the harness would read, as charter reads it: `Ok(None)` when it is not
/// there, `Err` when it is there and is not a JSON object.
fn object(path: &Path) -> Result<Option<serde_json::Value>, String> {
    let raw = match std::fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            return Err(format!(
                "{} could not be read ({e})",
                super::fsx::path_field(path)
            ));
        }
    };
    match crate::pyjson::loads_strict(&raw) {
        Some(doc) if doc.is_object() => Ok(Some(doc)),
        _ => Err(format!(
            "{} is not a JSON object",
            super::fsx::path_field(path)
        )),
    }
}

/// `doctor.shadowed_tools`: the declared persona tools `rules` would force a prompt for.
/// Coarse on purpose — the first word inside `Bash(...)`, and the blanket `Bash`, `Bash(*)`
/// and `*` — because the question is "would this obviously shadow a tool", not the harness's
/// own matcher.
fn shadowed(rules: &[String], declared: &BTreeMap<String, BTreeSet<String>>) -> BTreeSet<String> {
    let mut hit = BTreeSet::new();
    for rule in rules {
        let rule = rule.trim();
        if matches!(rule, "Bash" | "Bash(*)" | "*") {
            return declared.keys().cloned().collect();
        }
        let Some(inner) = rule.strip_prefix("Bash(").and_then(|r| r.strip_suffix(')')) else {
            continue;
        };
        let head = inner
            .split_whitespace()
            .next()
            .unwrap_or("")
            .trim_end_matches('*');
        if !head.is_empty() && declared.contains_key(head) {
            hit.insert(head.to_owned());
        }
    }
    hit
}

/// Which harnesses lack the ask rule for `charter report --yes` that `charter init` writes by
/// default (ADR 0059, amended 2026-09-26): Claude Code in the session directory's shared or
/// local file, opencode in the plane's `opencode.json`. `Err` when a file cannot be read the
/// way the writer reads it, so a broken file is never called a missing rule.
pub fn report_rule_missing(root: &Path, here: &Path) -> Result<Vec<&'static str>, String> {
    let mut claude = false;
    for rel in [settings::SETTINGS, ".claude/settings.local.json"] {
        match found(settings::ensure_rule(
            &here.join(rel),
            "ask",
            settings::REPORT_RULE,
            true,
        )) {
            Found::Present => claude = true,
            Found::Missing => {}
            Found::Unreadable(why) => return Err(why),
        }
    }
    let opencode = match found(settings::ensure_opencode_rule(
        root,
        settings::REPORT_PATTERN,
        "ask",
        true,
    )) {
        Found::Present => true,
        Found::Missing => false,
        Found::Unreadable(why) => return Err(why),
    };
    Ok([("claude-code", claude), ("opencode", opencode)]
        .into_iter()
        .filter(|(_, has)| !has)
        .map(|(h, _)| h)
        .collect())
}

/// `ask rules`: the plane's default ask rule for filing a report, and an ask rule that
/// shadows a tool a persona declares. An ask rule outranks a PreToolUse allow, so charter's
/// persona tool gate cannot pre-approve that tool; the row names the consequence and never
/// suggests deleting the rule, which is the operator's policy.
pub(super) fn ask_rules(d: &Doctor) -> Row {
    const NAME: &str = "ask rules";
    let here = canonical(&d.cwd);
    let path = here.join(settings::SETTINGS);
    let exists = match object(&path) {
        Err(why) => return Row::not_checked(NAME, super::one_line(&why, 1024)),
        Ok(found) => found.is_some(),
    };
    let missing = if d.has_plane {
        match report_rule_missing(&d.root, &here) {
            Ok(missing) => missing,
            Err(why) => return Row::not_checked(NAME, super::one_line(&why, 1024)),
        }
    } else {
        Vec::new()
    };
    let (mut details, mut hints): (Vec<String>, Vec<String>) = (Vec::new(), Vec::new());
    if !missing.is_empty() {
        details.push(format!(
            "no ask rule for `charter report --yes` under {}",
            missing.join(", ")
        ));
        let how = if here == d.root {
            "`purlis guard report` or `purlis doctor --fix` adds it"
        } else if reinit_reaches(&d.root, &here) {
            "`purlis doctor --fix` adds it at the plane root and carries it into the settings a \
             chat started here reads"
        } else {
            "No purlis command writes the settings a chat started in this directory reads; \
             start the chat at the plane root or in a workspace, where the rule is in force"
        };
        hints.push(format!(
            "A report files a PUBLIC issue under your own GitHub login; this rule makes the \
             harness ask you before `charter report … --yes` files one. {how}."
        ));
    }
    let rules = if exists {
        guardcmd::rules(&path, "ask")
    } else {
        Vec::new()
    };
    let mut declared: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    if !rules.is_empty()
        && d.has_plane
        && let Ok(personas) = memory::list_personas(&d.root)
    {
        for persona in personas {
            for tool in crate::personagrant::effective_tools(&d.root, &persona) {
                declared.entry(tool).or_default().insert(persona.clone());
            }
        }
    }
    let hit = shadowed(&rules, &declared);
    if !hit.is_empty() {
        let who: BTreeSet<&String> = hit.iter().flat_map(|t| &declared[t]).collect();
        let show = |items: Vec<&String>| {
            items
                .iter()
                .map(|s| super::one_line(s, super::DISPLAY_LIMIT))
                .collect::<Vec<_>>()
                .join(", ")
        };
        details.push(format!(
            "{} prompt(s) despite being declared by {}",
            show(hit.iter().collect()),
            show(who.into_iter().collect())
        ));
        hints.push(
            "An ask rule outranks a PreToolUse allow, so purlis's persona tool-gate cannot \
             pre-approve these. That may be exactly what you want — this names it so the \
             prompts are not a mystery."
                .to_owned(),
        );
    }
    if !details.is_empty() {
        return Row::warn(NAME, details.join("; "), hints.join(" "));
    }
    if rules.is_empty() {
        return Row::ok(NAME, "none");
    }
    Row::ok(
        NAME,
        format!("{} rule(s), none shadow a persona tool", rules.len()),
    )
}

/// What the rule writer would do with the handoff rule in one file, asked with the write
/// path minus the write — so this row and `charter guard` cannot disagree about "present".
enum Found {
    Present,
    Missing,
    Unreadable(String),
}

fn found(wrote: settings::Wrote) -> Found {
    match wrote {
        // A deny is stricter than the ask rule asked about: the rule is not missing.
        settings::Wrote::Present | settings::Wrote::Denied => Found::Present,
        settings::Wrote::Created => Found::Missing,
        settings::Wrote::Malformed(what) => Found::Unreadable(format!("{what} is not valid")),
        settings::Wrote::Blocked(dir) => Found::Unreadable(format!(
            "{} is not a directory",
            super::fsx::path_field(&dir)
        )),
        settings::Wrote::Failed(path, e) => Found::Unreadable(format!(
            "{} could not be read ({e})",
            super::fsx::path_field(&path)
        )),
    }
}

/// Whether `charter workspace reinit` writes the settings a chat started in `here` reads: a
/// workspace directory, or a clone directly inside one. Not `docs/`, a persona's folder, or a
/// directory deep in a clone, where no charter command writes them.
pub(super) fn reinit_reaches(root: &Path, here: &Path) -> bool {
    let Ok(below) = here.strip_prefix(root.join("workspaces")) else {
        return false;
    };
    matches!(below.components().count(), 1 | 2)
}

/// `handoff gate`: what consents to a handoff here (#1444). A handoff is a dispatch, and its
/// consent is purlis's own dispatch grant, so no rule of a harness is needed and a project
/// without one is as it should be. The row warns while the ask rule an older `purlis init`
/// wrote is still in the project's files: each harness that reads it asks again before every
/// handoff, beside the grant. It offers the `handoff-rule` fix, which removes exactly that rule.
/// A rule about a handoff that a person wrote is theirs, and is named, never warned about.
pub(super) fn handoff_gate(d: &Doctor) -> Row {
    const NAME: &str = "handoff gate";
    if !d.has_plane {
        return Row::ok(NAME, "no control plane found");
    }
    let found = match settings::retire_handoff_rule(&d.root, true) {
        Ok(found) => found,
        Err(why) => {
            return Row::not_checked(
                NAME,
                format!(
                    "{} — purlis cannot tell whether the retired handoff rule is there",
                    super::one_line(&why, 1024)
                ),
            );
        }
    };
    let listed = |files: &[(&'static str, Vec<String>)]| {
        files
            .iter()
            .map(|(file, rules)| format!("{file} ({})", rules.join(", ")))
            .collect::<Vec<_>>()
            .join("; ")
    };
    let yours = if found.left.is_empty() {
        String::new()
    } else {
        format!(
            "\n        \u{21b3} your own rule(s) stay, and your harness decides by them: {}",
            super::one_line(&listed(&found.left), 1024)
        )
    };
    if found.removed.is_empty() {
        return Row::ok(
            NAME,
            format!(
                "consent is the dispatch grant, which purlis asks you for; no harness rule is \
                 needed{yours}"
            ),
        );
    }
    Row::warn(
        NAME,
        format!(
            "the retired ask rule for a handoff is still in {}{yours}",
            super::one_line(&listed(&found.removed), 1024)
        ),
        "A handoff is a dispatch now: you are asked once for a pair of personas, on every \
         harness, and a chat handing off to its own persona asks nothing. While this rule is \
         here your harness asks again before every handoff. `purlis doctor --fix handoff-rule` \
         removes exactly the rule `purlis init` wrote and commits nothing. A teammate on an \
         older purlis is still asked by this rule and by nothing else, so remove it once they \
         have updated.",
    )
    .fixed_by(super::fix::FixId::HandoffRule)
}
