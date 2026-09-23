//! Every command line the leak guard's docstrings name as a bypass that SHIPPED, answered here
//! the way the frozen Python answers it — with no Python present.
//!
//! The answers are not written by hand. `tests/differential/shellseg.py --record` ran each of
//! these through the Python charter pinned at the commit the fixture planes come from;
//! `--check` fails if the file stops being what the oracle says. This replays the stage-3 half
//! of that recording, so the ordinary `cargo test` job holds the line and the differential job
//! — which fuzzes 200,000 generated cases against the live oracle — is the wider net rather
//! than the only one.
//!
//! # Why this is a test binary of its own
//!
//! Half of `_leak_reason`'s last arm is a question about a real directory: `guarded_state_entries`
//! scans one and `walk_into_guarded_state` resolves every operand against the PROCESS's working
//! directory. So this builds the harness's fixture plane in a temporary directory and stands in
//! it — and `set_current_dir` is process-wide, so it lives here, alone, rather than beside
//! tests that would race it.
//!
//! # Where the probe inputs come from
//!
//! Not from a third copy of the harness's tables. Stage 2 named "the per-answer keys are a
//! contract between three files and nothing makes them agree" as the thing to attack, so the
//! recording carries its own INPUTS in the `pr` key and this file reads them from the row. The
//! only tables here are the two the fixture itself defines — the directories a case may run in
//! and the entries the glob probe is put to — and both are built from the root this test just
//! made.

mod oracle_corpus;

use std::path::{Path, PathBuf};

use charter_core::{heredoc, leakguard, pypath, shellseg, shellwrap};
use serde_json::{Value, json};

/// The harness's `build_fixture`, to the letter: the plane every filesystem answer in the
/// recording is about.
///
/// **`.charter/` holds exactly ONE guarded entry on purpose.** `guarded_state_entries` hands its
/// caller the directory's own readdir order, so with two guarded entries under one operand the
/// walk's answer would be whichever the kernel yielded first — a fact about the filesystem, not
/// about the guard. `rich/` carries every filter case instead and is compared sorted.
fn build_fixture(root: &Path) {
    let state = root.join(".charter");
    let rich = root.join("rich");
    let w = |p: PathBuf, text: &str| {
        std::fs::create_dir_all(p.parent().expect("a file has a parent")).expect("mkdir");
        std::fs::write(p, text).expect("write");
    };
    w(state.join("vaults").join("db.json"), "{}\n");
    w(state.join("vaults.json"), "{}\n"); // the REGISTRY, not a vault
    w(state.join("state").join("x"), "x\n");
    std::fs::create_dir_all(rich.join("vaults")).expect("mkdir"); // EMPTY -> skipped
    w(rich.join("browser").join("p").join("prefs"), "x\n");
    std::fs::create_dir_all(rich.join("browser-empty")).expect("mkdir"); // EMPTY -> skipped
    for name in [
        "active-persona",
        "fingerprint.key",
        "browsers.txt",
        "vaults.json",
        "other",
    ] {
        w(rich.join(name), "x\n");
    }
    w(root.join("docs").join("a.md"), "a\n");
    w(root.join("sub").join("deep").join("b.txt"), "b\n");
    // The symlinks `realpath` is measured on: the cwd, the parent, the state directory, a chain
    // and a dangling name. Each is a spelling of an ancestor the guard has to resolve — and
    // `absvaults` is ABSOLUTE, because an absolute target is the one shape that makes CPython's
    // `realpath` reset its resolved path to `/`.
    //
    // No looping link, deliberately: on CPython 3.11 and 3.12 `Path.resolve()` raises a
    // `RuntimeError` for one and `_walk_into_guarded_state` catches only `OSError`, so the
    // ORACLE has no answer to record (charter#1166). `pypath`'s own unit test pins what this
    // implementation does there.
    for (link, target) in [
        ("here", ".".to_string()),
        ("up", "..".to_string()),
        ("tostate", ".charter".to_string()),
        ("dangling", "nowhere".to_string()),
        ("hop3", ".charter".to_string()),
        ("hop2", "hop3".to_string()),
        ("hop1", "hop2".to_string()),
        ("alsostate", ".charter".to_string()),
        (
            "absvaults",
            state.join("vaults").to_string_lossy().into_owned(),
        ),
    ] {
        let p = root.join(link);
        if !p.is_symlink() {
            std::os::unix::fs::symlink(&target, &p).expect("symlink");
        }
    }
}

