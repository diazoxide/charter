//! A dispatch grant and a persona that goes away (#1504, V100-61): an absence moves nothing,
//! the same persona coming back is not asked about, another under its name is set aside until
//! the person gives it back, and what nothing observed is said plainly. Every expected answer
//! is written out.

use std::path::Path;

use super::*;
use crate::dispatchgrant::{self, ANY, InForce, Pair};
use crate::personaverbs::define;
use crate::repocmd::Say;
use crate::sandbox::grant::Level;
use crate::sandbox::local;

const PROJECT: &str = "schema = 1\n";

fn pair(asking: &str, target: &str) -> Pair {
    Pair::new(asking, target).expect("a pair")
}

/// Writes the persona `name` into the project at `root` with `body`, as a hand or a pull
/// would.
fn by_hand_as(root: &Path, name: &str, body: &str) {
    let dir = root.join("personas").join(name);
    std::fs::create_dir_all(&dir).expect("its folder");
    std::fs::write(
        dir.join("persona.md"),
        format!("---\nname: {name}\ndelegate-when: {name} work\n---\n\n{body}\n"),
    )
    .expect("its definition");
}

fn by_hand(root: &Path, name: &str) {
    by_hand_as(root, name, &format!("# {name}"));
}

fn deleted_by_hand(root: &Path, name: &str) {
    std::fs::remove_dir_all(root.join("personas").join(name)).expect("removed by hand");
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

/// `purlis persona remove <name>`, as the CLI and the window both run it; what it said.
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

/// `purlis persona create <name>`: its exit code and what it said.
fn made_through_purlis(root: &Path, name: &str) -> (u8, Vec<String>) {
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
        if let Say::Info(text) | Say::Done(text) | Say::Warn(text) | Say::Fail(text) = line {
            said.push(text);
        }
    });
    (code, said)
}

fn dormant(asking: &str, target: &str, any: bool, was: &str) -> Dormant {
    Dormant {
        asking: asking.to_owned(),
        target: target.to_owned(),
        any,
        was: was.to_owned(),
    }
}

/// Everything devops holds and is held by: yours, "any persona", and the project's, each in
/// force on this machine. `steward -> qa` and `qa -> prod` name it nowhere.
fn everything_for_devops(root: &Path) {
    for (asking, target) in [("steward", "devops"), ("devops", "qa"), ("steward", "qa")] {
        local::grant_dispatch(root, asking, target).expect("yours");
    }
    dispatchgrant::allow_any(root, "devops", Level::You).expect("any, yours");
    crate::settings::dispatch::grant(root, &pair("devops", "prod")).expect("the project's");
    crate::settings::dispatch::grant(root, &pair("qa", "devops")).expect("the project's");
    crate::settings::dispatch::grant(root, &pair("qa", "prod")).expect("the project's");
    dispatchgrant::allow_any(root, "devops", Level::Project).expect("any, the project's");
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
}

/// The whole record of this machine, as bytes.
fn record(root: &Path) -> Vec<u8> {
    std::fs::read(local::path(root)).expect("the record")
}

fn everything_is_as_it_was_granted(root: &Path) {
    let now = InForce::read(root, Vec::new());
    assert_eq!(
        now.you,
        [
            pair("steward", "devops"),
            pair("devops", "qa"),
            pair("steward", "qa")
        ]
    );
    assert_eq!(now.you_any, ["devops"]);
    assert_eq!(
        now.project,
        [
            pair("devops", "prod"),
            pair("qa", "devops"),
            pair("qa", "prod")
        ]
    );
    assert_eq!(now.project_any, ["devops"]);
    assert!(
        local::made(root)
            .iter()
            .any(|one| one.target == "steward -> devops" && one.at == 7),
        "when and from which chat is kept"
    );
}

fn nothing_names_devops(root: &Path) {
    let now = InForce::read(root, Vec::new());
    assert_eq!(now.you, [pair("steward", "qa")]);
    assert_eq!(now.you_any, Vec::<String>::new());
    assert_eq!(now.project, [pair("qa", "prod")]);
    assert_eq!(now.project_any, Vec::<String>::new());
}

