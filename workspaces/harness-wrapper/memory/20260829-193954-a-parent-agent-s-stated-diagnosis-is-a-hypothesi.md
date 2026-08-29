# A parent agent's stated diagnosis is a hypothesis, not a premise. On cha

_2026-08-29 19:39 · persistent_

A parent agent's stated diagnosis is a hypothesis, not a premise. On charter #656 the brief asserted '#631 withheld BOTH window-style and pane-border-style from the harness when only one caused the box' — git show of the #628 commit proved window-style was never window-scoped at all, so the proposed fix would have been a straight revert of a shipped fix rather than the new thing it claimed to be. Check what a referenced commit ACTUALLY changed before building on a summary of it.
