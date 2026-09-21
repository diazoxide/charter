#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = [
#   "charter-cp @ git+https://github.com/diazoxide/charter@50d31dc66835592ccea625bab8f5a0da313f2444",
# ]
# ///
"""Differential test: does `charter-core` read a command line — and REFUSE one — the way Python
does?

`charter/hooks.py`'s `_segment_argv_parsed` and its closure are what every `PreToolUse` Bash
refusal stands on — a boundary the port invents strands a reader's operand and turns a deny into
an ALLOW. So the port is not argued against the Python source, it is MEASURED against the Python
code itself, running here, pinned to the same commit `run.py` pins.

Every function is compared per case, not one, so a divergence is attributed rather than merely
seen. Stage 1's six: `_unbacktick`, `_quote_map`, `_splice_continuations`, `_lex` (tokens with
their `bare` flag and their CHARACTER offsets, or the `ValueError` it raises),
`_split_punctuation` (the same, after a glued run comes apart) and `_segment_argv_parsed`.
Stage 2 adds the heredoc layout — `_strip_reader_heredocs`, `_heredoc_layout`,
`_heredoc_strip_plan`, `_line_pipelines`, `_heredoc_openers`, `_heredoc_header`,
`_heredoc_opener_words`, `_opener_program`, `_heredoc_could_run`, `_crowded_substitutions`,
`_pipeline_slice`, `_line_runs_text`, `_brief_heredocs`, `_desugar_ansi_c`,
`_compound_holds_executor`, `_segments_of`, `_comment_index`, `_ends_in_line_continuation`,
`_pipeline_continues` — and the wrapper/env split `_split_env_chdir` drags in with it:
`_commit_message_on_stdin`, `_gh_body_on_stdin`, `_redirect_reads`, `_git_globals`,
`_wrapper_option`, `_flag_name_value`, `_is_executor`, plus CPython's own `shlex.split` and
`shlex.quote`, which that walk calls and which the Rust reimplements.

Stage 3 adds the guard itself — `_leak_reason` and its neighbourhood: `_names_a_vault_path`,
`_file_operands`, `_spliced_operands`, `_gh_file_operands`, `_gh_at_path`, `_is_charter`,
`_lines_a_command_could_run`, `_walks_directories`, `_excluded_names`, `_guarded_state_entries`,
`_walk_into_guarded_state`, `_glob_selects_inside`, `_walks_into_guarded_state` — and the four
CPython answers that closure stands on and a port could plausibly get wrong: `os.path.normpath`,
`posixpath.join`, `os.path.realpath` and `fnmatch.fnmatch`.

**Every `pub` item of the five Rust modules is in `KEYS`**, or in `COVERED_ELSEWHERE` with the
key that compares it. Stage 1's one real harness defect was a field nobody diffed —
`_split_punctuation`'s per-piece offsets — and it was found by mutation rather than by reading,
so the rule is written down: a `pub` item in neither list is a rule with no evidence.

    ./shellseg.py                      # the default fuzz: 200,000 seeded cases
    ./shellseg.py --cases 2000         # fewer, while iterating
    ./shellseg.py --coverage           # which branches the cases REACH — read this first
    ./shellseg.py --record             # rewrite the checked-in curated corpus
    ./shellseg.py --check              # ...and fail if it moved

The Rust side is `cargo run --example shellseg_oracle -p charter-core`, which reads a
JSON-encoded command line per line and writes a JSON answer per line. It is an example rather
than a binary because nothing ships it. Build it first, or pass `--binary`.

Python is a DEV DEPENDENCY OF THIS TEST and of nothing else (spec decision 15).

# The alphabet, which is the ceiling of the evidence

Random bytes would test almost nothing: every interesting answer in this module lives on the
dozen characters a shell gives meaning to. So the cases are built by joining FRAGMENTS — the
quote and escape spellings, the operators longest and shortest, the redirections, a heredoc
opener, a comment, a newline, a backslash-newline, the programs the guard above this cares about
and a vault path — which is the same shape of alphabet the `secretshape` fuzz used.

Stage 1's author named that list as the ceiling of the evidence and named what it omitted;
stage 2 closes every omission — the astral plane, `!` `%` `^` `+` `:` `@` `[` `]`, most of the
latin-1 wordchar block, U+0085/U+00A0/U+2028 — and adds the two things the alphabet cannot
supply: the characters where CPython's `str.lower` and Rust's `to_lowercase` could part company,
and the Unicode decimal digits that `\\d` matches in both engines and `[0-9]` matches in neither.

Stage 3 applies that lesson to its own arm and adds two more generators, because a random join
reaches a tree walker standing on the state directory about as often as it opens a heredoc.
`--coverage` is the measurement, kept in the harness rather than in a PR body: it reports, per
branch of `_leak_reason`, how many of the generated cases got there. Run it before believing a
case count.

It also adds a SECOND GENERATOR, and the measurement that demanded it: 20,000 fragment-joins
dropped a heredoc body 15 times, and 13 of those were curated rows. A working heredoc needs its
delimiter to reappear ALONE on a later line, which a random join almost never produces — so the
layout's whole subject was barely being reached. `a_heredoc_case` builds from the grammar
instead, and half the cases come from it. The same 20,000 then dropped a body 92 times, opened
9,433 heredocs against 792, and reached `_compound_holds_executor` 580 times against 1.

The curated cases are every command line the Python docstrings name as a defect that shipped:
the quoted `)`, the `&` of `2>&1`, the `{` mid-command, the comment bypass in both spellings, the
substitution that stranded an operand, and the broken quote that folded every later line into
the first. They run in the fuzz too, at the front, so a run with `--cases 20` still covers them.
"""

from __future__ import annotations

import argparse
import fnmatch as _fnmatch
import json
import os
import posixpath
import random
import shlex
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parent.parent
CORPUS = REPO / "fixtures" / "corpora" / "shellseg-oracle.jsonl"
DEFAULT_BINARY = REPO / "target" / "debug" / "examples" / "shellseg_oracle"

VAULT = ".charter/vaults/x.json"

# --------------------------------------------------------------------------- #
# The fixture plane — stage 3's guard asks the FILESYSTEM, so the run needs one  #
# --------------------------------------------------------------------------- #
#
# `_guarded_state_entries` reads `config.STATE_DIR` and `_walk_into_guarded_state` resolves
# every operand against it, so half of `_leak_reason`'s last arm is a question about a real
# directory. The plane below is built before `charter.config` is imported — `$CHARTER_HOME` is
# read once, at import — and the Rust side is handed the same three paths in its environment,
# because the core takes the state directory as an ARGUMENT where the Python reads a global.
#
# **`.charter/` holds exactly ONE guarded entry on purpose.** `_guarded_state_entries` hands
# its caller the directory's own readdir order, so with two guarded entries under one operand
# the walk's answer is whichever the kernel yielded first — a fact about the filesystem, not
# about the guard, and not one two implementations can be held to. `rich/` carries every filter
# case instead and is compared SORTED. The limit that leaves is real and is stated rather than
# hidden: on a plane with a vault directory AND a browser profile, which of the two a refusal
# NAMES is not pinned by anything here.
ROOT = Path(os.path.realpath(tempfile.mkdtemp(prefix="shellseg-plane-")))
STATE = ROOT / ".charter"
RICH = ROOT / "rich"
MISSING = ROOT / "not-there"


def build_fixture() -> None:
    """The plane every filesystem answer in this run is about."""
    (STATE / "vaults").mkdir(parents=True, exist_ok=True)
    (STATE / "vaults" / "db.json").write_text("{}\n", encoding="utf-8")
    (STATE / "vaults.json").write_text("{}\n", encoding="utf-8")   # the REGISTRY, not a vault
    (STATE / "state").mkdir(exist_ok=True)
    (STATE / "state" / "x").write_text("x\n", encoding="utf-8")
    # Every entry-filter case, in a directory of its own so the order of the one above stays
    # unique: an EMPTY guarded directory is skipped, a guarded FILE is not, and a name that is
    # not the exact `vaults` or one of the three prefixes is not an entry at all.
    (RICH / "vaults").mkdir(parents=True, exist_ok=True)            # EMPTY -> skipped
    (RICH / "browser" / "p").mkdir(parents=True, exist_ok=True)
    (RICH / "browser" / "p" / "prefs").write_text("x\n", encoding="utf-8")
    (RICH / "browser-empty").mkdir(exist_ok=True)                   # EMPTY -> skipped
    for name in ("active-persona", "fingerprint.key", "browsers.txt", "vaults.json", "other"):
        (RICH / name).write_text("x\n", encoding="utf-8")
    (ROOT / "docs").mkdir(exist_ok=True)
    (ROOT / "docs" / "a.md").write_text("a\n", encoding="utf-8")
    (ROOT / "sub" / "deep").mkdir(parents=True, exist_ok=True)
    (ROOT / "sub" / "deep" / "b.txt").write_text("b\n", encoding="utf-8")
    # The symlinks `realpath` is measured on: one to the cwd, one to the parent, one into the
    # state directory, a chain and one dangling. Each is a spelling of an ancestor that the
    # guard has to resolve rather than read.
    #
    # `absvaults` is ABSOLUTE on purpose: an absolute symlink target RESETS the resolved path to
    # `/` in CPython's `realpath`, and with only relative links in the fixture a port that
    # skipped that reset changed no answer — a mutation that came back inert because the
    # evidence had no case, not because the rule had no effect.
    #
    # **There is deliberately NO LOOPING LINK**, and the reason is a finding rather than a
    # tidy-up. `_walk_into_guarded_state` resolves with `Path.resolve()`, and on CPython 3.11 and
    # 3.12 `pathlib`'s `check_eloop` turns the kernel's `ELOOP` into a **`RuntimeError`** — which
    # `except OSError` does not catch, so the ORACLE raises out of `pretooluse` (charter#1166;
    # 3.14 returns the path, as `os.path.realpath` does on all three). A differential cannot
    # arbitrate an input one side has no answer for, and CI's interpreter is 3.12. The Rust's own
    # behaviour there is pinned by a unit test in `pypath` instead.
    #
    # `hop1 → hop2 → hop3 → .charter` and `alsostate → .charter` keep the two things the loop was
    # there for: a link whose target is itself a link (so `realpath` really re-enters), and two
    # links onto one target (so the `seen` cache is hit rather than only filled).
    for link, target in (("here", "."), ("up", ".."), ("tostate", ".charter"),
                         ("dangling", "nowhere"), ("absvaults", str(STATE / "vaults")),
                         ("hop3", ".charter"), ("hop2", "hop3"), ("hop1", "hop2"),
                         ("alsostate", ".charter")):
        p = ROOT / link
        if not p.is_symlink():
            p.symlink_to(target)


