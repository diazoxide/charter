//! **A command too big to check is refused before any guard reads it** (#1355).
//!
//! A harness gives a `PreToolUse` hook a fixed time, and a hook still reading when it runs out
//! may be an allow. So three caps are applied to the command as written, each in one linear
//! pass, before any guard parses it: its size, how deeply its quotes and substitutions nest,
//! and how many wrapper programs stand in a row. Past any of them the call is refused with one
//! sentence that says to split the command. Ordinary large commands, a commit message or a pull
//! request body in a quoted heredoc, are well inside all three.

use std::path::{Path, PathBuf};

use purlis_core::forge::{Forge, Kind};
use purlis_core::guardcaps;
use purlis_core::handoffguard::Caller;
use purlis_core::toolgate::{self, Call, Launched, Plane};

/// The verdict on `cmd`, inside a plane or out of one, from an unattended chat.
fn verdict(cmd: &str, in_a_plane: bool) -> Option<toolgate::Verdict> {
    verdict_in(cmd, in_a_plane, "bypassPermissions")
}

/// The verdict on `cmd`, inside a plane or out of one, in the permission `mode` given.
fn verdict_in(cmd: &str, in_a_plane: bool, mode: &str) -> Option<toolgate::Verdict> {
    let dir = tempfile::tempdir().expect("a directory");
    let root = dir.path().display().to_string();
    let state: PathBuf = dir.path().join(".charter");
    let forges = vec![
        Forge::default_of(Kind::GitHub),
        Forge::default_of(Kind::GitLab),
    ];
    let plane = Plane {
        root: &root,
        forges: &forges,
        session_dir: "",
        launched: Launched::default(),
    };
    let call = Call {
        command: cmd,
        cwd: &root,
        state_dir: Path::new(&state),
        caller: Caller {
            agent_id: None,
            harness: Some("claude-code"),
            permission_mode: Some(mode),
        },
    };
    toolgate::verdict(&call, in_a_plane.then_some(&plane))
}

/// Whether `cmd` was refused for being too big to check, in both places a guard runs.
fn refused_as_too_big(cmd: &str) -> bool {
    [true, false]
        .into_iter()
        .all(|in_a_plane| verdict(cmd, in_a_plane).is_some_and(|v| v.reason == guardcaps::REASON))
}

/// A pull request body as people write them: headings, lists, code spans and fences, quotes,
/// apostrophes, parentheses and the shell's own spellings talked about in prose.
fn a_pr_body(bytes: usize) -> String {
    let paragraph = "## Why\n\nThe guard didn't read `\"$(cat <<'EOF'` the way bash does (see #917), \
                     so a line like `echo \"$(date)\"` was refused. It's fixed: `$((1+2))`, \
                     `<(sort a)` and `${x:-y}` are read as substitutions.\n\n\
                     - 1) the reader (shellseg)\n- 2) the floor's \"eval\" walk\n\n\
                     ```sh\nif [ -n \"$x\" ]; then echo \"it's $(pwd)\"; fi\n```\n\n";
    paragraph.repeat(bytes / paragraph.len() + 1)
}

#[test]
fn a_command_longer_than_the_size_cap_is_refused_with_a_sentence_saying_to_split_it() {
    purlis_core::unsteered!();
    let long = format!("echo {}", "a".repeat(guardcaps::MAX_COMMAND_BYTES));
    assert!(refused_as_too_big(&long));
    let said = verdict(&long, false).expect("refused").said();
    assert!(said.contains("too long"), "{said}");
    assert!(said.contains("split"), "{said}");
    // And it names the route out for the long text people really send: a file.
    assert!(said.contains("--body-file"), "{said}");
}

/// A string a guard derives from the command and reads in turn (heredocs taken out, a quoted
/// body, a shell's script) can nest deeper than the command did as the cap read it. A reading
/// that meets one past the cap fails closed: the call is refused as too big to check, in an
/// attended chat and an unattended one, never decided on a reading cut short.
#[test]
fn a_string_a_guard_derives_nested_past_the_cap_is_refused_in_both_modes() {
    purlis_core::unsteered!();
    let dir = tempfile::tempdir().expect("a directory");
    let vaults = dir.path().join(".charter").join("vaults");
    std::fs::create_dir_all(&vaults).expect("the vault directory");
    let vault = vaults.join("f.json");
    std::fs::write(&vault, "{}\n").expect("a vault");
    let vault = vault.display();
    // Each line on its own inside the cap, the whole twice as deep.
    let k = guardcaps::MAX_NESTING;
    let tail = "\ncat <<'EOF'\n'\nEOF";
    let shapes = [
        format!(
            "cat {}$(\necho {}{vault} {}{tail}",
            "$(echo ".repeat(k - 1),
            "$(echo ".repeat(k),
            ")".repeat(2 * k)
        ),
        // The same in double quotes.
        format!(
            "cat \"{}\n{}echo {vault}{}\"{tail}",
            "$(echo \"".repeat(k),
            "$(echo \"".repeat(k),
            "\")".repeat(2 * k)
        ),
    ];
    for cmd in &shapes {
        assert_eq!(
            guardcaps::refusal(cmd),
            None,
            "inside the cap as written: {cmd}"
        );
        for mode in ["default", "bypassPermissions"] {
            for in_a_plane in [true, false] {
                let refused = verdict_in(cmd, in_a_plane, mode);
                assert!(
                    refused.is_some(),
                    "{mode}, in a plane: {in_a_plane}: not refused: {cmd}"
                );
            }
        }
    }
}

