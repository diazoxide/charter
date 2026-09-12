# Charter may run the harness, but never draws it

ADR 0015 settled where charter sits relative to a harness it lives *inside*: "Charter
targets harnesses, not one host. It keeps only the policy the current harness cannot
express… where a harness's ceiling is lower, `charter doctor` names the deficit by name."
That ADR's whole shape assumes charter is a guest — a hook, a status line, a plugin —
running inside a process someone else started.

`charter <harness>` inverts the relationship it describes. Charter is no longer only the
guest; for this command it is the process that **starts** the harness, in a frame it
composes around it — the harness in the middle, charter's own panels on the edges. That is
a second question ADR 0015 never had to answer, and answering it wrong twice — first by
guessing wrong about what the boundary even is — is why it gets its own record rather than
a paragraph appended to that one.

## The candidate that looked obvious

The natural instinct, having already built `charter/tui.py` and a status line, is to keep
going: read the harness's own stdout, parse its escape sequences, draw the whole frame with
one renderer charter owns end to end. A spike built exactly that — a Textual widget backed
by `pyte`, a terminal-in-a-terminal, running `claude` and `opencode` inside it.

It worked. Both harnesses rendered correctly. That is precisely why this had to be settled
by measurement rather than argument: a design that fails outright is easy to reject, and
this one did not fail.

## What was measured

darwin, Python 3.14.4, `textual` 8.2.8, `pyte` 0.8.2, tmux 3.7c, a 150×42 frame, one
agent-shaped corpus (an ordinary Claude Code session's own output, replayed).

| | Textual + pyte | tmux, end to end |
| --- | --- | --- |
| throughput | **1.85 MB/s** | **25.2 MB/s** |
| parse alone | 2.4 MB/s (0.9 MB/s with scrollback enabled) | ~37 MB/s |

Rendering itself was never the bottleneck on either side — Textual's own paint measured
7.2 ms/frame, a 138 fps ceiling nowhere close to being reached. The gap is the parse:
`pyte` interpreting the harness's escape sequences in Python, against tmux's own C parser
doing the identical job roughly fifteen times faster, and scrollback made `pyte` alone
almost three times slower again. Two implementations of the same well-specified problem —
one already correct and shipped in every environment charter runs on, one freshly written
in the language charter happens to be written in.

**Both arms rendered `claude` and `opencode` correctly.** This was a cost decision, not a
feasibility one — the kind of decision measurement resolves and argument alone does not,
because "which one is faster" and "which one is right" are different questions and only
the spike could answer the first.

## The half a benchmark cannot show

Speed is the half that is measurable in an afternoon. The half that is not: what the
Textual/pyte widget was, and was not, at the moment it was measured. It drew text. It did
not draw a cursor. Still owed, unwritten: mouse, scrollback beyond raw buffer access,
bracketed paste, OSC sequences, wide characters, and `?2026` (synchronized output) — each
one a real terminal behaviour a real coding-agent session produces, and each one a place
`pyte` could silently render something subtly wrong rather than fail loudly. `pyte` itself
was last released 2023-11-12 — a terminal parser is exactly the kind of code where a
one-cell drift in cursor math shows up as a garbled screen months later, on somebody else's
terminal, and the library that would need patching is not actively maintained.

This project ships `dependencies = []` — every dependency is a promise to keep re-checking,
by hand, forever. Owning a terminal emulator would mean owning the correctness of
`\x1b[?2026h`, of double-width CJK glyphs, of an OSC 8 hyperlink escape, indefinitely,
in a widget that at 120 lines had implemented perhaps a third of what a real one needs —
against tmux, which has already solved this problem, ships on every machine charter already
requires, and needs nothing from charter to keep solving it.

## The decision

**tmux composes the rectangles and does every part of terminal emulation. Charter draws
only its own panels — the edges — and never touches the harness's own pane: never reads
its output, never parses its escape sequences, never decides what a cursor or a colour
means inside it.** `charter/frame/tmuxctl.py` is the one module in the codebase allowed to
shell out to `tmux`, precisely so this boundary has exactly one place it could be crossed
by accident.

Read the other direction, the same rule is ADR 0015's boundary, moved: that ADR drew the
line at what a harness can express and let charter's own reach change harness by harness.
This one draws a line at what charter *runs*, and keeps that line fixed regardless of which
harness is on the other side of it — the frame is identical whether the pane inside it is
`claude`, `codex`, or a command charter has never met (`charter frame -- <cmd>`), because
charter never has to understand what is in that pane to draw around it.

## Consequences

* Charter's frame code needs no terminal-emulation dependency at all — `dependencies = []`
  survives this feature.
* The floor for `charter <harness>` is tmux's own version (3.2 for the frame's menu, 3.3
  for its resize-recovery hook — `charter/frame/tmuxctl.py`), not a Python library's
  release cadence.
