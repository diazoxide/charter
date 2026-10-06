//! A chat's child agents, as the board draws them under it (FD-18, W8).
//!
//! A sub-agent or teammate a harness spawns is a **child run** of the chat's run (ADR 0066).
//! It is told by the payload's `agent_id`, read only on a harness where that field was measured
//! to mean one ([`crate::hookwire::sub_agent`]), so nothing here reads a harness's output.
//!
//! A child has three states only (ADR 0076 §6): it is `working` from the first hook that names
//! it, `completed` at its own `SubagentStop`, or it ends with its parent, in the parent's end
//! state, when the parent ends first. The board's words for those are [`State::Running`],
//! [`State::Done`] and whatever the chat ended as. **No child move changes the chat's own
//! state**: a child's asks come on its parent's hooks and are the chat's.

use super::State;

/// One child agent of a chat: the harness's id for it, and where it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Child {
    /// The harness's `agent_id`. Unique within the chat, never across chats.
    pub agent: String,
    pub state: State,
}

/// How many children one chat keeps. A child that has ended is let go first, oldest first; a
/// chat with this many live children is shown no more of them, so nothing in a chat's process
/// tree can grow the board without bound by naming agent ids.
pub const HELD: usize = 64;

/// The longest agent id a child is shown for. Claude Code's and Codex's are well under it.
pub const AGENT_ID_AT_MOST: usize = 64;

/// Whether `agent` is an id a child is shown for: at most [`AGENT_ID_AT_MOST`] characters of
/// `[A-Za-z0-9._:-]`. **Checked by the host**, never trusted to the hook that sent it:
/// anything in the chat's process tree holds the chat's token and can send any line, and every
/// child rides on every snapshot of its chat, so an unbounded id is a snapshot as large as a
/// hook line, sent to the window on each move. Any other id is no child; the report it came
/// on still counts.
pub fn is_agent_id(agent: &str) -> bool {
    !agent.is_empty()
        && agent.len() <= AGENT_ID_AT_MOST
        && agent
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-'))
}

/// A chat's children, in the order they were first seen.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Children {
    list: Vec<Child>,
}

impl Children {
    /// Every child, oldest first.
    pub fn list(&self) -> &[Child] {
        &self.list
    }

    /// A hook named `agent`. A child seen for the first time is `working`; one already known,
    /// live or ended, stays as it is, and an id that is not one ([`is_agent_id`]) is no child. Answers whether anything a reader can see changed.
    pub fn seen(&mut self, agent: &str) -> bool {
        if !is_agent_id(agent) || self.list.iter().any(|child| child.agent == agent) {
            return false;
        }
        self.add(agent, State::Running)
    }

    /// `agent`'s own `SubagentStop`: it completed. One never seen before is shown completed,
    /// since it was a child all the same. Answers whether anything a reader can see changed.
    pub fn ended(&mut self, agent: &str) -> bool {
        if !is_agent_id(agent) {
            return false;
        }
        match self.list.iter_mut().find(|child| child.agent == agent) {
            Some(child) if child.state == State::Running => {
                child.state = State::Done;
                true
            }
            Some(_) => false,
            None => self.add(agent, State::Done),
        }
    }

    /// The parent ended first, as `parent`: every child still working ends with it. Answers
    /// whether anything a reader can see changed.
    pub fn end_with(&mut self, parent: State) -> bool {
        let mut changed = false;
        for child in &mut self.list {
            if child.state == State::Running {
                child.state = parent;
                changed = true;
            }
        }
        changed
    }

    /// The parent's run was superseded by a new one in the same process (`/clear`): its
    /// children were that run's, so the chat shows none from now on. Answers whether anything
    /// a reader can see changed.
    pub fn superseded(&mut self) -> bool {
        let had = !self.list.is_empty();
        self.list.clear();
        had
    }

    fn add(&mut self, agent: &str, state: State) -> bool {
        if self.list.len() >= HELD {
            match self
                .list
                .iter()
                .position(|child| child.state != State::Running)
            {
                Some(oldest) => {
                    self.list.remove(oldest);
                }
                None => return false,
            }
        }
        self.list.push(Child {
            agent: agent.to_owned(),
            state,
        });
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn states(children: &Children) -> Vec<(&str, State)> {
        children
            .list()
            .iter()
            .map(|child| (child.agent.as_str(), child.state))
            .collect()
    }

    #[test]
    fn a_child_is_working_from_the_first_hook_that_names_it_and_seen_once() {
        let mut children = Children::default();

        assert!(children.seen("a1"));
        assert!(!children.seen("a1"), "a second hook from it is no move");
        assert!(children.seen("a2"));

        assert_eq!(
            states(&children),
            [("a1", State::Running), ("a2", State::Running)]
        );
    }

    #[test]
    fn a_child_completes_at_its_own_stop_and_a_later_hook_does_not_bring_it_back() {
        let mut children = Children::default();
        children.seen("a1");

        assert!(children.ended("a1"));
        assert!(!children.ended("a1"));
        assert!(!children.seen("a1"));

        assert_eq!(states(&children), [("a1", State::Done)]);
    }

    #[test]
    fn a_child_first_heard_at_its_stop_is_shown_completed() {
        let mut children = Children::default();

        assert!(children.ended("quiet"));

        assert_eq!(states(&children), [("quiet", State::Done)]);
    }

    #[test]
    fn a_working_child_ends_with_its_parent_in_the_parents_end_and_a_completed_one_keeps_its_own() {
        let mut children = Children::default();
        children.seen("a1");
        children.seen("a2");
        children.ended("a2");

        assert!(children.end_with(State::Failed));
        assert!(!children.end_with(State::Failed), "nothing left working");

        assert_eq!(
            states(&children),
            [("a1", State::Failed), ("a2", State::Done)]
        );
    }

    #[test]
    fn a_new_run_in_the_same_process_shows_none_of_the_old_runs_children() {
        let mut children = Children::default();
        children.seen("a1");

        assert!(children.superseded());
        assert!(!children.superseded());

        assert!(children.list().is_empty());
    }

    #[test]
    fn an_agent_id_too_long_or_in_characters_no_harness_uses_is_no_child() {
        // Anything in the chat's process tree holds its hook token and can name any agent: an
        // id is bounded here, so sixty-four of them can never make a snapshot megabytes long.
        let mut children = Children::default();
        let huge = "a".repeat(70 * 1024);

        assert!(!children.seen(&huge));
        assert!(!children.ended(&huge));
        assert!(!children.seen(&"b".repeat(AGENT_ID_AT_MOST + 1)));
        assert!(!children.seen("a b"));
        assert!(!children.seen("a\u{1b}[2J"));
        assert!(!children.seen(""));

        assert!(children.list().is_empty());
        let longest = "c".repeat(AGENT_ID_AT_MOST);
        assert!(children.seen(&longest));
        assert!(children.seen("toolu_01:sub-agent.2"));
    }

    #[test]
    fn a_chat_holds_a_bounded_number_of_children_letting_the_oldest_ended_one_go_first() {
        let mut children = Children::default();
        children.ended("old");
        for n in 1..HELD {
            children.seen(&format!("live{n}"));
        }
        assert_eq!(children.list().len(), HELD);

        assert!(children.seen("next"), "the ended one makes room");
        assert!(children.list().iter().all(|child| child.agent != "old"));
        assert!(
            !children.seen("one-too-many"),
            "every one held is working, so no more is shown"
        );
        assert_eq!(children.list().len(), HELD);
    }
}
