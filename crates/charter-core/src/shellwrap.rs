//! What a segment's **program** actually is, once the shell's own plumbing is off the front.
//!
//! A port of `charter/hooks.py`'s `_split_env_chdir` and the tables it reads: `_split_env`,
//! `_wrapper_option`, `_flag_name_value`, `_redirect_reads`, `_git_globals`, `_WRAPPERS`,
//! `_SHELL_KEYWORDS`, `_WRAPPER_VALUE_FLAGS`, `_WRAPPER_NOVALUE_LETTERS`,
//! `_WRAPPER_READ_FLAGS`, `_WRAPPER_CHDIR_FLAGS`, `_WRAPPER_LEADING_OPERANDS`,
//! `_WRAPPER_ASSIGN_OPERANDS`, `_SPLIT_STRING_FLAGS`, `_DURATION_RE`, `_ENV_ASSIGN_RE`,
//! `_REDIRECT_RE`, `_REDIRECT_READ_RE`, `_REDIRECTIONS`, `_REDIRECT_READS` and
//! `_GIT_VALUE_OPTS`.
//!
//! # Nothing calls this yet, on purpose
//!
//! `charter hook pretooluse` is ONE switch, and [`crate::shellseg`]'s header says why it does
//! not move until every arm is ported. **This is stage 2 of six and is wired to nothing.** It
//! comes in with the heredoc layout ([`crate::heredoc`]) because `_line_pipelines` names each
//! pipeline's program through `_split_env`, which is this.
//!
//! # Why the program is not token 0
//!
//! Every refusal above this reads `prog` from token 0 of a segment. Three things sit in front of
//! it and none of them is the program: `VAR=value` assignments, redirections (`< <vault> cat`
//! prints a vault while token 0 is `<`), and the wrapper run — `env`, `sudo`, `xargs`, `timeout`
//! and the shell keywords a segment can begin with. Each of those was a live vault read before it
//! was modelled, and the Python docstrings name the commands.
//!
//! The half that is easy to get wrong is the **option grammar**, because a wrapper flag that
//! takes a value hides the program behind it. `env -P /bin cat <vault>` named `/bin` as the
//! program; `sudo -T 5 cat <vault>` named `5`; `env -iC.charter/vaults cat x.json` relocated into
//! the vault directory because a short option was not the first thing in its own token. So a
//! short option is its LETTER and not its position, and a letter this grammar cannot place stops
//! the walk and reports **the rest of the segment as files this command may open** — which is
//! what keeps a short table a false negative instead of a bypass.
//!
//! # Two `\d`s and a `$` that are not Rust's
//!
//! `_REDIRECT_RE` and `_DURATION_RE` are CPython patterns, and CPython's `\d` on a `str` is
//! `\p{Nd}` — every Unicode decimal digit, not `[0-9]` — while CPython's `$` matches at the end
//! of the string **or just before a newline that ends it**. Both are reproduced rather than
//! approximated: the `regex` crate's `\d` is the same `\p{Nd}`, and the `$` is spelled `\n?$`
//! because the crate's own `$` is end-of-haystack. A token really can hold a newline (`cat "<
//! "` is one word to a shell), so this is reachable and not a curiosity.

use std::collections::{HashMap, VecDeque};
use std::sync::OnceLock;

use regex::Regex;

use crate::shellseg;

/// The REDIRECTION operators, longest first — `_REDIRECTIONS`. A redirection is not a control
/// operator and not a word: it is the shell's own file plumbing, and it may sit anywhere in a
/// simple command, the front included.
const REDIRECTIONS: [&str; 9] = ["<<<", "<<", "<>", "<&", ">>", ">&", ">|", "<", ">"];

/// The subset that opens a PATH FOR READING. `<<` and `<<<` name a heredoc delimiter and a
/// here-string, and `<&` duplicates a descriptor that is already open; none is a path.
const REDIRECT_READS: [&str; 2] = ["<>", "<"];

/// Programs that RUN another program: their own argv[1..] is the real invocation — `_WRAPPERS`.
/// Without stripping these the answer to "what is this command" is `env`, and
/// `env cat .charter/vaults/x.json` — verified live — printed a vault.
const WRAPPERS: [&str; 17] = [
    "env", "command", "builtin", "exec", "time", "nice", "ionice", "chrt", "nohup", "setsid",
    "stdbuf", "timeout", "unbuffer", "sudo", "doas", "su-exec",
    // `xargs` is here for the same reason the rest are: `xargs cat` is a `cat`.
    "xargs",
];

/// Shell KEYWORDS that can stand where a program stands once a command has been segmented.
/// `if true; then cat <vault>; fi` segments into `then cat <vault>`, whose token 0 is `then`.
pub(crate) const SHELL_KEYWORDS: [&str; 21] = [
    "if", "then", "elif", "else", "fi", "while", "until", "do", "done", "for", "in", "case",
    "esac", "select", "function", "!", "{", "}", "(", ")", "$",
];

