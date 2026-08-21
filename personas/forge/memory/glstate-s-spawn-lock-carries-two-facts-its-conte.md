# glstate's spawn lock carries TWO facts: its content is the pid of an in-

_2026-08-21 23:18 · persistent_

glstate's spawn lock carries TWO facts: its content is the pid of an in-flight refresh (empty when none), its mtime is when that last changed. maybe_spawn suppresses while that pid is alive (at most one refresh in flight ever); refresh() calls _mark_done() so the cooldown runs from COMPLETION not spawn; STUCK_AFTER=900s replaces a presumed-wedged refresh and stops a recycled pid suppressing forever. Do not simplify back to a bare touch() after Popen — that is the #324 defect.
