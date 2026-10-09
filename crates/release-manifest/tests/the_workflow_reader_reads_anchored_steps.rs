//! The line reader the workflow tests share reads a job's steps when the job names them with a
//! YAML anchor (`steps: &app-steps`), as ci.yml's `app` job does. Before, it matched only the
//! bare `steps:` line, so every test that walks a job's steps saw none in that job and passed
//! over it (#1023).

mod workflow;

use workflow::{job_named, jobs, read, step_uses, workflows_dir};

const ANCHORED: &str = "\
jobs:
  first:
    runs-on: ubuntu-24.04
    steps: &shared-steps
      - uses: actions/checkout@0123456789abcdef0123456789abcdef01234567 # v1
      - name: Build
        run: cargo build
  second:
    runs-on: macos-latest
    steps: *shared-steps
";

#[test]
fn a_job_whose_steps_carry_an_anchor_has_those_steps() {
    let jobs = jobs(ANCHORED);
    let steps = job_named(&jobs, "first", "the fixture").steps();
    assert_eq!(steps.len(), 2, "{steps:#?}");
    assert_eq!(
        step_uses(&steps[0]),
        Some("actions/checkout@0123456789abcdef0123456789abcdef01234567")
    );
}

#[test]
fn a_job_that_reuses_anchored_steps_by_alias_has_none_of_its_own() {
    // The alias's steps are the anchor's, and they are read where the anchor is.
    let jobs = jobs(ANCHORED);
    assert!(job_named(&jobs, "second", "the fixture").steps().is_empty());
}

#[test]
fn the_ci_app_job_has_its_anchored_steps_read() {
    let ci = jobs(&read(&workflows_dir().join("ci.yml")));
    let steps = job_named(&ci, "app", "ci.yml").steps();
    assert!(
        steps
            .iter()
            .any(|s| step_uses(s).is_some_and(|u| u.starts_with("actions/checkout@"))),
        "ci.yml's `app` job reads as having no checkout step: {steps:#?}"
    );
}
