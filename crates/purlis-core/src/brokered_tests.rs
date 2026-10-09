use super::*;

use std::time::{Duration, Instant};

fn now() -> chrono::NaiveDateTime {
    "2026-10-06T12:00:00".parse().expect("a stamp")
}

/// A project with workspace `alpha` and persona `steward`, and no manifest: nothing here reads one.
fn a_project() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("a directory");
    let root = dir.path().join("project");
    std::fs::create_dir_all(root.join("workspaces/alpha")).unwrap();
    std::fs::create_dir_all(root.join("personas/steward")).unwrap();
    std::fs::write(
        root.join("personas/steward/persona.md"),
        "---\nname: steward\n---\n# Steward\n",
    )
    .unwrap();
    (dir, root)
}

fn in_alpha_as(persona: Option<&str>) -> Asker {
    Asker {
        chat: 3,
        place: Place::Workspace("alpha".to_owned()),
        persona: persona.map(str::to_owned),
    }
}

fn files_in(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

#[test]
fn a_persona_remember_lands_in_the_chats_own_persona_memory() {
    let (_dir, root) = a_project();
    let write = Write::PersonaRemember {
        text: "Deploys go through the pipeline".to_owned(),
        title: None,
        shared: false,
    };

    let written = perform(&root, &in_alpha_as(Some("steward")), &write, now()).expect("written");

    assert_eq!(written.to, "steward");
    assert!(
        written.path.starts_with("personas/steward/memory/"),
        "{}",
        written.path
    );
    let text = std::fs::read_to_string(root.join(&written.path)).unwrap();
    assert!(text.contains("Deploys go through the pipeline"), "{text}");
}

#[test]
fn a_shared_remember_lands_in_shared_memory_whatever_the_chats_persona() {
    let (_dir, root) = a_project();
    let write = Write::PersonaRemember {
        text: "The forge is self-hosted".to_owned(),
        title: Some("Forge".to_owned()),
        shared: true,
    };

    let written = perform(&root, &in_alpha_as(None), &write, now()).expect("written");

    assert!(
        written.path.starts_with("personas/_shared/memory/"),
        "{}",
        written.path
    );
}

#[test]
fn a_chat_with_no_persona_has_no_persona_memory_of_its_own_to_write() {
    let (_dir, root) = a_project();
    let write = Write::PersonaRemember {
        text: "A fact".to_owned(),
        title: None,
        shared: false,
    };

    let why = perform(&root, &in_alpha_as(None), &write, now()).expect_err("refused");

    assert!(why.contains("--shared"), "{why}");
    assert!(!root.join("personas/_shared/memory").exists());
}

#[test]
fn a_workspace_remember_lands_in_the_chats_own_workspace() {
    let (_dir, root) = a_project();
    let write = Write::WorkspaceRemember {
        text: "The build needs two jobs".to_owned(),
        title: None,
    };

    let written = perform(&root, &in_alpha_as(None), &write, now()).expect("written");

    assert_eq!(written.to, "alpha");
    assert!(
        written.path.starts_with("workspaces/alpha/memory/"),
        "{}",
        written.path
    );
    assert!(root.join(&written.path).is_file());
}

#[test]
fn a_todo_lands_in_the_chats_own_workspace_once() {
    let (_dir, root) = a_project();
    let write = Write::Todo {
        text: "Port the docs command".to_owned(),
    };

    let written = perform(&root, &in_alpha_as(None), &write, now()).expect("written");
    assert!(
        written.path.starts_with("workspaces/alpha/todos/"),
        "{}",
        written.path
    );

    let again = perform(&root, &in_alpha_as(None), &write, now()).expect_err("a duplicate");
    assert!(again.contains("Port the docs command"), "{again}");
}

#[test]
fn a_chat_at_the_project_root_has_no_workspace_to_write() {
    let (_dir, root) = a_project();
    let asker = Asker {
        chat: 3,
        place: Place::PlaneRoot,
        persona: None,
    };
    for write in [
        Write::WorkspaceRemember {
            text: "x".to_owned(),
            title: None,
        },
        Write::Todo {
            text: "x".to_owned(),
        },
        Write::WorkspaceVision {
            text: "x".to_owned(),
        },
        Write::WorkspaceSection {
            section: Section::Glossary,
            text: "x".to_owned(),
        },
    ] {
        let why = perform(&root, &asker, &write, now()).expect_err("refused");
        assert!(why.contains("not in a workspace"), "{why}");
    }
    assert!(!root.join("workspace.md").exists());
}

// ---- workspace.md: its vision and its sections (#1384) ------------------------------------

fn charter_of(root: &Path) -> String {
    std::fs::read_to_string(root.join("workspaces/alpha/workspace.md")).expect("workspace.md")
}

#[test]
fn a_vision_is_set_in_the_chats_own_workspace_md_and_credited_to_it() {
    let (_dir, root) = a_project();
    std::fs::create_dir_all(root.join("workspaces/beta")).unwrap();
    let write = Write::WorkspaceVision {
        text: "Ship the docs site".to_owned(),
    };

    let written = perform(&root, &in_alpha_as(None), &write, now()).expect("written");

    assert_eq!(written.to, "alpha");
    assert_eq!(written.path, "workspaces/alpha/workspace.md");
    let ws = crate::workspaces::Plane::open(&root)
        .workspace("alpha")
        .unwrap();
    assert_eq!(ws.vision(), "Ship the docs site");
    assert!(!root.join("workspaces/beta/workspace.md").exists());
    let trace = std::fs::read_to_string(crate::trace::file(&root, "3")).expect("chat 3's trace");
    assert!(trace.contains("\"write\": \"workspace_vision\""), "{trace}");
}

#[test]
fn a_decision_and_a_term_are_added_to_their_sections_and_nothing_else_moves() {
    let (_dir, root) = a_project();
    let ws = crate::workspaces::Plane::open(&root)
        .workspace("alpha")
        .unwrap();
    ws.set_vision("Ship").unwrap();
    let before = charter_of(&root);

    for (section, text) in [
        (Section::Decisions, "Deploys go out on Fridays only"),
        (Section::Decisions, "The cache is per-user"),
        (Section::Glossary, "`svc` — the backend service"),
    ] {
        let write = Write::WorkspaceSection {
            section,
            text: text.to_owned(),
        };
        let written = perform(&root, &in_alpha_as(None), &write, now()).expect("written");
        assert_eq!(written.path, "workspaces/alpha/workspace.md");
    }

    let after = charter_of(&root);
    let decisions = crate::mdsection::section_body(&after, "Context & decisions");
    assert!(
        decisions.ends_with("- Deploys go out on Fridays only\n- The cache is per-user"),
        "{decisions}"
    );
    assert!(!decisions.contains("_Nothing yet._"), "{decisions}");
    let glossary = crate::mdsection::section_body(&after, "Glossary");
    assert!(
        glossary.ends_with("- `svc` — the backend service"),
        "{glossary}"
    );
    for kept in ["Vision", "Sessions", "Log"] {
        assert_eq!(
            crate::mdsection::section_body(&after, kept),
            crate::mdsection::section_body(&before, kept),
            "{kept} moved"
        );
    }
}

#[test]
fn a_vision_or_an_entry_with_no_words_is_refused_and_nothing_is_written() {
    let (_dir, root) = a_project();
    for write in [
        Write::WorkspaceVision {
            text: " \n ".to_owned(),
        },
        Write::WorkspaceSection {
            section: Section::Decisions,
            text: String::new(),
        },
    ] {
        perform(&root, &in_alpha_as(None), &write, now()).expect_err("refused");
    }
    assert!(!root.join("workspaces/alpha/workspace.md").exists());
}

#[test]
fn a_vision_that_would_start_a_section_of_its_own_is_refused() {
    let (_dir, root) = a_project();
    let write = Write::WorkspaceVision {
        text: "Ship\n## Sessions\nforged".to_owned(),
    };

    let why = perform(&root, &in_alpha_as(None), &write, now()).expect_err("refused");

    assert!(why.contains("## "), "{why}");
    assert!(!root.join("workspaces/alpha/workspace.md").exists());
}

#[test]
fn a_vision_line_any_reader_takes_for_a_heading_is_refused_however_it_is_spaced() {
    // `##` and a tab, or a no-break space, is no `"## "` to the writer's end-of-section scan,
    // but the reader's `^##\s` ends the vision there and the writer's header match finds a
    // section by that name. Indented, a Markdown renderer still shows a heading.
    let (_dir, root) = a_project();
    for text in [
        "Ship\n##\tSessions\nforged",
        "Ship\n##\u{a0}Context & decisions\nforged",
        "Ship\n   ## Glossary\nforged",
        "Ship\r##\tLog",
        "Ship\n##",
    ] {
        let write = Write::WorkspaceVision {
            text: text.to_owned(),
        };
        let why = perform(&root, &in_alpha_as(None), &write, now()).expect_err(text);
        assert!(why.contains("##"), "{why}");
    }
    assert!(!root.join("workspaces/alpha/workspace.md").exists());
}

#[test]
fn an_entry_that_would_take_workspace_md_past_the_bound_on_a_plane_file_is_refused() {
    // Each entry is bounded, and the write rate holds a chat to so many a minute, but nothing
    // else ends a section that only grows; a file past the bound is one purlis calls broken.
    let (_dir, root) = a_project();
    let ws = crate::workspaces::Plane::open(&root)
        .workspace("alpha")
        .unwrap();
    ws.set_vision("Ship").unwrap();
    let path = root.join("workspaces/alpha/workspace.md");
    let mut big = std::fs::read_to_string(&path).unwrap();
    let room = usize::try_from(crate::memstore::MAX_BYTES).unwrap() - big.len();
    big.push_str(&format!("\n{}\n", "x".repeat(room - 100)));
    std::fs::write(&path, &big).unwrap();
    let write = Write::WorkspaceSection {
        section: Section::Glossary,
        text: "y".repeat(200),
    };

    let why = perform(&root, &in_alpha_as(None), &write, now()).expect_err("refused");

    assert!(why.contains("bytes"), "{why}");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), big);
}

