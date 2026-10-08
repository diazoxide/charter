//! A removed persona's dispatch grants are set aside, a persona made again under the name
//! inherits none, and a rename rewrites every record (#1504, V100-61). Every expected answer
//! is written out.

use std::path::Path;

use super::*;
use crate::dispatchgrant::{self, ANY, Covers, InForce, Pair, covers};
use crate::personaverbs::define;
use crate::repocmd::Say;
use crate::sandbox::grant::Level;
use crate::sandbox::local;
use crate::sandbox::policy::Locks;

const PROJECT: &str = "schema = 1\n";

fn pair(asking: &str, target: &str) -> Pair {
    Pair::new(asking, target).expect("a pair")
}

/// Writes the persona `name` into the project at `root`, as a hand or a pull would.
fn by_hand(root: &Path, name: &str) {
    let dir = root.join("personas").join(name);
    std::fs::create_dir_all(&dir).expect("its folder");
    std::fs::write(
        dir.join("persona.md"),
        format!("---\nname: {name}\ndelegate-when: {name} work\n---\n\n# {name}\n"),
    )
    .expect("its definition");
}

/// A project with a project file and these personas.
fn project(personas: &[&str]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("a project");
    std::fs::write(crate::names::manifest(dir.path()), PROJECT).expect("the project file");
    for name in personas {
        by_hand(dir.path(), name);
    }
    dir
}

/// `purlis persona remove <name>`, as the CLI and the window both run it.
fn removed_through_purlis(root: &Path, name: &str) -> Vec<String> {
    let mut said = Vec::new();
    let selection = crate::active::ActivePersona {
        name: None,
        rung: crate::active::PersonaRung::Nothing,
    };
    let code = define::remove(root, name, false, &selection, &mut |line: Say| {
        if let Say::Info(text) | Say::Done(text) | Say::Warn(text) | Say::Fail(text) = line {
            said.push(text);
        }
    });
    assert_eq!(code, 0, "removed: {said:?}");
    said
}

/// `purlis persona create <name>`.
fn made_through_purlis(root: &Path, name: &str) {
    let ask = define::Create {
        name,
        role: None,
        delegate_when: Some("work of its own"),
        vault: None,
        extends: None,
        select: None,
        force: false,
    };
    let mut said = Vec::new();
    let code = define::create(root, &ask, None, &mut |line: Say| {
        said.push(format!("{line:?}"))
    });
    assert_eq!(code, 0, "created: {said:?}");
}

fn dormant(asking: &str, target: &str, was: &str) -> Dormant {
    Dormant {
        asking: asking.to_owned(),
        target: target.to_owned(),
        any: target == ANY,
        was: was.to_owned(),
    }
}

/// `pairs`, in an order of their own: the order the project's file lists them in is no fact
/// about them.
fn sorted(mut pairs: Vec<Pair>) -> Vec<Pair> {
    pairs.sort();
    pairs
}

/// Everything devops holds and is held by: yours, "any persona", and the project's, each in
/// force on this machine. `steward -> qa` names it nowhere.
fn everything_for_devops(root: &Path) {
    for (asking, target) in [("steward", "devops"), ("devops", "qa"), ("steward", "qa")] {
        local::grant_dispatch(root, asking, target).expect("yours");
    }
    dispatchgrant::allow_any(root, "devops", Level::You).expect("any, yours");
    crate::settings::dispatch::grant(root, &pair("devops", "prod")).expect("the project's");
    crate::settings::dispatch::grant(root, &pair("qa", "devops")).expect("the project's");
    crate::settings::dispatch::grant(root, &pair("qa", "prod")).expect("the project's");
    dispatchgrant::allow_any(root, "devops", Level::Project).expect("any, the project's");
}

fn nothing_names_devops(root: &Path) {
    let now = InForce::read(root, Vec::new());
    assert_eq!(now.you, [pair("steward", "qa")]);
    assert_eq!(now.you_any, Vec::<String>::new());
    assert_eq!(now.project, [pair("qa", "prod")]);
    assert_eq!(now.project_any, Vec::<String>::new());
    for (asking, target) in [
        ("steward", "devops"),
        ("devops", "qa"),
        ("devops", "prod"),
        ("qa", "devops"),
        ("devops", "steward"),
    ] {
        assert_eq!(
            covers(Some(asking), target, &now, &Locks::none()),
            Covers::NeedsGrant,
            "{asking} -> {target}"
        );
    }
}

// ---- removed ------------------------------------------------------------------------------------

