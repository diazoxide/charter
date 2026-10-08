//! A grant limited to one workspace (#1505): what it covers and what it does not, at each
//! level; how each record spells it so that nothing reads it as a grant that holds
//! everywhere; and what a workspace that goes away does to it. Every expected answer is
//! written out.
//!
//! The tests that write the project's committed file are at the end, under their own
//! heading: a sandbox that denies writing that file cannot run them.

use std::path::Path;

use super::*;
use crate::dispatchgrant::{Committed, Covers, InForce, committed, covers};
use crate::sandbox::policy::Locks;

fn pair(asking: &str, target: &str) -> Pair {
    Pair::new(asking, target).expect("a pair")
}

fn limited(asking: &str, target: &str, workspace: &str) -> Limited {
    Limited::new(asking, target, workspace).expect("a limited grant")
}

/// A project with the workspaces `names`.
fn project(names: &[&str]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("a project");
    std::fs::create_dir_all(dir.path().join("workspaces")).expect("workspaces");
    for name in names {
        make(dir.path(), name);
    }
    dir
}

fn make(root: &Path, workspace: &str) {
    std::fs::create_dir_all(root.join("workspaces").join(workspace)).expect("a workspace");
}

fn remove(root: &Path, workspace: &str) {
    std::fs::remove_dir_all(root.join("workspaces").join(workspace)).expect("removed");
}

/// What a chat running as `asking` is answered for `target`, for a task in `works_in`.
fn asked(root: &Path, asking: &str, target: &str, works_in: Option<&str>) -> Covers {
    let grants = InForce::read(root, Vec::new()).for_task_in(works_in);
    covers(Some(asking), target, &grants, &Locks::none())
}

/// This machine's record for the project at `root`, as JSON.
fn record(root: &Path) -> serde_json::Value {
    let text = std::fs::read_to_string(local::path(root)).expect("the record");
    serde_json::from_str(&text).expect("JSON")
}

fn by_hand(root: &Path, json: &str) {
    let path = local::path(root);
    std::fs::create_dir_all(path.parent().expect("a folder")).expect("made");
    std::fs::write(path, json).expect("written");
}

// ---- which workspace -----------------------------------------------------------------------------

#[test]
fn the_workspace_is_the_one_the_task_works_in_whichever_chat_asks() {
    // A handoff's own word first, then what the dispatch names or cuts a worktree in, then
    // the asking chat's own; none of them is the project's root.
    assert_eq!(
        works_in(Some("web"), Some("runners"), Some("ide")),
        Some("web")
    );
    assert_eq!(
        works_in(None, Some("runners"), Some("ide")),
        Some("runners")
    );
    assert_eq!(works_in(None, None, Some("ide")), Some("ide"));
    assert_eq!(works_in(None, None, None), None);
}

// ---- me on this machine ---------------------------------------------------------------------------

#[test]
fn my_grant_for_one_workspace_covers_a_task_there_and_nothing_anywhere_else() {
    let dir = project(&["runners", "web"]);
    let root = dir.path();
    grant_yours(
        root,
        &pair("steward", "devops"),
        &Within::Workspace("runners".to_owned()),
    )
    .expect("kept");

    assert_eq!(
        asked(root, "steward", "devops", Some("runners")),
        Covers::Covered
    );
    // Another workspace, the project's root, another target, and the pair the other way.
    assert_eq!(
        asked(root, "steward", "devops", Some("web")),
        Covers::NeedsGrant
    );
    assert_eq!(asked(root, "steward", "devops", None), Covers::NeedsGrant);
    assert_eq!(
        asked(root, "steward", "qa", Some("runners")),
        Covers::NeedsGrant
    );
    assert_eq!(
        asked(root, "devops", "steward", Some("runners")),
        Covers::NeedsGrant
    );
    // A workspace's name is matched as it is spelled.
    assert_eq!(
        asked(root, "steward", "devops", Some("Runners")),
        Covers::NeedsGrant
    );
}

#[test]
fn a_reader_that_never_says_where_the_task_works_counts_no_limited_grant() {
    let dir = project(&["runners"]);
    let root = dir.path();
    grant_yours(
        root,
        &pair("steward", "devops"),
        &Within::Workspace("runners".to_owned()),
    )
    .expect("kept");
    let grants = InForce::read(root, Vec::new());
    assert_eq!(grants.works_in, None);
    assert_eq!(
        covers(Some("steward"), "devops", &grants, &Locks::none()),
        Covers::NeedsGrant
    );
    assert_eq!(grants.level_of(Some("steward"), "devops"), None);
    assert_eq!(grants.named_level_of(Some("steward"), "devops"), None);
}

#[test]
fn a_limited_grant_is_kept_under_a_key_of_its_own_and_never_among_the_ones_that_hold_everywhere() {
    let dir = project(&["runners"]);
    let root = dir.path();
    grant_yours(
        root,
        &pair("steward", "devops"),
        &Within::Workspace("runners".to_owned()),
    )
    .expect("kept");
    let kept = record(root);
    // What a build that does not know the condition reads: nothing.
    assert_eq!(kept.get("dispatch_mine"), None);
    assert_eq!(kept.get("dispatch_any"), None);
    assert_eq!(
        kept["dispatch_mine_in"],
        serde_json::json!([
            {"asking": "steward", "target": "devops", "workspace": "runners", "seen": 0}
        ])
    );
    assert!(local::granted_dispatch(root).is_empty());
    assert!(crate::dispatchgrant::yours(root).is_empty());
}

