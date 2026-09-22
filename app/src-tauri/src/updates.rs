//! The app moving itself: which channel, whether it may, and what it says while it does.
//!
//! [`charter_core::updates`] holds the rules — the two channels, their endpoints, and whether
//! the key this build carries is a key at all. This file is the part that needs a runner: it
//! points Tauri's updater at the channel's manifest, refuses before it asks when the build
//! cannot verify what it would be given, and runs the one background timer that makes the
//! whole thing automatic.
//!
//! # Nothing installs without the operator
//!
//! charter **checks** on its own and **installs** on a click. That is not timidity about
//! updating; it is what charter is. Every session in the app is a child of this process (ADR
//! 0025), and installing on macOS replaces the bundle and needs a relaunch — so a silent
//! install is charter ending the operator's day's work, from a timer, with no warning. An app
//! that owned no processes could reasonably swap itself while nobody was looking. This one
//! cannot, so it asks.
//!
//! The check emits [`CHECKED`]; the install emits [`INSTALLED`] or [`FAILED`]. **A window is
//! not required for any of it** — the timer runs whether or not one is showing, and the
//! notification is how an operator with the window hidden finds out.
//!
//! # Where the status line hooks in (M6.4)
//!
//! Nothing in this file draws anything. M6.4's status line (`app/src/StatusLine.tsx`, #153)
//! is the surface, and it landed while this was being written, so the wiring is a follow-up
//! rather than part of this change. When it is added, the status line listens for [`CHECKED`]
//! and shows the offer, calls [`install_update`] from it, and shows [`update_channel`] beside
//! it with a control that calls [`set_update_channel`]. The skew a plane's pin reports is a
//! different item and a different question: `charter version` answers it today (charter ADR
//! 0030), and the status line's item for it asks `adopt::version_report`, never a comparison
//! of its own. This module deliberately does not restate it.

use charter_core::updates::{Channel, NotAKey, pubkey_usable};
use tauri::{Emitter, Runtime};
use tauri_plugin_updater::UpdaterExt;

/// The event a finished check emits, carrying [`Offer`] or nothing.
pub const CHECKED: &str = "update://checked";
/// The event a finished install emits. The app has to be relaunched after it.
pub const INSTALLED: &str = "update://installed";
/// The event a check or an install that went wrong emits, carrying the sentence.
pub const FAILED: &str = "update://failed";

/// What a check found, for the window and for the notification.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct Offer {
    /// The version on offer.
    pub version: String,
    /// The version running now.
    pub current: String,
    /// The channel it came from, so a surface never has to guess which manifest was read.
    pub channel: String,
    /// The release notes, as the manifest carries them.
    pub notes: String,
}

/// Which stream this machine takes charter from, as the word charter writes down.
///
/// Read fresh rather than cached, and that is what makes a channel change take effect without
/// a relaunch: the endpoint is chosen at the moment of the check and never at startup.
pub fn channel_now() -> Channel {
    charter_core::machine::config_root()
        .map(|root| charter_core::machine::read(&root).store.channel)
        .unwrap_or_default()
}

/// Put this machine on `channel`.
///
/// Through [`charter_core::machine::update`], which holds the store's lock across the read and
/// the write — the app and a `charter` in a terminal are two processes writing one file, and
/// this is the field they are most likely to write at the same time.
pub fn set_channel(channel: Channel) -> Result<(), String> {
    let root = charter_core::machine::config_root()
        .ok_or("this machine has no config home, so charter has nowhere to keep the channel")?;
    charter_core::machine::update(&root, |store| store.channel = channel)
        .map(|_| ())
        .map_err(|why| format!("the channel could not be recorded: {why}"))
}

