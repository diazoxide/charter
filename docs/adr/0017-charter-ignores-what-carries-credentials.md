# charter ignores what carries credentials

A charter command that writes into a plane leaves paths behind, and each one raises the
same question: does this get committed? Two rules, drawn where the answer stops being a
matter of taste:

1. **A path charter's own command causes to exist, and which carries credential material,
   charter ignores** — it writes the `.gitignore` line itself and says that it did.
2. **Every other path it leaves behind, charter states and does not decide.** It owes the
   operator the trade-off in its own output; it does not settle a legitimate choice by
   writing a line while nobody is looking.

## Why it is written down

Because it was being derived one site at a time, and the site that skipped it produced
[#278](https://github.com/diazoxide/charter/issues/278) — `charter browser install` left
several paths untracked with nothing said about any of them. The report's own words: *"each
plane picks one, in silence, and the reasoning is nowhere for the next reader."*

Deriving it per site also gets the *facts* wrong, which that report demonstrates. It
described `.playwright-cli/` as "per-machine session state". It is not: the vendor's source
names it `cliOutputDir`, with traces under `.playwright-cli/trace` and snapshots beside
them, while a session lives outside any repo in
`~/Library/Caches/ms-playwright/daemon/<hash>/<name>.session`. The conclusion survived the
correction — ignore it — but only because the rule is about what a path *carries*, and the
real answer there is worse than the reported one.

The rule was already being obeyed elsewhere, which is the signal that it is a rule rather
than a run of coincidences:

- `commands_secrets` refuses a vault file inside the plane that is not gitignored — *"it
  holds credentials"* — rather than trusting the operator to notice.
- `charter init` writes `/.charter/` into the baseline for the same reason, before any
  vault exists to protect.
- The `browser` skill's hard rules already said it in prose: *"never commit a session
  directory or a storage-state file — they carry live cookies, which are the credential in
  another form."* What was missing was not the decision. It was charter acting on a
  decision it had already published — and noticing that a trace is the same category with
  more in it.

## Where the line falls, and why there

Credential material is not a matter of preference. A Playwright trace records the network:
requests with their headers and bodies, and DOM snapshots of the pages that produced them.
A trace taken during a `charter secret exec` login therefore holds the login POST — the one
thing the credential bridge exists to keep out of reach — and committing it puts that in
everyone's clone and in the forge's history, recoverable long after a `git rm`. There is no
plane for which that is the right answer, so charter does not ask.

Generated content and configuration are the opposite. `.claude/skills/playwright-cli/` has
two defensible postures — committed, and a fresh clone works with no `npx` round trip, at the cost of a
tree a later `install` silently rewrites; or untracked, and the tree stays honest at the
cost of a generator run per clone. `.playwright/cli.config.json` is a project config file,
which many teams would reasonably commit. Charter has no standing to pick either, and a
`.gitignore` line written quietly *is* picking. Naming both costs in the command's own output is the whole
obligation, and it discharges the part of #278 that actually recurs: not the missing line,
but the missing reasoning.

## Consequences

- One writer: `util.append_gitignore` — append-only, idempotent, whole-line. Rewriting the
  file is not available to any caller, because `workspace.set_live()` splices its managed
  block at the literal anchor `!/workspaces/.gitkeep`.
- Charter reports the line it wrote, not the line it intended to write
  ([ADR 0013](0013-success-is-checked-divergence-is-named.md)).
- The vendor keeps its own half. `.playwright-cli/` is `@playwright/cli`'s directory, and
  the durable fix is upstream in the tool that writes it; charter's line is a stopgap for
  the operator standing in front of charter rather than in front of npm.
- **Check the path, do not take its description on trust.** The row here was nearly written
  against a claim about the directory that the vendor's own source contradicts. A rule about
  what a path carries is only as good as knowing what is in it.
- This does not license charter to manage a plane's `.gitignore` generally. The trigger is
  narrow — charter's command created the path, and the path carries credentials. A path
  that fails either half gets a sentence, not a write.
