//! Where `alacritty_terminal` and the pane disagree on what output does to the screen, the
//! engine does what the pane does.
//!
//! The pane is xterm.js. A pane attached live draws the program's output itself, and one that
//! opens late draws this engine's snapshot, so any output the two emulators apply differently
//! leaves two panes of one session showing different things until the program redraws (#441).
//!
//! **Wide characters cut in half.** An edit can cut a wide character in half: deleting
//! (`DCH`), inserting (`ICH`), erasing (`ECH`, `EL`, `ED`) or writing that starts on its second
//! half, or that ends on its first. xterm.js blanks the half that is left; so does xterm (`DamagedCells` in
//! `util.c`), and so does VTE (`cleanup_fragments`). `alacritty_terminal` keeps the character
//! whole when its second half is cut, and keeps the old colours on a second half left behind.
//! xterm.js blanks in the erase colour for an edit and in the pen for a write, and so does
//! this engine; VTE keeps the old colours, which no pane shows.
//!
//! **A pending wrap.** After the last column is written, the next character wraps. Deleting,
//! inserting or erasing characters, and moving down or up a row (`LF`, `IND`, `RI`), end that
//! in xterm, VTE and xterm.js, and the next character lands in the last column;
//! `alacritty_terminal` keeps it. A tab (`HT`) leaves the cursor where it is with the wrap
//! still pending in all three (xterm's `TabToNextStop` in `tabs.c` stops at the last column;
//! VTE's `move_cursor_tab_forward` returns early), where `alacritty_terminal` wraps.
//!
//! **Where xterm.js stands alone.** This engine follows it there, because that is what the
//! pane shows:
//!
//! - Erasing below (`ED 0`) erases nothing of the cursor's row in xterm.js, which has the
//!   cursor past the row, where xterm and VTE step back onto the last column and erase it.
//! - Restoring the cursor (`DECRC`, `SCORC`, and leaving the alternate screen) puts it back on
//!   the last column with no wrap pending; xterm and VTE restore the wrap. xterm.js also saves
//!   the cursor opening the alternate screen when it is open already, and restores it closing
//!   the alternate screen when it is not open; `alacritty_terminal` does neither.
//! - Inserting or deleting lines (`IL`, `DL`) with the cursor outside the scroll region does
//!   nothing in xterm and VTE, which leave the wrap pending; xterm.js ends it.
//! - In origin mode, xterm.js places the cursor from the top of the scroll region again after
//!   every relative move, so moving it by any amount moves it down by as many rows as the
//!   region starts below the top of the screen. That is a defect in xterm.js, copied here
//!   because it moves where the next character lands. `alacritty_terminal` has the same
//!   defect moving up or down; this engine adds it moving left or right.
//!
//! A backward tab (`CBT`) while a wrap is pending does nothing in xterm.js;
//! `alacritty_terminal` moved the cursor back and still wrapped the next character.
//!
//! **Lines and the scroll region.** Inserting or deleting lines moves the cursor to the first
//! column when it is inside the scroll region (xterm's `InsertLine` in `util.c`, VTE, and
//! xterm.js). Moving the cursor up or down (`CUU`, `CUD`, `CNL`, `CPL`) stops at the top of
//! the region when it starts at or below the top, and at the bottom when it starts at or above
//! the bottom (xterm's `CursorUp` and `CursorDown` in `cursor.c`, VTE's `move_cursor_up` and
//! `move_cursor_down`, and xterm.js). `alacritty_terminal` does neither. All three home the
//! cursor turning origin mode off as well as on (xterm's `srm_DECOM`, VTE's `eDEC_ORIGIN`);
//! `alacritty_terminal` homes it only turning it on. Restoring the cursor in origin mode keeps
//! it inside the region in xterm.js.
//!
//! `alacritty_terminal` keeps its scroll region private, so the engine keeps a copy in
//! [`Beside`], which a snapshot carries. xterm.js keeps a region for each screen and opens the
//! alternate screen with the whole screen, where `alacritty_terminal` keeps one for both. A
//! region whose bottom is past the screen ends at the bottom of the screen in xterm.js, which
//! ignores it when that leaves no rows; `alacritty_terminal` would keep a region with none.
//!
//! **What reaches the scrollback.** xterm.js puts a line into its scrollback only when a line
//! feed scrolls it off the top of the screen. Deleting lines (`DL`) or scrolling up (`SU`)
//! from the top of the screen drops the lines taken off, and erasing the screen (`ED 2`, what
//! `clear` sends) erases it in place, since the pane leaves `scrollOnEraseInDisplay` off.
//! `alacritty_terminal` moves the lines into its history in all three, and a pane that opened
//! late would get them in its scrollback. The engine makes those edits itself, without the
//! history, so its history is the pane's scrollback.
//!
//! **Restoring the cursor after the screen scrolled.** xterm.js saves the cursor's row in its
//! scrollback, not on the screen, so it comes back to a row that has moved up with every line
//! scrolled into the scrollback since, until the scrollback is full. xterm and VTE save the row
//! on the screen, and so does `alacritty_terminal`. The engine counts the lines scrolled into
//! its history since the cursor was saved, and moves the cursor up by as many as it restores
//! it.
//!
//! **A combining mark with nothing before it.** At the start of a row, xterm.js gives a
//! zero-width character, such as a combining mark, a cell of its own that it draws nothing in,
//! and moves the cursor past it; `alacritty_terminal` puts it on the cell under the cursor.
//! The engine writes a blank.
//!
//! **Two plain cases.** `alacritty_terminal` gets two edits wrong that every terminal above
//! gets right: deleting more characters than the row has left of the cursor also blanks what
//! is before it, and erasing above the cursor from the second row leaves the first.
//!
//! **Not covered.**
//!
//! - With wrapping off (`?7l`), xterm.js writes a character over the second half of a wide
//!   character in the last column and leaves the character standing, a state an
//!   `alacritty_terminal` grid cannot hold.
//! - xterm.js joins a zero-width character to the character before it only when nothing but
//!   printing came between them: after any control or escape sequence, it too takes a cell of
//!   its own. This engine joins it to the cell before the cursor wherever that is not the start
//!   of a row, as `alacritty_terminal` does.
//! - A resize moves the row xterm.js restores the cursor to in ways this engine does not
//!   follow.
//! - An escape sequence broken off by a character that is not ASCII is parsed differently by
//!   vte, below these rules, and by xterm.js.
//!
//! [`PaneRules`] sits between the parser and the terminal: it hands every call through, and
//! repairs the cells around those.

use alacritty_terminal::event::EventListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::{Cell, Flags};
use alacritty_terminal::term::{Term, TermMode};
use alacritty_terminal::vte::ansi::cursor_icon::CursorIcon;
use alacritty_terminal::vte::ansi::{
    Attr, CharsetIndex, ClearMode, CursorShape, CursorStyle, Handler, Hyperlink, KeyboardModes,
    KeyboardModesApplyBehavior, LineClearMode, Mode, ModifyOtherKeys, NamedPrivateMode,
    PrivateMode, Rgb, ScpCharPath, ScpUpdateMode, StandardCharset, TabulationClearMode,
};
use unicode_width::UnicodeWidthChar;

use std::ops::Range;

