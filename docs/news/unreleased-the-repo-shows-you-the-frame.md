---
version: unreleased
headline: the repo shows you the frame — a capture of charter's real surface, and it is what a shared link previews now
---

`docs/assets/` held a capture of every renderer charter has and none of the thing they
compose into. The frame is charter's only ambient surface since #895 — a strip of workspace
tabs, a strip of chat tabs, the repo table, the persona column, the harness in the middle —
and nothing in the repository showed it. `docs/frame.md` described it in four thousand
words and had no picture.

`docs/assets/frame.svg` is that picture, and it is a capture like every other file in that
directory: real panels reading a real plane, real git answers about real repositories, real
pane borders. Nothing in it is drawn.

## The social preview shows it instead of a status line

`social-card.svg` embedded `statusline.svg`, so the image every pasted link rendered — on
Reddit, in Slack, on X — was a picture of the status line charter had *just stopped wiring
into Claude Code*. It embeds the frame now, cropped to its chrome, both tab strips and the
first rows of the harness pane and the persona column beside it.

**`charter statusline` is untouched by that**, and the distinction is the whole point.
Three things still consume it — opencode's `/charter` slash command, the `status-bar`
remedy opencode and Codex both carry, and the frame's own panels, which are built out of
its renderers — so `statusline.svg` stays a capture, stays stamped, and stays in the
freshness gate. What changed is which capture argues for charter on a first impression.

## Capturing a frame needs a second terminal, because a border belongs to no pane

Every other capture here is one command's stdout on a pty (`ptyrun.py`). A frame is six
processes painting six rectangles, and the borders between them — and each pane's default
cells — are composed by tmux for its *client*, so no pane's output contains them. So
`capture-frame.sh` renders the frame inside a **second** tmux and captures the outer pane,
which is the same nesting `tests/test_a_planes_frame_really_reads_that_way.py` already uses
to measure what colour a border is.

Two things it does that the existing `$PATH` shim cannot:

* **The tree goes in a throwaway venv, not on `$PATH`.** Charter starts each panel as
  `sys.executable -P -m charter panel …`, and tmux starts those panes from the *server's*
  environment — captured whenever that shared server first started, possibly days ago and
  by a different charter. A shim reaches none of them: measured as four panels dead at
  once, `No module named charter`, and four `Pane is dead (status 1)` messages where the
  capture should have been. The venv is stdlib `venv` plus one `.pth` line — no pip, no
  network, so regenerating still needs no toolchain.
* **It cleans up after itself on charter's shared socket.** Frames share one tmux server
  by design, so the capture kills the session it created — read back from tmux rather than
  assumed, because charter suffixes a session name that is already taken — and never the
  server somebody else's frame is on.

## `ansi2svg.py` learned the one attribute that says which thing is chosen

It understood colour, bold, dim and underline — every attribute about how a glyph *looks* —
and dropped reverse video, which is how a terminal marks a **selection**. Charter's frame
uses it for three: the active workspace tab, the active chat tab, and the persona row. A
capture that loses it shows a frame with no current tab anywhere and looks perfectly fine
doing it, which is exactly the drift `docs/assets/README.md`'s no-hand-editing rule exists
to prevent. It is drawn as a filled cell per run, because a tab's own trailing spaces are
part of the mark.

## The rest of the captures were about to fail the gate

`demo.svg`, `personas.svg` and `statusline.svg` were stamped 0.55.0 against a charter that
is 0.56.0. `MAX_MINOR_LAG` is 1, so they passed and would have failed at 0.57.0. All three
are retaken:

* **`demo.svg`** shows `charter init` writing `.claude/settings.json (env)` rather than
  `(statusLine, env, plane-root guard)`, which is #895 landing in the quickstart.
* **`statusline.svg`** is taken without `--compose`. That flag drew Claude Code's prompt box
  under the render, because the status line never appeared anywhere else; it does not sit
  under that box anywhere now. It was the one drawn element inside a capture, and there is
  none left.
* **`personas.svg`** is the same roster against the plane `demo-plane.sh` builds now.

`frame.svg` was added to `captured.json` **and** to `test_asset_freshness`'s capture set in
the commit that created it. A capture nobody lists is exempt from the freshness check
forever, which is the one failure that test exists to stop.
