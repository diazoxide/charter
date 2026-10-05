//! `rename-local` (RN-5, V93f) on a temporary home: this machine's local state moves to the
//! purlis names, every move is journalled, the undo gives back the before-state byte for byte,
//! and a move that fails leaves the old name in place and read.
//!
//! Driven through the core's public surface — [`renamelocal::run`] and [`renamelocal::undo`]
//! on a [`Local`] naming the temporary home, and the readers every other caller uses
//! (`machine`, `halt`, `names`) — and asserted on what is on disk afterwards.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use charter_core::machine::{self, Consent, Contribution};
use charter_core::names;
use charter_core::renamelocal::{self, Local, Logs, Seams};

/// A machine with every old name: a config home holding approvals, a pin, a device id, the dev
/// channel and a pulled kill switch; the session host's folder; a data home; a log folder; and
/// one project in git with its local settings and state folder.
struct Machine {
    _dir: tempfile::TempDir,
    home: PathBuf,
    plane: PathBuf,
    local: Local,
    device: String,
}

fn git(dir: &Path, args: &[&str]) -> std::process::Output {
    charter_core::forklock::output(
        Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null"),
    )
    .expect("git runs")
}

fn machine() -> Machine {
    let dir = tempfile::tempdir().unwrap();
    let home = std::fs::canonicalize(dir.path()).unwrap();
    let plane = home.join("work/plane");
    std::fs::create_dir_all(&plane).unwrap();
    assert!(git(&plane, &["init", "-q"]).status.success());
    std::fs::write(plane.join("charter.toml"), "schema = 1\n").unwrap();
    std::fs::write(plane.join("charter.local.toml"), "[harness]\nuse = \"x\"\n").unwrap();
    std::fs::create_dir_all(plane.join(".charter/app")).unwrap();
    std::fs::write(plane.join(".charter/app/reopen.json"), "{}\n").unwrap();
    std::fs::write(
        plane.join(".gitignore"),
        "/.charter/\n/charter.local.toml\n",
    )
    .unwrap();

    let config_root = home.join(".config");
    std::fs::create_dir_all(&config_root).unwrap();
    machine::update(&config_root, |store| {
        store.remember(&plane, 100);
        store.approve(&plane, 100, Contribution::of(&plane));
        store.pin(&plane, true).unwrap();
        store.channel = charter_core::updates::Channel::Dev;
    })
    .unwrap();
    let device = machine::device_id(&config_root).unwrap();
    charter_core::halt::stop(&config_root, charter_core::halt::Actor::Cli, 7).unwrap();
    let daemon = machine::dir(&config_root).join("charterd");
    std::fs::create_dir_all(&daemon).unwrap();
    std::fs::write(daemon.join("scope"), "credential\n").unwrap();

    let data_base = home.join("data");
    std::fs::create_dir_all(data_base.join("charter/events")).unwrap();
    std::fs::write(data_base.join("charter/events/log"), "event\n").unwrap();
    let logs = Logs {
        old: home.join("Logs/dev.charter.app"),
        new: home.join("Logs/dev.purlis.app"),
    };
    std::fs::create_dir_all(&logs.old).unwrap();
    std::fs::write(logs.old.join("panics.log"), "panic\n").unwrap();

    Machine {
        _dir: dir,
        local: Local {
            config_root,
            data_base: Some(data_base),
            logs: Some(logs),
            planes: Vec::new(),
            own_app: None,
            plugin: None,
        },
        home,
        plane,
        device,
    }
}

fn nobody_running() -> Seams<'static> {
    Seams {
        busy: &|_, _| None,
        ..Seams::real()
    }
}

/// Every path under `root`, with a file's bytes and every entry's mode — but the config root's
/// lock file, which a run makes (empty) to lock and never removes, as a lock file is never
/// removed while another process may be about to take it.
fn tree(root: &Path) -> BTreeMap<PathBuf, (Option<Vec<u8>>, u32)> {
    let mut all = every_path(root);
    all.remove(&PathBuf::from(".config").join(renamelocal::busy::LOCK));
    all
}

fn every_path(root: &Path) -> BTreeMap<PathBuf, (Option<Vec<u8>>, u32)> {
    use std::os::unix::fs::PermissionsExt;
    let mut out = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            let meta = std::fs::symlink_metadata(&path).unwrap();
            let bytes = if meta.is_dir() {
                stack.push(path.clone());
                None
            } else {
                Some(std::fs::read(&path).unwrap())
            };
            out.insert(
                path.strip_prefix(root).unwrap().to_path_buf(),
                (bytes, meta.permissions().mode()),
            );
        }
    }
    out
}

