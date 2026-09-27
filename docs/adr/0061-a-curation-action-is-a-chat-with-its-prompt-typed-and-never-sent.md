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

The model and its resolution are `charter_core::curation`. The app's side, SI-2b, calls
[`resolve`](../../crates/charter-core/src/curation.rs) and nothing else to decide what is offered;
it is recorded under **The app's side** below.

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

## The app's side

- **Where it is offered.** A "Curate ▸" submenu on a workspace's, a persona's and the plane
  root tab's right-click menu (the plane root tab is the plane subject's, SI-1), and one palette
  row per action, `Curate <subject>: <label>`. charter's own first, then a
  group per declaring persona, then each action the core left out as a row that cannot run, with
  the core's sentence as its reason.
- **What opens.** The window names the subject and the action's id and nothing else;
  `curation::curate` in the app resolves the subject again, so the text typed is the core's now.
  The chat starts on the project's default profile (else the first the picker lists, which is
  the row a new chat's picker starts on), as the action's runner, in its directory, and its tab
  says `<label> · <subject>`. It is filed under the subject workspace when it runs in that
  workspace's directory, and on the plane root's tab otherwise; `start::ready` hands a chat at
  the root `$CHARTER_PLANE_ROOT_SESSION=1` as it does any other (SI-1).
- **When the prompt is typed.** The prompt is held in the app per chat, not in the window, until
  the chat's first `SessionStart` hook report that began a session. A report of a prompt, a
  turn's end or the chat's own end before that drops it; so does the chat ending or being
  closed. It is never typed late.
- **How.** One bracketed paste, `ESC [200~ … ESC [201~`, and nothing after it: no carriage
  return, no line feed. Line breaks inside are line feeds; every other control character is
  taken out, so a prompt holding `ESC [201~` cannot end the paste early and have the rest read
  as keys. It is pasted bracketed whether or not the program asked for it: the terminal engine
  offers no way to ask which modes are on, and all three harnesses turn it on.
- **Only once the terminal hands keys to the harness.** `SessionStart` says the harness started,
  not that it reads keys. Measured on Claude Code 2.1.283: in a folder it has just been told to
  trust, the hook fires inside a third of a second when the terminal is canonical again, and a
  two-line paste written then was echoed onto the screen and lost its second line. So the app
  asks the terminal's line discipline (`Session::edits_lines`, the pty's `ICANON`) every 20 ms
  for up to ten seconds and types the moment it is raw; a terminal still canonical then is not
  typed into, and stderr says so. That is the kernel's state, not the harness's output.
- **Which harnesses.** ~~Only one that reports `SessionStart` at launch
  (`Harness::reports_its_start_before_the_first_prompt`): Claude Code. Codex fires it inside the
  first turn and opencode's shim at the first prompt, so a prompt typed on it would follow what
  the operator had already sent. With either as the default profile every action is shown
  disabled with that reason, and `curate` refuses before anything starts.~~ Amended 2026-09-27
  below: Codex is typed into once its terminal is raw and quiet; opencode is still refused, for
  a measured reason. `Harness::ready_to_type` is the one answer.

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

## Amendment, 2026-09-27: Codex is typed into once its terminal is raw and quiet; opencode is not

**Pending the operator's ruling (Q25).** Codex and opencode report `SessionStart` only with the
first prompt, so no hook marks the moment a prompt can be typed into either. This amendment
gives Codex a moment that is not its output, and records why opencode still has none.

### The rule

A Codex curation chat's prompt is held from before the chat starts (`Typed::hold_until_quiet`),
and a thread of its own asks the chat's terminal every 20 ms:

1. **Is it raw?** `Session::edits_lines`, the pty's `ICANON`, as the Claude Code path asks.
2. **Has it then written nothing for one second?** `Session::quiet_for`: the time since bytes
   last arrived from the pty, counted from the later of that and the moment the terminal was
   first seen raw, so a harness silent while still canonical is not typed into the moment it
   goes raw.

When both hold, the prompt is typed exactly as on Claude Code: one bracketed paste, line feeds
inside, every other control character taken out, nothing after `ESC [201~`. Not raw and quiet
within 15 seconds of the start, it is let go of, typed nowhere, and stderr says so. The dropping
rules are unchanged: a report of a prompt, a turn's end or the chat's end, or the chat ending or
being closed, drops it — and so does the chat's `SessionStart`, which on Codex comes inside the
first turn and so means something was already sent. Claude Code's path is unchanged.

A prompt Codex would draw as a placeholder is not typed: codex-cli 0.147.0 draws a paste over
1,000 characters as `[Pasted Content N chars]`, which the operator could not read before sending
(`Harness::longest_paste_drawn_whole`). `curate` refuses such an action before anything starts,
with the length in its sentence. charter's own three are well under it.

### Why this is within "nothing parses harness output to decide anything"

