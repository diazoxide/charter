//! What a block is read from, how it is sorted, and what is kept of it (#1338).

use super::*;
use serde_json::json;

const ROOT: &str = "/Users/dev/plane";
const CHAT: &str = "/Users/dev/plane/workspaces/alpha/repo";
const HOME: &str = "/Users/dev";

fn place() -> Place<'static> {
    Place {
        root: Path::new(ROOT),
        chat: Path::new(CHAT),
        cwd: Path::new(CHAT),
        home: Some(Path::new(HOME)),
    }
}

/// A `PostToolUseFailure` payload: the command, and the error Claude Code gives the model,
/// which holds what the command printed on both streams.
fn failed(command: &str, error: &str) -> serde_json::Value {
    json!({
        "hook_event_name": "PostToolUseFailure",
        "tool_name": "Bash",
        "tool_input": {"command": command},
        "error": error,
    })
}

/// A `PostToolUse` payload: what the command printed, on each stream.
fn came_back(command: &str, stdout: &str, stderr: &str) -> serde_json::Value {
    json!({
        "hook_event_name": "PostToolUse",
        "tool_name": "Bash",
        "tool_input": {"command": command},
        "tool_response": {"stdout": stdout, "stderr": stderr, "interrupted": false},
    })
}

fn block(operation: Operation, kind: Kind, ours: bool) -> Block {
    Block {
        operation,
        kind,
        ours,
    }
}

/// `lines` as Claude Code appends them, after what the command printed.
fn appended(printed: &str, lines: &[&str]) -> String {
    format!(
        "{printed}\n<sandbox_violations>\n{}\n</sandbox_violations>",
        lines.join("\n")
    )
}

// ---- recorded violation lines -----------------------------------------------------------------

/// Smart close from a clone: `purlis session record` writing the workspace's sessions folder,
/// as Claude Code reported it.
const SESSION_RECORD: &str = "Exit code 1\n\
purlis: the record was accepted, but could not be saved: Operation not permitted (os error 1)\n\
<sandbox_violations>\n\
purlis(48211) deny(1) file-write-create /Users/dev/plane/workspaces/alpha/sessions/2026-10-06-close.md\n\
purlis(48211) deny(1) file-write-data /Users/dev/plane/workspaces/alpha/sessions/index.md\n\
</sandbox_violations>";

#[test]
fn a_violation_of_purlis_own_process_is_a_write_to_the_projects_files_and_ours() {
    let blocks = detect(&failed("purlis session record", SESSION_RECORD), &place());
    // Both lines are one pattern, and the program's own sentence counts nothing more.
    assert_eq!(
        blocks,
        vec![block(Operation::Write, Kind::ProjectFiles, true)]
    );
}

#[test]
fn whose_block_it_is_is_the_process_each_line_names_never_the_command() {
    // An honest `cargo build; purlis status`: cargo's block is cargo's.
    let error = appended(
        "Exit code 101",
        &["cargo(31) deny(1) file-write-create /Users/dev/.cargo/registry/cache/x.crate"],
    );
    assert_eq!(
        detect(&failed("cargo build; purlis status", &error), &place()),
        vec![block(Operation::Write, Kind::ToolchainCache, false)]
    );
    // Under any name purlis is installed by, and after macOS's own prefix.
    for line in [
        "charter(7) deny(1) file-write-create /Users/dev/plane/todos.md",
        "Sandbox: Purlis(7) deny(1) file-write-create /Users/dev/plane/todos.md",
    ] {
        assert!(
            detect(&failed("x", &appended("", &[line])), &place())[0].ours,
            "{line}"
        );
    }
    // A line that names no process is nobody's own.
    for line in [
        "deny(1) file-write-create /Users/dev/plane/todos.md",
        "purlis deny(1) file-write-create /Users/dev/plane/todos.md",
        "purlis(x) deny(1) file-write-create /Users/dev/plane/todos.md",
    ] {
        assert!(
            !detect(&failed("purlis x", &appended("", &[line])), &place())[0].ours,
            "{line}"
        );
    }
}