/// Where two trees differ, for a message.
fn differ(
    a: &BTreeMap<PathBuf, (Option<Vec<u8>>, u32)>,
    b: &BTreeMap<PathBuf, (Option<Vec<u8>>, u32)>,
) -> Vec<PathBuf> {
    a.keys()
        .chain(b.keys())
        .filter(|path| a.get(*path) != b.get(*path))
        .map(|path| {
            let show = |t: &BTreeMap<PathBuf, (Option<Vec<u8>>, u32)>| {
                t.get(path).map(|(bytes, mode)| {
                    (
                        bytes
                            .as_ref()
                            .map(|b| String::from_utf8_lossy(b).into_owned()),
                        *mode,
                    )
                })
            };
            path.join(format!("{:?} vs {:?}", show(a), show(b)))
        })
        .collect()
}

/// [`tree`], leaving out git's own index, which a question to git may refresh.
fn before_and_after(root: &Path) -> BTreeMap<PathBuf, (Option<Vec<u8>>, u32)> {
    tree(root)
        .into_iter()
        .filter(|(path, _)| !path.ends_with(".git/index"))
        .collect()
}

/// The machine still says what it said: approvals, the pin, the device id, the channel and the
/// kill switch, through the readers every other caller uses.
fn everything_survives(m: &Machine) {
    let store = machine::read(&m.local.config_root).store;
    assert_eq!(
        store.consent(&m.plane, &Contribution::of(&m.plane)),
        Consent::Unchanged,
        "the approval"
    );
    assert!(
        store.recent(&m.plane).is_some_and(|recent| recent.pinned),
        "the pin"
    );
    assert_eq!(store.channel, charter_core::updates::Channel::Dev);
    // Read only: asking `device_id` would write the store.
    assert_eq!(
        machine::known_device_id(&m.local.config_root).as_deref(),
        Some(m.device.as_str())
    );
    assert!(charter_core::halt::stopped_on_disk(&m.local.config_root));
    assert_eq!(
        std::fs::read_to_string(machine::charterd(&m.local.config_root).join("scope")).unwrap(),
        "credential\n"
    );
    assert_eq!(
        std::fs::read_to_string(names::state(&m.plane).join("app/reopen.json")).unwrap(),
        "{}\n"
    );
}

#[test]
fn old_names_move_to_the_purlis_names_and_the_journal_says_so() {
    charter_core::unsteered!();
    let m = machine();
    let config = &m.local.config_root;

    let moved = renamelocal::run(&m.local, &nobody_running());

    assert!(moved.complete && moved.changed, "{:#?}", moved.said);
    assert_eq!(moved.refused, None);
    assert!(!config.join("charter").exists() && config.join("purlis").is_dir());
    assert_eq!(machine::dir(config), config.join("purlis"));
    assert_eq!(
        machine::charterd(config),
        config.join("purlis").join("purlisd")
    );
    assert!(m.home.join("data/purlis/events/log").is_file());
    assert!(!m.home.join("data/charter").exists());
    assert!(m.home.join("Logs/dev.purlis.app/panics.log").is_file());
    assert!(!m.home.join("Logs/dev.charter.app").exists());
    assert!(m.plane.join("purlis.local.toml").is_file());
    assert!(!m.plane.join("charter.local.toml").exists());
    assert_eq!(names::state(&m.plane), m.plane.join(".purlis"));
    assert_eq!(
        names::local_settings(&m.plane),
        m.plane.join("purlis.local.toml")
    );

    // The journal is in the new config home, one line a step.
    let journal =
        std::fs::read_to_string(config.join("purlis").join(renamelocal::JOURNAL)).unwrap();
    assert!(journal.lines().count() >= 6, "{journal}");
    for moved in [
        "charterd",
        "charter.local.toml",
        ".charter",
        "dev.charter.app",
    ] {
        assert!(journal.contains(moved), "{moved} is journalled: {journal}");
    }

    // The record lists the project as it resolves, and is found through the config home's own
    // folder after it moved.
    let record = machine::dir(config).join(names::STATE_MOVED_RECORD);
    assert_eq!(
        std::fs::read_to_string(&record).unwrap(),
        format!("{}\n", m.plane.display())
    );
    assert!(names::listed_in(&record, &m.plane));
    // …so with a second `.charter/` beside it, as an older build would make, `.purlis/` stays
    // the state folder.
    std::fs::create_dir(m.plane.join(".charter")).unwrap();
    assert_eq!(
        names::state_name_with(&m.plane, |dir| names::listed_in(&record, dir)),
        ".purlis"
    );
    std::fs::remove_dir(m.plane.join(".charter")).unwrap();

    // Git ignores the new names already, without the committed `.gitignore` changing.
    let ignored = git(
        &m.plane,
        &[
            "check-ignore",
            ".purlis/app/reopen.json",
            "purlis.local.toml",
        ],
    );
    assert_eq!(
        String::from_utf8_lossy(&ignored.stdout),
        ".purlis/app/reopen.json\npurlis.local.toml\n",
        "{ignored:?}"
    );
    assert_eq!(
        std::fs::read_to_string(m.plane.join(".gitignore")).unwrap(),
        "/.charter/\n/charter.local.toml\n"
    );

    everything_survives(&m);
}

