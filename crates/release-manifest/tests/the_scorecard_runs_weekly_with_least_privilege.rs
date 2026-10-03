//! The OpenSSF Scorecard grades this repository's supply-chain practice once a week (#585).
//!
//! It is evidence, never a gate (V70): it runs on a weekly `schedule` and by hand, never on a
//! pull request, and no branch protection names it. It reads the repository and writes one
//! thing, its SARIF report into the repository's code scanning, so its job's token can read and
//! can write security events and nothing else, and the workflow grants nothing by default. Its
//! score stays in the repository, not on the
//! public OpenSSF API: the first releases are quiet (V38), so `publish_results` is false and
//! the job mints no OIDC token.
//!
//! The workflow is read by `workflow/mod.rs`, which the other workflow tests share.

mod workflow;

use workflow::{Job, job_named, jobs, read, step_uses, step_value, workflows_dir};

fn scorecard() -> (String, Vec<Job>) {
    let text = read(&workflows_dir().join("scorecard.yml"));
    let jobs = jobs(&text);
    (text, jobs)
}

/// The workflow's `on:` block, comments dropped, as its lines.
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
fn the_scorecard_runs_weekly_and_never_on_a_pull_request() {
    let (text, _) = scorecard();
    let on = triggers(&text);
    let cron = on
        .iter()
        .find_map(|l| l.strip_prefix("- cron:"))
        .map(|c| c.trim().trim_matches('"').to_owned())
        .unwrap_or_else(|| panic!("no schedule in {on:?}"));
    let fields: Vec<&str> = cron.split_whitespace().collect();
    assert_eq!(fields.len(), 5, "{cron}");
    assert!(
        fields[2] == "*" && fields[3] == "*" && fields[4].chars().all(|c| c.is_ascii_digit()),
        "once a week, on one weekday: {cron}"
    );
    for event in ["pull_request", "pull_request_target", "push"] {
        assert!(
            !on.iter().any(|l| l.starts_with(&format!("{event}:"))),
            "evidence runs weekly and by hand only (V70), not on `{event}`: {on:?}"
        );
    }
}

#[test]
fn the_scorecard_job_can_read_and_write_security_events_and_nothing_more() {
    let (text, jobs) = scorecard();
    assert!(
        text.lines().any(|l| l == "permissions: {}"),
        "the workflow grants nothing by default; the job names what it needs"
    );
    let analysis = job_named(&jobs, "analysis", "scorecard.yml");
    assert_eq!(
        analysis.permissions(),
        Some(
            [
                ("actions", "read"),
                ("contents", "read"),
                ("security-events", "write"),
            ]
            .map(|(k, v)| (k.to_owned(), v.to_owned()))
            .to_vec()
        ),
        "no `id-token: write` while results stay unpublished (V38)"
    );
    assert!(!analysis.reads_a_secret(), "the default token is enough");
}

#[test]
fn the_score_goes_to_code_scanning_and_not_to_the_public_api() {
    let (_, jobs) = scorecard();
    let analysis = job_named(&jobs, "analysis", "scorecard.yml");
    let steps = analysis.steps();
    let by = |action: &str| {
        steps
            .iter()
            .find(|s| step_uses(s).is_some_and(|u| u.starts_with(&format!("{action}@"))))
            .unwrap_or_else(|| panic!("no `{action}` step"))
            .clone()
    };

    let checkout = by("actions/checkout");
    assert_eq!(
        step_value(&checkout, "with", "persist-credentials"),
        Some("false")
    );

    let score = by("ossf/scorecard-action");
    assert_eq!(step_value(&score, "with", "publish_results"), Some("false"));
    assert_eq!(step_value(&score, "with", "results_format"), Some("sarif"));
    let file = step_value(&score, "with", "results_file").expect("a results file");

    let upload = by("github/codeql-action/upload-sarif");
    assert_eq!(step_value(&upload, "with", "sarif_file"), Some(file));
}
