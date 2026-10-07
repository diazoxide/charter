//! `charter persona stats` against the recorded scenarios' planes: each expected table and
//! note below is the recorded row's stdout and stderr, byte for byte (`persona-stats-…` in
//! `tests/fixtures/recorded/behaviour.jsonl`). The notes about dispatches are purlis's own
//! since a persona stopped being a sub-agent (#1451).

use chrono::NaiveDate;

use super::*;
use crate::personaverbs::tests_plane::{Heard, OPS, OPS_LITE, OPS_MCP, Plane, SOLO, VAULTS};

/// The day the recorded rows' planes are read on: every memory in them is from March 2026,
/// so none is within 14 days of it.
fn today() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 9, 24).unwrap()
}

fn run(plane: &Plane, name: Option<&str>, recent_days: i64) -> (u8, Heard) {
    let mut heard = Heard::default();
    let rc = stats(plane.root(), name, recent_days, today(), &mut heard.sink());
    (rc, heard)
}

fn legend(days: i64) -> String {
    format!(
        "• RECENT = memories in the last {days} days · VERIFY = share carrying a verification \
         marker (quality proxy) · DUP = share in a near-dup pair (noise) · DISP = times \
         work was DISPATCHED to it (committed dispatch log, and this machine's dispatch \
         records) · ⬡/◇ = memory-blind role (activity: profile), not judged by volume.\n"
    )
}

/// What `DISP` is read from, and what it cannot see.
const SOURCE: &str = "• DISP adds the committed dispatch log (personas/_dispatch/), whose rows \
                      for a persona are from when one was sent out as a sub-agent, and the \
                      record purlis keeps of each dispatch on this machine. Those records are \
                      never committed and are kept 30 days, so a dispatch made on another \
                      machine, or longer ago than that, is not counted here.\n";

const NONE_ANYWHERE: &str = "• No dispatch to a persona is in the committed dispatch log or in \
                             this machine's dispatch records.\n";

const ONE_DRAFT: &str = "• 1 draft persona(s) — no chat is dispatched to a persona while \
                         `draft: true` is set, so they are not counted above. Finish the \
                         charter, then drop the line.\n";

fn dormant(n: usize) -> String {
    format!(
        "! {n} dormant persona(s) — a REAL prune signal (old, zero memory, no declared \
         activity: profile). The steward can quiz-propose removal (cite this).\n"
    )
}

fn idle(n: usize) -> String {
    format!("• {n} idle persona(s) — have memory but none recent; watch, don't prune yet.\n")
}

fn never_dispatched(n: usize) -> String {
    format!(
        "! {n} persona(s) NEVER dispatched — they exist, lint green, and are unused. Check \
         whether a chat does their work itself where it could dispatch to them.\n"
    )
}

#[test]
fn the_roster_is_every_persona_and_the_shared_namespace_largest_memory_first() {
    // persona-stats-reports-the-roster-and-the-shared-namespace
    let plane = Plane::fixture("daily");
    let (rc, heard) = run(&plane, None, 14);
    assert_eq!(rc, 0);
    assert_eq!(
        heard.out,
        "PERSONA    MEM  RECENT  VERIFY  DUP  DISP  STATUS\n\
         _shared      1       0      0%   0%     —  ○ idle\n\
         devops       1       0      0%   0%     1  ⚑ draft\n\
         steward      0       —       —    —     0  ⚑ never dispatched\n\n"
    );
    assert_eq!(
        heard.err,
        [
            legend(14),
            SOURCE.into(),
            never_dispatched(1),
            ONE_DRAFT.into(),
            dormant(1),
            idle(2),
        ]
        .concat()
    );
}

