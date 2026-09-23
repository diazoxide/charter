//! **Recorded behaviour**: every scenario the Python differential ran, replayed against this
//! build's `charter` and compared with what was recorded (ADR 0046).
//!
//! Until 2026-09-23 each of these ran twice, once through the Python charter pinned at
//! `50d31dc` and once through the Rust binary, and the two had to agree
//! (`tests/differential/run.py`, `doctor_scenarios.py`). The operator ruled that charter-app
//! stands alone, so the Python side was run ONE last time and its answers frozen into
//! `tests/fixtures/recorded/behaviour.jsonl`: each row is one scenario — the fixture plane it
//! starts from, what its setup changed (`start`), the command, its environment and stdin, and
//! what the run must leave (`expect`). A row was only written for a scenario the original
//! differential passed on the same head, so where the two implementations were compared the
//! recorded text IS Python's answer, and where they differed by decision (a `Divergence`, a
//! rewrite, a stream the differential did not compare) it is the app's, with a `notes` line
//! saying which.
//!
//! What each scenario checks is what the differential checked of the Rust side, rule for rule:
//!
//! - the exit status;
//! - stdout and stderr, byte for byte after the scenario's own masks — or up to a declared cut,
//!   or, for a refusal whose words were never compared, that it says what it refuses with;
//! - a hook's verdict (`denies`, `allows`, `says`), a status line's alert rows, and strings
//!   neither stream may print (`never_says`);
//! - every file, link and directory the plane holds afterwards, bytes and file modes included,
//!   except the paths the scenario ignores (a `.git`'s index and reflogs), which it asks git
//!   about instead (`facts`);
//! - that nothing was written beside the plane, except where the command's work lands (a push
//!   to a stand-in forge), and that it did land there.
//!
//! **Run it**: `cargo test -p charter-cli --test recorded_behaviour`, with scenario names (or
//! prefixes) after `--` to run only those. **Change a recorded answer on purpose**:
//! `CHARTER_RECORDED_BLESS=1 cargo test -p charter-cli --test recorded_behaviour -- <names>`
//! rewrites the named rows from what the binary does now; the diff of the fixture is then the
//! change, and the PR says why (ADR 0046).

mod facts;
mod fixture;
mod serve;
mod text;
mod tree;

use std::io::Write as _;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use serde_json::{Value, json};

use fixture::Blobs;
use text::Tokens;
use tree::Tree;

/// A scenario's index, the problems its replay found, and its row re-recorded when blessing.
type Outcome = (usize, Vec<String>, Option<Value>);

/// How long one command may take before the scenario is failed and the process killed.
const DEADLINE: Duration = Duration::from_secs(180);

struct Options {
    filters: Vec<String>,
    skip: Vec<String>,
    exact: bool,
    threads: usize,
    list: bool,
    nothing: bool,
    bless: bool,
}

/// libtest's own flags, the ones `cargo test --workspace -- …` hands every test binary, so this
/// one answers them the way the others do.
fn options() -> Options {
    let mut o = Options {
        filters: Vec::new(),
        skip: Vec::new(),
        exact: false,
        threads: std::thread::available_parallelism().map_or(4, |n| n.get()),
        list: false,
        nothing: false,
        bless: std::env::var_os("CHARTER_RECORDED_BLESS").is_some_and(|v| v == "1"),
    };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let (flag, inline) = match arg.split_once('=') {
            Some((f, v)) if f.starts_with("--") => (f.to_owned(), Some(v.to_owned())),
            _ => (arg.clone(), None),
        };
        let mut value = || inline.clone().or_else(|| args.next()).unwrap_or_default();
        match flag.as_str() {
            "--test-threads" => o.threads = value().parse().unwrap_or(o.threads).max(1),
            "--skip" => o.skip.push(value()),
            "--exact" => o.exact = true,
            "--list" => o.list = true,
            // No scenario is ignored, and none is a benchmark.
            "--ignored" | "--bench" => o.nothing = true,
            "--format" | "--color" | "--logfile" | "-Z" | "--shuffle-seed" => {
                value();
            }
            f if f.starts_with('-') => {}
            _ => o.filters.push(arg),
        }
    }
    o
}