/// A terminal that edits as the pane does. Output is parsed into this, never into the
/// [`Term`] itself, along with what the engine keeps beside the terminal.
pub(super) struct PaneRules<'a, T>(pub(super) &'a mut Term<T>, pub(super) &'a mut Beside);

/// What these rules need to know that `alacritty_terminal` keeps private, or does not keep.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Beside {
    /// The scroll region of the main screen and of the alternate screen. xterm.js keeps one
    /// for each, and the alternate screen opens with the whole screen; `alacritty_terminal`
    /// keeps one for both, and is given the other one whenever the screens change.
    scroll_regions: [ScrollRegion; 2],
    /// How many lines the pane had in its scrollback when the cursor was saved, on the main
    /// screen and on the alternate screen.
    saved_at: [usize; 2],
}

impl Beside {
    /// What a terminal of `rows` rows starts with.
    pub(super) fn new(rows: usize) -> Self {
        Self {
            scroll_regions: [ScrollRegion::whole(rows), ScrollRegion::whole(rows)],
            saved_at: [0; 2],
        }
    }

    /// The terminal was resized to `rows` rows. `alacritty_terminal` resets its scroll region
    /// then, as long as the size changed.
    pub(super) fn resized(&mut self, rows: usize) {
        self.scroll_regions = [ScrollRegion::whole(rows), ScrollRegion::whole(rows)];
    }

    /// The scroll regions of the main screen and of the alternate screen, as the rows `DECSTBM`
    /// takes, counted from 1, or `None` where the region is the whole screen of `term`.
    pub(super) fn scroll_regions<T>(&self, term: &Term<T>) -> [Option<(i32, i32)>; 2] {
        let whole = ScrollRegion::whole(term.screen_lines());
        self.scroll_regions.clone().map(|region| {
            let Range { start, end } = region.0;
            (region != whole).then_some((start + 1, end))
        })
    }

    /// The row restoring the cursor puts it on in `term`.
    ///
    /// xterm.js saves the row in its scrollback, so the row the cursor comes back to has moved
    /// up with every line scrolled into it since, until the scrollback is full. xterm and VTE
    /// save the row on the screen.
    pub(super) fn saved_line<T>(&self, term: &Term<T>) -> Line {
        let screen = screen(term);
        let scrolled = term.history_size() as i64 - self.saved_at[screen] as i64;
        let last = term.screen_lines() as i64 - 1;
        let saved = i64::from(term.grid().saved_cursor.point.line.0);
        Line((saved - scrolled).clamp(0, last) as i32)
    }
}

/// The rows of a scroll region, from its top row up to, not including, the row below its
/// bottom, counted from 0.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ScrollRegion(Range<i32>);

impl ScrollRegion {
    /// A screen of `rows` rows scrolled whole, as a terminal starts, and as it is after a reset
    /// or a change of size.
    fn whole(rows: usize) -> Self {
        Self(0..rows as i32)
    }

    /// Rows `top` to `bottom`, counted from 1, with `bottom` already on the screen of `rows`
    /// rows and below `top`.
    fn set(&mut self, top: usize, bottom: usize) {
        self.0 = top as i32 - 1..bottom as i32;
    }

    fn top(&self) -> Line {
        Line(self.0.start)
    }

    fn contains(&self, line: Line) -> bool {
        self.0.contains(&line.0)
    }

    /// `line`, moved onto the nearest row of the region.
    fn clamp(&self, line: i32) -> Line {
        Line(line.clamp(self.0.start, self.0.end - 1))
    }

    /// How many of `rows` rows up the cursor on `line` moves: it stops at the top of the
    /// region when it starts at or below it.
    fn rows_up(&self, line: Line, rows: usize) -> usize {
        match usize::try_from(line.0 - self.0.start) {
            Ok(room) => rows.min(room),
            Err(_) => rows,
        }
    }

    /// How many of `rows` rows down the cursor on `line` moves: it stops at the bottom of the
    /// region when it starts at or above it.
    fn rows_down(&self, line: Line, rows: usize) -> usize {
        match usize::try_from(self.0.end - 1 - line.0) {
            Ok(room) => rows.min(room),
            Err(_) => rows,
        }
    }
}

/// The main screen and the alternate screen, as indices into what is kept per screen.
const MAIN: usize = 0;
const ALTERNATE: usize = 1;

/// Which screen `term` is showing: [`MAIN`] or [`ALTERNATE`].
fn screen<T>(term: &Term<T>) -> usize {
    if term.mode().contains(TermMode::ALT_SCREEN) {
        ALTERNATE
    } else {
        MAIN
    }
}

/// The mode that opens the alternate screen, saving the cursor, and closes it, restoring it.
const ALTERNATE_SCREEN: PrivateMode =
    PrivateMode::Named(NamedPrivateMode::SwapScreenAndSetRestoreCursor);

impl<T> PaneRules<'_, T> {
    /// Which screen is showing: [`MAIN`] or [`ALTERNATE`].
    fn screen(&self) -> usize {
        screen(self.0)
    }

    /// The scroll region of the screen showing.
    fn region(&self) -> &ScrollRegion {
        &self.1.scroll_regions[self.screen()]
    }

    fn cursor(&self) -> (Line, usize) {
        let point = self.0.grid().cursor.point;
        (point.line, point.column.0)
    }

    fn is_wide(&self, line: Line, column: usize) -> bool {
        self.0.grid()[line][Column(column)]
            .flags
            .contains(Flags::WIDE_CHAR)
    }

    /// Whether the cell at `column` is the second half of the wide character before it.
    fn is_second_half(&self, line: Line, column: usize) -> bool {
        column > 0 && self.is_wide(line, column - 1)
    }

    /// Whether the cell at `column` is a second half whose character is gone.
    fn is_left_behind(&self, line: Line, column: usize) -> bool {
        self.0.grid()[line][Column(column)]
            .flags
            .contains(Flags::WIDE_CHAR_SPACER)
            && !self.is_second_half(line, column)
    }

    /// After the cursor moved left or right: in origin mode, xterm.js places the cursor from
    /// the top of the scroll region again after every relative move, which moves it down by as
    /// many rows as the region starts below the top of the screen. `alacritty_terminal` does
    /// the same moving up or down, and not moving left or right.
    fn moved_across(&mut self) {
        if self.0.mode().contains(TermMode::ORIGIN) {
            let region = self.region();
            let line = region.clamp(self.cursor().0.0 + region.0.start);
            self.0.grid_mut().cursor.point.line = line;
        }
    }

    /// Moves the rows from `top` to the bottom of the scroll region up by `rows`, dropping the
    /// ones moved off `top` and blanking the ones left at the bottom, as xterm.js deletes lines
    /// (`DL`) and scrolls up (`SU`). `alacritty_terminal` moves the rows it drops into its
    /// history when `top` is the top of the screen; xterm.js puts only lines that a line feed
    /// scrolls off the screen into its scrollback. Unlike `alacritty_terminal`, this keeps no
    /// damage, selection or vi-mode cursor up to date: nothing in the engine reads them.
    fn shift_up(&mut self, top: Line, rows: usize) {
        // The row below the bottom of the region.
        let bottom = self.region().0.end;
        let rows = rows.min((bottom - top.0) as usize) as i32;
        let grid = self.0.grid_mut();
        for line in top.0..bottom - rows {
            grid[Line(line)] = grid[Line(line + rows)].clone();
        }
        let template = grid.cursor.template.clone();
        for line in bottom - rows..bottom {
            grid[Line(line)].reset(&template);
        }
    }

    /// Leaves the cursor on the last column with no wrap pending, as xterm, VTE and xterm.js
    /// do before deleting, inserting or erasing characters.
    fn end_pending_wrap(&mut self) {
        self.0.grid_mut().cursor.input_needs_wrap = false;
    }

    /// Blanks one cell in the erase colour, as xterm.js blanks what is left of a wide
    /// character.
    fn blank(&mut self, line: Line, column: usize) {
        let colour = self.0.grid().cursor.template.bg;
        self.0.grid_mut()[line][Column(column)] = Cell::from(colour);
    }

    /// Blanks one cell in the pen, as xterm.js blanks what writing leaves of a wide character.
    fn blank_in_pen(&mut self, line: Line, column: usize) {
        let mut cell = self.0.grid().cursor.template.clone();
        cell.c = ' ';
        cell.flags
            .remove(Flags::WIDE_CHAR | Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER);
        self.0.grid_mut()[line][Column(column)] = cell;
    }

    /// Erases cells from `start` up to, not including, `end` with `apply`, blanking a wide
    /// character that `start` cuts into and the second half of one that `end` cuts.
    fn erase(&mut self, start: usize, end: usize, apply: impl FnOnce(&mut Term<T>)) {
        let (line, _) = self.cursor();
        let cut_on_the_left = self.is_second_half(line, start);
        let cut_on_the_right = end < self.0.columns() && self.is_second_half(line, end);
        apply(self.0);
        if cut_on_the_left {
            self.blank(line, start - 1);
        }
        if cut_on_the_right {
            self.blank(line, end);
        }
    }
}

