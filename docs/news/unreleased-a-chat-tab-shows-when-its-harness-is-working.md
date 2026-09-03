---
version: unreleased
headline: a chat tab spins while that chat's harness is working — the turn's start was already on disk, the Stop hook that ends it was not
---

The chat bar says which chats exist and which one you are in. It did not say which of them
was *doing* anything, so the only way to find out whether the chat in the next tab had
finished was to switch to it and look.

```
before
  chats   api.1  *api.2   api.3   docs.1  +

after — api.1 and docs.1 are working, api.3 is not
  chats  ✻api.1  *api.2   api.3  ✻docs.1  +
```

## charter owns the harness's hooks, and half of this already worked

charter does not own the harness's *screen* — ADR 0018 permits reading that pane at
*"exactly two moments, both of which are moments the pane is about to stop existing"*. It
owns the harness's *hooks*, and that turns out to be enough:

* `hooks/hooks.json` wires `UserPromptSubmit`. The hook process runs inside the chat's own
  harness pane, and the launcher created that window with `-e CHARTER_SESSION_ID=<chat id>`
  — so a hook has always known exactly which chat it is in.
* `hooks.userpromptsubmit` has called `notify.plane_changed()` since it was written. **The
  turn-start edge was already on disk.**

**The whole gap was the falling edge.** `Stop` in `hooks/hooks.json` ran `charter workspace
_autosave`, and `hooks._HANDLERS` had eleven entries — `sessionstart`, `userpromptsubmit`,
four `pretooluse*`, five `posttooluse*` — and **no `stop` at all**. Nothing charter raised
could be lowered, which is why the thing to build was one handler and a tracker, not a
mechanism for watching a terminal.

`Stop`, and deliberately not `SubagentStop`. They share the autosave beside this because a
memo is worth writing when either fires; they must not share this one, because a dispatched
sub-agent finishing does not end the turn that dispatched it — on a fan-out the tab would
blink off once per worker.

## A turn that nobody ends: ten minutes, and why that number

Pressing Esc mid-turn fires no `Stop`. Without a fallback the mark would stand for the rest
of the plane's life, so a mark decays: `inflight.TURN_STALE_SECONDS`, **ten minutes**.

That is not "how long may a turn take". Every `pretooluse*` and `posttooluse*` handler
refreshes the mark, so a turn issuing tool calls refreshes its own TTL indefinitely and is
never cut off. What ten minutes bounds is a stretch **with no tool call whatsoever**.

**The limit is real and cannot be measured away.** A long *toolless* think refreshes
nothing, so a TTL short enough to catch an abandoned turn also blinks off during deep
thinking; that tab comes back on the turn's next tool call. Both errors cost you the same
single switch to look, and they differ in how they end — a blink-off is repaired by the
next tool call, a stale spinner stands until the number runs out. So the number errs on the
side charter always errs on: *not claiming*. `TheLimitIsAToollessThink` is a test rather
than a paragraph, so moving the number means coming back and restating the trade.

Its own constant, not `inflight.PRESUMED_DEAD_SECONDS`, for the reason that number's own
header gives about #308: two jobs with opposite horizons sharing one number is what made
the single old TTL wrong. That one is a *display* threshold — the record survives it and is
drawn differently (`45m?`), because a dispatch that has outlived every expectation is the
most interesting thing that tracker holds. This one is the opposite: past it charter does
not know, and the honest picture for *does not know* is the picture for *is not*.

## A chat on a harness charter cannot hear the end from shows nothing

opencode sets `$CHARTER_SESSION_ID` on its tool hooks and has no session-stop event —
`opencode.SHIM` hooks exactly `shell.env`, `tool.execute.before` and `tool.execute.after`.
Codex is tool-hooks only. Neither can dispatch `stop`, and neither can dispatch
`userpromptsubmit` either, which `tests/test_opencode_dispatches_every_hook` has recorded
as a declared deficit since #433.

A mark charter can raise and cannot lower is not a working light — it is a recency mark,
claiming *now* while measuring *recently*, and you cannot tell the two apart by looking. So
the rising edge is gated on the harness by name and a non-Claude chat is drawn exactly as
it is drawn today: nothing. `state.harness_session` already answers `None` rather than
guessing for these same harnesses, and *"charter does not know which harness this is"*
reaches the same picture as *"charter knows this one reports no stop"*, on purpose.

## The spinner takes the mark's cell, and the strip does not move

Every tab already carries a one-cell prefix — `*` for the chat you are in, a blank for the
others. The spinner goes **in that cell**, so a chat starting a turn changes not one column
of the strip: the cut, the row count and the click map all answer exactly what they
answered when nothing was working.

That matters more here than it sounds. `slots.TABS` resolves a click **by column**, so a
spinner drawn *beside* a name would re-cut the strip the moment a sibling started thinking,
and the cell you were about to press would hold another chat's name a moment later — the
double-press #767 exists to prevent, arriving through an animation. It is asserted at every
width from 0 to 120: the map and the row widths are identical with two chats working and
with none.

The active tab keeps its `*`. One cell, two facts, and `*` wins because it is the only one
of the two with no other way to be seen — `[frame] chrome = "off"` is the shipped default
and `NO_COLOR` strips the reverse-video block — and because the chat you are typing in is
the one whose harness you can already watch.

