//! The caps a command meets before any guard reads it (#1355).
//!
//! A harness gives a `PreToolUse` hook a fixed time, and whether it lets the tool call through
//! when that time runs out is the harness's choice. Every guard's cost is meant to grow with the
//! command's length, but reviews kept finding shapes whose cost grew much faster: substitutions
//! nested inside quotes inside substitutions, chains of wrapper programs. So the command is
//! measured first, each cap in time linear in its length, and refused with one sentence when it
//! is past one:
//!
//! - **Its size**, [`MAX_COMMAND_BYTES`]. Chosen so an ASCII pull request body at a forge's
//!   largest (GitHub's 65,536 characters) still fits in a heredoc, with room to spare. A body
//!   in a script that takes more bytes a character is longer than that, and its route out is a
//!   file: `--body-file`, `git commit -F`.
//! - **How deeply its substitutions nest**, [`MAX_NESTING`], as
//!   [`shellseg::substitution_depth`] reads it. Reading a line's substitutions costs its length
//!   times this, so bounding it is what keeps the reading linear. A string a guard derives from
//!   the command and reads in turn is held to it too: a reading that meets one nested deeper
//!   fails closed ([`shellseg::too_deep_within`]).
//! - **How many wrapper programs stand in front of one program**, [`MAX_LAYERS`]: `env`,
//!   `nice`, `eval`, a shell's `-c` and the rest, each a layer a guard reads through to find the
//!   program. Counted in command position, segment by segment, so a word in quoted prose is
//!   never a layer.
//!
//! The caps are the cheap first line. The hook's time budget is the backstop behind them: a
//! shape that slips past every cap and is still slow is refused when the budget runs out, never
//! timed out by the harness (`purlis-cli`'s `guard.rs`).
//!
//! The caps refuse; they never allow. A command inside them is read by every guard as before.

use crate::{shellseg, shellwrap};

/// The trace reason a command refused here is tallied under.
pub const REASON: &str = "too-big-to-check";

/// The longest command, in bytes, a guard reads.
///
/// Measured (#1355): a 65,536-character ASCII pull request body in a quoted heredoc is under
/// 70 KB, so this holds an ASCII body at a forge's largest with room to spare. A body in a
/// script of two, three or four bytes a character is not held at that length, and the refusal
/// names the route out, a file the forge CLI reads (`--body-file`). Every guard's cost is
/// linear in the size up to here, and `every_guard_reads_a_hostile_command_in_linear_time`
/// holds the whole verdict on the slowest shapes known, at this size and at the nesting cap,
/// well inside the hook's budget.
pub const MAX_COMMAND_BYTES: usize = 128 * 1024;

/// The deepest substitutions may nest. A commit message in `"$(cat <<'EOF'` is one level; the
/// deepest any of this repository's 3,000 latest commit messages reaches in a heredoc is 3
/// (measured for #1355), so 8 leaves room for a real command and none for a crafted one. It is
/// also as far out as [`shellseg::joined_segments`] hands a token, which is what keeps that
/// reading linear for any string; a reading that would have to go further fails closed.
pub const MAX_NESTING: usize = 8;

/// The most wrapper programs one program is run through — the cap the guards already held per
/// segment ([`shellwrap::split_env_chdir`]'s layers, #1286), with `eval` and a shell's `-c`
/// counted as layers too, applied before any guard parses the command.
pub const MAX_LAYERS: usize = 64;

/// How many commands deep the release floor reads a command inside a command: a shell's
/// script, an alias's expansion, the words fed to a shell's stdin (#866).
pub const MAX_COMMAND_DEPTH: usize = 16;