impl<T: EventListener> PaneRules<'_, T> {
    /// Gives `alacritty_terminal` the scroll region of the screen now showing, leaving the
    /// cursor where it is.
    fn use_region(&mut self) {
        let cursor = self.0.grid().cursor.clone();
        let Range { start, end } = self.region().0.clone();
        self.0
            .set_scrolling_region(start as usize + 1, Some(end as usize));
        self.0.grid_mut().cursor = cursor;
    }

    /// Leaves the cursor in the first column after lines were inserted or deleted, as xterm,
    /// VTE and xterm.js do when the cursor is inside the scroll region; outside it, the lines
    /// are left alone and so is the cursor.
    fn after_lines_moved(&mut self) {
        if self.region().contains(self.cursor().0) {
            self.0.carriage_return();
        }
    }

    /// Writes one character as xterm.js does. `alacritty_terminal` gets four things about a
    /// wide character wrong here:
    ///
    /// - Writing onto a second half turns its character into a blank of the old colours,
    ///   where xterm.js blanks it in the pen; writing over a first half leaves its second
    ///   half in the old colours too.
    /// - It clears the cell before a second half it writes over, taking that for its first
    ///   half, even when the second half is one whose character is gone, and the cell before
    ///   holds the character being written.
    /// - In insert mode it shifts the row by rotating it: the cells pushed off the end come
    ///   back in at the cursor, and a wide mark coming back that way sets off the clearing
    ///   above.
    /// - In insert mode a character that wraps is written over the next row, not inserted.
    ///
    /// So the wrap is done here, before the write, where the row and column the character
    /// lands on are known, and the marks that would mislead the write are taken off first.
    fn write(&mut self, c: char) {
        let Some(width) = c.width() else {
            return self.0.input(c);
        };
        if width == 0 && self.cursor().1 == 0 && !self.0.grid().cursor.input_needs_wrap {
            return self.write_alone();
        }
        let insert = self.0.mode().contains(TermMode::INSERT);
        if width > 0 {
            self.wrap_before(width);
        }
        let (line, column) = self.cursor();
        let columns = self.0.columns();
        // With a wrap still pending, wrapping is off: the character goes over the last column
        // or nowhere, and xterm.js has the cursor past the row.
        let wrap_pending = self.0.grid().cursor.input_needs_wrap;
        if !wrap_pending && self.is_second_half(line, column) {
            self.blank_in_pen(line, column - 1);
        }
        let end = (column + width).min(columns);
        if insert && column + width < columns {
            self.unmark(line, columns - width..columns);
        } else if !wrap_pending {
            self.unmark(line, column..end);
        }

        self.0.input(c);

        if self.0.grid().cursor.input_needs_wrap {
            // With wrapping off, a wide character with no room is dropped, and xterm.js
            // leaves the cursor on the last column, where `alacritty_terminal` has it waiting
            // to wrap.
            let dropped = width == 2 && (wrap_pending || column + 1 >= columns);
            if dropped && !self.0.mode().contains(TermMode::LINE_WRAP) {
                self.end_pending_wrap();
            }
            return;
        }
        let (line, next) = self.cursor();
        if self.is_left_behind(line, next) {
            self.blank_in_pen(line, next);
        }
        // A wide character pushed into the last column has no room for its second half.
        if insert && self.is_wide(line, columns - 1) {
            self.blank_in_pen(line, columns - 1);
        }
    }

    /// Writes a zero-width character, such as a combining mark, that has no character before
    /// it on the row. xterm.js gives it a cell of its own, even in insert mode, and moves the
    /// cursor past it; `alacritty_terminal` puts it on the cell under the cursor and stays.
    /// That cell has no width, and xterm.js's DOM renderer skips such a cell, colours and all,
    /// so what shows is a blank in the default colours.
    fn write_alone(&mut self) {
        let (line, column) = self.cursor();
        let cut = self.is_wide(line, column);
        self.0.grid_mut()[line][Column(column)] = Cell::default();
        if cut {
            self.blank_in_pen(line, column + 1);
        }
        self.0.grid_mut().cursor.point.column = Column(column + 1);
    }

    /// Moves to the next row before a character `width` cells wide is written, when it
    /// would wrap: as `alacritty_terminal` itself would, but before the write rather than in
    /// the middle of it.
    fn wrap_before(&mut self, width: usize) {
        if !self.0.mode().contains(TermMode::LINE_WRAP) {
            return;
        }
        let (line, column) = self.cursor();
        let last = self.0.columns() - 1;
        if !self.0.grid().cursor.input_needs_wrap {
            if width < 2 || column < last {
                return;
            }
            // A wide character with no room left on the row: the last column is left blank,
            // marked as the place a wide character did not fit.
            if self.is_second_half(line, column) {
                self.blank_in_pen(line, column - 1);
            }
            self.blank_in_pen(line, column);
            self.0.grid_mut()[line][Column(column)]
                .flags
                .insert(Flags::LEADING_WIDE_CHAR_SPACER);
        }
        self.0
            .grid_mut()
            .cursor_cell()
            .flags
            .insert(Flags::WRAPLINE);
        self.0.linefeed();
        self.0.carriage_return();
    }

    /// Takes the wide marks off cells about to be written over or pushed off the row.
    fn unmark(&mut self, line: Line, columns: std::ops::Range<usize>) {
        for column in columns {
            self.0.grid_mut()[line][Column(column)]
                .flags
                .remove(Flags::WIDE_CHAR | Flags::WIDE_CHAR_SPACER);
        }
    }
}

