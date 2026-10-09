use serde_json::json;

use super::*;

fn update(value: serde_json::Value) -> SessionUpdate {
    serde_json::from_value(value).expect("a session update")
}

#[test]
fn a_reply_longer_than_a_piece_comes_in_pieces_cut_on_character_boundaries() {
    assert_eq!(pieces("").count(), 0);
    assert_eq!(pieces("short").collect::<Vec<_>>(), ["short"]);
    // One byte, then two a character: the cap falls inside a character, so the first piece
    // ends one byte short of it.
    let text = format!("a{}", "é".repeat(MOST_TEXT_BYTES / 2));
    let cut: Vec<String> = pieces(&text).collect();
    assert_eq!(
        cut.iter().map(String::len).collect::<Vec<_>>(),
        [MOST_TEXT_BYTES - 1, 2]
    );
    assert_eq!(cut.concat(), text);
}

#[test]
fn a_long_step_is_cut_short_with_an_ellipsis() {
    assert_eq!(cut("step".to_owned(), 8), "step");
    assert_eq!(cut("abcdefgh".to_owned(), 8), "abcdefgh");
    assert_eq!(cut("abcdefghi".to_owned(), 8), "abcde…");
    let cut = cut("é".repeat(10), 8);
    assert_eq!(cut, "éé…");
    assert!(cut.len() <= 8);
}

#[test]
fn an_event_weighs_the_text_it_carries_and_a_little_for_itself() {
    assert_eq!(weight(&Event::Text("abc".to_owned())), 64 + 3);
    assert_eq!(weight(&Event::Ended), 64);
    assert_eq!(weight(&Event::Said(Said::Turn(Turn::Began))), 64);
    let plan = Plan {
        steps: vec![
            Step {
                text: "1234".to_owned(),
                done: false,
            },
            Step {
                text: "56".to_owned(),
                done: true,
            },
        ],
    };
    assert_eq!(weight(&Event::Said(Said::Plan(plan))), 64 + 6);
    assert_eq!(
        weight(&Event::ToolCallStatus {
            id: "t1".to_owned(),
            status: "completed".to_owned()
        }),
        64 + 2 + 9
    );
    assert_eq!(
        weight(&Event::Refused {
            method: "fs/read".to_owned()
        }),
        64 + 7
    );
}

fn lines_of(bytes: &[u8]) -> Vec<std::io::Result<Option<String>>> {
    let mut reader = futures::io::Cursor::new(bytes.to_vec());
    let mut read = Vec::new();
    futures::executor::block_on(async {
        loop {
            let line = capped_line(&mut reader).await;
            let done = !matches!(line, Ok(Some(_)));
            read.push(line);
            if done {
                break;
            }
        }
    });
    read
}

#[test]
fn a_line_is_read_without_its_ending_and_the_last_one_needs_none() {
    let read: Vec<Option<String>> = lines_of(b"one\r\ntwo\nthree")
        .into_iter()
        .map(|line| line.expect("read"))
        .collect();
    assert_eq!(
        read,
        [
            Some("one".to_owned()),
            Some("two".to_owned()),
            Some("three".to_owned()),
            None
        ]
    );
}

#[test]
fn a_line_past_the_cap_is_an_error_and_one_at_it_is_read() {
    let mut at_cap = vec![b'x'; MOST_LINE_BYTES];
    at_cap.push(b'\n');
    let read = lines_of(&at_cap);
    assert_eq!(
        read[0].as_ref().expect("read").as_ref().map(String::len),
        Some(MOST_LINE_BYTES)
    );
    let mut past = vec![b'x'; MOST_LINE_BYTES + 1];
    past.push(b'\n');
    assert!(lines_of(&past)[0].is_err());
}

#[test]
fn each_stop_reason_is_its_own_and_one_charter_does_not_know_reads_as_cancelled() {
    assert_eq!(stop(StopReason::EndTurn), Stop::EndTurn);
    assert_eq!(stop(StopReason::MaxTokens), Stop::MaxTokens);
    assert_eq!(stop(StopReason::MaxTurnRequests), Stop::MaxTurnRequests);
    assert_eq!(stop(StopReason::Refusal), Stop::Refusal);
    assert_eq!(stop(StopReason::Cancelled), Stop::Cancelled);
}

#[test]
fn a_plan_keeps_its_first_steps_each_cut_to_size() {
    let mut entries = vec![
        json!({"content": "p".repeat(MOST_STEP_BYTES * 2), "priority": "high", "status": "completed"}),
    ];
    entries.extend((0..MOST_PLAN_STEPS * 2).map(
        |at| json!({"content": format!("step {at}"), "priority": "low", "status": "pending"}),
    ));
    let reported = reported_as(update(json!({"sessionUpdate": "plan", "entries": entries})));
    let [Event::Said(Said::Plan(plan))] = reported.as_slice() else {
        panic!("{reported:?}");
    };
    assert_eq!(plan.steps.len(), MOST_PLAN_STEPS);
    assert!(plan.steps[0].done);
    assert!(plan.steps[0].text.len() <= MOST_STEP_BYTES);
    assert!(plan.steps[0].text.ends_with('…'));
    assert_eq!(plan.steps[1].text, "step 0");
}

