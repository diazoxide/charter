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
//! one of these commands is not refused, while a body a shell runs still is; and in the string
//! a `sh -c`, `bash -c` or `eval` runs ([`shellwrap::shell_scripts`]) or a heredoc feeds a
//! shell, at every level up to the nesting cap, as the handoff guard reads them (#1426,
//! [`in_a_string_a_shell_runs_by`]).
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
use crate::floorguard;
use crate::guardcaps;
use crate::handoffguard::as_the_shell_reads;
use crate::heredoc::{self, Heredocs};
use crate::leakguard;
use crate::proseguard::charter_words;
use crate::pypath;
use crate::scaffold::settings::{
    self, CONSENT_PATTERNS, HANDOFF_PATTERN, PROMOTE_PATTERN, PURLIS_CONSENT_PATTERNS,
};
use crate::shellseg;
use crate::shellsubst::{Inward, every_substitution, scan};
use crate::shellwrap::{self, base_lower};
use std::collections::HashSet;

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
///
/// **Every substitution a shell runs, at every depth** (#1417), is read as a command of its own
/// ([`every_substitution`], the leak guard's scanner): in double quotes, a `${ … }`, a backtick
/// or an expanding heredoc body, however deep it nests. Past the walk's bounds the call is
/// refused as too big to check, never read in part.
pub fn refusal(cmd: &str, plane: &Path, anchors: &[&Path]) -> Option<String> {
    refusal_of(&Heredocs::of(cmd), plane, anchors)
}

