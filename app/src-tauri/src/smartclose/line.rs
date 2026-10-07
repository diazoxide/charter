//! The line the person has typed into a chat's pane since their last Enter, read from their own
//! keys (#1361). It is what a smart-close pass is issued on: a `UserPromptSubmit` report comes
//! from inside the chat, where anything can send one, and the person's keys never do.
//!
//! **Known, or not.** Printable keys, a paste and Backspace are followed; Ctrl+C empties the
//! line, as the harnesses' own prompts do. Anything else — an arrow, Tab, a history key, the
//! harness's newline (`ESC CR`, Shift+Enter and Option+Enter), a backslash before Enter, a line
//! longer than [`LONGEST`] — leaves the line unknown until the next Enter. An unknown line is
//! never the smart-close command, so a line the app cannot follow costs the person the typed
//! pass, never more: the record is still written, and the window offers Close tab
//! (`Phase::KeptOpen`).
//!
//! **A picker's Enter on a typed prefix counts** (D-1361-6). The harnesses offer their commands
//! as the person types `/`, and Enter on `/sm` runs the one the picker highlights. A prefix of
//! `/smart-close` from `/sm` on, or of `/purlis:smart-close` from `/purlis:sm` on, is the person
//! asking for it; the `UserPromptSubmit` that follows must still say it was `/smart-close`
//! (`smartclose::heard`). `/` and `/s` alone never count: too many everyday commands start so.
//! An arrow in the picker means the person chose something other than the top match, which
//! the keys cannot name, so it stays unknown.

/// The longest line followed, as a bound on what is held per chat. Words after the command
/// count, so it is room for a sentence, not for the command alone.
const LONGEST: usize = 4096;

/// How much of a command's name a typed prefix must have, after its `/` and any prefix: `sm`.
const FLOOR: usize = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Line {
    /// Exactly these bytes are in the prompt.
    Typed(Vec<u8>),
    /// Something this cannot follow happened since the last Enter.
    Unknown,
}

impl Default for Line {
    fn default() -> Self {
        Self::Typed(Vec::new())
    }
}

impl Line {
    /// Follows the person's `bytes`, answering the line the first Enter in them submitted, if
    /// one did. Only the first: it is the one that can land in a chat waiting for the person,
    /// and every later one lands in the turn the first began.
    pub fn follow(&mut self, bytes: &[u8]) -> Option<Line> {
        let mut submitted = None;
        let mut rest = bytes;
        while let Some(&byte) = rest.first() {
            if let Some(body) = rest.strip_prefix(PASTE_STARTS) {
                let end = find(body, PASTE_ENDS).unwrap_or(body.len());
                for &pasted in &body[..end] {
                    // A newline in a paste is a newline in the prompt, never an Enter.
                    if matches!(pasted, b'\r' | b'\n') || pasted < 0x20 {
                        *self = Self::Unknown;
                    } else {
                        self.push(pasted);
                    }
                }
                rest = body.get(end + PASTE_ENDS.len()..).unwrap_or_default();
                continue;
            }
            rest = &rest[1..];
            match byte {
                b'\r' => {
                    let line = std::mem::take(self);
                    // A backslash before Enter is a newline to the harnesses, not a submit.
                    if matches!(&line, Self::Typed(typed) if typed.ends_with(b"\\")) {
                        *self = Self::Unknown;
                    } else if submitted.is_none() {
                        submitted = Some(line);
                    }
                }
                0x7f | 0x08 => self.backspace(),
                0x03 => *self = Self::default(),
                0x1b => {
                    *self = Self::Unknown;
                    rest = past_escape(rest);
                }
                0x00..=0x1f => *self = Self::Unknown,
                _ => self.push(byte),
            }
        }
        submitted
    }

    /// Whether nothing is typed: a line this followed, with no bytes in it.
    pub fn is_empty(&self) -> bool {
        matches!(self, Self::Typed(typed) if typed.is_empty())
    }

    fn push(&mut self, byte: u8) {
        if let Self::Typed(typed) = self {
            if typed.len() >= LONGEST {
                *self = Self::Unknown;
            } else {
                typed.push(byte);
            }
        }
    }

    /// Takes back one character, however many bytes it is.
    fn backspace(&mut self) {
        if let Self::Typed(typed) = self {
            while typed.pop().is_some_and(|byte| byte & 0xc0 == 0x80) {}
        }
    }

    /// Whether this is the person asking for purlis's smart-close skill as a slash command:
    /// `/smart-close` or `/purlis:smart-close` (the skill's old prefix too), alone or with words
    /// after it, or a prefix of one from `/sm` (`/purlis:sm`) on that a picker completes.
    pub fn is_smart_close(&self) -> bool {
        let Self::Typed(typed) = self else {
            return false;
        };
        let Ok(typed) = std::str::from_utf8(typed) else {
            return false;
        };
        let typed = typed.trim();
        purlis_core::state::smart_close_typed(typed) || picks_smart_close(typed)
    }
}

/// Whether `typed` is a prefix of `/smart-close`, or of it under a skill prefix, long enough
/// to reach [`FLOOR`] letters of its name.
fn picks_smart_close(typed: &str) -> bool {
    let namespace = purlis_core::names::SKILL_NAMESPACE;
    std::iter::once("")
        .chain(std::iter::once(namespace.write))
        .chain(namespace.reads.iter().copied())
        .any(|prefix| {
            let whole = format!("/{prefix}{}", purlis_core::state::SMART_CLOSE_SKILL);
            whole.starts_with(typed) && typed.len() >= 1 + prefix.len() + FLOOR
        })
}

