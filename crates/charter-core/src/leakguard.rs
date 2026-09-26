//! Arm A: the command that would print a secret into the transcript.
//!
//! A port of `charter/hooks.py`'s `_leak_reason` and the closure #92 measured at **64
//! definitions and 2,213 lines** — most of which was already standing when this arrived:
//! [`crate::shellseg`] (stage 1) reads the command line, [`crate::heredoc`] and
//! [`crate::shellwrap`] (stage 2) strip a reader's heredoc and name each segment's real
//! program. What is here is the guard itself and its own neighbourhood:
//! `_names_a_vault_path`, `_file_operands`, `_spliced_operands`, `_gh_file_operands`,
//! `_gh_at_path`, `_is_charter`, `_lines_a_command_could_run`, `_walks_directories`,
//! `_excluded_names`, `_guarded_state_entries`, `_walk_into_guarded_state`,
//! `_glob_selects_inside`, `_walks_into_guarded_state` and the tables each reads.
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
//! # What this catches, and what it does not
//!
//! The Python docstring states the ceiling and this port is bound by the same words: it
//! reliably catches the ORDINARY spellings — a reader with a vault path as an operand, behind
//! any number of wrappers, after a relocation however spelled, through an input redirection,
//! inside an unquoted substitution, on any line of a multi-line command — and it is **defeated
//! by deliberate obfuscation**. A quoted `"$(cat <vault>)"`, a glob, a brace expansion, a path
//! arriving through a variable, `sh -c '<string>'` and a program not in [`READERS`] all walk
//! past it, each because closing it means being a shell. Those limits are not a TODO: charter's
//! own `SECURITY.md` states them and `tests/test_documented_limits.py` pins them as behaviour.
//!
//! # The two halves, and why they are both here
//!
//! The guard decides on an operand's TEXT (`cat .charter/vaults/x.json`) and on where a walk
//! GOES (`grep -rn TOKEN .` from the plane root, which prints every vault file while naming
//! none of them, #474). The first half is pure and the second asks the filesystem, and the
//! difference shows in the port: [`guarded_state_entries`] takes the state directory as an
//! ARGUMENT where the Python reads `config.STATE_DIR`, because the core holds no globals — the
//! caller that knows the plane passes it, and the differential passes a fixture.
//!
//! # Fail-closed, in three places, each of which has been a bypass
//!
//! * an **unparseable** command line is not argv at all, so the raw string is scanned for
//!   `--reveal` and for a vault path. A false deny on an already-malformed command is
//!   survivable; printing a credential is not.
//! * `reads` is asked **before** the program is known: `xargs -a <vault> echo` prints the vault
//!   while the only program named is `echo`, and `< <vault> tee` is opened by the SHELL before
//!   anything is execed. Both can leave `prog` empty.
//! * a relocation is followed however it is spelled — `cd`, `pushd`, and a wrapper's own
//!   `env -C` / `sudo --chdir=`. The last of those was a live bypass: the value used to be read
//!   only to be thrown away, which let a flag do exactly what the `cd` branch had been written
//!   to stop.
//!
//! # Three defects this port found in the frozen Python
//!
//! The Python was the differential's oracle, so a port that was right where the oracle was wrong
//! was a port that failed its own test. Each was filed upstream and each has rows in
//! `fixtures/corpora/shellseg-oracle.jsonl` pinning the answer. The oracle is frozen now (ADR
//! 0046), so fixing one in this port is a decision to change those rows, made on purpose. Two
//! are fixed here, and the corpus rows were changed with them:
//!
//! * **charter#1164, fixed (#351)** — `_file_operands` skipped the value of `-f`/`--file`, and
//!   for `sed`, `awk`, `grep` and `rg` that value is a file the program OPENS. `awk` quotes the
//!   offending source text back on stderr, so it reached the transcript. See [`file_operands`],
//!   which also reads the option grammar the way getopt does now.
//! * **charter#1165, fixed (#350)** — `_excluded_names` read `rg`'s `--glob`/`-g`/`--iglob` as
//!   an exclusion whatever the value said, and for that tool a value WITHOUT a leading `!` is an
//!   inclusion. See [`excluded_names`].
//! * **charter#1166** — `_walk_into_guarded_state` resolves with `Path.resolve()` and guards it
//!   with `except OSError`, which is not every exception that call raises. An operand holding a
//!   **NUL** raises `ValueError`; a **symlink loop** raises `RuntimeError` on CPython 3.11 and
//!   3.12 (3.14 returns the path). Either leaves `pretooluse`, which then exits 1 — and a
//!   PreToolUse hook that exits anything but 2 is read as a non-blocking error, so the tool
//!   RUNS. This port **does not** reproduce that one: `std::fs` answers with an error rather
//!   than raising, so [`crate::pypath::realpath`] returns a path and
//!   [`walk_into_guarded_state`] carries on. It is the only place the two implementations are
//!   known to differ, it is the fail-CLOSED direction, and the differential cannot arbitrate it
//!   — an input the oracle crashes on has no answer to record — so the fixture plane plants
//!   neither, and `pypath`'s own unit test pins what this implementation does.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use regex::Regex;

use crate::heredoc;
use crate::pypath;
use crate::shellseg;
use crate::shellwrap::{self, base_lower};

/// Programs whose ordinary job is to print a file — `_READERS`.
///
/// A NAME check, and therefore an allowlist with a ceiling that cannot be raised by adding
/// names: `python3 -c "print(open(p).read())"`, `node -e`, `base64`, `cp`, `dd`, `jq`, `cut`,
/// `tr`, `curl --upload-file` and `git show HEAD:<path>` all read a file without appearing
/// here. The list is deliberately NOT widened — the missing name is always the next one, while
/// every added name buys false positives on ordinary work, and a guard that denies real
/// commands gets switched off.
const READERS: [&str; 16] = [
    "cat", "less", "more", "head", "tail", "bat", "nl", "tac", "xxd", "od", "strings", "grep",
    "rg", "ag", "awk", "sed",
];

/// Builtins that relocate the shell for every LATER segment — `_CHDIR_BUILTINS`.
///
/// `pushd` is `cd` with a stack, and following only `cd` made `pushd .charter/vaults && cat
/// x.json` a one-word bypass. `popd` is deliberately absent: it returns somewhere this parser
/// cannot know, and forgetting `here` there would be the fail-OPEN direction.
const CHDIR_BUILTINS: [&str; 2] = ["cd", "pushd"];

/// charter itself, including its pre-rename name — `_CHARTER_PROGS`. Kept because this is a
/// security guard and the cost of an extra alternative is one string, while the cost of
/// dropping it is a denial that stops happening on a machine where the old binary is installed.
pub(crate) const CHARTER_PROGS: [&str; 2] = ["charter", "edm"];

/// The state entries whose CONTENT is the secret, as an EXACT name — `_GUARDED_STATE_EXACT`.
///
/// Split from the prefixes for the reason the path pattern anchors `vaults` to a segment:
/// `.charter/vaults.json` is the REGISTRY — provider config and file paths, never a value — and
/// an ordinary read that a wider match denies.
const GUARDED_STATE_EXACT: [&str; 1] = ["vaults"];

/// …and as PREFIXES — `_GUARDED_STATE_PREFIXES`. The same three the path pattern spells as
/// text, listed here as directory entries because this half of the guard is about a walk that
/// never spells them at all.
const GUARDED_STATE_PREFIXES: [&str; 3] = ["browser", "active-", "fingerprint"];

/// Readers that DESCEND INTO DIRECTORIES, and the options that tell them to — `_TREE_WALKERS`.
/// `None` means the program walks the tree with no option at all: `rg` and `ag` search
/// everything below their operand, and below the cwd when given none.
///
/// A subset of [`READERS`] that inherits its ceiling exactly: a name missing from here is a
/// walk this guard does not see, the same way a name missing from [`READERS`] is a read it does
/// not see.
const TREE_WALKERS: [(&str, Option<&[&str]>); 3] = [
    (
        "grep",
        Some(&["-r", "-R", "--recursive", "--dereference-recursive"]),
    ),
    ("rg", None),
    ("ag", None),
];

/// Letters that swallow the rest of their token as a value, per walker — `_CLUSTER_STOPS`.
/// Without this a cluster scan reads the `R` in `grep -eR foo` — which is the PATTERN — as
/// `-R`.
const CLUSTER_STOPS: [(&str, &str); 1] = [("grep", "efmABCd")];

/// Options whose value EXCLUDES a directory from the walk — `_EXCLUDE_OPTS`, less the three
/// in [`SIGNED_GLOB_OPTS`].
///
/// Read so that the fix the denial prints actually runs: a guard that refuses the command it
/// recommends is one people learn to route around. Deliberately permissive and NOT a security
/// boundary — it takes any of these values as a real exclusion without checking that the
/// program would honour it there.
const EXCLUDE_OPTS: [&str; 3] = ["--exclude-dir", "--exclude", "--ignore-dir"];

/// `rg`'s glob options, whose value carries its own SIGN (#350): `!x` excludes `x` and `x`
/// *selects* it. So only a `!` value is an exclusion. And rg lets a later glob override an
/// earlier one, so an inclusion also cancels every exclusion glob before it.
const SIGNED_GLOB_OPTS: [&str; 3] = ["-g", "--glob", "--iglob"];