build_fixture()
# Set before charter is imported: `config._migrate_state_dir` reads `$CHARTER_HOME` once, at
# import, and uses it verbatim.
os.environ["CHARTER_HOME"] = str(STATE)

from charter import hooks  # noqa: E402  (after $CHARTER_HOME, deliberately)

#: Every command line a Python docstring in `hooks.py` names as a bypass that SHIPPED, plus the
#: ones its review rounds pinned. Each is a rule in `shellseg.rs`; deleting the rule changes the
#: answer for the line beside it.
CURATED: list[str] = [
    "",
    " ",
    "echo hi",
    # A quoted or escaped operator is a WORD, not a boundary — `cat` keeps its operand.
    rf"cat \) {VAULT}",
    rf"cat ')' {VAULT}",
    rf"cat '()' {VAULT}",
    rf"cat '&&' {VAULT}",
    rf"cat '|' {VAULT}",
    rf'cat "(" {VAULT}',
    # A glued punctuation RUN is one shlex token and has to come apart.
    rf"( true );cat {VAULT}",
    rf"(true)&&cat {VAULT}",
    rf"echo x|cat {VAULT}",
    # ...but not into operator CHARACTERS: the `&` of `2>&1` belongs to the redirection.
    rf"cat 2>&1 {VAULT}",
    rf"cat >|out {VAULT}",
    rf"cat <>{VAULT}",
    rf"tee <{VAULT}",
    rf"<{VAULT} tee",
    # `{`/`}` are reserved words: a boundary in command position, an argument anywhere else.
    rf"cat {{ {VAULT}",
    rf"{{ cat {VAULT}; }}",
    rf"( cat {VAULT} )",
    rf"cat ( x {VAULT}",
    rf"cat ) x {VAULT}",
    # ...and in COMMAND position, where a quoted `(`/`{` is the one place reading it as an
    # operator changes the answer: the guard then loses the word the group's program stands
    # behind. Added after a PROOF ONLY run measured that dropping `Tok::bare` from `is_op`
    # changed no answer in the 64 rows here — only 236 of 200,000 in the fuzz.
    rf"'(' cat {VAULT}",
    rf"'{{' cat {VAULT}",
    rf"\( cat {VAULT}",
    rf"\{{ cat {VAULT}",
    # A substitution is an inner segment AND keeps the outer one accumulating.
    rf"cat $(echo {VAULT})",
    rf"echo $(cat {VAULT})",
    rf'echo "$(cat {VAULT})"',
    rf"cat `echo {VAULT}`",
    rf"echo `cat {VAULT}`",
    rf"cat \$(echo {VAULT})",
    rf"cat <(echo {VAULT})",
    rf"cat >(tee {VAULT})",
    rf"x$(echo {VAULT})",
    rf"cat `echo {VAULT}",  # an odd backtick count leaves the substitution open
    # A comment ends at the newline, and only starts at a word start.
    f"echo hi#; cat {VAULT}",
    f"echo a # note\ncat {VAULT}",
    f"echo hi\ncat {VAULT}",
    f"# cat {VAULT}",
    f"echo '#' ; cat {VAULT}",
    # A line continuation is spliced where the backslash is live, and nowhere else.
    f"cat \\\n {VAULT}",
    f"echo '\\\n' ; cat {VAULT}",
    f"# note \\\ncat {VAULT}",
    f'echo "a\\\nb" ; cat {VAULT}',
    # Unbalanced quoting: the fallback has to keep bash's LINE structure.
    f"echo $'it\\'s fine' ; cat {VAULT}",
    f"cd .charter/vaults\ncat x.json\necho \"",
    f'grep -e "a\nb" x.json',
    'echo "',
    "echo '",
    "echo \\",
    # Prose that merely MENTIONS an operator stays prose.
    "echo 'example: cd somewhere ; git checkout -b my-branch'",
    'git commit -m "docs: document the --reveal flag"',
    # `$` inside `"…"` opens nothing: the regex anchor must not swallow the closing quote.
    'grep -v "^$" f',
    "git commit -m \"$(cat <<'EOF'\nbody\nEOF\n)\"",
    # Relocation, empty operands, and a few plain shapes.
    "cd .charter/vaults && cat x.json",
    "cat ''",
    'cat ""',
    "a;b",
    "a;;b",
    "a |& b",
    f"V={VAULT}; cat $V",
    f"env -C .charter/vaults cat x.json",
    # Python's whitespace is not Rust's: U+001C-U+001F are blank to `str.strip`/`str.split`.
    "\x1c",
    "echo\x1ca",
    f"echo \"\n\x1c\n\" ; cat {VAULT}",
    # The blank test and `str.split` are two SEPARATE readings of that difference, and only
    # this shape reaches the first: a line of nothing but separator controls, on a command the
    # lexer cannot take apart, with the broken quote LAST so the newlines before it are still
    # boundaries. Added after a PROOF ONLY run measured that making only the blank test
    # `char::is_whitespace` changed no answer in the 64 rows here — 40 of 200,000 in the fuzz.
    f"cat {VAULT}\n\x1c\necho \"",
    f" \x1c\ncat {VAULT}\necho \"",
    # A tab and a carriage return are whitespace; a newline is an operator.
    "echo\ta\rb",
    "echo a\r\ncat b",
    # ---- stage 2: the heredoc layout, and every defect the Python docstrings name.
    # A quoted body fed to a reader with no executor in the pipeline is DATA and comes out.
    f"cat <<'DOC'\n{VAULT}\nDOC",
    f"cat > file <<'DOC'\n{VAULT}\nDOC",
    f"env cat <<'DOC'\n{VAULT}\nDOC",
    # ...an UNQUOTED one is not: it expands before the reader sees it.
    f"cat <<DOC\n$(cat {VAULT})\nDOC",
    # ...and neither is one whose pipeline runs a shell. #973: the old pre-pass asked only
    # whether the LINE began with a reader.
    f"cat <<'A' | bash\n{VAULT}\nA",
    f"cat x && bash <<'A'\ncat {VAULT}\nA",
    f"cat x; bash <<'A'\ncat {VAULT}\nA",
    f"cat x || bash <<'A'\ncat {VAULT}\nA",
    f"cat x & bash <<'A'\ncat {VAULT}\nA",
    # A pipeline that spans physical lines: the executor is on the CONTINUATION.
    f"cat <<'A' |\n{VAULT}\nA\nbash",
    f"cat <<'A' \\\n| bash\n{VAULT}\nA",
    # #1086 class 2: a comment is not spliced, so its trailing backslash continues nothing.
    f"cat <<'A' # note\\\n{VAULT}\nA\ncat {VAULT}",
    # #1086 class 1: a quote bash has not closed swallows the newline, so a `<<` inside it
    # opens nothing.
    f'echo "a\n<<B\n" ; cat {VAULT}',
    # #975 / round 3's Critical: bash's terminator is not the regex's.
    f"cat <<'EO'F\nbody\nEOF\ncat {VAULT}",
    f"cat <<EO'F'\nbody\nEOF\ncat {VAULT}",
    f"charter handoff w <<BRIEF'X'\nbody\nBRIEFX\ncat {VAULT}",
    # `<<\EOF` quotes the delimiter exactly as `<<'EOF'` does, and the regex must SEE it or
    # the count disagrees and every plan on the line is unknown (round 4, finding 1).
    f"bash <<\\EOF\ncharter handoff w\nEOF",
    # `<<""` and `<<''` name the EMPTY delimiter and are real heredocs; `<<` alone is not.
    'cat <<""\nbody\n\ncat x',
    "cat <<\nnot a heredoc",
    # `<<-` strips leading TABS from the terminator, and only tabs.
    "cat <<-'A'\n\tbody\n\tA",
    "cat <<-'A'\n\tbody\n  A",
    # A terminator that never arrives: the body is kept whoever opened it.
    f"cat <<'A'\n{VAULT}",
    # A here-string is counted by the regex and not by the lexer — the plan is unknown.
    f"cat <<<x <<'A'\n{VAULT}\nA",
    # A `<<` inside quotes opens nothing (round 5, ruling B).
    "echo \"use <<EOF for heredocs\"",
    "rg -n '<<\\w' docs/",
    # A group, a subshell and a substitution are `None`, not a guess.
    f"{{ cat <<'A'; }} | bash\n{VAULT}\nA",
    f"( cat <<'A' )\n{VAULT}\nA",
    f"x=$(cat <<'A')\n{VAULT}\nA",
    # #1086 class 3: a compound command's own `;` is not a pipeline boundary.
    f"cat <<'EOF' | while read l; do eval \"$l\"; done\ncat {VAULT}\nEOF",
    # Round 5 ruling C: a live backtick makes the answer WRONG rather than absent.
    "x=`bash <<'EOF'\ncharter handoff w\nEOF",
    r'gh pr create --title "keeps its \`~/\`" -F - <<EOF' + "\nbody\nEOF",
    # #1086 class 4: `$'…'` is a token boundary `shlex` does not know.
    "cat <<'A' | sh -s $'it\\'s'\nbody\nA",
    # Round 6 ruling 4: two heredocs in one `$( … )`, one of them a shell's.
    f"x=$( cat <<'A' > n.md; bash <<'B' )\ncharter handoff w\nA\ntrue\nB",
    # Round 6 ruling 3: an opener charter cannot NAME is a reason to search the body.
    "${RUNNER} <<'EOF'\ncharter handoff w\nEOF",
    "$(which bash) <<'EOF'\ncharter handoff w\nEOF",
    # Round 7 item 3 / round 8 finding 1 / round 9 finding 1: a CLOSED substitution before the
    # opener is an argument, and a backtick is a group one character opens and closes.
    "( cat > \"$(date +%F).md\" <<'EOF' )\nbody\nEOF",
    "( tee \"$(mktemp)\" <<'EOF' )\nbody\nEOF",
    "( tee \"`date`\" <<'EOF' )\nbody\nEOF",
    "( bash `d; cat ` <<'EOF' )\nbody\nEOF",
    "mail -s \"${SUBJ}\" me <<'EOF'\nbody\nEOF",
    # #997 and #1070: a message and a body on stdin are data, and an EDITOR spelling is not.
    "git commit -F - <<'MSG'\nfix: it\nMSG",
    "git commit --file=- <<'MSG'\nfix: it\nMSG",
    "git -C /tmp commit -F - <<'MSG'\nfix: it\nMSG",
    "git commit -e -F - <<'MSG'\nfix: it\nMSG",
    "git commit -aF - <<'MSG'\nfix: it\nMSG",
    "git commit > -F- <<'MSG'\nfix: it\nMSG",
    "git commit -F - -- -F - <<'MSG'\nfix: it\nMSG",
    "gh pr create --body-file - <<'EOF' | tail -1\nbody\nEOF",
    "gh issue comment -F- <<'EOF'\nbody\nEOF",
    "gh pr edit -F - <<'EOF'\nbody\nEOF",
    "gh pr create --editor -F - <<'EOF'\nbody\nEOF",
    # A brief is charter's stdin, and only in that exact spelling.
    "charter handoff w <<'BRIEF'\nplease read .charter/vaults\nBRIEF",
    "charter handoff b && bash <<'EOF'\ncharter handoff w\nEOF",
    "python3 -m charter handoff w <<'BRIEF'\nbody\nBRIEF",
    # `<<'PYTHON'` opens no interpreter: the headers are cut out before the words are read.
    f"cat <<'PYTHON'\n{VAULT}\nPYTHON",
    # ---- stage 2: the wrapper/env split.
    f"env cat {VAULT}",
    f"env -i cat {VAULT}",
    f"env -P /bin cat {VAULT}",
    f"env -C .charter/vaults cat x.json",
    f"env -iC.charter/vaults cat x.json",
    f"env -iC .charter/vaults cat x.json",
    f"env -Cx=y/../.charter/vaults cat x.json",
    f"env -Sfoo=1 cat {VAULT}",
    f"env --split-string=foo=1 cat {VAULT}",
    f"env -S 'cat {VAULT}'",
    f"env -iS'cat {VAULT}'",
    f"env a-b=1 cat {VAULT}",
    f"env -- a-b=1 cat {VAULT}",
    f"sudo -T 5 cat {VAULT}",
    f"sudo -u root cat {VAULT}",
    f"sudo -bD/tmp cat {VAULT}",
    f"doas a-b=1 cat {VAULT}",
    f"chrt 5 cat {VAULT}",
    f"su-exec root cat {VAULT}",
    f"timeout 5 cat {VAULT}",
    f"timeout 1.5s cat {VAULT}",
    f"xargs -e cat {VAULT}",
    f"xargs -E EOF cat {VAULT}",
    f"xargs -a {VAULT} echo",
    f"xargs -0a{VAULT} echo",
    f"nice -n 5 cat {VAULT}",
    f"stdbuf -o0 cat {VAULT}",
    f"env --nonesuch cat {VAULT}",
    f"env -q cat {VAULT}",
    f"< {VAULT} cat",
    f"2>/dev/null env cat {VAULT}",
    f"tee < {VAULT}",
    f"FOO=bar env BAZ=qux cat {VAULT}",
    f"env GIT_SSH_COMMAND=/tmp/k git push",
    f"then cat {VAULT}",
    # `\d` is `\p{Nd}`: an Arabic-Indic digit is a file descriptor to both engines.
    f"٣> out cat {VAULT}",
    "timeout ٣ cat x",
    # ---- stage 2, second pass: the rows a MUTATION SWEEP found the recording could not see.
    #
    # Each of these was added because breaking the rule beside it changed answers in the fuzz and
    # changed NOTHING in the 162 rows above — which is a rule with no test on the day the fuzz is
    # trimmed. They are not decoration; each one is a mutation that now goes red.
    #
    # `--edit` in its LONG spellings. The short cluster `-e` was covered; the prefix branch
    # (`len(w) >= 3 and "--edit".startswith(w)`) was covered by nothing at all.
    "git commit --edit -F - <<'MSG'\nfix: it\nMSG",
    "git commit --edi -F - <<'MSG'\nfix: it\nMSG",
    # A compound word OUT of command position, with the executor in one. Every earlier row had
    # `while` where a command begins, so relaxing the position test changed no recorded answer.
    "echo while | eval x",
    # git stops reading its own globals at the first non-option token: the `-C` before `switch`
    # is a chdir and the one after it is `--force-create` (#483).
    "git -C /tmp switch -C neu",
    # The `\d*` of `_REDIRECT_RE` is only ever REACHED through a quoted token, because the lexer
    # hands a bare `2>` back as `2` and `>`. So a Unicode digit needs the quoted spelling to
    # matter at all — and then it decides whether the SHELL opens the path.
    f'cat "٣<" {VAULT}',
    f'env "٣>" x cat {VAULT}',
    # CPython's `$` matches before a newline that ends the string; the `regex` crate's does not.
    # A quoted token really can hold one.
    f'cat "<\n" {VAULT}',
    # In an UNQUOTED body a trailing backslash splices, so the next line cannot be the
    # terminator — bash runs the body on past it, and the first `A` here is body, not the end.
    f"cat <<A\nbody \\\nA\nA\ncat {VAULT}",
    # The pipeline, not the line: the `bash` after the `;` does not run this body. Asking the
    # whole line is the round-4 over-refusal, and the group is what makes the plan unknown so
    # that `_heredoc_could_run` is the answer at all.
    f"( cat <<'A' ; bash )\n{VAULT}\nA",
    # ...and with the break IN FRONT of the opener, which is the half that moves `lo` rather
    # than `hi`. Without it the slice is the whole line either way and the rule looks inert.
    f"( bash ; cat <<'A' )\n{VAULT}\nA",
    # A VERSIONED interpreter is one: `python3.12` is in no name list and runs the body.
    "python3.12 <<'EOF'\ncharter handoff w\nEOF",
    # An EVEN run of trailing backslashes is a literal `\`, not a continuation.
    "echo a\\\\",
    f"echo a\\\\\ncat {VAULT}",
    # ----------------------------------------------------------------------------- #
    # M3.1 stage 3: the leak guard itself. Every row is a command line a `hooks.py`   #
    # docstring names as a bypass that SHIPPED, or the false positive that closing it #
    # cost, and each is a rule in `leakguard.rs`.                                     #
    # ----------------------------------------------------------------------------- #
    # The spellings one keystroke apart from the denied form, which the TEXT pattern
    # missed until `normpath` was tested beside it.
    f"cat {VAULT}",
    "cat .charter//vaults/x.json",
    "cat .charter/./vaults/x.json",
    "cat .charter/../.charter/vaults/x.json",
    # ...and the case spellings, which are the same inode on APFS (#476's half that stayed).
    "cat .CHARTER/vaults/x.json",
    "cat .charter/VAULTS/x.json",
    # `vaults` anchored to a SEGMENT: the directory itself is denied (#462 round three) and the
    # REGISTRY beside it is not (#443's false positive, which came back through the other
    # predicate).
    "cat .charter/vaults",
    "cat .charter/vaults.json",
    "grep -rn vaults .charter/vaults.json",
    "ag TOKEN .charter/vaults.json",
    "cat .charter",
    "cat .charter/",
    "cat .charterx",
    "cat .edm/vaults/x.json",
    "cat .charter/active-persona",
    "cat .charter/fingerprint.key",
    "cat .charter/browser-profile",
    # The two characters CPython's `(?i)` folds to `i` and `regex`'s simple folding does not.
    "cat .charter/actİve-persona",
    "cat .charter/actıve-persona",
    "cat .charter/fıngerprİnt.key",
    # A relocation, in every spelling the guard follows — and the one it does not (`popd`).
    "cd .charter/vaults && cat x.json",
    "pushd .charter/vaults && cat x.json",
    "cd .charter && cd vaults && cat x.json",
    "env -C .charter/vaults cat x.json",
    "sudo --chdir=.charter/vaults cat x.json",
    "env -iC.charter/vaults cat x.json",
    "cd /tmp && cat x.json",
    "cd sub && cat ../.charter/vaults/x.json",
    "cd .charter/vaults; cd ..; cat x.json",
    # A wrapper's chdir moves THAT program only and does not outlive its segment.
    "env -C .charter/vaults cat x.json && cat x.json",
    # The SHELL opens it, before any program is execed — and `prog` can be empty.
    f"< {VAULT} tee",
    f"tee < {VAULT}",
    f"xargs -a {VAULT} echo",
    f"cat <> {VAULT}",
    # A reader whose first operand is a SCRIPT or a PATTERN, and the flag values that are not
    # paths. `sed -n 1p <vault>` is a read; `sed -i 's|<vault>|x|' f` is a mention.
    f"sed -n 1p {VAULT}",
    f"sed -i 's|{VAULT}|x|' f",
    f"sed -e p {VAULT}",
    f"sed -e {VAULT} f",
    f"grep -rn '{VAULT}' docs/",
    f"grep -e {VAULT} f",
    f"grep -f {VAULT} f",
    f"grep --regexp={VAULT} f",
    f"head -n 5 {VAULT}",
    f"head -n {VAULT}",
    f"awk '{{print}}' {VAULT}",
    f"awk -f prog.awk {VAULT}",
    f"od -N 5 {VAULT}",
    f"xxd -l 4 {VAULT}",
    f"cat -- {VAULT}",
    f"cat - {VAULT}",
    # The word a substitution splices back, and the false deny that buys. The third row is the
    # one the SPLICE alone denies: the first two already name `.charter` as an operand of their
    # own, so with only those a port that never joined a pair changed no refusal at all.
    "cat $(echo .charter)/vaults/x.json",
    "cat .charter /vaults/x.json",
    "cat $(echo .char)ter/vaults/x.json",
    "cat a b .charter /vaults/x.json",
    # `--reveal` as a real flag, and as a MENTION — the false positive this guard was rewritten
    # to stop having.
    "charter secret get v k --reveal",
    "charter secret get v k --reveal=1",
    "edm secret get v k --reveal",
    "python3 -m charter secret get v k --reveal",
    "python3.12 -m charter secret get v k --reveal",
    "CHARTER secret get v k --reveal",
    "/usr/local/bin/charter secret get v k --reveal",
    'git commit -m "docs: document the --reveal flag"',
    "rg -n -- --reveal charter/",
    "echo --reveal",
    "charter secret get v k --revealed",
    # gh is not a reader and uploads the file anyway (#1086 class 5); `-` is the stdin body.
    f"gh pr create -F {VAULT}",
    f"gh pr create --body-file={VAULT}",
    f"gh pr create -F{VAULT}",
    f"gh issue comment -T {VAULT}",
    f"gh release create v1 --notes-file {VAULT}",
    f"gh api -f body=@{VAULT}",
    f"gh api -F body=@{VAULT}",
    f"gh api -Fbody=@{VAULT}",
    f"gh api --input {VAULT}",
    f"gh api --field body=@{VAULT}",
    f"gh api --field body=x=@{VAULT}",
    "gh api -F body=@-",
    "gh pr create -F -",
    "gh pr create -F notes.md",
    f"gh pr create -- -F {VAULT}",
    f"gh --flag api --input {VAULT}",
    "gh pr create -F",
    # The operand that CONTAINS the vault directory without naming it (#474), and the fix the
    # refusal prints, which has to run.
    "grep -rn TOKEN .",
    "grep -r TOKEN",
    "rg TOKEN",
    "ag TOKEN",
    "rg TOKEN docs",
    "grep -n TOKEN .",
    "grep -d recurse TOKEN .",
    "grep --directories=recurse TOKEN .",
    "grep -eR foo .",
    "grep -Re foo .",
    "grep -rn --exclude-dir=.charter TOKEN .",
    "grep -rn --exclude-dir='.char*' TOKEN .",
    "grep -rn --exclude-dir='[.]charter' TOKEN .",
    "grep -rn --exclude-dir=vaults TOKEN .charter",
    "rg --glob '!.charter' TOKEN .",
    "rg --glob '**/.charter/**' TOKEN .",
    "rg --glob '*/.charter/*' TOKEN .",
    "grep -rn --exclude-dir=.charter TOKEN sub",
    "cd sub && grep -rn TOKEN .",
    "cd .charter && grep -rn TOKEN .",
    "grep -rn TOKEN ..",
    "grep -rn TOKEN here",
    "grep -rn TOKEN tostate",
    "grep -rn TOKEN dangling",
    "grep -rn TOKEN hop1",
    "grep -rn TOKEN up/",
    "env -C sub grep -rn TOKEN .",
    "grep -rn TOKEN .charter",
    "grep -rn TOKEN .charter/vaults",
    "grep -rn TOKEN /",
    # The unparseable path: argv is a guess, so the RAW string is scanned — the arm whose
    # absence let a command hide behind a broken quote.
    f"cat '{VAULT}",
    "echo 'x --reveal",
    "echo 'x --reveal=1",
    f"echo 'it is {VAULT}",
    "echo 'x\x1c--reveal\x1c",
    "echo 'x --reveal\n",
    # A heredoc body is data unless something runs it — the leak guard's half of A7's fact.
    f"cat <<'EOF'\n{VAULT}\nEOF",
    f"bash <<'EOF'\ncat {VAULT}\nEOF",
    f"git commit -F - <<'MSG'\nmentions {VAULT}\nMSG",
    f"git commit -e -F - <<'MSG'\nmentions {VAULT}\nMSG",
    f"gh pr create --body-file - <<'EOF'\nmentions {VAULT}\nEOF",
    f"cat <<'EOF'\nx\nEOF\ncat {VAULT}",
    f"cat <<'EOF' | bash\ncat {VAULT}\nEOF",
    # Wrappers in front of the program, including the one that packs a whole command.
    f"env FOO=1 cat {VAULT}",
    f"sudo -u root cat {VAULT}",
    f"timeout 5 cat {VAULT}",
    f"env -S 'cat {VAULT}'",
    f"nohup cat {VAULT}",
    f"stdbuf -oL cat {VAULT}",
    f"env -P /bin cat {VAULT}",
    # An absolute symlink, which is the one shape that makes `realpath` reset its resolved path.
    "grep -rn TOKEN absvaults",
    "cat absvaults/db.json",
    # The three defects this stage found in the FROZEN PYTHON, whose current answers are pinned
    # here so that the fix landing upstream shows up as a divergence rather than silently.
    # charter#1164: the value of `-f`/`--file` is a file these programs really OPEN, and the
    # guard skips it — `awk` even quotes the offending source text back on stderr.
    f"awk -f {VAULT} data.txt",
    f"grep -f {VAULT} data.txt",
    f"sed --file={VAULT} data.txt",
    # charter#1165: `rg`'s `--glob` without a `!` is an INCLUSION, and `_excluded_names` reads
    # it as an exclusion — so a search aimed AT the state directory is allowed.
    "rg --glob '.charter/**' TOKEN .",
    "rg -g .charter/** TOKEN .",
    "rg --iglob vaults TOKEN .",
    # ...and `--exclude` is a FILE exclusion that does not stop the descent, which the Python's
    # own docstring declares as deliberately permissive rather than as a defect.
    "grep -rn --exclude=.charter TOKEN .",
    # The prefix and suffix strips are CHAINED, not alternatives: `**/` then `*/`.
    "rg --glob '**/*/.charter/*/**' TOKEN .",
    "rg --glob '!!!/.charter//' TOKEN .",
]

