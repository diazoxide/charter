//! The dispatches refused while nobody was there (#1507): which refusals are kept, one entry a
//! pair and workspace with a count, an order no refusal moves, a dismissal that holds, the cap,
//! the expiry, and what a file written by anything else is read as. Every expected answer is
//! written out.

use std::path::Path;

use super::*;
use crate::dispatchunattended::{Missing, Refusal};

/// A refusal of `asking` to `target` in workspace `ide`, at `at`.
fn refused(root: &Path, asking: &str, target: &str, at: u64) -> Kept {
    keep(root, asking, target, Some("ide"), at).expect("written")
}

fn by_hand(root: &Path, text: &str) {
    let path = path(root);
    std::fs::create_dir_all(path.parent().expect("a folder")).expect("made");
    std::fs::write(path, text).expect("written");
}

/// The targets listed at `now`, in the list's order.
fn targets(root: &Path, now: u64) -> Vec<String> {
    list(root, now).into_iter().map(|one| one.target).collect()
}

const DAY: u64 = 24 * 60 * 60;

// ---- which refusals are kept ---------------------------------------------------------------------

#[test]
fn only_a_refusal_for_lack_of_a_grant_between_two_personas_is_kept() {
    let missing = |asking: Option<&str>, target: &str, unreviewed| {
        Refusal::Missing(Missing {
            asking: asking.map(str::to_owned),
            target: target.to_owned(),
            unreviewed,
        })
    };
    // No grant, and the project's pair nobody here reviewed: a person's allow mends both.
    assert_eq!(
        pair_kept(&missing(Some("steward"), "devops", false)),
        Some(("steward", "devops"))
    );
    assert_eq!(
        pair_kept(&missing(Some("steward"), "devops", true)),
        Some(("steward", "devops"))
    );
    // A chat on no persona has no pair a standing grant could name.
    assert_eq!(pair_kept(&missing(None, "devops", false)), None);
    // Nothing that is not a persona's name, and no persona to itself.
    assert_eq!(pair_kept(&missing(Some("steward"), "*", false)), None);
    assert_eq!(pair_kept(&missing(Some("Steward"), "devops", false)), None);
    assert_eq!(pair_kept(&missing(Some("steward"), "steward", false)), None);
    // Every other refusal is one no grant mends, and none is kept.
    for other in [
        Refusal::Never("steward".to_owned(), "devops".to_owned()),
        Refusal::NeverAbove("steward".to_owned(), "devops".to_owned()),
        Refusal::NeversUnread,
        Refusal::Unsandboxed("devops".to_owned()),
    ] {
        assert_eq!(pair_kept(&other), None, "{other:?}");
    }
}

// ---- kept and folded -----------------------------------------------------------------------------

#[test]
fn a_refusal_is_kept_with_who_wanted_whom_where_and_when() {
    let project = tempfile::tempdir().expect("a project");
    let root = project.path();
    assert_eq!(list(root, 1_000), vec![]);
    assert_eq!(refused(root, "steward", "devops", 1_000), Kept::New);
    assert_eq!(
        list(root, 1_000),
        vec![Refused {
            asking: "steward".to_owned(),
            target: "devops".to_owned(),
            workspace: Some("ide".to_owned()),
            first: 1_000,
            latest: 1_000,
            times: 1,
            dismissed: None,
        }]
    );
    assert!(path(root).ends_with("app/dispatches/refused-while-away.json"));
    assert_eq!(
        std::fs::read_to_string(path(root)).expect("the record"),
        "{\n  \"v\": 1,\n  \"refused\": [\n    {\n      \"asking\": \"steward\",\n      \
         \"target\": \"devops\",\n      \"workspace\": \"ide\",\n      \"first\": 1000,\n      \
         \"latest\": 1000,\n      \"times\": 1\n    }\n  ]\n}\n"
    );
}

