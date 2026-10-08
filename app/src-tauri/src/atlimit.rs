//! **A session at its task limit** (#1498, V100-54): a chat whose dispatch was refused for a
//! limit that a slot frees says so on its row, with the number and where to change it, until a
//! slot frees.
//!
//! **Only where a limit binds.** The mark is set by a refusal and by nothing else, and only for
//! a limit that counts tasks at work: how many one chat may have running, how many one lineage
//! may hold, and a persona's two counts ([`frees_with_a_slot`]). The depth, a limit of 0, the
//! loop rule and the message rate free with no slot, and are said to whoever asked, where they
//! asked.
//!
//! **It goes when a slot frees**, and that is asked, never guessed: each time the rows are
//! read, a marked chat's next dispatch to the same persona is decided against the limits and
//! the counts as they stand now, by the same functions the decision itself uses
//! ([`purlis_core::dispatchdecision::lineage_counting`], [`purlis_core::dispatchlimits::decide`]).
//! A task that reports or ends, a limit raised in Settings, and the chat's own close all clear
//! it at the next read. Held in memory: a launch starts with none, as no chat is refused yet.
//!
//! **And a session at its token limit** (#1512, V100-59): no slot frees that, since what a
//! session used only grows, but the person raising the limit does. So it is marked as a count
//! is, on a refusal and when the app's clock stops its tasks for it
//! ([`AtLimits::at_its_tokens`]), and re-decided the same way at every read: the row says
//! `at its token limit` with the figure until the limit is raised or taken out.

use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};

use purlis_core::dispatchdecision::{self, Counted, Refused};
use purlis_core::dispatchlimits::{self, Decision, Refused as Limited};

use crate::planes::Held;

/// What a chat's row says while it is at its task limit.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct AtLimit {
    /// The limit that binds, as a number.
    pub limit: u32,
    /// What the row says, naming which limit binds with its number ([`row_words`]): only the
    /// chat's own running limit is "its task limit", the one its tab menu's footer counts.
    pub row: String,
    /// The whole sentence for the person: which limit, how many, and where it is changed.
    pub said: String,
}

/// **What a row says of the limit `why`**, by which one it is, so it never reads as another
/// number than its tab menu's footer (`4 of 6 running`, the chat's own running limit):
///
/// - the chat's own running limit: `at its task limit (6)`, the footer's number;
/// - its chain's: `at its chain's limit (16 live)`;
/// - a persona's, which is the project's and not this chat's: `at devops's task limit (2,
///   across the project)` for what chats as that persona may have running, and `devops is
///   full (1 at once), not this chat's limit` for how many may run as it.
pub fn row_words(why: &Limited) -> Option<String> {
    use purlis_core::shown::short;
    Some(match why {
        Limited::TooManyRunning { limit, .. } => format!("at its task limit ({limit})"),
        Limited::LineageFull { limit, .. } => format!("at its chain's limit ({limit} live)"),
        Limited::PersonaDispatches { persona, limit, .. } => format!(
            "at {}'s task limit ({limit}, across the project)",
            short(persona)
        ),
        Limited::PersonaFull { persona, limit, .. } => format!(
            "{} is full ({limit} at once), not this chat's limit",
            short(persona)
        ),
        // A session's tokens (#1512): its own chat and its tasks together.
        Limited::SessionTokens { limit, .. } => format!(
            "at its token limit ({})",
            dispatchlimits::spelled(u64::from(*limit))
        ),
        _ => return None,
    })
}

/// The number of the limit `why` names, where the row says one.
fn limited_number(why: &Limited) -> Option<u32> {
    match why {
        Limited::TooManyRunning { limit, .. }
        | Limited::LineageFull { limit, .. }
        | Limited::PersonaDispatches { limit, .. }
        | Limited::PersonaFull { limit, .. }
        | Limited::SessionTokens { limit, .. } => Some(*limit),
        _ => None,
    }
}

/// Whether `why` is a refusal a slot frees: a count of tasks at work, which falls as one of
/// them reports or ends.
pub fn frees_with_a_slot(why: &Refused) -> bool {
    matches!(
        why,
        Refused::Limit(
            Limited::TooManyRunning { .. }
                | Limited::LineageFull { .. }
                | Limited::PersonaDispatches { .. }
                | Limited::PersonaFull { .. }
        )
    )
}

