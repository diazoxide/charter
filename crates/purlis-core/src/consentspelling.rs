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
//! another case, `python -m` or an assignment in front — and, as #1279 asks of the `charter`
//! spelling, only where the segment's source as written matches the twin's glob. Inside a string,
//! a heredoc or a substitution a shell runs it stays refused, twin or not. A project without the
//! twin, or with it weaker in any file, still refuses: it fails closed.
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

use std::path::Path;

use crate::cliname;
use crate::handoffguard::as_the_shell_reads;
use crate::heredoc;
use crate::leakguard;
use crate::proseguard::charter_words;
use crate::pypath;
use crate::scaffold::settings::{
    self, CONSENT_PATTERNS, HANDOFF_PATTERN, PROMOTE_PATTERN, PURLIS_CONSENT_PATTERNS,
};
use crate::shellseg;
use crate::shellwrap::{self, base_lower};

/// The trace reason this refusal is tallied under.
pub const REASON: &str = "consent-spelling";

/// The name every project's consent rules are spelt with.
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

/// Why `cmd` is refused in the project at `plane`, or `None`: a segment that runs a
/// consent-gated command the host's rule would not ask about — under a name the project's rules
/// do not spell, inside a string or a heredoc a shell runs, or under either name spelt other
/// than its rule matches. Read with every heredoc a reader takes as data left out.
///
/// `anchors` are where the host's settings may come from: the session's start folder and the
/// call's `cwd` ([`settings::twin_in_force`]). None given keeps the new spelling refused.
pub fn refusal(cmd: &str, plane: &Path, anchors: &[&Path]) -> Option<String> {
    let stripped = heredoc::strip_reader_heredocs(cmd);
    let at = At { plane, anchors };
    refusal_in(&stripped, at)
        .or_else(|| in_a_shell_string(&stripped))
        .or_else(|| in_a_shells_heredoc(cmd))
        .or_else(|| not_as_the_rule_spells_it(&stripped))
}

/// Where the call is made: the project, and the directory the host runs it in.
#[derive(Clone, Copy)]
struct At<'a> {
    plane: &'a Path,
    anchors: &'a [&'a Path],
}

/// What a segment runs, as a rule reads it: the program's name and the words after it. The one
/// reading every spelling backstop shares — this one for the project's consent rules, and
/// [`crate::rulespelling`] for the operator's own (#1286).
pub(crate) struct Reading {
    /// The program's file name, case-folded ([`base_lower`]): a path, another case, a `VAR=`
    /// prefix and a wrapper ([`shellwrap::split_env`]) all read as the bare name. For the
    /// command line itself, the name it was called by, `python -m` included ([`name_used`]).
    pub name: String,
    /// The words after the program: for the command line, its words as [`charter_words`] reads
    /// them.
    pub words: Vec<String>,
}

/// [`Reading`] of one segment's words, or `None` for a segment that names no program: the
/// program it finally runs, every wrapper in front of it taken off.
fn reading_of(toks: &[String]) -> Option<Reading> {
    let (prog, _env, argv) = shellwrap::split_env(toks);
    reading_of_argv(&prog, &argv)
}

/// [`Reading`] of a program and its argv.
fn reading_of_argv(prog: &str, argv: &[String]) -> Option<Reading> {
    if let Some(words) = charter_words(prog, argv) {
        let name = name_used(prog, argv)?;
        return Some(Reading { name, words });
    }
    let name = base_lower(prog);
    (!name.is_empty()).then(|| Reading {
        name,
        words: argv.iter().skip(1).cloned().collect(),
    })
}

