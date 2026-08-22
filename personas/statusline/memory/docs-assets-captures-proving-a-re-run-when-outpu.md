# docs/assets captures, proving a re-run when output is byte-identical: on

_2026-08-22 17:23 · persistent_

docs/assets captures, proving a re-run when output is byte-identical: only statusline.svg carries a version banner, and social-card.png does NOT — social-card.py crops the embedded render at CROP=296.0 while the banner sits at y=339.10, so a version-only bump leaves the PNG byte-identical and that is correct, not a failed rasterize. Usable evidence for the other captures: ansi2svg prints '<n> lines, <m> cols' as it rewrites the file wholesale, mtimes move, and capture-demo.sh genuinely hits the network (charter discover queries the org, charter clone clones the real repo). Keep the raw .ansi captures as the artifact that a capture executed. At 0.48.0 (a security release touching tui.sanitize) the entire three-capture diff was one <text> element: 0.47.1 -> 0.48.0.