#[test]
fn the_undo_gives_back_the_before_state_byte_for_byte() {
    charter_core::unsteered!();
    let m = machine();
    let before = before_and_after(&m.home);

    assert!(renamelocal::run(&m.local, &nobody_running()).complete);
    let undone = renamelocal::undo(&m.local, &nobody_running());
    assert!(undone.complete && undone.changed, "{:#?}", undone.said);

    // Everything but the journal itself, which now records the undo and holds the app's launch
    // off from moving the names again.
    let journal = Path::new(".config/charter/rename-local");
    let after: BTreeMap<_, _> = before_and_after(&m.home)
        .into_iter()
        .filter(|(path, _)| !path.starts_with(journal))
        .collect();
    assert!(after == before, "{:?}", differ(&after, &before));
    assert!(renamelocal::undone(&m.local));
    everything_survives(&m);

    // Run again on purpose, it moves them again; undo again, it puts them back again.
    assert!(renamelocal::run(&m.local, &nobody_running()).changed);
    assert!(!renamelocal::undone(&m.local));
    assert!(renamelocal::undo(&m.local, &nobody_running()).complete);
    let again: BTreeMap<_, _> = before_and_after(&m.home)
        .into_iter()
        .filter(|(path, _)| !path.starts_with(journal))
        .collect();
    assert!(again == before, "{:?}", differ(&again, &before));
}

#[test]
fn running_it_again_moves_nothing_and_writes_nothing() {
    charter_core::unsteered!();
    let m = machine();
    assert!(renamelocal::run(&m.local, &nobody_running()).changed);
    let once = tree(&m.home);

    let again = renamelocal::run(&m.local, &nobody_running());

    assert!(again.complete && !again.changed, "{:#?}", again.said);
    assert_eq!(tree(&m.home), once);
    // An undo with nothing since the last one is nothing, too.
    renamelocal::undo(&m.local, &nobody_running());
    let undone = tree(&m.home);
    let twice = renamelocal::undo(&m.local, &nobody_running());
    assert!(!twice.changed, "{:#?}", twice.said);
    assert_eq!(tree(&m.home), undone);
}

#[test]
fn a_move_that_fails_leaves_the_old_name_in_place_and_read() {
    charter_core::unsteered!();
    let m = machine();
    // The config home's folder and the project's state folder cannot be renamed.
    let refuse = |from: &Path, to: &Path| -> io::Result<()> {
        let name = from.file_name().unwrap_or_default();
        if name == "charter" && from.parent().is_some_and(|p| p.ends_with(".config"))
            || name == ".charter"
        {
            return Err(io::Error::other("injected"));
        }
        std::fs::rename(from, to)
    };
    let seams = Seams {
        rename: &refuse,
        busy: &|_, _| None,
        keyring: &renamelocal::keychain::real,
    };

    let moved = renamelocal::run(&m.local, &seams);

    assert!(!moved.complete, "{:#?}", moved.said);
    assert!(
        moved
            .said
            .iter()
            .any(|line| line.starts_with('✗') && line.contains("injected")),
        "{:#?}",
        moved.said
    );
    let config = &m.local.config_root;
    assert!(config.join("charter").is_dir() && !config.join("purlis").exists());
    assert_eq!(machine::dir(config), config.join("charter"));
    assert_eq!(names::state(&m.plane), m.plane.join(".charter"));
    // …and the record does not vouch for a `.purlis/` that was never made.
    assert!(!names::listed_in(
        &machine::dir(config).join(names::STATE_MOVED_RECORD),
        &m.plane
    ));
    // The rest went ahead, and the machine still reads all of it.
    assert!(m.plane.join("purlis.local.toml").is_file());
    everything_survives(&m);

    // And the undo puts back what did move.
    assert!(
        renamelocal::undo(
            &m.local,
            &Seams {
                busy: &|_, _| None,
                ..Seams::real()
            }
        )
        .complete
    );
    assert!(m.plane.join("charter.local.toml").is_file());
    assert!(m.home.join("data/charter/events/log").is_file());
    everything_survives(&m);
}