#[test]
fn a_grant_written_before_the_condition_holds_in_any_workspace() {
    let dir = project(&["runners", "web"]);
    let root = dir.path();
    // The record as a build before the condition wrote it.
    by_hand(
        root,
        r#"{"dispatch_mine": [{"asking": "steward", "target": "devops"}],
            "dispatch_any": ["qa"]}"#,
    );
    for place in [Some("runners"), Some("web"), None] {
        assert_eq!(asked(root, "steward", "devops", place), Covers::Covered);
        assert_eq!(asked(root, "qa", "devops", place), Covers::Covered);
    }
    // And the project's file, in the spelling it always had.
    let read = committed(Some("[dispatch.grants]\nsteward = [\"devops\", \"*\"]\n"));
    assert_eq!(read.pairs, [pair("steward", "devops")]);
    assert_eq!(read.any, ["steward"]);
    assert!(read.limited.is_empty());
    assert!(read.refused.is_empty());
}

// ---- any persona, limited -------------------------------------------------------------------------

#[test]
fn any_persona_limited_to_a_workspace_covers_every_persona_there_and_none_elsewhere() {
    let dir = project(&["runners", "web"]);
    let root = dir.path();
    local::keep_dispatch_in(
        root,
        Record::Mine,
        &limited("steward", ANY, "runners").on_disk(0),
    )
    .expect("kept");
    assert_eq!(
        asked(root, "steward", "devops", Some("runners")),
        Covers::Covered
    );
    assert_eq!(
        asked(root, "steward", "qa", Some("runners")),
        Covers::Covered
    );
    assert_eq!(
        asked(root, "steward", "devops", Some("web")),
        Covers::NeedsGrant
    );
    assert_eq!(asked(root, "steward", "devops", None), Covers::NeedsGrant);
    // One-way, and only for that persona's chats.
    assert_eq!(
        asked(root, "devops", "steward", Some("runners")),
        Covers::NeedsGrant
    );
    // It covers nothing that is not a persona's name.
    assert_eq!(
        asked(root, "steward", "../x", Some("runners")),
        Covers::NeedsGrant
    );
    // It names no pair: the rule for a chat nobody is at does not count it.
    let grants = InForce::read(root, Vec::new());
    assert_eq!(
        grants.named_level_in(Some("steward"), "devops", Some("runners")),
        None
    );
}

#[test]
fn a_star_written_as_the_target_of_a_limited_pair_by_hand_grants_nothing() {
    let dir = project(&["runners"]);
    let root = dir.path();
    // `*` as a target with no `any`, and `any` with a persona's name: neither is a grant.
    by_hand(
        root,
        r#"{"dispatch_mine_in": [
            {"asking": "steward", "target": "*", "workspace": "runners"},
            {"asking": "steward", "target": "devops", "any": true, "workspace": "runners"},
            {"asking": "*", "target": "devops", "workspace": "runners"},
            {"asking": "steward", "target": "qa", "workspace": "../runners"}
        ]}"#,
    );
    assert!(yours(root).is_empty());
    for target in ["devops", "qa"] {
        assert_eq!(
            asked(root, "steward", target, Some("runners")),
            Covers::NeedsGrant
        );
    }
}

#[test]
fn a_record_that_does_not_read_grants_nothing() {
    let dir = project(&["runners"]);
    let root = dir.path();
    for broken in [
        r#"{"dispatch_mine_in": "steward -> devops in runners"}"#,
        r#"{"dispatch_mine_in": [{"asking": "steward", "target": "devops"}]}"#,
        r#"{"dispatch_mine_in": [["steward", "devops", "runners"]]}"#,
        "{not json",
    ] {
        by_hand(root, broken);
        assert!(yours(root).is_empty(), "{broken}");
        assert_eq!(
            asked(root, "steward", "devops", Some("runners")),
            Covers::NeedsGrant,
            "{broken}"
        );
    }
}

// ---- never is not limited -------------------------------------------------------------------------

#[test]
fn a_never_holds_in_every_workspace_whatever_is_granted_for_one() {
    let dir = project(&["runners", "web"]);
    let root = dir.path();
    grant_yours(
        root,
        &pair("steward", "devops"),
        &Within::Workspace("runners".to_owned()),
    )
    .expect("kept");
    crate::dispatchgrant::never(root, &pair("steward", "devops")).expect("said");
    for place in [Some("runners"), Some("web"), None] {
        assert_eq!(asked(root, "steward", "devops", place), Covers::Never);
    }
}

// ---- a chat nobody is at, crossing into another workspace ---------------------------------------

#[test]
fn a_chat_nobody_is_at_crosses_only_under_a_grant_that_names_the_pair_and_covers_where_it_goes() {
    use crate::dispatchplace::nobody_to_ask;
    let to_beta = InForce {
        limited: vec![(Level::You, limited("steward", "devops", "beta"))],
        ..InForce::default()
    };
    // Limited to the workspace the new chat is to work in: it may.
    assert_eq!(
        nobody_to_ask(Some("steward"), Some("devops"), &to_beta, "beta"),
        None
    );
    // Limited to any other, the asking chat's own included: it may not.
    assert!(nobody_to_ask(Some("steward"), Some("devops"), &to_beta, "alpha").is_some());
    // The project's, accepted here, counts the same way.
    let ours = InForce {
        limited: vec![(Level::Project, limited("steward", "devops", "beta"))],
        ..InForce::default()
    };
    assert_eq!(
        nobody_to_ask(Some("steward"), Some("devops"), &ours, "beta"),
        None
    );
    assert!(nobody_to_ask(Some("steward"), Some("devops"), &ours, "alpha").is_some());
    // What the reader said of where the task works does not stand in for the destination.
    let said_alpha = to_beta.clone().for_task_in(Some("alpha"));
    assert_eq!(
        nobody_to_ask(Some("steward"), Some("devops"), &said_alpha, "beta"),
        None
    );
    let said_beta = to_beta.clone().for_task_in(Some("beta"));
    assert!(nobody_to_ask(Some("steward"), Some("devops"), &said_beta, "alpha").is_some());
    // "Any persona" limited to the destination names no pair, so it does not count there.
    let any = InForce {
        limited: vec![(Level::You, limited("steward", ANY, "beta"))],
        ..InForce::default()
    };
    assert!(nobody_to_ask(Some("steward"), Some("devops"), &any, "beta").is_some());
    // And only the pair it names, one way.
    assert!(nobody_to_ask(Some("devops"), Some("steward"), &to_beta, "beta").is_some());
    assert!(nobody_to_ask(Some("steward"), Some("qa"), &to_beta, "beta").is_some());
}

