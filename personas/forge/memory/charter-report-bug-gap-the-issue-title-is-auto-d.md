# charter report bug/gap: the issue TITLE is auto-derived from the body's

_2026-09-10 23:37 · persistent_

charter report bug/gap: the issue TITLE is auto-derived from the body's first line (stripped of any ATX heading marker, 72-char cap) — do not pass a separate title. The version/OS footer line is auto-appended by report.render() from recorded metadata — do not hand-type it into the body, it duplicates. report.scrub() redacts every local workspace and persona name found in the body by exact word match (e.g. 'chat-handoff', 'forge', 'release', 'statusline', 'steward' on this machine) and replaces it with '[workspace]'/'[persona]' — preview with report.scrub(text) before filing, since a hit mid-sentence reads oddly and the ordinary English word can collide.
