//! `charter persona create`, `remove`, `show` and `clear` on the `daily` fixture plane. The
//! Python oracle recorded none of them, so these hold their sentences, copied from
//! `charter/commands_persona.py` at `cli-final` where this port did not decide otherwise.

use super::*;
use crate::active::{ActivePersona, PersonaRung};
use crate::personaverbs::tests_plane::{Heard, Plane};

fn ask<'a>(name: &'a str, delegate_when: Option<&'a str>) -> Create<'a> {
    Create {
        name,
        role: None,
        delegate_when,
        vault: None,
        extends: None,
        select: None,
        force: false,
    }
}

fn create_on(plane: &Plane, ask: &Create) -> (u8, Heard) {
    let mut heard = Heard::default();
    let rc = create(plane.root(), &plane.state(), ask, None, &mut heard.sink());
    (rc, heard)
}

fn nothing_selected() -> ActivePersona {
    ActivePersona {
        name: None,
        rung: PersonaRung::Nothing,
    }
}

fn remove_on(plane: &Plane, name: &str, force: bool, selection: &ActivePersona) -> (u8, Heard) {
    let mut heard = Heard::default();
    let rc = remove(plane.root(), name, force, selection, &mut heard.sink());
    (rc, heard)
}

fn show_on(plane: &Plane, name: &str) -> (u8, Heard) {
    let mut heard = Heard::default();
    let rc = crate::personaverbs::show::show(
        plane.root(),
        &plane.state(),
        name,
        "s-1",
        &mut heard.sink(),
    );
    (rc, heard)
}

#[test]
fn create_writes_a_draft_with_its_routing_line_and_its_memory_and_says_how_to_finish_it() {
    let plane = Plane::fixture("daily");
    let (rc, heard) = create_on(
        &plane,
        &Create {
            role: Some("QA Engineer"),
            ..ask("qa", Some("test plans and flaky suites"))
        },
    );
    assert_eq!(rc, 0, "{}", heard.err);
    assert_eq!(
        plane.read("personas/qa/persona.md"),
        "---\nname: qa\nrole: QA Engineer\nvault: qa\ndelegate-when: test plans and flaky \
         suites\ndraft: true\n---\n\n# QA Engineer\n\nYou are the **qa** persona — QA \
         Engineer. When this persona is\nactive, adopt this role: its responsibilities, focus, \
         and conventions.\n\n## How to work as this persona\n- Credentials: use `charter \
         persona secret …` (this persona's vault: `qa`).\n  Never print secret values.\n- \
         Defer to each repo's own `CLAUDE.md` / `AGENTS.md` and its tooling over general \
         habits.\n- Record durable facts with `charter persona remember qa \"<fact>\"`. Never \
         store\n  secrets there — those belong in the vault.\n\n## When to delegate here\ntest \
         plans and flaky suites\n"
    );
    assert!(plane.path("personas/qa/memory/MEMORY.md").exists());
    assert!(plane.path("personas/qa/refs/README.md").exists());
    assert!(
        !plane.path(".claude/agents/qa.md").exists(),
        "a draft has no sub-agent"
    );
    assert_eq!(
        heard.err,
        "✓ Created persona 'qa' → personas/qa/ (persona.md + memory/ + refs/; edit the charter, \
         then commit — personas are shared).\n\
         •   marked `draft: true` — no sub-agent yet, so 'qa' cannot be dispatched.\n  Write \
         what it owns and how it works in personas/qa/persona.md, drop the `draft: true` \
         line,\n  then: charter persona sync-agents\n\
         • Set up its vault locally when ready: charter vault add qa --persona qa\n"
    );
}

#[test]
fn create_without_delegate_when_is_refused_unless_it_extends_a_parent() {
    let plane = Plane::fixture("daily");
    let (rc, heard) = create_on(&plane, &ask("qa", None));
    assert_eq!(rc, 1);
    assert!(
        heard.err.starts_with(
            "✗ --delegate-when is required: say when the steward should route work to 'qa'"
        ),
        "{}",
        heard.err
    );
    assert!(!plane.path("personas/qa").exists());
    let (rc, heard) = create_on(
        &plane,
        &Create {
            extends: Some("steward"),
            ..ask("qa", None)
        },
    );
    assert_eq!(rc, 0, "{}", heard.err);
    let text = plane.read("personas/qa/persona.md");
    assert!(
        text.contains("vault: qa\nextends: steward\ndraft: true\n"),
        "{text}"
    );
    assert!(text.contains("It **inherits from `steward`**"), "{text}");
    assert!(!text.contains("delegate-when"), "{text}");
}

