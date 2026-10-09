//! **Every store `docs/plane-format.md` names has a storage tier** (ADR 0069).
//!
//! There are four tiers: **Plane** (committed), **Clone state** (`.charter/` and the other
//! per-clone files git does not carry), **Machine** (outside every plane, each store either
//! `syncable` or `device-bound`) and **Keyring**. A store's tier decides whether FR-10's backup
//! carries it, whether a second machine may receive it, and whether deleting it costs anything
//! but a rebuild. A store that lands without one has had none of those questions asked, so this
//! test fails until it has.
//!
//! What it reads, between the heading "Finding the plane" and the appendix:
//!
//! - **Every `###` and `####` heading** carries a `**Tier:**` line in its section, unless it is
//!   listed in [`NOT_STORES`] with the reason it is not a store (a key, a group whose entries
//!   carry their own tiers, a rule, a list of conventions). So a new heading, with or without a
//!   path in backticks, fails until somebody gives it a tier or argues in a diff that it is not
//!   a store.
//! - **Every table whose first column is `Path`** has a `Tier` column, because a table of paths
//!   is a list of stores (`workspace-rename.json` is recorded only in one).
//! - **Every tier said anywhere in that range is one the ADR defines**, spelled as [`check`]
//!   reads it.
//!
//! What it does not read: a store named only inside a paragraph or a bullet, under a heading
//! that is about something else. The document's convention is that every store has its own
//! heading or table row, and review holds that convention; this test holds the tiers.
//!
//! **Why a line walker and not a Markdown parser.** The document's structure is line-shaped:
//! ATX headings at the start of a line, `|` tables, ``` fences. A parser such as
//! `pulldown-cmark` would be a new dependency for one doc test, and it would not make the
//! checks more exact, because a tier is a convention inside a list item's text either way. The
//! walker skips fenced blocks, and the tests at the bottom pin what it reads.

use std::path::Path;

/// Where the checked part of the document starts, and where it ends.
const FIRST: &str = "## Finding the plane";
const LAST: &str = "## Appendix";

/// Headings in the checked range that do not name a store, each with the reason.
const NOT_STORES: &[(&str, &str)] = &[
    (
        "Plane-root discovery",
        "a rule every file hangs off, not a file",
    ),
    (
        "`[frame]`",
        "keys inside `charter.toml`, whose tier is its own",
    ),
    ("`[[frame.component]]`", "a table inside `charter.toml`"),
    (
        "Plane-root files that exist but are **not** this area",
        "pointers to entries recorded elsewhere",
    ),
    (
        "`settings` — a workspace's layer",
        "a key inside `workspace.json`",
    ),
    (
        "Editing and archiving a memory",
        "what the app does to memory files that carry their own tiers",
    ),
    (
        "Moving a memory between scopes",
        "a memory file moved between stores that carry their own tiers",
    ),
    (
        "`.charter/…` — active-workspace pointers",
        "its table's rows carry their tiers",
    ),
    (
        "`[memory] share`",
        "a setting, and how it moves persona files",
    ),
    ("Secret reference syntax", "a syntax, not a file"),
    ("1Password provider", "a provider's shape, not a file"),
    ("Guarded paths", "a rule about the files above"),
    (
        "2a. Inside the plane",
        "a group heading: each file under it carries its own tier",
    ),
    (
        "Generated harness layer in `workspaces/<ws>/`",
        "a group heading: each file under it carries its own tier",
    ),
    (
        "2b. Outside the plane",
        "a group heading: each file under it carries its own tier",
    ),
    (
        "2d. Charter-private caches",
        "a group heading: each file under it carries its own tier",
    ),
    (
        "Conventions that apply to every file",
        "modes and write primitives, not a file",
    ),
    (
        "Top-level markers, gates and ledgers",
        "a group heading: each file under it carries its own tier",
    ),
    (
        "State charter keeps **outside** the plane",
        "its table's rows carry their tiers",
    ),
    ("Environment variables that move", "variables, not files"),
    (
        "What the tmux frame and the status line read",
        "a cross-reference to entries above",
    ),
];

/// The first part of a tier line: the tier, or `None` for a path charter records but does not
/// own (a harness's own file, the operator's checkout).
const TIERS: &[&str] = &[
    "Plane",
    "Plane when LIVE",
    "Clone state",
    "Machine",
    "Keyring",
    "None",
];

/// What may follow the tier, comma-separated.
const MARKS: &[&str] = &[
    "syncable",
    "device-bound",
    "rebuildable",
    "transient",
    "legacy",
    "Clone state when LOCAL",
];

