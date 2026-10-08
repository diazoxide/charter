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

use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};

use purlis_core::dispatchdecision::{self, Counted, Refused};
use purlis_core::dispatchlimits::{self, Decision, Refused as Limited};

use crate::planes::Held;

/// What a chat's row says while it is at its task limit.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct AtLimit {
    /// The limit that binds, as a number: what the row says it is at.
    pub limit: u32,
    /// The whole sentence for the person: which limit, how many, and where it is changed.
    pub said: String,
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

/// The marks of one plane: by the chat refused, the persona its dispatch was to and which of
/// its own it counts.
#[derive(Debug, Default)]
pub struct AtLimits {
    refused: Mutex<HashMap<u32, (Option<String>, Counted)>>,
}

impl AtLimits {
    fn marks(&self) -> std::sync::MutexGuard<'_, HashMap<u32, (Option<String>, Counted)>> {
        self.refused.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Chat `chat`'s dispatch to `to` was refused for `why`: marked where a slot frees it.
    /// Answers whether it was marked.
    pub fn refused(&self, chat: u32, to: Option<String>, counted: Counted, why: &Refused) -> bool {
        if !frees_with_a_slot(why) {
            return false;
        }
        self.marks().insert(chat, (to, counted));
        true
    }

    /// Whether chat `chat` carries a mark, bound or not: a test reads it.
    #[cfg(test)]
    pub fn marked(&self, chat: u32) -> bool {
        self.marks().contains_key(&chat)
    }
}

/// **What chat `chat`'s row says of its task limit**, or nothing where no limit binds it now:
/// the dispatch it was refused is decided again against the limits and counts as they stand,
/// and a mark a slot has freed is dropped.
pub(crate) fn still(held: &Held, chat: u32) -> Option<AtLimit> {
    let (to, counted) = held.at_limits().marks().get(&chat).cloned()?;
    let bound = binds(held, chat, to.as_deref(), counted);
    if bound.is_none() {
        held.at_limits().marks().remove(&chat);
    }
    bound
}

/// The count refusal a dispatch from chat `chat` to `to` would meet now, as the person reads it.
fn binds(held: &Held, chat: u32, to: Option<&str>, counted: Counted) -> Option<AtLimit> {
    let root = held.root();
    let default = purlis_core::start::persona_for_a_new_chat(root);
    let (pair, cwd, lineage) = held.chats().deciding_over(|open, starting| {
        let (_, asking) = open.iter().find(|(number, _)| *number == chat)?;
        let pair = dispatchdecision::pair_of(asking, to, default.as_deref());
        let lineage = dispatchdecision::lineage_counting(
            chat,
            open,
            default.as_deref(),
            &|n| starting(n) || crate::handoff::still_working(held, n),
            &pair,
            counted,
        );
        Some((pair, asking.cwd.clone(), lineage))
    })?;
    let workspace = cwd
        .as_deref()
        .and_then(|cwd| purlis_core::active::workspace_of_tree(root, cwd));
    let limits = dispatchlimits::of(
        root,
        workspace.as_deref(),
        pair.asking.as_deref(),
        pair.to.as_deref(),
    );
    match dispatchlimits::decide(&limits, &lineage) {
        Decision::Refused(why) => {
            let why = Refused::Limit(why);
            if !frees_with_a_slot(&why) {
                return None;
            }
            let Refused::Limit(
                Limited::TooManyRunning { limit, .. }
                | Limited::LineageFull { limit, .. }
                | Limited::PersonaDispatches { limit, .. }
                | Limited::PersonaFull { limit, .. },
            ) = &why
            else {
                return None;
            };
            Some(AtLimit {
                limit: *limit,
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
