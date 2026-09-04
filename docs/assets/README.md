# README assets

Two kinds of image live here, and the difference matters when one needs updating.

**Captures** are generated from real command output. None of them are mockups and none
should ever be hand-edited — regenerate instead, so a screenshot cannot quietly drift from
what charter actually prints. **Drawings** are authored by hand and say what they mean to
say; they have no source to re-run. **Composed** assets are drawings that embed a capture,
so they inherit the capture's freshness and are regenerated the same way.

| File | Kind | What it is | How to update |
| --- | --- | --- | --- |
| `frame.svg` | capture | The frame — charter's panels around a harness pane, borders and all | `capture-frame.sh` → `ansi2svg.py` |
| `statusline.svg` | capture | The plane render, taken against the demo plane below | `demo-plane.sh` → `charter statusline` → `ansi2svg.py` |
| `demo.svg` | capture | The quickstart, animated | `capture-demo.sh` → `ansi2svg.py --animate` |
| `personas.svg` | capture | The persona roster, rendered against the demo plane below | `demo-plane.sh` → `charter persona list` → `ansi2svg.py` |
| `model.svg` | drawing | The on-disk model | Edit by hand |
| `social-card.svg` | composed | GitHub's social preview — the image link previews show | `social-card.py` (re-reads `frame.svg`) |
| `social-card.png` | rendered | `social-card.svg` at 2560×1280 (2:1), for upload | See below; do not edit the PNG |

`social-card.png` is the only asset here that is not used by the repo itself: GitHub stores
the social preview separately, uploaded through **Settings → General → Social preview**,
which has no CLI. The PNG is committed so the upload is reproducible rather than a one-off
that exists only inside a settings page. Regenerate both in order — the SVG re-reads
`frame.svg`, so a stale frame makes a stale card:

```bash
python3 docs/assets/social-card.py
```

```bash
"/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" --headless \
  --force-device-scale-factor=2 --window-size=1280,640 \
  --screenshot=docs/assets/social-card.png file://$PWD/docs/assets/social-card.svg
```

GitHub recommends 1280×640 and caps uploads at 1MB; rendering at 2× keeps it sharp on
retina and still lands around 120KB.

## Staying current

`captured.json` records the charter version each capture was taken at. `tests/
test_asset_freshness.py` fails when one falls **more than a minor** behind, which leaves
exactly one release of slack: bump once and nothing happens, bump twice without
regenerating and the suite says so.

A check rather than a release step, deliberately. Regenerating on every publish would put a
terminal recorder on the critical path and let a flaky capture block a release — the same
reasoning that keeps `charter version sync` from running `claude plugin update` for you.

Update the stamp in the same commit as the regenerated asset:

```json
{ "demo.svg": "0.39.0", "statusline.svg": "0.39.0" }
```

The stamp is a sidecar rather than a comment inside each SVG, because `ansi2svg.py`
rewrites an SVG wholesale — a version written by the step that is supposed to be recording
it proves nothing.

## The tools

**`ansi2svg.py`** turns captured ANSI output into a self-contained SVG. Stdlib only, so
regenerating never needs a toolchain. It measures columns with the same rule
`charter/tui.py` uses, and positions every glyph explicitly rather than relying on
`textLength` — some SVG rasterizers ignore that attribute and walk the whole line off the
right edge. `--animate` reveals the capture line by line on a loop, using CSS rather than
script so it plays inside an `<img>`; a renderer that ignores the animation shows the
finished transcript, which is a worse asset but never a broken one.

It draws reverse video (`ESC[7m`) as a filled cell, which is the one attribute here that
is not about how a glyph looks: it is how a terminal says which thing is **chosen** — the
frame's active workspace tab, its active chat tab, the persona row. A renderer that
dropped it produced a capture with no selection visible anywhere and no way to tell from
the file that anything was missing.

**`ptyrun.py`** runs a command with its output on a pseudo-terminal. charter decides
whether to colour its output from `sys.stderr.isatty()`, read once at import time with no
environment override, so a capture taken down a plain pipe comes out monochrome. `script`
does the same job in one word but needs the *parent* to already own a terminal, which an
agent shell or a CI runner does not have.

**`capture-frame.sh`** captures the frame, which no `ptyrun` can: the frame is tmux's
composition of six processes, and the borders between them belong to none of them. So it
renders the frame inside a **second** tmux and captures the outer pane — the same nesting
`tests/test_a_planes_frame_really_reads_that_way.py` uses to measure a border's colour,
because tmux composes borders and default cells for a *client*, so only another terminal
can see them. It also puts this tree into a throwaway stdlib venv (a `.pth` line, no pip,
no network) rather than a `$PATH` shim: charter starts each panel as
`sys.executable -P -m charter panel …` from the tmux server's own environment, so a shim
reaches none of them — measured as four dead panels and `No module named charter` where
the capture should have been.

**`social-card.py`** composes the social preview: the wordmark and tagline over a crop of
`frame.svg`, embedded as a nested `<svg>` rather than re-rendered, so the card cannot
disagree with the capture it is made of. It used to be a hand drawing that said only what
could never go stale — a wordmark and three nouns — which also meant it never showed the
one thing that actually argues for charter. Which capture it crops is one constant
(`CAPTURE`), and it moved off `statusline.svg` with #898, because a preview of a status
line charter no longer wires anywhere was the wrong first impression to hand a link. The
bottom fade is load-bearing: the footer sits on it, and a shallower one leaves the URL
unreadable on live terminal rows.

