#!/usr/bin/env python3
"""Turn captured ANSI terminal output into a self-contained SVG "screenshot".

The README's terminal images are generated, not drawn: a real command's real bytes
go in, an SVG comes out. That is the whole point — a hand-mocked screenshot drifts
from the tool the first time an alignment changes, and nobody notices.

Stdlib only, so regenerating an asset never needs a toolchain:

    charter statusline < payload.json | python3 docs/assets/ansi2svg.py \\
        --title "charter statusline" -o docs/assets/statusline.svg

Columns are measured with the same rule ``charter/tui.py`` uses (East-Asian W/F
counts as two cells), so a run lands on the column charter believed it was writing
to. Each run is drawn with an explicit ``textLength``, which pins the grid even when
the viewer substitutes a font whose advance width isn't exactly 0.6em.
"""

from __future__ import annotations

import argparse
import re
import sys
import unicodedata

# GitHub's dark canvas, so the image reads as a terminal on either site theme.
BG = "#0d1117"
CHROME = "#161b22"
BORDER = "#30363d"
FG = "#c9d1d9"

# ANSI 30-37 / 90-97. Tuned to be legible on BG rather than to match any one terminal.
PALETTE = {
    30: "#6e7681", 31: "#ff7b72", 32: "#3fb950", 33: "#d29922",
    34: "#58a6ff", 35: "#bc8cff", 36: "#39c5cf", 37: "#b1bac4",
}
PALETTE.update({k + 60: v for k, v in PALETTE.items()})

FONT = ("ui-monospace, SFMono-Regular, 'SF Mono', Menlo, Consolas, "
        "'DejaVu Sans Mono', monospace")
FONT_SIZE = 13.0
CHAR_W = FONT_SIZE * 0.6
LINE_H = FONT_SIZE * 1.55
PAD_X, PAD_Y = 18.0, 14.0
TITLEBAR = 30.0

SGR = re.compile(r"\x1b\[([0-9;]*)m")
OTHER_ESC = re.compile(r"\x1b\[[0-9;?]*[A-Za-z]")


def cell_width(s: str) -> int:
    """Display columns, by the same rule charter's TUI applies when it lays out."""
    n = 0
    for ch in s:
        if unicodedata.combining(ch) or unicodedata.category(ch) in ("Mn", "Cf"):
            continue
        n += 2 if unicodedata.east_asian_width(ch) in ("W", "F") else 1
    return n


class Style:
    __slots__ = ("fg", "bold", "dim", "underline")

    def __init__(self):
        self.fg, self.bold, self.dim, self.underline = None, False, False, False

    def copy(self) -> "Style":
        s = Style()
        s.fg, s.bold, s.dim, s.underline = self.fg, self.bold, self.dim, self.underline
        return s

    def apply(self, params: str) -> None:
        codes = [int(p or 0) for p in params.split(";")] if params else [0]
        for c in codes:
            if c == 0:
                self.fg, self.bold, self.dim, self.underline = None, False, False, False
            elif c == 1:
                self.bold = True
            elif c == 2:
                self.dim = True
            elif c == 4:
                self.underline = True
            elif c in (22, 21):
                self.bold = self.dim = False
            elif c == 24:
                self.underline = False
            elif c == 39:
                self.fg = None
            elif c in PALETTE:
                self.fg = PALETTE[c]


def parse(text: str) -> list[list[tuple[str, Style]]]:
    """ANSI text -> per-line lists of (run, style). Style carries across lines."""
    lines, style = [], Style()
    for raw in text.replace("\r\n", "\n").rstrip("\n").split("\n"):
        raw = raw.replace("\t", "    ")
        runs, pos = [], 0
        for m in SGR.finditer(raw):
            chunk = raw[pos:m.start()]
            if chunk:
                runs.append((chunk, style.copy()))
            style.apply(m.group(1))
            pos = m.end()
        tail = raw[pos:]
        if tail:
            runs.append((tail, style.copy()))
        lines.append([(OTHER_ESC.sub("", t), s) for t, s in runs if OTHER_ESC.sub("", t)])
    return lines


def esc(s: str) -> str:
    return s.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")


