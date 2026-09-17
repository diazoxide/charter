//! [`Engine`] on `alacritty_terminal`.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use alacritty_terminal::event::{Event, EventListener, WindowSize};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::vte::ansi::{NamedColor, Processor, Rgb};

use super::{Engine, Screen, Size};

pub struct AlacrittyEngine {
    term: Term<Replies>,
    parser: Processor,
    replies: Replies,
}

impl AlacrittyEngine {
    /// A blank terminal of `size`, keeping at most `scrollback` lines of history.
    pub fn new(size: Size, scrollback: usize) -> Self {
        let size = size.at_least_min();
        let replies = Replies::new(size);
        let config = Config {
            scrolling_history: scrollback,
            ..Config::default()
        };
        Self {
            term: Term::new(config, &Cells(size), replies.clone()),
            parser: Processor::new(),
            replies,
        }
    }
}

impl Engine for AlacrittyEngine {
    fn advance(&mut self, bytes: &[u8]) {
        self.parser.advance(&mut self.term, bytes);
    }

    fn resize(&mut self, size: Size) {
        let size = size.at_least_min();
        self.term.resize(Cells(size));
        self.replies.state().size = size;
    }

    fn screen(&self) -> Screen {
        let grid = self.term.grid();
        let lines = (0..grid.screen_lines())
            .map(|row| {
                let row = &grid[Line(row as i32)];
                let mut text = String::new();
                for cell in (0..grid.columns()).map(|column| &row[Column(column)]) {
                    if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                        continue;
                    }
                    text.push(cell.c);
                    text.extend(cell.zerowidth().unwrap_or_default());
                }
                text.truncate(text.trim_end().len());
                text
            })
            .collect();
        let point = grid.cursor.point;
        Screen {
            size: Size {
                columns: grid.columns() as u16,
                rows: grid.screen_lines() as u16,
            },
            lines,
            cursor: (point.line.0 as u16, point.column.0 as u16),
        }
    }

    fn take_replies(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.replies.state().bytes)
    }
}

/// Answers what the program asks its terminal, and collects the answers.
#[derive(Clone)]
struct Replies(Arc<Mutex<ReplyState>>);

struct ReplyState {
    bytes: Vec<u8>,
    size: Size,
}

impl Replies {
    fn new(size: Size) -> Self {
        Self(Arc::new(Mutex::new(ReplyState {
            bytes: Vec::new(),
            size,
        })))
    }

    fn state(&self) -> MutexGuard<'_, ReplyState> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl EventListener for Replies {
    fn send_event(&self, event: Event) {
        let mut state = self.state();
        let reply = match event {
            Event::PtyWrite(text) => text,
            // No pixels exist headless; zero is how a terminal says it does not know.
            Event::TextAreaSizeRequest(format) => format(WindowSize {
                num_lines: state.size.rows,
                num_cols: state.size.columns,
                cell_width: 0,
                cell_height: 0,
            }),
            Event::ColorRequest(index, format) => format(default_color(index)),
            // Clipboard reads (OSC 52) stay unanswered on purpose: a program must never read
            // the operator's clipboard through charter.
            _ => return,
        };
        state.bytes.extend_from_slice(reply.as_bytes());
    }
}

/// The colour a query for palette `index` is answered with: xterm's defaults, on a dark
/// background.
fn default_color(index: usize) -> Rgb {
    const FOREGROUND: Rgb = Rgb {
        r: 0xd8,
        g: 0xd8,
        b: 0xd8,
    };
    const BACKGROUND: Rgb = Rgb {
        r: 0x18,
        g: 0x18,
        b: 0x18,
    };
    const ANSI: [(u8, u8, u8); 16] = [
        (0x00, 0x00, 0x00),
        (0xcd, 0x00, 0x00),
        (0x00, 0xcd, 0x00),
        (0xcd, 0xcd, 0x00),
        (0x00, 0x00, 0xee),
        (0xcd, 0x00, 0xcd),
        (0x00, 0xcd, 0xcd),
        (0xe5, 0xe5, 0xe5),
        (0x7f, 0x7f, 0x7f),
        (0xff, 0x00, 0x00),
        (0x00, 0xff, 0x00),
        (0xff, 0xff, 0x00),
        (0x5c, 0x5c, 0xff),
        (0xff, 0x00, 0xff),
        (0x00, 0xff, 0xff),
        (0xff, 0xff, 0xff),
    ];
    let level = |step: usize| if step == 0 { 0 } else { (55 + step * 40) as u8 };
    match index {
        0..=15 => {
            let (r, g, b) = ANSI[index];
            Rgb { r, g, b }
        }
        16..=231 => {
            let cube = index - 16;
            Rgb {
                r: level(cube / 36),
                g: level(cube / 6 % 6),
                b: level(cube % 6),
            }
        }
        232..=255 => {
            let grey = (8 + (index - 232) * 10) as u8;
            Rgb {
                r: grey,
                g: grey,
                b: grey,
            }
        }
        _ if index == NamedColor::Background as usize => BACKGROUND,
        _ => FOREGROUND,
    }
}

