//! **The limit launchd gives an app is raised before any chat opens** (ADR 0068 §9, SC-15).
//!
//! A test binary of its own, with one test in it, because it lowers and raises this process's
//! own `RLIMIT_NOFILE`: in a binary shared with other tests it would change what they run
//! under.
#![cfg(unix)]

use purlis_core::openfiles::{OPEN_MAX, Raised, raise};
use rustix::process::{Resource, Rlimit, getrlimit, setrlimit};

#[test]
fn a_process_started_with_launchds_256_raises_its_soft_limit_and_keeps_its_hard_one() {
    purlis_core::unsteered!();
    let given = getrlimit(Resource::Nofile);
    setrlimit(
        Resource::Nofile,
        Rlimit {
            current: Some(256),
            maximum: given.maximum,
        },
    )
    .expect("a soft limit can always be lowered");

    let raised = raise();

    let now = getrlimit(Resource::Nofile);
    let wanted = given.maximum.map_or(OPEN_MAX, |hard| hard.min(OPEN_MAX));
    assert_eq!(
        raised,
        Raised::From {
            from: 256,
            to: wanted
        }
    );
    assert_eq!(now.current, Some(wanted));
    assert_eq!(now.maximum, given.maximum, "the hard limit is not touched");
    // And a second start of the same process asks for nothing more.
    assert_eq!(raise(), Raised::AlreadyEnough(Some(wanted)));
}