/// Wrapper flags whose VALUE is a SEPARATE token, per wrapper — `_WRAPPER_VALUE_FLAGS`.
///
/// Per wrapper rather than one flat set, because the same spelling differs: `env -i` ignores the
/// environment and takes nothing, while `xargs -i` and `stdbuf -i` do take a value, and a flat
/// set would consume the program itself for `env -i cat <vault>`.
///
/// **The order inside each row is load-bearing**, because [`flag_name_value`] takes the FIRST
/// spelling that prefixes the token, exactly as Python's `next(f for f in spellings if …)` does.
const WRAPPER_VALUE_FLAGS: [(&str, &[&str]); 11] = [
    // `-P` (BSD `env -P utilpath`) was missing once, and a missing value flag is a fail-open:
    // `env -P /bin cat <vault>` named `/bin` as the program and printed the vault.
    ("env", &["-u", "--unset", "-C", "--chdir", "-P"]),
    // `-T` (`--command-timeout`) was missing for the same reason: `sudo -T 5 cat <vault>` named
    // `5` as the program.
    (
        "sudo",
        &[
            "-u",
            "--user",
            "-g",
            "--group",
            "-p",
            "--prompt",
            "-C",
            "--close-from",
            "-r",
            "--role",
            "-t",
            "--type",
            "-h",
            "--host",
            "-D",
            "--chdir",
            "-R",
            "--chroot",
            "-U",
            "--other-user",
            "-T",
            "--command-timeout",
        ],
    ),
    ("doas", &["-u", "-C", "-a"]),
    ("nice", &["-n", "--adjustment"]),
    (
        "ionice",
        &["-c", "--class", "-n", "--classdata", "-p", "--pid"],
    ),
    ("chrt", &["-p", "--pid"]),
    ("timeout", &["-s", "--signal", "-k", "--kill-after"]),
    (
        "stdbuf",
        &["-i", "--input", "-o", "--output", "-e", "--error"],
    ),
    ("exec", &["-a"]),
    ("time", &["-f", "--format", "-o", "--output"]),
    // `-e`/`--eof` are deliberately ABSENT: GNU xargs takes their value ATTACHED and optional
    // (`-eEOF`, `--eof=EOF`), so consuming the next token swallowed the program — `xargs -e cat
    // <vault>` named `cat` as the flag's value and the VAULT PATH as the program, which is the
    // fail-open this table exists to prevent. `-E` does take a separate value and stays.
    // Attached spellings are handled by the `=`/glued branches.
    (
        "xargs",
        &[
            "-I",
            "--replace",
            "-n",
            "--max-args",
            "-L",
            "--max-lines",
            "-P",
            "--max-procs",
            "-d",
            "--delimiter",
            "-s",
            "--max-chars",
            "-a",
            "--arg-file",
            "-E",
            "-J",
            "-R",
            "-S",
        ],
    ),
];

/// Short option LETTERS that take NO value, per wrapper — `_WRAPPER_NOVALUE_LETTERS`, the other
/// half of [`WRAPPER_VALUE_FLAGS`] and the half a bundle walk cannot do without.
///
/// getopt bundles short options, so `-iC<dir>` is `-i -C <dir>` and the chdir flag is not the
/// first thing in its own token. **The value table is consulted FIRST for every letter**, so a
/// letter in both would behave as value-taking — the fail-closed way round. That ordering is
/// deliberately **not** load-bearing and is checked rather than trusted: the two tables are
/// disjoint for every wrapper that appears in both (swept here, zero overlaps), no no-value table
/// holds a `-`, and no value table holds a bare `--`. Those three are what would make the order,
/// and [`wrapper_option`]'s comment about a long option needing no branch, start mattering again.
const WRAPPER_NOVALUE_LETTERS: [(&str, &str); 10] = [
    ("env", "0iv"), // BSD: `env [-0iv] [-C workdir] [-P utilpath] [-S string]`
    ("sudo", "ABbEeHiKklNnPSsVv"),
    ("doas", "Lns"),
    ("xargs", "0oprtx"),
    ("timeout", "v"),
    ("exec", "cl"), // bash: `exec [-cl] [-a name]`
    ("command", "pvV"),
    ("setsid", "cfw"),
    ("time", "alpqv"),
    ("ionice", "th"),
];

/// Wrapper flags naming a file the WRAPPER ITSELF opens — `_WRAPPER_READ_FLAGS`.
/// `xargs -a .charter/vaults/x.json echo` prints the vault, and the only program named on the
/// line is `echo`.
const WRAPPER_READ_FLAGS: [(&str, &[&str]); 1] = [("xargs", &["-a", "--arg-file"])];

