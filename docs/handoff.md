# A handoff's brief runs with your authority, so your harness asks you first

A handoff opens a chat, in this workspace or another, whose first message is a brief that the
chat you are talking to wrote. A first message is not a suggestion: the new chat starts working
on it with your authority, and nobody reads it again before it does. So the consent for a
handoff cannot be the model's own proposal, however faithfully that proposal quotes the brief.
It is your harness's permission prompt, showing the exact text, in front of `charter handoff`.

## The prompt is the consent

**The rule.** On a new plane, `charter init` writes one `ask` rule for `charter handoff *`:
`Bash(charter handoff *)` under `permissions.ask` in `.claude/settings.json`, and
`"charter handoff *": "ask"` under `permission.bash` in `opencode.json`. It is the one
permission rule `init` writes. Codex has no command-pattern permissions, so there is nowhere to
put the rule for it; `init` lists nothing for Codex, and `charter doctor` names the gap.

A plane `init` did not make adopts the rule with the command its news entry names:

```bash
charter guard handoff
```

That is `charter guard ask 'charter handoff *'` with nothing to type, and either one writes the
same rule to every harness that can hold it, or to none. The short form exists because a news
entry's `adopt:` line cannot carry quotes. `charter update` does not write the rule, and neither
does anything else that runs again. Removing the rule is your decision, and a writer that ran on
every update would quietly reverse it.

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

charter's hook refuses every call in both tables except `charter {hand,}off` and
`charter $'\x68andoff'`, which it does not recognise (see *What charter's hook does not see*,
below). One exact spelling is a rule a model can follow.

**Where the rule has to be.** Claude Code reads `.claude/settings.json` from the session's own
directory, not from above it. Measured on 2.1.268 in a plane built by `charter init`: the rule
only in the plane's file asked in a session at the plane root, and did not ask in a session at
`workspaces/<ws>/` whose generated settings held only `env`. With `permissions.ask` added to that
workspace's own file, it asked there too. A chat a frame opens stands in its workspace
directory, so that directory's file is the one that has to carry the rule. `charter doctor` run
from such a chat reads that file, and its `handoff gate` row warns when the rule is not there.

## What charter refuses that the prompt cannot cover

Inside a control plane, charter's Bash hook refuses a `charter handoff` in four situations the
prompt cannot see (the table and reasons are in [hooks.md](hooks.md), under *The guards*):

- **from a sub-agent**, when the hook payload carries `agent_id`. Measured on Claude Code 2.1.268
  and on codex-cli 0.147.0: a sub-agent's Bash call carries `agent_id`, and a main-conversation
  call does not. Whatever the sub-agent found goes back to the chat you are talking to, which
  can propose the handoff itself.
- **in an unattended run**, when the payload says `permission_mode: bypassPermissions`. The
  refusal names `charter ws todo` as the way to keep the work.
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

  An ANSI-C word (`$'don\'t'`) is read correctly by this scan, but the shared lexer behind the
  secret-leak guard cannot parse one. A call carrying it keeps every heredoc body visible to
  *that* guard, so prose in a brief on such a line can be refused as a read — measured on this
  branch, on 2b59d8a and on main, and identical in all three. For the leak guard that errs
  toward refusing. It is not a claim about this gate: the same scan feeds both, and while it
  mis-read `$'` inside `"…"` it erased real openers and let handoffs through. That is fixed and
  pinned in both directions.
- **with a stdin other than one quoted heredoc** on the handoff's own segment — so the prompt
  shows exactly the text the new chat is sent.

A handoff's brief is data, not commands, to charter's secret-leak guard. None of these is
refused as a read any more: a brief that names a vault path in prose, a brief that holds an
apostrophe (which used to leave the call unparseable, so the whole of it was scanned as raw
text), and a brief whose line OPENS with a reader — `cat .charter/vaults/dev.json would
print it, so never run that.` Those are the briefs a chat writes to warn the next chat off
a secret, and refusing them taught chats to leave the warning out. The brief ends where **bash** ends it, not
where a regex would: a line that only looks like the terminator (`<<` wants the delimiter alone
on the line) and a terminator eaten by a line continuation both keep the body going, so what
follows either one is still brief. A brief whose terminator never appears in the call is not
treated as data at all — bash reads such a body to the end of the input, and skipping it would
hide every command after it from the guard.

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
**when charter cannot name the program that opens a heredoc, it treats that body as something
that could run**, so a brief-shaped body behind `${VAR}` or `$( … )` is refused even when the
program is your editor or your pager. Measured examples: `( ${EDITOR} <<'EOF' )`,
`( ${PAGER} <<'EOF' )`, `( ${GIT} commit -F - <<'EOF' )` and `( $(which tee) notes.md <<'EOF' )`,
each with prose that names the handoff. charter cannot tell those from `( ${RUNNER} <<'EOF' )`,
where the variable really is a shell — they are the same shape, and a fail-safe that switches
off for a friendly-looking name is not a fail-safe. Spelling the program out (`cat`, `tee`,
`git`, your editor by name) avoids the prompt. **One apostrophe can switch the look off.** The
look inside `eval` and `sh -c` strings reads the call with reader heredoc bodies removed, so a
body that is *not* a reader's — a `python3 - <<'PY'`, `git commit -F -` or `tee` body — holding a
lone `'` (as in `don't`) leaves the call unparseable, and a handoff in a later `eval '…'` or
`bash -c '…'` is then allowed. A `cat` body is stripped before the look, so the same apostrophe
there costs nothing. It
recognises a word only when the word reads `charter` or `handoff` once quoting, expansion and glob
characters are removed, so it does not recognise a brace split inside the word (`{hand,}off`,
`h{a,}ndoff`), an ANSI-C escape (`$'\x68andoff'`, `$'\x63harter'`) or a parameter default split
across it (`hand${x:-}off`, `${x:-hand}${y:-off}`). Claude Code says the same of its own rule: a Bash rule "isn't a
security boundary around the program"
([What a Bash rule doesn't match](https://code.claude.com/docs/en/permissions#bash-rule-limits)).

## Where nothing refuses it

- **opencode.** The payload charter's plugin builds carries neither `agent_id` nor
  `permission_mode`, so a handoff from a sub-agent or an unattended run is not refused, and the
  `ask` rule in `opencode.json` is the whole gate. How opencode matches the spellings above has
  not been measured.
- **Codex.** There is no prompt in front of a handoff at all. codex-cli 0.147.0's `codex exec`
  reported `permission_mode: bypassPermissions` under its default settings, under
  `-c approval_policy=` `"on-request"`, `"untrusted"` and `"never"` (the last with
  `-s workspace-write`), and under `--dangerously-bypass-approvals-and-sandbox`, so a handoff from
  any of those is refused as unattended. Under `--approve-for-me` it reported `default`, and a
  handoff there is not refused. An interactive Codex session's `permission_mode` has not been
  measured; its handoff runs without asking unless one of the refusals above applies.

## Removing the rule

Delete `Bash(charter handoff *)` from `permissions.ask` in `.claude/settings.json`, and
`"charter handoff *"` from `permission.bash` in `opencode.json`. charter does not put it back.
`charter doctor`'s `handoff gate` row then warns `no ask rule for `charter handoff` under
claude-code, opencode`, names the command that adds it, and says the removal is your choice. It
is a warning, so `doctor`'s exit status does not change. The hook's refusals above do not depend
on the rule and stay in force without it.