#[test]
fn nothing_moves_while_a_charter_has_a_project_open() {
    charter_core::unsteered!();
    let m = machine();
    let before = tree(&m.home);

    let moved = renamelocal::run(
        &m.local,
        &Seams {
            busy: &|_, _| Some("a chat is running".to_owned()),
            ..Seams::real()
        },
    );

    let why = moved.refused.expect("refused");
    assert!(why.contains(renamelocal::busy::QUIT_FIRST), "{why}");
    assert!(why.contains("a chat is running"), "{why}");
    assert!(!moved.changed);
    assert_eq!(tree(&m.home), before);
}

#[test]
fn a_state_folder_under_both_names_is_never_merged() {
    charter_core::unsteered!();
    let m = machine();
    std::fs::create_dir(m.plane.join(".purlis")).unwrap();
    std::fs::write(m.plane.join(".purlis/planted"), "x").unwrap();

    let moved = renamelocal::run(&m.local, &nobody_running());

    assert!(!moved.complete);
    assert!(m.plane.join(".charter/app/reopen.json").is_file());
    assert_eq!(
        std::fs::read_dir(m.plane.join(".purlis")).unwrap().count(),
        1,
        "nothing was put into the purlis one"
    );
    // Not listed, so the old folder stays the state folder.
    assert_eq!(names::state(&m.plane), m.plane.join(".charter"));
}

fn journal_of(m: &Machine) -> PathBuf {
    machine::dir(&m.local.config_root).join(renamelocal::JOURNAL)
}

fn add_to_journal(m: &Machine, line: &str) {
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(journal_of(m))
        .unwrap();
    writeln!(file, "{line}").unwrap();
}

#[test]
fn a_process_holding_the_config_homes_lock_keeps_everything_where_it_is() {
    charter_core::unsteered!();
    let m = machine();
    let before = tree(&m.home);
    let held = renamelocal::busy::hold_shared(&m.local.config_root).expect("held");

    let moved = renamelocal::run(&m.local, &nobody_running());

    assert!(moved.refused.is_some(), "{:#?}", moved.said);
    assert_eq!(tree(&m.home), before);
    drop(held);
    assert!(renamelocal::run(&m.local, &nobody_running()).changed);
}

#[test]
fn an_older_builds_stop_under_the_old_name_still_stops_after_the_move() {
    charter_core::unsteered!();
    let m = machine();
    assert!(renamelocal::run(&m.local, &nobody_running()).complete);
    charter_core::halt::rearm(&m.local.config_root, 8).unwrap();
    assert!(!charter_core::halt::stopped_on_disk(&m.local.config_root));

    // An older build's `stop --all` writes where it always did.
    let old = m.local.config_root.join("charter");
    std::fs::create_dir_all(&old).unwrap();
    std::fs::write(old.join("halted"), "").unwrap();

    assert!(charter_core::halt::stopped_on_disk(&m.local.config_root));
}