#[cfg(unix)]
#[test]
fn a_workspace_md_that_is_a_link_out_of_the_project_is_never_written_through() {
    let (dir, root) = a_project();
    let outside = dir.path().join("outside.md");
    std::fs::write(&outside, "## Vision\n\nmine\n").unwrap();
    std::os::unix::fs::symlink(&outside, root.join("workspaces/alpha/workspace.md")).unwrap();

    for write in [
        Write::WorkspaceVision {
            text: "theirs".to_owned(),
        },
        Write::WorkspaceSection {
            section: Section::Glossary,
            text: "theirs".to_owned(),
        },
    ] {
        perform(&root, &in_alpha_as(None), &write, now()).expect_err("refused");
    }
    assert_eq!(
        std::fs::read_to_string(&outside).unwrap(),
        "## Vision\n\nmine\n"
    );
}

#[test]
fn a_section_write_reads_off_the_wire_by_its_word() {
    let write: Write =
        serde_json::from_str(r#"{"op":"workspace_section","section":"glossary","text":"term"}"#)
            .expect("read");
    assert_eq!(
        write,
        Write::WorkspaceSection {
            section: Section::Glossary,
            text: "term".to_owned(),
        }
    );
    assert!(
        serde_json::from_str::<Write>(
            r#"{"op":"workspace_section","section":"sessions","text":"x"}"#
        )
        .is_err(),
        "purlis's own Sessions line is no section a chat adds to"
    );
}

#[test]
fn a_workspace_that_is_not_there_is_refused_and_never_made() {
    let (_dir, root) = a_project();
    let asker = Asker {
        chat: 3,
        place: Place::Workspace("ghost".to_owned()),
        persona: None,
    };
    let write = Write::Todo {
        text: "x".to_owned(),
    };

    let why = perform(&root, &asker, &write, now()).expect_err("refused");

    assert!(why.contains("ghost"), "{why}");
    assert!(!root.join("workspaces/ghost").exists());
}

#[test]
fn every_brokered_write_is_credited_to_the_chat_that_asked() {
    let (_dir, root) = a_project();
    let write = Write::WorkspaceRemember {
        text: "Credited".to_owned(),
        title: None,
    };

    let written = perform(&root, &in_alpha_as(None), &write, now()).expect("written");

    let trace = std::fs::read_to_string(crate::trace::file(&root, "3")).expect("chat 3's trace");
    assert!(trace.contains("\"event\": \"brokered\""), "{trace}");
    assert!(trace.contains("\"chat\": \"3\""), "{trace}");
    assert!(trace.contains(&written.path), "{trace}");
}

// ---- what no brokered write may change (ADR 0067 §2, #1330, the #1341 review) -------------

#[test]
fn a_brokered_write_that_would_change_the_sandbox_is_refused() {
    let (_dir, root) = a_project();
    for name in [
        "purlis.toml",
        "charter.toml",
        "purlis.local.toml",
        "charter.local.toml",
    ] {
        let manifest = root.join(name);
        for after in [
            "schema = 1\n\n[sandbox]\nmode = \"off\"\n",
            "[sandbox]\nhosts = [\"evil.example\"]\n",
            "[chat_env]\nPATH = \"/tmp\"\n",
            // A chat never raises its own dispatch limits (#1439).
            "[dispatch]\ndepth = 8\n",
            "[dispatch.personas.devops]\nmay-run-at-once = 99\n",
            "not toml [",
        ] {
            let why = guard(&root, &manifest, Some(after)).expect_err(after);
            assert!(why.contains("[sandbox]"), "{why}");
        }
        // A text nobody gave is never written over a manifest.
        assert!(guard(&root, &manifest, None).is_err(), "{name}");
        // Nothing a chat runs under, before or after.
        assert_eq!(guard(&root, &manifest, Some("schema = 1\n")), Ok(()));
    }
}

#[test]
fn a_change_to_sandbox_or_chat_env_is_a_change_and_the_same_tables_are_not() {
    let on = "schema = 1\n\n[sandbox]\nmode = \"on\"\n";
    assert!(!changes_what_a_chat_runs_under(
        Some(on),
        "schema = 2\n\n[sandbox]\nmode = \"on\"\n"
    ));
    assert!(!changes_what_a_chat_runs_under(None, "schema = 1\n"));
    for after in [
        "schema = 1\n",
        "schema = 1\n\n[sandbox]\nmode = \"off\"\n",
        "schema = 1\n\n[sandbox]\nmode = \"on\"\nhosts = [\"evil.example\"]\n",
        "schema = 1\n\n[sandbox]\nmode = \"on\"\n\n[chat_env]\nPATH = \"/tmp\"\n",
        "not toml [",
    ] {
        assert!(changes_what_a_chat_runs_under(Some(on), after), "{after}");
    }
    assert!(changes_what_a_chat_runs_under(
        Some("not toml ["),
        "schema = 1\n"
    ));
}

#[test]
fn no_brokered_write_creates_widens_or_revokes_a_dispatch_grant() {
    // #1437: who may dispatch to whom is the person's to say, so a write a chat asks purlis to
    // make never changes `[dispatch]`, in a file that had it or one that did not.
    let granted = "schema = 1\n\n[dispatch.grants]\nsteward = [\"devops\"]\n";
    for (before, after) in [
        // Created.
        (Some("schema = 1\n"), granted),
        (None, granted),
        // Widened.
        (
            Some(granted),
            "schema = 1\n\n[dispatch.grants]\nsteward = [\"devops\", \"qa\"]\n",
        ),
        (
            Some(granted),
            "schema = 1\n\n[dispatch.grants]\nsteward = [\"devops\"]\nqa = [\"devops\"]\n",
        ),
        // Widened to any persona (#1503), beside a pair or in its place.
        (
            Some(granted),
            "schema = 1\n\n[dispatch.grants]\nsteward = [\"devops\", \"*\"]\n",
        ),
        (
            Some(granted),
            "schema = 1\n\n[dispatch.grants]\nsteward = [\"*\"]\n",
        ),
        (
            Some("schema = 1\n"),
            "schema = 1\n\n[dispatch.grants]\nsteward = [\"*\"]\n",
        ),
        // Revoked.
        (Some(granted), "schema = 1\n"),
        (
            Some(granted),
            "schema = 1\n\n[dispatch.grants]\nsteward = []\n",
        ),
    ] {
        assert!(
            changes_what_a_chat_runs_under(before, after),
            "{before:?} to {after}"
        );
    }
    // The same grants under another line of the file are no change.
    assert!(!changes_what_a_chat_runs_under(
        Some(granted),
        "schema = 2\n\n[dispatch.grants]\nsteward = [\"devops\"]\n"
    ));
}

#[test]
fn no_brokered_write_adds_widens_empties_or_removes_a_personas_profile_list() {
    // #1509: which profiles a persona's dispatched chats run on is the project's to say, so a
    // write a chat asks purlis to make never changes `[dispatch.profiles]`.
    let listed = "schema = 1\n\n[dispatch.profiles]\ndevops = [\"work\"]\n";
    for (before, after) in [
        // Added, in a file that had none and where there was no file.
        (Some("schema = 1\n"), listed),
        (None, listed),
        // Widened: another profile, and another persona.
        (
            Some(listed),
            "schema = 1\n\n[dispatch.profiles]\ndevops = [\"work\", \"codex\"]\n",
        ),
        (
            Some(listed),
            "schema = 1\n\n[dispatch.profiles]\ndevops = [\"work\"]\nqa = [\"work\"]\n",
        ),
        // Emptied, and removed.
        (
            Some(listed),
            "schema = 1\n\n[dispatch.profiles]\ndevops = []\n",
        ),
        (Some(listed), "schema = 1\n\n[dispatch.profiles]\n"),
        (Some(listed), "schema = 1\n"),
    ] {
        assert!(
            changes_what_a_chat_runs_under(before, after),
            "{before:?} to {after}"
        );
    }
    // The same lists under another line of the file are no change.
    assert!(!changes_what_a_chat_runs_under(
        Some(listed),
        "schema = 2\n\n[dispatch.profiles]\ndevops = [\"work\"]\n"
    ));
}

#[test]
fn a_brokered_write_never_touches_what_decides_whether_the_local_settings_are_ignored() {
    let (_dir, root) = a_project();
    for path in [
        ".gitignore",
        "workspaces/alpha/.gitignore",
        ".git/info/exclude",
        ".git/index",
        ".git/config",
        ".git/hooks/pre-commit",
        ".claude/settings.json",
    ] {
        let why = guard(&root, &root.join(path), None).expect_err(path);
        assert!(why.contains(path.rsplit('/').next().unwrap()), "{why}");
    }
    assert_eq!(
        guard(&root, &root.join("workspaces/alpha/memory/x.md"), None),
        Ok(())
    );
}

#[cfg(unix)]
#[test]
fn a_memory_folder_that_leads_into_git_is_refused_and_nothing_is_written_there() {
    let (_dir, root) = a_project();
    std::fs::create_dir_all(root.join(".git/info")).unwrap();
    std::os::unix::fs::symlink(root.join(".git/info"), root.join("workspaces/alpha/memory"))
        .unwrap();
    let write = Write::WorkspaceRemember {
        text: "x".to_owned(),
        title: None,
    };

    assert!(perform(&root, &in_alpha_as(None), &write, now()).is_err());
    assert_eq!(files_in(&root.join(".git/info")), Vec::<String>::new());
}

#[test]
fn every_name_the_sandbox_denies_as_later_code_is_refused_whatever_its_case() {
    let (_dir, root) = a_project();
    for path in [
        ".CLAUDE/rules/x.md",
        "workspaces/beta/.CLAUDE/commands/x.md",
        "workspaces/beta/.opencode/command/x.md",
        "personas/_shared/.Codex/x.md",
        "workspaces/alpha/.agents/skills/x.md",
        "workspaces/alpha/.VSCode/tasks.json",
        "workspaces/alpha/.GIT/hooks/pre-commit",
        "workspaces/alpha/.husky/x",
        "workspaces/alpha/.zshrc",
        "workspaces/alpha/OpenCode.JSON",
    ] {
        assert!(guard(&root, &root.join(path), None).is_err(), "{path}");
    }
}

#[test]
fn a_name_that_only_contains_a_later_code_name_is_written() {
    // The rule is whole names: a title about zshrc, or a workspace called `claude`, is fine.
    let (_dir, root) = a_project();
    std::fs::create_dir_all(root.join("workspaces/claude")).unwrap();
    assert_eq!(
        guard(
            &root,
            &root.join("workspaces/claude/memory/zshrc-tips.md"),
            None
        ),
        Ok(())
    );
    let asker = Asker {
        chat: 3,
        place: Place::Workspace("claude".to_owned()),
        persona: None,
    };
    let write = Write::WorkspaceRemember {
        text: "Keep the prompt short".to_owned(),
        title: Some("zshrc tips".to_owned()),
    };

    let written = perform(&root, &asker, &write, now()).expect("written");

    assert_eq!(written.to, "claude");
    assert!(root.join(&written.path).is_file(), "{}", written.path);
}

#[cfg(unix)]
#[test]
fn a_shared_memory_folder_linked_into_another_workspace_s_harness_config_is_refused() {
    // A chat at the project root plants `personas/_shared/memory` as a link to where another
    // workspace's chats load commands from: nothing is written there.
    let (_dir, root) = a_project();
    // Pointed at, not made: whether it is there yet, nothing may make it or write in it.
    let commands = root.join("workspaces/beta/.opencode/command");
    std::fs::create_dir_all(root.join("personas/_shared")).unwrap();
    std::os::unix::fs::symlink(&commands, root.join("personas/_shared/memory")).unwrap();
    let write = Write::PersonaRemember {
        text: "Run this".to_owned(),
        title: None,
        shared: true,
    };

    assert!(perform(&root, &in_alpha_as(None), &write, now()).is_err());
    assert!(!root.join("workspaces/beta/.opencode").exists());
    assert_eq!(files_in(&commands), Vec::<String>::new());
}

#[cfg(unix)]
#[test]
fn a_persona_memory_folder_that_is_a_link_inside_the_project_is_never_followed() {
    // A link that stays inside the project, and names nothing the guard knows, is refused by
    // the hold itself: a persona's store is held by descriptor, as a workspace's is (V74).
    let (_dir, root) = a_project();
    let elsewhere = root.join("workspaces/beta/notes");
    std::fs::create_dir_all(&elsewhere).unwrap();
    std::os::unix::fs::symlink(&elsewhere, root.join("personas/steward/memory")).unwrap();
    let write = Write::PersonaRemember {
        text: "Mine".to_owned(),
        title: None,
        shared: false,
    };

    let why = perform(&root, &in_alpha_as(Some("steward")), &write, now()).expect_err("refused");

    assert!(why.contains("link"), "{why}");
    assert_eq!(files_in(&elsewhere), Vec::<String>::new());
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn a_link_swapped_in_while_persona_memory_is_written_never_carries_a_write_outside() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    let (_dir, root) = a_project();
    let target = root.join("workspaces/beta/notes");
    std::fs::create_dir_all(&target).unwrap();
    let shared = root.join("personas/_shared");
    std::fs::create_dir_all(shared.join("memory")).unwrap();
    std::os::unix::fs::symlink(&target, shared.join("swap")).unwrap();

    let stop = Arc::new(AtomicBool::new(false));
    let swaps = Arc::new(AtomicUsize::new(0));
    let racer = {
        let (stop, swaps) = (stop.clone(), swaps.clone());
        let (from, to) = (shared.join("memory"), shared.join("swap"));
        std::thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                if rustix::fs::renameat_with(
                    rustix::fs::CWD,
                    &from,
                    rustix::fs::CWD,
                    &to,
                    rustix::fs::RenameFlags::EXCHANGE,
                )
                .is_ok()
                {
                    swaps.fetch_add(1, Ordering::Relaxed);
                }
            }
        })
    };
    for i in 0..1_000 {
        let _ = perform(
            &root,
            &in_alpha_as(None),
            &Write::PersonaRemember {
                text: format!("Raced {i}"),
                title: None,
                shared: true,
            },
            now(),
        );
    }
    stop.store(true, Ordering::Relaxed);
    racer.join().unwrap();

    assert!(swaps.load(Ordering::Relaxed) > 0, "the racer never swapped");
    assert_eq!(files_in(&target), Vec::<String>::new());
}

