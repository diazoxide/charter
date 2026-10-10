//! An age is an age, a declaration is the latest one, and a log full of junk still renders.

use std::path::{Path, PathBuf};

use super::*;

fn now() -> DateTime<Utc> {
    DateTime::from_timestamp(1_772_000_000, 0).unwrap()
}

fn at(offset_secs: i64) -> String {
    (now() - chrono::Duration::seconds(offset_secs)).to_rfc3339_opts(
        chrono::SecondsFormat::Secs,
        // `+00:00`, which is what `datetime.isoformat` writes for an aware UTC instant — NOT
        // `Z`, which is what charter would have to be reading if this used `Zulu`.
        false,
    )
}

fn a_plane() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    std::fs::create_dir_all(dir_for(&root, "alpha")).unwrap();
    (dir, root)
}

fn log(root: &Path, lines: &[serde_json::Value]) {
    let text: String = lines
        .iter()
        .map(|v| format!("{v}\n"))
        .collect::<Vec<_>>()
        .join("");
    std::fs::write(dir_for(root, "alpha").join("host.jsonl"), text).unwrap();
}

fn a_piece(root: &Path, repo: &str, piece: &str) {
    std::fs::create_dir_all(
        crate::worktree::root_of(root, "alpha")
            .join(repo)
            .join(piece),
    )
    .unwrap();
}

#[test]
fn the_latest_declaration_wins_and_the_earlier_ones_stay_in_the_log() {
    let (_held, root) = a_plane();
    log(
        &root,
        &[
            serde_json::json!({"ts": at(600), "event": "claimed", "repo": "svc", "piece": "p1"}),
            serde_json::json!({"ts": at(400), "event": "done", "repo": "svc", "piece": "p1"}),
            serde_json::json!({"ts": at(200), "event": "abandoned", "repo": "svc", "piece": "p1",
                               "reason": "blocked on review"}),
        ],
    );
    let declared = declarations(&root, "alpha");
    let entry = declared
        .get(&("svc".to_string(), "p1".to_string()))
        .unwrap();
    // A worker that declared `done` and then found it was not done must be able to say so.
    assert_eq!(outcome(Some(entry)), "abandoned: blocked on review");
    // The claim is still there and is not a declaration.
    assert!(claims(&root, "alpha").contains_key(&("svc".to_string(), "p1".to_string())));
    // Read oldest first, whatever order the lines were written in.
    assert_eq!(events(&root, "alpha").len(), 3);
}

#[test]
fn a_half_written_line_is_skipped_and_the_rest_of_the_log_still_reads() {
    let (_held, root) = a_plane();
    let good = serde_json::json!({"ts": at(60), "event": "claimed", "repo": "svc", "piece": "p1"});
    std::fs::write(
        dir_for(&root, "alpha").join("host.jsonl"),
        format!("{good}\nnot json at all\n[1,2]\n{{\"event\":\"claimed\"}}\n"),
    )
    .unwrap();
    // Three lines refused: one unparsable, one that is not an object, and one with no piece.
    assert_eq!(events(&root, "alpha").len(), 1);
}

#[test]
fn silence_is_measured_from_the_heartbeat_and_falls_back_to_the_claim() {
    let (_held, root) = a_plane();
    log(
        &root,
        &[
            serde_json::json!({"ts": at(3 * 86400), "event": "claimed", "repo": "svc",
                             "piece": "p1"}),
        ],
    );
    // No heartbeat yet: the age is the claim's, which is precisely the case worth seeing — a
    // worker that never got as far as a first turn.
    assert_eq!(
        silence(&root, "alpha", "svc", "p1", now()),
        Ok(Some("3d".into()))
    );

    let seen = seen_path(&root, "alpha", "svc", Some("p1"));
    std::fs::create_dir_all(seen.parent().unwrap()).unwrap();
    std::fs::write(&seen, format!("{{\"ts\": \"{}\"}}\n", at(7200))).unwrap();
    assert_eq!(
        silence(&root, "alpha", "svc", "p1", now()),
        Ok(Some("2h".into()))
    );

    // A declaration ends the silence: there is nothing being waited for.
    log(
        &root,
        &[
            serde_json::json!({"ts": at(3 * 86400), "event": "claimed", "repo": "svc",
                               "piece": "p1"}),
            serde_json::json!({"ts": at(60), "event": "done", "repo": "svc", "piece": "p1"}),
        ],
    );
    assert_eq!(silence(&root, "alpha", "svc", "p1", now()), Ok(None));
}

