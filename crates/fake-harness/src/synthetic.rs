//! Output shaped like an agent harness's: coloured text, a full repaint every 40 lines, each
//! repaint inside a synchronized-output block (`?2026`). ADR 0018 measured against this shape.

/// Ends a synchronized update. A terminal left inside one draws nothing until its own safety
/// timeout — a second, in xterm.js — so output never ends in the middle of one.
const END_SYNC: &[u8] = b"\x1b[?2026l";

/// Exactly `len` bytes of harness-shaped output, ending outside a synchronized update. The
/// same `len` always gives the same bytes.
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
    // Cutting at `len` usually lands inside a repaint, which would leave the update open: the
    // last bytes become the one that ends it. Below its length there is no whole `?2026h` to
    // close, so a fragment of one is all that is left, which no terminal reads as a mode.
    if len >= END_SYNC.len() && open_update(&output) {
        let from = len - END_SYNC.len();
        output[from..].copy_from_slice(END_SYNC);
    }
    output
}

/// Whether a synchronized update is open at the end of these bytes.
fn open_update(output: &[u8]) -> bool {
    let last_begin = find_last(output, b"\x1b[?2026h");
    let last_end = find_last(output, END_SYNC);
    match (last_begin, last_end) {
        (Some(begin), Some(end)) => begin > end,
        (Some(_), None) => true,
        _ => false,
    }
}

fn find_last(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .rposition(|window| window == needle)
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
        assert_eq!(begins, count(&output, END_SYNC));
    }

    #[test]
    fn it_never_ends_inside_a_synchronized_update() {
        // A terminal left inside one draws nothing until its own safety timeout fires, which
        // in xterm.js is a second: a pane showing this would look frozen.
        for len in [8, 100, 2000, 4096, 50_000, 1_000_000] {
            let output = generate(len);
            // Every marker, in the order it appears. The last one must not be the one that
            // begins an update.
            let mut markers = Vec::new();
            for at in 0..output.len() {
                let rest = &output[at..];
                if rest.starts_with(BEGIN_SYNC) {
                    markers.push("begin");
                } else if rest.starts_with(END_SYNC) {
                    markers.push("end");
                }
            }
            assert_ne!(
                markers.last(),
                Some(&"begin"),
                "{len} bytes end inside a synchronized update: {markers:?}"
            );
        }
    }

    #[test]
    fn it_is_coloured() {
        assert!(count(&generate(20_000), b"\x1b[38;5;") > 100);
    }
}
