# A state marker is only as good as its READERS, and 'written since releas

_2026-09-08 11:27 · persistent_

A state marker is only as good as its READERS, and 'written since release X' is not evidence anyone reads it. charter/frame/state.record_closed has existed since #796; state.was_closed was consulted in exactly ONE place ever (leave.plan, so a quit does not resurrect a closed chat) while chats._by_workspace — the single walk that of_workspace, counts_by_workspace and touched_by_workspace all are — never asked. So a closed chat stayed a clickable tab that answered 'no window any more' forever. When adding a marker, grep its reader count; when a UI 'does nothing', check whether the write happened and only the read is missing.