fn selected(o: &Options, name: &str) -> bool {
    let hit = |f: &String| {
        if o.exact {
            name == f
        } else {
            name.contains(f.as_str())
        }
    };
    (o.filters.is_empty() || o.filters.iter().any(hit)) && !o.skip.iter().any(hit)
}

fn main() {
    let o = options();
    let mut rows = fixture::scenarios();
    let names: Vec<String> = rows
        .iter()
        .map(|r| r["name"].as_str().expect("a name").to_owned())
        .collect();
    let chosen: Vec<usize> = (0..rows.len())
        .filter(|&i| !o.nothing && selected(&o, &names[i]))
        .collect();
    if o.list {
        for &i in &chosen {
            println!("{}: test", names[i]);
        }
        return;
    }

    let blobs = Mutex::new(Blobs::load());
    let next = AtomicUsize::new(0);
    let results: Mutex<Vec<Outcome>> = Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for _ in 0..o.threads.min(chosen.len().max(1)) {
            scope.spawn(|| {
                loop {
                    let k = next.fetch_add(1, Ordering::SeqCst);
                    let Some(&i) = chosen.get(k) else { break };
                    let (problems, blessed) = replay(&rows[i], &blobs, o.bless);
                    results
                        .lock()
                        .expect("results")
                        .push((i, problems, blessed));
                }
            });
        }
    });

    let mut results = results.into_inner().expect("results");
    results.sort_by(|a, b| names[a.0].cmp(&names[b.0]));
    let mut failed = Vec::new();
    let mut blessed = 0;
    for (i, problems, row) in results {
        if !problems.is_empty() {
            println!("DIFF {}", names[i]);
            for line in &problems {
                println!("{line}");
            }
            failed.push(names[i].clone());
        }
        if let Some(row) = row {
            if row != rows[i] {
                blessed += 1;
                println!("re-recorded {}", names[i]);
            }
            rows[i] = row;
        }
    }
    if o.bless {
        fixture::write_scenarios(&rows);
        blobs.lock().expect("blobs").save_if_changed(&rows);
        println!(
            "\nrecorded behaviour: {blessed} of {} scenarios re-recorded; a difference above \
             that no row can hold (a refusal's words, a hook's verdict) is still a failure — \
             run again without CHARTER_RECORDED_BLESS",
            chosen.len()
        );
        return;
    }
    println!();
    if failed.is_empty() {
        println!(
            "recorded behaviour: all {} scenarios as recorded",
            chosen.len()
        );
    } else {
        println!(
            "recorded behaviour: {} of {} scenarios differ from the record: {}",
            failed.len(),
            chosen.len(),
            failed.join(", ")
        );
        std::process::exit(1);
    }
}

/// One scenario: lay it out, run it, compare. The problems found, and — when blessing — the
/// row rewritten from what happened.
fn replay(row: &Value, blobs: &Mutex<Blobs>, bless: bool) -> (Vec<String>, Option<Value>) {
    let tmp = tempfile::Builder::new()
        .prefix("recorded-")
        .tempdir()
        .expect("a scratch directory");
    let side = tmp.path().join("rust");
    std::fs::create_dir_all(&side).expect("the side's directory");
    let out = run(row, &side, tmp.path(), blobs, bless);
    tree::unseal(tmp.path());
    out
}

