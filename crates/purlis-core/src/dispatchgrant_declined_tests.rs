//! "Not on my machine" (#1504): one grant of the project's stops being followed here, and the
//! committed file is left to the team. Every expected answer is written out.

use super::*;

const PROJECT: &str =
    "schema = 1\n\n[dispatch.grants]\nsteward = [\"devops\", \"*\"]\nqa = [\"devops\"]\n";

fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("a project");
    std::fs::write(crate::names::manifest(dir.path()), PROJECT).expect("the project file");
    dir
}

fn pair(asking: &str, target: &str) -> Pair {
    Pair::new(asking, target).expect("a pair")
}

fn file(root: &Path) -> String {
    std::fs::read_to_string(crate::names::manifest(root)).expect("the file")
}

#[test]
fn not_on_my_machine_takes_a_followed_pair_out_of_force_and_leaves_the_file_to_the_team() {
    let dir = project();
    let root = dir.path();
    acknowledge(
        root,
        &["steward -> devops".to_owned(), "qa -> devops".to_owned()],
    )
    .expect("allowed");
    assert_eq!(
        InForce::read(root, Vec::new()).project,
        [pair("steward", "devops"), pair("qa", "devops")]
    );

    decline(root, "steward", "devops").expect("declined");

    assert_eq!(
        InForce::read(root, Vec::new()).project,
        [pair("qa", "devops")]
    );
    assert_eq!(file(root), PROJECT);
    assert_eq!(declined(root), ["steward -> devops"]);
    assert_eq!(unacknowledged(root), [pair("steward", "devops")]);
    assert_eq!(changed(root), None, "declined here is no news here");
}

#[test]
fn allowing_what_the_notice_shows_never_accepts_a_pair_declined_here() {
    let dir = project();
    let root = dir.path();
    decline(root, "steward", "devops").expect("declined before it was ever allowed");

    let told = changed(root).expect("the other pair is still news");
    assert_eq!(told.added, ["qa -> devops"]);
    assert_eq!(told.now, ["qa -> devops"]);
    acknowledge(root, &told.now).expect("allowed");

    assert_eq!(
        InForce::read(root, Vec::new()).project,
        [pair("qa", "devops")]
    );
    assert_eq!(declined(root), ["steward -> devops"]);
    assert_eq!(changed(root), None);
}

#[test]
fn accepting_it_again_ends_the_decline() {
    let dir = project();
    let root = dir.path();
    decline(root, "steward", "devops").expect("declined");

    acknowledge_pair(root, &pair("steward", "devops")).expect("accepted");

    assert_eq!(
        InForce::read(root, Vec::new()).project,
        [pair("steward", "devops")]
    );
    assert_eq!(declined(root), Vec::<String>::new());
}

#[test]
fn any_persona_of_the_project_s_is_declined_and_accepted_apart_from_its_pairs() {
    let dir = project();
    let root = dir.path();
    acknowledge(root, &["steward -> devops".to_owned()]).expect("allowed");
    allow_any(root, "steward", Level::Project).expect("accepted");
    assert_eq!(any_of_the_project(root), ["steward"]);

    decline(root, "steward", ANY).expect("declined");

    assert_eq!(any_of_the_project(root), Vec::<String>::new());
    assert_eq!(any_unaccepted(root), ["steward"]);
    assert_eq!(declined(root), ["steward -> *"]);
    assert_eq!(
        InForce::read(root, Vec::new()).project,
        [pair("steward", "devops")]
    );
    assert_eq!(file(root), PROJECT);

    allow_any(root, "steward", Level::Project).expect("accepted again");
    assert_eq!(any_of_the_project(root), ["steward"]);
    assert_eq!(declined(root), Vec::<String>::new());
}

#[test]
fn nothing_is_declined_that_the_project_s_file_does_not_hold() {
    let dir = project();
    let root = dir.path();
    let gone = Err("purlis changed nothing: the project no longer has that grant.".to_owned());
    for (asking, target) in [
        ("devops", "qa"),
        ("qa", ANY),
        (ANY, "devops"),
        (ANY, ANY),
        ("steward", "devops -> qa"),
        ("steward -> devops", "qa"),
        ("steward", "steward"),
        ("", ""),
    ] {
        assert_eq!(decline(root, asking, target), gone, "{asking} to {target}");
    }
    assert_eq!(declined(root), Vec::<String>::new());
    assert!(
        !crate::sandbox::local::path(root).exists(),
        "nothing written"
    );
}

#[test]
fn a_decline_is_audited_as_its_own_kind_at_the_project_s_level() {
    let audited = Audited {
        act: Act::Decline,
        asking: Some("steward"),
        target: "devops",
        level: Level::Project,
    };
    assert_eq!(audited.kind(), "trust.dispatch.decline");
    assert_eq!(audited.body()["level"], "project");
}

#[test]
fn a_grant_purlis_set_aside_is_a_revoke_by_the_host_and_never_by_a_person() {
    let aside = Audited {
        act: Act::SetAside,
        asking: Some("steward"),
        target: "devops",
        level: Level::You,
    };
    assert_eq!(aside.kind(), "trust.dispatch.revoke");
    assert_eq!(aside.body()["actor_kind"], "host");
    assert_eq!(aside.body()["actor"], "purlis");
    let revoke = Audited {
        act: Act::Revoke,
        ..aside
    };
    assert_eq!(revoke.body()["actor_kind"], "human");
    let back = Audited {
        act: Act::GiveBack,
        asking: Some("devops"),
        target: "devops",
        level: Level::You,
    };
    assert_eq!(back.kind(), "trust.dispatch.give_back");
    assert_eq!(back.body()["actor_kind"], "human");
}

#[test]
fn accepting_any_persona_of_the_project_s_never_writes_the_project_s_file() {
    let dir = project();
    let root = dir.path();
    accept_any_of_the_project(root, "steward").expect("accepted");
    assert_eq!(any_of_the_project(root), ["steward"]);
    assert_eq!(file(root), PROJECT);

    // The file lost the grant: accepting it is refused, and does not put it back.
    let without = "schema = 1\n\n[dispatch.grants]\nsteward = [\"devops\"]\n";
    std::fs::write(crate::names::manifest(root), without).expect("a pull");
    for asking in ["steward", "qa", "*", ""] {
        assert_eq!(
            accept_any_of_the_project(root, asking),
            Err("purlis changed nothing: the project no longer has that grant.".to_owned())
        );
    }
    assert_eq!(file(root), without);
}