#[test]
fn a_plane_nothing_was_dispatched_on_says_so_and_judges_nobody_as_never_dispatched() {
    // persona-stats-on-a-plane-with-no-dispatches
    let plane = Plane::fixture("minimal");
    let (rc, heard) = run(&plane, None, 14);
    assert_eq!(rc, 0);
    assert_eq!(
        heard.out,
        "PERSONA    MEM  RECENT  VERIFY  DUP  DISP  STATUS\n\
         _shared      0       —       —    —     —  ✗ dormant\n\
         steward      0       —       —    —     0  ✗ dormant\n\n"
    );
    assert_eq!(
        heard.err,
        [legend(14), NONE_ANYWHERE.into(), SOURCE.into(), dormant(2),].concat()
    );
}

#[test]
fn chats_a_chat_started_are_one_total_because_the_log_names_no_persona_for_them() {
    // #1451: `persona stats` counts dispatches from what the committed log holds. A handoff
    // row has four fields and none is a persona (`dispatch::record_handoff`).
    let plane = Plane::fixture("minimal");
    let when = chrono::DateTime::parse_from_rfc3339("2026-10-07T10:00:00+00:00")
        .unwrap()
        .with_timezone(&chrono::Utc);
    for created in [false, true] {
        crate::dispatch::record_handoff(
            plane.root(),
            crate::dispatch::Placement::Here,
            created,
            when,
            "fixture-host",
        )
        .unwrap();
    }
    let (rc, heard) = run(&plane, None, 14);
    assert_eq!(rc, 0);
    assert_eq!(
        heard.err,
        [
            legend(14),
            "• 2 chat(s) were started by another chat (a handoff), by the committed dispatch \
             log. The log names no persona for them, so a row above counts one only where \
             this machine still keeps its record.\n"
                .into(),
            NONE_ANYWHERE.into(),
            SOURCE.into(),
            dormant(2),
        ]
        .concat()
    );
}

#[test]
fn one_named_persona_is_its_row_alone_with_no_shared_row_added() {
    // persona-stats-for-one-persona
    let plane = Plane::fixture("daily");
    let (rc, heard) = run(&plane, Some("devops"), 14);
    assert_eq!(rc, 0);
    assert_eq!(
        heard.out,
        "PERSONA    MEM  RECENT  VERIFY  DUP  DISP  STATUS\n\
         devops       1       0      0%   0%     1  ⚑ draft\n\n"
    );
    assert_eq!(
        heard.err,
        [legend(14), SOURCE.into(), ONE_DRAFT.into(), idle(1),].concat()
    );
}

#[test]
fn the_shared_namespace_asked_for_by_name_is_never_refused_and_never_dispatched() {
    // persona-stats-for-the-shared-namespace
    let plane = Plane::fixture("daily");
    let (rc, heard) = run(&plane, Some("_shared"), 14);
    assert_eq!(rc, 0);
    assert_eq!(
        heard.out,
        "PERSONA    MEM  RECENT  VERIFY  DUP  DISP  STATUS\n\
         _shared      1       0      0%   0%     —  ○ idle\n\n"
    );
    assert_eq!(heard.err, [legend(14), SOURCE.into(), idle(1)].concat());
}

#[test]
fn a_name_outside_the_alphabet_or_not_defined_is_refused_with_exit_one() {
    // persona-stats-refuses-a-name-outside-the-alphabet, …-a-persona-the-plane-does-not-define
    let plane = Plane::fixture("daily");
    let (rc, heard) = run(&plane, Some("Bad"), 14);
    assert_eq!((rc, heard.out.as_str()), (1, ""));
    assert_eq!(
        heard.err,
        "✗ invalid persona name 'Bad' (lowercase letters, digits, '.', '_', '-')\n"
    );
    let (rc, heard) = run(&plane, Some("ghost"), 14);
    assert_eq!((rc, heard.out.as_str()), (1, ""));
    assert_eq!(
        heard.err,
        "✗ no persona 'ghost' (create it: purlis persona create ghost)\n"
    );
}

#[test]
fn a_plane_with_no_personas_says_there_are_none_and_succeeds() {
    let plane = Plane::fixture("minimal");
    std::fs::remove_dir_all(plane.path("personas/steward")).unwrap();
    let (rc, heard) = run(&plane, None, 14);
    assert_eq!((rc, heard.out.as_str()), (0, ""));
    assert_eq!(heard.err, "• No personas yet.\n");
}

