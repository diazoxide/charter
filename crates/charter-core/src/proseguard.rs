//! Arms A5 and A6: prose that gets PUBLISHED or PERSISTED may not carry a live substitution.
//!
//! A port of `charter/hooks.py`'s `_forge_substitution_hit` (#703) and `_charter_substitution_hit`
//! (#778) with `_forge_prose_command`, `_charter_prose_command`, `_charter_words` and the three
//! tables they read. The judgement both share — would the SHELL run this substitution? — is
//! [`crate::livesub::live_substitution`], in its own module for the Python's own reason: a second
//! copy of a shell quoting walk is a second thing to be wrong about.
//!
//! # Where this is called from
//!
//! [`crate::toolgate`] — `charter/hooks.py:pretooluse`'s eight refusals, in its order — and
//! through it `charter hook pretooluse` (M3.1 stage 6).
//!
//! Until that stage the switch was closed: `main.rs`'s `is_a_tool_hook` answered every word in
//! the `pretooluse`/`posttooluse` namespace with exit 2 — *block* — because a PARTIAL guard on
//! the switch turns fail-closed into allow-everything-except-the-arm-that-is-ported. It moved
//! once all eight arms were standing, and never earlier. The other eight words in that
//! namespace still block, for the same reason they always did.
//! # What happened
//!
//! An agent filing an issue wrote ``--body "… `env -u PYTHONSAFEPATH` …"``, meaning the backticks
//! as a MARKDOWN CODE SPAN. Inside double quotes they are command substitution, so the shell ran
//! `env` and pasted 64 variables — four service-account tokens among them — into a public issue
//! body that a forge keeps edit history for. That is A5.
//!
//! A6 is the same slip reaching a committed file by charter's own commands: while writing up A5's
//! findings the coordinating agent ran ``charter persona remember "… `pending` …"``, zsh ran the
//! word, spliced its empty output, and the saved memory read *"appending to  each pass"* with the
//! word silently gone — in a directory this plane commits and pushes.
//!
//! # Two guards, not one, and the reason is in the tables
//!
//! A5 covers somebody else's tools and A6 charter's own. The table, the destination the denial
//! names and the remedy it offers are all different, and one constant answering two questions is
//! the #555 defect `hooks.py` already has a name for. A6 runs AFTER A5 so that a line which is
//! both — `charter …` piped into `gh issue create` — is explained by the one that publishes to a
//! forge.
//!
//! # What is NOT refused, which is the calibration that matters
//!
//! The shape the working rule prescribes: `--body-file -` with a QUOTED heredoc. A guard that
//! denied the path it steers agents onto would be switched off within a day. `git commit` and
//! charter's own commit-message commands stay out for the same reason measured the other way —
//! 29 of the last 30 messages on `main` carry a backtick and the spelling that writes them,
//! `-m "$(cat <<'EOF' … EOF)"`, is itself a live `$(`.
//!
//! # Scoped to the whole Bash call — not the argument, not even the segment
//!
//! The coarseness is chosen rather than conceded. Attributing a substitution to the argument it
//! lands in means tracking which token each character of a shell string belongs to; narrowing it
//! to the segment means splitting on operators the tokenizer cannot help with, because
//! [`crate::shellseg::unbacktick`] has already erased the distinction these guards turn on. Both
//! are more parser in the direction that fails OPEN when it is wrong. So
//! `cd "$(git rev-parse --show-toplevel)" && gh pr create --body-file b.md` is refused too, and
//! the remedy the denial names covers it: compute the value in a SEPARATE Bash call.

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::livesub::{SUBSTITUTIONS, live_substitution};
use crate::shellseg;
use crate::shellwrap::{self, base_lower};

/// `(program, noun, verb)` triples whose text a forge PUBLISHES — `_FORGE_PROSE`.
///
/// `merge` is absent on purpose: its `--body` is a merge-commit message rather than the point of
/// the command, and #703's evidence is issue and request bodies. Widening on evidence is cheap; a
/// guard that over-blocks gets switched off once and then covers nothing.
pub const FORGE_PROSE: [(&str, &str, &str); 19] = [
    ("gh", "issue", "create"),
    ("gh", "issue", "comment"),
    ("gh", "issue", "edit"),
    ("gh", "pr", "create"),
    ("gh", "pr", "comment"),
    ("gh", "pr", "edit"),
    ("gh", "pr", "review"),
    ("gh", "release", "create"),
    ("gh", "release", "edit"),
    ("gh", "gist", "create"),
    ("gh", "gist", "edit"),
    ("glab", "issue", "create"),
    ("glab", "issue", "note"),
    ("glab", "issue", "update"),
    ("glab", "mr", "create"),
    ("glab", "mr", "note"),
    ("glab", "mr", "update"),
    ("glab", "release", "create"),
    ("glab", "snippet", "create"),
];

