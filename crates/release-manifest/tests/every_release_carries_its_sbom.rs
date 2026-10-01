//! Every release carries a CycloneDX SBOM for each platform it ships, and every Rust binary in
//! it carries its own dependency list (`cargo-auditable`) (#584).
//!
//! The SBOM is read off what was BUILT, not off the lockfile: `build` links each binary with
//! `cargo auditable`, which embeds the exact crates it was compiled from, and the unprivileged
//! `sbom` job reads those back out of the shipped bundles with syft, beside the web front
//! end's production packages from `app/package-lock.json`. So the SBOM names what is in the
//! download, and a binary built without its dependency list fails the release instead of
//! producing an SBOM that quietly lists nothing.
//!
//! The SBOM files are build files like any other: uploaded as a `charter-*` artifact, attested
//! by `provenance`, verified and uploaded by `publish`.
//!
//! The workflow is read by `workflow/mod.rs`, which the other workflow tests share.

mod workflow;

use std::path::PathBuf;

use workflow::{Job, release_jobs, step_uses};

fn job<'a>(jobs: &'a [Job], name: &str) -> &'a Job {
    jobs.iter()
        .find(|j| j.name == name)
        .unwrap_or_else(|| panic!("release.yml has no `{name}` job"))
}

fn text_of(job: &Job) -> String {
    job.body.join("\n")
}

/// The wrapper Tauri's `--runner` calls in place of `cargo`.
const RUNNER: &str = "tools/auditable-cargo";

#[test]
fn every_rust_binary_the_release_ships_is_built_with_its_dependency_list() {
    let jobs = release_jobs();
    let build = job(&jobs, "build");
    let text = text_of(build);

    assert!(
        text.contains("cargo install --locked cargo-auditable --version "),
        "build installs one exact cargo-auditable:\n{text}"
    );
    assert!(
        text.contains(
            "cargo auditable build --release --locked -p charter-cli -p persona-statistics"
        ),
        "the sidecar and the built-in extension are built with cargo auditable"
    );
    let plain: Vec<&String> = build
        .body
        .iter()
        .filter(|l| l.contains("cargo build"))
        .collect();
    assert!(
        plain.is_empty(),
        "build runs a plain `cargo build`: {plain:#?}"
    );

    // Tauri runs `<runner> build …` itself, so the app binary goes through the wrapper. Both
    // `tauri build` calls, or the one that makes the published bundle may be the plain one.
    let tauri: Vec<&String> = build
        .body
        .iter()
        .filter(|l| l.contains("npx tauri build"))
        .collect();
    assert_eq!(tauri.len(), 2, "{tauri:#?}");
    for line in tauri {
        assert!(
            line.contains(&format!("--runner \"$GITHUB_WORKSPACE/{RUNNER}\"")),
            "`tauri build` without the auditable runner: {line}"
        );
    }
}

#[test]
fn the_runner_is_cargo_auditable_and_nothing_else() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(RUNNER);
    let script = workflow::read(&path);
    let code: Vec<&str> = script
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.trim_start().starts_with('#'))
        .collect();
    assert_eq!(code, [r#"exec cargo auditable "$@""#], "{script}");
    assert!(script.starts_with("#!/bin/sh\n"));

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert!(mode & 0o111 != 0, "{RUNNER} is not executable");
    }
}

#[test]
fn the_sbom_job_holds_nothing_and_reads_the_sbom_off_the_built_bundles() {
    let jobs = release_jobs();
    let sbom = job(&jobs, "sbom");

    assert!(
        sbom.needs().contains(&"build".to_owned()),
        "{:?}",
        sbom.needs()
    );
    assert_eq!(
        sbom.permissions(),
        Some(vec![("contents".to_owned(), "read".to_owned())]),
        "the SBOM is made in a job that holds nothing"
    );
    assert!(!sbom.reads_a_secret() && !sbom.publishes() && sbom.environment().is_none());
    assert!(!sbom.can_mint_an_oidc_token());

    let steps = sbom.steps();
    let download = steps
        .iter()
        .find(|s| step_uses(s).is_some_and(|u| u.starts_with("actions/download-artifact@")))
        .expect("the sbom job downloads the builds");
    assert!(download.iter().any(|l| l.trim() == "pattern: charter-*"));

    let syft: Vec<&Vec<String>> = steps
        .iter()
        .filter(|s| step_uses(s).is_some_and(|u| u.starts_with("anchore/sbom-action@")))
        .collect();
    assert_eq!(syft.len(), 2, "one SBOM per platform");
    for step in syft {
        let has = |want: &str| step.iter().any(|l| l.trim() == want);
        assert!(has("format: cyclonedx-json"), "{step:#?}");
        // The action would otherwise upload its own artifact, and on a release event attach
        // itself to the release outside `publish`.
        assert!(has("upload-artifact: false"), "{step:#?}");
        assert!(has("upload-release-assets: false"), "{step:#?}");
        assert!(
            step.iter().any(|l| l.trim().starts_with("syft-version: v")),
            "the syft version is pinned: {step:#?}"
        );
    }

    // What proves cargo-auditable worked: each shipped binary's own crate is in the SBOM.
    let text = text_of(sbom);
    for purl in [
        "pkg:cargo/charter-app@",
        "pkg:cargo/charter-cli@",
        "pkg:cargo/persona-statistics@",
        "pkg:npm/",
    ] {
        assert!(
            text.contains(purl),
            "the sbom job does not check for {purl}"
        );
    }

    let upload = steps
        .iter()
        .find(|s| step_uses(s).is_some_and(|u| u.starts_with("actions/upload-artifact@")))
        .expect("the sbom job uploads what it made");
    assert!(
        upload
            .iter()
            .any(|l| l.trim().starts_with("name: charter-sbom-")),
        "a `charter-*` artifact, so provenance and publish pick it up: {upload:#?}"
    );
}

#[test]
fn the_sbom_is_attested_and_published_like_every_build_file() {
    let jobs = release_jobs();
    let provenance = job(&jobs, "provenance");
    assert!(
        provenance.needs().contains(&"sbom".to_owned()),
        "provenance attests the SBOM, so it waits for it: {:?}",
        provenance.needs()
    );

    // `publish` leaves `.json` files out of the release assets so that a manifest is only ever
    // uploaded last and on its own. The SBOM is `.cdx.json` and has to get through.
    let publish = job(&jobs, "publish");
    let filters: Vec<&String> = publish
        .body
        .iter()
        .filter(|l| l.contains("== *.json"))
        .collect();
    assert_eq!(filters.len(), 2, "one per channel: {filters:#?}");
    for line in filters {
        assert!(
            line.contains("!= *.cdx.json"),
            "this filter drops the SBOM: {line}"
        );
    }
}