/// The mark a workspace file's tier needs, and the only tier it goes with.
const LOCAL: &str = "Clone state when LOCAL";

/// The problem with one tier, or `None` when it reads. `said` is everything after `**Tier:**`:
/// the tier and its marks, then optionally ` — ` and the reason. The tier and marks carry no
/// punctuation of their own, so `Clone state, transient.` is refused rather than tidied.
fn check(said: &str) -> Option<String> {
    let value = said.split(" — ").next().unwrap_or_default().trim();
    let mut parts = value.split(", ");
    let tier = parts.next().unwrap_or_default();
    if !TIERS.contains(&tier) {
        return Some(format!("`{tier}` is not a tier (one of {TIERS:?})"));
    }
    let marks: Vec<&str> = parts.collect();
    if let Some(odd) = marks.iter().find(|mark| !MARKS.contains(mark)) {
        return Some(format!("`{odd}` is not a mark (one of {MARKS:?})"));
    }
    let placed = marks
        .iter()
        .filter(|mark| ["syncable", "device-bound"].contains(mark))
        .count();
    let local = marks.contains(&LOCAL);
    match tier {
        "Machine" if placed != 1 => {
            Some("a Machine store is exactly one of `syncable` or `device-bound`".into())
        }
        "Machine" => None,
        "None" | "Keyring" if !marks.is_empty() => Some(format!("`{tier}` takes no marks")),
        _ if placed > 0 => Some("only a Machine store is `syncable` or `device-bound`".into()),
        "Plane when LIVE" if !local => {
            Some(format!("`Plane when LIVE` is always followed by `{LOCAL}`"))
        }
        "Plane when LIVE" => None,
        _ if local => Some(format!("`{LOCAL}` follows only `Plane when LIVE`")),
        _ => None,
    }
}

/// One `**Tier:**` value on a line, if the line has one.
fn tier_on(line: &str) -> Option<&str> {
    line.split_once("**Tier:**").map(|(_, rest)| rest.trim())
}

fn cells(row: &str) -> Vec<&str> {
    row.trim()
        .trim_matches('|')
        .split('|')
        .map(str::trim)
        .collect()
}

/// Where the walker is with respect to a table.
enum Table {
    /// Not in one: the next `|` line is a header.
    Outside,
    /// In a table with no Tier column: its rows are not checked.
    WithoutTiers,
    /// In a table whose tier is in this column.
    TierIn(usize),
}

/// What reading `text` found: every problem, one sentence each, and how many tier lines and
/// cells were read in the checked range.
struct Reading {
    problems: Vec<String>,
    tiers: usize,
}

fn read(text: &str) -> Reading {
    let mut problems = Vec::new();
    let mut tiers = 0;
    let (mut began, mut ended) = (false, false);
    let mut in_scope = false;
    let mut fenced = false;
    // The heading being read, if it names a store and no tier has been seen under it yet.
    let mut owed: Option<(usize, String)> = None;
    let mut table = Table::Outside;
    let settle = |owed: &mut Option<(usize, String)>, problems: &mut Vec<String>| {
        if let Some((at, heading)) = owed.take() {
            problems.push(format!("line {at}: {heading} has no **Tier:** line"));
        }
    };

    for (at, line) in text.lines().enumerate() {
        let number = at + 1;
        if line.starts_with("```") {
            fenced = !fenced;
            continue;
        }
        if fenced {
            continue;
        }
        if line.starts_with("## ") {
            settle(&mut owed, &mut problems);
            if line.starts_with(FIRST) {
                began = true;
                in_scope = true;
            } else if line.starts_with(LAST) && in_scope {
                ended = true;
                in_scope = false;
            }
            continue;
        }
        if !in_scope {
            continue;
        }
        if line.starts_with("### ") || line.starts_with("#### ") {
            settle(&mut owed, &mut problems);
            let heading = line.trim_start_matches('#').trim();
            let exempt = NOT_STORES
                .iter()
                .any(|(start, _)| heading.starts_with(start));
            if !exempt {
                owed = Some((number, heading.to_string()));
            }
            continue;
        }
        if line.trim_start().starts_with('|') {
            let row = cells(line);
            match table {
                Table::Outside => {
                    let column = row.iter().position(|cell| *cell == "Tier");
                    if row.first() == Some(&"Path") && column.is_none() {
                        problems.push(format!(
                            "line {number}: a table of paths has no Tier column"
                        ));
                    }
                    table = column.map_or(Table::WithoutTiers, Table::TierIn);
                }
                Table::TierIn(column) if !row.iter().all(|cell| cell.starts_with("---")) => {
                    tiers += 1;
                    let said = row.get(column).copied().unwrap_or_default();
                    if let Some(problem) = check(said) {
                        problems.push(format!("line {number}: {problem}"));
                    }
                }
                Table::TierIn(_) | Table::WithoutTiers => {}
            }
            continue;
        }
        table = Table::Outside;
        if let Some(said) = tier_on(line) {
            tiers += 1;
            owed = None;
            if let Some(problem) = check(said) {
                problems.push(format!("line {number}: {problem}"));
            }
        }
    }
    settle(&mut owed, &mut problems);
    if !began {
        problems.push(format!("no `{FIRST}` heading: nothing was checked"));
    }
    if !ended {
        problems.push(format!(
            "no `{LAST}` heading after `{FIRST}`: the range never closed"
        ));
    }
    Reading { problems, tiers }
}

