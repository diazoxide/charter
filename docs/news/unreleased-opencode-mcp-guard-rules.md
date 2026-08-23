---
version: unreleased
headline: An MCP guard rule now fires under opencode, and says what else it decides
---

`charter guard ask 'mcp__slack__send'` wrote the right rule for Claude Code and, for
opencode, `{"permission": {"bash": {"mcp__slack__send": "ask"}}}` — a rule over a *bash
command* literally named `mcp__slack__send`, which nothing can ever run. It printed a tick,
committed the file, and the operator was never prompted. That is 0.49.0's `Bash(mcp__…)`
defect one harness over; it survived that fix because the fix landed in `commands._as_rule`,
which opencode does not go through.

There was an honest way to write nothing here — `Harness.apply_ask_rule` returns
`unsupported` for a harness that cannot express a pattern, and says why — and it was not
available, because opencode *can* express this. Only the name was wrong. opencode registers
every MCP tool as `sanitize(server) + "_" + sanitize(tool)` and asks for permission under
exactly that id, so `mcp__slack__send` is `slack_send` there. `Permission.evaluate`
glob-matches the permission name as well as the pattern, so a whole server is `slack_*`.
Both are what charter now writes.

**The new rule can also decide things you did not name, and `guard` now says so.** opencode
keeps its own permissions in the same flat namespace as the MCP tool ids, resolves the LAST
matching rule, and resolves config after its own defaults. So `charter guard allow mcp__plan`
writes `plan_*`, which matches opencode's built-in `plan_enter` and `plan_exit` — both of
them `deny` — and outranks them. `charter guard` prints a warning naming the built-ins the
rule reaches and, where a narrower form exists, what to type instead. It is a warning rather
than a refusal: the rule is still the one you asked for, and refusing the whole-server form
for opencode alone would give one operator sentence two spellings across harnesses.

The whole-server glob stays as tight as opencode's own names allow and no tighter. `_` is
both the separator and a legal character either side of it, so `slack_*` covers a server
called `slack_admin` too, and no glob can tell them apart.

Verified against opencode 1.18.21 rather than inferred: `opencode debug agent build` prints
the resolved rule list, and with `plan_*` in `opencode.json` it shows `plan_enter` and
`plan_exit` denied at indices 9 and 10 and `plan_*` allowed at 17 — last match wins.

Found alongside the 0.49.0 authority audit (#374).
