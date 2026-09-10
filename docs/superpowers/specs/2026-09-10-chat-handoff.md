# A chat hands work to a new chat, and the new one starts working

**Status:** agreed 2026-09-10, in a grill between the operator and the `steward` persona;
amended the same day by the controller's rulings on the implementation plan's open questions
(each marked _ruling_ below).
**Depends on:** #936 — a handed-off chat must be born with its workspace fixed — and the fix
that makes a restrictive permission rule reach a chat started in a workspace (below, *Consent*).

## The failure

You are in a chat doing one thing and ask for another. Three things happen today, and all
three cost you:

- the model does it here, so one workspace's todos, memory and branch now carry two tasks;
- the model hands it to a sub-agent, and the answer you wanted to talk to comes back as a
  paragraph folded into this conversation and then is gone;
- the model tells you to open a chat yourself, and you retype the context it already had.

Nothing can open a chat from inside one. `charter frame-new-chat` opens only in the
presser's own workspace, sends nothing and moves the view; `charter claude …` run from an
agent's Bash tool has no terminal, so it executes a bare harness inside that tool call.

## What charter decides, and what it does not

charter has no model and makes no judgement about the content of work (CONTEXT.md). Where
a request should run is the harness model's call. charter supplies the facts, the two tests
below, and the mechanism — and never names the answer (ADR 0016).

There are three placements:

1. **A sub-agent** — the harness's own. charter never gates, rewrites or converts an Agent
   call; nothing in this spec touches one.
2. **A new chat in this workspace.**
3. **A new chat in another workspace**, existing or new.

2 and 3 are one mechanism, a **handoff**, because a chat belongs to its workspace for life
(phase-5 spec §4j): the only thing that differs is the workspace.

## Language

- **Chat** — a frame tab: one harness conversation, in one workspace, for life.
- **Handoff** — opening a chat, here or in another workspace, whose first message is a
  brief the operator approved.
- **Brief** — the self-contained message a handoff carries; the only context the new chat
  starts with.

_Avoid:_ session (for a chat), spawn, sub-session. `dispatch.handoffs_since_first_advice`,
which uses "handoff" for a sub-agent dispatch, is renamed `routed_since_first_advice`
(_ruling_).

## The two tests

1. **Sub-agent or chat — who reads the result?** If this chat needs the answer to continue,
   it is a sub-agent. If the operator will read it and talk to it, it is a chat. There is
   no report-back channel from a handed-off chat: a parent that needs the answer wanted a
   sub-agent.
2. **This workspace or another — does the ask serve this workspace's vision?** Yes → a chat
   here. No → another workspace, proposed by matching the ask against other workspaces'
   visions. A workspace with no vision is never proposed; `default` is never proposed; a
   workspace whose vision says it is delivered is proposed only when the ask reopens its
   task. The proposal always also offers a new workspace with a name and a vision. These are
   rules for the *proposal* — the command refuses none of them, so a chat already in
   `default` can still hand off within it (_ruling_).

## The command

```
charter handoff <workspace> [--create --vision "<vision>"] [--persona <name>] <<'BRIEF'
<the brief>
BRIEF
```

- **The workspace is always named**, the current one included — the permission prompt has
  to say where the chat goes, and `.` says nothing.
- An unknown workspace without `--create` is refused, naming `--create --vision`. `--create`
  on an existing workspace is refused. A created workspace is LOCAL.
- **The brief arrives on stdin**, as one quoted heredoc in the same Bash call, so the
  permission prompt shows the exact text that will be sent. There is no `--brief-file`: a
  prompt that shows a path is an approval of a path. A brief may still *name* files.
- The harness is the calling chat's. The persona is the one a new chat in that workspace
  gets when nothing is pinned: the launch empties `CHARTER_WORKSPACE` and `CHARTER_PERSONA`,
  so a pinned caller cannot file the new chat under its own workspace (_ruling_).
  `--persona` overrides, visibly in the prompt. There is no `--repo`: the brief says what to
  clone, and the new chat owns its own setup.

### Refusals

