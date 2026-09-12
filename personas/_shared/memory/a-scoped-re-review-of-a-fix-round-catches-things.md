# A scoped re-review of a fix round catches things the fix round introduce

_2026-09-10 16:15 · persistent_

A scoped re-review of a fix round catches things the fix round introduces. On 2026-09-10 a wording-only round on PR 943 replaced a vague comment with a specific FALSE one: it claimed stop and subagentstop reach the frame gather via notify.plane_changed, which hooks.py and inflight.py explicitly deny in their own docstrings (the absence is the design working). The implementer had verified seven call sites and still got two wrong; only the re-review that RE-ENUMERATED the call sites itself caught it. Never accept a fix round on the implementer's account of it, however small the diff.
