//! Brokered writes (ADR 0067 §2, #1333): the project's own files a chat the app started asks
//! the app to write, because its sandbox does not let it. The write is the core's
//! ([`purlis_core::brokered::perform`]); what is the app's is who asks.

use purlis_core::active::Place;
use purlis_core::hookwire::{Answer, WriteAsk};

use crate::planes::Held;

/// Chat `ask.chat` asked the app to write one of the project's own files for it.
///
/// **The place and the persona are the app's record of the chat, never the request's**: the
/// workspace its directory was in when it started, and the persona it was started as. The chat the line names is
/// the one whose token it carries, which the listener checks before this is asked. Each chat is
/// held to [`purlis_core::brokered::Rate`], and every write is credited to it in the trace.
pub fn write(held: &Held, ask: &WriteAsk) -> Answer {
    let Some(open) = held
        .chats()
        .open_now()
        .into_iter()
        .find(|open| open.session == ask.chat)
    else {
        return Answer::No {
            why: format!("chat {} is not one this app has open", ask.chat),
        };
    };
    if let Err(why) = held.brokered().allow(ask.chat, std::time::Instant::now()) {
        return Answer::No { why };
    }
    // Resolved once, as the chat started (`chats::Open::workspace`): a link made since cannot
    // move where its writes go.
    let place = open.workspace.map_or(Place::PlaneRoot, Place::Workspace);
    let asker = purlis_core::brokered::Asker {
        chat: ask.chat,
        place,
        persona: open.persona,
    };
    match purlis_core::brokered::perform(
        held.root(),
        &asker,
        &ask.write,
        chrono::Local::now().naive_local(),
    ) {
        Ok(written) => {
            tracing::info!(
                "purlis: wrote {} for chat {} (a brokered write)",
                written.path,
                ask.chat
            );
            Answer::Written {
                to: written.to,
                path: written.path,
            }
        }
        Err(why) => Answer::No { why },
    }
}