/// Readers whose FIRST non-flag operand is a program or pattern rather than a file, and the
/// flags that supply it INLINE instead — half of `_SCRIPT_OPERAND`. Kept to the tools where
/// the operand is unambiguous, because getting this wrong in the permissive direction costs a
/// false negative. A flag here takes a value that is only a mention, so the value is skipped.
///
/// A row here also means [`file_operands`] reads the tool's options as getopt does: clusters
/// and attached values. gawk's `-e`/`--source` is its inline program; an awk that does not know
/// the flag only makes the next word look like the program here, and the word after it a file.
const SCRIPT_OPERAND: [(&str, &[&str]); 5] = [
    ("sed", &["-e", "--expression"]),
    ("awk", &["-e", "--source"]),
    ("grep", &["-e", "--regexp"]),
    ("rg", &["-e", "--regexp"]),
    ("ag", &["-e", "--regexp"]),
];

/// The other half of `_SCRIPT_OPERAND`: flags that supply the program or pattern FROM A FILE,
/// whose value is therefore a file the program opens (#351).
///
/// The Python lumped these in with the inline flags and skipped the value, so `awk -f <vault>`
/// was allowed, and awk quotes a program that does not parse back on stderr. Here the value is
/// a file operand like any other. `ag` is absent on purpose: its `-f` is `--follow` and takes
/// no value. gawk's `-E`/`--exec` is `-f` that also ends the options. Another awk that does not
/// know it only makes the next word look like a file here, which is the direction that denies.
const SCRIPT_FILE: [(&str, &[&str]); 4] = [
    ("sed", &["-f", "--file"]),
    ("awk", &["-f", "--file", "-E", "--exec"]),
    ("grep", &["-f", "--file"]),
    ("rg", &["-f", "--file"]),
];

/// Flags whose value is a file the tool opens for something other than its program: a file of
/// ignore rules. The value is an operand, and the program still comes from the first
/// positional.
const FILE_VALUE: [(&str, &[&str]); 3] = [
    ("grep", &["--exclude-from"]),
    ("rg", &["--ignore-file"]),
    ("ag", &["-p", "--path-to-ignore"]),
];

/// Flags whose VALUE is a separate token and is never a path — `_TAKES_VALUE`.
///
/// Per tool, because the same spelling differs: `head -n 5` takes a count, `sed -n` is "quiet"
/// and takes nothing, and treating sed's `-n` as consuming a value swallows the script operand
/// — which would let `sed -n 1p .charter/active-persona` through, a real read.
///
/// **A walker's missing entry is a fail-open, not a false deny.** The Python listed only the
/// count flags. Every other value-taking flag of `grep`, `rg` and `ag` then had its VALUE read as
/// the pattern and the real pattern read as the one file operand. So `rg -t json TOKEN` was a
/// search of a directory named `TOKEN`, and the cwd walk it really does was never asked about.
/// Only flags whose value is REQUIRED on every implementation are listed: an optional value
/// (BSD grep's `--context[=num]`, `--color[=when]`) is never a separate word, and listing one
/// would swallow a real operand.
const TAKES_VALUE: [(&str, &[&str]); 7] = [
    ("head", &["-n", "-c", "--lines", "--bytes"]),
    ("tail", &["-n", "-c", "--lines", "--bytes"]),
    (
        "grep",
        &[
            "-m",
            "-A",
            "-B",
            "-C",
            "--max-count",
            "--after-context",
            "--before-context",
            "--include",
            "--include-dir",
            "--exclude",
            "--exclude-dir",
            "--label",
            "-d",
            "--directories",
            "-D",
            "--devices",
            "--binary-files",
            "--group-separator",
        ],
    ),
    (
        "rg",
        &[
            "-m",
            "-A",
            "-B",
            "-C",
            "--max-count",
            "--after-context",
            "--before-context",
            "--context",
            "-g",
            "--glob",
            "--iglob",
            "-t",
            "--type",
            "-T",
            "--type-not",
            "--type-add",
            "--type-clear",
            "-d",
            "--max-depth",
            "-j",
            "--threads",
            "-M",
            "--max-columns",
            "-E",
            "--encoding",
            "-r",
            "--replace",
            "--sort",
            "--sortr",
            "--max-filesize",
            "--color",
            "--colors",
            "--engine",
            "--pre",
            "--pre-glob",
            "--path-separator",
            "--context-separator",
            "--dfa-size-limit",
            "--regex-size-limit",
        ],
    ),
    (
        "ag",
        &[
            "-m",
            "-A",
            "-B",
            "-C",
            "--max-count",
            "-G",
            "--file-search-regex",
            "--ignore",
            "--ignore-dir",
            "--depth",
            "-W",
            "--width",
            "--workers",
        ],
    ),
    ("od", &["-N", "-j", "-t"]),
    ("xxd", &["-l", "-s", "-c"]),
];

/// `gh` flags whose VALUE is a local file `gh` reads and uploads to the forge (#1086 class 5) —
/// `_GH_BODY_FILE_FLAGS`. Verified from `gh --help` text, never from a live `gh`: each says
/// "Read … from file". `gh api`'s `-F` is `--field` and is walked apart, because there the file
/// lives after an `@` in a `key=value`.
const GH_BODY_FILE_FLAGS: [&str; 5] = ["-F", "--body-file", "--notes-file", "-T", "--template"];

/// The same flags with the value after an `=` — `_GH_BODY_FILE_LONG`.
const GH_BODY_FILE_LONG: [&str; 3] = ["--body-file=", "--notes-file=", "--template="];

/// The two short flags with the value attached (`-F<path>`, `-T<path>`) —
/// `_GH_BODY_FILE_SHORT`. A bare `-F` reaches this check only as the LAST word (with a word
/// after it, it took that word), and its empty tail is refused like any other non-path.
const GH_BODY_FILE_SHORT: [&str; 2] = ["-F", "-T"];

/// What to do about a walk that reaches the vault directory — `_WALK_FIX`, in both spellings,
/// so the refusal is one edit away from running.
pub const WALK_FIX: &str = "Exclude it — `grep -rn --exclude-dir=.charter …`, `rg --glob \
     '!.charter' …` — or search the path you actually mean. `charter … secret exec --env \
     NAME=<key> -- <cmd>` is how a command gets a value without anyone reading one.";

/// `_REVEAL_REASON`. Deliberately does NOT offer `secret cp` as a way to SEE a value: `cp`
/// materialises it into a file, and the agent's next move after reading a denial is whatever
/// the denial names — so that text was the documented route around itself (#423).
pub const REVEAL_REASON: &str = "would reveal a secret value into the conversation (--reveal). \
     Use `charter … secret exec --env NAME=<key> -- <cmd>` — hand it to a command, never to \
     this conversation. (`secret cp` writes a 0600 FILE for a tool that needs a path; reading \
     that file back is the same leak by another road, and no guard covers a path you chose.)";

/// `_READ_REASON`. Returned from three places — argv, the raw scan on the unparseable path, and
/// the Read/Grep guard — because a guard whose wording drifts per path is a guard whose reader
/// cannot tell which rule fired.
pub const READ_REASON: &str = "reads a vault/secret file directly (would print plaintext). Use \
     `charter … secret exec --env NAME=<key> -- <cmd>`, or `--file ENVVAR=<key>` for a tool \
     that needs a path — and do not read a materialised copy back either: no guard covers a \
     path you chose.";

/// `i` as CPython's `(?i)` reads it: the letter, and the two Turkic spellings the `regex`
/// crate's simple case folding leaves out.
///
/// #92 swept the two engines over every letter of `secretshape`'s keyword set and found they
/// differ for exactly U+0130 `İ` and U+0131 `ı` (CPython folds both to `i`, `regex` folds
/// neither) — and `active-` and `fingerprint` both spell an `i`, so
/// `.charter/act İ ve-persona` really is the same match to the oracle. `crate::secretshape`
/// carries the same class for the same reason; it is repeated rather than shared because the
/// two patterns are ports of two different Python regexes and a shared constant would invite
/// the next edit to change both.
const I_CLASS: &str = "[iıİ]";

/// `_VAULT_PATH_RE`: the vault DIRECTORY and everything under it, the browser profile, the
/// active-persona marker, the fingerprint key, and the state directory itself.
///
/// Three things are ported rather than approximated:
///
/// * **`IGNORECASE`**, because the answer must not depend on which filesystem the guard happens
///   to be running on: macOS/APFS folds case, so `.CHARTER/vaults/db.json` is the SAME INODE as
///   the denied form on half the machines charter runs on. Spelled `(?i)` plus [`I_CLASS`],
///   since `(?i)` alone is not CPython's for `i`.
/// * **the segment anchor** on `vaults`: `.charter/vaults.json` is the registry and an ordinary
///   read, so `vaults` must be a whole path SEGMENT — followed by `/` or by the end of the
///   operand. The `$` half is #462's round-three finding: a pattern demanding a literal
///   trailing slash could not see the operand naming the directory ITSELF, which is the one
///   operand that walks every vault file.
/// * **CPython's `$`**, which matches at the end of the string *or just before a newline that
///   ends it*, where the `regex` crate's matches only at the end. Spelled `\n?$`. A shell token
///   really can hold a newline, so this is reachable.
///
/// The separator is NOT folded: charter's harness does not run on Windows, so folding `\` would
/// buy nothing on any supported host and would deny POSIX filenames that legitimately contain
/// one (#476).
fn vault_path_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(&format!(
            r"(?i)\.(?:charter|edm)(?:/(?:vaults(?:/|\n?$)|browser|act{I_CLASS}ve-|f{I_CLASS}ngerpr{I_CLASS}nt)|/?\n?$)"
        ))
        .expect("the vault-path pattern compiles")
    })
}