/// Every call goes through to the terminal. `Handler`'s methods all have bodies that do
/// nothing, so a call left out here would be dropped without a word.
impl<T: EventListener> Handler for PaneRules<'_, T> {
    fn delete_chars(&mut self, count: usize) {
        self.end_pending_wrap();
        let (line, column) = self.cursor();
        let count = count.min(self.0.columns() - column);
        let cut = self.is_second_half(line, column);
        self.0.delete_chars(count);
        if cut {
            self.blank(line, column - 1);
        }
        // What shifted under the cursor may be a second half whose character was deleted.
        if self.is_left_behind(line, column) {
            self.blank(line, column);
        }
    }

    fn insert_blank(&mut self, count: usize) {
        self.end_pending_wrap();
        let (line, column) = self.cursor();
        if self.is_second_half(line, column) {
            self.blank(line, column - 1);
        }
        self.0.insert_blank(count);
        // A wide character pushed into the last column has no room left for its second half.
        let last = self.0.columns() - 1;
        if self.is_wide(line, last) {
            self.blank(line, last);
        }
    }

    fn erase_chars(&mut self, count: usize) {
        self.end_pending_wrap();
        let (_, column) = self.cursor();
        let end = column.saturating_add(count).min(self.0.columns());
        self.erase(column, end, |term| term.erase_chars(count));
    }

    fn clear_line(&mut self, mode: LineClearMode) {
        let (_, column) = self.cursor();
        let columns = self.0.columns();
        match mode {
            // A wrap pending leaves the cursor past the row in xterm.js, with nothing of the
            // row to its right.
            LineClearMode::Right if self.0.grid().cursor.input_needs_wrap => {
                self.0.clear_line(mode)
            }
            LineClearMode::Right => self.erase(column, columns, |term| term.clear_line(mode)),
            LineClearMode::Left => self.erase(0, column + 1, |term| term.clear_line(mode)),
            LineClearMode::All => self.0.clear_line(mode),
        }
    }

    fn clear_screen(&mut self, mode: ClearMode) {
        let (line, column) = self.cursor();
        let columns = self.0.columns();
        match mode {
            // A wrap pending leaves the cursor past the row in xterm.js, so nothing of the row
            // is erased. xterm and VTE step back onto the last column and erase it; the pane
            // is xterm.js.
            ClearMode::Below if self.0.grid().cursor.input_needs_wrap => {
                let last = Column(columns - 1);
                let kept = self.0.grid()[line][last].clone();
                self.0.clear_screen(mode);
                self.0.grid_mut()[line][last] = kept;
            }
            ClearMode::Below => self.erase(column, columns, |term| term.clear_screen(mode)),
            ClearMode::Above => {
                self.erase(0, column + 1, |term| term.clear_screen(mode));
                if line == Line(1) {
                    self.0.grid_mut().reset_region(..line);
                }
            }
            // xterm.js erases the screen in place. `alacritty_terminal` erases the alternate
            // screen so, and scrolls what is on the main one into its history, which would
            // give a pane that opens late a scrollback the live pane never had.
            ClearMode::All => self.0.grid_mut().reset_region(..),
            ClearMode::Saved => self.0.clear_screen(mode),
        }
    }

    fn set_title(&mut self, title: Option<String>) {
        self.0.set_title(title)
    }

    fn set_cursor_style(&mut self, style: Option<CursorStyle>) {
        self.0.set_cursor_style(style)
    }

    fn set_cursor_shape(&mut self, shape: CursorShape) {
        self.0.set_cursor_shape(shape)
    }

    fn input(&mut self, c: char) {
        self.write(c)
    }

    fn goto(&mut self, line: i32, column: usize) {
        self.0.goto(line, column)
    }

    fn goto_line(&mut self, line: i32) {
        self.0.goto_line(line)
    }

    fn goto_col(&mut self, column: usize) {
        self.0.goto_col(column)
    }

    // Moving up or down a number of rows stops at the edge of the scroll region in xterm,
    // VTE and xterm.js; `alacritty_terminal` goes on to the edge of the screen.

    fn move_up(&mut self, rows: usize) {
        let rows = self.region().rows_up(self.cursor().0, rows);
        self.0.move_up(rows)
    }

    fn move_down(&mut self, rows: usize) {
        let rows = self.region().rows_down(self.cursor().0, rows);
        self.0.move_down(rows)
    }

    fn identify_terminal(&mut self, intermediate: Option<char>) {
        self.0.identify_terminal(intermediate)
    }

    fn device_status(&mut self, status: usize) {
        self.0.device_status(status)
    }

    fn move_forward(&mut self, columns: usize) {
        self.0.move_forward(columns);
        self.moved_across();
    }

    fn move_backward(&mut self, columns: usize) {
        self.0.move_backward(columns);
        self.moved_across();
    }

    fn move_down_and_cr(&mut self, rows: usize) {
        let rows = self.region().rows_down(self.cursor().0, rows);
        self.0.move_down_and_cr(rows)
    }

    fn move_up_and_cr(&mut self, rows: usize) {
        let rows = self.region().rows_up(self.cursor().0, rows);
        self.0.move_up_and_cr(rows)
    }

    fn put_tab(&mut self, count: u16) {
        // xterm, VTE and xterm.js leave the cursor where it is, with the wrap still pending;
        // `alacritty_terminal` wraps.
        if self.0.grid().cursor.input_needs_wrap {
            return;
        }
        self.0.put_tab(count)
    }

    fn backspace(&mut self) {
        self.0.backspace()
    }

    fn carriage_return(&mut self) {
        self.0.carriage_return()
    }

    fn linefeed(&mut self) {
        self.end_pending_wrap();
        self.0.linefeed()
    }

    fn bell(&mut self) {
        self.0.bell()
    }

    fn substitute(&mut self) {
        self.0.substitute()
    }

    fn newline(&mut self) {
        self.0.newline()
    }

    fn set_horizontal_tabstop(&mut self) {
        self.0.set_horizontal_tabstop()
    }

    fn scroll_up(&mut self, rows: usize) {
        self.shift_up(self.region().top(), rows)
    }

    fn scroll_down(&mut self, rows: usize) {
        self.0.scroll_down(rows)
    }

    fn insert_blank_lines(&mut self, rows: usize) {
        self.end_pending_wrap();
        self.0.insert_blank_lines(rows);
        self.after_lines_moved();
    }

    fn delete_lines(&mut self, rows: usize) {
        self.end_pending_wrap();
        let (line, _) = self.cursor();
        if self.region().contains(line) {
            self.shift_up(line, rows);
        }
        self.after_lines_moved();
    }

    fn move_backward_tabs(&mut self, count: u16) {
        // xterm.js leaves the cursor where it is, with the wrap still pending;
        // `alacritty_terminal` moved it back and kept the wrap pending from the new column.
        if self.0.grid().cursor.input_needs_wrap {
            return;
        }
        self.0.move_backward_tabs(count)
    }

    fn move_forward_tabs(&mut self, count: u16) {
        self.0.move_forward_tabs(count)
    }

    fn save_cursor_position(&mut self) {
        let screen = self.screen();
        self.1.saved_at[screen] = self.0.history_size();
        self.0.save_cursor_position()
    }

    fn restore_cursor_position(&mut self) {
        self.0.restore_cursor_position();
        // xterm.js puts the cursor back on the last column when the wrap was pending as it was
        // saved. xterm and VTE restore the wrap; the pane is xterm.js.
        self.end_pending_wrap();
        let mut line = self.1.saved_line(self.0);
        // In origin mode, xterm.js keeps the cursor inside the scroll region.
        if self.0.mode().contains(TermMode::ORIGIN) {
            line = self.region().clamp(line.0);
        }
        self.0.grid_mut().cursor.point.line = line;
    }

    fn clear_tabs(&mut self, mode: TabulationClearMode) {
        self.0.clear_tabs(mode)
    }

    fn set_tabs(&mut self, interval: u16) {
        self.0.set_tabs(interval)
    }

    fn reset_state(&mut self) {
        self.0.reset_state();
        *self.1 = Beside::new(self.0.screen_lines());
    }

    fn reverse_index(&mut self) {
        self.end_pending_wrap();
        self.0.reverse_index()
    }

    fn terminal_attribute(&mut self, attr: Attr) {
        self.0.terminal_attribute(attr)
    }

    fn set_mode(&mut self, mode: Mode) {
        self.0.set_mode(mode)
    }

    fn unset_mode(&mut self, mode: Mode) {
        self.0.unset_mode(mode)
    }

    fn report_mode(&mut self, mode: Mode) {
        self.0.report_mode(mode)
    }

    fn set_private_mode(&mut self, mode: PrivateMode) {
        // Opening the alternate screen saves the cursor first in xterm.js, even when it is open
        // already. `alacritty_terminal` saves it only opening it.
        let opening = mode == ALTERNATE_SCREEN && self.screen() == MAIN;
        if opening {
            self.1.saved_at[MAIN] = self.0.history_size();
        } else if mode == ALTERNATE_SCREEN {
            self.save_cursor_position();
        }
        self.0.set_private_mode(mode);
        if opening {
            // It opens with a scroll region of the whole screen, whatever it had before.
            self.1.scroll_regions[ALTERNATE] = ScrollRegion::whole(self.0.screen_lines());
            self.use_region();
        }
    }

    fn unset_private_mode(&mut self, mode: PrivateMode) {
        let closing = mode == ALTERNATE_SCREEN && self.screen() != MAIN;
        self.0.unset_private_mode(mode);
        if closing {
            self.use_region();
        }
        // Closing the alternate screen restores the cursor in xterm.js, even when it is not
        // open. `alacritty_terminal` only goes back to the cursor the main screen had.
        if mode == ALTERNATE_SCREEN {
            self.restore_cursor_position();
        }
        // Turning origin mode off homes the cursor, as turning it on does, in xterm, VTE and
        // xterm.js; `alacritty_terminal` leaves the cursor where it is.
        if mode == PrivateMode::Named(NamedPrivateMode::Origin) {
            self.0.goto(0, 0);
        }
    }

    fn report_private_mode(&mut self, mode: PrivateMode) {
        self.0.report_private_mode(mode)
    }

    fn set_scrolling_region(&mut self, top: usize, bottom: Option<usize>) {
        // xterm.js takes a bottom past the screen for the bottom of the screen, and ignores a
        // region whose top is not above it then. `alacritty_terminal` would clamp both to the
        // screen and could keep a region with no rows.
        let rows = self.0.screen_lines();
        let bottom = bottom.filter(|&bottom| bottom <= rows).unwrap_or(rows);
        if top >= bottom {
            return;
        }
        let screen = self.screen();
        self.1.scroll_regions[screen].set(top, bottom);
        self.0.set_scrolling_region(top, Some(bottom))
    }

    fn set_keypad_application_mode(&mut self) {
        self.0.set_keypad_application_mode()
    }

    fn unset_keypad_application_mode(&mut self) {
        self.0.unset_keypad_application_mode()
    }

    fn set_active_charset(&mut self, index: CharsetIndex) {
        self.0.set_active_charset(index)
    }

    fn configure_charset(&mut self, index: CharsetIndex, charset: StandardCharset) {
        self.0.configure_charset(index, charset)
    }

    fn set_color(&mut self, index: usize, color: Rgb) {
        self.0.set_color(index, color)
    }

    fn dynamic_color_sequence(&mut self, prefix: String, index: usize, terminator: &str) {
        self.0.dynamic_color_sequence(prefix, index, terminator)
    }

    fn reset_color(&mut self, index: usize) {
        self.0.reset_color(index)
    }

    fn clipboard_store(&mut self, clipboard: u8, base64: &[u8]) {
        self.0.clipboard_store(clipboard, base64)
    }

    fn clipboard_load(&mut self, clipboard: u8, terminator: &str) {
        self.0.clipboard_load(clipboard, terminator)
    }

    fn decaln(&mut self) {
        self.0.decaln()
    }

    fn push_title(&mut self) {
        self.0.push_title()
    }

    fn pop_title(&mut self) {
        self.0.pop_title()
    }

    fn text_area_size_pixels(&mut self) {
        self.0.text_area_size_pixels()
    }

    fn text_area_size_chars(&mut self) {
        self.0.text_area_size_chars()
    }

    fn set_hyperlink(&mut self, link: Option<Hyperlink>) {
        self.0.set_hyperlink(link)
    }

    fn set_mouse_cursor_icon(&mut self, icon: CursorIcon) {
        self.0.set_mouse_cursor_icon(icon)
    }

    fn report_keyboard_mode(&mut self) {
        self.0.report_keyboard_mode()
    }

    fn push_keyboard_mode(&mut self, mode: KeyboardModes) {
        self.0.push_keyboard_mode(mode)
    }

    fn pop_keyboard_modes(&mut self, to_pop: u16) {
        self.0.pop_keyboard_modes(to_pop)
    }

    fn set_keyboard_mode(&mut self, mode: KeyboardModes, behavior: KeyboardModesApplyBehavior) {
        self.0.set_keyboard_mode(mode, behavior)
    }

    fn set_modify_other_keys(&mut self, mode: ModifyOtherKeys) {
        self.0.set_modify_other_keys(mode)
    }

    fn report_modify_other_keys(&mut self) {
        self.0.report_modify_other_keys()
    }

    fn set_scp(&mut self, char_path: ScpCharPath, update_mode: ScpUpdateMode) {
        self.0.set_scp(char_path, update_mode)
    }
}