/// A [`Size`] in the form `alacritty_terminal` measures it.
struct Cells(Size);

impl Dimensions for Cells {
    fn total_lines(&self) -> usize {
        self.screen_lines()
    }

    fn screen_lines(&self) -> usize {
        usize::from(self.0.rows)
    }

    fn columns(&self) -> usize {
        usize::from(self.0.columns)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIZE: Size = Size {
        columns: 20,
        rows: 4,
    };

    fn engine() -> AlacrittyEngine {
        AlacrittyEngine::new(SIZE, 100)
    }

    #[test]
    fn a_new_terminal_is_blank_with_the_cursor_home() {
        let screen = engine().screen();

        assert_eq!(screen.size, SIZE);
        assert_eq!(screen.lines, vec![""; 4]);
        assert_eq!(screen.cursor, (0, 0));
    }

    #[test]
    fn printed_lines_appear_on_screen() {
        let mut term = engine();

        term.advance(b"hello\r\nworld");

        let screen = term.screen();
        assert_eq!(screen.lines, vec!["hello", "world", "", ""]);
        assert_eq!(screen.cursor, (1, 5));
    }

    #[test]
    fn escape_sequences_move_the_cursor_and_overwrite() {
        let mut term = engine();

        // Print, jump back to row 1 column 1, overwrite.
        term.advance(b"abcdef\x1b[1;1HXY");

        assert_eq!(term.screen().lines[0], "XYcdef");
    }

    #[test]
    fn output_arriving_in_split_chunks_is_parsed_as_one_stream() {
        let mut term = engine();

        term.advance(b"red \x1b[3");
        term.advance(b"1mtext\x1b[0m");

        assert_eq!(term.screen().lines[0], "red text");
    }

    #[test]
    fn output_past_the_bottom_scrolls_the_screen() {
        let mut term = engine();

        term.advance(b"1\r\n2\r\n3\r\n4\r\n5");

        assert_eq!(term.screen().lines, vec!["2", "3", "4", "5"]);
    }

    #[test]
    fn resizing_changes_the_screen_size() {
        let mut term = engine();
        let wider = Size {
            columns: 40,
            rows: 6,
        };

        term.resize(wider);

        let screen = term.screen();
        assert_eq!(screen.size, wider);
        assert_eq!(screen.lines.len(), 6);
    }

    #[test]
    fn a_cursor_position_query_is_answered_through_replies() {
        let mut term = engine();
        term.advance(b"abc");

        term.advance(b"\x1b[6n");

        assert_eq!(term.take_replies(), b"\x1b[1;4R");
        assert!(
            term.take_replies().is_empty(),
            "replies are handed over once"
        );
    }

    #[test]
    fn a_zero_size_is_raised_to_the_smallest_terminal_instead_of_panicking() {
        let mut term = AlacrittyEngine::new(
            Size {
                columns: 0,
                rows: 0,
            },
            100,
        );
        term.resize(Size {
            columns: 0,
            rows: 5,
        });
        term.advance(b"still alive");

        assert_eq!(
            term.screen().size,
            Size {
                columns: 2,
                rows: 5
            }
        );
    }

    #[test]
    fn combining_marks_stay_with_their_base_character() {
        let mut term = engine();

        term.advance("e\u{301}x".as_bytes());

        assert_eq!(term.screen().lines[0], "e\u{301}x");
    }

    #[test]
    fn a_background_colour_query_is_answered() {
        let mut term = engine();

        term.advance(b"\x1b]11;?\x07");

        let reply = String::from_utf8(term.take_replies()).unwrap();
        assert!(reply.starts_with("\x1b]11;rgb:"), "got {reply:?}");
    }

    #[test]
    fn a_text_area_pixel_size_query_is_answered() {
        let mut term = engine();

        term.advance(b"\x1b[14t");

        assert!(!term.take_replies().is_empty());
    }
}
