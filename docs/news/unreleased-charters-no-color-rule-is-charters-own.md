---
version: unreleased
headline: charter's `NO_COLOR` rule is stated as charter's own, because the page it cited stopped saying it in 2022
---

No behaviour changes. What changes is that charter no longer attributes its `NO_COLOR`
reading to a standard that says something else.

## The stale quote

`chrome.no_colour` is `os.environ.get("NO_COLOR") is not None` — **presence**, so `""` and
`0` both mean no colour. Its docstring justified that as *"per the `no-color.org`
convention: any value, including the empty string and `0`, means no colour."*

That was the page's wording once. `jcs/no_color` commit `99f90e27` changed it on
2022-06-27, and the diff is the whole finding:

```diff
-check for the presence of a `NO_COLOR` environment variable that, when present
-(regardless of  its value), prevents the addition of ANSI color.**
+check for a `NO_COLOR` environment variable that, when present and not an empty
+string (regardless of its value), prevents the addition of ANSI color.**
```

So charter cited, as its authority, a sentence that had been replaced four years earlier —
and the replacement disagrees with charter on exactly one input, `NO_COLOR=""`. A citation
reads as a measurement; this one was a quote of something no longer on the page.

## The rule stands, and now says whose it is

A shell that exports `NO_COLOR=` has set it, and the operator who typed that asked for
something. Matching a *value* (`== "1"`) is the spelling-not-property mistake in the one
file that is about that mistake, and `NO_COLOR=` is the input it breaks on first. That
argument is charter's own and needs no borrowed authority, which is how it is written now.

There is also no convention left to defer to. Both readings are in the field, from their
own sources: ripgrep's manual says *"when the `NO_COLOR` environment variable is set
(regardless of value)"*, and `rich` moved the other way in PR #3675 — *"an empty NO_COLOR
env var is now considered disabled."*

## And charter is stricter than that standard in a second way

The same page answers, about bold, underline and italic: *"No. This standard only signals
the user's intention regarding adding ANSI color to text output."* They may still be
emitted under `NO_COLOR`.

Charter emits none of them. `chrome.recipes` serves the empty string for **every** role
including `heading`'s bold, and `panel._write` escapes a hard-coded escape a component
wrote. That is deliberate and the reason is one the standard is not about: the frame's
background is painted by tmux at charter's request, so the property charter keeps is *no
colour on your screen caused by charter, whichever process puts the bytes there* — and an
attribute charter still emitted would be charter still painting. It is stated now instead
of waiting to be discovered.

**If you set `NO_COLOR=""` expecting current-standard semantics, charter will give you no
colour.** That is the one operator-visible consequence, and it is the same as it was.

## Where the citation was, and what happened to the dated one

Three live sites are corrected — `chrome.no_colour`, `test_frame_chrome` and
`test_frame_provider_recipes`. The fourth is `docs/superpowers/specs/2026-08-28-frame-
visual-design.md`, which is a dated record; it keeps what it recorded and carries a dated
correction beside it rather than being rewritten.

## Verification

The commit, its date and its diff were read from `jcs/no_color` rather than taken on
report; the current normative sentence and the bold/underline/italic answer were read off
`no-color.org`; ripgrep's wording came from its own manual source and `rich`'s from its
changelog. The behaviour is pinned as before, and confirmed still pinned: switching
`no_colour` to the current-standard reading (`bool(os.environ.get("NO_COLOR"))`) turns 12
existing cases red.

Nothing to adopt.
