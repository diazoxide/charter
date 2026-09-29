//! Every job in `release.yml` that reads a signing secret or publishes a release runs in the
//! `release` environment whenever the run publishes (#344).
//!
//! The environment is where the secrets live, and its deployment policy is what keeps them off
//! any ref but `main` and `v*` tags. A job that reads `secrets.*` without naming it gets an
//! empty string: `plan` would read "no updater key" and skip every dev build, and `build` would
//! bundle unsigned. So the rule is checked on the file, the way it is written, rather than
//! found out on the next release.
//!
//! The workflow is read by `workflow/mod.rs`, which the pinning and cache tests share.

mod workflow;

use workflow::{Job, release_jobs};

/// What each guarded job's `environment:` says, exactly.
///
/// `plan` cannot read another job's output, so it keys on the event: every event but a
/// hand-started build can publish. `build` keys on what `plan` decided. `publish` only runs
/// when the run publishes, so it names the environment outright. And `x && 'release' || ''`,
/// never `x && '' || 'release'`: an empty string is falsy in a workflow expression, so the
/// second form is `release` every time.
const EXPECTED: [(&str, &str); 3] = [
    (
        "plan",
        "${{ github.event_name != 'workflow_dispatch' && 'release' || '' }}",
    ),
    (
        "build",
        "${{ needs.plan.outputs.publish == 'true' && 'release' || '' }}",
    ),
    ("publish", "release"),
];

#[test]
fn every_job_that_reads_a_signing_secret_or_publishes_runs_in_the_release_environment() {
    let jobs = release_jobs();
    let guarded: Vec<&Job> = jobs
        .iter()
        .filter(|job| job.reads_a_secret() || job.publishes())
        .collect();

    // Found by what they do, so a new job that reads a secret or publishes lands here and has
    // to be given its line in `EXPECTED`.
    let names: Vec<&str> = guarded.iter().map(|job| job.name.as_str()).collect();
    assert_eq!(names, EXPECTED.map(|(name, _)| name), "{names:?}");

    for (job, (_, expected)) in guarded.iter().zip(EXPECTED) {
        assert_eq!(
            job.environment(),
            Some(expected),
            "job `{}` reads a signing secret or publishes",
            job.name
        );
    }
}

#[test]
fn no_other_job_names_an_environment() {
    for job in release_jobs() {
        if EXPECTED.iter().all(|(name, _)| *name != job.name) {
            assert_eq!(job.environment(), None, "job `{}`", job.name);
        }
    }
}
