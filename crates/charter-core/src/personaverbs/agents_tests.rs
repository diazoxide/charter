//! `charter persona sync-agents` against the recorded scenarios: each `persona-sync-agents-…`
//! row of `tests/fixtures/recorded/behaviour.jsonl` replayed in-crate — its plane, its
//! options, and every line and generated file the Python charter answered with — and the
//! paths those rows do not reach: the question asked on a terminal, a worktree, and the
//! agents a sync must leave alone.

use std::path::Path;

use super::*;
use crate::personaverbs::tests_plane::{Heard, OPS, OPS_APPROVED, Plane, expected, recorded};

/// Every recorded `sync-agents` scenario, by name.
const RECORDED: [&str; 11] = [
    "persona-sync-agents-approve-mcp-dry-run-records-nothing",
    "persona-sync-agents-approve-mcp-off-a-terminal-refuses",
    "persona-sync-agents-approve-mcp-yes-records-each-server",
    "persona-sync-agents-for-one-persona",
    "persona-sync-agents-names-each-forge-a-plane-declares",
    "persona-sync-agents-prunes-stale-agents-and-leaves-hand-written-ones",
    "persona-sync-agents-refuses-a-persona-the-plane-does-not-define",
    "persona-sync-agents-renders-every-field-a-definition-declares",
    "persona-sync-agents-withholds-nothing-from-a-frontmatter-it-cannot-read",
    "persona-sync-agents-wraps-an-approved-server-in-its-vault",
    "persona-sync-agents-writes-each-finished-persona-and-skips-a-draft",
];

fn run(plane: &Plane, cwd: &Path, options: &Options, ask: Option<Ask>) -> (u8, Heard) {
    let mut heard = Heard::default();
    let rc = sync_agents(plane.root(), cwd, options, ask, &mut heard.sink());
    (rc, heard)
}

/// The recorded run's options, read off its argv.
fn options_of(args: &[String]) -> Options<'_> {
    assert_eq!(args[..2], ["persona", "sync-agents"]);
    let mut options = Options::default();
    let mut rest = args[2..].iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--persona" => options.persona = rest.next().map(String::as_str),
            "--approve-mcp" => options.approve_mcp = true,
            "--yes" => options.yes = true,
            "--dry-run" => options.dry_run = true,
            other => panic!("an argument the replay does not know: {other}"),
        }
    }
    options
}

#[test]
fn every_recorded_sync_agents_scenario_says_and_writes_what_python_did() {
    for name in RECORDED {
        let row = recorded(name);
        let plane = Plane::replay(&row);
        let args: Vec<String> = serde_json::from_value(row["args"].clone()).unwrap();
        // The recording ran with stdin not a terminal: there was nothing to ask on.
        let (rc, heard) = run(&plane, plane.root(), &options_of(&args), None);
        assert_eq!(
            u64::from(rc),
            row["expect"]["exit"].as_u64().unwrap(),
            "{name}"
        );
        assert_eq!(heard.out, expected(&row, "stdout"), "{name}");
        assert_eq!(heard.err, expected(&row, "stderr"), "{name}");
        let set = row["expect"]["tree"]["set"].as_object().unwrap();
        for (path, file) in set {
            match file["kind"].as_str() {
                Some("file") => assert_eq!(
                    plane.read(path),
                    file["text"].as_str().unwrap(),
                    "{name}: {path}"
                ),
                Some("dir") => assert!(plane.path(path).is_dir(), "{name}: {path}"),
                other => panic!("{name}: {path} of kind {other:?}"),
            }
        }
        for path in row["expect"]["tree"]["remove"].as_array().unwrap() {
            let path = path.as_str().unwrap();
            assert!(!plane.path(path).exists(), "{name}: {path} is still there");
        }
        // What the run neither changed nor removed is as the scenario left it.
        for (path, file) in row["start"]["set"].as_object().unwrap() {
            let rel = path.strip_prefix("plane/").unwrap();
            let removed = row["expect"]["tree"]["remove"]
                .as_array()
                .unwrap()
                .iter()
                .any(|p| p == rel);
            if file["kind"] == "file" && !removed && !set.contains_key(rel) {
                assert_eq!(
                    plane.read(rel),
                    file["text"].as_str().unwrap(),
                    "{name}: {rel}"
                );
            }
        }
        // Nothing is recorded as approved but by an approval.
        if !set.contains_key(".charter/mcp-approved.json")
            && !row["start"]["set"]
                .as_object()
                .unwrap()
                .contains_key("plane/.charter/mcp-approved.json")
        {
            assert!(!plane.path(".charter/mcp-approved.json").exists(), "{name}");
        }
    }
}