/// The plane of `persona-stats-reads-drift-advice-quality-and-reconciliation`: `ops` is an
/// orchestrator with two skills declared and one of them invoked, `devops` has three memories,
/// one carrying a verification marker and all three near-duplicates, and a backfill ran.
fn drift_plane() -> Plane {
    let plane = Plane::fixture("daily");
    plane.write(
        "personas/ops/persona.md",
        &OPS.replace(
            "disallowed-tools: WebFetch\n",
            "disallowed-tools: WebFetch\nactivity: orchestrator\n",
        ),
    );
    plane.write("personas/ops-lite/persona.md", OPS_LITE);
    plane.write("personas/solo/persona.md", SOLO);
    plane.write("personas/ops/mcp.json", OPS_MCP);
    plane.write(".charter/vaults.json", VAULTS);
    plane.write(
        "personas/_dispatch/2026-02.fixture-host.backfill.jsonl",
        "{\"agent\": \"ops\", \"ts\": \"2026-02-20T10:00:00\"}\n",
    );
    // Noon UTC, so the backfill's local date is 2026-03-02 in every timezone within 11 hours.
    std::fs::File::options()
        .write(true)
        .open(plane.path("personas/_dispatch/2026-02.fixture-host.backfill.jsonl"))
        .unwrap()
        .set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_772_452_800))
        .unwrap();
    plane.write(
        "personas/_dispatch/2026-03.fixture-host.jsonl",
        "{\"agent\": \"devops\", \"ts\": \"2026-03-02T09:33:00\"}\n\
         {\"event\": \"advice\", \"ts\": \"2026-03-02T09:35:00\"}\n\
         {\"agent\": \"general-purpose\", \"ts\": \"2026-03-02T09:36:00\"}\n\
         {\"agent\": \"ops\", \"ts\": \"2026-03-02T09:37:00\"}\n\
         {\"agent\": \"ops\", \"event\": \"resume\", \"ts\": \"2026-03-02T09:38:00\"}\n",
    );
    plane.write(
        "personas/_skills/2026-03.fixture-host.jsonl",
        "{\"persona\": \"ops\", \"skill\": \"deploy\", \"ts\": \"2026-03-02T09:50:00\"}\n\
         {\"persona\": \"ops\", \"skill\": \"superpowers:brainstorming\", \"ts\": \"2026-03-02T09:51:00\"}\n",
    );
    plane.write(
        "personas/devops/memory/cluster-prod-1-still-in-eu-west-1.md",
        "# Cluster prod-1 still in eu-west-1\n\n_2026-03-02 09:41 · persistent_\n\n\
         Cluster prod-1 lives in eu-west-1\n",
    );
    plane.write(
        "personas/devops/memory/verified-the-rollout-order.md",
        "# Verified the rollout order\n\n_2026-03-02 09:40 · persistent_\n\n\
         Confirmed the rollout order on staging: cluster prod-1 lives in eu-west-1.\n",
    );
    plane
}

#[test]
fn drift_advice_verification_and_duplicates_are_each_reported() {
    // persona-stats-reads-drift-advice-quality-and-reconciliation
    let plane = drift_plane();
    let (rc, heard) = run(&plane, None, 100_000);
    assert_eq!(rc, 0);
    assert_eq!(
        heard.out,
        "PERSONA     MEM  RECENT  VERIFY   DUP  DISP  STATUS\n\
         devops        3       3     33%  100%     1  ⚑ draft\n\
         _shared       1       1      0%    0%     —  ● active\n\
         ops           0       —       —     —     2  ⬡ orchestrator\n\
         ops-lite      0       —       —     —     0  ⚑ never dispatched\n\
         solo          0       —       —     —     0  ⚑ never dispatched\n\
         steward       0       —       —     —     0  ⚑ never dispatched\n\
         \n\
         SKILLS — declared vs actually invoked\n  \
         ops   declared and never used: tdd\n  \
         ops   used but not declared: brainstorming\n\n"
    );
    assert_eq!(
        heard.err,
        [
            legend(100_000),
            "• Routing advice: fired 1 time(s) · work handed to a persona 3 time(s) since the \
             first one (2026-03-02). Advice that fires and is never followed is the block \
             failing, not the roster — read it that way before adding more personas.\n"
                .into(),
            "• SKILLS drift is named, not resolved: an unused declaration may be dead weight or \
             a skill whose moment has not come, and an undeclared one may be a charter out of \
             date or a persona reaching past its remit. Which it is depends on intent purlis \
             cannot read.\n"
                .into(),
            SOURCE.into(),
            never_dispatched(3),
            ONE_DRAFT.into(),
            dormant(3),
        ]
        .concat()
    );
}