#: What a case is built out of. Single characters where the shell gives one meaning, and the
#: multi-character spellings whose LONGEST-FIRST order is the thing under test.
FRAGMENTS: list[str] = [
    " ", " ", " ", "\t", "\r", "\n", "\n",
    "'", '"', "\\", "$", "`", "#", "=",
    "(", ")", "{", "}", ";", "&", "|", "<", ">",
    "$(", "${", "$'", '$"', "<(", ">(", "&&", "||", ";;", "|&",
    "<<", "<<<", "<>", "<&", ">>", ">&", ">|",
    "\\\n", "\\'", '\\"', "\\\\", "\\n", "''", '""', "'x'", '"x"', "$'x'",
    "cat", "echo", "git", "charter", "env", "sudo", "xargs", "sh", "-c",
    "--reveal", "a", "b", "x", "EOF", VAULT, "2", "1",
    "é", "ß", "~", "*", "?", "/", ".", "-", "_", "\x1c", "\x1f",
    # --- widened for stage 2. Stage 1's author named this list as the CEILING OF THE EVIDENCE
    # and named what it omitted; every omission is closed here rather than inherited.
    #
    # The punctuation `shlex` gives no meaning to but `_line_runs_text`, `_comment_index` and
    # `shlex.quote`'s safe set do.
    "!", "%", "^", "+", ":", "@", "[", "]", ",",
    # The blanks CPython's `\s`/`str.isspace()` counts and an ASCII reading does not. U+0085 and
    # U+00A0 decide `_HEREDOC_RE`'s `\s*` — `<< EOF` IS a heredoc to the regex and is not
    # one to `_heredoc_header`, whose skip is `" \t"` — and U+2028 is the line separator Rust
    # and Python disagree about in `.lines()`.
    "", " ", " ", " ", "", "",
    # The latin-1 half of `shlex`'s posix `wordchars`, most of which stage 1 never generated.
    "à", "ç", "ð", "ø", "þ", "ÿ", "Æ", "Ø", "Þ",
    # Case folding: the four characters where CPython's `str.lower` and Rust's `to_lowercase`
    # could part company, plus the sigma whose mapping is context-sensitive. `base_lower` is
    # asked of every program name, so a divergence here is a divergence about what a program IS.
    "İ", "ı", "K", "ẞ", "Σ", "ΑΣ",
    # `\d` is `\p{Nd}` in BOTH engines and `[0-9]` in neither: `_REDIRECT_RE` and `_DURATION_RE`
    # accept an Arabic-Indic digit in front of a redirection.
    "٣", "୩", "１",
    # Astral plane: four bytes in UTF-8, ONE character to CPython's `str` indexing, and every
    # offset in this module is a character offset.
    "😀", "𝄞", "\U0001d7ce",
    # The heredoc spellings the layout turns on.
    "<<-", "<<'", "EOF'", "EO'F'", "<<\\", "-",
    # NOT here, and it is a gap rather than an omission: **U+0000**. An operand holding one makes
    # `Path.resolve()` raise `ValueError` out of `_walk_into_guarded_state`, `_leak_reason` and
    # `pretooluse` — charter#1166 — so putting it in the alphabet would crash the ORACLE rather
    # than measure it. The Rust answers there instead of raising, which is a declared divergence
    # named in `leakguard`'s header, and the differential cannot arbitrate it until the Python
    # is fixed.
    #
    # The wrapper grammar's own vocabulary: a value flag, a bundle, `env -S`, git's and gh's
    # stdin spellings, and an executor to stand downstream of a body.
    "-S", "-C", "-iC", "-F", "-F-", "--file=-", "--body-file=-", "-a", "-e", "--edit",
    "commit", "pr", "create", "issue", "comment", "bash", "python3.12", "tee", "handoff",
    "5", "root", "--", "FOO=1", "a-b=1",
]


