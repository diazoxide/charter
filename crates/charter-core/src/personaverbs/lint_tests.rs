//! `charter persona lint` on planes built for each finding. The Python oracle recorded no
//! `persona lint` scenario, so the sentences are held here, copied from
//! `charter/persona.py` at `cli-final`.

use super::*;
use crate::personaverbs::tests_plane::{Heard, Plane};

/// A persona that has everything a clean one needs, so a test adds exactly one fault.
const CLEAN: &str =
    "---\nname: {n}\nrole: Clean\nvault: none\ndelegate-when: clean work\n---\n\n# Clean\n";

fn clean(name: &str) -> String {
    CLEAN.replace("{n}", name)
}

fn plane(personas: &[(&str, &str)]) -> Plane {
    let plane = Plane::fixture("minimal");
    for (name, text) in personas {
        plane.write(&format!("personas/{name}/persona.md"), text);
    }
    plane
}

fn linter(plane: &Plane) -> Linter<'_> {
    // No `$HOME`, so no plugin cache and no skill check, unless a test gives one.
    Linter::new(plane.root(), plane.root()).with_home(None)
}

fn messages(issues: &[Issue]) -> Vec<(Level, &str)> {
    issues
        .iter()
        .map(|i| (i.level, i.message.as_str()))
        .collect()
}

#[test]
fn a_persona_with_role_vault_and_delegate_when_has_no_definition_finding() {
    let p = plane(&[("ok", &clean("ok"))]);
    assert_eq!(messages(&linter(&p).definition("ok")), vec![]);
}

#[test]
fn a_missing_role_vault_and_delegate_when_are_each_a_warning() {
    let p = plane(&[("bare", "---\nname: bare\n---\n\nBare.\n")]);
    assert_eq!(
        messages(&linter(&p).definition("bare")),
        vec![
            (Level::Warn, "no role"),
            (
                Level::Warn,
                "no vault named — add `vault:` or `vault: none` if this persona holds no credentials"
            ),
            (Level::Warn, "no delegate-when → weak auto-routing"),
        ]
    );
}

#[test]
fn a_draft_is_said_to_be_undispatchable() {
    let p = plane(&[("d", &clean("d").replace("---\n\n", "draft: true\n---\n\n"))]);
    let issues = linter(&p).definition("d");
    assert_eq!(issues.len(), 1, "{issues:?}");
    assert!(
        issues[0]
            .message
            .starts_with("draft: true → charter unfinished")
    );
}

#[test]
fn a_dangling_uses_and_a_quoted_extends_are_errors_in_their_own_words() {
    let p = plane(&[(
        "kid",
        &clean("kid").replace("---\n\n", "uses: ghost\nextends: \"ok\"\n---\n\n"),
    )]);
    let issues = linter(&p).definition("kid");
    let errors: Vec<&str> = issues
        .iter()
        .filter(|i| i.level == Level::Error)
        .map(|i| i.message.as_str())
        .collect();
    assert_eq!(errors.len(), 2, "{issues:?}");
    assert_eq!(errors[0], "uses: 'ghost' — no such persona (dangling)");
    assert!(
        errors[1].starts_with(
            "extends: '\"ok\"' is not a persona name — the quotes are part of the value"
        ),
        "{}",
        errors[1]
    );
}

#[test]
fn a_reference_that_is_a_path_is_called_a_path() {
    let p = plane(&[(
        "kid",
        &clean("kid").replace("---\n\n", "uses: ../x\n---\n\n"),
    )]);
    let issues = linter(&p).structural_errors("kid");
    assert_eq!(issues.len(), 1, "{issues:?}");
    assert!(
        issues[0]
            .message
            .starts_with("uses: '../x' is not a name — it is a path.")
    );
}

#[test]
fn an_inheritance_cycle_is_named_as_the_loop_it_makes() {
    let p = plane(&[
        ("a", &clean("a").replace("---\n\n", "extends: b\n---\n\n")),
        ("b", &clean("b").replace("---\n\n", "extends: a\n---\n\n")),
    ]);
    let issues = linter(&p).structural_errors("a");
    assert_eq!(
        messages(&issues),
        vec![(Level::Error, "extends: inheritance cycle (a → b → a)")]
    );
}

#[test]
fn a_key_charter_does_not_read_warns_and_a_miscased_one_is_an_error_said_once() {
    let p = plane(&[(
        "k",
        &clean("k").replace("---\n\n", "modell: opus\nVault: x\n---\n\n"),
    )]);
    let issues = linter(&p).definition("k");
    assert_eq!(issues.len(), 2, "{issues:?}");
    assert!(issues.iter().any(|i| {
        i.level == Level::Warn
            && i.message
                .starts_with("frontmatter key 'modell' is neither read by charter")
    }));
    assert!(issues.iter().any(|i| {
        i.level == Level::Error
            && i.message
                .starts_with("frontmatter key 'Vault' is read by nothing")
    }));
}