#[test]
fn the_same_pair_in_the_same_workspace_is_one_entry_with_a_count_and_the_latest_time() {
    let project = tempfile::tempdir().expect("a project");
    let root = project.path();
    assert_eq!(refused(root, "steward", "devops", 1_000), Kept::New);
    assert_eq!(refused(root, "steward", "devops", 1_060), Kept::Again);
    assert_eq!(refused(root, "steward", "devops", 1_120), Kept::Again);
    assert_eq!(
        list(root, 1_120),
        vec![Refused {
            asking: "steward".to_owned(),
            target: "devops".to_owned(),
            workspace: Some("ide".to_owned()),
            first: 1_000,
            latest: 1_120,
            times: 3,
            dismissed: None,
        }]
    );
}

#[test]
fn another_pair_the_other_direction_and_another_workspace_are_entries_of_their_own() {
    let project = tempfile::tempdir().expect("a project");
    let root = project.path();
    refused(root, "steward", "devops", 1_000);
    refused(root, "devops", "steward", 1_001);
    refused(root, "steward", "qa", 1_002);
    keep(root, "steward", "devops", Some("web"), 1_003).expect("written");
    keep(root, "steward", "devops", None, 1_004).expect("written");
    let listed: Vec<(String, String, Option<String>, u32)> = list(root, 1_004)
        .into_iter()
        .map(|one| (one.asking, one.target, one.workspace, one.times))
        .collect();
    // In the order they were first refused.
    assert_eq!(
        listed,
        vec![
            (
                "steward".to_owned(),
                "devops".to_owned(),
                Some("ide".to_owned()),
                1
            ),
            (
                "devops".to_owned(),
                "steward".to_owned(),
                Some("ide".to_owned()),
                1
            ),
            (
                "steward".to_owned(),
                "qa".to_owned(),
                Some("ide".to_owned()),
                1
            ),
            (
                "steward".to_owned(),
                "devops".to_owned(),
                Some("web".to_owned()),
                1
            ),
            ("steward".to_owned(), "devops".to_owned(), None, 1),
        ]
    );
}

#[test]
fn a_chat_asking_again_moves_no_row_its_own_or_another_s() {
    let project = tempfile::tempdir().expect("a project");
    let root = project.path();
    refused(root, "planner", "reviewer", 1_000);
    refused(root, "steward", "devops", 1_010);
    refused(root, "steward", "qa", 1_020);
    let order = vec!["reviewer".to_owned(), "devops".to_owned(), "qa".to_owned()];
    assert_eq!(targets(root, 1_020), order);
    // A chat asks again and again, across both of its pairs in turn.
    for n in 0..20 {
        let target = if n % 2 == 0 { "qa" } else { "devops" };
        assert_eq!(refused(root, "steward", target, 1_030 + n), Kept::Again);
        assert_eq!(targets(root, 1_030 + n), order, "after {n}");
    }
    assert_eq!(list(root, 1_100)[2].times, 11);
    // A file whose entries are not in that order is still read in it.
    by_hand(
        root,
        r#"{"v":1,"refused":[
            {"asking":"steward","target":"qa","first":1020,"latest":1049,"times":11},
            {"asking":"planner","target":"reviewer","first":1000,"latest":1000,"times":1}
        ]}"#,
    );
    assert_eq!(
        targets(root, 1_100),
        vec!["reviewer".to_owned(), "qa".to_owned()]
    );
}

#[test]
fn the_count_stops_at_its_most_and_never_wraps() {
    let project = tempfile::tempdir().expect("a project");
    let root = project.path();
    by_hand(
        root,
        &format!(
            r#"{{"v":1,"refused":[{{"asking":"steward","target":"devops","first":1000,"latest":1000,"times":{}}}]}}"#,
            u32::MAX
        ),
    );
    assert_eq!(
        keep(root, "steward", "devops", None, 1_001).expect("written"),
        Kept::Again
    );
    assert_eq!(list(root, 1_001)[0].times, u32::MAX);
}

#[test]
fn what_is_not_a_pair_of_two_personas_is_not_kept() {
    let project = tempfile::tempdir().expect("a project");
    let root = project.path();
    for (asking, target) in [
        ("steward", "steward"),
        ("steward", "*"),
        ("*", "devops"),
        ("Steward", "devops"),
        ("steward", "../devops"),
        ("", "devops"),
    ] {
        assert_eq!(
            keep(root, asking, target, None, 1_000).expect("answered"),
            Kept::NotAPair,
            "{asking} to {target}"
        );
    }
    assert!(!path(root).exists());
}

