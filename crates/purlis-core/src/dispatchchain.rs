//! **An older chat's chain, read back from purlis's own dispatch records** (#1548).
//!
//! A chat dispatched by this build keeps the personas above it on its own record
//! ([`crate::reopen::HandedFrom::above`], #1521). One dispatched before that key keeps only
//! who asked it, by that chat's number, which is read among the chats still open
//! ([`crate::dispatchdecision::lineage_counting`]). Where that walk meets a chat that has
//! closed, the chain is not known ([`crate::dispatchlimits::Lineage::chain_unread`]) and every
//! dispatch it could hold is refused. Before refusing, [`recovered`] reads the chain from the
//! dispatch records ([`crate::dispatchrecord`]): who dispatched whom, by each chat's id, and the
//! persona each ran as.
//!
//! # What it trusts
//!
//! **purlis's own records, and nothing a chat sent.** The dispatch records and the record of
//! open chats are both written by the app, in its own state, which a sandboxed chat can
//! neither read nor write. A record is read only where it reads as one the app would have
//! written ([`crate::dispatchrecord::sound`]) and names a persona as purlis mints one. A chat
//! is matched to its record by its id, never by its number, which is dealt again in another
//! launch.
//!
//! # Never shorter than what is known
//!
//! The chain is recovered only where it is whole:
//!
//! - it reaches a chat whose record keeps its own chain; or
//! - it reaches an open chat whose record names no asking chat (the person's own), which,
//!   where the asking chat's record names the lineage's first chat (`root`), is that chat; or
//! - where the records stop at a chat that has closed, that chat is the lineage's first, **by
//!   the id the asking chat's record keeps for it**, and the chain is at least as long as that
//!   record says it is deep.
//!
//! A depth alone is never enough: a chat dispatched below one from before the depth key was
//! given a depth that is short, and names no root. So a chain whose records stop anywhere but
//! at the lineage's first chat, or whose asking chat names none, recovers nothing.
//!
//! Records that disagree on who asked a chat, a record purlis would not have written, and a
//! loop no dispatch made each recover nothing: the chain stays unread, and refused as before.
//! A chain the records show longer than the depth is kept as long as they show it: deeper is
//! the safe side.

use crate::dispatchrecord::Record;
use crate::reopen::Chat;

/// **The personas above chat `asking`, nearest first**, read from `open` (the chats the app
/// has open, by their numbers) and `records` (the project's dispatch records), where they
/// show the whole chain. `default` is the persona a chat on none runs as. `None` where they
/// do not ([`self`]).
pub fn recovered(
    asking: u32,
    open: &[(u32, &Chat)],
    default: Option<&str>,
    records: &[Record],
) -> Option<Vec<Option<String>>> {
    let by_number = |number: u32| {
        open.iter()
            .find(|(n, _)| *n == number)
            .map(|(_, chat)| *chat)
    };
    let by_id = |id: &str| {
        open.iter()
            .find(|(_, chat)| chat.identity.id.as_deref() == Some(id))
            .map(|(number, chat)| (*number, *chat))
    };
    let persona = |named: Option<&String>| named.cloned().or_else(|| default.map(str::to_owned));
    let asked = by_number(asking)?;
    let depth = asked.from.as_ref().map_or(0, |from| from.depth);
    // The chat the person started, by its id, as the asking chat's record keeps it: copied
    // down each dispatch, and absent where a chat above predates the key.
    let first = asked.from.as_ref().and_then(|from| from.root.as_deref());
    let mut chain = Vec::new();
    // Every chat walked, by its number among the open and by its id, so a loop ends.
    let mut numbers = vec![asking];
    let mut ids: Vec<String> = asked.identity.id.iter().cloned().collect();
    let mut at = At::Open(asked);
    // Each step walks one chat up, and no chat twice: no more steps than there are chats.
    for _ in 0..=open.len().saturating_add(records.len()) {
        let id = match at {
            At::Open(chat) => {
                let Some(from) = chat.from.as_ref() else {
                    // The chat the person started, or a finished task reopened as an ordinary
                    // chat: nothing is above it. Where the asking chat's record names the
                    // lineage's first chat, it must be this one.
                    let top = first.is_none_or(|first| chat.identity.id.as_deref() == Some(first));
                    return top.then_some(chain);
                };
                if let Some(kept) = from.above.as_ref() {
                    chain.extend(kept.iter().cloned());
                    return Some(chain);
                }
                chat.identity.id.as_deref()
            }
            At::Gone(ref id) => Some(id.as_str()),
        };
        // Who asked it: by its record, where one names it by its id.
        let asker = match id.map(|id| asker_of(id, records)) {
            Some(Asked::Disagree) => return None,
            Some(Asked::By(asker)) => Some(asker),
            Some(Asked::Unknown) | None => None,
        };
        let next = match (asker, &at) {
            (Some((persona_of, asker_id)), _) => {
                chain.push(persona(persona_of.as_ref()));
                match asker_id {
                    Some(asker_id) if ids.contains(&asker_id) => return None,
                    Some(asker_id) => {
                        ids.push(asker_id.clone());
                        match by_id(&asker_id) {
                            Some((number, chat)) if !numbers.contains(&number) => {
                                numbers.push(number);
                                Some(At::Open(chat))
                            }
                            Some(_) => return None,
                            None => Some(At::Gone(asker_id)),
                        }
                    }
                    None => None,
                }
            }
            // No record names it: the chat its own record says asked it, where it is open.
            (None, At::Open(chat)) => match chat.from.as_ref().map(|from| from.chat) {
                Some(number) if numbers.contains(&number) => return None,
                Some(number) => by_number(number).map(|above| {
                    numbers.push(number);
                    chain.push(persona(above.persona.as_ref()));
                    if let Some(id) = &above.identity.id {
                        ids.push(id.clone());
                    }
                    At::Open(above)
                }),
                None => None,
            },
            // No record names it: whole only if it is the chat the person started, as the
            // asking chat's record names it, and the chain is as deep as that record says.
            // A depth alone is never enough: one written below a chat from before the key
            // is short.
            (None, At::Gone(gone)) => {
                let long = u32::try_from(chain.len()).unwrap_or(u32::MAX);
                let top = first == Some(gone.as_str());
                return (top && depth > 0 && long >= depth).then_some(chain);
            }
        };
        // Where what is known stops short of the chat the person started: not whole.
        at = next?;
    }
    None
}