**`demo-plane.sh`** builds a throwaway control plane worth screenshotting: real `charter`
commands, real git repos, real branches and real dirty/unpushed state. Only the *org* is
invented, so a render can show a plausible multi-repo day without exposing anyone's actual
work. A few things it writes directly rather than fetching — `inventory/repos.json`, the
forge-state cache, the `pieces/seen/` heartbeats and an in-flight dispatch record — are
the files `charter discover`, `charter gl-refresh`, the every-turn hook and
`inflight.start` would have written, in exactly their shape; the demo has no forge to
query and no live session to dispatch anything.

That last one is the trap to remember when adding a status-line surface. Anything drawn
from **live** state renders as nothing in a capture unless this script fabricates the
state first — the running badge (`⚡2 4m`) draws only while a dispatch is actually out, so
a plane without a record shows a roster where nobody is working, beside prose about the
badge. Add the fixture in the same change as the feature, or the next regeneration comes
back quietly half-stale.

The frame capture added two more of the same kind, and neither is live state — they are
*configuration* and *content*, which fail identically. Charter places neither tab bar
unless a plane writes a `[[frame.component]]` table naming one (`frame/builtins.py`), so
this script writes the whole arrangement out longhand; and a plane with one workspace draws
a workspace strip with one tab on it, which shows nothing about what a strip is for, so
there is a second workspace with a clone of its own.

```bash
./docs/assets/demo-plane.sh /tmp/demo-plane
cd /tmp/demo-plane
cat > /tmp/payload.json <<EOF
{"workspace":{"current_dir":"$PWD"},"model":{"display_name":"Opus 5"},"session_id":"demo","context_window":{"used_percentage":38,"current_usage":{"cache_read_input_tokens":74000,"cache_creation_input_tokens":6000}}}
EOF
COLUMNS=150 python3 docs/assets/ptyrun.py sh -c 'charter statusline < /tmp/payload.json' \
  | python3 docs/assets/ansi2svg.py --title "charter statusline" \
      -o docs/assets/statusline.svg
```

Three things in that command are load-bearing, and the capture this recipe replaced had
none of them:

* **`COLUMNS=150`.** The persona column needs `_LEFT_W + _COL_SEP + _RIGHT_MIN_W` = 134
  inner columns. At 110 the layout correctly falls back to stacking personas as rows, so
  the capture showed charter's narrow-terminal behaviour while the prose described the
  wide one.
* **`context_window` in the payload.** `ctx 38% · cache 92%` is read from there and from
  nowhere else. Without it `_session_strip` is empty, the whole bottom row disappears, and
  the brand collapses onto the last persona row.
* **`ptyrun.py`, and the payload in a FILE because of it.** `charter statusline` reads its
  payload on stdin, and `ptyrun` gives its child no stdin at all — the pty is what the
  child's *output* is on. Piping the payload in instead works only from a terminal, where
  `sys.stderr.isatty()` happens to be true; run the same line from an agent's shell or CI
  and the capture comes back monochrome with nothing to say it did.

**There is no `--compose` here any more.** That flag drew Claude Code's prompt box under
the render, because the status line never appeared alone — it sat directly beneath that
box, and an asset without it was out of the only context the thing was ever seen in. #895
ended that: charter wires no `statusLine`, so this render sits under a Claude Code prompt
nowhere. It was the one **drawing** inside a capture, and there is none left.

`charter statusline` itself is untouched by that and stays a capture here. It is what
opencode's `/charter` pipes, what `--watch` loops, and what the frame's panels are built
out of — so it is kept, kept stamped, and kept in the freshness gate. Un-stamping a live
capture would exempt it from the check forever, which is the failure
`tests/test_asset_freshness.py` exists to prevent.

`personas.svg` comes from the same plane, and needs no payload — it is one command's own
output. `COLUMNS=96` because the roster is narrow; the plane's own `.charter/` state
supplies the active persona, so the caller's session lock and active persona are unset
first or the render shows *your* roster rather than the demo's:

```bash
cd /tmp/demo-plane
env -u CHARTER_WORKSPACE -u CHARTER_PERSONA COLUMNS=96 \
  python3 docs/assets/ptyrun.py charter persona list \
  | python3 docs/assets/ansi2svg.py --title "charter persona list" \
      -o docs/assets/personas.svg
```

The vault rows are the reason `demo-plane.sh` writes three secrets. Without them every row
reads `not created yet`, and the capture shows charter's empty state beside prose about the
credentials it is holding.

**`capture-frame.sh`** is the third capture and the only one that takes no plane of its
own: it builds one, launches a frame against it, and prints the whole screen. One command,
because everything in it has to agree — the plane, the charter that built it and the
charter running the frame are one build, and the window size is the one the panels laid
themselves out for:

```bash
./docs/assets/capture-frame.sh /tmp/frame-capture \
  | python3 docs/assets/ansi2svg.py --title "charter frame" -o docs/assets/frame.svg
```

`COLUMNS` and `LINES` size the window (150×30 by default). 150 for the persona column's
own reason above — below 134 inner columns the sidebar stacks — and 30 because the repo
table is sized to its content and a taller window buys empty harness rows and nothing else.

## Checking a change

SVG text renders with the *viewer's* fonts, so check a regenerated asset in a browser
rather than a desktop preview — macOS Quick Look, in particular, ignores `textLength` and
will show you a broken image that is fine on GitHub.

```bash
"/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" --headless \
  --screenshot=/tmp/check.png --window-size=920,440 file://$PWD/docs/assets/model.svg
```

To check a *frame* of `demo.svg`, do not reach for `--virtual-time-budget` — it does not
advance CSS animations, and every screenshot comes back identically blank. Freeze a phase
with a negative delay instead, and put it **after** the `animation` shorthand, which would
otherwise reset it:

```bash
sed 's/infinite both}/infinite both;animation-delay:-9s}/' docs/assets/demo.svg > /tmp/f.svg
```