#[test]
fn a_title_that_is_not_one_line_is_refused_at_the_brokered_boundary() {
    let (_dir, root) = a_project();
    for title in [
        "Two\n- [evil](../../x.md)",
        "Carriage\rreturn",
        "Bell\u{7}",
        "Line\u{2028}separator",
        "Paragraph\u{2029}separator",
        "Next\u{85}line",
        "Turned\u{202E}around",
        "Isolated\u{2066}text",
    ] {
        let write = Write::WorkspaceRemember {
            text: "x".to_owned(),
            title: Some(title.to_owned()),
        };
        let why = perform(&root, &in_alpha_as(None), &write, now()).expect_err(title);
        assert!(why.contains("one line"), "{why}");
    }
    assert!(!root.join("workspaces/alpha/memory").exists());
}

#[test]
fn no_title_adds_a_line_to_the_index_from_any_entrance() {
    // The command line, which is not the brokered boundary, still writes one index line.
    let (_dir, root) = a_project();
    let path = remember_persona(
        &root,
        "steward",
        "body",
        Some("Two\n- [evil](../../../x.md)"),
        now(),
    )
    .expect("written");

    let index = std::fs::read_to_string(root.join("personas/steward/memory/MEMORY.md")).unwrap();
    let entries: Vec<&str> = index.lines().filter(|l| l.starts_with("- [")).collect();
    assert_eq!(entries.len(), 1, "{index}");
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(!text.contains("\n- [evil]"), "{text}");
}