#[test]
fn an_age_is_coarse_and_never_negative() {
    assert_eq!(since(now(), now()), "0m");
    assert_eq!(since(now() - chrono::Duration::seconds(59), now()), "0m");
    assert_eq!(since(now() - chrono::Duration::seconds(60), now()), "1m");
    assert_eq!(since(now() - chrono::Duration::seconds(3599), now()), "59m");
    assert_eq!(since(now() - chrono::Duration::seconds(3600), now()), "1h");
    assert_eq!(
        since(now() - chrono::Duration::seconds(86399), now()),
        "23h"
    );
    assert_eq!(since(now() - chrono::Duration::seconds(86400), now()), "1d");
    // A stamp from the future is not a negative age.
    assert_eq!(since(now() + chrono::Duration::seconds(600), now()), "0m");
}

#[test]
fn the_oldest_silence_is_the_one_reported_and_coarse_ages_do_not_sort_lexically() {
    let summary = Summary {
        total: 3,
        done: 0,
        gave_up: 0,
        quiet: vec!["9m".into(), "2d".into(), "5h".into()],
    };
    // `9m` beats `2d` lexically, which is the whole reason the rank exists.
    assert_eq!(summary.oldest_silence(), Some("2d"));
    assert_eq!(silence_rank("9m"), 540);
    assert_eq!(silence_rank("2d"), 172_800);
    // An age charter did not write ranks zero rather than raising.
    assert_eq!(silence_rank("?"), 0);
    assert_eq!(silence_rank(""), 0);
    assert_eq!(silence_rank("xd"), 0);
}

#[test]
fn the_summary_counts_what_the_pieces_said_about_themselves() {
    let (_held, root) = a_plane();
    for piece in ["p1", "p2", "p3", "p4"] {
        a_piece(&root, "svc", piece);
    }
    log(
        &root,
        &[
            serde_json::json!({"ts": at(9000), "event": "claimed", "repo": "svc", "piece": "p1"}),
            serde_json::json!({"ts": at(8000), "event": "done", "repo": "svc", "piece": "p1"}),
            serde_json::json!({"ts": at(9000), "event": "claimed", "repo": "svc", "piece": "p2"}),
            serde_json::json!({"ts": at(8000), "event": "abandoned", "repo": "svc",
                               "piece": "p2"}),
            serde_json::json!({"ts": at(2 * 86400), "event": "claimed", "repo": "svc",
                               "piece": "p3"}),
            serde_json::json!({"ts": at(7200), "event": "claimed", "repo": "svc",
                               "piece": "p4"}),
        ],
    );
    let summary = summary(&root, "alpha", now()).unwrap();
    assert_eq!(summary.total, 4);
    assert_eq!(summary.done, 1);
    assert_eq!(summary.gave_up, 1);
    assert_eq!(summary.quiet.len(), 2);
    assert_eq!(summary.oldest_silence(), Some("2d"));
}

#[test]
fn a_workspace_with_no_pieces_has_no_cell_at_all() {
    let (_held, root) = a_plane();
    // No worktree root: nothing to count, and a `pieces 0` on every turn is furniture.
    assert_eq!(summary(&root, "alpha", now()), None);
    // A worktree root with a repo directory and no pieces under it is the same answer.
    std::fs::create_dir_all(crate::worktree::root_of(&root, "alpha").join("svc")).unwrap();
    assert_eq!(summary(&root, "alpha", now()), None);
}

#[test]
fn a_piece_nobody_ever_claimed_is_not_silent_it_is_just_a_worktree() {
    let (_held, root) = a_plane();
    a_piece(&root, "svc", "by-hand");
    // `git worktree add` by hand leaves no claim, and charter does not invent one.
    let summary = summary(&root, "alpha", now()).unwrap();
    assert_eq!(summary.total, 1);
    assert!(summary.quiet.is_empty());
    assert_eq!(silence(&root, "alpha", "svc", "by-hand", now()), Ok(None));
}

