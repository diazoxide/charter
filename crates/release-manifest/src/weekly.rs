//! The weekly-user estimate, read off GitHub's download counts (OB-17).
//!
//! The first update check of each ISO week fetches the channel's weekly manifest instead of its
//! manifest ([`purlis_core::updates::weekly_due`]). The two files have the same bytes, and the
//! request names nothing about the machine, so the only thing that reaches anyone is GitHub's
//! download count of the weekly file. This reads those counts out of a release listing, the
//! JSON `gh api --paginate repos/diazoxide/charter/releases` writes, and reports them per
//! release beside each release's download counts.
//!
//! **A count is cumulative, so the estimate is a difference.** Given an earlier listing, each
//! release's weekly-manifest count grew by the number of first-of-the-week checks made while
//! that release was its channel's newest. The sum of that growth over a channel's releases,
//! between two listings a week apart, estimates that channel's weekly users. A weekly asset
//! that was replaced since (another id) started again from zero, so its whole count is growth.
//! `docs/updating.md`, *Counting weekly users*, states the estimator and its biases.
//!
//! Nothing here reaches the network, and nothing is kept: the listings are the operator's own
//! files, and this prints.

use purlis_core::updates::{Channel, DEV_TAG};

/// One release's line of the report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// `stable` or `dev`.
    pub channel: &'static str,
    /// The release's tag.
    pub tag: String,
    /// The weekly manifest's download count, or `None` for a release published without one.
    pub weekly: Option<u64>,
    /// When the weekly manifest asset was uploaded: the count is since then.
    pub since: Option<String>,
    /// Growth of [`Row::weekly`] since the earlier listing, when one was given.
    pub weekly_growth: Option<u64>,
    /// Downloads of the release's bundles and installers: every asset that is not a manifest,
    /// a signature or a provenance or SBOM file.
    pub downloads: u64,
    /// Growth of [`Row::downloads`] since the earlier listing, when one was given.
    pub downloads_growth: Option<u64>,
}

/// One asset as the listing has it.
struct Asset {
    id: u64,
    name: String,
    count: u64,
    created: Option<String>,
}

/// One release as the listing has it.
struct Release {
    tag: String,
    channel: Channel,
    assets: Vec<Asset>,
}

impl Release {
    fn weekly(&self) -> Option<&Asset> {
        let name = self.channel.weekly_manifest();
        self.assets.iter().find(|asset| asset.name == name)
    }

    fn downloads(&self) -> u64 {
        self.assets
            .iter()
            .filter(|asset| is_a_bundle(&asset.name))
            .map(|asset| asset.count)
            .sum()
    }
}

/// Whether an asset is something a person installs or the updater downloads, rather than a
/// manifest, a signature, an attestation or an SBOM.
fn is_a_bundle(name: &str) -> bool {
    ![".json", ".sig", ".jsonl", ".intoto"]
        .iter()
        .any(|suffix| name.ends_with(suffix))
}

/// Every release in a listing: one JSON array, or `gh api --paginate`'s arrays back to back.
fn releases(listing: &str) -> Result<Vec<Release>, String> {
    let mut out = Vec::new();
    for page in serde_json::Deserializer::from_str(listing).into_iter::<serde_json::Value>() {
        let page = page.map_err(|why| format!("the listing is not JSON: {why}"))?;
        let page = page
            .as_array()
            .ok_or("the listing is not an array of releases, as `gh api …/releases` writes")?;
        for release in page {
            let tag = release["tag_name"]
                .as_str()
                .ok_or("a release in the listing has no tag_name")?
                .to_owned();
            let prerelease = release["prerelease"].as_bool().unwrap_or(false);
            let channel = if prerelease && tag == DEV_TAG {
                Channel::Dev
            } else if prerelease {
                // A prerelease that is not the dev channel is read by no updater.
                continue;
            } else {
                Channel::Stable
            };
            let assets = release["assets"]
                .as_array()
                .map(|assets| {
                    assets
                        .iter()
                        .map(|asset| Asset {
                            id: asset["id"].as_u64().unwrap_or_default(),
                            name: asset["name"].as_str().unwrap_or_default().to_owned(),
                            count: asset["download_count"].as_u64().unwrap_or_default(),
                            created: asset["created_at"].as_str().map(str::to_owned),
                        })
                        .collect()
                })
                .unwrap_or_default();
            out.push(Release {
                tag,
                channel,
                assets,
            });
        }
    }
    Ok(out)
}

/// The report's rows for the listing `now`, with growth against `earlier` when it is given.
pub fn rows(now: &str, earlier: Option<&str>) -> Result<Vec<Row>, String> {
    let earlier = earlier.map(releases).transpose()?;
    Ok(releases(now)?
        .into_iter()
        .map(|release| {
            let before = earlier
                .as_ref()
                .and_then(|all| all.iter().find(|old| old.tag == release.tag));
            let weekly = release.weekly();
            let weekly_growth = earlier.as_ref().and(weekly).map(|asset| {
                match before.and_then(Release::weekly) {
                    Some(old) if old.id == asset.id => asset.count.saturating_sub(old.count),
                    // Replaced or new since: it counted from zero.
                    _ => asset.count,
                }
            });
            let downloads = release.downloads();
            let downloads_growth = earlier
                .as_ref()
                .map(|_| downloads.saturating_sub(before.map_or(0, Release::downloads)));
            Row {
                channel: release.channel.name(),
                tag: release.tag.clone(),
                weekly: weekly.map(|asset| asset.count),
                since: weekly.and_then(|asset| asset.created.clone()),
                weekly_growth,
                downloads,
                downloads_growth,
            }
        })
        .collect())
}

