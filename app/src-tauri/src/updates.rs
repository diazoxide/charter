//! The app moving itself: which channel, whether it may, and what it says while it does.
//!
//! [`purlis_core::updates`] holds the rules — the two channels, their endpoints, and whether
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
//! Nothing in this file draws anything. M6.4's status line (#153) is the surface:
//! `app/src/Updates.tsx` listens for [`CHECKED`] and shows the offer, calls [`install_update`]
//! from it, and shows [`update_channel`] beside it with a control that calls
//! [`set_update_channel`]. The skew a plane's pin reports is a
//! different item and a different question: `charter version` answers it today (charter ADR
//! 0030), and the status line's item for it asks `adopt::version_report`, never a comparison
//! of its own. This module deliberately does not restate it.

use std::sync::{Mutex, PoisonError};

use purlis_core::updates::{Channel, NotAKey, pubkey_usable};
use tauri::{Emitter, Manager, Runtime};
use tauri_plugin_updater::UpdaterExt;

/// The event a finished check emits, carrying [`Offer`] or nothing.
pub const CHECKED: &str = "update://checked";
/// The event a finished install emits. The app has to be relaunched after it.
pub const INSTALLED: &str = "update://installed";
/// The event a check or an install that went wrong emits, carrying the sentence.
pub const FAILED: &str = "update://failed";

/// The version this process has installed and not yet restarted into, if any.
///
/// Held so that [`restart_to_update`] refuses without one: the launch after that restart says
/// "charter restarted to install an update", and a restart that installed nothing would make
/// the one sentence it adds untrue.
#[derive(Default)]
pub struct Installed(Mutex<Option<String>>);

impl Installed {
    /// `version` is in place and runs from the next start.
    pub fn now(&self, version: &str) {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner) = Some(version.to_owned());
    }

    /// Nothing, when an update is waiting for a restart; otherwise the sentence saying there
    /// is none.
    pub fn ready(&self) -> Result<(), String> {
        match *self.0.lock().unwrap_or_else(PoisonError::into_inner) {
            Some(_) => Ok(()),
            None => {
                Err("no update has been installed, so there is nothing to restart into".to_owned())
            }
        }
    }
}

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
    purlis_core::machine::config_root()
        .map(|root| purlis_core::machine::read(&root).store.channel)
        .unwrap_or_default()
}

/// Put this machine on `channel`.
///
/// Through [`purlis_core::machine::update`], which holds the store's lock across the read and
/// the write — the app and a `charter` in a terminal are two processes writing one file, and
/// this is the field they are most likely to write at the same time.
pub fn set_channel(channel: Channel) -> Result<(), String> {
    let root = purlis_core::machine::config_root()
        .ok_or("this machine has no config home, so purlis has nowhere to keep the channel")?;
    purlis_core::machine::update(&root, |store| store.channel = channel)
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
/// [`purlis_core::updates::pubkey_usable`], which says so at length. What it catches is this
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
    let channel = channel_now();
    offer_from(
        app,
        channel,
        &channel.endpoint(),
        &channel.weekly_endpoint(),
    )
    .await
}