// ---- a workspace that is gone, and one made later under its name --------------------------------

#[test]
fn a_grant_for_a_workspace_that_is_not_there_covers_nothing_and_says_why() {
    let dir = project(&["web"]);
    let root = dir.path();
    local::keep_dispatch_in(
        root,
        Record::Mine,
        &limited("steward", "devops", "runners").on_disk(0),
    )
    .expect("kept");
    assert_eq!(
        asked(root, "steward", "devops", Some("runners")),
        Covers::NeedsGrant
    );
    let seen = noticed(root);
    assert!(!seen.is_there("runners"));
    assert_eq!(
        seen.why_not("runners", 0).as_deref(),
        Some(
            "runners is not a workspace of this project now, so this grant covers nothing. A \
             grant does not follow a workspace that was renamed: set its workspace again, or \
             remove it."
        )
    );
    // The grant is still in its record: nothing was moved.
    assert_eq!(yours(root), [(limited("steward", "devops", "runners"), 0)]);
}

#[test]
fn a_workspace_made_under_the_name_of_one_that_was_removed_inherits_no_grant() {
    let dir = project(&["runners"]);
    let root = dir.path();
    let within = Within::Workspace("runners".to_owned());
    grant_yours(root, &pair("steward", "devops"), &within).expect("kept");
    assert_eq!(
        asked(root, "steward", "devops", Some("runners")),
        Covers::Covered
    );

    // It goes, and a dispatch is judged while it is gone.
    remove(root, "runners");
    assert_eq!(
        asked(root, "steward", "devops", Some("runners")),
        Covers::NeedsGrant
    );
    assert_eq!(
        local::dispatch_workspaces(root),
        [KnownWorkspace {
            name: "runners".to_owned(),
            gone: 1,
            away: true
        }]
    );

    // Another is made under the name: the grant is not its.
    make(root, "runners");
    assert_eq!(
        asked(root, "steward", "devops", Some("runners")),
        Covers::NeedsGrant
    );
    assert_eq!(
        noticed(root).why_not("runners", 0).as_deref(),
        Some(
            "A workspace named runners was removed or renamed after this grant was made, so it \
             covers nothing in the one that is there now."
        )
    );
    // Judged again and again, it is counted gone once.
    assert_eq!(local::dispatch_workspaces(root)[0].gone, 1);
    assert!(!local::dispatch_workspaces(root)[0].away);

    // The person sets the grant's workspace again: their yes for the one that is there now.
    assert!(set_yours(root, "steward", "devops", &within, &within).expect("set"));
    assert_eq!(
        asked(root, "steward", "devops", Some("runners")),
        Covers::Covered
    );
    assert_eq!(yours(root), [(limited("steward", "devops", "runners"), 1)]);

    // And a grant made now is for the one there now, too.
    grant_yours(root, &pair("steward", "qa"), &within).expect("kept");
    assert_eq!(
        asked(root, "steward", "qa", Some("runners")),
        Covers::Covered
    );
}

#[test]
fn an_absence_nothing_looked_at_is_not_counted_and_the_new_workspace_has_the_grant() {
    // The limit, pinned: removed and made again with no dispatch judged and Settings not
    // read in between. A workspace has nothing but its name to tell the two apart by.
    let dir = project(&["runners"]);
    let root = dir.path();
    grant_yours(
        root,
        &pair("steward", "devops"),
        &Within::Workspace("runners".to_owned()),
    )
    .expect("kept");
    remove(root, "runners");
    make(root, "runners");
    assert_eq!(
        asked(root, "steward", "devops", Some("runners")),
        Covers::Covered
    );
}

#[test]
fn a_workspace_reached_through_a_link_is_no_workspace_a_grant_holds_in() {
    #[cfg(unix)]
    {
        let dir = project(&["web"]);
        let root = dir.path();
        std::os::unix::fs::symlink(
            root.join("workspaces").join("web"),
            root.join("workspaces").join("runners"),
        )
        .expect("a link");
        local::keep_dispatch_in(
            root,
            Record::Mine,
            &limited("steward", "devops", "runners").on_disk(0),
        )
        .expect("kept");
        assert!(!Seen::read(root).is_there("runners"));
        assert_eq!(
            asked(root, "steward", "devops", Some("runners")),
            Covers::NeedsGrant
        );
    }
}

#[test]
fn nothing_is_counted_gone_where_the_workspaces_cannot_be_looked_at() {
    let dir = tempfile::tempdir().expect("a project");
    let root = dir.path();
    // No `workspaces/` at all: nothing can be said of any name.
    local::keep_dispatch_in(
        root,
        Record::Mine,
        &limited("steward", "devops", "runners").on_disk(0),
    )
    .expect("kept");
    assert_eq!(
        asked(root, "steward", "devops", Some("runners")),
        Covers::NeedsGrant
    );
    assert!(local::dispatch_workspaces(root).is_empty());
    assert_eq!(
        noticed(root).why_not("runners", 0).as_deref(),
        Some(
            "purlis could not look at this project's workspaces, so this grant covers nothing \
             for now."
        )
    );
}

// ---- no grant for a name that is no workspace ----------------------------------------------------

