//! Every build file `release.yml` publishes carries a signed SLSA build provenance
//! attestation, made before anything is published and checked again on the bytes `publish`
//! uploads (#583).
//!
//! The attestation is what `gh attestation verify` checks: that a file came out of this
//! repository's `release.yml`, at a named commit. Signing it takes an OIDC token
//! (`id-token: write`) and storing it takes `attestations: write`. So exactly one job holds
//! them, it holds nothing else, and the jobs that hold a release key or can write the release
//! never get them. `publish` only READS attestations, to prove that what it is about to upload
//! is what was attested in this very run.
//!
//! The workflow is read by `workflow/mod.rs`, which the pinning and environment tests share.

mod workflow;

use workflow::{
    Job, downloads_by_name, job, release_jobs, run_lines, step_uses, workflow_permissions,
};

fn pairs(list: &[(&str, &str)]) -> Vec<(String, String)> {
    list.iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect()
}

/// Every flag a verification needs to be pinned to THIS run of THIS workflow, not merely to
/// "something this repository once built".
const RUN_PINNED: [&str; 6] = [
    r#"--repo "$GITHUB_REPOSITORY""#,
    r#"--signer-workflow "$GITHUB_REPOSITORY/.github/workflows/release.yml""#,
    r#"--signer-digest "$GITHUB_WORKFLOW_SHA""#,
    r#"--source-digest "$GITHUB_SHA""#,
    "--deny-self-hosted-runners",
    // On the stable channel only, and appended through this array.
    r#"ref=(--source-ref "$GITHUB_REF")"#,
];

/// The index of the job's step that runs `gh attestation verify`, after checking that it
/// carries every run-pinning flag and verifies every file in `dist/`.
fn run_pinned_verification(job: &Job) -> usize {
    let steps = job.steps();
    let (index, step) = steps
        .iter()
        .enumerate()
        .find(|(_, s)| s.iter().any(|l| l.contains("gh attestation verify")))
        .unwrap_or_else(|| panic!("job `{}` runs no `gh attestation verify`", job.name));
    let text = step.join("\n");
    assert!(
        text.contains("for f in dist/*; do"),
        "job `{}` verifies every file in dist/:\n{text}",
        job.name
    );
    for flag in RUN_PINNED {
        assert!(
            text.contains(flag),
            "job `{}`'s verification lacks `{flag}`:\n{text}",
            job.name
        );
    }
    assert!(
        text.contains(r#"[ "$CHANNEL" = stable ]"#) && text.contains(r#""${ref[@]}""#),
        "job `{}` pins the source ref on the stable channel:\n{text}",
        job.name
    );
    index
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
    let (attest_at, attest) = steps
        .iter()
        .enumerate()
        .find(|(_, s)| step_uses(s).is_some_and(|u| u.starts_with("actions/attest@")))
        .expect("the provenance job runs actions/attest");
    assert!(
        attest.iter().any(|l| l.trim() == "subject-path: dist/*"),
        "it attests every file the build jobs uploaded: {attest:#?}"
    );
    // Proved on the spot, the way a person installing it would: an attestation that does not
    // verify is not one.
    assert!(run_pinned_verification(provenance) > attest_at);
}

#[test]
fn publish_downloads_only_the_builds_and_verifies_them_before_it_uploads_anything() {
    let jobs = release_jobs();
    let publish = job(&jobs, "publish");
    assert!(
        publish.needs().contains(&"provenance".to_owned()),
        "publish needs {:?}; it must wait for `provenance`",
        publish.needs()
    );

    let steps = publish.steps();
    let verify_at = run_pinned_verification(publish);
    let first_upload = steps
        .iter()
        .position(|s| {
            s.iter().any(|l| {
                l.contains("gh release create")
                    || l.contains("gh release edit")
                    || l.contains("replace-release-assets.sh")
            })
        })
        .expect("publish uploads something");
    assert!(
        verify_at < first_upload,
        "publish verifies the provenance of what it uploads BEFORE it uploads it"
    );
}

#[test]
fn only_the_provenance_job_can_sign_and_publish_can_only_read() {
    let jobs = release_jobs();
    let provenance = job(&jobs, "provenance");
    assert_eq!(
        provenance.permissions(),
        Some(pairs(&[("attestations", "write"), ("id-token", "write")])),
        "the provenance job checks nothing out and writes nothing but the attestation"
    );
    assert!(
        !provenance.reads_a_secret() && !provenance.publishes(),
        "the job that signs provenance holds no release key and cannot publish"
    );
    assert_eq!(
        job(&jobs, "publish").permissions(),
        Some(pairs(&[("attestations", "read"), ("contents", "write")])),
        "publish writes the release and only reads attestations"
    );

    let signers: Vec<&str> = jobs
        .iter()
        .filter(|j| j.can_mint_an_oidc_token())
        .map(|j| j.name.as_str())
        .collect();
    assert_eq!(signers, ["provenance"], "{signers:?}");
    let attesters: Vec<&str> = jobs
        .iter()
        .filter(|j| {
            j.permissions()
                .is_some_and(|p| p.iter().any(|(k, v)| k == "attestations" && v == "write"))
        })
        .map(|j| j.name.as_str())
        .collect();
    assert_eq!(attesters, ["provenance"], "{attesters:?}");
}

#[test]
fn the_workflow_default_stays_read_only() {
    let text = workflow::read(&workflow::workflows_dir().join("release.yml"));
    assert_eq!(workflow_permissions(&text), pairs(&[("contents", "read")]));
}

/// What each build job uploads, by the exact name `build` and `sbom` give it.
const BUILD_ARTIFACTS: [&str; 3] = [
    "charter-linux-x86_64-${{ needs.plan.outputs.version }}",
    "charter-macos-arm64-${{ needs.plan.outputs.version }}",
    "charter-sbom-${{ needs.plan.outputs.version }}",
];

#[test]
fn provenance_and_publish_take_only_the_build_artifacts_by_their_exact_names() {
    let jobs = release_jobs();
    for name in ["provenance", "publish"] {
        assert_eq!(
            downloads_by_name(job(&jobs, name)),
            BUILD_ARTIFACTS,
            "job `{name}`"
        );
    }

    // The names those are: `build` names its artifact after the platform it built and the
    // version `plan` decided.
    let names = job(&jobs, "build")
        .steps()
        .into_iter()
        .find(|s| s[0].trim() == "- name: What this build is called")
        .expect("build names its artifact");
    let lines = run_lines(&names);
    for want in [
        "aarch64-apple-darwin) slug=macos-arm64; updater=darwin-aarch64 ;;",
        "x86_64-unknown-linux-gnu) slug=linux-x86_64; updater=linux-x86_64-appimage ;;",
    ] {
        assert!(lines.iter().any(|l| l == want), "{want} not in {lines:#?}");
    }
    // One `{ … } >> "$GITHUB_OUTPUT"` group writes the step's outputs (shellcheck SC2129), the
    // artifact's name among them. Read as one run of lines, so an echo moved out of the group,
    // where it would print to the log instead of becoming an output, fails here.
    let outputs = [
        "{",
        r#"echo "triple=$triple""#,
        r#"echo "slug=$slug""#,
        r#"echo "updater=$updater""#,
        r#"echo "artifact=charter-$slug-${VERSION}""#,
        r#"} >> "$GITHUB_OUTPUT""#,
    ];
    assert!(
        lines.windows(outputs.len()).any(|w| w == outputs),
        "{outputs:#?} not one run of lines in {lines:#?}"
    );
}
