//! [`Engine`] on `alacritty_terminal`.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Instant;

use alacritty_terminal::event::{Event, EventListener, WindowSize};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::vte::ansi::{NamedColor, Processor, Rgb};

use super::pane_rules::{Beside, PaneRules};
use super::{Engine, Screen, Size};

pub struct AlacrittyEngine {
    term: Term<Replies>,
    parser: Processor,
    replies: Replies,
    /// What the pane rules need to know that `term` does not show.
    beside: Beside,
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
            beside: Beside::new(size.rows.into()),
        }
    }
}

impl AlacrittyEngine {
    /// The scrollback and screen, cell by cell, for tests that look past the text.
    #[cfg(test)]
    pub(super) fn grid(
        &self,
    ) -> &alacritty_terminal::grid::Grid<alacritty_terminal::term::cell::Cell> {
        self.term.grid()
    }

    fn apply_expired_sync(&mut self) {
        let expired = self
            .open_update()
            .is_some_and(|deadline| Instant::now() >= deadline);
        if expired {
            self.parser
                .stop_sync(&mut PaneRules(&mut self.term, &mut self.beside));
        }
    }
}

impl Engine for AlacrittyEngine {
    fn advance(&mut self, bytes: &[u8]) {
        self.apply_expired_sync();
        self.parser
            .advance(&mut PaneRules(&mut self.term, &mut self.beside), bytes);
    }

    fn open_update(&self) -> Option<Instant> {
        self.parser.sync_timeout().sync_timeout()
    }

    fn resize(&mut self, size: Size) {
        let size = size.at_least_min();
        let (columns, rows) = (self.term.columns(), self.term.screen_lines());
        self.term.resize(Cells(size));
        if (columns, rows) != (size.columns.into(), size.rows.into()) {
            self.beside.resized(size.rows.into());
        }
        self.replies.state().size = size;
    }

