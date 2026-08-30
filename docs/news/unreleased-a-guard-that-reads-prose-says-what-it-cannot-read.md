---
version: unreleased
headline: The guard on the documented `pad` bound stops matching backticks and starts matching the bound — plus the chat-bar snippet that turned every other panel off
---

Three findings from an adversarial review of last week's frame work. They share a shape:
each one is a rule that was written down correctly and then enforced, or documented,
against something that is not the rule.

**A guard for "a check matched a spelling instead of a property" matched a spelling
instead of a property.**

[#669](https://github.com/diazoxide/charter/issues/669) was two copies of the `pad` bound
disagreeing in `docs/frame.md`, and the harmful copy was the sentence about *refusal* — an
arrangement charter cannot draw is refused whole, so a `pad` the documentation invited
costs the operator their entire `[[frame.component]]` block. The fix added a checker that
read every range the file stated and held each to `FRAME_PANE_PAD_MAX`. Measured against
it, appending one line at a time:

```
"a pad outside 0 to 12 is refused"        -> OK        <- wrong bound, suite green
"pad is capped at 12"                     -> OK        <- wrong bound, suite green
"a `pad` outside `0`-`12` is refused"     -> FAILED    <- only this spelling was caught
```

Both readers required the digits to be **backticked**, and the second additionally
required the number and the literal `` `pad` `` to be in the same sentence. Neither is a
property of the bound; both are properties of the markup. #669's own harmful sentence,
written without backticks, went straight through.

Two things changed. The reader is scoped by **the word `pad`** — `pad`, `pads`, `padded`,
`padding`, and not `trackpad` — over the paragraph that names it and one sentence either
side of it, so a bound stated a paragraph away is in reach. And a number is a *count of
cells* unless it is part of a version: `3.2` and `3.7c` are excluded because a dot has a
digit on the far side of it, not because a number is followed by a dot. That second
lookahead was its own defect — `(?![.\d])` skips every integer that ends an English
sentence, so `The maximum` `` `pad` `` `is 12.` was invisible to the old reader too.

**The attack is now the test.** Nineteen restatements of the bound, with the number left
as a hole, live beside the reader as data. Each is run with a number charter refuses,
where it must be red, and again with `FRAME_PANE_PAD_MAX` in the hole, where it must be
green — a reader that fails everything catches nothing, and templating the corpus means it
survives the constant moving.

**And the limits are written down and run.** No reader of English prose is complete, so
the shapes this one cannot catch — a bound spelled in words, a bound in a paragraph that
never names the key — are a second list, executed by a test that goes red if one of them
starts being caught. The signal means *someone widened the reader*, and the answer is to
move the line into the corpus, never to narrow the reader back. A guard that overstates
its reach is the thing this project refuses.

**The snippet for turning the chat bar on built a frame with only the chat bar.**

[#690](https://github.com/diazoxide/charter/issues/690). `[[frame.component]]` supersedes
`slots` and replaces the arrangement whole, so the four-line example under *"Turn one on
with a `[[frame.component]]` table"* resolved to exactly this:

```
slots      : ['chats']
components : ['chats']
```

No identity row, no attention strip, no repo table, no sidebar. The caveat existed, seven
hundred lines further down in the toggle-keys section, which is not where the reader is
being told to write the table. Four snippets in the file had that shape; all four now list
the whole arrangement, because a sentence of warning above a fence is not read by someone
who stops at the fence. Every `[[frame.component]]` example in `docs/frame.md` is now
resolved through `instance.frame_of` by the suite and has to come back as the frame it
prints.

**`size = 2` on the chat bar took the whole arrangement out of play, on a reason that was
false for the chat bar.**

[#687](https://github.com/diazoxide/charter/issues/687). The boundary's rule is *a
committed value is accepted exactly where something reads it* — and it was implemented by
asking which size **policy class** the component declared, which is a different question.
`chats` and `workspaces` declare `Fixed(1)`, so they took the echo-only branch, on the
stated grounds that their geometry is derived at import and a different number could only
be ignored.

That is true of `identity`, `attention` and `sidebar`. It is not true of the two bars.
Charter places neither by default, so neither is in `layout._PLACED`, neither enters
`SLOT_SIZE`, and `layout._size_of` answers them out of `_placed_here` → `_policy_cells` —
a live read of the committed number on every launch, which reaches tmux as `split-window
-l`. Measured: a `size` on a bar is read; the boundary refused it anyway, and refusing it
dropped **every** `[[frame.component]]` table the plane had, in silence.

So the reason was right and the implementation was asking the wrong question. The
condition is membership of `builtins.SLOT_OF` now — the table that decides whether
`layout` derives a component's geometry — and a test holds `SLOT_OF` and `SLOT_SIZE` to
being the same set for every registered component, because `instance` reads the cheap one
on purpose and must not reach `frame/layout.py` at module scope.

```toml
[[frame.component]]
use  = "chats"
edge = "top"
size = 3        # a three-row bar, drawn where a one-row bar was refused
```

Nothing else widens: `0`, a negative, `true`, `"3"` and `3.0` are refused by
`component.Fixed` as before, and `identity`, `attention` and `sidebar` still take their own
number and nothing else.

**Still open, and named rather than fixed here:** a refused `[frame]` value is reported
nowhere — a dropped `slots` entry and a rejected `history-limit` are exactly as quiet — so
the fix is one surface for the whole section rather than a special case for `size`. #687
files that half separately.

[#689](https://github.com/diazoxide/charter/issues/689),
[#690](https://github.com/diazoxide/charter/issues/690) and
[#687](https://github.com/diazoxide/charter/issues/687), all three found by an adversarial
review of work merged in the previous 24 hours.
