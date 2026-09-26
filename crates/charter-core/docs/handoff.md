# You asked one chat for a second thing, and it did it there

You are in a chat about the API and you ask about the deploy script. Three things happen,
and all three cost you:

- the model does it here, so one workspace's todos, memory and branch now carry two tasks;
- the model hands it to a sub-agent, and the answer you wanted to talk to comes back as a
  paragraph folded into this conversation and is then gone;
- the model tells you to open a chat yourself, and you retype the context it already had.

`charter handoff` is the fourth way: a chat in a workspace you name, opened in the app without
taking your screen, already working on a brief you read and approved.

## Three places a request can run, and the two questions that pick one

1. **A sub-agent** — your harness's own. charter never gates, rewrites or converts an Agent
   call; nothing on this page touches one.
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
No → another workspace, matched against other workspaces' visions (`charter workspace list`,
then `charter workspace vision -w <name>`). charter supplies the facts and never names the
answer — and these are rules for the *proposal*, not for the command. `charter handoff`
refuses none of them, so a chat already in `default` can still hand off within it.

## The command

```bash
charter handoff <workspace> --name "<short task>" [--report] [--create --vision "<vision>"] [--persona <name>] <<'BRIEF'
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

Every question is asked before anything is written — see *What `charter handoff` refuses*
below. Then, from a chat the app started:

1. The command asks the app, over the chat's hook socket, to open the chat.
2. With `--create`, the app creates the workspace and records its vision.
3. The app opens a chat in the target workspace's directory, on **the same harness profile as
   the chat that asked** — read from the app's own record of that chat, never from the request.
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

**Your yes to the prompt in front of `charter handoff` is the only one asked for.** The app
opens one chat per `charter handoff`: the command asks the app for a single-use ticket and
spends it on the same connection, so no single line on the socket opens a chat and no line can
be replayed. The ticket cannot tell the command you approved from another process running
inside the same chat, which could run `charter handoff` itself, just as it can already start a
harness in the background with `claude -p`. That is why a handed-off chat always lands as a
tab you can see, stamped with the chat it came from.

**Outside a chat the app started**, or when the app is not listening or does not answer,
nothing is opened and nothing is created. The command says so, tells you to open charter and
either run the handoff again from a chat the app started or start a chat in that workspace
from the window, and exits 1. If the app refuses, you get one more line saying why.

## The stamp

```
⟨handoff from <source chat's name> · workspace <source-workspace> · <YYYY-MM-DD HH:MM>⟩
```

Facts charter can observe, and no instruction. The new chat — and whoever reads the transcript
later — can tell the first message was not typed there. The source is named the way you see it:
the name you gave that chat, or its default, `steward 3` — never charter's number for it.

`charter handoff` writes the stamp with that number (`⟨handoff from chat 16 · …⟩`), because the
number is what the app checks it against: the app refuses to open a chat whose first message
does not carry the stamp of a handoff from the asking chat. Having checked it, the app writes the
chat's name in its place. Minutes, not seconds: the stamp is read by a person deciding whether
this is the message they approved a moment ago.

The same note is on the new chat's tab, as its tooltip, and in its pane's corner:
`↳ from steward 3 · platform-next`.

## A report back

A handoff made with `--report` owes the chat that made it **one report**. The new chat's first
message says so under the stamp, and says how: it finishes with

```bash
charter handoff report "<a few lines on what was done and what was found>"
```

The report reaches the chat that asked in two ways, and **neither types anything into it**, so
a chat in the middle of a turn is never interrupted:

- **a needs-you item** on that chat, `<name> reported back`. Its Go opens the chat that asked;
  its Ignore takes the item away and leaves the report where it is;
- **context on that chat's next turn.** When you next prompt it, its `UserPromptSubmit` hook
  hands the turn the report as `additionalContext`, each line quoted behind `> ` under a
  sentence that says it is what another chat said and not an instruction. A report is handed
  to one turn, and to no turn after it.

**The pairing is charter's.** The app records, when it opens the chat, which chat asked; the
report names no recipient, so no chat can send its report anywhere but back to the chat that
asked. A report is refused, saying why, from a chat no handoff opened, from a handoff made
without `--report`, and a second time from the same handoff, whatever happens in between —
prompting that chat again does not re-arm it; another answer needs another `--report`
handoff. It is refused before anything is sent when it is empty,
past 4,096 bytes, or holds a control character other than a line break or an invisible one.

**If the chat that asked has closed**, the report is kept for the workspace it asked from, and
the next chat to start there learns it at its `SessionStart`. A report that was still waiting
when its chat closed goes the same way.

Reports wait in `.charter/handbacks/` in the plane, one file each, until a hook takes them.

`charter handoff report` runs under the same prompt as every `charter handoff`, so you see the
report before it is sent. It needs no heredoc — its text is the command's own argument, which
the prompt shows as it is — and a live command substitution in it (`"$(cat notes.md)"`) is
refused, because that text is not the one the prompt showed. `charter handoff report <<'BRIEF'`,
with no summary after `report`, is still a handoff into a workspace called `report`.

## What `charter handoff` refuses before it changes anything

charter fails toward no change, so every one of these is asked **before the first write** —
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
| no app to open the chat in | to open charter (above) |

The app asks again what only it can answer: that the asking chat is one it has open, that the
chat is on a harness profile, that the first message is stamped from that chat, and that the
workspace has a directory to open a chat in. A refusal after `--create` made the workspace says
the workspace was created and stays.

A first message that is empty, starts with `-`, or is a single word is refused too — and **a
handoff cannot produce one**. The stamp goes in front, so every handoff's first message opens
with `⟨` and runs to eleven words or more: a brief of `--help me now` opens a chat.

**The byte cap is on the stamped message, not on your brief.** charter refuses a first message
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
- **The same harness only.** A Claude Code chat hands off to a Claude Code chat.
- **The brief is a command-line argument.** It reaches the harness as `claude "<brief>"` or
  `codex "<brief>"`, so any process on this machine that can list processes can read it while
  the harness starts. A brief never carries a secret, and a credential-shaped one is refused by
  kind before anything opens. Name where the credential lives instead, as the whole value on
  its line — `token: vault:forge/token`, ``token: `charter secret get forge token` ``,
  `token: op://Eng/deploy/token` or `token: vault://secret/data/app#TOKEN` — which is not
  refused while its names add up to at most 32 characters, not counting the `/`, `#` or single
  spaces that separate them, and none starts with a known token prefix. A token typed into one
  of those names is refused like any other, unless it is that short and unprefixed. (See
  [hooks.md](hooks.md), *A line that looks like a secret*.)