    fn screen(&mut self) -> Screen {
        self.apply_expired_sync();
        let grid = self.term.grid();
        let lines = (0..grid.screen_lines())
            .map(|row| {
                let row = &grid[Line(row as i32)];
                let mut text = String::new();
                for column in 0..grid.columns() {
                    let cell = &row[Column(column)];
                    // The second half of a wide character is drawn by the character. One whose
                    // character is gone is a blank, and keeps the columns after it in place.
                    if cell.flags.contains(Flags::WIDE_CHAR_SPACER)
                        && column > 0
                        && row[Column(column - 1)].flags.contains(Flags::WIDE_CHAR)
                    {
                        continue;
                    }
                    // A tab is a character the program put in the cell, and a blank is what
                    // the terminal draws for it.
                    text.push(if cell.c.is_control() { ' ' } else { cell.c });
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

    fn snapshot(&mut self) -> Vec<u8> {
        // Everything this engine has read has to be in the snapshot, because the view it is
        // for is sent the output that comes after. A synchronized update still open holds
        // bytes back, so it is ended here: half a frame drawn beats a frame lost.
        self.parser
            .stop_sync(&mut PaneRules(&mut self.term, &mut self.beside));
        super::snapshot::snapshot(&self.term, &self.beside)
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

    #[test]
    fn a_synchronized_update_that_never_ends_is_shown_once_its_timeout_passes() {
        // A program that dies mid-repaint never sends the end of the block. Its screen must
        // not freeze on what came before.
        let mut term = engine();
        term.advance(b"before\x1b[?2026h\x1b[H\x1b[2Jafter");
        assert_eq!(
            term.screen().lines[0],
            "before",
            "held back while the update is open"
        );

        std::thread::sleep(std::time::Duration::from_millis(300));

        assert_eq!(term.screen().lines[0], "after");
    }

    #[test]
    fn an_open_synchronized_update_reports_the_deadline_it_is_given_up_on() {
        let mut term = engine();

        term.advance(b"\x1b[?2026h\x1b[H\x1b[2Jhalf a frame");

        let deadline = term.open_update().expect("the update is open");
        assert!(
            deadline > Instant::now(),
            "an update just opened is not already past its deadline"
        );
    }

    #[test]
    fn output_with_no_synchronized_update_in_it_reports_no_deadline() {
        let mut term = engine();

        term.advance(b"a plain line\r\n");

        assert!(term.open_update().is_none());
    }

    #[test]
    fn an_update_the_program_closed_reports_no_deadline() {
        let mut term = engine();

        term.advance(b"\x1b[?2026h\x1b[H\x1b[2Jwhole frame\x1b[?2026l");

        assert!(term.open_update().is_none());
    }

    #[test]
    fn a_finished_synchronized_update_is_shown_at_once() {
        let mut term = engine();

        term.advance(b"before\x1b[?2026h\x1b[H\x1b[2Jafter\x1b[?2026l");

        assert_eq!(term.screen().lines[0], "after");
    }
}

#[cfg(test)]
mod snapshot_tests {
    use alacritty_terminal::grid::Dimensions;
    use alacritty_terminal::index::{Column, Line};
    use alacritty_terminal::term::TermMode;
    use proptest::prelude::*;

    use super::*;

    const SIZE: Size = Size {
        columns: 12,
        rows: 5,
    };
    const SCROLLBACK: usize = 50;

    fn fed(bytes: &[u8]) -> AlacrittyEngine {
        let mut term = AlacrittyEngine::new(SIZE, SCROLLBACK);
        term.advance(bytes);
        term
    }

    /// Every cell of the scrollback and the screen, as a person or a later escape sequence
    /// would see a difference.
    ///
    /// This is the active screen only: `alacritty_terminal` does not hand out the screen
    /// behind an open alternate screen, so the loss `snapshot` records there is one no number
    /// of cases here can catch.
    fn cells(term: &AlacrittyEngine) -> Vec<Vec<String>> {
        let grid = term.term.grid();
        (grid.topmost_line().0..=grid.bottommost_line().0)
            .map(|line| {
                let row = &grid[Line(line)];
                (0..grid.columns())
                    .map(|column| {
                        let cell = &row[Column(column)];
                        // The markers on the blanks a wide character leaves behind — at the
                        // end of a row too narrow for it, or where the character itself was
                        // overwritten — are not carried by a snapshot, and nothing draws them
                        // (see `snapshot`).
                        let orphan = column == 0
                            || !row[Column(column - 1)].flags.contains(Flags::WIDE_CHAR);
                        let mut flags = cell.flags & !Flags::LEADING_WIDE_CHAR_SPACER;
                        if orphan {
                            flags &= !Flags::WIDE_CHAR_SPACER;
                        }
                        // A cell can hold a tab, which draws nothing, and a snapshot puts the
                        // blank there instead (see `snapshot`).
                        let mut c = if cell.c.is_control() { ' ' } else { cell.c };
                        // A wide character pushed into the last column, where there is no
                        // room for its second half, is drawn as a blank (see `snapshot`).
                        if column + 1 == grid.columns() && flags.contains(Flags::WIDE_CHAR) {
                            flags &= !Flags::WIDE_CHAR;
                            c = ' ';
                        }
                        // A line only ever wraps at its last column. The mark can end up
                        // elsewhere when cells are deleted, where nothing reads it and a
                        // snapshot cannot put it back. Nor can it on the bottom row, or on
                        // the row waiting to wrap (see `snapshot`).
                        let waiting_to_wrap =
                            grid.cursor.input_needs_wrap && grid.cursor.point.line.0 == line;
                        if column + 1 < grid.columns()
                            || line == grid.bottommost_line().0
                            || waiting_to_wrap
                        {
                            flags &= !Flags::WRAPLINE;
                        }
                        if flags.contains(Flags::WIDE_CHAR_SPACER) {
                            // The second half of the wide character to its left. Cells
                            // deleted beside it can leave it with another character's pen.
                            // The pane, xterm.js, never holds such a pen: it writes the second
                            // half in the pen of the write that makes it, and writes it again
                            // in the current pen when either half is overwritten. So its own
                            // colours, attributes and link are not compared. Whether the line
                            // wraps after it still is.
                            return format!(
                                "second half of the wide character to its left {:?}",
                                flags & (Flags::WIDE_CHAR_SPACER | Flags::WRAPLINE)
                            );
                        }
                        // A cell that once carried a combining character keeps an empty
                        // place for one, which is not a difference anything can see.
                        let zerowidth = cell.zerowidth().filter(|marks| !marks.is_empty());
                        format!(
                            "{:?}{:?} fg={:?} bg={:?} {:?} ul={:?} link={:?}",
                            c,
                            zerowidth,
                            cell.fg,
                            cell.bg,
                            flags,
                            cell.underline_color(),
                            cell.hyperlink().map(|link| link.uri().to_owned()),
                        )
                    })
                    .collect()
            })
            .collect()
    }

    /// What changes how later output lands, or what keys send.
    fn state(term: &AlacrittyEngine) -> String {
        let grid = term.term.grid();
        let pen =
            |cursor: &alacritty_terminal::grid::Cursor<alacritty_terminal::term::cell::Cell>| {
                format!(
                    "{:?} fg={:?} bg={:?} {:?}",
                    cursor.point, cursor.template.fg, cursor.template.bg, cursor.template.flags
                )
            };
        let modes = *term.term.mode() & !(TermMode::URGENCY_HINTS | TermMode::VI);
        // The saved cursor as restoring it places it (see `pane_rules`).
        let mut saved = grid.saved_cursor.clone();
        saved.point.line = term.beside.saved_line(&term.term);
        format!(
            "cursor {} wrap-pending={} {:?} | saved {} | scroll region {:?} | {modes:?}",
            pen(&grid.cursor),
            grid.cursor.input_needs_wrap,
            term.term.cursor_style(),
            pen(&saved),
            term.beside.scroll_regions(&term.term),
        )
    }

    /// Escaped, so a failing case can be pasted straight into a test.
    fn readable(bytes: &[u8]) -> String {
        String::from_utf8_lossy(bytes)
            .chars()
            .map(|c| match c {
                '\x1b' => "\\x1b".to_owned(),
                '\r' => "\\r".to_owned(),
                '\n' => "\\n".to_owned(),
                c if c.is_control() => format!("\\x{:02x}", c as u32),
                c => c.to_string(),
            })
            .collect()
    }

    /// Every row as its characters alone, for reading a failure.
    fn text(term: &AlacrittyEngine) -> Vec<String> {
        let grid = term.term.grid();
        (grid.topmost_line().0..=grid.bottommost_line().0)
            .map(|line| {
                (0..grid.columns())
                    .map(|column| grid[Line(line)][Column(column)].c)
                    .collect()
            })
            .collect()
    }

    fn assert_rebuilt(bytes: &[u8]) {
        let mut original = fed(bytes);
        let snapshot = original.snapshot();
        let copy = fed(&snapshot);
        let (original_cells, copy_cells) = (cells(&original), cells(&copy));
        let differing = original_cells
            .iter()
            .zip(&copy_cells)
            .position(|(original, copy)| original != copy);
        if let Some(row) = differing {
            let columns: String = original_cells[row]
                .iter()
                .zip(&copy_cells[row])
                .enumerate()
                .filter(|(_, (original, copy))| original != copy)
                .map(|(column, (original, copy))| {
                    format!("\n  column {column}\n    original {original}\n        copy {copy}")
                })
                .collect();
            panic!(
                "row {row} of {} differs\n rows fed: {:#?}\nrows copy: {:#?}\n     fed: \"{}\"\nsnapshot: \"{}\"{columns}",
                original_cells.len(),
                text(&original),
                text(&copy),
                readable(bytes),
                readable(&snapshot),
            );
        }
        assert_eq!(
            copy_cells.len(),
            original_cells.len(),
            "different number of rows for \"{}\"",
            readable(bytes)
        );
        assert_eq!(
            state(&copy),
            state(&original),
            "state differs\n     fed: \"{}\"\nsnapshot: \"{}\"",
            readable(bytes),
            readable(&snapshot)
        );
    }

    #[test]
    fn a_snapshot_redraws_text_with_its_colours_and_attributes() {
        assert_rebuilt(
            b"plain \x1b[1;3;4mbold\x1b[0m\r\n\x1b[31;42mred\x1b[38;5;200;48;2;1;2;3mx\x1b[0m\r\n\x1b[2;7;8;9mdim\x1b[4:3;58;5;99mcurl",
        );
    }

    #[test]
    fn a_snapshot_keeps_the_scrollback() {
        assert_rebuilt(b"1\r\n2\r\n3\r\n4\r\n5\r\n6\r\n7\r\n8\r\n9");
    }

    #[test]
    fn a_snapshot_keeps_a_soft_wrapped_line_wrapped() {
        assert_rebuilt(b"abcdefghijklmnopqrstuvwxyz\r\nnext");
    }

    #[test]
    fn a_snapshot_keeps_wide_and_combining_characters() {
        assert_rebuilt("e\u{301} \u{4e2d}\u{6587}\r\n0123456789a\u{4e2d}".as_bytes());
    }

    #[test]
    fn a_snapshot_draws_a_wide_character_whose_second_half_came_from_another_one() {
        // Deleting cells at the second half of a wide character shifts the second half of
        // the next one into its place, in a different pen. Nothing draws a second half, and a
        // terminal writes it in the pen of the character it belongs to.
        assert_rebuilt("\u{4e2d}\u{4e2d}\x1b[7m\r\u{4e2d}\x08\x1b[2P".as_bytes());
    }

    #[test]
    fn a_snapshot_keeps_a_wide_character_pushed_into_the_last_column_while_a_wrap_is_pending() {
        // Inserting a character pushes the wide one into the last column, where it has no
        // room for its second half. With wrapping off, the next wide character is dropped
        // there and leaves a wrap pending on that cell. Printed again as it is, the wide
        // character wraps onto a new line and scrolls the screen.
        assert_rebuilt("\x1b[1;11H\u{4e2d}\x1b[4h\ra\x1b[?7l\x1b[1;12H\u{4e2d}".as_bytes());
    }

    #[test]
    fn a_snapshot_keeps_erased_cells_that_carry_a_background() {
        assert_rebuilt(b"\x1b[44m\x1b[2J\x1b[Htext\x1b[0m");
    }

    #[test]
    fn a_snapshot_puts_the_cursor_back_with_its_pen() {
        assert_rebuilt(b"line\r\n\x1b[3;7H\x1b[1;35m");
    }

    #[test]
    fn a_snapshot_keeps_a_wrap_that_is_pending_at_the_last_column() {
        assert_rebuilt(b"\x1b[32mabcdefghijkl");
    }

    #[test]
    fn a_snapshot_keeps_the_saved_cursor() {
        assert_rebuilt(b"\x1b[2;3H\x1b[33m\x1b7\x1b[0m\x1b[5;1H");
    }

    #[test]
    fn a_snapshot_after_the_screen_is_erased_has_the_scrollback_the_pane_has() {
        // `clear`: the pane erases the screen in place, so nothing reaches its scrollback.
        let mut term = fed(b"1\r\n2\r\n3\r\n4\r\n5\r\n6\r\n7\x1b[2J");
        let snapshot = String::from_utf8(term.snapshot()).unwrap();

        assert!(
            snapshot.starts_with("1\x1b[K\r\n2\x1b[K\r\n\x1b[K"),
            "{snapshot:?}"
        );
        assert!(!snapshot.contains('3'), "{snapshot:?}");
        assert_rebuilt(b"1\r\n2\r\n3\r\n4\r\n5\r\n6\r\n7\x1b[2J");
    }

    #[test]
    fn a_snapshot_keeps_the_scroll_region() {
        assert_rebuilt(b"\x1b[2;4r\x1b[3;5Hx");
    }

    #[test]
    fn a_snapshot_keeps_the_scroll_region_of_the_main_screen_behind_the_alternate_one() {
        assert_rebuilt(b"\x1b[2;4r\x1b[?1049h\x1b[3;5r");
    }

    #[test]
    fn a_snapshot_places_the_cursor_from_the_top_of_the_scroll_region_in_origin_mode() {
        assert_rebuilt(b"\x1b[2;4r\x1b[?6h\x1b[2;3Hx");
    }

    #[test]
    fn a_snapshot_keeps_where_restoring_the_cursor_puts_it_after_lines_scrolled_into_history() {
        // The pane saves the row in its scrollback: the row restored moves up with the lines.
        assert_rebuilt(b"\x1b[5;3H\x1b7\n\n");
    }

    #[test]
    fn a_snapshot_restores_the_modes_that_change_what_keys_and_the_mouse_send() {
        assert_rebuilt(b"\x1b[?1h\x1b=\x1b[?2004h\x1b[?1002h\x1b[?1006h\x1b[?1004h\x1b[?25l\x1b[?7l\x1b[4h\x1b[?1007l");
    }

    #[test]
    fn a_snapshot_of_the_alternate_screen_opens_it() {
        assert_rebuilt(b"main\x1b[?1049h\x1b[Halt screen");
    }

    #[test]
    fn a_snapshot_draws_a_tab_as_the_blanks_it_leaves_and_not_as_a_tab() {
        // Written as a tab, the receiving terminal jumps to its own tab stop instead, and
        // everything after it lands in the wrong column.
        let mut original = fed(b"ab\tcd");

        let snapshot = original.snapshot();

        assert!(
            !snapshot.contains(&b'\t'),
            "the snapshot carries a tab: {:?}",
            readable(&snapshot)
        );
        assert_rebuilt(b"ab\tcd");
    }

    #[test]
    fn a_snapshot_taken_inside_a_synchronized_update_carries_what_it_holds_back() {
        // The bytes of the update are already read and gone; a view opening now can only see
        // them in its snapshot.
        let mut original = fed(b"before\x1b[?2026h\x1b[H\x1b[2Jafter");

        let mut copy = fed(&original.snapshot());

        assert_eq!(copy.screen().lines[0], "after");
        assert_eq!(original.screen().lines[0], "after");
    }

    #[test]
    fn a_snapshot_puts_the_cursor_shape_back() {
        assert_rebuilt(b"\x1b[3 q");
        assert_rebuilt(b"\x1b[5 q");
    }

    #[test]
    fn a_snapshot_keeps_a_hyperlink() {
        assert_rebuilt(b"\x1b]8;;https://example.com\x1b\\link\x1b]8;;\x1b\\ after");
    }

    /// Pieces of output the snapshot must survive. Tab stops and character sets are left out:
    /// `alacritty_terminal` does not expose them, so a snapshot cannot carry them (see
    /// `snapshot`).
    fn piece() -> impl Strategy<Value = Vec<u8>> {
        prop_oneof![
            "[a-z ]{1,15}".prop_map(String::into_bytes),
            Just("\u{4e2d}".as_bytes().to_vec()),
            // Wide characters side by side, and the cursor stepped back onto a second half,
            // for the edits below to cut into.
            Just("\u{4e2d}\u{6587}".as_bytes().to_vec()),
            Just(b"\x1b[D".to_vec()),
            Just(b"\x1b[C".to_vec()),
            Just("e\u{301}".as_bytes().to_vec()),
            Just("\u{301}".as_bytes().to_vec()),
            Just(b"\r\n".to_vec()),
            Just(b"\r".to_vec()),
            Just(b"\n".to_vec()),
            Just(b"\x08".to_vec()),
            Just(b"\t".to_vec()),
            (1u8..8, 1u8..14).prop_map(|(row, col)| format!("\x1b[{row};{col}H").into_bytes()),
            prop::sample::select(vec![
                "0",
                "1",
                "2",
                "3",
                "4",
                "7",
                "8",
                "9",
                "22",
                "24",
                "27",
                "31",
                "42",
                "93",
                "104",
                "38;5;123",
                "48;2;9;8;7",
                "4:2",
                "4:3",
                "58;2;1;2;3",
                "39",
                "49",
            ])
            .prop_map(|sgr| format!("\x1b[{sgr}m").into_bytes()),
            prop::sample::select(vec![
                "\x1b[J",
                "\x1b[1J",
                "\x1b[2J",
                "\x1b[K",
                "\x1b[1K",
                "\x1b[2K",
                "\x1b[@",
                "\x1b[2@",
                "\x1b[20@",
                "\x1b[P",
                "\x1b[2P",
                "\x1b[20P",
                "\x1b[X",
                "\x1b[2X",
                "\x1b[20X",
                "\x1bD",
                "\x1bE",
                "\x0b",
                "\x1b[L",
                "\x1b[M",
                "\x1b[2L",
                "\x1b[3M",
                "\x1b[20M",
                "\x1b[s",
                "\x1b[u",
                "\x1b[I",
                "\x1b[Z",
                "\x1b[2Z",
                "\x1b[5A",
                "\x1b[5B",
                "\x1b[2E",
                "\x1b[2F",
                "\x1b[3S",
                "\x1b[3J",
                "\x1b[2;4r",
                "\x1b[3;9r",
                "\x1b[9;10r",
                "\x1b[r",
                "\x1b[?6h",
                "\x1b[?6l",
                "\x1bc",
                "\x1b[S",
                "\x1b[T",
                "\x1bM",
                "\x1b7",
                "\x1b8",
                "\x1b[?1049h",
                "\x1b[?1049l",
                "\x1b[?1h",
                "\x1b[?1l",
                "\x1b[?2004h",
                "\x1b[?25l",
                "\x1b[?25h",
                "\x1b[?7l",
                "\x1b[?7h",
                "\x1b[4h",
                "\x1b[4l",
                "\x1b[?1000h",
                "\x1b[?1003h",
                "\x1b[?1006h",
                "\x1b=",
                "\x1b>",
                "\x1b]8;;https://example.com\x1b\\",
                "\x1b]8;;\x1b\\",
            ])
            .prop_map(|seq| seq.as_bytes().to_vec()),
        ]
    }

    proptest! {
        #[test]
        fn any_output_is_redrawn_by_its_snapshot(pieces in prop::collection::vec(piece(), 0..60)) {
            assert_rebuilt(&pieces.concat());
        }

        /// The pane puts a line into its scrollback only when a line feed scrolls it off the
        /// screen, so these leave the scrollback as it was after any output (see `pane_rules`).
        #[test]
        fn erasing_the_screen_or_deleting_or_scrolling_lines_leaves_the_scrollback_alone(
            pieces in prop::collection::vec(piece(), 0..60),
            edit in prop_oneof![
                Just(&b"\x1b[2J"[..]),
                Just(&b"\x1b[3S"[..]),
                Just(&b"\x1b[H\x1b[3M"[..]),
                Just(&b"\x1b[20M"[..]),
            ],
        ) {
            let mut term = fed(&pieces.concat());
            let history = |term: &AlacrittyEngine| {
                let rows = cells(term);
                rows[..term.term.history_size()].to_vec()
            };
            let before = history(&term);
            term.advance(edit);

            prop_assert_eq!(history(&term), before);
        }
    }
}
