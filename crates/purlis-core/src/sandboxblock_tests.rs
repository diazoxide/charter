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

/// A manifest is protected only where the sandbox holds one (#1336): at the project root and in
/// each folder from the chat's up to it. Below the chat's folder it is the chat's own file.
#[test]
fn a_manifest_is_a_protected_file_only_where_the_sandbox_holds_it() {
    for (path, kind) in [
        (format!("{ROOT}/purlis.toml"), Kind::ProtectedFile),
        (format!("{ROOT}/charter.local.toml"), Kind::ProtectedFile),
        (
            format!("{ROOT}/workspaces/charter.toml"),
            Kind::ProtectedFile,
        ),
        (format!("{CHAT}/purlis.local.toml"), Kind::ProtectedFile),
        (format!("{CHAT}/sub/purlis.toml"), Kind::ChatFolder),
        (
            format!("{ROOT}/workspaces/beta/charter.toml"),
            Kind::ProjectFiles,
        ),
    ] {
        assert_eq!(kind_of(Path::new(&path), &place()), kind, "{path}");
    }
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
    // Its own folder and its temp folder (Claude Code's, under /tmp) are writable: a refusal
    // there is something else (a file's own flags, macOS's privacy controls), never a sandbox
    // block. macOS's per-user folders under /var/folders are not, and a write refused there is
    // the next test's.
    for line in [
        "touch: /Users/dev/plane/workspaces/alpha/repo/src/a.rs: Operation not permitted",
        "rm: /private/tmp/claude-501/x: Operation not permitted",
        "touch: src/a.rs: Operation not permitted",
    ] {
        assert!(
            detect(&came_back("x", "", line), &place()).is_empty(),
            "{line}"
        );
    }
}

const PER_USER_T: &str = "/var/folders/wl/hs3pdt2j0t9gyc_l73py38l80000gp/T";
const PER_USER_C: &str = "/var/folders/wl/hs3pdt2j0t9gyc_l73py38l80000gp/C";

/// What macOS tools that ignore `TMPDIR` printed in a sandboxed chat (#1120, measured on macOS
/// 26.2): they ask the system for the per-user temporary or cache folder instead, which no
/// sandboxed chat may write, so each is a block the notice names with its way round.
#[test]
fn a_tool_that_ignores_tmpdir_is_told_apart_from_the_chats_own_temp() {
    let (t, c) = (PER_USER_T, PER_USER_C);
    for (line, kind) in [
        (
            format!("mktemp: mkstemp failed on {t}/tmp.mWjq8inJrH: Operation not permitted"),
            Kind::SystemTemp,
        ),
        (
            format!("mktemp: mkdtemp failed on {t}/tmp.ZqSxZqUwQX: Operation not permitted"),
            Kind::SystemTemp,
        ),
        (
            format!("rm: /private{t}/x: Operation not permitted"),
            Kind::SystemTemp,
        ),
    ] {
        assert_eq!(
            detect(&came_back("x", "", &line), &place()),
            vec![block(Operation::Write, kind, false)],
            "{line}"
        );
    }
    for (line, kind) in [
        (
            format!("mktemp(1) deny(1) file-write-create /private{t}/tmp.G9c8tiJuCM"),
            Kind::SystemTemp,
        ),
        (
            format!("xcrun(2) deny(1) file-write-create /private{t}/xcrun_db-5UlOOWst"),
            Kind::SystemTemp,
        ),
        (
            format!("swift(3) deny(1) file-write-create /private{t}/p1120.txt"),
            Kind::SystemTemp,
        ),
        (
            format!(
                "swift-frontend(4) deny(1) file-write-create \
                 /private{c}/clang/ModuleCache/1XGORMFR2JUL/SwiftShims-6PVXR3VD9JVP.pcm"
            ),
            Kind::SystemCache,
        ),
    ] {
        assert_eq!(
            detect(&failed("x", &appended("Exit code 1", &[&line])), &place()),
            vec![block(Operation::Write, kind, false)],
            "{line}"
        );
    }
}

