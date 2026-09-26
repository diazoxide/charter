//! Every job in `release.yml` that reads a signing secret or publishes a release runs in the
//! `release` environment whenever the run publishes (#344).
//!
//! The environment is where the secrets live, and its deployment policy is what keeps them off
//! any ref but `main` and `v*` tags. A job that reads `secrets.*` without naming it gets an
//! empty string: `plan` would read "no updater key" and skip every dev build, and `build` would
//! bundle unsigned. So the rule is checked on the file, the way it is written, rather than
//! found out on the next release.
//!
//! A line reader rather than a YAML parser: the workflow's jobs are the two-space keys under
//! `jobs:` and their settings the four-space keys below them, which is all this needs, and no
//! YAML crate is in the workspace for one test.

use std::path::PathBuf;

/// One job under `jobs:`: its key, and the lines of its body with comments dropped.
struct Job {
    name: String,
    body: Vec<String>,
}

impl Job {
    fn reads_a_secret(&self) -> bool {
        self.body.iter().any(|l| l.contains("secrets."))
    }

    fn publishes(&self) -> bool {
        self.body.iter().any(|l| {
            l.contains("gh release create")
                || l.contains("gh release edit")
                || l.contains("replace-release-assets.sh")
        })
    }

    /// The job's own `environment:` value, if it has one.
    fn environment(&self) -> Option<&str> {
        self.body
            .iter()
            .find_map(|l| l.strip_prefix("    environment:"))
            .map(str::trim)
    }
}

fn release_jobs() -> Vec<Job> {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.github/workflows/release.yml");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    let mut jobs: Vec<Job> = Vec::new();
    let mut in_jobs = false;
    for line in text.lines() {
        if line == "jobs:" {
            in_jobs = true;
            continue;
        }
        if !in_jobs || line.trim_start().starts_with('#') {
            continue;
        }
        let is_job =
            line.starts_with("  ") && !line.starts_with("   ") && line.trim_end().ends_with(':');
        if is_job {
            jobs.push(Job {
                name: line.trim().trim_end_matches(':').to_owned(),
                body: Vec::new(),
            });
        } else if let Some(job) = jobs.last_mut() {
            job.body.push(line.to_owned());
        }
    }
    jobs
}

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
