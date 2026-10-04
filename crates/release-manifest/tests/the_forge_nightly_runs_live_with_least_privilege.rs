//! The live forge nightly (FG-4, #712) runs the forge contract against a real GitHub repo and a
//! real GitLab repo, with tokens the operator provisioned, and grants nothing else.
//!
//! It is evidence, never a gate (V70): nightly and by hand, and nothing else can start it, so a
//! fork's or a pull request's code never runs where a token is. Each forge's job runs in that
//! forge's own GitHub environment (`forge-live-github`, `forge-live-gitlab`), which holds one
//! secret under one static name, so the runner is sent that one token and no other secret (a
//! computed `secrets[…]` index sends the whole secrets context). The environments allow only
//! `main` to deploy, so a dispatch from another branch never runs changed code with the token.
//! The token's value is handed only to the step that runs the tests, after the build, which
//! keeps it out of the build scripts' environment; it does not keep it from code that reads the
//! runner's memory, which is why the environment and its branch policy are the boundary. The
//! job restores no cache, as the release jobs that hold a key do not (ADR 0042). Unprovisioned, a
//! job says so and stops, green. The job that keeps the nightly's issue holds no secret.

mod workflow;

use workflow::{Job, job_named, jobs, read, run_lines, step_uses, step_value, workflows_dir};

const FILE: &str = "forge-live.yml";

/// The one secret the workflow may name, and how.
const TOKEN: &str = "secrets.FORGE_LIVE_TOKEN";

fn nightly() -> (String, Vec<Job>) {
    let text = read(&workflows_dir().join(FILE));
    let jobs = jobs(&text);
    (text, jobs)
}

fn pairs(list: &[(&str, &str)]) -> Vec<(String, String)> {
    list.iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect()
}

/// Every reference to the secrets context on `line`, in either form: `secrets.NAME`, or a
/// `secrets[…]` index, with or without spaces.
fn secret_refs(line: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut rest = line;
    while let Some(at) = rest.find("secrets") {
        let before = rest[..at].chars().last();
        let after = rest[at + "secrets".len()..].trim_start();
        let word = before.is_none_or(|c| !(c.is_ascii_alphanumeric() || c == '_' || c == '.'));
        if word && (after.starts_with('.') || after.starts_with('[')) {
            let end = after
                .find(|c: char| !(c.is_ascii_alphanumeric() || "._[]'\"".contains(c)))
                .unwrap_or(after.len());
            found.push(format!("secrets{}", &after[..end]));
        }
        rest = &rest[at + "secrets".len()..];
    }
    found
}

/// Every use of the secrets context inside a `${{ … }}` expression on `line` that is not exactly
/// [`TOKEN`]: a bare `secrets` (`toJSON(secrets)`, `fromJSON(format(..., secrets))`) sends the
/// whole context to the runner, and so does any other name or index.
fn other_secret_uses(line: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut rest = line;
    while let Some(open) = rest.find("${{") {
        let inner = &rest[open + 3..];
        let close = inner.find("}}").unwrap_or(inner.len());
        let expr = &inner[..close];
        let mut at = 0;
        while let Some(i) = expr[at..].find("secrets") {
            let start = at + i;
            let end = start + "secrets".len();
            let before = expr[..start].chars().last();
            let after = expr[end..].chars().next();
            let word = |c: Option<char>| c.is_some_and(|c| c.is_ascii_alphanumeric() || c == '_');
            if !word(before) && before != Some('.') && !word(after) {
                let tail = &expr[start..];
                let exact = tail.strip_prefix(TOKEN).is_some_and(|t| {
                    !t.starts_with(|c: char| c.is_ascii_alphanumeric() || c == '_')
                });
                if !exact {
                    found.push(expr.trim().to_owned());
                }
            }
            at = end;
        }
        rest = &inner[close..];
    }
    found
}

/// The lines that reference a secret, comments dropped.
fn secret_lines<'a>(lines: impl Iterator<Item = &'a String>) -> Vec<String> {
    lines
        .filter(|l| !l.trim_start().starts_with('#'))
        .filter(|l| !secret_refs(l).is_empty())
        .map(|l| l.trim().to_owned())
        .collect()
}

