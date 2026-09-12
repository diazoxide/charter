# A deliberate project-tracking issue (a feature's umbrella issue, checkli

_2026-09-10 23:17 · persistent_

A deliberate project-tracking issue (a feature's umbrella issue, checklist, cross-refs to known issue/PR numbers) goes through gh issue create --body-file, never charter report gap: report gap's scrub() replaces any text matching a workspace/persona/vault name with a placeholder, and a feature named after its own workspace (e.g. chat-handoff) would have that name redacted out of its own doc-path links. report gap is for a Reporter's own incidental, unstructured discovery under ADR 0003 consent, not curated authorship. Label 'gap' (capability charter lacks) can still be applied by hand on a gh-created issue; never apply 'via-charter-report' unless report send actually sent it.