* Charter's panels stay simple by construction: a top/bottom strip is one line, measuring
  its own pane and repainting whole on every change charter's own hooks report — there is
  no cursor, no scrollback, no input focus for charter's own code to get subtly wrong,
  because none of that is charter's to draw.
* The harness's own pane keeps every terminal behaviour it already has, including ones
  charter has never heard of, because tmux is already handling it and charter never gets
  between the harness and the terminal it is actually talking to.
* A future feature that genuinely needs to read the harness's own output (say, to react to
  what it printed) needs its own measurement and its own ADR — this one settles rendering,
  not observation, and conflating the two is how a boundary like this erodes one convenient
  exception at a time.

## Amendment, 2026-09-01: charter reads that pane at two moments, and draws in it at none

The bullet above asked for a measurement and its own record before charter read the
harness's pane. **The measurement showed charter was already reading it**, and had been
since #384 — so what follows is a correction to this ADR's own description of the code
rather than a new permission granted to it.

`commands_frame._pane_last_words` runs `tmux capture-pane -p -S -` on the harness pane on
**both** launch paths, and its docstring records the 3.7c measurement that put it there: a
registered harness whose binary is missing produced *zero bytes* of output and exit 127, and
that capture is the only thing that turns it into a sentence. §4f of
`docs/superpowers/specs/2026-08-30-charter-opens-like-an-ide.md` then asked for a second
read: tmux history dies with its session, so quitting a plane discards every visible
transcript, and *"less invasive"* cannot mean that.

So the rule is stated as it actually holds, rather than as a prohibition with two
undocumented exceptions:

**tmux composes the rectangles and does every part of terminal emulation. Charter draws only
its own panels — the edges — and never draws in the harness's own pane, never parses its
escape sequences, and never decides what a cursor or a colour means inside it. It READS that
pane at exactly two moments, both of which are moments the pane is about to stop existing:**

1. **a harness that died before the frame was drawn** (`_pane_last_words`) — the only chance
   to say anything at all, because nothing is ever drawn on that path;
2. **a chat being stopped by `charter: quit`** (`_capture_transcript`) — bounded to the last
   2,000 lines and 512 KB, written to that chat's own file under `.charter/frame/`, and
   **offered on the way back rather than replayed**. `F2 → chat: previous transcript` opens
   it in a pager in a window of its own; the reopened harness's pane starts clean.

**Both exceptions are bounded by the same property, and it is the property that keeps this a
boundary rather than a preference: charter reads only what is about to be destroyed, and
writes nothing back.** The two failures this ADR was written against — owning a terminal
parser, and drawing where tmux draws — are untouched by either. Nothing here parses an
escape sequence: `-e` keeps them and `-N` keeps the trailing spaces `-e` alone trims, and the
bytes go to a file and to `less -R`, both of which understand them better than charter would.

**What is still refused, sharpened rather than repeated.** Reading that pane to *react* to
what it printed — a hook on its output, a parse of its state, a decision made from its
content — is still a different feature and still needs its own measurement and its own
record. The distinguishing question is now written down so the next reader does not have to
infer it: *does charter read this pane at a moment it is ending, and does it write nothing
back?* Two yeses is this amendment. Anything else is a new one.

**Measured cost, because "capture it" is not free.** One 200-column pane at charter's shipped
`history_limit = 50000` took the shared tmux server from 3.7 MB to **130 MB**, and
`capture-pane -p -S -` pipes that whole history through charter's own process. That is why
the capture asks tmux for the last N lines (`-S -2000`) rather than for everything and
trimming afterwards: the bound belongs where the memory is. Verified on tmux 3.7c and at the
3.2 floor.

## Amendment, 2026-09-12: before the `exec`, that pane is charter's own

Harness profiles put a charter process in the pane. `charter frame-launch --profile <name>`
is what tmux now starts in every chat pane, and it becomes the harness by `os.execvpe`
replacing itself with the profile's command (`charter/frame/launcher.py`). Before that
happens, charter may write on that screen, and there are exactly three things it writes.

