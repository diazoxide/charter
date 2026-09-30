//! Every build `release.yml` publishes carries a signed SLSA build provenance attestation, made
//! before anything is published (#583).
//!
//! The attestation is what `gh attestation verify` checks: that a file came out of this
//! repository's `release.yml`, at a named commit. Signing it takes an OIDC token
//! (`id-token: write`) and storing it takes `attestations: write`. Those two are a signing
//! identity, so exactly one job holds them, it holds nothing else, and no job that holds a
//! release key or can write the release gets them too.
//!
//! The workflow is read by `workflow/mod.rs`, which the pinning and environment tests share.

mod workflow;

use workflow::{Job, release_jobs, step_uses};

fn job<'a>(jobs: &'a [Job], name: &str) -> &'a Job {
    jobs.iter()
        .find(|j| j.name == name)
        .unwrap_or_else(|| panic!("release.yml has no `{name}` job"))
}

#[test]
fn the_provenance_job_attests_what_the_build_made_whenever_the_run_publishes() {
    let jobs = release_jobs();
    let provenance = job(&jobs, "provenance");

    assert!(
        provenance.needs().contains(&"build".to_owned()),
        "provenance needs {:?}; it attests the build's output, so it needs `build`",
        provenance.needs()
    );
    assert_eq!(
        provenance.condition(),
        Some("needs.plan.outputs.publish == 'true'"),
        "a published build is attested, and one published nowhere is not"
    );

    let steps = provenance.steps();
    let attest = steps
        .iter()
        .find(|s| step_uses(s).is_some_and(|u| u.starts_with("actions/attest@")))
        .expect("the provenance job runs actions/attest");
    assert!(
        attest.iter().any(|l| l.trim() == "subject-path: dist/*"),
        "it attests every file the build jobs uploaded: {attest:#?}"
    );
    // Proved on the spot, the way a person installing it would: an attestation that does not
    // verify is not one.
    assert!(
        steps
            .iter()
            .flatten()
            .any(|l| l.contains("gh attestation verify")),
        "the provenance job verifies what it attested"
    );
}

#[test]
fn nothing_is_published_before_it_is_attested() {
    let jobs = release_jobs();
    let publish = job(&jobs, "publish");
    assert!(
        publish.needs().contains(&"provenance".to_owned()),
        "publish needs {:?}; it must wait for `provenance`",
        publish.needs()
    );
}

#[test]
fn only_the_provenance_job_holds_a_signing_identity_and_it_holds_nothing_else() {
    let jobs = release_jobs();
    let provenance = job(&jobs, "provenance");
    assert_eq!(
        provenance.permissions(),
        Some(vec![
            ("attestations".to_owned(), "write".to_owned()),
            ("id-token".to_owned(), "write".to_owned()),
        ]),
        "the provenance job checks nothing out and writes nothing but the attestation"
    );
    assert!(
        !provenance.reads_a_secret() && !provenance.publishes(),
        "the job that signs provenance holds no release key and cannot publish"
    );

    let holders: Vec<&str> = jobs
        .iter()
        .filter(|j| j.holds_a_signing_identity())
        .map(|j| j.name.as_str())
        .collect();
    assert_eq!(holders, ["provenance"], "{holders:?}");
}

#[test]
fn the_workflow_default_stays_read_only() {
    let text = workflow::read(&workflow::workflows_dir().join("release.yml"));
    let top: Vec<&str> = text
        .lines()
        .skip_while(|l| *l != "permissions:")
        .skip(1)
        .take_while(|l| l.starts_with("  "))
        .map(str::trim)
        .collect();
    assert_eq!(top, ["contents: read"]);
}