| Situation | Where it is refused |
|---|---|
| an empty brief | the command |
| a brief past the measured size bound (plan measurement M2) — say so and point at paths inside the brief | the command |
| a brief shaped like a credential — name the kind, never the value (the brief is world-readable argv while the harness starts) | the command (_ruling_) |
| outside a frame, or inside the operator's own tmux | the command — prints the exact command to run in a new terminal |
| unattended (`permission_mode: bypassPermissions`) | the PreToolUse guard |
| called from a sub-agent (the payload carries `agent_id`) — Claude Code; Codex once measured | the PreToolUse guard |
| any spelling other than `charter handoff …`, e.g. `python3 -m charter handoff` — the host rule would not match it | the PreToolUse guard (_ruling_) |
| a stdin that is not one quoted heredoc in the same call — the prompt would not show the text | the PreToolUse guard (_ruling_) |

A chat nobody can see is silence by construction (CONTEXT.md), which is why nothing opens
detached. The command printed outside a frame stamps its source as `chat none` when there is
no `$CHARTER_SESSION_ID`, and carries the persona as a `CHARTER_PERSONA=<name>` prefix
(_ruling_).

### What a handoff does, in order

1. With `--create`, creates the workspace and records its vision.
2. Records a todo in the target workspace whose title is the brief's first line. If that
   todo is already listed, records nothing and says so — a second chat may be exactly what
   was approved (_ruling_).
3. Opens a chat in the target workspace **in the background** — no client moves; the new
   window gets its panels before anyone looks, as a reopened chat does. Its first message is
   the stamp line, a blank line, then the brief verbatim.
