# #355 — `set()` asks 1Password the same question three times

## The premise held

Traced on `0.50.1` with a recording runner before writing a line:

```
set()    ->  0: item get --reveal   1: item get   2: item get   3: item edit   4: read
delete() ->  0: item get --reveal   1: item get   2: item edit
keys()   ->  0: item get
```

Three `op item get` calls in `set()`, two in `delete()`, all fetching the same document.
`_existing_ids` and `_item_present` had one caller each, both inside the module, and no
test named either. The issue's account was accurate in every particular.

## What each question actually asked, and whether it still needs its own read

| # | Helper | Question | Reveal? | Reason it was separate | Does the reason still hold? |
| --- | --- | --- | --- | --- | --- |
| 0 | `_fields(reveal=True)` | what are the values? | yes | values must be revealed or the round-trip writes masks | **Yes** — and this is the read that survives |
| 1 | `_item_present()` | does the item exist? | no | `_fields`' `{}` means both *absent* and *present-with-no-fields* | **The distinction holds; the extra read does not** |
| 2 | `_existing_ids()` | what are the field ids? | no | stricter failure policy: it ran *after* presence was established, so no exit code was a legitimate absence | **No** — the policy existed only because it was a *second* read |

Call 0 fetches strictly more than 1 and 2 need — it is the same `op item get` plus
`--reveal`, and ids are metadata identical with or without it. So the revealed document is
a superset, and it is the right answer to keep.

Two reasons were real and are preserved rather than collapsed away:

- **`--reveal` still belongs to the write path alone.** `_document(reveal=...)` keeps the
  parameter, off by default. `keys()`/`health()` run from `vault list` and `doctor`;
  revealing there would pull every secret in the vault into memory on a listing.
  Collapsing onto the *unrevealed* document would have made all three answers agree by
  caching the one that overwrites every sibling secret with a mask — worse than the bug,
  and the thing this change had to be tested hardest against.
- **`None` vs `{}` is load-bearing.** `_document` returns `dict | None`, `None` meaning
  *proven absent*, and callers test `is None`. `_fields_of(doc)` returning `{}` no longer
  carries a presence claim at all.

## The change

`charter/secrets/onepassword.py`

- **new** `_document(reveal=False) -> dict | None` — the `op item get`, the #352
  prove-absence-by-listing logic, and the JSON parse. The only `op item get` on any path.
- **new** `_fields_of(doc)` / `_ids_of(doc)` — pure readings of one document. Each keeps
  its *original* extraction rule verbatim, including the places they differ (`_ids_of`
  keeps a field that has an id but no `value`; `_fields_of` skips it and raises on
  duplicate labels). Narrowing them to match would have been a silent behaviour change
  dressed as tidying.
- `_fields(reveal=False)` is now `_fields_of(_document(reveal))`, for the read path.
- `set`/`delete` fetch once and pass `ids=` into `_write`, which no longer fetches.
- **removed** `_item_present`, `_existing_ids`.

`set()` now makes 3 `op` calls instead of 5, `delete()` 2 instead of 3, `keys()` unchanged.

## Two disagreements the split reads made reachable

Neither needs a failure. Every `op` call exits 0.

1. **Ids keyed by a different label than the values.** A field renamed in the 1Password UI
   mid-write: fetch 0 sees `{id: <adopted>, label: OLD}`, fetch 2 sees the same id under
   `NEW`. `_existing_ids` keys by `NEW`, the values by `OLD`, so charter finds no id for
   the field it is writing and mints a fresh one — **renumbering a field on an item it does
   not own**, which is precisely the #354 mutation, reached with no rate limit and no
   failed call.
2. **Presence contradicting absence.** Reproduced against the pre-fix code:

   ```
   set() returned: None
   calls: ['item get', 'item list', 'item get', 'item get', 'item edit', 'read']
   item now holds labels: ['NEW_KEY']       # OTHER_SECRET is gone
   ```

   The item was proven absent at call 0; another writer created it with a secret in it;
   call 1 answered *present*, `_write` flipped to `item edit`, and an edit **replaces** —
   the other writer's credential was destroyed and `set()` returned success. With one read
   charter creates instead, which against a title now taken fails loudly on the read-back.
   That window is not new and is not widened: the same race sits between any presence check
   and the `op item create` after it. What the single read removes is *silent destruction
   reported as success*.

## One hazard the collapse introduced, and closed