/// [`refusal`] of a command already read, for a caller that reads it once for every backstop
/// ([`crate::toolgate::verdict`], #1426).
pub(crate) fn refusal_of(read: &Heredocs, plane: &Path, anchors: &[&Path]) -> Option<String> {
    // Read once for the whole command, every depth, and handed to the arm that reads it.
    let inward = read.inward();
    if inward.too_big {
        return Some(guardcaps::too_deep_refusal());
    }
    let stripped = &read.stripped;
    let at = At { plane, anchors };
    // Where the command sits first (a substitution, a `case` branch, a function body), so the
    // refusal says so; then how it is spelt.
    in_a_substitution(inward)
        .or_else(|| in_a_branch_or_body(stripped))
        .or_else(|| refusal_in(stripped, at))
        .or_else(|| in_a_shell_string(read))
        .or_else(|| not_as_the_rule_spells_it(stripped))
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
/// each segment's, as written. With `inward`, each command a substitution in `text` runs too,
/// at every depth ([`every_substitution`]), less any heredoc body a reader takes as data, with
/// a `case` branch and a function body read as commands of their own ([`commands_of`]). `true`
/// beside them when a segment has more wrappers in front of its program than are read, or the
/// substitutions reach past the walk's bounds.
pub(crate) fn every_reading_in(
    text: &str,
    wanted: &impl Fn(&str) -> bool,
    inward: bool,
) -> (Vec<Reading>, bool) {
    let mut out: Vec<Reading> = Vec::new();
    let mut too_deep = false;
    let mut read = |segments: Vec<Vec<String>>| {
        for toks in segments {
            let (readings, deep) = readings_of(&toks, wanted);
            out.extend(readings);
            too_deep |= deep;
        }
    };
    if !inward {
        return readings_in(&shellseg::segment_argv(text), wanted);
    }
    read(commands_of(&heredoc::strip_reader_heredocs(text)));
    let inner = every_substitution(text);
    for cased in &inner.texts {
        read(with_function_bodies(shellseg::segment_argv(
            &lines_it_runs(cased),
        )));
    }
    (out, too_deep || inner.too_big)
}

/// Every [`Reading`] of `segments` ([`readings_of`]), and `true` beside them when one has more
/// wrappers in front of its program than are read.
pub(crate) fn readings_in(
    segments: &[Vec<String>],
    wanted: &impl Fn(&str) -> bool,
) -> (Vec<Reading>, bool) {
    let mut out = Vec::new();
    let mut too_deep = false;
    for toks in segments {
        let (readings, deep) = readings_of(toks, wanted);
        out.extend(readings);
        too_deep |= deep;
    }
    (out, too_deep)
}

/// What `find` makes of the first command `text` runs that it answers for — its segments, a
/// `case` branch, a function body, or a substitution a shell runs at any depth
/// ([`every_substitution`]). For a string a shell is handed whole (`sh -c`, a body `bash`
/// reads), which no other reading here looks inside. A heredoc body a reader takes as data in
/// it, a script it writes to a file, is no command; only its substitutions run (#1426).
fn found<T>(text: &str, find: &impl Fn(&Reading) -> Option<T>) -> Option<T> {
    let first =
        |segments: Vec<Vec<String>>| segments.iter().find_map(|toks| find(&reading_of(toks)?));
    first(commands_of(&heredoc::strip_reader_heredocs(text))).or_else(|| {
        every_substitution(text).texts.iter().find_map(|cased| {
            first(with_function_bodies(shellseg::segment_argv(
                &lines_it_runs(cased),
            )))
        })
    })
}

/// What `find` makes of the first segment of `text` as written that it answers for.
fn found_in<T>(text: &str, find: &impl Fn(&Reading) -> Option<T>) -> Option<T> {
    shellseg::segment_argv(text)
        .iter()
        .find_map(|toks| find(&reading_of(toks)?))
}

/// The words of each command `text` runs, as the leak guard reads them (#1417): a `case`
/// pattern's `)` read as the end of a command ([`crate::shellsubst::Scan::as_commands`]), so
/// a branch's command is a command of its own, and a function body read where it is defined
/// ([`with_function_bodies`]). For a text that holds no heredoc body a reader takes as data.
fn commands_of(text: &str) -> Vec<Vec<String>> {
    let segments = match scan(text, false) {
        Some(scan) => shellseg::segment_argv(&scan.as_commands(false)),
        None => shellseg::segment_argv(text),
    };
    with_function_bodies(segments)
}

/// `segments`, each with a function definition's header taken off its front
/// ([`shellwrap::past_function_headers`]), so the first command of the body is read as the
/// command it is. A body is read where it is defined, called or not.
fn with_function_bodies(segments: Vec<Vec<String>>) -> Vec<Vec<String>> {
    segments
        .into_iter()
        .map(|toks| match shellwrap::past_function_headers(&toks) {
            Some(at) => toks[at..].to_vec(),
            None => toks,
        })
        .collect()
}

/// The lines of `text` a shell runs: a heredoc body a reader takes as data — a commit message,
/// a pull request's body, a script written to a file — left out
/// ([`leakguard::lines_a_command_could_run`]).
///
/// Its line continuations are taken out first, as the shell takes them out before it reads a
/// heredoc's delimiter: `<<E\` and a newline and `OF` is an unquoted `EOF`, whose body expands
/// (#1417).
fn lines_it_runs(text: &str) -> String {
    leakguard::lines_a_command_could_run(&text.replace("\\\n", ""))
        .into_iter()
        .map(|(row, _)| row)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Where a command sits that the host's rule, read against the command as written, never sees.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Place {
    /// In a command substitution (`$( … )`, a backtick), quoted or not, at any depth.
    Substitution,
    /// In a `case` branch.
    CaseBranch,
    /// In the body of a shell function the command defines.
    FunctionBody,
}

impl Place {
    /// Where it sits, as a refusal says it.
    pub(crate) fn said(self) -> &'static str {
        match self {
            Self::Substitution => "inside a command substitution",
            Self::CaseBranch => "inside a `case` branch",
            Self::FunctionBody => "inside a function body",
        }
    }
}

/// Whether `text` may define a shell function: it holds `function`, or a `(` with only blanks
/// before a `)`. A cheap test, so a text that defines none is not read again.
fn defines_a_function(text: &str) -> bool {
    text.contains("function")
        || text.match_indices('(').any(|(at, _)| {
            text[at + 1..]
                .trim_start_matches([' ', '\t'])
                .starts_with(')')
        })
}