/// `workspace`/`ws` and `worktree`/`wt` are the same command word to charter's parser —
/// `_CHARTER_NOUN_ALIASES`. Written once and applied below, because a hand-copied second half of
/// the table is a row that drifts the day somebody adds a verb to one of them.
const CHARTER_NOUN_ALIASES: [(&str, &str); 2] = [("workspace", "ws"), ("worktree", "wt")];

/// `(noun, verb, what charter does with the text, the file input it accepts)` —
/// `_CHARTER_PROSE_ROWS`.
///
/// **The line for inclusion is #710's own**, which kept `gh pr merge --body` out because its
/// `--body` is a merge-commit message rather than the point of the command: the free text has to
/// be **required or the primary operand**, and what holds it has to be read back as prose by a
/// later reader. That keeps out `workspace create --vision`, `workspace snapshot --description`,
/// `persona create --role/--delegate-when` and `vault add`, where the prose is a secondary
/// attribute of creating a thing.
///
/// **The fourth column is the remedy the denial may name, and it is per row** because it is not
/// uniform: only `report bug|gap` has a file input. Offering `--from-file` on `persona remember`,
/// which has none, would answer a refusal with a usage error.
pub const CHARTER_PROSE_ROWS: [(&str, &str, &str, Option<&str>); 11] = [
    (
        "persona",
        "remember",
        "a memory file under `personas/`, which this plane commits and pushes",
        None,
    ),
    (
        "persona",
        "log",
        "the persona's activity log under `personas/`, committed with it",
        None,
    ),
    (
        "workspace",
        "remember",
        "a memory file under `workspaces/`, committed once the workspace is LIVE",
        None,
    ),
    (
        "workspace",
        "note",
        "a memory file under `workspaces/` (`note` is `remember`)",
        None,
    ),
    (
        "workspace",
        "todo",
        "the workspace's todo list under `workspaces/`",
        None,
    ),
    (
        "workspace",
        "vision",
        "the workspace's Vision in `workspace.md`, committed once it is LIVE",
        None,
    ),
    (
        "worktree",
        "abandon",
        "the worktree's history — what whoever picks the piece up reads first",
        None,
    ),
    (
        "change",
        "create",
        "the change record's `why`, which `charter change push` writes into every request body \
         on the forge",
        None,
    ),
    (
        "change",
        "drop",
        "the exclusion's `why`, which `charter change push` writes into every request body on \
         the forge",
        None,
    ),
    (
        "report",
        "bug",
        "a report draft that `charter report send` publishes as a PUBLIC issue on charter's own \
         tracker",
        Some("--from-file"),
    ),
    (
        "report",
        "gap",
        "a report draft that `charter report send` publishes as a PUBLIC issue on charter's own \
         tracker",
        Some("--from-file"),
    ),
];

/// What a row of [`CHARTER_PROSE_ROWS`] says: the destination the denial names, and the file
/// input it may offer instead (`None` for a command that has none).
type ProseFacts = (&'static str, Option<&'static str>);

/// `_CHARTER_PROSE`, keyed the way `_charter_prose_command` looks it up.
type ProseTable = HashMap<(&'static str, &'static str), ProseFacts>;

/// `_CHARTER_PROSE`: [`CHARTER_PROSE_ROWS`] widened by [`CHARTER_NOUN_ALIASES`], built once
/// rather than written twice.
fn charter_prose() -> &'static ProseTable {
    static TABLE: OnceLock<ProseTable> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut out = HashMap::new();
        for (noun, verb, dest, from_file) in CHARTER_PROSE_ROWS {
            out.insert((noun, verb), (dest, from_file));
            if let Some((_, alias)) = CHARTER_NOUN_ALIASES.iter().find(|(n, _)| *n == noun) {
                out.insert((*alias, verb), (dest, from_file));
            }
        }
        out
    })
}

