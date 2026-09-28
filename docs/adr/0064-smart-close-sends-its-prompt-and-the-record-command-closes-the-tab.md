# Smart close sends its prompt, and the record command closes the tab

**Accepted 2026-09-28**, by the operator's rulings on SI-8 (smart IDE).

Closing a chat throws away everything it knew that is not already in the plane: why it did what
it did, what it left half-done, where the next chat should start. The operator ruled that
closing can first have **the same chat** write a **session record** into its workspace, fold
what lasts into `workspace.md`, and then close. That is **Smart close**.

This PR is the core, the command line, the skill and the documents. The app's side is the next
PR, and its rules are recorded here so it has one place to build from.

## The decision

### A session record is a summary the chat writes, and charter files

A **session record** is one Markdown file per record: `workspaces/<ws>/sessions/
<YYYYMMDD-HHMMSS>-<slug>.md` for a chat in a workspace, and `sessions/<…>.md` at the plane's root
for a chat at the plane root, which is in no workspace (SI-1). It is a summary and never the
transcript: at most 32 KiB, and its body is exactly five `## ` sections, in order — **Goal**,
**Done**, **Decisions**, **Open**, **How to resume** — each with something under it and nothing
before the first. A heading inside a code fence is text.

**The model writes the title and the body; charter writes everything else.** The frontmatter is
charter's alone, from facts it already holds:

| Key | From |
|---|---|
| `title`, `date` | the command's `--title`; the machine's clock |
| `chat`, `chat-name` | `$CHARTER_CHAT`, else `$CHARTER_SESSION_ID` (the same number, set in every app chat); the name the chat's tab shows, from the app's record (`reopen::shown_name`) |
| `persona` | the persona ladder, as every command reads it; `none` for none |
| `harness` | the program the app's record says the chat runs |
| `conversation` | `reopen::conversation_of` — see below |
| `workspace` | the workspace, or `plane root` |
| `piece` (one per line) | the piece the command runs in, and each `--piece <repo>/<piece>`, with the branch `git worktree list` reports; a piece git does not report is refused |

A fact charter does not have is written `unknown`, never left out, so every record has every key
and a reader never has to guess whether a key's absence meant something.

**The conversation id is SI-8a's answer.** `reopen::conversation_of(root, number)` is the one
way to ask which conversation a chat is in now: the app keeps the chat's `resume` in
`reopen.json` current from its own hook reports (the id a Codex or opencode chat names, the one a
Claude Code chat moves to on `/clear`). A chat it holds no id for is written `unknown`.

`sessions/index.md` (newest first, one line per record: date, title, link) and `workspace.md`'s
`## Sessions` line (the count, the latest record, a link to the index) are **rebuilt from the
records** every time one is written. The records are the one source of truth; a hand edit to
either lasts until the next record. `## Sessions` is in the workspace.md template, before
`## Log`, saying there are none yet. **The skill, not charter, folds** durable decisions, terms
and vision changes into `## Context & decisions`, `## Glossary` and `## Vision`: those are
judgments, and charter's line is only a pointer.

### LIVE and LOCAL

A workspace's records follow the workspace: the LIVE block un-ignores `sessions` and
`sessions/**` beside `memory` and `todos`, `charter workspace live` and `reinit` write it, and a
LOCAL workspace keeps them on disk. `meta_paths` lists `sessions` so going LOCAL untracks them as
it untracks memory.

**The plane root's records stay on this machine** (`/sessions/` in the plane's `.gitignore`,
written by `init` and added by `reinit`). A workspace is LOCAL until somebody makes it LIVE; the
plane root has no such switch, so the choice that publishes nothing by itself is the one that
fails toward no change. Committing them by default would push every plane-root chat's summary to
the plane's remote — public, for a plane like charter-plane — without anyone having chosen it.
Deleting the line shares them.

Secrets: the skill keeps them out, and `check` refuses a record whose title or body matches the
credential shapes `charter save` refuses a memory for (`secretshape::secret_kind`), naming the
kind and never the value. The save-time scan still guards whatever is committed.

### One writer, and it is what ends a Smart close

`charter session record --title … [--piece …] [-w …]` with the body on standard input is the
**only** write path. It validates (`sessionrecord::check`: title one drawable line of at most 120
characters, body shape, size cap, no control or invisible character but a line feed or a tab, no
credential shape), writes the record whole (temp file and rename, the name taken under a lock on
the directory), rebuilds the index and the pointer, and then **tells the app**: one fire-and-forget
line on `$CHARTER_HOOK_SOCKET`,

```json
{"chat": 3, "session_saved": "/abs/plane/workspaces/alpha/sessions/20260928-140312-ship-it.md"}
```

`hookwire::SessionSaved`, the fourth kind of line beside a report, an ask and `StartedByHand`
(ADR 0062), and the same shape: one connection, one line, a 250 ms write deadline, every failure
dropped. It is read after the other three and requires `session_saved`, which none carries, so
no line an older `charter` writes reads as one, and an app older than this reads the line as
nothing and drops the connection. `Listener::each_answering_noticing_and_saving` hands it to the
app; the existing `each_answering_and_noticing` drops it.

With no socket or no `$CHARTER_CHAT` (a chat started outside the app), or an app that did not
hear it, the record is written all the same and the command says the tab will not close by
itself. `charter session list [-w]` and `charter session show <file>` read them back.
`session` joins charter's core words.