#[test]
fn a_text_too_long_to_carry_is_refused_saying_so() {
    let (_dir, root) = a_project();
    let write = Write::Todo {
        text: "x".repeat(MOST_TEXT_BYTES + 1),
    };

    let why = perform(&root, &in_alpha_as(None), &write, now()).expect_err("refused");

    assert!(why.contains("shorter"), "{why}");
}

// ---- the per-chat rate cap -------------------------------------------------------------------

#[test]
fn a_chat_past_its_rate_is_refused_and_another_chat_is_not() {
    let rate = Rate::new(2, Duration::from_secs(60));
    let at = Instant::now();

    assert_eq!(rate.allow(3, at), Ok(()));
    assert_eq!(rate.allow(3, at), Ok(()));
    let why = rate.allow(3, at).expect_err("past the cap");
    assert!(why.contains("chat 3"), "{why}");
    assert_eq!(rate.allow(4, at), Ok(()));
    // The window moves on, and a chat that wrote nothing in it is no longer counted.
    assert_eq!(rate.allow(3, at + Duration::from_secs(61)), Ok(()));
    assert_eq!(rate.counted(), 1, "chat 4 is still counted");
    rate.forget(3);
    assert_eq!(rate.counted(), 0);
}

#[test]
fn two_texts_differ_in_what_a_chat_runs_under_only_where_those_tables_do() {
    // #1464: what a chat's commit is held to, as a brokered write is.
    let base = "schema = 1\n[dispatch.grants]\nsteward = [\"devops\"]\n";
    // The same text, and a change outside the three tables, do not differ.
    assert!(!runs_under_differs(Some(base), Some(base)));
    assert!(!runs_under_differs(
        Some(base),
        Some("schema = 1\nname = \"x\"\n[dispatch.grants]\nsteward = [\"devops\"]\n")
    ));
    assert!(!runs_under_differs(None, None));
    // A grant more, a sandbox table, an environment: each differs.
    for other in [
        "schema = 1\n[dispatch.grants]\nsteward = [\"devops\", \"*\"]\n",
        "schema = 1\n[dispatch.grants]\nsteward = [\"devops\"]\n[sandbox]\nenabled = false\n",
        "schema = 1\n[dispatch.grants]\nsteward = [\"devops\"]\n[chat_env]\nX = \"1\"\n",
    ] {
        assert!(runs_under_differs(Some(base), Some(other)), "{other}");
        assert!(runs_under_differs(Some(other), Some(base)), "{other}");
    }
    // No file against one with a grant, and a text that is not TOML against any other.
    assert!(runs_under_differs(None, Some(base)));
    assert!(runs_under_differs(Some(base), None));
    assert!(!runs_under_differs(None, Some("schema = 1\n")));
    assert!(runs_under_differs(Some("[[not toml"), Some(base)));
    assert!(runs_under_differs(
        Some("[[not toml"),
        Some("[[not toml either")
    ));
}