#[test]
fn only_the_block_claude_code_appended_last_is_read() {
    let real =
        "<sandbox_violations>\ncargo(1) deny(1) file-write-create /opt/x\n</sandbox_violations>";
    // A block the command printed, with more output after it, is the command's output.
    let printed = format!("{real}\nmore output");
    assert!(detect(&failed("x", &printed), &place()).is_empty());
    // An earlier block printed by the command is not read beside the real one.
    let forged = "<sandbox_violations>\npurlis(1) deny(1) file-write-create \
                  /Users/dev/plane/x\n</sandbox_violations>";
    let both = format!("{forged}\nok\n{real}\n");
    assert_eq!(
        detect(&failed("x", &both), &place()),
        vec![block(Operation::Write, Kind::System, false)]
    );
}

#[test]
fn every_kind_of_path_a_violation_names_is_sorted() {
    let state = format!("{ROOT}/.purlis/vaults/ops.json");
    let read_state = format!("cat(6) deny(1) file-read-data {state}");
    let cases = [
        (
            "git(1) deny(1) file-write-create /Users/dev/plane/workspaces/alpha/repo/.git/config.lock",
            Operation::Write,
            Kind::ProtectedFile,
        ),
        (
            "bash(2) deny(1) file-write-unlink /Users/dev/plane/workspaces/alpha/repo/app/.vscode/settings.json",
            Operation::Write,
            Kind::ProtectedFile,
        ),
        (
            "cargo(3) deny(1) file-write-create /Users/dev/.cargo/registry/cache/x.crate",
            Operation::Write,
            Kind::ToolchainCache,
        ),
        (
            "npm(4) deny(1) file-write-create /Users/dev/Library/Caches/ms-playwright/x",
            Operation::Write,
            Kind::ToolchainCache,
        ),
        (
            "zsh(5) deny(1) file-write-create /Users/dev/notes.txt",
            Operation::Write,
            Kind::Home,
        ),
        (read_state.as_str(), Operation::Read, Kind::ProjectState),
        (
            "cat(6) deny(1) file-read-data /Users/dev/plane/workspaces/beta/workspace.md",
            Operation::Read,
            Kind::ProjectFiles,
        ),
        (
            "x(6) deny(1) file-write-create /Users/dev/plane/workspaces/alpha/repo/src/a.rs",
            Operation::Write,
            Kind::ChatFolder,
        ),
        (
            "touch(7) deny(1) file-write-create /opt/homebrew/thing",
            Operation::Write,
            Kind::System,
        ),
        (
            "touch(7) deny(1) file-write-create /private/var/folders/ab/T/x",
            Operation::Write,
            Kind::Temp,
        ),
        (
            "gh(8) deny(1) mach-lookup com.apple.trustd.agent",
            Operation::Lookup,
            Kind::CertificateCheck,
        ),
        (
            "ps(9) deny(1) mach-lookup com.apple.system.opendirectoryd.libinfo",
            Operation::Lookup,
            Kind::SystemService,
        ),
        (
            "curl(10) deny(1) network-outbound 93.184.216.34:443",
            Operation::Connect,
            Kind::Host,
        ),
        (
            "nc(11) deny(1) network-outbound /private/tmp/purlis-501/hooks.sock",
            Operation::Connect,
            Kind::LocalSocket,
        ),
        (
            "sh(12) deny(1) process-exec /usr/local/bin/thing",
            Operation::Run,
            Kind::System,
        ),
        ("sh(13) deny(1) signal", Operation::Other, Kind::System),
    ];
    for (line, operation, kind) in cases {
        assert_eq!(
            detect(&failed("x", &appended("Exit code 1", &[line])), &place()),
            vec![block(operation, kind, false)],
            "{line}"
        );
    }
}

