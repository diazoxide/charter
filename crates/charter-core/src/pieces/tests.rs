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