/// The plane of `persona-sync-agents-renders-every-field-a-definition-declares`.
fn ops_plane() -> Plane {
    Plane::replay(&recorded(
        "persona-sync-agents-renders-every-field-a-definition-declares",
    ))
}

fn approve_options() -> Options<'static> {
    Options {
        persona: Some("ops"),
        approve_mcp: true,
        ..Options::default()
    }
}

#[test]
fn on_a_terminal_each_server_is_asked_about_and_only_a_yes_is_recorded() {
    let plane = ops_plane();
    let mut asked: Vec<String> = Vec::new();
    let mut ask = |prompt: &str| {
        asked.push(prompt.to_string());
        Some(prompt.contains("grafana"))
    };
    let (rc, heard) = run(&plane, plane.root(), &approve_options(), Some(&mut ask));
    assert_eq!(rc, 0);
    assert_eq!(
        asked,
        [
            "    approve ops/grafana? [y/N] ",
            "    approve ops/gsc? [y/N] "
        ]
    );
    assert!(
        heard
            .err
            .contains("•     skipped ops/gsc — the vault stays withheld\n"),
        "{}",
        heard.err
    );
    let grafana = mcp::credentialed(plane.root(), "ops")[0]
        .fingerprint
        .clone()
        .unwrap();
    assert_eq!(
        mcp::approved(&plane.state(), "ops"),
        [grafana].into_iter().collect()
    );
    let agent = plane.read(".claude/agents/ops.md");
    assert!(agent.contains(r#"{"grafana": {"type": "stdio", "command": "charter""#));
    assert!(agent.contains(r#"{"gsc": {"type": "stdio", "command": "uvx""#));
}

#[test]
fn an_interrupted_question_records_nothing_writes_nothing_and_exits_130() {
    let plane = ops_plane();
    let mut ask = |_: &str| None;
    let (rc, heard) = run(&plane, plane.root(), &approve_options(), Some(&mut ask));
    assert_eq!(rc, 130);
    assert!(
        heard
            .err
            .ends_with("✗ interrupted — nothing recorded for ops\n"),
        "{}",
        heard.err
    );
    assert!(!mcp::approvals_path(&plane.state()).exists());
    assert!(!plane.path(".claude/agents/ops.md").exists());
}

#[test]
fn yes_approves_without_asking_even_on_a_terminal() {
    let plane = ops_plane();
    let mut ask = |prompt: &str| -> Option<bool> { panic!("asked {prompt}") };
    let options = Options {
        yes: true,
        ..approve_options()
    };
    let (rc, _) = run(&plane, plane.root(), &options, Some(&mut ask));
    assert_eq!(rc, 0);
    assert_eq!(
        mcp::approved(&plane.state(), "ops"),
        OPS_APPROVED.map(String::from).into_iter().collect()
    );
}

#[test]
fn a_dry_run_on_a_terminal_shows_the_lines_and_neither_asks_nor_records() {
    let plane = ops_plane();
    let mut ask = |prompt: &str| -> Option<bool> { panic!("asked {prompt}") };
    let options = Options {
        dry_run: true,
        ..approve_options()
    };
    let (rc, heard) = run(&plane, plane.root(), &options, Some(&mut ask));
    assert_eq!(rc, 0);
    assert!(
        heard
            .err
            .contains("•   --dry-run: nothing approved. Re-run without it to be asked.\n")
    );
    assert!(!mcp::approvals_path(&plane.state()).exists());
}

#[test]
fn a_credentialed_server_charter_cannot_show_is_never_approved_and_says_why() {
    let plane = ops_plane();
    plane.write(
        "personas/ops/mcp.json",
        r#"{"mcpServers": {"blank": {"command": "   ", "secrets": {"T": "t"}}}}"#,
    );
    let options = Options {
        yes: true,
        ..approve_options()
    };
    let (rc, heard) = run(&plane, plane.root(), &options, None);
    assert_eq!(rc, 0);
    assert!(
        heard.err.starts_with(
            "!   cannot approve ops/blank — (charter cannot show this entry in full — nothing \
             to approve)\n"
        ),
        "{}",
        heard.err
    );
    assert!(heard.err.contains(
        "•   ops/blank → (charter cannot show this entry in full — nothing to approve)\n"
    ));
    assert!(mcp::approved(&plane.state(), "ops").is_empty());
}

#[test]
fn a_persona_without_delegate_when_is_described_by_its_role_or_its_name_and_an_own_description_wins()
 {
    let plane = Plane::fixture("minimal");
    plane.write("personas/ops/persona.md", "---\nname: ops\n---\n\nOps.\n");
    plane.write(
        "personas/doc/persona.md",
        "---\nrole: Docs\ndescription: Writes \"the\" docs\n---\n",
    );
    let text = |name: &str| {
        let def = super::super::resolve(plane.root(), name).unwrap();
        render(plane.root(), &plane.state(), name, &def)
    };
    assert!(text("ops").starts_with(
        "---\nname: ops\ndescription: \"The ops persona. Delegate ops tasks to it. Holds no \
         credentials of its own.\"\n---\n"
    ));
    assert!(
        text("doc").starts_with("---\nname: doc\ndescription: \"Writes \\\"the\\\" docs\"\n---\n")
    );
}

#[test]
fn only_worktree_isolation_isolates() {
    let meta = |v: &str| BTreeMap::from([("dispatch-isolation".to_string(), v.to_string())]);
    assert!(isolates(&meta(" worktree ")));
    assert!(!isolates(&meta("none")));
    assert!(!isolates(&BTreeMap::new()));
}

#[test]
fn a_grant_already_named_a_blank_denylist_and_a_persona_using_itself_add_nothing() {
    let plane = ops_plane();
    plane.write(
        "personas/ops/persona.md",
        &OPS.replace("agent-tools: Read, Bash", "agent-tools: Read, mcp__status")
            .replace("disallowed-tools: WebFetch", "disallowed-tools:  ")
            .replace("uses: devops, steward", "uses: ops, , devops"),
    );
    let def = super::super::resolve(plane.root(), "ops").unwrap();
    let text = render(plane.root(), &plane.state(), "ops", &def);
    assert!(
        text.contains("\ntools: Read, mcp__status, mcp__grafana__*, mcp__gsc__*\n"),
        "{text}"
    );
    assert!(!text.contains("disallowedTools"), "{text}");
    assert!(
        text.contains("- **You may also use these personas: `devops`.**"),
        "{text}"
    );
    // With no servers, the allowlist is the persona's own.
    std::fs::remove_file(plane.path("personas/ops/mcp.json")).unwrap();
    let text = render(plane.root(), &plane.state(), "ops", &def);
    assert!(text.contains("\ntools: Read, mcp__status\n"), "{text}");
    assert!(!text.contains("mcpServers"), "{text}");
}

#[test]
fn a_key_written_once_in_its_own_spelling_is_no_issue() {
    let plane = ops_plane();
    assert!(key_issues(plane.root(), "ops").is_empty());
    assert!(key_issues(plane.root(), "ghost").is_empty());
}

#[test]
fn a_draft_leaves_a_hand_written_agent_at_its_name_alone() {
    let plane = Plane::fixture("daily");
    plane.write(".claude/agents/devops.md", "---\nname: devops\n---\nmine\n");
    let (rc, _) = run(&plane, plane.root(), &Options::default(), None);
    assert_eq!(rc, 0);
    assert_eq!(
        plane.read(".claude/agents/devops.md"),
        "---\nname: devops\n---\nmine\n"
    );
    assert!(is_generated(&plane.path(".claude/agents/steward.md")));
    assert!(!is_generated(&plane.path(".claude/agents/devops.md")));
    assert!(!is_generated(&plane.path(".claude/agents/ghost.md")));
}

#[test]
fn one_persona_prunes_nothing_and_only_md_files_are_ever_pruned() {
    let plane = Plane::fixture("daily");
    let stale = format!("---\nname: old\n---\n<!-- {MARKER} from personas/old/persona.md -->\n");
    plane.write(".claude/agents/old.md", &stale);
    plane.write(".claude/agents/notes.txt", &stale);
    let one = Options {
        persona: Some("steward"),
        ..Options::default()
    };
    let (rc, heard) = run(&plane, plane.root(), &one, None);
    assert_eq!(rc, 0);
    assert!(!heard.err.contains("Removed"), "{}", heard.err);
    assert!(plane.path(".claude/agents/old.md").exists());
    let (rc, heard) = run(&plane, plane.root(), &Options::default(), None);
    assert_eq!(rc, 0);
    assert!(
        heard
            .err
            .contains("• Removed stale generated agents: old\n")
    );
    assert!(!plane.path(".claude/agents/old.md").exists());
    assert_eq!(plane.read(".claude/agents/notes.txt"), stale);
}

#[test]
fn an_empty_persona_option_syncs_every_persona() {
    let plane = Plane::fixture("daily");
    let empty = Options {
        persona: Some(""),
        ..Options::default()
    };
    let (rc, heard) = run(&plane, plane.root(), &empty, None);
    assert_eq!(rc, 0);
    assert_eq!(
        heard.err,
        expected(
            &recorded("persona-sync-agents-writes-each-finished-persona-and-skips-a-draft"),
            "stderr"
        )
    );
}

#[test]
fn a_plane_with_no_personas_has_nothing_to_sync() {
    let plane = Plane::fixture("minimal");
    std::fs::remove_dir_all(plane.path("personas/steward")).unwrap();
    let (rc, heard) = run(&plane, plane.root(), &Options::default(), None);
    assert_eq!(rc, 0);
    assert_eq!(
        heard.err,
        "• No personas to sync. Add one first: write personas/<name>/persona.md.\n"
    );
}

/// A linked worktree of `plane`, as git lays one out: a `.git` FILE naming the plane's
/// `.git/worktrees/<name>`. Only the layout is read, so no git is run.
fn worktree(plane: &Plane) -> tempfile::TempDir {
    let tree = tempfile::tempdir().unwrap();
    let gitdir = plane.path(".git/worktrees/wt");
    std::fs::create_dir_all(&gitdir).unwrap();
    std::fs::write(
        tree.path().join(".git"),
        format!("gitdir: {}\n", gitdir.display()),
    )
    .unwrap();
    tree
}

#[test]
fn from_a_worktree_the_agents_are_generated_into_that_worktree_and_the_plane_says_untouched() {
    let plane = Plane::fixture("minimal");
    let tree = worktree(&plane);
    let persona = tree.path().join("personas/steward");
    std::fs::create_dir_all(&persona).unwrap();
    std::fs::copy(
        plane.path("personas/steward/persona.md"),
        persona.join("persona.md"),
    )
    .unwrap();
    let (rc, heard) = run(&plane, tree.path(), &Options::default(), None);
    assert_eq!(rc, 0);
    let written = tree.path().canonicalize().unwrap().join(".claude/agents");
    assert_eq!(
        heard.err,
        format!(
            "✓ Synced 1 persona sub-agent(s) → .claude/agents/ (steward)\n\
             •   written into the worktree you ran from: {}\n\
             •   the plane's own copy ({}) is untouched — it updates when this branch merges.\n\
             •   invoke 'steward' via the Agent/Task tool (subagent_type: steward)\n\
             • New/changed agents load on the next Claude Code session (restart to use now).\n",
            written.display(),
            plane.path(".claude/agents").display()
        )
    );
    assert!(written.join("steward.md").is_file());
    assert!(!plane.path(".claude/agents").exists());
}

#[test]
fn a_worktree_without_personas_of_a_plane_with_them_is_refused_and_nothing_is_written() {
    let plane = Plane::fixture("minimal");
    let tree = worktree(&plane);
    let (rc, heard) = run(&plane, tree.path(), &Options::default(), None);
    assert_eq!(rc, 1);
    let shown = tree.path().canonicalize().unwrap();
    assert_eq!(
        heard.err,
        format!(
            "✗ {} is a worktree of the plane at {}, and carries no `personas/`. Generated \
             sub-agents belong to the tree that holds their sources, and this one holds none — \
             nothing was written. Run this in {}, or check out a branch that carries \
             `personas/`.\n",
            shown.display(),
            plane.root().display(),
            plane.root().display()
        )
    );
    assert!(!tree.path().join(".claude").exists());
    assert!(!plane.path(".claude/agents").exists());
    // A plane with no personas either has nothing for the worktree to lack.
    std::fs::remove_dir_all(plane.path("personas")).unwrap();
    let (rc, heard) = run(&plane, tree.path(), &Options::default(), None);
    assert_eq!(rc, 0);
    assert!(
        heard.err.starts_with("• No personas to sync."),
        "{}",
        heard.err
    );
}

/// Set on the child [`on_a_terminal_confirm_asks_and_only_y_or_yes_is_a_yes`] re-runs itself
/// as, whose stdin is a terminal.
#[cfg(unix)]
const ON_A_TERMINAL: &str = "PERSONAVERBS_CONFIRM_ON_A_TERMINAL";

#[cfg(unix)]
#[test]
fn on_a_terminal_confirm_asks_and_only_y_or_yes_is_a_yes() {
    use std::io::{Read, Write};

    if std::env::var_os(ON_A_TERMINAL).is_some() {
        let mut confirm = confirm_on_terminal().expect("stdin is a terminal here");
        let answers = [confirm("first? "), confirm("second? "), confirm("third? ")];
        println!("ANSWERS {answers:?}");
        return;
    }
    let pair = crate::forklock::while_a_terminal_is_opened(|| {
        portable_pty::native_pty_system().openpty(portable_pty::PtySize::default())
    })
    .unwrap();
    let mut command = portable_pty::CommandBuilder::new(std::env::current_exe().unwrap());
    let me = format!(
        "{}::on_a_terminal_confirm_asks_and_only_y_or_yes_is_a_yes",
        module_path!().trim_start_matches("charter_core::")
    );
    command.args(["--exact", &me, "--nocapture", "--test-threads=1"]);
    command.env(ON_A_TERMINAL, "1");
    let mut child = pair.slave.spawn_command(command).unwrap();
    drop(pair.slave);
    let mut reader = pair.master.try_clone_reader().unwrap();
    let said = std::thread::spawn(move || {
        let mut all = Vec::new();
        let mut buf = [0u8; 4096];
        while let Ok(n) = reader.read(&mut buf) {
            if n == 0 {
                break;
            }
            all.extend_from_slice(&buf[..n]);
        }
        String::from_utf8_lossy(&all).into_owned()
    });
    // A line discipline in canonical mode hands the child one line per read, and `^D` at the
    // start of a line is end of file.
    let mut writer = pair.master.take_writer().unwrap();
    writer.write_all(b" YES \nno\n\x04").unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if std::time::Instant::now() > deadline {
            let _ = child.kill();
            panic!("the child on a terminal did not finish");
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    };
    drop(writer);
    drop(pair.master);
    let said = said.join().unwrap();
    assert!(status.success(), "{said}");
    assert!(said.contains("test result: ok. 1 passed"), "{said}");
    assert!(said.contains("first? "), "{said}");
    assert!(
        said.contains("ANSWERS [Some(true), Some(false), Some(false)]"),
        "{said}"
    );
}