fn lay_out_plane(side: &Path, plane: &str) {
    let root = side.join("plane");
    if plane.is_empty() {
        std::fs::create_dir_all(&root).expect("a directory that is not a plane yet");
    } else {
        fixture::copy_plane(&fixture::planes().join(plane), &root);
        let listing = fixture::planes().join(format!("{plane}.empty-dirs"));
        if let Ok(text) = std::fs::read_to_string(listing) {
            for rel in text.split_whitespace() {
                std::fs::create_dir_all(root.join(rel)).expect("an empty directory");
            }
        }
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("after 1970")
        .as_secs();
    fixture::stamp(
        &root,
        std::time::UNIX_EPOCH + std::time::Duration::from_secs(now),
    );
    std::fs::create_dir_all(side.join("home")).expect("the side's home");
}

/// Every file beside the plane, with its contents — the differential's `_outside`.
fn outside(side: &Path) -> std::collections::BTreeMap<String, Vec<u8>> {
    fn walk(side: &Path, dir: &Path, out: &mut std::collections::BTreeMap<String, Vec<u8>>) {
        let Ok(read) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in read.filter_map(Result::ok) {
            let path = entry.path();
            let rel = path
                .strip_prefix(side)
                .expect("under the side")
                .to_string_lossy()
                .into_owned();
            if rel == "plane" || rel == "pins" {
                continue;
            }
            let Ok(meta) = std::fs::symlink_metadata(&path) else {
                continue;
            };
            if meta.is_dir() {
                walk(side, &path, out);
            } else if path.is_dir() {
                // A link to a directory: `is_dir()` followed it, so the differential skipped it.
            } else {
                let bytes = std::fs::read(&path).unwrap_or_else(|_| b"<unreadable>".to_vec());
                out.insert(rel, bytes);
            }
        }
    }
    let mut out = std::collections::BTreeMap::new();
    walk(side, side, &mut out);
    out
}

struct Ran {
    code: i32,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

fn execute(row: &Value, tokens: &Tokens, fence: &Path) -> Result<Ran, String> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_charter"));
    for arg in row["args"].as_array().expect("args") {
        command.arg(tokens.unpaths(arg.as_str().expect("an argument")));
    }
    command
        .current_dir(tokens.unpaths(row["cwd"].as_str().expect("a cwd")))
        .env_clear();
    for (name, value) in row["env"].as_object().expect("an environment") {
        command.env(name, tokens.unpaths(value.as_str().expect("a value")));
    }
    // This test build is fenced (charter-app#129), and a fenced build unset falls back to the
    // temporary directory the ENVIRONMENT names — which a cleared environment does not. So the
    // fence is named: this scenario's own scratch tree, and nothing else.
    command
        .env("CHARTER_PLANE_FENCE", fence)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().map_err(|e| format!("the binary: {e}"))?;
    let stdin = row["stdin"]
        .as_str()
        .unwrap_or_default()
        .as_bytes()
        .to_vec();
    let mut pipe = child.stdin.take().expect("stdin");
    let writer = std::thread::spawn(move || {
        let _ = pipe.write_all(&stdin);
    });
    let pid = child.id();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(child.wait_with_output());
    });
    let output = match rx.recv_timeout(DEADLINE) {
        Ok(done) => done.map_err(|e| format!("waiting: {e}"))?,
        Err(_) => {
            // Only the process this scenario started, by its own id.
            let _ = Command::new("kill").arg("-9").arg(pid.to_string()).status();
            return Err(format!(
                "the command ran past {}s and was killed",
                DEADLINE.as_secs()
            ));
        }
    };
    let _ = writer.join();
    use std::os::unix::process::ExitStatusExt;
    let code = output
        .status
        .code()
        .unwrap_or_else(|| -output.status.signal().unwrap_or(0));
    Ok(Ran {
        code,
        stdout: output.stdout,
        stderr: output.stderr,
    })
}