/// Every [`Reading`] of one segment's words that `wanted` asks for by name: each wrapper the
/// shell runs on the way ([`shellwrap::Invocation::layers`] — `sudo`, `env`, `timeout`, the
/// command line's `secret exec`), outermost first, then the program it finally runs, which is
/// always read. A wrapper's reading is built only when `wanted` names it, so a long chain of
/// wrappers no rule names costs nothing more than reading it. `true` beside them when more
/// wrappers stand in front than are read ([`shellwrap::MAX_LAYERS`]).
fn readings_of(toks: &[String], wanted: &impl Fn(&str) -> bool) -> (Vec<Reading>, bool) {
    let it = shellwrap::split_env_chdir(toks);
    let mut out: Vec<Reading> = (0..it.layers.len())
        .filter(|&k| {
            it.layer_program(k)
                .is_some_and(|prog| wanted(&base_lower(prog)))
        })
        .filter_map(|k| {
            let argv = it.layer(k);
            reading_of_argv(argv.first()?, &argv)
        })
        .collect();
    out.extend(reading_of_argv(&it.prog, &it.argv));
    (out, it.too_deep)
}

/// Every [`Reading`] in `text` ([`readings_of`], with `wanted` choosing the wrapper layers):
/// each segment's, and each substitution's a `"…"` holds ([`quoted_substitutions`]), less any
/// heredoc body a reader takes as data. `true` beside them when a segment has more wrappers in
/// front of its program than are read.
pub(crate) fn every_reading_in(text: &str, wanted: &impl Fn(&str) -> bool) -> (Vec<Reading>, bool) {
    let mut out: Vec<Reading> = Vec::new();
    let mut too_deep = false;
    let mut read = |text: &str| {
        for toks in shellseg::segment_argv(text) {
            let (readings, deep) = readings_of(&toks, wanted);
            out.extend(readings);
            too_deep |= deep;
        }
    };
    read(text);
    for inner in quoted_substitutions(text) {
        read(&lines_it_runs(&inner));
    }
    (out, too_deep)
}

/// What `find` makes of the first segment of `text` it answers for — in the words as written, or
/// in a substitution a `"…"` holds ([`quoted_substitutions`]).
fn found<T>(text: &str, find: &impl Fn(&Reading) -> Option<T>) -> Option<T> {
    found_in(text, find).or_else(|| {
        quoted_substitutions(text)
            .iter()
            .find_map(|inner| found_in(&lines_it_runs(inner), find))
    })
}

/// [`found`] over the words as written.
fn found_in<T>(text: &str, find: &impl Fn(&Reading) -> Option<T>) -> Option<T> {
    shellseg::segment_argv(text)
        .iter()
        .find_map(|toks| find(&reading_of(toks)?))
}

/// The text of each `$( … )` and backtick inside double quotes in `text`: the shell runs it,
/// though the word it sits in is quoted ([`shellseg::quote_map`] opens a command context there).
fn quoted_substitutions(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let found = closes(&chars, &body_chars(text));
    // Read lazily: only a substitution left open falls back to it.
    let mut quoted: Option<Vec<bool>> = None;
    let mut out = Vec::new();
    let mut taken_to = 0usize;
    for &from in &found.quoted {
        if from < taken_to {
            continue; // inside a body already taken
        }
        // Balanced, the body runs to its close. Never closed — which a shell refuses to run at
        // all — it runs only to where the quote map is back in the quoted context, so a crafted
        // line of unclosed substitutions is not read again from each one to its end.
        let to = found.closes.get(&from).copied().unwrap_or_else(|| {
            let quoted = quoted.get_or_insert_with(|| shellseg::quote_map(text));
            (from..chars.len())
                .find(|&k| quoted.get(k).copied().unwrap_or(true))
                .unwrap_or(chars.len())
                .saturating_sub(1)
                .max(from)
        });
        out.push(chars[from..to].iter().collect());
        taken_to = to + 1;
    }
    out
}

/// What [`closes`] finds: where each substitution closes, keyed by where its body starts, and
/// the body starts of the ones that open inside double quotes, in order.
#[derive(Default)]
struct Closes {
    closes: std::collections::HashMap<usize, usize>,
    quoted: Vec<usize>,
}

/// Which characters of `text` stand on a heredoc body's line or its terminator's
/// ([`heredoc::heredoc_layout`], the walk every guard reads bodies with): text, not brackets or
/// quotes, to the shells that end the body only at its delimiter.
fn body_chars(text: &str) -> Vec<bool> {
    if !text.contains("<<") {
        return Vec::new();
    }
    let layout = heredoc::heredoc_layout(text);
    // Read only where the layout is the text, line for line; otherwise no body is skipped.
    if !layout.iter().map(|l| l.text.as_str()).eq(text.split('\n')) {
        return Vec::new();
    }
    let mut out = Vec::new();
    for (k, line) in layout.iter().enumerate() {
        if k > 0 {
            out.push(false); // the newline
        }
        out.extend(std::iter::repeat_n(line.body, line.text.chars().count()));
    }
    out
}