/// What `look` finds in a command a `case` branch or a function body in `text` runs, which no
/// segment as written starts with, and where it sits. Read over the lines a shell runs
/// ([`lines_it_runs`]), so a heredoc body a reader takes as data, a script written to a file
/// included, is never read as commands. A `case` branch is read once each pattern's `)` ends a
/// command; a function body once its header is off the front of its segment.
pub(crate) fn in_a_branch_or_body_by<T>(
    text: &str,
    look: &impl Fn(&str) -> Option<T>,
) -> Option<(T, Place)> {
    let cased = scan(text, false).filter(|scan| scan.has_case_patterns());
    if cased.is_none() && !defines_a_function(text) {
        return None;
    }
    let runs = lines_it_runs(text);
    let as_written: HashSet<Vec<String>> = shellseg::segment_argv(text).into_iter().collect();
    let new = |words: &Vec<String>| !words.is_empty() && !as_written.contains(words);
    let branches = match scan(&runs, false).filter(|scan| scan.has_case_patterns()) {
        Some(scan) => shellseg::segment_argv(&scan.as_commands(false)),
        None => shellseg::segment_argv(&runs),
    };
    branches
        .iter()
        .filter(|words| cased.is_some() && new(words))
        .find_map(|words| look(&quoted(words)).map(|it| (it, Place::CaseBranch)))
        .or_else(|| {
            with_function_bodies(branches)
                .iter()
                .filter(|words| new(words))
                .find_map(|words| look(&quoted(words)).map(|it| (it, Place::FunctionBody)))
        })
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

/// A consent-gated command under either name in a string or a heredoc body a shell runs, at
/// any level up to the cap ([`in_a_string_a_shell_runs_by`]), or the refusal of one nested
/// past it.
fn in_a_shell_string(read: &Heredocs) -> Option<String> {
    let find = |it: &Reading| consent_gated(it, &Gated::not_a7s);
    Some(
        match in_a_string_a_shell_runs_by(read, &|text: &str| found(text, &find))? {
            Nested::Found(it) => shell_string_refusal(&it),
            Nested::TooDeep => guardcaps::too_deep_refusal(),
        },
    )
}

/// What [`in_shell_strings_at_every_level_by`] found.
pub(crate) enum Nested<T> {
    /// What its `look` found, at some level.
    Found(T),
    /// A string handed on more than [`guardcaps::MAX_NESTING`] levels in: not read, so the call
    /// is too big to check, whatever the string holds.
    TooDeep,
}

/// What the consent and operator-rule backstops' `look` finds in a string or a heredoc body a
/// shell runs, at every level (#1426): the first level read as before, a heredoc's lines one by
/// one ([`in_a_shell_string_by`], [`in_a_shells_heredoc_by`]), so every answer it gave stands;
/// then every level through the walk the handoff guard reads with too
/// ([`in_shell_strings_at_every_level_by`]). `look` is handed each string as the shell reads it
/// ([`as_the_shell_reads`]). `read` is the command, its heredocs read once.
pub(crate) fn in_a_string_a_shell_runs_by<T>(
    read: &Heredocs,
    look: &impl Fn(&str) -> Option<T>,
) -> Option<Nested<T>> {
    if let Some(it) = in_a_shell_string_by(&read.stripped, look)
        .or_else(|| in_a_shells_heredoc_by(&read.rows, look))
    {
        return Some(Nested::Found(it));
    }
    in_shell_strings_at_every_level_by(read, &|inner: &Heredocs| {
        look(&as_the_shell_reads(&inner.text))
    })
}

/// The walk every backstop reads nested shell strings with — the consent, operator-rule and
/// handoff backstops alike (#1419, #1426): what `look` finds in a text a shell is handed to
/// run, at every level ([`strings_handed_on`]). The host's rule reads only the outer command,
/// so it sees none of them.
///
/// `look` is handed each text raw, exactly as the shell receives it (D-1419-3), with its
/// heredocs read once (D-1419-13), and reads that text's own level only; the strings it hands
/// on are this walk's. Each distinct text is read once per command, and the texts each one
/// hands on are found once for every backstop that walks the same command
/// ([`Heredocs::handed_on`]).
///
/// **A level is a text handed to a shell**, in command position: the script of a `-c` or an
/// `eval`, the body of a heredoc that feeds a shell, a script fed to a shell on its stdin, or
/// the command a `find -exec` runs. Text in a body no shell is handed (a commit message, a body
/// the shells read differently) is no level, however often it names one.
///
/// **Fails closed past the cap**: a text handed on more than [`guardcaps::MAX_NESTING`] levels
/// in is not read; it is [`Nested::TooDeep`], whatever it holds, and reported to
/// [`shellseg::too_deep_within`].
pub(crate) fn in_shell_strings_at_every_level_by<T>(
    read: &Heredocs,
    look: &impl Fn(&Heredocs) -> Option<T>,
) -> Option<Nested<T>> {
    in_strings_from(read, 1, look, &mut HashSet::new())
}

/// What `look` finds in the texts `read` hands a shell ([`strings_handed_on`]), they being
/// `depth` levels into the command, and in theirs, further in.
fn in_strings_from<'a, T>(
    read: &'a Heredocs,
    depth: usize,
    look: &impl Fn(&Heredocs) -> Option<T>,
    seen: &mut HashSet<&'a str>,
) -> Option<Nested<T>> {
    let strings = read.handed_on(strings_handed_on);
    if strings.is_empty() {
        return None;
    }
    if depth > guardcaps::MAX_NESTING {
        shellseg::met_a_string_too_deep();
        return Some(Nested::TooDeep);
    }
    for inner in strings {
        // A text met again found nothing the first time, or the walk would have stopped there.
        if !seen.insert(inner.text.as_str()) {
            continue;
        }
        if let Some(it) = look(inner) {
            return Some(Nested::Found(it));
        }
        if let Some(nested) = in_strings_from(inner, depth + 1, look, seen) {
            return Some(nested);
        }
    }
    None
}

