# Recorded corpora

Real harness output, recorded off a pseudo-terminal, for benchmarks (spec §Limits, research
§9.3) and for tests that need output no synthetic generator would think of.

| File | What it is |
| --- | --- |
| `claude-code-session.raw` | Claude Code 2.1.274 answering "Print the numbers 1 to 2000, one per line", on a 150×42 terminal, macOS 26.2, 2026-09-17. 132 KB, ending mid-session where the recorder hung up. |
| `shellseg-oracle.jsonl` | Command lines the Python charter's `hooks.py` docstrings name as bypasses that SHIPPED, each with what the frozen Python answers for six functions. Recorded, not written: `tests/differential/shellseg.py --record` produced it and `--check` fails if it stops matching the oracle. Replayed offline by `crates/charter-core/tests/the_shell_is_read_the_way_python_reads_it.rs`, so the plain `cargo test` job holds the line with no Python present. |

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
