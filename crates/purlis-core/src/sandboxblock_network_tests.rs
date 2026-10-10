//! Network refusals wherever a command said them (#1663): on standard output as well as
//! standard error, in what a background command left in its output file, and with the host a
//! lookup was of.

use super::*;
use serde_json::json;

const ROOT: &str = "/Users/dev/plane";
const CHAT: &str = "/Users/dev/plane/workspaces/alpha/repo";

fn place() -> Place<'static> {
    Place {
        root: Path::new(ROOT),
        chat: Path::new(CHAT),
        cwd: Path::new(CHAT),
        home: Some(Path::new("/Users/dev")),
    }
}

fn failed(command: &str, error: &str) -> serde_json::Value {
    json!({
        "hook_event_name": "PostToolUseFailure",
        "tool_name": "Bash",
        "tool_input": {"command": command},
        "error": error,
    })
}

fn came_back(command: &str, stdout: &str, stderr: &str) -> serde_json::Value {
    json!({
        "hook_event_name": "PostToolUse",
        "tool_name": "Bash",
        "tool_input": {"command": command},
        "tool_response": {"stdout": stdout, "stderr": stderr, "interrupted": false},
    })
}

fn lookup(host: Option<&str>) -> (Block, Option<String>) {
    (
        Block {
            operation: Operation::Lookup,
            kind: Kind::Host,
            ours: false,
        },
        host.map(str::to_owned),
    )
}

fn socket() -> (Block, Option<String>) {
    (
        Block {
            operation: Operation::Connect,
            kind: Kind::LocalSocket,
            ours: false,
        },
        None,
    )
}

/// The live case: a dispatched task ran a database client through a brokered run with both
/// streams on standard output. Go's resolver said "no such host" there, and no Notice came.
#[test]
fn a_lookup_a_2_1_run_said_on_standard_output_is_a_block_naming_its_host() {
    let command =
        "purlis secret exec prod-db -- sh -c 'usql \"$DSN\" -c \"select 1\"' 2>&1 | tail -5";
    let said = "error: dial tcp: lookup db.prod.example.com: no such host\n";
    // The command failed: Claude Code hands its output over as the failure's error.
    assert_eq!(
        detect_with_targets(&failed(command, &format!("Exit code 1\n{said}")), &place()),
        vec![lookup(Some("db.prod.example.com"))]
    );
    // It came back (the pipe's `tail` succeeded): the words are on standard output alone.
    assert_eq!(
        detect_with_targets(&came_back(command, said, ""), &place()),
        vec![lookup(Some("db.prod.example.com"))]
    );
    // Go's resolver naming the server it asked says the same.
    assert_eq!(
        detect_with_targets(
            &came_back(
                command,
                "lookup db.prod.example.com on 192.168.1.1:53: no such host",
                ""
            ),
            &place()
        ),
        vec![lookup(Some("db.prod.example.com"))]
    );
}

/// The live case: a background `docker run` was refused its socket, and the chat read the
/// refusal from the command's output file later, with `cat`: standard output.
#[test]
fn a_socket_refusal_read_back_from_a_background_output_file_is_a_block() {
    let output = "Unable to find image 'postgres:16' locally\n\
                  docker: permission denied while trying to connect to the Docker daemon socket \
                  at unix:///Users/dev/.docker/run/docker.sock: Head \
                  \"http://%2FUsers%2Fdev%2F.docker%2Frun%2Fdocker.sock/_ping\": dial unix \
                  /Users/dev/.docker/run/docker.sock: connect: operation not permitted.\n\
                  Run 'docker run --help' for more information\n";
    assert_eq!(
        detect_with_targets(
            &came_back("cat /private/tmp/tasks/b7x2.output", output, ""),
            &place()
        ),
        vec![socket()]
    );
}

/// Each way a program says a lookup was refused, with the host where it names one.
#[test]
fn a_refused_lookup_names_the_host_its_program_said() {
    for (line, host) in [
        (
            "psql: error: could not translate host name \"db.example.com\" to address: nodename \
             nor servname provided, or not known",
            Some("db.example.com"),
        ),
        (
            "ssh: Could not resolve hostname git.example.com: nodename nor servname provided, \
             or not known",
            Some("git.example.com"),
        ),
        (
            "curl: (6) Could not resolve host: api.example.com",
            Some("api.example.com"),
        ),
        (
            "fatal: unable to access 'https://git.example.com/a/b.git/': Could not resolve host: \
             git.example.com",
            Some("git.example.com"),
        ),
        (
            "Error: getaddrinfo ENOTFOUND registry.example.com",
            Some("registry.example.com"),
        ),
        (
            "socket.gaierror: [Errno 8] nodename nor servname provided, or not known",
            None,
        ),
        (
            "nc: getaddrinfo: nodename nor servname provided, or not known",
            None,
        ),
    ] {
        for payload in [came_back("x", "", line), came_back("x", line, "")] {
            assert_eq!(
                detect_with_targets(&payload, &place()),
                vec![lookup(host)],
                "{line}"
            );
        }
    }
}