/// Every text `read` hands a shell to run, raw, one level further in:
///
/// - what each of its commands hands on ([`handed_by_the_commands_of`]): the script of a `-c`
///   or an `eval`, a script fed on stdin, the command of a `find -exec`; in its own commands
///   and in the commands of each substitution it runs, at any depth
///   ([`handed_in_substitutions`], off the substitutions `read` holds, read once);
/// - the body of each heredoc that feeds a shell ([`bodies_handed_on`]);
/// - the same in each line of a body the shells read differently, read one line at a time
///   ([`Heredocs::lines_a_shell_may_run_in_place`]). The body itself is handed to no shell, so
///   it is no level.
///
/// A heredoc body a reader takes as data hands nothing on.
fn strings_handed_on(read: &Heredocs) -> Vec<String> {
    let mut out = handed_by_the_commands_of(&read.stripped);
    out.extend(bodies_handed_on(read));
    out.extend(handed_in_substitutions(read.inward()));
    for line in read.lines_a_shell_may_run_in_place() {
        out.extend(handed_by_the_commands_of(&line));
        if line.contains("$(") || line.contains('`') {
            out.extend(handed_in_substitutions(&every_substitution(&line)));
        }
    }
    let mut met = HashSet::new();
    out.retain(|inner| met.insert(inner.clone()));
    out
}

/// The heredoc bodies of `read` a shell is handed as a script: each one that feeds a shell
/// ([`Heredocs::bodies_fed_to_a_shell`]), and, where a command sources its stdin by name
/// (`source /dev/stdin <<'EOF'`, which the layout names no shell for), every body
/// ([`Heredocs::every_body`]).
fn bodies_handed_on(read: &Heredocs) -> Vec<String> {
    let mut out = read.bodies_fed_to_a_shell();
    let text = &read.stripped;
    let names_stdin =
        text.contains("/dev/stdin") || text.contains("/dev/fd/0") || text.contains("/proc/self/");
    if names_stdin
        && text.contains("<<")
        && commands_of(text).iter().any(|words| {
            let (prog, _env, argv) = shellwrap::split_env(words);
            shellwrap::takes_its_script_from_stdin(&prog, &argv)
        })
    {
        out.extend(read.every_body());
    }
    out
}

