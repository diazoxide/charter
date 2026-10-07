//! The two channels differ in optimisation profile and bundle set, and in nothing else
//! (ADR 0092, #1429).
//!
//! A **dev channel build** is compiled with `dev-release` and makes no `.dmg`. A **stable
//! build** is compiled with `release` and makes every bundle. A build started by hand is a
//! stable-profile build unless it asks for the other. `plan` decides this once per run, and
//! every later step reads the decision, so no step can build one profile and collect, check or
//! describe another.
//!
//! What the channels may NOT differ in is held by the tests beside this one: every job with a
//! secret runs in the `release` environment and restores no cache, on both channels
//! (`the_release_workflow_runs_in_the_release_environment.rs`,
//! `the_workflows_run_only_what_they_pinned.rs`).

mod workflow;

use std::path::PathBuf;

use workflow::{Job, job, release_jobs, run_lines, step_value};

/// The step called `name` in `job`.
fn step(job: &Job, name: &str) -> Vec<String> {
    job.steps()
        .into_iter()
        .find(|s| s[0].trim() == format!("- name: {name}"))
        .unwrap_or_else(|| panic!("job `{}` has no step `{name}`", job.name))
}

/// The settings of `[profile.<name>]` in the workspace's Cargo.toml, as written, sorted.
fn profile(name: &str) -> Vec<String> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.toml");
    let text = workflow::read(&path);
    let header = format!("[profile.{name}]");
    let mut settings: Vec<String> = text
        .lines()
        .skip_while(|l| *l != header)
        .skip(1)
        .take_while(|l| !l.starts_with('['))
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_owned)
        .collect();
    assert!(!settings.is_empty(), "Cargo.toml has no {header}");
    settings.sort();
    settings
}

#[test]
fn dev_release_is_release_with_a_lighter_link_and_nothing_else() {
    // The stable build's profile, exactly as it was before the dev channel got its own.
    assert_eq!(
        profile("release"),
        [
            "codegen-units = 1",
            "lto = true",
            "opt-level = 3",
            r#"panic = "abort""#,
            "strip = true",
        ]
    );
    // `opt-level`, `panic` and `strip` are inherited. A third setting here is a second way the
    // channels differ, which is the operator's to decide (ADR 0092).
    assert_eq!(
        profile("dev-release"),
        [
            "codegen-units = 16",
            r#"inherits = "release""#,
            r#"lto = "thin""#,
        ]
    );
}

/// The lines of `plan`'s decision for one event: from its `case` arm to the next arm.
fn arm(lines: &[String], event: &str) -> Vec<String> {
    const ARMS: [&str; 4] = ["push)", "workflow_run)", "schedule)", "*)"];
    let start = lines
        .iter()
        .position(|l| l == event)
        .unwrap_or_else(|| panic!("plan has no `{event}` arm: {lines:#?}"));
    lines[start + 1..]
        .iter()
        .take_while(|l| !ARMS.contains(&l.as_str()) && *l != "esac")
        .cloned()
        .collect()
}

#[test]
fn plan_gives_each_way_in_its_profile_and_its_bundles() {
    let jobs = release_jobs();
    let plan = job(&jobs, "plan");
    let lines = run_lines(&step(
        plan,
        "Channel, version, and whether anything is published",
    ));
    let has = |event: &str, want: &str| {
        let arm = arm(&lines, event);
        assert!(
            arm.iter().any(|l| l == want),
            "`{event}` lacks `{want}`: {arm:#?}"
        );
    };

    // A `v*` tag: the stable build.
    has("push)", "channel=stable; publish=true");
    has(
        "push)",
        "profile=release; dmg=true; appimage_required=true ;;",
    );
    // `ci` green on `main`: the dev channel build.
    has("workflow_run)", "channel=dev; publish=true");
    has(
        "workflow_run)",
        "profile=dev-release; dmg=false; appimage_required=true ;;",
    );
    // The nightly: the stable build's profile and bundles, published nowhere.
    has("schedule)", "channel=dev; publish=false");
    has(
        "schedule)",
        "profile=release; dmg=true; appimage_required=true ;;",
    );
    // A build started by hand: the profile it asked for, `release` when it asked for none, and
    // an extra bundle that may fail.
    has("*)", "channel=dev; publish=false");
    has(
        "*)",
        r#"profile="${ASKED_PROFILE:-release}"; dmg=true; appimage_required=false"#,
    );
    has("*)", "release | dev-release) ;;");

    // No other line of the step sets a profile, so no arm can be overruled further down.
    let sets: Vec<&String> = lines.iter().filter(|l| l.starts_with("profile=")).collect();
    assert_eq!(sets.len(), 4, "{sets:#?}");

    let step = step(plan, "Channel, version, and whether anything is published");
    assert_eq!(
        step_value(&step, "env", "ASKED_PROFILE"),
        Some("${{ inputs.profile }}")
    );
}

