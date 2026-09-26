//! What the `charter` binary does for extensions once a command is done: tell the ones that
//! hear it (charter-app#343).
//!
//! **After the command's own answer is out.** Its lines are printed and stdout flushed before
//! anything is asked, and its exit status was decided before this was called, so nothing an
//! extension answers — or fails to — can change what the command said or how it ended. Each
//! failure is one line on stderr naming the extension. **The process does wait for it**, at
//! most one deadline and only when an extension that hears the event is slow: that is what
//! keeps the note in front of whoever ran the command (ADR 0041, amended 2026-09-25).
//!
//! A machine with no extension record pays one failed look for the config directory, and a
//! record with nothing hearing the event one manifest read per approved extension: the core's
//! `extension::events::deliver` asks nothing it does not have to.

use std::io::Write;
use std::path::Path;

use charter_core::executor::Executor;
use charter_core::extension::briefing::Bounds;
use charter_core::extension::events::{self, Event};
use charter_core::extension::project::Choices;

/// Set in a debug build, how long every extension's program is given, in milliseconds (#422):
/// the test suite's seam for the deadlines this binary arms. Compiled out of a release build,
/// which always gives [`charter_core::executor::DEADLINE`] and [`Bounds::SESSION_START`].
///
/// **Why a test needs it.** The deadlines are the product's — five seconds for an event or a
/// command, two for each extension at a chat's start and three for all of them (ADR 0041,
/// ADR 0053) — and a test extension is a program copied in fresh, which macOS assesses before
/// its first run. On a loaded machine that alone outlasts two seconds, so a test whose subject
/// is not the deadline gives a long one here; a test whose subject is the deadline gives a
/// short one and a program that never answers, and is fast and certain either way.
#[cfg(debug_assertions)]
const DEADLINE_ENV: &str = "CHARTER_TEST_EXTENSION_DEADLINE_MS";

/// The deadline the test suite set, in a debug build it set one in. A value that is not a
/// whole number of milliseconds panics rather than falling back to the real deadline, which
/// would bring back the very failure the seam exists to prevent.
fn test_deadline() -> Option<std::time::Duration> {
    #[cfg(debug_assertions)]
    {
        std::env::var(DEADLINE_ENV).ok().map(|ms| {
            std::time::Duration::from_millis(ms.parse().unwrap_or_else(|_| {
                panic!("{DEADLINE_ENV} is {ms:?}, not a whole number of milliseconds")
            }))
        })
    }
    #[cfg(not(debug_assertions))]
    {
        None
    }
}

/// The executor this binary starts an extension's program with — for an event it hears, and
/// for a command run from the command line.
pub fn executor() -> Executor {
    match test_deadline() {
        Some(deadline) => Executor::default().with_deadline(deadline),
        None => Executor::default(),
    }
}

/// How long `charter hook sessionstart` waits for extensions: [`Bounds::SESSION_START`], or,
/// in a test, the deadline it asked for for each, and the wait for all of them together kept
/// in the same proportion to it.
pub fn session_start_bounds() -> Bounds {
    let real = Bounds::SESSION_START;
    match test_deadline() {
        Some(each) => Bounds {
            each,
            total: each.mul_f64(real.total.as_secs_f64() / real.each.as_secs_f64()),
        },
        None => real,
    }
}

/// Tell every approved extension on in `root`'s project that hears it that `event` happened.
pub fn tell(root: &Path, event: &Event) {
    // `_if_there`: a machine that never made a config directory has no extension to tell, and
    // a command must not make one on its way out.
    let Some(config) = charter_core::machine::config_root_if_there() else {
        return;
    };
    let _ = std::io::stdout().flush();
    let choices = Choices::read_in(root, event.workspace());
    for note in events::deliver(&executor(), &config, &choices, event) {
        eprintln!("charter: {note}");
    }
}

/// The folders a fork carries for the extensions this machine approved.
pub fn carried() -> Vec<String> {
    charter_core::machine::config_root_if_there()
        .map(|config| events::carried(&config, &charter_core::extension::BuiltIn::none()))
        .unwrap_or_default()
}