- **12,288 bytes for the stamped message**, above.

## How a chat learns any of this exists

A command nobody is told about is a command nobody runs, and the three failures at the top of
this page are what happens instead.

**`charter:handoff`** is the procedure, shipped as a skill with the app's plugin: apply the
two tests, find the workspace, write the brief from a template, quiz with the brief shown **in
full**, and run the command only on a yes. A per-prompt "where this could run" hint is not in
this version yet.

## A brief runs with your authority, so your harness asks you first

A handoff opens a chat, in this workspace or another, whose first message is a brief that the
chat you are talking to wrote. A first message is not a suggestion: the new chat starts working
on it with your authority, and nobody reads it again before it does. So the consent for a
handoff cannot be the model's own proposal, however faithfully that proposal quotes the brief.
It is your harness's permission prompt, showing the exact text, in front of `charter handoff`.

**The prompt is the consent for ONE spelling, and charter refuses the rest.** The rule below was
measured against `charter handoff …` as those two bare words; Claude Code says of its own Bash
rules that one "isn't a security boundary around the program", and 2.1.268 ran `charter
'handoff'`, `python3 -m charter handoff` and a path to charter with no prompt at all. So the ask
rule is not the boundary — it is the prompt for the exact spelling, and charter's own hook
refuses the spellings it can recognise that the rule was measured not to match.

## The prompt is the consent

**The rule.** On a new plane, `charter init` writes one `ask` rule for `charter handoff *`:
`Bash(charter handoff *)` under `permissions.ask` in `.claude/settings.json`, and
`"charter handoff *": "ask"` under `permission.bash` in `opencode.json`. Codex has no
command-pattern permissions, so there is nowhere to put the rule for it. A command that adds
the rule to a plane `init` did not make is not in this version yet: add the line by hand.
Nothing charter runs again writes the rule, so removing it stays your decision.

**What the rule covers, measured.** Claude Code 2.1.268 was run against a local stand-in for its
model API that answered with one scripted Bash call, under a throwaway `HOME`, so no account and
none of your own configuration was involved. With the rule in the session's own
`.claude/settings.json`:

| The call | `manual`, `acceptEdits`, `auto`, `bypassPermissions` |
| --- | --- |
| `charter handoff beta <<'BRIEF'`, a body, `BRIEF` | asks |
| the same with an unquoted `<<BRIEF`, or a body holding `$(x)` and backticks | asks |
| `charter handoff beta` with no heredoc | asks |
| `FOO=1 charter handoff …`, `env charter handoff …`, `cd . && charter handoff …` | asks |
| `python3 -m charter handoff …`, `/path/to/charter handoff …` | **runs with no prompt** |

