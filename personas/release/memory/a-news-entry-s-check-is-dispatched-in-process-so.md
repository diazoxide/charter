# A news entry's `check:` is dispatched in-process, so a `check:` naming a

_2026-08-20 23:19 · persistent_

A news entry's `check:` is dispatched in-process, so a `check:` naming a command that itself probes (`doctor`, `news --pending`) used to re-enter without bound — dormant while `unreleased`, armed by `charter news stamp` inside the bump PR. Since #313 a probe never runs from inside a probe and such an entry reports `unknown`, never adopted; the guard is process-local, so the cross-process case (`charter update` -> _handoff -> `charter news --since`) is still open as #314.
