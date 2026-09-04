---
version: unreleased
headline: the persona column stops re-reading the whole dispatch log on every repaint — a month file is now parsed once per version of that file, so the cost stops growing with the age of the plane
---

**The measurement came first and the cache came second, a release apart.** #882 made the
persona switcher sort by use, which put `dispatch.tally()` on a path that repaints every
turn. The implementer measured what that cost, found it growing with nothing an operator
does, and filed it rather than fixing it — a cache invented inside an ordering change is a
cache nobody asked for. This is that issue.

## A cost that grows with the age of the plane

`personas/_dispatch/` holds one jsonl file per month per host, and it only ever grows: a
month file that is not the current one is closed forever, because the writer only ever
opens `<this month>.<host>.jsonl`. `tally()` read **all of them**, every call, and parsed
every line.

`persona.by_use` calls it for the switcher's order, `statusline._persona_chip_cells` draws
that column, and a frame panel is a held process that repaints it for as long as the frame
lives. So the reading was monotonic in **how long the plane has existed** and in nothing
anybody did. Re-measured on charter's own plane, at its ~225 dispatches a month, against
synthesized stores of that shape:

| log age | rows | before | after a first read |
|---|---|---|---|
| today | 450 | 0.37 ms | 0.06 ms |
| 1 year | 2 700 | 2.06 ms | 0.26 ms |
| 2 years | 5 400 | 4.14 ms | 0.52 ms |
| 5 years | 13 500 | 11.12 ms | 1.27 ms |

Nothing was slow today. The point is the column on the left: it degrades silently, and
only on the planes that have been used longest — the ones whose operators would least
expect it.

## The key is the invalidation, not a summary

`dispatch._rows_of` memoises one file's parsed rows on `(path, mtime, size)`. It stores
**nothing derived** — not a count, not an order, not a total — only the rows the jsonl
already holds, discarded the instant the file they came from is not the file that was
read. That is what keeps it from becoming a second answer to a question the log already
answers, and it is the same shape `statusline._usage_stamp` and the frame's gather cache
already invalidate on.

**Size is in the key because mtime alone is a filesystem's promise rather than a fact.**
APFS keeps nanoseconds; some ext4 configurations report whole seconds, and a dispatch is
appended to the current month many times inside one such tick. The test that holds this
forges the mtime back to its old value with `os.utime`, changes the file's length, and
demands the new tally — then asserts the forgery actually took, so it cannot pass because
the filesystem quietly refused to hold the value the case is about.

**And the stat is taken before the read, which is the direction that errs safe.** A row
appended between the two is memoised under the older stamp and re-read on the next call.
Taken after, that same row would be memoised under the newer stamp and stay invisible for
the whole life of the process.

## What was deliberately not done

Nothing is cached in the caller, and no ordering is cached. #882's tie-break is a second
pass over runs of tied dispatch counts rather than a term in one sort key, precisely so
the per-persona memory glob is paid only by names that tie — on charter's own plane,
never. Caching the output of that would have replaced a structure that is already free
with one that has to be invalidated.

The tally itself is unchanged: the same rows, the same counts, the same file format.
There is nothing to adopt.