/// A read of the per-user folders that macOS's privacy controls refuse, not the sandbox: `du`,
/// `find` and `ls` over them print the same words, and are no block.
#[test]
fn a_programs_refused_read_of_the_per_user_folders_is_not_a_block() {
    for line in [
        format!("du: {PER_USER_C}/com.apple.WebKit.WebContent.Sandbox: Operation not permitted"),
        format!("find: {PER_USER_C}/com.apple.x: Operation not permitted"),
        format!("ls: /private{PER_USER_T}/com.apple.y: Operation not permitted"),
    ] {
        assert!(
            detect(&came_back("x", "", &line), &place()).is_empty(),
            "{line}"
        );
    }
}

/// The temp folder's notice gives mktemp's way round; the cache folder's says Swift and clang
/// builds cannot write it yet, and offers no variable to set.
#[test]
fn the_per_user_folders_notices_name_each_folder_and_its_way_round() {
    let temp = block(Operation::Write, Kind::SystemTemp, false).said();
    assert!(temp.contains("temporary folder"), "{temp}");
    assert!(temp.contains("mktemp -p \"$TMPDIR\""), "{temp}");
    let cache = block(Operation::Write, Kind::SystemCache, false).said();
    assert!(cache.contains("cache folder"), "{cache}");
    assert!(cache.contains("Swift and clang builds"), "{cache}");
    assert!(
        !cache.contains("TMPDIR") && !cache.contains("temporary"),
        "{cache}"
    );
}