#[test]
fn a_persona_removed_through_purlis_leaves_no_grant_in_force_and_each_of_yours_is_set_aside() {
    let dir = project(&["steward", "devops", "qa", "prod"]);
    let root = dir.path();
    everything_for_devops(root);
    let before = InForce::read(root, Vec::new());
    assert_eq!(
        covers(Some("devops"), "steward", &before, &Locks::none()),
        Covers::Covered,
        "any persona"
    );

    let said = removed_through_purlis(root, "devops");

    nothing_names_devops(root);
    assert_eq!(
        list(root),
        [
            dormant("steward", "devops", "devops"),
            dormant("devops", "qa", "devops"),
            dormant("devops", ANY, "devops"),
        ]
    );
    assert!(
        said.iter().any(|line| line
            == "Set aside 3 dispatch grants on this machine that named 'devops': in force for \
                no chat until you give each back or remove it, in Settings › Project › Dispatch."),
        "{said:?}"
    );
    // The project's file is the team's, and is left as it is: its grants for the name wait for
    // a yes on this machine again.
    assert_eq!(
        dispatchgrant::committed_at(root),
        [
            pair("devops", "prod"),
            pair("qa", "devops"),
            pair("qa", "prod")
        ]
    );
    assert_eq!(
        dispatchgrant::unacknowledged(root),
        [pair("devops", "prod"), pair("qa", "devops")]
    );
    assert_eq!(dispatchgrant::any_unaccepted(root), ["devops"]);
}

#[test]
fn a_persona_made_again_under_the_name_inherits_nothing_until_the_person_gives_it_back() {
    let dir = project(&["steward", "devops", "qa", "prod"]);
    let root = dir.path();
    everything_for_devops(root);
    removed_through_purlis(root, "devops");

    made_through_purlis(root, "devops");

    nothing_names_devops(root);
    assert_eq!(list(root).len(), 3);

    // The person gives one pair back, and "any persona": each alone.
    revive(root, "devops", "qa").expect("given back");
    assert_eq!(
        dispatchgrant::yours(root),
        [pair("steward", "qa"), pair("devops", "qa")]
    );
    assert_eq!(dispatchgrant::any_yours(root), Vec::<String>::new());
    revive(root, "devops", ANY).expect("given back");
    assert_eq!(dispatchgrant::any_yours(root), ["devops"]);
    assert_eq!(list(root), [dormant("steward", "devops", "devops")]);
}

#[test]
fn a_grant_is_not_given_back_while_a_name_in_it_is_no_persona() {
    let dir = project(&["steward", "devops", "qa"]);
    let root = dir.path();
    local::grant_dispatch(root, "steward", "devops").expect("yours");
    dispatchgrant::allow_any(root, "devops", Level::You).expect("any");
    removed_through_purlis(root, "devops");

    for target in ["devops", ANY] {
        let asking = if target == ANY { "devops" } else { "steward" };
        assert_eq!(
            revive(root, asking, target),
            Err(
                "This project has no persona named devops, so the grant stays set aside. \
                 Remove it, or make the persona first."
                    .to_owned()
            )
        );
    }
    assert_eq!(list(root).len(), 2, "both still set aside");
    assert_eq!(dispatchgrant::yours(root), Vec::<Pair>::new());
    assert_eq!(
        revive(root, "steward", "qa"),
        Err("purlis changed nothing: that grant is no longer there.".to_owned())
    );
}

#[test]
fn remove_takes_one_that_was_set_aside_out_for_good() {
    let dir = project(&["steward", "devops"]);
    let root = dir.path();
    local::grant_dispatch(root, "steward", "devops").expect("yours");
    removed_through_purlis(root, "devops");

    assert_eq!(remove(root, "steward", "devops"), Ok(true));
    assert_eq!(list(root), Vec::<Dormant>::new());
    assert_eq!(remove(root, "steward", "devops"), Ok(false));
    made_through_purlis(root, "devops");
    assert_eq!(dispatchgrant::yours(root), Vec::<Pair>::new());
}

#[test]
fn a_persona_removed_by_hand_is_set_aside_when_the_table_is_read() {
    let dir = project(&["steward", "devops", "qa"]);
    let root = dir.path();
    local::grant_dispatch(root, "steward", "devops").expect("yours");
    local::grant_dispatch(root, "steward", "qa").expect("yours");
    std::fs::remove_dir_all(root.join("personas/devops")).expect("removed by hand");

    assert_eq!(
        sweep(root).expect("swept"),
        [dormant("steward", "devops", "devops")]
    );

    by_hand(root, "devops");
    assert_eq!(dispatchgrant::yours(root), [pair("steward", "qa")]);
    assert_eq!(sweep(root).expect("swept"), Vec::<Dormant>::new());
}