/// The public key this build was configured with, straight out of the running config.
///
/// Read from `app.config()` rather than from a constant, because the thing that must be
/// checked is what **Tauri** will verify against — a constant beside it could agree with the
/// source and disagree with the build.
fn configured_pubkey<R: Runtime>(app: &tauri::AppHandle<R>) -> String {
    app.config()
        .plugins
        .0
        .get("updater")
        .and_then(|updater| updater.get("pubkey"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

/// Whether this build can verify an update, answered before anything is asked for.
///
/// **The refusal is the point.** Tauri verifies the signature at the end of the download and
/// fails there, which means a build with a placeholder key announces a version, downloads it,
/// and only then says it cannot check it. This turns that into one sentence at the start, and
/// it is the only thing standing between "the operator forgot to replace the placeholder" and
/// "charter looks like it updates".
///
/// It cannot tell the operator's key from anyone else's — see
/// [`charter_core::updates::pubkey_usable`], which says so at length. What it catches is this
/// repository's own placeholder and a key pasted in wrong.
fn may_update<R: Runtime>(app: &tauri::AppHandle<R>) -> Result<(), String> {
    pubkey_usable(&configured_pubkey(app)).map_err(|not: NotAKey| not.why().to_owned())
}

/// Ask the channel's manifest whether there is a newer charter, and say what came back.
///
/// Every exit emits: [`CHECKED`] with an [`Offer`] or with `null`, or [`FAILED`] with the
/// sentence. A check that emitted nothing on one path would leave a status bar spinning.
pub async fn look<R: Runtime>(app: tauri::AppHandle<R>) {
    match offer(&app).await {
        Ok(found) => {
            if let Some(offer) = &found {
                tell_the_operator(&app, offer);
            }
            let _ = app.emit(CHECKED, found);
        }
        Err(why) => {
            let _ = app.emit(FAILED, why);
        }
    }
}

/// The check itself, with no emitting in it, so the decision and the telling are separable.
async fn offer<R: Runtime>(app: &tauri::AppHandle<R>) -> Result<Option<Offer>, String> {
    may_update(app)?;
    let channel = channel_now();
    let endpoint = channel
        .endpoint()
        .parse()
        .map_err(|why| format!("charter's update endpoint is not a url: {why}"))?;
    let updater = app
        .updater_builder()
        // **The endpoint, every time.** `tauri.conf.json` names the stable one so a build can
        // never be pointed at nothing, and this replaces it with the channel's — which is why
        // switching channels takes effect at the next check rather than at the next launch.
        .endpoints(vec![endpoint])
        .map_err(|why| format!("charter's update endpoint was refused: {why}"))?
        .build()
        .map_err(|why| format!("charter's updater could not be built: {why}"))?;
    let found = updater.check().await.map_err(|why| {
        format!(
            "charter could not reach the {} channel: {why}",
            channel.name()
        )
    })?;
    Ok(found.map(|update| Offer {
        version: update.version.clone(),
        current: update.current_version.clone(),
        channel: channel.name().to_owned(),
        notes: update.body.clone().unwrap_or_default(),
    }))
}

/// A notification, because the window is often hidden — closing it hides it (ADR 0025).
fn tell_the_operator<R: Runtime>(app: &tauri::AppHandle<R>, offer: &Offer) {
    use tauri_plugin_notification::NotificationExt;
    let _ = app
        .notification()
        .builder()
        .title(format!("charter {} is available", offer.version))
        .body(format!(
            "on the {} channel. Install it from charter, and charter will restart.",
            offer.channel
        ))
        .show();
}

/// Download the offered update and put it in place.
///
/// **Checks again rather than holding the earlier answer**, which costs one request and buys
/// two things: an install cannot be fired against an offer made hours ago on a channel the
/// operator has since changed, and there is no `Update` held across two commands whose
/// lifetime has to be reasoned about. The manifest is a static file; asking it twice is
/// cheap.
pub async fn install<R: Runtime>(app: tauri::AppHandle<R>) {
    let done = async {
        may_update(&app)?;
        let channel = channel_now();
        let endpoint = channel
            .endpoint()
            .parse()
            .map_err(|why| format!("charter's update endpoint is not a url: {why}"))?;
        let updater = app
            .updater_builder()
            .endpoints(vec![endpoint])
            .map_err(|why| format!("charter's update endpoint was refused: {why}"))?
            .build()
            .map_err(|why| format!("charter's updater could not be built: {why}"))?;
        let Some(update) = updater.check().await.map_err(|why| {
            format!(
                "charter could not reach the {} channel: {why}",
                channel.name()
            )
        })?
        else {
            return Err("there is no newer charter on this channel any more".to_owned());
        };
        // The signature is verified inside this call, over the bytes that were downloaded,
        // before anything is unpacked. Nothing here can turn that off and nothing here tries.
        update
            .download_and_install(|_, _| {}, || {})
            .await
            .map_err(|why| format!("charter {} could not be installed: {why}", update.version))?;
        Ok::<String, String>(update.version.clone())
    }
    .await;
    match done {
        Ok(version) => {
            let _ = app.emit(INSTALLED, version);
        }
        Err(why) => {
            let _ = app.emit(FAILED, why);
        }
    }
}

/// The timer that makes it automatic.
///
/// Started from `setup`, never awaited, and it holds nothing: a check that fails emits
/// [`FAILED`] and the next one comes round anyway, because a laptop that was on a train when
/// the timer fired is the ordinary case and not a fault worth remembering.
///
/// **A test build and a development build never start it** —
/// [`charter_core::updates::checks_on_its_own`] is the rule and its note is why.
pub fn watch<R: Runtime>(app: &tauri::AppHandle<R>) {
    if !charter_core::updates::checks_on_its_own(
        charter_core::fence::FENCED,
        cfg!(debug_assertions),
    ) {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(charter_core::updates::FIRST_CHECK_AFTER).await;
        loop {
            look(app.clone()).await;
            tokio::time::sleep(charter_core::updates::CHECK_EVERY).await;
        }
    });
}

/// Which channel this machine takes charter from: `stable` or `dev`.
#[tauri::command]
#[specta::specta]
pub fn update_channel() -> String {
    channel_now().name().to_owned()
}

/// Put this machine on a channel. A word charter does not know is refused, not guessed at.
#[tauri::command]
#[specta::specta]
pub fn set_update_channel(channel: String) -> Result<(), String> {
    let chosen = Channel::named(&channel)
        .ok_or_else(|| format!("{channel:?} is not an update channel (stable, dev)"))?;
    set_channel(chosen)
}

/// Look for a newer charter now. The answer arrives as [`CHECKED`] or [`FAILED`].
#[tauri::command]
#[specta::specta]
pub fn check_for_update(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(look(app));
}

/// Install the newer charter. The answer arrives as [`INSTALLED`] or [`FAILED`]; charter has
/// to be relaunched after it, and says so rather than doing it under the operator's sessions.
#[tauri::command]
#[specta::specta]
pub fn install_update(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(install(app));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `tauri.conf.json`, as the build reads it.
    fn config() -> serde_json::Value {
        let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tauri.conf.json"))
            .expect("tauri.conf.json is beside this crate");
        serde_json::from_str(&text).expect("tauri.conf.json is json")
    }

    #[test]
    fn the_shipped_config_points_the_updater_at_the_stable_channel_over_https() {
        // The endpoint in the file is the one a build falls back on if anything ever fails to
        // override it, so it must be the SAFE channel and not the rolling one.
        let updater = &config()["plugins"]["updater"];
        let endpoints = updater["endpoints"]
            .as_array()
            .expect("the updater config names endpoints");
        assert_eq!(endpoints.len(), 1, "one fallback, or it is not a fallback");
        assert_eq!(endpoints[0], Channel::Stable.endpoint());
        assert!(
            !endpoints[0].as_str().unwrap().contains("dev"),
            "the fallback endpoint is the dev channel"
        );
    }

    #[test]
    fn the_shipped_config_requires_a_signature_that_names_its_own_version() {
        // Off by default upstream, and the reason it is on here is a downgrade attack: the
        // manifest is not signed, so without this anyone who can serve one can pair a made-up
        // version with a genuine older release's url and signature.
        let updater = &config()["plugins"]["updater"];
        assert_eq!(
            updater["requireSignedVersion"],
            serde_json::Value::Bool(true),
            "requireSignedVersion is not on, and a downgrade is a valid signature"
        );
        // And the three escapes that would undo the rest of it are not taken anywhere.
        for dangerous in [
            "dangerousInsecureTransportProtocol",
            "dangerousAcceptInvalidCerts",
            "dangerousAcceptInvalidHostnames",
            "allowDowngrades",
        ] {
            assert!(
                updater.get(dangerous).is_none(),
                "{dangerous} is set in tauri.conf.json"
            );
        }
    }

    #[test]
    fn the_shipped_config_carries_a_pubkey_field_and_charter_judges_it_rather_than_tauri() {
        // Tauri REQUIRES the field — a config without it is a plugin that will not
        // deserialise, which is an app that does not start. So the placeholder has to be a
        // string, and the thing that has to refuse it is charter's own check.
        let config = config();
        let pubkey = config["plugins"]["updater"]["pubkey"]
            .as_str()
            .expect("the updater config carries a pubkey");
        assert!(
            !pubkey.is_empty(),
            "an empty pubkey is a config Tauri accepts"
        );
        match pubkey_usable(pubkey) {
            // Before the operator generates the keypair: the placeholder, refused by name.
            Err(NotAKey::Unset) => {
                assert_eq!(pubkey, charter_core::updates::PUBKEY_UNSET);
            }
            // After: a real key, and nothing in between is a state this repository may be in.
            Ok(()) => {}
            Err(other) => panic!(
                "tauri.conf.json's pubkey is neither the placeholder nor a key: {}",
                other.why()
            ),
        }
    }

    #[test]
    fn the_bundle_is_built_with_the_updater_artifacts_the_manifest_points_at() {
        // Without this the bundler writes a `.app` and a `.deb` and no `.app.tar.gz` — so
        // there is nothing for a manifest to name and nothing for it to sign. It is one
        // boolean and it is invisible until a release is cut.
        assert_eq!(
            config()["bundle"]["createUpdaterArtifacts"],
            serde_json::Value::Bool(true)
        );
    }

    #[test]
    fn the_version_the_bundle_carries_is_the_version_this_crate_was_built_as() {
        // `app.package_info().version` comes from this config, and the updater compares the
        // manifest against it. A config that drifted from Cargo would make the app compare
        // against a number nothing releases.
        assert_eq!(config()["version"], env!("CARGO_PKG_VERSION"));
    }
}