/// `_REVEAL_RE`: `--reveal` as a real flag, for the raw-string scan on the unparseable path
/// only. Anywhere a tokenizer succeeded, argv is used instead — a commit message may
/// legitimately mention the flag, which is the false positive this guard was rewritten to stop
/// having.
///
/// CPython's `\s` on a `str` is `White_Space` **plus U+001C–U+001F**, which the `regex` crate's
/// `\s` is not; the four are spelled out. `crate::memstore::is_python_space` records the same
/// difference for the same reason.
fn reveal_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?:^|[\s\x{1C}-\x{1F}])--reveal(?:=|[\s\x{1C}-\x{1F}]|\n?$)")
            .expect("the reveal pattern compiles")
    })
}

/// `_VAULT_PATH_RE.search(text)` on `text` exactly as given — no `normpath`, no second spelling.
///
/// The persona tool gate (`toolgate._touches_control_surface`, [`crate::personagate`]) asks the
/// pattern this way, on a spelling it has normalised by its own rule, and it must be the SAME
/// pattern the leak guard refuses on: two copies of "what is a vault path" is how the Read route
/// and the Bash route came to disagree about the vault directory (#462).
pub(crate) fn vault_path_matches(text: &str) -> bool {
    vault_path_re().is_match(text)
}

/// True when `operand` names a guarded path, in any spelling of the SAME path —
/// `_names_a_vault_path`.
///
/// The pattern is a text match, so `.charter//vaults/db.json` and `.charter/./vaults/db.json` —
/// one keystroke apart from the denied form, identical to the kernel, and not a wrapper or a
/// clever program — walked straight past it. Testing `normpath` as well collapses `//`, `/./`
/// and `a/b/..` to the one canonical spelling.
///
/// **Both forms are tested rather than only the normalised one**, because `normpath` can move
/// the answer in the permissive direction: `cat .charter/vaults/../../elsewhere` matches as
/// written and normalises OUT of the plane. A union can only widen, and it widens by exactly
/// the spellings that name the same path.
///
/// **The property this can hold, stated exactly.** It decides on the TEXT OF THE OPERAND AS
/// WRITTEN, modulo separator noise, dot segments and letter case — and case and separators are
/// properties of a string it already holds, so it is complete over them. Everything else that
/// changes which file an operand ends up naming is the work of a SHELL — glob, parameter and
/// brace expansion, command substitution, and the directory a preceding `cd` moved — and every
/// one of those happens strictly AFTER this has answered, on text it never sees.
pub fn names_a_vault_path(operand: &str) -> bool {
    let re = vault_path_re();
    re.is_match(operand) || re.is_match(&pypath::normpath(operand))
}

/// True when this invocation is charter itself, including `python3 -m charter` — `_is_charter`.
///
/// Case-folded for the same reason the path pattern is: on a case-insensitive filesystem
/// `CHARTER secret get … --reveal` runs the same binary, and a guard that matches one casing of
/// a name is a guard with a Shift key for a bypass.
pub fn is_charter(prog: &str, args: &[String]) -> bool {
    let base = base_lower(prog);
    if CHARTER_PROGS.contains(&base.as_str()) {
        return true;
    }
    if !base.starts_with("python") {
        return false;
    }
    // Python's `args.index("-m")` is the FIRST `-m`, and the name is the word after it.
    match args.iter().position(|a| a == "-m") {
        Some(i) => args
            .get(i + 1)
            .is_some_and(|n| CHARTER_PROGS.contains(&n.to_lowercase().as_str())),
        None => false,
    }
}

/// The arguments `prog` would actually open — its file operands. `_file_operands`.
///
/// The leak rule used to scan every token, which is true enough for `cat` and false for every
/// tool whose first operand is a program or a pattern: `sed -i 's|<path>|…|' f` rewrites a
/// mention and `grep -rn "<path>" docs/` searches for one. Neither opens the path, and both
/// were denied as reads of it.
///
/// Flag VALUES are skipped for the same reason — `grep -e <path> file` supplies the pattern
/// behind a flag — while the file that follows stays an operand, so hiding a real read behind
/// `-e` does not work.
///
/// **A script read from a FILE is an operand** (#351, upstream charter#1164). The Python lumped
/// `-f`/`--file` in with `-e`/`--regexp`/`--expression` and skipped the value of both, but only
/// the second group supplies the pattern inline. `-f` names a file the program really opens,
/// and `awk` quotes that file's text back on stderr when it does not parse. So [`SCRIPT_FILE`]'s
/// values are operands here.
///
/// **The option grammar is getopt's, not the Python's exact-match.** For the script tools,
/// a flag is recognised however getopt accepts it: `--regexp=x`, `-ex`, as the last letter of a
/// cluster (`-rf <file>`), and, for a from-file flag only, as a prefix of its long name
/// (`--fil`, which getopt takes for `--file` when nothing else starts that way). An EXACT name
/// always wins first, so `--binary` is never read as `--binary-files`, and only a from-file flag
/// is matched by prefix, because a wrong match there only turns more words into operands, which
/// denies. The Python matched
/// only the bare spelling, and every other spelling left the pattern "untaken", so the FILE
/// after it was read as the pattern and never asked about. `--` ends the options for every
/// reader, so a `-e` after it is an operand. Both changes moved recorded answers in
/// `fixtures/corpora/shellseg-*.jsonl*` on purpose (ADR 0046).
pub fn file_operands(prog: &str, args: &[String]) -> Vec<String> {
    let base = base_lower(prog);
    let inline = table(&SCRIPT_OPERAND, &base);
    let from_file: &[&str] = table(&SCRIPT_FILE, &base).unwrap_or(&[]);
    let file_value: &[&str] = table(&FILE_VALUE, &base).unwrap_or(&[]);
    // `split_env_chdir` hands back argv WITH the program still at [0]; dropping it here is what
    // makes "the first positional is the script" mean the first real operand.
    let argv: &[String] = if args.first().is_some_and(|a| a == prog) {
        &args[1..]
    } else {
        args
    };
    let takes: &[&str] = table(&TAKES_VALUE, &base).unwrap_or(&[]);
    // Only the script tools are read as getopt reads them. `xxd` spells long options with ONE
    // dash (`-ps`, `-seek`), so a cluster reading there would swallow a real operand.
    let getopt = inline.is_some();
    let kind = |flag: &str, long: bool| -> Option<ValueKind> {
        let exact = [
            (from_file, ValueKind::ScriptFile),
            (inline.unwrap_or(&[]), ValueKind::Script),
            (file_value, ValueKind::File),
            (takes, ValueKind::NotAPath),
        ]
        .into_iter()
        .find_map(|(list, k)| list.contains(&flag).then_some(k));
        exact.or_else(|| {
            (getopt && long && flag.len() > 2 && from_file.iter().any(|f| f.starts_with(flag)))
                .then_some(ValueKind::ScriptFile)
        })
    };
    let mut out = Vec::new();
    // What the NEXT word is, when a flag left its value there.
    let mut pending: Option<ValueKind> = None;
    let mut script_taken = inline.is_none();
    let mut options_ended = false;
    for a in argv {
        match pending.take() {
            Some(ValueKind::ScriptFile | ValueKind::File) => {
                out.push(a.clone());
                continue;
            }
            Some(_) => continue,
            None => {}
        }
        if options_ended || !a.starts_with('-') {
            if !script_taken {
                script_taken = true; // this positional IS the script/pattern
                continue;
            }
            out.push(a.clone());
            continue;
        }
        if a == "--" {
            options_ended = true;
            continue;
        }
        // Python's `a.startswith("-")`: a BARE `-` is a flag here, unlike in `walks_directories`
        // where it is named as an exception. Faithful, and the direction is a missed operand.
        let (found, attached) = if a.starts_with("--") {
            match a.split_once('=') {
                Some((name, val)) => (kind(name, true), Some(val)),
                None => (kind(a, true), None),
            }
        } else if getopt {
            // A cluster: the first letter that takes a value takes the rest of the word, or
            // the next word when nothing is left.
            let body = &a[1..];
            body.char_indices()
                .find_map(|(i, ch)| {
                    kind(&format!("-{ch}"), false).map(|k| {
                        let rest = &body[i + ch.len_utf8()..];
                        (Some(k), (!rest.is_empty()).then_some(rest))
                    })
                })
                .unwrap_or((None, None))
        } else if a.contains('=') {
            (None, None)
        } else {
            (kind(a, false), None)
        };
        let Some(k) = found else { continue };
        if matches!(k, ValueKind::Script | ValueKind::ScriptFile) {
            script_taken = true; // the pattern/program came from this flag
        }
        match attached {
            Some(val) if matches!(k, ValueKind::ScriptFile | ValueKind::File) => {
                out.push(val.to_string())
            }
            Some(_) => {}
            None => pending = Some(k),
        }
    }
    out
}

/// What a flag's value is, to [`file_operands`].
#[derive(Clone, Copy, PartialEq, Eq)]
enum ValueKind {
    /// The pattern or program itself: a mention, never opened.
    Script,
    /// A file the pattern or program is read FROM: opened, so an operand.
    ScriptFile,
    /// Another file the tool opens, such as a file of ignore rules: an operand, and the program
    /// still comes from the first positional.
    File,
    /// A count, a type name, a glob: never a path.
    NotAPath,
}

