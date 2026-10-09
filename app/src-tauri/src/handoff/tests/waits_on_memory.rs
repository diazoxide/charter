//! A dispatch waits while the machine is short on memory, and starts when it is not (#1467).
//!
//! Against the app's own records, with the project's memory reader stood in for: a test's
//! project reads no machine, and its own timer leaves the looking to the test, which asks
//! [`memory_freed`] itself at the moment it names.

use purlis_core::dispatchdecision::{MEMORY_WAIT, gave_up_on_memory};
use purlis_core::handback::Answered;
use purlis_core::memorypressure::Memory;

use super::*;

/// The project's memory reads `memory` from now on.
fn memory_is(held: &Held, memory: Memory) {
    held.held_dispatches().memory().stand_in(Some(memory));
}

/// What the Dispatches tab lists of the dispatches that never started: state and task.
fn not_started(held: &Held) -> Vec<(String, Option<String>)> {
    crate::dispatches::not_started_rows(held)
        .into_iter()
        .map(|row| (row.state, row.task))
        .collect()
}

fn listed(state: &str, task: &str) -> Vec<(String, Option<String>)> {
    vec![(state.to_owned(), Some(task.to_owned()))]
}

/// A project with personas on a pretend session host, a steward chat in `alpha`, and memory
/// short.
fn short_with_a_steward() -> (Plane, Pretend, Planes, PlaneId, Arc<Held>, u32) {
    let plane = a_plane_with_personas();
    let host = Pretend::default();
    let (planes, id, asking) = a_steward_chat(&host, &plane);
    let held = planes.held(&id).expect("held");
    memory_is(&held, Memory::Short);
    (plane, host, planes, id, held, asking)
}

#[test]
fn a_dispatch_the_limits_and_the_grant_allow_waits_on_memory_and_the_chat_is_told() {
    let (_plane, _host, _planes, id, held, asking) = short_with_a_steward();
    let before = held.chats().open_now().len();

    let (said, told) = dispatch(&held, &id, &Tickets::default(), asking, None, "tidy up");

    assert_eq!(
        said,
        Answer::WaitingOnMemory {
            to: Some("steward".to_owned())
        }
    );
    assert!(told.is_none(), "nothing arrived");
    assert_eq!(held.chats().open_now().len(), before, "nothing started");
    assert!(dispatch_records(&held).is_empty(), "nothing recorded");
    // The person sees which one waits, and on what.
    assert_eq!(not_started(&held), listed("waiting-on-memory", "tidy up"));
}

#[test]
fn once_memory_frees_the_held_dispatch_starts_on_the_brief_it_asked_and_the_chat_is_told() {
    let (_plane, host, _planes, id, held, asking) = short_with_a_steward();
    let (said, _) = dispatch(&held, &id, &Tickets::default(), asking, None, "tidy up");
    assert!(matches!(said, Answer::WaitingOnMemory { .. }), "{said:?}");
    let before = held.chats().open_now().len();

    // Still short: it waits on, and nothing is told.
    memory_freed(&held, &id, Instant::now(), &nobody);
    assert_eq!(held.chats().open_now().len(), before);
    assert!(told_on_its_next_turn(&held, asking).is_empty());

    memory_is(&held, Memory::Enough);
    let arrived = Mutex::new(None);
    memory_freed(&held, &id, Instant::now(), &|it| {
        *arrived.lock().unwrap() = Some(it);
    });

    let arrived = arrived.into_inner().unwrap().expect("the window is told");
    assert_eq!(arrived.label.as_deref(), Some("tidy up"));
    assert_eq!(held.chats().open_now().len(), before + 1, "it started");
    let first = host
        .openings()
        .last()
        .and_then(|opening| opening.args.last().cloned())
        .expect("the new chat's first message");
    assert!(
        first.contains("# Check the queue"),
        "on the brief it asked: {first}"
    );
    let told = told_on_its_next_turn(&held, asking);
    assert_eq!(told.len(), 1, "{told:?}");
    assert_eq!(told[0].answered, Some(Answered::Started));
    assert_eq!(told[0].from, "tidy up");
    assert!(not_started(&held).is_empty(), "no longer waiting");
    // Once: a second look finds nothing to start.
    memory_freed(&held, &id, Instant::now(), &nothing_opens);
    assert_eq!(held.chats().open_now().len(), before + 1);
}

