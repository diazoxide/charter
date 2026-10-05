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
//! the new name with nothing else to remember.
//!
//! # The rules' own name, spelt another way (#1279)
//!
//! A host rule is a glob over the command **as written**, so the `charter` spelling gets a
//! prompt only when the command starts with the bare name the rule spells. Everything else the
//! guard reads as the same command — a path to the binary, another case, a `VAR=` prefix or a
//! wrapper, a quoted or escaped word, a subshell or a substitution — is a string the rule does
//! not match, and would run with no prompt. So a segment that runs a consent-gated command under
//! the rules' name is refused unless its source text is one a rule matches. **Fail-closed**: a
//! spelling the host might also accept is still refused, and the refusal names the one it
//! certainly asks about. The same command inside a string or a heredoc a shell runs is refused
//! under either name, because the rule reads only the outer command.
//!
//! **A handoff is A7's** ([`crate::handoffguard`]), which reads its spelling and its brief more
//! closely than this does, so the rules' name spelling a handoff is not judged here.

use crate::cliname;
use crate::handoffguard::as_the_shell_reads;
use crate::heredoc;
use crate::leakguard;
use crate::proseguard::charter_words;
use crate::pypath;
use crate::scaffold::settings::{CONSENT_PATTERNS, HANDOFF_PATTERN, PROMOTE_PATTERN};
use crate::shellseg;
use crate::shellwrap::{self, base_lower};

/// The trace reason this refusal is tallied under.
pub const REASON: &str = "consent-spelling";

/// The name the consent rules are spelt with until a project is renamed (RN-7).
const RULES_NAME: &str = cliname::ALIAS;

/// A consent-gated command a segment runs: the name it calls the command line by, the words
/// after it, and the rule that asks about it.
struct Gated {
    name: String,
    words: Vec<String>,
    pattern: &'static str,
}

impl Gated {
    /// Spelt under a name the rules do not spell.
    fn under_a_new_name(&self) -> bool {
        self.name != RULES_NAME
    }

    /// Not a handoff under the rules' name, which is A7's to judge.
    fn not_a7s(&self) -> bool {
        self.under_a_new_name() || self.pattern != HANDOFF_PATTERN
    }

    /// What the rule asks about, as a command to type.
    fn as_the_rule_spells_it(&self) -> &'static str {
        match self.pattern {
            HANDOFF_PATTERN => "handoff",
            PROMOTE_PATTERN => "ws todo promote",
            _ => "report … --yes",
        }
    }
}

/// Why `cmd` is refused, or `None`: a segment that runs a consent-gated command the host's
/// rule would not ask about — under a name the rules do not spell, inside a string or a heredoc
/// a shell runs, or under the rules' name spelt other than the rule matches. Read with every
/// heredoc a reader takes as data left out.
pub fn refusal(cmd: &str) -> Option<String> {
    let stripped = heredoc::strip_reader_heredocs(cmd);
    refusal_in(&stripped)
        .or_else(|| in_a_shell_string(&stripped))
        .or_else(|| in_a_shells_heredoc(cmd))
        .or_else(|| not_as_the_rule_spells_it(&stripped))
}

/// The first segment of `text` that runs a consent-gated command `keep` accepts — in the words
/// as written, or in a substitution a `"…"` holds ([`quoted_substitutions`]), which the word
/// reader keeps whole inside its quoted word.
fn gated(text: &str, keep: impl Fn(&Gated) -> bool) -> Option<Gated> {
    gated_in(text, &keep).or_else(|| {
        quoted_substitutions(text)
            .iter()
            .find_map(|inner| gated_in(&lines_it_runs(inner), &keep))
    })
}

/// The text of each `$( … )` and backtick inside double quotes in `text`: the shell runs it,
/// though the word it sits in is quoted ([`shellseg::quote_map`] opens a command context there).
fn quoted_substitutions(text: &str) -> Vec<String> {
    let quoted = shellseg::quote_map(text);
    let chars: Vec<char> = text.chars().collect();
    let quoted_at = |i: usize| quoted.get(i).copied().unwrap_or(true);
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let body = match chars[i] {
            '$' if quoted_at(i) && chars.get(i + 1) == Some(&'(') && !quoted_at(i + 2) => {
                Some(i + 2)
            }
            '`' if quoted_at(i) && !quoted_at(i + 1) => Some(i + 1),
            _ => None,
        };
        let Some(from) = body else {
            i += 1;
            continue;
        };
        // Back in the quoted context one past the closing `)` or backtick.
        let to = (from..chars.len())
            .find(|&k| quoted_at(k))
            .unwrap_or(chars.len());
        out.push(chars[from..to.saturating_sub(1).max(from)].iter().collect());
        i = to.max(i + 1);
    }
    out
}

/// The lines of `text` a shell runs: a heredoc body a reader takes as data — a commit message,
/// a pull request's body — left out ([`leakguard::lines_a_command_could_run`]).
fn lines_it_runs(text: &str) -> String {
    leakguard::lines_a_command_could_run(text)
        .into_iter()
        .map(|(row, _)| row)
        .collect::<Vec<_>>()
        .join("\n")
}