/// Where each command substitution in `chars` closes, keyed by where its body starts: the `)`
/// that balances a `$(`, or the backtick that ends one, with quotes inside a body read as
/// quotes — so `'rm'` or `"a)b"` inside it does not end it early — and a heredoc body
/// (`in_body`) and a `${ … }` stepped over whole, so a quote or a `)` in either does not either.
/// One pass over the text, a stack of the contexts it is in, so the cost grows with the text and
/// never with how many substitutions are left open. One left open has no entry.
fn closes(chars: &[char], in_body: &[bool]) -> Closes {
    /// A context the pass is inside.
    enum Ctx {
        Single,
        Double,
        /// A `${ … }`.
        Brace,
        /// A `$(` whose body starts here.
        Sub(usize),
        /// A plain `(` inside a command context.
        Group,
        /// A backtick whose body starts here.
        Tick(usize),
    }
    let mut out = Closes::default();
    let mut stack: Vec<Ctx> = Vec::new();
    let mut k = 0;
    while k < chars.len() {
        if in_body.get(k).copied().unwrap_or(false) {
            k += 1;
            continue;
        }
        let c = chars[k];
        let next = chars.get(k + 1).copied();
        match stack.last() {
            Some(Ctx::Single) => {
                if c == '\'' {
                    stack.pop();
                }
            }
            Some(Ctx::Tick(from)) => match c {
                '\\' => k += 1,
                '`' => {
                    out.closes.insert(*from, k);
                    stack.pop();
                }
                _ => {}
            },
            Some(Ctx::Double) => match c {
                '\\' => k += 1,
                '"' => {
                    stack.pop();
                }
                '$' if next == Some('(') => {
                    stack.push(Ctx::Sub(k + 2));
                    out.quoted.push(k + 2);
                    k += 1;
                }
                '$' if next == Some('{') => {
                    stack.push(Ctx::Brace);
                    k += 1;
                }
                '`' => {
                    stack.push(Ctx::Tick(k + 1));
                    out.quoted.push(k + 1);
                }
                _ => {}
            },
            Some(Ctx::Brace) => match c {
                '\\' => k += 1,
                '}' => {
                    stack.pop();
                }
                '\'' => stack.push(Ctx::Single),
                '"' => stack.push(Ctx::Double),
                '`' => stack.push(Ctx::Tick(k + 1)),
                '$' if next == Some('(') => {
                    stack.push(Ctx::Sub(k + 2));
                    k += 1;
                }
                '$' if next == Some('{') => {
                    stack.push(Ctx::Brace);
                    k += 1;
                }
                _ => {}
            },
            // A command context: the top level, a substitution's body or a group in one.
            _ => match c {
                '\\' => k += 1,
                '\'' => stack.push(Ctx::Single),
                '"' => stack.push(Ctx::Double),
                '`' => stack.push(Ctx::Tick(k + 1)),
                '$' if next == Some('(') => {
                    stack.push(Ctx::Sub(k + 2));
                    k += 1;
                }
                '$' if next == Some('{') => {
                    stack.push(Ctx::Brace);
                    k += 1;
                }
                '(' if !stack.is_empty() => stack.push(Ctx::Group),
                ')' => {
                    // A group's `)`, or one at the top level that closes nothing, records none.
                    if let Some(Ctx::Sub(from)) = stack.pop() {
                        out.closes.insert(from, k);
                    }
                }
                _ => {}
            },
        }
        k += 1;
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
    found_in(text, &|it: &Reading| consent_gated(it, keep))
}

/// The consent-gated command `it` runs under the command line's name, if `keep` accepts it.
fn consent_gated(it: &Reading, keep: &impl Fn(&Gated) -> bool) -> Option<Gated> {
    if !cliname::is_installed(&it.name) {
        return None;
    }
    let as_the_rule_reads = format!("{RULES_NAME} {}", it.words.join(" "));
    let pattern = CONSENT_PATTERNS
        .iter()
        .copied()
        .find(|pattern| matches(&as_the_rule_reads, pattern))?;
    let it = Gated {
        name: it.name.clone(),
        words: it.words.clone(),
        pattern,
    };
    keep(&it).then_some(it)
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
    let find = |it: &Reading| consent_gated(it, &Gated::not_a7s);
    in_a_shell_string_by(text, &|text: &str| found(text, &find)).map(|it| shell_string_refusal(&it))
}

/// What `look` finds one level into a string a segment of `text` hands a shell to run
/// (`sh -c '…'`, `eval '…'`, [`shellwrap::shell_scripts`]): the host's rule reads only the
/// outer command.
pub(crate) fn in_a_shell_string_by<T>(text: &str, look: &impl Fn(&str) -> Option<T>) -> Option<T> {
    let toks = shellseg::split_punctuation(shellseg::lex(text).ok()?);
    heredoc::segments_of(&toks)
        .iter()
        .find_map(|(seg, _before)| {
            let words: Vec<String> = seg.iter().map(|t| t.text.clone()).collect();
            shellwrap::shell_scripts(&words)
                .iter()
                .find_map(|inner| look(&as_the_shell_reads(inner)))
        })
}

/// A consent-gated command under either name on a line a heredoc feeds a shell
/// ([`leakguard::lines_a_command_could_run`], the plan the leak guard and A7 share): the rule
/// saw only the `bash` that opened it.
fn in_a_shells_heredoc(cmd: &str) -> Option<String> {
    let find = |it: &Reading| consent_gated(it, &Gated::not_a7s);
    in_a_shells_heredoc_by(cmd, &|text: &str| found(text, &find))
        .map(|it| shell_string_refusal(&it))
}

/// What `look` finds on a line a heredoc feeds a shell (`bash <<'EOF'`, `cat <<EOF | sh`).
pub(crate) fn in_a_shells_heredoc_by<T>(cmd: &str, look: &impl Fn(&str) -> Option<T>) -> Option<T> {
    leakguard::lines_a_command_could_run(cmd)
        .iter()
        .filter(|(_, in_a_shells_body)| *in_a_shells_body)
        .find_map(|(row, _)| look(&as_the_shell_reads(row)))
}

/// A segment of `text` that runs a consent-gated command under the rules' own name, whose
/// source a rule does not match.
fn not_as_the_rule_spells_it(text: &str) -> Option<String> {
    let find =
        |it: &Reading| consent_gated(it, &|it: &Gated| !it.under_a_new_name() && it.not_a7s());
    let asked = |source: &str, _: &Gated| {
        CONSENT_PATTERNS
            .iter()
            .any(|pattern| pypath::fnmatch(source, pattern))
    };
    not_as_written_by(text, &|text: &str| found(text, &find), asked)
        .map(|(it, _)| spelling_refusal(&it))
}

/// What `look` finds in the first segment of `text` whose source as written `asked` says no
/// rule of the host's matches, with that source. Text the lexer cannot read, or a segment it
/// measured no offsets for, is judged on its words and never as asked: it fails closed.
pub(crate) fn not_as_written_by<T>(
    text: &str,
    look: &impl Fn(&str) -> Option<T>,
    asked: impl Fn(&str, &T) -> bool,
) -> Option<(T, String)> {
    let Ok(toks) = shellseg::lex(text) else {
        return look(text).map(|it| (it, String::new()));
    };
    let toks = shellseg::split_punctuation(toks);
    let chars: Vec<char> = text.chars().collect();
    heredoc::segments_of(&toks)
        .iter()
        .find_map(|(seg, _before)| {
            let (first, last) = (seg.first()?, seg.last()?);
            let Some(source) = source_of(&chars, first.start, last.end) else {
                let words: Vec<String> = seg.iter().map(|t| t.text.clone()).collect();
                return look(&words.join(" ")).map(|it| (it, String::new()));
            };
            let it = look(&source)?;
            (!asked(&source, &it)).then_some((it, source))
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
/// project's rules do not spell. The new name is let through only for a segment spelt as the
/// project's twin rule starts and matches ([`spelt_as_a_twin`]) where that twin is in force
/// ([`ruled_under`]); found inside a `"$( … )"` it is refused under any spelling, since the
/// host's rule reads only the outer command.
fn refusal_in(text: &str, at: At<'_>) -> Option<String> {
    let lifted = |it: &Gated| {
        it.name == cliname::PRIMARY && spelt_as_a_twin(text) && ruled_under(&it.words, at)
    };
    let it = gated_in(text, &|it: &Gated| it.under_a_new_name() && !lifted(it)).or_else(|| {
        quoted_substitutions(text)
            .iter()
            .find_map(|inner| gated_in(&lines_it_runs(inner), &Gated::under_a_new_name))
    })?;
    let (name, words) = (&it.name, &it.words);
    let verb = words.first().map_or("", String::as_str);
    Some(format!(
        "`{name} {verb}` waits for your consent through a rule spelt `{RULES_NAME} …`, \
         which this spelling does not match, so nothing would ask you. Spell it \
         `{RULES_NAME} {verb} …` until this project's rules name `{primary}` too \
         (`{RULES_NAME} doctor --fix rename-plane` writes them).",
        primary = cliname::PRIMARY,
    ))
}

/// Whether the host asks before the command `words` under the new name, in the project at
/// `at`: the project carries the twin of every consent rule the command meets, in every
/// harness.
fn ruled_under(words: &[String], at: At<'_>) -> bool {
    let as_the_rule_reads = format!("{RULES_NAME} {}", words.join(" "));
    CONSENT_PATTERNS
        .iter()
        .filter(|pattern| matches(&as_the_rule_reads, pattern))
        .all(|pattern| settings::twin_in_force(at.plane, at.anchors, pattern, cliname::PRIMARY))
}

/// Whether every segment of `text` that runs the command line under the new name is spelt as a
/// twin rule matches it. It starts with that name as its SOURCE spells it: the first word, bare
/// (no quote or escape touched it), and its characters in the source exactly `purlis`.
/// `'purlis'`, `\purlis` and `pur''lis` lex to the same word and match no host rule, so they stay
/// refused (D-RN7-4). And a consent-gated one's source, as written, matches a twin's glob, as
/// #1279 asks of the `charter` spelling: `purlis 'report' … --yes` or `--y\es` is refused. A
/// line that does not lex answers `false`.
fn spelt_as_a_twin(text: &str) -> bool {
    let Ok(toks) = shellseg::lex(text) else {
        return false;
    };
    let toks = shellseg::split_punctuation(toks);
    let chars: Vec<char> = text.chars().collect();
    heredoc::segments_of(&toks).iter().all(|(seg, _before)| {
        let (Some(first), Some(last)) = (seg.first(), seg.last()) else {
            return true;
        };
        let words: Vec<String> = seg.iter().map(|t| t.text.clone()).collect();
        let (prog, _env, argv) = shellwrap::split_env(&words);
        let source = source_of(&chars, first.start, last.end);
        // Read over the source, so a command in a `$( … )` or `( … )` the segment holds counts.
        let gated = gated_in(
            source.as_deref().unwrap_or(&words.join(" ")),
            &Gated::under_a_new_name,
        )
        .is_some();
        let renamed = gated || name_used(&prog, &argv).is_some_and(|name| name != RULES_NAME);
        if !renamed {
            return true;
        }
        let bare = first.bare
            && first.text == cliname::PRIMARY
            && crate::handoffguard::source_of(&chars, first.start, first.end) == cliname::PRIMARY;
        bare && (!gated
            || source.is_some_and(|source| {
                PURLIS_CONSENT_PATTERNS
                    .iter()
                    .any(|pattern| pypath::fnmatch(&source, pattern))
            }))
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
pub(crate) fn matches(line: &str, pattern: &str) -> bool {
    pypath::fnmatch(line, pattern)
        || pattern
            .strip_suffix(" *")
            .is_some_and(|bare| pypath::fnmatch(line, bare))
}
