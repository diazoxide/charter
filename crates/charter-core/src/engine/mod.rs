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

/// What is on a terminal's screen right now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Screen {
    pub size: Size,
    /// One entry per row, trailing blanks removed.
    pub lines: Vec<String>,
    /// Row and column of the cursor, zero-based.
    pub cursor: (u16, u16),
}

/// A terminal emulator with no renderer: bytes in, screen state out.
pub trait Engine: Send {
    /// Feeds output the program wrote to its terminal.
    fn advance(&mut self, bytes: &[u8]);

    /// Changes the size, reflowing what is on screen.
    fn resize(&mut self, size: Size);

    /// The current screen.
    fn screen(&self) -> Screen;

    /// Bytes the terminal owes the program, such as answers to a cursor-position query.
    /// They must be written back to the program's input, or programs that ask will hang.
    fn take_replies(&mut self) -> Vec<u8>;
}