#: The pieces a WELL-FORMED heredoc is assembled from. A random join of FRAGMENTS almost never
#: produces one — measured, 20,000 such cases dropped a body 15 times and 13 of those were
#: curated rows — because a working heredoc needs its delimiter to reappear ALONE on a later
#: line. That is the ceiling of the evidence stage 1 warned about, in its sharpest form: the
#: layout's whole point is what happens between an opener and its terminator, and the fuzz was
#: barely reaching it. So half the cases are built here instead, from the grammar rather than
#: from the alphabet.
OPENERS = ["<<EOF", "<<'EOF'", '<<"EOF"', "<<\\EOF", "<<-EOF", "<<-'EOF'",
           "<<EO'F'", "<<'EO'F", '<<""', "<<''", "<<- 'EOF'", "<<\tEOF", "<<<EOF"]
PROGRAMS = ["cat", "bash", "sh -s", "python3", "tee out.md", "env cat", "sudo -u root cat",
            "git commit -F -", "git commit -e -F -", "gh pr create --body-file -",
            "gh pr edit -F -", "charter handoff w", "charter handoff", "mail -s x me",
            "${RUNNER}", "$(which bash)", "`which bash`", "xargs cat", "wc -l",
            'tee "$(mktemp)"', "grep -q x",
            # the wrapper grammar's own shapes, so `chdir` and `reads` are reached at scale
            "env -C .charter/vaults cat", "sudo --chdir=/tmp cat", "env -S 'cat x'",
            "xargs -a in.txt echo", "env -P /bin cat", "timeout 5 cat", "chrt 5 cat"]