#[cfg(test)]
/// The screens these tests expect are the ones `@xterm/headless` 6.0.0 draws for the same
/// output on a terminal of the same size.
mod tests {
    use alacritty_terminal::grid::Dimensions;
    use alacritty_terminal::index::{Column, Line};
    use alacritty_terminal::term::cell::Flags;
    use alacritty_terminal::vte::ansi::{Color, NamedColor};

    use super::super::{AlacrittyEngine, Engine, Size};

    const SIZE: Size = Size {
        columns: 12,
        rows: 5,
    };

    fn fed(bytes: &str) -> AlacrittyEngine {
        let mut term = AlacrittyEngine::new(SIZE, 50);
        term.advance(bytes.as_bytes());
        term
    }

    fn line(term: &mut AlacrittyEngine, row: usize) -> String {
        term.screen().lines[row].clone()
    }

    /// The background of one cell of the screen.
    fn background(term: &AlacrittyEngine, row: i32, column: usize) -> Color {
        term.grid()[Line(row)][Column(column)].bg
    }

    fn flags(term: &AlacrittyEngine, row: i32, column: usize) -> Flags {
        term.grid()[Line(row)][Column(column)].flags
    }

    const DEFAULT: Color = Color::Named(NamedColor::Background);

    /// The lines of the scrollback, oldest first, each without the blanks it ends with.
    fn scrollback(term: &AlacrittyEngine) -> Vec<String> {
        let grid = term.grid();
        (grid.topmost_line().0..0)
            .map(|line| {
                let row = &grid[Line(line)];
                let text: String = (0..grid.columns())
                    .map(|column| row[Column(column)].c)
                    .collect();
                text.trim_end().to_owned()
            })
            .collect()
    }