#[test]
fn every_store_the_plane_format_names_has_a_tier() {
    purlis_core::unsteered!();
    let doc = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/plane-format.md");
    let text = std::fs::read_to_string(&doc).expect("docs/plane-format.md is readable");
    let reading = read(&text);
    assert!(
        reading.problems.is_empty(),
        "docs/plane-format.md names a store without a tier it can read (ADR 0069):\n{}",
        reading.problems.join("\n")
    );
    assert!(
        reading.tiers > 100,
        "only {} tiers were read in the checked range: the walk read the wrong part",
        reading.tiers
    );
}

#[test]
fn a_new_file_without_a_tier_is_named() {
    purlis_core::unsteered!();
    let text = "## Finding the plane\n\n### `a.json`\n\n- **Tier:** Plane\n\n\
                ### `b.json`\n\n- **Status:** stable\n\n\
                ### A store with no path in its heading\n\nProse.\n\n## Appendix\n";
    let found = read(text).problems;
    assert_eq!(found.len(), 2, "{found:?}");
    assert!(found[0].contains("`b.json`"), "{found:?}");
    assert!(found[1].contains("no path in its heading"), "{found:?}");
}

#[test]
fn a_renamed_range_heading_is_a_failure_and_not_a_pass() {
    purlis_core::unsteered!();
    let text = "## Finding a plane\n\n### `a.json`\n\n- **Status:** stable\n\n## Appendix\n";
    let reading = read(text);
    assert_eq!(reading.tiers, 0);
    assert!(
        reading
            .problems
            .iter()
            .any(|p| p.contains("nothing was checked")),
        "{:?}",
        reading.problems
    );
    let unclosed = read("## Finding the plane\n\n### `a.json`\n\n- **Tier:** Plane\n");
    assert!(
        unclosed.problems.iter().any(|p| p.contains("never closed")),
        "{:?}",
        unclosed.problems
    );
}

#[test]
fn a_tier_outside_the_range_is_not_counted() {
    purlis_core::unsteered!();
    let text = "- **Tier:** Plane\n\n## Finding the plane\n\n## Appendix\n\n- **Tier:** Plane\n";
    assert_eq!(read(text).tiers, 0);
}

#[test]
fn a_table_of_paths_carries_a_tier_per_row() {
    purlis_core::unsteered!();
    let without =
        "## Finding the plane\n\n| Path | What |\n|---|---|\n| `x` | y |\n\n## Appendix\n";
    assert_eq!(read(without).problems.len(), 1);
    let with =
        "## Finding the plane\n\n| Path | Tier |\n|---|---|\n| `x` | Machine |\n\n## Appendix\n";
    let found = read(with).problems;
    assert!(
        found.len() == 1 && found[0].contains("syncable"),
        "{found:?}"
    );
}

#[test]
fn a_tier_is_one_the_adr_defines() {
    purlis_core::unsteered!();
    assert_eq!(check("Plane — committed."), None);
    assert_eq!(
        check("Plane when LIVE, Clone state when LOCAL — the LIVE block."),
        None
    );
    assert_eq!(check("Clone state, rebuildable"), None);
    assert_eq!(check("Machine, device-bound, rebuildable"), None);
    assert_eq!(check("Keyring"), None);
    assert!(check("App data").is_some());
    assert!(check("Machine").is_some());
    assert!(check("Machine, syncable, device-bound").is_some());
    assert!(check("Clone state, syncable").is_some());
    assert!(check("Keyring, rebuildable").is_some());
    assert!(check("Plane, Clone state when LOCAL").is_some());
    assert!(check("Plane when LIVE").is_some());
    assert!(check("Clone state, cached").is_some());
    assert!(check("Clone state, transient.").is_some());
}