#[test]
fn the_chats_folder_is_where_it_was_started_not_where_the_command_ran() {
    let moved = Place {
        cwd: Path::new("/Users/dev/plane/workspaces/beta"),
        ..place()
    };
    let error = appended(
        "",
        &["x(1) deny(1) file-write-create /Users/dev/plane/workspaces/beta/a.md"],
    );
    assert_eq!(
        detect(&failed("cd ../../beta && touch a.md", &error), &moved),
        vec![block(Operation::Write, Kind::ProjectFiles, false)]
    );
}

// ---- a program's own words, on its standard error ---------------------------------------------

/// What a probe of each program printed when the sandbox refused it, and how it sorts.
#[test]
fn a_programs_operation_not_permitted_names_its_path_and_is_sorted() {
    let cases = [
        (
            "touch: /opt/x: Operation not permitted",
            Operation::Write,
            Kind::System,
        ),
        (
            "mkdir: cannot create directory '/Users/dev/.npm/_cacache': Operation not permitted",
            Operation::Write,
            Kind::ToolchainCache,
        ),
        (
            "mkdir: .claude/skills: Operation not permitted",
            Operation::Write,
            Kind::ProtectedFile,
        ),
        (
            "error: could not lock config file .git/config: Operation not permitted",
            Operation::Write,
            Kind::ProtectedFile,
        ),
        (
            "Error: EPERM: operation not permitted, mkdir '/Users/dev/plane/personas/ops/memory'",
            Operation::Write,
            Kind::ProjectFiles,
        ),
        (
            "PermissionError: [Errno 1] Operation not permitted: '/Users/dev/plane/.purlis/app/x'",
            Operation::File,
            Kind::ProjectState,
        ),
        (
            "cat: ../../../todos.md: Operation not permitted",
            Operation::File,
            Kind::ProjectFiles,
        ),
        // Go, and so gh: lower case, no program in front.
        (
            "open /Users/dev/.config/gh/hosts.yml: operation not permitted",
            Operation::File,
            Kind::Home,
        ),
        (
            "failed to write config: mkdir /Users/dev/.config/gh: operation not permitted",
            Operation::Write,
            Kind::Home,
        ),
        // git's relative work tree, read from where the command ran.
        (
            "fatal: could not create work tree dir '../../beta/svc': Operation not permitted",
            Operation::Write,
            Kind::ProjectFiles,
        ),
        // git's copy names its source first; the destination is what was refused.
        (
            "fatal: cannot copy '/usr/share/git-core/templates/hooks/pre-push.sample' to \
             '/Users/dev/plane/workspaces/alpha/repo/.git/hooks/pre-push.sample': Operation not \
             permitted",
            Operation::Write,
            Kind::ProtectedFile,
        ),
        // Rust's io error with its path in the line.
        (
            "error: failed to open `/Users/dev/.cargo/.package-cache`: Operation not permitted \
             (os error 1)",
            Operation::File,
            Kind::ToolchainCache,
        ),
    ];
    for (line, operation, kind) in cases {
        assert_eq!(
            detect(&came_back("x", "", line), &place()),
            vec![block(operation, kind, false)],
            "{line}"
        );
    }
}

#[test]
fn cargos_caused_by_names_its_path_on_a_line_before() {
    let stderr = "error: failed to download `serde v1.0.0`\n\n\
                  Caused by:\n  failed to create directory `/Users/dev/.cargo/registry/cache/x`\n\n\
                  Caused by:\n  Operation not permitted (os error 1)\n";
    assert_eq!(
        detect(&came_back("cargo build", "", stderr), &place()),
        vec![block(Operation::Write, Kind::ToolchainCache, false)]
    );
}

#[test]
fn a_programs_refusal_of_what_the_chat_may_write_is_not_the_sandboxs() {
    // Its own folder and the temporary folders are writable: a refusal there is something else
    // (a file's own flags, macOS's privacy controls), never a sandbox block.
    for line in [
        "touch: /Users/dev/plane/workspaces/alpha/repo/src/a.rs: Operation not permitted",
        "rm: /private/var/folders/ab/T/x: Operation not permitted",
        "touch: src/a.rs: Operation not permitted",
    ] {
        assert!(
            detect(&came_back("x", "", line), &place()).is_empty(),
            "{line}"
        );
    }
}