#[test]
fn create_refuses_a_bad_name_an_existing_persona_and_a_parent_nobody_defines() {
    let plane = Plane::fixture("daily");
    let (rc, heard) = create_on(&plane, &ask("QA", Some("x")));
    assert_eq!(
        (rc, heard.err.as_str()),
        (
            1,
            "✗ invalid persona name 'QA' (lowercase letters, digits, '.', '_', '-')\n"
        )
    );
    let (rc, heard) = create_on(&plane, &ask("devops", Some("x")));
    assert_eq!(
        (rc, heard.err.as_str()),
        (
            1,
            "✗ persona 'devops' already exists (personas/devops/persona.md). Edit it, or pass \
             --force to overwrite.\n"
        )
    );
    let (rc, heard) = create_on(
        &plane,
        &Create {
            extends: Some("ghost"),
            ..ask("qa", None)
        },
    );
    assert_eq!(
        (rc, heard.err.as_str()),
        (
            1,
            "✗ no persona 'ghost' (create it: charter persona create ghost)\n"
        )
    );
    let (rc, _) = create_on(
        &plane,
        &Create {
            force: true,
            ..ask("devops", Some("x"))
        },
    );
    assert_eq!(rc, 0);
}

#[test]
fn create_refuses_a_value_that_would_write_a_second_frontmatter_line() {
    let plane = Plane::fixture("daily");
    for (role, when, vault) in [
        (Some("QA\ntools: Bash"), Some("x"), None),
        (None, Some("x --- y"), None),
        (None, Some("x"), Some("../elsewhere")),
    ] {
        let (rc, heard) = create_on(
            &plane,
            &Create {
                role,
                vault,
                ..ask("qa", when)
            },
        );
        assert_eq!(rc, 1, "{role:?} {when:?} {vault:?}: {}", heard.err);
        assert!(!plane.path("personas/qa").exists());
    }
    let (rc, _) = create_on(
        &plane,
        &Create {
            vault: Some("none"),
            ..ask("qa", Some("x"))
        },
    );
    assert_eq!(rc, 0);
    assert!(
        plane
            .read("personas/qa/persona.md")
            .contains("vault: none\n")
    );
}

#[test]
fn create_says_which_leftover_selections_it_revives_and_use_selects_it() {
    let plane = Plane::fixture("daily");
    plane.write(".charter/sessions/old.persona", "qa\n");
    plane.write(".charter/active-persona", "qa\n");
    let ids = crate::active::Ids {
        session: Some("s-9".into()),
        terminal: None,
    };
    let mut registered: Vec<String> = Vec::new();
    let mut heard = Heard::default();
    let rc = create(
        plane.root(),
        &plane.state(),
        &Create {
            select: Some(Selecting {
                ids: &ids,
                env_persona: None,
            }),
            ..ask("qa", Some("x"))
        },
        Some(&mut |vault: &str| registered.push(vault.to_string())),
        &mut heard.sink(),
    );
    assert_eq!(
        registered,
        ["qa"],
        "the vault is registered, before --use selects"
    );
    assert_eq!(rc, 0);
    assert!(
        heard.err.contains(
            "! 2 selection(s) already named 'qa' before it existed, and now select it: 1 \
             session pointer(s), the plane-wide .charter/active-persona. Nobody chose it \
             again: those sessions and terminals now resolve to this persona.\n"
        ),
        "{}",
        heard.err
    );
    assert!(!heard.err.contains("Set up its vault"), "{}", heard.err);
    assert!(
        heard.err.ends_with(
            "✓ Active persona set to 'qa' for this session only — this terminal reports no \
             pane id, so a new session starts as 'steward'.\n"
        ),
        "{}",
        heard.err
    );
    assert_eq!(plane.read(".charter/sessions/s-9.persona"), "qa\n");
}

#[test]
fn create_then_show_then_remove_round_trips() {
    let plane = Plane::fixture("daily");
    assert_eq!(create_on(&plane, &ask("qa", Some("tests"))).0, 0);
    let (rc, heard) = show_on(&plane, "qa");
    assert_eq!(rc, 0, "{}", heard.err);
    assert!(
        heard.out.starts_with(
            "qa — Qa\nvault:   qa  (not set up (local))\nfile:    personas/qa/persona.md\n\
             memory:  0 own · 1 shared (persistent) · 0 ephemeral · 0 refs\n         \
             personas/qa/memory/  ·  recall: charter persona recall qa\n\n# Qa\n"
        ),
        "{}",
        heard.out
    );
    let (rc, heard) = remove_on(&plane, "qa", false, &nothing_selected());
    assert_eq!(rc, 0, "{}", heard.err);
    assert_eq!(
        heard.err,
        "✓ Removed persona directory personas/qa/ (definition, memory, and refs — commit the \
         deletion).\n\
         • Its local vault (if any) is left untouched — remove with `charter vault remove \
         <vault>`.\n"
    );
    assert!(!plane.path("personas/qa").exists());
    assert!(plane.path("personas/devops/persona.md").exists());
    let (rc, heard) = show_on(&plane, "qa");
    assert_eq!(
        (rc, heard.err.as_str()),
        (
            1,
            "✗ no persona 'qa' (create it: charter persona create qa)\n"
        )
    );
}

