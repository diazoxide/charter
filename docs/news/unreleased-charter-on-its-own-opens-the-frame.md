---
version: unreleased
headline: '`charter` on its own opens the frame, once your plane says which harness it means'
---

The command was `charter claude`, every time, for the whole life of the frame. Now it is:

```toml
[harness]
default = "claude"
```

and then `charter`.

That is the entire feature. `charter` with no subcommand is rewritten into `charter
claude` and run — not something like it, the same command — so the workspace picker,
`--no-frame`, `--probe`, `--workspace`, the chat it opens and the exit code it carries back
are all the ones you already have. `charter claude` still works, and so does every other
subcommand.

## Charter does not pick a harness for you

A plane that writes no `[harness] default` gets exactly what it got yesterday: the usage
list. That is a decision, not an omission. The two obvious guesses are both worse than
asking:

- *whatever is installed* — a machine with `claude` and `codex` on it has no answer, and
  the answer changes the day a colleague installs a third;
- *the one you ran last* — a file on your laptop deciding what a command in a committed
  file does.

Naming it is one line, and once named it is in the repo where the rest of the plane's
shape lives.

**`claude`, where a plane has no strong feeling, and the reason is measurable.** It is the
only harness that writes its session id somewhere charter can read — the only caller of
`record_harness_session` is Claude Code's own `statusLine` hook — which makes it the only
harness with a context gauge in the top bar. Codex and opencode both carry a `status-bar`
deficit that says charter cannot render into their chrome at all. A bare command that
quietly cost you the gauge would be charter choosing for you without saying so.

## A name charter cannot launch is reported, not ignored

```
$ charter
✗ charter: [harness] default = "clyde" in ~/plane/charter.toml is not a harness charter
  can launch — one of: claude, opencode, codex. Nothing was started.
```

A refused value falls back to *no default*, and no default prints the usage message —
which is the same output a plane that declared nothing gets. Left silent, a typo would be
indistinguishable from a key you never wrote, and you would go looking in the wrong place
for the reason your new command does nothing. `charter doctor` says the same thing on its
`charter.toml` row, for the plane where somebody else committed the typo and you are the
one running into it.

The legal names come out of charter's own harness registry rather than a list somewhere,
so a harness added to charter is a legal default the day it is added.

## `charter | head` still prints usage, and that is the part that needed care

Bare `charter` opens a frame only when its output is a terminal. Piped or redirected, it
prints the usage message and exits 2 — which is what it did before this key existed.

The reason is that charter's launcher, with output that is not a terminal, does not build
a frame at all: it `exec`s the harness in place of itself. Until now bare `charter` was an
argument-parsing error, so `charter 2>&1 | head` was a free way for a script to ask whether
charter is installed. Without this rule, that pipeline would have started an agent session
on any plane that set a default — correct against every configuration anyone tested, wrong
the first time something automated went looking for charter.

`charter claude` into a pipe is unchanged: it still runs the harness bare, exactly as
before.