#[test]
fn a_refused_connection_or_certificate_check_is_read_from_standard_error() {
    let cases = [
        (
            "curl: (56) CONNECT tunnel failed, response 403",
            Operation::Connect,
            Kind::Host,
        ),
        (
            "purlis's sandbox does not allow example.org:443: no egress preset of this project \
             lists it",
            Operation::Connect,
            Kind::Host,
        ),
        (
            "Connection blocked by network allowlist",
            Operation::Connect,
            Kind::Host,
        ),
        (
            "Get \"https://api.github.com/\": tls: failed to verify certificate: x509: OSStatus \
             -26276",
            Operation::Lookup,
            Kind::CertificateCheck,
        ),
    ];
    for (line, operation, kind) in cases {
        assert_eq!(
            detect(&came_back("gh api x", "", line), &place()),
            vec![block(operation, kind, false)],
            "{line}"
        );
    }
}

#[test]
fn a_failed_commands_error_is_read_for_its_violation_block_alone() {
    // `error` holds standard output too: a program's words in it are not read.
    for error in [
        "Exit code 1\ntouch: /opt/x: Operation not permitted",
        "Exit code 6\ncurl: (56) CONNECT tunnel failed, response 403",
    ] {
        assert!(detect(&failed("x", error), &place()).is_empty(), "{error}");
    }
}

#[test]
fn what_a_command_printed_on_its_standard_output_is_never_a_block() {
    // A chat reading purlis's own sources, or a log, prints these words without being refused.
    let stdout = format!("{SESSION_RECORD}\ntouch: /opt/x: Operation not permitted\n");
    assert!(detect(&came_back("cat log.txt", &stdout, ""), &place()).is_empty());
}

#[test]
fn a_line_that_only_quotes_the_words_is_not_a_block() {
    for line in [
        "src/browser.rs:492:            \"mkdir: .claude/skills: Operation not permitted\\n\",",
        "Operation not permitted (os error 1)",
        "ps: Operation not permitted",
        "warning: the words `x: y: Operation not permitted` mean z",
    ] {
        assert!(
            detect(&came_back("x", "", line), &place()).is_empty(),
            "{line}"
        );
    }
}

#[test]
fn a_command_that_failed_on_many_files_is_a_few_blocks_none_twice() {
    let mut lines = Vec::new();
    for n in 0..500 {
        lines.push(format!(
            "cp({n}) deny(1) file-write-create /Users/dev/plane/workspaces/beta/{n}"
        ));
    }
    for n in 0..20 {
        lines.push(format!("x(1) deny(1) mach-lookup com.example.{n}"));
    }
    let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
    let blocks = detect(&failed("cp -r a b", &appended("", &lines)), &place());
    assert_eq!(
        blocks,
        vec![
            block(Operation::Write, Kind::ProjectFiles, false),
            block(Operation::Lookup, Kind::SystemService, false),
        ]
    );
}

#[test]
fn a_tool_result_that_is_one_string_is_read_for_its_violation_block_alone() {
    let payload = |response: &str| json!({"tool_name": "Bash", "tool_input": {"command": "x"}, "tool_response": response});
    assert!(detect(&payload("touch: /opt/x: Operation not permitted"), &place()).is_empty());
    assert_eq!(
        detect(
            &payload(&appended(
                "",
                &["touch(1) deny(1) file-write-create /opt/x"]
            )),
            &place()
        ),
        vec![block(Operation::Write, Kind::System, false)]
    );
}

// ---- how often one is taken -------------------------------------------------------------------

