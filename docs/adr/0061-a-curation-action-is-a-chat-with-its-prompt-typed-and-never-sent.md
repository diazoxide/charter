# A curation action is a chat with its prompt typed and never sent

**Accepted 2026-09-26**, by the operator's rulings on SI-2 (smart IDE).

Keeping a plane healthy is mostly the same few conversations: retire a workspace without losing
what it learned, compact a persona's memory, fold a lesson into a charter. Each one starts with
the operator typing the same paragraph into a new chat. The operator ruled that charter offers
those conversations itself, on the thing they are about, and that personas can add their own.

## The decision

**A curation action opens a new chat with a prompt already typed into it, and never sends it.**
The operator reads the prompt and presses Enter. There is no setting, flag or file key that
sends it instead. The prompt is the whole contract between the action and the operator, so the
operator always sees it before anything runs.

This PR is the model and its resolution, in `charter_core::curation`. The app's menus (a
"Curate ▸" group, palette rows, and pasting the prompt once the chat's first SessionStart hook
has reported) are a later PR, and they call [`resolve`](../../crates/charter-core/src/curation.rs).

### What a subject is offered

A **subject** is a workspace, a persona or the plane. `resolve(root, subject)` answers an ordered
list, each item with its id, label, source, the persona that runs it, the directory it runs in
and its rendered prompt, plus a warning for every action it left out.

