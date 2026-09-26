//! A workspace's changes as `panel::Block`s: what the window's changes tab draws (#470).
//!
//! Produced here, in the core, in the vocabulary a stranger's view answers in, exactly as the
//! persona view is: the window draws it with the code that draws every other view.
//!
//! **Asked only when the tab is opened and when its Refresh is pressed.** The forge half is a
//! reading ([`observe`]), and the time of that reading is the first thing the view says. Nothing
//! on a workspace switch or a render asks for it.

use std::path::Path;

use chrono::{DateTime, Utc};

use super::observe::{Observation, Observed, observe};
use super::record::Record;
use super::store;
use crate::forge::checks::Ci;
use crate::forge::pr::State;
use crate::panel::{Block, Detail, Empty, Fact, Mark, Row, Tone};
use crate::shown;

/// The changes of workspace `ws`, each read from the forge now.
pub fn blocks(plane: &Path, ws: &str, now: DateTime<Utc>) -> Vec<Block> {
    let listing = store::read_all(plane, ws);
    let observed: Vec<(Record, Observation)> = listing
        .records
        .into_iter()
        .map(|record| {
            let seen = observe(plane, ws, &record, now);
            (record, seen)
        })
        .collect();
    let mut troubles: Vec<String> = listing.unread.map(|u| u.why).into_iter().collect();
    troubles.extend(
        listing
            .refused
            .iter()
            .map(|(slug, why)| format!("{}: {why}", shown::short(slug))),
    );
    drawn(ws, now, &troubles, &observed)
}

/// The blocks for what was read: pure, so what the tab says is testable without a forge.
pub fn drawn(
    ws: &str,
    now: DateTime<Utc>,
    troubles: &[String],
    changes: &[(Record, Observation)],
) -> Vec<Block> {
    let mut out = vec![Block::Facts(vec![
        Fact {
            label: "workspace".into(),
            value: shown::line(ws),
        },
        Fact {
            label: "read from the forge".into(),
            value: now.format("%Y-%m-%d %H:%M:%S UTC").to_string(),
        },
    ])];
    for why in troubles {
        out.push(Block::Note {
            text: shown::line(why),
            tone: Tone::Trouble,
        });
    }
    if changes.is_empty() {
        out.push(Block::List {
            rows: Vec::new(),
            empty: Empty {
                headline: format!("No cross-repo changes in {}", shown::line(ws)),
                body: Some(
                    "Declare one with `charter change create <slug> --why \"…\"`, then add each \
                     repo with `charter change add <slug> <repo>`."
                        .into(),
                ),
                offer: None,
            },
        });
        return out;
    }
    for (record, seen) in changes {
        let (merged, of) = seen.landed();
        out.push(Block::Note {
            text: format!(
                "{} · {merged} of {of} merged · {}",
                shown::line(&record.change),
                shown::line(&record.why)
            ),
            tone: Tone::Default,
        });
        out.push(Block::List {
            rows: seen.members.iter().map(|m| row(record, m)).collect(),
            empty: Empty {
                headline: "No members yet".into(),
                body: Some(format!(
                    "Add one with `charter change add {} <repo>`.",
                    shown::line(&record.change)
                )),
                offer: None,
            },
        });
    }
    out
}