fn names(all: &[&str]) -> Vec<String> {
    all.iter().map(|one| (*one).to_owned()).collect()
}

// ---- away: nothing moves ------------------------------------------------------------------------

#[test]
fn a_persona_that_is_away_moves_no_grant_and_is_only_marked_by_a_read_or_a_judged_dispatch() {
    let dir = project(&["steward", "devops", "qa", "prod"]);
    let root = dir.path();
    everything_for_devops(root);
    judged(root, &[]).expect("a dispatch judged while it is there");
    let before = record(root);
    deleted_by_hand(root, "devops");

    // Worked out, and nothing written.
    let state = state(root, &[]).expect("listed");
    assert_eq!(state.away, ["devops"]);
    assert_eq!(state.returned, Vec::<String>::new());
    assert_eq!(state.back, Vec::<String>::new());
    assert_eq!(record(root), before);

    // Settings reads, a dispatch is judged, Settings reads again: one mark, and no more.
    noticed(root, &[]).expect("read");
    let marked = record(root);
    assert_ne!(marked, before);
    assert_eq!(judged(root, &[]).expect("judged"), Judged::default());
    noticed(root, &[]).expect("read again");
    assert_eq!(record(root), marked);
    everything_is_as_it_was_granted(root);
    assert_eq!(list(root), Vec::<Dormant>::new());
    let known = local::dispatch_known(root);
    let devops = known
        .iter()
        .find(|one| one.name == "devops")
        .expect("known");
    assert!(devops.away && !devops.hash.is_empty());
    assert!(known.iter().filter(|one| one.away).count() == 1);
}

#[test]
fn the_same_persona_back_from_a_branch_switch_is_not_asked_about_and_nothing_was_moved() {
    let dir = project(&["steward", "devops", "qa", "prod"]);
    let root = dir.path();
    everything_for_devops(root);
    judged(root, &[]).expect("judged while it is there");
    deleted_by_hand(root, "devops");
    noticed(root, &[]).expect("Settings opened on the other branch");
    judged(root, &[]).expect("and a dispatch judged there");

    by_hand(root, "devops");

    // Before anything is judged, the table already reads it as in force: the bytes are the same.
    assert_eq!(state(root, &[]), Some(State::default()));
    assert_eq!(judged(root, &[]).expect("judged"), Judged::default());
    everything_is_as_it_was_granted(root);
    assert_eq!(list(root), Vec::<Dormant>::new());
    assert!(local::dispatch_known(root).iter().all(|one| !one.away));
}

#[test]
fn a_definition_edited_while_its_persona_is_there_is_no_absence() {
    let dir = project(&["steward", "devops"]);
    let root = dir.path();
    local::grant_dispatch(root, "steward", "devops").expect("yours");
    judged(root, &[]).expect("judged");

    by_hand_as(root, "devops", "# devops, with more to say");

    assert_eq!(state(root, &[]), Some(State::default()));
    assert_eq!(judged(root, &[]).expect("judged"), Judged::default());
    assert_eq!(dispatchgrant::yours(root), [pair("steward", "devops")]);
    // And what is known of it is what it is now: away and back like this is the same persona.
    deleted_by_hand(root, "devops");
    judged(root, &[]).expect("judged while away");
    by_hand_as(root, "devops", "# devops, with more to say");
    assert_eq!(judged(root, &[]).expect("judged"), Judged::default());
}

// ---- away, then another under the name ----------------------------------------------------------