/// Whether one segment runs its program through more than [`MAX_LAYERS`] layers: the wrappers
/// [`shellwrap::split_env_chdir`] reads through, then `eval` or a shell's `-c`, then the
/// wrappers after that, each standing where the program would. Only words in command position
/// count, so prose a command carries in quotes is one word and never a layer.
///
/// Linear in the segment: every round takes one layer at least, and it stops past the cap.
fn too_many_layers(segment: &[String]) -> bool {
    let mut layers = 0usize;
    let mut at = 0usize;
    loop {
        let it = shellwrap::split_env_chdir(&segment[at..]);
        layers += it.layers.len();
        if it.too_deep || layers > MAX_LAYERS {
            return true;
        }
        at += it.taken.len();
        let base = shellwrap::base_lower(&it.prog);
        let past = if base == "eval" {
            1
        } else if shellwrap::STRING_SHELLS.contains(&base.as_str()) {
            // The shell's own options, and among them a `-c`, alone or in a cluster (`-lc`).
            let options = it
                .argv
                .iter()
                .skip(1)
                .take_while(|w| w.starts_with('-') || w.starts_with('+'))
                .count();
            let runs_a_string = it.argv[1..=options]
                .iter()
                .any(|w| w.starts_with('-') && !w.starts_with("--") && w.contains('c'));
            if !runs_a_string {
                return false;
            }
            1 + options
        } else {
            return false;
        };
        layers += 1;
        if layers > MAX_LAYERS {
            return true;
        }
        at += past;
    }
}

/// The sentence a command nested too deeply to read is refused with.
pub fn too_deep_refusal() -> String {
    format!(
        "this command's substitutions are too deeply nested for purlis to check in time (the \
         most it reads is {MAX_NESTING} levels), so split it into smaller commands, or pass \
         long text with `--body-file` / `git commit -F <file>`."
    )
}

/// Why `cmd` is too big for the guards to read in time, or `None` when it is inside every cap.
pub fn refusal(cmd: &str) -> Option<String> {
    if cmd.len() > MAX_COMMAND_BYTES {
        return Some(format!(
            "this command is too long for purlis to check in time ({} bytes; the most it reads \
             is {MAX_COMMAND_BYTES}), so split it into smaller commands, or put the long text \
             in a file and pass the file: a pull request or issue body with `--body-file`, a \
             commit message with `git commit -F`.",
            cmd.len()
        ));
    }
    let (segments, depth) = shellseg::segments_and_depth(cmd);
    if depth > MAX_NESTING {
        return Some(too_deep_refusal());
    }
    if segments.iter().any(|segment| too_many_layers(segment)) {
        return Some(format!(
            "this command runs its program through more than {MAX_LAYERS} wrapper programs \
             (`env`, `nice`, `eval`, a shell's `-c` and the like), which is too deeply nested \
             for purlis to check in time, so split it into smaller commands."
        ));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_commit_heredoc_is_one_level_deep() {
        let cmd = "git commit -m \"$(cat <<'EOF'\nIt's done (see #1).\nEOF\n)\"";
        assert!(
            shellseg::substitution_depth(cmd) <= 4,
            "{}",
            shellseg::substitution_depth(cmd)
        );
        assert_eq!(refusal(cmd), None);
    }

    #[test]
    fn depth_counts_substitutions_open_at_once_and_not_ones_in_a_row() {
        assert_eq!(shellseg::substitution_depth("echo $(a $(b <(c)))"), 3);
        assert_eq!(shellseg::substitution_depth("echo $(a) $(b) $(c)"), 1);
        assert_eq!(shellseg::substitution_depth("echo '$(a $(b))'"), 0);
    }

    /// Words split from a line: what [`too_many_layers`] is handed for one segment.
    fn words(line: &str) -> Vec<String> {
        line.split_whitespace().map(str::to_owned).collect()
    }

    #[test]
    fn layers_are_counted_in_command_position_and_only_there() {
        let at_the_cap = format!("{}ls", "env ".repeat(MAX_LAYERS));
        assert!(!too_many_layers(&words(&at_the_cap)));
        let past_it = format!("env {at_the_cap}");
        assert!(too_many_layers(&words(&past_it)));
        // `eval` and a shell's `-c`, in any cluster, are layers as a wrapper is.
        let mixed = format!("{}ls", "eval nice bash -lc ".repeat(MAX_LAYERS / 3 + 1));
        assert!(too_many_layers(&words(&mixed)));
        // Words that are not where a program stands are not layers.
        let prose = format!("echo {}", "env nice eval bash -c ".repeat(MAX_LAYERS));
        assert!(!too_many_layers(&words(&prose)));
    }
}