**Completion is signalled by the command, never by reading output.** The app learns the record
is saved from the line the command sends, exactly as it learns a chat's state from hooks: nothing
watches the terminal for a sentence the model printed (spec decision 3).

### Smart close sends its prompt

**This is the deliberate exception to ADR 0061.** A curation action types its prompt and never
sends it, because the prompt is the whole contract with the operator and nothing may run before
they have read it. Smart close is different in kind: the operator has already chosen what
happens — by clicking **Smart close** on a chat they are looking at — and the prompt names one
skill of charter's own, on that chat, whose only write is a record the operator asked for. The
click is the consent, so the app **sends** the prompt. There is still no setting that makes a
curation action send its prompt; this exception is Smart close's alone.

The prompt is one line naming the skill in words, never a `/slash` command, so it works on every
harness that reaches charter's skills (ADR 0063): *Use charter's smart-close skill to write this
session's record and close the chat.* The skill ends with `charter session record`, and so the
command that writes the record is the command that closes the tab.

## The app's side (the next PR)

- **When the prompt is sent.** On a chat that is **Waiting**, at once. On a **Running** chat it is
  queued until that chat's next `Stop`, then sent. On a chat that **needs you** (a permission or
  a question), Smart close is refused, with a sentence: the chat is asking the operator something,
  and the operator answers it first. A chat that was **never prompted** or whose state is
  **Unknown** has nothing to record: it is offered **Close** only.
- **How it ends.** On a `SessionSaved` whose `chat` is the chat being smart-closed, the app
  closes that tab as Close does. A `SessionSaved` for a chat that is not being smart-closed closes
  nothing.
- **When it does not end.** About five minutes after the prompt was sent with no record, the tab
  goes back to normal with one sentence saying no record arrived and the chat was left open.
  **Smart close never closes a chat without its record.**
- **While it runs.** The tab wears a closing look that reuses the `charter-breathe` animation,
  still under reduced motion through the motion tokens. Its menu offers **Cancel smart close**,
  and the operator typing into the chat cancels it too — the app knows the input is theirs, as it
  does for a curation prompt (ADR 0061, Q29). A cancelled Smart close leaves the chat open and
  running; a record it writes afterwards closes nothing.
- **Asking.** Closing a chat asks **Smart close** (primary), **Close** or **Cancel**. **Close**
  is the default when the chat has had at most one turn, where there is little to record.

## Getting a session back (SI-8d, added 2026-09-28)

The operator's ruling (Q9): *the user can always get old sessions back.*

- **A Sessions panel** per workspace, and one on the plane root's tab for the plane's own, lists
  `sessionrecord::list` newest first: the title, when, persona and harness, and `↻ resumable`
  where the record holds a conversation. A row opens the record as a read-only view tab
  (`{ view: "session", key: <plane-relative path> }`), rendered Markdown with no HTML.
- **A record is named by its plane-relative path**, and `sessionrecord::locate` is the one
  reading of it — the CLI's `session show`, the app's commands and the briefing all go through
  it, and it refuses every other spelling rather than normalising one.
- **Resume starts a NEW chat**, never the one that wrote the record: in the record's place, on
  a profile of its harness (the project's default first, then a declared one, then the
  built-in, because the record does not name the profile), with `Start::resume` set to its
  conversation so `start::ready` builds the harness's own resume words — the relaunch's builder,
  not a second one — and as its persona where the plane still has it. `Start::resuming` puts the
  record's path in `$CHARTER_RESUMING_RECORD`, and the briefing quotes the record from it, every
  line behind `> ` under a sentence saying it is data, up to 8,000 characters, in place of the
  last-session line.
- **When the conversation cannot be given, the chat is fresh and says why**: no id in the
  record, no harness in it, no profile here that runs it. A harness that cannot find the
  conversation cannot be asked in advance; it says so by its program failing **before it
  reported anything** — a state the board holds from hooks and the exit status, never from
  output. The window then closes that chat and asks again with `after_failure`, which starts the
  same record fresh, once, and says the harness could not bring the conversation back.

## What this costs

- **A record costs a turn.** Smart close spends one more turn of the model on the way out.
- **The title is the model's words in every later briefing.** Each session start names the
  place's last record in one line: *Last session: "<title>" — <path>*, the title quoted and
  said to be data, never a task. A title that reads as an order is still only read.
- **A plane-root record is not shared** unless the operator deletes the `.gitignore` line.
- **A record made from inside a piece** records that piece; one made from the workspace
  directory records only the pieces passed with `--piece`.

## What was rejected

- **A transcript, or a log.** It is large, holds everything the chat saw (secrets included), and
  is what a harness already keeps. A record is what the next chat needs, in five sections.
- **Frontmatter the model writes.** Then a record could claim a chat, a branch or a workspace it
  never had. The model gives two things; charter gives the rest.
- **Reading the chat's output to know it is done.** Parsing harness output decides nothing
  (spec decision 3); the command that did the work says it is done.
- **Charter folding the record into `workspace.md`.** Deciding what is durable is judgment. The
  skill does it; charter keeps one pointer line.
- **Committing plane-root records by default.** Above.
- **Reading the harness's "no conversation found" to fall back.** Output decides nothing (spec
  decision 3). The failed exit before any report is the signal, and its cost is said: a resumed
  chat whose program fails for another reason before its first report is also started again
  fresh, once.