// ---- the heartbeat writer (`pieces.seen`, `hooks._touch_piece`) ----

fn a_canonical_plane() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(dir.path()).unwrap();
    (dir, root)
}

#[test]
fn a_heartbeat_is_written_as_charter_writes_it() {
    let (_d, root) = a_canonical_plane();
    let path = seen(
        &root,
        "alpha",
        "svc",
        Some("p1"),
        Some("s-1"),
        Some("ops"),
        now(),
    )
    .unwrap();
    assert_eq!(path, seen_path(&root, "alpha", "svc", Some("p1")));
    let stamp = at(0);
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        format!(
            "{{\"by\": {{\"ops\": \"{stamp}\"}}, \"persona\": \"ops\", \"session\": \"s-1\", \
             \"ts\": \"{stamp}\"}}\n"
        )
    );
    // No persona, no session: the two keys that need one are absent and the session is null.
    let clone = seen(&root, "alpha", "svc", None, None, None, now()).unwrap();
    assert_eq!(
        std::fs::read_to_string(clone).unwrap(),
        format!("{{\"session\": null, \"ts\": \"{stamp}\"}}\n")
    );
}

#[test]
fn a_persona_seen_within_the_hour_stays_present_and_an_older_one_drops() {
    let (_d, root) = a_canonical_plane();
    let p = seen_path(&root, "alpha", "svc", Some("p1"));
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(
        &p,
        serde_json::json!({"ts": at(10), "by": {"recent": at(600), "stale": at(7200)}}).to_string(),
    )
    .unwrap();
    seen(&root, "alpha", "svc", Some("p1"), None, Some("ops"), now()).unwrap();
    let got: Value = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    let by = got["by"].as_object().unwrap();
    assert!(by.contains_key("recent") && by.contains_key("ops"));
    assert!(!by.contains_key("stale"));
}

/// The names `by` holds after `ops` is seen over a record whose `by` is `prev`.
fn present_after(prev: serde_json::Value) -> Vec<String> {
    let (_d, root) = a_canonical_plane();
    let p = seen_path(&root, "alpha", "svc", Some("p1"));
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(&p, prev.to_string()).unwrap();
    seen(&root, "alpha", "svc", Some("p1"), None, Some("ops"), now()).unwrap();
    let got: Value = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    got["by"].as_object().unwrap().keys().cloned().collect()
}

#[test]
fn a_heartbeat_keeps_the_newest_eight_personas_and_drops_the_oldest() {
    // Seven already present, the oldest first; `ops` makes eight, which is all kept.
    let seven: serde_json::Map<String, Value> = (1..=7)
        .map(|n| (format!("p{n}"), Value::String(at(100 * (8 - n)))))
        .collect();
    let mut kept = present_after(serde_json::json!({"ts": at(10), "by": seven.clone()}));
    kept.sort();
    assert_eq!(kept, ["ops", "p1", "p2", "p3", "p4", "p5", "p6", "p7"]);

    // An eighth makes nine, one past the bound: the oldest, `p0`, is the one dropped.
    let mut eight = seven;
    eight.insert("p0".into(), Value::String(at(900)));
    let mut kept = present_after(serde_json::json!({"ts": at(10), "by": eight}));
    kept.sort();
    assert_eq!(kept, ["ops", "p1", "p2", "p3", "p4", "p5", "p6", "p7"]);
}

#[test]
fn a_presence_stamp_with_no_offset_drops_the_whole_touch() {
    // Python's subtraction raises on a naive stamp, and the touch is dropped rather than half
    // done: nothing is written, and the record is left as it was.
    let (_d, root) = a_canonical_plane();
    let p = seen_path(&root, "alpha", "svc", Some("p1"));
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    let before =
        serde_json::json!({"ts": at(10), "by": {"old": "2026-01-01T00:00:00"}}).to_string();
    std::fs::write(&p, &before).unwrap();

    assert_eq!(
        seen(&root, "alpha", "svc", Some("p1"), None, Some("ops"), now()),
        None
    );
    assert_eq!(std::fs::read_to_string(&p).unwrap(), before);
}