// ---- workspace.md: what would hide later sections, and its budget (#1598) ------------------

#[test]
fn a_vision_or_an_entry_that_opens_a_comment_or_a_fence_and_does_not_close_it_is_refused() {
    let (_dir, root) = a_project();
    for (text, said) in [
        ("Ship <!-- the rest is hidden", "HTML comment"),
        ("Ship\n<!--\nhidden", "HTML comment"),
        ("Ship <!-- one --> and <!-- two", "HTML comment"),
        ("Ship\n```\ncode", "code fence"),
        ("Ship\n~~~~\ncode\n~~~", "code fence"),
        ("Ship\n  ```rust\nfn x() {}\n", "code fence"),
        ("Ship\n```\ncode\n~~~", "code fence"),
        // Indented four spaces, a fence neither opens nor closes (CommonMark).
        ("Ship\n```\ncode\n    ```\nhidden", "code fence"),
        ("Ship\n\n    ```\n```\nhidden", "code fence"),
        // The raw HTML blocks that run to the end of the document without their end.
        ("Ship\n<pre>\nhidden", "raw HTML block"),
        ("Ship\n<SCRIPT type=x>\nhidden", "raw HTML block"),
        ("Ship\n<style\nhidden", "raw HTML block"),
        ("Ship\n<textarea>", "raw HTML block"),
        ("Ship\n<?php\nhidden", "raw HTML block"),
        ("Ship\n<!DOCTYPE\nhidden", "raw HTML block"),
        ("Ship\n   <![CDATA[ x\nhidden", "raw HTML block"),
    ] {
        for write in [
            Write::WorkspaceVision {
                text: text.to_owned(),
            },
            Write::WorkspaceSection {
                section: Section::Decisions,
                text: text.to_owned(),
            },
        ] {
            let why = perform(&root, &in_alpha_as(None), &write, now()).expect_err(text);
            assert!(why.contains(said), "{text}: {why}");
            assert!(why.contains("nothing was written"), "{why}");
        }
    }
    assert!(!root.join("workspaces/alpha/workspace.md").exists());
}

