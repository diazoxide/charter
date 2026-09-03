---
version: unreleased
headline: charter's hooks stop writing into repositories that are not control planes — outside a plane the plugin now says nothing, writes nothing and grants nothing, while the vault-leak and live-substitution guards keep refusing
---

`config.HAS_CONTROL_PLANE` gated five places in `charter/hooks.py`: the A2/A3/A3b/A4
denials inside `pretooluse` and `_state_write_reason`. ADR 0015 raises the objection this
answers — *"a plugin installed for every project does run charter's hooks in repos with no
control plane"* — and answers it with a promise: *"the guards gate on
`config.HAS_CONTROL_PLANE` and stay silent outside a plane."* Six of the eleven handlers in
`hooks._HANDLERS` had never heard of the flag.

Outside a plane `config.STATE_DIR` is `<cwd>/.charter`, so what those handlers wrote, they
wrote into whatever repository you happened to be standing in. Measured on 0.55.0, each
handler run once as a real subprocess in an ordinary git repository with no `charter.toml`:

| handler | left behind |
| --- | --- |
| `sessionstart` | `.charter/sessions/<sid>.tools`, `.charter/sessions/<sid>.gate` |
| `userpromptsubmit` | `.charter/sessions/<sid>.configver` |
| `pretooluse` | `.charter/guard-seen.json` |
| `pretooluse-dispatch` | `.charter/dispatch-inflight/<agent>.<id>.json` |
| `posttooluse` | `.charter/sessions/<sid>.memnudge` |
| `posttooluse-skill`, `posttooluse-message` | the persona tallies |
| `posttooluse-dispatch` | `personas/_dispatch/<month>.<hostname>.jsonl`, `.charter/agent-personas.json`, `.charter/persona-state/trace/<sid>.jsonl` |

`git status` then read `?? .charter/` — and, for the dispatch tally, `?? personas/` at a
path no `.gitignore` anywhere covers, carrying the machine's hostname into somebody else's
checkout. `charter/harness/opencode.py` already states the rule this broke, about planes:
*"A plane is somebody's repo, and charter's housekeeping has no business in its `git
status`."* It is no less true one level out.

Two more, neither of them in the report.

**charter spoke.** `SessionStart` injected *"Confirm the workspace before any repo work …
Ask the user — via a quiz (AskUserQuestion)"* into sessions in repositories that have no
control plane and no workspaces.

**charter granted.** The persona tool-gate answers `allow`, and the harness then runs that
command without prompting. What it reads to decide is `personas/<n>/persona.md` and the
active-persona pointer — charter's own files inside a plane, and outside one just contents
of the repository you cloned, because `config.PERSONAS_DIR` is `<cwd>/personas` there. A
checked-in `personas/rogue/persona.md` declaring `tools: [curl]` beside a
`.charter/active-persona` naming it was enough for `curl https://evil.example/x` to come
back `allow`.

## Gated at the handlers, not at the nudges

The obvious repair was to teach `_workspace_confirm_nudge` and `_mark_guard_seen` — the two
the report named — to ask the flag. That would have left five more writers and the tool-gate
untouched, and would have left the next nudge to remember. So the gate is at the entry
points: one predicate, `hooks._in_a_plane`, read once per handler, and
`tests/test_a_repo_that_is_not_a_plane_gets_no_housekeeping.py` drives the property off
`_HANDLERS` itself, so a handler added later inherits it rather than opting in.

## It is a gate, not an off switch

The per-turn cost is an argument for making the plugin one quiet no-op outside a plane, and
that argument is refused here. Three refusals are facts about the shell or about a secret
rather than policies a plane happens to hold, `pretooluse` has said so in a comment since
0.42, and `$CHARTER_HOME` puts a real vault directory within reach of a cwd that holds no
`charter.toml`. So with no plane anywhere, these still deny exactly as before:

* the vault-leak guard on Bash (**A**) and its twin on `Read`/`Grep` (`pretooluse-read`) —
  one predicate, and gating half of a matched pair is how #462's bypass shipped;
* the live-substitution guard on a forge command (**A5**) and on charter's own text-taking
  commands (**A6**).

Inside a plane nothing changes at all: A2/A3/A3b/A4 fire on the same commands, the tool-gate
smooths the same declarations, and every tally, nudge and session pointer is written where
it always was. What is new is that a refusal outside a plane is now *delivered without being
tallied* — the denial still goes out, and `hooks._trace` records nothing, because a trace is
read by `charter persona stats` against a plane and there is no plane there to read it.
