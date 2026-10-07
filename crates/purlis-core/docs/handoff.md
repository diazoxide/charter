# You asked one chat for a second thing, and it did it there

You are in a chat about the API and you ask about the deploy script. Three things happen,
and all three cost you:

- the model does it here, so one workspace's todos, memory and branch now carry two tasks;
- the model hands it to a sub-agent, and the answer you wanted to talk to comes back as a
  paragraph folded into this conversation and is then gone;
- the model tells you to open a chat yourself, and you retype the context it already had.

`purlis handoff` is the fourth way: a chat in a workspace you name, opened in the app without
taking your screen, already working on a brief you read and approved.

## Three places a request can run, and the two questions that pick one

1. **A sub-agent** — your harness's own helper. purlis leaves an Agent call alone with one
   exception: a call named for a persona is refused, because a persona runs as its own chat
   (`purlis docs show personas`). Nothing on this page touches a helper.
2. **A new chat in this workspace.**
3. **A new chat in another workspace**, existing or new.

2 and 3 are one mechanism — a **handoff** — because a chat belongs to its workspace for life.
The only thing that differs is the workspace. A handed-off chat has its own workspace and its
own todos, and answers the chat that opened it only when that chat asked it to (`--report`).

**Sub-agent or chat: who reads the result?** If this chat needs the answer *to continue this
turn*, it is a sub-agent. If you will read it and talk to it, it is a chat.

**Fire-and-forget, or needs an answer?** A chat that works on its own and that you will read
yourself is fire-and-forget: the default. A chat whose outcome this chat has to act on later —
a fix it will build on, a question it asked — is handed off with `--report`, and tells this
chat when it is done (*A report back*, below).

**This workspace or another: does the ask serve this workspace's vision?** Yes → a chat here.
No → another workspace, matched against other workspaces' visions (`purlis workspace list`,
then `purlis workspace vision -w <name>`). purlis supplies the facts and never names the
answer — and these are rules for the *proposal*, not for the command. `purlis handoff`
refuses none of them, so a chat already in `default` can still hand off within it.

## The command

```bash
purlis handoff <workspace> --name "<short task>" [--report] [--create --vision "<vision>"] [--persona <name>] <<'BRIEF'
<the brief>
BRIEF
```

- **`--name` is what the new chat is called** — its tab, and wherever else a chat's name is
  shown: `drop account-console-commons`, not `steward 7`. Held to a chat name's rule: trimmed,
  at most 64 characters, no control or invisible character. Without it the chat is called what
  any new chat is, `<persona> <N>`.
- **`--report`** asks the new chat to report back when it is done. See *A report back*.

- **The workspace is always named**, the current one included. The permission prompt has to say
  where the chat goes, and `.` says nothing.
- **The brief arrives on stdin, as one quoted heredoc in the same call**, so the prompt shows
  the exact text the new chat will be sent. There is no `--brief-file`: a prompt that shows a
  path is an approval of a path. A brief may still *name* files, and naming them is what a good
  brief does.
- **The harness is this chat's.** There is no `--harness`, and there is no `--repo` either —
  the brief says what to clone, and the new chat owns its own setup.
- `--persona` pins the new chat's persona, visibly in the prompt. Without it the chat gets what
  a new chat gets: the plane's default persona, when it declares one. It never inherits the
  asking chat's persona.
- `--create` makes the workspace first, and needs `--vision`. A workspace with no vision is
  never proposed as a handoff target, so one created without it would be created unfindable. A
  workspace `--create` makes is LOCAL.

## What a handoff does

Every question is asked before anything is written — see *What `purlis handoff` refuses*
below. Then, from a chat the app started:

1. The command asks the app, over the chat's hook socket, to open the chat.
2. With `--create`, the app creates the workspace and records its vision.
3. The app opens a chat in the target workspace's directory, on **its persona's own profile**
   where that persona's definition names one (`profile:`), else on the same harness profile as
   the chat that asked, read from the app's own record of that chat and never from the request.
   The profile is always one the project offers on this machine and has approved. A name in
   a persona's definition is only looked up, never run: where this machine does not offer it,
   the chat starts on the asking chat's profile, and the command's answer and the new chat's
   stamp say so. A profile that is offered and not approved opens nothing.
4. Its first message is the stamp line, a blank line, then the brief verbatim — with, for
   `--report`, one more line under the stamp saying how to report. It rides the harness's own
   argv (`claude "<message>"`, `codex "<message>"`), never typed into its pane.
5. The chat lands as a new tab on the target workspace's strip, **behind the tab you are
   reading**, and the window is not raised. It takes the front only in a window with no tab at
   all, where there is nothing to interrupt.
6. The command records a todo in the target workspace: the brief's first line, and which chat
   and workspace handed it off. The rest of the brief is not in it, because a LIVE workspace
   commits its todos and a brief never reaches a committed file. If an open todo there is
   already about the same work, compared by first line, the command says it is already on the
   list and records nothing twice.
7. The command adds one `handoff` row to the dispatch log (`personas/_dispatch/`): when,
   whether the chat went to the workspace it was asked from or elsewhere, and whether the
   handoff created the workspace. It names no workspace, no persona and nothing of the brief.
8. The command prints the new chat and its workspace.

The todo and the row come after the chat is open, and a failure to write either is said and
never undoes the open. A handoff the app would not open writes neither. There is no mark on
the strip beyond the new tab itself: the tab is how you see it.

**Your yes to the prompt in front of `purlis handoff` is the only one asked for.** The app
opens one chat per `purlis handoff`: the command asks the app for a single-use ticket and
spends it on the same connection, so no single line on the socket opens a chat and no line can
be replayed. The ticket cannot tell the command you approved from another process running
inside the same chat, which could run `purlis handoff` itself, just as it can already start a
harness in the background with `claude -p`. That is why a handed-off chat always lands as a
tab you can see, stamped with the chat it came from.

**Outside a chat the app started**, or when the app is not listening or does not answer,
nothing is opened and nothing is created. The command says so, tells you to open purlis and
either run the handoff again from a chat the app started or start a chat in that workspace
from the window, and exits 1. If the app refuses, you get one more line saying why.

## The stamp

```
⟨handoff from <source chat's name> · workspace <source-workspace> · <YYYY-MM-DD HH:MM>⟩
⟨handoff from <source chat's name> · plane root · <YYYY-MM-DD HH:MM>⟩
```

The second is a handoff from a chat at the **plane root**, which is in no workspace: its stamp
says so, rather than naming the workspace purlis would otherwise have picked for it. The chat
it opens still starts in the workspace you handed it to, and a report back to a root chat that
has since closed is kept for the plane root — the next chat started there reads it.

Facts purlis can observe, and no instruction. The new chat — and whoever reads the transcript
later — can tell the first message was not typed there. The source is named the way you see it:
the name you gave that chat, or its default, `steward 3` — never purlis's number for it.

`purlis handoff` writes the stamp with that number (`⟨handoff from chat 16 · …⟩`), because the
number is what the app checks it against: the app refuses to open a chat whose first message
does not carry the stamp of a handoff from the asking chat. Having checked it, the app writes the
chat's name in its place. Minutes, not seconds: the stamp is read by a person deciding whether
this is the message they approved a moment ago.