/// The workflow's `on:` events: the keys directly under `on:`.
fn events(text: &str) -> Vec<String> {
    text.lines()
        .skip_while(|l| *l != "on:")
        .skip(1)
        .take_while(|l| l.is_empty() || l.starts_with(' ') || l.starts_with('#'))
        .filter(|l| l.starts_with("  ") && !l.starts_with("   ") && !l.trim().starts_with('#'))
        .map(|l| l.trim().trim_end_matches(':').to_owned())
        .collect()
}

/// The lines of the job's own `env:` block, if it has one.
fn job_env(job: &Job) -> Vec<String> {
    let Some(start) = job.body.iter().position(|l| l.trim_end() == "    env:") else {
        return Vec::new();
    };
    job.body[start + 1..]
        .iter()
        .take_while(|l| l.trim().is_empty() || l.starts_with("      "))
        .cloned()
        .collect()
}

#[test]
fn the_secret_reader_sees_both_forms() {
    assert_eq!(secret_refs("x: ${{ secrets.A }}"), ["secrets.A"]);
    assert_eq!(
        secret_refs("x: ${{ secrets[matrix.token] }}"),
        ["secrets[matrix.token]"]
    );
    assert_eq!(secret_refs("x: ${{ secrets ['B'] }}"), ["secrets['B']"]);
    assert!(secret_refs("x: ${{ github.token }} # the secrets we hold").is_empty());
    assert!(secret_refs("mysecrets.x").is_empty());

    assert_eq!(
        other_secret_uses("x: ${{ secrets.FORGE_LIVE_TOKEN }}"),
        Vec::<String>::new()
    );
    assert_eq!(
        other_secret_uses("x: ${{ toJSON(secrets) }}"),
        ["toJSON(secrets)"]
    );
    assert_eq!(
        other_secret_uses("x: ${{ secrets.FORGE_LIVE_TOKEN_2 }}"),
        ["secrets.FORGE_LIVE_TOKEN_2"]
    );
    assert_eq!(
        other_secret_uses("x: ${{ secrets[format('A{0}', 1)] }}"),
        ["secrets[format('A{0}', 1)]"]
    );
    assert!(other_secret_uses("x: ${{ github.token }} secrets outside").is_empty());
}

#[test]
fn it_runs_nightly_and_by_hand_and_on_nothing_else() {
    let (text, _) = nightly();
    assert_eq!(events(&text), ["schedule", "workflow_dispatch"]);
    let cron = text
        .lines()
        .find_map(|l| l.trim().strip_prefix("- cron:"))
        .map(|c| c.trim().trim_matches('"').to_owned())
        .expect("a schedule");
    let fields: Vec<&str> = cron.split_whitespace().collect();
    assert_eq!(fields[2..], ["*", "*", "*"], "every night: {cron}");
}

#[test]
fn the_workflow_grants_nothing_and_each_job_only_what_it_needs() {
    let (text, jobs) = nightly();
    assert!(
        text.lines().any(|l| l == "permissions: {}"),
        "the workflow grants nothing by default; each job names what it needs"
    );
    let names: Vec<&str> = jobs.iter().map(|j| j.name.as_str()).collect();
    assert_eq!(names, ["live", "notice"]);
    let live = job_named(&jobs, "live", FILE);
    assert_eq!(live.permissions(), Some(pairs(&[("contents", "read")])));
    let notice = job_named(&jobs, "notice", FILE);
    assert_eq!(notice.permissions(), Some(pairs(&[("issues", "write")])));
}