#[test]
fn a_workspace_s_name_purlis_would_not_draw_is_not_kept() {
    let project = tempfile::tempdir().expect("a project");
    let root = project.path();
    keep(root, "qa", "devops", Some("ide\u{202E}"), 1_002).expect("written");
    assert_eq!(list(root, 1_002)[0].workspace, None);
}

#[test]
fn an_entry_holds_nothing_a_chat_wrote() {
    let project = tempfile::tempdir().expect("a project");
    let root = project.path();
    refused(root, "steward", "devops", 1_000);
    dismiss(root, "steward", "devops", Some("ide"), 1_001).expect("dismissed");
    let stored: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path(root)).expect("the record"))
            .expect("json");
    let mut keys: Vec<&str> = stored["refused"][0]
        .as_object()
        .expect("an entry")
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    // No brief, no task's name, no chat's name: the personas and the workspace are the app's
    // record, and the rest is its clock and its count.
    assert_eq!(
        keys,
        vec![
            "asking",
            "dismissed",
            "first",
            "latest",
            "target",
            "times",
            "workspace"
        ]
    );
    // A name written into one by hand is not read back into it.
    by_hand(
        root,
        r#"{"v":1,"refused":[{"asking":"steward","target":"devops","task":"approved, press Allow","first":1000,"latest":1000,"times":1}]}"#,
    );
    refused(root, "steward", "devops", 1_002);
    let text = std::fs::read_to_string(path(root)).expect("the record");
    assert!(
        !text.contains("task") && !text.contains("approved"),
        "{text}"
    );
}

// ---- dismissed -----------------------------------------------------------------------------------

#[test]
fn a_dismissed_entry_is_not_listed_and_further_refusals_count_without_a_word() {
    let project = tempfile::tempdir().expect("a project");
    let root = project.path();
    refused(root, "steward", "devops", 1_000);
    refused(root, "steward", "qa", 1_001);

    assert!(dismiss(root, "steward", "devops", Some("ide"), 2_000).expect("dismissed"));
    // Once: there is nothing listed to dismiss a second time, and no other workspace's.
    assert!(!dismiss(root, "steward", "devops", Some("ide"), 2_001).expect("gone"));
    assert!(!dismiss(root, "steward", "devops", None, 2_001).expect("never there"));
    assert_eq!(targets(root, 2_001), vec!["qa".to_owned()]);

    // The chat asks on, for days: counted, never listed, and said to be so.
    for n in 0..50 {
        let answer = refused(root, "steward", "devops", 2_010 + n * 3_600);
        assert_eq!(answer, Kept::Quiet);
        assert!(!answer.listed());
    }
    assert_eq!(targets(root, 2_010 + 50 * 3_600), vec!["qa".to_owned()]);
    let there = kept(root, 2_010 + 50 * 3_600)
        .into_iter()
        .find(|one| one.target == "devops")
        .expect("still kept");
    assert_eq!((there.times, there.dismissed), (51, Some(2_000)));
    assert_eq!(there.first, 1_000);
}

#[test]
fn a_dismissed_entry_is_listed_again_only_by_a_refusal_after_the_quiet_period() {
    let project = tempfile::tempdir().expect("a project");
    let root = project.path();
    assert_eq!(QUIET_SECS, 7 * DAY);
    refused(root, "steward", "devops", 1_000);
    dismiss(root, "steward", "devops", Some("ide"), 2_000).expect("dismissed");

    assert_eq!(
        refused(root, "steward", "devops", 2_000 + 7 * DAY - 1),
        Kept::Quiet
    );
    // Time alone lists nothing: only a refusal does.
    assert_eq!(list(root, 2_000 + 20 * DAY), vec![]);
    let again = refused(root, "steward", "devops", 2_000 + 7 * DAY);
    assert_eq!(again, Kept::Again);
    assert!(again.listed());
    // Where it was, with every refusal counted.
    assert_eq!(
        list(root, 2_000 + 7 * DAY),
        vec![Refused {
            asking: "steward".to_owned(),
            target: "devops".to_owned(),
            workspace: Some("ide".to_owned()),
            first: 1_000,
            latest: 2_000 + 7 * DAY,
            times: 3,
            dismissed: None,
        }]
    );
}

