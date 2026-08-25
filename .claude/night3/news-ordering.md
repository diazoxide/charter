# news-ordering
PR: https://github.com/diazoxide/charter/pull/490 — retitled "A release leads with the entry that matters, not the one named earliest (#486)", body rewritten for the narrowed scope (the #458 section replaced by an explanation of why #492's fix is better, #452's refutation kept, a new round-two section, and the #502/#503 filings). All CI checks green on the pushed head: test 3.11/3.12/3.13/3.14 pass on both runs.
Branch: small-surfaces-458-486-452 (now 0453ef1, single commit on origin/main b4f135d; pushed with --force-with-lease from a detached HEAD because the branch is checked out in a sibling worktree)
Weaker: NO. Nothing on this branch is weaker than origin/main, checked deliberately rather than assumed.

The whole diff is additive denial. `charter news --for` on main has exactly one way to exit 1 (no renderable body); this branch keeps that path byte-identical and adds two more refusals in front of it (unreadable ordering value, two `lead: true` claims). Every input main refuses, this branch still refuses; inputs this branch refuses, main accepted.

Checked the one place a new early-return could have weakened it: for a version with no entries, ordering_errors([]) returns [] and control falls straight through to the original `if not body: return 1`. Same answer as main.

The release.yml change is text only — the step's condition `if ! python -m charter news --for "$version" >/dev/null;` is unchanged, and the annotation and step name are what moved. The redirect was not widened; the new test forbids ever widening it to 2>&1.

news.all()'s sort key gained rank(e) as a third element, which reorders presentation and gates nothing. marker() only prefixes a headline. The range view gained warnings and still returns 0 on the same inputs — deliberately, since refusing there would lose a catching-up reader nineteen entries over a twentieth's typo.

## Bypass

NEW SPELLING, REPRODUCED: an ordering field declared with an EMPTY value is read as False, honoured as nothing, and reported by nothing.

`news._read` does `raw = (meta.get(field) or "").strip()`, which collapses "key absent" and "key present, value empty" into the same `""`, and `_flag("")` returns `False` — not `None`. So `field in meta` is never asked, and present-but-empty takes the "absent is False" path.

The realistic authoring shape is the YAML habit — the value on the continuation line:

    ---
    version: 0.60.0
    headline: important
    security:
      true
    ---

`persona.parse` gives `meta["security"] == ""` (the `  true` line has no colon and is dropped). Run against the branch, with a second ordinary entry named `a-ordinary`:

    rank 2  security False  bad ()   0.60.0-a-ordinary.md
    rank 2  security False  bad ()   0.60.0-z-important.md
    ordering_errors: []
    cmd_news(--for 0.60.0) -> rc 0, stderr empty
    body: "### ordinary" first, "### important" second

The author declared a security fix; charter published it below the ordinary entry, exited 0, and said nothing. That is #486's own mechanism restored through the field added to prevent it — and it is precisely what `_flag`'s docstring says cannot happen: "Present-but-unrecognised is **None**, not False, and that distinction is the point of this function." Same failure from `security:` with a trailing space, from `security:` typed and the value forgotten, and from a duplicate key whose second occurrence is empty (last write wins).

`WhatShippedStillParses` is not a net for this either: `ordering_errors` returns `[]`, so a shipped entry in this shape passes.

THE DOCSTRING THAT CLAIMS IT IS CAUGHT (C, bundled — fix in the same commit). `ADocumentedExampleIsParsedTheWayCharterParsesIt`'s class docstring says: "A trailing `# …`, an inline YAML `{}`, a quoted `"true"`, a value indented onto the next line: each yields something `_flag` does not understand, and each fails here." Three of the four hold. The fourth is false, and I proved it end-to-end rather than by inference: I added `docs/news-example-probe.md` containing a fenced frontmatter block with `security:` and `true` on the next line, then ran the test — `OK`, 1 test, passed. `_flag("")` is `False`, so `assertIsNotNone` is satisfied. The test is otherwise well-built (it already gates on `if field not in meta: continue`, so it is structured to catch this); it is `_read`/`_flag` collapsing presence that defeats it.

FIX IS SMALL AND I VERIFIED IT IS NON-BREAKING. In `news._read`:

    value = None if (field in meta and raw == "") else _flag(raw)

With that one line: the repro above becomes rc 1 with "0.60.0-z-important.md: `security: ` is not a value charter reads …", and `tests/test_news_ordering.py` + `tests/test_news.py` still pass 54/54. The message wants a small tweak for the empty case (it renders as "`security: `"), which is the implementer's call. The docstring sentence then becomes true rather than needing narrowing.

## Blocking

### 1

(A) THE DEFECT STILL REPRODUCES on a new spelling of the same field. `charter/news.py` `_read()` does `raw = (meta.get(field) or "").strip()`, which makes "key present with an empty value" indistinguishable from "key absent", and `_flag("")` returns False. So an entry that declares `security:` with the value on the next line (the YAML habit), or `security:` with nothing after it, or `security: ` with a trailing space, is silently read as false — the entry sinks below the ordinary entries, `charter news --for` exits 0, and stderr is empty. Reproduced: two 0.60.0 entries, `z-important` declaring `security:` newline `  true` and `a-ordinary` declaring nothing, render as `### ordinary` then `### important` with `ordering_errors == []` and rc 0. This is exactly the failure `_flag`'s own docstring says is impossible ("Present-but-unrecognised is **None**, not False, and that distinction is the point of this function"), on the code this PR adds, and `WhatShippedStillParses` gives no net because `ordering_errors` returns []. Fix verified: `value = None if (field in meta and raw == "") else _flag(raw)` in `_read` — the repro then refuses with rc 1, and tests/test_news_ordering.py + tests/test_news.py still pass 54/54.

### 2

(C, must be settled in the same commit — it is the sentence covering the finding above) `tests/test_news_ordering.py` `ADocumentedExampleIsParsedTheWayCharterParsesIt` class docstring claims "A trailing `# …`, an inline YAML `{}`, a quoted `\"true\"`, a value indented onto the next line: each yields something `_flag` does not understand, and each fails here." The fourth is false. I added `docs/news-example-probe.md` with a fenced frontmatter block showing `security:` and `true` on the next line and ran the test: OK, passed. A doc example teaching the multi-line spelling would ship uncaught by the test written to stop exactly that. Closing the `_read` gap makes the sentence true; leaving the gap open means the sentence has to be narrowed to admit that charter silently reads an empty declaration as false, which is a worse thing to have to write in the module whose premise is that it never does.

