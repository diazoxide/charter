//! `charter persona list` — the roster: which persona is active and by which rung, and each
//! persona's role, vault and vault status. A port of `commands_persona.cmd_persona_list`.

use std::path::Path;

use crate::active::{ActivePersona, PersonaRung};
use crate::repocmd::{Say, Sink};
use crate::tui::{Align, column, pad};

/// How every surface reporting a selection says the persona it names does not exist —
/// `commands_persona._MISSING`.
pub const MISSING: &str = "no persona by that name exists, so no persona is active";

/// What VAULT STATUS says for a vault the registry names, until the secrets port can ask
/// its provider. Python asks the provider's `health()`, which for a plain-file vault counts
/// its secrets and for a 1Password one runs `op` — the secrets registry, being ported on its
/// own branch; see the module header of [`super`].
pub const REGISTERED_UNCHECKED: &str = "registered — its health is not checked here yet";

/// `persona.ways_out`: the commands that move a selection held at the rung labelled
/// `source`, as a clause. **One answer for every surface that names a missing persona** —
/// this roster and the session briefing — because the true answer depends on the rung, and a
/// second copy is the one that offers a command that does nothing.
///
/// Python's clause also offers `charter persona clear`, which this charter does not have;
/// what it offers instead is the one thing that makes the missing name a persona.
pub fn ways_out(source: &str) -> &'static str {
    if source == PersonaRung::Environment.label() {
        return "unset `$CHARTER_PERSONA`, or set it to a persona that exists; it outranks \
                `charter persona use`, so that does not move it";
    }
    "`charter persona use <persona>` selects one that exists, or add the missing one as \
     `personas/<name>/persona.md`"
}

/// `persona.selection().exists`: the name a rung decided is a persona this plane defines.
pub fn exists(root: &Path, name: &str) -> bool {
    crate::personas::valid_name(name) && crate::personas::def_path(root, name).exists()
}

/// `commands_persona._vault_status`: `no vault`, `not set up (local)` when the registry does
/// not name it, else the registered vault's own health — see [`REGISTERED_UNCHECKED`].
pub fn vault_status(root: &Path, state: &Path, vault: Option<&str>) -> String {
    let Some(vault) = vault.filter(|v| !v.is_empty() && *v != "—") else {
        return "no vault".into();
    };
    match super::registered_vaults(root, state) {
        // Python's sentence carries `json`'s own reason after the path; this one stops at
        // the path, which is the part the reader acts on.
        Err(why) => why,
        Ok(vaults) if !vaults.contains_key(vault) => "not set up (local)".into(),
        Ok(_) => REGISTERED_UNCHECKED.into(),
    }
}

/// `charter persona list`, and its exit code.
pub fn list(root: &Path, state: &Path, selection: &ActivePersona, say: Sink) -> u8 {
    let names = super::names(root);
    if names.is_empty() {
        say(Say::Info(
            "No personas yet. Add one: write personas/<name>/persona.md".into(),
        ));
        return 0;
    }
    let active = selection.name.as_deref();
    let missing = active.is_some_and(|n| !exists(root, n));
    let source = crate::personas::one_line(selection.rung.label());
    say(Say::Out(format!(
        "Active persona: {}  (via {source}{})",
        active.map_or_else(|| "—".to_string(), crate::personas::one_line),
        if missing {
            format!(" — {MISSING}")
        } else {
            String::new()
        }
    )));
    if missing {
        say(Say::Out(format!(
            "Ways out: {}.",
            ways_out(selection.rung.label())
        )));
    }
    say(Say::Out(String::new()));

    struct Row {
        name: String,
        shown: String,
        role: String,
        vault: Option<String>,
        vault_shown: String,
    }
    let rows: Vec<Row> = names
        .iter()
        .map(|n| {
            let role = super::own_meta(root, n)
                .and_then(|m| m.get("role").cloned())
                .unwrap_or_default();
            let vault = super::vault_of(root, state, n);
            Row {
                name: n.clone(),
                shown: crate::personas::one_line(n),
                role: crate::personas::one_line(&role),
                vault_shown: crate::personas::one_line(vault.as_deref().unwrap_or("—")),
                vault,
            }
        })
        .collect();
    let nw = column("PERSONA", rows.iter().map(|r| r.shown.as_str()), 2, None);
    let rw = column("ROLE", rows.iter().map(|r| r.role.as_str()), 2, Some(38));
    let vw = column(
        "VAULT",
        rows.iter().map(|r| r.vault_shown.as_str()),
        2,
        None,
    );
    let row = |mark: &str, name: &str, role: &str, vault: &str, status: &str| {
        let line = format!(
            "{mark}{}{}{}{status}",
            pad(name, nw, Align::Left),
            pad(role, rw, Align::Left),
            pad(vault, vw, Align::Left)
        );
        crate::memstore::py_rstrip(&line).to_string()
    };
    say(Say::Out(row(
        "  ",
        "PERSONA",
        "ROLE",
        "VAULT",
        "VAULT STATUS",
    )));
    for r in &rows {
        let mark = if Some(r.name.as_str()) == active {
            "* "
        } else {
            "  "
        };
        let status = crate::shown::one_line(
            &vault_status(root, state, r.vault.as_deref()),
            crate::memstore::PATH_LIMIT,
        );
        say(Say::Out(row(
            mark,
            &r.shown,
            &r.role,
            &r.vault_shown,
            &status,
        )));
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_vault_the_registry_does_not_name_is_not_set_up_and_none_is_no_vault() {
        let dir = tempfile::tempdir().unwrap();
        let state = dir.path().join(".charter");
        std::fs::create_dir_all(&state).unwrap();
        std::fs::write(
            state.join("vaults.json"),
            r#"{"vaults": {"ops": {"provider": "plain-file"}}}"#,
        )
        .unwrap();
        assert_eq!(vault_status(dir.path(), &state, None), "no vault");
        assert_eq!(vault_status(dir.path(), &state, Some("—")), "no vault");
        assert_eq!(
            vault_status(dir.path(), &state, Some("dev")),
            "not set up (local)"
        );
        assert_eq!(
            vault_status(dir.path(), &state, Some("ops")),
            REGISTERED_UNCHECKED
        );
    }
}