#[test]
fn drift_is_one_personas_declared_leaves_against_the_leaves_it_invoked() {
    let plane = drift_plane();
    // Another persona's invocation of `tdd` is not `ops`'s, and a row naming no skill
    // invokes none.
    plane.write(
        "personas/_skills/2026-04.fixture-host.jsonl",
        "{\"persona\": \"solo\", \"skill\": \"tdd\"}\n\
         {\"persona\": \"solo\"}\n\
         {\"persona\": \"solo\", \"skill\": \"\"}\n\
         [\"solo\", \"skill\"]\n",
    );
    assert_eq!(
        drift(plane.root(), "ops"),
        (vec!["tdd".to_string()], vec!["brainstorming".to_string()])
    );
    assert_eq!(
        skills_used(plane.root(), "solo"),
        ["tdd".to_string()].into()
    );
    assert_eq!(drift(plane.root(), "steward"), (vec![], vec![]));
}

#[test]
fn a_persona_drifting_one_way_only_is_listed_with_that_one_line() {
    let plane = drift_plane();
    plane.write(
        "personas/_skills/2026-03.fixture-host.jsonl",
        "{\"persona\": \"ops\", \"skill\": \"deploy\"}\n",
    );
    let (_, heard) = run(&plane, None, 100_000);
    assert!(
        heard.out.ends_with(
            "\nSKILLS — declared vs actually invoked\n  \
             ops   declared and never used: tdd\n\n"
        ),
        "{}",
        heard.out
    );
    plane.write(
        "personas/_skills/2026-03.fixture-host.jsonl",
        "{\"persona\": \"ops\", \"skill\": \"deploy\"}\n\
         {\"persona\": \"ops\", \"skill\": \"tdd\"}\n\
         {\"persona\": \"solo\", \"skill\": \"lint\"}\n",
    );
    let (_, heard) = run(&plane, None, 100_000);
    assert!(
        heard.out.ends_with(
            "\nSKILLS — declared vs actually invoked\n  \
             solo   used but not declared: lint\n\n"
        ),
        "{}",
        heard.out
    );
}

fn memory(plane: &Plane, persona: &str, file: &str, day: &str, body: &str) {
    plane.write(
        &format!("personas/{persona}/memory/{file}"),
        &format!("# {file}\n\n_{day} 09:00 · persistent_\n\n{body}\n"),
    );
}

#[test]
fn a_memory_is_recent_up_to_and_including_the_last_day_of_the_window() {
    let plane = Plane::fixture("minimal");
    memory(
        &plane,
        "steward",
        "edge.md",
        "2026-09-10",
        "alpha beta gamma",
    );
    memory(
        &plane,
        "steward",
        "older.md",
        "2026-09-09",
        "delta epsilon zeta",
    );
    let r = row(plane.root(), "steward", 14, today());
    assert_eq!((r.count, r.recent), (2, 1));
    assert_eq!(r.status, "active");
    let r = row(plane.root(), "steward", 13, today());
    assert_eq!((r.recent, r.status.as_str()), (0, "idle"));
}

