//! Every command line stage 4's docstrings name as a bypass that SHIPPED, answered here the way
//! the frozen Python answers it — with no Python present.
//!
//! The answers are not written by hand. `tests/differential/shellseg.py --record` ran each of
//! these through the Python charter pinned at the commit the fixture planes come from;
//! `--check` fails if the file stops being what the oracle says. This replays the stage-4 half
//! of that recording — A2 (the golden rule), A4 (the release floor), the live-substitution walk
//! and A5/A6 (the two prose guards) — so the ordinary `cargo test` job holds the line and the
//! differential job, which fuzzes 200,000 generated cases against the live oracle, is the wider
//! net rather than the only one.
//!
//! # The one thing this needs on disk
//!
//! A `charter.toml`. `single_credential_hit`'s denial set is built from the hosts the plane
//! knows, which the Python reads through `config.ROOT` and this takes as an argument — so the
//! recording is only replayable beside the same `[[forge]]` blocks the harness wrote. There is
//! no other filesystem answer here, and nothing changes the process's working directory, which
//! is why this is a separate binary from the leak guard's replay rather than a test inside it.
//!
//! # Where the probe inputs come from
//!
//! The scanners' start positions and the heredocs they are handed are derived from the command
//! itself, exactly as the harness derives them, and the derivation is short enough to be read
//! side by side. The TABLES are not copied here at all — they are compared as data through the
//! recording's `tbl` key.

mod oracle_corpus;

use std::path::Path;

use charter_core::{credguard, floorguard, forge, livesub, proseguard, shellseg, shellwrap};
use serde_json::{Value, json};

/// The harness's `build_fixture` stage-4 half, to the letter. The third block does NOT resolve,
/// on purpose: `known_forges` degrades per block, and both implementations have to agree that
/// one bad block costs only itself.
const FIXTURE_CHARTER_TOML: &str = "schema = 1\n\n\
     [[forge]]\nkind = \"github\"\nowner = \"diazoxide\"\n\n\
     [[forge]]\nkind = \"gitlab\"\nhost = \"git.internal\"\n\n\
     [[forge]]\nkind = \"github\"\nhost = \"gitlab.com\"\n\n\
     [[forge]]\nkind = \"gitlab\"\nhost = \"UP.EXAMPLE\"\n\n\
     [[forge]]\nkind = \"nosuchforge\"\nhost = \"broken.example\"\n";

/// `PROBE_AT` in the harness.
const PROBE_AT: usize = 4;

/// One row of `_CHARTER_PROSE` as the `tbl` answer carries it: `((noun, verb), (dest, file))`.
type ProseRow = (
    (&'static str, &'static str),
    (&'static str, Option<&'static str>),
);

/// `PROBE_MODES` in the harness.
const PROBE_MODES: [Option<&str>; 5] = [
    None,
    Some("bypassPermissions"),
    Some("default"),
    Some("acceptEdits"),
    Some("BYPASSPERMISSIONS"),
];

/// `PROBE_PENDING` in the harness.
fn probe_pending() -> Vec<(String, bool, bool)> {
    vec![
        ("EOF".to_string(), true, false),
        ("EOF".to_string(), false, false),
        ("EOF".to_string(), true, true),
        (String::new(), true, false),
    ]
}

/// `probe_positions` in the harness.
fn probe_positions(chars: &[char]) -> Vec<usize> {
    let mut out = vec![0usize];
    for (i, c) in chars.iter().enumerate() {
        if out.len() >= PROBE_AT {
            break;
        }
        if matches!(c, '\'' | '"' | '$' | '\n') {
            out.push(i + 1);
        }
    }
    while out.len() < PROBE_AT {
        out.push(chars.len());
    }
    out
}

/// `rotation` in the harness.
fn rotation<T: Clone>(cmd: &str, table: &[T], count: usize) -> Vec<T> {
    let n = cmd.chars().count();
    (0..count)
        .map(|k| table[(n + k) % table.len()].clone())
        .collect()
}