1. **charter's own**, always first and in this order. They ship inside the binary, are never
   files, and their ids are `charter/<id>`:
   - `charter/safe-remove`, "Safe remove", on workspaces and personas: audit, promote durable
     learnings to shared or persona memory or the plane's docs, then run `charter workspace
     remove` or `charter persona remove`, whose guards still apply.
   - `charter/compact`, "Compact & improve", on workspaces and personas: the existing optimize
     and dedupe reports, pruning only with the operator's yes, then durable lessons folded into
     `workspace.md` or `persona.md`.
   - `charter/add-curation-action`, on personas: the harness writes a new action file for that
     persona.
2. **Each persona's**, by persona name and then by id: one file per action at
   `personas/<persona>/curation/<id>.md` (`docs/plane-format.md` records it), with `label`,
   `on` and an optional `runs-in` in the frontmatter and the prompt template as the body. The
   id is `<persona>/<id>`, and **the declaring persona runs it**.

**A built-in cannot be overridden or impersonated.** A persona's file whose id or label
(case-insensitively) is a built-in's is an error. `charter persona lint` reports it, and
`resolve` leaves it out and says so in a warning naming the file. Nothing is ever dropped
silently: every action with an error is named in the warnings of any list it might have been
meant for. One function, `curation::parse`, decides what is wrong with a file, and `resolve`,
`lint` and `charter persona curation add` all ask it.

### The template is data

Exactly four variables: `{subject.kind}`, `{subject.name}`, `{subject.path}`, `{plane.root}`.
They are substituted in one plain pass. A value is never scanned again, no shell sees the text,
and no environment variable, vault or secret is read. Any other `{word}` is a lint error rather
than text, so a typo, or a `${VAR}` copied from a shell script, never reaches a chat looking as
if it had been filled in. Braces around anything that is not a word (`{"a": 1}`) stay text.

### Who runs charter's own

For a persona subject, **the persona being curated runs it**: it curates itself, with its own
memory and charter in front of it. For a workspace or the plane, charter has no persona of its
own to name. `steward` is this project's plane's front door, not a charter concept, and a plane
may have no persona at all. So the runner is **the plane's default persona**: `[persona]
default` in `charter.toml`, else the legacy `personas/.default`, each only when it names a
persona that exists (`active::plane_default_persona`, the same answer `charter persona default`
and the persona ladder give). With neither, the chat runs as **no persona**. This fails toward no
change: a default naming a persona that was removed resolves to no persona, never to a guess.

### Where it runs

| Subject | No `runs-in` | `runs-in: subject` | `runs-in: plane` |
|---|---|---|---|
| workspace | its directory | its directory | plane root |
| persona | plane root | `personas/<name>/` | plane root |
| plane | plane root | plane root | plane root |

`charter/safe-remove` always runs at the plane root, because its subject's directory is going
away. The other built-ins take the default.

### The command line

- `charter persona curation list [<persona>]`, `add <persona> <id> --label … --on … [--runs-in
  …]` (the prompt on standard input) and `remove <persona> <id>`. These are persona verbs
  because the files belong to the persona, beside `persona create` and `persona remove`. `add`
  writes only a file that reads back with no error, and never over an existing one: to change
  one, remove it and add it again, which keeps a hand-edited file safe from a typo.
- `charter curation show <subject>`, with `workspace:<name>`, `persona:<name>` or `plane`.
  What a subject is offered is charter's own and every persona's together, so it belongs to no
  persona and gets its own noun. The subject is positional because it is the one thing the
  command is about, like `charter persona show <name>`. `curation` joins charter's core words,
  so neither an extension nor a harness profile can take it.

### The skills the prompts name

Each built-in's prompt is plain language that names a skill of charter's plugin and never uses
a harness's `/slash` syntax: `safe-remove`, `compact` and `add-curation-action`, in
`app/src-tauri/plugin/skills/`. Their content is CLI commands, so any harness can follow it. A
test holds that every built-in names a skill the plugin ships.

**They reach Claude Code only, exactly as the six existing skills do.** Claude Code loads the
bundled plugin with `--plugin-dir` in an app chat and the installed copy outside it (ADR 0057),
and both carry `skills/`. Codex and opencode get hooks from charter and no skills: the opencode
shim carries only hooks (ADR 0058), and Codex gets `-c hooks.*` flags (ADR 0050 records why
charter does not install into Codex's plugin cache). That gap is not new and this ADR does not
close it. It is why each prompt says what to do in a sentence as well as naming the skill: a
Codex or opencode chat can follow the prompt, the CLI commands it names and their `--help`, with
no skill at all. Closing the gap is one adapter per harness (charter is harness-agnostic), and
belongs with whichever change teaches those harnesses charter's skills.

**Amended 2026-09-26: the gap is closed** by [ADR 0063](0063-every-harness-a-chat-runs-on-is-handed-charters-skills-by-its-own-route.md).
An opencode chat the app starts discovers charter's skills as its own, through the shim, and a
Codex chat is briefed on them at `SessionStart`, with the path to each `SKILL.md`. The prompts
still say what to do in a sentence as well, because a chat outside the app, or a Codex chat whose
hooks are not trusted yet, has no skills from charter.

## What this costs

- A persona's `extends:` does not carry its curation actions. Each persona offers only its own
  files, so an action meant for a family is declared once per persona or on the parent alone.
- ~~`{word}` in a prompt is always a variable. A prompt that needs literal braces around a word
  (a template language, say) cannot have them.~~ Lifted by the amendment of 2026-09-26 below:
  `{{word}}` types `{word}`.
- Every persona's actions on persona subjects are offered on every persona. A persona cannot
  scope an action to "only personas I own".

## What was rejected

- **Sending the prompt, or an opt-in to send it.** The operator ruled no opt-out of review.
- **Built-ins as files a plane can edit.** Then a plane could rewrite what "Safe remove" does,
  and the menu would say "Safe remove" over it.
- **`steward` as the built-ins' runner.** It is one plane's persona name, not charter's.
- **YAML frontmatter.** `persona.md` has line-based frontmatter, and a second parser for the
  same shape of file would be a second answer to what a key means.

## Amendment, 2026-09-26: the name `charter` is reserved, and `{{` `}}` escape a brace

Two gaps this decision left open, closed the day it was accepted.

**A persona named `charter` could pass its own action off as a built-in.** Its ids read
`charter/<id>`, the built-ins' namespace, so a file with an id no built-in uses looked like one
in every menu. The id and label checks above cannot see it, because nothing about the file
clashes; the persona's name does. So `charter` is a reserved persona name, beside the leading
`_` charter already keeps for its own namespaces (`personas::RESERVED`, one function,
`reserved_refusal`, asked by all three readers):

- `charter persona create charter` is refused with a sentence saying why.
- A persona that already has the name is not made unusable: it still loads, runs and can be
  removed, because the reservation is not part of the name grammar every command checks
  (`shape_refusal`). `charter persona lint` reports it as an error.
- `curation::parse` refuses every file under `personas/charter/curation/`, so `resolve` leaves
  each out with the usual warning naming the file, and `charter persona curation add charter`
  writes nothing.

The comparison ignores case, though the persona alphabet is lowercase and already refuses
`Charter`.

**A literal `{word}` could not be typed.** Now `{{` is a literal `{` and `}}` a literal `}`, the
convention of Rust's `format!` and Python's `str.format`, read in the same single pass as the
variables: nothing is expanded twice, and a substituted value is still never scanned, so a
value holding `{{` is typed as it is. `{{word}}` types `{word}` and is not a variable, so lint
accepts it; `{{{subject.name}}}` types the name in braces. A lone `}` stays text, as it was,
rather than becoming an error the way it is in `format!`, so no prompt that parsed before stops
parsing. What moves: `{{subject.name}}` used to type the name in braces and now types
`{subject.name}`, and `}}` used to type two braces and now types one. Curation actions shipped
in no release before this, so no plane holds a prompt that relied on either.