### Three of the six glyphs that were asked for did not go on the row

The request was Claude Code's own spinner, `· ✢ ✶ ✳ ✽ ✻`. `tui.width` answers **1** for all
six, and on this row `tui.width` is not the whole question: it reads the East-Asian tables,
and an *Ambiguous* character is one a terminal may draw two cells wide while those tables
say one. That is what `slots._BAR_RULE` is ASCII to avoid, and `statusline._persona_chips`
records two Ambiguous glyphs breaking this project's layout already.

| glyph | codepoint | East-Asian width | verdict |
|---|---|---|---|
| `·` | U+00B7 | **Ambiguous** | refused |
| `✢` | U+2722 | Neutral | kept |
| `✳` | U+2733 | Neutral | refused — carries an emoji presentation variant (`✳️`), so a terminal with an emoji fallback font may draw it wide |
| `✶` | U+2736 | Neutral | kept |
| `✽` | U+273D | **Ambiguous** | refused |
| `✻` | U+273B | Neutral | kept |

What ships is `✢ ✶ ✻ ✶` — the three that survive, cycled out and back so the sparkle grows
and shrinks the way the requested one does. Neutral is the property `statusline`'s own
marker test asserts (`▪▸▫`, chosen after `◈`/`◆` shipped broken), and it is the strongest
one available short of ASCII.

**ASCII was tried first and is worse here**, for a reason that is not about width at all: a
chat id is `[A-Za-z0-9._-]`, so a pulse like `.oOo` draws `Oapi.3` and puts a character you
read as part of a name where the name begins. None of the three that shipped is a character
a chat id may contain, and that is asserted as its own property.

## What it costs, measured on this machine

The strip does not tick unless a chat is actually working, and learning that costs one
`stat`:

| | measured | at 5 Hz |
|---|---|---|
| `inflight.turn_stamp()`, nothing working | 4.6 µs | 23 µs/s — **0.002% of a core** |
| `inflight.working_chats()`, 3 marks | 31.9 µs | — |
| `slots.chats_bar`, 3 chats, none working | 429.8 µs | 0.215% of a core |
| `slots.chats_bar`, 3 chats, two working | 454.1 µs | **0.227% of a core** |
| `inflight.turn_bump()`, one hook | 11.5 µs | per tool call |

An idle plane pays the 4.8 µs and nothing else, forever — and only on the pane that draws
the strip. Against `slots.ANIMATED`'s own recorded budget, *"one `render("right")` costs
4 816 µs … at 5Hz that one pane alone is ~2.4% of a core"*, an animated chat strip is a
tenth of what this frame already animates.

A refresh is free by construction: `turn_bump` is a `utime` on a file that already exists,
which does not move the tracker directory's mtime — so a turn issuing a tool call a second
re-reads nothing. Only a turn *starting* and a turn *ending* cost a panel a read.

#780 is not what this contradicts. That finding prices a **tmux round trip** at ~7–13 ms and
is why a renderer may not make one; `slots.chats_bar` calls `chats.roster` and nothing else,
and `frame/chats.py` says outright that *"nothing here touches tmux and nothing here starts
anything"*. It settles the read, not the tick.

## Its own gate, not the one `bottom` uses

`panel._watch` ticks an animated slot while `_running(inflight_cache) or
_notice_pending(fid)` — plane-wide in-flight dispatches, and this frame's own switch-notice
dwell. Neither has anything to do with whether a sibling chat's harness is thinking: a plane
with no dispatch running would never tick the strip, and one with a dispatch running would
tick it for half an hour with every tab idle.

So `chats` gets a second set (`slots.BAR_ANIMATED`) and a gate of its own, and each panel
pays for exactly the gate its own renderer draws from. `OnlyTheChatStripSpins` keeps that
honest the way `OnlyTheAnimatedSlotAnimates` does: every renderer in the frame is drawn
twice at two clock readings with a chat marked as working, and the output must differ for
exactly the names in the set.

## Where it lives

A second tracker in `charter/inflight.py`, keyed by **chat**, beside the one keyed by
**agent**. They are not merged: a dispatch record is `{"agent", "kind", "ts"}` — *"no fid,
no chat, no workspace"* — which is the right shape for the dispatch-overlap nudge that reads
agent names back to you as a sentence, and a chat id reaching that nudge is #420's
wrong-and-confident failure arriving through the other axis.

Nothing leaves the plane and nothing is committed: one **empty** file per working chat under
`.charter/chat-turns/`, made through `config.touch_for` like every other piece of charter's
own state. It carries its meaning in existing and in its mtime, which is that helper's own
sentence and is exact here — the chat's id is the file's *name*, and the TTL is its mtime,
so there is nothing left for bytes to say and a reader never opens one.

That works because a name that could not be a chat id is **refused rather than repaired**.
`$CHARTER_SESSION_ID` is an environment variable, so the value that becomes a filename is
one anything in that shell can choose; `_safe_name` is asked as a question there instead of
being used as a mangling, so `../../.ssh/authorized_keys` gets no mark at all rather than a
flattened one — the same answer `chats.check` gives a name off an argv. `.` and `..` spell
themselves and are refused by name on top of that, because a chat "called" `..` would
otherwise have the falling edge unlink a directory. The cost is stated rather than hidden: a
chat id longer than 64 characters gets no mark, and shows what every chat showed before this
existed.