4. The chat is born with its workspace fixed (#936): it asks no workspace question.
5. Keeps the full brief in the chat's private state, never committed, and carries it in the
   reopen manifest so it survives the chat directory being reaped (_ruling_).
6. Appends `{"event": "handoff", "ts", "placement", "created"}` to the dispatch tally — no
   workspace name, because a LOCAL workspace's name must not reach a committed file
   (_ruling_) — and clears `routing: require`'s pending mark for the calling chat's turn,
   in `hooks.pretooluse`, for every harness.
7. Moves the target workspace to the front of the plane's recorded tab order.
8. Marks the target workspace tab as arrived (below) and names the new chat on the
   attention row. A handoff into the workspace you are already in moves its tab and marks
   nothing: you are on it, and the chats strip shows the new tab (_ruling_).
9. Prints the new chat's id and workspace.

### The stamp

```
⟨handoff from chat <source-chat> · workspace <source-workspace> · <YYYY-MM-DD HH:MM>⟩
```

Facts charter knows, no instruction. The new chat, and whoever reads it later, can tell the
first message was not typed there.

## Isolation and continuation

- The brief is the whole context: no pointer to the parent's transcript, no forked
  conversation.
- A handed-off chat that will write **claims its own piece** (`charter wt add <repo>
  <piece>`) before writing. The handoff never pre-creates a worktree: the claimant is the
  creator (ADR 0011). This applies wherever the chat lands — any workspace with other chats
  has the same two-chats-one-clone risk. The chat opens in the workspace directory.
- A Claude Code chat reopens with its conversation (`charter reopen`). A chat whose
  conversation reopens empty is shown its brief at SessionStart as a labelled data block,
  never re-sent as a message. **opencode has no SessionStart**, so an opencode chat that
  reopens empty cannot be shown it (_ruling_ — stated as a limit).
- "Starts working" means the first message is sent. Permission prompts behave exactly as
  they do in any chat; the arrived mark stays until someone looks.

## Consent

The brief becomes the new chat's first user message — it runs with the operator's authority.
So:

- The model proposes with a quiz that shows the brief in full.
- **The gate is the harness's own permission prompt:** an `ask` rule for `charter handoff`
  (ADR 0014). An explicit `ask` rule prompts in every Claude Code mode, `bypassPermissions`
  and `auto` included — a measured precondition (plan G1); if a heredoc body defeats the
  host's match, the gate falls back to charter's own PreToolUse `ask`, the policy the host
  cannot express (ADR 0015).
- **A restrictive rule has to reach the chat that runs the command.** A framed chat's
  session root is its workspace directory, Claude Code reads settings only from there, and
  charter mirrors only `enabledPlugins` and `env` into it — so today no `ask` rule, the
  operator's own `charter guard ask` rules included, is in force in a workspace chat. The
  prerequisite fix mirrors `permissions.ask` and `permissions.deny` into the generated
  workspace settings. Grants (`allow`) never travel sideways: that would put a permission in
  force where nobody clicked for it (_ruling_).
- `charter init` writes the handoff rule on a new plane. An existing plane adopts it through
  the news entry's `adopt:` line, which the update skill walks; `charter update` itself writes
  nothing, so removing the rule stays the operator's choice. `charter doctor` names the rule
  when it is missing (_ruling_).
- A handed-off chat may propose a handoff of its own, under the same gate. There is no depth
  limit, because every hop needs a yes.

## Advice: "Where this could run"

- The UserPromptSubmit roster block widens into **Where this could run**: the persona roster
  as today, this workspace's vision, the two tests, and the `charter:handoff` skill. Facts,
  never a pick. It fires on the Commitment point's trigger and no longer requires the acting
  persona to declare `routing`; the roster rows keep their own conditions.
- **`charter:handoff`** ships as a skill: the procedure (apply the tests → write the brief →
  quiz with the brief shown → run the command) and the brief template — goal, what is known
  (with paths), done when, constraints, and the claim-a-piece line.
- **`charter workspace list`** gains a vision column: the untruncated first line, as the
  trailing field, because the model matches against those words (_ruling_).

## The strip

- **Arrival move:** when a handoff lands in a workspace, that workspace moves to the front
  of the plane's recorded tab order. Nothing else moves a tab mid-run — a tab never moves
  under a press (#767), and the plane keeps one order (#923).
- **Arrived mark:** the target tab is drawn in the `ok` accent **and** carries a glyph in the
  strip's reserved mark cell, so the mark survives `NO_COLOR` without shifting a tab
  (_ruling_). tmux draws a pane identically for every client of its session, so the mark
  clears plane-wide on the first switch, focus or attach into that workspace — not per client
  (_ruling_). It is the first slice of §4g (charter opens like an IDE). No bell.

## Limits

- A handed-off chat never reports back to the chat that opened it.
- No handoff into a different harness (#538's backlog).
- No "wants you" mark for a chat that stopped at a prompt after you visited it, and no
  per-client arrived mark — both are the next §4g slice.
- The sub-agent and unattended refusals read the harness's hook payload. Claude Code carries
  both; Codex's `agent_id` is unmeasured; where a harness does not expose them, `doctor` names
  the gap rather than charter pretending to enforce it.
- An opencode chat that reopens empty is not shown its brief.
- The brief reaches the harness as a command-line argument (`claude "<brief>"`,
  `codex "<brief>"`, `opencode --prompt "<brief>"`), so any process on this machine that can
  list processes can read it while the harness starts. A brief never carries a secret, and a
  credential-shaped one is refused.

## Build order

Each step is its own PR — tests first, an independent adversarial review, merged on green.
No version bump and no tag. **The gate lands before the command** (_ruling_), so `main` never
carries a `charter handoff` that nothing asks about.

0. #936 — a framed chat's launch-recorded workspace is its lock; SessionStart resolves by the
   chat id.
0b. Restrictive permission rules (`ask`, `deny`) reach a chat started in a workspace.
1. The frame opens a chat in a named workspace in the background, with a first message
   (plan Task 1).
2. The gate — the default `ask` rule (`init`, news `adopt:`), the `doctor` check, the guard's
   refusals, and the command-shape recogniser the refusals and the command share (plan Task 4).
3. `charter handoff` — the command, its own refusals, the stamp, the todo, the private brief,
   the tally event (plan Task 2).
4. The strip — the arrival move and the arrived mark (plan Task 3).
5. The advice — the widened block, the `charter:handoff` skill, the vision column, a
   reopened empty chat shown its brief (plan Task 5).
6. The words — CONTEXT.md, the docs pages, an ADR for the consent rule, `unreleased-*` news
   (plan Task 6).