#: Seconds of dwell before a line appears, by what kind of line it is. A prompt gets a
#: beat before it the way a person pauses before typing; output pours out.
DWELL_PROMPT = 0.95
DWELL_OUTPUT = 0.10
DWELL_BLANK = 0.30
DWELL_TAIL = 3.0     # hold the finished frame before the loop restarts


def schedule(lines) -> tuple[list[float], float]:
    """(reveal time per line, total duration) for the animated form."""
    times, t = [], 0.0
    for runs in lines:
        text = "".join(r for r, _ in runs)
        if not text.strip():
            t += DWELL_BLANK
        elif text.lstrip().startswith("❯"):
            t += DWELL_PROMPT
        else:
            t += DWELL_OUTPUT
        times.append(t)
    return times, t + DWELL_TAIL


def render(lines, title: str | None, animate: bool = False) -> str:
    cols = max((sum(cell_width(t) for t, _ in ln) for ln in lines), default=0)
    top = PAD_Y + (TITLEBAR if title is not None else 0)
    w = cols * CHAR_W + PAD_X * 2
    h = len(lines) * LINE_H + top + PAD_Y

    out = [
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{w:.0f}" height="{h:.0f}" '
        f'viewBox="0 0 {w:.2f} {h:.2f}" role="img" '
        f'aria-label="{esc(title or "terminal output")}">',
        f'<rect x="0.5" y="0.5" width="{w - 1:.2f}" height="{h - 1:.2f}" rx="9" '
        f'fill="{BG}" stroke="{BORDER}"/>',
    ]
    if title is not None:
        out.append(
            f'<path d="M0.5 9.5a9 9 0 0 1 9-9h{w - 19:.2f}a9 9 0 0 1 9 9v{TITLEBAR - 9:.2f}'
            f'h-{w - 1:.2f}z" fill="{CHROME}" stroke="{BORDER}"/>'
        )
        for i, c in enumerate(("#ff5f57", "#febc2e", "#28c840")):
            out.append(f'<circle cx="{18 + i * 16}" cy="{TITLEBAR / 2:.1f}" r="5" fill="{c}"/>')
        out.append(
            f'<text x="{w / 2:.2f}" y="{TITLEBAR / 2 + 4:.1f}" text-anchor="middle" '
            f'font-family="{FONT}" font-size="11" fill="#8b949e">{esc(title)}</text>'
        )

    if animate:
        times, total = schedule(lines)
        css = [f".ln{{animation:{total:.2f}s linear infinite both}}"]
        for i, t in enumerate(times):
            p = 100.0 * t / total
            # The two stops must be at *different* percentages. Declare the same
            # percentage twice and the later rule simply wins, so `0%,p{opacity:0}`
            # followed by `p,100%{opacity:1}` leaves one keyframe at p — and the line
            # ramps smoothly from 0 to 1 across the whole animation instead of cutting
            # in. A hundredth of a percent is invisible and restores the step.
            off = max(0.0, p - 0.01)
            css.append(f"@keyframes k{i}{{0%,{off:.3f}%{{opacity:0}}"
                       f"{p:.3f}%,100%{{opacity:1}}}}")
            css.append(f".l{i}{{animation-name:k{i}}}")
        # A renderer that ignores CSS animation shows every line at full opacity, which
        # is the finished frame — a worse asset than the animation, never a broken one.
        out.append("<style>" + "".join(css) + "</style>")

    out.append(f'<g font-family="{FONT}" font-size="{FONT_SIZE}">')
    for row, runs in enumerate(lines):
        y = top + row * LINE_H + FONT_SIZE
        col = 0
        if animate:
            out.append(f'<g class="ln l{row}">')
        for text, st in runs:
            # Whitespace is never drawn — it is the gap between two positioned chunks.
            # Emitting it inside a <text> put the layout at the mercy of the renderer's
            # xml:space handling, which is what collapsed "todo 2" into "todo2".
            for chunk in re.finditer(r"\S+", text):
                start = col + cell_width(text[:chunk.start()])
                # One x per glyph rather than a textLength over the run. textLength is
                # the tidier attribute and browsers honour it, but not every SVG
                # rasterizer does — and one that ignores it silently walks the whole
                # line off the right edge. An explicit x per character cannot drift.
                xs, c = [], start
                for ch in chunk.group():
                    xs.append(f"{PAD_X + c * CHAR_W:.2f}")
                    c += cell_width(ch)
                attrs = [f'x="{" ".join(xs)}"', f'y="{y:.2f}"',
                         f'fill="{st.fg or FG}"']
                if st.bold:
                    attrs.append('font-weight="600"')
                if st.dim:
                    attrs.append('opacity="0.55"')
                if st.underline:
                    attrs.append('text-decoration="underline"')
                out.append(f'<text {" ".join(attrs)}>{esc(chunk.group())}</text>')
            col += cell_width(text)
        if animate:
            out.append("</g>")
    out.append("</g></svg>")
    return "\n".join(out) + "\n"


