---
version: unreleased
headline: The deletion sweep applies the edit it printed, and stops asking a question that has one answer
---

#655 proposed a third verdict for the deletion sweep. Beside `pinned` and `survived` it
wanted **`indistinguishable`** — for the mutant no honest test can redden, produced by a
rule the tool can check rather than by a suppression list a person maintains. Its author
was careful to say it was a proposal, and careful to name the measurement that should come
first:

> across the last N branches, how many survivors are decidably-equivalent by a rule, versus
> how many are the genuine "the test asserts too little" case the report is designed to
> surface? If the answer is "almost all are the genuine case", the right fix is nothing at
> all.

**The measurement was run, and the answer is "almost all are the genuine case".** So there
is no third verdict. What the measurement did turn up is a defect in the tool that would
have made `--enforce` unusable on its first day, and that is what this changes.

## The measurement

Every sweep result GitHub still holds an artifact for was collected: 136 shard files from
54 completed runs across 2026-08-28 and 2026-08-30, on ten branches that produced a
survivor. That is 3,044 mutation records, 1,285 distinct mutations, and **461 distinct
survivors.**

| operator | survivors | | operator | survivors |
|---|---:|---|---|---:|
| `uncontain` | 109 | | `shift-boundary` | 25 |
| `drop-if` | 70 | | `drop-conjunct` | 15 |
| `retune-string` | 62 | | `retune-constant` | 10 |
| `swap-synonym` | 51 | | `drop-isinstance` | 7 |
| `no-fallback` | 45 | | `drop-comprehension-if` | 5 |
| `collapse-ifexp` | 33 | | `unclamp` | 3 |
| `narrow-except` | 26 | | | |

Of those 461, the number a **rule** could have called equivalent is **one**. Not a small
fraction — one line, `change.py:142`.

And the three cases #655 names are not among the decidable ones:

**`path.partition("/")` -> `rpartition("/")`** (eight survivors) is equivalent only if a
GitHub `path_with_namespace` really does hold exactly one slash. Nothing in
`charter/forge/github.py` constrains it — the value arrives as
`repo.get("path_with_namespace") or ""` — so the constraint lives in a remote API's
contract, not in the code. A tool that asserts it is a suppression list with a rule's
manners, and it is exactly the thing #370 refuses: an exception nothing re-checks when the
code moves.

**`GIT_TIMEOUT = 20 -> 21`** would have to be decided from the evidence the sweep already
collects — "N modules execute this file and not one names the symbol". But that sentence is
the report's *loudest finding*, not an equivalence: turning it into a reason to stop asking
inverts the tool, and it would be lifted by renaming the constant.

**`_BODY_BUDGET = 100_000 -> 100_001`** is the one offered as the sharpest case, against
`RELEASE_BODY_MAX = 125_000` beside it, which is pinned. The two really are different, and
the difference is not decidability: GitHub's number is pinned by a test that names GitHub's
number, and charter's is pinned from both ends by a *window* — `test_the_staged_release_has_headroom`
from above and `test_the_bound_is_an_exception_rather_than_the_normal_path` from below —
which `100_001` sits inside. No rule can compute that window. A reader can, and
`_BODY_BUDGET`'s own docstring already does, which is the remedy that scales: a comment,
at the line, saying why.

So: **no new verdict, no suppression list, and the report keeps saying what it says.**

## What the measurement found instead: the mutant was not always the edit

While counting survivors, one shape kept reading oddly. `charter/commands.py:975` was
reported like this:

```
shipped : (pattern or "").strip
mutant  : pattern or "".lstrip
```

The question printed beside it is "is `how much` pinned?". The mutation applied does not
move how much is stripped — it **deletes the strip on every path but one**, because
`ast` node positions exclude the parentheses a programmer wrote around a subexpression, so
`sp.text` of the receiver is `pattern or ""` and splicing it back rebuilds the expression.

Measured over `charter/` and `tools/`: **144 of 8,903 expression mutations spliced
something other than the edit they described.** 137 are that receiver, one spelling or
another. The remaining seven are worse:

```python
(vc or {}).get("config", {})    ->    vc or {}["config"]
```

`{}["config"]` is a `KeyError`. So the suite goes red, and the fallback is recorded as
**pinned** — a guard certified as tested by a crash that has nothing to do with it. That is
the false pin the whole file is built to prevent, four times in `charter/`, plus two
`unclamp` sites and one `uncontain`.

This is the failure `--enforce` could not have survived. A blocked branch whose author is
sent to write a test for a property the mutation never perturbed has no move to make, which
is the same dead end #632 closed for the unkillable annotation — arriving from the other
direction.

**Fixed as a property, not as a list of sites.** `parenthesised` spells any replacement
whose top node binds loosely enough to re-associate, and `LOOSE`/`TIGHT` between them name
every subclass of `ast.expr` — asserted in the suite, so the day Python grows a new
expression the tests say so rather than the tool guessing. (They said so immediately: on
3.14, PEP 750's `TemplateStr` and `Interpolation` are two nodes that did not exist when
this was written.) Re-measured after the change: **0 of 9,189.**

It also *recovers* questions the sweep had been dropping in silence. A receiver spread
over two lines — an implicitly concatenated pair of f-strings, say — produced a mutant
that did not parse, and `mutations_for` skips those without a word. Brackets make them
parse: **25 mutations across the tree are asked for the first time.**

## And one question that only ever had one answer

`SYNONYMS` justifies each pair by the single axis it moves. `split`/`rsplit` moves nothing
when no `maxsplit` is given: `s.split(sep)` and `s.rsplit(sep)` return the same list for
every string and every separator, because the only thing the `r` decides is which end runs
out of splits first and with an unlimited budget neither does. That is verified
exhaustively in the suite over every string of up to five characters from an alphabet
containing the separator, for `str` and `bytes` and the no-argument whitespace form.

67 of the 98 `split`/`rsplit` call sites in `charter/` and `tools/` pass no `maxsplit`. Each
was a mutation that could never be killed by anybody — #630's own self-sweep already listed
`split(".")[-1]` against `rsplit(".")[-1]` among the sixteen it said nobody could close —
and every one becomes a *permanent* survivor now that the receiver's brackets survive the
splice, because until now the lost parentheses were accidentally making the mutant a
different program.

**It is answered where #632 answered its sibling: the mutation is not offered.** A survivor
no test can kill is a false positive, and the place to fix a false positive is the question,
not the answer. Not a verdict, not an entry on a list — the operator declines, in one
predicate, with the reason in its docstring where a reader can disagree with it. The 31
sites that *do* pass a `maxsplit` are untouched and stay a real question.

Together the two changes move the whole-tree count from **11,277 to 11,235**: 67
withdrawn because they had one answer, 25 recovered because they now parse.
`swap-synonym` itself goes from 975 to 909 — 67 refused, one of them handed back.

The gate still blocks nothing. `--enforce` is still absent, deliberately.