/// `operands`, plus each ADJACENT PAIR joined — the word a substitution splices back.
/// `_spliced_operands`.
///
/// A substitution's output is glued to whatever follows the `)` with no space, and the
/// tokenizer has already thrown that adjacency away: `cat $(echo .charter)/vaults/x.json`
/// reaches the guard as `.charter` and `/vaults/x.json`. Neither names a vault; concatenated
/// they name exactly one, and that is the file `cat` opens.
///
/// Pairs rather than the whole join, so the extra candidates stay proportional and a denial can
/// always be traced to two neighbouring words. The cost is a command passing two genuinely
/// separate operands that happen to concatenate — a false DENY on a command that does not do
/// anything useful, against a false ALLOW on a working exfiltration.
pub fn spliced_operands(operands: &[String]) -> Vec<String> {
    let mut out: Vec<String> = operands.to_vec();
    for pair in operands.windows(2) {
        out.push(format!("{}{}", pair[0], pair[1]));
    }
    out
}

/// The file `gh api`'s `--field key=value` reads, or `None` — `_gh_at_path`.
///
/// gh's `@` magic: a value of `@<path>` reads that file. The `@` sits after the FIRST `=` — gh's
/// `parseField` splits the field there — so `body=@notes.md` reads `notes.md`, `body=@a=b.md`
/// reads `a=b.md`, and `body=x=@notes.md` is the literal string. `@-` is stdin and comes back as
/// `-`, the one spelling of stdin the caller refuses, so `-F -`, `--input -` and `key=@-` are
/// one rule in one place.
pub fn gh_at_path(value: &str) -> Option<String> {
    let val = value.split_once('=').map(|(_, v)| v)?;
    val.strip_prefix('@').map(str::to_string)
}

/// The local paths a `gh` command opens and uploads to the forge (#1086 class 5) —
/// `_gh_file_operands`.
///
/// Class 5 is a plain argv-reader case wearing gh's clothes: `gh pr create -F <vault>` reads the
/// vault and publishes it as a PR body — worse than printing it, because it leaves the value on
/// the forge. `gh` is in no reader list (it is not a printer), so the file named by its
/// body/notes/template flag was never asked about at all.
///
/// Best-effort, in the guard's usual fail-closed-but-narrow direction: a value from the next
/// token, an `=` form and an attached short cluster are all read; a flag this does not list
/// keeps the file unread, a missed deny rather than a wrong one.
///
/// `args` is one segment's argv, program first, and the program is already `gh` — the caller
/// matches the basename before asking, the way it matches a reader before [`file_operands`], so
/// nothing here asks about `args[0]`.
pub fn gh_file_operands(args: &[String]) -> Vec<String> {
    let rest: &[String] = args.get(1..).unwrap_or(&[]);
    // The subcommand is the first word that is not a flag. This reads PAST a flag rather than
    // stopping at one, because a miss here is a missed DENY: `gh --flag api --input <vault>` is
    // the read it says it is.
    let api = rest
        .iter()
        .find(|w| !w.starts_with('-'))
        .is_some_and(|w| w == "api");
    let mut out: Vec<String> = Vec::new();
    let mut add = |path: Option<&str>| {
        if let Some(p) = path
            && !p.is_empty()
            && p != "-"
        {
            out.push(p.to_string());
        }
    };
    let n = rest.len();
    let mut i = 0usize;
    while i < n {
        let w = rest[i].as_str();
        if w == "--" {
            break;
        }
        if api {
            if (w == "-F" || w == "--field" || w == "--input") && i + 1 < n {
                let val = rest[i + 1].as_str();
                if w == "--input" {
                    add(Some(val));
                } else {
                    add(gh_at_path(val).as_deref());
                }
                i += 2;
                continue;
            }
            if let Some(v) = w.strip_prefix("--input=") {
                add(Some(v));
            } else if let Some(v) = w.strip_prefix("--field=") {
                add(gh_at_path(v).as_deref());
            } else if let Some(v) = w.strip_prefix("-F") {
                add(gh_at_path(v).as_deref()); // a bare last `-F` has no field, so no file
            }
        } else {
            if GH_BODY_FILE_FLAGS.contains(&w) && i + 1 < n {
                add(Some(rest[i + 1].as_str()));
                i += 2;
                continue;
            }
            // Python's `w.split("=", 1)[1]`: everything after the FIRST `=`, which for these
            // spellings is exactly the text after the matched prefix.
            if let Some(v) = GH_BODY_FILE_LONG.iter().find_map(|f| w.strip_prefix(f)) {
                add(Some(v));
            } else if let Some(v) = GH_BODY_FILE_SHORT.iter().find_map(|f| w.strip_prefix(f)) {
                add(Some(v));
            }
        }
        i += 1;
    }
    out
}

/// The lines of `cmd` a shell would run, each with whether a heredoc body an EXECUTOR receives
/// is what holds it — `_lines_a_command_could_run`.
///
/// What A7 reads, and it is here because the fact is the leak guard's too. A body nobody
/// executes is data — a brief, a commit message, a document — and reading it as commands refused
/// real work: a `git commit -F - <<'EOF'` whose message named `charter handoff` in backticks was
/// refused as a handoff. A body a shell DOES run is the opposite case: the host's rule sees only
/// `bash`, so a handoff in there gets no prompt at all.
pub fn lines_a_command_could_run(cmd: &str) -> Vec<(String, bool)> {
    if !cmd.contains("<<") {
        // Python's `cmd.split("\n")`, which breaks on U+000A and on nothing else — not
        // `str::lines`, which also eats a `\r` and never yields the empty tail.
        return cmd.split('\n').map(|l| (l.to_string(), false)).collect();
    }
    heredoc::heredoc_layout(cmd)
        .into_iter()
        .filter(|l| !l.body || l.executed)
        .map(|l| (l.text, l.executed))
        .collect()
}

/// The directory-name patterns an invocation asks its walker to skip — `_excluded_names`.
///
/// `!x` is `rg`'s spelling of "exclude x"; `*/x/*` and `**/x/**` are how the same thing is
/// spelled to a glob that matches whole paths. The punctuation comes off and the name stays,
/// and the two prefix strips are CHAINED rather than alternatives, so `**/*/x` loses both.
///
/// **The sign of a glob is read** (#350, upstream charter#1165). The Python stripped the `!` off
/// every value of `--glob`/`-g`/`--iglob`, which threw away the one character that tells `rg`'s
/// two meanings apart: `--glob '!x'` excludes `x` and `--glob 'x'` *selects* it. So a glob aimed
/// AT the state directory read as an exclusion of it, and the walk was allowed. Here only a `!`
/// value of those options is an exclusion. An inclusion glob excludes nothing, and it cancels
/// the exclusion globs before it, because rg lets the later glob win. The directory options
/// ([`EXCLUDE_OPTS`]) have no sign and are unchanged. This moved recorded answers in
/// `fixtures/corpora/shellseg-oracle.jsonl` on purpose (ADR 0046).
pub fn excluded_names(prog: &str, args: &[String]) -> Vec<String> {
    let argv: &[String] = if args.first().is_some_and(|a| a == prog) {
        &args[1..]
    } else {
        args
    };
    // Each excluding value, with the kind of option that gave it.
    let mut raw: Vec<(String, ExcludeOpt)> = Vec::new();
    let mut value_next: Option<ExcludeOpt> = None;
    let read_value = |raw: &mut Vec<(String, ExcludeOpt)>, val: &str, opt: ExcludeOpt| {
        if opt == ExcludeOpt::Directory || val.starts_with('!') {
            raw.push((val.to_string(), opt));
        } else {
            // An inclusion: it excludes nothing, and it overrides every glob before it.
            raw.retain(|(_, from)| *from != ExcludeOpt::SignedGlob);
        }
    };
    for a in argv {
        if let Some(opt) = value_next.take() {
            read_value(&mut raw, a, opt);
            continue;
        }
        // Python's `partition("=")`: no `=` gives the whole token as the name and an empty
        // separator, which is how "the value is the NEXT token" is told from "it is attached".
        let (name, val) = match a.split_once('=') {
            Some((name, val)) => (name, Some(val)),
            None => (a.as_str(), None),
        };
        let opt = if SIGNED_GLOB_OPTS.contains(&name) {
            ExcludeOpt::SignedGlob
        } else if EXCLUDE_OPTS.contains(&name) {
            ExcludeOpt::Directory
        } else {
            continue;
        };
        match val {
            Some(val) => read_value(&mut raw, val, opt),
            None => value_next = Some(opt),
        }
    }
    raw.into_iter()
        .map(|(p, _)| p)
        // Python's `if p` filters the ORIGINAL token, before any stripping: a pattern that is
        // empty only once the punctuation is off is kept as `""`.
        .filter(|p| !p.is_empty())
        .map(|p| {
            let p = p.trim_start_matches('!');
            let p = p.trim_matches('/');
            let p = p.strip_prefix("**/").unwrap_or(p);
            let p = p.strip_prefix("*/").unwrap_or(p);
            let p = p.strip_suffix("/**").unwrap_or(p);
            let p = p.strip_suffix("/*").unwrap_or(p);
            p.to_string()
        })
        .collect()
}

/// Which kind of option gave [`excluded_names`] a value.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ExcludeOpt {
    /// [`EXCLUDE_OPTS`]: the value is always an exclusion.
    Directory,
    /// [`SIGNED_GLOB_OPTS`]: an exclusion only with a leading `!`.
    SignedGlob,
}

