---
version: unreleased
headline: The frame's geometry stops reading `charter.toml` behind its callers' backs, so pinning the repo strip on charter's own plane no longer turns the test suite red
---

The repo strip gained a height you could choose, and on one plane you could not use it:
**charter's own.** Writing the three lines that feature's own news entry gives you

```toml
[[frame.component]]
use  = "repos"
size = 15
```

into this repository's `charter.toml` turned six tests red. `charter.toml` is a tracked
file here, so committing them would have turned CI red for everyone — a feature that
could not be used on the plane that shipped it.

Nothing about the feature changes. A pin is still fifteen rows with one clone and fifteen
with thirty, still capped so your session keeps its floor, still the one pane tmux is
never told the height of. What changes is where charter reads your number.

## What went wrong

`layout.repos_rows` answers one question — *how tall is the repo strip?* — from the rows
its content wants, a floor, and what the window can spare. It is charter's one piece of
frame geometry that is nothing but arithmetic: hand it a content count and a window size
and it answers the same thing on every machine, with no tmux, no cache and no plane
behind it. Its tests are written to exactly that, and they say so.

The pin was read from inside it. So on a plane that had committed one:

```
repos_rows(content_rows=4, window_rows=50, slots=["top","bottom","repos"])  ->  15
```

Four rows of content, a fifty-row window, neither the floor nor the cap anywhere near
binding — and the answer is a number out of a file the caller never named.

That is not the same defect as a test that reads your clock or your terminal width. Those
read a machine that happens to differ. This read the repository's own committed file, so
the failure was deterministic and arrived by following the documentation.

## What changed

The plane is read once, at the boundary that already turns a frame into pane sizes
(`commands_frame._slot_sizes`), and the number is handed down to the arithmetic. All
three paths that size panes — a launch, a density re-layout, and the recompute on every
terminal resize — go through it, so they cannot disagree, and `layout.repos_rows` is
arithmetic again.

The first cut chose the config read deliberately, arguing that threading a value through
five signatures to reach one leaf is its own defect. That argument is real, and it was
borrowed from a different question: five is the count for *where does a placed component
sit and how many cells does it cost*, which five separate functions each have to ask
about a name none of them knows in advance. A pinned strip is one number, for one slot,
with one consumer. Measured, its path is two signatures and three call sites — and the
three were already building the sibling argument the same way three times over, so the
read landed in one line and deleted two copies of something else.

Pinned by a test that adds `size = 15` to this repository's real `charter.toml` and
asserts the arithmetic still answers what it was asked.