#[test]
fn no_standing_grant_is_made_accepted_or_set_for_a_name_that_is_no_workspace() {
    let dir = project(&["web"]);
    let root = dir.path();
    let runners = Within::Workspace("runners".to_owned());
    // For me: refused, with nothing written and nothing counted for the name.
    let refused = grant_yours(root, &pair("steward", "devops"), &runners).expect_err("refused");
    assert_eq!(refused.kind(), std::io::ErrorKind::NotFound);
    assert_eq!(
        refused.to_string(),
        "runners is not a workspace of this project now, so purlis keeps no grant for it."
    );
    assert!(yours(root).is_empty());
    assert!(!local::path(root).exists(), "nothing was written");
    // Set from one that holds everywhere: refused, and the grant is as it was.
    grant_yours(root, &pair("steward", "devops"), &Within::Any).expect("kept");
    assert!(set_yours(root, "steward", "devops", &Within::Any, &runners).is_err());
    assert!(yours_holds(root, "steward", "devops", &Within::Any));
    assert!(yours(root).is_empty());
    // So a workspace made under the name later has nothing waiting for it.
    make(root, "runners");
    assert!(yours(root).is_empty());
    assert!(local::dispatch_workspaces(root).is_empty());
}

// ---- purlis's own workspace commands -------------------------------------------------------------

fn now() -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::from_timestamp(1_760_000_000, 0).expect("a time")
}

/// A project whose workspaces `names` purlis itself made.
fn made_by_purlis(names: &[&str]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("a project");
    for name in names {
        crate::wscmd::ensure::ensure(dir.path(), name, now(), "fixture").expect("made");
    }
    dir
}

#[test]
fn a_workspace_purlis_removes_and_makes_again_inherits_no_grant_with_nothing_judged_between() {
    let dir = made_by_purlis(&["runners"]);
    let root = dir.path();
    grant_yours(
        root,
        &pair("steward", "devops"),
        &Within::Workspace("runners".to_owned()),
    )
    .expect("kept");

    let mut said = Vec::new();
    let done = crate::wscmd::remove::remove(root, "runners", true, &mut |line| {
        said.push(line.to_string());
    });
    assert_eq!(done.code, 0, "{said:?}");
    assert!(
        said.iter().any(|line| line.contains(
            "1 dispatch grant holds in 'runners' only. It does not go to a workspace made as \
             'runners' later: it covers nothing now."
        )),
        "{said:?}"
    );
    // No dispatch is judged and Settings is not read: the removal itself counted the name.
    assert_eq!(local::dispatch_workspaces(root)[0].gone, 1);
    crate::wscmd::ensure::ensure(root, "runners", now(), "fixture").expect("made again");
    assert_eq!(
        asked(root, "steward", "devops", Some("runners")),
        Covers::NeedsGrant
    );
}

#[test]
fn a_grant_does_not_follow_a_rename_and_a_workspace_made_under_the_old_name_inherits_none() {
    let dir = made_by_purlis(&["runners"]);
    let root = dir.path();
    grant_yours(
        root,
        &pair("steward", "devops"),
        &Within::Workspace("runners".to_owned()),
    )
    .expect("kept");

    let mut said = Vec::new();
    crate::wscmd::rename::rename(
        &crate::wscmd::rename::Request {
            root,
            old: "runners",
            new: "ci",
            running: &[],
            config_root: None,
        },
        &mut |line| said.push(line.to_string()),
    );
    assert!(root.join("workspaces/ci").is_dir(), "{said:?}");
    assert!(!root.join("workspaces/runners").exists(), "{said:?}");
    // The grant is where it was, names the old name, and covers nothing under either.
    assert_eq!(yours(root), [(limited("steward", "devops", "runners"), 0)]);
    assert_eq!(
        asked(root, "steward", "devops", Some("ci")),
        Covers::NeedsGrant
    );
    assert_eq!(local::dispatch_workspaces(root)[0].gone, 1);
    // A workspace made as the old name, with nothing judged in between, has none of it.
    crate::wscmd::ensure::ensure(root, "runners", now(), "fixture").expect("made");
    assert_eq!(
        asked(root, "steward", "devops", Some("runners")),
        Covers::NeedsGrant
    );
    // The person sets the grant's workspace to the new name: their yes for it.
    let (old, new) = (
        Within::Workspace("runners".to_owned()),
        Within::Workspace("ci".to_owned()),
    );
    assert!(set_yours(root, "steward", "devops", &old, &new).expect("set"));
    assert_eq!(
        asked(root, "steward", "devops", Some("ci")),
        Covers::Covered
    );
}

#[test]
fn a_workspace_purlis_makes_under_a_name_a_grant_still_holds_is_counted_before_it_is_made() {
    // A grant left for a name that is no workspace (written by hand, or by an earlier build):
    // whatever purlis makes under the name looks first, so the grant is not its.
    let dir = made_by_purlis(&["web"]);
    let root = dir.path();
    local::keep_dispatch_in(
        root,
        Record::Mine,
        &limited("steward", "devops", "runners").on_disk(0),
    )
    .expect("kept");
    crate::wscmd::ensure::ensure(root, "runners", now(), "fixture").expect("made");
    assert_eq!(
        asked(root, "steward", "devops", Some("runners")),
        Covers::NeedsGrant
    );
    assert_eq!(naming(root, "runners"), 1);
}

#[test]
fn what_a_rename_and_a_removal_say_of_the_grants_they_left_behind() {
    assert_eq!(
        left_behind_said(1, "runners", Some("ci")),
        "1 dispatch grant holds in 'runners' only. It does not follow the rename: it covers \
         nothing in 'ci', and nothing in a workspace made as 'runners' later. Set its workspace \
         again, or remove it, in Settings › Project › Dispatch."
    );
    assert_eq!(
        left_behind_said(2, "runners", None),
        "2 dispatch grants hold in 'runners' only. They do not go to a workspace made as \
         'runners' later: they cover nothing now. Set the workspace of each again, or remove \
         it, in Settings › Project › Dispatch."
    );
}

// ---- narrowing and widening, for me --------------------------------------------------------------