/// True when `prog` would descend into subdirectories of the operands it is given —
/// `_walks_directories`.
pub fn walks_directories(prog: &str, args: &[String]) -> bool {
    let base = base_lower(prog);
    let Some(flags) = table(&TREE_WALKERS, &base) else {
        return false;
    };
    let Some(flags) = flags else {
        return true; // walks the tree with no option at all
    };
    let argv: &[String] = if args.first().is_some_and(|a| a == prog) {
        &args[1..]
    } else {
        args
    };
    let stops: &str = table(&CLUSTER_STOPS, &base).unwrap_or("");
    for (i, a) in argv.iter().enumerate() {
        // The Python also skips `-` and `--` here, and both exceptions are **inert**: a bare
        // `-` names no option and has no cluster letters to scan, and `--` is skipped by the
        // `starts_with("--")` arm below. Mutated away and measured against the oracle: zero
        // answers changed, over the fuzz and over the recording. So they are not spelled here,
        // where they were two mutants no input could tell apart (#311).
        if !a.starts_with('-') {
            continue;
        }
        let (name, attached) = match a.split_once('=') {
            Some((n, v)) => (n, Some(v)),
            None => (a.as_str(), None),
        };
        if flags.contains(&name) {
            return true;
        }
        // `grep -d recurse` / `--directories=recurse` is the long way to spell `-r`.
        if name == "-d" || name == "--directories" {
            let val = match attached {
                Some(v) => v,
                None => argv.get(i + 1).map(String::as_str).unwrap_or(""),
            };
            if val == "recurse" {
                return true;
            }
            continue;
        }
        if a.starts_with("--") {
            continue;
        }
        for ch in a.chars().skip(1) {
            if flags.contains(&format!("-{ch}").as_str()) {
                return true;
            }
            if stops.contains(ch) {
                break; // this letter takes the rest as its value
            }
        }
    }
    false
}

/// The state entries a directory WALK would read, asked of the filesystem —
/// `_guarded_state_entries`.
///
/// `state_dir` is a parameter where the Python reads `config.STATE_DIR`, and it is derived from
/// that setting rather than from the literal `.charter/` for the reason `toolgate._control_roots`
/// gives: `$CHARTER_HOME` puts this directory somewhere no pattern can spell, and the legacy
/// `.edm/` puts it somewhere else again.
///
/// **An EMPTY guarded directory is left out on purpose.** A fresh plane has `vaults/` and
/// nothing in it, and a guard that refuses `grep -r TODO .` there is refusing an ordinary search
/// to protect nothing — the fastest way to teach people that this denial is noise.
///
/// The ORDER is the directory's own, as `read_dir` gives it, because Python's `os.scandir`
/// gives its caller the same — and it is **not pinned by anything**. On a plane holding a vault
/// directory AND a browser profile, which of the two a refusal names is decided by the kernel,
/// not by this function, and that is true of the Python too. The differential's fixture
/// therefore keeps exactly one guarded entry in the directory it answers walks about, and
/// compares the filter rules against a second directory SORTED. Said out loud rather than
/// hidden, because it is a hole in the evidence and not a property of the code.
pub fn guarded_state_entries(state_dir: &Path) -> Vec<PathBuf> {
    let Ok(dir) = std::fs::read_dir(state_dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in dir.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if !GUARDED_STATE_EXACT.contains(&name)
            && !GUARDED_STATE_PREFIXES.iter().any(|p| name.starts_with(p))
        {
            continue;
        }
        let path = entry.path();
        // `os.DirEntry.is_dir()` FOLLOWS a symlink and answers False for a broken one, so
        // `fs::metadata` (which follows) is the match and `symlink_metadata` is not. Any other
        // error is Python's `except OSError: continue`.
        let is_dir = match std::fs::metadata(&path) {
            Ok(md) => md.is_dir(),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
            Err(_) => continue,
        };
        if is_dir {
            match std::fs::read_dir(&path) {
                Ok(mut inner) => {
                    if inner.next().is_none() {
                        continue; // an empty directory has nothing to leak
                    }
                }
                Err(_) => continue,
            }
        }
        out.push(path);
    }
    out
}

/// True when `pattern` selects at least one file that is really inside `entry` —
/// `_glob_selects_inside`.
///
/// A filesystem question rather than a reading of the glob: `*.py` over a directory holding only
/// `.json` vaults selects nothing, and refusing that search would be a false denial on the
/// commonest narrowed search an agent makes.
///
/// **Bounded, and the bound FAILS CLOSED** — an entry too large to answer cheaply is treated as
/// selected. That is the one place the answer depends on the order the directory is read in, and
/// it depends on it only in the direction that denies.
pub fn glob_selects_inside(entry: &Path, pattern: &str, limit: usize) -> bool {
    let pat = pattern.rsplit('/').next().unwrap_or(pattern);
    // `Path.is_file()` follows symlinks and answers False for anything missing.
    if std::fs::metadata(entry).is_ok_and(|m| m.is_file()) {
        let name = entry.file_name().and_then(|n| n.to_str()).unwrap_or("");
        return pypath::fnmatch(name, pat);
    }
    let mut seen = 0usize;
    // `os.walk`, top-down, not following symlinked directories.
    let mut stack = vec![entry.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue; // `os.walk` swallows an unreadable directory
        };
        let mut sub: Vec<PathBuf> = Vec::new();
        for e in rd.flatten() {
            let path = e.path();
            // `os.walk` classifies with `entry.is_dir()`, which FOLLOWS symlinks — a link to a
            // directory is a directory here, and `followlinks=False` only stops the descent.
            let is_dir = std::fs::metadata(&path).is_ok_and(|m| m.is_dir());
            if is_dir {
                if !std::fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink()) {
                    sub.push(path);
                }
                continue;
            }
            seen += 1;
            let name = e.file_name();
            let name = name.to_str().unwrap_or("");
            if seen > limit || pypath::fnmatch(name, pat) {
                return true;
            }
        }
        sub.reverse();
        stack.extend(sub);
    }
    false
}

/// The guarded state entry a walk rooted at one of `operands` would descend into —
/// `_walk_into_guarded_state`.
///
/// **The property, named: does the walk REACH the directory** — not how the operand is spelled
/// and not how the recursion is requested. `.`, `..`, `$PWD` typed out, an absolute path and a
/// path through a symlinked parent are five spellings of one ancestor, and the six bypasses this
/// guard family has had were all a literal set of spellings. So the operand is resolved against
/// the shell's directory and compared by ANCESTRY, and the thing it is compared against is asked
/// of the filesystem rather than matched as text.
///
/// Everything between the operand and the entry is a directory the walk has to enter, so
/// excluding any one of them keeps it out — and the comparison is `fnmatch`, because
/// `--exclude-dir` takes a GLOB and `.charter*` is how people write it.
pub fn walk_into_guarded_state(
    cwd: &str,
    operands: &[String],
    excluded: &[String],
    state_dir: &Path,
) -> Option<PathBuf> {
    let targets = guarded_state_entries(state_dir);
    if targets.is_empty() {
        return None;
    }
    // Python's `Path(cwd or ".")`.
    let base_dir = if cwd.is_empty() { "." } else { cwd };
    for operand in operands {
        let joined = if pypath::is_abs(operand) {
            operand.clone()
        } else {
            pypath::join(base_dir, operand)
        };
        let base = PathBuf::from(pypath::realpath(&joined));
        for target in &targets {
            let resolved = pypath::realpath_of(target);
            if resolved != base && !resolved.starts_with(&base) {
                continue;
            }
            let parts: Vec<String> = if resolved == base {
                Vec::new()
            } else {
                resolved
                    .strip_prefix(&base)
                    .map(|r| {
                        r.components()
                            .map(|c| c.as_os_str().to_string_lossy().into_owned())
                            .collect()
                    })
                    .unwrap_or_default()
            };
            let name = resolved
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let hop: Vec<String> = parts.into_iter().chain(std::iter::once(name)).collect();
            if hop
                .iter()
                .any(|part| excluded.iter().any(|pat| pypath::fnmatch(part, pat)))
            {
                continue;
            }
            return Some(resolved);
        }
    }
    None
}

/// [`walk_into_guarded_state`] for one invocation, or `None` if it walks nothing —
/// `_walks_into_guarded_state`.
///
/// A walker with no operand searches the CWD — `grep -r PATTERN` and `rg PATTERN` both do — so
/// "no path was named" is a path, and it is the one an agent standing in the plane root types
/// most often.
pub fn walks_into_guarded_state(
    prog: &str,
    args: &[String],
    cwd: &str,
    state_dir: &Path,
) -> Option<PathBuf> {
    if !walks_directories(prog, args) {
        return None;
    }
    let mut operands = file_operands(prog, args);
    if operands.is_empty() {
        operands.push(".".to_string());
    }
    walk_into_guarded_state(cwd, &operands, &excluded_names(prog, args), state_dir)
}