**A refusal** — the local file became committable since the pre-tmux check, the command is
not on `PATH`, the `execvpe` itself raised — goes into the pane, and where somebody is at
the keyboard the launcher **holds the pane open until they press Enter.**

**A question** is the second, and it is the only one of the three that READS from the pane
as well as writing to it. A profile whose command or environment is new or has changed
since it last ran is not refused where somebody is in front of it: the launcher prints what
it would run and reads one line, `run this? [y/N]` (`charter/profiletrust.py`, and the
plan's *A new or changed command asks once*). A no starts nothing and exits on the
workspace picker's own cancel code, saying nothing more — the operator has just answered.
Where the question cannot be answered it is a refusal instead, which is the bound below.

**A claim to a chat that cannot be proven** is the third, and it is not a refusal either:
the launch goes ahead. `launcher.framed_chat()` asks tmux whether this process's own pid is
the `#{pane_pid}` of a live pane belonging to the chat it was handed, because `$TMUX_PANE`
and `$CHARTER_SESSION_ID` are inherited by a model's tool shell and prove nothing. A claim
that does not check prints one line saying so and returns `None`, and the launcher then runs
as a launch with **no frame** — it records nothing for that chat, which is the point of the
proof. Silently downgrading would cost a chat its session id and its resume id with nothing
said, so the sentence goes where the person watching is: the pane the harness is about to
take over.

All three are charter writing in a chat's pane — and the question reads from it too — which
the rule above says it never does. The rule is right and this is not an exception to it,
because of *when*: **no harness has ever run in that pane.** The `exec` has not happened —
or it was attempted and raised, which leaves the same pane with the same charter process in
it and no harness either way — so there is no
harness process, no output of its own on that screen, nothing to draw over and nothing to
parse. Every failure this ADR was written against needs a harness on the other side of the
pane to occur at all — owning a terminal parser, deciding what somebody else's cursor means,
painting over somebody else's frame — and none of them is reachable before the process that
would produce them exists.

**The distinguishing question, in the same shape as the one above:** *has a harness ever run
in this pane, and is charter's own process still the one in it?* No and yes is this
amendment. Anything else is the rule as written: the instant `execvpe` succeeds the pane is
the harness's, `state.record_launch` says so, and nothing charter owns writes there again.

**Why the refusal cannot simply be printed and exited on, which is the whole reason this is
a decision and not a detail.** Measured 2026-09-11 on tmux 3.7c and at the 3.2 floor, 40
runs, in the pull request that shipped this launcher
([#981](https://github.com/diazoxide/charter/pull/981)): `_launch`'s eager
`#{pane_dead_status}` ask completes **6-14 ms** after the start while a Python launcher's
first line runs at **19-22 ms**, so the ask is always too early to catch a refusal — and by
the time anything else could look, the chat-teardown hook has killed the window.
`_pane_last_words` answered `[]` in all 40 runs on charter's own server. A refusal printed
and exited on is a refusal nobody reads: the window carrying it is gone before the sentence
can be collected. So the pane has to hold it, and holding it is the part that needs this
record.

**Bounded, and each bound is checkable from outside.**

* **Only while no harness has ever run in that pane** — the distinguishing question above,
  and not the pair this amendment first wrote, *only a refusal, and only before the `exec`*.
  Each half of that pair is wrong exactly once: an unproven chat prints a line that is **not
  a refusal**, and an `execvpe` that RAISES prints its refusal **after** the exec was
  attempted. Neither leaves a harness in the pane, which is why the rule survives both and
  the pair did not. The bound is checkable from outside because there is one moment it
  turns: `state.record_launch`, written the instant the pane stops being charter's.
* **Three kinds of line, and charter wrote every one.** A refusal, plus `press Enter to
  close this chat.` where the pane waits; the approval prompt, which is the profile's
  command and environment on rows of their own and then `run this? [y/N]`; or the one line
  an unproven chat prints before launching anyway. No escape sequences of charter's own, no
  panel, no layout, and never a fourth thing later.
* **The prompt is escaped AND whole, and those are two promises.** Every byte of it outside
  printable ASCII comes out as a reversible escape (`contain.escaped`, ruling 35), because
  it comes from a file a chat can write and an ESC in it could redraw the question to show
  one command while another is approved. And **nothing is clipped**: a sentence bounds what
  it quotes, because a sentence ends in a remedy that a long value would push off the
  screen, but a prompt exists to be read before it is answered, and a command approved with
  its tail unseen is what the ask is against. It wraps. The refusal sentences are the other
  kind of surface: they quote a profile's name only to say which one, and bound it with
  `contain.readable`'s fixed `...` marker, as every refusal Task 2 shipped does. A row the
  operator chooses or approves from — a selector row, a doctor row — says how much it kept
  back; the prompt never has to.
* **The question is asked only where it can be answered.** Both of the pane's ends must be a
  terminal (`profiletrust.can_ask`), and the open must be an attended one. A pane that fails
  either test is refused with a sentence rather than asked, because a question nobody can
  answer is a chat that never starts and never says why — which is the same failure the wait
  below is bounded against, one moment earlier.
* **Every refusal the pane SAYS is recorded, and only the WAIT is conditional.**
  `_refused_in_pane` writes the sentence into the chat's state directory on every path it
  runs (`state.record_launch`), the attended one included, where the launch that opened the
  chat reports it. What an attended open adds is the wait — and that needs two conditions
  rather than one: the open must be ATTENDED **and** the pane's stdin must be a terminal,
  because `_wait_for_the_operator` returns at once when `sys.stdin.isatty()` is false. So an
  attended launcher run out of a pipe prints and exits, and a reopen, a handoff or a
  background open never stops at all.
* **A decline is the one exception, and it is not a hole.** `cmd_frame_launch` returns on it
  before `_refused_in_pane` runs, so nothing is said in the pane and nothing is written under
  the chat. Both are right: the operator answered this question themselves a moment ago and
  was told `charter: nothing started.` as they did, so there is no sentence they have not
  read. On charter's own server nobody reads the record for it either: a decline is
  reachable only from an ATTENDED open, and `_await_the_launcher` is asked only by an
  unattended one. **In an operator's tmux it reads worse, and says so here:**
  `_launch_in_operator_tmux` reads the record whether or not the open was attended, so a
  decline there that races the eager check, or a press whose streams are `/dev/null`,
  reports the window gone with charter's unknown-death code rather than "nothing started".
  That is a less exact sentence, not a hole — nothing ran, and the decline was answered on
  the terminal that asked. A refusal charter decided still records; the one the operator
  decided does not need to.
* **A line, not a keystroke.** Waiting on one keypress means putting the pane's terminal
  into raw mode, and a `tcsetattr` from a pane on a Linux CI runner left the launcher killed
  by a signal — an empty `#{pane_dead_status}` — so the refusal went with the window after
  all. The pane's own line discipline does the waiting instead: no mode change, nothing to
  restore, nothing of the terminal's state for charter to get wrong.
* **Reading the harness is untouched.** This amendment adds a WRITE before the harness
  exists, and one READ OF THE KEYBOARD — a line off the pane's stdin, through its own line
  discipline — which is not a reading of the pane at all: there is no output on that screen
  but charter's own, and nothing is parsed. The two reads of the harness's pane that the
  2026-09-01 amendment allows are unchanged, and charter still never reads this pane to
  react to what a harness printed in it.

Recorded here rather than in the records task that closes this phase, because the code that
relies on it shipped in the same pull request
([#981](https://github.com/diazoxide/charter/pull/981); the rule is *an ADR amendment ships
with the code that first relies on it*, `docs/superpowers/plans/2026-09-11-harness-profiles.md`,
Task 6): otherwise `main` carries an unqualified prohibition while the code contradicts
it, and the only thing telling a reader otherwise is a spec they have no reason to open.

*Corrected 2026-09-12 by that records task, against the shipped
`charter/frame/launcher.py`: the unproven-chat line named as one of the things charter
writes there, the `execvpe`-that-raised case covered, "only before the `exec`" replaced by
the distinguishing question, the wait's two conditions both stated, the record separated
from the wait, and the measurement cited where a reader can open it.*

*Extended 2026-09-12 by the task that added the approval
([#992](https://github.com/diazoxide/charter/pull/992)), under the same rule: the launcher
now ASKS in that pane as well as writing in it, so the question is named as the second thing
it writes, its containment and the two conditions for putting it are bounded, and the read
it makes is distinguished from the two reads of a harness's pane the 2026-09-01 amendment
allows.*
