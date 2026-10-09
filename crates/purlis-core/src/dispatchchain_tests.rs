//! An older chat's chain, read back from purlis's own dispatch records (#1548).

use super::*;
use crate::dispatchrecord::{self, Asker, ChatRef, Mode as RecordMode, Place, Worker};
use crate::reopen::{Chat, HandedFrom, Identity, Mode, Owed};

const STEWARD: &str = "01K6CHA1N000000000000000S1";
const DEVOPS: &str = "01K6CHA1N000000000000000D2";
const QA: &str = "01K6CHA1N000000000000000Q3";
const OTHER: &str = "01K6CHA1N000000000000000X9";

fn named(persona: Option<&str>, id: &str) -> Chat {
    Chat {
        program: "claude".to_owned(),
        name: "1".to_owned(),
        persona: persona.map(str::to_owned),
        identity: Identity {
            id: Some(id.to_owned()),
            ..Default::default()
        },
        ..Default::default()
    }
}

/// A chat chat `by` dispatched, `depth` down, written before the chain was kept, in the
/// lineage of the person's steward chat.
fn older(by: u32, depth: u32, persona: Option<&str>, id: &str) -> Chat {
    rooted(older_in(by, depth, persona, id), Some(STEWARD))
}

/// `chat`, in the lineage of the chat whose id is `root` (`None`: a record that names none).
fn rooted(mut chat: Chat, root: Option<&str>) -> Chat {
    if let Some(from) = chat.from.as_mut() {
        from.root = root.map(str::to_owned);
    }
    chat
}

/// [`older`], naming no lineage.
fn older_in(by: u32, depth: u32, persona: Option<&str>, id: &str) -> Chat {
    Chat {
        from: Some(HandedFrom {
            chat: by,
            name: "steward 1".to_owned(),
            workspace: crate::active::Place::PlaneRoot,
            report: Owed::Due,
            mode: Mode::Task,
            depth,
            root: None,
            above: None,
            by_person: false,
        }),
        ..named(persona, id)
    }
}

fn chat_ref(number: u32, id: Option<&str>, persona: Option<&str>) -> ChatRef {
    ChatRef {
        chat: number,
        id: id.map(str::to_owned),
        name: format!("chat {number}"),
        persona: persona.map(str::to_owned),
    }
}

/// The record of a dispatch from `asker` to `worker`, as the app writes one.
fn record(asker: ChatRef, worker: ChatRef) -> dispatchrecord::Record {
    let root = tempfile::tempdir().expect("a project");
    dispatchrecord::open(
        root.path(),
        dispatchrecord::Opening {
            mode: RecordMode::Task,
            asker: Asker {
                chat: asker,
                ..Default::default()
            },
            persona: worker.persona.clone(),
            worker: Worker {
                chat: worker,
                ..Default::default()
            },
            task: None,
            place: Place::default(),
            brief: "Do the work.".to_owned(),
            report_owed: true,
        },
        chrono::Utc::now(),
    )
    .expect("a record")
}

fn steward_to_devops() -> dispatchrecord::Record {
    record(
        chat_ref(1, Some(STEWARD), Some("steward")),
        chat_ref(2, Some(DEVOPS), Some("devops")),
    )
}

fn devops_to_qa() -> dispatchrecord::Record {
    record(
        chat_ref(2, Some(DEVOPS), Some("devops")),
        chat_ref(3, Some(QA), Some("qa")),
    )
}

fn names(chain: &[Option<&str>]) -> Vec<Option<String>> {
    chain.iter().map(|one| one.map(str::to_owned)).collect()
}

#[test]
fn a_chain_whose_chats_above_closed_is_read_from_the_dispatch_records() {
    // steward (1) to devops (2) to qa (3), written before the chain was kept; 1 and 2 closed.
    let three = older(2, 2, Some("qa"), QA);
    let records = [devops_to_qa(), steward_to_devops()];
    assert_eq!(
        recovered(3, &[(3, &three)], None, &records),
        Some(names(&[Some("devops"), Some("steward")]))
    );
}

#[test]
fn a_chain_the_records_do_not_cover_down_to_its_depth_is_not_recovered() {
    // The record of who dispatched devops was collected: the records reach one chat above,
    // and the chat's own record says two. Never read as the shorter chain.
    let three = older(2, 2, Some("qa"), QA);
    assert_eq!(recovered(3, &[(3, &three)], None, &[devops_to_qa()]), None);
    // No record at all.
    assert_eq!(recovered(3, &[(3, &three)], None, &[]), None);
}

#[test]
fn a_chain_with_no_depth_kept_is_not_recovered_from_records_that_just_stop() {
    // Written before depths were kept: where the records stop says nothing of where the
    // chain does.
    let three = older_in(2, 0, Some("qa"), QA);
    let records = [devops_to_qa(), steward_to_devops()];
    assert_eq!(recovered(3, &[(3, &three)], None, &records), None);
}

#[test]
fn a_chain_under_a_handoff_from_before_the_keys_is_never_recovered_short() {
    // The person's steward chat handed off to devops before depths and roots were kept, so
    // devops's record names no lineage and no depth, and no dispatch record says who handed
    // it off. After the update devops dispatched qa: qa is "one down" and names no root,
    // though steward is above devops. Both closed. The records reach devops and stop: as
    // deep as qa's own record says, and still shorter than the truth. Not recovered.
    let three = older_in(2, 1, Some("qa"), QA);
    assert_eq!(recovered(3, &[(3, &three)], None, &[devops_to_qa()]), None);
    // Where the records stop at a chat that is not the lineage's first, the same.
    let elsewhere = rooted(older_in(2, 1, Some("qa"), QA), Some(OTHER));
    assert_eq!(
        recovered(3, &[(3, &elsewhere)], None, &[devops_to_qa()]),
        None
    );
    // Reaching the lineage's first chat is whole only as deep as the record says, too.
    let deeper = older(2, 3, Some("qa"), QA);
    let records = [devops_to_qa(), steward_to_devops()];
    assert_eq!(recovered(3, &[(3, &deeper)], None, &records), None);
}