#[test]
fn verify_and_dup_are_whole_percentages_of_the_memories_and_absent_without_any() {
    let plane = Plane::fixture("minimal");
    let r = row(plane.root(), "steward", 14, today());
    assert_eq!((r.count, r.verify_pct, r.dup_pct), (0, None, None));
    assert_eq!(r.status, "dormant");
    memory(
        &plane,
        "steward",
        "a.md",
        "2026-01-01",
        "Verified the deploy order",
    );
    memory(
        &plane,
        "steward",
        "b.md",
        "2026-01-01",
        "Reproduced the deploy order",
    );
    memory(
        &plane,
        "steward",
        "c.md",
        "2026-01-01",
        "Something else entirely",
    );
    let r = row(plane.root(), "steward", 14, today());
    assert_eq!(r.count, 3);
    assert_eq!(r.verify_pct, Some(67));
    assert_eq!(r.dup_pct, Some(67));
    assert_eq!(r.status, "idle");
}

#[test]
fn a_standby_or_advisory_role_is_shown_as_memory_blind_and_never_counted_dormant() {
    let plane = Plane::fixture("minimal");
    plane.write(
        "personas/steward/persona.md",
        "---\nname: steward\nactivity: Standby\n---\n",
    );
    plane.write(
        "personas/adviser/persona.md",
        "---\nactivity: advisory\n---\n",
    );
    let (rc, heard) = run(&plane, None, 14);
    assert_eq!(rc, 0);
    assert_eq!(
        heard.out,
        "PERSONA    MEM  RECENT  VERIFY  DUP  DISP  STATUS\n\
         _shared      0       —       —    —     —  ✗ dormant\n\
         adviser      0       —       —    —     0  ◇ advisory\n\
         steward      0       —       —    —     0  ◇ standby\n\n"
    );
    assert!(heard.err.ends_with(&dormant(1)), "{}", heard.err);
    // Any other activity profile is judged by volume like no profile at all.
    plane.write("personas/adviser/persona.md", "---\nactivity: busy\n---\n");
    assert_eq!(row(plane.root(), "adviser", 14, today()).status, "dormant");
}

#[test]
fn every_status_has_its_own_glyph_and_an_unknown_one_a_dot() {
    for (status, mark) in [
        ("active", "●"),
        ("idle", "○"),
        ("dormant", "✗"),
        ("draft", "⚑"),
        ("orchestrator", "⬡"),
        ("standby", "◇"),
        ("advisory", "◇"),
        ("never dispatched", "⚑"),
        ("elsewhere", "·"),
    ] {
        assert_eq!(glyph(status), mark, "{status}");
    }
}

#[test]
fn an_offset_of_hours_minutes_or_seconds_moves_the_instant_and_not_the_printed_date() {
    let at = |raw: &str| ts(&serde_json::json!({ "ts": raw })).map(|(utc, _)| utc.to_string());
    assert_eq!(
        at("2026-03-02T10:00:00+02").as_deref(),
        Some("2026-03-02 08:00:00")
    );
    assert_eq!(
        at("2026-03-02T10:00:00+0203").as_deref(),
        Some("2026-03-02 07:57:00")
    );
    assert_eq!(
        at("2026-03-02T10:00:00+02:03").as_deref(),
        Some("2026-03-02 07:57:00")
    );
    assert_eq!(
        at("2026-03-02T10:00:00+02:03:04").as_deref(),
        Some("2026-03-02 07:56:56")
    );
    assert_eq!(
        at("2026-03-02T10:00:00-02:03:04").as_deref(),
        Some("2026-03-02 12:03:04")
    );
    // An offset of any other length does not read, and neither does the stamp.
    assert_eq!(at("2026-03-02T10:00:00+020"), None);
    assert_eq!(
        ts(&serde_json::json!({ "ts": "2026-03-01T23:30:00+02:03:04" })).map(|(_, d)| d),
        NaiveDate::from_ymd_opt(2026, 3, 1)
    );
}

