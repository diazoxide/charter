//! A terminal's state written back out as the escape sequences that draw it.
//!
//! This is how a pane that opens late catches up: the UI's terminal plays the snapshot, then
//! the session's output from that moment on (VS Code reconnects its terminals the same way).
//!
//! A wide character without its second half after it, such as one pushed into the last
//! column, where the terminal has no room for the second half it needs, is drawn as a blank in
//! its colours: printing it in the last column would wrap it onto the next row. That holds
//! for the cell reprinted to leave a wrap pending as well.
//!
//! The second half of a wide character is written in the character's own pen. In the original
//! it can hold another character's pen, after cells deleted beside it shifted that one in.
//! The pane never holds such a pen: xterm.js writes the second half in the pen of the write
//! that makes it, and writes it again whenever either half is overwritten.
//!
//! A wrap mark is carried only where wrapping can put it back: not on the bottom row, which
//! has nothing below it to continue onto, and not on the row the cursor waits to wrap from,
//! where printing the cell again is what leaves the wrap pending. It costs a line joined
//! differently if the pane is resized before the program draws again.
//!
//! What a snapshot cannot carry, because `alacritty_terminal` keeps it private:
//!
//! - **The main screen, while the alternate screen is open**, and the cursor saved on entering
//!   it. A snapshot taken then opens the alternate screen on a blank main screen, so a pane
//!   that opened late and then sees the program leave the alternate screen finds the scrollback
//!   underneath gone. It stays gone: leaving the alternate screen redraws nothing by itself.
//! - **The tab stops** a program moved. Output that arrives after the snapshot and relies on
//!   them lands in the wrong column until the program sets them again.
//! - **The character set** a program selected, so line-drawing output that arrives after the
//!   snapshot is drawn as the letters it is mapped from. `ncurses` re-issues the selection
//!   around every run of such output, which repairs it in practice.

use std::fmt::Write as _;

use alacritty_terminal::grid::{Cursor, Dimensions, Row};
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::{Cell, Flags};
use alacritty_terminal::term::{Term, TermMode};
use alacritty_terminal::vte::ansi::{Color, CursorShape, NamedColor};

use super::pane_rules::Beside;