/// Whether `why` is said on the asking chat's row until it no longer binds: a count a slot
/// frees, or a session's tokens, which the person raising the limit frees (#1512).
pub fn shown_on_its_row(why: &Refused) -> bool {
    frees_with_a_slot(why) || matches!(why, Refused::Limit(Limited::SessionTokens { .. }))
}

/// The marks of one plane: by the chat refused, the persona its dispatch was to and which of
/// its own it counts.
///
/// Each mark carries the number of the refusal that set it, so a read that finds a slot free
/// drops only the mark it read, never one a refusal set meanwhile ([`still`]).
#[derive(Debug, Default)]
pub struct AtLimits {
    refused: Mutex<Marks>,
}

#[derive(Debug, Default)]
struct Marks {
    by_chat: HashMap<u32, Mark>,
    set: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Mark {
    to: Option<String>,
    counted: Counted,
    number: u64,
}

impl AtLimits {
    fn marks(&self) -> std::sync::MutexGuard<'_, Marks> {
        self.refused.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Chat `chat` dispatched, or closed: it is at no limit a refusal of its said (#1498, F3,
    /// F5). A dispatch let through is the plainest word that a slot is free for it.
    pub fn clear(&self, chat: u32) {
        self.marks().by_chat.remove(&chat);
    }

    /// Drops chat `chat`'s mark if it is still the one numbered `number`.
    fn drop_if(&self, chat: u32, number: u64) {
        let mut marks = self.marks();
        if marks
            .by_chat
            .get(&chat)
            .is_some_and(|mark| mark.number == number)
        {
            marks.by_chat.remove(&chat);
        }
    }

    /// Chat `chat`'s dispatch to `to` was refused for `why`: marked where a slot frees it.
    /// Answers whether it was marked.
    pub fn refused(&self, chat: u32, to: Option<String>, counted: Counted, why: &Refused) -> bool {
        if !shown_on_its_row(why) {
            return false;
        }
        let mut marks = self.marks();
        marks.set += 1;
        let number = marks.set;
        marks.by_chat.insert(
            chat,
            Mark {
                to,
                counted,
                number,
            },
        );
        true
    }

    /// The app's clock stopped session `chat`'s tasks at its token limit (#1512): its row says
    /// so, as a refusal for it would, until the limit no longer binds.
    pub fn at_its_tokens(&self, chat: u32) {
        let mut marks = self.marks();
        if marks.by_chat.contains_key(&chat) {
            return;
        }
        marks.set += 1;
        let number = marks.set;
        marks.by_chat.insert(
            chat,
            Mark {
                to: None,
                counted: Counted::Tasks,
                number,
            },
        );
    }

    /// Whether chat `chat` carries a mark, bound or not: a test reads it.
    #[cfg(test)]
    pub fn marked(&self, chat: u32) -> bool {
        self.marks().by_chat.contains_key(&chat)
    }
}

/// **What chat `chat`'s row says of its task limit**, or nothing where no limit binds it now:
/// the dispatch it was refused is decided again against the limits and counts as they stand,
/// and a mark a slot has freed is dropped.
///
/// The decision is made with no lock of the marks held (it reads the chats and the files); a
/// refusal that marks the chat again meanwhile keeps its mark, since only the mark that was
/// read is dropped.
pub(crate) fn still(held: &Held, chat: u32) -> Option<AtLimit> {
    let mark = held.at_limits().marks().by_chat.get(&chat).cloned()?;
    let bound = binds(held, chat, mark.to.as_deref(), mark.counted);
    if bound.is_none() {
        held.at_limits().drop_if(chat, mark.number);
    }
    bound
}

/// The count refusal a dispatch from chat `chat` to `to` would meet now, as the person reads it.
fn binds(held: &Held, chat: u32, to: Option<&str>, counted: Counted) -> Option<AtLimit> {
    let root = held.root();
    let default = purlis_core::start::persona_for_a_new_chat(root);
    let (limits, lineage) = held.chats().deciding_over(|open, starting| {
        let (_, asking) = open.iter().find(|(number, _)| *number == chat)?;
        let pair = dispatchdecision::pair_of(asking, to, default.as_deref());
        let mut lineage = dispatchdecision::lineage_counting(
            chat,
            open,
            default.as_deref(),
            &|n| starting(n) || crate::handoff::still_working(held, n),
            &pair,
            counted,
        );
        let workspace = asking
            .cwd
            .as_deref()
            .and_then(|cwd| purlis_core::active::workspace_of_tree(root, cwd));
        let limits = dispatchlimits::of(
            root,
            workspace.as_deref(),
            pair.asking.as_deref(),
            pair.to.as_deref(),
        );
        // What the session used, read only where a limit of it is in force (#1512).
        if limits.tokens_per_session.is_some() {
            lineage.tokens = crate::overlimit::session_tokens(root, held.board(), open, chat);
        }
        Some((limits, lineage))
    })?;
    match dispatchlimits::decide(&limits, &lineage) {
        Decision::Refused(why) => {
            let why = Refused::Limit(why);
            if !shown_on_its_row(&why) {
                return None;
            }
            let Refused::Limit(limited) = &why else {
                return None;
            };
            Some(AtLimit {
                limit: limited_number(limited)?,
                row: row_words(limited)?,
                said: crate::handoff::said_to_the_person(&why),
            })
        }
        Decision::Allowed => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_count_that_falls_as_a_task_ends_is_a_limit_a_slot_frees() {
        let freed = [
            Limited::TooManyRunning {
                limit: 6,
                running: 6,
            },
            Limited::LineageFull {
                limit: 16,
                lineage: 16,
            },
            Limited::PersonaDispatches {
                persona: "devops".to_owned(),
                limit: 2,
                running: 2,
            },
            Limited::PersonaFull {
                persona: "devops".to_owned(),
                limit: 1,
                running: 1,
            },
        ];
        for why in freed {
            assert!(frees_with_a_slot(&Refused::Limit(why.clone())), "{why:?}");
        }
        let not_freed = [
            Refused::Limit(Limited::TooDeep { limit: 3, depth: 3 }),
            Refused::Limit(Limited::Loop("steward".to_owned())),
            Refused::Limit(Limited::TooManyMessages {
                limit: 10,
                sent: 10,
            }),
            Refused::Held,
            Refused::NoPersona("ghost".to_owned()),
        ];
        for why in not_freed {
            assert!(!frees_with_a_slot(&why), "{why:?}");
        }
    }

    #[test]
    fn a_row_names_which_limit_binds_and_only_its_own_is_its_task_limit() {
        let said = |why: Limited| row_words(&why).expect("a count");
        assert_eq!(
            said(Limited::TooManyRunning {
                limit: 6,
                running: 6
            }),
            "at its task limit (6)"
        );
        assert_eq!(
            said(Limited::LineageFull {
                limit: 16,
                lineage: 16
            }),
            "at its chain's limit (16 live)"
        );
        assert_eq!(
            said(Limited::PersonaDispatches {
                persona: "devops".to_owned(),
                limit: 2,
                running: 2
            }),
            "at devops's task limit (2, across the project)"
        );
        assert_eq!(
            said(Limited::PersonaFull {
                persona: "devops".to_owned(),
                limit: 1,
                running: 1
            }),
            "devops is full (1 at once), not this chat's limit"
        );
        assert_eq!(row_words(&Limited::TooDeep { limit: 3, depth: 3 }), None);
    }

    #[test]
    fn a_read_drops_only_the_mark_it_read_never_one_set_meanwhile() {
        let marks = AtLimits::default();
        let full = Refused::Limit(Limited::TooManyRunning {
            limit: 6,
            running: 6,
        });
        marks.refused(4, None, Counted::Tasks, &full);
        let read = marks.marks().by_chat[&4].number;
        // A second refusal lands while the read decides: its mark stands.
        marks.refused(4, None, Counted::Tasks, &full);
        marks.drop_if(4, read);
        assert!(marks.marked(4));
        let now = marks.marks().by_chat[&4].number;
        marks.drop_if(4, now);
        assert!(!marks.marked(4));
        // And a close or a dispatch let through clears it outright.
        marks.refused(4, None, Counted::Tasks, &full);
        marks.clear(4);
        assert!(!marks.marked(4));
    }

    #[test]
    fn a_refusal_no_slot_frees_marks_nothing() {
        let marks = AtLimits::default();
        let deep = Refused::Limit(Limited::TooDeep { limit: 3, depth: 3 });
        assert!(!marks.refused(4, None, Counted::Tasks, &deep));
        assert!(!marks.marked(4));
        let full = Refused::Limit(Limited::TooManyRunning {
            limit: 6,
            running: 6,
        });
        assert!(marks.refused(4, Some("devops".to_owned()), Counted::Tasks, &full));
        assert!(marks.marked(4));
    }
}