fn run(
    row: &Value,
    side: &Path,
    fence: &Path,
    blobs: &Mutex<Blobs>,
    bless: bool,
) -> (Vec<String>, Option<Value>) {
    let expect = &row["expect"];
    let mut problems: Vec<String> = Vec::new();
    let mut new = row.clone();

    lay_out_plane(side, row["plane"].as_str().unwrap_or_default());
    let tokens = Tokens::new(side);
    let modes = {
        let blobs = blobs.lock().expect("blobs");
        tree::lay_down(side, &row["start"], &blobs, &tokens)
    };
    let before = tree::snapshot(side, &tokens);
    tree::close(modes);
    let outside_before = outside(side);
    let served = row["serve"]
        .as_object()
        .map(|serve| serve::start(serve, side));

    let ran = execute(row, &tokens, fence);
    if let Some(served) = served {
        served.finish();
    }
    let ran = match ran {
        Ok(ran) => ran,
        Err(why) => return (vec![format!("    {why}")], None),
    };
    for pair in row["teardown"].as_array().into_iter().flatten() {
        use std::os::unix::fs::PermissionsExt;
        let path = side.join(pair[0].as_str().expect("a path"));
        let mode = pair[1].as_u64().expect("a mode") as u32;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode));
    }
    let after = tree::snapshot(side, &tokens);
    let outside_after = outside(side);
    let stdout = text::captured(&ran.stdout);
    let stderr = text::captured(&ran.stderr);

    // ---- exit
    if ran.code != expect["exit"].as_i64().unwrap_or(0) as i32 {
        problems.push(format!(
            "    exited {}, and the record says {} — stderr: {:?}",
            ran.code,
            expect["exit"],
            stderr.trim()
        ));
        new["expect"]["exit"] = json!(ran.code);
    }

    // ---- never_says, on the raw streams: a secret is not less printed for being masked
    for needle in expect["never_says"].as_array().into_iter().flatten() {
        let needle = needle.as_str().expect("a needle");
        for (stream, said) in [("stdout", &stdout), ("stderr", &stderr)] {
            if said.contains(needle) {
                problems.push(format!(
                    "    PRINTED THE SECRET on {stream}: {needle:?} — a credential is refused by \
                     KIND, never by the matched text"
                ));
            }
        }
    }

    // ---- stderr
    let e = &expect["stderr"];
    let smasks = text::masks(&e["masks"]);
    let mut err = stderr.clone();
    if let Some(rules) = e["drop_lines"].as_array() {
        let rules: Vec<regex::Regex> = rules
            .iter()
            .map(|p| regex::Regex::new(p.as_str().expect("a pattern")).expect("a regex"))
            .collect();
        let (mut kept, mut taken) = (String::new(), String::new());
        for line in text::lines(&stderr) {
            if rules.iter().any(|r| r.is_match(line)) {
                taken.push_str(line);
            } else {
                kept.push_str(line);
            }
        }
        let dropped = tokens.stream(&text::masked(&taken, &smasks));
        let want = e["dropped"].as_str().unwrap_or_default();
        if dropped != want {
            problems.push("    the block of stderr lines this scenario sets apart differs:".into());
            problems.extend(text::diff(want, &dropped));
            new["expect"]["stderr"]["dropped"] = json!(dropped);
        }
        err = kept;
    }
    match e["rule"].as_str() {
        Some("exact") => {
            let got = tokens.stream(&text::masked(&err, &smasks));
            let want = e["text"].as_str().unwrap_or_default();
            if got != want {
                problems.push("    stderr differs from the record:".into());
                problems.extend(text::diff(want, &got));
                new["expect"]["stderr"]["text"] = json!(got);
            }
        }
        Some("contains") => {}
        other => problems.push(format!(
            "    the record has no stderr rule it knows: {other:?}"
        )),
    }
    if let Some(needle) = e["contains"].as_str()
        && !stderr.contains(needle)
    {
        problems.push(format!(
            "    did not refuse with {needle:?}; it said {:?}",
            stderr.trim()
        ));
    }

    // ---- stdout
    let s = &expect["stdout"];
    let omasks = text::masks(&s["masks"]);
    match s["rule"].as_str() {
        Some("exact") => {
            let got = tokens.stream(&text::masked(&stdout, &omasks));
            let want = s["text"].as_str().unwrap_or_default();
            if got != want {
                problems.push("    stdout differs from the record:".into());
                problems.extend(text::diff(want, &got));
                new["expect"]["stdout"]["text"] = json!(got);
            }
        }
        Some("prefix") => {
            let cut = s["cut"].as_str().expect("a cut");
            match stdout.split_once(cut) {
                None => problems.push(format!(
                    "    stdout never reaches {cut:?}, the boundary the record is cut at"
                )),
                Some((head, _)) => {
                    let got = tokens.stream(head);
                    let want = s["text"].as_str().unwrap_or_default();
                    if got != want {
                        problems.push(format!("    stdout differs before {cut:?}:"));
                        problems.extend(text::diff(want, &got));
                        new["expect"]["stdout"]["text"] = json!(got);
                    }
                }
            }
        }
        other => problems.push(format!(
            "    the record has no stdout rule it knows: {other:?}"
        )),
    }
    if let Some(denies) = expect["denies"].as_str() {
        match text::decision(&stdout) {
            None => problems.push(format!(
                "    printed no PreToolUse denial at all: {stdout:?}"
            )),
            Some(said) if !said.contains(denies) => problems.push(format!(
                "    denied with something else — wanted {denies:?}, got {said:?}"
            )),
            Some(_) => {}
        }
    }
    if expect["allows"].as_bool() == Some(true) && !stdout.is_empty() {
        problems.push(format!("    did not allow — it printed {stdout:?}"));
    }
    if let Some(says) = expect["says"].as_str()
        && !text::spoken(&stdout).contains(says)
    {
        problems.push(format!("    did not say {says:?} — it printed {stdout:?}"));
    }
    if let Some(want) = expect["alerts"].as_array() {
        let got: Vec<Value> = stdout
            .lines()
            .filter(|l| l.contains(text::ALERT_MARK))
            .map(|l| json!(tokens.stream(l)))
            .collect();
        if &got != want {
            problems.push("    the status line's alert rows differ:".into());
            for line in want {
                problems.push(format!("      recorded {line}"));
            }
            for line in &got {
                problems.push(format!("      now      {line}"));
            }
            new["expect"]["alerts"] = json!(got);
        }
    }

    // ---- the plane afterwards
    let ignore: Vec<String> = expect["ignore"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|v| v.as_str().expect("a path").to_owned())
        .collect();
    let before_plane = tree::compared(&tree::under(&before, "plane"), &ignore);
    let got_plane = tree::compared(&tree::under(&after, "plane"), &ignore);
    {
        let mut blobs = blobs.lock().expect("blobs");
        let want_plane: Tree = tree::compared(
            &tree::applied(&before_plane, &expect["tree"], &blobs),
            &ignore,
        );
        let differences = tree::differences(&want_plane, &got_plane);
        if !differences.is_empty() {
            problems.push("    the plane it left differs from the record:".into());
            problems.extend(differences);
            if bless {
                new["expect"]["tree"] = tree::delta(&before_plane, &got_plane, &mut blobs);
            }
        }
    }

    // ---- facts
    if let Some(f) = expect["facts"].as_object() {
        let kind = f["kind"].as_str().expect("a kind");
        let args = f["args"].as_array().cloned().unwrap_or_default();
        match facts::read(kind, &args, &side.join("plane")) {
            Err(why) => problems.push(format!("    facts: {why}")),
            Ok(got) => {
                let got = tokens.paths(&got);
                let want = f["text"].as_str().unwrap_or_default();
                if facts::platform_neutral(&got) != facts::platform_neutral(want) {
                    problems.push(format!("    the {kind} facts differ:"));
                    problems.extend(text::diff(want, &got));
                    new["expect"]["facts"]["text"] = json!(got);
                }
            }
        }
    }

    // ---- beside the plane
    let mut wrote: Vec<String> = Vec::new();
    for (rel, bytes) in &outside_after {
        match outside_before.get(rel) {
            None => wrote.push(rel.clone()),
            Some(was) if was != bytes => wrote.push(rel.clone()),
            _ => {}
        }
    }
    wrote.extend(
        outside_before
            .keys()
            .filter(|rel| !outside_after.contains_key(*rel))
            .cloned(),
    );
    let allowed: Vec<&str> = expect["outside"]["allowed"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|v| v.as_str().expect("a prefix"))
        .collect();
    for rel in &wrote {
        if !allowed.iter().any(|p| rel.starts_with(p)) {
            problems.push(format!("    WROTE OUTSIDE ITS PLANE: {rel}"));
        }
    }
    let hit: Vec<Value> = allowed
        .iter()
        .filter(|p| wrote.iter().any(|rel| rel.starts_with(**p)))
        .map(|p| json!(p))
        .collect();
    if expect["outside"]["hit"]
        .as_array()
        .is_some_and(|h| h != &hit)
    {
        problems.push(format!(
            "    the record says the command's work lands in {}, and now it lands in {}",
            expect["outside"]["hit"],
            Value::Array(hit.clone())
        ));
        new["expect"]["outside"]["hit"] = Value::Array(hit);
    }

    let blessed = bless.then_some(new);
    (problems, blessed)
}