/// The curated rows alone — the command lines a docstring names. The wide replay below reads
/// the frozen fuzz subset as well (`oracle_corpus::shellseg`).
fn corpus() -> Vec<Value> {
    oracle_corpus::jsonl("shellseg-oracle.jsonl")
        .into_iter()
        .map(|r| r.row)
        .collect()
}

fn lsub(v: Option<&'static str>) -> Value {
    match v {
        None => Value::Null,
        Some(s) => Value::String(s.to_string()),
    }
}

fn pair(v: Option<(&'static str, String)>) -> Value {
    match v {
        None => Value::Null,
        Some((shape, said)) => json!([shape, said]),
    }
}

/// The `tbl` answer, built the way the module builds its own tables rather than from a copy.
fn tables(cmd: &str) -> Value {
    let mut forge_prose: Vec<[&str; 3]> = proseguard::FORGE_PROSE
        .iter()
        .map(|(a, b, c)| [*a, *b, *c])
        .collect();
    forge_prose.sort_unstable();
    let mut publish: Vec<[&str; 3]> = floorguard::PUBLISH_FORGE
        .iter()
        .map(|(a, b, c)| [*a, *b, *c])
        .collect();
    publish.sort_unstable();
    let mut tags: Vec<&str> = floorguard::TAG_HARMLESS.to_vec();
    tags.sort_unstable();
    let mut charter_prose: Vec<ProseRow> = Vec::new();
    for (noun, verb, dest, from_file) in proseguard::CHARTER_PROSE_ROWS {
        charter_prose.push(((noun, verb), (dest, from_file)));
        if let Some(alias) = match noun {
            "workspace" => Some("ws"),
            "worktree" => Some("wt"),
            _ => None,
        } {
            charter_prose.push(((alias, verb), (dest, from_file)));
        }
    }
    charter_prose.sort_unstable_by_key(|((noun, verb), _)| (*noun, *verb));
    let rows: Vec<Value> = charter_prose
        .iter()
        .map(|((noun, verb), (dest, from_file))| json!([[noun, verb], [dest, from_file]]))
        .collect();
    json!([
        [
            json!(forge_prose.len()),
            json!(rotation(cmd, &forge_prose, 3))
        ],
        json!(publish),
        json!(tags),
        json!(livesub::SUBSTITUTIONS),
        json!(floorguard::UNATTENDED_MODE),
        [json!(rows.len()), json!(rotation(cmd, &rows, 2))],
    ])
}

#[test]
fn the_recorded_python_answer_is_the_answer_these_guards_give() {
    let (ats, rows): (Vec<String>, Vec<Value>) = oracle_corpus::shellseg()
        .into_iter()
        .map(|r| (r.at, r.row))
        .unzip();
    assert!(
        rows.len() >= 440,
        "the corpus is the evidence; {} rows is not it",
        rows.len()
    );
    let tmp = tempfile::tempdir().expect("a temporary directory");
    let root: &Path = tmp.path();
    std::fs::write(root.join("charter.toml"), FIXTURE_CHARTER_TOML).expect("write charter.toml");
    let forges = forge::known_ordered(root);

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
        let chars: Vec<char> = cmd.chars().collect();
        let at = probe_positions(&chars);
        let pending = probe_pending();
        let segments = shellseg::segment_argv(cmd);
        let befores = shellwrap::exported_env(&segments);

        check("tbl", &row["tbl"], tables(cmd));
        check(
            "kf",
            &row["kf"],
            json!(
                forges
                    .iter()
                    .map(|f| json!([f.host, f.kind.cli()]))
                    .collect::<Vec<_>>()
            ),
        );
        check(
            "sph",
            &row["sph"],
            json!(
                credguard::ssh_prefix_hosts(&forges)
                    .into_iter()
                    .map(|(p, h)| json!([p, h]))
                    .collect::<Vec<_>>()
            ),
        );
        check(
            "sch",
            &row["sch"],
            match credguard::single_credential_hit(cmd, &forges) {
                None => Value::Null,
                Some((shape, detail)) => json!([shape, detail]),
            },
        );
        check(
            "scr",
            &row["scr"],
            json!(credguard::single_credential_reason(cmd, &forges)),
        );
        check(
            "s4seg",
            &row["s4seg"],
            Value::Array(
                segments
                    .iter()
                    .zip(befores.iter())
                    .map(|(toks, before)| {
                        let (prog, seg_env, argv) = shellwrap::split_env(toks);
                        let args: Vec<String> = argv.iter().skip(1).cloned().collect();
                        let mut env = before.clone();
                        env.extend(seg_env);
                        json!([
                            credguard::git_subcommand(&args),
                            credguard::has_ssh_command_config(&args),
                            credguard::has_config_env_sshcommand(&args),
                            credguard::has_git_config_env_sshcommand(&env),
                            credguard::is_sshcommand_config_write(&args),
                            credguard::url_args(&args),
                            proseguard::charter_words(&prog, &argv),
                        ])
                    })
                    .collect(),
            ),
        );
        check("ee", &row["ee"], json!(befores));
        check(
            "rfr",
            &row["rfr"],
            json!(floorguard::release_floor_reason(cmd, true)),
        );
        check(
            "rfa",
            &row["rfa"],
            json!(floorguard::release_floor_reason(cmd, false)),
        );
        check(
            "unat",
            &row["unat"],
            json!(
                PROBE_MODES
                    .iter()
                    .map(|m| floorguard::unattended(*m))
                    .collect::<Vec<_>>()
            ),
        );
        check("ls", &row["ls"], lsub(livesub::live_substitution(cmd)));
        check(
            "ace",
            &row["ace"],
            json!(
                at.iter()
                    .map(|&i| json!([i, livesub::ansi_c_end(&chars, i)]))
                    .collect::<Vec<_>>()
            ),
        );
        check(
            "dqs",
            &row["dqs"],
            json!(
                at.iter()
                    .map(|&i| {
                        let (hit, next) = livesub::double_quoted_substitution(&chars, i);
                        json!([i, [lsub(hit), json!(next)]])
                    })
                    .collect::<Vec<_>>()
            ),
        );
        check(
            "hsub",
            &row["hsub"],
            lsub(livesub::heredoc_substitution(cmd)),
        );
        check(
            "hb",
            &row["hb"],
            json!(
                at.iter()
                    .map(|&i| {
                        let (hit, next) = livesub::heredoc_bodies(&chars, i, &pending);
                        json!([i, [lsub(hit), json!(next)]])
                    })
                    .collect::<Vec<_>>()
            ),
        );
        check(
            "fpc",
            &row["fpc"],
            json!(proseguard::forge_prose_command(cmd)),
        );
        check(
            "fsh",
            &row["fsh"],
            pair(proseguard::forge_substitution_hit(cmd)),
        );
        check(
            "cpc",
            &row["cpc"],
            match proseguard::charter_prose_command(cmd) {
                None => Value::Null,
                Some((where_, dest, from_file)) => json!([where_, dest, from_file]),
            },
        );
        check(
            "csh",
            &row["csh"],
            pair(proseguard::charter_substitution_hit(cmd)),
        );
    }
    assert!(
        wrong.is_empty(),
        "{} of {} recorded cases are answered differently here:\n{}",
        wrong.len(),
        rows.len(),
        wrong.join("\n")
    );
}