/// #1416: `system-temp` also holds the per-user folders `0` and `X`, which are no temporary
/// folder, so its sentence names the per-user folders and holds the temporary one up as one of
/// them; and the cache folder's sentence names no harness, so it reads true in any chat.
#[test]
fn the_per_user_folders_sentences_fit_every_folder_and_harness_they_are_said_of() {
    let temp = block(Operation::Write, Kind::SystemTemp, false).said();
    assert!(temp.contains("one of macOS's per-user folders"), "{temp}");
    assert!(!temp.contains("per-user temporary folder"), "{temp}");
    let cache = block(Operation::Write, Kind::SystemCache, false).said();
    for harness in ["Claude Code", "opencode", "Codex"] {
        assert!(!cache.contains(harness), "{cache}");
    }
    assert!(cache.contains("harness runs its own sandbox"), "{cache}");
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

/// A host refused a moment after another is its own block, with its own Notice to allow it:
/// the same block is the same block on the same target, whichever road told it.
#[test]
fn another_host_refused_within_the_minute_is_heard_and_the_same_one_is_not() {
    let mut throttle = Throttle::default();
    let now = std::time::Instant::now();
    let host = block(Operation::Connect, Kind::Host, false);
    assert!(throttle.lets_on(3, &host, Some("api.example.com:443"), now));
    assert!(
        throttle.lets_on(3, &host, Some("cluster.example-k8s.com:6443"), now),
        "another host is its own block"
    );
    assert!(
        !throttle.lets_on(3, &host, Some("API.example.com:443"), now),
        "the same host again at once, in any case"
    );
    assert!(
        throttle.lets_on(3, &host, None, now),
        "a host the report did not name is its own block too"
    );
    assert!(!throttle.lets(3, &host, now), "and is that one again");
    // Still at most a handful a minute, whatever hosts a chat names.
    let let_through = (0..100)
        .filter(|n| throttle.lets_on(3, &host, Some(&format!("h{n}.example:443")), now))
        .count();
    assert_eq!(let_through, Throttle::PER_CHAT - 3);
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

/// A block of a kind this build does not know, written by a newer one, is kept as it was and
/// let go of by its age like any other: a build that reads it is no reason to lose the rest.
#[test]
fn a_block_of_a_kind_this_build_does_not_know_is_kept_across_a_record() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let now = 100 * DAY;
    let file = path(root);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(
        &file,
        json!({"blocks": [
            {"at": now - DAY, "operation": "write", "kind": "a-kind-from-later", "ours": false},
            {"at": now - 8 * DAY, "operation": "write", "kind": "a-kind-from-later", "ours": false},
            {"at": now - DAY, "operation": "write", "kind": "home", "ours": false},
        ]})
        .to_string(),
    )
    .unwrap();
    record(root, &block(Operation::Read, Kind::Home, false), now).unwrap();
    let text = std::fs::read_to_string(&file).unwrap();
    assert_eq!(text.matches("a-kind-from-later").count(), 1, "{text}");
    assert_eq!(text.matches("\"at\"").count(), 3, "{text}");
    assert_eq!(
        counts(root, now),
        vec![
            Count {
                operation: Operation::Write,
                blocks: 1,
                ours: 0
            },
            Count {
                operation: Operation::Read,
                blocks: 1,
                ours: 0
            },
        ]
    );
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

// ---- what a grant could name (#1342) --------------------------------------------------------

#[test]
fn a_refused_write_or_host_carries_what_a_grant_would_name_and_nothing_else_does() {
    let targets = |payload: serde_json::Value| -> Vec<Option<String>> {
        detect_with_targets(&payload, &place())
            .into_iter()
            .map(|(_, target)| target)
            .collect()
    };
    // A write outside the project, as Seatbelt names it, and a relative one read from the cwd.
    assert_eq!(
        targets(failed(
            "x",
            &appended(
                "",
                &[
                    "cargo(1) deny(1) file-write-create /Users/dev/.cargo/registry/x.lock",
                    "touch(2) deny(1) file-write-create /opt/tool/cache",
                ]
            )
        )),
        [
            Some("/Users/dev/.cargo/registry/x.lock".to_owned()),
            Some("/opt/tool/cache".to_owned())
        ]
    );
    assert_eq!(
        targets(came_back(
            "x",
            "",
            "touch: ../../beta/x: Operation not permitted"
        )),
        [Some("/Users/dev/plane/workspaces/beta/x".to_owned())]
    );
    // A host, where the report names one, and none where it does not.
    assert_eq!(
        targets(came_back(
            "x",
            "",
            "purlis's sandbox does not allow example.org:443: no egress preset or host of this \
             project lists it"
        )),
        [Some("example.org:443".to_owned())]
    );
    assert_eq!(
        targets(failed(
            "x",
            &appended("", &["curl(10) deny(1) network-outbound 93.184.216.34:443"])
        )),
        [Some("93.184.216.34:443".to_owned())]
    );
    assert_eq!(
        targets(came_back(
            "x",
            "",
            "Connection blocked by network allowlist"
        )),
        [None]
    );
    // Never for purlis's own state, a protected file, a read, a socket, or purlis's own process.
    for line in [
        "git(3) deny(1) file-write-create /Users/dev/plane/.purlis/app/x",
        "git(3) deny(1) file-write-create /Users/dev/plane/workspaces/alpha/repo/.git/config",
        "cat(4) deny(1) file-read-data /Users/dev/.config/purlis/vaults/x",
        "nc(5) deny(1) network-outbound /private/tmp/purlis-501/hooks.sock",
        "purlis(6) deny(1) file-write-create /Users/dev/plane/workspaces/beta/sessions/x.md",
    ] {
        assert_eq!(
            targets(failed("x", &appended("", &[line]))),
            [None],
            "{line}"
        );
    }
    // What `detect` keeps is the block alone, as it was.
    assert_eq!(
        detect(
            &failed(
                "x",
                &appended("", &["touch(2) deny(1) file-write-create /opt/x"])
            ),
            &place()
        ),
        [block(Operation::Write, Kind::System, false)]
    );
}

#[test]
fn purlis_s_own_refusal_wording_is_purlis_s_own_block_with_nothing_to_grant() {
    // #1421: on Codex and opencode no violation block is appended, and neither of purlis's own
    // sentences ends the way a program's refusal does, so neither showed any notice. Each is
    // purlis's own write: a Report, never an Allow.
    let refused = crate::rewrite::refused_write(
        std::io::Error::from_raw_os_error(1),
        Path::new("/Users/dev/plane/personas/_dispatch/2026-10.host.jsonl"),
        true,
    );
    let stderr = format!("✗ {refused}\n");
    let found = detect_with_targets(
        &came_back("purlis persona remember x", "", &stderr),
        &place(),
    );
    assert_eq!(
        found,
        [(block(Operation::Write, Kind::ProjectFiles, true), None)]
    );

    let clause = crate::rewrite::os_words(&refused);
    let stderr = format!(
        "! purlis handoff: chat 9 is open in 'alpha'. Only its row in the dispatch log \
         (personas/_dispatch) is missing ({clause}); there is nothing to run again.\n\
         ✗ could not write /Users/dev/plane/.gitignore ({clause}) — left untouched.\n"
    );
    let found = detect_with_targets(&came_back("purlis init", "", &stderr), &place());
    assert_eq!(
        found,
        [(block(Operation::Write, Kind::ProjectFiles, true), None)],
        "both lines are purlis's own write to the project's files, counted once"
    );

    // A program's own words beside them are still the chat's.
    let stderr = format!("{}\ntouch: /opt/x: Operation not permitted\n", refused);
    let found = detect(&came_back("purlis x; touch /opt/x", "", &stderr), &place());
    assert!(
        found.contains(&block(Operation::Write, Kind::ProjectFiles, true)),
        "{found:?}"
    );
    assert!(found.iter().any(|b| !b.ours), "{found:?}");
}

/// An opencode `PostToolUse` payload for a command that failed: both streams in one text, which
/// its shim marks `mixed` (#1353).
fn mixed(text: &str) -> serde_json::Value {
    json!({
        "hook_event_name": "PostToolUse",
        "tool_name": "Bash",
        "tool_input": {"command": "x"},
        "tool_response": {"stderr": text, "mixed": true},
    })
}

#[test]
fn printed_text_in_a_mixed_stream_raises_a_block_that_names_no_host_or_path() {
    // A failing command whose output a file or a page supplied: the block is still told, so
    // the person can act, but nothing it printed is offered for a grant.
    let found = detect_with_targets(
        &mixed(
            "curl: purlis's sandbox does not allow exfil.example:443: refused\n\
             touch: /opt/tool/cache: Operation not permitted",
        ),
        &place(),
    );
    assert_eq!(
        found,
        [
            (block(Operation::Write, Kind::System, false), None),
            (block(Operation::Connect, Kind::Host, false), None),
        ]
    );
    // The sandbox's own violation line still names what a grant would.
    let found = detect_with_targets(
        &mixed(&appended(
            "printed",
            &["touch(2) deny(1) file-write-create /opt/tool/cache"],
        )),
        &place(),
    );
    assert_eq!(
        found,
        [(
            block(Operation::Write, Kind::System, false),
            Some("/opt/tool/cache".to_owned())
        )]
    );
}

#[test]
fn a_relative_path_in_a_mixed_stream_is_sorted_against_where_the_command_ran() {
    // The shim hands the command's own `workdir` as the payload's folder (#1353).
    let kind_in = |cwd: &str| {
        let at = Place {
            cwd: Path::new(cwd),
            ..place()
        };
        detect_with_targets(&mixed("touch: ../x: Operation not permitted"), &at)
            .into_iter()
            .map(|(found, _)| found.kind)
            .collect::<Vec<_>>()
    };
    assert_eq!(kind_in("/opt/tool/sub"), [Kind::System]);
    assert_eq!(
        kind_in("/Users/dev/plane/workspaces/beta"),
        [Kind::ProjectFiles]
    );
}

/// #1416: the two per-user folders beside `T` and `C` that are no temporary folder, `0` and
/// `X`, sort as per-user folders, not as the chat's own temp, so a write refused there is not
/// dropped from standard error; and none of them is offered as a grant's target.
#[test]
fn the_per_user_folders_0_and_x_sort_as_per_user_folders_and_offer_no_grant() {
    let at = "/var/folders/wl/hs3pdt2j0t9gyc_l73py38l80000gp";
    for which in ["0", "X"] {
        for spelled in [
            format!("{at}/{which}/com.apple.x/y"),
            format!("/private{at}/{which}/com.apple.x/y"),
        ] {
            assert_eq!(
                kind_of(Path::new(&spelled), &place()),
                Kind::SystemTemp,
                "{spelled}"
            );
            let line = format!("touch: {spelled}: Operation not permitted");
            assert_eq!(
                detect_with_targets(&came_back("x", "", &line), &place()),
                vec![(block(Operation::Write, Kind::SystemTemp, false), None)],
                "{line}"
            );
        }
    }
    // A read there is macOS's privacy controls, as for `T` and `C`.
    let read = format!("du: {at}/0/com.apple.x: Operation not permitted");
    assert!(detect(&came_back("du", "", &read), &place()).is_empty());
}

/// #1416: the data volume's own spelling of a path, `/System/Volumes/Data/…`, sorts as the path
/// it names, and so is never offered as a grant's target where that path would not be.
#[test]
fn a_path_spelled_through_the_data_volume_sorts_as_the_path_it_names() {
    let data = "/System/Volumes/Data";
    for (path, kind) in [
        (format!("{data}{PER_USER_T}/tmp.x"), Kind::SystemTemp),
        (
            format!("{data}/private{PER_USER_T}/tmp.x"),
            Kind::SystemTemp,
        ),
        (
            format!("{data}/private{PER_USER_C}/clang/ModuleCache/a.pcm"),
            Kind::SystemCache,
        ),
        (format!("{data}{ROOT}/.purlis/app/x"), Kind::ProjectState),
        (format!("{data}{CHAT}/src/a.rs"), Kind::ChatFolder),
        (
            format!("{data}{HOME}/.cargo/registry/x"),
            Kind::ToolchainCache,
        ),
        (format!("{data}/private/tmp/x"), Kind::Temp),
    ] {
        assert_eq!(kind_of(Path::new(&path), &place()), kind, "{path}");
    }
    let line =
        format!("xcrun(2) deny(1) file-write-create {data}/private{PER_USER_T}/xcrun_db-5UlOOWst");
    assert_eq!(
        detect_with_targets(&failed("x", &appended("Exit code 1", &[&line])), &place()),
        vec![(block(Operation::Write, Kind::SystemTemp, false), None)]
    );
}

/// #1416: swiftc's and xcrun's own words for a write the sandbox refused them, which name the
/// per-user folder in quotes and the system's words after it.
#[test]
fn swiftc_and_xcrun_refusals_are_read_from_standard_error() {
    let (t, c) = (PER_USER_T, PER_USER_C);
    for (line, kind) in [
        (
            format!(
                "<unknown>:0: error: unable to open output file \
                 '{c}/clang/ModuleCache/1XGORMFR2JUL/SwiftShims-6PVXR3VD9JVP.pcm': \
                 'Operation not permitted'"
            ),
            Kind::SystemCache,
        ),
        (
            format!(
                "error: unable to open output file '/private{c}/clang/ModuleCache/x.pcm': \
                 'Operation not permitted'"
            ),
            Kind::SystemCache,
        ),
        (
            format!(
                "xcrun: error: couldn't create cache file '{t}/xcrun_db-5UlOOWst' \
                 (errno=Operation not permitted)"
            ),
            Kind::SystemTemp,
        ),
    ] {
        assert_eq!(
            detect_with_targets(&came_back("swiftc a.swift", "", &line), &place()),
            vec![(block(Operation::Write, kind, false), None)],
            "{line}"
        );
    }
    // The same words about a path the chat may write are not the sandbox's.
    let own =
        format!("error: unable to open output file '{CHAT}/build/x.o': 'Operation not permitted'");
    assert!(detect(&came_back("x", "", &own), &place()).is_empty());
    // Nor is a line that only quotes them in the middle.
    let quoted = format!(
        "note: saw \"couldn't create cache file '{t}/xcrun_db' (errno=Operation not \
         permitted)\" earlier"
    );
    assert!(detect(&came_back("x", "", &quoted), &place()).is_empty());
}

/// #1416: Foundation's refusal of a write, as a Swift program prints the error, names the path
/// in its `NSFilePath` and the system's words in its underlying POSIX error. Read as any other
/// program's own words: sorted by the path, and only outside what the chat may write. A line
/// that names no path says nothing a block could be sorted by, and is not one, as
/// `ps: Operation not permitted` is not.
#[test]
fn foundations_refused_write_is_read_by_the_path_it_names() {
    let foundation = |path: &str| {
        format!(
            "Fatal error: Error Domain=NSCocoaErrorDomain Code=513 \"You don’t have permission to \
             save the file “p1120.txt” in the folder “T”.\" UserInfo={{NSFilePath={path}, \
             NSUnderlyingError=0x600000c84030 {{Error Domain=NSPOSIXErrorDomain Code=1 \
             \"Operation not permitted\"}}}}"
        )
    };
    for (path, kind) in [
        (format!("{PER_USER_T}/p1120.txt"), Kind::SystemTemp),
        (format!("/private{PER_USER_T}/p1120.txt"), Kind::SystemTemp),
        (
            "/Users/dev/plane/workspaces/beta/x.txt".to_owned(),
            Kind::ProjectFiles,
        ),
    ] {
        let line = foundation(&path);
        assert_eq!(
            detect(&came_back("swift p.swift", "", &line), &place()),
            vec![block(Operation::Write, kind, false)],
            "{line}"
        );
    }
    // A path holding a comma or a brace is read whole, and a file name in the description
    // that looks like a key is not the path.
    let odd = "/Users/dev/plane/workspaces/beta/a, b}c/x.txt";
    let line = foundation(odd).replace(
        "the file “p1120.txt”",
        "the file “NSFilePath=/Users/dev/plane/workspaces/alpha/x”",
    );
    let found = detect_with_targets(&came_back("swift p.swift", "", &line), &place());
    assert_eq!(found.len(), 1, "{line}");
    assert_eq!(
        found[0].0,
        block(Operation::Write, Kind::ProjectFiles, false)
    );
    assert!(
        format!("{:?}", found[0]).contains("a, b}c"),
        "{:?}",
        found[0]
    );
    // In what the chat may write, the sandbox did not refuse it.
    let own = foundation(&format!("{CHAT}/out.txt"));
    assert!(detect(&came_back("x", "", &own), &place()).is_empty());
    // With no path, or with another underlying error, there is nothing to sort.
    for line in [
        "Error Domain=NSCocoaErrorDomain Code=513 \"You don’t have permission.\" \
         UserInfo={NSUnderlyingError=0x6 {Error Domain=NSPOSIXErrorDomain Code=1 \
         \"Operation not permitted\"}}"
            .to_owned(),
        foundation(&format!("{PER_USER_T}/p.txt")).replace(
            "NSPOSIXErrorDomain Code=1 \"Operation not permitted\"",
            "NSPOSIXErrorDomain Code=13 \"Permission denied\"",
        ),
    ] {
        assert!(
            detect(&came_back("x", "", &line), &place()).is_empty(),
            "{line}"
        );
    }
}

/// The path between a line's last two quotes, past a word's apostrophe before them.
#[test]
fn the_last_quoted_path_is_read_past_an_apostrophe() {
    assert_eq!(last_quoted("cannot copy 'a' to 'b'"), Some("b"));
    assert_eq!(
        last_quoted("couldn't create cache file '/x/y'"),
        Some("/x/y")
    );
    assert_eq!(last_quoted("failed to create directory `/x`"), Some("/x"));
    assert_eq!(last_quoted("can't open /x"), None);
}