    // Each expectation below is what xterm.js 6.0.0, the pane, shows for the same bytes.

    #[test]
    fn deleting_characters_at_the_second_half_of_a_wide_character_blanks_the_character() {
        let mut term = fed("中x\x1b[1;2H\x1b[P");

        assert_eq!(line(&mut term, 0), " x");
    }

    #[test]
    fn deleting_characters_that_leave_only_a_second_half_behind_blanks_everything() {
        // The case #441 was found with.
        let mut term = fed("中中\x1b[7m\r中\x08\x1b[2P");

        assert_eq!(line(&mut term, 0), "");
    }

    #[test]
    fn deleting_the_first_half_of_a_wide_character_leaves_a_blank_in_the_erase_colour() {
        let term = fed("\x1b[41m中\x1b[0mx\x1b[1;1H\x1b[P");

        assert_eq!(background(&term, 0, 0), DEFAULT);
    }

    #[test]
    fn deleting_more_characters_than_the_row_has_left_keeps_what_is_before_the_cursor() {
        let mut term = fed("abcdefgh\x1b[1;6H\x1b[20P");

        assert_eq!(line(&mut term, 0), "abcde");
    }

    #[test]
    fn inserting_blanks_at_the_second_half_of_a_wide_character_blanks_the_character() {
        let mut term = fed("中x\x1b[1;2H\x1b[@");

        assert_eq!(line(&mut term, 0), "   x");
    }

    #[test]
    fn inserting_blanks_that_push_a_wide_character_into_the_last_column_blanks_it_in_the_erase_colour()
     {
        let term = fed("\x1b[1;11H\x1b[41m中\x1b[0m\x1b[1;1H\x1b[@");

        assert_eq!(background(&term, 0, 11), DEFAULT);
        assert!(!flags(&term, 0, 11).contains(Flags::WIDE_CHAR));
    }

    #[test]
    fn erasing_characters_from_the_second_half_of_a_wide_character_blanks_the_character() {
        let mut term = fed("中x\x1b[1;2H\x1b[X");

        assert_eq!(line(&mut term, 0), "  x");
    }

    #[test]
    fn erasing_characters_up_to_the_first_half_of_a_wide_character_blanks_its_second_half() {
        let term = fed("\x1b[41m中\x1b[0mx\x1b[1;1H\x1b[X");

        assert_eq!(background(&term, 0, 1), DEFAULT);
    }

    #[test]
    fn erasing_the_line_from_the_second_half_of_a_wide_character_blanks_the_character() {
        let mut term = fed("中x\x1b[1;2H\x1b[K");

        assert_eq!(line(&mut term, 0), "");
    }

    #[test]
    fn erasing_the_line_up_to_the_first_half_of_a_wide_character_blanks_its_second_half() {
        let term = fed("\x1b[41m中\x1b[0mx\x1b[1;1H\x1b[1K");

        assert_eq!(background(&term, 0, 1), DEFAULT);
    }

    #[test]
    fn erasing_the_screen_from_the_second_half_of_a_wide_character_blanks_the_character() {
        let mut term = fed("中x\x1b[1;2H\x1b[J");

        assert_eq!(line(&mut term, 0), "");
    }

    #[test]
    fn erasing_the_screen_up_to_the_first_half_of_a_wide_character_blanks_its_second_half() {
        let term = fed("\x1b[41m中\x1b[0mx\x1b[1;1H\x1b[1J");

        assert_eq!(background(&term, 0, 1), DEFAULT);
    }

    #[test]
    fn erasing_the_screen_above_from_the_second_row_erases_the_first() {
        let mut term = fed("aaa\r\nbbb\x1b[1J");

        assert_eq!(line(&mut term, 0), "");
    }

    #[test]
    fn deleting_inserting_or_erasing_characters_ends_a_pending_wrap() {
        // xterm, VTE and xterm.js all do; the next character lands in the last column.
        for edit in ["\x1b[P", "\x1b[@", "\x1b[X"] {
            let mut term = fed(&format!("abcdefghijkl{edit}x"));

            assert_eq!(line(&mut term, 1), "", "after {edit:?}");
            assert!(line(&mut term, 0).ends_with('x'), "after {edit:?}");
        }
    }

    #[test]
    fn moving_down_or_up_a_row_ends_a_pending_wrap() {
        // xterm, VTE and xterm.js all do; the next character lands in the last column.
        for (movement, row) in [("\n", 2), ("\x1bD", 2), ("\x0b", 2), ("\x1bM", 0)] {
            let mut term = fed(&format!("\r\nabcdefghijkl{movement}x"));

            assert!(
                line(&mut term, row).ends_with('x'),
                "after {movement:?}: {:?}",
                term.screen().lines
            );
        }
    }

    #[test]
    fn a_wide_character_dropped_at_the_last_column_with_wrapping_off_leaves_no_wrap_pending() {
        // The cursor stays on the last column, so erasing from it erases that column.
        let mut term = fed("\x1b[?7l\x1b[1;6Hbcdefgh中\x1b[J");

        assert_eq!(line(&mut term, 0), "     bcdefg");
    }

    #[test]
    fn inserting_characters_keeps_the_ones_before_the_cursor_when_a_wide_one_is_pushed_off() {
        // `alacritty_terminal` rotates the row: the second half pushed off comes back in at
        // the cursor, and writing over it blanked the character before.
        let mut term = fed("\x1b[4h\x1b[1;8H中\r\x1b[2@ab");

        assert_eq!(line(&mut term, 0), "ab");
    }

    #[test]
    fn inserting_a_wide_character_that_wraps_shifts_the_next_row() {
        let mut term = fed("\x1b[4h\x1b[2;8Hx\x1b[1;12H中");

        assert_eq!(line(&mut term, 1), "中       x");
    }

    #[test]
    fn writing_a_wide_character_over_a_second_half_whose_character_is_gone_keeps_it() {
        let mut term = fed("中\x1b[D\x1b[@中");

        assert_eq!(line(&mut term, 0), " 中");
    }

    #[test]
    fn erasing_the_screen_below_while_a_wrap_is_pending_keeps_the_last_column() {
        // xterm.js has the cursor past the row then. xterm and VTE step back onto the last
        // column and erase it; the pane is xterm.js.
        let mut term = fed("abcdefghijkl\x1b[J");

        assert_eq!(line(&mut term, 0), "abcdefghijkl");
    }

    #[test]
    fn erasing_the_screen_below_while_a_wrap_is_pending_keeps_a_wide_character_whole() {
        let term = fed("\x1b[1;11H中\x1b[J");

        assert!(flags(&term, 0, 10).contains(Flags::WIDE_CHAR));
        assert!(flags(&term, 0, 11).contains(Flags::WIDE_CHAR_SPACER));
    }