/// `"gh issue create"` — the prose-publishing forge command in `cmd`, or `None` —
/// `_forge_prose_command`.
///
/// **Adjacent PAIRS rather than the first two words**, which is where A4's own reader stops:
/// `gh --repo o/r issue create` puts `o/r` in front, and a guard that read `words[0], words[1]`
/// would see `("gh", "o/r", "issue")` and allow. The tokenizer is what keeps that honest in the
/// other direction — a quoted `--search "pr create"` stays ONE word and can never supply the
/// pair.
///
/// **Pairs over every token, flags included.** Dropping `-…` tokens first was the obvious
/// spelling and it was strictly worse: removing a flag JOINS the words it stood between, so
/// `gh issue list --label issue --state create` paired two flag VALUES into
/// `("gh", "issue", "create")` and refused a command that publishes nothing. It bought nothing
/// back — a global flag's value sits in FRONT of the noun rather than between the noun and the
/// verb. The Python's deletion sweep found it: the filter survived deletion because no test
/// distinguished the two, and looking for the test showed the mutant was the better code.
pub fn forge_prose_command(cmd: &str) -> Option<String> {
    for toks in shellseg::segment_argv(cmd) {
        let (prog, _env, argv) = shellwrap::split_env(&toks);
        let base = base_lower(&prog);
        if base != "gh" && base != "glab" {
            continue;
        }
        let words = argv.get(1..).unwrap_or(&[]);
        // Python's `zip(words, words[1:])` — every ADJACENT pair, flags included.
        for pair in words.windows(2) {
            if FORGE_PROSE.contains(&(base.as_str(), pair[0].as_str(), pair[1].as_str())) {
                return Some(format!("{base} {} {}", pair[0], pair[1]));
            }
        }
    }
    None
}

/// The words after `charter` on this segment, or `None` if it is not charter — `_charter_words`.
///
/// **Two spellings, and the second is not exotic in charter's own repository — it is the one an
/// agent working ON charter types all day.** `CONTRIBUTING.md` and shared persona memory both say
/// to live-test a checkout with `python3 -m charter …`, and on that line the basename of the
/// program is `python3`, so [`forge_prose_command`]'s reader would see no `charter` at all. `-B`,
/// `-u` and an `env -u …` prefix all sit between the interpreter and `-m`, which is why the
/// module is looked for by scanning rather than at a fixed index.
///
/// **`-m` as its own word, and no getopt behind it.** Python accepts `-mcharter` and `-Bm
/// charter`, and neither is matched here. That is a fail-OPEN hole and it is named rather than
/// closed: taking it would mean a short-option cluster parser inside a guard whose whole stated
/// risk is being a parser somebody has to re-derive, bought for a spelling nothing in charter's
/// repository writes.
///
/// **This is NOT [`crate::leakguard::is_charter`]**, and the difference is deliberate: that one
/// also answers to `edm`, the pre-rename binary, and this one does not, because the table below
/// is keyed on today's verbs. The Python draws the same line.
pub fn charter_words(prog: &str, argv: &[String]) -> Option<Vec<String>> {
    let base = base_lower(prog);
    if base == "charter" {
        return Some(argv.iter().skip(1).cloned().collect());
    }
    if base.starts_with("python") {
        // Python's `range(1, len(argv) - 1)`: `k + 1` must exist, and the FIRST `-m` decides —
        // a `-m` whose next word is not `charter` answers `None` rather than scanning on.
        for k in 1..argv.len().saturating_sub(1) {
            if argv[k] == "-m" {
                return if argv[k + 1] == "charter" {
                    Some(argv[k + 2..].to_vec())
                } else {
                    None
                };
            }
        }
    }
    None
}