const PASTE_STARTS: &[u8] = b"\x1b[200~";
const PASTE_ENDS: &[u8] = b"\x1b[201~";

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// What follows an escape sequence whose `ESC` is behind `rest`: a CSI's parameters and its
/// final byte, an SS3's one key, or the one byte an Alt key or `ESC CR` carries.
fn past_escape(rest: &[u8]) -> &[u8] {
    match rest.first() {
        Some(b'[') => {
            let body = &rest[1..];
            let end = body
                .iter()
                .position(|byte| (0x40..=0x7e).contains(byte))
                .map_or(body.len(), |at| at + 1);
            &body[end..]
        }
        Some(b'O') => rest.get(2..).unwrap_or_default(),
        Some(_) => &rest[1..],
        None => rest,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The line the first Enter in `keys`, each one write, submitted.
    fn submitted(keys: &[&[u8]]) -> Option<Line> {
        let mut line = Line::default();
        keys.iter().find_map(|key| line.follow(key))
    }

    fn is_smart_close(keys: &[&[u8]]) -> bool {
        submitted(keys).is_some_and(|line| line.is_smart_close())
    }

    #[test]
    fn the_command_typed_alone_is_smart_close() {
        assert!(is_smart_close(&[b"/smart-close\r"]));
        assert!(is_smart_close(&[b"/purlis:smart-close", b"\r"]));
        assert!(is_smart_close(&[b"/smart-closx", b"\x7f", b"e", b"\r"]));
        assert!(is_smart_close(&[b"go on", b"\x03", b"/smart-close\r"]));
        assert!(is_smart_close(&[b"\x1b[200~/smart-close\x1b[201~", b"\r"]));

        // A line starts empty after each Enter.
        let mut line = Line::default();
        line.follow(b"anything\r");
        assert!(
            line.follow(b"/smart-close\r")
                .is_some_and(|l| l.is_smart_close())
        );
    }

    #[test]
    fn a_line_that_is_more_or_other_than_the_command_is_not() {
        for keys in [
            &[&b"\r"[..]][..],
            &[b"go on\r"],
            &[b"/smart-closer\r"],
            &[b"/smart-close", b"\x03", b"go on\r"],
            &[b"/smart-close", b"\x03", b"\r"],
            &[b"/\r"],
            &[b"/s\r"],
            &[b"/purlis:s\r"],
            &[b"/purlis:\r"],
            &[b"/smx\r"],
            &[b"/sm now\r"],
            &[b"/smart-close", b"\x1b\r"],
            &[b"/smart-close\\", b"\r", b"\r"],
            &[b"/smart-close\x1b\r\r"],
            &[b"\x1b[200~/smart-close\nx\x1b[201~\r"],
        ] {
            assert!(!is_smart_close(keys), "{keys:?}");
        }
    }

    #[test]
    fn keys_it_cannot_follow_leave_the_line_unknown_until_the_next_enter() {
        for keys in [
            &[&b"\x1b[A"[..], b"\r"][..],
            &[b"/smart", b"\t", b"\r"],
            &[b"/smart-close", b"\x1b[D", b"\r"],
            &[b"\x15/smart-close\r"],
            &[b"\x1bOA/smart-close\r"],
        ] {
            assert_eq!(submitted(keys), Some(Line::Unknown), "{keys:?}");
        }
        let mut line = Line::default();
        assert_eq!(line.follow(b"\x1b[A\r"), Some(Line::Unknown));
        assert!(
            line.follow(b"/smart-close\r")
                .is_some_and(|l| l.is_smart_close())
        );
    }

    #[test]
    fn words_after_the_whole_command_count() {
        assert!(is_smart_close(&[b"/smart-close now", b"\r"]));
        assert!(is_smart_close(&[b"/purlis:smart-close and tidy up\r"]));
    }

    #[test]
    fn a_picker_s_enter_on_a_prefix_from_sm_counts() {
        for keys in [
            &[&b"/sm\r"[..]][..],
            &[b"/", b"s", b"m", b"\r"],
            &[b"/smart\r"],
            &[b"/purlis:sm\r"],
            &[b"/charter:smart-cl\r"],
        ] {
            assert!(is_smart_close(keys), "{keys:?}");
        }
    }

    #[test]
    fn a_picker_chosen_with_an_arrow_or_tab_is_unknown() {
        for keys in [
            &[&b"/sm"[..], b"\x1b[B", b"\r"][..],
            &[b"/s", b"\x1b[A", b"\r"],
            &[b"/sm", b"\t", b"\r"],
        ] {
            assert_eq!(submitted(keys), Some(Line::Unknown), "{keys:?}");
        }
    }

    #[test]
    fn ctrl_c_clears_a_typed_smart_close() {
        let mut line = Line::default();
        assert_eq!(line.follow(b"/smart-close"), None);
        assert_eq!(line.follow(b"\x03"), None);
        assert_eq!(line, Line::default());
        assert!(!line.follow(b"\r").is_some_and(|l| l.is_smart_close()));
    }

    #[test]
    fn a_line_longer_than_any_command_is_not_followed() {
        let mut line = Line::default();
        line.follow(&[b'x'; LONGEST + 1]);
        assert_eq!(line, Line::Unknown);
    }

    #[test]
    fn backspace_takes_back_a_whole_character() {
        assert!(is_smart_close(&[
            "/smart-closé".as_bytes(),
            b"\x7f",
            b"e\r"
        ]));
    }

    #[test]
    fn only_the_first_enter_in_one_write_is_the_one_answered() {
        assert_eq!(
            submitted(&[b"go on\r/smart-close\r"]),
            Some(Line::Typed(b"go on".to_vec()))
        );
    }
}