The quiet period uses **only the fact that bytes arrived, and when**: `Session::quiet_for` is a
timestamp the reading thread sets on every read, before and without looking at what was read.
Nothing inspects a byte of it, so no wording, colour, layout or version of Codex's screen can
change when a prompt is typed, and no harness state is inferred from it: the chat's state still
comes from hooks only, and nothing but the one paste depends on this. It is the same kind of
fact as the terminal's `ICANON`, which ADR 0061 already reads: the kernel's, about the pty, not
the harness's, about itself. What it cannot tell is *what* is on the screen — a dialog that is
quiet looks like an input that is ready — and the cost of that is below.

### Measured

codex-cli 0.147.0 and opencode 1.18.32, each started exactly as the app starts it (the same
`-c hooks.*` flags, and for opencode the same `OPENCODE_CONFIG_CONTENT` shim and `OPENCODE_PURE=0`)
in charter's own `Session` and terminal engine at 160×50, with a scratch `HOME`, `CODEX_HOME` and
`XDG_*` directories and a stand-in model server, so nothing reached a real model and neither the
operator's `~/.codex` nor `~/.config/opencode` was touched (listed before and after). A hook
logger stood in for `charter hook`. A control run that pressed Enter after the paste showed the
prompt reaching the stand-in and `userpromptsubmit` in the log, so both detectors see a send.

| | Codex 0.147.0 | opencode 1.18.32 |
|---|---|---|
| `?2004h` (bracketed paste on) | before the terminal goes raw, every run | within 1 ms of it going raw, every run |
| raw after start | 30–40 ms warm, 0.2 s cold, up to 1.2 s at load 40 | 0.63–0.94 s idle, up to 5 s at load 50 |
| longest silence after raw, before the first screen is whole | 0.16–0.18 s idle, 0.3 s with every core busy, 0.67 s at load 40 | 0.83–0.85 s idle, 1.25–1.42 s with every core busy, over 8 s at load 50 |
| a paste during that silence | landed whole (pasted 80 ms after raw, mid-draw) | **lost**: the box is drawn, then takes no keys until its boot ends |
| raw + 1 s quiet, a two-line prompt | 6 of 6: both lines in the input, one paste, nothing sent | — |
| raw + 1 s quiet, charter's Safe remove prompt | 5 of 5 whole in the input, nothing sent | — |
| raw + 2 s or 2.5 s quiet | 27 of 27 | 20 of 20 up to load 44; **6 of 10 lost** at load 50, the boot outlasting the quiet period |

"Nothing sent" is no request at the stand-in and no `userpromptsubmit` in the hook log, for
every run. A prompt of 2 + 3 + 1 lines holding digits pasted into Codex's hooks-review dialog
did not answer it.

### What this costs

- **A quiet dialog swallows the prompt.** Codex asks once to review charter's hooks ("Hooks need
  review") the first time it sees them. That screen is raw and quiet, the paste lands on it and
  is discarded — nothing is sent and nothing is chosen, measured — and the chat opens with an
  empty input once the operator answers. charter cannot tell, because telling would mean
  reading the screen. The folder-trust screen animates, is never quiet, and is never typed into:
  the prompt is let go of at 15 s.
- **A prompt typed while the operator types.** Nothing drops the prompt when the operator
  starts typing in the second before it is pasted; it lands after their text, still unsent.
- **A second later than on Claude Code.** The prompt appears about 1.3 s after the chat opens.

### Why not opencode

opencode draws its input box, goes raw, and then boots silently before the box takes a key. The
silence is its own work (plugins, the project, the file watcher; its log shows it), so it grows
with load: 0.83 s idle, 1.4 s with every core busy, and more than 8 s while this machine ran
other builds at load 50 — and a paste in it vanished, typed nowhere, with nothing to say so. The
silence has no bound charter can know: a quiet period over the 8 s measured would keep every
opencode curation chat empty for most of ten seconds and still lose a prompt on a busier
machine, and one short enough to wait for lost six of ten at load 50. So opencode as the default profile
is still refused, now with that reason. Two findings for whoever gives it a moment:

- The shim runs inside opencode and could report when the server is ready — the one kind of
  signal that is a hook rather than a guess. That is a change to ADR 0058's shim and is not made
  here.
- opencode draws a paste of three lines or more than 150 characters as `[Pasted ~N lines]`;
  `experimental.disable_paste_summary` in the session's `OPENCODE_CONFIG_CONTENT` draws it
  whole (measured), unless the operator has toggled the summary in opencode, whose stored
  choice wins. Every built-in prompt is over that, so an opencode curation chat needs it.

### Rejected

- **A longer quiet period for everyone.** It does not make opencode safe (above), and it makes
  Codex slower for no measured gain: Codex took a paste at any moment after going raw.
- **Reading the screen for the input box.** That is parsing harness output, and it breaks on the
  next redesign of either TUI.
- **Typing without bracketed paste.** Each line feed would be Enter.