/// [`gated`] over the words as written.
fn gated_in(text: &str, keep: &impl Fn(&Gated) -> bool) -> Option<Gated> {
    shellseg::segment_argv(text).iter().find_map(|toks| {
        let (prog, _env, argv) = shellwrap::split_env(toks);
        let words = charter_words(&prog, &argv)?;
        let name = name_used(&prog, &argv)?;
        let as_the_rule_reads = format!("{RULES_NAME} {}", words.join(" "));
        let pattern = CONSENT_PATTERNS
            .iter()
            .copied()
            .find(|pattern| matches(&as_the_rule_reads, pattern))?;
        let it = Gated {
            name,
            words,
            pattern,
        };
        keep(&it).then_some(it)
    })
}

/// What a command a shell runs from a string or a heredoc body is told, under either name.
fn shell_string_refusal(it: &Gated) -> String {
    let name = &it.name;
    let what = it.as_the_rule_spells_it();
    format!(
        "`{name} {what}` is refused inside a string or a heredoc a shell runs (`sh -c '…'`, \
         `eval`, `bash <<'EOF'`). It waits for your consent through a rule spelt \
         `{RULES_NAME} …`, and the host matches that rule against the outer command only, so \
         nothing would ask you. Run it directly, spelt `{RULES_NAME} {what}`, the bare name at \
         the start of its command."
    )
}

/// One level into each string a segment of `text` hands a shell: a consent-gated command there
/// under either name.
fn in_a_shell_string(text: &str) -> Option<String> {
    let toks = shellseg::split_punctuation(shellseg::lex(text).ok()?);
    heredoc::segments_of(&toks)
        .iter()
        .find_map(|(seg, _before)| {
            let words: Vec<String> = seg.iter().map(|t| t.text.clone()).collect();
            shellwrap::shell_scripts(&words)
                .iter()
                .find_map(|inner| gated(&as_the_shell_reads(inner), Gated::not_a7s))
        })
        .map(|it| shell_string_refusal(&it))
}

/// A consent-gated command under either name on a line a heredoc feeds a shell
/// ([`leakguard::lines_a_command_could_run`], the plan the leak guard and A7 share): the rule
/// saw only the `bash` that opened it.
fn in_a_shells_heredoc(cmd: &str) -> Option<String> {
    leakguard::lines_a_command_could_run(cmd)
        .iter()
        .filter(|(_, in_a_shells_body)| *in_a_shells_body)
        .find_map(|(row, _)| gated(&as_the_shell_reads(row), Gated::not_a7s))
        .map(|it| shell_string_refusal(&it))
}

/// A segment of `text` that runs a consent-gated command under the rules' own name, whose
/// source a rule does not match. Text the lexer cannot read is judged whole, and refused if it
/// holds such a command at all.
fn not_as_the_rule_spells_it(text: &str) -> Option<String> {
    let rules_name = |it: &Gated| !it.under_a_new_name() && it.not_a7s();
    let Ok(toks) = shellseg::lex(text) else {
        return gated(text, rules_name).map(|it| spelling_refusal(&it));
    };
    let toks = shellseg::split_punctuation(toks);
    let chars: Vec<char> = text.chars().collect();
    heredoc::segments_of(&toks)
        .iter()
        .find_map(|(seg, _before)| {
            let (first, last) = (seg.first()?, seg.last()?);
            let Some(source) = source_of(&chars, first.start, last.end) else {
                // No offsets to read the spelling off: judged on the words, and never as asked.
                let words: Vec<String> = seg.iter().map(|t| t.text.clone()).collect();
                return gated(&words.join(" "), rules_name).map(|it| spelling_refusal(&it));
            };
            let it = gated(&source, rules_name)?;
            let asked = CONSENT_PATTERNS
                .iter()
                .any(|pattern| pypath::fnmatch(&source, pattern));
            (!asked).then(|| spelling_refusal(&it))
        })
}

/// The source text between two character offsets, or `None` where the lexer measured neither
/// — read as a spelling no rule matches.
fn source_of(chars: &[char], start: isize, end: isize) -> Option<String> {
    let (start, end) = (usize::try_from(start).ok()?, usize::try_from(end).ok()?);
    chars.get(start..end).map(|c| c.iter().collect())
}

/// What a command under the rules' name, spelt another way, is told.
fn spelling_refusal(it: &Gated) -> String {
    let what = it.as_the_rule_spells_it();
    format!(
        "`{RULES_NAME} {what}` waits for your consent through a rule spelt `{RULES_NAME} …`, and \
         the host matches that rule against the command as written, so this spelling (a path, \
         another case, a prefix or wrapper, a quote, a subshell or a substitution) gets no \
         prompt. Spell it `{RULES_NAME} {what}`, the bare name at the start of its command."
    )
}

/// The refusal for the first segment of `text` that is a consent-gated command under a name the
/// rules do not spell.
fn refusal_in(text: &str) -> Option<String> {
    let it = gated(text, Gated::under_a_new_name)?;
    let (name, words) = (&it.name, &it.words);
    let verb = words.first().map_or("", String::as_str);
    Some(format!(
        "`{name} {verb}` waits for your consent through a rule spelt `{RULES_NAME} …`, \
         which this spelling does not match, so nothing would ask you. Spell it \
         `{RULES_NAME} {verb} …` until this project's rules are renamed."
    ))
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