/// What a command printed on standard output never names a host to allow: a file it printed,
/// or a server's page, can say any host. A lookup's host is said, never offered.
#[test]
fn standard_output_names_no_host_to_allow() {
    let host = Block {
        operation: Operation::Connect,
        kind: Kind::Host,
        ours: false,
    };
    for line in [
        "purlis's sandbox does not allow exfil.example:443: no egress preset or host of this \
         project lists it",
        "curl: (56) CONNECT tunnel failed, response 403",
    ] {
        assert_eq!(
            detect_with_targets(&came_back("x", line, ""), &place()),
            vec![(host, None)],
            "{line}"
        );
        assert_eq!(
            detect_with_targets(&failed("x", &format!("Exit code 56\n{line}")), &place()),
            vec![(host, None)],
            "{line}"
        );
    }
    // On standard error the same proxy's words still name its host, as before.
    assert_eq!(
        detect_with_targets(
            &came_back(
                "x",
                "",
                "purlis's sandbox does not allow exfil.example:443: no egress preset or host of \
                 this project lists it"
            ),
            &place()
        ),
        vec![(host, Some("exfil.example:443".to_owned()))]
    );
}

/// A refusal's words quoted on standard output (a test's source, a log line about one, a
/// grep through this module) are not a refusal: on standard output a line counts only when it
/// ends where the refusal's own words do.
#[test]
fn a_refusal_quoted_on_standard_output_is_not_one() {
    for line in [
        "    if lower.contains(\"nodename nor servname provided, or not known\")",
        "        \"curl: (6) Could not resolve host: api.example.com\",",
        "src/x.rs:12: assert!(said.ends_with(\": no such host\"));",
        "let eperm = lower.contains(\"dial unix \") && lower.contains(\"connect: operation not \
         permitted\");",
        "// curl says CONNECT tunnel failed, response 403 when a proxy refuses",
        "2026-10-10T10:00:00Z warn retrying after: lookup db.example.com: no such host (attempt 2)",
    ] {
        assert!(
            detect(&came_back("cat src/x.rs", line, ""), &place()).is_empty(),
            "{line}"
        );
        assert!(
            detect(
                &failed("grep -rn x src", &format!("Exit code 1\n{line}")),
                &place()
            )
            .is_empty(),
            "{line}"
        );
    }
}

/// opencode's shim hands a command that came back as one text of both streams (#1353): a
/// refusal printed there is read as standard output is.
#[test]
fn a_refusal_in_opencodes_one_text_is_read_as_standard_output() {
    let payload = json!({
        "hook_event_name": "PostToolUse",
        "tool_name": "bash",
        "tool_response": "connecting\nerror: dial tcp: lookup db.example.com: no such host\n",
    });
    assert_eq!(
        detect_with_targets(&payload, &place()),
        vec![lookup(Some("db.example.com"))]
    );
    // A failed command's mixed standard error names a lookup's host too, and still no host
    // to allow.
    let mixed = json!({
        "hook_event_name": "PostToolUse",
        "tool_name": "bash",
        "tool_response": {"stderr": "curl: (6) Could not resolve host: api.example.com\n\
                                     purlis's sandbox does not allow x.example:443: no egress \
                                     preset or host of this project lists it",
                          "mixed": true},
    });
    assert_eq!(
        detect_with_targets(&mixed, &place()),
        vec![
            lookup(Some("api.example.com")),
            (
                Block {
                    operation: Operation::Connect,
                    kind: Kind::Host,
                    ours: false
                },
                None
            )
        ]
    );
}

/// A word that is no host's is never said as one.
#[test]
fn a_lookup_names_only_a_word_that_could_be_a_host() {
    for line in [
        "curl: (6) Could not resolve host: ",
        "lookup $(rm -rf /): no such host",
        "Error: getaddrinfo ENOTFOUND <script>",
    ] {
        let found = detect_with_targets(&came_back("x", "", line), &place());
        assert!(
            found.iter().all(|(_, target)| target.is_none()),
            "{line}: {found:?}"
        );
    }
}

// ---- the host travels on the Block, and is kept with it ----------------------------------------