/// `probe_cwds` in the harness.
fn cwds(root: &Path) -> [String; 6] {
    [
        root.to_string_lossy().into_owned(),
        root.join("sub").to_string_lossy().into_owned(),
        String::new(),
        ".".to_string(),
        root.join(".charter").to_string_lossy().into_owned(),
        "sub".to_string(),
    ]
}

/// `glob_entries` in the harness.
fn glob_entries(root: &Path) -> [PathBuf; 6] {
    [
        root.join(".charter").join("vaults"),
        root.join("rich").join("browser"),
        root.join("rich").join("active-persona"),
        root.join(".charter").join("vaults").join("db.json"),
        root.join("not-there"),
        root.join("sub"),
    ]
}

/// `rel` in the harness: a path as the corpus carries it.
fn rel(root: &Path, p: Option<&Path>) -> Value {
    let Some(p) = p else { return Value::Null };
    let p = p.to_string_lossy().into_owned();
    let root = root.to_string_lossy().into_owned();
    if p == root {
        return Value::String(".".into());
    }
    match p.strip_prefix(&format!("{root}/")) {
        Some(rest) => Value::String(rest.to_string()),
        None => Value::String("<outside>".into()),
    }
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .expect("a recorded probe list")
        .iter()
        .map(|x| x.as_str().expect("a recorded string").to_string())
        .collect()
}