#[test]
fn an_hour_alone_is_two_digits_as_fromisoformat_requires() {
    let at = |raw: &str| ts(&serde_json::json!({ "ts": raw })).map(|(utc, _)| utc.to_string());
    assert_eq!(at("2026-03-04T10").as_deref(), Some("2026-03-04 10:00:00"));
    assert_eq!(at("2026-03-04 10").as_deref(), Some("2026-03-04 10:00:00"));
    assert_eq!(at("2026-03-04T1"), None);
}

// ----- DISP counts the record of each dispatch (#1452) -----

/// A dispatch to `persona`, as the app records one on this machine.
fn a_recorded_dispatch(plane: &Plane, persona: &str, mode: crate::dispatchrecord::Mode) {
    use crate::dispatchrecord::{Asker, Opening, Place, Worker};
    crate::dispatchrecord::open(
        plane.root(),
        Opening {
            mode,
            asker: Asker::default(),
            persona: Some(persona.to_owned()),
            worker: Worker::default(),
            task: None,
            place: Place::default(),
            brief: "a brief".to_owned(),
            report_owed: false,
        },
        chrono::Utc::now(),
    )
    .expect("a record");
}

#[test]
fn a_dispatch_this_machine_recorded_is_counted_in_its_persona_s_row_beside_the_log_s() {
    // The daily plane's log holds one row for devops and none for steward, who is `never
    // dispatched` by the log alone.
    let plane = Plane::fixture("daily");
    a_recorded_dispatch(&plane, "steward", crate::dispatchrecord::Mode::Task);
    a_recorded_dispatch(&plane, "steward", crate::dispatchrecord::Mode::Handoff);
    a_recorded_dispatch(&plane, "devops", crate::dispatchrecord::Mode::Task);

    let (rc, heard) = run(&plane, None, 14);

    assert_eq!(rc, 0);
    assert_eq!(
        heard.out,
        "PERSONA    MEM  RECENT  VERIFY  DUP  DISP  STATUS\n\
         _shared      1       0      0%   0%     —  ○ idle\n\
         devops       1       0      0%   0%     2  ⚑ draft\n\
         steward      0       —       —    —     2  ✗ dormant\n\n"
    );
    assert!(heard.err.contains(SOURCE), "{}", heard.err);
    assert!(!heard.err.contains("NEVER dispatched"), "{}", heard.err);
}

#[test]
fn a_record_alone_is_a_dispatch_where_the_log_holds_none() {
    let plane = Plane::fixture("minimal");
    a_recorded_dispatch(&plane, "steward", crate::dispatchrecord::Mode::Task);

    let (_, heard) = run(&plane, None, 14);

    assert_eq!(
        heard.out,
        "PERSONA    MEM  RECENT  VERIFY  DUP  DISP  STATUS\n\
         _shared      0       —       —    —     —  ✗ dormant\n\
         steward      0       —       —    —     1  ✗ dormant\n\n"
    );
    assert_eq!(heard.err, [legend(14), SOURCE.into(), dormant(2)].concat());
}

#[cfg(unix)]
#[test]
fn records_this_process_may_not_read_are_said_as_unread_and_never_as_no_dispatches() {
    // What a sandboxed chat finds (D-1452-11): the store is there and it is denied.
    use std::os::unix::fs::PermissionsExt;
    let plane = Plane::fixture("minimal");
    a_recorded_dispatch(&plane, "steward", crate::dispatchrecord::Mode::Task);
    let store = crate::dispatchrecord::dir(plane.root());
    std::fs::set_permissions(&store, std::fs::Permissions::from_mode(0o000)).unwrap();
    // Root reads through any mode: there the refusal cannot be made, and nothing is claimed.
    let denied = std::fs::read_dir(&store).is_err();

    let (rc, heard) = run(&plane, None, 14);

    std::fs::set_permissions(&store, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert_eq!(rc, 0);
    if denied {
        assert_eq!(
            heard.err,
            [legend(14), format!("! {RECORDS_UNREAD}\n"), dormant(2),].concat()
        );
        assert!(
            !heard.err.contains("No dispatch to a persona"),
            "{}",
            heard.err
        );
    }
}