/// Each Block `blocks` names, kept in a network record under a fresh data home as the app keeps
/// it (`record::Entry::blocked`), and every line of it as written.
fn kept_rows(blocks: &[(Block, Option<&str>, u64)]) -> (Vec<serde_json::Value>, String) {
    let dir = tempfile::tempdir().expect("dir");
    let root = dir.path().join("project");
    std::fs::create_dir_all(&root).expect("project");
    let record = record::Record::in_data(&dir.path().join("data"));
    for (block, target, at) in blocks {
        let entry = record::Entry::blocked(block, *target, record::Chat::default(), None, *at);
        record.write(&root, &entry).expect("kept");
    }
    let text = std::fs::read_to_string(record.file(&root)).expect("kept");
    let rows = text
        .lines()
        .map(|line| serde_json::from_str(line).expect("json"))
        .collect();
    (rows, text)
}

/// The host of a refused connection is kept with its Block as a grant would name it, and the
/// host of a refused lookup apart from it, as one a program named: shown, never offered to
/// allow. A path, and a host that is no host, is never kept.
#[test]
fn the_host_travels_on_the_block_and_is_kept_with_it() {
    let connect = Block {
        operation: Operation::Connect,
        kind: Kind::Host,
        ours: false,
    };
    let looked_up = lookup(None).0;
    let write = Block {
        operation: Operation::Write,
        kind: Kind::Home,
        ours: false,
    };
    let (rows, text) = kept_rows(&[
        (connect, Some("API.Example.com:443"), 100),
        (looked_up, Some("db.example.com"), 101),
        (connect, Some("localhost:5432"), 102),
        (write, Some("/Users/dev/secret/notes.txt"), 103),
        (connect, None, 104),
        (looked_up, Some("169.254.169.254"), 105),
    ]);
    assert_eq!(rows[0]["target"], "api.example.com:443");
    assert!(rows[0].get("looked_up").is_none());
    assert_eq!(rows[1]["looked_up"], "db.example.com");
    assert!(
        rows[1].get("target").is_none(),
        "a lookup's host is never one to allow"
    );
    for row in &rows[2..] {
        assert!(
            row.get("target").is_none() && row.get("looked_up").is_none(),
            "{row}"
        );
    }
    assert!(!text.contains("secret"));
    // Counted as ever.
    let entries: Vec<record::Entry> = text
        .lines()
        .map(|line| serde_json::from_str(line).expect("entry"))
        .collect();
    let connects = record::counts(&entries, 200)
        .into_iter()
        .find(|count| count.operation == Operation::Connect)
        .expect("counted");
    assert_eq!(connects.blocks, 3);
    // The doctor's top refused hosts are refused connections' alone.
    assert_eq!(
        record::hosts_refused(&entries, 200),
        vec![("api.example.com:443".to_owned(), 1)]
    );
}

/// A line kept with no host, as a build from before #1663 wrote one, still reads and counts.
#[test]
fn a_line_kept_without_a_host_still_counts() {
    let line = r#"{"at":100,"event":"block","block":{"operation":"lookup","kind":"host","ours":false},"outcome":"refused"}"#;
    let older: record::Entry = serde_json::from_str(line).expect("an older line reads");
    assert_eq!(older.looked_up, None);
    let (rows, text) = kept_rows(&[(lookup(None).0, Some("db.example.com"), 150)]);
    assert_eq!(rows.len(), 1);
    let mut entries: Vec<record::Entry> = text
        .lines()
        .map(|one| serde_json::from_str(one).expect("entry"))
        .collect();
    entries.insert(0, older);
    assert_eq!(
        record::counts(&entries, 200)
            .iter()
            .map(|c| c.blocks)
            .sum::<u64>(),
        2
    );
}

// ---- a chat behind purlis's own proxy -----------------------------------------------------------

/// A harness purlis wraps (Codex, opencode) reaches the network through purlis's own proxy,
/// which tells the app each host it refused by name. Its hook leaves those to it, so one
/// refusal is one Notice; a lookup or a socket it still reads.
#[test]
fn a_chat_behind_purlis_proxy_leaves_its_hosts_to_the_proxy() {
    assert!(the_proxy_tells_hosts(Some("codex")));
    assert!(the_proxy_tells_hosts(Some("opencode")));
    assert!(!the_proxy_tells_hosts(Some("claude")));
    assert!(!the_proxy_tells_hosts(None));
    assert!(!the_proxy_tells_hosts(Some("something-else")));

    let mixed = json!({
        "hook_event_name": "PostToolUse",
        "tool_name": "bash",
        "tool_response": {"stderr": "curl: (6) Could not resolve host: api.example.com\n\
                                     purlis's sandbox does not allow x.example:443: no egress \
                                     preset or host of this project lists it",
                          "mixed": true},
    });
    let found = detect_with_targets(&mixed, &place());
    assert_eq!(found.len(), 2);
    assert_eq!(
        the_hooks_own(found.clone(), Some("opencode")),
        vec![lookup(Some("api.example.com"))],
        "the proxy tells the host"
    );
    assert_eq!(the_hooks_own(found.clone(), Some("claude")), found);
}