#[test]
fn my_grant_is_narrowed_to_a_workspace_and_widened_again_each_in_one_write() {
    let dir = project(&["runners", "web"]);
    let root = dir.path();
    let (any, runners, web) = (
        Within::Any,
        Within::Workspace("runners".to_owned()),
        Within::Workspace("web".to_owned()),
    );
    grant_yours(root, &pair("steward", "devops"), &any).expect("kept");
    assert!(yours_holds(root, "steward", "devops", &any));

    // Narrowed: it leaves the record that holds everywhere.
    assert!(set_yours(root, "steward", "devops", &any, &runners).expect("set"));
    assert!(crate::dispatchgrant::yours(root).is_empty());
    assert!(yours_holds(root, "steward", "devops", &runners));
    assert_eq!(
        asked(root, "steward", "devops", Some("runners")),
        Covers::Covered
    );
    assert_eq!(
        asked(root, "steward", "devops", Some("web")),
        Covers::NeedsGrant
    );

    // Moved to another workspace.
    assert!(set_yours(root, "steward", "devops", &runners, &web).expect("set"));
    assert_eq!(
        asked(root, "steward", "devops", Some("runners")),
        Covers::NeedsGrant
    );
    assert_eq!(
        asked(root, "steward", "devops", Some("web")),
        Covers::Covered
    );

    // A second one for the pair, then widened: the one that holds everywhere replaces both.
    grant_yours(root, &pair("steward", "devops"), &runners).expect("kept");
    assert!(set_yours(root, "steward", "devops", &web, &any).expect("set"));
    assert!(yours(root).is_empty());
    assert_eq!(
        crate::dispatchgrant::yours(root),
        [pair("steward", "devops")]
    );
    assert_eq!(asked(root, "steward", "devops", None), Covers::Covered);

    // A grant that is not there as said is not changed, and nothing is made.
    assert!(!set_yours(root, "steward", "qa", &any, &runners).expect("asked"));
    assert!(!set_yours(root, "steward", "devops", &web, &runners).expect("asked"));
    assert!(yours(root).is_empty());

    // "Any persona" narrows and widens the same way.
    local::grant_dispatch_any(root, "qa").expect("kept");
    assert!(set_yours(root, "qa", ANY, &any, &runners).expect("set"));
    assert!(crate::dispatchgrant::any_yours(root).is_empty());
    assert_eq!(
        asked(root, "qa", "devops", Some("runners")),
        Covers::Covered
    );
    assert_eq!(asked(root, "qa", "devops", Some("web")), Covers::NeedsGrant);
    assert!(set_yours(root, "qa", ANY, &runners, &any).expect("set"));
    assert_eq!(crate::dispatchgrant::any_yours(root), ["qa"]);
}

#[test]
fn a_grant_is_set_to_no_workspace_the_project_lacks_and_to_no_name_that_is_none() {
    let dir = project(&["runners"]);
    let root = dir.path();
    assert_eq!(
        can_set(
            root,
            "steward",
            "devops",
            &Within::Workspace("runners".to_owned())
        ),
        Ok(())
    );
    assert_eq!(
        can_set(
            root,
            "steward",
            "devops",
            &Within::Workspace("web".to_owned())
        ),
        Err("web is not a workspace of this project now, so nothing was changed.".to_owned())
    );
    assert!(
        can_set(
            root,
            "steward",
            "devops",
            &Within::Workspace("../x".to_owned())
        )
        .is_err()
    );
    assert!(
        can_set(
            root,
            "steward",
            "steward",
            &Within::Workspace("runners".to_owned())
        )
        .is_err()
    );
    assert!(
        can_set(
            root,
            "*",
            "devops",
            &Within::Workspace("runners".to_owned())
        )
        .is_err()
    );
}

// ---- a persona that changed hands ----------------------------------------------------------------

#[test]
fn a_limited_grant_that_names_a_name_that_changed_hands_ends() {
    let dir = project(&["runners"]);
    let root = dir.path();
    let within = Within::Workspace("runners".to_owned());
    grant_yours(root, &pair("steward", "devops"), &within).expect("kept");
    grant_yours(root, &pair("steward", "qa"), &within).expect("kept");
    local::keep_dispatch_in(
        root,
        Record::Mine,
        &limited("devops", ANY, "runners").on_disk(0),
    )
    .expect("kept");
    // The name is one the records hold, so its going is seen.
    assert!(local::dispatch_granted_names(root).contains(&"devops".to_owned()));

    let aside = local::set_aside_dispatch(root, &|name| name == "devops").expect("set aside");
    let ended: Vec<String> = aside
        .ended
        .iter()
        .map(|(level, one)| {
            format!(
                "{} {} -> {} in {}",
                level.word(),
                one.asking,
                one.target,
                one.workspace
            )
        })
        .collect();
    assert_eq!(
        ended,
        [
            "you steward -> devops in runners",
            "you devops -> * in runners"
        ]
    );
    assert!(!aside.is_empty());
    // Nothing of it is set aside to be given back: a grant given back would hold everywhere.
    assert!(aside.grants.is_empty());
    assert!(local::dormant_dispatch(root).is_empty());
    assert_eq!(yours(root), [(limited("steward", "qa", "runners"), 0)]);
}

// ---- the project's file: the spelling ------------------------------------------------------------

const LIMITED: &str = "[dispatch.grants]\n\
     steward = [\"qa\", { to = \"devops\", in = \"runners\" }, { to = \"*\", in = \"web\" }]\n";

#[test]
fn the_project_s_file_spells_a_limited_grant_as_a_table_and_it_is_never_read_as_one_that_holds_everywhere()
 {
    let read = committed(Some(LIMITED));
    assert_eq!(read.pairs, [pair("steward", "qa")]);
    assert!(read.any.is_empty());
    assert_eq!(
        read.limited,
        [
            limited("steward", "devops", "runners"),
            limited("steward", ANY, "web")
        ]
    );
    assert!(read.refused.is_empty(), "{:?}", read.refused);
}

/// `[dispatch.grants]` as a build before the condition read it: each entry of a persona's
/// list is a persona's name or `"*"`, and anything else grants nothing.
fn as_a_build_before_the_condition_reads(text: &str) -> (Vec<(String, String)>, Vec<String>) {
    let top: toml::Table = text.parse().expect("TOML");
    let (mut pairs, mut any) = (Vec::new(), Vec::new());
    let grants = top["dispatch"]["grants"].as_table().expect("a table");
    for (asking, targets) in grants {
        for target in targets.as_array().expect("a list") {
            match target.as_str() {
                Some("*") => any.push(asking.clone()),
                Some(name) if Pair::new(asking, name).is_ok() => {
                    pairs.push((asking.clone(), name.to_owned()));
                }
                _ => {}
            }
        }
    }
    (pairs, any)
}

