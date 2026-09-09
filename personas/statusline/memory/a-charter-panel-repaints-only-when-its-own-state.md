# A charter panel repaints ONLY when its own state.version(fid) bumps (pan

_2026-09-08 11:27 · persistent_

A charter panel repaints ONLY when its own state.version(fid) bumps (panel.TICK=0.2s is just the stat interval). Changing another chat's state — closing it, say — moves no version but that chat's own, so every OTHER chat's strip stays stale indefinitely; measured 7s+ with correct on-disk data and it corrected only when an unrelated click woke the panel. Any change to what a sibling strip should draw must bump the siblings (state.bump) or the plane is right and the screen is wrong.