#[test]
fn the_recorded_python_answer_is_the_answer_this_guard_gives() {
    let (ats, rows): (Vec<String>, Vec<Value>) = oracle_corpus::shellseg()
        .into_iter()
        .map(|r| (r.at, r.row))
        .unzip();
    assert!(
        rows.len() >= 300,
        "the corpus is the evidence; {} rows is not it",
        rows.len()
    );
    let tmp = tempfile::tempdir().expect("a temporary directory");
    // `canonicalize`, because the harness resolved its own root before recording: on macOS
    // `/var` is a symlink and an unresolved root makes every `rel` answer `<outside>`.
    let root = tmp.path().canonicalize().expect("the root resolves");
    build_fixture(&root);
    let state = root.join(".charter");
    let rich = root.join("rich");
    let missing = root.join("not-there");
    // Process-wide, which is why this test binary holds one test. `realpath` resolves a
    // relative operand against the process's directory, so an answer about `.` is only the same
    // answer from the same place.
    std::env::set_current_dir(&root).expect("the fixture root is enterable");

    let mut wrong: Vec<String> = Vec::new();
    for (at, row) in ats.iter().zip(&rows) {
        let cmd = row["cmd"].as_str().expect("every row names its command");
        let mut check = |what: &str, want: &Value, got: Value| {
            if want != &got {
                wrong.push(format!(
                    "{at} {cmd:?}\n    {what} python={want}\n    {what}   rust={got}"
                ));
            }
        };

        // The probe inputs come out of the recording, so this file holds no copy of the tables.
        let pr = row["pr"].as_array().expect("every row records its probes");
        let cwd = cwds(&root)[pr[0].as_u64().expect("an index") as usize].clone();
        let ops = strings(&pr[1]);
        let pats = strings(&pr[2]);
        let names = strings(&pr[3]);
        let all_entries = glob_entries(&root);
        let start = pr[4].as_u64().expect("an index") as usize;
        let entries: Vec<PathBuf> = (0..2)
            .map(|k| all_entries[(start + k) % all_entries.len()].clone())
            .collect();

        check(
            "lr",
            &row["lr"],
            json!(leakguard::leak_reason(cmd, &cwd, &state)),
        );
        check(
            "lacr",
            &row["lacr"],
            json!(
                leakguard::lines_a_command_could_run(cmd)
                    .into_iter()
                    .map(|(text, ex)| json!([text, ex]))
                    .collect::<Vec<_>>()
            ),
        );
        let (segments, _parsed) =
            shellseg::segment_argv_parsed(&heredoc::strip_reader_heredocs(cmd));
        check(
            "gseg",
            &row["gseg"],
            Value::Array(
                segments
                    .iter()
                    .map(|seg| {
                        let it = shellwrap::split_env_chdir(seg.as_slice());
                        let operands = leakguard::file_operands(&it.prog, &it.argv);
                        let walked =
                            leakguard::walks_into_guarded_state(&it.prog, &it.argv, &cwd, &state);
                        Value::Array(vec![
                            json!(leakguard::is_charter(&it.prog, &it.argv)),
                            json!(operands),
                            json!(leakguard::spliced_operands(&operands)),
                            json!(leakguard::gh_file_operands(&it.argv)),
                            json!(leakguard::excluded_names(&it.prog, &it.argv)),
                            json!(leakguard::walks_directories(&it.prog, &it.argv)),
                            rel(&root, walked.as_deref()),
                        ])
                    })
                    .collect(),
            ),
        );
        check(
            "nvp",
            &row["nvp"],
            json!(
                ops.iter()
                    .map(|o| leakguard::names_a_vault_path(o))
                    .chain(std::iter::once(leakguard::names_a_vault_path(cmd)))
                    .collect::<Vec<_>>()
            ),
        );
        check(
            "iab",
            &row["iab"],
            json!(ops.iter().map(|o| pypath::is_abs(o)).collect::<Vec<_>>()),
        );
        check(
            "np",
            &row["np"],
            json!(ops.iter().map(|o| pypath::normpath(o)).collect::<Vec<_>>()),
        );
        check(
            "pj",
            &row["pj"],
            json!(
                ops.iter()
                    .flat_map(|a| ops.iter().map(move |b| pypath::join(a, b)))
                    .collect::<Vec<_>>()
            ),
        );
        check(
            "rp",
            &row["rp"],
            Value::Array(
                ops.iter()
                    .map(|o| rel(&root, Some(Path::new(&pypath::realpath(o)))))
                    .collect(),
            ),
        );
        check(
            "fnm",
            &row["fnm"],
            json!(
                names
                    .iter()
                    .flat_map(|n| pats
                        .iter()
                        .map(move |p| json!([n, p, pypath::fnmatch(n, p)])))
                    .collect::<Vec<_>>()
            ),
        );
        check(
            "gap",
            &row["gap"],
            json!(
                ops.iter()
                    .map(|o| leakguard::gh_at_path(o))
                    .collect::<Vec<_>>()
            ),
        );
        check(
            "gse",
            &row["gse"],
            Value::Array(
                [&state, &rich, &missing]
                    .into_iter()
                    .map(|d| {
                        let mut ns: Vec<String> = leakguard::guarded_state_entries(d)
                            .iter()
                            .map(|p| {
                                p.file_name()
                                    .map(|n| n.to_string_lossy().into_owned())
                                    .unwrap_or_default()
                            })
                            .collect();
                        ns.sort();
                        json!(ns)
                    })
                    .collect(),
            ),
        );
        check(
            "gsi",
            &row["gsi"],
            json!(
                entries
                    .iter()
                    .flat_map(|e| pats.iter().flat_map(move |p| {
                        [0usize, 512]
                            .into_iter()
                            .map(move |lim| leakguard::glob_selects_inside(e, p, lim))
                    }))
                    .collect::<Vec<_>>()
            ),
        );
        check(
            "wigs0",
            &row["wigs0"],
            rel(
                &root,
                leakguard::walk_into_guarded_state(&cwd, &ops, &pats, &state).as_deref(),
            ),
        );
    }
    assert!(
        wrong.is_empty(),
        "{} of {} recorded cases are answered differently here:\n{}",
        wrong.len(),
        rows.len(),
        wrong.join("\n")
    );

    // --- and the second half: the recording really covers the rules ----------------------
    //
    // A recording can go stale in the one direction that matters — if the rows stop covering
    // the rules, the replay above passes while proving nothing. So each bypass the Python
    // docstrings say SHIPPED is asserted BY NAME, with a membership check, the way stage 1 and
    // stage 2 did it.
    let recorded: Vec<String> = rows
        .iter()
        .map(|row| row["cmd"].as_str().unwrap_or_default().to_owned())
        .collect();
    let has = |cmd: &str| {
        assert!(
            recorded.iter().any(|r| r == cmd),
            "{cmd:?} is not in the recording, so the wide test above is not running it",
        );
    };
    let vault = ".charter/vaults/x.json";

    // Each of the three refusal texts is a `pub const` with no key of its own in the harness,
    // because `leak_reason` returns it verbatim. That is only true while the corpus really
    // holds a denial of each kind, so it is asserted here rather than assumed.
    let denials: Vec<&str> = rows.iter().filter_map(|r| r["lr"].as_str()).collect();
    for (what, text) in [
        ("READ_REASON", leakguard::READ_REASON),
        ("REVEAL_REASON", leakguard::REVEAL_REASON),
        ("WALK_FIX", leakguard::WALK_FIX),
    ] {
        assert!(
            denials.iter().any(|d| d.contains(text)),
            "no recorded refusal carries {what}, so nothing compares its wording",
        );
    }

    // `_names_a_vault_path`'s second arm: `normpath` collapses the spellings a text pattern
    // alone walked straight past. One keystroke apart from the denied form, identical to the
    // kernel, and not a wrapper or a clever program.
    for cmd in [
        "cat .charter//vaults/x.json",
        "cat .charter/./vaults/x.json",
        "cat .charter/../.charter/vaults/x.json",
    ] {
        has(cmd);
        assert!(
            leakguard::leak_reason(cmd, "", &state).is_some(),
            "{cmd}: the separator noise is the same path",
        );
    }
    // ...and the case spellings, which are the same inode on a folding filesystem.
    for cmd in ["cat .CHARTER/vaults/x.json", "cat .charter/VAULTS/x.json"] {
        has(cmd);
        assert!(leakguard::leak_reason(cmd, "", &state).is_some(), "{cmd}");
    }
    // ...including the two characters CPython's `(?i)` folds to `i` and Rust's does not.
    for cmd in [
        "cat .charter/act\u{130}ve-persona",
        "cat .charter/act\u{131}ve-persona",
    ] {
        has(cmd);
        assert!(
            leakguard::leak_reason(cmd, "", &state).is_some(),
            "{cmd}: CPython's IGNORECASE folds the Turkic spellings of `i`",
        );
    }

    // `vaults` is a path SEGMENT: the directory itself is denied (#462 round three) and the
    // REGISTRY beside it is not (#443's false positive, which came back through the other
    // predicate).
    has("cat .charter/vaults");
    assert!(leakguard::leak_reason("cat .charter/vaults", "", &state).is_some());
    has("cat .charter/vaults.json");
    assert!(
        leakguard::leak_reason("cat .charter/vaults.json", "", &state).is_none(),
        "the registry is provider config and paths, never a value",
    );
    has("grep -rn vaults .charter/vaults.json");
    assert!(
        leakguard::leak_reason("grep -rn vaults .charter/vaults.json", "", &state).is_none(),
        "searching the registry for the word is an ordinary read",
    );

    // A relocation is followed however it is spelled — and a wrapper's own chdir was a live
    // bypass, because its value used to be read only to be thrown away.
    for cmd in [
        "cd .charter/vaults && cat x.json",
        "pushd .charter/vaults && cat x.json",
        "env -C .charter/vaults cat x.json",
        "sudo --chdir=.charter/vaults cat x.json",
        "env -iC.charter/vaults cat x.json",
    ] {
        has(cmd);
        assert!(
            leakguard::leak_reason(cmd, "", &state).is_some(),
            "{cmd}: the relocation moves where the later operand resolves",
        );
    }

    // The SHELL opens a redirection's target before any program is execed, and a wrapper's own
    // file flag is opened by the wrapper — both can leave `prog` empty, which is why `reads` is
    // asked above the program test.
    for cmd in [
        format!("< {vault} tee"),
        format!("tee < {vault}"),
        format!("xargs -a {vault} echo"),
    ] {
        has(&cmd);
        assert!(
            leakguard::leak_reason(&cmd, "", &state).is_some(),
            "{cmd}: the program named is not the one that opens the file",
        );
    }

    // A reader whose first operand is a SCRIPT or a PATTERN. Getting this wrong in one
    // direction denies a mention; in the other it allows a read.
    has(&format!("sed -n 1p {vault}"));
    assert!(leakguard::leak_reason(&format!("sed -n 1p {vault}"), "", &state).is_some());
    has(&format!("sed -i 's|{vault}|x|' f"));
    assert!(
        leakguard::leak_reason(&format!("sed -i 's|{vault}|x|' f"), "", &state).is_none(),
        "a rewrite MENTIONS the path; it does not open it",
    );
    has(&format!("grep -e {vault} f"));
    assert!(
        leakguard::leak_reason(&format!("grep -e {vault} f"), "", &state).is_none(),
        "the pattern came from the flag",
    );
    has(&format!("grep -f {vault} f"));
    assert!(
        leakguard::leak_reason(&format!("grep -f {vault} f"), "", &state).is_none(),
        "`-f` supplies the pattern FILE, and the Python reads it as the script operand",
    );

    // The word a substitution splices back: neither operand names a vault and the join does.
    has("cat $(echo .charter)/vaults/x.json");
    assert!(leakguard::leak_reason("cat $(echo .charter)/vaults/x.json", "", &state).is_some());

    // `--reveal` as a FLAG, never as a mention — the false positive this guard was rewritten to
    // stop having.
    for cmd in [
        "charter secret get v k --reveal",
        "python3 -m charter secret get v k --reveal",
        "CHARTER secret get v k --reveal",
    ] {
        has(cmd);
        assert_eq!(
            leakguard::leak_reason(cmd, "", &state).as_deref(),
            Some(leakguard::REVEAL_REASON),
            "{cmd}",
        );
    }
    // ...judged from a directory the plane's state is NOT under, because `rg` walks trees and
    // the point being made here is about the `--reveal` arm rather than about the walk.
    let elsewhere = root.join("docs").to_string_lossy().into_owned();
    for cmd in [
        "git commit -m \"docs: document the --reveal flag\"",
        "rg -n -- --reveal charter/",
    ] {
        has(cmd);
        assert!(
            leakguard::leak_reason(cmd, &elsewhere, &state).is_none(),
            "{cmd}: a commit message may legitimately MENTION the flag",
        );
    }

    // gh is not a reader and uploads the file anyway (#1086 class 5); `-` is the stdin body and
    // stays allowed.
    for cmd in [
        format!("gh pr create -F {vault}"),
        format!("gh api --input {vault}"),
        format!("gh api -F body=@{vault}"),
        format!("gh --flag api --input {vault}"),
    ] {
        has(&cmd);
        assert!(
            leakguard::leak_reason(&cmd, "", &state).is_some(),
            "{cmd}: gh reads the file and leaves the value on the forge",
        );
    }
    for cmd in ["gh pr create -F -", "gh pr create -F notes.md"] {
        has(cmd);
        assert!(leakguard::leak_reason(cmd, "", &state).is_none(), "{cmd}");
    }

    // The operand that CONTAINS the vault directory without naming it (#474) — and the fix the
    // refusal prints, which has to run.
    let at = root.to_string_lossy().into_owned();
    for cmd in ["grep -rn TOKEN .", "rg TOKEN", "ag TOKEN", "grep -r TOKEN"] {
        has(cmd);
        assert!(
            leakguard::leak_reason(cmd, &at, &state).is_some(),
            "{cmd}: the walk reaches the vault while naming nothing",
        );
    }
    for cmd in [
        "grep -rn --exclude-dir=.charter TOKEN .",
        "grep -rn --exclude-dir='.char*' TOKEN .",
        "rg --glob '!.charter' TOKEN .",
        "rg --glob '**/.charter/**' TOKEN .",
    ] {
        has(cmd);
        assert!(
            leakguard::leak_reason(cmd, &at, &state).is_none(),
            "{cmd}: a guard that refuses the command it recommends is one people route around",
        );
    }
    // ...and the `.` that is not a walk at all, because `grep` was given no recursion.
    has("grep -n TOKEN .");
    assert!(leakguard::leak_reason("grep -n TOKEN .", &at, &state).is_none());

    // The raw scan on the unparseable path: argv is a guess, so the string itself is matched.
    // Without this arm a command hides behind a broken quote.
    for cmd in [
        format!("cat '{vault}"),
        "echo 'x --reveal".to_string(),
        format!("echo 'it is {vault}"),
    ] {
        has(&cmd);
        assert!(
            leakguard::leak_reason(&cmd, "", &state).is_some(),
            "{cmd}: a false deny on a malformed command is survivable; a leak is not",
        );
    }

    // A heredoc body is data unless something runs it — the leak guard's half of A7's fact.
    let quiet = format!("cat <<'EOF'\n{vault}\nEOF");
    has(&quiet);
    assert!(
        leakguard::leak_reason(&quiet, "", &state).is_none(),
        "a body `cat` swallows is data, not a command line",
    );
    let loud = format!("bash <<'EOF'\ncat {vault}\nEOF");
    has(&loud);
    assert!(
        leakguard::leak_reason(&loud, "", &state).is_some(),
        "a body a SHELL receives is commands, and the guard has to see them",
    );
    let message = format!("git commit -F - <<'MSG'\nmentions {vault}\nMSG");
    has(&message);
    assert!(
        leakguard::leak_reason(&message, "", &state).is_none(),
        "a commit message is stored, never run (#997)",
    );
}