#[test]
fn a_held_dispatch_is_decided_against_the_limits_again_when_memory_frees() {
    let plane = a_plane_with_personas();
    std::fs::write(
        plane.root.join(purlis_core::plane::MANIFEST),
        "[persona]\ndefault = \"steward\"\n[dispatch]\nrunning-per-chat = 1\n",
    )
    .expect("the manifest");
    let host = Pretend::default();
    let (planes, id, asking) = a_steward_chat(&host, &plane);
    let held = planes.held(&id).expect("held");
    memory_is(&held, Memory::Short);
    let (said, _) = dispatch(&held, &id, &Tickets::default(), asking, None, "tidy up");
    assert!(matches!(said, Answer::WaitingOnMemory { .. }), "{said:?}");
    // Memory frees, and another task takes the one slot before the held one is looked at.
    memory_is(&held, Memory::Enough);
    let (own, _) = dispatch(&held, &id, &Tickets::default(), asking, None, "first in");
    assert!(matches!(own, Answer::Dispatched { .. }), "{own:?}");
    let before = held.chats().open_now().len();

    memory_freed(&held, &id, Instant::now(), &nothing_opens);

    assert_eq!(held.chats().open_now().len(), before, "the limit holds it");
    let told = told_on_its_next_turn(&held, asking);
    assert_eq!(told.len(), 1, "told once: {told:?}");
    assert_eq!(
        told[0].summary,
        "it did not start: this chat already has 1 task running, and it may have 1 at once. \
         Wait for one to report, then dispatch again."
    );
}

#[test]
fn a_dispatch_still_waiting_past_the_bound_starts_nothing_and_the_chat_is_told() {
    let (_plane, _host, _planes, id, held, asking) = short_with_a_steward();
    let (said, _) = dispatch(&held, &id, &Tickets::default(), asking, None, "tidy up");
    assert!(matches!(said, Answer::WaitingOnMemory { .. }), "{said:?}");
    let before = held.chats().open_now().len();
    let asked_at = Instant::now();

    // Within the bound it waits on.
    memory_freed(
        &held,
        &id,
        asked_at + MEMORY_WAIT - std::time::Duration::from_secs(30),
        &nothing_opens,
    );
    assert_eq!(not_started(&held), listed("waiting-on-memory", "tidy up"));
    assert!(told_on_its_next_turn(&held, asking).is_empty());

    memory_freed(
        &held,
        &id,
        asked_at + MEMORY_WAIT + std::time::Duration::from_secs(1),
        &nothing_opens,
    );

    assert_eq!(held.chats().open_now().len(), before, "nothing started");
    let told = told_on_its_next_turn(&held, asking);
    assert_eq!(told.len(), 1, "told once: {told:?}");
    assert_eq!(
        told[0].summary,
        format!("it did not start: {}", gave_up_on_memory())
    );
    assert_eq!(not_started(&held), listed("gave-up-on-memory", "tidy up"));
    // And memory freeing later starts nothing of it.
    memory_is(&held, Memory::Enough);
    memory_freed(&held, &id, Instant::now(), &nothing_opens);
    assert_eq!(held.chats().open_now().len(), before);
}