TAILS = ["", " | bash", " | tail -1", "; bash", " && bash", " || bash", " & bash", " |",
         " \\", " # note", " # note\\", " > out", " 2>&1", " | while read l; do eval \"$l\"; done"]
BODIES = [VAULT, f"cat {VAULT}", f"$(cat {VAULT})", "charter handoff w", "plain prose",
          "ends in a splice \\", "EOF", "\tEOF", " EOF", "EOFX", "", "#EOF", "a | b"]
TERMINATORS = ["EOF", "\tEOF", "  EOF", "EOFX", "", "EO", None]
WRAPS = [None, "( {} )", "{{ {} ; }}", "$( {} )", "x=$( {} )", "`{}`", 'echo "$( {} )"']


def a_heredoc_case(rng: random.Random) -> str:
    """One generated command that really opens a heredoc, body and terminator included."""
    openers = rng.randint(1, 2)
    head = " ".join(
        f"{rng.choice(PROGRAMS)} {rng.choice(OPENERS)}" if k == 0 else rng.choice(OPENERS)
        for k in range(openers)
    ) + rng.choice(TAILS)
    wrap = rng.choice(WRAPS)
    if wrap is not None:
        head = wrap.format(head)
    lines = [head]
    for _ in range(openers):
        for _ in range(rng.randint(0, 3)):
            lines.append(rng.choice(BODIES))
        term = rng.choice(TERMINATORS)
        if term is not None:
            lines.append(term)
    if rng.random() < 0.6:
        lines.append(rng.choice([f"cat {VAULT}", "bash", f"echo {VAULT}", "true"]))
    return "\n".join(lines)


#: The walker grammar. Stage 2's lesson, applied to stage 3's arm: a random join of FRAGMENTS
#: reaches `_walks_into_guarded_state` almost never, because reaching it needs a tree walker, a
#: recursion flag and an operand that resolves onto the state directory all in one line. So this
#: builds one — measured below, it is 27% of the run and it is where the last arm's answers are.
WALK_PROGS = ["grep -rn", "grep -r", "grep -R", "grep --recursive", "grep -d recurse",
              "grep --directories=recurse", "grep -eR", "grep -n", "grep -rn -m 2",
              "rg", "ag", "rg -e", "ag --max-count 2", "cat", "sed -n"]
WALK_EXCLUDES = ["", " --exclude-dir=.charter", " --exclude-dir='.char*'", " --exclude-dir=",
                 " --glob '!.charter'", " --glob '**/.charter/**'", " --exclude-dir=vaults",
                 " --ignore-dir=.charter", " -g '!.charter'", " --exclude-dir='[.]charter'",
                 " --exclude=*", " --iglob '!vaults'", " --exclude-dir=.charter/",
                 " --exclude-dir='[b-a]'", " --exclude-dir='?charter'"]
WALK_WHERE = ["", "cd sub && ", "cd .charter && ", "pushd docs && ", "env -C sub ",
              "sudo --chdir=/tmp ", "cd / && ", "cd .. && ", "cd here && "]
WALK_OPERANDS = ["", " .", " ..", " sub", " docs", " here", " up", " tostate", " .charter",
                 " .charter/vaults", " /", " dangling", " hop1", " alsostate", " ./sub/../.charter", " ''",
                 " absvaults", " here/absvaults", " rich"]

#: The reader grammar — the other half of `_leak_reason`, which decides on the TEXT of an
#: operand rather than on where a walk goes.
READ_PROGS = ["cat", "sed -n 1p", "head -n 5", "tail", "od -N 3", "xxd -l 4", "awk",
              "grep -e P", "strings", "nl", "tac", "bat", "less", "gh pr create", "gh api",
              "gh issue comment", "charter secret get", "edm secret get", "python3 -m charter",
              "tee <", "xargs -a", "env cat", "sudo -u root cat", "timeout 5 cat", "git show",
              "env -C .charter/vaults cat", "cd .charter/vaults && cat"]
READ_FLAGS = ["", " -F", " -T", " --body-file=", " --input", " -f body=@", " -F body=@",
              " --reveal", " --reveal=1", " -F-", " --", " -e", " -n", " --notes-file"]
READ_OPERANDS = [" " + VAULT, " .charter", " .charter/vaults", " docs/a.md", " x.json",
                 " .charter/vaults.json", " $(echo .charter)/vaults/x.json",
                 " .charter //vaults/x.json", " .charter/actİve-persona",
                 " .CHARTER/VAULTS/x.json", " .charter/./vaults/x.json", " -", " ''",
                 # the splice-ONLY shapes: neither word names a vault and the join does
                 " $(echo .char)ter/vaults/x.json", " .char ter/vaults/x.json",
                 " absvaults/db.json"]