/// Wrapper flags that CHANGE DIRECTORY before running the program — `_WRAPPER_CHDIR_FLAGS`. Its
/// own table because the same letter means something else one wrapper over: `sudo -C` is
/// `--close-from` (a file-descriptor number) while `env -C` is the chdir.
///
/// Stripping these was a BYPASS, not a fix: reading `-C` only in order to skip it left
/// `env -C .charter/vaults cat x.json` naming no guarded path anywhere.
const WRAPPER_CHDIR_FLAGS: [(&str, &[&str]); 2] =
    [("env", &["-C", "--chdir"]), ("sudo", &["-D", "--chdir"])];

/// `env -S 'cat <vault>'` packs the whole command into ONE token. Treated as tokens rather than
/// as a value to skip, because skipping it would leave an empty argv and the guard would see no
/// program at all.
const SPLIT_STRING_FLAGS: [&str; 2] = ["-S", "--split-string"];

/// Bare POSITIONAL operands a wrapper takes BEFORE the program — `_WRAPPER_LEADING_OPERANDS`.
/// `chrt 5 cat <vault>` and `su-exec root cat <vault>` both named the operand as the program.
/// `timeout`'s duration is the same shape and keeps its own branch, because "the token looks
/// like a duration" is what a count would lose.
const WRAPPER_LEADING_OPERANDS: [(&str, usize); 2] = [("chrt", 1), ("su-exec", 1)];

/// Wrappers whose OPERAND SCAN accepts `name=value` — by their OWN rule, which is not the
/// shell's. `env`'s scan is `strchr(arg, '=')`: any argument containing an `=` is an assignment,
/// so `env a-b=1 cat .charter/vaults/x.json` runs the `cat` while a shell-identifier test named
/// `a-b=1` as the program and printed the vault.
const WRAPPER_ASSIGN_OPERANDS: [&str; 2] = ["env", "sudo"];

/// git's own global options that take a VALUE. git stops reading its globals at the first
/// non-option token, and every option after that belongs to the SUBCOMMAND.
const GIT_VALUE_OPTS: [&str; 7] = [
    "-c",
    "-C",
    "--config-env",
    "--git-dir",
    "--work-tree",
    "--namespace",
    "--super-prefix",
];

fn table<'a, V: Copy>(rows: &'a [(&'a str, V)], key: &str) -> Option<V> {
    rows.iter().find(|(k, _)| *k == key).map(|(_, v)| *v)
}

/// `_REDIRECT_RE`: an optional file-descriptor number, then one of [`REDIRECTIONS`].
fn redirect_re() -> &'static Regex {
    static ONCE: OnceLock<Regex> = OnceLock::new();
    ONCE.get_or_init(|| anchored(&REDIRECTIONS))
}

/// `_REDIRECT_READ_RE`: the same, for the redirections that open a path FOR READING.
fn redirect_read_re() -> &'static Regex {
    static ONCE: OnceLock<Regex> = OnceLock::new();
    ONCE.get_or_init(|| anchored(&REDIRECT_READS))
}

/// One table, two recognisers, built from the same list so the splitter and the recogniser
/// cannot come to disagree about what a redirection is.
///
/// `\n?$` is CPython's `$`, not the crate's; see the module header.
fn anchored(ops: &[&str]) -> Regex {
    let alts = ops
        .iter()
        .map(|o| regex::escape(o))
        .collect::<Vec<_>>()
        .join("|");
    Regex::new(&format!(r"^\d*(?:{alts})\n?$")).expect("a pattern this module wrote")
}

/// `_DURATION_RE`: `timeout 5 cat <vault>` — the duration is a bare positional, not a flag.
fn duration_re() -> &'static Regex {
    static ONCE: OnceLock<Regex> = OnceLock::new();
    ONCE.get_or_init(|| {
        Regex::new(r"^\d+(?:\.\d+)?[smhd]?\n?$").expect("a pattern this module wrote")
    })
}

/// `_ENV_ASSIGN_RE`: `^[A-Za-z_][A-Za-z0-9_]*=`, a SHELL identifier followed by `=`.
///
/// Written out rather than compiled, because the class is ASCII and because the distinction it
/// draws against [`WRAPPER_ASSIGN_OPERANDS`] is the whole of the Python's `#555` note: this is
/// the shell's question, asked at the front of a segment, and `env`'s own operand scan is a
/// different predicate over the same bytes.
pub fn is_env_assignment(tok: &str) -> bool {
    let mut chars = tok.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    for c in chars {
        if c == '=' {
            return true;
        }
        if !(c.is_ascii_alphanumeric() || c == '_') {
            return false;
        }
    }
    false
}

/// `os.path.basename` on posix: everything after the last `/`.
pub fn basename(path: &str) -> &str {
    match path.rfind('/') {
        Some(i) => &path[i + 1..],
        None => path,
    }
}

