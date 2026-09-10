---
version: unreleased
headline: A Claude Code chat is briefed for the workspace it was opened in, instead of being told to pick one
---

Every new chat in a charter frame started its session with the wrong picture of where it
was. Measured on 0.60.0, in a chat launched in one workspace, its first context:

- told it **"Confirm the workspace before any repo work. No workspace is locked for this
  session yet (it would otherwise default to `default`)"**,
- listed `default`'s open todos as its own, and
- listed its own workspace among the *other* workspaces on the plane.

## Two ids, and the briefing asked by the wrong one

Inside a frame, a hook sees two session ids: the chat's, and the harness's own, which
arrives in the hook's payload. The workspace pointer, the lock and the record of what the
chat was launched for are all keyed on the chat's. The session briefing looked them up by
the harness's, missed every one, and fell through to `default`. The memory reminder that
fires after a run of edits made the same mistake.

## And following the advice split the chat

Only a chat whose workspace you *picked* at launch was locked. Every other chat had no lock:
`charter claude -w api`, a launch that took its workspace from a pointer, a reopened chat.
So `charter workspace use <name>`, the command the briefing recommended, succeeded there.
The chat's commands then acted on the other workspace while its tab stayed in its own, and
`charter workspace use <own>` to put it back was refused.

## What is true now

- **A chat is locked to the workspace it was launched in**, picked or not. Its briefing
  shows that workspace's todos, never lists it as another one, and asks nothing. If the
  shell that opened the chat had `$CHARTER_WORKSPACE` set, the chat is locked to that pin
  instead, which is the precedence that variable has everywhere else in charter.
- **`charter workspace use <other>` inside a chat is refused**, and says what to do: open a
  chat in that workspace (its tab, or `F2 → workspace`), or pass `--workspace <other>` for
  a single command. `charter workspace use <own>` still succeeds.
- **`charter workspace unlock` inside a chat refuses** instead of reporting a release that
  did not happen.
- `--force` still moves a chat's commands for anyone who means it. It never moves the chat,
  and going back needs no second `--force`.

An agent spawned from a chat inherits the chat's session id, so the lock holds for it too:
its `workspace use` would have written the chat's own pointer. Agents work by the directory
they stand in, which outranks every pointer, or by `--workspace`, and neither is affected.

Outside a frame nothing changes. A session is still locked by the workspace it confirms.

**Not opencode yet.** Its plugin replaces `$CHARTER_SESSION_ID` with opencode's own session
id in every shell and hook it spawns, so inside a frame charter cannot tell which chat it is
in. An opencode chat holds no lock, is still told to pick a workspace, and
`charter workspace use <other>` in it still succeeds. That is pre-existing, and it is
tracked as #946.
