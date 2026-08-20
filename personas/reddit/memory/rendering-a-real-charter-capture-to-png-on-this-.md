# Rendering a real charter capture to PNG on this Mac (no rsvg-convert/cai

_2026-08-20 00:25 · persistent_

Rendering a real charter capture to PNG on this Mac (no rsvg-convert/cairosvg/magick): pipe the command through docs/assets/ptyrun.py (git needs --no-pager or ptyrun hangs on the pager), pipe the ANSI to docs/assets/ansi2svg.py, then sed the SVG's width/height/viewBox to a SQUARE with the content vertically centred (e.g. height=W, viewBox='0 -(W-H)/2 W W') before 'qlmanage -t -s <2W> -o . f.svg' — Quick Look always emits a square and clips the right edge otherwise — and finally 'sips -c H W' to crop back to the content box.
