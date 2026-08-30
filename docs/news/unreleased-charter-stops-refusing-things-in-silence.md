---
version: unreleased
headline: A frame charter would not build, and a command that did nothing, both say so now
---

*"I put `bg = "midnight"` in one table, launched, and got the default frame. `frame-probe`
said `✓`. `doctor`'s frame row said `✓`. Nothing anywhere mentioned my arrangement."*

Four things charter already knew and did not tell you.

**A `[[frame.component]]` arrangement charter cannot draw is refused whole — and now it
says which key did it.** The whole-arrangement rule stays and is right: dropping just the
table charter could not make sense of hands you a frame with a panel silently missing from
it, and a missing repo table is a plane that looks like it has no clones. What was wrong is
that the refusal left no trace anywhere. The frame that came up was the `slots` one, byte
for byte identical to the frame you would have had if you had never written the tables, and
every surface whose job is to report configuration health reported health.

```
$ charter frame-probe
! charter frame: tmux 3.7 — a frame can run on this machine
  ↳ `[[frame.component]]` is not in force — `bg = "midnight"` on `repos` is not one of
    charter's 17 pane background words… the frame you get is the one `[frame] slots`
    describes and every other table's `edge`, `size`, `bg`, `pad` and `key` goes with it.

$ charter doctor
  !  charter.toml   [[frame.component]] is refused; the frame is drawing `[frame] slots`
```

`charter.toml` and not the `frame` row, because it is the same fact as the two ignored
settings already there — a committed key that is not in force and reads exactly like one
nobody wrote. Every refusal on the documented list gets a sentence naming the table, the key
and the value: an unknown `use`, a misspelt key, a duplicate, a `visible` that is not a
bool, a `pad` outside 0–5, a `size` charter cannot give that component, an `edge` it cannot
move a built-in to, a `key` it will not bind or that something already has, a provider with
no `edge`/`size`, a provider this machine has no distribution for — and the single-bracket
`[frame.component]` typo, which TOML reads as a table rather than an array of them.

**A plane that wrote no arrangement still says nothing**, and that line is the whole reason
this is worth having. A warning that fires on working configurations gets switched off and
then protects nothing; the reason is `None` for every plane that did not write the key,
which is every plane charter ships.

**Six `frame-*` commands stopped reporting success for doing nothing.** `charter
frame-chat`, `frame-density`, `frame-toggle`, `frame-chrome`, `frame-switch` and
`frame-resize` each opened with four bytes of silence and a zero exit when there was no
frame to act on. Inside a tmux you already have, two of them are the *only* route to their
action — charter binds no key there — so typing one in the window you started from, rather
than the one charter opened, was indistinguishable from it working.

```
$ charter frame-toggle repos ; echo $?
charter: charter frame-toggle acts on the frame it is run inside, and this shell is not in
one — nothing was changed.
  Run it in the window `charter <harness>` opened. …
  charter docs show frame
1
```

The commands tmux fires for itself — `frame-palette`, `frame-respawn`, `frame-gather` — are
untouched and still exit 0 in silence, because a non-zero status inside a `run-shell` is
what makes tmux print into your harness pane. `frame-switch` was not on the original list
and turned out to be the same defect: it is the one of the six with a `--help` line, so it
is the one `charter --help` shows you.

**`charter claude --help` now says what the window it opens is.** Three facts, on every
launcher and on `frame-probe`: your palette key opens everything, `F12` takes the keyboard
back from a pane that has stopped answering, and scrollback is tmux's copy-mode rather than
your terminal's — which `docs/frame.md` calls the difference people notice first, and which
you had no reason to connect to charter at all. The palette key is read from your plane, so
a `[frame] hotkey` you moved is the key the page names.

**And the tmux 3.2 resize limit stops promising a relaunch.** Both surfaces said a
stretched frame stayed stretched until you started a new one. It stays stretched until you
ask. Measured at the floor, a frame launched at 120x40 and dragged to 80x24 and back:

```
%1 5x120   %0 22x97   %4 22x22   %3 5x120   %2 5x120     <- stretched, and staying
$ charter frame-resize
%1 1x120   %0 34x97   %4 34x22   %3 1x120   %2 1x120     <- the launch geometry, exactly
```

That is the same command the missing hook would have called; only the hook is
version-dependent. The same message also stopped saying "everything else in the frame
works" — at 80x24 on 3.2 the sidebar is squeezed to two columns of truncated glyphs and the
repo pane holds `⋯ too narrow for the repo table`, a line written to be transient that here
never settles, because nothing measures again.
