//! Where `alacritty_terminal` and the pane disagree on what output does to the screen, the
//! engine does what the pane does.
//!
//! The pane is xterm.js. A pane attached live draws the program's output itself, and one that
//! opens late draws this engine's snapshot, so any output the two emulators apply differently
//! leaves two panes of one session showing different things until the program redraws (#441).
//!
//! **Wide characters cut in half.** Deleting (`DCH`), inserting (`ICH`), erasing (`ECH`,
//! `EL`, `ED`) or writing from a wide character's second half, or up to its first, leaves half
//! a character. xterm.js blanks the half that is left; so does xterm (`DamagedCells` in
//! `util.c`), and so does VTE (`cleanup_fragments`). `alacritty_terminal` keeps the character
//! whole when its second half is cut, and keeps the old colours on a second half left behind.
//! xterm.js blanks in the erase colour for an edit and in the pen for a write, and so does
//! this engine; VTE keeps the old colours, which no pane shows.
//!
//! **A pending wrap.** After the last column is written, the next character wraps. Deleting,
//! inserting or erasing characters, and moving down or up a row (`LF`, `IND`, `RI`), end that
//! in xterm, VTE and xterm.js, and the next character lands in the last column;
//! `alacritty_terminal` keeps it. Erasing below (`ED 0`) while a wrap is pending erases
//! nothing of the cursor's row in xterm.js, which has the cursor past the row, where xterm and
//! VTE step back onto the last column and erase it. This engine follows xterm.js there,
//! because that is what the pane shows.
//!
//! **Two plain cases.** `alacritty_terminal` gets two edits wrong that every terminal above
//! gets right: deleting more characters than the row has left of the cursor also blanks what
//! is before it, and erasing above the cursor from the second row leaves the first.
//!
//! What stays apart: with wrapping off (`?7l`), xterm.js writes a character over the second
//! half of a wide character in the last column and leaves the character standing, a state an
//! `alacritty_terminal` grid cannot hold.
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
    KeyboardModesApplyBehavior, LineClearMode, Mode, ModifyOtherKeys, PrivateMode, Rgb,
    ScpCharPath, ScpUpdateMode, StandardCharset, TabulationClearMode,
};
use unicode_width::UnicodeWidthChar;

/// A terminal that edits as the pane does. Output is parsed into this, never into the
/// [`Term`] itself.
pub(super) struct PaneRules<'a, T>(pub(super) &'a mut Term<T>);

impl<T> PaneRules<'_, T> {
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

    /// Erases cells from `start` up to, not including, `end` with `erase`, blanking a wide
    /// character that `start` cuts into and the second half of one that `end` cuts.
    fn erase(&mut self, start: usize, end: usize, erase: impl FnOnce(&mut Term<T>)) {
        let (line, _) = self.cursor();
        let cut_on_the_left = self.is_second_half(line, start);
        let cut_on_the_right = end < self.0.columns() && self.is_second_half(line, end);
        erase(self.0);
        if cut_on_the_left {
            self.blank(line, start - 1);
        }
        if cut_on_the_right {
            self.blank(line, end);
        }
    }
}

impl<T: EventListener> PaneRules<'_, T> {
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
        let left_behind = self.0.grid()[line][Column(next)]
            .flags
            .contains(Flags::WIDE_CHAR_SPACER)
            && !self.is_second_half(line, next);
        if left_behind {
            self.blank_in_pen(line, next);
        }
        // A wide character pushed into the last column has no room for its second half.
        if insert && self.is_wide(line, columns - 1) {
            self.blank_in_pen(line, columns - 1);
        }
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
        if self.0.grid()[line][Column(column)]
            .flags
            .contains(Flags::WIDE_CHAR_SPACER)
            && !self.is_second_half(line, column)
        {
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
            ClearMode::All | ClearMode::Saved => self.0.clear_screen(mode),
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

    fn move_up(&mut self, rows: usize) {
        self.0.move_up(rows)
    }

    fn move_down(&mut self, rows: usize) {
        self.0.move_down(rows)
    }

    fn identify_terminal(&mut self, intermediate: Option<char>) {
        self.0.identify_terminal(intermediate)
    }

    fn device_status(&mut self, status: usize) {
        self.0.device_status(status)
    }

    fn move_forward(&mut self, columns: usize) {
        self.0.move_forward(columns)
    }

    fn move_backward(&mut self, columns: usize) {
        self.0.move_backward(columns)
    }

    fn move_down_and_cr(&mut self, rows: usize) {
        self.0.move_down_and_cr(rows)
    }

    fn move_up_and_cr(&mut self, rows: usize) {
        self.0.move_up_and_cr(rows)
    }

    fn put_tab(&mut self, count: u16) {
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
        self.0.scroll_up(rows)
    }

    fn scroll_down(&mut self, rows: usize) {
        self.0.scroll_down(rows)
    }

    fn insert_blank_lines(&mut self, rows: usize) {
        self.0.insert_blank_lines(rows)
    }

    fn delete_lines(&mut self, rows: usize) {
        self.0.delete_lines(rows)
    }

    fn move_backward_tabs(&mut self, count: u16) {
        self.0.move_backward_tabs(count)
    }

    fn move_forward_tabs(&mut self, count: u16) {
        self.0.move_forward_tabs(count)
    }

    fn save_cursor_position(&mut self) {
        self.0.save_cursor_position()
    }

    fn restore_cursor_position(&mut self) {
        self.0.restore_cursor_position()
    }

    fn clear_tabs(&mut self, mode: TabulationClearMode) {
        self.0.clear_tabs(mode)
    }

    fn set_tabs(&mut self, interval: u16) {
        self.0.set_tabs(interval)
    }

    fn reset_state(&mut self) {
        self.0.reset_state()
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
        self.0.set_private_mode(mode)
    }

    fn unset_private_mode(&mut self, mode: PrivateMode) {
        self.0.unset_private_mode(mode)
    }

    fn report_private_mode(&mut self, mode: PrivateMode) {
        self.0.report_private_mode(mode)
    }

    fn set_scrolling_region(&mut self, top: usize, bottom: Option<usize>) {
        self.0.set_scrolling_region(top, bottom)
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
mod tests {
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
}
