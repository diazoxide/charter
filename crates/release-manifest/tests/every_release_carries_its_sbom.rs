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
//! The SBOM files are build files like any other: uploaded as a `purlis-*` artifact, attested
//! by `provenance`, verified and uploaded by `publish`.
//!
//! The workflow is read by `workflow/mod.rs`, which the other workflow tests share.

mod workflow;

use std::path::PathBuf;

use workflow::{Job, downloads_by_name, job, release_jobs, run_lines, step_uses, step_value};

/// The wrapper Tauri's `--runner` calls in place of `cargo`.
const RUNNER: &str = "tools/auditable-cargo";

#[test]
fn every_rust_binary_the_release_ships_is_built_with_its_dependency_list() {
    let jobs = release_jobs();
    let build = job(&jobs, "build");

    // One exact cargo-auditable, from its own lockfile. `cargo install` takes no checksum for
    // the crate it installs; crates.io never replaces a published version, and Cargo checks
    // every crate it downloads against the index.
    let install = step(build, "cargo-auditable");
    let version = step_value(&install, "env", "CARGO_AUDITABLE_VERSION").expect("a version");
    assert!(
        version.split('.').count() == 3
            && version
                .split('.')
                .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit())),
        "CARGO_AUDITABLE_VERSION is one exact version: {version}"
    );
    assert_eq!(
        run_lines(&install),
        [r#"cargo install --locked cargo-auditable --version "$CARGO_AUDITABLE_VERSION""#]
    );

    assert_eq!(
        run_lines(&step(
            build,
            "The purlis binary the app's hooks run, its charter alias, and the built-in extensions"
        )),
        [
            r#"cargo auditable build --profile "$PROFILE" --locked -p purlis-cli -p persona-statistics"#
        ]
    );

    // Tauri runs `<runner> build …` itself, so the app binary goes through the wrapper. Both
    // `tauri build` calls, or the one that makes the published bundle may be the plain one.
    // `-- --profile` is the same profile the command line above was built with (ADR 0092).
    assert_eq!(
        run_lines(&step(build, "Build the app")),
        [format!(
            r#"npx tauri build --config src-tauri/tauri.build.conf.json --bundles "$BUNDLES" --runner "$GITHUB_WORKSPACE/{RUNNER}" -- --profile "$PROFILE""#
        )]
    );
    assert_eq!(
        run_lines(&step(build, "Build the extra bundle")),
        [format!(
            r#"npx tauri build --config src-tauri/tauri.build.conf.json --bundles ${{{{ matrix.bundles }}}},${{{{ matrix.extra_bundles }}}} --runner "$GITHUB_WORKSPACE/{RUNNER}" -- --profile "$PROFILE""#
        )]
    );

    // And nothing in the job compiles around it.
    let plain: Vec<String> = build
        .steps()
        .iter()
        .flat_map(|s| run_lines(s))
        .filter(|l| l.starts_with("cargo build") || l.contains(" cargo build"))
        .collect();
    assert!(
        plain.is_empty(),
        "build runs a plain `cargo build`: {plain:#?}"
    );
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

/// The step called `name` in `job`.
fn step(job: &Job, name: &str) -> Vec<String> {
    job.steps()
        .into_iter()
        .find(|s| s[0].trim() == format!("- name: {name}"))
        .unwrap_or_else(|| panic!("job `{}` has no step `{name}`", job.name))
}

fn position(lines: &[String], want: &str) -> usize {
    lines
        .iter()
        .position(|l| l == want)
        .unwrap_or_else(|| panic!("`{want}` is not a command of {lines:#?}"))
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
    assert_eq!(
        downloads_by_name(sbom),
        [
            "purlis-linux-x86_64-${{ needs.plan.outputs.version }}",
            "purlis-macos-arm64-${{ needs.plan.outputs.version }}",
        ]
    );

    // Only GitHub's own actions. A third-party action gets the run's artifact token, and with
    // it could upload an artifact that provenance would attest and publish would ship.
    let actions: Vec<String> = sbom
        .steps()
        .iter()
        .filter_map(|s| step_uses(s))
        .map(|u| u.split('@').next().unwrap_or_default().to_owned())
        .collect();
    assert_eq!(
        actions,
        [
            "actions/checkout",
            "actions/download-artifact",
            "actions/download-artifact",
            "actions/upload-artifact",
        ]
    );

    // syft, at one version, checked against a sha256 committed here BEFORE it is unpacked,
    // and run from a `run:` step, which is handed no runtime token.
    let syft = step(
        sbom,
        "syft, at one version and checksum, reads each platform's bundle",
    );
    let version = step_value(&syft, "env", "SYFT_VERSION").expect("SYFT_VERSION");
    assert!(
        version.split('.').count() == 3
            && version
                .split('.')
                .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit())),
        "SYFT_VERSION is one exact version: {version}"
    );
    let sha = step_value(&syft, "env", "SYFT_SHA256").expect("SYFT_SHA256");
    assert!(
        sha.len() == 64
            && sha
                .chars()
                .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)),
        "SYFT_SHA256 is a sha256: {sha}"
    );
    let lines = run_lines(&syft);
    let fetch = position(
        &lines,
        r#"curl -fsSL --proto '=https' -o "$RUNNER_TEMP/syft.tar.gz" "https://github.com/anchore/syft/releases/download/v${SYFT_VERSION}/syft_${SYFT_VERSION}_linux_amd64.tar.gz""#,
    );
    let check = position(
        &lines,
        r#"echo "$SYFT_SHA256  $RUNNER_TEMP/syft.tar.gz" | sha256sum -c -"#,
    );
    let unpack = position(
        &lines,
        r#"tar -xzf "$RUNNER_TEMP/syft.tar.gz" -C "$RUNNER_TEMP" syft"#,
    );
    assert!(
        fetch < check && check < unpack,
        "fetched, checked, THEN unpacked"
    );
    for platform in ["macos-arm64", "linux-x86_64"] {
        let scan = position(
            &lines,
            &format!(
                r#""$RUNNER_TEMP/syft" scan "dir:sbom/{platform}" -o "cyclonedx-json=out/purlis-{platform}.cdx.json""#
            ),
        );
        assert!(unpack < scan);
    }

    // What proves cargo-auditable worked: each shipped binary's own crate is in the SBOM.
    let names = step(
        sbom,
        "Each SBOM names every binary it ships, and the front end",
    );
    assert_eq!(
        step_value(&names, "env", "REQUIRED_PURLS"),
        Some("pkg:cargo/purlis-app@ pkg:cargo/purlis-cli@ pkg:cargo/persona-statistics@ pkg:npm/")
    );
    let lines = run_lines(&names);
    position(&lines, r#"for sbom in out/*.cdx.json; do"#);
    position(
        &lines,
        r#"purls=$(jq -r '.components[]?.purl // empty' "$sbom")"#,
    );
    position(&lines, r#"for want in $REQUIRED_PURLS; do"#);

    let upload = sbom
        .steps()
        .into_iter()
        .find(|s| step_uses(s).is_some_and(|u| u.starts_with("actions/upload-artifact@")))
        .expect("the sbom job uploads what it made");
    assert_eq!(
        step_value(&upload, "with", "name"),
        Some("purlis-sbom-${{ needs.plan.outputs.version }}")
    );
    assert_eq!(step_value(&upload, "with", "path"), Some("out/"));
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