The quoted-heredoc call also asked under `default` and `plan`, and under `dontAsk` it was
refused without a prompt, so nothing ran there either. A directory with no rule ran the
quoted-heredoc, unquoted, `$(x)` and no-heredoc calls in all four columns above, and the
quoted-heredoc call under `dontAsk` and `plan` as well. "Asks" was read two ways: a print-mode
session with nobody to answer (`--permission-prompts none`) denied the call as needing approval,
and an interactive session under `bypassPermissions` showed *Permission rule
Bash(charter handoff \*) requires confirmation for this command* with the full brief above it;
declining ran nothing.

**Quoting and spacing, measured the same way** on 2.1.268, under `manual` and
`bypassPermissions` with identical results, each call carrying a heredoc body:

| How the two words are written | With the rule |
| --- | --- |
| `\charter handoff`, `'charter' handoff`, `"charter" handoff`, `char""ter handoff`, `ch\arter handoff` | asks |
| `charter  handoff` (two spaces), `charter` + tab + `handoff`, `charter \` + newline + `handoff` | asks |
| `charter 'handoff'`, `charter "handoff"`, `charter h""andoff` | **runs with no prompt** |
| `charter` + U+00A0 + `handoff` | no prompt, and the shell found no command by that name, so nothing ran |

Every call but the last ran without the rule. The Bash tool ran them through zsh on the machine
measured.

**Shell expansions and shell strings, measured the same way** on 2.1.268 under `manual`:

| The call | With the rule |
| --- | --- |
| `charter handoff<<'BRIEF' beta`, `charter handoff  beta` (two spaces after `handoff`) | asks |
| `$'charter' handoff`, `charter $'handoff'`, `charter ha$''ndoff` | **runs with no prompt** |
| `charter {handoff,}`, `charter ${x:-handoff}`, `charter hando?f` beside a file named `handoff` | **runs with no prompt** |
| `eval '…'` or `bash -c '…'` holding the handoff | **runs with no prompt** |
| `charter {hand,}off`, `charter $'\x68andoff'` | **runs with no prompt** |
| `bash <<'EOF'` whose body is the handoff | **runs with no prompt** |

charter's hook refuses every call in both tables except `charter {hand,}off`, which it does
not recognise (see *What charter's hook does not see*, below). One exact spelling is a rule a model can follow.

**Where the rule has to be.** Claude Code reads `.claude/settings.json` from the session's own
directory, not from above it. Measured on 2.1.268 in a plane built by `charter init`: the rule
only in the plane's file asked in a session at the plane root, and did not ask in a session at
`workspaces/<ws>/` whose generated settings held only `env`. With `permissions.ask` added to that
workspace's own file, it asked there too. A handed-off chat stands in its workspace directory,
so that directory's file is the one that has to carry the rule.

**How it gets there.** The plane's `ask` rules ride into every workspace's generated
`.claude/settings.json`, so the gate is in force in a workspace chat without anyone copying it
by hand; `charter workspace reinit <workspace>` brings a workspace whose layer is behind up to
date. An `allow` rule never travels — widening what a chat may do is the plane's own business.
charter writes these settings at the plane root, in a workspace directory and at a checkout's
own root, and nowhere else, so for a chat rooted in `docs/`, in `personas/<p>/`, or deep inside
a checkout, nothing puts a shared rule in force. `charter doctor` does not check the handoff
gate yet.

## What charter refuses that the prompt cannot cover

Inside a control plane, charter's Bash hook refuses a `charter handoff` in these situations the
prompt cannot see (the table and reasons are in [hooks.md](hooks.md), under *The guards*):

- **from a sub-agent**, when the hook payload carries `agent_id`. Measured on Claude Code 2.1.268
  and on codex-cli 0.147.0: a sub-agent's Bash call carries `agent_id`, and a main-conversation
  call does not. Whatever the sub-agent found goes back to the chat you are talking to, which
  can propose the handoff itself.
- **in an unattended run**, when the payload says `permission_mode: bypassPermissions`. The
  refusal names `charter ws todo --workspace <workspace>` as the way to keep the work.
- **in a spelling it can recognise as other than `charter handoff …`** at the start of its
  command: a wrapper, a prefix, a path or `python3 -m charter`; a word quoted or escaped; a word
  that still reads `charter` or `handoff` once its quoting, expansion and glob characters
  (`$ ' " \ { } ? * [ ]`) are removed, such as `$'handoff'`, `${x:-handoff}` or `{handoff,}`, or one
  that `handoff` matches as a glob, such as `hando?f`; a gap other than one space before or after
  `handoff`, a line continuation included. The words are compared as written, not as a shell would
  read them, because the rule above did not match `python3 -m charter handoff`, a path to charter,
  a quoted `handoff` or those expansions.
