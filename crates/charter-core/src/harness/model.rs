//! The neutral harness model (ADR 0073, FD-13): what a chat's harness tells charter, in
//! charter's words and not any one harness's.
//!
//! Every level speaks it. At level 2 a harness's own hooks report in charter's hook words
//! (`charter hook stop`, the words [`crate::state::Event`] parses), and [`Said::of_hook`] reads
//! one of those into this model. At level 3 a protocol client (HP-2 for ACP, HP-3 for a
//! harness's own protocol) will produce the same values, so nothing that reads a chat's state
//! has to know which level or which harness it came from.
//!
//! Six things, as the ticket names them: a **session**, a **turn**, an **item** inside a turn,
//! an **ask** that hands control to the operator, a **plan**, and **usage**. Level 2 carries
//! the first four today. A plan and usage have no hook that reports them as a state charter
//! draws; they are here so that a level-3 adapter has a place to put them, and the chat board
//! moves on none of them.

use crate::state::{Detail, Ending, Event, Started};

/// One thing a harness said about a chat.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Said {
    Session(Session),
    Turn(Turn),
    Item(Item),
    Ask(Ask),
    Plan(Plan),
    Usage(Usage),
}

/// What happened to the harness's session: the conversation charter follows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Session {
    /// A session is here: started, resumed, or a new conversation after a clear.
    Began(Began),
    /// The conversation was compacted. Nothing about the turn changed.
    Compacted,
    /// The conversation was cleared away, and another begins at once in the same process.
    /// The session did not end.
    ClearedAway,
    /// The session itself is over.
    Ended,
}

/// How a session came to be here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Began {
    /// Started or resumed, or nothing said which: what a session beginning means when the
    /// harness says nothing else.
    Fresh,
    /// A new conversation in the same process, after the old one was cleared.
    Cleared,
}

/// A turn: one prompt and everything the agent does about it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Turn {
    /// A prompt was submitted, and the agent is working on it.
    Began,
    /// The agent has nothing more to do, so the next move is the operator's.
    Ended,
}

/// One thing done inside a turn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    /// A child agent the turn dispatched has finished. The turn that dispatched it goes on.
    ChildEnded,
}

/// The harness handed control to the operator: a permission, a question, or a nudge that the
/// chat is idle. At level 2 it carries nothing more; a level-3 ask carries its options, so it
/// can be answered as data (ADR 0073 §1).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Ask {
    /// The answers the harness offers, in its order. Empty where the harness did not say.
    pub options: Vec<String>,
}

/// The steps the agent says it will take, in its order.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Plan {
    pub steps: Vec<Step>,
}

/// One step of a [`Plan`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub text: String,
    pub done: bool,
}

/// What a turn cost, as far as the harness said.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Usage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    /// How much of the context window is used, as a whole percentage.
    pub context_percent: Option<u8>,
}

impl Said {
    /// What a level-2 hook report means: `event` is the word `charter hook` was run with, and
    /// `detail` what its payload said beyond the event's name.
    ///
    /// The words are charter's own, the same for every harness: each adapter's hooks run
    /// `charter hook <word>`, so this reading is not any one harness's.
    pub fn of_hook(event: Event, detail: Detail) -> Self {
        match event {
            Event::SessionStart => Self::Session(match detail.started {
                Started::Freshly | Started::Unsaid => Session::Began(Began::Fresh),
                Started::Cleared => Session::Began(Began::Cleared),
                Started::Compacted => Session::Compacted,
            }),
            Event::UserPromptSubmit => Self::Turn(Turn::Began),
            Event::Notification => Self::Ask(Ask::default()),
            Event::SubagentStop => Self::Item(Item::ChildEnded),
            Event::Stop => Self::Turn(Turn::Ended),
            Event::SessionEnd => Self::Session(match detail.ending {
                Ending::Cleared => Session::ClearedAway,
                Ending::ForGood => Session::Ended,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn started(started: Started) -> Detail {
        Detail {
            started,
            ..Detail::default()
        }
    }

    fn ending(ending: Ending) -> Detail {
        Detail {
            ending,
            ..Detail::default()
        }
    }

    #[test]
    fn a_session_start_is_a_session_beginning_unless_it_was_a_compaction() {
        assert_eq!(
            Said::of_hook(Event::SessionStart, started(Started::Freshly)),
            Said::Session(Session::Began(Began::Fresh))
        );
        // Nothing said which is what a session start means when nothing says otherwise.
        assert_eq!(
            Said::of_hook(Event::SessionStart, started(Started::Unsaid)),
            Said::Session(Session::Began(Began::Fresh))
        );
        assert_eq!(
            Said::of_hook(Event::SessionStart, started(Started::Cleared)),
            Said::Session(Session::Began(Began::Cleared))
        );
        assert_eq!(
            Said::of_hook(Event::SessionStart, started(Started::Compacted)),
            Said::Session(Session::Compacted)
        );
    }

    #[test]
    fn a_session_end_for_a_clear_is_not_the_session_ending() {
        // Measured on claude 2.1.276: `/clear` fires `SessionEnd(reason=clear)` and then
        // `SessionStart(source=clear)` from the same process.
        assert_eq!(
            Said::of_hook(Event::SessionEnd, ending(Ending::Cleared)),
            Said::Session(Session::ClearedAway)
        );
        assert_eq!(
            Said::of_hook(Event::SessionEnd, ending(Ending::ForGood)),
            Said::Session(Session::Ended)
        );
    }

    #[test]
    fn a_prompt_begins_a_turn_and_stop_ends_it() {
        assert_eq!(
            Said::of_hook(Event::UserPromptSubmit, Detail::default()),
            Said::Turn(Turn::Began)
        );
        assert_eq!(
            Said::of_hook(Event::Stop, Detail::default()),
            Said::Turn(Turn::Ended)
        );
    }

    #[test]
    fn a_notification_is_an_ask_with_no_options_said() {
        assert_eq!(
            Said::of_hook(Event::Notification, Detail::default()),
            Said::Ask(Ask { options: vec![] })
        );
    }

    #[test]
    fn a_child_agent_finishing_is_an_item_of_the_turn_and_not_its_end() {
        assert_eq!(
            Said::of_hook(Event::SubagentStop, Detail::default()),
            Said::Item(Item::ChildEnded)
        );
    }
}