#[cfg(unix)]
#[test]
fn a_file_in_bin_that_cannot_run_is_named_with_its_chmod() {
    let p = plane(&[("b", &clean("b"))]);
    p.write("personas/b/bin/notes.txt", "x\n");
    assert_eq!(
        messages(&linter(&p).definition("b")),
        vec![(
            Level::Warn,
            "`bin/notes.txt` is not executable — it will fail at the moment it is needed; `chmod +x personas/b/bin/notes.txt`"
        )]
    );
}

#[test]
fn a_definition_that_is_not_text_says_so_rather_than_that_it_does_not_load() {
    let p = plane(&[]);
    std::fs::create_dir_all(p.path("personas/bin")).unwrap();
    std::fs::write(p.path("personas/bin/persona.md"), b"---\nrole: \xff\n---\n").unwrap();
    let issues = linter(&p).lint("bin");
    assert_eq!(issues.len(), 1, "{issues:?}");
    assert!(
        issues[0].message.starts_with("persona.md: '"),
        "{:?}",
        issues[0]
    );
    assert!(
        issues[0]
            .message
            .ends_with("is not utf-8 text (invalid utf-8 at offset 10)")
    );
}

#[test]
fn credentials_with_no_vault_and_a_refused_server_name_are_errors() {
    let p = plane(&[("m", &clean("m"))]);
    p.write(
        "personas/m/mcp.json",
        r#"{"mcpServers": {"bad name": {"url": "x"}, "g": {"command": "g", "secrets": {"T": "t"}}}}"#,
    );
    let issues = linter(&p).definition("m");
    assert_eq!(issues.len(), 2, "{issues:?}");
    assert!(
        issues[0]
            .message
            .starts_with("mcp: server name 'bad name' is refused")
    );
    assert_eq!(
        issues[1].message,
        "mcp: server(s) g declare `secrets` or `secret_files` but this persona names no vault — add `vault:` or drop the declaration"
    );
}

#[test]
fn a_credential_this_machine_has_not_approved_is_a_warning() {
    let p = plane(&[("m", &clean("m").replace("vault: none", "vault: mv"))]);
    p.write(
        "personas/m/mcp.json",
        r#"{"mcpServers": {"g": {"command": "g", "secrets": {"T": "t"}}}}"#,
    );
    let issues = linter(&p).definition("m");
    assert_eq!(issues.len(), 1, "{issues:?}");
    assert_eq!(issues[0].level, Level::Warn);
    assert!(
        issues[0]
            .message
            .starts_with("mcp: 'g' declares a credential this machine has not approved")
    );
}

fn skill(home: &Path, rel: &str, front: &str) {
    let path = home.join(".claude").join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, format!("---\n{front}\n---\n\nBody.\n")).unwrap();
}

#[test]
fn a_declared_skill_is_looked_for_in_the_plugin_cache_and_the_skill_folders() {
    let home = tempfile::tempdir().unwrap();
    skill(
        home.path(),
        "plugins/cache/x/skills/tdd/SKILL.md",
        "name: tdd",
    );
    skill(
        home.path(),
        "skills/deploy/SKILL.md",
        "name: deploy\ndisable-model-invocation: true",
    );
    let p = plane(&[(
        "s",
        &clean("s").replace("---\n\n", "skills: sp:tdd, deploy, gone\n---\n\n"),
    )]);
    let l = Linter::new(p.root(), p.root()).with_home(Some(home.path().to_path_buf()));
    let issues = l.definition("s");
    assert_eq!(issues.len(), 2, "{issues:?}");
    assert_eq!(issues[0].level, Level::Warn);
    assert!(
        issues[0]
            .message
            .starts_with("declares skill `deploy`, which is human-only")
    );
    assert_eq!(issues[1].level, Level::Error);
    assert!(
        issues[1]
            .message
            .starts_with("declares skill `gone` — not found")
    );
    // With no plugin cache, nothing can be verified and nothing is said.
    std::fs::remove_dir_all(home.path().join(".claude/plugins")).unwrap();
    let l = Linter::new(p.root(), p.root()).with_home(Some(home.path().to_path_buf()));
    assert_eq!(l.definition("s"), vec![]);
}

