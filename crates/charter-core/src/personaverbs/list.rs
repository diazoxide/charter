//! `charter persona list` — the roster: which persona is active and by which rung, and each
//! persona's role, vault and vault status. A port of `commands_persona.cmd_persona_list`.

use std::path::Path;

use crate::active::{ActivePersona, PersonaRung};
use crate::repocmd::{Say, Sink};
use crate::tui::{Align, column, pad};

/// How every surface reporting a selection says the persona it names does not exist —
/// `commands_persona._MISSING`.
pub const MISSING: &str = "no persona by that name exists, so no persona is active";

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
/// not name it, else the registered vault's provider's own health line — a plain-file
/// vault's count and mode, a 1Password item's field count — or the registry's refusal.
pub fn vault_status(root: &Path, state: &Path, vault: Option<&str>) -> String {
    use crate::secrets::{cmd, registry};
    let Some(vault) = vault.filter(|v| !v.is_empty() && *v != "—") else {
        return "no vault".into();
    };
    let ctx = super::vault_ctx(root, state);
    let doc = match registry::load_registry(&ctx) {
        Ok(doc) => doc,
        Err(e) => return e.to_string(),
    };
    if !registry::vaults(&doc).contains_key(vault) {
        return "not set up (local)".into();
    }
    match registry::vault_in(&doc, vault) {
        Ok(v) => cmd::health(&ctx, &v).1,
        Err(e) => e.to_string(),
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
    use crate::personaverbs::tests_plane::{Heard, Plane, SOLO};

    fn run(plane: &Plane, name: Option<&str>, rung: PersonaRung) -> (u8, Heard) {
        let selection = ActivePersona {
            name: name.map(String::from),
            rung,
        };
        let mut heard = Heard::default();
        let rc = list(plane.root(), &plane.state(), &selection, &mut heard.sink());
        (rc, heard)
    }

    const ROSTER: &str = "  PERSONA  ROLE             VAULT   VAULT STATUS\n\
                          \x20 devops   DevOps Engineer  devops  not set up (local)\n";

    #[test]
    fn the_active_persona_is_starred_and_named_with_the_rung_that_chose_it() {
        // persona-list-marks-the-active-persona-and-each-vaults-status
        let plane = Plane::fixture("daily");
        let (rc, heard) = run(&plane, Some("steward"), PersonaRung::PlaneDefault);
        assert_eq!((rc, heard.err.as_str()), (0, ""));
        assert_eq!(
            heard.out,
            format!(
                "Active persona: steward  (via charter.toml)\n\n{ROSTER}\
                 * steward  Steward          —       no vault\n"
            )
        );
    }

    #[test]
    fn a_selection_naming_no_persona_is_none_and_the_way_out_depends_on_the_rung() {
        // persona-list-says-a-selection-naming-no-persona-is-none
        let plane = Plane::fixture("daily");
        let (rc, heard) = run(&plane, Some("ghost"), PersonaRung::SessionPointer);
        assert_eq!(rc, 0);
        assert_eq!(
            heard.out,
            format!(
                "Active persona: ghost  (via session — no persona by that name exists, so no \
                 persona is active)\n\
                 Ways out: `charter persona use <persona>` selects one that exists, or add the \
                 missing one as `personas/<name>/persona.md`.\n\n{ROSTER}\
                 \x20 steward  Steward          —       no vault\n"
            )
        );
        // persona-list-under-the-environment-says-the-environment-is-the-way-out
        let (_, heard) = run(&plane, Some("ghost"), PersonaRung::Environment);
        assert!(
            heard.out.starts_with(
                "Active persona: ghost  (via $CHARTER_PERSONA — no persona by that name exists, \
                 so no persona is active)\n\
                 Ways out: unset `$CHARTER_PERSONA`, or set it to a persona that exists; it \
                 outranks `charter persona use`, so that does not move it.\n\n"
            ),
            "{}",
            heard.out
        );
    }

    #[test]
    fn nothing_selected_is_a_dash_and_no_row_is_starred() {
        let plane = Plane::fixture("daily");
        let (_, heard) = run(&plane, None, PersonaRung::Nothing);
        assert_eq!(
            heard.out,
            format!(
                "Active persona: —  (via none)\n\n{ROSTER}\
                 \x20 steward  Steward          —       no vault\n"
            )
        );
    }

    #[test]
    fn a_persona_with_no_role_has_an_empty_role_cell_and_its_registry_tagged_vault() {
        // persona-list-with-a-persona-that-declares-no-role
        let plane = Plane::fixture("daily");
        plane.write("personas/solo/persona.md", SOLO);
        let (_, heard) = run(&plane, Some("steward"), PersonaRung::PlaneDefault);
        assert_eq!(
            heard.out,
            format!(
                "Active persona: steward  (via charter.toml)\n\n{ROSTER}\
                 \x20 solo                      —       no vault\n\
                 * steward  Steward          —       no vault\n"
            )
        );
    }

    #[test]
    fn a_plane_with_no_personas_says_how_to_add_one() {
        // persona-list-on-a-plane-with-no-personas
        let plane = Plane::fixture("minimal");
        std::fs::remove_dir_all(plane.path("personas/steward")).unwrap();
        let (rc, heard) = run(&plane, None, PersonaRung::Nothing);
        assert_eq!((rc, heard.out.as_str()), (0, ""));
        assert_eq!(
            heard.err,
            "• No personas yet. Add one: write personas/<name>/persona.md\n"
        );
    }

    #[test]
    fn a_persona_exists_only_under_a_valid_name_with_a_definition() {
        let plane = Plane::fixture("daily");
        assert!(exists(plane.root(), "steward"));
        assert!(!exists(plane.root(), "ghost"));
        plane.write("personas/Bad/persona.md", "---\nrole: x\n---\n");
        assert!(!exists(plane.root(), "Bad"));
    }

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
            "no 'file' configured"
        );
    }
}