/// Deny a command that would print a secret into the transcript — `_leak_reason`.
///
/// Inspects real INVOCATIONS, not the raw string. Both patterns used to be substring scans over
/// the whole command line, so a command that merely *mentioned* the words was hard-denied with a
/// reason that misdescribed what it had done — `git commit -m "docs: document the --reveal
/// flag"` and `grep -rn "vaults" .charter/vaults.json` among them.
///
/// There is no second line of defence behind this one: `posttooluse_bash` does not scan Bash
/// output, so a command that gets past here prints whatever it prints.
///
/// `state_dir` is the plane's state directory, which the Python reads from `config.STATE_DIR`;
/// see [`guarded_state_entries`]. `cwd` is the directory the command runs in, and it is used for
/// exactly one thing — resolving the walk's operand — because the TEXT arms decide on the
/// operand as written and a resolver on this hot path is what the Python declines to be.
pub fn leak_reason(cmd: &str, cwd: &str, state_dir: &Path) -> Option<String> {
    let cmd = heredoc::strip_reader_heredocs(cmd);
    let (segments, parsed) = shellseg::segment_argv_parsed(&cmd);
    if !parsed {
        // No tokenizer got through, so argv is a guess. Match the string itself — a false deny
        // on an already-malformed command is survivable; printing a credential is not.
        if reveal_re().is_match(&cmd) {
            return Some(REVEAL_REASON.to_string());
        }
        if names_a_vault_path(&cmd) {
            return Some(READ_REASON.to_string());
        }
    }
    let mut here = String::new();
    for toks in &segments {
        let it = shellwrap::split_env_chdir(toks.as_slice());
        let base = base_lower(&it.prog);
        if !it.prog.is_empty() && CHDIR_BUILTINS.contains(&base.as_str()) {
            // `cd`/`pushd` relocate the shell for every LATER segment.
            if let Some(dest) = it.argv.iter().skip(1).find(|a| !a.starts_with('-')) {
                here = if pypath::is_abs(dest) {
                    dest.clone()
                } else {
                    pypath::join(&here, dest)
                };
            }
            continue;
        }
        // A wrapper's chdir moves THIS program only — `env -C d cat x` leaves the shell where it
        // was — so it layers onto `here` for this segment and does not outlive it.
        let where_ = if it.chdir.is_empty() {
            here.clone()
        } else if pypath::is_abs(&it.chdir) {
            it.chdir.clone()
        } else {
            pypath::join(&here, &it.chdir)
        };

        // Before the `prog` test, because these opens do not depend on what the program turns
        // out to be: `xargs -a <vault> echo` prints the vault while the only program named is
        // `echo`, and `< <vault> tee` is opened by the SHELL before anything is execed.
        if let Some(hit) = opens(&it.reads, &where_) {
            return Some(hit);
        }
        if it.prog.is_empty() {
            continue;
        }
        if is_charter(&it.prog, &it.argv)
            && it
                .argv
                .iter()
                .any(|a| a == "--reveal" || a.starts_with("--reveal="))
        {
            return Some(REVEAL_REASON.to_string());
        }
        // `script` runs its command on a pseudo-terminal, and a pty is a terminal to
        // `--reveal`: the value is printed to it and the agent reads what `script` relays.
        // Recognised cheaply — any word of its arguments naming charter, beside the flag.
        if base == "script"
            && it.argv.iter().skip(1).any(|a| {
                a.split_whitespace()
                    .any(|w| CHARTER_PROGS.contains(&base_lower(w).as_str()))
            })
            // `_REVEAL_RE` alone. Python also asks `a == "--reveal" or a.startswith("--reveal=")`
            // first, and both are spellings the pattern already matches (`^--reveal$`,
            // `^--reveal=`), so the two extra tests could never change the answer — which is
            // what made them mutants no test could catch (#311).
            && it.argv.iter().skip(1).any(|a| reveal_re().is_match(a))
        {
            return Some(REVEAL_REASON.to_string());
        }
        if READERS.contains(&base.as_str())
            && let Some(hit) = opens(
                &spliced_operands(&file_operands(&it.prog, &it.argv)),
                &where_,
            )
        {
            return Some(hit);
        }
        // `gh` is not a reader — it prints nothing — but a body/notes/template flag naming a
        // vault reads it and uploads it to the forge (#1086 class 5). Its file operands go
        // through the SAME `opens` a reader's do, so `gh pr create -F <vault>` is denied and
        // `-F <ordinary file>` (and `-F -`, the stdin body) stays allowed.
        if base == "gh"
            && let Some(hit) = opens(&gh_file_operands(&it.argv), &where_)
        {
            return Some(hit);
        }
        // …and the operand that CONTAINS the vault directory without naming it (#474). The two
        // arms above decide on the TEXT of the operand, so `grep -rn TOKEN .` from the plane
        // root printed every vault file while naming none of them. This one decides on where the
        // walk GOES — against `where_`, not `cwd`, because the walk's operand has to resolve
        // against the directory the program will really run in.
        let at = if pypath::is_abs(&where_) {
            where_.clone()
        } else {
            pypath::join(cwd, &where_)
        };
        if let Some(hit) = walks_into_guarded_state(&it.prog, &it.argv, &at, state_dir) {
            let name = hit
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            return Some(format!(
                "walks a directory tree that contains the plane's own `{name}` — every file in \
                 it would be printed into the transcript, and none of them is named on this \
                 command line. {WALK_FIX}"
            ));
        }
    }
    None
}

/// The reason `paths` may not be opened from `where_`, or `None` — `_leak_reason`'s `_opens`.
///
/// Reused so a wrapper's own file and a reader's operand cannot be judged by two different
/// rules, and asking [`names_a_vault_path`] — the SAME predicate the Read/Grep guard asks — so
/// the two vault guards cannot answer differently for the same string.
fn opens(paths: &[String], where_: &str) -> Option<String> {
    let hit = paths.iter().any(|a| {
        names_a_vault_path(a)
            || (!where_.is_empty() && names_a_vault_path(&pypath::join(where_, a)))
    });
    hit.then(|| READ_REASON.to_string())
}