/// `beside` is what the engine keeps beside `term`: the scroll region, and where restoring the
/// cursor puts it.
pub(super) fn snapshot<T>(term: &Term<T>, beside: &Beside) -> Vec<u8> {
    let grid = term.grid();
    let mode = *term.mode();
    let mut out = Out::default();

    let [main_region, alternate_region] = beside.scroll_regions(term);
    let region = if mode.contains(TermMode::ALT_SCREEN) {
        // The main screen keeps its own scroll region behind the alternate one.
        if let Some((top, bottom)) = main_region {
            let _ = write!(out.bytes, "\x1b[{top};{bottom}r");
        }
        out.text("\x1b[?1049h\x1b[H");
        alternate_region
    } else {
        main_region
    };

    let columns = grid.columns();
    let (top, bottom) = (grid.topmost_line().0, grid.bottommost_line().0);
    let mut continues = false;
    for line in top..=bottom {
        let row = &grid[Line(line)];
        let wraps = row[Column(columns - 1)].flags.contains(Flags::WRAPLINE);
        let drawn = if wraps {
            columns
        } else {
            let used = (0..columns)
                .rposition(|column| row[Column(column)] != Cell::default())
                .map_or(0, |last| last + 1);
            // A line continued from the one above must print something, or the wrap that
            // joins them never happens.
            used.max(usize::from(continues))
        };
        for column in (0..drawn).map(Column) {
            if spacer_of_a_wide_character(row, column) {
                continue;
            }
            out.drawable_cell(row, column);
        }
        if drawn < columns {
            // Whatever is left of the row is blank in the original. It is erased rather than
            // left alone, because a line that wraps at the bottom of the screen scrolls, and
            // the row scrolled in carries the background the wrap happened with.
            out.plain();
            out.text("\x1b[K");
        }
        if !wraps && line != bottom {
            out.plain();
            out.text("\r\n");
        }
        continues = wraps;
    }

    out.plain();
    let saved = &grid.saved_cursor;
    out.move_to_point(beside.saved_line(term), saved.point.column);
    out.pen(&saved.template);
    out.text("\x1b7");
    out.plain();

    // It homes the cursor, and so goes before the cursor is placed; and it changes where the
    // rows above scroll, and so goes after them.
    if let Some((top, bottom)) = region {
        let _ = write!(out.bytes, "\x1b[{top};{bottom}r");
    }

    for (flag, on, off) in [
        (TermMode::APP_CURSOR, "\x1b[?1h", ""),
        (TermMode::APP_KEYPAD, "\x1b=", ""),
        (TermMode::MOUSE_REPORT_CLICK, "\x1b[?1000h", ""),
        (TermMode::MOUSE_DRAG, "\x1b[?1002h", ""),
        (TermMode::MOUSE_MOTION, "\x1b[?1003h", ""),
        (TermMode::UTF8_MOUSE, "\x1b[?1005h", ""),
        (TermMode::SGR_MOUSE, "\x1b[?1006h", ""),
        (TermMode::FOCUS_IN_OUT, "\x1b[?1004h", ""),
        (TermMode::BRACKETED_PASTE, "\x1b[?2004h", ""),
        (TermMode::LINE_FEED_NEW_LINE, "\x1b[20h", ""),
        (TermMode::ALTERNATE_SCROLL, "", "\x1b[?1007l"),
        (TermMode::SHOW_CURSOR, "", "\x1b[?25l"),
        // Origin mode homes the cursor, so it goes before the cursor is placed.
        (TermMode::ORIGIN, "\x1b[?6h", ""),
    ] {
        out.text(if mode.contains(flag) { on } else { off });
    }
    // In origin mode, the cursor is placed from the top of the scroll region.
    if let Some((top, _)) = region.filter(|_| mode.contains(TermMode::ORIGIN)) {
        out.origin = top - 1;
    }

    // The shape a program chose for the cursor, which a pane would otherwise draw as a block.
    if let Some(shape) = cursor_shape(term) {
        let _ = write!(out.bytes, "\x1b[{shape} q");
    }

    let cursor = &grid.cursor;
    if cursor.input_needs_wrap {
        // Only printing into the last column leaves a wrap pending, so the cell there is
        // printed again, as it is.
        let row = &grid[cursor.point.line];
        let mut column = cursor.point.column;
        if spacer_of_a_wide_character(row, column) {
            column -= 1;
        }
        out.move_to_point(cursor.point.line, column);
        out.drawable_cell(row, column);
    } else {
        out.move_to(cursor);
    }

    // Both change how printing lands, so they go after everything this snapshot prints.
    if !mode.contains(TermMode::LINE_WRAP) {
        out.text("\x1b[?7l");
    }
    if mode.contains(TermMode::INSERT) {
        out.text("\x1b[4h");
    }
    out.pen(&cursor.template);
    out.bytes.into_bytes()
}

/// The `DECSCUSR` number for the cursor's shape, or `None` where nothing draws a cursor.
fn cursor_shape<T>(term: &Term<T>) -> Option<u8> {
    let style = term.cursor_style();
    let steady = |blinking: u8| blinking + u8::from(!style.blinking);
    match style.shape {
        CursorShape::Block => Some(steady(1)),
        CursorShape::Underline => Some(steady(3)),
        CursorShape::Beam => Some(steady(5)),
        // Neither has a number of its own; a hidden cursor is `?25l`, which the modes carry,
        // and a hollow block is what a terminal draws for an unfocused window.
        CursorShape::HollowBlock | CursorShape::Hidden => None,
    }
}

/// The bytes written so far, and the pen and hyperlink they leave the terminal with.
#[derive(Default)]
struct Out {
    bytes: String,
    /// The row the cursor is placed from.
    origin: i32,
    sgr: String,
    link: Option<(String, String)>,
}

impl Out {
    fn text(&mut self, text: &str) {
        self.bytes.push_str(text);
    }

    fn cell(&mut self, cell: &Cell) {
        self.pen(cell);
        // A cell can hold a tab, which the program put there and which draws nothing. Written
        // out, it would move the cursor to the next tab stop instead, and everything after it
        // would land in the wrong column.
        self.bytes
            .push(if cell.c.is_control() { ' ' } else { cell.c });
        self.bytes.extend(cell.zerowidth().unwrap_or_default());
    }

    /// Switches to `cell`'s colours, attributes and hyperlink, writing only what changed.
    fn pen(&mut self, cell: &Cell) {
        let sgr = sgr(cell);
        if sgr != self.sgr {
            let _ = write!(self.bytes, "\x1b[0{sgr}m");
            self.sgr = sgr;
        }
        let link = cell
            .hyperlink()
            .map(|link| (link.id().to_owned(), link.uri().to_owned()));
        if link != self.link {
            match &link {
                Some((id, uri)) => {
                    let _ = write!(self.bytes, "\x1b]8;id={id};{uri}\x1b\\");
                }
                None => self.bytes.push_str("\x1b]8;;\x1b\\"),
            }
            self.link = link;
        }
    }

