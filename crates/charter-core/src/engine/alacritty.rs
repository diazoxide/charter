//! [`Engine`] on `alacritty_terminal`.

use std::sync::{Arc, Mutex, PoisonError};

use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::vte::ansi::Processor;

use super::{Engine, Screen, Size};

pub struct AlacrittyEngine {
    term: Term<Replies>,
    parser: Processor,
    replies: Replies,
}

impl AlacrittyEngine {
    /// A blank terminal of `size`, keeping at most `scrollback` lines of history.
    pub fn new(size: Size, scrollback: usize) -> Self {
        let replies = Replies::default();
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
        self.term.resize(Cells(size));
    }

    fn screen(&self) -> Screen {
        let grid = self.term.grid();
        let lines = (0..grid.screen_lines())
            .map(|row| {
                let row = &grid[Line(row as i32)];
                let text: String = (0..grid.columns())
                    .map(|column| &row[Column(column)])
                    .filter(|cell| !cell.flags.contains(Flags::WIDE_CHAR_SPACER))
                    .map(|cell| cell.c)
                    .collect();
                text.trim_end().to_owned()
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
        std::mem::take(
            &mut *self
                .replies
                .0
                .lock()
                .unwrap_or_else(PoisonError::into_inner),
        )
    }
}

/// Collects what the terminal writes back to the program.
#[derive(Clone, Default)]
struct Replies(Arc<Mutex<Vec<u8>>>);

impl EventListener for Replies {
    fn send_event(&self, event: Event) {
        if let Event::PtyWrite(text) = event {
            self.0
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .extend_from_slice(text.as_bytes());
        }
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
}