#[test]
fn a_parser_that_does_not_know_the_condition_reads_a_limited_grant_as_no_grant() {
    // The loop an earlier build ran: the limited entries are not strings, so they are neither
    // a pair nor "any persona".
    let (pairs, any) = as_a_build_before_the_condition_reads(LIMITED);
    assert_eq!(pairs, [("steward".to_owned(), "qa".to_owned())]);
    assert!(any.is_empty());
    // A reader that expects each list to hold only names does not read the table at all.
    let typed: Result<std::collections::BTreeMap<String, Vec<String>>, _> =
        LIMITED.parse::<toml::Table>().expect("TOML")["dispatch"]["grants"]
            .clone()
            .try_into();
    assert!(typed.is_err());
    // Nothing in the limited spelling is a string that could be taken for a target.
    let top: toml::Table = LIMITED.parse().expect("TOML");
    let strings: Vec<&str> = top["dispatch"]["grants"]["steward"]
        .as_array()
        .expect("a list")
        .iter()
        .filter_map(toml::Value::as_str)
        .collect();
    assert_eq!(strings, ["qa"]);
}

#[test]
fn a_limited_entry_that_is_not_exactly_a_target_and_a_workspace_grants_nothing_and_is_said() {
    let shape = "dispatch.grants.steward holds a grant for one workspace that is not written \
                 as { to = \"devops\", in = \"runners\" }, which grants nothing";
    for (entry, why) in [
        // A key missing: never read as "any workspace".
        ("{ to = \"devops\" }", shape.to_owned()),
        ("{ in = \"runners\" }", shape.to_owned()),
        ("{}", shape.to_owned()),
        // A key more: a condition this build does not know is not dropped to widen the grant.
        (
            "{ to = \"devops\", in = \"runners\", until = \"2027\" }",
            shape.to_owned(),
        ),
        // A value that is not a name.
        ("{ to = \"devops\", in = 3 }", shape.to_owned()),
        ("{ to = [\"devops\"], in = \"runners\" }", shape.to_owned()),
        (
            "{ to = \"devops\", in = \"../runners\" }",
            "../runners cannot name a workspace, so purlis keeps no dispatch grant for it."
                .to_owned(),
        ),
        (
            "{ to = \"devops\", in = \"*\" }",
            "* cannot name a workspace, so purlis keeps no dispatch grant for it.".to_owned(),
        ),
        (
            "{ to = \"dev*\", in = \"runners\" }",
            "dev* is not a persona's name, so purlis keeps no dispatch grant for it.".to_owned(),
        ),
        (
            "{ to = \"steward\", in = \"runners\" }",
            "A steward chat dispatches to steward with no grant, so there is none to keep."
                .to_owned(),
        ),
    ] {
        let text = format!("[dispatch.grants]\nsteward = [{entry}]\n");
        let read = committed(Some(&text));
        assert_eq!(
            (read.pairs.len(), read.any.len(), read.limited.len()),
            (0, 0, 0),
            "{entry}"
        );
        assert_eq!(read.refused, [why], "{entry}");
    }
    // A star where the asking persona goes limits nothing either.
    let read = committed(Some(
        "[dispatch.grants]\n\"*\" = [{ to = \"devops\", in = \"runners\" }]\n",
    ));
    assert_eq!(
        read,
        Committed {
            refused: vec![
                "* is not a persona's name, so purlis keeps no dispatch grant for it.".to_owned()
            ],
            ..Committed::default()
        }
    );
}

#[test]
fn the_project_s_grant_for_one_workspace_covers_a_task_there_and_nothing_anywhere_else() {
    // In force as [`InForce::read`] hands them on: the file's, accepted here.
    let grants = InForce {
        limited: vec![
            (Level::Project, limited("steward", "devops", "runners")),
            (Level::Project, limited("qa", ANY, "web")),
        ],
        ..InForce::default()
    };
    let asked = |asking: &str, target: &str, place: Option<&str>| {
        covers(
            Some(asking),
            target,
            &grants.clone().for_task_in(place),
            &Locks::none(),
        )
    };
    assert_eq!(asked("steward", "devops", Some("runners")), Covers::Covered);
    assert_eq!(asked("steward", "devops", Some("web")), Covers::NeedsGrant);
    assert_eq!(asked("steward", "devops", None), Covers::NeedsGrant);
    assert_eq!(asked("qa", "devops", Some("web")), Covers::Covered);
    assert_eq!(asked("qa", "devops", Some("runners")), Covers::NeedsGrant);
    let there = grants.clone().for_task_in(Some("runners"));
    assert_eq!(
        there.level_of(Some("steward"), "devops"),
        Some(Level::Project)
    );
    assert_eq!(
        there.named_level_of(Some("steward"), "devops"),
        Some(Level::Project)
    );
}

// ---- the project's file: the edits, as text ------------------------------------------------------