#[test]
fn a_comment_or_a_fence_closed_in_the_text_and_one_inside_code_is_written() {
    let (_dir, root) = a_project();
    for text in [
        "Ship <!-- a note --> the docs",
        "Ship\n<!--\na note\n-->\ndone",
        "Ship\n```\n<!-- inside a fence\n```",
        "Ship\n~~~\ncode ``` here\n~~~~",
        "Write `<!--` to open a comment",
        "Write ``a ` <!-- b`` here",
        "Ship\n<pre>\nkept as it is\n</pre>\ndone",
        "Ship\n<style>p {}</style>\ndone",
        "Ship\n<!DOCTYPE html>\ndone",
        "Ship\n<?xml version=\"1.0\"?>\ndone",
        "Ship <script> in a line is no block",
        "Ship\n<preview> is no pre block",
        "Ship\n```\n<pre>\n```\ndone",
    ] {
        for write in [
            Write::WorkspaceVision {
                text: text.to_owned(),
            },
            Write::WorkspaceSection {
                section: Section::Glossary,
                text: text.to_owned(),
            },
        ] {
            perform(&root, &in_alpha_as(None), &write, now()).expect(text);
        }
    }
}

/// The text of a `workspace.md` `bytes` long, in alpha, whose vision is set: what a person who
/// writes a lot by hand leaves.
fn a_charter_of(root: &Path, bytes: usize) -> String {
    let ws = crate::workspaces::Plane::open(root)
        .workspace("alpha")
        .unwrap();
    ws.set_vision("Ship").unwrap();
    let path = root.join("workspaces/alpha/workspace.md");
    let mut text = std::fs::read_to_string(&path).unwrap();
    let room = bytes - text.len() - 2;
    text.push_str(&format!("\n{}\n", "x".repeat(room)));
    std::fs::write(&path, &text).unwrap();
    text
}

