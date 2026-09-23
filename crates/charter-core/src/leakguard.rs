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
//! # Three defects this port found in the frozen Python, and REPRODUCES
//!
//! The Python is the differential's oracle, so a port that is right where the oracle is wrong is
//! a port that fails its own test. Each is filed upstream and each has a row in
//! `fixtures/corpora/shellseg-oracle.jsonl` pinning today's answer, so the fix landing there
//! shows up here as a divergence rather than silently:
//!
//! * **charter#1164** — `_file_operands` skips the value of `-f`/`--file`, and for `sed`, `awk`,
//!   `grep`, `rg` and `ag` that value is a file the program OPENS. `awk` quotes the offending
//!   source text back on stderr, so it reaches the transcript. See [`file_operands`].
//! * **charter#1165** — `_excluded_names` reads `rg`'s `--glob`/`-g`/`--iglob` as an exclusion
//!   whatever the value says, and for that tool a value WITHOUT a leading `!` is an inclusion.
//!   See [`excluded_names`].
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

/// Options whose value EXCLUDES a directory from the walk — `_EXCLUDE_OPTS`.
///
/// Read so that the fix the denial prints actually runs: a guard that refuses the command it
/// recommends is one people learn to route around. Deliberately permissive and NOT a security
/// boundary — it takes any of these values as a real exclusion without checking that the
/// program would honour it there.
const EXCLUDE_OPTS: [&str; 6] = [
    "--exclude-dir",
    "--exclude",
    "--ignore-dir",
    "-g",
    "--glob",
    "--iglob",
];

/// Readers whose FIRST non-flag operand is a program or pattern rather than a file, and the
/// flags that supply it instead — `_SCRIPT_OPERAND`. Kept to the three tools where the operand
/// is unambiguous, because getting this wrong in the permissive direction costs a false
/// negative.
const SCRIPT_OPERAND: [(&str, &[&str]); 5] = [
    ("sed", &["-e", "--expression", "-f", "--file"]),
    ("awk", &["-f", "--file"]),
    ("grep", &["-e", "--regexp", "-f", "--file"]),
    ("rg", &["-e", "--regexp", "-f", "--file"]),
    ("ag", &["-e", "--regexp", "-f", "--file"]),
];

/// Flags whose VALUE is a separate token and is never a path — `_TAKES_VALUE`.
///
/// Per tool, because the same spelling differs: `head -n 5` takes a count, `sed -n` is "quiet"
/// and takes nothing, and treating sed's `-n` as consuming a value swallows the script operand
/// — which would let `sed -n 1p .charter/active-persona` through, a real read.
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
        ],
    ),
    ("rg", &["-m", "-A", "-B", "-C", "--max-count"]),
    ("ag", &["-m", "-A", "-B", "-C", "--max-count"]),
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
pub fn vault_path_matches(text: &str) -> bool {
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
/// **charter#1164, reproduced on purpose.** [`SCRIPT_OPERAND`] lumps `-f`/`--file` in with
/// `-e`/`--regexp`/`--expression`, and only the second group supplies the pattern INLINE: `-f`
/// names a file the program really opens, and `awk` quotes that file's text back on stderr when
/// it does not parse. So `awk -f <vault> data` is allowed here, as it is in the Python. The
/// frozen Python is this port's oracle and the fix belongs there; the corpus pins today's
/// answer so it cannot change on one side alone.
pub fn file_operands(prog: &str, args: &[String]) -> Vec<String> {
    let base = base_lower(prog);
    let flags = table(&SCRIPT_OPERAND, &base);
    // `split_env_chdir` hands back argv WITH the program still at [0]; dropping it here is what
    // makes "the first positional is the script" mean the first real operand.
    let argv: &[String] = if args.first().is_some_and(|a| a == prog) {
        &args[1..]
    } else {
        args
    };
    let takes: &[&str] = table(&TAKES_VALUE, &base).unwrap_or(&[]);
    let mut out = Vec::new();
    let mut skip_next = false;
    let mut script_taken = flags.is_none();
    for a in argv {
        if skip_next {
            skip_next = false;
            continue;
        }
        // Python's `a.startswith("-")`: a BARE `-` is a flag here, unlike in `walks_directories`
        // where it is named as an exception. Faithful, and the direction is a missed operand.
        if a.starts_with('-') {
            if let Some(flags) = flags
                && flags.contains(&a.as_str())
            {
                script_taken = true; // the pattern/program came from this flag
                // `!a.contains('=')` is **inert**, here and in the Python: this arm is reached
                // only on exact equality with a table entry, and no entry spells an `=`. Mutated
                // to an unconditional `true` and measured against the oracle: zero answers
                // changed, over the fuzz and over the recording. Kept because it is the Python's.
                skip_next = !a.contains('=');
            } else if !a.contains('=') && takes.contains(&a.as_str()) {
                skip_next = true; // its value is a count, never a path
            }
            continue;
        }
        if !script_taken {
            script_taken = true; // this positional IS the script/pattern
            continue;
        }
        out.push(a.clone());
    }
    out
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
/// **charter#1165, reproduced on purpose.** Stripping the `!` throws away the one character
/// that tells `rg`'s two meanings apart: `--glob '!x'` excludes `x` and `--glob 'x'` *selects*
/// it. A glob aimed AT the state directory therefore reads here as an exclusion of it, and the
/// walk is allowed. The Python is this port's oracle and the fix belongs there; the corpus pins
/// today's answer so it cannot change on one side alone.
pub fn excluded_names(prog: &str, args: &[String]) -> Vec<String> {
    let argv: &[String] = if args.first().is_some_and(|a| a == prog) {
        &args[1..]
    } else {
        args
    };
    let mut raw: Vec<String> = Vec::new();
    let mut take_next = false;
    for a in argv {
        if take_next {
            take_next = false;
            raw.push(a.clone());
            continue;
        }
        // Python's `partition("=")`: no `=` gives the whole token as the name and an empty
        // separator, which is how "the value is the NEXT token" is told from "it is attached".
        match a.split_once('=') {
            Some((name, val)) if EXCLUDE_OPTS.contains(&name) => raw.push(val.to_string()),
            Some(_) => {}
            None if EXCLUDE_OPTS.contains(&a.as_str()) => take_next = true,
            None => {}
        }
    }
    raw.into_iter()
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
        // The `-` and `--` exceptions are **inert**, here and in the Python: a bare `-` has no
        // cluster letters to scan and `--` is skipped by the `starts_with("--")` arm below
        // anyway. Mutated away and measured against the oracle: zero answers changed, over the
        // fuzz and over the recording. Kept because it says what the two tokens ARE.
        if !a.starts_with('-') || a == "-" || a == "--" {
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
