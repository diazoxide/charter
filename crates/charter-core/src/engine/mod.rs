//! A terminal's state, held headless: what a harness has drawn, whether or not a pane shows it.
//!
//! The core owns one engine per session, so a hidden session costs memory and no UI resources.
//! The UI asks for a [`Screen`] only when it shows the pane.

mod alacritty;

pub use alacritty::AlacrittyEngine;

/// A terminal's size in character cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Size {
    pub columns: u16,
    pub rows: u16,
}

impl Size {
    /// The smallest terminal an engine keeps. A UI asks for 0×0 for a collapsed pane, and the
    /// grid cannot be that small.
    pub const MIN: Size = Size {
        columns: 2,
        rows: 1,
    };

    /// This size, raised to at least [`Size::MIN`] on each side.
    pub fn at_least_min(self) -> Size {
        Size {
            columns: self.columns.max(Self::MIN.columns),
            rows: self.rows.max(Self::MIN.rows),
        }
    }
}

/// What is on a terminal's screen right now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Screen {
    pub size: Size,
    /// One entry per row, trailing blanks removed.
    pub lines: Vec<String>,
    /// Row and cell column of the cursor, zero-based. A column counts cells, so after a wide
    /// character it is not the character index into `lines[row]`.
    pub cursor: (u16, u16),
}

/// A terminal emulator with no renderer: bytes in, screen state out.
pub trait Engine: Send {
    /// Feeds output the program wrote to its terminal.
    fn advance(&mut self, bytes: &[u8]);

    /// Changes the size, reflowing what is on screen. Sizes below [`Size::MIN`] are raised to it.
    fn resize(&mut self, size: Size);

    /// The current screen. A synchronized update (`?2026`) the program opened and never closed
    /// is applied first once its timeout has passed, so a program that dies mid-repaint cannot
    /// freeze its screen.
    fn screen(&mut self) -> Screen;

    /// Bytes the terminal owes the program, such as answers to a cursor-position query.
    /// They must be written back to the program's input, or programs that ask will hang.
    fn take_replies(&mut self) -> Vec<u8>;
}
