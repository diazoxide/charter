# A news entry's headline: wrapped in YAML-style single quotes ships those

_2026-09-04 17:50 · persistent_

A news entry's headline: wrapped in YAML-style single quotes ships those quotes into the rendered release-note heading — charter's frontmatter is flat key: value (persona.parse), not YAML, so nothing unquotes it. 0.56.0 published six such headlines unnoticed. #902 moved the check into code, so the pre-tag grep is no longer yours to remember: the suite asks it of every entry (tests/test_news_frontmatter_is_not_yaml.py) and `charter news --for <version>` — the release guard — refuses a version whose entries quote a value. Fix the entry, never the rendered notes. The six 0.56.0 entries are deliberately NOT corrected: they are what the published Release says, and quoted_values still reports them, so `charter news --for 0.56.0` refuses on purpose.