/// `("charter persona remember", what it does with the text, its file input)` — or `None` when no
/// segment of `cmd` is one. `_charter_prose_command`.
///
/// **The FIRST two words, where [`forge_prose_command`] has to walk adjacent pairs.** That
/// difference is a measured fact about charter rather than a shortcut: `gh --repo o/r issue
/// create` puts a flag's value in front of the noun, and charter's root parser has **no option
/// that takes a value** — only `-h` and `--version`. Reading two words is strictly narrower, and
/// what it buys is real: `charter recall --scope persona remember` is a search whose `persona`
/// and `remember` genuinely are adjacent argv words, and every pair walker refuses it.
pub fn charter_prose_command(cmd: &str) -> Option<(String, &'static str, Option<&'static str>)> {
    for toks in shellseg::segment_argv(cmd) {
        let (prog, _env, argv) = shellwrap::split_env(&toks);
        let Some(words) = charter_words(&prog, &argv) else {
            continue;
        };
        // BOTH halves decide, and they fail differently — which is why the Python pins this one
        // line with two tests. Without the emptiness test, `charter "…"` with a single operand
        // reaches `words[1]` and raises IndexError, which in a hook is a broken turn rather than
        // a verdict. With `< 2` widened to `<= 2`, a bare `charter ws note` on a line whose
        // substitution sits in another segment is ALLOWED — a fail-open on exactly the two-word
        // form the table is keyed on.
        if words.len() < 2 {
            continue;
        }
        if let Some((dest, from_file)) =
            charter_prose().get(&(words[0].as_str(), words[1].as_str()))
        {
            return Some((
                format!("charter {} {}", words[0], words[1]),
                dest,
                *from_file,
            ));
        }
    }
    None
}

/// `(the substitution's spelling, the denial)` for a prose-publishing forge command whose line
/// carries a live substitution — or `None`. A5, `_forge_substitution_hit`.
///
/// Returns the spelling as well as the sentence, following
/// [`crate::credguard::single_credential_hit`] and for #289's reason: the trace field that says
/// WHICH shape tripped a guard is what makes "what fired this 335 times" answerable from the
/// records, and a denial that only ever returns prose cannot answer it.
///
/// **Both cheap tests come first because this is a per-Bash-call hot path**, and a command with
/// no backtick and no `$(` in it is almost every command. The substitution test comes before the
/// name test here and AFTER it in [`charter_substitution_hit`], which is measured rather than
/// copied: `gh` and `glab` are two- and four-character substrings that fall inside ordinary
/// English (`through`, `night`), so they reject almost nothing, while `charter` is seven
/// characters and rare.
///
/// # Only ONE of the three filters is inert, and the Python says all three are
///
/// The `hooks.py` section above these functions records a deletion sweep that called both of
/// them survivors that "cannot change a verdict, the table lookup and `_live_substitution`
/// decide". That is true of the SUBSTITUTION test — this port's own sweep reports it inert over
/// 20,000 cases and over the whole recording, because [`live_substitution`] re-decides exactly
/// that question. It is **false of the NAME test**, in both guards: the filter is a
/// case-SENSITIVE substring test and [`forge_prose_command`] / [`charter_words`] fold the
/// program with [`base_lower`], so `GH issue create` and `CHARTER persona remember` are rejected
/// here and would be refused by the rule behind. Removing the name filter moves 43 answers in
/// 20,000 (A5) and 31 (A6).
///
/// That is charter#1173, and this port **reproduces it** — the frozen Python is the
/// differential's oracle, and a port that is right where the oracle is wrong fails its own test.
/// The corpus carries the uppercase spellings so the upstream fix shows up as a divergence
/// rather than silently. It is the same class `credguard` already documents as fixed in A2:
/// a case-sensitive compare on a program name is one Shift key from absent.
pub fn forge_substitution_hit(cmd: &str) -> Option<(&'static str, String)> {
    if !SUBSTITUTIONS.iter().any(|s| cmd.contains(s)) {
        return None;
    }
    if !cmd.contains("gh") && !cmd.contains("glab") {
        return None;
    }
    let spelling = live_substitution(cmd)?;
    let where_ = forge_prose_command(cmd)?;
    let shown = if spelling == "`" { "`…`" } else { "$(…)" };
    // Python's `where.split()[0]`, which is the program name the pair was found under.
    let program = where_.split_whitespace().next().unwrap_or("").to_string();
    Some((
        spelling,
        format!(
            "`{where_}` publishes prose a reader sees, and this line carries a LIVE {shown} \
             command substitution. The shell runs it and substitutes its OUTPUT before {program} \
             is started — charter is handed the command, never the value it becomes — and a forge \
             keeps public edit history, so what gets published cannot be withdrawn by editing it. \
             Put the text in a file and pass `--body-file <path>`, or pipe it in with \
             `--body-file -` and a QUOTED heredoc (`<<'BODY'`; an unquoted `<<BODY` expands \
             exactly the same way). If you meant a markdown code span, it is the same character — \
             backticks stay literal only inside single quotes or a quoted heredoc. This guard \
             reads the SHAPE of the line; it does not know what the command would print, and does \
             not claim to keep a credential off a forge."
        ),
    ))
}

