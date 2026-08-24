---
version: unreleased
headline: The repo table moves to the bottom of the frame, and the left sidebar is retired
---

The frame's whole claim is that it shows the plane's live state better than the single
status line it suppresses. In one place it showed *less*: the only slot drawing repos was
a 22-column `left` sidebar, and the wide table it was standing in for wants 32 columns for
the repo name and 34 for the branch before anything else. Its own docstring conceded the
point. So a real branch name was always elided, and on this project's own branches —
`worktree-recall-since`, `browser-session-scope`: 21-28 characters is the norm — a dirty,
CI-failing, unpushed repo could render as `charter worktree-reca…`, which reads as clean.

**`bottom` draws the full table now, at the widths it was designed for**, and `left` is
gone. The shipped `[frame] slots` is `["top", "bottom", "right"]`, and the 22 columns the
sidebar was taking go back to your agent session.

**`bottom` is variable-height, and that is the mechanism rather than a detail.** It was
one row. It is now the attention row it always was — the alert and the command that fixes
it, this session's news, the todo count, the spinner — plus one row per repo (and per
worktree, in a single-repo workspace) underneath. The table joins that row; it never
evicts it. A workspace with no clones gets exactly the one row it used to have, which is
also what the "always empty sidebar" report turned out to be: the `default` workspace has
zero clones and the panel was telling the truth.

Two bounds decide the height, and both are worth knowing:

- **A floor of one row.** The alert line is the thing a cramped terminal most needs.
- **A cap that leaves your agent session at least 12 rows.** Measured against tmux 3.7c,
  `resize-pane -y 40` in a 20-row window is not refused — tmux grants it and takes the
  difference out of the neighbouring pane, and the neighbour is the harness. It left the
  harness **one row tall**.

**Which means the `window-resized` hook had to stop carrying a number.** It used to hold
literal `resize-pane -t %1 -y 1` text, computed once when the frame was laid out. A height
that depends on the window cannot be a constant, so the hook now calls charter back —
`charter frame-resize` — and the sizes are recomputed against the window that actually
exists. That costs a short charter process per resize event, backgrounded, the same shape
the panel-respawn hook has had since 0.51.

It also closes a hole nobody had to reach for. The old action interpolated pane ids read
back off **disk** into a string tmux re-parses as a command line, and the shape check on
those ids guarded only the slot being closed — every slot a density change *kept* went
through unexamined. A `%1;kill-server` written into a frame's own state directory would
have armed `kill-server` on every resize for the life of the window. No pane id reaches
that text at all now; the check is hoisted above both branches, and the builder that
resizes refuses a bad id itself rather than trusting its caller.

**The density presets are re-derived, not patched.** `full` now means "every edge charter
draws". `minimal` and `normal` still expand to the same two strips, but what separates
them is no longer only how much each panel *says* — `minimal` also keeps at most four rows
of table, and the four that survive are ranked (the repo you are standing in, the ones
with something on them), with the `…(+N more)` line still saying how many it hid.

**The panel's idle cost is unchanged, and that was a constraint rather than a hope.** A
panel's idle tick is still exactly one `stat`. `bottom` is the one slot that animates, so a
table that walked a directory per row would have paid that back fourteen times over, five
times a second, for the length of every dispatch. Every row comes out of the gather cache;
nothing here opens a repo directory. The one column that cannot be answered that way —
presence, "who else is standing in this tree" — is absent rather than faked, exactly as
the status line drops it on a pane too narrow to hold it.

If the frame ends up narrower than the table's own 95 columns, the table is not drawn
rather than drawn cut off. A row trimmed past the branch loses the CI glyph and the open
change, and a failing repo then reads as a clean one — "no room to say" beats "nothing to
say". The shipped `min-cols` is 100, so this only comes up on a hand-lowered floor or
during a resize.

Nothing to adopt: upgrading is the whole of it. A `charter.toml` still naming `left` in
`[frame] slots` is not an error — the name is dropped the way any unknown slot always was,
and you get the rest of your list. If yours pins the order, check it: `bottom` listed
after `right` is inset to 177 columns instead of the window's full width, and the table
starts giving up its right-hand columns.
