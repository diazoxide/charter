//! `personas` and `persona grant`: the roster's health and the active persona's, read by
//! [`crate::personaverbs::lint::Linter`] — the one implementation `charter persona lint`
//! prints — so the doctor cannot call a persona well-formed that `lint` calls broken.
//!
//! The `personas` row also counts the sub-agent files purlis generated before a persona ran as
//! its own chat (#1451), and offers the fix that removes them.

use super::{Doctor, Row};
use crate::personaverbs::lint::{Level, Linter};

/// `personas`: `persona lint` across every persona, one line. WARN, never FAIL: an untidy
/// persona does not stop anybody cloning a repo or reaching a forge.
pub(super) fn personas(d: &Doctor) -> Row {
    let row = roster(d);
    let leftover = crate::personaverbs::retired::generated(&d.root);
    if leftover.is_empty() {
        return row;
    }
    let said = format!(
        "{} generated sub-agent file(s) remain in .claude/agents/",
        leftover.len()
    );
    let detail = match row.status {
        super::Status::Ok => said,
        _ => format!("{} · {said}", row.detail),
    };
    let fix = "purlis doctor --fix persona-agents  (a persona runs as its own chat now; this \
               removes the sub-agent files purlis wrote and says what it changed)";
    let hint = if row.hint.is_empty() {
        fix.to_owned()
    } else {
        format!("{}  ·  {fix}", row.hint)
    };
    Row::warn(&row.name, detail, hint).fixed_by(super::fix::FixId::PersonaAgents)
}

/// `persona lint` across every persona, one line.
fn roster(d: &Doctor) -> Row {
    const NAME: &str = "personas";
    let state = crate::personaverbs::state_dir(&d.root);
    let linter = Linter::new(&d.root, &state).with_home(d.home.clone());
    let names = linter.names();
    if names.is_empty() {
        return Row::ok(NAME, "none defined");
    }
    let mut errors: Vec<&str> = Vec::new();
    let mut warns: Vec<&str> = Vec::new();
    let mut drafts: Vec<&str> = Vec::new();
    for name in &names {
        let issues = linter.definition(name);
        if crate::personaverbs::is_draft(&d.root, name) {
            drafts.push(name);
        }
        if issues.iter().any(|i| i.level == Level::Error) {
            errors.push(name);
        }
        if issues.iter().any(|i| i.level == Level::Warn) {
            warns.push(name);
        }
    }
    if errors.is_empty() && warns.is_empty() {
        return Row::ok(NAME, format!("{} persona(s), all clean", names.len()));
    }
    let mut bits = Vec::new();
    if !errors.is_empty() {
        bits.push(format!(
            "{} with error(s): {}",
            errors.len(),
            errors.join(", ")
        ));
    }
    if !drafts.is_empty() {
        bits.push(format!("{} draft: {}", drafts.len(), drafts.join(", ")));
    }
    let soft: Vec<&str> = warns.into_iter().filter(|n| !drafts.contains(n)).collect();
    if !soft.is_empty() {
        bits.push(format!(
            "{} with warning(s): {}",
            soft.len(),
            soft.join(", ")
        ));
    }
    Row::warn(
        NAME,
        bits.join(" · "),
        "purlis persona lint  (per-persona detail and how to fix each)",
    )
}

/// `persona grant`: the ACTIVE persona is broken and its tools are still auto-approved. The
/// tool gate reads a persona's tools whether or not it is well-formed, so this reports the
/// pairing rather than revoking a grant the operator wrote by hand — silently, which is how a
/// revoked grant would arrive. Quiet when the active persona is well-formed.
pub(super) fn persona_grant(d: &Doctor) -> Row {
    const NAME: &str = "persona grant";
    let asking = crate::active::Asking {
        root: &d.root,
        cwd: &d.cwd,
        flag: None,
        ids: &d.ids,
        env: d.persona_env.as_deref(),
    };
    let Some(active) = crate::active::persona(&asking).name else {
        return Row::ok(NAME, "no active persona");
    };
    let state = crate::personaverbs::state_dir(&d.root);
    let issues = Linter::new(&d.root, &state)
        .with_home(d.home.clone())
        .structural_errors(&active);
    if issues.is_empty() {
        return Row::ok(NAME, format!("'{active}' is well-formed"));
    }
    let tools = crate::personagrant::effective_tools(&d.root, &active);
    if tools.is_empty() {
        return Row::ok(NAME, format!("'{active}' is broken but grants no tools"));
    }
    let why = issues
        .iter()
        .take(2)
        .map(|i| i.message.as_str())
        .collect::<Vec<_>>()
        .join("; ");
    Row::warn(
        NAME,
        format!(
            "'{active}' is broken and still auto-approves {}",
            tools.into_iter().collect::<Vec<_>>().join(", ")
        ),
        format!(
            "{why}  → purlis persona lint {active}  (the gate reads this persona's tools \
             whether or not it loads cleanly, so a prompt you expected to see may not appear)"
        ),
    )
}
