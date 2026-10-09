//! `charter persona lint` on planes built for each finding. The Python oracle recorded no
//! `persona lint` scenario, so the sentences are held here, copied from
//! `charter/persona.py` at `cli-final`.

use super::*;
use crate::personaverbs::tests_plane::{Heard, Plane, generated_agent};

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
    assert_eq!(
        messages(&linter(&p).definition("d")),
        vec![(
            Level::Warn,
            "draft: true → charter unfinished, so no chat is dispatched to it. Finish the \
             charter, then drop the line"
        )]
    );
}

#[test]
fn a_key_that_only_fed_the_retired_sub_agent_is_a_warning_that_names_its_ticket() {
    // #1451: nothing enforces these lines now, and a reader must not take one for a rule.
    let p = plane(&[(
        "r",
        &clean("r").replace(
            "---\n\n",
            "agent-tools: Read, Grep\nskills: deploy\nmemory: project\ndispatch-isolation: \
             worktree\n---\n\n",
        ),
    )]);
    let issues = linter(&p).definition("r");
    let said: Vec<&str> = issues.iter().map(|i| i.message.as_str()).collect();
    // Three, not four: `dispatch-isolation` is read again (#1453), as the persona's default
    // place to work when a dispatch names none.
    assert_eq!(issues.len(), 3, "{said:#?}");
    assert!(issues.iter().all(|i| i.level == Level::Warn));
    for key in ["agent-tools", "skills", "memory"] {
        assert!(
            said.iter()
                .any(|m| m.starts_with(&format!("`{key}:` is no longer read: "))
                    && m.contains("not in this version yet (#1460)")),
            "{key}: {said:#?}"
        );
    }
    // What widened is said (D-1451-19): this persona was read-only by its tool list.
    assert!(
        said.iter()
            .any(|m| m
                .contains("As a sub-agent this persona could not edit files; as a chat it can")),
        "{said:#?}"
    );
    assert!(
        !said.iter().any(|m| m.contains("dispatch-isolation")),
        "a key purlis reads is no warning: {said:#?}"
    );
    assert_eq!(
        crate::personaverbs::retired::key_notice("dispatch-isolation", "worktree"),
        None
    );
}

#[test]
fn a_deny_list_is_reported_as_honoured_or_refused_per_harness_and_not_as_retired() {
    // D-1451-18. A parent's line denies its child too, so the child is told as well.
    let p = plane(&[
        (
            "rev",
            &clean("rev").replace("---\n\n", "disallowed-tools: Write, Edit\n---\n\n"),
        ),
        (
            "kid",
            "---\nname: kid\nextends: rev\nrole: Kid\nvault: none\ndelegate-when: w\n---\n\n# K\n",
        ),
    ]);
    for who in ["rev", "kid"] {
        assert_eq!(
            messages(&linter(&p).definition(who)),
            vec![(
                Level::Warn,
                "`disallowed-tools:` is honoured on Claude Code: a chat as this persona is \
                 started with those tools denied. On Codex and opencode purlis cannot deny \
                 them, so a chat as this persona is refused there, and so is a dispatch to it"
            )],
            "{who}"
        );
    }
}

#[test]
fn a_charter_that_still_teaches_the_sub_agent_route_is_a_warning() {
    // F3: both real projects' front-door charters do. Reported, never rewritten.
    let p = plane(&[(
        "front",
        &clean("front").replace(
            "# Clean\n",
            "# Clean\n\nDelegate with the Agent tool (`subagent_type: devops`).\n",
        ),
    )]);
    assert_eq!(
        messages(&linter(&p).definition("front")),
        vec![(
            Level::Warn,
            "its charter still mentions `subagent_type`: a persona is no longer a harness \
             sub-agent, so a chat that follows that text is refused. Say `purlis dispatch --to \
             <persona>` there instead"
        )]
    );
}

#[test]
fn a_persona_named_like_a_harness_s_own_helper_is_a_warning() {
    // F5: the helper of that name is refused as "a persona" from then on.
    let p = plane(&[("general-purpose", &clean("general-purpose"))]);
    let issues = linter(&p).definition("general-purpose");
    assert_eq!(issues.len(), 1, "{issues:?}");
    assert!(
        issues[0]
            .message
            .starts_with("this persona is named like a harness's own helper, `general-purpose`."),
        "{issues:?}"
    );
}

#[test]
fn the_keys_that_have_a_job_now_are_no_finding() {
    let p = plane(&[(
        "j",
        &clean("j").replace(
            "---\n\n",
            "color: blue\nicon: rocket\nprofile: claude\ndescription: Does the \
             thing\nagent-description: Does it well\n---\n\n",
        ),
    )]);
    assert_eq!(messages(&linter(&p).definition("j")), vec![]);
}