#[test]
fn a_chats_write_that_would_grow_workspace_md_past_its_budget_is_refused_saying_how() {
    let (_dir, root) = a_project();
    let before = a_charter_of(&root, WORKSPACE_MD_GROWS_TO_AT_MOST - 100);

    for write in [
        Write::WorkspaceSection {
            section: Section::Decisions,
            text: "y".repeat(200),
        },
        Write::WorkspaceVision {
            text: "z".repeat(200),
        },
    ] {
        let why = perform(&root, &in_alpha_as(None), &write, now()).expect_err("refused");
        assert!(why.contains("64 KiB"), "{why}");
        assert!(why.contains("purlis workspace remember"), "{why}");
        assert!(why.contains("nothing was written"), "{why}");
    }
    assert_eq!(charter_of(&root), before);

    // A short entry still fits.
    let write = Write::WorkspaceSection {
        section: Section::Decisions,
        text: "fits".to_owned(),
    };
    perform(&root, &in_alpha_as(None), &write, now()).expect("written");
}

#[test]
fn a_workspace_md_already_past_its_budget_is_refused_only_what_grows_it() {
    let (_dir, root) = a_project();
    a_charter_of(&root, WORKSPACE_MD_GROWS_TO_AT_MOST + 1000);

    let grows = Write::WorkspaceSection {
        section: Section::Glossary,
        text: "term".to_owned(),
    };
    perform(&root, &in_alpha_as(None), &grows, now()).expect_err("it grows the file");
    // A shorter vision than the one there shrinks it, and is written.
    let shrinks = Write::WorkspaceVision {
        text: "S".to_owned(),
    };
    perform(&root, &in_alpha_as(None), &shrinks, now()).expect("it shrinks the file");
}