def a_walk_case(rng: random.Random) -> str:
    """One generated command that really reaches the guarded-state walk."""
    return (f"{rng.choice(WALK_WHERE)}{rng.choice(WALK_PROGS)} TOKEN"
            f"{rng.choice(WALK_EXCLUDES)}{rng.choice(WALK_OPERANDS)}")


#: What breaks the tokenizer, so the RAW-STRING arm is reached at all. Measured: without these
#: the whole `not parsed` branch — the one that exists because a command could otherwise hide
#: behind a broken quote — saw `--reveal` six times in 20,000 cases.
READ_BREAKAGE = ["", "", "", "", " '", ' "', " \\", " 'it is", ' "quoted']


def a_reader_case(rng: random.Random) -> str:
    """One generated command that really puts an operand to a reader, a `gh` flag or charter."""
    return (f"{rng.choice(READ_PROGS)}{rng.choice(READ_BREAKAGE)}{rng.choice(READ_FLAGS)}"
            f"{rng.choice(READ_OPERANDS)}")


def a_case(rng: random.Random) -> str:
    """One generated command line.

    Four generators: a random join of FRAGMENTS — which is where the LEXER's answers live, and
    which stage 1 measured on — a well-formed heredoc, which is where the LAYOUT's answers live,
    and stage 3's two, which are where the GUARD's are. None reaches another's interesting cases
    on its own, and the shares are set by what each one is measured to reach (the table in the
    PR body).
    """
    r = rng.random()
    if r < 0.27:
        return a_walk_case(rng)
    if r < 0.54:
        return a_reader_case(rng)
    if r < 0.77:
        return a_heredoc_case(rng)
    return "".join(rng.choice(FRAGMENTS) for _ in range(rng.randint(1, 14)))


# --------------------------------------------------------------------------- #
# Stage 3's probe tables                                                        #
# --------------------------------------------------------------------------- #
#
# `normpath`, `join`, `realpath`, `fnmatch`, `names_a_vault_path` and `gh_at_path` are pure
# functions of one or two strings and reach the guard from inside it, so they are put questions
# of their own rather than only the ones a generated command happens to ask. The window ROTATES
# with the case's length so a 200,000-case run sweeps all of them without any case carrying the
# whole table: that keeps the answer's size flat and is deterministic on both sides.

#: Operands for `names_a_vault_path`, `normpath`, `realpath`, `join` and `gh_at_path`.
PROBE_OPERANDS = [
    VAULT, ".charter", ".charter/", ".charterx", ".edm/vaults", ".", "..", "/", "//", "///x",
    "sub", "docs", "here", "up", "tostate", "hop1", "dangling", "", "x", "a/b/../vaults",
    ".charter//vaults", ".charter/./vaults", ".charter/vaults/../..", ".CHARTER/VAULTS/x",
    ".charter/actİve-persona", ".charter/fıngerprint.key", "body=@notes.md",
    "body=@-", "body=x=@notes.md", "body", "=@x", "sub/../..", "/a/b/../..", "../..",
    "İ", " ", "x\n", ".charter/vaults\n", "-", "./",
    # The state directory at the END of the operand, with the trailing newline CPython's `$`
    # matches before and the `regex` crate's does not. Without these the `\n?$` on the LAST
    # alternative of `_VAULT_PATH_RE` was a rule no case could tell from its absence.
    ".charter\n", "x/.charter\n", ".edm\n",
    # ...and the absolute symlink, which is the only thing in the fixture that makes
    # `realpath`'s "an absolute target resets the path" rule do any work.
    "absvaults", "absvaults/db.json", "here/absvaults",
]

#: Patterns for `fnmatch` and `glob_selects_inside`. Every one is a shape `fnmatch._translate`
#: answers differently: an unclosed `[`, a `]` first in the class, an impossible range that
#: collapses to "matches nothing", the set-operation characters CPython escapes, and the
#: negated empty range.
PROBE_PATTERNS = [
    "*", "?", ".charter", ".char*", "[a-z]*", "[!a-z]", "[]]", "[]a]", "[b-a]", "[b-a-c]",
    "[a-]", "[-a]", "[^a]", "[[a]", "[", "[!]", "*.json", "**/x", "a**b", "[a\\-c]", "[&&]",
    "[|~]", "x[0-9]y", "*/*", "İ*", "[iı]", "**", "***", "[a-c-e]", "[!b-a]",
    "db.json", "*.py", "[\\]", "a[b", "[z-a]x", "[--0]", "[+--]",
]

#: Names for `fnmatch`, including the ones a class grammar gets wrong.
PROBE_NAMES = [
    "vaults", ".charter", "db.json", "x", "", "-", "]", "^", "[", "a-c", "b", "İ",
    "ı", "browser", "active-persona", "a\\b", "&", "|", "~", "0", "a.json", "prefs",
    "ab", "aXb", "+", ".", "*",
    # `fnmatch` is not `glob`: a `*` matches a `/` too. The guard's own callers only ever ask
    # about one path COMPONENT, so nothing else here reaches that rule.
    "a/b", "x/y/z",
]

#: The directories the command in each case is judged to run in. Chosen by the case's LENGTH so
#: both implementations pick the same one with nothing extra on the wire — the transport stays
#: one JSON string per line, exactly as stages 1 and 2 left it.
def probe_cwds() -> list[str]:
    return [str(ROOT), str(ROOT / "sub"), "", ".", str(ROOT / ".charter"), "sub"]


#: How many probes of each kind a case carries.
PROBE_PATHS = 4
PROBE_GLOBS = 3


def rotation(cmd: str, table: list, count: int) -> list:
    """`count` entries of `table`, rotated by the case's CHARACTER length.

    A character count, not a byte count: CPython's `len` is code points and this has to be the
    same number on both sides.
    """
    n = len(cmd)
    return [table[(n + k) % len(table)] for k in range(count)]


def probe_cwd(cmd: str) -> str:
    cwds = probe_cwds()
    return cwds[len(cmd) % len(cwds)]


def rel(p) -> str | None:
    """A path as the corpus can carry it: relative to the fixture root, or `<outside>`.

    The fixture lives in a temporary directory whose name is different on every machine, so an
    absolute answer could be neither recorded nor replayed. Both sides project identically, so
    this can lose resolution — an answer outside the root is only known to be outside — and it
    can never invent an agreement.
    """
    if p is None:
        return None
    p = str(p)
    root = str(ROOT)
    if p == root:
        return "."
    return p[len(root) + 1:] if p.startswith(root + "/") else "<outside>"


#: The wrappers whose option GRAMMAR `_wrapper_option` is asked about, plus one that is in no
#: table at all — the row that pins "a letter this grammar cannot place ends the walk UNPLACED".
PROBE_WRAPPERS = ("env", "sudo", "xargs", "stdbuf", "timeout", "doas", "exec", "nonesuch")

#: The spellings `_flag_name_value` is asked with. It is a pure function of (token, spellings)
#: and has no table of its own, so the harness supplies one that exercises both of its rules:
#: a glued SHORT form and a long `--flag=value`.
PROBE_SPELLINGS = ("-S", "--split-string", "-C", "--chdir", "-u")

#: How many words of a case the per-token probes are run over. Every one of them is O(1), and
#: the cap keeps a 200,000-case run's JSON from being dominated by them.
PROBE_TOKENS = 6

#: How many OPTION-shaped words `_wrapper_option`/`_flag_name_value` are asked about. They are
#: reached only from `_split_env_chdir`'s `nxt.startswith("-") and len(nxt) > 1` branch, so
#: probing an ordinary word tests a call the guard never makes — and, measured, sweeping every
#: word was 52% of the recorded corpus's bytes for no evidence at all.
PROBE_OPTIONS = 4


def shown(toks: list) -> list:
    return [[t.text, t.bare, t.start, t.end] for t in toks]


def header_of(line: str, at: int):
    h = hooks._heredoc_header(line, at)
    return None if h is None else [h[0], h[1], h[2], h[3]]


def opener_rows(line: str) -> list:
    """`_heredoc_openers`, as JSON: BOTH offsets and every group the regex captured.

    `m.end()` is here because nothing else in this harness reads it — it is visible in neither
    the plan nor the layout — and "the field nobody diffs" is the one real harness defect stage 1
    found, by mutation rather than by reading. The Rust hand-writes this pattern, so where it
    ENDS is a claim about where `finditer` resumes and what the next opener is.
    """
    return [[m.start(), m.end(), m.group("delim"), bool(m.group("dash")),
             bool(m.group("bs")), m.group("q") or None]
            for m in hooks._heredoc_openers(line)]


def plan_of(line: str):
    plan = hooks._heredoc_strip_plan(line)
    if plan is None:
        return None
    return [[delim, drop, None if head is None else list(head), ex]
            for delim, drop, head, ex in plan]


def pipelines_of(line: str):
    out = hooks._line_pipelines(line)
    if out is None:
        return None
    return [[argvs, ex, hcounts] for argvs, ex, hcounts in out]


