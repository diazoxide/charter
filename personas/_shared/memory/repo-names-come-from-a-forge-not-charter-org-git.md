# Repo names come from a FORGE, not charter: org/.github is real and commo

_2026-08-21 18:38 · persistent_

Repo names come from a FORGE, not charter: org/.github is real and common, and both workspace.valid_name and persona.valid_name reject it (leading dot). Never reuse valid_name to contain a repo name read from a file — use charter/contain.py's permissive segment_ok. valid_name is the creation-time ergonomic rule; containment is a different question.
