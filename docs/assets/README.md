# README assets

Every image in this directory is **generated from real command output**. None of them are
mockups, and none should be hand-edited — regenerate instead, so a screenshot can never
quietly drift from what charter actually prints.

| File | What it is | How to regenerate |
| --- | --- | --- |
| `statusline.svg` | The status line, rendered against the demo plane below | `demo-plane.sh` → `charter statusline` → `ansi2svg.py` |
| `demo.svg` | The quickstart, animated | `capture-demo.sh` → `ansi2svg.py --animate` |
| `model.svg` | The on-disk model, hand-drawn | Edit by hand — it is a diagram, not a capture |

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

**`demo-plane.sh`** builds a throwaway control plane worth screenshotting: real `charter`
commands, real git repos, real branches and real dirty/unpushed state. Only the *org* is
invented, so a render can show a plausible multi-repo day without exposing anyone's actual
work. Two things it writes directly rather than fetching — `inventory/repos.json` and the
forge-state cache — are the files `charter discover` and `charter gl-refresh` would have
written, in exactly their shape; the demo has no forge to query.

```bash
./docs/assets/demo-plane.sh /tmp/demo-plane
cd /tmp/demo-plane
COLUMNS=110 sh -c 'echo "{\"workspace\":{\"current_dir\":\"$PWD\"},\"model\":{\"display_name\":\"Opus 5\"},\"session_id\":\"demo\"}" | charter statusline' \
  | python3 docs/assets/ansi2svg.py --title "charter statusline" -o docs/assets/statusline.svg
```

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