#[test]
fn a_previous_record_with_no_stamp_is_not_read_for_who_was_present() {
    // A record without a `ts` is not a heartbeat — Python's `_seen_record` returns None for it —
    // so the personas it lists are not carried forward.
    for prev in [
        serde_json::json!({"by": {"recent": at(60)}}),
        serde_json::json!({"ts": "", "by": {"recent": at(60)}}),
    ] {
        assert_eq!(present_after(prev), ["ops"]);
    }
    assert_eq!(
        present_after(serde_json::json!({"ts": at(10), "by": {"recent": at(60)}})),
        ["ops", "recent"]
    );
}

#[test]
fn a_touch_marks_the_piece_or_the_clone_the_cwd_stands_in_and_nothing_else() {
    let (_d, root) = a_canonical_plane();
    let piece = root.join("workspaces/alpha/.worktrees/svc/p1/src");
    let clone = root.join("workspaces/alpha/svc/src");
    std::fs::create_dir_all(&piece).unwrap();
    std::fs::create_dir_all(&clone).unwrap();
    touch(&root, &piece, Some("s"), None, now());
    touch(&root, &clone, Some("s"), None, now());
    touch(&root, &root, Some("s"), None, now());
    touch(
        &root,
        &root.join("workspaces/alpha"),
        Some("s"),
        None,
        now(),
    );
    assert!(seen_path(&root, "alpha", "svc", Some("p1")).is_file());
    assert!(seen_path(&root, "alpha", "svc", None).is_file());
    let listed: Vec<_> = std::fs::read_dir(dir_for(&root, "alpha").join(SEEN_DIR))
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(listed.len(), 2, "{listed:?}");
}

#[test]
fn the_age_falls_back_to_the_claim_and_then_to_a_question_mark() {
    let (_d, root) = a_canonical_plane();
    assert_eq!(
        seen_age(&root, "alpha", "svc", "p1", now()).as_deref(),
        Some("?")
    );
    std::fs::create_dir_all(dir_for(&root, "alpha")).unwrap();
    log(
        &root,
        &[serde_json::json!({"event": "claimed", "repo": "svc", "piece": "p1", "ts": at(7200)})],
    );
    assert_eq!(
        seen_age(&root, "alpha", "svc", "p1", now()).as_deref(),
        Some("2h")
    );
}

/// #434: the presence record is replaced whole, so a crash between the write and the rename
/// leaves the previous record rather than an empty one.
#[test]
fn a_presence_write_that_dies_before_its_rename_leaves_the_old_record_whole() {
    let (_d, root) = a_canonical_plane();
    let p = seen_path(&root, "alpha", "svc", None);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(&p, "{\"session\": null, \"ts\": \"old\"}\n").unwrap();
    let _killed = crate::rewrite::hook::set(|_, _| Err(std::io::Error::other("killed")));

    assert_eq!(seen(&root, "alpha", "svc", None, None, None, now()), None);

    assert_eq!(
        std::fs::read_to_string(&p).unwrap(),
        "{\"session\": null, \"ts\": \"old\"}\n"
    );
}

/// #434: a link at the presence record is refused, and what it points at is untouched.
#[cfg(unix)]
#[test]
fn a_presence_record_that_is_a_link_is_refused_and_its_target_is_untouched() {
    let (d, root) = a_canonical_plane();
    let theirs = d.path().join("theirs");
    std::fs::write(&theirs, "THEIRS\n").unwrap();
    let p = seen_path(&root, "alpha", "svc", None);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(&theirs, &p).unwrap();
    let fired = std::rc::Rc::new(std::cell::Cell::new(false));
    let seen_it = std::rc::Rc::clone(&fired);
    let _hook = crate::rewrite::hook::set(move |_, _| {
        seen_it.set(true);
        Ok(())
    });

    assert_eq!(seen(&root, "alpha", "svc", None, None, None, now()), None);

    assert_eq!(std::fs::read_to_string(&theirs).unwrap(), "THEIRS\n");
    assert!(p.is_symlink());
    assert!(!fired.get(), "refused before any temp was written");
}