    /// The cell in `column` of `row`, as it can be drawn there.
    ///
    /// A wide character is drawn as the blank its colours surround when the cell after it is
    /// not its second half: in the last column, pushed there by cells being inserted, printing
    /// it would wrap it onto the next row, or scroll the screen from the bottom row. The engine
    /// blanks both as the pane does (see `pane_rules`), so this is a fallback.
    fn drawable_cell(&mut self, row: &Row<Cell>, column: Column) {
        let cell = &row[column];
        let whole = !cell.flags.contains(Flags::WIDE_CHAR)
            || (column.0 + 1 < row.len()
                && row[column + 1].flags.contains(Flags::WIDE_CHAR_SPACER));
        if whole {
            self.cell(cell);
        } else {
            self.blank(cell);
        }
    }

    /// `cell`'s colours and attributes, without its character.
    fn blank(&mut self, cell: &Cell) {
        self.pen(cell);
        self.bytes.push(' ');
    }

    /// Back to the default pen, so that lines the terminal scrolls in stay blank.
    fn plain(&mut self) {
        self.pen(&Cell::default());
    }

    fn move_to(&mut self, cursor: &Cursor<Cell>) {
        self.move_to_point(cursor.point.line, cursor.point.column);
    }

    fn move_to_point(&mut self, line: Line, column: Column) {
        let _ = write!(
            self.bytes,
            "\x1b[{};{}H",
            line.0 - self.origin + 1,
            column.0 + 1
        );
    }
}

/// Whether this cell is the second half of a wide character next to it, which the terminal
/// writes for itself when it prints that character.
///
/// The other two blanks a wide character can leave behind — the one at the end of a row too
/// narrow for it, and one whose character was later overwritten — are printed as the blanks
/// they are, which keeps the rest of the row in place. Only their markers are lost, and
/// nothing draws them.
fn spacer_of_a_wide_character(row: &Row<Cell>, column: Column) -> bool {
    row[column].flags.contains(Flags::WIDE_CHAR_SPACER)
        && column.0 > 0
        && row[column - 1].flags.contains(Flags::WIDE_CHAR)
}

/// The SGR parameters, each preceded by `;`, that give a cell its colours and attributes.
fn sgr(cell: &Cell) -> String {
    let mut sgr = String::new();
    let flags = cell.flags;
    for (flag, code) in [
        (Flags::BOLD, "1"),
        (Flags::DIM, "2"),
        (Flags::ITALIC, "3"),
        (Flags::UNDERLINE, "4"),
        (Flags::DOUBLE_UNDERLINE, "4:2"),
        (Flags::UNDERCURL, "4:3"),
        (Flags::DOTTED_UNDERLINE, "4:4"),
        (Flags::DASHED_UNDERLINE, "4:5"),
        (Flags::INVERSE, "7"),
        (Flags::HIDDEN, "8"),
        (Flags::STRIKEOUT, "9"),
    ] {
        if flags.contains(flag) {
            sgr.push(';');
            sgr.push_str(code);
        }
    }
    color(&mut sgr, cell.fg, "3", "9", "38");
    color(&mut sgr, cell.bg, "4", "10", "48");
    if let Some(underline) = cell.underline_color() {
        color(&mut sgr, underline, "", "", "58");
    }
    sgr
}

/// Appends `color` as SGR parameters: `normal`/`bright` prefix the eight basic colours, and
/// `extended` introduces indexed and RGB colours. The default colour appends nothing.
fn color(sgr: &mut String, color: Color, normal: &str, bright: &str, extended: &str) {
    let _ = match color {
        Color::Named(name) => match basic(name) {
            Some(index) if index < 8 && !normal.is_empty() => write!(sgr, ";{normal}{index}"),
            Some(index) if !bright.is_empty() => write!(sgr, ";{bright}{}", index - 8),
            Some(index) => write!(sgr, ";{extended};5;{index}"),
            None => Ok(()),
        },
        Color::Indexed(index) => write!(sgr, ";{extended};5;{index}"),
        Color::Spec(rgb) => write!(sgr, ";{extended};2;{};{};{}", rgb.r, rgb.g, rgb.b),
    };
}

/// The palette index of one of the sixteen basic colours; `None` for the default colours. A
/// dim colour is a flag on the cell, never a colour of its own, so there is none here.
fn basic(name: NamedColor) -> Option<usize> {
    let index = name as usize;
    (index < 16).then_some(index)
}
