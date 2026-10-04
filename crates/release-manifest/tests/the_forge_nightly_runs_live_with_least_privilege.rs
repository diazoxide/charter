//! The live forge nightly (FG-4, #712) runs the forge contract against a real GitHub repo and a
//! real GitLab repo, with tokens the operator provisioned, and grants nothing else.
//!
//! It is evidence, never a gate (V70): nightly and by hand, never on a pull request, where a
//! fork's code could reach the tokens. Each forge's job holds exactly one token, its own, and
//! hands it only to the step that runs the tests, after the build: a dependency's build script
//! never sees it. That job restores no cache, as the release jobs that hold a key do not (ADR
//! 0042). Unprovisioned, a job says so and stops, green. The workflow grants nothing by default;
//! the one job that may write, the issue that a red night keeps open, holds no token.

mod workflow;

use workflow::{Job, job_named, jobs, read, run_lines, step_uses, step_value, workflows_dir};

const FILE: &str = "forge-live.yml";

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

/// The workflow's `on:` block, comments dropped, as its trimmed lines.
fn triggers(text: &str) -> Vec<String> {
    text.lines()
        .skip_while(|l| *l != "on:")
        .skip(1)
        .take_while(|l| l.is_empty() || l.starts_with(' ') || l.starts_with('#'))
        .filter(|l| !l.trim().is_empty() && !l.trim_start().starts_with('#'))
        .map(|l| l.trim().to_owned())
        .collect()
}

#[test]
fn it_runs_nightly_and_by_hand_and_never_on_a_pull_request_or_a_push() {
    let (text, _) = nightly();
    let on = triggers(&text);
    let cron = on
        .iter()
        .find_map(|l| l.strip_prefix("- cron:"))
        .map(|c| c.trim().trim_matches('"').to_owned())
        .unwrap_or_else(|| panic!("no schedule in {on:?}"));
    let fields: Vec<&str> = cron.split_whitespace().collect();
    assert_eq!(fields[2..], ["*", "*", "*"], "every night: {cron}");
    assert!(on.iter().any(|l| l == "workflow_dispatch:"), "{on:?}");
    for event in [
        "pull_request",
        "pull_request_target",
        "push",
        "workflow_run",
    ] {
        assert!(
            !on.iter().any(|l| l.starts_with(&format!("{event}:"))),
            "the tokens are reachable from `{event}`: {on:?}"
        );
    }
}

#[test]
fn the_workflow_grants_nothing_and_the_live_job_only_reads_the_code() {
    let (text, jobs) = nightly();
    assert!(
        text.lines().any(|l| l == "permissions: {}"),
        "the workflow grants nothing by default; each job names what it needs"
    );
    let live = job_named(&jobs, "live", FILE);
    assert_eq!(live.permissions(), Some(pairs(&[("contents", "read")])));
    let notice = job_named(&jobs, "notice", FILE);
    assert_eq!(
        notice.permissions(),
        Some(pairs(&[("contents", "read"), ("issues", "write")]))
    );
    assert!(
        !notice.reads_a_secret(),
        "the job that writes holds no token"
    );
    let names: Vec<&str> = jobs.iter().map(|j| j.name.as_str()).collect();
    assert_eq!(names, ["live", "notice"]);
}

#[test]
fn each_forge_job_holds_its_own_token_and_only_the_test_step_is_handed_it() {
    let (_, jobs) = nightly();
    let live = job_named(&jobs, "live", FILE);
    // One row per forge, each naming its own secret and its own owner variable.
    for forge in ["GITHUB", "GITLAB"] {
        for line in [
            format!("token: FORGE_LIVE_{forge}_TOKEN"),
            format!("owner: FORGE_LIVE_{forge}_OWNER"),
        ] {
            assert!(
                live.body.iter().any(|l| l.trim() == line),
                "the matrix has no `{line}`"
            );
        }
    }
    let reading: Vec<String> = live
        .body
        .iter()
        .filter(|l| l.contains("secrets."))
        .cloned()
        .collect();
    assert!(
        reading.is_empty(),
        "a secret named outright rather than the row's own: {reading:#?}"
    );
    let steps = live.steps();
    let handed: Vec<&Vec<String>> = steps
        .iter()
        .filter(|s| s.iter().any(|l| l.contains("secrets[matrix.token]")))
        .collect();
    assert_eq!(
        handed.len(),
        2,
        "the token reaches the provisioning check and the test run, and nothing else"
    );
    let check = handed[0];
    assert_eq!(
        step_value(check, "env", "TOKEN"),
        Some("${{ secrets[matrix.token] != '' }}"),
        "the check sees whether the token is set, not its value"
    );
    let run = handed[1];
    let commands = run_lines(run).join("\n");
    assert!(
        commands.contains("--ignored") && commands.contains("_live::"),
        "the step handed the token runs the live cases: {commands}"
    );
    let build = steps
        .iter()
        .position(|s| run_lines(s).join(" ").contains("--no-run"))
        .expect("a build step before the token is handed over");
    let at = steps.iter().position(|s| std::ptr::eq(s, run)).unwrap();
    assert!(build < at, "the build runs before the token is handed over");
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
fn an_unprovisioned_forge_says_so_and_stops_green() {
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
    for step in &steps[1..] {
        assert!(
            step.iter()
                .any(|l| l.trim() == "if: steps.provisioned.outputs.ready == 'true'"),
            "a step runs on an unprovisioned forge: {step:#?}"
        );
    }
}