/// One member's row: its repo and branch, its request, and its checks at the head.
fn row(record: &Record, m: &Observed) -> Row {
    let short = |sha: &str| -> String { shown::line(sha).chars().take(7).collect() };
    let (note, tone) = match &m.request {
        Err(why) => (
            format!("could not ask: {}", shown::line(why)),
            Tone::Trouble,
        ),
        Ok(None) => ("no request".to_string(), Tone::Plain),
        Ok(Some(req)) => {
            let state = super::observe::standing(&req.state);
            let mut note = format!("#{} {state} · head {}", req.number, short(&req.head));
            let mut tone = match req.state {
                State::Closed => Tone::Trouble,
                _ => Tone::Plain,
            };
            if let Some(checks) = &m.checks {
                note.push_str(&format!(" · checks {}", checks.ci.word()));
                // Only PASSED is drawn as fine; NOT RUN and UNKNOWN are never green.
                if checks.ci != Ci::Passed {
                    tone = Tone::Trouble;
                }
            }
            (note, tone)
        }
    };
    let mut text = format!("{} · {}", shown::line(&m.repo), shown::line(&m.branch));
    let needs = record
        .member(&m.repo)
        .map(|member| member.needs.clone())
        .unwrap_or_default();
    if !m.waiting_on.is_empty() {
        let names: Vec<String> = m.waiting_on.iter().map(|n| shown::line(n)).collect();
        text.push_str(&format!(
            " · {} {}",
            if m.merged() {
                "merged ahead of"
            } else {
                "blocked by"
            },
            names.join(", ")
        ));
    }
    let mut detail = vec![format!("branch: {}", shown::line(&m.branch))];
    if !needs.is_empty() {
        let names: Vec<String> = needs.iter().map(|n| shown::line(n)).collect();
        detail.push(format!("needs: {}", names.join(", ")));
    }
    if let Ok(Some(req)) = &m.request {
        detail.push(format!("request: {}", shown::line(&req.url)));
        detail.push(format!("head: {}", shown::line(&req.head)));
    }
    if let Some(why) = m.checks.as_ref().and_then(|c| c.why.as_ref()) {
        detail.push(format!("checks: {}", shown::line(why)));
    }
    Row {
        key: m.repo.clone(),
        text,
        note: Some(note),
        mark: Mark::Repo,
        tone,
        detail: Some(Detail::Text(detail.join("\n"))),
        runs: None,
        actions: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::change::Member;
    use crate::forge::checks::Checks;
    use crate::forge::pr::Request;

    fn at() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-09-26T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    fn record() -> Record {
        let mut r = Record::new("api-2", "bump the API", "t", "2026-09-26T00:00:00+00:00");
        r.members = vec![
            Member {
                repo: "svc".into(),
                branch: "change/api-2".into(),
                needs: vec![],
            },
            Member {
                repo: "web".into(),
                branch: "change/api-2".into(),
                needs: vec!["svc".into()],
            },
        ];
        r
    }

    fn open(ci: Ci) -> Observed {
        Observed {
            repo: "svc".into(),
            branch: "change/api-2".into(),
            request: Ok(Some(Request {
                number: 7,
                url: "https://x/pull/7".into(),
                state: State::Open,
                head: "9f3a1c2d4e5f".into(),
            })),
            checks: Some(Checks {
                total: Some(0),
                ci,
                why: None,
            }),
            waiting_on: vec![],
        }
    }

    fn rows(blocks: &[Block]) -> Vec<Row> {
        blocks
            .iter()
            .filter_map(|b| match b {
                Block::List { rows, .. } => Some(rows.clone()),
                _ => None,
            })
            .flatten()
            .collect()
    }

    #[test]
    fn a_workspace_with_no_changes_says_how_to_create_one() {
        let blocks = drawn("alpha", at(), &[], &[]);
        let Some(Block::List { rows, empty }) = blocks.last() else {
            panic!("{blocks:?}")
        };
        assert!(rows.is_empty());
        assert_eq!(empty.headline, "No cross-repo changes in alpha");
        assert!(
            empty
                .body
                .as_deref()
                .unwrap()
                .contains("charter change create")
        );
    }

    #[test]
    fn the_time_of_the_reading_is_the_first_thing_said() {
        let Block::Facts(facts) = &drawn("alpha", at(), &[], &[])[0] else {
            panic!()
        };
        assert_eq!(facts[1].label, "read from the forge");
        assert_eq!(facts[1].value, "2026-09-26 10:00:00 UTC");
    }

    #[test]
    fn each_member_is_a_row_with_its_request_head_and_checks() {
        let web = Observed {
            repo: "web".into(),
            request: Ok(None),
            checks: None,
            waiting_on: vec!["svc".into()],
            ..open(Ci::Passed)
        };
        let seen = Observation {
            at: at(),
            members: vec![open(Ci::Passed), web],
        };
        let blocks = drawn("alpha", at(), &[], &[(record(), seen)]);
        assert!(blocks.iter().any(|b| matches!(b, Block::Note { text, .. }
            if text == "api-2 · 0 of 2 merged · bump the API")));
        let rows = rows(&blocks);
        assert_eq!(rows[0].text, "svc · change/api-2");
        assert_eq!(
            rows[0].note.as_deref(),
            Some("#7 open · head 9f3a1c2 · checks PASSED")
        );
        assert_eq!(rows[0].tone, Tone::Plain);
        assert_eq!(rows[1].text, "web · change/api-2 · blocked by svc");
        assert_eq!(rows[1].note.as_deref(), Some("no request"));
    }

    #[test]
    fn not_run_and_unknown_are_never_drawn_as_fine() {
        for ci in [Ci::NotRun, Ci::Unknown, Ci::Failed, Ci::Running] {
            let seen = Observation {
                at: at(),
                members: vec![open(ci)],
            };
            let row = &rows(&drawn("alpha", at(), &[], &[(record(), seen)]))[0];
            assert_eq!(row.tone, Tone::Trouble, "{ci:?}");
            assert!(row.note.as_deref().unwrap().ends_with(ci.word()));
        }
    }

    #[test]
    fn a_record_charter_cannot_read_is_a_trouble_note() {
        let blocks = drawn("alpha", at(), &["bad: unknown key state".into()], &[]);
        assert!(
            blocks
                .iter()
                .any(|b| matches!(b, Block::Note { text, tone: Tone::Trouble }
            if text == "bad: unknown key state"))
        );
    }

    #[test]
    fn a_member_charter_could_not_ask_about_says_why() {
        let seen = Observation {
            at: at(),
            members: vec![Observed {
                request: Err("gh: not logged in\nforged".into()),
                checks: None,
                ..open(Ci::Passed)
            }],
        };
        let row = &rows(&drawn("alpha", at(), &[], &[(record(), seen)]))[0];
        assert_eq!(row.tone, Tone::Trouble);
        assert_eq!(
            row.note.as_deref(),
            Some("could not ask: gh: not logged in\\x0aforged")
        );
    }
}
