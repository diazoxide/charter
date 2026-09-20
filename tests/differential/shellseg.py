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

Five functions are compared per case, not one, so a divergence is attributed rather than merely
seen: `_unbacktick`, `_quote_map`, `_splice_continuations`, `_lex` (tokens with their `bare` flag
and their CHARACTER offsets, or the `ValueError` it raises) and `_segment_argv_parsed` (the
segments and the parsed flag).

    ./shellseg.py                      # the default fuzz: 200,000 seeded cases
    ./shellseg.py --cases 2000         # fewer, while iterating
    ./shellseg.py --record             # rewrite the checked-in curated corpus
    ./shellseg.py --check              # ...and fail if it moved

The Rust side is `cargo run --example shellseg_oracle -p charter-core`, which reads a
JSON-encoded command line per line and writes a JSON answer per line. It is an example rather
than a binary because nothing ships it. Build it first, or pass `--binary`.

Python is a DEV DEPENDENCY OF THIS TEST and of nothing else (spec decision 15).

# The alphabet

Random bytes would test almost nothing: every interesting answer in this module lives on the
dozen characters a shell gives meaning to. So the cases are built by joining FRAGMENTS — the
quote and escape spellings, the operators longest and shortest, the redirections, a heredoc
opener, a comment, a newline, a backslash-newline, the programs the guard above this cares about
and a vault path — which is the same shape of alphabet the `secretshape` fuzz used.

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
]


def a_case(rng: random.Random) -> str:
    """One generated command line: a random join of FRAGMENTS, 1 to 14 of them."""
    return "".join(rng.choice(FRAGMENTS) for _ in range(rng.randint(1, 14)))


def shown(toks: list) -> list:
    return [[t.text, t.bare, t.start, t.end] for t in toks]


def oracle(cmd: str) -> dict:
    """What the frozen Python charter says this command line is."""
    try:
        toks = hooks._lex(cmd)
    except ValueError as err:
        lex: dict = {"err": str(err)}
        sp: dict = {"err": str(err)}
    else:
        lex = {"toks": shown(toks)}
        # Reported apart from `lex`, because the offsets a punctuation run's PIECES carry are
        # visible in neither the token list (which is pre-split) nor the segments (which are
        # text only) — and A7 is the caller that reads them. Measured: without this key, a port
        # that gave every piece of `);` the whole run's offsets diverged on nothing.
        sp = {"toks": shown(hooks._split_punctuation(toks))}
    segments, parsed = hooks._segment_argv_parsed(cmd)
    return {
        "ub": hooks._unbacktick(cmd),
        "qm": "".join("1" if b else "0" for b in hooks._quote_map(cmd)),
        "sc": hooks._splice_continuations(cmd),
        "lex": lex,
        "sp": sp,
        "seg": [segments, parsed],
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


def attribute(want: dict, got: dict) -> list[str]:
    """WHICH of the five functions disagreed — a divergence is reported by name, not by blob."""
    return [k for k in ("ub", "qm", "sc", "lex", "sp", "seg") if want[k] != got[k]]


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--cases", type=int, default=200_000)
    ap.add_argument("--seed", type=int, default=20260920)
    ap.add_argument("--binary", type=Path, default=DEFAULT_BINARY)
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
    answers = ask_rust(args.binary, cases)

    bad = 0
    by_function: dict[str, int] = {}
    for cmd, got in zip(cases, answers, strict=True):
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
