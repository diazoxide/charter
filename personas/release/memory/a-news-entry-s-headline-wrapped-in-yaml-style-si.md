# A news entry's headline: wrapped in YAML-style single quotes ships those

_2026-09-04 17:50 · persistent_

A news entry's headline: wrapped in YAML-style single quotes ships those quotes into the rendered release-note heading — charter's frontmatter is flat key: value (persona.parse), not YAML, so nothing unquotes it. 0.56.0 published six such headlines unnoticed. Before tagging, grep "^headline: '" docs/news/<version>-*.md and fix the entry, never the rendered notes. Tracked as #902.