/// What the commands of each substitution hand a shell, at any depth, off `inward`, the
/// substitutions read once ([`every_substitution`]), each read as the substitution backstop
/// reads it ([`in_a_substitution_by`]): a heredoc body a reader takes as data is no command.
fn handed_in_substitutions(inward: &Inward) -> Vec<String> {
    let mut out = Vec::new();
    for cased in &inward.texts {
        out.extend(handed_by_the_commands_of(&lines_it_runs(cased)));
        if cased.contains("<<") {
            out.extend(bodies_handed_on(&Heredocs::of(cased)));
        }
    }
    out
}

/// Every text the commands of `text` hand a shell to run ([`handed_as_written`]), each command
/// read where it stands: in a `case` branch or a function body too. For a text that holds no
/// heredoc body a reader takes as data.
///
/// **Each script as the shell that is handed it receives it.** The shared lexer keeps a
/// backslash in front of a `$` or a backtick inside double quotes, where a shell drops it: to
/// the shell, `bash -c "echo \$(x)"` hands on `echo $(x)`, which runs `x`. The corpus pins the
/// lexer, so the text is read a second time with those backslashes dropped as a shell drops
/// them ([`as_a_shell_unquotes`]), through the same reading, and both readings are kept
/// (#1426). The second reading is the first one's in every other way: the same commands, a
/// `case` branch and a function body among them, and a word that joins an ANSI-C part to a
/// double-quoted one read part by part.
///
/// **A text the lexer cannot read whole is still read** (it fails closed): with every heredoc
/// body emptied ([`crate::shellsubst::Scan::as_commands`]), so a body's apostrophe does not run
/// on over the commands around it, and, where that does not read either, line by line.
fn handed_by_the_commands_of(text: &str) -> Vec<String> {
    let both_ways = |text: &str, out: &mut Vec<String>| {
        let (handed, parsed) = handed_as_written(text);
        out.extend(handed);
        if text.contains("\\$") || text.contains("\\`") {
            out.extend(handed_as_written(&as_a_shell_unquotes(text)).0);
        }
        parsed
    };
    let mut out = Vec::new();
    if !may_hand_a_script_on(text) || both_ways(text, &mut out) {
        return out;
    }
    if let Some(scan) = scan(text, true).filter(crate::shellsubst::Scan::has_heredoc_bodies)
        && both_ways(&scan.as_commands(true), &mut out)
    {
        return out;
    }
    if text.contains('\n') {
        for line in text.split('\n') {
            both_ways(line, &mut out);
        }
    }
    out
}

/// Whether a command of `text` may hand a script on at all: it may name a shell (every one of
/// [`shellwrap::STRING_SHELLS`] ends in `sh`), `eval`, `find` or `source`, or it holds what `.`
/// would source (stdin by name, a process substitution). Asked as a hot-path prefilter must ask
/// ([`shellwrap::may_name`]), so it never says no to a spelling the reading behind it accepts;
/// most texts name none, and are not read again for the scripts they hand on.
fn may_hand_a_script_on(text: &str) -> bool {
    if text.contains("$'") || text.contains("$\\\n") {
        return true;
    }
    let text = shellwrap::prefilter_text(text);
    ["sh", "eval", "find", "source", "/dev/", "/proc/", "<("]
        .iter()
        .any(|name| text.contains(name))
}

/// `text` as [`commands_of`] reads it for its commands: each `case` pattern's `)` read as the
/// end of a command, and its line continuations taken out.
fn as_commands_read(text: &str) -> std::borrow::Cow<'_, str> {
    match scan(text, false) {
        Some(scan) => scan.as_commands(false).into(),
        None => text.into(),
    }
}

