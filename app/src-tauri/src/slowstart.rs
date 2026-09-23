//! What charter says when it is taking too long to appear.
//!
//! A desktop app that is slow to start has nothing to show for it: no window, and on some
//! desktops not even an icon. charter-app#24 measured one such start — **26 to 31 seconds**
//! on a Linux session whose D-Bus session bus has a desktop portal that is activatable but
//! cannot come up. The whole wait is inside `tauri::Builder::build()`: GTK creates a proxy
//! for `org.freedesktop.portal.Desktop` with auto-start on, the bus tries to start
//! `xdg-desktop-portal`, that waits for a backend which needs a desktop session, and D-Bus
//! gives up after its default 25 s. WebKitGTK then asks the same portal for the colour
//! scheme and gives up after its own 5 s.
//!
//! **charter makes neither call**, and the one lever it has is process-wide — start with no
//! session bus — which would take single-instance, the tray and notifications with it. ADR
//! 0026's amendment records that trade and hands the fix to M4.
//!
//! What is left is the part charter owes the operator either way: **do not make them sit in
//! front of nothing.** So a launch that passes the limit says so, twice, on the only two
//! channels that exist:
//!
//! - on standard error while it is still waiting, which is what a terminal launch, a `.desktop`
//!   file's journal and a CI log have;
//! - on screen the moment there is a screen, which is what an operator who clicked an icon has.
//!
//! Neither is a fix, and neither is written as one.

use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::Duration;

/// The spec's cold-start limit (`docs/spec.md`, Limits;
/// ADR 0026 holds it at 2 s). A launch that passes it is a launch worth explaining.
pub const LIMIT: Duration = Duration::from_secs(2);

/// Waits for the launch to finish and says so on `say` if it does not, once, at `limit`.
///
/// Call it on a thread of its own: it is meant to run while the thread that builds the app is
/// inside a call that has not returned. `finished` going away counts as finished — a launch
/// that panicked has a panic to show for itself and does not need this as well.
pub fn while_it_waits(
    finished: &Receiver<()>,
    limit: Duration,
    os: &str,
    say: &mut dyn FnMut(&str),
) {
    if finished.recv_timeout(limit) != Err(RecvTimeoutError::Timeout) {
        return;
    }
    say(&still_starting(limit, os));
}

/// What is said while the wait is still going on.
fn still_starting(limit: Duration, os: &str) -> String {
    let mut said = format!(
        "charter: still starting — no window yet, {} after launch.",
        seconds(limit)
    );
    if let Some(cause) = the_known_cause(os) {
        said.push(' ');
        said.push_str(cause);
    }
    said
}

/// The one line the window puts on screen about a launch that took `took`, or nothing when
/// the launch was inside the limit and there is nothing to explain.
pub fn why(took: Duration, os: &str) -> Option<String> {
    if took <= LIMIT {
        return None;
    }
    let mut said = format!(
        "charter took {} to start, against a {} limit.",
        seconds(took),
        seconds(LIMIT)
    );
    if let Some(cause) = the_known_cause(os) {
        said.push(' ');
        said.push_str(cause);
    }
    Some(said)
}

/// The cause that has actually been measured, where it can apply — and nothing anywhere else.
///
/// It is hedged on purpose. A slow Linux start CAN be something else entirely (a cold disk,
/// a loaded machine), and a sentence that named the portal as a fact would be charter telling
/// an operator something it has not checked. What it has is the one cause that has been
/// traced, and the issue that holds the trace.
fn the_known_cause(os: &str) -> Option<&'static str> {
    (os == "linux").then_some(
        "The cause charter has measured on Linux is a desktop portal that cannot start: GTK \
         asks the session bus for org.freedesktop.portal.Desktop and D-Bus gives up after 25 \
         s, then WebKitGTK asks it for the colour scheme and gives up after 5 more. charter \
         makes neither call — charter-app#24.",
    )
}

/// A duration the way a person says one: `2 s`, `31.4 s`, `450 ms`.
fn seconds(took: Duration) -> String {
    if took < Duration::from_secs(1) {
        return format!("{} ms", took.as_millis());
    }
    let whole = took.as_secs_f64();
    if (whole - whole.round()).abs() < 0.05 {
        format!("{} s", whole.round() as u64)
    } else {
        format!("{whole:.1} s")
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use super::*;

    #[test]
    fn a_launch_inside_the_limit_says_nothing_at_all() {
        let (done, finished) = mpsc::channel();
        let mut said = Vec::new();

        done.send(()).expect("the receiver is alive");
        while_it_waits(&finished, LIMIT, "linux", &mut |line| {
            said.push(line.to_string())
        });

        assert!(said.is_empty(), "an ordinary launch said {said:?}");
    }

    #[test]
    fn a_launch_that_panicked_is_not_also_reported_as_slow() {
        // The sender is gone, which is what a `build()` that unwound looks like from here.
        // A panic already says what happened, in the app's own words.
        let (done, finished) = mpsc::channel::<()>();
        drop(done);
        let mut said = Vec::new();

        while_it_waits(&finished, LIMIT, "linux", &mut |line| {
            said.push(line.to_string())
        });

        assert!(said.is_empty(), "a panicked launch said {said:?}");
    }

    #[test]
    fn a_launch_still_going_at_the_limit_says_so_once_while_it_is_still_going() {
        let (done, finished) = mpsc::channel::<()>();
        let mut said = Vec::new();

        while_it_waits(&finished, Duration::from_millis(20), "macos", &mut |line| {
            said.push(line.to_string())
        });

        assert_eq!(said.len(), 1, "said {said:?}");
        assert!(
            said[0].contains("still starting") && said[0].contains("no window yet"),
            "{}",
            said[0]
        );
        // It is still running. A line in the past tense would be a lie about a launch that
        // has not happened.
        assert!(!said[0].contains("took"), "{}", said[0]);
        drop(done);
    }

    #[test]
    fn only_linux_is_told_the_cause_that_was_measured_on_linux() {
        let slow = Duration::from_secs(31);

        let linux = why(slow, "linux").expect("a slow launch is explained");
        let mac = why(slow, "macos").expect("a slow launch is explained");

        assert!(linux.contains("portal"), "{linux}");
        assert!(linux.contains("charter-app#24"), "{linux}");
        assert!(
            !mac.contains("portal"),
            "macOS was told about a wait it cannot have: {mac}"
        );
        assert!(mac.contains("31 s"), "{mac}");
    }

    #[test]
    fn a_launch_inside_the_limit_has_nothing_to_put_on_screen() {
        assert_eq!(why(Duration::from_millis(370), "macos"), None);
        assert_eq!(why(Duration::from_millis(370), "linux"), None);
        assert_eq!(why(LIMIT, "linux"), None, "exactly the limit is not a miss");
        assert!(
            why(LIMIT + Duration::from_millis(1), "linux").is_some(),
            "a millisecond over the limit is a miss"
        );
    }

    #[test]
    fn what_it_took_is_said_the_way_a_person_says_it() {
        assert_eq!(seconds(Duration::from_millis(450)), "450 ms");
        assert_eq!(seconds(Duration::from_secs(2)), "2 s");
        assert_eq!(seconds(Duration::from_millis(31_400)), "31.4 s");
        assert_eq!(seconds(Duration::from_millis(30_020)), "30 s");
    }
}