/// A `dict.get` over a small table kept as a slice — the shape `shellwrap` uses for the same
/// reason: a table read three times a line does not want a hash map built per call.
fn table<V: Copy>(rows: &[(&str, V)], key: &str) -> Option<V> {
    rows.iter().find(|(k, _)| *k == key).map(|(_, v)| *v)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    /// CPython's `re.IGNORECASE` and the `regex` crate's `(?i)` are not the same function, and
    /// [`vault_path_re`] spells two letters where they could part company. Both are swept here
    /// over **all 1,112,064 non-surrogate codepoints**, against the set CPython really matches —
    /// which was taken from the frozen interpreter rather than reasoned about:
    ///
    /// * **`i`** (in `active-` and `fingerprint`) — CPython matches `I`, `i`, `İ` (U+0130) and
    ///   `ı` (U+0131). The crate's simple case folding matches neither Turkic form, which is the
    ///   whole reason [`I_CLASS`] exists, and this is what says the class is neither too narrow
    ///   nor too wide.
    /// * **`s`** (in `vaults` and `browser`) — CPython matches `S`, `s` and `ſ` (U+017F), and so
    ///   does the crate. No help is needed there; asserted rather than assumed, because "only
    ///   `i` needs help" is a claim about every OTHER letter too.
    ///
    /// The remaining letters of the pattern (`charter`, `edm`, `vaults`, `browser`, `active-`,
    /// `fingerprint`) fold to exactly their own two cases in both engines; `i` and `s` are the
    /// only ones CPython gives a third spelling.
    #[test]
    fn the_pattern_folds_case_the_way_cpython_does_over_every_codepoint() {
        let i_class = ['\u{49}', '\u{69}', '\u{130}', '\u{131}'];
        let s_class = ['\u{53}', '\u{73}', '\u{17f}'];
        for c in (0..=0x10FFFFu32).filter_map(char::from_u32) {
            assert_eq!(
                vault_path_re().is_match(&format!(".charter/act{c}ve-persona")),
                i_class.contains(&c),
                "the `i` of `active-` at U+{:04X}",
                c as u32,
            );
            assert_eq!(
                vault_path_re().is_match(&format!(".charter/vault{c}/x")),
                s_class.contains(&c),
                "the `s` of `vaults` at U+{:04X}",
                c as u32,
            );
        }
    }

    /// CPython's `\s` on a `str` is `White_Space` **plus U+001C–U+001F**, and the `regex` crate's
    /// is not. `_REVEAL_RE` asks it on both sides of the flag, so a run where the two disagree is
    /// a refusal that happens on one implementation and not the other.
    #[test]
    fn the_reveal_scan_reads_cpythons_whitespace() {
        for sep in [
            '\u{1c}', '\u{1d}', '\u{1e}', '\u{1f}', ' ', '\t', '\n', '\u{a0}', '\u{2028}',
        ] {
            assert!(
                reveal_re().is_match(&format!("echo{sep}--reveal{sep}x")),
                "U+{:04X} separates a flag for CPython",
                sep as u32,
            );
        }
        // ...and CPython's `$` matches before a newline that ENDS the string, where the crate's
        // does not. A shell token really can hold one.
        assert!(reveal_re().is_match("echo --reveal\n"));
        assert!(!reveal_re().is_match("echo --revealed"));
        assert!(!reveal_re().is_match("echo x--reveal"));
    }

    fn words(line: &str) -> Vec<String> {
        line.split_whitespace().map(str::to_owned).collect()
    }

    /// `leak_reason` against a state directory that is not there, so only the text arms answer.
    fn reason(cmd: &str) -> Option<String> {
        leak_reason(cmd, "", Path::new("/nonexistent-plane-state"))
    }

    /// A plane root whose state directory holds one vault file, so a walk from the root reaches
    /// it: the directory (kept alive by the caller), the root as a cwd, and the state directory.
    fn plane_with_a_vault() -> (tempfile::TempDir, String, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let state = dir.path().join(".charter");
        std::fs::create_dir_all(state.join("vaults")).unwrap();
        std::fs::write(state.join("vaults/db.json"), "{}").unwrap();
        let at = dir.path().to_string_lossy().into_owned();
        (dir, at, state)
    }

    /// A heredoc the shell reads one way and the strip plan another hid a real read (#359). The
    /// here-string's `<<'x'` looked like a heredoc to the old opener pattern and `<<'A B'` did
    /// not, so the counts matched and the reader body ran on to a line `x`, taking the `cat` of
    /// the vault with it. bash and zsh both end the body at `A B` and run that `cat`.
    #[test]
    fn a_heredoc_is_read_where_the_shell_reads_it_before_its_body_is_dropped() {
        let cmd = "cat <<'A B'; cat <<<'x'\njunk\nA B\ncat .charter/vaults/x.json\nx";
        assert!(reason(cmd).is_some(), "{cmd:?}");
    }

    /// A heredoc in a substitution closed on its own line: GNU bash 3.2.57 and zsh 5.9 run the
    /// next lines as commands, so they are never dropped as its body (#359).
    #[test]
    fn a_heredoc_in_a_substitution_closed_on_its_line_keeps_the_next_lines() {
        for cmd in [
            "x=$( cat <<'EOF' )\ncat .charter/vaults/x.json\nEOF",
            "x=`cat <<'EOF'`\ncat .charter/vaults/x.json\nEOF",
            "echo \"$(cat <<'EOF')\"\ncat .charter/vaults/x.json\nEOF",
        ] {
            assert!(reason(cmd).is_some(), "{cmd:?}");
        }
    }

    #[test]
    fn the_vault_path_pattern_is_asked_as_written() {
        assert!(vault_path_matches(".charter/vaults/db.json"));
        assert!(vault_path_matches("/p/.charter/vaults"));
        assert!(vault_path_matches(".edm/active-persona"));
        assert!(!vault_path_matches(".charter/vaults.json"));
        assert!(!vault_path_matches("README.md"));
        // As written: the spelling `names_a_vault_path` would normalise is not this one's.
        assert!(!vault_path_matches(".charter//vaults/db.json"));
        assert!(names_a_vault_path(".charter//vaults/db.json"));
    }

    #[test]
    fn charter_reveals_only_with_the_flag_and_only_when_charter_is_the_program() {
        for cmd in [
            "charter secret get db --reveal",
            "charter secret get db --reveal=yes",
            "edm secret get db --reveal",
            "cd /tmp && charter secret get db --reveal",
        ] {
            assert_eq!(reason(cmd).as_deref(), Some(REVEAL_REASON), "{cmd}");
        }
        for cmd in [
            "charter secret get db",
            "charter secret list",
            "charter status",
            "echo --reveal",
            "git commit -m --reveal",
            "charter secret get db --revealed",
        ] {
            assert_eq!(reason(cmd), None, "{cmd}");
        }
    }

    #[test]
    fn script_reveals_only_when_its_command_is_charter_with_the_flag() {
        for cmd in [
            "script -q -c 'charter secret get db --reveal' /dev/null",
            "script -q -c 'charter secret get db --reveal=1' /dev/null",
            "script -q /dev/null charter secret get db --reveal",
            "script -q /dev/null edm secret get db --reveal=1",
        ] {
            assert_eq!(reason(cmd).as_deref(), Some(REVEAL_REASON), "{cmd}");
        }
        for cmd in [
            // charter, and no flag
            "script -q -c 'charter secret list' /dev/null",
            // the flag, and no charter
            "script -q -c 'echo --reveal' /dev/null",
            // both, but the program is not script
            "nohup sh -c 'charter secret get db --reveal'",
            "tee -a 'charter secret get db --reveal'",
        ] {
            assert_eq!(reason(cmd), None, "{cmd}");
        }
    }

    #[test]
    fn only_gh_uploads_the_file_its_body_flag_names() {
        for cmd in [
            "gh pr create -F .charter/vaults/db.json",
            "gh release create v1 --notes-file .charter/vaults/db.json",
            "gh api repos/o/r/issues --input .charter/vaults/db.json",
        ] {
            assert_eq!(reason(cmd).as_deref(), Some(READ_REASON), "{cmd}");
        }
        for cmd in [
            "gh pr create -F notes.md",
            "gh pr create -F -",
            // Not gh, so its `-F` is not gh's: `tee` is no reader and writes rather than reads.
            "tee -F .charter/vaults/db.json",
            "curl --body-file .charter/vaults/db.json",
        ] {
            assert_eq!(reason(cmd), None, "{cmd}");
        }
    }

    #[test]
    fn a_readers_operand_is_asked_about_and_a_non_readers_is_not() {
        assert_eq!(
            reason("cat .charter/vaults/db.json").as_deref(),
            Some(READ_REASON)
        );
        assert_eq!(
            reason("cd .charter && cat vaults/db.json").as_deref(),
            Some(READ_REASON)
        );
        assert_eq!(reason("touch .charter/vaults/db.json"), None);
        assert_eq!(reason("cat README.md"), None);
    }

    #[test]
    fn gh_file_operands_reads_values_after_and_attached_to_each_flag() {
        let ops = |line: &str| gh_file_operands(&words(line));
        assert_eq!(
            ops("gh api x -F a=@one.md --input two.json -Fb=@three.md --field=c=@four.md"),
            ["one.md", "two.json", "three.md", "four.md"]
        );
        // The subcommand is the first word that is not a flag, read past any flag before it.
        assert_eq!(ops("gh --verbose api --input=y.json"), ["y.json"]);
        // A flag with nothing after it, first or last, takes nothing and walks off no end.
        assert!(ops("gh api -F").is_empty());
        assert!(ops("gh api x y --input").is_empty());
        assert!(ops("gh pr create --body-file").is_empty());
        assert!(ops("gh pr create -t x -F").is_empty());
        assert_eq!(
            ops("gh pr create -F body.md -T tpl.md --notes-file=n.md -Fattached.md"),
            ["body.md", "tpl.md", "n.md", "attached.md"]
        );
        assert_eq!(ops("gh pr create -F one.md -- -F two.md"), ["one.md"]);
        assert!(ops("gh pr create -F -").is_empty());
    }

    #[test]
    fn exclusions_and_recursion_are_read_with_or_without_the_program_in_front() {
        assert_eq!(
            excluded_names("grep", &words("grep -r x --exclude-dir=.charter .")),
            [".charter"]
        );
        assert_eq!(
            excluded_names("grep", &words("--exclude-dir .charter -r x .")),
            [".charter"]
        );
        assert!(walks_directories("grep", &words("grep -r x .")));
        assert!(walks_directories("grep", &words("-r x .")));
        assert!(walks_directories("grep", &words("-d recurse x .")));
        assert!(walks_directories("grep", &words("--directories=recurse x")));
        assert!(walks_directories("grep", &words("-ir x")));
        // A word that is not an option is never read as a cluster, whatever letters it holds.
        assert!(!walks_directories("grep", &words("grep xr file")));
        assert!(!walks_directories("grep", &words("grep -eR file")));
        assert!(!walks_directories("grep", &words("grep - --")));
        assert!(!walks_directories("grep", &words("grep -d skip x")));
        assert!(!walks_directories("cat", &words("cat -r x")));
        assert!(walks_directories("rg", &words("rg x")));
    }

    #[cfg(unix)]
    #[test]
    fn a_broken_link_is_a_guarded_entry_and_one_that_cannot_be_asked_about_is_not() {
        let dir = tempfile::tempdir().unwrap();
        let state = dir.path();
        // `vaults` pointing nowhere: `is_dir()` answers False for it, so it is kept as a file.
        std::os::unix::fs::symlink(state.join("gone"), state.join("vaults")).unwrap();
        // `browser-x` pointing at itself: asking raises, so it is skipped.
        std::os::unix::fs::symlink(state.join("browser-x"), state.join("browser-x")).unwrap();
        // `active-empty` an empty directory: nothing to leak, so skipped.
        std::fs::create_dir(state.join("active-empty")).unwrap();
        // `fingerprint` a directory with a file in it: kept.
        std::fs::create_dir(state.join("fingerprint")).unwrap();
        std::fs::write(state.join("fingerprint/f"), "x").unwrap();
        // not a guarded name at all
        std::fs::create_dir(state.join("state")).unwrap();
        std::fs::write(state.join("state/x"), "x").unwrap();

        let mut got = guarded_state_entries(state);
        got.sort();
        assert_eq!(got, [state.join("fingerprint"), state.join("vaults")]);
    }

    #[test]
    fn a_glob_over_more_files_than_the_bound_fails_closed_and_at_the_bound_does_not() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.json"), "{}").unwrap();
        std::fs::write(dir.path().join("b.json"), "{}").unwrap();
        assert!(!glob_selects_inside(dir.path(), "*.py", 2));
        assert!(glob_selects_inside(dir.path(), "*.py", 1));
        assert!(glob_selects_inside(dir.path(), "*.json", 2));
        assert!(glob_selects_inside(dir.path(), "src/a.*", 2));
        assert!(!glob_selects_inside(&dir.path().join("a.json"), "*.py", 0));
        assert!(glob_selects_inside(&dir.path().join("a.json"), "*.json", 0));
    }

    #[cfg(unix)]
    #[test]
    fn a_glob_is_asked_of_every_real_subdirectory_and_of_no_linked_one() {
        // `os.walk(followlinks=False)`: a directory is descended into, a link to one is not.
        let dir = tempfile::tempdir().unwrap();
        let entry = dir.path().join("entry");
        std::fs::create_dir_all(entry.join("deep/deeper")).unwrap();
        std::fs::write(entry.join("deep/deeper/key.pem"), "x").unwrap();
        assert!(glob_selects_inside(&entry, "*.pem", 100));

        let elsewhere = dir.path().join("elsewhere");
        std::fs::create_dir_all(&elsewhere).unwrap();
        std::fs::write(elsewhere.join("cert.crt"), "x").unwrap();
        std::os::unix::fs::symlink(&elsewhere, entry.join("linked")).unwrap();
        assert!(!glob_selects_inside(&entry, "*.crt", 100));
    }

    #[test]
    fn only_an_exclude_option_names_an_exclusion() {
        assert!(excluded_names("grep", &words("grep -r --color=auto x .")).is_empty());
        assert_eq!(
            excluded_names("rg", &words("rg --glob=!vaults -g !browser* x")),
            ["vaults", "browser*"]
        );
    }

    /// `rg`'s `--glob`/`-g`/`--iglob` carry their SIGN in the value (#350): `!x` excludes `x`,
    /// and `x` selects it. A glob aimed INTO the state directory is a narrowing onto it, and
    /// reading it as an exclusion let the walk through.
    #[test]
    fn an_rg_glob_without_a_bang_is_an_inclusion_and_excludes_nothing() {
        for line in [
            "rg -g .charter x .",
            "rg --glob **/.charter/** x .",
            "rg --glob=.charter x .",
            "rg --iglob .CHARTER x .",
            "rg -g=vaults x .",
        ] {
            assert!(excluded_names("rg", &words(line)).is_empty(), "{line}");
        }
        assert_eq!(
            excluded_names("rg", &words("rg -g !.charter --iglob=!vaults x .")),
            [".charter", "vaults"]
        );
        // The directory options have no sign: their value is always an exclusion, even one
        // spelled with a `!`, and a later inclusion glob does not cancel it.
        assert_eq!(
            excluded_names("rg", &words("rg --ignore-dir .charter x .")),
            [".charter"]
        );
        assert_eq!(
            excluded_names("rg", &words("rg --exclude-dir=!odd -g .charter x .")),
            ["odd"]
        );
        // rg lets a later glob override an earlier one, so an inclusion after an exclusion
        // glob cancels it, and one before it does not.
        assert!(excluded_names("rg", &words("rg -g !.charter -g .charter x .")).is_empty());
        assert_eq!(
            excluded_names("rg", &words("rg -g *.rs -g !.charter x .")),
            [".charter"]
        );

        let (_dir, at, state) = plane_with_a_vault();
        for cmd in [
            "rg --glob '**/.charter/**' TOKEN .",
            "rg -g '.charter/**' secret",
            "rg -g .charter -g vaults TOKEN .",
            "rg -g '!.charter' -g '.charter/**' TOKEN .",
        ] {
            assert!(leak_reason(cmd, &at, &state).is_some(), "{cmd}");
        }
        for cmd in [
            "rg --glob '!.charter' TOKEN .",
            "rg -g '*.rs' -g '!.charter' TOKEN .",
        ] {
            assert_eq!(leak_reason(cmd, &at, &state), None, "{cmd}");
        }
    }

    /// `-f`/`--file` names a file the program OPENS (#351): awk quotes a program that does not
    /// parse back on stderr, and grep turns the file's lines into patterns. The inline flags
    /// (`-e`, `--regexp`, `--expression`) still supply a pattern that is only a mention.
    #[test]
    fn a_script_read_from_a_file_is_an_operand() {
        let vault = ".charter/vaults/db.json";
        for cmd in [
            format!("awk -f {vault} data"),
            format!("awk --file {vault} data"),
            format!("awk --file={vault} data"),
            format!("awk -f{vault} data"),
            format!("sed -f {vault} data"),
            format!("sed --file={vault} data"),
            format!("sed -n -f {vault} data"),
            format!("grep -f {vault} f"),
            format!("grep --file={vault} f"),
            format!("grep -f{vault} f"),
            format!("grep -rf {vault} src"),
            format!("rg -f {vault} f"),
            format!("rg --file {vault} f"),
        ] {
            assert_eq!(reason(&cmd).as_deref(), Some(READ_REASON), "{cmd}");
        }
        for cmd in [
            format!("grep -e {vault} f"),
            format!("grep --regexp={vault} f"),
            format!("sed -e s|{vault}|x| f"),
            format!("sed --expression=s|{vault}|x| f"),
            format!("rg -e {vault} f"),
            "awk -f prog.awk data".to_string(),
        ] {
            assert_eq!(reason(&cmd), None, "{cmd}");
        }
        // The program came from the file, so the first positional is a data file.
        assert_eq!(
            file_operands("awk", &words("awk -f prog.awk data")),
            ["prog.awk", "data"]
        );
    }

    /// The pattern flag in every spelling getopt accepts: with `=`, attached, and as the last
    /// letter of a cluster. Each one used to leave the pattern "untaken", so the FILE after it
    /// was read as the pattern and never asked about. And `--` ends the options, so a `-e`
    /// after it is the pattern itself.
    #[test]
    fn a_pattern_flag_is_read_however_it_is_spelled() {
        let vault = ".charter/vaults/db.json";
        for cmd in [
            format!("grep --regexp=x {vault}"),
            format!("sed --expression=p {vault}"),
            format!("grep -ex {vault}"),
            format!("grep -iex {vault}"),
            format!("grep -ie x {vault}"),
            format!("sed -ne p {vault}"),
            format!("rg --regexp=x {vault}"),
            format!("grep -- -e {vault}"),
            format!("head -n5 {vault}"),
            format!("tail --lines=5 {vault}"),
            // A unique prefix of a long name is that name to getopt.
            format!("sed --fil {vault} x"),
            format!("awk --fi={vault} x"),
            format!("grep --reg x {vault}"),
            // `xxd` spells long options with one dash, so `-ps` is not `-p -s <value>`.
            format!("xxd -ps {vault}"),
            // An exact option name is that option, even when it is also the start of a longer
            // one: `--binary` takes no value, whatever `--binary-files` does.
            format!("grep --binary TOKEN {vault}"),
            format!("rg --binary TOKEN {vault}"),
            // gawk's inline program flags.
            format!("awk -e{{print}} {vault}"),
            format!("awk --source={{print}} {vault}"),
            format!("awk -e {{print}} {vault}"),
            // A file of ignore rules is a file the tool opens.
            format!("rg --ignore-file {vault} TOKEN src"),
            format!("grep -r --exclude-from={vault} TOKEN src"),
            format!("ag --path-to-ignore {vault} TOKEN src"),
        ] {
            assert_eq!(reason(&cmd).as_deref(), Some(READ_REASON), "{cmd}");
        }
        assert_eq!(
            file_operands("grep", &words("grep -rn -m 2 -A1 -e x -- -v a b")),
            ["-v", "a", "b"]
        );
        assert_eq!(file_operands("head", &words("head -n 5 -c3 a")), ["a"]);
    }

    /// A walker's value-taking flag that the guard does not know as one turns its VALUE into
    /// the pattern and the real pattern into the walk's root. So `rg -t json TOKEN` was read as
    /// a search of a directory named `TOKEN`, and the cwd walk it really does was never asked
    /// about.
    #[test]
    fn a_walkers_value_flag_does_not_move_the_walk_off_the_cwd() {
        let (_dir, at, state) = plane_with_a_vault();
        for cmd in [
            "rg -t json TOKEN",
            "rg --type json TOKEN",
            "rg -T md TOKEN",
            "rg --max-depth 9 TOKEN",
            "rg -j 2 TOKEN",
            "rg --sort path TOKEN",
            "grep -r --include '*.json' TOKEN",
            "grep -r --exclude-dir .git TOKEN",
            "grep -r --exclude '*.md' TOKEN",
            "ag --depth 9 TOKEN",
            "ag --ignore-dir .git TOKEN",
        ] {
            assert!(leak_reason(cmd, &at, &state).is_some(), "{cmd}");
        }
        // ...and the recommended fix still runs with the value spelled as its own word.
        assert_eq!(
            leak_reason("grep -r --exclude-dir .charter TOKEN", &at, &state),
            None
        );
        assert_eq!(
            leak_reason("rg -t json -g '!.charter' TOKEN", &at, &state),
            None
        );
    }

    /// `ag`'s `-f` is `--follow` and takes no value, so the word after it is the PATTERN and
    /// the file after that is a file ag prints.
    #[test]
    fn ags_follow_flag_takes_no_value() {
        let vault = ".charter/vaults/db.json";
        assert!(reason(&format!("ag -f TOKEN {vault}")).is_some());
        assert!(reason(&format!("ag --follow TOKEN {vault}")).is_some());
        assert_eq!(file_operands("ag", &words("ag -f TOKEN a.txt")), ["a.txt"]);
    }

    proptest! {
        /// A guard that panics is a guard that does not answer, and `charter hook pretooluse`
        /// blocks with exit 2 and nothing else — so a panic would be an ALLOW (the Python has the
        /// same shape; see charter#1166). This is the property that says no command line can get
        /// one: the walk indexes paths, the option grammars index argv, and none of them can step
        /// off the end.
        #[test]
        fn no_command_line_makes_the_guard_panic(
            cmd in ".{0,120}",
            cwd in "[a-z./]{0,20}",
        ) {
            // A state directory that is not there, so the property is about the parsing and the
            // option grammars rather than about one fixture's contents; the walk against a real
            // tree is what `the_leak_guard_answers_what_the_python_answers.rs` replays.
            let _ = leak_reason(&cmd, &cwd, std::path::Path::new("/nonexistent-plane-state"));
        }

        /// The same, for the two option grammars a segment reaches directly.
        #[test]
        fn no_argv_makes_the_option_grammars_panic(
            argv in proptest::collection::vec("[-a-zA-Z0-9=/.!*\\[\\]]{0,8}", 0..8),
        ) {
            let prog = argv.first().cloned().unwrap_or_default();
            let _ = file_operands(&prog, &argv);
            let _ = excluded_names(&prog, &argv);
            let _ = walks_directories(&prog, &argv);
            let _ = gh_file_operands(&argv);
            let _ = is_charter(&prog, &argv);
        }
    }
}