    #[test]
    fn writing_onto_the_second_half_of_a_wide_character_blanks_the_character() {
        let mut term = fed("中\x08a");

        assert_eq!(line(&mut term, 0), " a");
    }

    #[test]
    fn writing_over_the_first_half_of_a_wide_character_leaves_a_blank_in_the_pen() {
        let term = fed("\x1b[41m中\x1b[0m\x1b[1;1Ha");

        assert_eq!(background(&term, 0, 1), DEFAULT);
    }

    #[test]
    fn writing_a_wide_character_that_wraps_over_a_first_half_leaves_a_blank_in_the_pen() {
        let term = fed("\r\n \x1b[41m中\x1b[0m\x1b[1;12H中");

        assert_eq!(background(&term, 1, 2), DEFAULT);
    }

    #[test]
    fn inserting_a_character_at_the_second_half_of_a_wide_character_blanks_the_character() {
        let mut term = fed("中x\x1b[4h\x1b[1;2Ha");

        assert_eq!(line(&mut term, 0), " a x");
    }

    #[test]
    fn inserting_a_character_that_pushes_a_wide_character_into_the_last_column_blanks_it_in_the_pen()
     {
        let term = fed("\x1b[1;11H\x1b[41m中\x1b[0m\x1b[4h\x1b[1;1Ha");

        assert_eq!(background(&term, 0, 11), DEFAULT);
        assert!(!flags(&term, 0, 11).contains(Flags::WIDE_CHAR));
    }

    #[test]
    fn a_tab_while_a_wrap_is_pending_leaves_the_wrap_pending() {
        // xterm, VTE and xterm.js all do; `alacritty_terminal` wraps, and the carriage return
        // after it went back to the start of the next row.
        let mut term = fed("abcdefghijkl\t\rX");

        assert_eq!(line(&mut term, 0), "Xbcdefghijkl");
        assert_eq!(line(&mut term, 1), "");
    }

    #[test]
    fn a_backward_tab_while_a_wrap_is_pending_does_nothing() {
        // In xterm.js; `alacritty_terminal` moved the cursor back and kept the wrap pending.
        let mut term = fed("abcdefghijkl\x1b[Z\x1b[Px");

        assert_eq!(line(&mut term, 0), "abcdefghijkx");
    }

    #[test]
    fn restoring_the_cursor_ends_a_wrap_pending_when_it_was_saved() {
        // xterm.js keeps the cursor in the last column; xterm and VTE restore the wrap.
        let mut term = fed("abcdefghijkl\x1b7\x1b[H\x1b8X");

        assert_eq!(line(&mut term, 0), "abcdefghijkX");
        assert_eq!(line(&mut term, 1), "");
    }

    #[test]
    fn restoring_the_cursor_the_sco_way_ends_a_wrap_pending_when_it_was_saved() {
        let mut term = fed("abcdefghijkl\x1b[s\x1b[H\x1b[uX");

        assert_eq!(line(&mut term, 0), "abcdefghijkX");
        assert_eq!(line(&mut term, 1), "");
    }

    #[test]
    fn inserting_lines_moves_the_cursor_to_the_first_column() {
        let mut term = fed("abc\x1b[Lx");

        assert_eq!(line(&mut term, 0), "x");
        assert_eq!(line(&mut term, 1), "abc");
    }

    #[test]
    fn deleting_lines_moves_the_cursor_to_the_first_column() {
        let mut term = fed("abc\x1b[Mx");

        assert_eq!(line(&mut term, 0), "x");
    }

    #[test]
    fn inserting_lines_while_a_wrap_is_pending_ends_it() {
        let mut term = fed("abcdefghijkl\x1b[Lx");

        assert_eq!(line(&mut term, 0), "x");
        assert_eq!(line(&mut term, 1), "abcdefghijkl");
    }

    #[test]
    fn deleting_lines_while_a_wrap_is_pending_ends_it() {
        let mut term = fed("abcdefghijkl\x1b[Mx");

        assert_eq!(line(&mut term, 0), "x");
        assert_eq!(line(&mut term, 1), "");
    }

    #[test]
    fn inserting_lines_outside_the_scroll_region_leaves_the_cursor_in_its_column() {
        let mut term = fed("\x1b[2;3rabc\x1b[Lx");

        assert_eq!(line(&mut term, 0), "abcx");
    }

    #[test]
    fn inserting_or_deleting_lines_outside_the_scroll_region_ends_a_pending_wrap() {
        // xterm.js does; xterm and VTE leave the wrap pending, as they do nothing at all.
        for edit in ["\x1b[L", "\x1b[M"] {
            let mut term = fed(&format!("\x1b[2;3rabcdefghijkl{edit}x"));

            assert_eq!(line(&mut term, 0), "abcdefghijkx", "after {edit:?}");
            assert_eq!(line(&mut term, 1), "", "after {edit:?}");
        }
    }

    #[test]
    fn a_scroll_region_that_is_not_one_is_ignored() {
        let mut term = fed("\x1b[3;2rabc\x1b[Lx");

        assert_eq!(line(&mut term, 0), "x");
    }

    #[test]
    fn a_scroll_region_is_undone_by_resetting_it() {
        let mut term = fed("abc\x1b[2;3r\x1b[r\x1b[1;4H\x1b[Mx");

        assert_eq!(line(&mut term, 0), "x");
    }

    #[test]
    fn a_scroll_region_is_undone_by_resetting_the_terminal() {
        let mut term = fed("\x1b[2;3r\x1bcabc\x1b[Lx");

        assert_eq!(line(&mut term, 0), "x");
    }

    #[test]
    fn a_scroll_region_is_undone_by_resizing() {
        // `alacritty_terminal` resets the region only when the size changes.
        let mut term = fed("\x1b[2;3r");
        term.resize(Size {
            columns: 12,
            rows: 6,
        });
        term.advance(b"abc\x1b[Lx");

        assert_eq!(line(&mut term, 0), "x");
    }

    #[test]
    fn moving_up_stops_at_the_top_of_the_scroll_region() {
        for movement in ["\x1b[A", "\x1b[5A", "\x1b[5F"] {
            let mut term = fed(&format!("\x1b[2;4r\x1b[3;3H{movement}\rx"));

            assert_eq!(line(&mut term, 1), "x", "after {movement:?}");
        }
    }

    #[test]
    fn moving_down_stops_at_the_bottom_of_the_scroll_region() {
        for movement in ["\x1b[B", "\x1b[5B", "\x1b[5E"] {
            let mut term = fed(&format!("\x1b[2;4r\x1b[4;3H{movement}\rx"));

            assert_eq!(line(&mut term, 3), "x", "after {movement:?}");
        }
    }

    #[test]
    fn moving_down_from_above_the_scroll_region_stops_at_its_bottom() {
        let mut term = fed("\x1b[2;4r\x1b[1;1H\x1b[9Bx");

        assert_eq!(line(&mut term, 3), "x");
    }

    #[test]
    fn moving_up_from_below_the_scroll_region_stops_at_its_top() {
        let mut term = fed("\x1b[2;4r\x1b[5;1H\x1b[9Ax");

        assert_eq!(line(&mut term, 1), "x");
    }