/// `os.path.basename(tok).lower()`.
///
/// `str.lower()` and `str::to_lowercase` are both Unicode full case mappings, and the two agree
/// on every character a table here could match: the sets are ASCII, so a divergence could only
/// come from a non-ASCII character whose lowercase is ASCII — U+212A KELVIN SIGN and U+017F LATIN
/// SMALL LETTER LONG S — and both engines fold both the same way. The fuzz alphabet carries
/// `Σ`, `İ`, `ı`, `K` (U+212A) and `ẞ` so that claim is measured rather than asserted.
pub fn base_lower(tok: &str) -> String {
    basename(tok).to_lowercase()
}

/// Whether `tok` is a REDIRECTION token — `_REDIRECT_RE.match`.
///
/// It is never the program and never the program's operand, and it may appear anywhere in a
/// simple command, the front included: `< .charter/vaults/x.json cat` prints the vault while
/// token 0 is `<`, so every guard that reads token 0 as the program saw no reader, and the path
/// was not an operand of anything either.
pub fn is_redirect_token(tok: &str) -> bool {
    redirect_re().is_match(tok)
}

/// The paths the SHELL opens for reading on behalf of one segment's command —
/// `_redirect_reads`.
///
/// Scanned across the WHOLE segment, not just the front, because a redirection binds to its
/// command from either side: `cat < <vault>` and `< <vault> cat` are the same open, and so is
/// `tee < <vault>`, whose program is in no reader list at all.
pub fn redirect_reads(toks: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for (i, t) in toks.iter().enumerate() {
        // Python's `enumerate(toks[:-1])`: the last token has no successor to be a target.
        if i + 1 >= toks.len() {
            break;
        }
        if redirect_read_re().is_match(t) {
            out.push(toks[i + 1].clone());
        }
    }
    out
}

/// `(flag name, the value ATTACHED to it)` for one option token — `_flag_name_value`. An empty
/// value means "not attached here"; the caller decides whether the NEXT token is it.
///
/// **The glued SHORT form is read before the long form's `=`.** getopt gives a short option
/// everything glued after it, `=` included, so `-Sfoo=1` packs `foo=1`, while
/// `--split-string=foo=1` splits at its FIRST `=` and packs the same thing. Reading the `=` rule
/// first split the glued form at the packed value's OWN `=`: `env -Sfoo=1 cat <vault>` came back
/// with `1` as the program and printed the vault.
///
/// **What is load-bearing is the DISJOINTNESS, not the order, and that was measured rather than
/// assumed.** A glued short form never starts with `--`, and the `=` rule requires it, so the two
/// arms cannot both match one token: swapping them changes **no answer** over 3,000 fuzz cases
/// and none of the recorded corpus. Dropping the `--` requirement *and* putting the `=` rule
/// first — which is the #547 shape — changes 21. The Python's own comment reads as though the
/// ordering were the repair; the ordering is a consequence, and this is the fact that holds.
pub fn flag_name_value(tok: &str, spellings: &[&str]) -> (String, String) {
    let glued = spellings.iter().find(|f| {
        !f.starts_with("--") && tok.starts_with(**f) && tok.chars().count() > f.chars().count()
    });
    if let Some(f) = glued {
        // `strip_prefix` rather than `tok[f.len()..]`: the `find` above already proved the
        // prefix, and this says so in the type instead of in a comment.
        let rest = tok.strip_prefix(*f).unwrap_or_default();
        return ((*f).to_string(), rest.to_string());
    }
    if tok.starts_with("--")
        && let Some((name, value)) = tok.split_once('=')
    {
        return (name.to_string(), value.to_string());
    }
    (tok.to_string(), String::new())
}

/// What [`wrapper_option`] read out of one of a wrapper's option tokens.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WrapperOption {
    /// The flag's name, or `""` when this token named none.
    pub name: String,
    /// The value attached to it, or `""`.
    pub value: String,
    /// The value is the NEXT token.
    pub wants_next: bool,
    /// **The honest half.** `false` means this grammar could not account for the token, so the
    /// next token is NOT reliably the program — a value flag nobody has listed looks exactly
    /// like a flag that takes nothing. Callers that may not miss treat the rest of the segment
    /// as reachable; see [`split_env_chdir`].
    pub placed: bool,
}