#[test]
fn another_persona_under_a_name_seen_gone_gets_nothing_until_the_person_gives_it_back_once() {
    let dir = project(&["steward", "devops", "qa", "prod"]);
    let root = dir.path();
    everything_for_devops(root);
    judged(root, &[]).expect("judged while it is there");
    deleted_by_hand(root, "devops");
    judged(root, &[]).expect("a dispatch judged while it is away");

    by_hand_as(root, "devops", "# another devops altogether");

    // Read by Settings first: it is said to be back and waiting, and nothing is moved.
    let before = record(root);
    noticed(root, &[]).expect("read");
    let waiting = state(root, &[]).expect("listed");
    assert_eq!(waiting.returned, ["devops"]);
    assert_eq!(waiting.back, ["devops"]);
    assert_eq!(record(root), before, "a read moves nothing");

    // The next dispatch judged sets its grants aside, before any grant is read.
    let done = judged(root, &[]).expect("judged");
    assert_eq!(done.changed_hands, ["devops"]);
    assert_eq!(
        done.aside.grants,
        [
            dormant("steward", "devops", false, "devops"),
            dormant("devops", "qa", false, "devops"),
            dormant("devops", ANY, true, "devops"),
        ]
    );
    assert_eq!(
        done.aside
            .accepted
            .iter()
            .map(|one| one.said.as_str())
            .collect::<Vec<_>>(),
        ["devops -> *", "devops -> prod", "qa -> devops"]
    );
    nothing_names_devops(root);
    assert_eq!(judged(root, &[]).expect("judged again"), Judged::default());
    // The project's file is the team's, and is as it was.
    assert_eq!(
        dispatchgrant::committed_at(root),
        [
            pair("devops", "prod"),
            pair("qa", "devops"),
            pair("qa", "prod")
        ]
    );
    let waiting = state(root, &[]).expect("listed");
    assert_eq!(waiting.returned, Vec::<String>::new());
    assert_eq!(waiting.back, ["devops"]);

    // One acknowledgement, for the name, and everything it had is back as it was kept.
    let back = give_back(root, "devops").expect("given back");
    assert_eq!(back.grants.len(), 3);
    assert_eq!(back.accepted.len(), 3);
    let now = InForce::read(root, Vec::new());
    assert_eq!(
        now.you,
        [
            pair("steward", "qa"),
            pair("steward", "devops"),
            pair("devops", "qa")
        ]
    );
    assert_eq!(now.you_any, ["devops"]);
    assert_eq!(
        now.project,
        [
            pair("devops", "prod"),
            pair("qa", "devops"),
            pair("qa", "prod")
        ]
    );
    assert_eq!(now.project_any, ["devops"]);
    assert!(
        local::made(root)
            .iter()
            .any(|one| one.target == "steward -> devops" && one.at == 7),
        "when and from which chat came back with it"
    );
    assert_eq!(list(root), Vec::<Dormant>::new());
    assert_eq!(state(root, &[]), Some(State::default()));
    assert_eq!(
        dispatchgrant::changed(root),
        None,
        "no news of the project's"
    );
    assert_eq!(
        give_back(root, "devops"),
        Err(
            "purlis changed nothing: nothing is waiting to be given back to that persona."
                .to_owned()
        )
    );
}

#[test]
fn a_name_that_is_back_and_was_only_marked_is_given_back_by_lifting_the_mark() {
    let dir = project(&["steward", "devops"]);
    let root = dir.path();
    local::grant_dispatch(root, "steward", "devops").expect("yours");
    judged(root, &[]).expect("judged");
    deleted_by_hand(root, "devops");
    noticed(root, &[]).expect("Settings read while it is away");
    by_hand_as(root, "devops", "# another");

    // No dispatch was judged since: the grant was never moved, and it is not in force.
    assert_eq!(state(root, &[]).expect("listed").returned, ["devops"]);
    let back = give_back(root, "devops").expect("given back");

    assert_eq!(back, SetAside::default(), "nothing had to move");
    assert_eq!(state(root, &[]), Some(State::default()));
    assert_eq!(judged(root, &[]).expect("judged"), Judged::default());
    assert_eq!(dispatchgrant::yours(root), [pair("steward", "devops")]);
}