#[test]
fn a_recorded_line_is_the_one_pythons_pieces_record_wrote() {
    // `json.dumps(line, sort_keys=True)` of `pieces.record`'s closed FIELDS, one line per event
    // in `<host>.jsonl` — the shape `docs/plane-format.md` documents and every reader here
    // parses. A null session and persona are written, as Python writes `None`; an empty
    // reason is not written at all.
    let (_held, root) = a_plane();
    let who = Who {
        session: Some("s-1".into()),
        persona: None,
        host: "box".into(),
        log: "box".into(),
    };

    let path = record(
        &root,
        "alpha",
        Event::Claimed,
        "svc",
        "p1",
        None,
        &who,
        now(),
    )
    .unwrap();
    record(
        &root,
        "alpha",
        Event::Abandoned,
        "svc",
        "p1",
        Some("blocked"),
        &who,
        now(),
    )
    .unwrap();

    assert_eq!(path, dir_for(&root, "alpha").join("box.jsonl"));
    let stamp = at(0);
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        format!(
            "{{\"event\": \"claimed\", \"host\": \"box\", \"persona\": null, \"piece\": \"p1\", \
             \"repo\": \"svc\", \"session\": \"s-1\", \"ts\": \"{stamp}\"}}\n\
             {{\"event\": \"abandoned\", \"host\": \"box\", \"persona\": null, \"piece\": \"p1\", \
             \"reason\": \"blocked\", \"repo\": \"svc\", \"session\": \"s-1\", \"ts\": \"{stamp}\"}}\n"
        )
    );
    assert_eq!(
        outcome(declarations(&root, "alpha").get(&("svc".into(), "p1".into()))),
        "abandoned: blocked"
    );
}

#[test]
fn who_here_files_its_line_under_the_device_id_and_keeps_the_hostname_as_its_label() {
    // FD-25 (#985): one constructor for this machine's `Who`, so no caller pairs a hostname
    // with a log name by hand. Before an id is minted the log keeps the hostname's name, and
    // asking writes no id.
    let (_held, root) = a_plane();
    let config = tempfile::tempdir().unwrap();
    let host = crate::dispatch::host();

    let before = Who::here(Some(config.path()), None, None);
    assert_eq!(
        (before.host.as_str(), before.log.as_str()),
        (host.as_str(), host.as_str())
    );
    assert_eq!(Who::here(None, None, None).log, host);
    assert!(
        !crate::machine::file(config.path()).exists(),
        "naming the log minted a device id"
    );

    let id = crate::machine::device_id(config.path()).unwrap();
    let who = Who::here(Some(config.path()), Some("s-1".into()), Some("ops".into()));
    assert_eq!(
        who,
        Who {
            session: Some("s-1".into()),
            persona: Some("ops".into()),
            host: host.clone(),
            log: id.clone(),
        }
    );

    let path = record(
        &root,
        "alpha",
        Event::Claimed,
        "svc",
        "p1",
        None,
        &who,
        now(),
    )
    .unwrap();
    assert_eq!(path, dir_for(&root, "alpha").join(format!("{id}.jsonl")));
    let line: serde_json::Value =
        serde_json::from_str(std::fs::read_to_string(&path).unwrap().trim()).unwrap();
    assert_eq!(line["host"], host.as_str());
}

#[test]
fn a_log_that_is_a_link_is_not_written_through() {
    let (_held, root) = a_plane();
    let outside = tempfile::tempdir().unwrap();
    let target = outside.path().join("elsewhere.jsonl");
    std::fs::write(&target, "").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&target, dir_for(&root, "alpha").join("box.jsonl")).unwrap();
    let who = Who {
        session: None,
        persona: None,
        host: "box".into(),
        log: "box".into(),
    };

    let wrote = record(&root, "alpha", Event::Done, "svc", "p1", None, &who, now());

    #[cfg(unix)]
    {
        assert!(wrote.is_none());
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "");
    }
}