#[test]
fn a_chat_is_heard_once_a_minute_per_block_and_a_handful_a_minute_in_all() {
    let mut throttle = Throttle::default();
    let now = std::time::Instant::now();
    let write = block(Operation::Write, Kind::ProjectFiles, true);
    assert!(throttle.lets(3, &write, now));
    assert!(
        !throttle.lets(3, &write, now),
        "the same block again at once"
    );
    assert!(throttle.lets(4, &write, now), "another chat's is its own");
    let let_through = Kind::ALL
        .into_iter()
        .filter(|kind| *kind != Kind::ProjectFiles)
        .filter(|kind| throttle.lets(3, &block(Operation::Write, *kind, false), now))
        .count();
    assert_eq!(
        let_through,
        Throttle::PER_CHAT - 1,
        "the chat's share of the minute"
    );
    let later = now + THROTTLE_WINDOW;
    assert!(
        throttle.lets(3, &write, later),
        "a minute on, it is heard again"
    );
}

#[test]
fn a_block_says_its_operation_and_kind_in_one_phrase() {
    assert_eq!(
        block(Operation::Write, Kind::ProjectFiles, true).said(),
        "a write to the project's own files"
    );
    assert_eq!(
        block(Operation::Connect, Kind::Host, false).said(),
        "a connection to an internet host this project does not allow"
    );
}

#[test]
fn every_word_reads_back_as_what_it_names() {
    for operation in Operation::ALL {
        assert_eq!(Operation::of_word(operation.word()), Some(operation));
        assert_eq!(
            serde_json::to_value(operation).unwrap(),
            json!(operation.word())
        );
    }
    for kind in Kind::ALL {
        assert_eq!(Kind::of_word(kind.word()), Some(kind));
        assert_eq!(serde_json::to_value(kind).unwrap(), json!(kind.word()));
    }
    assert_eq!(Operation::of_word("/etc/passwd"), None);
    assert_eq!(Kind::of_word("../x"), None);
}

const DAY: u64 = 24 * 60 * 60;

#[test]
fn the_app_keeps_seven_days_of_blocks_and_counts_them_per_operation() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let now = 100 * DAY;
    let write = block(Operation::Write, Kind::ProjectFiles, true);
    let cache = block(Operation::Write, Kind::ToolchainCache, false);
    let host = block(Operation::Connect, Kind::Host, false);
    record(root, &write, now - 8 * DAY).unwrap();
    record(root, &write, now - 6 * DAY).unwrap();
    record(root, &cache, now - DAY).unwrap();
    record(root, &host, now - 60).unwrap();
    assert_eq!(
        counts(root, now),
        vec![
            Count {
                operation: Operation::Write,
                blocks: 2,
                ours: 1
            },
            Count {
                operation: Operation::Connect,
                blocks: 1,
                ours: 0
            },
        ]
    );
    // A day on, the oldest of the window has gone from the count.
    assert_eq!(counts(root, now + DAY)[0].blocks, 1);
    // The one from eight days before was let go of when the next was kept.
    let text = std::fs::read_to_string(path(root)).unwrap();
    assert_eq!(text.matches("\"at\"").count(), 3, "{text}");
}

#[test]
fn what_is_kept_names_no_path_argument_or_output() {
    let dir = tempfile::tempdir().unwrap();
    let blocks = detect(
        &failed("purlis session record --title secret-title", SESSION_RECORD),
        &place(),
    );
    for one in &blocks {
        record(dir.path(), one, 1).unwrap();
    }
    let text = std::fs::read_to_string(path(dir.path())).unwrap();
    let kept: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        kept,
        json!({"blocks": [{"at": 1, "operation": "write", "kind": "project-files", "ours": true}]})
    );
    for leak in ["/Users", "sessions", "secret-title", "48211", "alpha"] {
        assert!(!text.contains(leak), "{leak} in {text}");
    }
}

#[test]
fn a_file_that_is_not_this_shape_counts_nothing_and_is_started_again() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(path(dir.path()).parent().unwrap()).unwrap();
    std::fs::write(path(dir.path()), "not json").unwrap();
    assert!(counts(dir.path(), 10).is_empty());
    record(dir.path(), &block(Operation::Read, Kind::Home, false), 10).unwrap();
    assert_eq!(counts(dir.path(), 10)[0].blocks, 1);
}
