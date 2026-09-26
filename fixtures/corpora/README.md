# Recorded corpora

Real harness output, recorded off a pseudo-terminal, for benchmarks (spec §Limits, research
§9.3) and for tests that need output no synthetic generator would think of.

| File | What it is |
| --- | --- |
| `claude-code-session.raw` | Claude Code 2.1.274 answering "Print the numbers 1 to 2000, one per line", on a 150×42 terminal, macOS 26.2, 2026-09-17. 132 KB, ending mid-session where the recorder hung up. |
| `shellseg-oracle.jsonl` | Command lines the Python charter's `hooks.py` docstrings name as bypasses that SHIPPED (529 rows), each with what the frozen Python answers for every function of the shell reader, the heredoc layout, the wrapper split, the leak guard, A2, A4, A5/A6 and A7. Every filesystem answer in it is a path **relative to the fixture plane the replay builds**, never an absolute one, so the file is the same on every machine; the probe INPUTS ride along in the `pr` key, so the replay tests hold no copy of the recorder's tables. |
| `shellseg-generated.jsonl.gz` | The same answers for 2,399 command lines out of the retired fuzz's 200,000 seeded cases, chosen as described below. Same row shape. 17.7 MB as JSON Lines, 1.84 MB gzipped. |
| `planeroot-oracle.jsonl` | Every command line A3's and A3b's docstrings name as a bypass that shipped, and every defect filed from their port (202 rows): `{case, request, answer}`, where `request` carries every probe the recorder derived and every path is written against `@B@`, the fixture's base directory. |
| `planeroot-generated.jsonl.gz` | The same for 1,710 of the retired fuzz's 50,000 seeded cases, each recorded WITH the git layer on. 3.5 MB as JSON Lines, 0.32 MB gzipped. |
| `planeroot-fixture.json` | The git-repository fixture both plane-root corpora were answered against, as steps; the replay builds it again from these. |

## The oracle corpora are frozen

The four `shellseg-*` and `planeroot-*` corpora were recorded **once, on 2026-09-23, from the
Python charter pinned at 50d31dc** (`diazoxide/charter-plane`), by the differential harnesses that
used to live in `tests/differential/` (`shellseg.py`, `planeroot.py`). Those harnesses are
retired: charter-app is standalone, and nothing in its code or CI installs or asks the Python
charter any more. The recordings are the app's contract from that day on. They are replayed with
no Python present by `crates/charter-core/tests/the_shell_is_read_the_way_python_reads_it.rs`,
`…/the_leak_guard_answers_what_the_python_answers.rs`,
`…/the_golden_rule_and_the_prose_guards_answer_what_the_python_answers.rs`,
`…/the_handoff_guard_answers_what_the_python_answers.rs` and
`…/the_plane_root_guards_answer_what_the_python_answers.rs`; a failure names the row as
`file:line` (1-based, of the decompressed file for a `.gz`).

**How the generated subsets were chosen.** The recorder generated exactly the cases the fuzz
generated (`shellseg.py`: 200,000 from seed 20260920, 199,471 of them generated; `planeroot.py`:
50,000 from seed 20260922, 49,798 generated) and measured, for every generated case, the
branches the harness's own `reached()` reports — 47 guard branches for the shell reader, 43
branch names for the plane-root guards. For the shell reader it also measured the reader's
vocabulary, which `reached()` does not: every bare operator token the lexer produced, the one a
line ends on, the lexer's error, and which per-line predicates answered true (57 features). It
then took a greedy k-cover — repeatedly the case covering the most still-under-covered features,
until every feature the full run reached was reached by min(20, n) chosen cases (min(5, n) for a
reader-vocabulary feature) — and added a fixed seeded random sample on top (seed 20260923: 2,000
shell cases, 1,500 plane-root cases), dropping duplicates. Every branch and feature the full run
reached, the subset reaches. The files are gzip with a zero timestamp, so re-recording wrote them
byte for byte the same.

**One chosen case was taken out again**, because its answer belongs to the machine and not to
the guard: `sudo --chdir=/tmp grep --recursive TOKEN --exclude-dir='[b-a]' .` walks `/tmp`, and
whether that walk contains the fixture plane's `vaults` depends on where the machine keeps its
temporary directories. On macOS that is `/var/folders/…`, so the recording answered "allowed".
On Linux it is `/tmp`, so the replay refused. The live differential ran both sides on one machine
and never saw it. Replaying the corpora on Linux and on macOS shows every other row answering
the same on both.

The vocabulary features exist because the branch cover alone was measured to be too coarse: a
`pipeline_continues` that forgot `|&` survived a subset chosen on guard branches, while the full
run reached a trailing `|&` 65 times. With the features, the replay fails on it.

**Changing an answer.** There is no oracle to ask any more, so an answer changes only on
purpose: edit the row (for a `.gz`, decompress, edit, and recompress with
`gzip -9n`), and say in the pull request which row, what it said, what it says now, and why the
app now disagrees with the Python it was ported from. Rows are never regenerated wholesale.