- **inside a string or a heredoc a shell runs**, one level deep: `eval`, or `sh`, `bash`, `zsh`,
  `dash` or `ksh` with `-c` (alone or in a cluster such as `-lc`) or reading a heredoc body
  (`bash <<'EOF'`). The refusal says to run it directly.

  **Text that only mentions a handoff is not one.** A heredoc body a reader takes
  (`cat > notes.md <<'EOF'`), a quoted argument (`grep 'charter handoff' docs`), an `echo`'s
  words, and the later lines of a quoted string that spans lines — a `git commit -m '…'`
  message, a `python3 -c "…"` script — are data, and are not searched for a handoff. Where a
  multi-line quote closes partway along a line, the rest of that line is a command again and is
  judged as one. What a shell runs is searched: a `-c` string, `eval`'s words, and a heredoc
  fed to a shell.

  **A heredoc body is searched when its OWN opener is a shell or an interpreter** — `bash`,
  `sh`, `python3`, `perl`, one of those behind `env` or `nohup`, or `ssh`, where the remote
  shell runs it — **or when charter cannot resolve the opener to a name**, as with
  `${RUNNER} <<'EOF'`, where the program is decided at runtime, or `$(which bash) <<'EOF'`,
  where the word came out of a substitution. Every other opener hands its body on without
  running it, so the body is data: `git commit -F -`, `tee`, `mail`, `wc`, and every reader such
  as `cat`. A brief — the body of a `charter handoff` heredoc — is data for the same reason.

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

  **Downstream, only a program charter can NAME counts.** An unresolvable *opener* is a reason
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

A handoff's brief is data, not commands, to charter's secret-leak guard. None of these is
refused as a read: a brief that names a vault path in prose, a brief that holds an apostrophe,
and a brief whose line OPENS with a reader — `cat .charter/vaults/dev.json would print it, so
never run that.` Those are the briefs a chat writes to warn the next chat off a secret, and
refusing them would teach chats to leave the warning out. The brief ends where **bash** ends it,
not where a regex would: a line that only looks like the terminator (`<<` wants the delimiter
alone on the line) and a terminator eaten by a line continuation both keep the body going, so
what follows either one is still brief. A brief whose terminator never appears in the call is
not treated as data at all — bash reads such a body to the end of the input, and skipping it
would hide every command after it from the guard.

## What charter's hook does not see

The hook refuses the spellings of a handoff it can recognise, so a chat working in good faith
keeps your prompt in front of its handoff. It reads a command's words; it is not a shell, and it
does not stop a chat set on getting around it. It does not see a handoff run by an interpreter
(`python3 -c`, `node -e`, or `os.system` inside a `python3 - <<'PY'` body), through a variable,
from a script file, or more than one string deep. It does not see a shell hidden behind a
**name charter cannot know**: `r() { bash; }; r <<'EOF'` defines a function and calls it, so the
opener reads as `r`, the body is treated as data, and the handoff in it runs — the same class as
an interpreter or a script file, and evasion-shaped rather than a spelling a chat reaches for.

The same rule costs something in the other direction, and it is the price of the fail-safe:
**when the word that NAMES THE PROGRAM is itself a variable or a substitution, charter cannot
name the program and treats that body as something that could run** — so a brief-shaped body is
refused even when the program is your editor or your pager. An expansion elsewhere on the line
does not do that: a redirect target or an argument leaves the program plainly named, and those
are allowed in all three spellings — `( tee ${OUT} <<'EOF' )`, `( tee "$(mktemp)" <<'EOF' )` and
`( tee "`mktemp`" <<'EOF' )`. Measured examples of the costly shape: `( ${EDITOR} <<'EOF' )`,
`( ${PAGER} <<'EOF' )`, `( ${GIT} commit -F - <<'EOF' )` and `( $(which tee) notes.md <<'EOF' )`,
each with prose that names the handoff. charter cannot tell those from `( ${RUNNER} <<'EOF' )`,
where the variable really is a shell — they are the same shape, and a fail-safe that switches
off for a friendly-looking name is not a fail-safe. Spelling the program out (`cat`, `tee`,
`git`, your editor by name) avoids the prompt. **One apostrophe can switch the look off.** The
look inside `eval` and `sh -c` strings reads the call with reader heredoc bodies removed, so a
body that is *not* a reader's — a `python3 - <<'PY'`, `git commit -F -` or `tee` body — holding a
lone `'` (as in `don't`) leaves the call unparseable, and a handoff in a later `eval '…'` or
`bash -c '…'` is then allowed. A `cat` body is stripped before the look, so the same apostrophe
there costs nothing. It recognises a word only when the word reads `charter` or `handoff` once
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

Delete `Bash(charter handoff *)` from `permissions.ask` in `.claude/settings.json`, and
`"charter handoff *"` from `permission.bash` in `opencode.json`. charter does not put it back.
The hook's refusals above do not depend on the rule and stay in force without it.
