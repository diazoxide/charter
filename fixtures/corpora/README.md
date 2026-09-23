# Recorded corpora

Real harness output, recorded off a pseudo-terminal, for benchmarks (spec §Limits, research
§9.3) and for tests that need output no synthetic generator would think of.

| File | What it is |
| --- | --- |
| `claude-code-session.raw` | Claude Code 2.1.274 answering "Print the numbers 1 to 2000, one per line", on a 150×42 terminal, macOS 26.2, 2026-09-17. 132 KB, ending mid-session where the recorder hung up. |
| `shellseg-oracle.jsonl` | Command lines the Python charter's `hooks.py` docstrings name as bypasses that SHIPPED (529 rows), each with what the frozen Python answers for every function of the shell reader, the heredoc layout, the wrapper split, the leak guard, A2, A4, A5/A6 and A7. Every filesystem answer in it is a path **relative to the fixture plane the replay builds**, never an absolute one, so the file is the same on every machine; the probe INPUTS ride along in the `pr` key, so the replay tests hold no copy of the recorder's tables. |
| `shellseg-generated.jsonl.gz` | The same answers for 2,400 command lines out of the retired fuzz's 200,000 seeded cases, chosen as described below. Same row shape. 17.7 MB as JSON Lines, 1.84 MB gzipped. |
| `planeroot-oracle.jsonl` | Every command line A3's and A3b's docstrings name as a bypass that shipped, and every defect filed from their port (202 rows): `{case, request, answer}`, where `request` carries every probe the recorder derived and every path is written against `@B@`, the fixture's base directory. |
| `planeroot-generated.jsonl.gz` | The same for 1,710 of the retired fuzz's 50,000 seeded cases, each recorded WITH the git layer on. 3.5 MB as JSON Lines, 0.32 MB gzipped. |
| `planeroot-fixture.json` | The git-repository fixture both plane-root corpora were answered against, as steps; the replay builds it again from these. |

## The oracle corpora are frozen

The four `shellseg-*` and `planeroot-*` corpora were recorded **once, on 2026-09-23, from the
Python charter pinned at 50d31dc** (`diazoxide/charter`), by the differential harnesses that
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

The vocabulary features exist because the branch cover alone was measured to be too coarse: a
`pipeline_continues` that forgot `|&` survived a subset chosen on guard branches, while the full
run reached a trailing `|&` 65 times. With the features, the replay fails on it.

**Changing an answer.** There is no oracle to ask any more, so an answer changes only on
purpose: edit the row (for a `.gz`, decompress, edit, and recompress with
`gzip -9n`), and say in the pull request which row, what it said, what it says now, and why the
app now disagrees with the Python it was ported from. Rows are never regenerated wholesale.

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
