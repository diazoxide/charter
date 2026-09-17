//! Output shaped like an agent harness's: coloured text, a full repaint every 40 lines, each
//! repaint inside a synchronized-output block (`?2026`). ADR 0018 measured against this shape.

/// Opens and ends a synchronized update. A terminal left inside one draws nothing until its
/// own safety timeout — a second, in xterm.js — so a repaint is either written whole or not
/// written at all, and the output never ends inside one.
const BEGIN_UPDATE: &[u8] = b"\x1b[?2026h\x1b[H\x1b[2J";
const END_UPDATE: &[u8] = b"\x1b[?2026l";

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

/// Exactly `len` bytes of harness-shaped output: whole repaints for as long as another fits,
/// then plain lines, which belong to no update, for whatever is left over. The same `len`
/// always gives the same bytes.
pub fn generate(len: usize) -> Vec<u8> {
    let mut output = Vec::with_capacity(len + 4096);
    let mut line = 0usize;

    // Whole repaints only: one that would not fit is not begun, so no update is ever cut open.
    loop {
        let next = repaint(line);
        if output.len() + next.len() > len {
            break;
        }
        output.extend_from_slice(&next);
        line += LINES_PER_REPAINT;
    }

    // The rest is plain lines, with no escape sequence in them at all, cut to the byte.
    // Colour here would be cut in the middle: an unfinished sequence swallows whatever is
    // written next — a sentinel line after the output, say — into its own parameters.
    while output.len() < len {
        output.extend_from_slice(&plain(line));
        line += 1;
    }
    output.truncate(len);
    output
}

/// One repaint: the whole screen redrawn inside a synchronized update, starting at `from`.
fn repaint(from: usize) -> Vec<u8> {
    let mut block = Vec::from(BEGIN_UPDATE);
    for line in from..from + LINES_PER_REPAINT {
        block.extend_from_slice(&text(line));
    }
    block.extend_from_slice(END_UPDATE);
    block
}

/// A line with nothing in it a cut could break: no escape sequence, no character wider than
/// a byte.
fn plain(line: usize) -> Vec<u8> {
    format!(
        "  {} {} (line {line})\r\n",
        WORDS[line % WORDS.len()],
        WORDS[(line / 3) % WORDS.len()]
    )
    .into_bytes()
}

/// One line of a repaint, coloured.
fn text(line: usize) -> Vec<u8> {
    let colour = 16 + line % 216;
    let words = WORDS[line % WORDS.len()];
    let detail = WORDS[(line / 3) % WORDS.len()];
    format!("\x1b[38;5;{colour}m● {words}\x1b[0m {detail} (line {line})\r\n").into_bytes()
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
    fn no_length_leaves_a_synchronized_update_open_or_unclosed() {
        // A terminal left inside an update draws nothing until its own safety timeout, which
        // in xterm.js is a second: a pane showing that output looks frozen. An update that is
        // ended without being begun is harmless, and is still not this generator's to write.
        for len in (0..5000).chain([50_000, 200_000, 400_000]) {
            let output = generate(len);
            let mut open = 0i32;
            let mut lowest = 0i32;
            for at in 0..output.len() {
                let rest = &output[at..];
                if rest.starts_with(BEGIN_SYNC) {
                    open += 1;
                } else if rest.starts_with(END_SYNC) {
                    open -= 1;
                    lowest = lowest.min(open);
                }
            }
            assert_eq!(open, 0, "{len} bytes end with {open} updates open");
            assert_eq!(lowest, 0, "{len} bytes end an update that was never begun");
        }
    }

    #[test]
    fn it_is_coloured() {
        assert!(count(&generate(20_000), b"\x1b[38;5;") > 100);
    }

    #[test]
    fn no_length_ends_in_the_middle_of_an_escape_sequence() {
        // An unfinished sequence swallows what is written after the output — the sentinel a
        // benchmark waits for — into its own parameters. `BENCH-END` arrived as `ENCH-END`.
        for len in (0..5000).chain([50_000, 200_000, 300_013, 400_000]) {
            let output = generate(len);
            let Some(last) = output.iter().rposition(|byte| *byte == 0x1b) else {
                continue;
            };
            let finished = output[last + 1..]
                .iter()
                .any(|byte| (0x40..=0x7e).contains(byte));
            assert!(
                finished,
                "{len} bytes end inside an escape sequence: {:?}",
                String::from_utf8_lossy(&output[last..])
            );
        }
    }
}