def oracle(cmd: str) -> dict:
    """What the frozen Python charter says this command line is.

    **Every answer separately**, so a divergence is attributed rather than merely seen — the rule
    stage 1 set and the reason `split_punctuation`'s per-piece offsets were found to be compared
    by nothing. Stage 2 adds the heredoc layout and the wrapper/env split, and holds to it: a
    port that got `_heredoc_could_run` wrong and `_heredoc_layout` right by luck fails on `hcr`
    and says so.
    """
    try:
        toks = hooks._lex(cmd)
    except ValueError as err:
        lex: dict = {"err": str(err)}
        sp: dict = {"err": str(err)}
        che = None
        so = None
    else:
        lex = {"toks": shown(toks)}
        # Reported apart from `lex`, because the offsets a punctuation run's PIECES carry are
        # visible in neither the token list (which is pre-split) nor the segments (which are
        # text only) — and A7 is the caller that reads them. Measured: without this key, a port
        # that gave every piece of `);` the whole run's offsets diverged on nothing.
        split = hooks._split_punctuation(toks)
        sp = {"toks": shown(split)}
        che = hooks._compound_holds_executor(split)
        so = [[[t.text for t in seg], before] for seg, before in hooks._segments_of(split)]
    try:
        psp: dict = {"toks": shlex.split(cmd)}
    except ValueError as err:
        psp = {"err": str(err)}

    segments, parsed = hooks._segment_argv_parsed(cmd)
    openers = hooks._heredoc_openers(cmd)
    starts = [m.start() for m in openers]
    # Every `<<` in the source, quoted or not: `_heredoc_header` is asked about positions the
    # opener filter throws away too, and its answer there is what a KEPT body's end depends on.
    at_shift = [i for i in range(max(0, len(cmd) - 1)) if cmd[i:i + 2] == "<<"]
    words = [hooks._heredoc_opener_words(cmd, s) for s in starts]
    probe = cmd.split()[:PROBE_TOKENS]
    opts = [w for w in cmd.split() if w.startswith("-") and len(w) > 1][:PROBE_OPTIONS]

    return {
        "ub": hooks._unbacktick(cmd),
        "qm": "".join("1" if b else "0" for b in hooks._quote_map(cmd)),
        "sc": hooks._splice_continuations(cmd),
        "lex": lex,
        "sp": sp,
        "seg": [segments, parsed],
        # ---- stage 2: the heredoc layout
        "dac": hooks._desugar_ansi_c(cmd),
        "psp": psp,
        "sq": shlex.quote(cmd),
        "ho": opener_rows(cmd),
        "hh": [[i, header_of(cmd, i)] for i in at_shift],
        "lp": pipelines_of(cmd),
        "che": che,
        "so": so,
        "hsp": plan_of(cmd),
        "bh": sorted(hooks._brief_heredocs(cmd)),
        "cs": sorted(hooks._crowded_substitutions(cmd)),
        "ci": hooks._comment_index(cmd),
        "elc": hooks._ends_in_line_continuation(cmd),
        "pc": hooks._pipeline_continues(cmd),
        "lrt": hooks._line_runs_text(cmd),
        "how": [[s, w] for s, w in zip(starts, words)],
        "op": [[s, hooks._opener_program(w)] for s, w in zip(starts, words)],
        "ps": [[s, hooks._pipeline_slice(cmd, s)] for s in starts],
        "hcr": [[s, hooks._heredoc_could_run(cmd, s)] for s in starts],
        "hl": [list(row) for row in hooks._heredoc_layout(cmd)],
        "srh": hooks._strip_reader_heredocs(cmd),
        # ---- stage 2: the wrapper/env split, per segment of the parsed command
        "sec": [list(hooks._split_env_chdir(seg)) for seg in segments],
        "cms": [hooks._commit_message_on_stdin(seg) for seg in segments],
        "ghb": [hooks._gh_body_on_stdin(seg) for seg in segments],
        "rr": [hooks._redirect_reads(seg) for seg in segments],
        "gg": [list(hooks._git_globals(seg)) for seg in segments],
        "wo": [[base, tok, *hooks._wrapper_option(base, tok)]
               for base in PROBE_WRAPPERS for tok in opts],
        "fnv": [[tok, *hooks._flag_name_value(tok, PROBE_SPELLINGS)] for tok in opts],
        "ie": [[tok, hooks._is_executor(tok)] for tok in probe],
        # ---- stage 3: the leak guard
        **stage3(cmd),
    }


def stage3(cmd: str) -> dict:
    """What the frozen Python's leak guard says — `_leak_reason` and its own neighbourhood.

    The per-segment answers are taken over the segments of the STRIPPED command, because that is
    what `_leak_reason` walks: a body a reader swallows is not a segment of anything.
    """
    cwd = probe_cwd(cmd)
    ops = rotation(cmd, PROBE_OPERANDS, PROBE_PATHS)
    pats = rotation(cmd, PROBE_PATTERNS, PROBE_GLOBS)
    names = rotation(cmd, PROBE_NAMES, PROBE_GLOBS)
    entries = rotation(cmd, glob_entries(), 2)
    stripped = hooks._strip_reader_heredocs(cmd)
    segs, _parsed = hooks._segment_argv_parsed(stripped)
    per = []
    for seg in segs:
        prog, _env, args, _chdir, _reads = hooks._split_env_chdir(seg)
        operands = hooks._file_operands(prog, args)
        per.append([
            hooks._is_charter(prog, args),
            operands,
            hooks._spliced_operands(operands),
            hooks._gh_file_operands(args),
            hooks._excluded_names(prog, args),
            hooks._walks_directories(prog, args),
            rel(hooks._walks_into_guarded_state(prog, args, cwd)),
        ])
    return {
        # The probe INPUTS, recorded beside the answers. Stage 2 named "the per-answer keys are
        # a contract between three files, and nothing makes them agree" as the thing to attack;
        # this is the answer to it. The harness and the example each build the rotation from
        # their own copy of the tables, so a drift between them shows up HERE, by name, instead
        # of as a mysterious divergence in whatever the probes fed. And the replay test reads
        # its inputs from this key, so it holds no copy of the tables at all.
        "pr": [len(cmd) % len(probe_cwds()), ops, pats, names,
               len(cmd) % len(glob_entries())],
        "lr": hooks._leak_reason(cmd, cwd),
        "lacr": [list(row) for row in hooks._lines_a_command_could_run(cmd)],
        "gseg": per,
        "nvp": [hooks._names_a_vault_path(o) for o in ops] + [hooks._names_a_vault_path(cmd)],
        "iab": [o.startswith("/") for o in ops],
        "np": [os.path.normpath(o) for o in ops],
        "pj": [posixpath.join(a, b) for a in ops for b in ops],
        "rp": [rel(os.path.realpath(o)) for o in ops],
        "fnm": [[n, p, _fnmatch.fnmatch(n, p)] for n in names for p in pats],
        "gap": [hooks._gh_at_path(o) for o in ops],
        "gse": [_entries_of(d) for d in (STATE, RICH, MISSING)],
        "gsi": [hooks._glob_selects_inside(Path(e), p, lim)
                for e in entries for p in pats for lim in (0, 512)],
        "wigs0": rel(hooks._walk_into_guarded_state(cwd, ops, pats)),
    }


#: The entries `_glob_selects_inside` is put to: a non-empty guarded directory, a nested tree, a
#: guarded FILE, a file and a path that is not there. It is asked with a limit of 0 as well as
#: its own 512, because the bound is the only part of it that fails CLOSED and a fixture small
#: enough to record cannot reach 512 files.
def glob_entries() -> list[str]:
    return [str(STATE / "vaults"), str(RICH / "browser"), str(RICH / "active-persona"),
            str(STATE / "vaults" / "db.json"), str(MISSING), str(ROOT / "sub")]


def _entries_of(state_dir) -> list[str]:
    """The NAMES `_guarded_state_entries` returns for a directory that is not the configured one.

    The Python reads `config.STATE_DIR` where the Rust takes a parameter, so the harness moves
    that global for the length of one call. That is harness work on a module attribute;
    `hooks.py` itself is untouched, and the value is put back whatever happens.

    **Sorted, and that is a limit rather than a tidy-up.** `_guarded_state_entries` hands its
    caller the directory's own readdir order, which is a property of the filesystem and not of
    the guard; the fixture's configured state directory therefore holds exactly one guarded
    entry, so no other answer here depends on the order, and this one drops it.
    """
    from charter import config as cfg
    was = cfg.STATE_DIR
    try:
        cfg.STATE_DIR = str(state_dir)
        return sorted(Path(p).name for p in hooks._guarded_state_entries())
    finally:
        cfg.STATE_DIR = was