#[test]
fn a_colour_purlis_cannot_draw_is_a_warning_and_a_hex_is_none() {
    // #1460: the persona view and the fix said it; lint, the one check a person runs over
    // every persona, did not.
    let with = |colour: &str| clean("c").replace("---\n\n", &format!("color: {colour}\n---\n\n"));
    let p = plane(&[("c", &with("#1a2b3c"))]);
    assert_eq!(messages(&linter(&p).definition("c")), vec![]);
    let p = plane(&[("c", &with("chartreuse"))]);
    assert_eq!(
        messages(&linter(&p).definition("c")),
        vec![(
            Level::Warn,
            "`color: chartreuse` is not a colour purlis draws, so this persona keeps the \
             colour of its name. Use red, orange, yellow, green, teal, blue, purple, pink or \
             #rrggbb"
        )]
    );
    // A name Claude Code gave a sub-agent's colour: the fix rewrites it, and lint says so.
    let p = plane(&[("c", &with("cyan"))]);
    assert_eq!(
        messages(&linter(&p).definition("c")),
        vec![(
            Level::Warn,
            "`color: cyan` is not a colour purlis draws, so this persona keeps the colour of \
             its name. `purlis doctor --fix persona-agents` rewrites it to `teal`"
        )]
    );
}

#[test]
fn a_model_is_a_finding_unless_it_names_the_profile_a_chat_starts_on() {
    let with = |extra: &str| clean("m").replace("---\n\n", &format!("{extra}\n---\n\n"));
    // A built-in profile's name, and no `profile:` line: it is what a chat starts on.
    let p = plane(&[("m", &with("model: codex"))]);
    assert_eq!(messages(&linter(&p).definition("m")), vec![]);
    // A model's name is no profile's.
    let p = plane(&[("m", &with("model: opus"))]);
    assert_eq!(
        messages(&linter(&p).definition("m")),
        vec![(
            Level::Warn,
            "`model: opus` is no longer read: it named a model for the generated sub-agent, \
             and this project offers no profile called that on this machine. A chat as this \
             persona runs on the asking chat's profile, with that profile's model and its \
             cost, not on `opus`. Name the profile this persona's chats start on with \
             `profile:`, or delete the line"
        )]
    );
    // And a `profile:` line answers instead of any `model:`.
    let p = plane(&[("m", &with("profile: claude\nmodel: codex"))]);
    assert_eq!(
        messages(&linter(&p).definition("m")),
        vec![(
            Level::Warn,
            "`model: codex` is no longer read: this persona's profile is named with \
             `profile:`. Delete the line"
        )]
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
                .starts_with("frontmatter key 'modell' is not read by purlis")
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
    assert_eq!(issues.len(), 3, "{issues:?}");
    assert!(
        issues[0]
            .message
            .starts_with("mcp: server name 'bad name' is refused")
    );
    assert_eq!(
        issues[1].message,
        "mcp: server(s) g declare `secrets` or `secret_files` but this persona names no vault — add `vault:` or drop the declaration"
    );
    assert_eq!(issues[2].level, Level::Warn);
}

#[test]
fn a_persona_s_servers_are_said_where_a_chat_gets_them_and_an_unapproved_one_how_to_approve() {
    // #1451, D-1451-17: a chat as the persona is started with them on Claude Code.
    let p = plane(&[
        ("m", &clean("m").replace("vault: none", "vault: mv")),
        (
            "kid",
            "---\nname: kid\nextends: m\nrole: Kid\ndelegate-when: nights\n---\n\n# Kid\n",
        ),
    ]);
    p.write(
        "personas/m/mcp.json",
        r#"{"mcpServers": {"g": {"command": "g", "secrets": {"T": "t"}}}}"#,
    );
    assert_eq!(
        messages(&linter(&p).definition("m")),
        vec![
            (
                Level::Warn,
                "mcp: 'g' declares a credential this machine has not approved, so it is \
                 withheld from this persona's chats. Read the command and approve it in a \
                 terminal with `purlis persona approve-mcp --persona m`, or drop \
                 `secrets`/`secret_files` from `mcp.json` if it should hold no credential"
            ),
            (
                Level::Warn,
                "mcp: `mcp.json` is read, and it declares 1 MCP server(s). A chat as this \
                 persona is started with them on Claude Code; on Codex and opencode they are \
                 not started"
            ),
        ]
    );
    // The child inherits the server and its withholding, and has no file of its own.
    let kid = linter(&p).definition("kid");
    assert_eq!(kid.len(), 1, "{kid:?}");
    assert!(
        kid[0]
            .message
            .contains("`purlis persona approve-mcp --persona kid`")
    );
}

fn skill(home: &Path, rel: &str, front: &str) {
    let path = home.join(".claude").join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, format!("---\n{front}\n---\n\nBody.\n")).unwrap();
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
            "purlis names `sp:gone` — not an installed skill (renamed/removed upstream?); fix it or pin the marketplace"
        )]
    );
}