/// The section of `text` under the `###` heading `heading`, up to the next `###` or `---`.
fn section<'a>(text: &'a str, heading: &str) -> &'a str {
    let start = text
        .find(&format!("\n### {heading}\n"))
        .unwrap_or_else(|| panic!("no `### {heading}` heading"));
    let body = &text[start + 1..];
    let end = body[4..]
        .find("\n### ")
        .map(|at| at + 4)
        .unwrap_or(body.len());
    &body[..end]
}

/// V43 put the clone-key and the device id in `app/reopen.json`. It is still the reopen record
/// V2 names, which ADR 0069 makes Clone state: the key says which clone it belongs to, and a
/// copy that carries it is told apart rather than trusted.
#[test]
fn the_reopen_record_with_its_clone_key_is_still_clone_state() {
    purlis_core::unsteered!();
    let doc = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/plane-format.md");
    let text = std::fs::read_to_string(&doc).expect("docs/plane-format.md is readable");
    let reopen = section(&text, "`app/reopen.json`");

    let tier = reopen
        .lines()
        .find_map(tier_on)
        .expect("app/reopen.json has a **Tier:** line");
    assert_eq!(check(tier), None, "{tier}");
    assert!(tier.starts_with("Clone state"), "{tier}");
    assert!(tier.contains("ADR 0069"), "{tier}");
    for row in [
        "| `clone` |",
        "| `clone.key` |",
        "| `clone.root` |",
        "| `clone.device` |",
    ] {
        assert!(reopen.contains(row), "app/reopen.json has no {row} row");
    }
}