/// One of `base`'s option tokens, read — `_wrapper_option`.
///
/// **A short option is its LETTER, not its position in the token.** getopt bundles, so `-iC<dir>`
/// is `-i -C <dir>`; matching a flag by `tok.startswith(flag)` only ever sees a short option
/// written FIRST, and `env -iC.charter/vaults cat x.json` relocated into the vault directory
/// while three other spellings of the same flag were denied.
///
/// So the token is read in this order: the long `--flag=value` form and a value glued to a short
/// flag that is FIRST ([`flag_name_value`]); then, for a single-dash token, the letters are
/// walked — a letter in the value table takes the rest of the token (or the next token when
/// nothing is left), a letter in the no-value table is stepped over, and anything else ENDS the
/// walk UNPLACED rather than being guessed at.
///
/// **A long option needs no branch of its own, and the Python had one it deleted.** `--nonesuch`
/// walks into the loop, whose first character is the second `-`; that is in no wrapper's no-value
/// letters, so the walk ends unplaced on its first step — the same answer an early return gave.
/// Written down as inert here for the same reason it is written down there: a `-` appearing in a
/// no-value table is what would make it start mattering again.
pub fn wrapper_option(base: &str, tok: &str) -> WrapperOption {
    let placed_nothing = |placed: bool| WrapperOption {
        name: String::new(),
        value: String::new(),
        wants_next: false,
        placed,
    };
    if tok == "--" {
        // POSIX end-of-options: placed, and it takes nothing. Not left to the walk below, where
        // it would come back UNPLACED and put the whole rest of the segment into `reads` for a
        // token that means nothing more than "the options stop here".
        return placed_nothing(true);
    }
    let mut takes: Vec<&str> = table(&WRAPPER_VALUE_FLAGS, base).unwrap_or(&[]).to_vec();
    if base == "env" {
        takes.extend_from_slice(&SPLIT_STRING_FLAGS);
    }
    let (name, value) = flag_name_value(tok, &takes);
    if !value.is_empty() {
        // `--chdir=<dir>`, `-C<dir>`, `-Sfoo=1`
        return WrapperOption {
            name,
            value,
            wants_next: false,
            placed: true,
        };
    }
    if takes.contains(&name.as_str()) {
        // `-C <dir>`, `--chdir <dir>`
        return WrapperOption {
            name,
            value: String::new(),
            wants_next: true,
            placed: true,
        };
    }
    let novalue = table(&WRAPPER_NOVALUE_LETTERS, base).unwrap_or("");
    let chars: Vec<char> = tok.chars().collect();
    for (j, ch) in chars.iter().enumerate().skip(1) {
        let letter = format!("-{ch}");
        if takes.contains(&letter.as_str()) {
            let attached: String = chars[j + 1..].iter().collect();
            let wants_next = attached.is_empty();
            return WrapperOption {
                name: letter,
                value: attached,
                wants_next,
                placed: true,
            };
        }
        if !novalue.contains(*ch) {
            return placed_nothing(false); // a letter this grammar cannot place
        }
    }
    placed_nothing(true) // every letter placed, none takes a value
}

/// `(git's OWN options, the subcommand and everything after)` — `_git_globals`.
///
/// git stops reading its own globals at the first non-option token, and every option after that
/// belongs to the SUBCOMMAND: `-C` means "change directory" in `git -C <dir> switch neu` and
/// means `--force-create` in `git switch -C neu`. Stripping `-C <value>` from anywhere read the
/// second as the first, and the guard stood aside while git created a branch at the plane root.
///
/// `args` INCLUDES the program, which is dropped here, so the caller can take `rest[0]` as the
/// subcommand rather than as `git`.
pub fn git_globals(args: &[String]) -> (Vec<String>, Vec<String>) {
    // `args[1:]` when there is one, `[]` when there is not — Python's slice, which clamps.
    let argv = args.get(1..).unwrap_or(&[]);
    let mut i = 0usize;
    while i < argv.len() && argv[i].starts_with('-') {
        i += if GIT_VALUE_OPTS.contains(&argv[i].as_str()) {
            2
        } else {
            1
        };
    }
    // Python's slice clamps; `i` can pass the end when the last option wanted a value.
    let cut = i.min(argv.len());
    (argv[..cut].to_vec(), argv[cut..].to_vec())
}

/// What one tokenized segment turns out to be — `_split_env_chdir`'s five-tuple.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Invocation {
    /// The program, or `""` when nothing is left to name one.
    pub prog: String,
    /// The `VAR=value` assignments in front of it, which keep flowing across a wrapper — what
    /// lets the one-credential guard see `env GIT_SSH_COMMAND=/tmp/k git push`.
    pub env: Vec<String>,
    /// The program and its arguments.
    pub argv: Vec<String>,
    /// The directory a wrapper's own chdir flag moves the program to. RETURNED rather than
    /// discarded, because the value is what makes a later relative operand resolve.
    pub chdir: String,
    /// The files this command OPENS without them being an operand of a reader: a wrapper's own
    /// file flag, the target of an input redirection, and — where the option grammar ran out —
    /// the whole rest of the segment.
    pub reads: Vec<String>,
}

/// `(program, env-assignment prefixes, argv)` — [`split_env_chdir`] without the directory or the
/// files, for the guards that only name a program.
pub fn split_env(toks: &[String]) -> (String, Vec<String>, Vec<String>) {
    let it = split_env_chdir(toks);
    (it.prog, it.env, it.argv)
}