def ask_rust(binary: Path, cases: list[str]) -> list[dict]:
    """The same question, put to the Rust module."""
    payload = "".join(json.dumps(c) + "\n" for c in cases)
    # The core holds no globals, so the three fixture paths go over the environment and the
    # example passes them in. `cwd` is the fixture root for both sides: `realpath` resolves a
    # relative operand against the process's own directory, so the two have to stand in the same
    # place for that answer to mean anything.
    env = {**os.environ, "CHARTER_HOME": str(STATE),
           "SHELLSEG_FIXTURE_ROOT": str(ROOT), "SHELLSEG_FIXTURE_RICH": str(RICH),
           "SHELLSEG_FIXTURE_MISSING": str(MISSING)}
    run = subprocess.run(
        [str(binary)], input=payload, capture_output=True, text=True, check=False,
        env=env, cwd=str(ROOT),
    )
    if run.returncode != 0:
        sys.exit(f"{binary} exited {run.returncode}\n{run.stderr}")
    # `split("\n")`, NOT `splitlines()`. Python's `str.splitlines` also breaks on U+000B,
    # U+000C, U+001C-U+001F, U+0085, U+2028 and U+2029 — and `serde_json` escapes only the
    # first group, because the last three are ordinary characters inside a JSON string. Stage
    # 2 put U+0085/U+2028/U+2029 in the alphabet, and the reader promptly reported "answered
    # 18,510 of 10,000 cases". The transport is newline-delimited JSON; `\n` is the delimiter.
    lines = [ln for ln in run.stdout.split("\n") if ln]
    if len(lines) != len(cases):
        sys.exit(f"{binary} answered {len(lines)} of {len(cases)} cases")
    return [json.loads(ln) for ln in lines]


#: Every answer compared, in the order a reader of a failure wants them: the token layer first,
#: then the heredoc layout, then the wrapper split. **Adding a `pub` item to the Rust without
#: adding it here is the defect class stage 1 found by mutation** — `split_punctuation`'s
#: per-piece offsets were compared by nothing at all, which is why `sp` exists.
KEYS = (
    "ub", "qm", "sc", "lex", "sp", "seg",
    "dac", "psp", "sq", "ho", "hh", "lp", "che", "so", "hsp", "bh", "cs",
    "ci", "elc", "pc", "lrt", "how", "op", "ps", "hcr", "hl", "srh",
    "sec", "cms", "ghb", "rr", "gg", "wo", "fnv", "ie",
    # ---- stage 3. `gseg` carries the six per-segment answers plus the walk, in one list per
    # segment, because they are all asked of the same `_split_env_chdir` and a divergence in one
    # of them is a divergence about that segment.
    "pr", "lr", "lacr", "gseg", "nvp", "iab", "np", "pj", "rp", "fnm", "gap", "gse", "gsi",
    "wigs0",
)

#: The `pub` items with no key of their own, and why each is already covered. Kept as a LIST
#: rather than as prose so the next stage has to answer for anything it adds: a `pub` item that
#: is in neither `KEYS` nor here is a rule with no evidence, which is the defect stage 1 found
#: by mutation rather than by reading.
COVERED_ELSEWHERE = {
    # The three refusal texts are returned verbatim by `leak_reason`, so every denial in `lr`
    # compares them character for character — and `the_shell_is_read_the_way_python_reads_it.rs`
    # asserts each literal is really in the recorded corpus, so a corpus edit cannot quietly
    # take the evidence away.
    "leakguard::READ_REASON": "lr",
    "leakguard::REVEAL_REASON": "lr",
    "leakguard::WALK_FIX": "lr",
    # `walks_into_guarded_state` is the seventh column of `gseg`; `walk_into_guarded_state` is
    # `wigs0`, which puts it operands and exclusions the command did not have to supply.
    "leakguard::walks_into_guarded_state": "gseg",
}


#: The branches of `_leak_reason` a run has to REACH for its size to be evidence, and the
#: predicate that says it reached one. Stage 2 measured that a random fragment-join opened a
#: heredoc 15 times in 20,000 cases; `--coverage` is that measurement, kept, so the next stage
#: does not have to rediscover that a big number over uninteresting inputs proves nothing.
def reached(cmd: str) -> set[str]:
    hit: set[str] = set()
    cwd = probe_cwd(cmd)
    stripped = hooks._strip_reader_heredocs(cmd)
    if stripped != cmd:
        hit.add("heredoc-stripped")
    segs, parsed = hooks._segment_argv_parsed(stripped)
    if not parsed:
        hit.add("unparseable")
        if hooks._REVEAL_RE.search(stripped):
            hit.add("raw-reveal")
        elif hooks._names_a_vault_path(stripped):
            hit.add("raw-read")
    if hooks._leak_reason(cmd, cwd):
        hit.add("denied")
    for seg in segs:
        prog, _env, args, chdir, reads = hooks._split_env_chdir(seg)
        base = os.path.basename(prog).lower()
        if prog and base in hooks._CHDIR_BUILTINS:
            hit.add("chdir-builtin")
            continue
        if chdir:
            hit.add("wrapper-chdir")
        if reads:
            hit.add("redirect-reads")
            if any(hooks._names_a_vault_path(r) for r in reads):
                hit.add("redirect-reads-hit")
        if not prog:
            continue
        if hooks._is_charter(prog, args):
            hit.add("charter")
            if any(a == "--reveal" or a.startswith("--reveal=") for a in args):
                hit.add("reveal")
        if base in hooks._READERS:
            hit.add("reader")
            if any(hooks._names_a_vault_path(o)
                   for o in hooks._spliced_operands(hooks._file_operands(prog, args))):
                hit.add("reader-hit")
        if base == "gh":
            hit.add("gh")
            if hooks._gh_file_operands(args):
                hit.add("gh-file")
        if hooks._walks_directories(prog, args):
            hit.add("walker")
            if hooks._excluded_names(prog, args):
                hit.add("walker-excluded")
            if hooks._walks_into_guarded_state(prog, args, cwd) is not None:
                hit.add("walk-hit")
    return hit


COVERAGE_BRANCHES = ("heredoc-stripped", "unparseable", "raw-reveal", "raw-read",
                     "chdir-builtin", "wrapper-chdir", "redirect-reads", "redirect-reads-hit",
                     "charter", "reveal", "reader", "reader-hit", "gh", "gh-file", "walker",
                     "walker-excluded", "walk-hit", "denied")


def attribute(want: dict, got: dict) -> list[str]:
    """WHICH of the answers disagreed — a divergence is reported by name, not by blob."""
    return [k for k in KEYS if want.get(k) != got.get(k)]


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--cases", type=int, default=200_000)
    ap.add_argument("--seed", type=int, default=20260920)
    ap.add_argument("--binary", type=Path, default=DEFAULT_BINARY)
    ap.add_argument("--batch", type=int, default=10_000,
                    help="cases per subprocess call; the run's memory bound")
    ap.add_argument("--record", action="store_true", help="rewrite the curated corpus")
    ap.add_argument("--check", action="store_true", help="fail if the curated corpus moved")
    ap.add_argument("--coverage", action="store_true",
                    help="which of the guard's branches the generated cases REACH, and how "
                         "often — run this before believing a case count")
    args = ap.parse_args()
    args.binary = args.binary.resolve()
    # Both sides stand in the fixture root: `realpath` resolves a relative operand against the
    # PROCESS's directory, so an answer about `.` is only the same answer from the same place.
    # Every path this script holds was resolved at import.
    os.chdir(ROOT)

    if args.record or args.check:
        rows = [json.dumps({"cmd": c, **oracle(c)}, sort_keys=True) for c in CURATED]
        text = "".join(r + "\n" for r in rows)
        if args.record:
            CORPUS.parent.mkdir(parents=True, exist_ok=True)
            CORPUS.write_text(text, encoding="utf-8")
            print(f"wrote {len(rows)} cases to {CORPUS}")
            return 0
        if CORPUS.read_text(encoding="utf-8") != text:
            print(f"{CORPUS} is not what the oracle says; run --record", file=sys.stderr)
            return 1
        print(f"{CORPUS}: {len(rows)} cases, unchanged")
        return 0

    if args.coverage:
        rng = random.Random(args.seed)
        cases = CURATED + [a_case(rng) for _ in range(max(0, args.cases - len(CURATED)))]
        tally = dict.fromkeys(COVERAGE_BRANCHES, 0)
        for cmd in cases:
            for name in reached(cmd):
                tally[name] += 1
        print(f"{len(cases)} cases")
        for name in COVERAGE_BRANCHES:
            n = tally[name]
            print(f"  {name:20s} {n:8d}  {100.0 * n / len(cases):5.1f}%")
        return 0 if all(tally[b] for b in COVERAGE_BRANCHES) else 1

    if not args.binary.exists():
        sys.exit(
            f"{args.binary} is not built — "
            f"`cargo build -p charter-core --example shellseg_oracle`"
        )
    rng = random.Random(args.seed)
    cases = CURATED + [a_case(rng) for _ in range(max(0, args.cases - len(CURATED)))]

    bad = 0
    by_function: dict[str, int] = {}
    # In BATCHES, because stage 2's answer is about 1.5 kB and 200,000 of them is 300 MB of
    # stdout to hold at once — on a runner with other jobs on it. The batch is the whole run's
    # memory bound and changes no answer: each case is independent.
    for lo in range(0, len(cases), args.batch):
        chunk = cases[lo:lo + args.batch]
        for cmd, got in zip(chunk, ask_rust(args.binary, chunk), strict=True):
            want = oracle(cmd)
            if want == got:
                continue
            bad += 1
            for fn in attribute(want, got):
                by_function[fn] = by_function.get(fn, 0) + 1
            if bad <= 20:
                print(f"--- diverged on {cmd!r}")
                for fn in attribute(want, got):
                    print(f"    {fn}: python={want[fn]!r}")
                    print(f"    {fn}:   rust={got[fn]!r}")
    print(f"{len(cases)} cases, {bad} divergences")
    if bad:
        print(f"by function: {by_function}")
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
