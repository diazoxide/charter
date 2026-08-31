---
version: unreleased
headline: The top bar stops keeping a roster it drew before the frame's shape was on disk
---

Launches came up with the persona roster drawn twice — once across the identity row and
again down the sidebar — and stayed that way for the life of that frame. Roughly one in
six on an idle machine, and **about two in three on a busy one**, which is what a control
plane running several frames is. #530 had removed that duplication a release ago; this is why it kept coming back.

## What was happening

`_top` asks `slots._sidebar_live` on every repaint whether the sidebar is on screen, and
that reads `state.panes` — the record of which tmux pane charter meant as which slot. The
record is written by the launcher, and it can only be written **after** every
`split-window`, because the pane ids in it are what those splits returned.

A panel is a `charter panel` process in a pane of its own, and `top` is the first pane
split. So `top` can be up and painting while the launcher is still carving `bottom`,
`repos` and `right`. When it is, `state.panes` answers `{}`, `_sidebar_live` answers
`False` — its documented-safe direction, because a wrong `True` would take the plane's
only roster off a screen that has no sidebar — and the roster goes on the top bar.

That would have been a flicker if anything had told the panel to look again. Nothing did.
`top` is not an animated slot: it repaints on a version bump and on nothing else. The
launch's own bump happens before the first split, so the only bump that could still land
after the record was the detached repo gather's — and the gather is deliberately racing
the frame's construction, so it just as often landed first. Once it had, the frame was
still until the session's first tool call.

## The fix is one line, on the writing side

`commands_frame._draw_panels` now bumps the frame immediately after `state.record_panes`.
Writing the shape down is an event rather than a silent file, so a panel that painted
before the record has a reason to paint again, and its second paint reads the shape.

That is the ordering fix #748 asked for, but neither of the two it proposed. Deferring
every panel's first paint until the record exists would put a wait on the launch path,
which is already the slowest thing charter does. Making the gather's bump wait on the
record would put the correction behind a detached child that `_spawn_gather` is explicitly
allowed to fail to spawn. Bumping where the shape is written needs neither: it is the
order `_apply_arrangement` already keeps around the other call to `record_panes`, for this
same question one keypress later, and the order `notify.plane_changed` states outright — a poller that saw the new
version must never then read the old record.

## Measured

Real launches on the private-server path — a real pty, a real `tmux attach`, the same
plane and the same command every time — with `main` and this branch **alternated**, run
for run, because the race moves with machine load and two batches taken back to back are
not a comparison. A frame counts as duplicated only if the roster was still on the top bar
once the frame had gone still.

```
                                        origin/main      this branch
launches that kept the roster            16 / 25            0 / 25
                                          (64%)
times the top bar painted at all          1 every time   2 on 22 of 25
median time to first content              0.385 s          0.390 s
```

The middle row is the defect and the fix in one number. On `main` the top bar paints
**once per launch and never again** — so whatever it drew before the shape reached disk is
what that frame keeps. On this branch it paints a second time, and the second paint reads
the shape.

Forcing the race — the launcher's own `record_panes` write delayed so the panels lose
every time — shows the same two lines with nothing left to chance. The pane's own history,
verbatim:

```
origin/main                        this branch
⬢ alpha*  ◆ steward · ◇ perso…     ⬢ alpha*  ◆ steward · ◇ perso…
                                   ⬢ alpha*  ◆ steward
(one paint, 5 of 5 launches)       (two paints, 5 of 5 launches)
```

On an otherwise idle machine the rate is lower and the defect is the same: 16 of 90
launches on `main` at 200x50 kept the roster, against 0 of 90 on this branch. Across every
rig run for this change — idle, loaded, and forced — `main` produced 61 standing
duplications and this branch produced **none in 163 launches**.

The roster still goes up on the launches that lose the race; that paint is honest, because
charter genuinely cannot tell yet, and it is the one `_sidebar_live` has always called the
safe direction. What has changed is that it is now followed by a paint that can tell. On 2
of 5 forced launches the correction landed before the client had even attached, so the
duplicated row never reached the terminal at all.

The launch pays one `os.replace` of a version file and at most one extra repaint per
panel, both spent before any client is attached and before the frame's window is selected.
It does not show up in the time to first content.