/// One tokenized segment, with the shell's plumbing off the front — `_split_env_chdir`.
///
/// Strips three things before naming the program: `VAR=value` assignments, REDIRECTIONS, and the
/// wrapper/keyword run. They interleave (`sudo FOO=bar env BAZ=qux cat …`, `2>/dev/null env cat
/// …`), so this is one loop rather than three.
///
/// Deliberately NOT re-parsing `sh -c '<string>'`: that is a documented limit of the guard. A
/// wrapper is a program that runs its own argv; `sh -c` runs a string.
///
/// **A `-…` token is read as an OPTION wherever it stands**, even past the point where the
/// wrapper's own grammar has stopped reading options. `env` is really
/// `option* assignment* utility arg*` — `env FOO=1 -i sh -c …` answers *"env: -i: No such file
/// or directory"* — but modelling that faithfully would deny LESS than the Python does, and it
/// is the wrong direction anyway: reading a stray `-Sbar=2` as one more `-S` unpacks it and
/// keeps scanning rightward for a program, and reading a stray `-i` as a flag steps over a token
/// that cannot be a reader. Neither can hand the guard a program that is not there.
pub fn split_env_chdir(toks: &[String]) -> Invocation {
    let mut env: Vec<String> = Vec::new();
    let mut chdir = String::new();
    // Computed on the ORIGINAL tokens, before anything is popped: a redirection binds to its
    // command from either side, so the whole segment is the scan.
    let mut reads: Vec<String> = redirect_reads(toks);
    let mut toks: VecDeque<String> = toks.iter().cloned().collect();

    while let Some(front) = toks.front().cloned() {
        if redirect_re().is_match(&front) {
            // A redirection in FRONT of the command — `< <vault> cat`, `2>/dev/null env …`.
            // Neither it nor its target is the program; the target is already in `reads`.
            toks.pop_front();
            toks.pop_front();
            continue;
        }
        if is_env_assignment(&front) {
            env.push(toks.pop_front().expect("the front was just read"));
            continue;
        }
        let base = base_lower(&front);
        if !SHELL_KEYWORDS.contains(&front.as_str()) && !WRAPPERS.contains(&base.as_str()) {
            break;
        }
        toks.pop_front();
        let mut leading = table(&WRAPPER_LEADING_OPERANDS, base.as_str()).unwrap_or(0);
        while let Some(nxt) = toks.front().cloned() {
            if nxt.starts_with('-') && nxt.chars().count() > 1 {
                toks.pop_front();
                // ONE reading of the flag, giving both its NAME and its VALUE: the name decides
                // whether the next token is the program, the value is where a chdir flag
                // relocates to. Two readings is how the value came to be lost.
                let opt = wrapper_option(&base, &nxt);
                let mut value = opt.value;
                if opt.wants_next
                    && let Some(v) = toks.pop_front()
                {
                    value = v;
                    if nxt != opt.name {
                        // A value taken from the next token because a letter INSIDE a bundle
                        // asked for it (`env -iC <dir>`, which really does chdir). That token is
                        // the program if this grammar is wrong about the letter's arity, so the
                        // rest of the segment is reported as reachable.
                        reads.extend(toks.iter().cloned());
                    }
                }
                if !opt.placed {
                    // An option this wrapper's grammar could not account for. The token after it
                    // is NOT reliably the program — a value flag nobody listed is
                    // indistinguishable from one that takes nothing, which is how
                    // `env -P /bin cat <vault>` named `/bin` as the program and printed the
                    // vault. So the guard stops trusting the program and reports the rest of the
                    // segment as files this command may open. This is what keeps a SHORT table a
                    // false negative instead of a bypass.
                    reads.extend(toks.iter().cloned());
                    continue;
                }
                if base == "env" && SPLIT_STRING_FLAGS.contains(&opt.name.as_str()) {
                    // `env -S 'cat <vault>'` packs a whole command into one token, and `env -iS…`
                    // packs it behind a bundled `-i`. Treated as tokens rather than skipped,
                    // because skipping leaves an empty argv and the guard sees no program at all.
                    let unpacked = shellseg::posix_split(&value)
                        .unwrap_or_else(|_| shellseg::py_split(&value));
                    for (k, word) in unpacked.into_iter().enumerate() {
                        toks.insert(k, word);
                    }
                    continue;
                }
                if !value.is_empty() {
                    if table(&WRAPPER_CHDIR_FLAGS, base.as_str())
                        .unwrap_or(&[])
                        .contains(&opt.name.as_str())
                    {
                        chdir = value.clone();
                    }
                    if table(&WRAPPER_READ_FLAGS, base.as_str())
                        .unwrap_or(&[])
                        .contains(&opt.name.as_str())
                    {
                        reads.push(value);
                    }
                }
                continue;
            }
            if WRAPPER_ASSIGN_OPERANDS.contains(&base.as_str()) && nxt.contains('=') {
                // THIS wrapper's rule for what an assignment is, not the shell's.
                env.push(toks.pop_front().expect("the front was just read"));
                continue;
            }
            if leading > 0 {
                leading -= 1;
                toks.pop_front(); // the wrapper's own operand, not the program
                continue;
            }
            if base == "timeout" && duration_re().is_match(&nxt) {
                toks.pop_front(); // the duration, not the program
                continue;
            }
            break;
        }
    }
    let argv: Vec<String> = toks.into_iter().collect();
    Invocation {
        prog: argv.first().cloned().unwrap_or_default(),
        env,
        argv,
        chdir,
        reads,
    }
}