#[test]
fn nothing_is_given_back_to_a_name_that_is_no_persona_and_one_set_aside_is_removed_alone() {
    let dir = project(&["steward", "devops", "qa"]);
    let root = dir.path();
    local::grant_dispatch(root, "steward", "devops").expect("yours");
    local::grant_dispatch(root, "devops", "qa").expect("yours");
    dispatchgrant::allow_any(root, "devops", Level::You).expect("any");
    judged(root, &[]).expect("judged");
    deleted_by_hand(root, "devops");
    judged(root, &[]).expect("judged while away");
    by_hand_as(root, "devops", "# another");
    judged(root, &[]).expect("set aside");
    deleted_by_hand(root, "devops");

    assert_eq!(
        give_back(root, "devops"),
        Err(
            "This project has no persona named devops, so its grants stay as they are. Make \
             the persona first, or remove them."
                .to_owned()
        )
    );
    assert_eq!(list(root).len(), 3);

    // Remove takes the one entry pressed, and "any persona" is told from a pair by its key.
    assert_eq!(remove(root, "devops", ANY, false), Ok(false));
    assert_eq!(remove(root, "devops", ANY, true), Ok(true));
    assert_eq!(remove(root, "steward", "devops", true), Ok(false));
    assert_eq!(remove(root, "steward", "devops", false), Ok(true));
    assert_eq!(list(root), [dormant("devops", "qa", false, "devops")]);

    // Given back while the other persona of a pair is gone, that pair stays set aside.
    by_hand_as(root, "devops", "# another");
    deleted_by_hand(root, "qa");
    let back = give_back(root, "devops").expect("acknowledged");
    assert_eq!(back.grants, Vec::<Dormant>::new());
    assert_eq!(list(root), [dormant("devops", "qa", false, "devops")]);
    assert_eq!(dispatchgrant::yours(root), Vec::<Pair>::new());
}

#[test]
fn a_grant_kept_only_for_one_chat_has_its_name_marked_and_said_to_have_changed_hands() {
    let dir = project(&["steward", "devops"]);
    let root = dir.path();
    let also = names(&["steward", "devops"]);
    judged(root, &also).expect("judged while it is there");
    deleted_by_hand(root, "devops");
    assert_eq!(state(root, &also).expect("listed").away, ["devops"]);
    judged(root, &also).expect("judged while away");
    by_hand_as(root, "devops", "# another");

    let done = judged(root, &also).expect("judged");

    assert_eq!(done.changed_hands, ["devops"]);
    assert_eq!(
        done.aside,
        SetAside::default(),
        "this machine's records held nothing for it"
    );
    assert_eq!(
        judged(root, &also).expect("judged again"),
        Judged::default()
    );
}

// ---- what nothing observed ----------------------------------------------------------------------

#[test]
fn an_absence_nothing_observed_leaves_no_mark_and_the_new_persona_has_the_grants() {
    // The limit, pinned: removed and made again with no dispatch judged and no read of
    // Settings in between. Only an identity recorded in the definition would tell the two
    // apart, and purlis records none.
    let dir = project(&["steward", "devops"]);
    let root = dir.path();
    local::grant_dispatch(root, "steward", "devops").expect("yours");
    judged(root, &[]).expect("judged");

    deleted_by_hand(root, "devops");
    by_hand_as(root, "devops", "# another, arriving in the same pull");

    assert_eq!(state(root, &[]), Some(State::default()));
    assert_eq!(judged(root, &[]).expect("judged"), Judged::default());
    assert_eq!(dispatchgrant::yours(root), [pair("steward", "devops")]);
}

// ---- purlis removes and creates -----------------------------------------------------------------

#[test]
fn purlis_removing_a_persona_moves_nothing_marks_it_gone_and_forgets_what_it_was() {
    let dir = project(&["steward", "devops", "qa", "prod"]);
    let root = dir.path();
    everything_for_devops(root);
    dispatchgrant::never(root, &pair("steward", "devops")).expect("never");
    judged(root, &[]).expect("judged");

    let said = removed_through_purlis(root, "devops");

    everything_is_as_it_was_granted(root);
    assert_eq!(list(root), Vec::<Dormant>::new());
    assert!(
        said.iter().any(|line| line
            == "Dispatch grants on this machine name 'devops'. They are in force for no chat \
                while it is not a persona, and a persona made later under this name gets them \
                only if you give them back, in Settings › Project › Dispatch."),
        "{said:?}"
    );
    assert!(
        said.iter().any(|line| line
            == "What you said never to for 'devops' still holds, and will hold for a persona \
                made under this name, until you lift it in Settings › Project › Dispatch."),
        "{said:?}"
    );
    // The very same bytes put back by hand are still another persona: purlis removed it.
    by_hand(root, "devops");
    assert_eq!(state(root, &[]).expect("listed").returned, ["devops"]);
    assert_eq!(judged(root, &[]).expect("judged").changed_hands, ["devops"]);
    nothing_names_devops(root);
    assert_eq!(
        dispatchgrant::nevers(root),
        [("steward".to_owned(), "devops".to_owned())],
        "a never is not set aside"
    );
}

