//! `persona-agents` against a project on disk: what it removes, what it rewrites, what it
//! leaves and says it left, and that a second run changes nothing.

use std::path::{Path, PathBuf};

use super::*;

struct Project {
    _dir: tempfile::TempDir,
    root: PathBuf,
}

fn project() -> Project {
    let dir = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(dir.path()).unwrap();
    Project { _dir: dir, root }
}

impl Project {
    fn write(&self, rel: &str, text: &str) {
        let path = self.root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    fn read(&self, rel: &str) -> String {
        std::fs::read_to_string(self.root.join(rel)).unwrap()
    }

    fn has(&self, rel: &str) -> bool {
        self.root.join(rel).exists()
    }

    /// The fix, on a machine that offers `offered`, each a profile that travels with the
    /// project, and where git can give every file back.
    fn fix(&self, offered: &[&str]) -> Vec<String> {
        self.fix_where(offered, offered, &|_| true)
    }

    fn fix_where(
        &self,
        offered: &[&str],
        travels: &[&str],
        restorable: &dyn Fn(&str) -> bool,
    ) -> Vec<String> {
        let owned = |names: &[&str]| -> Vec<String> {
            names.iter().map(|name| (*name).to_owned()).collect()
        };
        let facts = Facts {
            offered: &owned(offered),
            travels: &owned(travels),
            restorable,
        };
        match apply_with(&self.root, &facts) {
            Fixed::Ran {
                said,
                complete: true,
            } => said,
            other => panic!("{other:?}"),
        }
    }
}

/// A sub-agent file as `persona sync-agents` wrote it, under the old spelling of the marker.
fn generated(name: &str) -> String {
    let marker = crate::names::SYNC_AGENTS_MARKER
        .spellings()
        .last()
        .expect("an old spelling");
    format!(
        "---\nname: {name}\ndescription: \"The {name} persona.\"\n---\n<!-- {marker} from \
         personas/{name}/persona.md — edit the persona, not this file. -->\n\nThis sub-agent \
         acts as the **{name}** persona — Role — in an\nisolated context. Adopt the charter \
         below as your role.\n\n# {name}\n"
    )
}

const HAND: &str = "---\nname: runner\ndescription: runs the suite\n---\n\nRun it.\n";

fn tree(root: &Path) -> Vec<String> {
    fn walk(dir: &Path, root: &Path, out: &mut Vec<String>) {
        let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
            .unwrap()
            .flatten()
            .map(|entry| entry.path())
            .collect();
        entries.sort();
        for path in entries {
            if path.is_dir() {
                walk(&path, root, out);
            } else {
                out.push(format!(
                    "{}\n{}",
                    path.strip_prefix(root).unwrap().display(),
                    std::fs::read_to_string(&path).unwrap()
                ));
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out
}

#[test]
fn generated_files_go_and_every_other_file_is_named_as_left_alone() {
    let p = project();
    p.write("personas/ops/persona.md", "---\nrole: Ops\n---\n\n# Ops\n");
    p.write(".claude/agents/ops.md", &generated("ops"));
    // A generated file whose persona is long gone is still purlis's.
    p.write(".claude/agents/gone.md", &generated("gone"));
    p.write(".claude/agents/runner.md", HAND);
    let edited = generated("qa").replace("---\n<!--", "---\nA note of my own.\n<!--");
    p.write(".claude/agents/qa.md", &edited);
    p.write(".claude/agents/notes.txt", "not an agent\n");

    let said = p.fix(&[]);

    assert!(!p.has(".claude/agents/ops.md"));
    assert!(!p.has(".claude/agents/gone.md"));
    assert_eq!(p.read(".claude/agents/runner.md"), HAND);
    assert_eq!(p.read(".claude/agents/qa.md"), edited);
    assert_eq!(p.read(".claude/agents/notes.txt"), "not an agent\n");
    assert_eq!(
        said,
        [
            "✓ removed .claude/agents/gone.md (purlis generated it).",
            "✓ removed .claude/agents/ops.md (purlis generated it).",
            "• left alone .claude/agents/qa.md: it carries purlis's marker, but not as purlis \
             writes it, so somebody edited it by hand. Remove it yourself if it is not wanted.",
            "• left alone .claude/agents/runner.md: hand-written (it does not carry purlis's \
             marker).",
            "✓ removed 2 generated sub-agent file(s) and rewrote 0 persona key(s). Nothing is \
             committed: the change is in the working tree for the project's next save. To take \
             it back before then: `git restore -- .claude/agents personas`.",
        ]
    );
}

#[cfg(unix)]
#[test]
fn a_link_in_the_agents_folder_is_never_followed_or_removed() {
    let p = project();
    let outside = tempfile::tempdir().unwrap();
    let target = outside.path().join("ops.md");
    std::fs::write(&target, generated("ops")).unwrap();
    std::fs::create_dir_all(p.root.join(".claude/agents")).unwrap();
    std::os::unix::fs::symlink(&target, p.root.join(".claude/agents/ops.md")).unwrap();

    let said = p.fix(&[]);

    assert!(target.exists(), "the file the link names is not purlis's");
    assert!(p.root.join(".claude/agents/ops.md").is_symlink());
    assert_eq!(
        said[0],
        "• left alone .claude/agents/ops.md: it is a link or is reached through one, and \
         purlis wrote no such file."
    );
}

#[test]
fn model_becomes_profile_only_where_the_project_offers_a_profile_of_that_name() {
    let p = project();
    p.write(
        "personas/ops/persona.md",
        "---\nname: ops\nmodel: codex\nrole: Ops\n---\n\n# Ops\n\nmodel: codex stays in prose.\n",
    );
    p.write("personas/qa/persona.md", "---\nmodel: opus\n---\n");
    p.write(
        "personas/web/persona.md",
        "---\nprofile: claude\nmodel: codex\n---\n",
    );

    let said = p.fix(&["claude", "codex"]);

    assert_eq!(
        p.read("personas/ops/persona.md"),
        "---\nname: ops\nprofile: codex\nrole: Ops\n---\n\n# Ops\n\nmodel: codex stays in \
         prose.\n"
    );
    assert_eq!(p.read("personas/qa/persona.md"), "---\nmodel: opus\n---\n");
    assert_eq!(
        p.read("personas/web/persona.md"),
        "---\nprofile: claude\nmodel: codex\n---\n"
    );
    assert_eq!(
        said,
        [
            "✓ personas/ops/persona.md: `model: codex` is now `profile: codex`.",
            "• personas/qa/persona.md: `model: opus` is no longer read: it named a model for \
             the generated sub-agent, and this project offers no profile called that on this \
             machine. A chat as this persona runs on the asking chat's profile, with that \
             profile's model and its cost, not on `opus`. Name the profile this persona's \
             chats start on with `profile:`, or delete the line.",
            "• personas/web/persona.md: `model: codex` is no longer read: this persona's \
             profile is named with `profile:`. Delete the line.",
            "✓ removed 0 generated sub-agent file(s) and rewrote 1 persona key(s). Nothing is \
             committed: the change is in the working tree for the project's next save. To take \
             it back before then: `git restore -- .claude/agents personas`.",
        ]
    );
    // What was written is what the profile reader reads back.
    assert_eq!(
        crate::personaprofile::named_by(&p.root, "ops")
            .profile
            .as_deref(),
        Some("codex")
    );
}

#[test]
fn a_model_is_not_carried_over_where_a_parent_s_profile_line_already_answers() {
    // A chat of `kid` starts on `claude` today: the chain's `profile:` line is read and its
    // own `model:` is not. Rewriting the line would move its chats to another harness.
    let p = project();
    p.write("personas/base/persona.md", "---\nprofile: claude\n---\n");
    let kid = "---\nextends: base\nmodel: codex\n---\n";
    p.write("personas/kid/persona.md", kid);
    let out = "---\nextends: quiet\nmodel: codex\n---\n";
    p.write("personas/quiet/persona.md", "---\nprofile: none\n---\n");
    p.write("personas/out/persona.md", out);

    let said = p.fix(&["claude", "codex"]);

    assert_eq!(p.read("personas/kid/persona.md"), kid);
    assert_eq!(p.read("personas/out/persona.md"), out);
    assert_eq!(
        crate::personaprofile::named_by(&p.root, "kid")
            .profile
            .as_deref(),
        Some("claude")
    );
    for who in ["kid", "out"] {
        assert!(
            said.contains(&format!(
                "• personas/{who}/persona.md: `model: codex` is no longer read: this \
                 persona's profile is named with `profile:`. Delete the line."
            )),
            "{who}: {said:#?}"
        );
    }
    assert!(
        said.last()
            .unwrap()
            .starts_with("✓ no generated persona sub-agent file")
    );
}

#[test]
fn a_harness_colour_purlis_spells_otherwise_is_rewritten_and_any_other_is_reported() {
    let p = project();
    p.write("personas/a/persona.md", "---\ncolor: cyan\n---\n");
    p.write(
        "personas/b/persona.md",
        "---\ncolor: Magenta\r\nrole: B\r\n---\n",
    );
    p.write("personas/c/persona.md", "---\ncolor: blue\n---\n");
    p.write("personas/d/persona.md", "---\ncolor: chartreuse\n---\n");

    let said = p.fix(&[]);

    assert_eq!(p.read("personas/a/persona.md"), "---\ncolor: teal\n---\n");
    assert_eq!(
        p.read("personas/b/persona.md"),
        "---\ncolor: pink\r\nrole: B\r\n---\n"
    );
    assert_eq!(p.read("personas/c/persona.md"), "---\ncolor: blue\n---\n");
    assert_eq!(
        p.read("personas/d/persona.md"),
        "---\ncolor: chartreuse\n---\n"
    );
    assert_eq!(
        said[..3],
        [
            "✓ personas/a/persona.md: `color: cyan` is now `color: teal`.",
            "✓ personas/b/persona.md: `color: Magenta` is now `color: pink`.",
            "• personas/d/persona.md: `color: chartreuse` is not a colour purlis draws, so \
             this persona keeps the colour of its name. Use red, orange, yellow, green, teal, \
             blue, purple, pink or #rrggbb.",
        ]
    );
    assert_eq!(
        crate::personamark::mark(&p.root, "a").colour.as_deref(),
        Some("teal")
    );
}

#[test]
fn a_key_nothing_reads_is_reported_with_what_widened_and_its_line_is_kept() {
    let p = project();
    let ops = "---\nrole: Ops\nagent-tools: Read, Bash\ndisallowed-tools: WebFetch\nskills: \
               deploy\nmemory: project\ndispatch-isolation: worktree\ndescription: Runs the \
               clusters\n---\n\n# Ops\n";
    p.write("personas/ops/persona.md", ops);
    p.write(
        "personas/ops/mcp.json",
        r#"{"mcpServers": {"status": {"type": "http", "url": "https://s.example.com/mcp"}}}"#,
    );
    p.write("personas/kid/persona.md", "---\nextends: ops\n---\n");

    let said = p.fix(&[]);

    assert_eq!(p.read("personas/ops/persona.md"), ops);
    let about = |key: &str| {
        said.iter()
            .find(|line| line.starts_with(&format!("• personas/ops/persona.md: `{key}:`")))
            .unwrap_or_else(|| panic!("{key}: {said:#?}"))
            .clone()
    };
    for key in ["agent-tools", "skills", "memory"] {
        let line = about(key);
        assert!(
            line.starts_with(&format!(
                "• personas/ops/persona.md: `{key}:` is no longer read: "
            )),
            "{line}"
        );
        assert!(line.contains("(#1460)"), "{line}");
        assert!(line.ends_with("The line is left where it is."), "{line}");
    }
    // D-1451-19: what widened, in the persona's own line of the report.
    assert!(
        about("agent-tools")
            .contains("As a sub-agent this persona could not edit files; as a chat it can"),
        "{}",
        about("agent-tools")
    );
    // `dispatch-isolation` is read again (#1453), so the fix says nothing of it.
    assert!(
        !said.iter().any(|line| line.contains("dispatch-isolation")),
        "{said:#?}"
    );
    // `disallowed-tools` is not retired: it says where it holds.
    assert_eq!(
        about("disallowed-tools"),
        "• personas/ops/persona.md: `disallowed-tools:` is honoured on Claude Code: a chat as \
         this persona is started with those tools denied. On Codex and opencode purlis cannot \
         deny them, so a chat as this persona is refused there, and so is a dispatch to it."
    );
    // `description` has a job, so nothing is said of it.
    assert!(!said.iter().any(|line| line.contains("`description")));
    // The servers are said of the persona that owns the file, and not of its child.
    let servers: Vec<&String> = said
        .iter()
        .filter(|line| line.contains("mcp.json"))
        .collect();
    assert_eq!(
        servers,
        [
            "• personas/ops/mcp.json: it declares 1 MCP server(s). A chat as this persona is \
          started with them on Claude Code; on Codex and opencode they are not started. \
          `purlis persona lint` says which of them wait for an approval."
        ]
    );
    assert_eq!(
        said.last().unwrap(),
        "✓ no generated persona sub-agent file to remove and no persona key to carry over: \
         nothing was changed."
    );
}

#[test]
fn a_generated_file_git_cannot_give_back_is_left_and_named() {
    // F1: a fix never deletes what could not be brought back. Untracked, or tracked with
    // changes that are not committed: both are "git cannot restore it as it is".
    let p = project();
    p.write(".claude/agents/ops.md", &generated("ops"));
    p.write(".claude/agents/qa.md", &generated("qa"));

    let said = p.fix_where(&[], &[], &|rel| rel == ".claude/agents/ops.md");

    assert!(!p.has(".claude/agents/ops.md"));
    assert_eq!(p.read(".claude/agents/qa.md"), generated("qa"));
    assert_eq!(
        said,
        [
            "✓ removed .claude/agents/ops.md (purlis generated it).",
            "• left alone .claude/agents/qa.md: it is a file purlis generated, but git does not \
             track it or it has changes that are not committed, so removing it could not be \
             undone. \
             Commit it or remove it yourself, then run this again.",
            "✓ removed 1 generated sub-agent file(s) and rewrote 0 persona key(s). Nothing is \
             committed: the change is in the working tree for the project's next save. To take \
             it back before then: `git restore -- .claude/agents personas`.",
        ]
    );
}

#[test]
fn a_file_that_stays_under_a_persona_s_name_is_said_to_be_unstartable() {
    // F5: the next sub-agent call to that name is refused as a call to the persona.
    let p = project();
    p.write("personas/runner/persona.md", "---\nrole: Runner\n---\n");
    p.write(".claude/agents/runner.md", HAND);
    p.write(".claude/agents/other.md", HAND);

    let said = p.fix(&[]);

    assert_eq!(
        said[..2],
        [
            "• left alone .claude/agents/other.md: hand-written (it does not carry purlis's \
             marker).",
            "• left alone .claude/agents/runner.md: hand-written (it does not carry purlis's \
             marker). It cannot be started while a persona is called `runner`: a sub-agent \
             call to that name is refused. Rename the file or the persona.",
        ]
    );
}

#[test]
fn the_harness_s_memory_of_a_retired_sub_agent_is_named_and_left() {
    // F2: `memory:` made the harness keep a store per sub-agent. Nothing reads it again.
    let p = project();
    p.write("personas/ops/persona.md", "---\nrole: Ops\n---\n");
    p.write(".claude/agent-memory/ops/MEMORY.md", "# notes\n");
    p.write(".claude/agent-memory/ops/a-fact.md", "a fact\n");
    std::fs::create_dir_all(p.root.join(".claude/agent-memory/empty")).unwrap();

    let said = p.fix(&[]);

    assert_eq!(
        said[0],
        "• .claude/agent-memory/ops/ holds 2 file(s) the harness kept for the `ops` \
         sub-agent. Nothing reads them now. Move what is worth keeping into the persona's own \
         memory with `purlis persona remember ops \"<fact>\"`; the folder is left as it is."
    );
    assert_eq!(said.len(), 2, "{said:#?}");
    assert_eq!(p.read(".claude/agent-memory/ops/a-fact.md"), "a fact\n");
}

#[test]
fn a_model_that_names_a_profile_of_this_machine_alone_is_not_written_into_the_project() {
    // F9: a `profile:` line is committed, and a teammate's machine may not offer the name.
    let p = project();
    let local = "---\nmodel: work\n---\n";
    p.write("personas/ops/persona.md", local);

    let said = p.fix_where(&["claude", "work"], &["claude"], &|_| true);

    assert_eq!(p.read("personas/ops/persona.md"), local);
    assert_eq!(
        said[0],
        "• personas/ops/persona.md: `model: work` names a profile that is this machine's \
         alone, so it is left as it is: a `profile:` line travels with the project. On this \
         machine its chats start on `work`; elsewhere, on the asking chat's profile."
    );
}

#[test]
fn a_second_run_changes_nothing_and_says_again_what_it_left() {
    let p = project();
    p.write(
        "personas/ops/persona.md",
        "---\nmodel: codex\ncolor: cyan\nagent-tools: Read\n---\n",
    );
    p.write(".claude/agents/ops.md", &generated("ops"));
    p.write(".claude/agents/runner.md", HAND);

    let first = p.fix(&["codex"]);
    let after = tree(&p.root);
    let second = p.fix(&["codex"]);

    assert_eq!(tree(&p.root), after, "the second run wrote something");
    assert_eq!(first.iter().filter(|line| line.starts_with('✓')).count(), 4);
    assert_eq!(
        second,
        [
            "• left alone .claude/agents/runner.md: hand-written (it does not carry purlis's \
             marker).",
            "• personas/ops/persona.md: `agent-tools:` is no longer read: it was the tool \
             list of the generated sub-agent. As a sub-agent this persona could not edit \
             files; as a chat it can, and it has every other tool of its harness too. Tool \
             rules that allow one persona's chats only some tools are not in this version yet \
             (#1460); `disallowed-tools:` denies a chat the ones it names. The line is left \
             where it is.",
            "✓ no generated persona sub-agent file to remove and no persona key to carry \
             over: nothing was changed.",
        ]
    );
}

#[test]
fn a_project_with_nothing_to_migrate_is_told_so_and_gains_no_file() {
    let p = project();
    p.write(
        "personas/ops/persona.md",
        "---\nrole: Ops\nprofile: codex\n---\n",
    );
    let before = tree(&p.root);

    let said = p.fix(&["codex"]);

    assert_eq!(tree(&p.root), before);
    assert_eq!(
        said,
        [
            "✓ no generated persona sub-agent file to remove and no persona key to carry \
             over: nothing was changed."
        ]
    );
}

#[test]
fn a_key_written_twice_is_left_for_lint_and_not_guessed_at() {
    let p = project();
    let twice = "---\nmodel: codex\nmodel: claude\ncolor: cyan\ncolor: blue\n---\n";
    p.write("personas/ops/persona.md", twice);

    let said = p.fix(&["codex", "claude"]);

    assert_eq!(p.read("personas/ops/persona.md"), twice);
    assert!(
        said[0].contains("`model:` is written more than once"),
        "{said:#?}"
    );
    assert!(
        said[1].contains("`color:` is written more than once"),
        "{said:#?}"
    );
}

#[test]
fn what_this_machine_kept_for_the_retired_sub_agents_is_removed() {
    // #1460: the in-flight records and the agent map were written by hooks that are gone, and
    // nothing reads them; they are this machine's alone, so removing them changes no commit.
    let p = project();
    p.write("personas/ops/persona.md", "---\nrole: Ops\n---\n");
    p.write(
        ".charter/dispatch-inflight/ops.ab12cd.json",
        "{\"agent\": \"ops\", \"kind\": \"dispatch\", \"ts\": 1.0}",
    );
    p.write(".charter/dispatch-inflight/Explore.ef34gh.json", "{}");
    p.write(".charter/agent-personas.json", "{\"a1b2c3\": \"ops\"}");
    p.write(".charter/guard-seen.json", "{}\n");

    let said = p.fix(&[]);

    assert!(!p.has(".charter/dispatch-inflight"));
    assert!(!p.has(".charter/agent-personas.json"));
    assert_eq!(
        p.read(".charter/guard-seen.json"),
        "{}\n",
        "nothing else of the state"
    );
    assert_eq!(
        said,
        [
            "✓ removed .charter/dispatch-inflight/ (2 file(s)): the record of sub-agents in \
             flight, which nothing reads now.",
            "✓ removed .charter/agent-personas.json: the map of sub-agents to personas, which \
             nothing reads now.",
            "✓ removed what this machine kept for the retired sub-agents. No committed file \
             changed.",
        ]
    );
    // And a second run finds nothing.
    assert_eq!(
        p.fix(&[]),
        [
            "✓ no generated persona sub-agent file to remove and no persona key to carry over: \
          nothing was changed."
        ]
    );
}

#[cfg(unix)]
#[test]
fn a_leftover_that_is_a_link_or_holds_something_else_is_left_and_named() {
    let p = project();
    p.write("elsewhere/keep.json", "KEEP\n");
    std::fs::create_dir_all(p.root.join(".charter")).unwrap();
    std::os::unix::fs::symlink(
        p.root.join("elsewhere/keep.json"),
        p.root.join(".charter/agent-personas.json"),
    )
    .unwrap();
    p.write(".charter/dispatch-inflight/ops.ab12cd.json", "{}");
    p.write(".charter/dispatch-inflight/notes/mine.txt", "MINE\n");

    let said = p.fix(&[]);

    assert_eq!(p.read("elsewhere/keep.json"), "KEEP\n");
    assert!(
        std::fs::symlink_metadata(p.root.join(".charter/agent-personas.json"))
            .unwrap()
            .is_symlink()
    );
    assert!(!p.has(".charter/dispatch-inflight/ops.ab12cd.json"));
    assert_eq!(
        p.read(".charter/dispatch-inflight/notes/mine.txt"),
        "MINE\n"
    );
    assert_eq!(
        said[..2],
        [
            "• left alone .charter/dispatch-inflight/: it holds something purlis did not \
             write there. Its 1 record file(s) were removed."
                .to_owned(),
            "• left alone .charter/agent-personas.json: it is a link, and purlis wrote no \
             such file."
                .to_owned(),
        ]
    );
}
