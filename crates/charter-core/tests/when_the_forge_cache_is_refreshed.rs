//! The policy that decides WHEN a forge refresh runs — charter-app#69.
//!
//! `cistate` reads the cache and `glrefresh` writes it; until `glstate` nothing decided to run
//! one, so the app's CI column showed whatever the last `charter gl-refresh` typed by hand had
//! left. These are the three brakes and the lock, each one on its own, plus the two that
//! matter together: a refresh happens when the policy says it should, does not when it should
//! not, and two triggers in quick succession do not start two refreshes.

use std::path::{Path, PathBuf};

use charter_core::glstate::{
    self, Decided, NO_BACKGROUND_CHECKS, REFRESH_TTL, Refreshing, SPAWN_COOLDOWN, STUCK_AFTER, When,
};
use serde_json::json;

/// A plane with a `.charter/cache/`, and the trees a refresh would fetch for.
struct Rig {
    dir: tempfile::TempDir,
}

impl Rig {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("a plane");
        std::fs::create_dir_all(dir.path().join(".charter/cache")).expect("a cache directory");
        Self { dir }
    }

    fn root(&self) -> PathBuf {
        std::fs::canonicalize(self.dir.path()).expect("a resolved plane")
    }

    /// One tree, under the plane, spelled the way `glrefresh::trees` would spell it.
    fn tree(&self, name: &str) -> PathBuf {
        self.root().join("workspaces/alpha").join(name)
    }

    /// What a refresh at `stamped` would have left in the cache for `trees`.
    fn cached(&self, trees: &[(&PathBuf, f64)]) {
        let mut cache = serde_json::Map::new();
        for (tree, stamped) in trees {
            cache.insert(
                charter_core::glrefresh::key_for(tree),
                json!({"branch": "main", "ts": stamped, "change": null, "ci": "success", "sigil": "#"}),
            );
        }
        std::fs::write(
            self.root().join(charter_core::glrefresh::CACHE),
            serde_json::to_string(&cache).expect("a cache"),
        )
        .expect("the cache is written");
    }

    /// The lock as it would be `age` seconds after a refresh was spawned (`Some(pid)`) or
    /// finished (`None`). A NEGATIVE age is a lock stamped in the future, which is what a
    /// clock that moved backwards leaves behind.
    fn locked(&self, pid: Option<u32>, age: f64) {
        charter_core::glrefresh::write_lock(&self.root(), pid);
        let ago = std::time::Duration::from_secs_f64(age.abs());
        let when = if age >= 0.0 {
            std::time::SystemTime::now() - ago
        } else {
            std::time::SystemTime::now() + ago
        };
        filetime(&self.root().join(charter_core::glrefresh::LOCK), when);
    }
}

/// Set a file's modification time. The lock's MTIME is half of what it records, so a test
/// that could not move it could only ever exercise "just now".
fn filetime(path: &Path, when: std::time::SystemTime) {
    let seconds = when
        .duration_since(std::time::UNIX_EPOCH)
        .expect("after the epoch")
        .as_secs();
    // `touch -d @<epoch>` on GNU, `touch -t` on BSD — so go through Rust instead, which needs
    // no coreutils dialect: open the file and set the times through `rustix`.
    let file = std::fs::OpenOptions::new()
        .write(true)
        .open(path)
        .expect("the lock is there");
    let stamp = rustix::fs::Timestamps {
        last_access: rustix::fs::Timespec {
            tv_sec: seconds as _,
            tv_nsec: 0,
        },
        last_modification: rustix::fs::Timespec {
            tv_sec: seconds as _,
            tv_nsec: 0,
        },
    };
    rustix::fs::futimens(&file, &stamp).expect("the lock's mtime moves");
}

fn now() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("after the epoch")
        .as_secs_f64()
}

/// The decision, with no brake set in the environment and nothing alive.
fn decide(rig: &Rig, trees: &[PathBuf]) -> Decided {
    decide_with(rig, trees, &|_| false, &|_| None)
}

/// The decision, with `alive` and the environment said out loud.
fn decide_with(
    rig: &Rig,
    trees: &[PathBuf],
    alive: &dyn Fn(u32) -> bool,
    env: &dyn Fn(&str) -> Option<String>,
) -> Decided {
    glstate::decide(&When {
        plane: &rig.root(),
        trees,
        now: now(),
        env,
        alive,
    })
}

// --- the cache's age ------------------------------------------------------------------- //