    #[test]
    fn restoring_the_cursor_after_lines_scrolled_into_history_follows_them_up() {
        // xterm.js saves the row in its scrollback; xterm and VTE save the row on the screen.
        let mut term = fed("\x1b[5;1H\x1b7\n\nx\x1b8y");

        assert_eq!(line(&mut term, 2), "y");
    }

    #[test]
    fn restoring_the_cursor_after_the_history_is_erased_stays_on_the_screen() {
        let mut term = fed("\x1b[5;1H\x1b7\n\n\x1b[3Jx\x1b8y");

        assert_eq!(line(&mut term, 4), "y");
    }

    #[test]
    fn leaving_the_alternate_screen_ends_a_wrap_pending_as_it_was_opened() {
        let mut term = fed("abcdefghijkl\x1b[?1049hx\x1b[?1049ly");

        assert_eq!(line(&mut term, 0), "abcdefghijky");
        assert_eq!(line(&mut term, 1), "");
    }

    #[test]
    fn restoring_the_cursor_after_lines_are_deleted_from_the_top_keeps_its_row() {
        // `alacritty_terminal` moves the deleted lines into its history; xterm.js drops them.
        for scroll in ["\x1b[H\x1b[3M", "\x1b[3S"] {
            let mut term = fed(&format!("\x1b[4;2H\x1b7{scroll}\x1b8x"));

            assert_eq!(line(&mut term, 3), " x", "after {scroll:?}");
        }
    }

    #[test]
    fn a_combining_mark_with_nothing_before_it_on_the_row_takes_a_cell_of_its_own() {
        // xterm.js draws nothing in that cell and moves the cursor past it.
        let mut term = fed("\u{301}x");

        assert_eq!(line(&mut term, 0), " x");
    }

    #[test]
    fn a_combining_mark_written_at_the_start_of_a_row_replaces_the_character_there() {
        for row in ["ab", "\u{4e2d}"] {
            let mut term = fed(&format!("{row}\r\u{301}x"));

            assert_eq!(line(&mut term, 0), " x", "over {row:?}");
        }
    }

    #[test]
    fn a_scroll_region_below_the_screen_is_ignored() {
        // xterm.js takes the bottom for the bottom of the screen, which leaves no rows.
        let mut term = fed("abc\x1b[9;10r\x1b[Lx");

        assert_eq!(line(&mut term, 0), "x");
    }

    #[test]
    fn turning_origin_mode_off_homes_the_cursor() {
        let mut term = fed("\x1b[5;3H\x1b[?6lx");

        assert_eq!(line(&mut term, 0), "x");
    }

    #[test]
    fn restoring_the_cursor_after_the_screen_is_erased_keeps_its_row() {
        // `alacritty_terminal` moves the lines erased into its history; xterm.js drops them.
        let mut term = fed("a\r\nb\r\nc\x1b[4;2H\x1b7\x1b[2J\x1b8x");

        assert_eq!(line(&mut term, 3), " x");
    }

    #[test]
    fn restoring_the_cursor_after_the_screen_scrolls_up_and_the_history_is_erased_keeps_its_row() {
        let mut term = fed("\x1b7\x1b[3S\x1b[3J\x1b8x");

        assert_eq!(line(&mut term, 0), "x");
    }

    #[test]
    fn restoring_the_cursor_after_the_history_is_erased_keeps_the_row_it_had_in_the_scrollback() {
        // xterm.js keeps the saved row in its scrollback, and erasing the scrollback does not
        // move it: the cursor comes back as many rows lower as the scrollback had lines.
        let mut term = fed("\n\n\n\n\n\n\n\x1b[2;3H\x1b7\x1b[3J\x1b8x");

        assert_eq!(line(&mut term, 4), "  x");
    }

    #[test]
    fn restoring_the_cursor_in_origin_mode_keeps_it_inside_the_scroll_region() {
        let mut term = fed("\x1b7\x1b[3;5r\x1b[?6h\x1b8x");

        assert_eq!(line(&mut term, 2), "x");
    }

    #[test]
    fn moving_left_or_right_in_origin_mode_moves_down_by_where_the_scroll_region_starts() {
        // xterm.js places the cursor from the top of the region again after a relative move.
        for movement in ["\x1b[C", "\x1b[D"] {
            let mut term = fed(&format!("\x1b[?6h\x1b[2;4r{movement}x"));

            assert_eq!(line(&mut term, 2).trim(), "x", "after {movement:?}");
        }
    }

    #[test]
    fn closing_the_alternate_screen_restores_the_cursor_even_when_it_is_not_open() {
        let mut term = fed("\x1b[3;4H\x1b7\x1b[H\x1b[?1049lx");

        assert_eq!(line(&mut term, 2), "   x");
    }

    #[test]
    fn opening_the_alternate_screen_saves_the_cursor_even_when_it_is_open() {
        let mut term = fed("\x1b[?1049h\x1b[3;4H\x1b[?1049h\x1b[H\x1b8x");

        assert_eq!(line(&mut term, 2), "   x");
    }

    #[test]
    fn the_alternate_screen_has_a_scroll_region_of_its_own() {
        let mut term = fed("\x1b[2;4r\x1b[?1049h\x1b[5Bx");

        assert_eq!(line(&mut term, 4), "x");
    }

    #[test]
    fn closing_the_alternate_screen_brings_back_the_scroll_region_of_the_main_one() {
        let mut term = fed("\x1b[?1049h\x1b[2;4r\x1b[?1049l\x1b[5Bx");

        assert_eq!(line(&mut term, 4), "x");
    }

    #[test]
    fn the_alternate_screen_opens_with_a_scroll_region_of_the_whole_screen() {
        let mut term = fed("\x1b[?1049h\x1b[2;4r\x1b[?1049l\x1b[?1049h\x1b[5Bx");

        assert_eq!(line(&mut term, 4), "x");
    }

    #[test]
    fn erasing_the_screen_leaves_the_scrollback_alone() {
        // `clear` does this. xterm.js erases the screen in place; `alacritty_terminal` scrolled
        // what was on it into its history.
        let mut term = fed("abc\r\ndef\x1b[2J");

        assert!(scrollback(&term).is_empty());
        assert_eq!(line(&mut term, 0), "");
    }

    #[test]
    fn erasing_the_screen_keeps_what_had_scrolled_into_the_scrollback_before() {
        let term = fed("1\r\n2\r\n3\r\n4\r\n5\r\n6\r\n7\x1b[2J");

        assert_eq!(scrollback(&term), ["1", "2"]);
    }

    #[test]
    fn scrolling_up_drops_the_lines_it_takes_off_the_top() {
        let mut term = fed("abc\r\ndef\x1b[S");

        assert!(scrollback(&term).is_empty());
        assert_eq!(line(&mut term, 0), "def");
    }

    #[test]
    fn deleting_lines_at_the_top_drops_them() {
        let mut term = fed("abc\r\ndef\x1b[H\x1b[M");

        assert!(scrollback(&term).is_empty());
        assert_eq!(line(&mut term, 0), "def");
    }

    #[test]
    fn a_line_feed_at_the_bottom_of_a_scroll_region_at_the_top_still_scrolls_into_the_scrollback() {
        let term = fed("abc\r\ndef\x1b[1;4r\x1b[4;1H\n");

        assert_eq!(scrollback(&term), ["abc"]);
    }
}