#[test]
fn an_undo_stopped_by_a_recreated_folder_stays_pending_and_a_second_one_finishes_it() {
    charter_core::unsteered!();
    let m = machine();
    let before = before_and_after(&m.home);
    assert!(renamelocal::run(&m.local, &nobody_running()).complete);
    // An older build made `charter/` again, with one file of its own.
    let old = m.local.config_root.join("charter");
    std::fs::create_dir_all(&old).unwrap();
    std::fs::write(old.join("theirs"), "kept\n").unwrap();

    let partial = renamelocal::undo(&m.local, &nobody_running());

    assert!(!partial.complete, "{:#?}", partial.said);
    assert!(
        partial.said.iter().any(|line| line.contains("made again")),
        "{:#?}",
        partial.said
    );
    // The launch moves nothing back while the undo is pending.
    assert!(renamelocal::undone(&m.local));
    assert!(
        !std::fs::read_to_string(journal_of(&m))
            .unwrap()
            .contains("\"undone\"")
    );

    // The operator moves the stray folder away; a second undo finishes, and nothing is lost.
    let aside = m.home.join("aside");
    std::fs::rename(&old, &aside).unwrap();
    let finished = renamelocal::undo(&m.local, &nobody_running());
    assert!(finished.complete, "{:#?}", finished.said);
    assert_eq!(read_to(&aside.join("theirs")), "kept\n");
    let after: BTreeMap<_, _> = before_and_after(&m.home)
        .into_iter()
        .filter(|(path, _)| {
            !path.starts_with(".config/charter/rename-local") && !path.starts_with("aside")
        })
        .collect();
    assert!(after == before, "{:?}", differ(&after, &before));
    everything_survives(&m);
}

fn read_to(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap()
}

#[test]
fn an_undo_of_a_step_rename_local_never_makes_is_refused_whole() {
    charter_core::unsteered!();
    let m = machine();
    assert!(renamelocal::run(&m.local, &nobody_running()).complete);
    let victim = m.home.join("notes.txt");
    std::fs::write(&victim, "one\ntwo\n").unwrap();
    let elsewhere = m.home.join("elsewhere");
    std::fs::create_dir(&elsewhere).unwrap();
    let forged = [
        serde_json::json!({"op": "append", "file": victim, "len": 4, "text": "two\n"}),
        serde_json::json!({"op": "move", "what": "x", "from": m.home.join("gone"), "to": elsewhere}),
        serde_json::json!({"op": "made", "path": m.home.join("work")}),
    ];
    for entry in forged {
        let journal = read_to(&journal_of(&m));
        add_to_journal(&m, &entry.to_string());

        let undone = renamelocal::undo(&m.local, &nobody_running());

        let why = undone.refused.expect("refused");
        assert!(why.contains("never makes"), "{why}");
        assert_eq!(read_to(&victim), "one\ntwo\n");
        assert!(elsewhere.is_dir() && m.home.join("work").is_dir());
        std::fs::write(journal_of(&m), journal).unwrap();
    }
    // The journal as rename-local wrote it is still undone.
    assert!(renamelocal::undo(&m.local, &nobody_running()).complete);
}

#[test]
fn an_append_journalled_but_never_made_is_left_alone_by_the_undo() {
    charter_core::unsteered!();
    let m = machine();
    // A run that died between journalling the exclude lines and writing them.
    let exclude = m.plane.join(".git/info/exclude");
    let bytes = std::fs::read(&exclude).unwrap();
    std::fs::create_dir_all(journal_of(&m).parent().unwrap()).unwrap();
    add_to_journal(
        &m,
        &serde_json::json!({
            "op": "append", "file": exclude, "len": bytes.len(), "text": "/.purlis/\n"
        })
        .to_string(),
    );

    let undone = renamelocal::undo(&m.local, &nobody_running());

    assert!(undone.complete, "{:#?}", undone.said);
    assert_eq!(std::fs::read(&exclude).unwrap(), bytes);
}

#[test]
fn a_record_line_whose_purlis_folder_is_not_there_is_dropped_by_the_next_run() {
    charter_core::unsteered!();
    let m = machine();
    // A run that died between the record and the rename vouched for a folder never made.
    let record = machine::dir(&m.local.config_root).join(names::STATE_MOVED_RECORD);
    std::fs::create_dir_all(record.parent().unwrap()).unwrap();
    let stray = m.home.join("other");
    std::fs::create_dir_all(stray.join(".charter")).unwrap();
    std::fs::write(&record, format!("{}\n", stray.display())).unwrap();

    assert!(renamelocal::run(&m.local, &nobody_running()).complete);

    assert!(!names::listed_in(
        &machine::dir(&m.local.config_root).join(names::STATE_MOVED_RECORD),
        &stray
    ));
    assert!(names::listed_in(
        &machine::dir(&m.local.config_root).join(names::STATE_MOVED_RECORD),
        &m.plane
    ));
}

