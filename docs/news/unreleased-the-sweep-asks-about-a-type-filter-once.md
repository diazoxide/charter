---
version: unreleased
headline: The sweep asks about a type filter once, and the row it keeps is the one that names the filter
---

The second of two rule pairs in `tools/sweep.py` that were planning one edit twice, and the
same answer as the first: neither rule is dropped, one half of each is kept.

## Two rules, one program, two rows (#797)

An `isinstance` call that **is** the whole `test` of an `if` with somewhere else for control
to go satisfies two rules at once. `disable-branch` forces the test both ways, because a
branch in a chain cannot be excised; `drop-isinstance` forces it one way, because that is
what removing a type filter means. When the test *is* the call, one of those three mutants
is a second copy of another — one span, one replacement, one program:

```
charter/hooks.py:6250   before='isinstance(node, dict)'
    disable-branch   after='True'   "is the rest of the chain pinned, or does nothing
                                     change when this condition always holds?"
    drop-isinstance  after='True'   "is the type filter pinned?"

tools/sweep.py:847      before='isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef))'
    disable-branch   after='True'
    drop-isinstance  after='True'
```

Both rows survive the plan's de-duplication, which is keyed on
`(line, col, operator, replacement)` — two operators, two keys — and both survive the
report guard added with #721, which asks that sibling mutants carry distinct questions,
tags and lines. These do. That is exactly the point: the two rows are told apart perfectly
well and they are still **one question asked twice**. The cost is an extra sandbox run at
each site and a second row in a report a person reads, where a reviewer has to work out
that two rows are one edit before deciding they are one finding.

## The row that stays is the one whose question is about this node

Not "drop a rule". `drop-isinstance` keeps the forced direction, because
*"is the type filter pinned?"* names what the mutant deleted, where *"is the rest of the
chain pinned…?"* is true of any condition whatsoever and tells a reviewer nothing they
could not read off the line. `disable-branch` stands down for that one direction and keeps
the opposite one, so the pair is still a pair:

```
charter/hooks.py:6250 [disable-branch]  isinstance(node, dict) — is this branch pinned, or
                                        does nothing change when its condition never holds?
charter/hooks.py:6250 [drop-isinstance] isinstance(node, dict) — is the type filter pinned?
```

Two rows on the line, one per direction, each with its own question, and no direction
asked twice. It is #755's trade made the same way — there the module-constant rule had the
better *question* and the non-decimal rule the better *spelling*, and the fix kept one half
of each — and the rule both rest on is `swap-synonym`'s, from #655: a mutation that
produces a program another mutation already produces is **not offered** rather than given a
verdict of its own.

**The exclusion is narrow and structural.** The two rules read one helper, so what
`disable-branch` declines is exactly what `drop-isinstance` produces and nothing more.
`if isinstance(x, str) and x:` is not this shape — the test is the `and`, `drop-isinstance`
never fires, and both branch directions are still asked. A bare `if isinstance(…)` with no
`else` is still the type filter rule's alone. `while isinstance(…)` is untouched, because
`disable-branch` does not reach a `while` at all.

**The negated spelling is fixed too, and the tree has no instance of it.** `not isinstance(…)`
is the same shape forced the other way, so its collision is on `False` rather than `True`.
Writing the fix from the two sites that were measured rather than from the shape would have
left half of it undone.

## Re-measured over the whole tree, not over the fixture

Every mutation planned for `charter/` and `tools/`, grouped by file and by
`ast.dump(ast.parse(mutant_source))` so that two mutants which normalise to one AST are one
program however they are spelled:

| | `main` @ `34a74e0` | this branch, same sources |
|---|---|---|
| mutations over 98 files | 11,829 | 11,827 |
| groups of two mutants of one node that are one program | **2** | **0** |

The two rows that go are the two `disable-branch -> True` rows above. Nothing else moves and
nothing is added.

The two roots are kept apart in that table for a reason: `tools/sweep.py` is itself one of
the swept sources, so running the changed rule over its own changed file would confound
"mutations the rule stopped planning" with "mutations the fix's new lines added". Over the
branch's own tree the count is 11,831 — two *more* than `main`, because the fix is four
mutable lines longer than the code it replaces — and still **0** colliding groups. Neither
of those is the number a pull request's sweep charges, which is the diff in `charter/` and
nothing else.

This is the third time in a row the tree-wide re-enumeration has been worth more than the
fixtures: it is what found #797 while #755 was being fixed, and #754's own fix had
re-introduced the very collision it was fixing because the fixture was written from the
shapes rather than from the tree. The general guard now carries both `isinstance` chains, in
both spellings — its only `isinstance` before was a bare `if`, where the two rules do not
overlap, which is how a defect of this class sat inside the test written to catch it.

## Every shard is re-dealt by this

`shard_of` deals the plan round-robin — `plan[index - 1::count]` — so a plan two mutations
shorter puts different mutations on different machines. Measured over the same sources:
the first plan position that differs is the 7,515th of 11,829, and over eight shards
**4,313 of the 11,827 remaining mutations land on a different shard**; the 7,514 before the
first removal stay where they were.

Nothing that was ever a distinct question is dropped and nothing is added; membership simply
moves. #754 was verified on the property that plan order does not move, which is why that
work and this are deliberately separate commits.
