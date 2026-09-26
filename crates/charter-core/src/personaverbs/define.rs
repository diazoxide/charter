//! `charter persona create` and `charter persona remove` — write a persona's definition, and
//! delete one. A port of `commands_persona.cmd_persona_create` and `cmd_persona_remove`.
//!
//! # A new persona is a draft
//!
//! The scaffold holds only true statements about the persona, and `draft: true` says it is
//! unfinished: no sub-agent is generated for a draft, so a persona dispatched before anyone
//! wrote its charter cannot hand the scaffold to a sub-agent as its remit. `persona lint`
//! and the doctor's `personas` row say it is a draft until the line is dropped.
//!
//! # What `create` refuses that Python did not
//!
//! A `--role`, `--delegate-when` or `--extends` holding a line break or `---` is refused.
//! Each is written into the frontmatter as one line, so a line break there writes a key the
//! operator never typed — `--role $'x\ntools: Bash'` would declare a tool the persona gate
//! then approves — and a `---` ends the frontmatter early. `--vault` must be a vault name the
//! registry accepts, or `none`. Python wrote all four as given.
//!
//! # `--with-vault` registers a keyring vault
//!
//! Python registered a plain-file vault. This charter keeps a vault in the system keyring by
//! default (ADR 0047), so `--with-vault` registers what `charter vault add <vault> --persona
//! <name>` would, and the caller does that registering — this module never touches a vault.

use std::path::Path;

use crate::active::Ids;
use crate::repocmd::{Say, Sink};

/// What `charter persona create` was asked for.
pub struct Create<'a> {
    pub name: &'a str,
    pub role: Option<&'a str>,
    pub delegate_when: Option<&'a str>,
    pub vault: Option<&'a str>,
    pub extends: Option<&'a str>,
    /// `--with-vault`: the caller registers the vault; this only stops the hint saying how.
    pub with_vault: bool,
    /// `--use`: select it, for this session and pane, as `charter persona use` would.
    pub select: Option<Selecting<'a>>,
    pub force: bool,
}

/// Who `--use` selects the new persona for.
pub struct Selecting<'a> {
    pub ids: &'a Ids,
    pub env_persona: Option<&'a str>,
}

/// The scaffold a new persona starts from — `commands_persona._TEMPLATE`.
const TEMPLATE: &str = "---
name: {name}
role: {role}
vault: {vault}
draft: true
---

# {role}

You are the **{name}** persona — {role}. When this persona is
active, adopt this role: its responsibilities, focus, and conventions.