#[test]
fn a_persona_made_through_purlis_inherits_nothing_a_removal_by_hand_left() {
    let dir = project(&["steward", "devops"]);
    let root = dir.path();
    local::grant_dispatch(root, "devops", "steward").expect("yours");
    dispatchgrant::allow_any(root, "devops", Level::You).expect("any");
    std::fs::remove_dir_all(root.join("personas/devops")).expect("removed by hand");

    made_through_purlis(root, "devops");

    let now = InForce::read(root, Vec::new());
    assert_eq!(now.you, Vec::<Pair>::new());
    assert_eq!(now.you_any, Vec::<String>::new());
    assert_eq!(
        list(root),
        [
            dormant("devops", "steward", "devops"),
            dormant("devops", ANY, "devops")
        ]
    );
}

#[test]
fn a_never_stays_in_force_for_the_name_whatever_becomes_of_the_persona() {
    let dir = project(&["steward", "devops"]);
    let root = dir.path();
    dispatchgrant::never(root, &pair("steward", "devops")).expect("never");
    removed_through_purlis(root, "devops");
    made_through_purlis(root, "devops");
    sweep(root).expect("swept");

    assert_eq!(
        dispatchgrant::nevers(root),
        [("steward".to_owned(), "devops".to_owned())]
    );
    local::grant_dispatch(root, "steward", "devops").expect("a new grant");
    assert_eq!(
        covers(
            Some("steward"),
            "devops",
            &InForce::read(root, Vec::new()),
            &Locks::none()
        ),
        Covers::Never
    );
}

#[test]
fn nothing_is_written_where_every_name_is_a_persona() {
    let dir = project(&["steward", "devops"]);
    let root = dir.path();
    local::grant_dispatch(root, "steward", "devops").expect("yours");
    let before = std::fs::read(local::path(root)).expect("the record");

    assert_eq!(sweep(root).expect("swept"), Vec::<Dormant>::new());
    assert_eq!(
        persona_gone(root, "qa").expect("asked"),
        Vec::<Dormant>::new()
    );
    assert_eq!(
        std::fs::read(local::path(root)).expect("the record"),
        before
    );
}

#[test]
fn a_name_written_by_hand_that_is_no_persona_s_is_set_aside_and_never_given_back() {
    let dir = project(&["steward", "devops"]);
    let root = dir.path();
    let path = local::path(root);
    std::fs::create_dir_all(path.parent().expect("a folder")).expect("made");
    std::fs::write(
        &path,
        r#"{"dispatch_mine": [{"asking": "steward", "target": "*"},
                              {"asking": "Steward", "target": "devops"},
                              {"asking": "steward", "target": "devops"}],
            "dispatch_any": ["*", "steward -> devops"]}"#,
    )
    .expect("written by hand");

    let moved = sweep(root).expect("swept");

    // A star written as a pair's target is a pair nobody can be given, and is set aside as
    // one: never as "any persona", which only the record of that name says.
    let star_as_a_target = Dormant {
        any: false,
        ..dormant("steward", "*", "*")
    };
    assert_eq!(
        moved,
        [
            star_as_a_target.clone(),
            dormant("Steward", "devops", "Steward"),
            dormant("*", ANY, "*"),
            dormant("steward -> devops", ANY, "steward -> devops"),
        ]
    );
    assert_eq!(dispatchgrant::yours(root), [pair("steward", "devops")]);
    assert_eq!(dispatchgrant::any_yours(root), Vec::<String>::new());
    // Giving any of them back is refused, and above all the first: it reads like "any persona
    // for steward", which nobody ever granted.
    for one in &moved {
        assert!(revive(root, &one.asking, &one.target).is_err(), "{one:?}");
    }
    assert_eq!(dispatchgrant::any_yours(root), Vec::<String>::new());
    assert_eq!(dispatchgrant::yours(root), [pair("steward", "devops")]);
    assert_eq!(list(root), moved, "each still set aside, for Remove");
    assert_eq!(remove(root, "steward", "*"), Ok(true));
}

// ---- renamed ------------------------------------------------------------------------------------