#[test]
fn a_limited_grant_is_added_to_and_taken_out_of_the_file_with_every_other_line_kept() {
    use crate::settings::dispatch::{with, with_in, without, without_in};
    let before = "schema = 1\n\n# who may ask whom\n[dispatch.grants]\nsteward = [\"qa\"]\n";
    let one = limited("steward", "devops", "runners");
    let after = with_in(before, &one).expect("added");
    assert_eq!(
        after,
        "schema = 1\n\n# who may ask whom\n[dispatch.grants]\n\
         steward = [\"qa\", { to = \"devops\", in = \"runners\" }]\n"
    );
    // Granted already: the same text.
    assert_eq!(with_in(&after, &one).expect("asked"), after);
    // What a build before the condition does to the list leaves the limited entry as it is:
    // it adds and removes names, and matches strings only.
    let widened = with(&after, &pair("steward", "devops")).expect("added");
    assert_eq!(
        committed(Some(&widened)).limited,
        std::slice::from_ref(&one)
    );
    assert_eq!(
        committed(Some(&widened)).pairs,
        [pair("steward", "qa"), pair("steward", "devops")]
    );
    let back = without(&widened, &pair("steward", "devops")).expect("removed");
    assert_eq!(committed(Some(&back)).limited, std::slice::from_ref(&one));
    // Taken out: the pair that holds everywhere, for another target, stays.
    assert_eq!(without_in(&after, &one).expect("removed"), before);
    // Where there is no table yet, it is made; the last grant out takes it away again.
    let made = with_in("schema = 1\n", &one).expect("added");
    assert_eq!(committed(Some(&made)).limited, std::slice::from_ref(&one));
    assert_eq!(without_in(&made, &one).expect("removed"), "schema = 1\n");
    // "Any persona", limited.
    let any = limited("steward", ANY, "web");
    let starred = with_in(before, &any).expect("added");
    assert_eq!(committed(Some(&starred)).limited, [any]);
    assert!(committed(Some(&starred)).any.is_empty());
}

#[test]
fn a_project_grant_is_narrowed_and_widened_in_one_edit_of_the_file() {
    use crate::settings::dispatch::with_within;
    let (any, runners, web) = (
        Within::Any,
        Within::Workspace("runners".to_owned()),
        Within::Workspace("web".to_owned()),
    );
    let before = "[dispatch.grants]\nsteward = [\"devops\", \"qa\"]\n";
    let narrowed = with_within(before, "steward", "devops", &any, &runners).expect("narrowed");
    let read = committed(Some(&narrowed));
    assert_eq!(read.pairs, [pair("steward", "qa")]);
    assert_eq!(read.limited, [limited("steward", "devops", "runners")]);
    // Moved, then a second one beside it, then widened: one entry that holds everywhere.
    let moved = with_within(&narrowed, "steward", "devops", &runners, &web).expect("moved");
    assert_eq!(
        committed(Some(&moved)).limited,
        [limited("steward", "devops", "web")]
    );
    let two = crate::settings::dispatch::with_in(&moved, &limited("steward", "devops", "runners"))
        .expect("added");
    let widened = with_within(&two, "steward", "devops", &web, &any).expect("widened");
    let read = committed(Some(&widened));
    assert_eq!(
        read.pairs,
        [pair("steward", "qa"), pair("steward", "devops")]
    );
    assert!(read.limited.is_empty());
    // "Any persona" the same way.
    let star = "[dispatch.grants]\nsteward = [\"*\"]\n";
    let narrowed = with_within(star, "steward", ANY, &any, &runners).expect("narrowed");
    let read = committed(Some(&narrowed));
    assert!(read.any.is_empty());
    assert_eq!(read.limited, [limited("steward", ANY, "runners")]);
    // A grant the file does not hold as said is not changed.
    assert_eq!(
        with_within(before, "steward", "devops", &runners, &web),
        Err("purlis changed nothing: the project no longer has that grant.".to_owned())
    );
    assert!(with_within(before, "qa", "devops", &any, &runners).is_err());
}

#[test]
fn widening_a_project_grant_leaves_an_entry_this_build_does_not_read_as_it_is() {
    use crate::settings::dispatch::with_within;
    // A later build's entry, with a key this build does not know: it grants nothing here, and
    // it is not this build's to take out of the team's file.
    let before = "[dispatch.grants]\nsteward = [{ to = \"devops\", in = \"runners\" }, \
                  { to = \"devops\", in = \"web\", until = \"2027\" }]\n";
    let widened = with_within(
        before,
        "steward",
        "devops",
        &Within::Workspace("runners".to_owned()),
        &Within::Any,
    )
    .expect("widened");
    assert!(
        widened.contains("{ to = \"devops\", in = \"web\", until = \"2027\" }"),
        "{widened}"
    );
    let read = committed(Some(&widened));
    assert_eq!(read.pairs, [pair("steward", "devops")]);
    assert!(read.limited.is_empty());
    assert_eq!(read.refused.len(), 1);
}

// ---- the audit -----------------------------------------------------------------------------------

#[test]
fn the_audit_says_which_workspace_a_grant_is_limited_to_and_null_for_any() {
    use crate::dispatchgrant::{Act, Audited};
    let limited = Audited {
        act: Act::Grant,
        asking: Some("steward"),
        target: "devops",
        level: Level::You,
        workspace: Some("runners"),
    };
    assert_eq!(limited.kind(), "trust.dispatch.grant");
    assert_eq!(
        limited.body(),
        serde_json::json!({
            "actor_kind": "human",
            "actor": "operator",
            "scope": "local-ui",
            "level": "you",
            "asking": "steward",
            "target": "devops",
            "workspace": "runners",
        })
    );
    let anywhere = Audited {
        workspace: None,
        ..limited
    };
    assert_eq!(anywhere.body()["workspace"], serde_json::Value::Null);
}

// ---- with the project's committed file on disk ---------------------------------------------------
// A sandbox that denies writing the project's file cannot run these.

fn with_file(names: &[&str], text: &str) -> tempfile::TempDir {
    let dir = project(names);
    std::fs::write(crate::names::manifest(dir.path()), text).expect("the project file");
    dir
}

