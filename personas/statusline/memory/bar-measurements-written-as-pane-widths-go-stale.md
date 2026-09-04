# Bar measurements written as PANE WIDTHS go stale the moment the lead cha

_2026-09-04 12:26 · persistent_

Bar measurements written as PANE WIDTHS go stale the moment the lead changes (a heading removed, an inset resized). Anchor them to 'room' — width minus lead — which is what slots._cuts actually takes, and they stay true across any lead change. #880 found five stale numbers in one comment this way.
