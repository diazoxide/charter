//! A consent-gated command spelt under the command line's new name (RN-3, #1255; D-RN3-9).
//!
//! The host asks the operator before the commands a project's consent rules name
//! ([`CONSENT_PATTERNS`]: a handoff, a report filed with `--yes`, a todo promoted to a forge).
//! Those rules were spelt `charter …` alone until RN-7. The same command spelt `purlis …`
//! matches none of them, so in a project that carries only those the host would run it with no
//! prompt at all. This refuses it there, with the spelling to use instead.
//!
//! **Lifted where the project carries the purlis rule (RN-7, closing D-RN3-4 and D-RN3-9).**
//! `init`, `reinit` and the `rename-plane` fix write each rule's `purlis …` twin beside it. The
//! `purlis` spelling is let through for a command whose twin holds it at least as strictly as
//! the `charter` rule, in every settings file the host reads for the call
//! ([`settings::twin_in_force`]), and only spelt exactly as the twin starts — the bare word
//! `purlis` at the start of its segment, unquoted and unescaped in the source; not a path,
//! another case, `python -m` or an assignment in front. A project without the twin, or with
//! it weaker in any file, still refuses: it fails closed.
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

use std::path::Path;

use crate::cliname;
use crate::handoffguard::as_the_shell_reads;
use crate::heredoc;
use crate::proseguard::charter_words;
use crate::pypath;
use crate::scaffold::settings::{self, CONSENT_PATTERNS};
use crate::shellseg;
use crate::shellwrap::{self, base_lower};

/// The trace reason this refusal is tallied under.
pub const REASON: &str = "consent-spelling";

/// The name every project's consent rules are spelt with.
const RULES_NAME: &str = cliname::ALIAS;

/// Why `cmd` is refused in the project at `plane`, or `None`: a segment that runs the command
/// line under a name the project's consent rules do not spell, as a command one of them would
/// ask about — in the call itself, with the heredocs a reader takes as data left out, or in a
/// string a shell it starts runs.
///
/// `anchors` are where the host's settings may come from: the session's start folder and the
/// call's `cwd` ([`settings::twin_in_force`]). None given keeps the new spelling refused.
pub fn refusal(cmd: &str, plane: &Path, anchors: &[&Path]) -> Option<String> {
    let stripped = heredoc::strip_reader_heredocs(cmd);
    let at = At { plane, anchors };
    refusal_in(&stripped, at).or_else(|| in_a_shell_string(&stripped, at))
}

/// Where the call is made: the project, and the directory the host runs it in.
#[derive(Clone, Copy)]
struct At<'a> {
    plane: &'a Path,
    anchors: &'a [&'a Path],
}

/// [`refusal_in`] over each string a segment of `text` hands a shell, one level deep.
fn in_a_shell_string(text: &str, at: At<'_>) -> Option<String> {
    let toks = shellseg::split_punctuation(shellseg::lex(text).ok()?);
    heredoc::segments_of(&toks)
        .iter()
        .find_map(|(seg, _before)| {
            let words: Vec<String> = seg.iter().map(|t| t.text.clone()).collect();
            shellwrap::shell_scripts(&words)
                .iter()
                .find_map(|inner| refusal_in(&as_the_shell_reads(inner), at))
        })
}

/// The refusal for the first segment of `text` that is a consent-gated command under a name the
/// project's rules do not spell.
fn refusal_in(text: &str, at: At<'_>) -> Option<String> {
    shellseg::segment_argv(text).iter().find_map(|toks| {
        let (prog, _env, argv) = shellwrap::split_env(toks);
        let words = charter_words(&prog, &argv)?;
        let name = name_used(&prog, &argv)?;
        if name == RULES_NAME {
            return None;
        }
        let as_the_rule_reads = format!("{RULES_NAME} {}", words.join(" "));
        let gated: Vec<&str> = CONSENT_PATTERNS
            .iter()
            .copied()
            .filter(|pattern| matches(&as_the_rule_reads, pattern))
            .collect();
        if gated.is_empty() || (spelt_bare(text) && ruled_under(&prog, &gated, at)) {
            return None;
        }
        let verb = words.first().map_or("", String::as_str);
        Some(format!(
            "`{name} {verb}` waits for your consent through a rule spelt `{RULES_NAME} …`, \
             which this spelling does not match, so nothing would ask you. Spell it \
             `{RULES_NAME} {verb} …` until this project's rules name `{primary}` too \
             (`{RULES_NAME} doctor --fix rename-plane` writes them).",
            primary = cliname::PRIMARY,
        ))
    })
}

/// Whether the host asks before the command under the spelling `prog`, in the project at
/// `plane`: `prog` is the bare new name, exactly as a twin rule starts, and the project carries
/// the twin of every consent rule the command meets, in every harness.
fn ruled_under(prog: &str, gated: &[&str], at: At<'_>) -> bool {
    prog == cliname::PRIMARY
        && gated
            .iter()
            .all(|pattern| settings::twin_in_force(at.plane, at.anchors, pattern, cliname::PRIMARY))
}

/// Whether every segment of `text` that runs the command line under the new name starts with
/// that name as its SOURCE spells it: the first word, bare (no quote or escape touched it), and
/// its characters in the source exactly `purlis`. `'purlis'`, `\purlis` and `pur''lis` lex to the
/// same word and match no host rule, so they stay refused (D-RN7-4). A line that does not lex
/// answers `false`.
fn spelt_bare(text: &str) -> bool {
    let Ok(toks) = shellseg::lex(text) else {
        return false;
    };
    let toks = shellseg::split_punctuation(toks);
    let chars: Vec<char> = text.chars().collect();
    heredoc::segments_of(&toks).iter().all(|(seg, _before)| {
        let words: Vec<String> = seg.iter().map(|t| t.text.clone()).collect();
        let (prog, _env, argv) = shellwrap::split_env(&words);
        let renamed = name_used(&prog, &argv).is_some_and(|name| name != RULES_NAME);
        if !renamed {
            return true;
        }
        seg.first().is_some_and(|first| {
            first.bare
                && first.text == cliname::PRIMARY
                && crate::handoffguard::source_of(&chars, first.start, first.end)
                    == cliname::PRIMARY
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