#[test]
fn purlis_removing_a_persona_no_grant_names_writes_and_says_nothing_of_dispatch() {
    let dir = project(&["steward", "devops", "qa"]);
    let root = dir.path();
    local::grant_dispatch(root, "steward", "devops").expect("yours");
    let before = record(root);

    let said = removed_through_purlis(root, "qa");

    assert_eq!(record(root), before);
    assert!(
        said.iter().all(|line| !line.contains("ispatch")),
        "{said:?}"
    );
}

#[test]
fn purlis_creating_a_persona_sets_aside_what_an_earlier_one_of_the_name_had_mark_or_none() {
    let dir = project(&["steward", "devops", "qa", "prod"]);
    let root = dir.path();
    everything_for_devops(root);
    dispatchgrant::never(root, &pair("steward", "devops")).expect("never");
    // Deleted by hand, and nothing judged or read since: no mark.
    deleted_by_hand(root, "devops");

    let (code, said) = made_through_purlis(root, "devops");

    assert_eq!(code, 0, "{said:?}");
    nothing_names_devops(root);
    assert_eq!(list(root).len(), 3);
    assert!(
        said.iter().any(|line| line
            == "Set aside 6 dispatch grants on this machine that named an earlier 'devops': in \
                force for no chat until you give them back to this one, or remove them, in \
                Settings › Project › Dispatch."),
        "{said:?}"
    );
    assert!(
        said.iter().any(|line| line
            == "1 pair you said never to on this machine names 'devops', said of an earlier \
                persona of this name. A never still holds for this one; lift it in Settings › \
                Project › Dispatch."),
        "{said:?}"
    );
    // The new persona is known as itself from its first judged dispatch, and is not set aside
    // twice.
    assert_eq!(judged(root, &[]).expect("judged"), Judged::default());
    assert_eq!(state(root, &[]).expect("listed").back, ["devops"]);
    give_back(root, "devops").expect("given back");
    everything_is_as_it_was_granted_but_for_order(root);
}

fn everything_is_as_it_was_granted_but_for_order(root: &Path) {
    let now = InForce::read(root, Vec::new());
    let mut you = now.you.clone();
    you.sort();
    assert_eq!(
        you,
        [
            pair("devops", "qa"),
            pair("steward", "devops"),
            pair("steward", "qa")
        ]
    );
    assert_eq!(now.you_any, ["devops"]);
    assert_eq!(now.project.len(), 3);
    assert_eq!(now.project_any, ["devops"]);
}

#[test]
fn purlis_creating_a_persona_that_only_had_acceptances_says_so_too() {
    let dir = project(&["steward", "qa"]);
    let root = dir.path();
    crate::settings::dispatch::grant(root, &pair("qa", "devops")).expect("the project's");

    let (code, said) = made_through_purlis(root, "devops");

    assert_eq!(code, 0);
    assert!(
        said.iter()
            .any(|line| line.starts_with("Set aside 1 dispatch grant on this machine that named")),
        "{said:?}"
    );
    assert_eq!(dispatchgrant::unacknowledged(root), [pair("qa", "devops")]);
}

