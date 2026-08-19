---
version: unreleased
headline: `persona lint` can see the skill `browser install` just wrote
check: persona lint
---

Two charter commands disagreed about what a skill is.

`charter browser install` writes `.claude/skills/playwright-cli/` — that path chosen because
the harness reads project skills from there. `charter persona lint` walked only
`~/.claude/plugins`, so a persona declaring `skills: playwright-cli` was told the skill was
*"not installed here … Remove it or install the plugin"*.

That remedy could not be followed. There is no plugin to install: charter deliberately does
not vendor Playwright's pages, which is the entire reason `browser install` exists. The only
way out was to drop the declaration — and then nothing preloads the one skill charter told
you to install.

Lint now resolves a declared skill against all three places the harness reads from:
`~/.claude/plugins`, `~/.claude/skills`, and this plane's `.claude/skills`. When a skill
genuinely is missing, the error names all three rather than sending you after a package that
may not exist.

Unchanged: with no plugin cache at all, the check still stays silent. Charter cannot see
plugin-provided skills there, so checking anyway would report every one of them as missing —
confidently wrong, which is worse than quiet.