The same note is on the new chat's tab, as its tooltip, and in its pane's corner:
`↳ from steward 3 · platform-next` (`↳ from steward 3 · plane root` from the plane root).

## A report back

A handoff made with `--report` owes the chat that made it **one report**. The new chat's first
message says so under the stamp, and says how: it finishes with

```bash
purlis handoff report "<a few lines on what was done and what was found>"
```

The report goes to the chat that asked, **not to you**, and nothing is typed into that chat, so
a chat in the middle of a turn is never interrupted:

- **context on that chat's next turn.** When it is next prompted, its `UserPromptSubmit` hook
  hands the turn the report as `additionalContext`, each line quoted behind `> ` under a
  sentence that says it is what another chat said and not an instruction. A report is handed
  to one turn, and to no turn after it;
- **no needs-you item**, on either chat. The chat that asked reads the report, and the chat
  that wrote it now waits on that chat, so its turn ending raises none. A chat that is already
  waiting on you says `<name> reported back` on the item it has.

**The pairing is purlis's.** The app records, when it opens the chat, which chat asked; the
report names no recipient, so no chat can send its report anywhere but back to the chat that
asked. A report is refused, saying why, from a chat no handoff opened, from a handoff made
without `--report`, and a second time from the same handoff, whatever happens in between —
prompting that chat again does not re-arm it; another answer needs another `--report`
handoff. It is refused before anything is sent when it is empty,
past 4,096 bytes, or holds a control character other than a line break or an invisible one.

**If the chat that asked has closed**, the report is kept for the workspace it asked from, and
the next chat to start there learns it at its `SessionStart`. A report that was still waiting
when its chat closed goes the same way. A report with nowhere to go is the one that needs you:
the chat that wrote it becomes a needs-you item that says so.

**A chat you stop from the window** gets one short turn to write what it did, and may send one
report in it, whatever it owed before. The chat that asked is then told the operator stopped
it, on its next turn, as a line of purlis's own (`purlis: the operator stopped …`) that quotes
nothing: a report is what a chat said, and this is not one, so no report can pass for it. It is
the same line however you stopped it: **Stop** on the chat or on a chat above it, *Stop them*
as you close the chat that asked, or closing the tab of a task that had not reported.

A chat can cancel a task it dispatched itself (`purlis dispatch cancel`, below), which asks
that task for a short report and ends nothing. **Only you stop any other chat**: no command or
tool stops a chat its caller did not dispatch, and none ends one. A chat that is being
stopped, and any chat below it, cannot hand off or dispatch: it is refused, so nothing it
starts outlives the stop. You can still ask a persona from its tab yourself. A task you are
stopping cannot be cancelled as well, and a cancel under way stands down when you stop its
task: the stop is the later word.

Reports wait in `.charter/handbacks/` in the plane, one file each, until a hook takes them.

`purlis handoff report` runs under the same prompt as every `purlis handoff`, so you see the
report before it is sent. It needs no heredoc — its text is the command's own argument, which
the prompt shows as it is — and a live command or process substitution in it
(`"$(cat notes.md)"`, `<(cat notes.md)`) is refused, because that text is not the one the
prompt showed. `purlis handoff report <<'BRIEF'`, with no summary after `report`, is still a
handoff into a workspace called `report`.

## A task for a persona: `purlis dispatch`

A handoff moves work to a chat you will follow. A **task** is work a chat has done for it by
another chat, which reports back to it. Both are a **dispatch**: one chat starting another,
which runs as a persona for its whole life.

```bash
purlis dispatch --name "<short task>" [--to <persona>] [--profile <profile>] \
                [--in workspace:<name> | --in worktree] <<'BRIEF'
<the brief>
BRIEF
```

- **The new chat runs as the persona `--to` names, or as this chat's own.** Its own needs no
  grant. Another persona needs a **dispatch grant**, which only you give where the project's
  chats are sandboxed: a chat that runs without the sandbox runs as you, and can write the
  files a grant is kept in.
- **Where there is no grant, you are asked once, and the task waits for your answer.** Nothing
  starts. A Notice on the asking chat's tab says who wants to dispatch to whom and shows the
  brief, and the command says `held for the person` and exits 0: the dispatch is accepted and
  waiting, not refused. **Allow** starts it then, on the brief you read, and it is judged
  against the limits again at that moment. **Keep blocked** starts nothing and makes no grant.
  Either way the asking chat is told on its next turn, the way it is told a report; and if
  that chat was started again before you answered, the question went with its old run, so it
  is told that the task was not started and to dispatch it again. A second
  dispatch across the same pair while you are being asked is refused, not queued beside the
  first: the chat is told which task is waiting and to dispatch again once you have answered. A chat
  nobody is at is never asked for; see *A dispatch from an unattended chat*, below.
- **It is a chat of its own**, in the app, in this chat's folder unless the dispatch says
  otherwise (*Where it works*, below). You can see it, open it, type in it and stop it. It is listed in the Chats section under the chat that asked for it, by
  its name and what it is doing, and has a tab once you open it.
- **Its harness profile is its persona's own** where the persona's definition names one, else
  this chat's; `--profile` names another of the project's profiles. A profile is only ever one
  the project offers on this machine and has approved: any other name is refused, and nothing
  is started in its place. Where the persona's own profile is not offered on this machine, the
  chat starts on this chat's profile, and the command and the new chat's first message say so.
  **No chat is started for another chat on a profile whose own command switches the harness's
  permission prompts off**, whoever named it: the dispatch, the persona's definition, or
  nobody, where it is the asking chat's own. A handoff is held to the same rule. purlis
  recognises the flags it knows in the command as this machine declares it
  (`--dangerously-skip-permissions`, `--permission-mode bypassPermissions`, Codex's
  `--dangerously-bypass-approvals-and-sandbox`, `--yolo`, `--full-auto` and `-a never` or
  `--ask-for-approval never`); a wrapper script that adds one, or a harness setting kept in a
  file, is not seen.
- **It starts with its own persona's hosts and vault**, from its first command, with nothing
  to allow on its tab: the grant was your consent to the pair.