#[test]
fn an_open_chat_that_names_no_asker_ends_the_chain_only_if_it_is_the_lineage_s_first() {
    // steward (1) is open and names no asking chat, but qa's lineage began at another chat:
    // steward is not the top, whatever its record says now.
    let one = named(Some("steward"), STEWARD);
    let three = rooted(older_in(2, 2, Some("qa"), QA), Some(OTHER));
    let records = [devops_to_qa(), steward_to_devops()];
    assert_eq!(
        recovered(3, &[(1, &one), (3, &three)], None, &records),
        None
    );
}

#[test]
fn the_records_and_the_chats_still_open_are_read_as_one_chain() {
    // steward (1) is still open, the person's own; devops (2) closed. The records lead from
    // qa to devops to steward, and steward's own record names nobody above it.
    let one = named(Some("steward"), STEWARD);
    let three = older(2, 2, Some("qa"), QA);
    let records = [devops_to_qa(), steward_to_devops()];
    assert_eq!(
        recovered(3, &[(1, &one), (3, &three)], None, &records),
        Some(names(&[Some("devops"), Some("steward")]))
    );
    // A chat above whose record keeps its own chain finishes it.
    let mut two = older(9, 1, Some("devops"), DEVOPS);
    if let Some(from) = two.from.as_mut() {
        from.above = Some(names(&[Some("ops")]));
    }
    assert_eq!(
        recovered(3, &[(5, &two), (3, &three)], None, &[devops_to_qa()]),
        Some(names(&[Some("devops"), Some("ops")]))
    );
}

#[test]
fn a_chat_on_no_persona_in_the_records_is_the_default_persona() {
    let three = older(2, 2, Some("qa"), QA);
    let records = [
        devops_to_qa(),
        record(
            chat_ref(1, Some(STEWARD), None),
            chat_ref(2, Some(DEVOPS), Some("devops")),
        ),
    ];
    assert_eq!(
        recovered(3, &[(3, &three)], Some("steward"), &records),
        Some(names(&[Some("devops"), Some("steward")]))
    );
    assert_eq!(
        recovered(3, &[(3, &three)], None, &records),
        Some(names(&[Some("devops"), None]))
    );
}

#[test]
fn records_that_disagree_on_who_asked_recover_nothing() {
    let three = older(2, 2, Some("qa"), QA);
    let records = [
        devops_to_qa(),
        steward_to_devops(),
        record(
            chat_ref(4, Some(OTHER), Some("ops")),
            chat_ref(2, Some(DEVOPS), Some("devops")),
        ),
    ];
    assert_eq!(recovered(3, &[(3, &three)], None, &records), None);
}

#[test]
fn a_record_purlis_would_not_have_written_recovers_nothing() {
    let three = older(2, 2, Some("qa"), QA);
    // A persona's name no persona can have.
    let mut odd = steward_to_devops();
    odd.asker.chat.persona = Some("../steward".to_owned());
    assert_eq!(
        recovered(3, &[(3, &three)], None, &[devops_to_qa(), odd]),
        None
    );
    // A record that does not read as one the app writes.
    let mut odd = steward_to_devops();
    odd.started = "yesterday".to_owned();
    assert_eq!(
        recovered(3, &[(3, &three)], None, &[devops_to_qa(), odd]),
        None
    );
}

#[test]
fn records_that_name_each_other_recover_nothing() {
    // A loop no dispatch made: devops asked qa, and qa asked devops.
    let three = older(2, 2, Some("qa"), QA);
    let records = [
        devops_to_qa(),
        record(
            chat_ref(3, Some(QA), Some("qa")),
            chat_ref(2, Some(DEVOPS), Some("devops")),
        ),
    ];
    assert_eq!(recovered(3, &[(3, &three)], None, &records), None);
}

#[test]
fn a_chat_is_matched_to_its_record_by_its_id_and_never_by_its_number() {
    // A record that names the worker by number alone matches no chat: a number is dealt again.
    let three = older(2, 1, Some("qa"), QA);
    let by_number = record(
        chat_ref(2, Some(DEVOPS), Some("devops")),
        chat_ref(3, None, Some("qa")),
    );
    assert_eq!(recovered(3, &[(3, &three)], None, &[by_number]), None);
    // A chat with no id has no record to read.
    let mut no_id = older(2, 1, Some("qa"), QA);
    no_id.identity.id = None;
    assert_eq!(recovered(3, &[(3, &no_id)], None, &[devops_to_qa()]), None);
}

#[test]
fn records_that_go_on_past_the_depth_are_kept_whole() {
    // Deeper is the safe side: a chain the records show longer than the chat's own depth is
    // kept as long as they show it.
    let three = older(2, 1, Some("qa"), QA);
    let records = [devops_to_qa(), steward_to_devops()];
    assert_eq!(
        recovered(3, &[(3, &three)], None, &records),
        Some(names(&[Some("devops"), Some("steward")]))
    );
}

#[test]
fn a_chat_the_person_started_has_nothing_to_recover() {
    let one = named(Some("steward"), STEWARD);
    assert_eq!(
        recovered(1, &[(1, &one)], None, &[steward_to_devops()]),
        Some(Vec::new())
    );
    assert_eq!(recovered(7, &[(1, &one)], None, &[]), None);
}