#[test]
fn every_declaration_refusal_has_a_sentence_for_the_window() {
    // #989: the window says it of a branch and its folder, and names no command to run;
    // `charter worktree done` keeps its own sentence.
    let refusals = [
        NotDeclared::NoReason,
        NotDeclared::NoSuchPiece {
            ws: "alpha".into(),
            repo: "svc".into(),
            piece: "nope".into(),
        },
        NotDeclared::Worktree(crate::worktree::Refusal::NoSuchPiece {
            ws: "alpha".into(),
            repo: "svc".into(),
            piece: "nope".into(),
        }),
        NotDeclared::NotWritten { ws: "alpha".into() },
    ];
    for refusal in refusals {
        let said = refusal.in_window();
        assert!(!said.is_empty());
        for word in ["worktree", "piece", "charter worktree", "git -C", "--force"] {
            assert!(!said.contains(word), "{word:?} in {said}");
        }
    }
    assert!(
        NotDeclared::NoSuchPiece {
            ws: "alpha".into(),
            repo: "svc".into(),
            piece: "nope".into(),
        }
        .in_window()
        .contains("no branch folder called 'nope'")
    );
}

// ---- a branch cut for a chat that never started (#835) ----

fn stamp(offset_secs: i64) -> serde_json::Value {
    serde_json::Value::String(at(offset_secs))
}

#[test]
fn a_cut_nothing_has_spoken_for_is_unclaimed_for_as_long_as_it_has_existed() {
    let cut_at = now() - chrono::Duration::days(3);
    assert_eq!(unclaimed_age(cut_at, &[], now()).as_deref(), Some("3d"));
    // A missing heartbeat is a `None` stamp, and says nothing.
    assert_eq!(unclaimed_age(cut_at, &[None], now()).as_deref(), Some("3d"));
}

#[test]
fn a_claim_after_the_cut_speaks_for_it() {
    let cut_at = now() - chrono::Duration::hours(2);
    let claim = stamp(2 * 3600 - 5);
    assert_eq!(unclaimed_age(cut_at, &[Some(&claim)], now()), None);
}

#[test]
fn a_claim_in_the_same_second_as_the_cut_speaks_for_it() {
    // The log writes whole seconds, so a claim made 400ms after the cut reads as earlier.
    let cut_at = now() - chrono::Duration::hours(2) + chrono::Duration::milliseconds(400);
    let claim = stamp(2 * 3600);
    assert_eq!(unclaimed_age(cut_at, &[Some(&claim)], now()), None);
}

#[test]
fn a_claim_before_the_cut_is_about_an_earlier_branch_of_that_name() {
    let cut_at = now() - chrono::Duration::hours(2);
    let earlier = stamp(5 * 86400);
    assert_eq!(
        unclaimed_age(cut_at, &[Some(&earlier)], now()).as_deref(),
        Some("2h")
    );
}

#[test]
fn a_cut_whose_chat_may_still_be_starting_is_not_called_unclaimed() {
    let young = now() - chrono::Duration::seconds(UNCLAIMED_AFTER_SECS - 1);
    assert_eq!(unclaimed_age(young, &[], now()), None);
    let old = now() - chrono::Duration::seconds(UNCLAIMED_AFTER_SECS);
    assert_eq!(unclaimed_age(old, &[], now()).as_deref(), Some("5m"));
}

#[test]
fn a_stamp_that_is_not_an_instant_is_no_evidence_either_way() {
    let cut_at = now() - chrono::Duration::days(1);
    let naive = serde_json::Value::String("2026-03-01T00:00:00".into());
    let junk = serde_json::json!(17);
    assert_eq!(
        unclaimed_age(cut_at, &[Some(&naive), Some(&junk)], now()).as_deref(),
        Some("1d")
    );
}

// ---- when a branch was cut, from its reflog (#835) ----

const ZERO: &str = "0000000000000000000000000000000000000000";
const SHA: &str = "1111111111111111111111111111111111111111";

fn reflog_line(seconds: i64, message: &str) -> String {
    format!("{ZERO} {SHA} Jo Q. Writer <jo@example.invalid> {seconds} +0200\t{message}\n")
}

fn a_common_dir(branch: &str, reflog: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let common = dir.path().join("clone").join(".git");
    let at = common.join("logs").join("refs").join("heads").join(branch);
    std::fs::create_dir_all(at.parent().unwrap()).unwrap();
    std::fs::write(at, reflog).unwrap();
    (dir, common)
}