/// **The file a chat touched is in no store** (FM-6, D-86a). The hook names it to the app on a
/// line of its own, which the app hands to the window in memory. Every store that line's
/// neighbours reach is searched for it: the hook spool and the socket's folder
/// (`.charter/app/`, Clone state) when no app takes the line, the event log (Machine) when one
/// does, and the diagnostic log (`$CHARTER_LOG_DIR`), which a touch refused for its token is
/// said in. `app/reopen.json` is the app's to write and is held by the app's own test
/// (`hooks::tests::a_touched_path_reaches_the_window_and_no_file_the_host_writes`); the plane
/// format says the line is never stored, and this reads that it does.
///
/// The channel runs in a child of this test, so the diagnostic log can be installed there with
/// its directory set, without touching this process's environment or its global subscriber.
#[cfg(unix)]
#[test]
fn the_path_a_chat_touched_is_in_no_store_the_plane_format_names() {
    purlis_core::unsteered!();
    const CHILD: &str = "CHARTER_TEST_TOUCHED_CHILD";
    if let Some(run) = std::env::var_os(CHILD) {
        purlis_core::applog::install();
        a_chat_touches_a_file(Path::new(&run));
        return;
    }

    let doc = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/plane-format.md");
    let text = std::fs::read_to_string(&doc).expect("docs/plane-format.md is readable");
    let socket_entry = section(&text, "`app/hooks.sock`");
    assert!(
        socket_entry.contains("`touching`") && socket_entry.contains("never stored"),
        "the plane format says the touched path is never stored"
    );

    let run = tempfile::tempdir().expect("a run directory");
    let logs = run.path().join("logs");
    let out = purlis_core::forklock::output(
        std::process::Command::new(std::env::current_exe().expect("this test"))
            .args([
                "--exact",
                "the_path_a_chat_touched_is_in_no_store_the_plane_format_names",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(CHILD, run.path())
            .env("CHARTER_LOG_DIR", &logs),
    )
    .expect("the child runs");
    let (stdout, stderr) = (
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    assert!(
        out.status.success(),
        "the child failed:\n{stdout}\n{stderr}"
    );
    assert!(
        !stdout.contains(TOUCHED) && !stderr.contains(TOUCHED),
        "{stdout}\n{stderr}"
    );

    let mut read = Vec::new();
    let mut stack = vec![run.path().to_path_buf()];
    while let Some(at) = stack.pop() {
        for entry in std::fs::read_dir(&at).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let bytes = std::fs::read(&path).unwrap_or_default();
            let text = String::from_utf8_lossy(&bytes).into_owned();
            assert!(
                !text.contains(TOUCHED),
                "{} holds the touched path",
                path.display()
            );
            read.push((path, text));
        }
    }
    let read_in = |dir: &Path| {
        read.iter()
            .filter(|(path, _)| path.starts_with(dir))
            .count()
    };
    assert!(
        read_in(&run.path().join("project")) > 0,
        "the spool was read"
    );
    assert!(
        read_in(&run.path().join("machine")) > 0,
        "the event log was read"
    );
    assert!(
        read.iter()
            .any(|(path, text)| path.starts_with(&logs) && text.contains("did not carry")),
        "the diagnostic log was written, and holds the refused touch: {read:?}"
    );
}

/// The canary path a chat's tool touches.
const TOUCHED: &str = "CANARY-touched-0b7c";

/// Chat 2's tool touches a file, under `run`: once with no app listening, then once with an app
/// that records its tool calls, and once more with no token.
#[cfg(unix)]
fn a_chat_touches_a_file(run: &Path) {
    use purlis_core::eventlog::{ArgsKey, Log, Recorder, args_hash};
    use purlis_core::hookwire::{
        Decision, Hearing, Listener, ToolCall, Touching, deliver_tool, touch,
    };
    use std::sync::{Arc, Mutex, mpsc};

    let root = run.join("project");
    let socket = root.join(".charter/app/hooks.sock");
    let events = run.join("machine/events/DEVICE");
    let file = format!("{}/src/{TOUCHED}.rs", root.display());
    let input = serde_json::json!({"file_path": file});
    let call = ToolCall {
        chat: 2,
        tool_hook: "pretooluse-read".to_owned(),
        tool: Some("Read".to_owned()),
        call: Some("toolu_1".to_owned()),
        args: Some(args_hash(&input)),
        decision: Decision::None,
        rule: None,
        hook_ms: 1,
        agent: None,
        at_ms: 0,
    };
    let touching = Touching {
        chat: 2,
        touching: file.clone(),
        wrote: false,
    };

    // What the same hook says the chat is doing (#1493): the file's base name, on a line of
    // its own that is held to the touch's rule, in memory only.
    let doing = purlis_core::hookwire::Doing {
        chat: 2,
        doing: purlis_core::doing::Said::Began {
            kind: purlis_core::doing::Kind::Reading,
            name: Some(format!("{TOUCHED}.rs")),
        },
        agent: None,
    };

    // No app listening: the tool call is spooled, the touch is lost.
    std::fs::create_dir_all(&root).expect("a project");
    let token = {
        let listener = Listener::bind(&root, &socket).expect("a socket");
        listener.tokens().issue_to_this_process(2).expect("a token")
    };
    deliver_tool(&socket, Some(&token), &call).expect("spooled");
    assert!(touch(&socket, Some(&token), &touching).is_err());
    assert!(purlis_core::hookwire::tell_doing(&socket, Some(&token), &doing).is_err());

    // An app listening: the tool call is recorded, the touch handed on in memory, and a touch
    // without the chat's token refused in the diagnostic log.
    let _ = std::fs::remove_file(&socket);
    let listener = Listener::bind(&root, &socket).expect("a socket");
    let token = listener.tokens().issue_to_this_process(2).expect("a token");
    let recorder = Arc::new(Mutex::new(Recorder::new(
        Log::open(&events, "DEVICE").expect("a log"),
        ArgsKey::open(&events).expect("a key"),
    )));
    let (tx, heard) = mpsc::channel();
    let tx = Mutex::new(tx);
    let reading = listener.hear(Hearing {
        secret_exec: Box::new(|_, _, writer| purlis_core::secrets::brokered::not_answered(writer)),
        each: Box::new(|_| Ok(())),
        answer: Box::new(|_, _| panic!("no ask")),
        noticed: Box::new(|_| {}),
        saved: Box::new(|_| {}),
        refused: Box::new(|_| Ok(())),
        tool: {
            let recorder = Arc::clone(&recorder);
            let plane = root.clone();
            Box::new(move |call| {
                let mut log = recorder.lock().unwrap();
                let event = log.tool(&plane, &call, std::time::Instant::now())?;
                log.durable().through(event.seq)
            })
        },
        blocked: Box::new(|_| {}),
        doing: Box::new(|_| {}),
        touching: Box::new(move |touching| tx.lock().unwrap().send(touching).unwrap()),
        permission: Box::new(|_| None),
    });
    deliver_tool(&socket, Some(&token), &call).expect("recorded");
    touch(&socket, Some(&token), &touching).expect("told");
    assert_eq!(
        heard.recv_timeout(std::time::Duration::from_secs(5)),
        Ok(touching.clone()),
        "the app heard the touch"
    );
    touch(&socket, None, &touching).expect("written");
    purlis_core::hookwire::tell_doing(&socket, Some(&token), &doing).expect("told");
    purlis_core::hookwire::tell_doing(&socket, None, &doing).expect("written");
    // The refusal is said on the listener's thread; give it its moment before the reader goes.
    std::thread::sleep(std::time::Duration::from_millis(300));
    drop(reading);
    drop(recorder);
}