#[test]
fn a_plane_nothing_has_ever_fetched_is_due_a_refresh() {
    // The state the issue is about: the column says "nothing has fetched this checkout", and
    // on a plane where nobody typed `charter gl-refresh` it said it for ever.
    let rig = Rig::new();
    let tree = rig.tree("svc");

    assert_eq!(decide(&rig, &[tree]), Decided::Due);
}

#[test]
fn a_cache_inside_the_refresh_window_is_not_fetched_again() {
    let rig = Rig::new();
    let tree = rig.tree("svc");
    rig.cached(&[(&tree, now() - 10.0)]);

    assert_eq!(decide(&rig, &[tree]), Decided::FreshEnough);
}

#[test]
fn an_entry_older_than_the_refresh_window_is_due() {
    let rig = Rig::new();
    let tree = rig.tree("svc");
    rig.cached(&[(&tree, now() - REFRESH_TTL.as_secs_f64() - 1.0)]);

    assert_eq!(decide(&rig, &[tree]), Decided::Due);
}

#[test]
fn one_stale_tree_out_of_many_is_enough_to_refresh_them_all() {
    // Python refreshes the whole workspace or none of it, and the reason is `workspace.
    // repo_trees`: "a repo can never be drawn without its forge state having been fetched".
    let rig = Rig::new();
    let (fresh, old) = (rig.tree("svc"), rig.tree("web"));
    rig.cached(&[
        (&fresh, now() - 10.0),
        (&old, now() - REFRESH_TTL.as_secs_f64() - 1.0),
    ]);

    assert_eq!(decide(&rig, &[fresh, old]), Decided::Due);
}

#[test]
fn a_tree_the_cache_has_never_heard_of_is_stale_however_fresh_its_neighbours_are() {
    let rig = Rig::new();
    let (known, added) = (rig.tree("svc"), rig.tree("just-cloned"));
    rig.cached(&[(&known, now() - 10.0)]);

    assert_eq!(decide(&rig, &[known, added]), Decided::Due);
}

#[test]
fn a_workspace_with_no_trees_refreshes_nothing() {
    // `any([])` is false in Python too. A workspace with no clone has nothing a forge could
    // be asked about, and spawning there would be a forge process per focus for ever.
    let rig = Rig::new();

    assert_eq!(decide(&rig, &[]), Decided::FreshEnough);
}

// --- the cooldown, and the lock ---------------------------------------------------------- //

#[test]
fn two_triggers_in_quick_succession_do_not_start_two_refreshes() {
    // The cooldown, and the thing it is for: the operator clicking between two workspaces, or
    // a panel asked twice, is one refresh and not two forge processes holding a credential.
    let rig = Rig::new();
    let tree = rig.tree("svc");
    rig.locked(Some(4242), 10.0);

    assert!(matches!(decide(&rig, &[tree]), Decided::CoolingDown { .. }));
}

#[test]
fn the_cooldown_runs_from_the_last_completion_as_well_as_the_last_spawn() {
    // charter#324: the mtime alone said "a refresh was STARTED 120 s ago", which is not a
    // reason to skip another. `refresh` rewrites the lock when the work is OVER, and that is
    // what arms the cooldown — so a refresh that has just finished suppresses the next
    // trigger, with no pid in the file at all.
    let rig = Rig::new();
    let tree = rig.tree("svc");
    charter_core::glrefresh::write_lock(&rig.root(), Some(4242));
    // The refresh finishes. `refresh` over no trees does the writing and nothing else.
    charter_core::glrefresh::refresh(&rig.root(), &[], now());

    let found = decide_with(&rig, std::slice::from_ref(&tree), &|_| true, &|_| None);

    assert!(
        matches!(found, Decided::CoolingDown { .. }),
        "a refresh that just finished did not arm the cooldown: {found:?}"
    );
    assert_eq!(
        glstate::in_flight(&rig.root(), now())
            .expect("the lock is readable")
            .expect("there is a lock")
            .0,
        None,
        "a finished refresh still claims to be in flight"
    );
}

#[test]
fn past_the_cooldown_a_refresh_that_is_still_running_suppresses_a_second() {
    // What suppresses a refresh is that the first is STILL RUNNING — not that one was started
    // two minutes ago. Without this arm a wedged refresh invited a replacement every 120 s for
    // as long as the app stayed open, each one holding the forge credential.
    let rig = Rig::new();
    let tree = rig.tree("svc");
    rig.locked(Some(4242), SPAWN_COOLDOWN.as_secs_f64() + 10.0);

    let found = decide_with(&rig, &[tree], &|pid| pid == 4242, &|_| None);

    assert!(
        matches!(found, Decided::AlreadyRefreshing { pid: 4242, .. }),
        "got {found:?}"
    );
}