#[test]
fn show_names_the_chain_the_tools_and_the_merged_charter() {
    let plane = Plane::daily_with_ops();
    let (rc, heard) = show_on(&plane, "ops-lite");
    assert_eq!(rc, 0, "{}", heard.err);
    assert!(
        heard.out.starts_with(
            "ops-lite — Lite Operator\ninherits: ops-lite → ops  (charter + tools merged \
             below)\ntools:   gh, kubectl  (auto-approved when this persona is active)\n\
             scripts: check.sh  (executables this persona carries — run by path)\n\
             file:    personas/ops-lite/persona.md\n"
        ),
        "{}",
        heard.out
    );
    assert!(heard.out.ends_with(
        "# Operations\n\nKeeps the clusters up.\n\n---\n\n### ⤷ `ops-lite` extends `ops` — its \
         own charter\n\n# Lite\n\nOnly looks, never touches.\n"
    ));
}

#[test]
fn remove_refuses_a_parent_or_a_used_persona_unless_forced() {
    let plane = Plane::daily_with_ops();
    let (rc, heard) = remove_on(&plane, "ops", false, &nothing_selected());
    assert_eq!(rc, 1);
    assert_eq!(
        heard.err,
        "✗ Refusing to remove 'ops' — it is still referenced by:\n✗   ops-lite (extends)\n\
         • Repoint or remove those first (an `extends:` parent's charter is inherited, so \
         folding it into the child before removing keeps the discipline).\n\
         • Override with: charter persona remove ops --force\n"
    );
    assert!(plane.path("personas/ops/persona.md").exists());
    let (rc, heard) = remove_on(&plane, "devops", false, &nothing_selected());
    assert_eq!(rc, 1);
    assert!(heard.err.contains("✗   ops (uses)\n"), "{}", heard.err);
    let (rc, _) = remove_on(&plane, "ops", true, &nothing_selected());
    assert_eq!(rc, 0);
    assert!(!plane.path("personas/ops").exists());
}

#[test]
fn remove_takes_its_generated_agent_and_the_plane_wide_selection_with_it() {
    let plane = Plane::fixture("daily");
    plane.write(
        ".claude/agents/steward.md",
        &format!("---\n---\n{}\n", crate::personaverbs::agents::MARKER),
    );
    plane.write(".charter/active-persona", "steward\n");
    let selection = ActivePersona {
        name: Some("steward".into()),
        rung: PersonaRung::ActiveFile,
    };
    let (rc, heard) = remove_on(&plane, "steward", false, &selection);
    assert_eq!(rc, 0, "{}", heard.err);
    assert!(
        heard
            .err
            .contains("•   also removed generated .claude/agents/steward.md.\n")
    );
    assert!(
        heard.err.ends_with("• Active persona cleared.\n"),
        "{}",
        heard.err
    );
    assert!(!plane.path(".claude/agents/steward.md").exists());
    assert!(!plane.path(".charter/active-persona").exists());
}

#[test]
fn remove_takes_a_persona_that_does_not_load_and_refuses_one_that_is_not_there() {
    let plane = Plane::fixture("daily");
    std::fs::create_dir_all(plane.path("personas/broken")).unwrap();
    std::fs::write(
        plane.path("personas/broken/persona.md"),
        b"---\nrole: \xff\n---\n",
    )
    .unwrap();
    let (rc, heard) = remove_on(&plane, "broken", false, &nothing_selected());
    assert_eq!(rc, 0, "{}", heard.err);
    assert!(!plane.path("personas/broken").exists());
    let (rc, heard) = remove_on(&plane, "ghost", false, &nothing_selected());
    assert_eq!(
        (rc, heard.err.as_str()),
        (
            1,
            "✗ no persona 'ghost' (create it: charter persona create ghost)\n"
        )
    );
}