/// The shell builtins that EXPORT — `_EXPORT_BUILTINS`. `declare` and `typeset` are `export`
/// spelled two other ways, and they bundle their `x` (`declare -gx`).
const EXPORT_BUILTINS: [&str; 3] = ["export", "declare", "typeset"];

/// For each segment, the `VAR=value` assignments an EARLIER segment of the same command line has
/// exported into the environment that segment will run in — `_exported_env`.
///
/// **The property is "what this command line has set for its later segments", and the spelling
/// that stood in for it was "an assignment attached to the invocation" (#496).** [`split_env`]
/// hands a guard the prefix on the command itself, so `GIT_DIR=<plane>/.git git checkout feature`
/// was refused — and `export GIT_DIR=<plane>/.git && git checkout feature`, the same variable
/// reaching the same git for the same reason, was allowed. Verified end to end against git
/// 2.50.1. The one-credential guard ([`crate::credguard`]) had the identical gap on
/// `export GIT_SSH_COMMAND=…`, found by sweeping the shape rather than by a report, and is wired
/// to the same answer.
///
/// Parallel to `segments` rather than folded into the caller's walk, because two guards ask it
/// and a second hand-written copy would grow its own blind spots.
///
/// **The shapes modelled, each because a shell really does it** (checked against bash 5 and zsh,
/// which agree):
///
/// * `export NAME=VALUE` — and `declare -x` / `typeset -x`.
/// * `NAME=VALUE` as a segment of its own, then `export NAME`. A bare assignment segment sets a
///   SHELL variable and exports nothing — `FOO=1; <child>` really does leave `FOO` unset in the
///   child — so it is tracked but not exported until something exports it.
/// * `set -a` (`set -o allexport`), after which a bare assignment segment IS exported.
///
/// **This environment only ever GROWS.** `unset`, `export -n` and a subshell that ends are not
/// modelled: forgetting a variable is the fail-OPEN direction, and a list that gets shorter as
/// the command line gets longer is a bypass by construction. The cost is refusing
/// `export GIT_DIR=x && unset GIT_DIR && git checkout feature`, which nobody types by accident.
///
/// **The honest boundary is `cd`'s.** A `$(…)`, a sourced file, a `~/.bashrc` and a variable
/// already in the session's environment before the hook ran are all outside it — the
/// `PreToolUse` payload carries the command and the cwd, not the environment the command will
/// inherit. Stated limits, not gaps.
pub fn exported_env(segments: &[Vec<String>]) -> Vec<Vec<String>> {
    let mut out: Vec<Vec<String>> = Vec::with_capacity(segments.len());
    let mut exported: Vec<String> = Vec::new();
    // Python's `dict[str, str]`: last write wins, and the value is the whole `NAME=VALUE` token.
    let mut shell_vars: HashMap<String, String> = HashMap::new();
    let mut allexport = false;
    for toks in segments {
        out.push(exported.clone());
        let mut i = 0;
        while i < toks.len() && SHELL_KEYWORDS.contains(&toks[i].as_str()) {
            i += 1;
        }
        let rest = &toks[i..];
        let Some(first) = rest.first() else { continue };
        let prog = base_lower(first);
        let args = &rest[1..];
        if prog == "set" {
            // `set -a`, `set -ax`, `set -o allexport` — the LETTER, not the token, for the reason
            // `wrapper_option` walks letters: `-ax` is `-a -x`.
            if args.iter().any(|a| {
                (a.starts_with('-') && !a.starts_with("--") && a[1..].contains('a'))
                    || a == "allexport"
            }) {
                allexport = true;
            }
            continue;
        }
        if EXPORT_BUILTINS.contains(&prog.as_str()) {
            if prog != "export"
                && !args
                    .iter()
                    .any(|a| a.starts_with('-') && !a.starts_with("--") && a[1..].contains('x'))
            {
                // `declare FOO=1` without `-x` is a shell variable, not an export.
                for a in args {
                    if is_env_assignment(a) {
                        shell_vars.insert(assignment_name(a), a.clone());
                    }
                }
                continue;
            }
            // No `starts_with('-')` skip here, and that is deliberate rather than an oversight:
            // every key of `shell_vars` came from [`is_env_assignment`], so each is a shell
            // identifier and none starts with `-`. A flag token can therefore neither be recorded
            // by the first arm nor found by the second. The Python deleted that skip and pins the
            // reason with a test, since an unpinned reason is how dead code comes back to life.
            for a in args {
                if is_env_assignment(a) {
                    shell_vars.insert(assignment_name(a), a.clone());
                    exported.push(a.clone());
                } else if let Some(v) = shell_vars.get(a) {
                    exported.push(v.clone()); // `FOO=1; export FOO`
                }
            }
            continue;
        }
        if rest.iter().all(|t| is_env_assignment(t)) {
            // A segment that is NOTHING but assignments: a shell variable each, exported only
            // under `set -a`.
            for t in rest {
                shell_vars.insert(assignment_name(t), t.clone());
                if allexport {
                    exported.push(t.clone());
                }
            }
        }
    }
    out
}