**Answers changed on purpose so far.** #345, #346 and #347 fixed guard bypasses the frozen
Python had, so the rows that recorded its allow now record the refusal. In the plane-root rows:
a `cd` not joined by `&&` (after `;`, `||`, `&`, a newline, or in a pipeline or subshell), a `cd`
whose destination the guard cannot name (`$VAR`, a glob, `cd -`) or reads as the shell does (`~`,
a logical `..`), a wrapper's chdir flag, a `git` in capitals or split by quotes (`g''it`), an
inline alias used in a different case, a re-cased root, and a working directory inside the root's
repository. In the prose-guard rows: a `gh`, `glab` or `charter` in capitals. The re-cased rows
were recorded on a filesystem that folds case; on one that does not, the replay links
`@B@/PLANE` to the root so the recorded answer holds on both. The keys that moved are the verdicts (`bra`,
`rst`, `fsh`, `csh`) and what the walk found on the way (`prg`, `rga`, `cb`, `gt`).

**And in the shell-reader corpora (#348, #350, #351):**

- The release floor (`rfr`): a `git tag` read clears only its own segment (#348).
- The leak guard (`lr` and `gseg`): an `rg` glob without `!` is an inclusion (#350), and a
  script or pattern read with `-f`/`--file` is a file operand (#351). With those, the leak guard
  reads the options of `grep`, `rg`, `ag`, `sed` and `awk` the way getopt does: values after
  `=`, in clusters and after long-name prefixes; `--` ending the options; and the other flags
  that take a value.

**And where the shell reader learned more of bash's quoting.** The frozen Python read words with
`shlex`, which knows neither ANSI-C quoting (`$'…'`) nor bash's `$"…"`, and which keeps a
backslash-newline as part of the next word. The reader now reads all three as bash does, so
169 shell-reader rows record what the shell makes of those lines (`shellseg-oracle.jsonl` rows
42, 45, 46, 81, 102, 170, 410, 411, 418, 496, 501 and 502, and 157 generated rows). Every one of
them holds a `$'…'`, a backslash-newline, or (one row) `${` before a blank; none changed from
refused to allowed. The keys that moved are the reader's own (`lex`, `sp`, `seg`, `so`, `sec`,
`gg`, `lp`, `che`, `cms`, `ghb`, `rr`, `ee`, `dac`, `a7seg`, `hs7`, `gseg`, `s4seg`), the heredoc
readings that follow from them (`hh`, `hcr`, `bh`, `hsp`, `dqs`, `hb`, `hsub`), and the verdicts
of the leak guard and A7 (`lr`, `ih`, `hl7`, `hr`, `hrd`, `ssh7`), each now a refusal where the
Python allowed.

**And where a heredoc has one reading (#359).** The frozen Python found heredoc openers with a
pattern (`_HEREDOC_RE`) that knew only a delimiter spelled as one identifier in one pair of
quotes, and ended bodies with bash's header reading; the two disagreed on some lines. The
openers now come from the header reading alone, so `ho` records each opener as
`[start, [delim, expands, dash, end]]` instead of the pattern's match. That shape change is the
only change on 100 rows of `shellseg-oracle.jsonl` and 403 generated rows. The other rows moved
because the header reading sees `<<''`, `<<""`, `<<'A B'`, `<<EO'F'`, `<<\EOF` and `<<$'…'` as
the heredocs they are, and no heredoc inside a here-string's `<<<`:
`shellseg-oracle.jsonl` rows 84, 85, 86, 88, 89, 93, 98 and 403 to 407, and 227 generated rows.
Those keys are the openers and what is asked of each (`how`, `op`, `ps`, `hcr`), the header at
a `<<` (`hh`), the strip plan and layout (`hsp`, `bh`, `hl`, `srh`, `gseg`, `lacr`), and A5's
body walk (`hb`), where an expanding body no longer ends on a line spliced onto the one before
it. No leak-guard (`lr`), A5 or A6 verdict moved. A7 (`hr`, `hrd`, `hl7`) moved on 16 generated
rows:

- rows 957, 1356, 1640, 1740 and 1747 now refuse where the Python allowed, and rows 95, 793,
  1111 and 1968 refuse for the shell-string reason instead of the brief-source one. Each opens a
  heredoc inside a `$( … )` that closes on the same line. GNU bash 3.2.57 runs the lines after
  it as commands, and so does zsh 5.9 on all of them but row 1356; the `charter handoff` among
  them ran. GNU bash 5.2.15 and 5.3 read those lines as the body instead.
- rows 453, 1593, 1787 and 1822 still refuse, for the brief-source reason instead of the
  shell-string one. They hold only a here-string `<<<EOF`, in which the Python's pattern saw a
  heredoc, so the lines after them are commands.
- rows 177, 1606 and 2007 now allow what the Python refused. Their `charter handoff` line is
  inside the body of a `<<""` or `<<EO'F'` heredoc that runs to the end of the input, and it ran
  in none of GNU bash 3.2.57, 5.2.15 and 5.3 and zsh 5.9. The Python refused it only because
  its pattern did not see that heredoc. Rows 177 and 1606 are still refused by the leak guard.

## The session recording

Re-record with:

```bash
python3 tools/record-corpus.py fixtures/corpora/claude-code-session.raw \
  "Print the numbers 1 to 2000, one per line, with no commentary. Do not stop early."
```

Run it in a neutral directory (`/private/tmp/…`), never inside a plane or a home directory:
the harness prints its working directory, and the recording is public.

**Check every new recording before committing it.** A harness prints whatever it was shown.
Grep for usernames, hostnames, tokens and e-mail addresses first.

**This recording contains no `?2026` synchronized-output blocks.** That version of Claude Code
did not use them for this answer, so the synthetic generator in `fake-harness` remains the way
that path is exercised.

For bigger loads, replay it repeatedly rather than recording a huge file:
`fake-harness --corpus fixtures/corpora/claude-code-session.raw --loops 16`.