#[test]
fn substitutions_nested_past_the_cap_are_refused() {
    purlis_core::unsteered!();
    let deep = guardcaps::MAX_NESTING + 1;
    for cmd in [
        // Two lengths in three of this one will not tokenize, and are read as written.
        "'\"$(".repeat(deep + 1),
        format!("echo {}1{}", "$((".repeat(deep), "))".repeat(deep)),
        format!("echo \"{}", "$(\\\"".repeat(deep)),
        format!(
            "echo \"{}ls{}",
            "$(echo \"".repeat(deep),
            "\")".repeat(deep)
        ),
        format!("{}ls", "echo \"$(".repeat(deep)),
    ] {
        assert!(refused_as_too_big(&cmd), "{cmd}");
        let said = verdict(&cmd, false).expect("refused").said();
        assert!(said.contains("too deeply nested"), "{said}");
        assert!(said.contains("--body-file"), "{said}");
    }
}

#[test]
fn more_wrapper_programs_in_a_row_than_the_cap_are_refused() {
    purlis_core::unsteered!();
    let many = guardcaps::MAX_LAYERS + 1;
    for cmd in [
        format!("{}echo hi", "eval ".repeat(many)),
        format!("{}ls", "env ".repeat(many)),
        format!("{}ls", "nice -n 1 ".repeat(many)),
        format!("{}echo hi", "bash -c ".repeat(many)),
        format!("{}ls", "sudo env A=1 ".repeat(many)),
    ] {
        assert!(refused_as_too_big(&cmd), "{cmd}");
        let said = verdict(&cmd, false).expect("refused").said();
        assert!(said.contains("wrapper"), "{said}");
    }
}

#[test]
fn an_ordinary_large_commit_message_or_pr_body_is_not_refused() {
    purlis_core::unsteered!();
    // GitHub takes a pull request body of at most 65,536 characters.
    let body = a_pr_body(65_536);
    for cmd in [
        format!("gh pr create --title t --body-file - <<'EOF'\n{body}EOF"),
        format!("git commit -F - <<'EOF'\n{body}EOF"),
        format!("git commit -m \"$(cat <<'EOF'\n{body}EOF\n)\""),
        // A long script of ordinary commands, each with its own few substitutions.
        "x=\"$(git rev-parse HEAD)\" && echo \"$(date) $x\" && ls -la | grep -v '^$'\n".repeat(600),
        // Wrapper words in prose are not wrappers in a row.
        format!(
            "git commit -m \"$(cat <<'EOF'\n{}EOF\n)\"",
            "Run env, nice, time and timeout under sudo, then eval.\n".repeat(200)
        ),
        // Nor in a quoted body, however often the prose names one.
        format!(
            "gh pr create --title t --body \"{}\"",
            "To check it, run the command env nice bash -c ls by hand. ".repeat(70)
        ),
    ] {
        let refused = verdict(&cmd, true).filter(|v| v.reason == guardcaps::REASON);
        assert_eq!(refused, None, "{} bytes", cmd.len());
    }
}

/// A body longer in bytes than the size cap holds, as text in a script of three bytes a
/// character is at a forge's largest, goes in a file, and a command that passes the file is
/// read like any other.
#[test]
fn a_body_at_a_forges_largest_passed_as_a_file_is_not_refused() {
    purlis_core::unsteered!();
    let dir = tempfile::tempdir().expect("a directory");
    let body = "漢字かな交じり文。".repeat(65_536 / 9 + 1);
    let body: String = body.chars().take(65_536).collect();
    assert!(body.len() > guardcaps::MAX_COMMAND_BYTES);
    let file = dir.path().join("body.md");
    std::fs::write(&file, &body).expect("the body");
    let cmd = format!("gh pr create --title t --body-file {}", file.display());
    for in_a_plane in [true, false] {
        for mode in ["default", "bypassPermissions"] {
            let refused =
                verdict_in(&cmd, in_a_plane, mode).filter(|v| v.reason == guardcaps::REASON);
            assert_eq!(refused, None, "{mode}, in a plane: {in_a_plane}");
        }
    }
    // Inline, the same body is past the cap, and the refusal names the file route.
    let inline = format!("gh pr create --title t --body-file - <<'EOF'\n{body}\nEOF");
    let said = verdict(&inline, false).expect("refused").said();
    assert!(said.contains("--body-file"), "{said}");
}