/// `tok.split("=", 1)[0]` for a token [`is_env_assignment`] has already accepted.
fn assignment_name(tok: &str) -> String {
    match tok.split_once('=') {
        Some((name, _)) => name.to_string(),
        None => tok.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A flag token can be neither recorded as a variable nor found as one — the fact that makes
    /// the deleted `starts_with('-')` skip in [`exported_env`] dead code rather than a guard.
    #[test]
    fn a_flag_cannot_be_mistaken_for_a_variable_name() {
        assert!(!is_env_assignment("-x"));
        assert!(!is_env_assignment("--global"));
        let segs = vec![
            vec!["export".into(), "-x".into(), "FOO=1".into()],
            vec!["git".into(), "push".into()],
        ];
        assert_eq!(exported_env(&segs)[1], vec!["FOO=1".to_string()]);
    }

    /// The #496 shape: an EARLIER segment's export reaches a later segment's git.
    #[test]
    fn an_export_reaches_a_later_segment() {
        let segs = vec![
            vec!["export".into(), "GIT_SSH_COMMAND=/tmp/k".into()],
            vec!["git".into(), "push".into()],
        ];
        assert_eq!(
            exported_env(&segs)[1],
            vec!["GIT_SSH_COMMAND=/tmp/k".to_string()]
        );
    }

    /// A bare assignment segment sets a SHELL variable and exports nothing until something
    /// exports it — or until `set -a`.
    #[test]
    fn a_bare_assignment_exports_only_once_something_exports_it() {
        let plain = vec![
            vec!["FOO=1".into()],
            vec!["git".into(), "push".into()],
            vec!["export".into(), "FOO".into()],
            vec!["git".into(), "push".into()],
        ];
        let out = exported_env(&plain);
        assert!(out[1].is_empty());
        assert_eq!(out[3], vec!["FOO=1".to_string()]);

        let allexport = vec![
            vec!["set".into(), "-ax".into()],
            vec!["FOO=1".into()],
            vec!["git".into(), "push".into()],
        ];
        assert_eq!(exported_env(&allexport)[2], vec!["FOO=1".to_string()]);
    }

    /// The three facts that make [`wrapper_option`]'s two written-down inert branches inert.
    ///
    /// Asserted rather than assumed, because an unpinned reason is how dead code comes back to
    /// life: a letter in BOTH tables would make the order the walk consults them in load-bearing,
    /// a `-` in a no-value table would give `--nonesuch` a path through the letter walk instead
    /// of ending it unplaced on its first step, and a bare `--` in a value table would make the
    /// `tok == "--"` early return a behaviour change rather than a shortcut.
    #[test]
    fn the_two_letter_tables_cannot_disagree() {
        for (base, novalue) in WRAPPER_NOVALUE_LETTERS {
            let mut takes: Vec<&str> = table(&WRAPPER_VALUE_FLAGS, base).unwrap_or(&[]).to_vec();
            if base == "env" {
                takes.extend_from_slice(&SPLIT_STRING_FLAGS);
            }
            for ch in novalue.chars() {
                let letter = format!("-{ch}");
                assert!(
                    !takes.contains(&letter.as_str()),
                    "{base}: `{letter}` is in BOTH tables, so which is read first decides",
                );
            }
            assert!(
                !novalue.contains('-'),
                "{base}: a `-` here gives a long option a path through the letter walk",
            );
        }
        for (base, takes) in WRAPPER_VALUE_FLAGS {
            assert!(
                !takes.contains(&"--"),
                "{base}: a bare `--` here makes the end-of-options early return load-bearing",
            );
        }
    }

    /// `git_globals` stops where git stops: at the first token that is not an option.
    ///
    /// `-C` means "change directory" in `git -C <dir> switch neu` and `--force-create` in
    /// `git switch -C neu`. Stripping it from anywhere read the second as the first, and the
    /// guard stood aside while git created a branch at the plane root.
    #[test]
    fn git_stops_reading_its_own_globals_at_the_first_non_option() {
        let argv: Vec<String> = ["git", "-C", "/tmp", "switch", "-C", "neu"]
            .iter()
            .map(|s| (*s).to_string())
            .collect();
        let (globals, rest) = git_globals(&argv);
        assert_eq!(globals, vec!["-C".to_string(), "/tmp".to_string()]);
        assert_eq!(
            rest,
            vec!["switch".to_string(), "-C".to_string(), "neu".to_string()],
        );
        // and the empty argv, which Python's `args[1:]` clamps rather than raising
        assert_eq!(git_globals(&[]), (Vec::new(), Vec::new()));
    }
}
