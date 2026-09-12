# docs/frame.md is scanned whole by test_frame_pane_style's two prose read

_2026-09-12 20:11 · persistent_

docs/frame.md is scanned whole by test_frame_pane_style's two prose readers, and both are about NUMERALS rather than about `pad`: stated_ranges reads any <n>-<n> as a range of cells, so a hyphenated date like 2026-09-12 anywhere in the file is a false 'range (2026, 9)'; and pad_numbers reads one sentence either side of every pad word over the FLATTENED file, while TheReaderIsAttackedWithTheSpellingsThatBeatTheLastOne appends its corpus sentence to the END — so the file's last sentence is always in scope, and ending it with 'ADR 0018](adr/0018-…)' reads as a pad of 18. Measured 2026-09-12 on PR #996: 25 red tests, all from one added paragraph.