/// **Who is above a chat in its chain, as its own record keeps it**: what the dispatch
/// question reads to leave out a box the decision would refuse ([`crate::dispatchwants::also`]).
/// Read from the record alone, with no walk and no dispatch records, so for an older record
/// it fails closed: [`Above::Unread`], and fewer boxes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Above {
    /// The personas above it, nearest first, `None` for a chat on no persona. Empty for the
    /// chat the person started. There is no default: a caller says which it means.
    Known(Vec<Option<String>>),
    /// A record written before the chain was kept: any persona may be above it.
    Unread,
}

impl Above {
    /// What `chat`'s own record keeps: nothing above a chat no dispatch started, the chain a
    /// dispatched chat keeps, and [`Above::Unread`] for one that keeps none.
    pub fn of(chat: &Chat) -> Self {
        match chat.from.as_ref() {
            None => Self::Known(Vec::new()),
            Some(from) => from.above.clone().map_or(Self::Unread, Self::Known),
        }
    }
}

/// A chat on the way up: one the app has open, or one known only by its id from a record.
enum At<'a> {
    Open(&'a Chat),
    Gone(String),
}

/// What the records say of who dispatched the chat whose id is `id`.
enum Asked {
    /// No record names it.
    Unknown,
    /// The persona the asking chat ran as, and its id where the record names one.
    By((Option<String>, Option<String>)),
    /// Records that name it disagree, or one is not a record purlis would have written.
    Disagree,
}

/// Who the records say dispatched the chat whose id is `id`.
fn asker_of(id: &str, records: &[Record]) -> Asked {
    let mut found: Option<(Option<String>, Option<String>)> = None;
    for record in records
        .iter()
        .filter(|record| record.worker.chat.id.as_deref() == Some(id))
    {
        let asker = &record.asker.chat;
        let named_well = asker
            .persona
            .as_deref()
            .is_none_or(crate::contain::persona_name_ok);
        if !crate::dispatchrecord::sound(record) || !named_well {
            return Asked::Disagree;
        }
        let this = (asker.persona.clone(), asker.id.clone());
        match &found {
            Some(seen) if *seen != this => return Asked::Disagree,
            _ => found = Some(this),
        }
    }
    found.map_or(Asked::Unknown, Asked::By)
}

#[cfg(test)]
#[path = "dispatchchain_tests.rs"]
mod tests;