/// Every text the commands of `text`, as the lexer reads them, hand a shell to run, and whether
/// the lexer read the text whole:
///
/// - the script of a `-c` or an `eval` ([`shellwrap::shell_scripts_of`]);
/// - the script a here-string hands a shell, or a `source` of stdin
///   ([`shellwrap::here_string_script_of`]);
/// - what a pipe feeds a shell that reads its script from stdin ([`piped_into_a_shell`]);
/// - what a process substitution prints, where a shell or a `source` reads it as its script
///   ([`printed_by_process_substitutions`]);
/// - the command a `find -exec` runs ([`shellwrap::commands_a_find_runs`]), as a command line.
///
/// The last four are scripts a shell is handed without a `-c` string or a heredoc (D-1426-9).
fn handed_as_written(text: &str) -> (Vec<String>, bool) {
    let source = as_commands_read(text);
    let (segments, parsed) = shellseg::segment_argv_parsed(&source);
    let mut out = Vec::new();
    let (mut reads_stdin, mut reads_a_file) = (false, false);
    for words in with_function_bodies(segments) {
        let (prog, _env, argv) = shellwrap::split_env(&words);
        out.extend(shellwrap::shell_scripts_of(&prog, &argv));
        out.extend(
            shellwrap::commands_a_find_runs(&prog, &argv)
                .iter()
                .map(|run| quoted(run)),
        );
        if !shellwrap::runs_a_script_file(&prog) {
            continue;
        }
        out.extend(shellwrap::here_string_script_of(&prog, &argv));
        reads_stdin |= shellwrap::takes_its_script_from_stdin(&prog, &argv);
        reads_a_file |= argv.iter().any(|w| w == "<");
    }
    if reads_stdin && source.contains('|') {
        out.extend(piped_into_a_shell(&source));
    }
    if reads_a_file && source.contains("<(") {
        out.extend(printed_by_process_substitutions(&source));
    }
    (out, parsed)
}

/// How many commands in front of a pipe into a shell are read as feeding it, once a group or
/// a subshell stands there (`{ a; b; } | sh`): the walk does not say where the group began, so
/// it reads back this far. Keeps the reading linear in the text.
const FEEDERS_READ: usize = 32;

/// What a pipe feeds each command of `source` that reads its script from stdin
/// ([`shellwrap::takes_its_script_from_stdin`]: `echo '…' | bash`, `printf … | sh -s`,
/// `… | source /dev/stdin`): what each command in front of it on the pipeline could print
/// ([`floorguard::fed_lines`], the release floor's reading of the same). A text the lexer
/// cannot read has no pipeline to follow, so every command of it is read as a feeder.
fn piped_into_a_shell(source: &str) -> Vec<String> {
    let fed_by = |argv: &[String]| {
        let at = shellwrap::past_function_headers(argv).unwrap_or(0);
        floorguard::fed_lines(&argv[at..])
    };
    let Some(segments) = shellseg::joined_argv(source) else {
        return shellseg::segment_argv(source)
            .iter()
            .flat_map(|argv| fed_by(argv))
            .collect();
    };
    let piped = |op: &Option<String>| matches!(op.as_deref(), Some("|" | "|&"));
    let mut out = Vec::new();
    for (k, reader) in segments.iter().enumerate() {
        if !piped(&reader.before) {
            continue;
        }
        let at = shellwrap::past_function_headers(&reader.argv).unwrap_or(0);
        let (prog, _env, argv) = shellwrap::split_env(&reader.argv[at..]);
        if !shellwrap::takes_its_script_from_stdin(&prog, &argv) {
            continue;
        }
        // Back along the pipeline; and where a group's end stands in front of the pipe, back
        // over the commands that may be the group's.
        let mut grouped = false;
        for feeder in segments[..k].iter().rev().take(FEEDERS_READ) {
            out.extend(fed_by(&feeder.argv));
            grouped |= !piped(&feeder.after);
            if !grouped && !piped(&feeder.before) {
                break;
            }
        }
    }
    out
}

/// What the commands inside each process substitution of `source` (`<( … )`) could print
/// ([`floorguard::fed_lines`]): a shell or a `source` handed one reads that as its script
/// (`bash <(echo '…')`, `source <( … )`, `bash < <( … )`). Quoting is not read, so one inside
/// quotes is read too, which only ever reads more.
fn printed_by_process_substitutions(source: &str) -> Vec<String> {
    let chars: Vec<char> = source.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i + 1 < chars.len() {
        if chars[i] != '<' || chars[i + 1] != '(' {
            i += 1;
            continue;
        }
        let from = i + 2;
        let mut depth = 1usize;
        let mut j = from;
        while j < chars.len() && depth > 0 {
            match chars[j] {
                '\\' => j += 1,
                '\'' => {
                    j += 1;
                    while j < chars.len() && chars[j] != '\'' {
                        j += 1;
                    }
                }
                '(' => depth += 1,
                ')' => depth -= 1,
                _ => {}
            }
            j += 1;
        }
        let to = j
            .saturating_sub(usize::from(depth == 0))
            .clamp(from, chars.len());
        let body: String = chars[from..to].iter().collect();
        for argv in commands_of(&body) {
            out.extend(floorguard::fed_lines(&argv));
        }
        i = j.max(from);
    }
    out
}