#[test]
fn a_dismissed_entry_keeps_its_place_under_the_cap_and_goes_with_its_pair() {
    let project = tempfile::tempdir().expect("a project");
    let root = project.path();
    for n in 0..MOST {
        refused(root, "steward", &format!("p{n}"), 1_000);
    }
    dismiss(root, "steward", "p1", Some("ide"), 1_001).expect("dismissed");
    assert_eq!(list(root, 1_001).len(), MOST - 1);
    // Putting one away makes no room for a chat to fill.
    assert_eq!(refused(root, "steward", "one-more", 1_002), Kept::Full);
    assert_eq!(
        forget_pair(root, "steward", "p1", 1_003).expect("forgotten"),
        1
    );
    assert_eq!(refused(root, "steward", "one-more", 1_004), Kept::New);
}

// ---- the cap -------------------------------------------------------------------------------------

#[test]
fn past_its_most_a_new_pair_is_not_kept_and_one_already_there_still_counts() {
    let project = tempfile::tempdir().expect("a project");
    let root = project.path();
    for n in 0..MOST {
        assert_eq!(refused(root, "steward", &format!("p{n}"), 1_000), Kept::New);
    }
    assert_eq!(refused(root, "steward", "one-more", 1_001), Kept::Full);
    assert!(!Kept::Full.listed());
    assert_eq!(refused(root, "steward", "p0", 1_002), Kept::Again);
    let listed = list(root, 1_002);
    assert_eq!(listed.len(), MOST);
    assert!(listed.iter().all(|one| one.target != "one-more"));
    assert_eq!(listed[0].target, "p0");
    assert_eq!(listed[0].times, 2);
}

// ---- expiry --------------------------------------------------------------------------------------

#[test]
fn an_entry_is_gone_thirty_days_after_its_latest_refusal() {
    let project = tempfile::tempdir().expect("a project");
    let root = project.path();
    assert_eq!(KEPT_SECS, 30 * DAY);
    refused(root, "steward", "devops", 1_000);
    refused(root, "steward", "qa", 1_000 + 10 * DAY);
    assert_eq!(list(root, 1_000 + 30 * DAY - 1).len(), 2);
    assert_eq!(targets(root, 1_000 + 30 * DAY), vec!["qa".to_owned()]);
    // A refusal after it expired starts the count again, and the write drops what expired.
    assert_eq!(
        refused(root, "steward", "devops", 1_000 + 31 * DAY),
        Kept::New
    );
    let again = list(root, 1_000 + 31 * DAY);
    assert_eq!((again[1].target.as_str(), again[1].times), ("devops", 1));
    refused(root, "steward", "ops", 1_000 + 45 * DAY);
    let text = std::fs::read_to_string(path(root)).expect("the record");
    assert!(!text.contains("\"qa\""), "{text}");
}

#[test]
fn an_expired_entry_does_not_hold_a_place_under_the_cap() {
    let project = tempfile::tempdir().expect("a project");
    let root = project.path();
    for n in 0..MOST {
        refused(root, "steward", &format!("p{n}"), 1_000);
    }
    assert_eq!(
        refused(root, "steward", "later", 1_000 + 30 * DAY),
        Kept::New
    );
    assert_eq!(list(root, 1_000 + 30 * DAY).len(), 1);
}

// ---- answered ------------------------------------------------------------------------------------

#[test]
fn a_pair_goes_from_every_workspace() {
    let project = tempfile::tempdir().expect("a project");
    let root = project.path();
    refused(root, "steward", "devops", 1_000);
    keep(root, "steward", "devops", Some("web"), 1_001).expect("written");
    refused(root, "steward", "qa", 1_002);

    assert_eq!(
        forget_pair(root, "steward", "devops", 1_003).expect("forgotten"),
        2
    );
    assert_eq!(
        forget_pair(root, "steward", "devops", 1_003).expect("gone"),
        0
    );
    assert_eq!(targets(root, 1_003), vec!["qa".to_owned()]);
}