#[test]
fn a_build_started_by_hand_is_a_stable_profile_build_unless_it_asks() {
    let text = workflow::read(&workflow::workflows_dir().join("release.yml"));
    let input: Vec<&str> = text
        .lines()
        .skip_while(|l| *l != "      profile:")
        .skip(1)
        .take_while(|l| l.starts_with("        "))
        .map(str::trim)
        .filter(|l| !l.starts_with("description:"))
        .collect();
    assert_eq!(
        input,
        [
            "type: choice",
            "options: [release, dev-release]",
            "default: release"
        ]
    );
}

#[test]
fn every_step_of_the_build_reads_the_one_profile_plan_chose() {
    let jobs = release_jobs();
    let build = job(&jobs, "build");
    assert!(
        build
            .body
            .iter()
            .any(|l| l == "      PROFILE: ${{ needs.plan.outputs.profile }}"),
        "build does not take its profile from plan"
    );

    // `target/release` written out is a step that builds, collects or checks the stable
    // profile's output whatever this run built.
    let fixed: Vec<&String> = build
        .body
        .iter()
        .filter(|l| l.contains("target/release") || l.contains("--release"))
        .collect();
    assert!(fixed.is_empty(), "{fixed:#?}");

    // And the binary is asked. The version line names the profile it was built with, so a
    // run that asked for one and built another stops before anything is bundled.
    let staged = run_lines(&step(
        build,
        "Stage them where Tauri's bundler picks them up",
    ));
    for want in [
        r#"line=$("target/$PROFILE/purlis" --version)"#,
        r#"*"($PROFILE profile)") ;;"#,
    ] {
        assert!(staged.iter().any(|l| l == want), "{want}: {staged:#?}");
    }
}

#[test]
fn the_summary_names_the_profile_the_run_built() {
    let jobs = release_jobs();
    let build = job(&jobs, "build");
    let said = run_lines(&step(build, "What a person sees when they install it"));
    let built: Vec<&String> = said.iter().filter(|l| l.contains("Built from")).collect();
    assert_eq!(built.len(), 2, "{built:#?}");
    for line in built {
        assert!(line.contains(r"\`$PROFILE\` profile"), "{line}");
        assert!(!line.contains("release profile"), "{line}");
    }
}

#[test]
fn a_required_extra_bundle_is_made_by_the_one_build_and_a_dev_build_makes_no_dmg() {
    let jobs = release_jobs();
    let build = job(&jobs, "build");

    // One `tauri build` for both bundles when the AppImage is required, on both channels.
    let app = step(build, "Build the app");
    assert_eq!(
        step_value(&app, "env", "BUNDLES"),
        Some(
            "${{ (matrix.extra_required && needs.plan.outputs.appimage_required == 'true') && format('{0},{1}', matrix.bundles, matrix.extra_bundles) || matrix.bundles }}"
        )
    );

    // The second build is only ever the optional one: skipped when the first made the bundle,
    // and for the `.dmg` of a dev channel build. So it may always fail.
    let extra = step(build, "Build the extra bundle");
    let value = |key: &str| {
        let prefix = format!("        {key}:");
        extra
            .iter()
            .find_map(|l| l.strip_prefix(prefix.as_str()))
            .map(str::trim)
    };
    assert_eq!(
        value("if"),
        Some(
            "${{ !(matrix.extra_required && needs.plan.outputs.appimage_required == 'true') && !(matrix.extra_bundles == 'dmg' && needs.plan.outputs.dmg != 'true') }}"
        )
    );
    assert_eq!(value("continue-on-error"), Some("true"));

    // Only macOS has a `.dmg` and only Linux has a required extra, which is what the two
    // conditions above read.
    let matrix: Vec<&str> = build
        .body
        .iter()
        .map(|l| l.trim())
        .filter(|l| l.starts_with("extra_bundles:") || l.starts_with("extra_required:"))
        .collect();
    assert_eq!(
        matrix,
        [
            "extra_bundles: dmg",
            "extra_required: false",
            "extra_bundles: appimage",
            "extra_required: true"
        ]
    );
}

#[test]
fn a_dev_publish_takes_down_the_dmg_an_earlier_one_left() {
    // Assets are replaced by name, so a name this build did not make would stay on the `dev`
    // release for ever, naming an older build.
    let jobs = release_jobs();
    let publish = job(&jobs, "publish");
    let lines = run_lines(&step(
        publish,
        "Dev — replace the assets of the one `dev` prerelease",
    ));
    let at = |want: &str| {
        lines
            .iter()
            .position(|l| l.contains(want))
            .unwrap_or_else(|| panic!("`{want}` is not in {lines:#?}"))
    };
    let removed = at(r#"select(.name | endswith(".dmg"))"#);
    assert!(at(r#"tools/replace-release-assets.sh dev "${assets[@]}""#) < removed);
    // Before the manifest, which goes last.
    assert!(removed < at("tools/replace-release-assets.sh dev dev.json dev-weekly.json"));
}