/// [`offer`], asking `endpoint` for `channel`'s manifest, or `weekly` for its weekly twin when
/// this is the machine's first check of the ISO week: the channel's own addresses in the app,
/// and a server on loopback in a test.
///
/// **The weekly count (V13, OB-17).** The weekly manifest has the manifest's bytes, so the
/// check is answered the same either way, and the request is the same from every machine: the
/// only trace it leaves is one more in GitHub's download count of that file. The week is
/// claimed in the machine store **before** the request, so a machine that cannot note it (no
/// config home, no store on this platform) is never counted, rather than counted at every
/// check. A request that got no answer, or was never sent, gives the week back, so a laptop
/// offline at its first check is counted at its next. A release published before the weekly manifest existed
/// answers 404 for it; the check then reads the manifest, and the next check tries again.
async fn offer_from<R: Runtime>(
    app: &tauri::AppHandle<R>,
    channel: Channel,
    endpoint: &str,
    weekly: &str,
) -> Result<Option<Offer>, String> {
    may_update(app)?;
    let mut found = None;
    if let Some(claim) = WeeklyClaim::take() {
        match ask(app, weekly).await {
            Ok(asked) if came_back(&asked) => found = Some(asked),
            // No answer, or a request that was never sent: the week was not counted.
            _ => claim.give_back(),
        }
    }
    let found = match found {
        Some(found) => found,
        None => ask(app, endpoint).await?,
    };
    let found = found.map_err(|why| {
        format!(
            "purlis could not reach the {} channel: {why}",
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

/// One updater request to `endpoint`, listed in the network log (OB-15): one of the only
/// Charter reads a run without an account makes. `Err` is a request that could not be built.
async fn ask<R: Runtime>(
    app: &tauri::AppHandle<R>,
    endpoint: &str,
) -> Result<Result<Option<tauri_plugin_updater::Update>, tauri_plugin_updater::Error>, String> {
    let url = endpoint
        .parse()
        .map_err(|why| format!("purlis's update endpoint is not a url: {why}"))?;
    let updater = app
        .updater_builder()
        // **The endpoint, every time.** `tauri.conf.json` names the stable one so a build can
        // never be pointed at nothing, and this replaces it with the channel's — which is why
        // switching channels takes effect at the next check rather than at the next launch.
        .endpoints(vec![url])
        .map_err(|why| format!("purlis's update endpoint was refused: {why}"))?
        .build()
        .map_err(|why| format!("purlis's updater could not be built: {why}"))?;
    let started = std::time::Instant::now();
    let found = updater.check().await;
    purlis_core::netlog::updater_read(endpoint, came_back(&found), started.elapsed());
    Ok(found)
}

/// This ISO week's weekly count, claimed in the machine store (OB-17).
struct WeeklyClaim {
    root: std::path::PathBuf,
    week: purlis_core::updates::IsoWeek,
    before: Option<purlis_core::updates::IsoWeek>,
}

impl WeeklyClaim {
    /// The claim, when this check is the machine's first of the week and the week could be
    /// noted; `None` otherwise, and always under `DO_NOT_TRACK`.
    fn take() -> Option<WeeklyClaim> {
        use purlis_core::updates::{DO_NOT_TRACK, IsoWeek, do_not_track, weekly_due};
        if do_not_track(std::env::var_os(DO_NOT_TRACK).as_deref()) {
            return None;
        }
        let root = purlis_core::machine::config_root()?;
        let week = IsoWeek::now();
        let mut claimed = None;
        purlis_core::machine::update(&root, |store| {
            if weekly_due(store.weekly, week, false) {
                claimed = Some(store.weekly);
                store.weekly = Some(week);
            }
        })
        .ok()?;
        claimed.map(|before| WeeklyClaim { root, week, before })
    }

    /// The request got no answer: the week was not counted, so the next check may count it.
    fn give_back(self) {
        let _ = purlis_core::machine::update(&self.root, |store| {
            if store.weekly == Some(self.week) {
                store.weekly = self.before;
            }
        });
    }
}

/// Whether an updater request was answered, for the network log: everything but a request
/// that never reached a server or never got a reply. A manifest or bundle refused for its
/// signature or its version was answered, and then refused here.
fn came_back<T>(result: &Result<T, tauri_plugin_updater::Error>) -> bool {
    use tauri_plugin_updater::Error;
    !matches!(
        result,
        Err(Error::Reqwest(_)
            | Error::Network(_)
            | Error::ReleaseNotFound
            | Error::InsecureTransportProtocol
            | Error::UrlParse(_)
            | Error::EmptyEndpoints)
    )
}

/// A notification, because the window is often hidden — closing it hides it (ADR 0025).
fn tell_the_operator<R: Runtime>(app: &tauri::AppHandle<R>, offer: &Offer) {
    use tauri_plugin_notification::NotificationExt;
    let _ = app
        .notification()
        .builder()
        .title(format!("purlis {} is available", offer.version))
        .body(format!(
            "on the {} channel. Install it from purlis, and purlis will restart.",
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
            .map_err(|why| format!("purlis's update endpoint is not a url: {why}"))?;
        let leaving = app.clone();
        let updater = app
            .updater_builder()
            .endpoints(vec![endpoint])
            .map_err(|why| format!("purlis's update endpoint was refused: {why}"))?
            // **Windows only**: there the plugin runs the installer and ends this process with
            // `std::process::exit`, which runs no exit event — so the install IS the restart,
            // and what Restart to update writes is written here instead (charter-app#251). The
            // installer starts charter again, and that launch asks as it would after a restart.
            // Nothing on macOS or Linux calls this.
            .on_before_exit(move || {
                leaving
                    .state::<crate::planes::Planes>()
                    .let_go_of_all_to_update();
            })
            .build()
            .map_err(|why| format!("purlis's updater could not be built: {why}"))?;
        let started = std::time::Instant::now();
        let found = updater.check().await;
        purlis_core::netlog::updater_read(
            &channel.endpoint(),
            came_back(&found),
            started.elapsed(),
        );
        let Some(update) = found.map_err(|why| {
            format!(
                "purlis could not reach the {} channel: {why}",
                channel.name()
            )
        })?
        else {
            return Err("there is no newer purlis on this channel any more".to_owned());
        };
        // The signature is verified inside this call, over the bytes that were downloaded,
        // before anything is unpacked. Nothing here can turn that off and nothing here tries.
        let started = std::time::Instant::now();
        let installed = update.download_and_install(|_, _| {}, || {}).await;
        // The signed bundle the manifest named. A bundle refused for its signature was
        // still downloaded, so it is listed as answered.
        purlis_core::netlog::updater_read(
            update.download_url.as_str(),
            came_back(&installed),
            started.elapsed(),
        );
        installed
            .map_err(|why| format!("purlis {} could not be installed: {why}", update.version))?;
        Ok::<String, String>(update.version.clone())
    }
    .await;
    match done {
        Ok(version) => {
            app.state::<Installed>().now(&version);
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
/// [`purlis_core::updates::checks_on_its_own`] is the rule and its note is why.
pub fn watch<R: Runtime>(app: &tauri::AppHandle<R>) {
    if !purlis_core::updates::checks_on_its_own(purlis_core::fence::FENCED, cfg!(debug_assertions))
    {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(purlis_core::updates::FIRST_CHECK_AFTER).await;
        loop {
            look(app.clone()).await;
            tokio::time::sleep(purlis_core::updates::CHECK_EVERY).await;
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

/// Restart into the update this process installed, and put back what was open
/// (charter-app#251).
///
/// Every plane's record is written first, saying it was written by a restart to update, and
/// every session is ended — [`crate::planes::Planes::let_go_of_all_to_update`] — so all of it
/// is on disk before the restart is asked for. The launch that follows asks #250's question,
/// with "charter restarted to install an update." in it and **Reopen all** in front, and a
/// launch that does not follow — the relaunch failed, the operator started charter by hand a
/// day later — reads the same records and asks the same question.
///
/// **Which chats are mid-turn is asked before this, by the window**, which is where each
/// chat's state is drawn (`Updates.tsx`). By the time this runs the operator has said to go.
///
/// Tauri's own restart, never one of charter's: `request_restart` runs the exit event first
/// (the single-instance plugin gives up its socket there, so the new process is not handed
/// straight back to this one), then starts the binary the bundle now names — on macOS read
/// from the new `Info.plist`, because an update may have renamed it.
#[tauri::command]
#[specta::specta]
pub fn restart_to_update(
    app: tauri::AppHandle,
    installed: tauri::State<'_, Installed>,
    planes: tauri::State<'_, crate::planes::Planes>,
) -> Result<(), String> {
    restarting(&installed, &planes, || app.request_restart())
}

/// [`restart_to_update`]'s order, with the restart handed in so a test can watch it: refused
/// before anything is touched, then everything written and ended, and only then the restart.
fn restarting(
    installed: &Installed,
    planes: &crate::planes::Planes,
    restart: impl FnOnce(),
) -> Result<(), String> {
    installed.ready()?;
    planes.let_go_of_all_to_update();
    restart();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Set in the child [`the_real_check_lists_its_read_in_the_network_log`] runs in.
    const NETLOG_CHILD: &str = "CHARTER_TEST_UPDATER_NETLOG_CHILD";

    /// A key that passes [`pubkey_usable`]'s shape check, for a check nothing installs from.
    const A_KEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXkgZm9yIGEgdGVzdApSV1FBQVFJREJBVUdCd2dKQ2dzTURRNFBFQkVTRXhRVkZoY1lHUm9iSEIwZUh5QWhJaU1rSlNZbgo=";

    /// A manifest server on loopback that answers each request with an older version than this
    /// one, and says how many requests it answered, each counted before its reply is written.
    fn manifest_server() -> (String, std::sync::Arc<std::sync::atomic::AtomicUsize>) {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let url = format!("http://{}/latest.json", listener.local_addr().unwrap());
        let served = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let count = served.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let mut stream = stream;
                let mut buf = [0u8; 4096];
                let _ = stream.read(&mut buf);
                // Counted before the reply is written: once the client has read the reply it
                // can assert on the count, so a count bumped after the write could lose that
                // race on a busy machine.
                count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let body = r#"{"version":"0.0.1","notes":"","pub_date":"2020-01-01T00:00:00Z","url":"http://127.0.0.1:1/x","signature":"x"}"#;
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
            }
        });
        (url, served)
    }

    /// OB-15: the update check the timer and the window run, `offer`'s own path, lists its read
    /// in the network log as the updater's. It runs in a child with a machine store of its own.
    #[test]
    fn the_real_check_lists_its_read_in_the_network_log() {
        if std::env::var_os(NETLOG_CHILD).is_none() {
            let store = tempfile::tempdir().expect("a machine store");
            purlis_core::testrun::rerun(
                &["updates::tests::the_real_check_lists_its_read_in_the_network_log"],
                &[
                    (NETLOG_CHILD, std::ffi::OsStr::new("1")),
                    (purlis_core::machine::HOME_VAR, store.path().as_os_str()),
                    ("XDG_CONFIG_HOME", std::ffi::OsStr::new("")),
                ],
            );
            let lines = purlis_core::netlog::entries(store.path());
            assert_eq!(lines.len(), 1, "{lines:?}");
            let line = &lines[0];
            assert_eq!(line.feature, purlis_core::netlog::Feature::Updater);
            assert!(line.host.starts_with("127.0.0.1:"), "{line:?}");
            assert_eq!((line.method.as_str(), line.answered), ("GET", true));
            return;
        }
        let (url, served) = manifest_server();
        let mut context = tauri_context!(test = true);
        context.config_mut().plugins.0.insert(
            "updater".into(),
            serde_json::json!({ "pubkey": A_KEY, "endpoints": [url] }),
        );
        let app = tauri::test::mock_builder()
            .plugin(tauri_plugin_updater::Builder::new().build())
            .build(context)
            .expect("the app builds with its updater");
        let found =
            tauri::async_runtime::block_on(offer_from(app.handle(), Channel::Stable, &url, &url));
        assert!(matches!(found, Ok(None)), "{found:?}");
        assert_eq!(served.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    /// Set in the children [`no_machine_sends_anything_of_its_own_in_the_weekly_check`] runs.
    const WEEKLY_CHILD: &str = "CHARTER_TEST_UPDATER_WEEKLY_CHILD";
    /// The manifest server's address, handed to those children.
    const WEEKLY_SERVER: &str = "CHARTER_TEST_UPDATER_WEEKLY_SERVER";

    /// A manifest server on loopback that answers every path with an older version, and keeps
    /// each request's bytes as they arrived.
    fn recording_server() -> (String, std::sync::Arc<Mutex<Vec<String>>>) {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let base = format!("http://{}", listener.local_addr().unwrap());
        let seen = std::sync::Arc::new(Mutex::new(Vec::new()));
        let keep = seen.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let mut stream = stream;
                let mut buf = vec![0u8; 16384];
                let n = stream.read(&mut buf).unwrap_or(0);
                keep.lock()
                    .unwrap()
                    .push(String::from_utf8_lossy(&buf[..n]).into_owned());
                let body = r#"{"version":"0.0.1","notes":"","pub_date":"2020-01-01T00:00:00Z","url":"http://127.0.0.1:1/x","signature":"x"}"#;
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
            }
        });
        (base, seen)
    }

    /// OB-17's privacy test. Three machines, each with a device id of its own, make two update
    /// checks each against one server. The first check of the week of each machine that has
    /// not opted out reads the weekly manifest, and that request is **byte for byte the same
    /// from every machine**: it carries no device id, no hostname, no user and nothing else
    /// that tells one machine from another, so nothing can link it across devices or weeks.
    /// The second check of the week reads the manifest, and a machine with `DO_NOT_TRACK` set
    /// never reads the weekly one.
    #[test]
    fn no_machine_sends_anything_of_its_own_in_the_weekly_check() {
        if std::env::var_os(WEEKLY_CHILD).is_none() {
            let (base, seen) = recording_server();
            let mut devices = Vec::new();
            for opted_out in ["", "", "1"] {
                let store = tempfile::tempdir().expect("a machine store");
                devices.push(purlis_core::machine::device_id(store.path()).expect("a device id"));
                purlis_core::testrun::rerun(
                    &["updates::tests::no_machine_sends_anything_of_its_own_in_the_weekly_check"],
                    &[
                        (WEEKLY_CHILD, std::ffi::OsStr::new("1")),
                        (WEEKLY_SERVER, std::ffi::OsStr::new(&base)),
                        (
                            purlis_core::updates::DO_NOT_TRACK,
                            std::ffi::OsStr::new(opted_out),
                        ),
                        (purlis_core::machine::HOME_VAR, store.path().as_os_str()),
                        ("XDG_CONFIG_HOME", std::ffi::OsStr::new("")),
                    ],
                );
                let kept = purlis_core::machine::read(store.path()).store.weekly;
                if opted_out.is_empty() {
                    assert_eq!(kept, Some(purlis_core::updates::IsoWeek::now()));
                } else {
                    assert_eq!(kept, None, "an opted-out machine noted a count");
                }
            }
            assert_ne!(devices[0], devices[1], "two machines, two device ids");
            let seen = seen.lock().unwrap().clone();
            let lines: Vec<&str> = seen
                .iter()
                .map(|r| r.lines().next().unwrap_or(""))
                .collect();
            assert_eq!(
                lines,
                [
                    "GET /latest-weekly.json HTTP/1.1",
                    "GET /latest.json HTTP/1.1",
                    "GET /latest-weekly.json HTTP/1.1",
                    "GET /latest.json HTTP/1.1",
                    "GET /latest.json HTTP/1.1",
                    "GET /latest.json HTTP/1.1",
                ],
                "{seen:#?}"
            );
            assert_eq!(seen[0], seen[2], "two machines' weekly checks differ");
            let host = gethostname();
            let user = std::env::var("USER").unwrap_or_default();
            for request in &seen {
                for device in &devices {
                    assert!(!request.contains(device.as_str()), "{request}");
                }
                assert!(host.is_empty() || !request.contains(&host), "{request}");
                assert!(user.len() < 3 || !request.contains(&user), "{request}");
                // Only headers that are the same on every machine.
                for header in request.lines().skip(1).take_while(|l| !l.is_empty()) {
                    let name = header.split(':').next().unwrap_or("").to_ascii_lowercase();
                    assert!(
                        ["host", "user-agent", "accept", "accept-encoding"]
                            .contains(&name.as_str()),
                        "an unexpected header: {header}"
                    );
                }
            }
            return;
        }
        let base = std::env::var(WEEKLY_SERVER).expect("the server");
        let mut context = tauri_context!(test = true);
        context.config_mut().plugins.0.insert(
            "updater".into(),
            serde_json::json!({ "pubkey": A_KEY, "endpoints": [format!("{base}/latest.json")] }),
        );
        let app = tauri::test::mock_builder()
            .plugin(tauri_plugin_updater::Builder::new().build())
            .build(context)
            .expect("the app builds with its updater");
        for _ in 0..2 {
            let found = tauri::async_runtime::block_on(offer_from(
                app.handle(),
                Channel::Stable,
                &format!("{base}/latest.json"),
                &format!("{base}/latest-weekly.json"),
            ));
            assert!(matches!(found, Ok(None)), "{found:?}");
        }
    }

    /// The machine's hostname, for the test above to look for.
    fn gethostname() -> String {
        let mut command = std::process::Command::new("hostname");
        purlis_core::forklock::output(&mut command)
            .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_owned())
            .unwrap_or_default()
    }

    /// A release published before the weekly manifest existed answers 404 for it. The check
    /// then reads the manifest, and the week is not noted, so the next check tries again.
    #[test]
    fn a_release_without_a_weekly_manifest_still_answers_the_check() {
        if std::env::var_os(WEEKLY_CHILD).is_none() {
            let store = tempfile::tempdir().expect("a machine store");
            purlis_core::testrun::rerun(
                &["updates::tests::a_release_without_a_weekly_manifest_still_answers_the_check"],
                &[
                    (WEEKLY_CHILD, std::ffi::OsStr::new("1")),
                    (purlis_core::updates::DO_NOT_TRACK, std::ffi::OsStr::new("")),
                    (purlis_core::machine::HOME_VAR, store.path().as_os_str()),
                    ("XDG_CONFIG_HOME", std::ffi::OsStr::new("")),
                ],
            );
            assert_eq!(purlis_core::machine::read(store.path()).store.weekly, None);
            return;
        }
        let (url, served) = manifest_server();
        let missing = std::net::TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let missing_url = format!(
            "http://{}/latest-weekly.json",
            missing.local_addr().unwrap()
        );
        std::thread::spawn(move || {
            use std::io::{Read, Write};
            for mut stream in missing.incoming().flatten() {
                let mut buf = [0u8; 4096];
                let _ = stream.read(&mut buf);
                let _ = write!(
                    stream,
                    "HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\nconnection: close\r\n\r\n"
                );
            }
        });
        let mut context = tauri_context!(test = true);
        context.config_mut().plugins.0.insert(
            "updater".into(),
            serde_json::json!({ "pubkey": A_KEY, "endpoints": [url] }),
        );
        let app = tauri::test::mock_builder()
            .plugin(tauri_plugin_updater::Builder::new().build())
            .build(context)
            .expect("the app builds with its updater");
        let found = tauri::async_runtime::block_on(offer_from(
            app.handle(),
            Channel::Stable,
            &url,
            &missing_url,
        ));
        assert!(matches!(found, Ok(None)), "{found:?}");
        assert_eq!(served.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    /// A weekly request that could not even be built (here, an address that is not a url) sent
    /// nothing, so the week is given back for the next check, and this one reads the manifest.
    #[test]
    fn a_weekly_request_that_was_never_sent_gives_the_week_back() {
        if std::env::var_os(WEEKLY_CHILD).is_none() {
            let store = tempfile::tempdir().expect("a machine store");
            purlis_core::testrun::rerun(
                &["updates::tests::a_weekly_request_that_was_never_sent_gives_the_week_back"],
                &[
                    (WEEKLY_CHILD, std::ffi::OsStr::new("1")),
                    (purlis_core::updates::DO_NOT_TRACK, std::ffi::OsStr::new("")),
                    (purlis_core::machine::HOME_VAR, store.path().as_os_str()),
                    ("XDG_CONFIG_HOME", std::ffi::OsStr::new("")),
                ],
            );
            assert_eq!(purlis_core::machine::read(store.path()).store.weekly, None);
            return;
        }
        let (url, _) = manifest_server();
        let mut context = tauri_context!(test = true);
        context.config_mut().plugins.0.insert(
            "updater".into(),
            serde_json::json!({ "pubkey": A_KEY, "endpoints": [url] }),
        );
        let app = tauri::test::mock_builder()
            .plugin(tauri_plugin_updater::Builder::new().build())
            .build(context)
            .expect("the app builds with its updater");
        let found = tauri::async_runtime::block_on(offer_from(
            app.handle(),
            Channel::Stable,
            &url,
            "not a url",
        ));
        // And the check itself still reads the manifest.
        assert!(matches!(found, Ok(None)), "{found:?}");
    }

    /// `tauri.conf.json`, as the build reads it.
    fn config() -> serde_json::Value {
        let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tauri.conf.json"))
            .expect("tauri.conf.json is beside this crate");
        serde_json::from_str(&text).expect("tauri.conf.json is json")
    }

    #[test]
    fn a_restart_to_update_is_refused_until_an_update_is_installed() {
        // The launch after it would say "charter restarted to install an update", and that
        // sentence is only ever true when one was.
        let installed = Installed::default();
        assert!(installed.ready().is_err());

        installed.now("0.2.0");

        assert_eq!(installed.ready(), Ok(()));
    }

    #[test]
    fn a_restart_to_update_touches_nothing_without_an_update_and_restarts_only_last() {
        let dir = tempfile::tempdir().expect("a directory");
        let config = dir.path().join("config");
        let root = dir.path().join("plane");
        std::fs::create_dir_all(&root).expect("the plane's directory");
        std::fs::write(root.join(purlis_core::plane::MANIFEST), "").expect("its charter.toml");
        let planes = crate::planes::Planes::telling(
            std::sync::Arc::new(|_: crate::hooks::Moved| {}),
            crate::Shipped::default(),
            Some(config.clone()),
        );
        let plane = planes.open(&root);
        let installed = Installed::default();
        let mut restarted = false;

        assert!(restarting(&installed, &planes, || restarted = true).is_err());
        assert!(!restarted, "it restarted with no update installed");
        assert!(
            planes.held(&plane).is_ok(),
            "a refused restart let go of a plane"
        );
        assert!(
            !purlis_core::reopen::take_restart_to_update(&config),
            "a refused restart left word of a restart"
        );

        installed.now("0.2.0");
        let mut held_at_the_restart = None;
        restarting(&installed, &planes, || {
            held_at_the_restart = Some(planes.open_now().len());
        })
        .expect("it restarts");

        assert_eq!(
            held_at_the_restart,
            Some(0),
            "the restart was asked for before every plane was written and let go of"
        );
        assert!(purlis_core::reopen::take_restart_to_update(&config));
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
                assert_eq!(pubkey, purlis_core::updates::PUBKEY_UNSET);
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
