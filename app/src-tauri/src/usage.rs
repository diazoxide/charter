//! What one chat's conversation has cost: the `ctx`/`cache` gauge charter ADR 0019 recorded as
//! lost, and ADR 0038 left with no home.
//!
//! **The loss, in the ADR's words:** *"A framed Claude Code session has no context/cache gauge
//! on any surface."* Charter's statusline IS Claude Code's `statusLine`, so running Claude Code
//! inside charter cost the operator the context percentage he would see outside it. Two halves
//! were missing in the app, and this is the second:
//!
//! 1. **The feed.** Claude Code hands those numbers to its `statusLine` command and nothing
//!    else. `charter_core::harness` now arms `charter statusline` as this session's
//!    `statusLine`, and that command records each turn (`usage::record`).
//! 2. **The reader.** This command reads what was recorded — through the core's gated read,
//!    the same one the writer uses — and hands the window the numbers and the tone each is
//!    drawn in. The window draws them in the chat's own pane (`ChatGauge.tsx`).
//!
//! # Which file is this chat's
//!
//! A usage file is keyed by Claude Code's own session id. The board knows it: charter chose it
//! at the start, and the board follows the chat's own `/clear` to the new one and refuses a
//! nested harness's (`Board::conversation`, ADR 0024 C5/C6). A chat with no known conversation
//! has no gauge — never a guessed one.

use charter_core::usage::{self, Tone};

use crate::planes::{PlaneId, Planes};

/// How a number reads, as the window colours it — `charter_core::usage::Tone`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum GaugeTone {
    Ok,
    Warn,
    Bad,
}

impl From<Tone> for GaugeTone {
    fn from(tone: Tone) -> Self {
        match tone {
            Tone::Ok => Self::Ok,
            Tone::Warn => Self::Warn,
            Tone::Bad => Self::Bad,
        }
    }
}

/// One percentage on the gauge, and the tone its threshold gives it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct Percent {
    /// A whole percentage. `i32`, because `specta` refuses the core's `i64`, and a recorded
    /// value outside it is not a percentage anyone could read anyway.
    pub value: i32,
    pub tone: GaugeTone,
}

/// The prefix rebuilds this conversation has paid for (`↻N 696k`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct Rebuilds {
    pub count: u32,
    /// The total, already spelled as charter spells tokens (`696k`, `1.2M`), so the window
    /// and the frame's panel cannot come to round it differently.
    pub cost: String,
    pub tone: GaugeTone,
}

/// Everything the chat's gauge draws. Every part is absent when charter does not know it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct ChatUsage {
    /// `ctx NN%`: how full the context window was at the last turn that said.
    pub context: Option<Percent>,
    /// `cache NN%`: the share of the last turn's input served from cache.
    pub cache: Option<Percent>,
    pub rebuilds: Option<Rebuilds>,
}

/// What one chat's recorded usage says, or nothing.
///
/// **`None` is the ordinary answer and it draws nothing**: a chat on a harness with no
/// `statusLine` (Codex, a shell), a Claude Code chat before its first turn, a chat whose
/// record charter would not read. `frame/slots.py`'s rule, which `recorded_context_gauge`
/// keeps: a gauge silently reading zero is worse than no gauge.
///
/// Not on a blocking thread: it is one bounded read of a sixteen-line file.
#[tauri::command]
#[specta::specta]
pub fn chat_usage(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    session: u32,
) -> Result<Option<ChatUsage>, String> {
    let held = planes.held(&plane)?;
    // Copied out, so the board is not held across a file read.
    let Some(conversation) = held
        .hooks()
        .board()
        .conversation(session)
        .map(str::to_owned)
    else {
        return Ok(None);
    };
    Ok(of(held.root(), &conversation))
}

/// The gauge for Claude Code's session `conversation` on `plane`. The command is this after
/// it has asked the board which conversation the chat holds.
pub(crate) fn of(plane: &std::path::Path, conversation: &str) -> Option<ChatUsage> {
    let drawn = usage::gauge(&usage::history(plane, conversation));
    let percent = |(value, tone): (i64, Tone)| Percent {
        value: i32::try_from(value).unwrap_or(if value < 0 { i32::MIN } else { i32::MAX }),
        tone: tone.into(),
    };
    let found = ChatUsage {
        context: drawn.context.map(percent),
        cache: drawn.cache.map(percent),
        rebuilds: drawn.rebuilds.map(|(count, cost, tone)| Rebuilds {
            count: u32::try_from(count).unwrap_or(u32::MAX),
            cost,
            tone: tone.into(),
        }),
    };
    (found.context.is_some() || found.cache.is_some() || found.rebuilds.is_some()).then_some(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SID: &str = "11111111-2222-4333-8444-555555555555";

    fn plane_with(rows: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().expect("a directory");
        let root = std::fs::canonicalize(dir.path()).expect("it resolves");
        let sessions = root.join(usage::SESSIONS);
        std::fs::create_dir_all(&sessions).expect("a sessions directory");
        std::fs::write(sessions.join(format!("{SID}.usage")), rows).expect("a record");
        (dir, root)
    }

    #[test]
    fn a_recorded_conversation_is_drawn_with_the_tone_each_number_earns() {
        let (_dir, root) = plane_with("800000,2000,100,40\n10000,696088,1,83\n");

        let drawn = of(&root, SID).expect("a gauge");

        assert_eq!(
            drawn.context,
            Some(Percent {
                value: 83,
                tone: GaugeTone::Bad
            })
        );
        assert_eq!(
            drawn.cache,
            Some(Percent {
                value: 1,
                tone: GaugeTone::Bad
            })
        );
        assert_eq!(
            drawn.rebuilds,
            Some(Rebuilds {
                count: 1,
                cost: "696k".into(),
                tone: GaugeTone::Bad
            })
        );
    }

    #[test]
    fn a_conversation_with_nothing_recorded_draws_nothing_rather_than_zeros() {
        let (_dir, root) = plane_with("");

        assert_eq!(of(&root, SID), None);
        assert_eq!(of(&root, "nobody-at-all"), None);
    }
}
