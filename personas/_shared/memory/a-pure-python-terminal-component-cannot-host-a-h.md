# A pure-Python terminal component cannot host a harness at usable speed,

_2026-08-22 00:11 · persistent_

A pure-Python terminal component cannot host a harness at usable speed, measured 2026-08-21 on darwin/Python 3.14.4: pyte 0.8.2 parses 2.4 MB/s (0.9 with HistoryScreen) and a Textual 8.2.8 + pyte frame pushes 1.85 MB/s end to end, against tmux 3.7c at 25.2 MB/s for the same corpus in the same 150x42 frame — a 13 MB build log is ~7s frozen versus 0.51s. Rendering was NEVER the bottleneck (7.2 ms/frame, 138 fps ceiling); VT parsing is. Both arms rendered claude and opencode correctly, so this is a cost fact, not a feasibility one. This is why ADR 0018 has tmux compose the frame and charter own no terminal emulation.