/// The report as text: a line per release, then, against an earlier listing, the estimate per
/// channel.
pub fn report(now: &str, earlier: Option<&str>) -> Result<String, String> {
    let rows = rows(now, earlier)?;
    let mut out = String::from("channel  release        weekly manifest            downloads\n");
    let grown = |n: Option<u64>| n.map(|n| format!(" (+{n})")).unwrap_or_default();
    for row in &rows {
        let weekly = match (row.weekly, &row.since) {
            (Some(n), Some(since)) => format!("{n}{} since {since}", grown(row.weekly_growth)),
            (Some(n), None) => format!("{n}{}", grown(row.weekly_growth)),
            (None, _) => "none published".to_owned(),
        };
        out.push_str(&format!(
            "{:<8} {:<14} {:<26} {}{}\n",
            row.channel,
            row.tag,
            weekly,
            row.downloads,
            grown(row.downloads_growth)
        ));
    }
    if earlier.is_some() {
        out.push_str("\nweekly users between the two listings, estimated\n");
        for channel in Channel::ALL {
            let sum: u64 = rows
                .iter()
                .filter(|row| row.channel == channel.name())
                .filter_map(|row| row.weekly_growth)
                .sum();
            out.push_str(&format!("  {}: {sum}\n", channel.name()));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two releases and the dev prerelease, as `gh api repos/{owner}/{repo}/releases` lists
    /// them, trimmed to the fields the estimate reads.
    const NOW: &str = r#"[
      {"tag_name":"dev","prerelease":true,"assets":[
        {"id":90,"name":"dev.json","download_count":400,"created_at":"2026-10-04T08:00:00Z"},
        {"id":91,"name":"dev-weekly.json","download_count":7,"created_at":"2026-10-04T08:00:00Z"},
        {"id":92,"name":"charter-linux-x86_64-appimage.AppImage","download_count":3,"created_at":"2026-10-04T08:00:00Z"}
      ]},
      {"tag_name":"v0.5.0","prerelease":false,"assets":[
        {"id":10,"name":"latest.json","download_count":900,"created_at":"2026-09-20T10:00:00Z"},
        {"id":11,"name":"latest-weekly.json","download_count":130,"created_at":"2026-09-20T10:00:00Z"},
        {"id":12,"name":"charter-darwin-aarch64.dmg","download_count":50,"created_at":"2026-09-20T10:00:00Z"},
        {"id":13,"name":"charter-darwin-aarch64.app.tar.gz.sig","download_count":5,"created_at":"2026-09-20T10:00:00Z"},
        {"id":14,"name":"charter-linux-x86_64-appimage.AppImage","download_count":20,"created_at":"2026-09-20T10:00:00Z"}
      ]},
      {"tag_name":"v0.4.0","prerelease":false,"assets":[
        {"id":1,"name":"latest.json","download_count":300,"created_at":"2026-08-01T10:00:00Z"},
        {"id":2,"name":"charter-darwin-aarch64.dmg","download_count":40,"created_at":"2026-08-01T10:00:00Z"}
      ]}
    ]"#;

    #[test]
    fn each_release_reports_its_weekly_manifest_count_and_its_downloads() {
        let rows = rows(NOW, None).expect("a listing");
        let seen: Vec<(&str, &str, Option<u64>, u64)> = rows
            .iter()
            .map(|r| (r.channel, r.tag.as_str(), r.weekly, r.downloads))
            .collect();
        assert_eq!(
            seen,
            [
                ("dev", "dev", Some(7), 3),
                ("stable", "v0.5.0", Some(130), 70),
                // Published before the weekly manifest existed: no count, not a zero.
                ("stable", "v0.4.0", None, 40),
            ]
        );
        assert_eq!(rows[0].since.as_deref(), Some("2026-10-04T08:00:00Z"));
    }

    #[test]
    fn against_an_earlier_listing_the_estimate_is_the_growth_of_each_weekly_count() {
        let earlier = NOW
            .replace(r#""download_count":130"#, r#""download_count":100"#)
            .replace(r#""download_count":50,"#, r#""download_count":45,"#)
            // The dev build was replaced since: a new asset, so its whole count is new.
            .replace(r#""id":91"#, r#""id":81"#);
        let rows = rows(NOW, Some(&earlier)).expect("two listings");
        let growth: Vec<(&str, Option<u64>, Option<u64>)> = rows
            .iter()
            .map(|r| (r.tag.as_str(), r.weekly_growth, r.downloads_growth))
            .collect();
        assert_eq!(
            growth,
            [
                ("dev", Some(7), Some(0)),
                ("v0.5.0", Some(30), Some(5)),
                ("v0.4.0", None, Some(0)),
            ]
        );
        let text = report(NOW, Some(&earlier)).unwrap();
        assert!(text.contains("stable: 30"), "{text}");
        assert!(text.contains("dev: 7"), "{text}");
    }

    #[test]
    fn gh_paginated_output_reads_as_one_listing() {
        // `gh api --paginate` writes one JSON array per page, back to back.
        let pages = format!(
            "{NOW}\n{}",
            r#"[{"tag_name":"v0.3.0","prerelease":false,"assets":[]}]"#
        );
        assert_eq!(rows(&pages, None).unwrap().len(), 4);
    }

    #[test]
    fn a_listing_that_is_not_one_is_refused_with_why() {
        assert!(rows("{}", None).is_err());
        assert!(rows("not json", None).is_err());
    }
}