#[test]
fn a_rename_rewrites_the_name_in_every_record_and_is_no_news_on_this_machine() {
    let dir = project(&["steward", "devops", "qa", "prod"]);
    let root = dir.path();
    everything_for_devops(root);
    dispatchgrant::never(root, &pair("devops", "steward")).expect("never");
    dispatchgrant::never(root, &pair("prod", "devops")).expect("never");
    std::fs::write(
        crate::names::manifest(root),
        format!(
            "{}\n# kept\n[dispatch]\ndepth = 3\n",
            std::fs::read_to_string(crate::names::manifest(root)).expect("the file")
        ),
    )
    .expect("another line of the file");
    dispatchgrant::decline(root, "qa", "prod").expect("declined");
    local::record_made(
        root,
        local::Made {
            what: "dispatch".to_owned(),
            target: "steward -> devops".to_owned(),
            level: "you".to_owned(),
            at: 7,
            chat: Some("steward 1".to_owned()),
        },
    )
    .expect("recorded");

    // As the code that renames a persona would: the folder, then the records.
    std::fs::rename(root.join("personas/devops"), root.join("personas/ops")).expect("renamed");
    assert_eq!(persona_renamed(root, "devops", "ops"), Ok(()));

    let now = InForce::read(root, Vec::new());
    assert_eq!(
        now.you,
        [
            pair("steward", "ops"),
            pair("ops", "qa"),
            pair("steward", "qa")
        ]
    );
    assert_eq!(now.you_any, ["ops"]);
    assert_eq!(
        sorted(now.project.clone()),
        [pair("ops", "prod"), pair("qa", "ops")]
    );
    assert_eq!(now.project_any, ["ops"]);
    assert_eq!(
        now.never,
        [
            ("ops".to_owned(), "steward".to_owned()),
            ("prod".to_owned(), "ops".to_owned())
        ]
    );
    assert_eq!(dispatchgrant::declined(root), ["qa -> prod"]);
    assert_eq!(dispatchgrant::changed(root), None, "your own rename");
    assert_eq!(list(root), Vec::<Dormant>::new());
    assert_eq!(sweep(root).expect("swept"), Vec::<Dormant>::new());
    let made = local::made(root);
    assert!(
        made.iter()
            .any(|one| one.target == "steward -> ops" && one.at == 7),
        "{made:?}"
    );
    let text = std::fs::read_to_string(crate::names::manifest(root)).expect("the file");
    assert!(!text.contains("devops"), "{text}");
    assert!(text.contains("# kept\n[dispatch]\ndepth = 3\n"), "{text}");
    // The old name grants and refuses nothing.
    for (asking, target) in [("devops", "qa"), ("steward", "devops")] {
        assert_eq!(
            covers(Some(asking), target, &now, &Locks::none()),
            Covers::NeedsGrant
        );
    }
    assert_eq!(
        covers(Some("ops"), "steward", &now, &Locks::none()),
        Covers::Never
    );
}

#[test]
fn a_renamed_persona_gains_nothing_an_earlier_persona_of_its_new_name_left() {
    let dir = project(&["steward", "devops", "prod"]);
    let root = dir.path();
    // An earlier `ops` was removed by hand, and nothing has read the table since.
    local::grant_dispatch(root, "ops", "prod").expect("the earlier persona's");
    dispatchgrant::allow_any(root, "ops", Level::You).expect("the earlier persona's");
    local::grant_dispatch(root, "devops", "steward").expect("its own");

    std::fs::rename(root.join("personas/devops"), root.join("personas/ops")).expect("renamed");
    assert_eq!(persona_renamed(root, "devops", "ops"), Ok(()));

    assert_eq!(dispatchgrant::yours(root), [pair("ops", "steward")]);
    assert_eq!(dispatchgrant::any_yours(root), Vec::<String>::new());
    assert_eq!(
        list(root),
        [dormant("ops", "prod", "ops"), dormant("ops", ANY, "ops")]
    );
}

#[test]
fn a_rename_says_what_it_could_not_rewrite_and_rewrites_the_rest() {
    let dir = project(&["steward", "devops"]);
    let root = dir.path();
    local::grant_dispatch(root, "steward", "devops").expect("yours");
    let nevers = crate::dispatchnever::path(root);
    std::fs::write(&nevers, "not json").expect("a record that does not read");

    let refused = persona_renamed(root, "devops", "ops").expect_err("said");

    assert_eq!(refused.len(), 1, "{refused:?}");
    assert!(
        refused[0].starts_with("purlis could not rename devops in the pairs you said never to"),
        "{refused:?}"
    );
    assert_eq!(dispatchgrant::yours(root), [pair("steward", "ops")]);
    assert_eq!(std::fs::read_to_string(&nevers).expect("left"), "not json");
}

#[test]
fn a_rename_to_or_from_what_is_no_persona_s_name_changes_nothing() {
    let dir = project(&["steward", "devops"]);
    let root = dir.path();
    local::grant_dispatch(root, "steward", "devops").expect("yours");
    let before = std::fs::read(local::path(root)).expect("the record");

    for (from, to) in [
        ("devops", "*"),
        ("*", "devops"),
        ("devops", "a -> b"),
        ("", "ops"),
    ] {
        assert!(persona_renamed(root, from, to).is_err(), "{from} to {to}");
    }
    assert_eq!(persona_renamed(root, "devops", "devops"), Ok(()));
    assert_eq!(
        std::fs::read(local::path(root)).expect("the record"),
        before
    );
}