def _width_of(lines) -> int:
    return max((sum(cell_width(t) for t, _ in ln) for ln in lines if ln), default=0)


def compose_box(text: str, width: int) -> str:
    """Claude Code's prompt box, as ANSI, sized to match the capture beneath it.

    Rendered through `parse` like any other captured line rather than emitted as SVG
    directly: the box then inherits the same glyph metrics, colour handling and animation
    schedule as everything else, so it cannot drift into its own layout.

    Rounded corners because that is what Claude Code draws, and the point of the box is
    recognition — a reader should see where the status line actually sits without being
    told. It is the one part of these assets that is a DRAWING rather than a capture, so it
    says something charter does not print; `docs/assets/README.md` keeps that distinction
    and this is deliberately on the drawing side of it.
    """
    DIM, RESET, CYAN = "\033[2m", "\033[0m", "\033[36m"
    inner = max(10, width - 2)
    body = text if cell_width(text) <= inner - 4 else text[:inner - 5] + "…"
    pad = inner - 2 - cell_width(body)
    return (f"{DIM}╭{'─' * inner}╮{RESET}\n"
            f"{DIM}│{RESET} {CYAN}>{RESET} {body}{' ' * max(0, pad - 2)} {DIM}│{RESET}\n"
            f"{DIM}╰{'─' * inner}╯{RESET}\n"
            f"\n")


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("-o", "--output", help="write here instead of stdout")
    ap.add_argument("--title", default=None,
                    help="window title bar text; omit for a bare frame")
    ap.add_argument("--input", help="read here instead of stdin")
    ap.add_argument("--animate", action="store_true",
                    help="reveal the capture line by line, looping")
    ap.add_argument("--compose", metavar="TEXT", default=None,
                    help="draw Claude Code's input box above the capture, with TEXT in it. "
                         "The status line renders directly beneath that box in a real "
                         "session, and a render that omits it shows the asset out of the "
                         "only context it ever appears in.")
    a = ap.parse_args()

    raw = (open(a.input, encoding="utf-8").read() if a.input
           else sys.stdin.read())
    lines = parse(raw)
    if a.compose:
        lines = parse(compose_box(a.compose, _width_of(lines))) + lines
    if not lines:
        print("ansi2svg: no input", file=sys.stderr)
        return 1

    widths = {sum(cell_width(t) for t, _ in ln) for ln in lines if ln}
    # Only a *boxed* render is supposed to be uniform, and a ragged one there means the
    # capture was truncated by a narrow COLUMNS. A transcript is ragged by nature, so
    # checking it would cry wolf on every demo.
    first = "".join(t for t, _ in lines[0]) if lines[0] else ""
    if first.lstrip().startswith(("┌", "╭", "┏")) and len(widths) > 1:
        print(f"ansi2svg: warning — boxed render has uneven line widths {sorted(widths)}; "
              "the frame will look ragged (was COLUMNS too small?)", file=sys.stderr)

    svg = render(lines, a.title, animate=a.animate)
    if a.output:
        with open(a.output, "w", encoding="utf-8") as f:
            f.write(svg)
        print(f"{a.output}  ({len(lines)} lines, {max(widths)} cols)", file=sys.stderr)
    else:
        sys.stdout.write(svg)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