#[test]
fn a_charter_is_seen_by_its_program_and_its_fallback_socket() {
    charter_core::unsteered!();
    let listing = "\
  10     1  501 /Applications/charter.app/Contents/MacOS/charter
  11    10  501 /usr/local/bin/purlis
  12     1  502 /usr/local/bin/charter
  13     1  501 /bin/zsh
  14     1  501 charter-app
  20    19  501 /x/target/debug/purlis
";
    assert_eq!(
        renamelocal::busy::others_named(listing, 501, 20, 19),
        [
            "charter (pid 10)",
            "purlis (pid 11)",
            "charter-app (pid 14)"
        ]
    );

    let base = tempfile::tempdir().unwrap();
    for folder in ["charter-op-00ab", "purlis-op-00cd", "other-op-00ef"] {
        std::fs::create_dir(base.path().join(folder)).unwrap();
    }
    let mut found = renamelocal::busy::in_fallback(base.path());
    found.sort();
    assert_eq!(
        found,
        [
            base.path().join("charter-op-00ab/hooks.sock"),
            base.path().join("purlis-op-00cd/hooks.sock"),
        ]
    );
    // A socket file nothing answers on is a crashed app's.
    assert!(!renamelocal::busy::answers(&found[0]));
}

/// A Linux machine whose single-instance locks live under `dir`.
fn linux_places(dir: &Path) -> renamelocal::busy::Places {
    renamelocal::busy::Places {
        linux: true,
        runtime: Some(dir.to_path_buf()),
        uid: 501,
        ..Default::default()
    }
}

#[test]
fn the_app_at_its_launch_is_not_kept_waiting_by_its_own_instance_lock_but_by_the_others() {
    charter_core::unsteered!();
    use renamelocal::busy::Instances;
    let dir = tempfile::tempdir().unwrap();
    let places = linux_places(dir.path());
    let lock = |id: &str| {
        let file = std::fs::File::create(dir.path().join(format!("{id}.lock"))).unwrap();
        file.lock().unwrap();
        file
    };

    // This app's own lock, as `one_per_user` holds it by the time the launch asks.
    let own = lock("dev.charter.app");
    assert_eq!(
        Instances::of(&places, Some("dev.charter.app")).running(),
        None
    );
    // A terminal asks about every identifier, this one included.
    assert!(Instances::of(&places, None).running().is_some());
    drop(own);

    // The other identifier's app is a second app, and the launch waits for it.
    let other = lock("dev.purlis.app");
    let why = Instances::of(&places, Some("dev.charter.app"))
        .running()
        .expect("the other app is running");
    assert!(why.contains("dev.purlis.app.lock"), "{why}");
    drop(other);
}

#[test]
fn the_macos_single_instance_socket_is_named_as_the_plugin_names_it() {
    charter_core::unsteered!();
    let places = renamelocal::busy::Places {
        macos: true,
        socket_dir: PathBuf::from("/tmp"),
        ..Default::default()
    };
    assert_eq!(
        renamelocal::busy::Instances::of(&places, None).sockets,
        [
            PathBuf::from("/tmp/dev_purlis_app_si.sock"),
            PathBuf::from("/tmp/dev_charter_app_si.sock"),
        ]
    );
    assert_eq!(
        renamelocal::busy::Instances::of(&places, Some("dev.charter.app")).sockets,
        [PathBuf::from("/tmp/dev_purlis_app_si.sock")]
    );
}

#[test]
fn a_process_list_that_cannot_be_read_refuses_rather_than_passes() {
    charter_core::unsteered!();
    use renamelocal::busy::others_in;
    let fine = "  20 19 501 /x/purlis\n  21 1 501 /bin/zsh\n".to_owned();
    assert_eq!(others_in(Ok((true, fine.clone())), 501, 20, 19), Ok(vec![]));
    // `ps` did not start.
    assert!(others_in(Err(io::Error::other("no ps")), 501, 20, 19).is_err());
    // `ps` failed.
    assert!(others_in(Ok((false, fine)), 501, 20, 19).is_err());
    // A list that does not show this very process shows nothing reliably.
    assert!(others_in(Ok((true, "  21 1 501 /bin/zsh\n".to_owned())), 501, 20, 19).is_err());
    // A charter in it is named.
    assert_eq!(
        others_in(
            Ok((
                true,
                "  20 19 501 /x/purlis\n  30 1 501 /usr/bin/charter\n".to_owned()
            )),
            501,
            20,
            19
        ),
        Ok(vec!["charter (pid 30)".to_owned()])
    );
}
