#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = [
#   "charter-cp @ git+https://github.com/diazoxide/charter@50d31dc66835592ccea625bab8f5a0da313f2444",
# ]
# ///
"""Differential test: does `charter-core::shellseg` read a command line the way Python does?

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

**Every `pub` item of the three Rust modules is in `KEYS`.** Stage 1's one real harness defect
was a field nobody diffed — `_split_punctuation`'s per-piece offsets — and it was found by
mutation rather than by reading, so the rule is now written down: a `pub` item with no key is a
rule with no evidence.

    ./shellseg.py                      # the default fuzz: 200,000 seeded cases
    ./shellseg.py --cases 2000         # fewer, while iterating
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
import json
import random
import subprocess
import sys
from pathlib import Path

from charter import hooks

HERE = Path(__file__).resolve().parent
REPO = HERE.parent.parent
CORPUS = REPO / "fixtures" / "corpora" / "shellseg-oracle.jsonl"
DEFAULT_BINARY = REPO / "target" / "debug" / "examples" / "shellseg_oracle"

VAULT = ".charter/vaults/x.json"

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


def a_case(rng: random.Random) -> str:
    """One generated command line.

    Two generators, half each: a random join of FRAGMENTS — which is where the LEXER's answers
    live, and which stage 1 measured on — and a well-formed heredoc, which is where the LAYOUT's
    answers live. Neither reaches the other's interesting cases on its own.
    """
    if rng.random() < 0.5:
        return a_heredoc_case(rng)
    return "".join(rng.choice(FRAGMENTS) for _ in range(rng.randint(1, 14)))


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
    """`_heredoc_openers`, as JSON: the offset and every group the regex captured."""
    return [[m.start(), m.group("delim"), bool(m.group("dash")),
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
        psp: dict = {"toks": __import__("shlex").split(cmd)}
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
        "sq": __import__("shlex").quote(cmd),
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
    }


def ask_rust(binary: Path, cases: list[str]) -> list[dict]:
    """The same question, put to the Rust module."""
    payload = "".join(json.dumps(c) + "\n" for c in cases)
    run = subprocess.run(
        [str(binary)], input=payload, capture_output=True, text=True, check=False
    )
    if run.returncode != 0:
        sys.exit(f"{binary} exited {run.returncode}\n{run.stderr}")
    lines = [ln for ln in run.stdout.splitlines() if ln]
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
)


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
    args = ap.parse_args()

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
