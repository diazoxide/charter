//! **No webview leaves the app** (FM-2 review): every navigation off the app's own origin is
//! refused, and so is every request for a new window.
//!
//! The window draws what agents write — a branch's markdown rendered in the file tab, with its
//! links — and a link is opened in the operator's browser through the opener plugin
//! (`ReleaseNotes.tsx`'s `ExternalLink`), never in the webview. This is the wall behind that: a
//! link that slips past it (a middle click, a form, a script's `location`) navigates nothing,
//! because a page from elsewhere loaded in the window would be one the IPC bridge answers.
//!
//! Standard Tauri hardening: a plugin's `on_navigation`, which every webview of the app runs, and
//! `on_new_window` on each window charter builds.

use tauri::plugin::{Builder, TauriPlugin};
use tauri::webview::{NewWindowFeatures, NewWindowResponse};
use tauri::{Manager, Runtime, Url};

/// Whether `url` is one of the app's own pages: its bundled frontend, as each platform's webview
/// serves it (`tauri://localhost`, `http(s)://tauri.localhost`), or — in a development build —
/// the dev server's origin.
pub(crate) fn stays_in_the_app(url: &Url, dev: Option<&Url>) -> bool {
    let own = match url.scheme() {
        "tauri" => url.host_str() == Some("localhost"),
        "http" | "https" => url.host_str() == Some("tauri.localhost"),
        _ => false,
    };
    own || dev.is_some_and(|dev| dev.origin() == url.origin())
}

/// The plugin that refuses every navigation off the app, in every webview.
pub(crate) fn plugin<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("navguard")
        .on_navigation(|webview, url| {
            let config = webview.config();
            let dev = config.build.dev_url.as_ref().filter(|_| tauri::is_dev());
            stays_in_the_app(url, dev)
        })
        .build()
}

/// A request for a new window — `target=_blank`, a middle click, `window.open` — is refused:
/// charter opens its own windows, and a link goes to the browser.
pub(crate) fn no_new_window<R: Runtime>(_: Url, _: NewWindowFeatures) -> NewWindowResponse<R> {
    NewWindowResponse::Deny
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(text: &str) -> Url {
        Url::parse(text).unwrap()
    }

    #[test]
    fn the_app_s_own_pages_are_allowed_on_every_platform() {
        for own in [
            "tauri://localhost/",
            "tauri://localhost/index.html#x",
            "http://tauri.localhost/",
            "https://tauri.localhost/assets/a.js",
        ] {
            assert!(stays_in_the_app(&url(own), None), "{own}");
        }
    }

    #[test]
    fn anywhere_else_is_refused() {
        for away in [
            "https://example.com/",
            "http://localhost:1420/",
            "https://tauri.localhost.evil.example/",
            "http://evil.example/?tauri.localhost",
            "tauri://evil/",
            "file:///etc/passwd",
            "data:text/html,<p>x</p>",
            "javascript:alert(1)",
            "about:blank",
            "blob:tauri://localhost/1",
        ] {
            assert!(!stays_in_the_app(&url(away), None), "{away}");
        }
    }

    #[test]
    fn a_development_build_also_allows_its_dev_server_and_only_its_origin() {
        let dev = url("http://localhost:1420");

        assert!(stays_in_the_app(
            &url("http://localhost:1420/x"),
            Some(&dev)
        ));
        assert!(!stays_in_the_app(
            &url("http://localhost:1421/"),
            Some(&dev)
        ));
        assert!(!stays_in_the_app(
            &url("https://localhost:1420/"),
            Some(&dev)
        ));
    }
}