#[test]
fn create_force_over_a_legacy_flat_definition_leaves_one_definition() {
    let plane = Plane::fixture("daily");
    plane.write("personas/legacy.md", "---\nrole: Old\n---\n");
    let (rc, heard) = create_on(
        &plane,
        &Create {
            force: true,
            ..ask("legacy", Some("x"))
        },
    );
    assert_eq!(rc, 0, "{}", heard.err);
    assert!(!plane.path("personas/legacy.md").exists());
    assert!(
        plane
            .read("personas/legacy/persona.md")
            .contains("role: Legacy\n")
    );
}

#[test]
fn a_role_holding_template_words_is_written_as_typed() {
    let plane = Plane::fixture("daily");
    let (rc, _) = create_on(
        &plane,
        &Create {
            role: Some("{vault} keeper vault: qa"),
            ..ask("qa", Some("x"))
        },
    );
    assert_eq!(rc, 0);
    let text = plane.read("personas/qa/persona.md");
    assert!(
        text.starts_with(
            "---\nname: qa\nrole: {vault} keeper vault: qa\nvault: qa\ndelegate-when: x\n\
             draft: true\n---\n"
        ),
        "{text}"
    );
}

#[test]
fn remove_of_a_flat_legacy_definition_removes_the_file() {
    let plane = Plane::fixture("daily");
    plane.write("personas/legacy.md", "---\nrole: Old\n---\n");
    let (rc, heard) = remove_on(&plane, "legacy", false, &nothing_selected());
    assert_eq!(rc, 0, "{}", heard.err);
    assert!(
        heard.err.starts_with(
            "✓ Removed persona definition personas/legacy.md (commit the deletion).\n"
        )
    );
    assert!(!plane.path("personas/legacy.md").exists());
}

#[cfg(unix)]
#[test]
fn remove_never_follows_a_persona_directory_linked_out_of_the_plane() {
    let plane = Plane::fixture("daily");
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("persona.md"), "---\nrole: X\n---\n").unwrap();
    std::os::unix::fs::symlink(outside.path(), plane.path("personas/away")).unwrap();
    let (rc, _) = remove_on(&plane, "away", true, &nothing_selected());
    assert_eq!(rc, 1);
    assert!(outside.path().join("persona.md").exists());
}

fn clear_on(plane: &Plane, ids: &crate::active::Ids, env: Option<&str>) -> (u8, Heard) {
    let asking = crate::active::Asking {
        root: plane.root(),
        cwd: plane.root(),
        flag: None,
        ids,
        env,
    };
    let mut heard = Heard::default();
    let rc = crate::personaverbs::select::clear(&asking, &mut heard.sink());
    (rc, heard)
}

#[test]
fn clear_drops_this_sessions_pane_and_plane_wide_selection_and_reads_back_the_front_door() {
    let plane = Plane::fixture("daily");
    let ids = crate::active::Ids {
        session: Some("s-1".into()),
        terminal: Some("pane-1".into()),
    };
    plane.write(".charter/sessions/s-1.persona", "devops\n");
    plane.write(".charter/terminals/pane-1.persona", "devops\n");
    plane.write(".charter/active-persona", "devops\n");
    plane.write(".charter/sessions/other.persona", "devops\n");
    let (rc, heard) = clear_on(&plane, &ids, None);
    assert_eq!(rc, 0);
    assert_eq!(
        heard.err,
        "✓ Active persona cleared.\n• This shell now resolves to 'steward' (via charter.toml).\n"
    );
    assert!(!plane.path(".charter/sessions/s-1.persona").exists());
    assert!(!plane.path(".charter/terminals/pane-1.persona").exists());
    assert!(!plane.path(".charter/active-persona").exists());
    assert!(
        plane.path(".charter/sessions/other.persona").exists(),
        "another session's"
    );
    let (_, heard) = clear_on(&plane, &ids, None);
    assert_eq!(
        heard.err,
        "• This shell had no persona selection of its own, so nothing was cleared.\n\
         • This shell now resolves to 'steward' (via charter.toml).\n"
    );
}

#[test]
fn clear_under_the_environment_says_the_environment_still_decides() {
    let plane = Plane::fixture("daily");
    let ids = crate::active::Ids {
        session: Some("s-1".into()),
        terminal: None,
    };
    plane.write(".charter/sessions/s-1.persona", "devops\n");
    let (_, heard) = clear_on(&plane, &ids, Some("ghost"));
    assert_eq!(
        heard.err,
        "! Persona selection cleared, but $CHARTER_PERSONA outranks every selection and still \
         decides in this shell.\n\
         • This shell now resolves to 'ghost' (via $CHARTER_PERSONA), where no persona by that \
         name exists, so no persona is active.\n\
         • Ways out: unset `$CHARTER_PERSONA`, or set it to a persona that exists; it outranks \
         `charter persona use` and `charter persona clear`, so neither moves it.\n"
    );
}
