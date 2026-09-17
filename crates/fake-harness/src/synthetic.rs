//! Output shaped like an agent harness's: coloured text, a full repaint every 40 lines, each
//! repaint inside a synchronized-output block (`?2026`). ADR 0018 measured against this shape.

/// Exactly `len` bytes of harness-shaped output. The same `len` always gives the same bytes.
pub fn generate(len: usize) -> Vec<u8> {
    const LINES_PER_REPAINT: usize = 40;
    const WORDS: [&str; 8] = [
        "Reading",
        "crates/core/src/session.rs",
        "running",
        "cargo test",
        "ok",
        "Edited",
        "3 files",
        "done",
    ];

    let mut output = Vec::with_capacity(len + 4096);
    let mut line = 0usize;
    while output.len() < len {
        output.extend_from_slice(b"\x1b[?2026h\x1b[H\x1b[2J");
        for _ in 0..LINES_PER_REPAINT {
            let colour = 16 + line % 216;
            let words = WORDS[line % WORDS.len()];
            let detail = WORDS[(line / 3) % WORDS.len()];
            output.extend_from_slice(
                format!("\x1b[38;5;{colour}m● {words}\x1b[0m {detail} (line {line})\r\n")
                    .as_bytes(),
            );
            line += 1;
        }
        output.extend_from_slice(b"\x1b[?2026l");
    }
    output.truncate(len);
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    const BEGIN_SYNC: &[u8] = b"\x1b[?2026h";
    const END_SYNC: &[u8] = b"\x1b[?2026l";

    fn count(haystack: &[u8], needle: &[u8]) -> usize {
        haystack
            .windows(needle.len())
            .filter(|window| *window == needle)
            .count()
    }

    #[test]
    fn it_is_exactly_the_length_asked_for() {
        for len in [0, 1, 100, 4096, 1_000_000] {
            assert_eq!(generate(len).len(), len);
        }
    }

    #[test]
    fn it_is_the_same_every_time() {
        assert_eq!(generate(50_000), generate(50_000));
    }

    #[test]
    fn it_repaints_inside_synchronized_output_blocks() {
        let output = generate(200_000);

        let begins = count(&output, BEGIN_SYNC);
        assert!(begins > 10, "only {begins} synchronized repaints");
        // The last block may be cut off by the length limit, never more than one.
        assert!(begins - count(&output, END_SYNC) <= 1);
    }

    #[test]
    fn it_is_coloured() {
        assert!(count(&generate(20_000), b"\x1b[38;5;") > 100);
    }
}