#[test]
fn a_branch_was_cut_when_its_reflog_says_it_was_created() {
    let created = now().timestamp() - 3 * 86400;
    let reflog = reflog_line(created, "branch: Created from HEAD")
        + &reflog_line(now().timestamp() - 60, "commit: later work");
    let (_dir, common) = a_common_dir("feature/chat-1", &reflog);

    assert_eq!(
        branch_created(&common, "feature/chat-1"),
        DateTime::<Utc>::from_timestamp(created, 0)
    );
}

#[test]
fn a_reflog_whose_first_line_is_not_a_creation_says_nothing() {
    // `git reflog expire` dropped the creation: the oldest line left is not when it was cut.
    let (_dir, common) = a_common_dir("chat-1", &reflog_line(1_700_000_000, "commit: work"));
    assert_eq!(branch_created(&common, "chat-1"), None);
    // No reflog at all, as with `core.logAllRefUpdates` off.
    assert_eq!(branch_created(&common, "chat-2"), None);
    for junk in [
        "",
        "no tab here",
        "a b c\tbranch: Created from HEAD",
        "a b c d notanumber +0000\tbranch: Created from HEAD",
    ] {
        assert_eq!(created_in(junk), None, "{junk:?}");
    }
}

#[cfg(unix)]
#[test]
fn a_fifo_at_a_reflogs_name_says_nothing_instead_of_holding_the_listing() {
    // The clone's git directory is written by the git a chat runs: a FIFO planted where the
    // reflog belongs would block a plain open for good, and every listing of branches with it.
    let (_dir, common) = a_common_dir("chat-1", &reflog_line(1_700_000_000, "commit: work"));
    let at = common.join("logs/refs/heads/chat-2");
    let made = crate::forklock::status(std::process::Command::new("mkfifo").arg(&at))
        .expect("mkfifo runs");
    assert!(made.success(), "the test needs a fifo to plant");

    let (say, heard) = std::sync::mpsc::channel();
    std::thread::spawn(move || say.send(branch_created(&common, "chat-2")));
    let answered = heard
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("reading a fifo reflog must not block");
    assert_eq!(answered, None);
}

#[test]
fn a_clone_with_a_git_file_keeps_its_branches_in_the_common_dir_it_names() {
    let dir = tempfile::tempdir().unwrap();
    let plain = dir.path().join("plain");
    std::fs::create_dir_all(plain.join(".git")).unwrap();
    assert_eq!(common_git_dir(&plain), Some(plain.join(".git")));

    // `--separate-git-dir`: the `.git` file names the git directory itself.
    let separate = dir.path().join("separate");
    std::fs::create_dir_all(&separate).unwrap();
    std::fs::write(separate.join(".git"), "gitdir: ../separate.git\n").unwrap();
    assert_eq!(
        common_git_dir(&separate),
        Some(separate.join("../separate.git"))
    );

    // A linked tree: its git directory names the common one in `commondir`.
    let linked = dir.path().join("linked");
    let admin = dir.path().join("main.git").join("worktrees").join("linked");
    std::fs::create_dir_all(&linked).unwrap();
    std::fs::create_dir_all(&admin).unwrap();
    std::fs::write(
        linked.join(".git"),
        format!("gitdir: {}\n", admin.display()),
    )
    .unwrap();
    std::fs::write(admin.join("commondir"), "../..\n").unwrap();
    assert_eq!(common_git_dir(&linked), Some(admin.join("../..")));

    assert_eq!(common_git_dir(&dir.path().join("none")), None);
}

#[test]
fn a_claim_mark_is_read_per_branch_from_gits_config_listing() {
    let out = "branch.chat-1.charterclaimed\n2026-10-09T12:00:00+00:00\0\
               branch.Feature/X.charterclaimed\n2026-10-08T00:00:00+00:00\0\
               branch..charterclaimed\nnobody\0\
               branch.chat-1.charterbase\nmain\0";
    let marks = claim_marks_in(out);
    assert_eq!(
        marks.get("chat-1").map(String::as_str),
        Some("2026-10-09T12:00:00+00:00")
    );
    assert_eq!(
        marks.get("Feature/X").map(String::as_str),
        Some("2026-10-08T00:00:00+00:00")
    );
    assert_eq!(marks.len(), 2);
}