#[test]
fn a_refresh_whose_process_is_gone_does_not_suppress_the_next_one() {
    // The pid in the lock is how "still running" is asked. A refresh that crashed leaves its
    // pid behind, and the answer has to be the process, not the file.
    let rig = Rig::new();
    let tree = rig.tree("svc");
    rig.locked(Some(4242), SPAWN_COOLDOWN.as_secs_f64() + 10.0);

    assert_eq!(
        decide_with(&rig, &[tree], &|_| false, &|_| None),
        Decided::Due
    );
}

#[test]
fn a_refresh_wedged_past_the_stuck_window_is_replaced_although_it_is_alive() {
    // Three times the refresh window: a refresh still running after this would land data that
    // was already stale several times over. It is also what stops a RECYCLED pid suppressing
    // refreshes for ever — which is the failure the window bounds.
    let rig = Rig::new();
    let tree = rig.tree("svc");
    rig.locked(Some(4242), STUCK_AFTER.as_secs_f64() + 10.0);

    assert_eq!(
        decide_with(&rig, &[tree], &|_| true, &|_| None),
        Decided::Due
    );
}

#[test]
fn a_lock_naming_no_pid_is_the_plain_cooldown_and_nothing_more() {
    // A truncated write, a hand edit, or a pid from another machine all read as "no pid",
    // which degrades to the cooldown rather than to a refusal.
    let rig = Rig::new();
    let tree = rig.tree("svc");

    rig.locked(None, 10.0);
    assert!(matches!(
        decide_with(&rig, std::slice::from_ref(&tree), &|_| true, &|_| None),
        Decided::CoolingDown { .. }
    ));

    rig.locked(None, SPAWN_COOLDOWN.as_secs_f64() + 10.0);
    assert_eq!(
        decide_with(&rig, &[tree], &|_| true, &|_| None),
        Decided::Due,
        "a lock with no pid may not suppress past the cooldown"
    );
}

#[test]
fn a_lock_whose_content_is_not_a_pid_charter_wrote_names_no_process() {
    let rig = Rig::new();
    let path = rig.root().join(charter_core::glrefresh::LOCK);
    for content in ["not-a-pid", "+5", "0", "", "  ", "12 34"] {
        std::fs::write(&path, content).expect("the lock is written");

        assert_eq!(
            glstate::in_flight(&rig.root(), now())
                .expect("the lock is readable")
                .expect("there is a lock")
                .0,
            None,
            "{content:?} was read as a pid"
        );
    }
    std::fs::write(&path, " 4242\n").expect("the lock is written");
    assert_eq!(
        glstate::in_flight(&rig.root(), now())
            .expect("the lock is readable")
            .expect("there is a lock")
            .0,
        Some(4242),
        "a pid charter wrote is read back"
    );
}

#[test]
fn a_clock_that_moved_backwards_does_not_suppress_every_refresh_after_it() {
    // A lock stamped in the FUTURE would otherwise read as an age of minus something, which is
    // inside every window there is, for as long as the clock stays behind.
    let rig = Rig::new();
    let tree = rig.tree("svc");
    rig.locked(Some(4242), -10_000.0);

    let (_pid, age) = glstate::in_flight(&rig.root(), now())
        .expect("the lock is readable")
        .expect("there is a lock");
    assert_eq!(age, 0.0, "an age is never negative");
    assert!(matches!(
        decide_with(&rig, &[tree], &|_| false, &|_| None),
        Decided::CoolingDown { .. }
    ));
}

// --- the two brakes that are not about time ---------------------------------------------- //

#[test]
fn the_operators_own_brake_stops_it_before_the_lock_or_the_cache_is_read() {
    // `$CHARTER_NO_BACKGROUND_CHECKS` is a request not to phone home, and it is answered
    // FIRST: on a plane that has never been refreshed, where every other brake is off.
    let rig = Rig::new();
    let tree = rig.tree("svc");

    let found = decide_with(&rig, &[tree], &|_| false, &|name| {
        if name == NO_BACKGROUND_CHECKS {
            Some("1".to_owned())
        } else {
            None
        }
    });

    assert_eq!(found, Decided::TurnedOff);
}

