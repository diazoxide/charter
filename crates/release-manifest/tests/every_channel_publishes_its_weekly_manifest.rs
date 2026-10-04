//! Each channel's publish uploads its weekly manifest beside its manifest, and both last (OB-17).
//!
//! `release-manifest` writes the two from one string; this checks the workflow uploads both,
//! after every asset they name, so the first update check of a week never reads a manifest
//! that is missing or one that names a bundle not uploaded yet.

use charter_core::updates::{Channel, DEV_TAG};

fn release_yml() -> String {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../.github/workflows/release.yml"
    );
    std::fs::read_to_string(path).expect("release.yml is in the repository")
}

#[test]
fn every_channel_uploads_its_weekly_manifest_with_its_manifest_after_its_assets() {
    let workflow = release_yml();
    for (channel, tag) in [(Channel::Stable, "\"$TAG\""), (Channel::Dev, DEV_TAG)] {
        let manifests = format!(
            "tools/replace-release-assets.sh {tag} {} {}",
            channel.manifest(),
            channel.weekly_manifest()
        );
        let assets = format!("tools/replace-release-assets.sh {tag} \"${{assets[@]}}\"");
        let at = workflow
            .find(&manifests)
            .unwrap_or_else(|| panic!("release.yml has no `{manifests}`"));
        let after = workflow[..at]
            .rfind(&assets)
            .or_else(|| workflow[..at].rfind("gh release create"));
        assert!(
            after.is_some(),
            "{} is uploaded before the assets it names",
            channel.name()
        );
    }
}
