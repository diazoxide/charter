//! What the `charter` binary does for extensions once a command is done: tell the ones that
//! hear it (charter-app#343).
//!
//! **After the command's own answer is out.** Its lines are printed and stdout flushed before
//! anything is asked, and its exit status was decided before this was called, so nothing an
//! extension answers — or fails to — can change what the command said or how it ended. Each
//! failure is one line on stderr naming the extension.
//!
//! A machine with no extension record pays one failed look for the config directory, and a
//! record with nothing hearing the event one manifest read per approved extension: the core's
//! `extension::events::deliver` asks nothing it does not have to.

use std::io::Write;
use std::path::Path;

use charter_core::executor::Executor;
use charter_core::extension::events::{self, Event};
use charter_core::extension::project::Choices;

/// Tell every approved extension on in `root`'s project that hears it that `event` happened.
pub fn told(root: &Path, event: &Event) {
    // `_if_there`: a machine that never made a config directory has no extension to tell, and
    // a command must not make one on its way out.
    let Some(config) = charter_core::machine::config_root_if_there() else {
        return;
    };
    let _ = std::io::stdout().flush();
    let choices = Choices::read_in(root, event.workspace());
    for note in events::deliver(&Executor::default(), &config, &choices, event) {
        eprintln!("charter: {note}");
    }
}

/// The folders a fork carries for the extensions this machine approved.
pub fn carried() -> Vec<String> {
    charter_core::machine::config_root_if_there()
        .map(|config| events::carried(&config))
        .unwrap_or_default()
}
