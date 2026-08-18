# README assets

Two kinds of image live here, and the difference matters when one needs updating.

**Captures** are generated from real command output. None of them are mockups and none
should ever be hand-edited — regenerate instead, so a screenshot cannot quietly drift from
what charter actually prints. **Drawings** are authored by hand and say what they mean to
say; they have no source to re-run. **Composed** assets are drawings that embed a capture,
so they inherit the capture's freshness and are regenerated the same way.

| File | Kind | What it is | How to update |
| --- | --- | --- | --- |
| `statusline.svg` | capture | The status line, rendered against the demo plane below | `demo-plane.sh` → `charter statusline` → `ansi2svg.py` |
| `demo.svg` | capture | The quickstart, animated | `capture-demo.sh` → `ansi2svg.py --animate` |
| `personas.svg` | capture | The persona roster, rendered against the demo plane below | `demo-plane.sh` → `charter persona list` → `ansi2svg.py` |
| `model.svg` | drawing | The on-disk model | Edit by hand |
| `social-card.svg` | composed | GitHub's social preview — the image link previews show | `social-card.py` (re-reads `statusline.svg`) |
| `social-card.png` | rendered | `social-card.svg` at 2560×1280 (2:1), for upload | See below; do not edit the PNG |

`social-card.png` is the only asset here that is not used by the repo itself: GitHub stores
the social preview separately, uploaded through **Settings → General → Social preview**,
which has no CLI. The PNG is committed so the upload is reproducible rather than a one-off
that exists only inside a settings page. Regenerate both in order — the SVG re-reads
`statusline.svg`, so a stale status line makes a stale card:

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

**`ptyrun.py`** runs a command with its output on a pseudo-terminal. charter decides
whether to colour its output from `sys.stderr.isatty()`, read once at import time with no
environment override, so a capture taken down a plain pipe comes out monochrome. `script`
does the same job in one word but needs the *parent* to already own a terminal, which an
agent shell or a CI runner does not have.

**`social-card.py`** composes the social preview: the wordmark and tagline over a crop of
`statusline.svg`, embedded as a nested `<svg>` rather than re-rendered, so the card cannot
disagree with the image the README leads with. It used to be a hand drawing that said only
what could never go stale — a wordmark and three nouns — which also meant it never showed
the one thing that actually argues for charter. The bottom fade is load-bearing: the footer
sits on it, and a shallower one leaves the URL unreadable on live terminal rows.

**`demo-plane.sh`** builds a throwaway control plane worth screenshotting: real `charter`
commands, real git repos, real branches and real dirty/unpushed state. Only the *org* is
invented, so a render can show a plausible multi-repo day without exposing anyone's actual
work. Two things it writes directly rather than fetching — `inventory/repos.json` and the
forge-state cache — are the files `charter discover` and `charter gl-refresh` would have
written, in exactly their shape; the demo has no forge to query.

```bash
./docs/assets/demo-plane.sh /tmp/demo-plane
cd /tmp/demo-plane
COLUMNS=150 sh -c 'echo "{\"workspace\":{\"current_dir\":\"$PWD\"},\"model\":{\"display_name\":\"Opus 5\"},\"session_id\":\"demo\",\"context_window\":{\"used_percentage\":38,\"current_usage\":{\"cache_read_input_tokens\":74000,\"cache_creation_input_tokens\":6000}}}" | charter statusline' \
  | python3 docs/assets/ansi2svg.py --title "charter statusline" \
      --compose 'get payments-service onto the new idempotency keys' \
      -o docs/assets/statusline.svg
```

Three things in that command are load-bearing, and the previous capture had none of them:

* **`COLUMNS=150`.** The persona column needs `_LEFT_W + _COL_SEP + _RIGHT_MIN_W` = 134
  inner columns. At 110 the layout correctly falls back to stacking personas as rows, so
  the capture showed charter's narrow-terminal behaviour while the prose described the
  wide one.
* **`context_window` in the payload.** `ctx 38% · ⚡92%` is read from there and from
  nowhere else. Without it `_session_strip` is empty, the whole bottom row disappears, and
  the brand collapses onto the last persona row.
* **`--compose`.** The status line never appears alone — it renders directly beneath
  Claude Code's prompt box. A render without it shows the asset out of the only context it
  is ever seen in. This is the one **drawing** inside a capture; everything above the box
  is still real output.


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
