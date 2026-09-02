---
version: unreleased
headline: A new chat no longer inherits the last chat's workspace lock
---

*"I launched with `--workspace alpha`, and `charter workspace use alpha` told me the
session was locked to `gamma`."*

## What was happening

A chat id is `<workspace>.<n>`, and `n` is **allocated, not minted**: charter hands out the
lowest free ordinal, and an ordinal is free the moment the chat's state is reaped. So the
"new" chat you open after closing one very often has the same *name* as the one you closed.

Everything charter keys on a session id lives in `.charter/sessions/<id>.*`, and inside a
frame the frame **is** the charter session — so that `<id>` is the chat id. Reaping removed
`.charter/frame/<id>/` and nothing else, which meant the next tenant of the ordinal
inherited the previous one's pointer, its lock, its persona selection and its tool-gate
markers.

Measured, reproduced without tmux — one chat that selected `gamma`, reaped, then relaunched
onto the same ordinal:

```
is_locked      : gamma
for_session    : gamma
resolve        : gamma          <- what every `charter` command in that shell acts on
set_active a   : locked         <- `charter workspace use alpha`, refused
```

The release before this one closed the visible half (#794): the panels draw `alpha` and the
chat belongs to `alpha`, because membership stopped reading that pointer. What that left was
arguably worse to be on the receiving end of — the panels and the commands disagreed, and
the refusal named a lock nobody in that chat had set.

## What changed

Reaping a chat now removes `.charter/sessions/<chat id>.*` along with
`.charter/frame/<chat id>/`.

**The sweep is the id prefix, not a list of suffixes**, and that is a lesson this codebase
already paid for once: `workspace._prune` enumerated five marker families, drifted three
times, and was replaced by "every file in the directory" (#366). Eight families are keyed on
a session id today, across five modules — the workspace pointer and the lock, the persona
pointer, the usage record, the tool-gate's two files, and three of the hooks' markers — and
a ninth written next year is covered the day it is written rather than the day somebody
remembers this function.

Nothing else moves. The match is anchored and dot-terminated, so `alpha.1` does not touch
`alpha.10`'s files or `xalpha.1`'s (a real collision: chat ids have no boundary character
between the workspace and the dot, so one workspace's name can end with another's). A live
chat keeps its selection, a chat on another tmux server keeps its selection, and a harness
session outside a frame — keyed by its own id — is never in the sweep at all.

## The alternative, and why not

#731 offered a second fix: let the launcher's `--workspace` outrank a pointer it did not
write. That only ever helps a launch carrying the flag, and bare `charter` is now the
ordinary way in — and it leaves the lock, which no flag addresses, exactly where it is.
PR #757's body named this fix instead, and this is that.

## Verification

Twelve cases, no tmux — the defect reproduces on two directories. Six were red before the
change; the other six are the blast radius, and each was confirmed to fail against a
deliberately over-wide implementation rather than assumed to be measuring something:

- dropping the `.` from the prefix reddens the `alpha.10` case;
- `startswith` → `in` reddens the `xalpha.1` case;
- hoisting the sweep out of the keep-rules reddens both the live-chat and other-server cases;
- both `except OSError` clauses are entered by patching the exact call that raises, and each
  turns red when its exception type is changed.

Nothing to adopt.
