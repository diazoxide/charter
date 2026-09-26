//! `charter news` and `charter update`, through the binary.
//!
//! `charter news` prints the app's own CHANGELOG.md, compiled in (#352): every section newest
//! first, or one with `--for`. The Python charter's news corpus, its range view and its
//! `--pending` probes are gone, and each retired flag is refused by name. `update` installs
//! nothing: it says so, names the channel, and points at `charter news`.

use std::path::Path;
use std::process::{Command, Output};

fn charter(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_charter"))
        .args(args)
        .current_dir(root)
        .env("CHARTER_ROOT", root)
        // `update` reads the machine store for the update channel (ADR 0042), so the
        // store is pinned too, inside the run's own temp tree. Unpinned, a
        // fenced build dies rather than read the operator's `~/.config` (charter-app#129).
        .env("CHARTER_CONFIG_HOME", config_home(root))
        .env("NO_COLOR", "1")
        .env("TERM", "dumb")
        .output()
        .expect("the binary runs")
}

/// The config home a test's `charter` keeps its machine store in: inside the plane's own temp
/// directory, so it goes when the plane does, and under a name that is not `.charter`, so a test
/// asserting the plane gained no state is not answered by the store.
fn config_home(root: &Path) -> std::path::PathBuf {
    root.join("machine-config-home")
}

fn plane() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("charter.toml"), "").unwrap();
    // Made here because the fence resolves the path it is handed, and a directory that does
    // not exist yet resolves nowhere — so an absent config home reads as outside the fence.
    std::fs::create_dir_all(config_home(dir.path())).unwrap();
    dir
}

fn out(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn err(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

#[test]
fn news_is_the_changelog_this_build_carries_newest_first() {
    let dir = plane();
    let said = charter(dir.path(), &["news"]);
    assert!(said.status.success(), "{said:?}");
    assert_eq!(
        err(&said),
        "",
        "the sections are stdout's and nothing else is said"
    );
    let text = out(&said);
    let newest = text
        .find("## [0.1.1] - 2026-09-24")
        .expect("0.1.1 is there");
    let oldest = text
        .find("## [0.1.0] - 2026-09-23")
        .expect("0.1.0 is there");
    assert!(newest < oldest, "newest first:\n{text}");
}

#[test]
fn news_does_not_need_an_update_baseline_and_never_asks_for_one() {
    // #352: a plane with the Python charter's update baseline, and one without, get the same
    // answer, and neither is told it has no history.
    let bare = plane();
    let stamped = plane();
    let baseline = stamped.path().join(".charter/cache/update-baseline");
    std::fs::create_dir_all(baseline.parent().unwrap()).unwrap();
    std::fs::write(&baseline, "0.61.0\n").unwrap();

    let a = charter(bare.path(), &["news"]);
    let b = charter(stamped.path(), &["news"]);
    assert_eq!(out(&a), out(&b));
    assert!(!err(&a).contains("baseline"), "{}", err(&a));
    assert!(!out(&a).is_empty());
}

#[test]
fn news_for_a_version_is_its_release_notes_on_stdout() {
    let dir = plane();
    let body = charter(dir.path(), &["news", "--for", "0.1.0"]);
    assert!(body.status.success(), "{body:?}");
    assert_eq!(
        err(&body),
        "",
        "the body is stdout's and nothing else is said"
    );
    let section = charter_core::news::CHANGELOG;
    let want = changelog::section(section, "0.1.0").unwrap().notes;
    assert_eq!(out(&body), format!("{want}\n"));
}

#[test]
fn news_for_a_version_the_changelog_does_not_have_exits_one_and_prints_nothing() {
    let dir = plane();
    let missing = charter(dir.path(), &["news", "--for", "9.9.9"]);
    assert_eq!(missing.status.code(), Some(1));
    assert_eq!(out(&missing), "");
    assert!(
        err(&missing).contains("CHANGELOG.md has no section for 9.9.9."),
        "{}",
        err(&missing)
    );
}

#[test]
fn the_retired_flags_are_refused_by_name_rather_than_as_unknown() {
    let dir = plane();
    for args in [
        &["news", "--since", "0.60.0"][..],
        &["news", "--until", "0.62.1"][..],
        &["news", "--pending"][..],
    ] {
        let said = charter(dir.path(), args);
        assert_eq!(said.status.code(), Some(1), "{args:?}: {said:?}");
        assert_eq!(out(&said), "");
        assert!(
            err(&said).contains("is retired"),
            "{args:?}: {}",
            err(&said)
        );
        assert!(
            err(&said).contains("charter news --for <version>"),
            "{}",
            err(&said)
        );
    }
}

#[test]
fn update_says_which_half_it_does_and_installs_nothing() {
    let dir = plane();
    let said = charter(dir.path(), &["update"]);
    assert!(said.status.success(), "{said:?}");
    assert!(
        err(&said).contains("does not install anything here"),
        "{}",
        err(&said)
    );
    assert!(err(&said).contains("charter news"), "{}", err(&said));
    assert!(!err(&said).contains("baseline"), "{}", err(&said));
    assert_eq!(out(&said), "");
    // Nothing was created: this command reads.
    assert!(!dir.path().join(".charter").exists(), "update wrote state");
}

#[test]
fn the_flags_that_move_a_python_package_are_answered_rather_than_rejected() {
    // An agent that typed `--bump` is owed the reason. clap refusing it as an unknown flag
    // would exit 2, which is the code a harness hook reads as "block".
    let dir = plane();
    let said = charter(dir.path(), &["update", "--to", "0.63.0", "--bump"]);
    assert!(said.status.success(), "{said:?}");
    assert!(
        err(&said).contains("--to names a published version"),
        "{}",
        err(&said)
    );
    assert!(
        err(&said).contains("the pin was left alone"),
        "{}",
        err(&said)
    );
}

#[test]
fn update_names_the_channel_and_moves_it_only_to_a_channel_that_exists() {
    let dir = plane();
    let home = config_home(dir.path());
    let said = charter(dir.path(), &["update"]);
    assert!(
        err(&said).contains("updates from the stable channel"),
        "{}",
        err(&said)
    );

    let moved = charter(dir.path(), &["update", "--channel", "dev"]);
    assert!(moved.status.success(), "{moved:?}");
    assert_eq!(
        charter_core::machine::read(&home).store.channel,
        charter_core::updates::Channel::Dev
    );
    let said = charter(dir.path(), &["update"]);
    assert!(
        err(&said).contains("updates from the dev channel"),
        "{}",
        err(&said)
    );

    // A typo exits 1 and leaves the channel where it was, rather than guessing.
    let refused = charter(dir.path(), &["update", "--channel", "nightly"]);
    assert_eq!(refused.status.code(), Some(1), "{refused:?}");
    assert_eq!(
        charter_core::machine::read(&home).store.channel,
        charter_core::updates::Channel::Dev
    );
}