// ---- a chat's commit and the app's write to the project file (#1586) ----------------------

/// #1586: a chat stages the project file as it stands, and before it commits the app writes the
/// file (an Allow, or an Add for everyone, puts a grant in `[dispatch]`). The staged file then
/// holds other `[dispatch]` tables than the working one, so the chat's commit is refused, and
/// the refusal tells it to stage the file again. That is the intended answer, pinned here.
#[test]
fn a_chats_commit_staged_before_the_app_wrote_the_project_file_is_refused_and_told_to_stage_it_again()
 {
    let staged = "schema = 1\nname = \"x\"\n[dispatch.grants]\nsteward = [\"devops\"]\n";
    let the_app_wrote =
        "schema = 1\nname = \"x\"\n[dispatch.grants]\nsteward = [\"devops\", \"qa\"]\n";

    assert!(runs_under_differs(Some(staged), Some(the_app_wrote)));
    let refusal = crate::diffscan::RUNS_UNDER_REFUSAL;
    assert!(
        refusal.contains("differ from the file in the working tree"),
        "{refusal}"
    );
    assert!(refusal.contains("stage the file as it is"), "{refusal}");
    assert!(refusal.contains("Do not use --no-verify"), "{refusal}");

    // Staged again, as it now stands, the commit is not held.
    assert!(!runs_under_differs(
        Some(the_app_wrote),
        Some(the_app_wrote)
    ));
}
