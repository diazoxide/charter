//! A consent-gated command spelt under the command line's new name (RN-3, #1255; D-RN3-9).
//!
//! The host asks the operator before the commands a project's consent rules name
//! ([`CONSENT_PATTERNS`]: a handoff, a report filed with `--yes`, a todo promoted to a forge).
//! Those rules are spelt `charter …`, and they stay that way until the project is renamed
//! (RN-7). The same command spelt `purlis …` matches none of them, so the host would run it with
//! no prompt at all. This refuses it, with the spelling to use instead, until RN-7 writes rules
//! that name `purlis` and this list of refused names empties.
//!
//! **Read as A7 reads a handoff**: over the call with every heredoc a reader takes as data
//! stripped ([`heredoc::strip_reader_heredocs`]), so a commit message or a note that mentions
//! one of these commands is not refused, while a body a shell runs still is; and one level into
//! the string a `sh -c`, `bash -c` or `eval` runs ([`shellwrap::shell_scripts`]), as
//! [`crate::handoffguard::shell_string_handoff`] reads it.
//!
//! **Read off the table the rules are written from**, so a rule added there is refused under
//! the new name with nothing else to remember. **Only the names the rules do not spell**: the
//! `charter` spelling is the host's to judge, and is not asked about here.

use crate::cliname;
use crate::handoffguard::as_the_shell_reads;
use crate::heredoc;
use crate::proseguard::charter_words;
use crate::pypath;
use crate::scaffold::settings::CONSENT_PATTERNS;
use crate::shellseg;
use crate::shellwrap::{self, base_lower};

/// The trace reason this refusal is tallied under.
pub const REASON: &str = "consent-spelling";

/// The name the consent rules are spelt with until a project is renamed (RN-7).
const RULES_NAME: &str = cliname::ALIAS;

/// Why `cmd` is refused, or `None`: a segment that runs the command line under a name the
/// consent rules do not spell, as a command one of them would ask about — in the call itself,
/// with the heredocs a reader takes as data left out, or in a string a shell it starts runs.
pub fn refusal(cmd: &str) -> Option<String> {
    let stripped = heredoc::strip_reader_heredocs(cmd);
    refusal_in(&stripped).or_else(|| in_a_shell_string(&stripped))
}

/// [`refusal_in`] over each string a segment of `text` hands a shell, one level deep.
fn in_a_shell_string(text: &str) -> Option<String> {
    let toks = shellseg::split_punctuation(shellseg::lex(text).ok()?);
    heredoc::segments_of(&toks)
        .iter()
        .find_map(|(seg, _before)| {
            let words: Vec<String> = seg.iter().map(|t| t.text.clone()).collect();
            shellwrap::shell_scripts(&words)
                .iter()
                .find_map(|inner| refusal_in(&as_the_shell_reads(inner)))
        })
}

/// The refusal for the first segment of `text` that is a consent-gated command under a name the
/// rules do not spell.
fn refusal_in(text: &str) -> Option<String> {
    shellseg::segment_argv(text).iter().find_map(|toks| {
        let (prog, _env, argv) = shellwrap::split_env(toks);
        let words = charter_words(&prog, &argv)?;
        let name = name_used(&prog, &argv)?;
        if name == RULES_NAME {
            return None;
        }
        let as_the_rule_reads = format!("{RULES_NAME} {}", words.join(" "));
        let gated = CONSENT_PATTERNS
            .iter()
            .any(|pattern| matches(&as_the_rule_reads, pattern));
        gated.then(|| {
            let verb = words.first().map_or("", String::as_str);
            format!(
                "`{name} {verb}` waits for your consent through a rule spelt `{RULES_NAME} …`, \
                 which this spelling does not match, so nothing would ask you. Spell it \
                 `{RULES_NAME} {verb} …` until this project's rules are renamed."
            )
        })
    })
}

/// The name the segment calls the command line by, case-folded: its program's file name, or
/// the module after `python -m`.
fn name_used(prog: &str, argv: &[String]) -> Option<String> {
    let base = base_lower(prog);
    if cliname::is_installed(&base) {
        return Some(base);
    }
    let at = argv.iter().position(|a| a == "-m")?;
    argv.get(at + 1).map(|module| module.to_lowercase())
}

/// Whether `line` is one the host's glob `pattern` asks about. A trailing ` *` also matches
/// the bare command, the wider reading, because asking once too often is the safe side.
fn matches(line: &str, pattern: &str) -> bool {
    pypath::fnmatch(line, pattern)
        || pattern
            .strip_suffix(" *")
            .is_some_and(|bare| pypath::fnmatch(line, bare))
}
