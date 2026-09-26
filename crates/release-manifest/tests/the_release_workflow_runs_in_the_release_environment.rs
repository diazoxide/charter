//! Every job in `release.yml` that reads a signing secret or publishes a release runs in the
//! `release` environment (#344).
//!
//! The environment is where the secrets live, and its deployment policy is what keeps them off
//! any ref but `main` and `v*` tags. A job that reads `secrets.*` without naming it gets an
//! empty string: `plan` would read "no updater key" and skip every dev build, and `build` would
//! bundle unsigned. A job that names it where it should not would be refused on a branch the
//! policy does not admit. So the rule is checked on the file, the way it is written, rather
//! than found out on the next release.
//!
//! A line reader rather than a YAML parser: the workflow's jobs are the two-space keys under
//! `jobs:`, which is all this needs, and no YAML crate is in the workspace for one test.

use std::path::PathBuf;

fn workflow() -> String {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.github/workflows/release.yml");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

/// Each job under `jobs:`, as its name and the lines of its body, comments dropped.
fn jobs(text: &str) -> Vec<(String, Vec<String>)> {
    let mut out: Vec<(String, Vec<String>)> = Vec::new();
    let mut in_jobs = false;
    for line in text.lines() {
        if line == "jobs:" {
            in_jobs = true;
            continue;
        }
        if !in_jobs || line.trim_start().starts_with('#') {
            continue;
        }
        let is_job = line.len() > 2
            && line.starts_with("  ")
            && !line.starts_with("   ")
            && line.trim_end().ends_with(':');
        if is_job {
            out.push((line.trim().trim_end_matches(':').to_owned(), Vec::new()));
        } else if let Some((_, body)) = out.last_mut() {
            body.push(line.to_owned());
        }
    }
    out
}

fn reads_a_secret(body: &[String]) -> bool {
    body.iter().any(|l| l.contains("secrets."))
}

fn publishes(body: &[String]) -> bool {
    body.iter().any(|l| {
        l.contains("gh release create")
            || l.contains("gh release edit")
            || l.contains("replace-release-assets.sh")
    })
}

/// The job's own `environment:` value, if it has one.
fn environment(body: &[String]) -> Option<String> {
    body.iter()
        .find_map(|l| l.strip_prefix("    environment:"))
        .map(|v| v.trim().to_owned())
}

#[test]
fn every_job_that_reads_a_signing_secret_or_publishes_names_the_release_environment() {
    let text = workflow();
    let jobs = jobs(&text);
    let guarded: Vec<&(String, Vec<String>)> = jobs
        .iter()
        .filter(|(_, body)| reads_a_secret(body) || publishes(body))
        .collect();

    // Not vacuous: the three jobs this is about are found, by what they do.
    let names: Vec<&str> = guarded.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, ["plan", "build", "publish"], "{names:?}");

    for (name, body) in guarded {
        let env = environment(body).unwrap_or_else(|| {
            panic!("job `{name}` reads a signing secret or publishes, and names no environment")
        });
        assert!(
            env.contains("'release'") || env == "release",
            "job `{name}` runs in `{env}`, not the `release` environment"
        );
    }
}

#[test]
fn the_publish_job_always_runs_in_the_release_environment() {
    let text = workflow();
    let jobs = jobs(&text);
    let (_, body) = jobs
        .iter()
        .find(|(n, _)| n == "publish")
        .expect("a publish job");
    assert_eq!(environment(body).as_deref(), Some("release"));
}

/// `x && '' || 'release'` is `release` every time, because an empty string is falsy in a
/// workflow expression. A conditional environment has to be written the other way round.
#[test]
fn no_conditional_environment_is_written_the_way_that_always_picks_release() {
    let text = workflow();
    for (name, body) in jobs(&text) {
        if let Some(env) = environment(&body) {
            assert!(
                !env.replace(' ', "").contains("&&''||"),
                "job `{name}`'s environment `{env}` always evaluates to its fallback"
            );
        }
    }
}
