//! The CI panel's whole source: a cache file some other process wrote.
//!
//! Nothing here starts a process, opens a socket or reads a token. The panel's job is to say
//! what was last fetched and *when*, or to say that nothing was — and never to be blank.
//!
//! Every value in the file is another program's, and the file is unsigned and writable by
//! anything running as the operator, so the tests below are mostly about what charter will
//! NOT take out of it.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use charter_core::cistate::{self, NotRead, Reading};

/// A plane with a `.charter/cache/` in it, and nothing else.
fn plane() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let at = std::fs::canonicalize(dir.path()).unwrap();
    std::fs::write(at.join("charter.toml"), "schema = 1\n").unwrap();
    std::fs::create_dir_all(at.join(".charter/cache")).unwrap();
    (dir, at)
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// Write the cache with one entry for `tree`, exactly as the refresher keys it.
fn cache_holding(plane: &Path, tree: &Path, entry: &str) {
    let key = tree.display().to_string();
    let doc = format!("{{{key:?}: {entry}}}");
    std::fs::write(plane.join(cistate::CACHE), doc).unwrap();
}

/// The tree a reading is about. It never has to exist: the key is a string.
fn tree(plane: &Path) -> PathBuf {
    plane.join("workspaces/alpha/svc")
}

fn why(reading: &Reading) -> String {
    match reading {
        Reading::NotFetched(why) => why.clone(),
        other => panic!("expected nothing to serve, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------------------
// What is served                                                                          #
// ---------------------------------------------------------------------------------------

#[test]
fn a_plane_nothing_has_refreshed_has_no_forge_state_and_that_is_not_a_refusal() {
    let (_keep, at) = plane();

    let cache = cistate::read(&at).expect("a plane with no cache is not a refusal");

    assert!(cache.is_empty());
    assert!(matches!(
        cache.about(&tree(&at), "main"),
        Reading::NotFetched(_)
    ));
}

#[test]
fn the_state_the_last_refresh_recorded_for_this_branch_is_what_the_panel_shows() {
    let (_keep, at) = plane();
    let svc = tree(&at);
    cache_holding(
        &at,
        &svc,
        &format!(
            r##"{{"branch": "main", "ts": {}, "ci": "failed", "change": 41, "sigil": "#"}}"##,
            now() - 60
        ),
    );

    let reading = cistate::read(&at)
        .expect("the cache reads")
        .about(&svc, "main");

    match reading {
        Reading::Fetched {
            state,
            change,
            sigil,
            seconds_ago,
        } => {
            assert_eq!(state.as_deref(), Some("failed"));
            assert_eq!(change, Some(41));
            assert_eq!(sigil, Some('#'));
            // The age is the answer's, and the panel says it: a two-hour-old "success" is
            // not the same claim as one from a minute ago.
            assert!((55..=120).contains(&seconds_ago), "{seconds_ago}");
        }
        other => panic!("the entry is served: {other:?}"),
    }
}

#[test]
fn an_entry_that_names_no_pipeline_is_not_the_same_as_nobody_having_looked() {
    // Python writes `ci: null` for "there is no pipeline" and for "the call failed" alike,
    // so this says only what the file says — but it is still a *fetch*, with an age, and
    // that is the part "not fetched" would throw away.
    let (_keep, at) = plane();
    let svc = tree(&at);
    cache_holding(
        &at,
        &svc,
        &format!(r#"{{"branch": "main", "ts": {}, "ci": null}}"#, now()),
    );

    let reading = cistate::read(&at)
        .expect("the cache reads")
        .about(&svc, "main");

    assert!(
        matches!(reading, Reading::Fetched { state: None, .. }),
        "{reading:?}"
    );
}

#[test]
fn an_entry_keyed_under_another_spelling_of_the_same_checkout_is_still_found() {
    // The refresher keys by the path as IT walked the plane, and two programs reach the same
    // checkout under two spellings all the time — `/tmp` is a link to `/private/tmp`, and a
    // plane reached through a link into `workspaces/` is another. On the string alone every
    // one of those reads as "nobody has fetched this", for ever and silently. No fixture that
    // writes the key the app is about to build can catch that, so this one writes a
    // DIFFERENT one.
    let (_keep, at) = plane();
    std::fs::create_dir_all(at.join("workspaces/alpha/svc")).unwrap();
    std::os::unix::fs::symlink("alpha", at.join("workspaces/alias")).unwrap();
    let real = at.join("workspaces/alpha/svc");
    let aliased = at.join("workspaces/alias/svc");

    // Written under the aliased spelling, asked for under the real one …
    cache_holding(
        &at,
        &aliased,
        &format!(r#"{{"branch": "main", "ts": {}, "ci": "success"}}"#, now()),
    );
    let reading = cistate::read(&at)
        .expect("the cache reads")
        .about(&real, "main");
    assert!(
        matches!(reading, Reading::Fetched { .. }),
        "the aliased key was not found: {reading:?}"
    );

    // … and the other way round, because neither side is the canonical one.
    cache_holding(
        &at,
        &real,
        &format!(r#"{{"branch": "main", "ts": {}, "ci": "success"}}"#, now()),
    );
    let reading = cistate::read(&at)
        .expect("the cache reads")
        .about(&aliased, "main");
    assert!(
        matches!(reading, Reading::Fetched { .. }),
        "the real key was not found from the alias: {reading:?}"
    );
}

#[test]
fn an_entry_for_a_different_checkout_is_not_served_for_this_one() {
    // The guard on the guard: resolving both sides must not turn every miss into a hit.
    let (_keep, at) = plane();
    std::fs::create_dir_all(at.join("workspaces/alpha/svc")).unwrap();
    std::fs::create_dir_all(at.join("workspaces/alpha/tool")).unwrap();
    cache_holding(
        &at,
        &at.join("workspaces/alpha/svc"),
        &format!(r#"{{"branch": "main", "ts": {}, "ci": "success"}}"#, now()),
    );

    let reading = cistate::read(&at)
        .expect("the cache reads")
        .about(&at.join("workspaces/alpha/tool"), "main");

    assert!(why(&reading).contains("nothing has fetched"), "{reading:?}");
}

// ---------------------------------------------------------------------------------------
// What is not served, and why it says so                                                  #
// ---------------------------------------------------------------------------------------

#[test]
fn an_entry_fetched_for_another_branch_is_not_served_for_this_one() {
    // The checkout moved since the refresh. Serving the old answer under the new branch is
    // how a red pipeline reads as somebody else's.
    let (_keep, at) = plane();
    let svc = tree(&at);
    cache_holding(
        &at,
        &svc,
        &format!(r#"{{"branch": "main", "ts": {}, "ci": "success"}}"#, now()),
    );

    let reading = cistate::read(&at)
        .expect("the cache reads")
        .about(&svc, "release/1.2");

    assert!(why(&reading).contains("different branch"), "{reading:?}");
}

#[test]
fn an_entry_older_than_the_window_is_not_served_however_green_it_is() {
    let (_keep, at) = plane();
    let svc = tree(&at);
    cache_holding(
        &at,
        &svc,
        &format!(
            r#"{{"branch": "main", "ts": {}, "ci": "success"}}"#,
            now() - cistate::DISPLAY.as_secs() - 60
        ),
    );

    let reading = cistate::read(&at)
        .expect("the cache reads")
        .about(&svc, "main");

    assert!(why(&reading).contains("two hours"), "{reading:?}");
}

#[test]
fn an_entry_with_no_stamp_is_not_served_as_though_it_had_just_been_written() {
    let (_keep, at) = plane();
    let svc = tree(&at);
    cache_holding(&at, &svc, r#"{"branch": "main", "ci": "success"}"#);

    let reading = cistate::read(&at)
        .expect("the cache reads")
        .about(&svc, "main");

    assert!(matches!(reading, Reading::NotFetched(_)), "{reading:?}");
}

#[test]
fn a_ci_word_charter_does_not_know_is_never_put_on_the_panel() {
    // The file is unsigned and another program writes it. The seven words are what both
    // forges are pinned to, and a cell holding anything else is drawn by whatever renders
    // the panel — an escape sequence included.
    let (_keep, at) = plane();
    let svc = tree(&at);
    for hostile in [r#""\u001b[2Jowned""#, r#""SUCCESS""#, r#"7"#, r#"{}"#] {
        cache_holding(
            &at,
            &svc,
            &format!(r#"{{"branch": "main", "ts": {}, "ci": {hostile}}}"#, now()),
        );

        let reading = cistate::read(&at)
            .expect("the cache reads")
            .about(&svc, "main");

        assert!(
            why(&reading).contains("does not know"),
            "{hostile} was served: {reading:?}"
        );
    }
}

#[test]
fn a_change_that_is_not_a_whole_number_above_zero_is_dropped_and_the_row_still_draws() {
    // charter #326: the change id is the one forge field that reaches a line a terminal
    // interprets. Python coerces it on read as well as on write, because an entry written
    // by a charter with a bug renders for two hours after the upgrade that fixed it.
    let (_keep, at) = plane();
    let svc = tree(&at);
    for hostile in [r#""12""#, r#"true"#, r#"0"#, r#"-4"#, r#""\u001b[2J""#] {
        cache_holding(
            &at,
            &svc,
            &format!(
                r#"{{"branch": "main", "ts": {}, "ci": "success", "change": {hostile}}}"#,
                now()
            ),
        );

        let reading = cistate::read(&at)
            .expect("the cache reads")
            .about(&svc, "main");

        match reading {
            Reading::Fetched { state, change, .. } => {
                assert_eq!(change, None, "{hostile} was taken as a change");
                assert_eq!(state.as_deref(), Some("success"), "and the row still draws");
            }
            other => panic!("{hostile}: {other:?}"),
        }
    }
}

#[test]
fn a_sigil_that_is_not_one_of_the_two_a_forge_uses_is_dropped() {
    let (_keep, at) = plane();
    let svc = tree(&at);
    // A whole escape sequence AND a single ordinary character. Only the second holds the
    // allowlist: the first is refused by the length check alone, so a version that took any
    // single character passed a test written with the escape sequence only. A mutation found
    // that.
    for hostile in [r#""\u001b[2J""#, r#""x""#] {
        cache_holding(
            &at,
            &svc,
            &format!(
                r#"{{"branch": "main", "ts": {}, "change": 4, "sigil": {hostile}}}"#,
                now()
            ),
        );

        let reading = cistate::read(&at)
            .expect("the cache reads")
            .about(&svc, "main");

        assert!(
            matches!(reading, Reading::Fetched { sigil: None, .. }),
            "{hostile} was taken as a sigil: {reading:?}"
        );
    }
}

#[test]
fn the_field_an_older_charter_wrote_the_change_under_is_still_read() {
    let (_keep, at) = plane();
    let svc = tree(&at);
    cache_holding(
        &at,
        &svc,
        &format!(r#"{{"branch": "main", "ts": {}, "mr": 9}}"#, now()),
    );

    let reading = cistate::read(&at)
        .expect("the cache reads")
        .about(&svc, "main");

    assert!(
        matches!(
            reading,
            Reading::Fetched {
                change: Some(9),
                ..
            }
        ),
        "{reading:?}"
    );
}

// ---------------------------------------------------------------------------------------
// The file itself                                                                         #
// ---------------------------------------------------------------------------------------

#[test]
fn a_cache_file_that_is_a_symlink_is_refused_rather_than_followed() {
    let (_keep, at) = plane();
    let elsewhere = tempfile::tempdir().unwrap();
    let planted = elsewhere.path().join("planted.json");
    std::fs::write(
        &planted,
        r#"{"/anywhere": {"branch": "main", "ci": "success"}}"#,
    )
    .unwrap();
    std::os::unix::fs::symlink(&planted, at.join(cistate::CACHE)).unwrap();

    let refusal = cistate::read(&at).expect_err("charter's own path may not be a link");

    // The VARIANT, not the word: a link at the leaf is also "not a regular file", and a test
    // that accepted either would stay green with the link walk taken out entirely.
    assert!(
        matches!(refusal, NotRead::ThroughALink { .. }),
        "refused for the right reason: {refusal}"
    );
}

#[test]
fn a_cache_directory_that_is_a_symlink_is_refused_too() {
    // The file itself is then perfectly ordinary, which is exactly why the whole way down
    // is walked rather than only the leaf.
    let (_keep, at) = plane();
    let elsewhere = tempfile::tempdir().unwrap();
    std::fs::write(
        elsewhere.path().join("glstate.json"),
        r#"{"/anywhere": {"branch": "main", "ci": "success"}}"#,
    )
    .unwrap();
    std::fs::remove_dir_all(at.join(".charter/cache")).unwrap();
    std::os::unix::fs::symlink(elsewhere.path(), at.join(".charter/cache")).unwrap();

    let refusal = cistate::read(&at).expect_err("charter's own path may not be a link");

    assert!(
        matches!(refusal, NotRead::ThroughALink { .. }),
        "refused for the right reason: {refusal}"
    );
}

#[test]
fn a_cache_that_is_not_a_regular_file_is_refused_rather_than_opened() {
    // A FIFO here would block the read for ever, and the panel would never draw.
    let (_keep, at) = plane();
    std::fs::create_dir_all(at.join(cistate::CACHE)).unwrap();

    let refusal = cistate::read(&at).expect_err("a directory is not a cache");

    assert!(
        matches!(refusal, NotRead::NotAFile { .. }),
        "refused for the right reason: {refusal}"
    );
    assert!(format!("{refusal}").contains("directory"), "{refusal}");
}

#[test]
fn a_cache_charter_cannot_parse_is_said_rather_than_read_as_empty() {
    // "Nothing was fetched" and "this file is broken" send an operator to different places.
    let (_keep, at) = plane();
    for junk in ["{", "[]", "\"a string\"", "null"] {
        std::fs::write(at.join(cistate::CACHE), junk).unwrap();

        let refusal = cistate::read(&at).expect_err("junk is not what charter writes");

        assert!(
            format!("{refusal}").contains("entries charter writes"),
            "{junk}: {refusal}"
        );
    }
}