#[test]
fn a_generated_sub_agent_that_is_still_there_is_a_warning_and_any_other_file_is_not() {
    let p = plane(&[("a", &clean("a"))]);
    let l = linter(&p);
    // None there: nothing to say. purlis generates none, so a missing one is no finding.
    assert_eq!(l.leftover_agent("a"), vec![]);
    p.write(".claude/agents/a.md", &generated_agent("a"));
    assert_eq!(
        messages(&l.leftover_agent("a")),
        vec![(
            Level::Warn,
            ".claude/agents/a.md is a sub-agent purlis generated before a persona ran as its \
             own chat. purlis no longer writes or reads it — remove it with `purlis doctor \
             --fix persona-agents`"
        )]
    );
    p.write(".claude/agents/a.md", "hand-written\n");
    assert_eq!(l.leftover_agent("a"), vec![]);
}

fn run(p: &Plane, name: Option<&str>, only: Option<&str>) -> (u8, Heard) {
    let mut heard = Heard::default();
    let rc = lint_command(&linter(p), name, only, &mut heard.sink());
    (rc, heard)
}

#[test]
fn the_command_exits_one_on_an_error_and_zero_on_warnings_alone() {
    let p = plane(&[("ok", &clean("ok"))]);
    p.write(".claude/agents/ok.md", &generated_agent("ok"));
    // A leftover generated sub-agent is a warning; the fixture's own `steward` has none.
    let (rc, heard) = run(&p, None, None);
    assert_eq!(rc, 0);
    assert_eq!(
        heard.err,
        "! ok: .claude/agents/ok.md is a sub-agent purlis generated before a persona ran as \
         its own chat. purlis no longer writes or reads it — remove it with `purlis doctor \
         --fix persona-agents`\n✓ steward: ok\n"
    );
    p.write(
        "personas/ok/persona.md",
        &clean("ok").replace("---\n\n", "uses: ghost\n---\n\n"),
    );
    std::fs::remove_file(p.path(".claude/agents/ok.md")).unwrap();
    let (rc, heard) = run(&p, Some("ok"), None);
    assert_eq!(rc, 1);
    assert_eq!(
        heard.err,
        "✗ ok: uses: 'ghost' — no such persona (dangling)\n\
         ✗ 1 error(s) — dangling reuse or unloadable persona.\n"
    );
}

#[test]
fn only_narrows_to_one_finding_and_makes_it_an_error() {
    let p = plane(&[]);
    p.write(".claude/agents/steward.md", &generated_agent("steward"));
    let (rc, heard) = run(&p, Some("steward"), Some("sub-agent"));
    assert_eq!(rc, 1);
    assert!(
        heard
            .err
            .starts_with("✗ steward: .claude/agents/steward.md is a sub-agent purlis generated"),
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

#[test]
fn a_persona_named_charter_is_an_error_because_the_name_is_reserved() {
    let p = plane(&[("charter", &clean("charter"))]);
    assert_eq!(
        messages(&linter(&p).definition("charter")),
        vec![(
            Level::Error,
            "the persona name 'charter' is reserved — `charter/<id>` names purlis's own \
             actions, so this one would pass as a built-in. Rename the persona"
        )]
    );
}

#[test]
fn what_of_wants_is_not_offered_is_said_and_a_clean_line_says_nothing() {
    // #1502: the line grants nothing, and a name in it that the question will not show is
    // said, so nobody takes it for one that is offered.
    let p = plane(&[
        (
            "ok",
            &clean("ok").replace("---\n\n", "wants: [peer]\n---\n\n"),
        ),
        ("peer", &clean("peer")),
        (
            "wide",
            &clean("wide").replace("---\n\n", "wants: [peer, ghost, wide, *]\n---\n\n"),
        ),
    ]);
    assert_eq!(messages(&linter(&p).definition("ok")), vec![]);
    assert_eq!(
        messages(&linter(&p).definition("wide")),
        vec![
            (
                Level::Warn,
                "wants names 'ghost', which is not a persona of this project, so it is not \
                 offered"
            ),
            (
                Level::Warn,
                "wants names the persona itself, which is ignored: a chat dispatches to its own \
                 persona with no grant"
            ),
            (
                Level::Warn,
                "wants names `*`, which is ignored: any persona is granted in Settings only, \
                 and `wants` grants nothing"
            ),
        ]
    ); // A child that inherits the line is told whose line it is.
    p.write(
        "personas/kid/persona.md",
        "---\nname: kid\nextends: wide\nrole: Kid\nvault: none\ndelegate-when: nights\n---\n\n# Kid\n",
    );
    let kid = linter(&p).definition("kid");
    assert_eq!(
        messages(&kid)[0],
        (
            Level::Warn,
            "wants names 'ghost', which is not a persona of this project, so it is not offered, \
             in the line it inherits from 'wide'"
        )
    );
}