#[test]
fn a_dispatch_the_person_allowed_waits_on_memory_and_then_starts_on_the_grant_they_kept() {
    let (_plane, _host, _planes, id, held, asking) = short_with_a_steward();
    let (said, _) = dispatch(
        &held,
        &id,
        &Tickets::default(),
        asking,
        Some("devops"),
        "check the cluster",
    );
    assert!(matches!(said, Answer::NeedsGrant { .. }), "{said:?}");
    let pending = held.dispatch_grants().waiting(asking)[0].id;
    let before = held.chats().open_now().len();

    // Allowed on its tab, for this chat: a grant kept, which covers it when it is decided
    // again (D-1467-6).
    on_the_ground(&held, |ground| {
        held.dispatch_grants()
            .allow(ground, pending, purlis_core::sandbox::grant::Level::Chat)
    })
    .expect("allowed");

    eventually(|| {
        (not_started(&held) == listed("waiting-on-memory", "check the cluster")).then_some(())
    })
    .expect("it waits on memory");
    assert_eq!(held.chats().open_now().len(), before, "nothing started yet");

    memory_is(&held, Memory::Enough);
    memory_freed(&held, &id, Instant::now(), &nobody);

    assert_eq!(held.chats().open_now().len(), before + 1, "it started");
    assert!(
        held.dispatch_grants().waiting(asking).is_empty(),
        "the grant they kept covers it, and they are not asked again"
    );
    let told = told_on_its_next_turn(&held, asking);
    assert_eq!(told.len(), 1, "{told:?}");
    assert_eq!(told[0].answered, Some(Answered::Started));
}

#[test]
fn a_handoff_waits_on_memory_as_a_task_does_and_opens_once_memory_frees() {
    let plane = Plane::new();
    let host = Pretend::default();
    let planes = planes_on(&host);
    let id = planes.open(&plane.root);
    let held = planes.held(&id).expect("held");
    let asking = a_chat_on_work(&held, &plane.root);
    memory_is(&held, Memory::Short);
    let tickets = Tickets::default();
    let ticket = ticket(&held, &id, &tickets, asking);
    let before = held.chats().open_now().len();

    let said = answer(
        &held,
        &id,
        &tickets,
        1,
        a_named_open(asking, &ticket, stamped(asking), Some("lint")),
        &nothing_opens,
    );

    assert!(matches!(said, Answer::WaitingOnMemory { .. }), "{said:?}");
    assert_eq!(held.chats().open_now().len(), before, "nothing opened");

    memory_is(&held, Memory::Enough);
    memory_freed(&held, &id, Instant::now(), &nobody);

    assert_eq!(held.chats().open_now().len(), before + 1, "it opened");
    let told = told_on_its_next_turn(&held, asking);
    assert_eq!(told.len(), 1, "{told:?}");
    assert_eq!(told[0].answered, Some(Answered::Started));
}

#[test]
fn what_a_chat_that_closed_waited_on_memory_for_goes_with_it() {
    let (_plane, _host, _planes, id, held, asking) = short_with_a_steward();
    let (said, _) = dispatch(&held, &id, &Tickets::default(), asking, None, "tidy up");
    assert!(matches!(said, Answer::WaitingOnMemory { .. }), "{said:?}");

    let _ = held.close_chat(asking);
    memory_is(&held, Memory::Enough);
    let before = held.chats().open_now().len();
    memory_freed(&held, &id, Instant::now(), &nothing_opens);

    assert_eq!(held.chats().open_now().len(), before, "nothing started");
    assert!(not_started(&held).is_empty());
}

#[test]
fn a_chat_that_asks_in_a_loop_while_memory_is_short_is_refused_past_the_bound_it_may_hold() {
    let (_plane, _host, _planes, id, held, asking) = short_with_a_steward();
    for n in 0..MOST_ON_MEMORY_PER_CHAT {
        let (said, _) = dispatch(
            &held,
            &id,
            &Tickets::default(),
            asking,
            None,
            &format!("task {n}"),
        );
        assert!(
            matches!(said, Answer::WaitingOnMemory { .. }),
            "{n}: {said:?}"
        );
    }

    let (said, _) = dispatch(&held, &id, &Tickets::default(), asking, None, "one more");

    assert_eq!(
        said,
        Answer::No {
            why: too_many_on_memory()
        }
    );
    assert_eq!(
        held.held_dispatches().on_memory_listed().len(),
        MOST_ON_MEMORY_PER_CHAT
    );
}
