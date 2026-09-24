//! The engine against real harness output, not only output we generated ourselves.

use std::path::PathBuf;

use charter_core::engine::{AlacrittyEngine, Engine, Size};

const SIZE: Size = Size {
    columns: 150,
    rows: 42,
};

fn corpus() -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/corpora/claude-code-session.raw");
    std::fs::read(&path).unwrap_or_else(|err| panic!("cannot read {}: {err}", path.display()))
}

#[test]
fn a_recorded_claude_code_session_renders_to_the_screen_it_ended_on() {
    charter_core::unsteered!();
    let mut engine = AlacrittyEngine::new(SIZE, 10_000);

    // In chunks, because a PTY delivers it that way and escape sequences straddle the joins.
    // In chunks, because a PTY delivers it that way and escape sequences straddle the joins.
    for chunk in corpus().chunks(997) {
        engine.advance(chunk);
    }

    let screen = engine.screen();
    assert_eq!(screen.size, SIZE);
    let text = screen.lines.join("\n");
    assert!(
        text.contains("2000"),
        "the answer's last number is on screen: {text}"
    );
}
