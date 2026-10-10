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
fn a_failed_commands_error_is_read_for_its_violation_block_and_network_refusals_alone() {
    // `error` holds standard output too: a program's refusal of a path in it is not read. A
    // refusal of the network is, as standard output's is (#1663), naming no host to allow.
    assert!(
        detect(
            &failed("x", "Exit code 1\ntouch: /opt/x: Operation not permitted"),
            &place()
        )
        .is_empty()
    );
    assert_eq!(
        detect_with_targets(
            &failed(
                "x",
                "Exit code 6\ncurl: (56) CONNECT tunnel failed, response 403"
            ),
            &place()
        ),
        vec![(block(Operation::Connect, Kind::Host, false), None)]
    );
}

#[test]
fn what_a_command_printed_on_its_standard_output_is_never_a_block_of_a_path() {
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

// ---- Claude Code's proxy (#1631) --------------------------------------------------------------

/// What a command that reached three hosts through Claude Code's proxy came back with, exit 0:
/// curl printed `000` for each, and Claude Code appended its report to standard error. A refused
/// connection is said by the proxy, not Seatbelt: no process, no `deny(`, and its reason after.
const PROXY_REFUSED_THREE: &str = "https://api.example.com/health 000\n\
https://console.example.com/gateway/v1/ready 000\n\
https://mcp.example.com/health 000\n\
Shell cwd was reset to /Users/dev/plane/workspaces/alpha/repo\n\
<sandbox_violations>\n\
deny network-outbound api.example.com:443 (host is not on the allow list)\n\
deny network-outbound console.example.com:443 (host is not on the allow list)\n\
deny network-outbound mcp.example.com:443 (host is not on the allow list)\n\
</sandbox_violations>";

#[test]
fn a_host_claude_codes_proxy_refused_is_a_block_for_each_host_with_the_host_to_allow() {
    let found = detect_with_targets(&came_back("curl …", "", PROXY_REFUSED_THREE), &place());
    let host = block(Operation::Connect, Kind::Host, false);
    assert_eq!(
        found,
        vec![
            (host, Some("api.example.com:443".to_owned())),
            (host, Some("console.example.com:443".to_owned())),
            (host, Some("mcp.example.com:443".to_owned())),
        ]
    );
    // The same, from a command that failed: the block is the sandbox's own, so it is read there
    // too. And one host twice is one block.
    let twice = appended(
        "Exit code 7",
        &[
            "deny network-outbound api.example.com:443 (host is not on the allow list)",
            "deny network-outbound API.example.com:443 (host is not on the allow list)",
        ],
    );
    assert_eq!(
        detect_with_targets(&failed("curl …", &twice), &place()),
        vec![(host, Some("api.example.com:443".to_owned()))]
    );
}

#[test]
fn a_proxy_line_names_only_a_host_never_the_words_after_it() {
    // A reason Claude Code may word otherwise, and a line with none.
    for (line, target) in [
        (
            "deny network-outbound registry.example.org:443 (blocked by managed policy)",
            Some("registry.example.org:443"),
        ),
        ("deny network-outbound 10.0.0.7:6443", Some("10.0.0.7:6443")),
        // No host at all, only a reason: nothing to grant, still a refused connection.
        (
            "deny network-outbound (host is not on the allow list)",
            None,
        ),
    ] {
        assert_eq!(
            detect_with_targets(&came_back("x", "", &appended("", &[line])), &place()),
            vec![(
                block(Operation::Connect, Kind::Host, false),
                target.map(str::to_owned)
            )],
            "{line}"
        );
    }
    // A socket path is a local socket, which no grant names.
    assert_eq!(
        detect_with_targets(
            &came_back(
                "x",
                "",
                &appended(
                    "",
                    &["deny network-outbound /private/tmp/x.sock (not allowed)"]
                )
            ),
            &place()
        ),
        vec![(block(Operation::Connect, Kind::LocalSocket, false), None)]
    );
    // A client's word for the same refusal names no host: it is the one the line named, not
    // a second block.
    let both = format!(
        "curl: (56) CONNECT tunnel failed, response 403\n{}",
        appended(
            "",
            &["deny network-outbound api.example.com:443 (host is not on the allow list)"]
        )
    );
    assert_eq!(
        detect_with_targets(&came_back("curl -v x", "", &both), &place()),
        vec![(
            block(Operation::Connect, Kind::Host, false),
            Some("api.example.com:443".to_owned())
        )]
    );
}

/// A program that looks a host up itself, rather than going through the sandbox's proxy, is
/// refused the lookup by the sandbox, and macOS's resolver says only that the name is not known
/// (#1631): no violation line comes with it. Allowing the host would not let such a program
/// through, so the host it names is said and never offered (#1663,
/// `sandboxblock_network_tests`).
#[test]
fn a_lookup_the_sandbox_refused_is_a_lookup_of_a_host_with_nothing_to_grant() {
    for (line, host) in [
        (
            "psql: error: could not translate host name \"db.example.com\" to address: nodename \
             nor servname provided, or not known",
            Some("db.example.com"),
        ),
        (
            "socket.gaierror: [Errno 8] nodename nor servname provided, or not known",
            None,
        ),
        (
            "nc: getaddrinfo: nodename nor servname provided, or not known",
            None,
        ),
        (
            "curl: (6) Could not resolve host: api.example.com",
            Some("api.example.com"),
        ),
    ] {
        assert_eq!(
            detect_with_targets(&came_back("x", "", line), &place()),
            vec![(
                block(Operation::Lookup, Kind::Host, false),
                host.map(str::to_owned)
            )],
            "{line}"
        );
    }
}

/// A refused lookup is never said to be of a host the project does not allow (#1631 review):
/// the host may well be allowed, and the chat told so would ask the person to allow it, which
/// would not let the program through.
#[test]
fn a_refused_lookup_is_said_as_a_program_past_the_proxy_never_as_a_host_not_allowed() {
    let said = block(Operation::Lookup, Kind::Host, false).said();
    assert!(!said.contains("does not allow"), "{said}");
    assert!(said.contains("proxy"), "{said}");
    let told = told_the_chat(&[block(Operation::Lookup, Kind::Host, false)]).unwrap();
    assert!(!told.contains("does not allow"), "{told}");
    // A refused connection still names the project's allowlist.
    assert!(
        block(Operation::Connect, Kind::Host, false)
            .said()
            .contains("this project does not allow")
    );
}

/// Docker's client refused its socket (#1631): a local socket, which no grant names. Docker's
/// sentence alone counts only on macOS (#1637, below).
#[test]
fn a_refused_docker_socket_is_a_connection_to_a_local_socket() {
    let alone = cfg!(target_os = "macos").then_some(
        "permission denied while trying to connect to the docker API at \
         unix:///Users/dev/.docker/run/docker.sock",
    );
    for line in alone.into_iter().chain([
        "Got permission denied while trying to connect to the Docker daemon socket at \
         unix:///var/run/docker.sock: Get \"http://%2Fvar%2Frun%2Fdocker.sock/v1.47/info\": dial \
         unix /var/run/docker.sock: connect: operation not permitted",
        "dial unix /Users/dev/.colima/default/docker.sock: connect: operation not permitted",
    ]) {
        assert_eq!(
            detect_with_targets(&came_back("docker info", "", line), &place()),
            vec![(block(Operation::Connect, Kind::LocalSocket, false), None)],
            "{line}"
        );
    }
    // A daemon that is not running is not the sandbox.
    assert!(
        detect(
            &came_back(
                "docker info",
                "",
                "Cannot connect to the Docker daemon at unix:///var/run/docker.sock. Is the \
                 docker daemon running?"
            ),
            &place()
        )
        .is_empty()
    );
}

/// Docker's own "permission denied" sentence is the sandbox's only where the sandbox can refuse
/// a socket that way (#1637): on macOS, Seatbelt's refusal reads so. Elsewhere it is a user
/// outside the docker group, which no Start without the sandbox mends. The socket's `EPERM`
/// (Go's `connect: operation not permitted`) is the sandbox's everywhere, and `EACCES`
/// (`connect: permission denied`) is a socket's own file mode everywhere.
#[test]
fn dockers_permission_words_are_a_block_only_where_the_sandbox_can_say_them() {
    let docker_alone = "permission denied while trying to connect to the docker API at \
                        unix:///Users/dev/.docker/run/docker.sock";
    let docker_eperm = "Got permission denied while trying to connect to the Docker daemon socket \
                        at unix:///var/run/docker.sock: Get \"http://%2Fvar%2Frun%2Fdocker.sock/\
                        v1.47/info\": dial unix /var/run/docker.sock: connect: operation not \
                        permitted";
    let docker_eacces = "permission denied while trying to connect to the Docker daemon socket \
                         at unix:///var/run/docker.sock: Get \"http://%2Fvar%2Frun%2Fdocker.sock/\
                         v1.47/info\": dial unix /var/run/docker.sock: connect: permission denied";
    let go_eperm = "dial unix /Users/dev/.colima/default/docker.sock: connect: operation not \
                    permitted";
    let socket = Some((Operation::Connect, Kind::LocalSocket, None));
    // Where the sandbox says Docker's words (macOS).
    assert_eq!(network_refusal_on(docker_alone, true), socket);
    assert_eq!(network_refusal_on(docker_eperm, true), socket);
    assert_eq!(network_refusal_on(go_eperm, true), socket);
    assert_eq!(network_refusal_on(docker_eacces, true), None, "a file mode");
    // Where it does not (Linux): only the socket's own `EPERM` is the sandbox's.
    assert_eq!(
        network_refusal_on(docker_alone, false),
        None,
        "the docker group"
    );
    assert_eq!(network_refusal_on(docker_eperm, false), socket);
    assert_eq!(network_refusal_on(go_eperm, false), socket);
    assert_eq!(
        network_refusal_on(docker_eacces, false),
        None,
        "a file mode"
    );
    // This build reads them as its own platform does.
    assert_eq!(
        network_refusal(docker_alone),
        cfg!(target_os = "macos").then_some((Operation::Connect, Kind::LocalSocket, None))
    );
}

#[test]
fn the_chat_is_told_what_was_blocked_and_to_wait_only_when_a_notice_is_up() {
    assert_eq!(
        told_the_chat(&[]),
        None,
        "no block taken, no Notice to point at"
    );
    let host = block(Operation::Connect, Kind::Host, false);
    let told = told_the_chat(&[host, host]).expect("a block was taken");
    assert!(
        told.contains("blocked a connection to an internet host this project does not allow."),
        "{told}"
    );
    assert_eq!(
        told.matches("a connection to").count(),
        1,
        "said once: {told}"
    );
    for words in [
        "Notice",
        "this chat's tab",
        "wait for their answer",
        "do not work around",
    ] {
        assert!(told.contains(words), "{words}: {told}");
    }
    assert!(!told.contains("Report"), "{told}");
    let ours = told_the_chat(&[block(Operation::Write, Kind::ProjectFiles, true)]).unwrap();
    assert!(
        ours.contains("purlis bug") && ours.contains("Report"),
        "{ours}"
    );
}