#[test]
fn the_project_s_limited_grant_waits_for_this_machine_s_yes_and_then_covers_its_workspace_only() {
    let dir = with_file(&["runners", "web"], &format!("schema = 1\n\n{LIMITED}"));
    let root = dir.path();
    let one = limited("steward", "devops", "runners");
    // A pull brought it in: it covers nothing until it is accepted here.
    assert_eq!(
        unaccepted(root),
        [one.clone(), limited("steward", ANY, "web")]
    );
    assert_eq!(
        asked(root, "steward", "devops", Some("runners")),
        Covers::NeedsGrant
    );

    accept(root, &one).expect("accepted");
    assert_eq!(
        asked(root, "steward", "devops", Some("runners")),
        Covers::Covered
    );
    assert_eq!(
        asked(root, "steward", "devops", Some("web")),
        Covers::NeedsGrant
    );
    assert_eq!(asked(root, "steward", "devops", None), Covers::NeedsGrant);
    // Accepting it accepted nothing else: not the star, and not the pair for every workspace.
    assert_eq!(unaccepted(root), [limited("steward", ANY, "web")]);
    assert_eq!(record(root).get("dispatch_seen"), None);
    assert_eq!(record(root).get("dispatch_any_seen"), None);

    // "Not on my machine": the acceptance goes, the file stays.
    assert!(unaccept(root, &one).expect("taken away"));
    assert_eq!(
        asked(root, "steward", "devops", Some("runners")),
        Covers::NeedsGrant
    );
    // Nothing is accepted that the file does not hold.
    assert_eq!(
        accept(root, &limited("steward", "qa", "runners")),
        Err("purlis changed nothing: the project no longer has that grant.".to_owned())
    );
    // Nor for a workspace that is not there: the star's is for `web`, which is removed.
    remove(root, "web");
    assert_eq!(
        accept(root, &limited("steward", ANY, "web")),
        Err(
            "web is not a workspace of this project now, so purlis keeps no grant for it."
                .to_owned()
        )
    );
    assert!(crate::settings::dispatch::grant_in(root, &limited("steward", "qa", "web")).is_err());
}

#[test]
fn accepting_a_pair_for_every_workspace_does_not_accept_it_for_one_nor_the_reverse() {
    let dir = with_file(
        &["runners"],
        "[dispatch.grants]\nsteward = [{ to = \"devops\", in = \"runners\" }]\n",
    );
    let root = dir.path();
    // The acknowledgement of the pair as one that holds everywhere, left from before a
    // teammate narrowed it.
    crate::dispatchgrant::acknowledge(root, &["steward -> devops".to_owned()]).expect("said");
    assert_eq!(
        asked(root, "steward", "devops", Some("runners")),
        Covers::NeedsGrant
    );
    // And the reverse: accepted for runners, then a teammate widens it.
    accept(root, &limited("steward", "devops", "runners")).expect("accepted");
    crate::dispatchgrant::acknowledge(root, &[]).expect("said");
    std::fs::write(
        crate::names::manifest(root),
        "[dispatch.grants]\nsteward = [\"devops\"]\n",
    )
    .expect("a pull");
    assert_eq!(
        asked(root, "steward", "devops", Some("runners")),
        Covers::NeedsGrant
    );
    assert_eq!(asked(root, "steward", "devops", None), Covers::NeedsGrant);
    // The widening is news: the Notice that says a grant arrived tells it (#1506).
    let waiting: Vec<String> = crate::dispatcharrival::arrival(root)
        .waiting
        .iter()
        .map(crate::dispatcharrival::Arrived::said)
        .collect();
    assert_eq!(waiting, ["steward -> devops"]);
}

#[test]
fn a_limited_grant_absent_from_the_file_on_disk_covers_nothing_and_is_not_dropped() {
    // Bound to the project's history as a pair is (#1506, the train's review M1): a grant
    // merely absent from the file on disk is not in force, and a branch switched back finds
    // it accepted. A commit that took it out is the settling's to see
    // (`dispatcharrival_tests.rs`).
    let text = "[dispatch.grants]\nsteward = [{ to = \"devops\", in = \"runners\" }]\n";
    let dir = with_file(&["runners"], text);
    let root = dir.path();
    let one = limited("steward", "devops", "runners");
    accept(root, &one).expect("accepted");
    std::fs::write(crate::names::manifest(root), "schema = 1\n").expect("another branch");
    assert_eq!(
        asked(root, "steward", "devops", Some("runners")),
        Covers::NeedsGrant
    );
    std::fs::write(crate::names::manifest(root), text).expect("switched back");
    assert_eq!(
        asked(root, "steward", "devops", Some("runners")),
        Covers::Covered
    );
    assert!(unaccepted(root).is_empty());
}

#[test]
fn a_project_grant_s_workspace_is_changed_for_everyone_and_this_machine_follows_what_it_wrote() {
    use crate::settings::dispatch::{grant_in, revoke_in, set_within};
    let dir = with_file(&["runners", "web"], "schema = 1\n");
    let root = dir.path();
    let one = limited("steward", "devops", "runners");
    grant_in(root, &one).expect("granted");
    assert_eq!(
        asked(root, "steward", "devops", Some("runners")),
        Covers::Covered
    );
    assert_eq!(
        asked(root, "steward", "devops", Some("web")),
        Covers::NeedsGrant
    );

    let (any, runners) = (Within::Any, Within::Workspace("runners".to_owned()));
    set_within(root, "steward", "devops", &runners, &any).expect("widened");
    assert_eq!(
        asked(root, "steward", "devops", Some("web")),
        Covers::Covered
    );
    assert!(project_holds(root, "steward", "devops", &any));
    assert!(!project_holds(root, "steward", "devops", &runners));
    // No news on this machine: the person here wrote it, so nothing waits as arrived.
    assert!(crate::dispatcharrival::arrival(root).waiting.is_empty());

    set_within(root, "steward", "devops", &any, &runners).expect("narrowed");
    assert_eq!(
        asked(root, "steward", "devops", Some("runners")),
        Covers::Covered
    );
    assert_eq!(
        asked(root, "steward", "devops", Some("web")),
        Covers::NeedsGrant
    );
    assert!(crate::dispatcharrival::arrival(root).waiting.is_empty());

    revoke_in(root, &one).expect("revoked");
    assert_eq!(
        asked(root, "steward", "devops", Some("runners")),
        Covers::NeedsGrant
    );
    assert!(local::dispatch_seen_in(root).is_empty());
}