#[cfg(unix)]
#[test]
fn purlis_does_not_create_a_persona_whose_name_s_grants_it_could_not_set_aside() {
    use std::os::unix::fs::PermissionsExt;
    let dir = project(&["steward"]);
    let root = dir.path();
    local::grant_dispatch(root, "steward", "devops").expect("yours");
    // As a sandboxed chat finds it: the record's folder cannot be written.
    let folder = local::path(root).parent().expect("a folder").to_path_buf();
    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o555)).expect("read-only");
    if std::fs::write(folder.join("probe"), "").is_ok() {
        // Run as a user no permission stops (root): there is no refusal to see here.
        return;
    }

    let (code, said) = made_through_purlis(root, "devops");

    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o755)).expect("restored");
    assert_eq!(code, 1, "{said:?}");
    assert!(
        !crate::personas::def_path(root, "devops").exists(),
        "not created"
    );
    let refusal = said.last().expect("said");
    assert!(
        refusal.starts_with(
            "Dispatch grants on this machine name 'devops', and purlis could not set them aside ("
        ) && refusal.ends_with(
            "so it did not create the persona: a new persona must not inherit them. Create \
                 it from the purlis window, or first remove those grants in Settings › Project \
                 › Dispatch."
        ),
        "{refusal}"
    );
    // The remedy it names works while the name is no persona: the grant is revoked as any is.
    assert!(local::revoke_dispatch(root, "steward", "devops").expect("revoked"));
    let (code, said) = made_through_purlis(root, "devops");
    assert_eq!(code, 0, "{said:?}");
    // And a name no grant names needs no write at all.
    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o555)).expect("read-only");
    let (code, said) = made_through_purlis(root, "prod");
    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o755)).expect("restored");
    assert_eq!(code, 0, "{said:?}");
}

// ---- records edited by hand ---------------------------------------------------------------------

#[test]
fn a_star_written_as_a_pair_s_target_is_never_given_back_as_any_persona() {
    let dir = project(&["steward", "devops"]);
    let root = dir.path();
    let path = local::path(root);
    std::fs::create_dir_all(path.parent().expect("a folder")).expect("made");
    // `steward` is seen gone, so what names it is set aside when it is another's.
    std::fs::write(
        &path,
        r#"{"dispatch_mine": [{"asking": "steward", "target": "*"},
                              {"asking": "steward", "target": "devops"}],
            "dispatch_known": [{"name": "steward", "away": true}]}"#,
    )
    .expect("written by hand");

    let done = judged(root, &[]).expect("judged");

    assert_eq!(
        done.aside.grants,
        [
            dormant("steward", "*", false, "steward"),
            dormant("steward", "devops", false, "steward"),
        ]
    );
    let back = give_back(root, "steward").expect("acknowledged");
    assert_eq!(
        back.grants,
        [dormant("steward", "devops", false, "steward")]
    );
    assert_eq!(dispatchgrant::any_yours(root), Vec::<String>::new());
    assert_eq!(dispatchgrant::yours(root), [pair("steward", "devops")]);
    assert_eq!(list(root), [dormant("steward", "*", false, "steward")]);
}

#[test]
fn an_entry_set_aside_whose_name_a_hand_edit_dropped_reads_and_is_nobody_s_to_be_given() {
    let dir = project(&["steward", "devops"]);
    let root = dir.path();
    let path = local::path(root);
    std::fs::create_dir_all(path.parent().expect("a folder")).expect("made");
    std::fs::write(
        &path,
        r#"{"dispatch_mine": [{"asking": "steward", "target": "devops"}],
            "dispatch_dormant": [{"asking": "devops", "target": "steward"}]}"#,
    )
    .expect("written by hand");

    // The rest of the record still reads: one dropped key costs no grant.
    assert_eq!(dispatchgrant::yours(root), [pair("steward", "devops")]);
    assert_eq!(list(root), [dormant("devops", "steward", false, "")]);
    assert_eq!(state(root, &[]), Some(State::default()));
    assert!(give_back(root, "").is_err());
    assert_eq!(remove(root, "devops", "steward", false), Ok(true));
}

#[test]
fn nothing_is_worked_out_marked_or_set_aside_where_the_personas_cannot_be_listed() {
    let dir = tempfile::tempdir().expect("a project");
    let root = dir.path();
    local::grant_dispatch(root, "steward", "devops").expect("yours");
    // `personas` is a file, so there is no listing it.
    std::fs::write(root.join("personas"), "not a folder").expect("written");
    let before = record(root);

    assert_eq!(state(root, &[]), None);
    noticed(root, &[]).expect("read");
    assert_eq!(judged(root, &[]).expect("judged"), Judged::default());
    assert_eq!(record(root), before);
}