#[test]
fn only_the_live_job_names_a_secret_and_only_its_own_environments_token() {
    let (text, jobs) = nightly();
    // Every reference anywhere in the file, the workflow's own `env:` included.
    let all = secret_lines(text.lines().map(str::to_owned).collect::<Vec<_>>().iter());
    for line in &all {
        assert_eq!(
            secret_refs(line),
            [TOKEN],
            "a secret other than the environment's one token, or a computed index: {line}"
        );
    }
    // Any other use of the context inside an expression, however it is written.
    for line in text.lines().filter(|l| !l.trim_start().starts_with('#')) {
        assert_eq!(
            other_secret_uses(line),
            Vec::<String>::new(),
            "the secrets context used other than as {TOKEN}: {line}"
        );
    }
    let notice = job_named(&jobs, "notice", FILE);
    assert_eq!(
        secret_lines(notice.body.iter()),
        Vec::<String>::new(),
        "the job that writes the issue holds no secret"
    );
    let live = job_named(&jobs, "live", FILE);
    assert_eq!(
        live.environment(),
        Some("forge-live-${{ matrix.forge }}"),
        "each forge's job runs in that forge's environment"
    );
    assert_eq!(
        secret_lines(job_env(live).iter()),
        Vec::<String>::new(),
        "a job-level env hands the token to every step"
    );
}

#[test]
fn the_token_reaches_only_the_test_step_after_the_build() {
    let (_, jobs) = nightly();
    let live = job_named(&jobs, "live", FILE);
    let steps = live.steps();
    let handed: Vec<usize> = (0..steps.len())
        .filter(|&i| !secret_lines(steps[i].iter()).is_empty())
        .collect();
    assert_eq!(handed.len(), 1, "the token reaches one step: {handed:?}");
    let run = &steps[handed[0]];
    assert_eq!(
        step_value(run, "env", "TOKEN"),
        Some("${{ secrets.FORGE_LIVE_TOKEN }}")
    );
    let commands = run_lines(run).join("\n");
    assert!(
        commands.contains("--ignored") && commands.contains("_live::"),
        "the step handed the token runs the live cases: {commands}"
    );
    let build = steps
        .iter()
        .position(|s| run_lines(s).join(" ").contains("--no-run"))
        .expect("a build step");
    assert!(
        build < handed[0],
        "the build runs before the token is handed over"
    );
    for step in &steps {
        let action = step_uses(step).unwrap_or_default();
        assert!(
            ![
                "Swatinem/rust-cache@",
                "actions/cache",
                "actions/setup-node@"
            ]
            .iter()
            .any(|a| action.starts_with(a)),
            "the job holding a token restores no cache: {action}"
        );
        if action.starts_with("actions/checkout@") {
            assert_eq!(
                step_value(step, "with", "persist-credentials"),
                Some("false")
            );
        }
    }
}

#[test]
fn an_unprovisioned_forge_says_so_and_runs_nothing() {
    let (_, jobs) = nightly();
    let live = job_named(&jobs, "live", FILE);
    let steps = live.steps();
    let first = &steps[0];
    assert!(
        first.iter().any(|l| l.trim() == "id: provisioned"),
        "the first step asks whether the fixture is provisioned: {first:#?}"
    );
    let said = run_lines(first).join("\n");
    assert!(
        said.contains("GITHUB_STEP_SUMMARY") && said.contains("::notice"),
        "an unprovisioned run says what is missing where it is read: {said}"
    );
    // Everything that checks out, builds or runs code is gated; only the record of what was
    // tested, which the notice job reads, is kept either way.
    for step in &steps[1..] {
        let gated = step
            .iter()
            .any(|l| l.trim() == "if: steps.provisioned.outputs.ready == 'true'");
        let record = step_uses(step).is_some_and(|u| u.starts_with("actions/upload-artifact@"));
        assert!(
            gated || record,
            "a step runs on an unprovisioned forge: {step:#?}"
        );
        // The record runs ungated, so it uploads the one folder that holds it and nothing of
        // the workspace.
        if record {
            assert_eq!(
                step_value(step, "with", "path"),
                Some("${{ runner.temp }}/outcome"),
                "the record uploads only what was tested: {step:#?}"
            );
        }
    }
    assert!(
        steps
            .iter()
            .any(|s| step_uses(s).is_some_and(|u| u.starts_with("actions/upload-artifact@"))),
        "the record of what was tested, which the notice job reads"
    );
}