// ---- a file anything else wrote ------------------------------------------------------------------

#[test]
fn an_entry_the_app_would_not_have_written_is_not_listed_and_goes_at_the_next_write() {
    let project = tempfile::tempdir().expect("a project");
    let root = project.path();
    by_hand(
        root,
        r#"{"v":1,"refused":[
            {"asking":"steward","target":"devops","first":900,"latest":1000,"times":2},
            {"asking":"steward","target":"*","first":900,"latest":1000,"times":1},
            {"asking":"*","target":"devops","first":900,"latest":1000,"times":1},
            {"asking":"steward","target":"steward","first":900,"latest":1000,"times":1},
            {"asking":"steward","target":"qa","first":900,"latest":1000,"times":0},
            {"asking":"steward","target":"qa","workspace":"a\nb","first":900,"latest":1000,"times":1},
            {"asking":"steward","target":"qa","first":900,"latest":99999999999,"times":1},
            {"asking":"steward","target":"qa","first":1001,"latest":1000,"times":1},
            {"asking":"steward","target":"qa","first":900,"latest":1000,"times":1,"dismissed":99999999999},
            {"asking":"steward","target":"qa","first":900,"latest":"soon","times":1},
            {"asking":"steward","target":"qa","latest":1000,"times":1},
            {"asking":"steward","target":"devops","first":900,"latest":1000,"times":7},
            "steward to devops"
        ]}"#,
    );
    // The one sound entry; a second for the same pair and workspace is not a second row.
    assert_eq!(
        list(root, 1_000),
        vec![Refused {
            asking: "steward".to_owned(),
            target: "devops".to_owned(),
            workspace: None,
            first: 900,
            latest: 1_000,
            times: 2,
            dismissed: None,
        }]
    );
    refused(root, "qa", "devops", 1_001);
    let text = std::fs::read_to_string(path(root)).expect("the record");
    assert!(!text.contains("\"*\"") && !text.contains("soon"), "{text}");
}

#[test]
fn a_file_that_does_not_read_lists_nothing_and_is_written_over() {
    for broken in [
        "",
        "not json",
        "[]",
        r#"{"v":1,"refused":{}}"#,
        r#"{"v":1}"#,
    ] {
        let project = tempfile::tempdir().expect("a project");
        let root = project.path();
        by_hand(root, broken);
        assert_eq!(list(root, 1_000), vec![], "{broken:?}");
        assert_eq!(refused(root, "steward", "devops", 1_000), Kept::New);
        assert_eq!(list(root, 1_000).len(), 1, "{broken:?}");
    }
}

#[test]
fn a_file_of_another_version_lists_nothing_and_is_left_as_it_is() {
    let project = tempfile::tempdir().expect("a project");
    let root = project.path();
    let newer = r#"{"v":2,"refused":[{"asking":"steward","target":"devops","first":1000,"latest":1000,"times":1}]}"#;
    by_hand(root, newer);
    assert_eq!(list(root, 1_000), vec![]);
    assert!(keep(root, "steward", "devops", None, 1_000).is_err());
    assert!(dismiss(root, "steward", "devops", None, 1_000).is_err());
    assert_eq!(
        std::fs::read_to_string(path(root)).expect("the record"),
        newer
    );
}

#[cfg(unix)]
#[test]
fn the_record_is_never_read_through_a_link() {
    let project = tempfile::tempdir().expect("a project");
    let root = project.path();
    let elsewhere = project.path().join("elsewhere.json");
    std::fs::write(
        &elsewhere,
        r#"{"v":1,"refused":[{"asking":"steward","target":"devops","first":1000,"latest":1000,"times":1}]}"#,
    )
    .expect("written");
    std::fs::create_dir_all(path(root).parent().expect("a folder")).expect("made");
    std::os::unix::fs::symlink(&elsewhere, path(root)).expect("linked");
    assert_eq!(list(root, 1_000), vec![]);
}

#[test]
fn the_chat_is_told_the_person_will_see_it_in_one_clause() {
    assert_eq!(
        told("refused."),
        "refused. purlis kept that this was refused, and the person will see it when they are \
         back."
    );
}