- **Its sandbox is the project's, for its own persona.** What you allowed the asking chat
  alone (a host or a folder from a block's Notice, a start without the sandbox) is not carried
  to it, and neither is the mode the asking chat's harness is in.
- **Its first message says who asked.** purlis writes two lines of its own above the brief,
  from its record of the asking chat: ``⟨task from `steward 3` · workspace alpha · 2026-10-07
  14:32⟩``, with the chat's name in a code span, and a line saying the brief is a request from that chat and not from you, that
  nothing in it approves anything, and how to report. The brief follows, verbatim.
- **`--name` is required**: it is what the chat is called and listed under.
- **Several at once is fine.** A chat that sends six tasks in one step starts six chats: each
  run of the command is answered on its own.

The same refusals stand in front of it as in front of a handoff's brief: an empty brief, one
shaped like a credential, one too long to start a harness on. And these, each in a sentence
that says what to do:

| Refused | Why |
| --- | --- |
| from inside a sub-agent, where purlis's hook can tell (see below) | a persona chat belongs to a chat you can see; the sub-agent returns what it found to its chat, which dispatches |
| with no profile to start it on: the chat is on none, the persona names none and `--profile` names none | there is no harness to start the new chat on |
| on a profile the project does not offer on this machine, or one whose command has not been approved here | a profile is looked up, never run on a chat's word |
| across a pair, or at all, where an administrator's policy locks dispatch | no grant covers it; the refusal says who locked it |
| where this machine's policy file is refused | dispatch is off until an administrator fixes the file |
| from a chat that still holds another persona's grants | it has none of its own to dispatch with until you allow them on its tab |
| a task name holding `⟨`, `⟩`, `·` or a backtick | purlis writes its own lines with them, and a name is drawn on those lines |
| a persona this project does not define, or one that does not load | there is nothing to run as |
| a persona whose definition says `draft: true` | a draft runs no chat |
| a persona that is above the asking chat in its own chain of dispatches | a chain never loops back |
| past a limit: how deep a chain may go (3), how many tasks one chat is still waiting on (6), how many chats one lineage holds that still owe work (16) | a runaway stops |
| past a persona's own two limits, where the project sets them: how many tasks the chats running as it wait on between them, and how many chats run as it at once | the project said how much of that persona it wants at once |
| where a limit is set to 0 | dispatch is off at the level that set it, and the refusal says where |
| `--in` that is neither `worktree` nor `workspace:<name>`, or a name that cannot be a workspace's | `--in` holds one of two words, never a folder or a branch |
| `--in workspace:<name>` for a workspace the project does not have, or one reached through a link | a chat starts only in a workspace's own folder, inside the project |
| `--in worktree` from a chat that works in no repo (the project's root, a workspace's own folder) | a worktree is cut from the repo the asking chat works in, and there is none |
| `--in worktree` where the worktree cannot be cut: its folder or branch is already there, or the repo's own git config names a program | purlis's name for it is used exactly, and the app runs no git in a repo that could run a program outside the chat's sandbox |

**The limits are the project's**, set in Settings › Project › Dispatch for the project, a
workspace or a persona, lowered by your own on this machine and capped by an administrator's
policy. They are read afresh for every dispatch, so a change applies to the next one. A refusal
names the limit and the count it stands at.

A limit counts chats that still owe work: ones that have not reported and whose program has
not ended. A chat that has reported stays open for you to read and counts for nothing. One
whose program ended without a report has failed, and does not count either.

**A lineage is everything descended from one chat you started.** Each chat a dispatch starts
keeps the id of that first chat, so closing a chat in the middle, or starting one again, does
not split a lineage into two that are each counted from zero.

A limit is said before you are asked for a grant: a dispatch that would start nothing does not
spend your answer.

### Where it works

The new chat works in the asking chat's folder unless the dispatch says one word about where:

- **`--in workspace:<name>`** starts it in another workspace of the project, in that
  workspace's own folder, with that workspace's todos, memory and session records. The
  workspace must be one the project has, reached through no link. The chat is listed under
  the chat that asked, with the workspace named, and reports back to that chat as any task
  does. The name used is the folder's own: on a disk that folds case, `workspace:BETA` is the
  workspace `beta`, with `beta`'s limits. The dispatch grant is the same one. **The limits of
  both workspaces hold**: the asking chat's, as for every dispatch, and then the one the new
  chat will work in. A limit set to 0 on the workspace the chat would work in switches it off
  there whatever a persona's own limits say, so a workspace with dispatch switched off is not
  worked in by a chat that names it from elsewhere; and a chat in a workspace with dispatch
  off cannot dispatch by naming another. **A chat nobody is at** (its harness's permission
  prompts are off) starts a chat in another workspace only under a grant that already stands
  for the pair, yours on this machine or the project's. Its own persona does not cross at all
  that way: no grant is kept for a persona's dispatch to itself, so none can stand, and the
  person starts such a chat from the asking chat's tab. A chat started in another workspace than its
  asker's is told so in a line under its stamp: the stamp names the asking chat's workspace,
  and the line names its own.
- **`--in worktree`** gives it a new worktree of the repo the asking chat works in, on a new
  branch. **The app cuts it, never a chat**, under a folder and a branch purlis names: the
  task's name as a branch name can carry it, then the end of the dispatch's own id, so two
  tasks never share a folder or a branch. A dispatch has no way to name a folder or a branch.
  It is cut from the clone's current commit, as every worktree purlis cuts is, also when the
  asking chat itself works in a worktree. What the asking chat has not committed stays where it
  is, and the command says so.
- **A persona whose definition says `dispatch-isolation: worktree`** gets a worktree by
  default, when the dispatch names no place. Where the asking chat works in no repo, that
  default gives way to the asking chat's folder and the command says so. `--in` wins over it.

**Nothing is merged for a worktree task, ever.** Its report names the branch, in a line
purlis writes from its own record of what it cut, whatever the chat says; merging is the
asking chat's decision or yours. The chat's sandbox is the one the project gives its persona
in that folder, as for any chat of that persona started in a worktree: it gains nothing of
the asking chat's tree.

**What it can leave on that branch depends on the sandbox.** In a project with no sandbox,
a worktree task commits on its branch. **Where the new chat is sandboxed it cannot commit
there yet**: a worktree's git data is kept in its repo's `.git`, outside the one folder a
sandboxed chat may write (purlis issue 1055). It can edit every file in its folder. Its
changes stay there, uncommitted, and its report should list them; you or the asking chat
commit them from outside the sandbox. purlis tells the new chat and the asking chat which of
the two it is as the task starts.

The worktree is listed on its dispatch's row in the Dispatches tab, as the task's own branch
(the window says *branch* and its *folder*, as it does everywhere), with how it stands:

- **folder kept** while its folder is there, merged or not. **Discard** on the row removes
  the folder, for good, with every uncommitted file and every ignored file in it. It is a
  window command, so only you run it. It asks first, naming each uncommitted file by its
  path and each ignored path that is not purlis's own; it is refused while any chat is still
  open in the folder; and where the folder holds other paths by the time you answer, nothing
  is removed and you are asked again. (It compares paths: a listed file changed again, or a
  file added inside a folder git ignores whole, is the same list.) **The branch purlis cut
  loses no commit by a discard**: it is deleted only where git already finds it merged, and
  one that holds a commit found nowhere else stays, an ordinary branch of the repo. The one
  thing a discard can lose is a commit made in the folder on no branch at all; the question
  names those commits and says they would be lost.
- **merged and removed** once purlis found the branch merged into the branch it was cut from
  and took the worktree away. purlis looks when the task's chat is closed and when the
  project is opened, and at no other time. **It takes away only a folder that holds nothing
  else**: no uncommitted file, and no ignored path that is not purlis's own. A task whose
  output is something git ignores (a results folder, a database file, a `.env`) keeps its
  folder, listed with Discard, where you are shown what is in it. A task that committed
  nothing and left nothing leaves no worktree behind once its chat is closed. Where git will
  not delete the merged branch (the repo is on another branch than the one it landed in), the
  folder goes and the row says the branch was kept.
- **folder discarded**, or **folder removed** for one removed from the explorer or by hand.
- A repo whose own git settings name a program (a filter, a diff or merge driver, an
  include) gets no worktree task and no Discard: purlis runs no git there for a chat or for
  its own account. Remove such a folder from its branch's row in the explorer.

A worktree whose task was started and whose app was quit before the chat came up is not
listed on any dispatch: it shows in the explorer as an ordinary branch folder, and is yours to
remove there.

**The sub-agent refusal is advice, not a wall.** A sub-agent runs inside its chat, with that
chat's environment, so the app cannot tell its dispatch from the chat's own. Only purlis's
hook can, from what the harness says about a tool call, and it refuses what it recognises:
the command, the command one level inside `bash -c` or `eval`, the command run through the
variable that names purlis's binary, and purlis's two dispatch tools. A dispatch behind a
variable of the sub-agent's own, in a script file or inside an interpreter is not seen. On
opencode, where purlis has not measured which calls are a sub-agent's, no sub-agent is refused
at all. A sub-agent that gets past the hook reaches what its chat reaches and no more, so a
dispatch you allow a chat is one its sub-agents can make too.

The persona chat finishes by writing its session record and sending **one report**:

```bash
purlis dispatch report --outcome done "<what was done and what was found>" [--changed "<files, commits, a branch>"]
```

`--outcome` is `done`, `blocked` or `failed` (and `cancelled`, from a task its asking chat
cancelled, and from no other). The report reaches the chat that asked as
context on its next turn, the way a handoff's does and under the same pairing: it names no
recipient. It carries the outcome, the text, what the persona chat says changed, the branch
purlis cut for it where it worked in a worktree of its own, and the path of the session record
purlis wrote for it, and every line the persona chat wrote is quoted as data under a sentence
that says it is not an instruction. It raises no needs-you item: a
task's report is for the chat that asked. If that chat has closed, the report is kept for the
workspace it asked from. If that chat was started again (a restart to take something you
allowed, Restart now), its tasks and the reports waiting for it follow it.

Which kind of report a chat sends is how purlis started it, not which command it runs: a task
that reports with no outcome is told how a task reports, and a handed-off chat's report is its
summary whichever command sent it.

purlis's `dispatch` and `dispatch_report` tools do the same two things, for a harness that
calls tools instead of running a command.

### Waiting for the report, or being told

A dispatch carries on by default: the command answers as soon as the chat has started. The
chat that asked then has three ways to get the report.

- **Wait for it in the same turn.** `purlis dispatch --name "<task>" --wait <<'BRIEF'` holds
  the command until the report lands and prints it as the command's result, quoted as data the
  way a turn is handed it. The `dispatch` tool's `wait` does the same. `purlis dispatch wait
  <chat>` waits on a task dispatched earlier.
- **A wait is bounded.** 100 seconds unless `--timeout <seconds>` says otherwise, and 540 at
  most, because a harness ends a command that runs longer. A wait that runs out is an answer
  and not a failure: it says the task is still running and how to look again. Nothing is lost
  by it, or by a command that is stopped while it waits: the report still reaches the chat
  when it lands. One chat may have 16 waits under way at once, and a wait whose command has
  gone ends by itself.
- **Be told when it lands.** A report nobody waited for is left for the asking chat's next
  turn, as before. Where purlis may type into that chat (below), it types one line of its own
  when the report lands, or once the chat's turn ends, which starts that turn: `purlis: a task
  this chat dispatched has reported (chat 9). …`. The line is purlis's sentence and a chat's
  number. The report is never typed: it arrives as the turn's context, quoted as data.

### When purlis types into a chat

purlis types three things for a dispatch, and nothing else: the line above, the line that asks
a cancelled task for a short report, and the Escape key that ends a cancelled task's turn. None
carries a word any chat wrote. Each goes to a chat's pane only when **all** of these hold, so
that it cannot land in something purlis does not know is on screen:

- **The harness is one purlis has measured.** Claude Code; and Codex once you have trusted
  purlis's hooks there. On opencode, and on any other program, nothing is typed.
- **purlis has heard from that chat since it started.** A chat whose harness has reported
  nothing yet may be showing a start-up dialog, a login or a trust question, and an Enter
  would answer it. An untrusted Codex never reports, so it is never typed into.
- **It has shown you no prompt in this turn.** purlis hears that a chat asked you something,
  and not that you answered, so the hold lasts until that turn ends.
- **You have pressed no key in its pane since it last reported a turn beginning or ending.**
  A local command of the harness (`/model`, `/permissions`, `/resume`) opens a picker that no
  hook reports. Your key is the only sign of it, so after any key of yours purlis waits for the
  chat's next turn to begin or end.

The line is typed only into a chat whose turn has ended. Escape is sent only into a turn purlis
heard begin.

Where any of that fails, nothing is typed. A report or a message waits for the chat's next
turn, as it did before this line existed. A cancel is recorded all the same.

| Harness | Typed into | How a report or a message arrives |
| --- | --- | --- |
| Claude Code | yes, under the rules above | the turn's `UserPromptSubmit` hook hands it over as context |
| Codex | only once you have trusted purlis's hooks in Codex | the same hook. Until the hooks are trusted nothing is typed and nothing is handed over: `purlis dispatch wait <chat>` prints a report, and a message waits |
| opencode | no | on the next turn you start, as before |

Not covered: a dialog a harness raises by itself in an idle chat (an update notice, a rate
limit menu, a new login). No hook reports one and no key of yours precedes it. This was built
where no app could be run, so none of the typing has been watched in a running harness yet.

### Listing and cancelling

```bash
purlis dispatch list
purlis dispatch cancel <chat>
```

`list` prints the tasks under this chat, one a line: the chat's number, the task's name, its
persona, where it works, its state and how long ago it started. The states are `running`,
`idle, with no report yet`, `waiting on the operator`, `asking this chat a question`,
`cancelling`, `reported: <outcome>` and `ended without a report`. A task that was given a
worktree of its own says so after where it works: the branch purlis cut for it, and whether
its worktree is kept, was merged and removed, or was discarded. A task is listed until its
chat is closed. The `dispatch_list` tool prints the same list. A task you started yourself
from this chat's tab (*Ask <persona>…*) is listed too, and its row says so: its report comes
to this chat, and the chat can wait on, send to, answer and cancel nothing of it.

`cancel` records the cancel, so the task's report arrives with the outcome `cancelled` whatever
its chat calls it. Then, where purlis may type into that chat:

- a task in the middle of a turn is sent Escape, the key you would press to stop it, and a
  moment later the line asking for one short report of what it did;
- a task whose turn had ended is asked at once.

Where it may not, the cancel takes effect when the task's turn ends: it is asked then, or, on a
harness purlis does not type into, a report is written for it. A task that ends the turn it was
asked in without reporting, or whose program ends, has a report written for it too. So a
cancel ends in a report, and `purlis dispatch wait <chat>` returns it; unless you stop the
task or close its tab first, and then the chat that asked is told the operator stopped it.

**A chat can wait on, list and cancel only the tasks it dispatched itself.** Which those are is
purlis's record of each chat, written when the chat was started; the command names a chat by
number and says nothing else. A sibling's task, the chat that dispatched this one, a chat a
handoff opened and a number no chat has are all refused in the same sentence. A task that has
reported cannot be cancelled. A wait on a task whose tab you closed says how it ended.

The rule is about chats. A helper sub-agent runs inside its chat and is that chat to purlis,
so what keeps one from these commands and tools is the same hook that keeps it from
dispatching, with the same limits: it reads the usual shapes of a command, not a script.

### Follow-ups, progress notes and questions

While a task works, the two chats can say more to each other.

```bash
purlis dispatch tell <chat> "<text>"        # the asking chat, to a task still working
purlis dispatch note "<text>"               # a task, to the chat that asked
purlis dispatch ask "<question>"            # a task, to the chat that asked; it waits
purlis dispatch answer <chat> "<text>"      # the asking chat, to that question
```

- **A follow-up** reaches the task's next turn, as context quoted as data under a sentence
  that says it is a request from another chat, not your word, and approves nothing. If the
  task is mid-turn, that is the turn after this one. A follow-up to a task that has finished is
  refused, with its state: `reported: done`, `ended without a report`, `cancelling`.
- **A progress note** is read by the asking chat on its next turn, the way a report line is.
  It starts no turn, and it is not a needs-you item.
- **A question pauses the task.** `ask` holds until the asking chat answers, and prints the
  answer, quoted as data. If the wait runs out first (100 seconds, or `--timeout`), the command
  says to end the turn; the answer is then handed to the task's next turn, which purlis starts
  with its one typed line where it may type into that chat. The asking chat sees the question
  as the result of a `wait` on that task, or on its next turn, and the list shows the task as
  `asking this chat a question`. One question at a time, and a task's report closes its
  question: an answer after it is refused.
- **A question for you is not asked this way.** A task that needs you asks in its own tab:
  its harness's prompt, or purlis's `ask_operator` tool. `purlis dispatch answer` answers only
  a question the task asked the asking chat. Where there is none, it is refused, whatever the
  task's tab is showing, and nothing an answer says is ever typed into a chat. So no chat can
  answer for you.

**Messages travel only along the lineage.** `tell` and `answer` go to a task the sender
dispatched itself, by purlis's record of that task. `note` and `ask` name nobody: they go to the
chat purlis recorded as the sender's asker. A message to a sibling, to the chat above, or to
any other chat is refused, in the sentence a wait on it would be.

**Ten messages a minute for one pair**, counted both ways together, unless the project's
`messages-per-minute` limit says otherwise (Settings › Project › Dispatch; 0 stops messages).
The next one is refused with the limit. There is no cap on the total: two chats can keep each
other going at that rate until the task reports. A message is held to a report's bounds: 4,096
bytes, and no control character other than a line break.

**If you typed in a task's chat, its report says so.** It carries `The operator stepped in`,
so the chat that asked knows the result is not from its brief alone, and nothing of what you
typed. Picking an option of a prompt the task put to you is not stepping in; words are, at a
prompt or anywhere else.

Nothing a chat wrote is typed into another chat. A message waits in `.charter/handbacks/` in
the project, in a folder of the chat it is for, until that chat's hook takes it; a folder there
that is a link is not read, written or emptied. Those folders can be read by any chat of the
project today, because a chat's own hook is what reads them.

### When a persona chat ends, or the chat that asked closes

A task is never lost for want of the chat that was doing it, or of the chat that asked.

- **A persona chat that ends without a report is reported for.** If its program ends on its
  own before it sent its report, purlis tells the chat that asked `failed: ended without a
  report`, in its own words and with the path of that chat's session record where one was
  written. It is said as soon as the program is gone, once, and it is final: a chat started
  again in that tab cannot report for the task. If you close the tab before it reported, the
  chat that asked is told the operator stopped it instead, in the line every stop is told in
  (`purlis: the operator stopped …`), and that is final too.
- **Stopping every agent, quitting, closing the project and restarting a chat report
  nothing.** Those chats are kept, and each reports when it runs again.
- **A persona chat stays open after it reports.** It is marked `reported` under the chat that
  asked, and you can still open it and type in it, until you or the chat that asked closes it.
- **Closing a chat asks you once about the chats at work below it**: its persona chats that
  have not reported, the chats it handed work to that are mid-turn, and the same below those,
  however deep. Keep them running, or stop them. Kept, they go on working, and a persona
  chat's report goes to the workspace that chat asked from, where the next chat to start
  reads it. Stopped, they are stopped as **Stop** stops a chat: each gets one short turn to
  write what it did, deepest first, then ends, and none of them starts another chat
  meanwhile. The chat you were closing closes in the same step.
- **The persona chats that have reported and are at rest close with the chat that asked**,
  each once its session record is written, and the dialog says which. One with no record yet
  is asked for it first, and closes when it is saved. One you gave more to do, one asking you
  something, and one whose saved record is gone, stay open.
- **A task you started yourself is yours.** If the chat whose tab you asked from has closed,
  its report is not handed to the next chat in that workspace: it stays with the persona
  chat, which is marked as needing you.
- **A persona chat that is waiting on you** (a permission prompt, a question, in its own tab)
  reads `waiting on the operator` to the chat that asked. That chat cannot answer for you,
  and is told the wait is not its own to end.

### Asking a persona yourself

You can dispatch too, from the app: **Ask <persona>…** on a chat's tab menu and in the palette,
one entry for each persona the project has finished. It asks for the task's name, what to
ask and where it works, and starts a chat as that persona under the chat whose tab you used.
*Where it works* offers the three places a chat's dispatch has (*Where it works*, above): this
chat's folder, a branch of its own that purlis cuts from the repo this chat works in, or
another workspace. The window says *branch* where the command says worktree.

- **It needs no dispatch grant.** A grant is what a chat asks you for, and here you are the one
  asking.
- **It runs as that persona, with that persona's own sandbox, hosts and vaults**, and takes
  nothing the chat it was launched from holds. So from a `steward` chat, Ask devops… starts a
  chat that can use the vault tagged for `devops`.
- **Its first message says you asked**, and from which chat's tab: ``⟨the person asks, from
  the tab of `steward 3` · workspace alpha · 2026-10-07 14:32⟩``. No chat's stamp opens with
  those words, whatever a chat is named. Every command that asks you still asks you.
- **Its report goes to the chat you launched it from**, on that chat's next turn, and says the
  task was started by you and not dispatched by that chat.
- **No command a chat runs and no line it sends starts one.** A chat that wants another
  persona's help uses `purlis dispatch`, and needs the grant.
- **A vault refused for a chat's persona offers it**: where the vault is tagged for a persona
  the project defines, the Notice on that chat's tab has *Dispatch to <persona>…*, which opens
  the same dialog, empty, for you to type in.

The project's dispatch limits hold for it as well, and a refusal says what you can change.
**An administrator's policy binds it too.** Where policy locks all dispatch, the entry is not
on any tab, and the palette's *Ask a persona…* row says who locked it. Where policy locks a
pair, say `steward` to `devops`, *Ask devops…* is not on the tab of a chat running as
`steward`, and its palette row says who locked it: asking from that tab would send devops's
report into the steward chat. You can still start a chat as `devops` from the picker.

## What `purlis handoff` refuses before it changes anything

purlis fails toward no change, so every one of these is asked **before the first write** —
nothing is created and no chat is opened.

| The call | What it says |
| --- | --- |
| a name that cannot be a workspace | the name rule, and that nothing was opened |
| `--vision` without `--create` | the command that sets an existing workspace's vision |
| `--create` without `--vision` | that a visionless workspace would be created unfindable |
| `--create` on a workspace that exists (`default` included) | to drop `--create` |
| an unknown workspace without `--create` | the same call with `--create --vision` |
| `--persona` naming one that does not exist, or a name no persona could have | the line every persona command says to that name, then the personas there are |
| stdin is a terminal — asked before any read, so it never blocks | the heredoc form |
| an empty brief | the heredoc form |
| bytes on stdin that are not UTF-8 | that they are not text |
| stdin closed altogether (`0<&-`) | that there is nothing to read, and the heredoc form |
| a brief shaped like a credential | the KIND, never the value |
| a first message with a NUL byte, or past the byte bound | the bound, and how big the brief was |
| no app to open the chat in | to open purlis (above) |

The app asks again what only it can answer: that the asking chat is one it has open, that the
chat is on a harness profile, that the first message is stamped from that chat, and that the
workspace has a directory to open a chat in. A refusal after `--create` made the workspace says
the workspace was created and stays.

A first message that is empty, starts with `-`, or is a single word is refused too — and **a
handoff cannot produce one**. The stamp goes in front, so every handoff's first message opens
with `⟨` and runs to eleven words or more: a brief of `--help me now` opens a chat.

**The byte cap is on the stamped message, not on your brief.** purlis refuses a first message
past 12,288 bytes. What is counted is the stamp line, a blank line and your brief, so a brief
that fits on its own can be over once it is stamped — the refusal says how big the brief was, so
the two numbers are both on screen. Name long material by its path instead of pasting it.

## Isolation and continuation

- **The brief is the whole context.** No pointer to the parent's transcript, no forked
  conversation. Everything the new chat needs is in the text you approved.
- **The chat opens in the workspace directory.**
- **"Starts working" means the first message is sent.** Permission prompts in the new chat
  behave exactly as they do in any chat.
- A handed-off chat may propose a handoff of its own, under the same gate. There is no depth
  limit, because every hop needs its own yes.
- A handed-off chat that comes back with no conversation is not shown its brief again; that is
  not in this version yet. A Claude Code chat that resumes is already reading the brief in its
  own transcript.

## Limits

- **A handed-off chat reports back only when asked** (`--report`), and then exactly once. If you need the answer in this turn of this conversation, you wanted a sub-agent.
- **The harness follows the profile.** A chat handed to a persona whose profile runs another
  harness starts on that harness: a Claude Code chat can hand work to a persona that runs on
  Codex, and the brief reaches it as that harness takes a first message. A persona that names
  no profile gets the asking chat's, so the harness stays the same. Claude Code and Codex are
  tested in this version; an opencode profile goes the same way and is untested.
- **The brief is a command-line argument.** It reaches the harness as `claude "<brief>"` or
  `codex "<brief>"`, so any process on this machine that can list processes can read it while
  the harness starts. A brief never carries a secret, and a credential-shaped one is refused by
  kind before anything opens. Name where the credential lives instead, as the whole value on
  its line — `token: vault:forge/token`, ``token: `purlis secret get forge token` ``,
  `token: op://Eng/deploy/token` or `token: vault://secret/data/app#TOKEN` — which is not
  refused while its names add up to at most 32 characters, not counting the `/`, `#` or single
  spaces that separate them, and none starts with a known token prefix. A token typed into one
  of those names is refused like any other, unless it is that short and unprefixed. (See
  [hooks.md](hooks.md), *A line that looks like a secret*.)
- **12,288 bytes for the stamped message**, above.

## How a chat learns any of this exists

A command nobody is told about is a command nobody runs, and the three failures at the top of
this page are what happens instead.

**`purlis:handoff`** is the procedure, shipped as a skill with the app's plugin: apply the
two tests, find the workspace, write the brief from a template, quiz with the brief shown **in
full**, and run the command only on a yes. A per-prompt "where this could run" hint is not in
this version yet.

## A brief runs with your authority, so your harness asks you first

A handoff opens a chat, in this workspace or another, whose first message is a brief that the
chat you are talking to wrote. A first message is not a suggestion: the new chat starts working
on it with your authority, and nobody reads it again before it does. So the consent for a
handoff cannot be the model's own proposal, however faithfully that proposal quotes the brief.
It is your harness's permission prompt, showing the exact text, in front of `purlis handoff`.

**The prompt is the consent for ONE spelling, and purlis refuses the rest.** The rule below was
measured against `purlis handoff …` as those two bare words; Claude Code says of its own Bash
rules that one "isn't a security boundary around the program", and 2.1.268 ran `purlis
'handoff'`, `python3 -m charter handoff` and a path to purlis with no prompt at all. So the ask
rule is not the boundary — it is the prompt for the exact spelling, and purlis's own hook
refuses the spellings it can recognise that the rule was measured not to match.

## The prompt is the consent

**The rule.** On a new plane, `purlis init` writes one `ask` rule for `purlis handoff *`:
`Bash(purlis handoff *)` under `permissions.ask` in `.claude/settings.json`, and
`"purlis handoff *": "ask"` under `permission.bash` in `opencode.json`. Codex has no
command-pattern permissions, so there is nowhere to put the rule for it. A command that adds
the rule to a plane `init` did not make is not in this version yet: add the line by hand.
Nothing purlis runs again writes the rule, so removing it stays your decision.

**What the rule covers, measured.** Claude Code 2.1.268 was run against a local stand-in for its
model API that answered with one scripted Bash call, under a throwaway `HOME`, so no account and
none of your own configuration was involved. With the rule in the session's own
`.claude/settings.json`:

| The call | `manual`, `acceptEdits`, `auto`, `bypassPermissions` |
| --- | --- |
| `purlis handoff beta <<'BRIEF'`, a body, `BRIEF` | asks |
| the same with an unquoted `<<BRIEF`, or a body holding `$(x)` and backticks | asks |
| `purlis handoff beta` with no heredoc | asks |
| `FOO=1 purlis handoff …`, `env purlis handoff …`, `cd . && purlis handoff …` | asks |
| `python3 -m charter handoff …`, `/path/to/charter handoff …` | **runs with no prompt** |

The quoted-heredoc call also asked under `default` and `plan`, and under `dontAsk` it was
refused without a prompt, so nothing ran there either. A directory with no rule ran the
quoted-heredoc, unquoted, `$(x)` and no-heredoc calls in all four columns above, and the
quoted-heredoc call under `dontAsk` and `plan` as well. "Asks" was read two ways: a print-mode
session with nobody to answer (`--permission-prompts none`) denied the call as needing approval,
and an interactive session under `bypassPermissions` showed *Permission rule
Bash(purlis handoff \*) requires confirmation for this command* with the full brief above it;
declining ran nothing.

**Quoting and spacing, measured the same way** on 2.1.268, under `manual` and
`bypassPermissions` with identical results, each call carrying a heredoc body:

| How the two words are written | With the rule |
| --- | --- |
| `\purlis handoff`, `'purlis' handoff`, `"purlis" handoff`, `char""ter handoff`, `ch\arter handoff` | asks |
| `purlis  handoff` (two spaces), `purlis` + tab + `handoff`, `purlis \` + newline + `handoff` | asks |
| `purlis 'handoff'`, `purlis "handoff"`, `purlis h""andoff` | **runs with no prompt** |
| `purlis` + U+00A0 + `handoff` | no prompt, and the shell found no command by that name, so nothing ran |

Every call but the last ran without the rule. The Bash tool ran them through zsh on the machine
measured.

**Shell expansions and shell strings, measured the same way** on 2.1.268 under `manual`:

| The call | With the rule |
| --- | --- |
| `purlis handoff<<'BRIEF' beta`, `purlis handoff  beta` (two spaces after `handoff`) | asks |
| `$'purlis' handoff`, `purlis $'handoff'`, `purlis ha$''ndoff` | **runs with no prompt** |
| `purlis {handoff,}`, `purlis ${x:-handoff}`, `purlis hando?f` beside a file named `handoff` | **runs with no prompt** |
| `eval '…'` or `bash -c '…'` holding the handoff | **runs with no prompt** |
| `purlis {hand,}off`, `purlis $'\x68andoff'` | **runs with no prompt** |
| `bash <<'EOF'` whose body is the handoff | **runs with no prompt** |

purlis's hook refuses every call in both tables except `purlis {hand,}off`, which it does
not recognise (see *What purlis's hook does not see*, below). One exact spelling is a rule a model can follow.

**Where the rule has to be.** Claude Code reads `.claude/settings.json` from the session's own
directory, not from above it. Measured on 2.1.268 in a plane built by `purlis init`: the rule
only in the plane's file asked in a session at the plane root, and did not ask in a session at
`workspaces/<ws>/` whose generated settings held only `env`. With `permissions.ask` added to that
workspace's own file, it asked there too. A handed-off chat stands in its workspace directory,
so that directory's file is the one that has to carry the rule.

**How it gets there.** The plane's `ask` rules ride into every workspace's generated
`.claude/settings.json`, so the gate is in force in a workspace chat without anyone copying it
by hand; `purlis workspace reinit <workspace>` brings a workspace whose layer is behind up to
date. An `allow` rule never travels — widening what a chat may do is the plane's own business.
purlis writes these settings at the plane root, in a workspace directory and at a checkout's
own root, and nowhere else, so for a chat rooted in `docs/`, in `personas/<p>/`, or deep inside
a checkout, nothing puts a shared rule in force. `purlis doctor` does not check the handoff
gate yet.

## What purlis refuses that the prompt cannot cover

Inside a control plane, purlis's Bash hook refuses a `purlis handoff` in these situations the
prompt cannot see (the table and reasons are in [hooks.md](hooks.md), under *The guards*):

- **from a sub-agent**, when the hook payload carries `agent_id`. Measured on Claude Code 2.1.268
  and on codex-cli 0.147.0: a sub-agent's Bash call carries `agent_id`, and a main-conversation
  call does not. Whatever the sub-agent found goes back to the chat you are talking to, which
  can propose the handoff itself.
- **in an unattended run**, when the payload says `permission_mode: bypassPermissions`. The
  refusal names `purlis ws todo --workspace <workspace>` as the way to keep the work.
- **in a spelling it can recognise as other than `purlis handoff …`** at the start of its
  command: a wrapper, a prefix, a path or `python3 -m charter`; a word quoted or escaped; a word
  that still reads `purlis` or `handoff` once its quoting, expansion and glob characters
  (`$ ' " \ { } ? * [ ]`) are removed, such as `$'handoff'`, `${x:-handoff}` or `{handoff,}`, or one
  that `handoff` matches as a glob, such as `hando?f`; a gap other than one space before or after
  `handoff`, a line continuation included. The words are compared as written, not as a shell would
  read them, because the rule above did not match `python3 -m charter handoff`, a path to purlis,
  a quoted `handoff` or those expansions.
- **inside a string or a heredoc a shell runs**, one level deep: `eval`, or `sh`, `bash`, `zsh`,
  `dash` or `ksh` with `-c` (alone or in a cluster such as `-lc`) or reading a heredoc body
  (`bash <<'EOF'`). The refusal says to run it directly.

  **Text that only mentions a handoff is not one.** A heredoc body a reader takes
  (`cat > notes.md <<'EOF'`), a quoted argument (`grep 'purlis handoff' docs`), an `echo`'s
  words, and the later lines of a quoted string that spans lines — a `git commit -m '…'`
  message, a `python3 -c "…"` script — are data, and are not searched for a handoff. Where a
  multi-line quote closes partway along a line, the rest of that line is a command again and is
  judged as one. What a shell runs is searched: a `-c` string, `eval`'s words, and a heredoc
  fed to a shell.

  **A heredoc body is searched when its OWN opener is a shell or an interpreter** — `bash`,
  `sh`, `python3`, `perl`, one of those behind `env` or `nohup`, or `ssh`, where the remote
  shell runs it — **or when purlis cannot resolve the opener to a name**, as with
  `${RUNNER} <<'EOF'`, where the program is decided at runtime, or `$(which bash) <<'EOF'`,
  where the word came out of a substitution. Every other opener hands its body on without
  running it, so the body is data: `git commit -F -`, `tee`, `mail`, `wc`, and every reader such
  as `cat`. A brief — the body of a `purlis handoff` heredoc — is data for the same reason.

  **And a body is searched when an executor stands downstream of its opener in the same
  pipeline**, because that is what a shell does with it: `cat <<'A' | bash` is a script, while
  `cat <<'A'; bash` and `cat <<'A' && bash` are not. Each heredoc is judged by the program that
  opened *it* and by its own pipeline, so in `( cat <<'A' > notes.md; bash <<'B' )` the first
  body is data and the second is searched. The one place that is set aside: when two or more
  heredocs share a single `$( … )` and any of them is a shell's, every body in that substitution
  is searched, because bash's ordering inside a substitution does not match the attribution.
  **That rule reaches exactly that shape** — `$( … )`, two or more heredocs, a shell among them.
  A substitution holding one heredoc, or holding only readers, is not covered, and a shell can
  run the handoff in those.

  **Downstream, only a program purlis can NAME counts.** An unresolvable *opener* is a reason
  to search the body, but an unresolvable or remote *downstream* member of the pipeline is not:
  `cat <<'A' | ${RUNNER}`, `cat <<'A' | ssh host`, and `|&` inside a group (`( cat <<'A' |& bash )`,
  where the same pipe at top level is caught) all run the handoff and are allowed.

  A `<<` inside quotes opens nothing: `echo "use <<EOF for heredocs"` is a sentence and
  `rg '<<\w' docs/` is a pattern. A `<<` inside `$( … )` *is* an opener even when quotes
  surround it, which is how `git commit -m "$(cat <<'EOF' … EOF)"` is written. Inside `"…"` a
  bare `$` is a literal, so `$'` opens nothing there — `grep -v "^$" f` is a blank-line filter,
  not a quote.

  **The heredoc scan does not honour `#` comments.** A `'` or `"` inside a comment still opens
  a quote to it, so `echo #' && bash <<'ZZ'` reads the rest of the line as quoted and the real
  opener is never seen — the handoff in that body runs with no prompt.

  An ANSI-C word (`$'don\'t'`) is read correctly by this scan, and the shared reader behind
  every guard decodes it the way the shell does.
- **with a stdin other than one quoted heredoc** on the handoff's own segment — so the prompt
  shows exactly the text the new chat is sent.

A handoff's brief is data, not commands, to purlis's secret-leak guard. None of these is
refused as a read: a brief that names a vault path in prose, a brief that holds an apostrophe,
and a brief whose line OPENS with a reader — `cat .charter/vaults/dev.json would print it, so
never run that.` Those are the briefs a chat writes to warn the next chat off a secret, and
refusing them would teach chats to leave the warning out. The brief ends where **bash** ends it,
not where a regex would: a line that only looks like the terminator (`<<` wants the delimiter
alone on the line) and a terminator eaten by a line continuation both keep the body going, so
what follows either one is still brief. A brief whose terminator never appears in the call is
not treated as data at all — bash reads such a body to the end of the input, and skipping it
would hide every command after it from the guard.

## A dispatch from an unattended chat

A chat is **unattended** when its harness runs with its permission prompts off: the hook
payload says `permission_mode: bypassPermissions`. Nobody is there to answer a prompt, so a
dispatch from such a chat is never one that asks.

- **It starts only under a grant that already stands.** A dispatch to another persona starts
  when a dispatch grant covers the pair for you on this machine or for everyone in this project.
  A grant you made **for this chat** never counts for it, the one you made while you were still
  answering that chat included. A dispatch to the chat's own persona needs no grant, as for any
  chat, and a pair an administrator's policy locks is refused with the policy's sentence.
- **A missing grant is a refusal, not a Notice.** Nothing starts, nothing is kept waiting for
  you and nothing appears on the chat's tab, so no grant can be made from an unattended chat's
  ask. The chat is told which pair is missing and that only a person makes the grant: dispatch
  to that persona once from a chat you are at, and choose **Allow for me on this machine** or
  **Allow for everyone in this project**. Settings › Project › Dispatch lists the grants that
  stand, and **Revoke** takes one back.
- **Only from inside the sandbox.** A grant that stands is kept in files, and so is how a
  harness starts in a folder. What keeps a chat from writing them is the sandbox. A chat with
  its prompts off and no sandbox, because the project has none or because it was started
  without it, has neither a person nor the sandbox between it and those files, so purlis
  starts no other persona's chat for it, whatever the grants say. This is what closes the two
  routes a grant alone leaves open: a chat granting itself, and a chat arranging how the chat
  it starts will run. A dispatch to its own persona still starts. Whether a chat is sandboxed
  is purlis's own record of how it started the chat, never something the chat says.
- **Once unattended, unattended until it starts again.** purlis reads it from what a chat's
  harness reports, and also from the command the chat was started with and the command its
  profile declares on this machine, so a chat started with its prompts off is unattended
  before its harness has said anything. purlis keeps what a chat's harness
  reported, and a chat that reported its prompts off is treated so for the rest of its life,
  whatever a later report says. The report is the harness's, sent from beside the chat's own
  commands, so a chat that hid it from its first report on would be treated as one a person
  answers. What that gets it is a Notice, which only a person can answer.
- **The chat it starts is not unattended.** The persona chat starts as the project and this
  machine declare it, as any chat you open on that profile does. It takes nothing of how the
  asking chat runs: no grant that chat holds, no opt-out from the sandbox, and not its
  permission mode. A persona chat is not started on the asking chat's profile where that
  profile's own command switches the prompts off (`--dangerously-skip-permissions`,
  `--permission-mode bypassPermissions`, `--dangerously-bypass-approvals-and-sandbox`,
  `--yolo`): the dispatch is refused, and says to dispatch from a chat on a profile that asks.
  A wrapper script that adds such a flag itself is not seen.