/// `text` with the backslash dropped in front of a `$` or a backtick inside double quotes, as
/// a shell drops it when it unquotes the word, and nothing else changed: what the lexer then
/// reads of a double-quoted word is what a shell hands on. Single quotes and ANSI-C quoting
/// (`$'…'`) are stepped over, and a `$( … )` inside double quotes is read as unquoted text up
/// to its `)`, so the quotes inside it pair as the shell pairs them.
fn as_a_shell_unquotes(text: &str) -> String {
    /// What stands open: double quotes, or a `$( … )` with the parentheses open inside it.
    enum Open {
        Quotes,
        Substitution(usize),
    }
    let mut out = String::with_capacity(text.len());
    let mut open: Vec<Open> = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        let quoted = matches!(open.last(), Some(Open::Quotes));
        match c {
            '\\' => match chars.next() {
                Some(d @ ('$' | '`')) if quoted => out.push(d),
                Some(d) => {
                    out.push('\\');
                    out.push(d);
                }
                None => out.push('\\'),
            },
            '"' => {
                out.push(c);
                if quoted {
                    open.pop();
                } else {
                    open.push(Open::Quotes);
                }
            }
            '$' if chars.peek() == Some(&'(') => {
                out.push_str("$(");
                chars.next();
                open.push(Open::Substitution(0));
            }
            '$' if !quoted && chars.peek() == Some(&'\'') => {
                out.push_str("$'");
                chars.next();
                while let Some(d) = chars.next() {
                    out.push(d);
                    if d == '\\' {
                        out.extend(chars.next());
                    } else if d == '\'' {
                        break;
                    }
                }
            }
            '\'' if !quoted => {
                out.push(c);
                for d in chars.by_ref() {
                    out.push(d);
                    if d == '\'' {
                        break;
                    }
                }
            }
            '(' | ')' if !quoted => {
                out.push(c);
                match (c, open.last_mut()) {
                    ('(', Some(Open::Substitution(n))) => *n += 1,
                    (')', Some(Open::Substitution(0))) => {
                        open.pop();
                    }
                    (')', Some(Open::Substitution(n))) => *n -= 1,
                    _ => {}
                }
            }
            _ => out.push(c),
        }
    }
    out
}

