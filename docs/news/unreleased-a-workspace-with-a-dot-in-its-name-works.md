---
version: unreleased
headline: A workspace with a dot in its name can open a second chat, and stops writing its identity on another workspace's frame
---

`charter workspace create api.2` is a name charter accepts. Opening a second chat in that
workspace returned 1 with nothing to explain it, and the frame's own identity was written
onto a *different* workspace's tmux session at rc 0.

## A session name is not a string tmux reads as a string

tmux parses a `-t` argument as `session:window.pane`, so a dot in a name is a separator
before it is a character. `state.workspace_prefix` turns a workspace into the tmux session
name, and the two tmux versions charter supports disagree about what happens next:

**tmux 3.7c keeps the dot and then splits on it.**

```
$ tmux … new-window -d -a -t api.2 -n api.2.2 …
can't specify pane here                                   rc=1
$ tmux … set-environment -t api.2 CHARTER_SESSION_ID api.2.1
                                                          rc=0
$ tmux … show-environment -t api CHARTER_SESSION_ID
CHARTER_SESSION_ID=api.2.1        ← a sibling workspace's session
```

The first is `cmd_launch` adding the workspace's second chat, so a dotted workspace was a
one-chat workspace. The second is the silent-success class again — `$CHARTER_SESSION_ID`
is what the hotkey bind and every palette action resolve themselves from, so that is one
frame's identity handed to another, reported as having worked.

**tmux 3.2 — the floor — does not keep it at all.** `new-session -s api.2` creates a
session actually named `api_2`, so charter asked for one name and got another, and every
later target missed the session it had just made.

## Why the fix is not at the target

A trailing `:` disambiguates on 3.7c — `-t 'api.2:'` resolves the session and lands
correctly. On 3.2 it answers `can't find session: api.2`, because there is no such session
to find. A rule that works on one of two supported versions is not a rule.

So the fix is where charter *chooses the identifier*: `workspace_prefix` no longer mints a
dot. The workspace keeps its own name — `WORKSPACE_NAME_RE` is untouched and nothing an
operator may call a thing has been narrowed — and the session derived from it is `api_2`,
which is what 3.2 was going to spell anyway and which 3.7c gives back unchanged.

**Narrowing the alphabet instead would have been the cheaper edit and the worse one.** It
is shared with the change-slug alphabet, and a plane that already had such a workspace
would not get a refusal — it would get a workspace that stops resolving, `frame_workspace`
answering `None` and `workspace_dir` refusing, and the thing simply vanishing. Measured on
this machine: 15 workspaces across six clones, none with a dot. Nothing is gained by
forbidding the input and something real is risked.

## And one dot fewer in a chat id

A chat of `api.2` is now `api_2.1` rather than `api.2.1`, so the only dot in a chat id is
`state._CHAT_SEP` — the ordinal separator. `chats._order` reads an ordinal off the last
dot, and now there is only one to read.

## The refusal that could not reach the screen

`_say_on_screen` aimed `display-message -t <chat id>`, and a chat id is *always* dotted.
Measured on 3.7c, charter's own shape: `-t harness-wrapper.2` resolves to session
`harness-wrapper`, its **current** window, and `2` as a pane index — and
`-t harness-wrapper.9` answers rc 0 with pane index 0 rather than failing. The session half
happened to be the right screen, which is why nothing was ever seen to go wrong; the target
never once meant what it was spelled to mean.

There is no spelling that fixes a window — `session:window` needs the session, and this
side has none in hand — so it now aims at the record charter already keeps of where the
frame's harness runs, which is a `%N` and cannot be parsed as anything else. A frame whose
pane record is unusable is told nothing rather than told on somebody else's screen.