#[test]
fn a_charter_naming_an_enabled_plugins_skill_must_name_an_invokable_one() {
    let home = tempfile::tempdir().unwrap();
    skill(home.path(), "plugins/x/skills/tdd/SKILL.md", "name: tdd");
    let p = plane(&[(
        "s",
        &clean("s").replace(
            "# Clean\n",
            "# Clean\n\nUse `sp:tdd`, `sp:gone` and `other:gone`.\n",
        ),
    )]);
    p.write(
        ".claude/settings.json",
        r#"{"enabledPlugins": {"sp@market": true}}"#,
    );
    let l = Linter::new(p.root(), p.root()).with_home(Some(home.path().to_path_buf()));
    assert_eq!(
        messages(&l.definition("s")),
        vec![(
            Level::Error,
            "charter names `sp:gone` — not an installed skill (renamed/removed upstream?); fix it or pin the marketplace"
        )]
    );
}

#[test]
fn the_generated_agent_is_missing_stale_or_left_alone() {
    let p = plane(&[("a", &clean("a"))]);
    let l = linter(&p);
    assert_eq!(
        messages(&l.agent_sync("a")),
        vec![(
            Level::Warn,
            "no generated sub-agent — run `charter persona sync-agents`"
        )]
    );
    p.write(".claude/agents/a.md", &format!("{}\nold\n", agents::MARKER));
    assert_eq!(
        messages(&l.agent_sync("a")),
        vec![(
            Level::Warn,
            "generated sub-agent is stale — run `charter persona sync-agents`"
        )]
    );
    let def = crate::personaverbs::resolve(p.root(), "a").unwrap();
    p.write(
        ".claude/agents/a.md",
        &agents::render(p.root(), p.root(), "a", &def),
    );
    assert_eq!(l.agent_sync("a"), vec![]);
    p.write(".claude/agents/a.md", "hand-written\n");
    assert_eq!(l.agent_sync("a"), vec![]);
}

use crate::personaverbs::agents;

fn run(p: &Plane, name: Option<&str>, only: Option<&str>) -> (u8, Heard) {
    let mut heard = Heard::default();
    let rc = lint_command(&linter(p), name, only, &mut heard.sink());
    (rc, heard)
}

#[test]
fn the_command_exits_one_on_an_error_and_zero_on_warnings_alone() {
    let p = plane(&[("ok", &clean("ok"))]);
    let def = crate::personaverbs::resolve(p.root(), "ok").unwrap();
    p.write(
        ".claude/agents/ok.md",
        &agents::render(p.root(), p.root(), "ok", &def),
    );
    // The fixture's own `steward` has no generated agent: a warning.
    let (rc, heard) = run(&p, None, None);
    assert_eq!(rc, 0);
    assert_eq!(
        heard.err,
        "✓ ok: ok\n! steward: no generated sub-agent — run `charter persona sync-agents`\n"
    );
    p.write(
        "personas/ok/persona.md",
        &clean("ok").replace("---\n\n", "uses: ghost\n---\n\n"),
    );
    let (rc, heard) = run(&p, Some("ok"), None);
    assert_eq!(rc, 1);
    assert_eq!(
        heard.err,
        "✗ ok: uses: 'ghost' — no such persona (dangling)\n\
         ! ok: generated sub-agent is stale — run `charter persona sync-agents`\n\
         ✗ 1 error(s) — dangling reuse or unloadable persona.\n"
    );
}

#[test]
fn only_narrows_to_one_finding_and_makes_it_an_error() {
    let p = plane(&[]);
    let (rc, heard) = run(&p, Some("steward"), Some("sub-agent"));
    assert_eq!(rc, 1);
    assert!(
        heard.err.starts_with("✗ steward: no generated sub-agent"),
        "{}",
        heard.err
    );
    let (rc, heard) = run(&p, Some("steward"), Some("vault"));
    assert_eq!((rc, heard.err.as_str()), (0, "✓ steward: ok\n"));
}

#[test]
fn a_name_outside_the_alphabet_is_refused_and_one_nothing_defines_does_not_load() {
    let p = plane(&[]);
    let (rc, heard) = run(&p, Some("Bad"), None);
    assert_eq!(rc, 1);
    assert_eq!(
        heard.err,
        "✗ invalid persona name 'Bad' (lowercase letters, digits, '.', '_', '-')\n"
    );
    let (rc, heard) = run(&p, Some("ghost"), None);
    assert_eq!(rc, 1);
    assert!(
        heard
            .err
            .starts_with("✗ ghost: persona 'ghost' does not load\n"),
        "{}",
        heard.err
    );
}

#[test]
fn a_plane_with_no_personas_has_nothing_to_lint() {
    let p = plane(&[]);
    std::fs::remove_dir_all(p.path("personas")).unwrap();
    let (rc, heard) = run(&p, None, None);
    assert_eq!((rc, heard.err.as_str()), (0, "• No personas to lint.\n"));
}
