---
version: unreleased
headline: opencode now runs every guard charter ships, starting with the one that refuses to read a vault
---

Under opencode, `cat .charter/vaults/devops.json` was denied — with a well-worded refusal
that **named the path**. The `read` tool on that same path succeeded and printed every
plaintext credential into the transcript. The guard handed over the target.

That is #90 verbatim, one harness over, and the reason is one line of the generated plugin.
It forwarded every tool call to `charter hook pretooluse` on the principle that "every
decision stays in Python, where it has tests" — but `pretooluse` is the *Bash* guard. It
reads `tool_input.command` and never looks at the tool name. The file-reading guard is a
different handler (`pretooluse-read`), and nothing on this harness ever called it. Claude
Code's `hooks/hooks.json` has registered a `Read|Grep` matcher for it since 0.44.0; opencode
had no equivalent, and `charter harness list` — the mechanism charter offers for exactly
this question — did not print the gap, because nothing had declared it.

**The plugin now routes by tool, from a table in Python that the shim is generated from.**
`read`/`grep` reach the vault-read guard, `write`/`edit` the edit guard, `task` the dispatch
recorder, and anything else falls to the Bash guard as the catch-all. The same move on the
other side of the tool call: `bash`, `write`/`edit`, `skill` and `task` each reach the
`PostToolUse` handler the manifest names for them, where before all four went to one — so
the skill and dispatch tallies were silently empty on this harness, and the "you just wrote
a secret into committed memory" warning never fired.

**Routing was only half of it. The argument names are the harness's, not charter's.**
opencode's `read` takes `filePath`, not `file_path` — read off the running server's own
`/experimental/tool` schema at 1.18.21 rather than guessed — and its `write` and `edit` do
the same, while its `skill` calls the skill `name` rather than `skill`. Routing the call to
the right guard and then looking up a key that is never present would have shipped a guard
that ran, decided nothing, and looked wired. charter now reads both spellings everywhere it
reads a path, additively, so nothing changes for Claude Code.

**A `deny` was always carried; an `ask` never was, and that is now declared.** opencode's
`tool.execute.before` can allow or throw, and throwing is what denial IS — so the vault
guard, the one-credential rule and the containment rule all refuse here exactly as they do
under Claude Code. What has no spelling is the middle answer. charter's own tool-time asks —
the routing nudge and the overlapping-dispatch nudge — allow and are not shown, and
`charter harness list` and `charter doctor` now print that as the `ask-decisions` ceiling
rather than leaving you to find it. (`charter guard ask` is unaffected: those rules land in
`opencode.json` and opencode prompts for them itself.)

The test that should have caught this read `hooks/hooks.json` — one harness's answer — and
concluded the guard was "actually wired". A test that reads the *generated shim text* and
diffs it against the manifest is not enough either, and an adversarial review of the first
version of this fix is what showed it: put the literal `charter hook pretooluse` back at the
call site and leave the routing table sitting three lines above it, unread, and every
assertion still passed. A table present but ignored is precisely the state that shipped for
four releases. So the suite now pins the **call site**: the reachability check resolves each
`charter hook` through the constants that call site actually names, and — wherever Bun or
Node is installed — the shim is *executed*, against a stand-in for Bun's `$` that records
the command line it builds, and asked which handler each tool really reached. A handler
added tomorrow fails the suite until opencode routes it or somebody writes down why it
cannot; a handler unwired tomorrow fails it the same day.

**To adopt it: update charter.** The plugin is stamped with the version that wrote it, so
`charter init` / `charter update` replaces it. A plugin you edited yourself is left alone
and reported, as always — move it aside and run `charter reinit` to take this.

Found in the 2026-08-24 security audit (#433).