#[test]
fn what_an_update_reports_in_charter_s_words() {
    assert_eq!(
        reported_as(update(
            json!({"sessionUpdate": "usage_update", "used": 50, "size": 200})
        )),
        [Event::Said(Said::Usage(Usage {
            input_tokens: None,
            output_tokens: None,
            context_percent: Some(25)
        }))]
    );
    // A window of no size reports no percentage, rather than divide by it.
    assert_eq!(
        reported_as(update(
            json!({"sessionUpdate": "usage_update", "used": 50, "size": 0})
        )),
        [Event::Said(Said::Usage(Usage::default()))]
    );
    assert_eq!(
        reported_as(update(
            json!({"sessionUpdate": "tool_call_update", "toolCallId": "t1"})
        )),
        []
    );
    assert_eq!(
        reported_as(update(
            json!({"sessionUpdate": "tool_call_update", "toolCallId": "t1", "status": "failed"})
        )),
        [Event::ToolCallStatus {
            id: "t1".to_owned(),
            status: "failed".to_owned()
        }]
    );
    let long = "a".repeat(MOST_TEXT_BYTES + 1);
    assert_eq!(
        reported_as(update(json!({"sessionUpdate": "agent_message_chunk",
                                   "content": {"type": "text", "text": long}})))
        .len(),
        2
    );
    assert_eq!(
        reported_as(update(json!({"sessionUpdate": "agent_thought_chunk",
                                   "content": {"type": "text", "text": "hmm"}}))),
        []
    );
}

#[test]
fn what_waits_to_be_written_is_bounded_in_bytes_and_one_line_past_the_bound_closes_it() {
    let unwritten = Unwritten::default();
    let half = "h".repeat(MOST_UNWRITTEN_BYTES / 2 - 1);
    unwritten.hold(half.clone()).expect("half fits");
    unwritten
        .hold(half.clone())
        .expect("the other half fits, line endings included");
    assert!(
        unwritten.hold("x".to_owned()).is_err(),
        "one byte past the bound"
    );
    // Closed: nothing more is held, and what was held is let go.
    assert!(unwritten.hold(String::new()).is_err());
    assert_eq!(lock(&unwritten.queue).lines.len(), 0);
}

#[test]
fn a_line_longer_than_the_bound_is_refused_on_its_own() {
    let unwritten = Unwritten::default();
    assert!(unwritten.hold("y".repeat(MOST_UNWRITTEN_BYTES)).is_err());
}

#[test]
fn a_prompt_too_long_to_write_is_refused_before_it_is_sent() {
    let session = SessionId::new("s-1");
    assert_eq!(prompt_request(&session, "hello").map(|_| ()), Ok(()));
    // The text alone fills the budget, so the line it is written in cannot fit.
    assert_eq!(
        prompt_request(&session, &"p".repeat(MOST_UNWRITTEN_BYTES)).map(|_| ()),
        Err(TurnFailed::TooLong)
    );
    // Measured as written: each quote is escaped, so half the budget in quotes is over it.
    assert_eq!(
        prompt_request(&session, &"\"".repeat(MOST_UNWRITTEN_BYTES / 2)).map(|_| ()),
        Err(TurnFailed::TooLong)
    );
    // Under the bound with room for the envelope: sent.
    assert!(prompt_request(&session, &"p".repeat(MOST_UNWRITTEN_BYTES - 4096)).is_ok());
    assert_eq!(
        TurnFailed::TooLong.to_string(),
        "the prompt is longer than purlis sends to an agent, 8 MiB"
    );
}

#[test]
fn the_writer_writes_each_line_in_order_and_gives_its_bytes_back() {
    let unwritten = Arc::new(Unwritten::default());
    unwritten.hold("one".to_owned()).expect("held");
    unwritten.hold("two".to_owned()).expect("held");
    let (read, written) = std::io::pipe().expect("a pipe");
    let writer = {
        let unwritten = Arc::clone(&unwritten);
        std::thread::spawn(move || unwritten.write_to(written))
    };
    let mut lines = std::io::BufRead::lines(std::io::BufReader::new(read));
    assert_eq!(lines.next().expect("a line").expect("read"), "one");
    assert_eq!(lines.next().expect("a line").expect("read"), "two");
    unwritten.close();
    writer.join().expect("the writer ends once closed");
    assert_eq!(lock(&unwritten.queue).bytes, 0);
}

#[test]
fn the_board_hears_a_chat_s_turns_and_asks_and_nothing_of_what_it_said() {
    // ADR 0073: the board reads the neutral model, so a level-3 chat moves exactly as a level-2
    // chat whose hooks said the same. Its text, its tool calls and a refused call move nothing;
    // its end is the program's exit, which the host reads from the operating system.
    let asks = Asks::new();
    let ask = crate::harness::model::Ask::default();
    let raised = asks.raise("chat-1", ask.clone(), Instant::now()).raised;
    let began = Said::Turn(Turn::Began);

    assert_eq!(Event::Said(began.clone()).said(), Some(began));
    assert_eq!(Event::Raised(raised).said(), Some(Said::Ask(ask)));
    for quiet in [
        Event::Text("hello".to_owned()),
        Event::ToolCall {
            id: "t".to_owned(),
            title: "ls".to_owned(),
            kind: "execute".to_owned(),
            status: "pending".to_owned(),
        },
        Event::ToolCallStatus {
            id: "t".to_owned(),
            status: "completed".to_owned(),
        },
        Event::Refused {
            method: "fs/read_text_file".to_owned(),
        },
        Event::Ended,
    ] {
        assert_eq!(quiet.said(), None, "{quiet:?}");
    }
}
