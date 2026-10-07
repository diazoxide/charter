//! The profile guard: a nightly build of `main` with the stable profile, holding no secret
//! and publishing nowhere (ADR 0092, #1429).
//!
//! The dev channel builds with `dev-release`, so a green dev publish no longer shows that the
//! `release` profile still builds. The guard does, every night, on the path a build started by
//! hand takes: outside the `release` environment, where no signing key exists. A red night
//! keeps one issue open.
//!
//! Which profile and bundles the schedule gets is held in
//! `a_dev_channel_build_is_lighter_and_a_stable_build_is_not.rs`. This file holds what the
//! guard may reach.

mod workflow;

use workflow::{Job, job, release_jobs, run_lines, step_uses};

const GUARD: &str = "profile-guard";

fn release_yml() -> String {
    workflow::read(&workflow::workflows_dir().join("release.yml"))
}

#[test]
fn the_release_workflow_runs_every_night() {
    let text = release_yml();
    let schedule: Vec<&str> = text
        .lines()
        .skip_while(|l| *l != "  schedule:")
        .skip(1)
        .take_while(|l| l.starts_with("    "))
        .map(str::trim)
        .collect();
    assert_eq!(schedule.len(), 1, "one schedule: {schedule:?}");
    let cron = schedule[0]
        .strip_prefix("- cron: \"")
        .and_then(|c| c.strip_suffix('"'))
        .unwrap_or_else(|| panic!("not a cron line: {}", schedule[0]));
    let fields: Vec<&str> = cron.split(' ').collect();
    // Minute and hour fixed, every day: one run a night.
    assert_eq!(fields.len(), 5, "{cron}");
    assert!(
        fields[..2].iter().all(|f| f.parse::<u8>().is_ok()),
        "{cron}"
    );
    assert_eq!(fields[2..], ["*", "*", "*"], "{cron}");
}

#[test]
fn a_scheduled_run_holds_no_key_and_publishes_nowhere() {
    let jobs = release_jobs();

    // `plan` names the events that get the `release` environment, and the schedule is not one
    // of them. The secrets live in that environment and nowhere else (#344), so every
    // `secrets.*` a scheduled `plan` reads is empty, which is what makes its build unsigned.
    assert_eq!(
        job(&jobs, "plan").environment(),
        Some(
            "${{ (github.event_name == 'push' || github.event_name == 'workflow_run') && 'release' || '' }}"
        )
    );
    // `build` takes the environment only when `plan` said the run publishes, and `plan` says
    // a scheduled run does not (the test beside this one holds that arm).
    assert_eq!(
        job(&jobs, "build").environment(),
        Some("${{ needs.plan.outputs.publish == 'true' && 'release' || '' }}")
    );
    // The two jobs that sign provenance and write the release run only for a publish.
    for name in ["provenance", "publish"] {
        assert_eq!(
            job(&jobs, name).condition(),
            Some("needs.plan.outputs.publish == 'true'"),
            "job `{name}`"
        );
    }
}

#[test]
fn the_guards_own_job_holds_issues_write_and_nothing_else() {
    let jobs = release_jobs();
    let guard: &Job = job(&jobs, GUARD);

    assert!(!guard.reads_a_secret(), "{GUARD} reads a secret");
    assert!(!guard.publishes(), "{GUARD} publishes");
    assert_eq!(guard.environment(), None);
    assert!(!guard.can_mint_an_oidc_token());
    assert_eq!(
        guard.permissions(),
        Some(vec![("issues".to_owned(), "write".to_owned())])
    );

    // It only reads how the night's jobs ended, and only on the schedule: a build started by
    // hand, a dev publish and a stable release never touch the issue.
    assert_eq!(
        guard.condition(),
        Some("always() && github.event_name == 'schedule'")
    );
    assert_eq!(guard.needs(), ["plan", "build", "sbom"]);

    // No action at all: so no checkout, no cache restored or saved, and nothing third-party
    // beside `issues: write`.
    let steps = guard.steps();
    let uses: Vec<&str> = steps.iter().filter_map(|s| step_uses(s)).collect();
    assert!(uses.is_empty(), "{uses:?}");
}

#[test]
fn a_red_night_opens_one_issue_or_rewrites_it_and_a_clean_one_closes_it() {
    let jobs = release_jobs();
    let guard = job(&jobs, GUARD);
    let steps = guard.steps();
    assert_eq!(steps.len(), 1, "{steps:#?}");
    let lines = run_lines(&steps[0]);
    let count = |want: &str| lines.iter().filter(|l| l.contains(want)).count();

    // One title, matched exactly against every open issue, so there is never a second issue.
    assert_eq!(
        count("TITLE='the stable profile does not build on main'"),
        1
    );
    assert_eq!(count("--paginate"), 1, "every page of open issues");
    assert_eq!(count(r#"if ($0 == t) {print n; exit}"#), 1);
    assert_eq!(
        count(r#"gh issue create --title "$TITLE" --body-file "$BODY_FILE""#),
        1
    );
    assert_eq!(
        count(r#"gh issue edit "$EXISTING" --body-file "$BODY_FILE""#),
        1
    );
    assert_eq!(count(r#"gh issue close "$EXISTING""#), 1);
    // Never a comment on a red night: a comment a night is a notification a night.
    assert_eq!(count("gh issue comment"), 0);
}

#[test]
fn the_guard_does_not_wait_behind_a_publish_or_hold_one_up() {
    let text = release_yml();
    let group = text
        .lines()
        .find_map(|l| l.strip_prefix("  group: "))
        .expect("release.yml has a concurrency group");
    assert!(
        group.starts_with("release-${{ github.event_name == 'schedule' && 'profile-guard' || "),
        "{group}"
    );
}