`None` now means *proven absent*, so a **successful** `op item get` whose body parses to
`null` would have returned the sentinel and sent `set()` down `op item create` against a
title the vault already holds — the duplicate-item outcome #354 hardened against, arriving
through a read that worked. Before the collapse `None` had no meaning here and such a body
raised `AttributeError`: ugly, but loud. `_document` now refuses any body that is not a
JSON object, without quoting it (op's stdout on this path *is* the item). Four tests and
five mutations cover it.

## Tests

- **new** `tests/test_op_reads_the_item_once.py` (23 tests). Its fake is the first to model
  two things: that `op` answers `item get` differently with and without `--reveal`, and
  that the document can change between two of charter's reads with nothing failing.
- **rewritten** `tests/test_op_unreadable_item_is_not_renumbered.py` (#354). Its own
  docstring said *"if this ever drops to one (#355), the `fail_get_from` indices below stop
  meaning what their comments say and must be revisited"*. It dropped to one, so they were
  revisited, not deleted: every index moved to 0, `assertFirstReadSucceeded` became
  `assertTheOnlyReadFailed`/`assertTheOnlyReadSucceeded`, and the three #354 swallows are
  accounted for individually — (a) unreachable by construction and pinned structurally,
  (b) the parse and (c) the presence answer still live and still tested.

Six tests were written first and watched fail against `main` for the intended reasons:
3 vs 1 `item get` in `set()`, 2 vs 1 in `delete()`, `[True, False, False]` vs `[True]` for
`--reveal`, the adopted id written as `PROD_KUBECONFIG` instead of `27r3gph…`, and the two
presence-race assertions.

### Two tests that could have passed for the wrong reason, caught before commit

- `test_delete_raises_too` and `test_delete_raises_rather_than_renumbering` assert
  `assertRaises(VaultError)`. `SecretNotFound` **is** a `VaultError`, and a swallowed read
  hands `delete` an empty field set — so the bare assertion would have been satisfied by
  exactly the swallow it exists to forbid. Both now assert
  `assertNotIsInstance(..., SecretNotFound)`. Mutations M7 and M8 do not catch them without
  it.
- `test_an_item_that_exists_with_no_fields_is_edited` failed under its mutation as an
  *error* from the read-back rather than on the assertion naming the defect. The subcommand
  assertion now runs first.

### Mutation testing — 20 mutations, 42 tests, 0 survivors

Every mutation was applied, run with `__pycache__` cleared, restored, and the restore
verified byte-for-byte. Baseline OK, every mutation FAILED, restored OK. Every one of the
42 tests in the two files is killed by at least one.

| | Mutation | Kills |
| --- | --- | --- |
| M1 | `_write` fetches the ids again | the call-count tests, the rename race |
| M2 / M2b | `set()` / `delete()` collapse onto the **unrevealed** document | the mask tests — *the "worse than the bug" guard* |
| M3 | the shared read always reveals | `keys()`/`health()` never-reveal |
| M4 | presence by truthiness, not `is None` | present-but-empty is edited |
| M5 | proven absence returns `{}` not `None` | the create/edit choice, 6 tests |
| M6 | the ids are dropped | adoption non-destructiveness, 4 tests |
| M7 | the parse failure is swallowed | 3 tests |
| M8 | absence assumed rather than proven | 22 tests, incl. #322/#352's |
| M9 | the concurrent-writer hook never fires | the race fixtures' own preconditions |
| M10 | the fixture stops concealing | the concealment precondition |
| M11 | the write is a no-op | every control test |
| M12 | the empty-item fixture is absent instead | its precondition |
| M13 / M14 | `delete` stops raising `SecretNotFound` / duplicate labels allowed | pre-existing rules moved into `_fields_of` |
| M15–M19 | the `null`-body sentinel guard and its fixture | the four new sentinel tests |

### The five known flavours of test-that-cannot-fail

1. **Ambient env var** — no test declares `config["env"]`, so `env_overlay()` returns `{}`
   whatever this process carries. Verified green with `OP_SERVICE_ACCOUNT_TOKEN`,
   `OP_ENG_DEVOPS_TOKEN` and `OP_ACCOUNT` all set, and with a PATH that has no `op`.
2. **A fix invalidating its own sibling test** — this happened, was expected, and is the
   whole of the rewrite above. Nine tests in the #354 file went red; each property was
   re-expressed against the one-read structure rather than dropped, and the count assertion
   was inverted (1, not 3) so the window cannot silently reopen.
3. **Fixture coincidence** — the three existing op fakes return real values whether or not
   `--reveal` was asked for, so `--reveal` could be deleted from the write path and every
   one of their assertions would still hold. The new fake models the concealment, and
   `test_the_precondition_the_fake_really_conceals_without_reveal` proves the fixture is
   doing work (M10).
4. **A fallback masking the mutation** — the two `SecretNotFound`-masquerading cases above.
5. **A fixture value given new meaning** — the adopted id is `27r3gphb4fnsonx5ikcaw3cxwq`,
   deliberately a string no key name could equal, so "the id survived" cannot be true by
   coincidence with the label.

No test invokes a real `op`: with `subprocess.run`/`Popen` and `util.run` replaced by
tripwires that raise, all 107 tests across the four op modules still pass. `op` **is**
installed on this machine, which is what makes `OpCase`'s `shutil.which` stub load-bearing
rather than decorative.

No secret value is printed, logged or persisted anywhere in the change. Fixture values are
inert strings; the fake records field **names** only; the two new error paths name the item
and never the body (M16 catches quoting `proc.stdout`).

## Also

`docs/news/unreleased-op-asks-once.md`, staged per the `charter news stamp` convention.

## Not done, deliberately

- The issue notes this "wants a machine with a real 1Password vault to verify against".
  There is still none here, and none of this was exercised against a real `op item edit` /
  `item create`. The fake is the whole of the evidence for the round trip.
- `docs/secrets.md` still documents the **one-item-per-key** schema ("item
  `charter-devops-KUBECONFIG`", "One item per *vault* … is wrong here"), which the code
  replaced some time ago. Pre-existing drift, out of scope here, worth its own issue.

## Verification

```
python3 -m unittest discover -s tests   ->  Ran 3739 tests   OK
```

(3710 before; +29 net, after the #354 file's nine rewritten tests.)