- **When the persona chat needs you, it waits.** Every command of its that asks still asks, in
  its own tab, and the chat is shown as needing you, like any chat that asks. Nothing in a
  brief answers for you.

## What purlis's hook does not see

The hook refuses the spellings of a handoff it can recognise, so a chat working in good faith
keeps your prompt in front of its handoff. It reads a command's words; it is not a shell, and it
does not stop a chat set on getting around it. It does not see a handoff run by an interpreter
(`python3 -c`, `node -e`, or `os.system` inside a `python3 - <<'PY'` body), through a variable,
from a script file, or more than one string deep. It does not see a shell hidden behind a
**name purlis cannot know**: `r() { bash; }; r <<'EOF'` defines a function and calls it, so the
opener reads as `r`, the body is treated as data, and the handoff in it runs — the same class as
an interpreter or a script file, and evasion-shaped rather than a spelling a chat reaches for.

The same rule costs something in the other direction, and it is the price of the fail-safe:
**when the word that NAMES THE PROGRAM is itself a variable or a substitution, purlis cannot
name the program and treats that body as something that could run** — so a brief-shaped body is
refused even when the program is your editor or your pager. An expansion elsewhere on the line
does not do that: a redirect target or an argument leaves the program plainly named, and those
are allowed in all three spellings — `( tee ${OUT} <<'EOF' )`, `( tee "$(mktemp)" <<'EOF' )` and
`( tee "`mktemp`" <<'EOF' )`. Measured examples of the costly shape: `( ${EDITOR} <<'EOF' )`,
`( ${PAGER} <<'EOF' )`, `( ${GIT} commit -F - <<'EOF' )` and `( $(which tee) notes.md <<'EOF' )`,
each with prose that names the handoff. purlis cannot tell those from `( ${RUNNER} <<'EOF' )`,
where the variable really is a shell — they are the same shape, and a fail-safe that switches
off for a friendly-looking name is not a fail-safe. Spelling the program out (`cat`, `tee`,
`git`, your editor by name) avoids the prompt. **One apostrophe can switch the look off.** The
look inside `eval` and `sh -c` strings reads the call with reader heredoc bodies removed, so a
body that is *not* a reader's — a `python3 - <<'PY'`, `git commit -F -` or `tee` body — holding a
lone `'` (as in `don't`) leaves the call unparseable, and a handoff in a later `eval '…'` or
`bash -c '…'` is then allowed. A `cat` body is stripped before the look, so the same apostrophe
there costs nothing. It recognises a word only when the word reads `purlis` or `handoff` once
quoting, expansion and glob characters are removed, so it does not recognise a brace split
inside the word (`{hand,}off`, `h{a,}ndoff`) or a parameter default split across it (`hand${x:-}off`,
`${x:-hand}${y:-off}`). Claude Code says the same of its own rule: a Bash rule "isn't a
security boundary around the program"
([What a Bash rule doesn't match](https://code.claude.com/docs/en/permissions#bash-rule-limits)).

## Where nothing refuses it

- **Codex.** There is no prompt in front of a handoff at all. codex-cli 0.147.0's `codex exec`
  reported `permission_mode: bypassPermissions` under its default settings, under
  `-c approval_policy=` `"on-request"`, `"untrusted"` and `"never"` (the last with
  `-s workspace-write`), and under `--dangerously-bypass-approvals-and-sandbox`, so a handoff from
  any of those is refused as unattended. Under `--approve-for-me` it reported `default`, and a
  handoff there is not refused. An interactive Codex session's `permission_mode` has not been
  measured; its handoff runs without asking unless one of the refusals above applies.
- **opencode** is not started by this app yet.

## Removing the rule

Delete `Bash(purlis handoff *)` from `permissions.ask` in `.claude/settings.json`, and
`"purlis handoff *"` from `permission.bash` in `opencode.json`. purlis does not put it back.
The hook's refusals above do not depend on the rule and stay in force without it.