## How to work as this persona
- Credentials: use `charter persona secret …` (this persona's vault: `{vault}`).
  Never print secret values.
- Defer to each repo's own `CLAUDE.md` / `AGENTS.md` and its tooling over general habits.
- Record durable facts with `charter persona remember {name} \"<fact>\"`. Never store
  secrets there — those belong in the vault.
";

/// Appended when the persona states its own routing intent — `_TEMPLATE_DELEGATE`.
const TEMPLATE_DELEGATE: &str = "\n## When to delegate here\n{delegate_when}\n";

/// The text of a new persona's `persona.md`.
fn scaffold(
    name: &str,
    role: &str,
    vault: &str,
    delegate_when: &str,
    extends: Option<&str>,
) -> String {
    let mut text = TEMPLATE
        .replace("{name}", name)
        .replace("{role}", role)
        .replace("{vault}", vault);
    let vault_line = format!("vault: {vault}\n");
    if !delegate_when.is_empty() {
        text = text.replacen(
            &vault_line,
            &format!("{vault_line}delegate-when: {delegate_when}\n"),
            1,
        );
        text.push_str(&TEMPLATE_DELEGATE.replace("{delegate_when}", delegate_when));
    }
    if let Some(parent) = extends {
        text = text.replacen(&vault_line, &format!("{vault_line}extends: {parent}\n"), 1);
        text = text.replacen(
            "active, adopt this role: its responsibilities, focus, and conventions.",
            &format!(
                "active, adopt this role. It **inherits from `{parent}`** (that persona's \
                 charter + tools apply); the sections below are what THIS persona ADDS on top."
            ),
            1,
        );
    }
    text
}

/// Why a value typed for one frontmatter line cannot be written as one, or `None`.
fn one_line_refusal(flag: &str, value: &str) -> Option<String> {
    let breaks = value
        .chars()
        .any(|c| c.is_control() || matches!(c, '\u{2028}' | '\u{2029}'));
    (breaks || value.contains("---")).then(|| {
        format!(
            "{flag} is written into persona.md's frontmatter as one line, so it may hold no \
             line break, control character or `---`: '{}'",
            crate::personas::one_line(value)
        )
    })
}

/// `charter persona create <name>`, and its exit code.
pub fn create(root: &Path, state: &Path, ask: &Create, say: Sink) -> u8 {
    let name = ask.name;
    if let Some(refused) = crate::personas::shape_refusal(name) {
        say(Say::Fail(refused));
        return 1;
    }
    let existing = crate::personas::def_path(root, name);
    if existing.exists() && !ask.force {
        say(Say::Fail(format!(
            "persona '{name}' already exists ({}). Edit it, or pass --force to overwrite.",
            super::rel(root, &existing)
        )));
        return 1;
    }
    let extends = ask.extends.filter(|e| !e.is_empty());
    if let Some(parent) = extends
        && let Some(refused) = crate::personas::name_refusal(root, parent)
    {
        say(Say::Fail(refused));
        return 1;
    }
    let delegate_when = crate::memstore::py_strip(ask.delegate_when.unwrap_or_default());
    if delegate_when.is_empty() && extends.is_none() {
        say(Say::Fail(format!(
            "--delegate-when is required: say when the steward should route work to '{name}', \
             e.g.\n  charter persona create {name} --delegate-when \"CI/CD pipelines, k8s \
             deploys, cluster access\"\nIt becomes the persona's routing line in its \
             dispatchable description. (Inheriting one? Pass --extends <parent> instead.)"
        )));
        return 1;
    }
    let role = ask
        .role
        .filter(|r| !r.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| super::py_title(name));
    let vault = ask.vault.filter(|v| !v.is_empty()).unwrap_or(name);
    for (flag, value) in [
        ("--role", role.as_str()),
        ("--delegate-when", delegate_when),
    ] {
        if let Some(refused) = one_line_refusal(flag, value) {
            say(Say::Fail(refused));
            return 1;
        }
    }
    if vault != super::NO_VAULT && !crate::secrets::registry::name_ok(vault) {
        say(Say::Fail(crate::secrets::registry::name_refusal(vault)));
        return 1;
    }

    // Counted before anything is written, and only for a name that defines nothing yet: a
    // pointer naming a persona `--force` is overwriting was never stale.
    let revived = if existing.exists() {
        (0, 0, 0)
    } else {
        crate::active::pointers_naming(root, name)
    };
    let plane = crate::workspaces::Plane::open(root);
    let persona = match plane.persona(name) {
        Ok(persona) => persona,
        Err(e) => {
            say(Say::Fail(e.to_string()));
            return 1;
        }
    };
    let file = persona.dir().join("persona.md");
    if let Err(refused) = crate::contain::writable(root, &file) {
        say(Say::Fail(refused.to_string()));
        return 1;
    }
    let text = scaffold(name, &role, vault, delegate_when, extends);
    let written = std::fs::create_dir_all(persona.dir())
        .and_then(|()| std::fs::write(&file, text))
        .and_then(|()| persona.scaffold_memory());
    let shared = plane
        .persona(crate::personas::SHARED)
        .map_err(|e| std::io::Error::other(e.to_string()))
        .and_then(|shared| shared.scaffold_memory());
    if let Err(e) = written.and(shared) {
        say(Say::Fail(e.to_string()));
        return 1;
    }
    say(Say::Done(format!(
        "Created persona '{name}' → personas/{name}/ (persona.md + memory/ + refs/; edit the \
         charter, then commit — personas are shared)."
    )));
    say_revived(name, revived, say);
    match super::agents::write_agent(root, state, name, say) {
        super::agents::Outcome::Written => say(Say::Info(format!(
            "  generated .claude/agents/{name}.md — invokable as subagent '{name}'."
        ))),
        super::agents::Outcome::Draft => say(Say::Info(format!(
            "  marked `draft: true` — no sub-agent yet, so '{name}' cannot be dispatched.\n  \
             Write what it owns and how it works in personas/{name}/persona.md, drop the \
             `draft: true` line,\n  then: charter persona sync-agents"
        ))),
        _ => {}
    }
    if !ask.with_vault {
        say(Say::Info(format!(
            "Set up its vault locally when ready: charter vault add {vault} --persona {name}"
        )));
    }
    if let Some(selecting) = &ask.select {
        let scope = super::select::set_active(root, name, selecting.ids);
        say(Say::Done(format!(
            "Active persona set to '{name}'{}.",
            super::select::scope_note(root, scope)
        )));
        super::select::warn_env(name, selecting.env_persona, say);
    }
    0
}

/// `_say_revived`: the selections a removed persona of this name left behind, which the new
/// one now is — made by nobody who ran `use` for it.
fn say_revived(name: &str, (sessions, terminals, active): (usize, usize, usize), say: Sink) {
    let mut rungs = Vec::new();
    if sessions > 0 {
        rungs.push(format!("{sessions} session pointer(s)"));
    }
    if terminals > 0 {
        rungs.push(format!("{terminals} terminal pointer(s)"));
    }
    if active > 0 {
        rungs.push("the plane-wide .charter/active-persona".to_string());
    }
    if rungs.is_empty() {
        return;
    }
    say(Say::Warn(format!(
        "{} selection(s) already named '{name}' before it existed, and now select it: {}. \
         Nobody chose it again: those sessions and terminals now resolve to this persona.",
        sessions + terminals + active,
        rungs.join(", ")
    )));
}

/// `_dependents_of`: every other persona that `extends:` or `uses:` `name`, as `other (why)`.
fn dependents_of(root: &Path, name: &str) -> Vec<String> {
    let mut out = Vec::new();
    for other in super::names(root) {
        if other == name {
            continue;
        }
        let meta = super::own_meta(root, &other).unwrap_or_default();
        let extends = meta
            .get("extends")
            .map(|e| crate::memstore::py_strip(e))
            .unwrap_or_default();
        if extends == name {
            out.push(format!("{other} (extends)"));
            continue;
        }
        let uses = crate::personagrant::csv_list(meta.get("uses").map(String::as_str));
        if uses.iter().any(|u| u == name) {
            out.push(format!("{other} (uses)"));
        }
    }
    out.sort();
    out
}

/// `charter persona remove <name>`, and its exit code. `selection` is what this shell
/// resolves to now: the plane-wide file is dropped when it is what selects `name`.
pub fn remove(
    root: &Path,
    name: &str,
    force: bool,
    selection: &crate::active::ActivePersona,
    say: Sink,
) -> u8 {
    if let Some(refused) = crate::personas::name_refusal(root, name) {
        say(Say::Fail(refused));
        return 1;
    }
    let dependents = dependents_of(root, name);
    if !dependents.is_empty() && !force {
        say(Say::Fail(format!(
            "Refusing to remove '{name}' — it is still referenced by:"
        )));
        for dependent in &dependents {
            say(Say::Fail(format!("  {dependent}")));
        }
        say(Say::Info(
            "Repoint or remove those first (an `extends:` parent's charter is inherited, so \
             folding it into the child before removing keeps the discipline)."
                .into(),
        ));
        say(Say::Info(format!(
            "Override with: charter persona remove {name} --force"
        )));
        return 1;
    }
    let dir = root.join("personas").join(name);
    let dir_layout = dir.join("persona.md").exists();
    let target = if dir_layout {
        dir.clone()
    } else {
        crate::personas::def_path(root, name)
    };
    // Both the directory and its definition are asked: a committed link at either would
    // send the delete out of the plane.
    if let Err(refused) = crate::contain::writable(root, &target)
        .and_then(|()| crate::contain::writable(root, &crate::personas::def_path(root, name)))
    {
        say(Say::Fail(refused.to_string()));
        return 1;
    }
    let removed = if dir_layout {
        std::fs::remove_dir_all(&dir)
    } else {
        std::fs::remove_file(&target)
    };
    if let Err(e) = removed {
        say(Say::Fail(format!(
            "could not remove {}: {e}",
            super::rel(root, &target)
        )));
        return 1;
    }
    if dir_layout {
        say(Say::Done(format!(
            "Removed persona directory personas/{name}/ (definition, memory, and refs — commit \
             the deletion)."
        )));
    } else {
        say(Say::Done(format!(
            "Removed persona definition {} (commit the deletion).",
            super::rel(root, &target)
        )));
    }
    if super::agents::remove_agent(root, name) {
        say(Say::Info(format!(
            "  also removed generated .claude/agents/{name}.md."
        )));
    }
    say(Say::Info(
        "Its local vault (if any) is left untouched — remove with `charter vault remove \
         <vault>`."
            .into(),
    ));
    if selection.name.as_deref() == Some(name)
        && selection.rung == crate::active::PersonaRung::ActiveFile
    {
        let file = root.join(".charter").join("active-persona");
        if crate::contain::no_link_on_the_way(root, &file).is_ok() {
            let _ = std::fs::remove_file(&file);
        }
        say(Say::Info("Active persona cleared.".into()));
    }
    0
}

#[cfg(test)]
#[path = "define_tests.rs"]
mod tests;