#[cfg(unix)]
#[test]
fn a_lock_reached_through_a_link_suppresses_rather_than_spawns() {
    // charter's own path under `.charter/` may not be a link — `cistate` keeps the same gate
    // on the file beside this one. Suppress and not spawn: the cost of skipping a refresh is
    // a stale column, and the cost of getting this wrong is the pile-up the lock prevents.
    let rig = Rig::new();
    let tree = rig.tree("svc");
    let outside = tempfile::tempdir().expect("somewhere else");
    std::fs::write(outside.path().join("theirs"), "4242").expect("their file");
    std::os::unix::fs::symlink(
        outside.path().join("theirs"),
        rig.root().join(charter_core::glrefresh::LOCK),
    )
    .expect("the link");

    assert!(matches!(
        decide(&rig, &[tree]),
        Decided::LockUnreadable { .. }
    ));
}

// --- the spawn itself --------------------------------------------------------------------- //

/// A stand-in `charter` that records the arguments and the plane it was handed, then sleeps
/// so it is still in flight when the second trigger asks.
fn stand_in(at: &Path) -> PathBuf {
    let binary = at.join("charter-stand-in");
    std::fs::write(
        &binary,
        "#!/bin/sh\nprintf '%s %s|%s\\n' \"$1\" \"$3\" \"$CHARTER_ROOT\" \
         >> \"$(dirname \"$0\")/ran\"\nexec sleep 30\n",
    )
    .expect("the stand-in is written");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755))
            .expect("it is runnable");
    }
    binary
}

#[cfg(unix)]
#[test]
fn a_refresh_is_spawned_once_and_the_next_trigger_finds_it_in_flight() {
    // **The whole feature, end to end.** Focus a workspace whose cache is empty and a refresh
    // starts, keyed to that workspace and to this plane; focus it again and nothing else does.
    let rig = Rig::new();
    let outside = tempfile::tempdir().expect("somewhere for the stand-in");
    let binary = stand_in(outside.path());
    let tree = rig.tree("svc");
    let root = rig.root();

    let first = glstate::maybe_spawn(&root, "alpha", std::slice::from_ref(&tree), &binary);
    let second = glstate::maybe_spawn(&root, "alpha", &[tree], &binary);

    let Refreshing::Started { pid } = first else {
        panic!("the first trigger did not start a refresh: {first:?}");
    };
    assert!(
        matches!(second, Refreshing::Declined(Decided::CoolingDown { .. })),
        "the second trigger started another refresh: {second:?}"
    );
    // The lock names the process that is doing the work, not merely that one was started —
    // which is what makes "is it still running" answerable at all.
    assert_eq!(
        glstate::in_flight(&root, now())
            .expect("the lock is readable")
            .expect("there is a lock")
            .0,
        Some(pid)
    );
    let ran = until_written(&outside.path().join("ran"));
    assert_eq!(
        ran.lines().count(),
        1,
        "one trigger, one refresh, and this ran {ran:?}"
    );
    assert_eq!(
        ran.trim(),
        format!("gl-refresh alpha|{}", root.display()),
        "the refresh was not keyed to the workspace and the plane it was asked for"
    );
    let _ = rustix::process::kill_process(
        rustix::process::Pid::from_raw(pid as i32).expect("a pid"),
        rustix::process::Signal::KILL,
    );
}

#[cfg(unix)]
#[test]
fn a_spawn_that_fails_does_not_arm_the_cooldown() {
    // A transient failure must not suppress the next trigger's retry — Python returns before
    // `_write_lock` for exactly this. Without it, a `charter` binary that went missing for one
    // moment would leave the column empty for two minutes at a time.
    let rig = Rig::new();
    let tree = rig.tree("svc");
    let root = rig.root();

    let refused = glstate::maybe_spawn(&root, "alpha", &[tree], Path::new("/definitely/not/here"));

    assert!(
        matches!(refused, Refreshing::NotStarted { .. }),
        "got {refused:?}"
    );
    assert_eq!(
        glstate::in_flight(&root, now()).expect("the lock is readable"),
        None,
        "a spawn that never happened armed the cooldown"
    );
}

/// The stand-in's own record, once it has written one.
fn until_written(path: &Path) -> String {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        if let Ok(text) = std::fs::read_to_string(path)
            && !text.is_empty()
        {
            // Give a second line the chance to arrive, so "exactly one" is not merely "the
            // first one got there first".
            std::thread::sleep(std::time::Duration::from_millis(200));
            return std::fs::read_to_string(path).unwrap_or(text);
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the refresh never ran"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}