/// The second half, and the one that goes stale silently: **does the recording still COVER the
/// rules?**
///
/// A recording can fail in one direction the replay above cannot see — if the rows stop covering
/// the rules, it passes while proving nothing. So each bypass the Python docstrings say SHIPPED
/// is asserted BY NAME, with a membership check, the way stages 1, 2 and 3 did it.
#[test]
fn the_recording_still_covers_the_rules() {
    let rows = corpus();
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

    // The two long refusal openings are `pub const`s with no key of their own in the harness,
    // because their guard returns them verbatim in front of every denial. That is only true
    // while the corpus really holds a denial of each kind, so it is asserted here.
    let creds: Vec<&str> = rows.iter().filter_map(|r| r["scr"].as_str()).collect();
    assert!(
        creds
            .iter()
            .any(|d| d.starts_with(credguard::SINGLE_CREDENTIAL_FIX)),
        "no recorded golden-rule refusal carries SINGLE_CREDENTIAL_FIX, so nothing compares it",
    );
    let floors: Vec<&str> = rows.iter().filter_map(|r| r["rfr"].as_str()).collect();
    assert!(
        floors
            .iter()
            .any(|d| d.starts_with(floorguard::RELEASE_FLOOR_FIX)),
        "no recorded floor refusal carries RELEASE_FLOOR_FIX, so nothing compares it",
    );
    // …and the attended answer must really be `None` everywhere, or the gate is not a gate.
    assert!(
        rows.iter().all(|r| r["rfa"].is_null()),
        "a recorded ATTENDED case is refused, which is not what the floor is",
    );

    // ---- A2 -------------------------------------------------------------------------------
    let root = tempfile::tempdir().expect("a temporary directory");
    std::fs::write(root.path().join("charter.toml"), FIXTURE_CHARTER_TOML).expect("write");
    let forges = forge::known_ordered(root.path());
    let denies = |cmd: &str| credguard::single_credential_hit(cmd, &forges).is_some();

    // #496 in this guard: what an EARLIER segment exported reaches the same git as an attached
    // prefix does. The attached spelling was denied and this one was allowed.
    for cmd in [
        "GIT_SSH_COMMAND=/tmp/k git push",
        "export GIT_SSH_COMMAND=/tmp/k && git push",
        "typeset -gx GIT_SSH=/tmp/k && git push",
        "GIT_SSH_COMMAND=/tmp/k; export GIT_SSH_COMMAND; git push",
        "set -a && GIT_SSH_COMMAND=/tmp/k && git push",
    ] {
        has(cmd);
        assert!(denies(cmd), "{cmd}: an exported override reaches this git");
    }
    // `declare` without `-x` is a shell variable and exports nothing.
    has("declare GIT_SSH_COMMAND=/tmp/k && git push");
    assert!(!denies("declare GIT_SSH_COMMAND=/tmp/k && git push"));
    // The environment only ever GROWS: forgetting one is the fail-OPEN direction.
    has("export GIT_SSH_COMMAND=/tmp/k && unset GIT_SSH_COMMAND && git push");
    assert!(denies(
        "export GIT_SSH_COMMAND=/tmp/k && unset GIT_SSH_COMMAND && git push"
    ));

    // A Shift key is not a bypass — the program, the host and the config key all fold.
    for cmd in [
        "GIT push git@github.com:o/r.git",
        "git clone GIT@GITHUB.COM:o/r.git",
        "git -c CORE.SSHCOMMAND=x push",
        "GIT_CONFIG_KEY_0=CORE.SSHCOMMAND git push",
    ] {
        has(cmd);
        assert!(denies(cmd), "{cmd}: the case is the same command");
    }
    // …and a URL inside a MESSAGE is free text, not an operand.
    for cmd in [
        "git commit -m git@github.com:o/r.git",
        "git commit --message=git@github.com:o/r.git",
        "git commit -F git@github.com:o/r.git",
    ] {
        has(cmd);
        assert!(!denies(cmd), "{cmd}: a message value is not a repo URL");
    }
    // The DECLARED self-hosted host, which no class default can ever match on its own.
    has("git push git@git.internal:o/r.git");
    assert!(denies("git push git@git.internal:o/r.git"));

    // CPython's `$` matches before a trailing newline and the crate's does not. A quoted newline
    // is how a token carries one.
    has("GIT_CONFIG_KEY_0='core.sshCommand\n' git push");
    assert!(denies("GIT_CONFIG_KEY_0='core.sshCommand\n' git push"));
    has("GIT_CONFIG_KEY_0='core.ssh\nCommand' git push");
    assert!(!denies("GIT_CONFIG_KEY_0='core.ssh\nCommand' git push"));
    // `\d` is `\p{Nd}` in both engines, `(?i)` folds `ı`/`İ` to `i` in CPython and in neither in
    // the crate — and **the ROUTE is what makes either of those reachable**, which is a finding
    // rather than a detail. A bare prefix cannot carry a Unicode digit: `is_env_assignment` is
    // `^[A-Za-z_][A-Za-z0-9_]*=`, so the shell does not read it as an assignment and the pattern
    // never sees it. Through `env`, whose own operand rule is only "contains an `=`", it does.
    // Without these three rows both faithfulness measures would be INERT.
    has("GIT_CONFIG_KEY_٣=core.sshCommand git push");
    assert!(
        !denies("GIT_CONFIG_KEY_٣=core.sshCommand git push"),
        "a Unicode digit makes this not an assignment to the SHELL",
    );
    for cmd in [
        "env GIT_CONFIG_KEY_٣=core.sshCommand git push",
        "env GıT_CONFIG_KEY_0=core.sshCommand git push",
        "env GİT_CONFIG_KEY_0=core.sshCommand git push",
        "sudo GIT_CONFIG_KEY_٣=core.sshCommand git push",
    ] {
        has(cmd);
        assert!(
            denies(cmd),
            "{cmd}: a wrapper's assignment rule is not the shell's"
        );
    }

    // Reading the override stays allowed; every write shape does not.
    for cmd in [
        "git config --get core.sshCommand",
        "git config core.sshCommand",
    ] {
        has(cmd);
        assert!(
            !denies(cmd),
            "{cmd}: golden rule 0 is about transport, not looking"
        );
    }
    for cmd in [
        "git config core.sshCommand 'ssh -i /tmp/k'",
        "git config core.sshCommand=x",
        "git config --add core.sshCommand",
        "git -C /repo config core.sshCommand x",
    ] {
        has(cmd);
        assert!(denies(cmd), "{cmd}: this PERSISTS the override");
    }

    // Signing is read by SUBCOMMAND and never by positional membership.
    has("git log -S commit");
    assert!(
        !denies("git log -S commit"),
        "the pickaxe is a content search"
    );
    has("git commit -s -m x");
    assert!(!denies("git commit -s -m x"), "`-s` on commit is a trailer");
    has("git tag -s v1");
    assert!(denies("git tag -s v1"), "`-s` on tag is a GPG signature");

    // ---- A4 -------------------------------------------------------------------------------
    let floor = |cmd: &str| floorguard::release_floor_reason(cmd, true).is_some();
    has("git tag v1.2.3");
    assert!(floor("git tag v1.2.3"));
    has("git tag -l");
    assert!(!floor("git tag -l"));
    // A harmless tag flag clears the WHOLE command line, not its segment — the Python `return`s
    // out of its loop, and a port that narrowed that to a `continue` would deny where it allows.
    // **Which SEGMENT it stands in decides**, because the walk is left to right and the first
    // arm to answer wins: the same two commands the other way round are refused. Both rows are
    // recorded, so the pair is what pins the control flow rather than one of them.
    has("git tag -l && gh release create v1");
    assert!(
        !floor("git tag -l && gh release create v1"),
        "the tag arm returns for the command line, not for its segment",
    );
    has("gh release create v1 && git tag -l");
    assert!(
        floor("gh release create v1 && git tag -l"),
        "…and a publish that stands FIRST has already answered",
    );
    // charter's own landing verb, in every spelling `is_charter` knows — and `gh pr create`,
    // which is deliberately absent from the table.
    for cmd in [
        "charter change land",
        "edm change land",
        "python3 -m charter change land",
    ] {
        has(cmd);
        assert!(floor(cmd), "{cmd}: landing is merging");
    }
    has("gh pr create --title x");
    assert!(
        !floor("gh pr create --title x"),
        "opening a request is not publishing"
    );

    // ---- the quoting walk, A5 and A6 --------------------------------------------------------
    let forge_denies = |cmd: &str| proseguard::forge_substitution_hit(cmd).is_some();
    let charter_denies = |cmd: &str| proseguard::charter_substitution_hit(cmd).is_some();

    // #703 itself, and the shape the working rule PRESCRIBES — the calibration that decides
    // whether this guard survives its first day.
    has("gh issue create --body \"a `env -u PYTHONSAFEPATH` b\"");
    assert!(forge_denies(
        "gh issue create --body \"a `env -u PYTHONSAFEPATH` b\""
    ));
    has("gh pr create --body-file - <<'BODY'\na `env` b\nBODY\n");
    assert!(
        !forge_denies("gh pr create --body-file - <<'BODY'\na `env` b\nBODY\n"),
        "a QUOTED heredoc is the path the rule steers agents onto",
    );
    has("gh pr create --body-file - <<BODY\na `env` b\nBODY\n");
    assert!(
        forge_denies("gh pr create --body-file - <<BODY\na `env` b\nBODY\n"),
        "an UNQUOTED heredoc expands exactly the same way",
    );
    // An empty QUOTED delimiter is a real heredoc bash does not expand — the false REFUSAL the
    // Python's deletion sweep found.
    has("gh issue create --body-file - <<\"\"\na `env` b\n\n");
    assert!(!forge_denies(
        "gh issue create --body-file - <<\"\"\na `env` b\n\n"
    ));
    // A bare `$VAR` is not `$'`, and a plain `<` is not `<<`: both are fail-OPEN halves that the
    // sweep found and that the second half of each condition is what closes.
    has("gh issue create --body \"$VAR 'q' `env`\"");
    assert!(forge_denies("gh issue create --body \"$VAR 'q' `env`\""));
    has("gh issue create --body-file - < 'notes.md' && echo `env`");
    assert!(forge_denies(
        "gh issue create --body-file - < 'notes.md' && echo `env`"
    ));
    // A here-STRING is an ordinary double-quoted word, not a heredoc.
    has("gh issue create --body-file - <<<\"a `env` b\"");
    assert!(forge_denies(
        "gh issue create --body-file - <<<\"a `env` b\""
    ));

    // The flag filter that would have JOINED two flag values into a pair is absent.
    has("gh issue list --label issue --state create --body \"a `env` b\"");
    assert!(!forge_denies(
        "gh issue list --label issue --state create --body \"a `env` b\""
    ));
    // …while a global flag's value in FRONT of the noun still pairs correctly.
    has("gh --repo o/r issue create --body \"a `env` b\"");
    assert!(forge_denies(
        "gh --repo o/r issue create --body \"a `env` b\""
    ));

    // A6: the #778 memory itself, and the reader that takes the FIRST two words.
    has("charter persona remember \"appending to `pending` each pass\"");
    assert!(charter_denies(
        "charter persona remember \"appending to `pending` each pass\""
    ));
    has("charter recall --scope persona remember \"a `env` b\"");
    assert!(
        !charter_denies("charter recall --scope persona remember \"a `env` b\""),
        "a search whose words are merely adjacent is not a memory",
    );
    // One operand must not reach `words[1]` — the IndexError the sweep found, which in a hook is
    // a broken turn rather than a verdict.
    has("charter \"a `env` b\"");
    assert!(!charter_denies("charter \"a `env` b\""));
    // `-mcharter` is the NAMED fail-open hole, and `edm` is `is_charter`'s and not this table's.
    has("python3 -mcharter persona remember \"a `env` b\"");
    assert!(!charter_denies(
        "python3 -mcharter persona remember \"a `env` b\""
    ));
    has("edm persona remember \"a `env` b\"");
    assert!(!charter_denies("edm persona remember \"a `env` b\""));
    // charter's commit-message commands stay OUT, measured the other way (#711).
    has("charter save -m \"a `env` b\"");
    assert!(!charter_denies("charter save -m \"a `env` b\""));
    has("git commit -m \"$(cat <<'EOF'\na `env` b\nEOF\n)\"");
    assert!(!forge_denies(
        "git commit -m \"$(cat <<'EOF'\na `env` b\nEOF\n)\""
    ));

    // The remedy is PER ROW: only `report bug|gap` has a file input, and offering `--from-file`
    // on `persona remember` would answer a refusal with a usage error.
    let said = rows
        .iter()
        .filter_map(|r| r["csh"].as_array())
        .filter_map(|a| a[1].as_str())
        .collect::<Vec<_>>();
    assert!(
        said.iter()
            .any(|s| s.contains("`--from-file <path>` (or `--stdin`)")),
        "no recorded A6 refusal offers the file input, so the per-row remedy is uncompared",
    );
    assert!(
        said.iter().any(|s| !s.contains("--from-file")),
        "every recorded A6 refusal offers a file input, so the None row is uncompared",
    );
}