/// `(the substitution's spelling, the denial)` for a charter command that persists prose whose
/// line carries a live substitution — or `None`. A6, `_charter_substitution_hit`.
///
/// Shares [`live_substitution`] with A5 rather than re-deciding what a shell would run: that walk
/// is the part checked against a real bash, and a second copy of it is a second thing to be
/// wrong. What differs is the table, the destination named, and the remedy.
///
/// **The remedy is not A5's, and that had to be measured too.** 221 of the 284 committed memory
/// bodies on `main` contain an apostrophe — and *all nine* of the backtick-carrying ones do — so
/// "use single quotes", the obvious answer, fails on exactly the population this refuses. What
/// works is one backslash per backtick: inside double quotes ``\` `` is a literal backtick and
/// the apostrophes keep working.
pub fn charter_substitution_hit(cmd: &str) -> Option<(&'static str, String)> {
    if !cmd.contains("charter") {
        return None;
    }
    if !SUBSTITUTIONS.iter().any(|s| cmd.contains(s)) {
        return None;
    }
    let spelling = live_substitution(cmd)?;
    let (where_, dest, from_file) = charter_prose_command(cmd)?;
    let shown = if spelling == "`" { "`…`" } else { "$(…)" };
    let fix = match from_file {
        Some(flag) => format!(
            ", or pass `{flag} <path>` (or `--stdin`) and keep the text out of argv altogether"
        ),
        None => String::new(),
    };
    Some((
        spelling,
        format!(
            "`{where_}` takes text charter PERSISTS — {dest} — and this line carries a LIVE \
             {shown} command substitution. The shell runs it and splices its OUTPUT in before \
             charter is started, so what gets saved is not what you typed: charter is handed the \
             command, never the value it becomes. This is #778 — the memory that read \
             \"appending to  each pass\" with the word gone — and #703, where the same slip on a \
             forge body published sixty-four environment variables. Keep the character literal \
             instead: backslash-escape each one (`\\`code\\``), which leaves apostrophes working, \
             or single-quote the whole argument when it holds none{fix}. If you meant to \
             interpolate a computed value, compute it in a SEPARATE Bash call and pass it as \
             \"$VAR\" — a parameter expansion is not a substitution. This guard reads the SHAPE \
             of the line; it does not know what the command would print, and does not claim to \
             keep a credential out of a file."
        ),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The #703 defect itself, and the shape the working rule prescribes, which must NOT be
    /// refused — the calibration that decides whether this guard survives its first day.
    #[test]
    fn the_defect_is_refused_and_the_prescribed_shape_is_not() {
        let hit = forge_substitution_hit("gh issue create --body \"a `env` b\"");
        assert_eq!(hit.as_ref().map(|h| h.0), Some("`"));
        assert_eq!(
            forge_substitution_hit("gh issue create --body-file - <<'BODY'\na `env` b\nBODY\n"),
            None
        );
        // …and an UNQUOTED heredoc expands exactly the same way, so it is refused.
        assert_eq!(
            forge_substitution_hit("gh issue create --body-file - <<BODY\na `env` b\nBODY\n")
                .map(|h| h.0),
            Some("`")
        );
    }

    /// Scoped to the whole Bash call: a substitution in ANOTHER segment still refuses.
    #[test]
    fn the_scope_is_the_whole_call() {
        assert!(
            forge_substitution_hit(
                "cd \"$(git rev-parse --show-toplevel)\" && gh pr create --body-file b.md"
            )
            .is_some()
        );
        assert!(
            charter_substitution_hit(
                "cd \"$(git rev-parse --show-toplevel)\" && charter persona remember 'x'"
            )
            .is_some()
        );
    }

    /// Adjacent pairs, because a global flag's value sits in front of the noun.
    #[test]
    fn a_global_flag_value_does_not_hide_the_pair() {
        assert_eq!(
            forge_prose_command("gh --repo o/r issue create --body x"),
            Some("gh issue create".to_string())
        );
    }

    /// …and the filter that would have joined two flag VALUES into a pair is absent, because
    /// removing a flag joins the words it stood between.
    #[test]
    fn two_flag_values_do_not_become_a_pair() {
        assert_eq!(
            forge_prose_command("gh issue list --label issue --state create"),
            None
        );
    }

    /// A quoted `--search "pr create"` stays ONE word and can never supply the pair.
    #[test]
    fn a_quoted_pair_is_one_word() {
        assert_eq!(
            forge_prose_command("gh issue list --search 'pr create'"),
            None
        );
    }

    /// charter's own commands, in both spellings, and the noun aliases.
    #[test]
    fn charters_own_prose_commands_are_found_in_every_spelling() {
        assert_eq!(
            charter_prose_command("charter persona remember 'x'").map(|f| f.0),
            Some("charter persona remember".to_string())
        );
        assert_eq!(
            charter_prose_command("python3 -B -m charter ws note 'x'").map(|f| f.0),
            Some("charter ws note".to_string())
        );
        assert_eq!(
            charter_prose_command("charter wt abandon 'x'").map(|f| f.0),
            Some("charter wt abandon".to_string())
        );
        // A `-m` whose module is not charter answers None rather than scanning on.
        assert_eq!(
            charter_prose_command("python3 -m pytest persona remember"),
            None
        );
        // `-mcharter` is the named fail-OPEN hole, not a rule with a bug.
        assert_eq!(
            charter_prose_command("python3 -mcharter persona remember 'x'"),
            None
        );
    }

    /// The FIRST two words, not adjacent pairs: charter's root parser takes no value-bearing
    /// option, so `charter recall --scope persona remember` is a search and must not be refused.
    #[test]
    fn charters_reader_takes_the_first_two_words() {
        assert_eq!(
            charter_prose_command("charter recall --scope persona remember"),
            None
        );
        // One operand must not reach `words[1]`.
        assert_eq!(charter_prose_command("charter persona"), None);
    }

    /// The remedy is PER ROW, because only `report bug|gap` has a file input — offering
    /// `--from-file` on `persona remember` would answer a refusal with a usage error.
    #[test]
    fn the_remedy_is_per_row() {
        let (_s, memory) =
            charter_substitution_hit("charter persona remember \"a `x` b\"").expect("a hit");
        assert!(!memory.contains("--from-file"), "{memory}");
        let (_s, report) =
            charter_substitution_hit("charter report bug \"a `x` b\"").expect("a hit");
        assert!(
            report.contains("`--from-file <path>` (or `--stdin`)"),
            "{report}"
        );
    }

    /// The denial names the SHAPE and never the matched text — the rule #92's `never_says` was
    /// added for.
    #[test]
    fn the_denial_never_quotes_the_value() {
        let secret = "hunter2istheword";
        let cmd = format!("gh issue create --body \"a `echo {secret}` b\"");
        let (shape, said) = forge_substitution_hit(&cmd).expect("a hit");
        assert_eq!(shape, "`");
        assert!(!said.contains(secret), "{said}");
        let cmd = format!("charter persona remember \"a $(echo {secret}) b\"");
        let (shape, said) = charter_substitution_hit(&cmd).expect("a hit");
        assert_eq!(shape, "$(");
        assert!(!said.contains(secret), "{said}");
    }

    /// `git commit` and charter's commit-message commands stay out, measured the other way.
    #[test]
    fn a_commit_message_is_not_on_either_table() {
        assert_eq!(
            forge_substitution_hit("git commit -m \"$(cat <<'EOF'\na `x` b\nEOF\n)\""),
            None
        );
        assert_eq!(
            charter_substitution_hit("charter save -m \"a `x` b\""),
            None
        );
        assert_eq!(
            charter_substitution_hit("charter workspace rename -m \"a `x` b\""),
            None
        );
    }

    /// The two prefilters cannot change a verdict; they only avoid a call. Both directions are
    /// asserted so a mutation that deletes one is caught by the differential rather than here —
    /// what is pinned here is that they are PREFILTERS.
    #[test]
    fn the_prefilters_only_avoid_a_call() {
        // A command that passes both filters and is still allowed.
        assert_eq!(forge_substitution_hit("gh issue list `true`"), None);
        assert_eq!(charter_substitution_hit("charter recall `true`"), None);
        // A command the table would match, with no substitution on the line.
        assert_eq!(forge_substitution_hit("gh issue create --body x"), None);
        assert_eq!(charter_substitution_hit("charter persona remember x"), None);
    }
}