/// What `look` finds one level into a string a segment of `text` hands a shell to run
/// (`sh -c '…'`, `eval '…'`, [`shellwrap::shell_scripts`]): the host's rule reads only the
/// outer command.
fn in_a_shell_string_by<T>(text: &str, look: &impl Fn(&str) -> Option<T>) -> Option<T> {
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

/// What `look` finds on a line a heredoc feeds a shell (`bash <<'EOF'`, `cat <<EOF | sh`), in
/// `rows`, the lines the command could run ([`leakguard::lines_a_command_could_run`], the plan
/// the leak guard and A7 share): the rule saw only the `bash` that opened it.
fn in_a_shells_heredoc_by<T>(
    rows: &[(String, bool)],
    look: &impl Fn(&str) -> Option<T>,
) -> Option<T> {
    rows.iter()
        .filter(|(_, in_a_shells_body)| *in_a_shells_body)
        .find_map(|(row, _)| look(&as_the_shell_reads(row)))
}

/// A consent-gated command under either name in a substitution a shell runs anywhere in the
/// command as written (`inward`), an expanding heredoc body's included, which
/// [`heredoc::strip_reader_heredocs`] takes out of every other reading here: the host's rule
/// reads only the outer command. A handoff too: A7 judges the one it finds first, and one it
/// does not find in a substitution is one no rule asks about.
fn in_a_substitution(inward: &Inward) -> Option<String> {
    let find = |it: &Reading| consent_gated(it, &|_: &Gated| true);
    in_a_substitution_by(inward, &|segments: &[Vec<String>]| {
        segments.iter().find_map(|toks| find(&reading_of(toks)?))
    })
    .map(|it| placed_refusal(&it, Place::Substitution))
}

/// A consent-gated command under either name, not a handoff (A7's), in a `case` branch or a
/// function body ([`in_a_branch_or_body_by`]).
fn in_a_branch_or_body(text: &str) -> Option<String> {
    let find = |it: &Reading| consent_gated(it, &Gated::not_a7s);
    in_a_branch_or_body_by(text, &|text: &str| found_in(text, &find))
        .map(|(it, place)| placed_refusal(&it, place))
}

/// What `look` finds in the commands of a substitution a shell runs anywhere in the command, at
/// any depth, as `inward` read them once ([`every_substitution`]): each read less the heredoc
/// bodies a reader takes as data, with a function body read as commands of its own.
pub(crate) fn in_a_substitution_by<T>(
    inward: &Inward,
    look: &impl Fn(&[Vec<String>]) -> Option<T>,
) -> Option<T> {
    inward.texts.iter().find_map(|cased| {
        look(&with_function_bodies(shellseg::segment_argv(
            &lines_it_runs(cased),
        )))
    })
}

/// `words` as a command line a shell reads back as the same words.
fn quoted(words: &[String]) -> String {
    words
        .iter()
        .map(|w| crate::handoff::quote(w))
        .collect::<Vec<_>>()
        .join(" ")
}

/// What a consent-gated command the host's rule never sees, where it sits, is told.
fn placed_refusal(it: &Gated, place: Place) -> String {
    let what = it.as_the_rule_spells_it();
    format!(
        "`{name} {what}` is refused {where_}. It waits for your consent through a rule spelt \
         `{RULES_NAME} …`, and the host matches that rule against the command as written, which \
         does not start with it, so nothing would ask you. Run it as a command of its own, spelt \
         `{RULES_NAME} {what}`, the bare name at the start.",
        name = it.name,
        where_ = place.said(),
    )
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
    not_as_written_by(text, &|text: &str| found_in(text, &find), asked)
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
    let data = data_lines(text);
    heredoc::segments_of(&toks)
        .iter()
        .find_map(|(seg, _before)| {
            let (first, last) = (seg.first()?, seg.last()?);
            // A line of a heredoc body a reader takes as data is no command: only the
            // substitutions in it run, and those are read on their own.
            if usize::try_from(first.start).is_ok_and(|at| data.get(at).copied().unwrap_or(false)) {
                return None;
            }
            let Some(source) = source_of(&chars, first.start, last.end) else {
                let words: Vec<String> = seg.iter().map(|t| t.text.clone()).collect();
                return look(&words.join(" ")).map(|it| (it, String::new()));
            };
            let it = look(&source)?;
            (!asked(&source, &it)).then_some((it, source))
        })
}

/// Which characters of `text` stand on a line of a heredoc body that no shell runs: data a
/// reader takes, such as a script written to a file. Empty where the layout is not the text
/// line for line, or the text holds a line continuation, so nothing is skipped.
fn data_lines(text: &str) -> Vec<bool> {
    // A line continuation can turn what the layout reads as a quoted delimiter into an unquoted
    // one, so with one in the text nothing is skipped.
    if !text.contains("<<") || text.contains("\\\n") {
        return Vec::new();
    }
    let layout = heredoc::heredoc_layout(text);
    if !layout.iter().map(|l| l.text.as_str()).eq(text.split('\n')) {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(text.len());
    for (k, line) in layout.iter().enumerate() {
        if k > 0 {
            out.push(false); // the newline
        }
        let data = line.body && !line.executed;
        out.extend(std::iter::repeat_n(data, line.text.chars().count()));
    }
    out
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
    // Over the lines a shell runs: a heredoc body a reader takes as data names no command.
    let it = gated_in(&lines_it_runs(text), &|it: &Gated| {
        it.under_a_new_name() && !lifted(it)
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
