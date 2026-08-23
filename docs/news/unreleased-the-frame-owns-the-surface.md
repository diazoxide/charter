---
version: unreleased
headline: Inside a frame the status line goes quiet and the frame fills all four edges
---

The frame's panels are built out of the status line's own renderers, on purpose, so a fix
to a repo row or an alert lands on both surfaces at once. Turned on together that drew the
plane's state on the top strip, again on the bottom strip, and once more in Claude Code's
own footer three lines below them. **The frame owns the surface now (ADR 0019): inside a
frame `charter statusline` prints an empty line, and the panels are the one place the
plane's state appears.** Outside a frame nothing changes — same footer, same renderers,
same everything.

**Suppressed does not mean switched off, and this is the part worth reading before
tidying anything.** The command still runs and still records. Claude Code passes this
session's token usage to the `statusLine` command and to nothing else — no hook ever sees
those numbers — and the cache-hit trend, the prefix-rebuild count and its cumulative token
cost are all reconstructed from what that command writes down. Unwiring `statusLine` from
`.claude/settings.json` because it "prints nothing now" would not remove a duplicate; it
would delete the record, silently, and nothing would notice until somebody went looking
for a history that stopped months earlier.

Three things have to be true before a line goes blank, and each one is guarding against a
different way of getting it wrong:

- **stdout is a pipe**, which is how Claude Code calls it. Run `charter statusline`
  yourself in a terminal, or `charter statusline --watch`, and you get the full render —
  a frame elsewhere on your screen is no reason to answer you with a blank line.
- **`$CHARTER_SESSION_ID` names a frame directory on this plane.** That variable is set by
  any harness that knows its own session, not only by a frame.
- **the launcher named at the end of that id is still running.** A frame id ends in its
  launcher's pid, so `os.kill(pid, 0)` settles it with one syscall and no tmux call on a
  path that runs every time the footer repaints. Without it, a directory left behind by a
  crashed launcher would blank that plane's status line forever with nothing on screen to
  explain why.

**And because the frame now has to show what it suppresses, it fills all four edges by
default.** `[frame] slots` ships as `["top", "bottom", "left", "right"]` — `left` is
narrow repo rows, `right` is persona chips, and both have had renderers since the parity
release; they were built, tested and switched off. Two one-line strips were the status
line again in a worse shape, which is exactly how the first frame release was reported:
*"only top and bottom single lines added, no left right sidebar."* A terminal below
`[frame] min-cols`/`min-rows` still drops the sidebars first and ends up with the frame it
used to have, so nothing has to be configured for a small window.

The order of that list is the order the panes are split in, which makes it the geometry.
Measured against tmux 3.7c at 200×50: the shipped order gives a full-width 200-column
bottom row with 46-row sidebars between the two strips; moving `bottom` to the end instead
gives 48-row sidebars and a bottom row of 154 columns inset between them. The bottom row
is the one that carries an alert and the command that fixes it, and it drops whole fields
when it runs out of width, so it gets the columns.

**One thing is genuinely lost and worth saying plainly:** `ctx NN%` and `cache NN%` lived
on the status line, and no panel draws them yet, so a framed Claude Code session does not
show them. The recording is kept alive precisely so a panel can be given one. codex and
opencode never had them at all — neither is handed a per-turn usage payload, so there has
never been anything to draw from.

`$CHARTER_SESSION_ID` is also settled and documented rather than left as an accident:
inside a frame it holds the FRAME's id, so the agent's shell, every panel and every
`charter` command typed inside the frame agree on one charter session. That is why
`charter workspace use <name>` at the agent moves the panels — the pointer is written
under the frame's id and the panels read it back under the same one. Claude Code's own
session id still arrives in the status line's payload and still keys what comes with it.
`docs/frame.md` and ADR 0019 carry the whole rule.

Nothing to adopt: upgrading is the whole of it. A plane that has pinned `[frame] slots` in
its own `charter.toml` keeps exactly what it pinned.
