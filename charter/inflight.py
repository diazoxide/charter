"""In-flight work tracking — the signal the completion tally cannot give.

``personas/_dispatch/`` records a dispatch when it **finishes**, so two dispatches
five minutes apart sequentially are indistinguishable from two that overlapped.
That makes it useless for the one failure it would be worth catching: two
code-writing personas editing the same working tree at once, which fails quietly
— no error, just interleaved edits and whichever commit lands last.

This records work when it **starts** and clears it when it ends, so overlap is
actually observable.

**Every record carries a KIND, and #420 is why.** #387 promised the frame "a
spinner while a dispatch, clone or `gl-refresh` runs"; only dispatches animated,
because :func:`start` had exactly one caller. Wiring the other two in was not a
one-liner: the SAME records feed the dispatch-overlap nudge through
:func:`still_running`, and a record named ``clone`` would have made that nudge
tell an operator *"`x` writes code and `clone` are already running"* — wrong, and
wrong in the confident, human-readable way that is worse than silence.

So a record says what it is, and every reader says which kinds it means. The
default everywhere is :data:`DISPATCH`, deliberately: the readers that must not
see a clone (the nudge, the per-persona chips, the session's own ``⚡ N``) get
that by NOT asking, so the next kind somebody invents cannot leak into them by
being forgotten. The frame's spinner is the one caller that opts into "anything
live" (``kind=None``), which is exactly what it is for.

A record written by a charter that predates the field reads as a
:data:`DISPATCH`, which is what it was.

Local and ephemeral: it lives under the state dir, is never committed, and holds
only an agent name, a kind and a timestamp — the same discipline as the committed
tally, which deliberately stores counts and dates, never prompt text.

Everything here is best-effort. A tracker that breaks a turn is worse than one
that misses an overlap.
"""

from __future__ import annotations

import json
import os
import tempfile
import time
from pathlib import Path

# One number used to do both jobs, and doing both is what made it wrong (#308). A record
# past the old TTL was DELETED, so the single most interesting thing this tracker can hold
# — a dispatch that has outlived every reasonable expectation — rendered as nothing at all,
# and irreversibly: "presumed dead" and "never happened" were the same picture. The two
# jobs want opposite horizons, so they get their own numbers.

#: A dispatch still marked in-flight after this long is **presumed dead** — the process was
#: killed, or PostToolUse never fired. Still returned, flagged, and drawn (`45m?`); charter
#: cannot know whether it died or is genuinely still working, only that nobody should still
#: be expecting it. Long enough not to doubt a genuinely slow sub-agent mid-run.
PRESUMED_DEAD_SECONDS = 30 * 60

#: When a record is finally discarded. Far out, because everything before it is a thing a
#: human might still be looking at — but finite, so a stray from a killed process cannot
#: accumulate into a permanent false warning.
PRUNE_SECONDS = 24 * 60 * 60


#: A sub-agent handed work by the `Task`/`Agent` tool. What this tracker held for its
#: whole life before #420, and still what every reader means unless it says otherwise —
#: including a record written before the field existed (:func:`_kind_of`).
DISPATCH = "dispatch"

#: One repo being cloned into a workspace (`commands.cmd_clone`, one per repo, so eight
#: parallel clones read as eight).
CLONE = "clone"

#: A forge-state refresh (`commands.cmd_gl_refresh`) — the detached child
#: `glstate.maybe_spawn` starts, not the parent that spawned it.
REFRESH = "gl-refresh"

#: One action of the command surface, started from the palette and still running
#: (`frame.actions.ActionRegistry.invoke`). Actions are fire-and-report (§4g): invoke
#: returns having STARTED the work, so this record is the only thing that knows the work
#: is still going, and the frame's spinner — the one reader that asks for ``kind=None`` —
#: is what shows it.
#:
#: A kind of its own rather than a reuse of :data:`DISPATCH`, for the reason the module
#: docstring gives about ``clone``: the same records feed the dispatch-overlap nudge, and
#: an action recorded as a dispatch would make that nudge say *"`switch-workspace` writes
#: code and `x` are already running"* — wrong, and wrong in the confident, readable way
#: that is worse than silence. Every reader that must not see one gets that by NOT asking.
ACTION = "action"


def _dir() -> Path:
    from . import config
    return config.STATE_DIR / "dispatch-inflight"


def _kind_of(rec: dict) -> str:
    """What kind of work *rec* describes — :data:`DISPATCH` for anything that does not
    say, which is every record this tracker held before #420 and every one written by an
    older charter still sitting on disk. A non-string is treated the same way rather than
    passed through: the value is compared against a caller's filter, and a filter that
    can never match would silently hide a live record from the nudge that needs it."""
    kind = rec.get("kind")
    return kind if isinstance(kind, str) and kind else DISPATCH


def _safe_name(agent: str) -> str:
    return "".join(c if c.isalnum() or c in "._-" else "_" for c in agent)[:64]


def stamp() -> int | None:
    """One ``stat`` that answers "has anything about the tracker changed?" — or ``None``.

    A frame panel wants to ANIMATE while work is in flight and be perfectly still
    otherwise (#387), which means asking this question several times a second, forever,
    on an idle machine. :func:`live_records` is cheap but it is not free: it opens the
    directory and reads every entry in it. This is the cheap half — a single ``stat`` of
    the directory itself, whose mtime moves whenever a record is CREATED or REMOVED,
    which is the only way the set of live records can change. `frame/panel.py` re-reads
    the records only when this number moves.

    ``None`` for "no such directory", which is the common case on a machine that has never
    dispatched — and is a real answer, not an error: nothing can be in flight, and it costs
    the same single failed ``stat`` to learn it.

    The mtime is deliberately NOT enough on its own for one thing, and the caller owns
    that half: a record crossing :data:`PRESUMED_DEAD_SECONDS` changes what a caller
    should say about it while touching no file at all, so `panel._running` also re-reads
    when the earliest such deadline passes. Splitting it that way keeps the IDLE path —
    no records at all, so no deadline either — at exactly one syscall.
    """
    try:
        return _dir().stat().st_mtime_ns
    except OSError:
        return None


def live_records(exclude_token: str | None = None, *,
                 kind: str | None = DISPATCH) -> list[tuple[str, float, bool]]:
    """``(agent, started_at, presumed_dead)`` per record, duplicates preserved.

    *kind* selects which records answer: one of :data:`DISPATCH`/:data:`CLONE`/
    :data:`REFRESH`, or ``None`` for every kind. **It defaults to `DISPATCH`, and that
    default is the guard** — see the module docstring. The frame's spinner is the one
    caller that wants everything; every other reader means dispatches and gets them
    without having to remember to say so.

    The kind is not in the returned tuple. Nothing that filters also needs to display it,
    and a fourth element would have to be threaded through `panel._running`'s cache and
    every test that builds a record by hand for no reader's benefit.

    The start time is what separates "two agents are out" from "two agents have been
    out for forty minutes", and only the second is worth interrupting for. It is read
    from the record's own ``ts``, falling back to the file's mtime — the same instant,
    and the only answer available for a record written by a charter that predates the
    field.

    ``presumed_dead`` is measured from that same start time, not from the mtime the
    pruning reads: it is the flag on the age a caller draws, so the two can never
    disagree about which side of the threshold a record sits on. Pruning stays on the
    mtime because it happens *before* the parse — which is what lets a corrupt stray be
    cleaned up at all.

    :func:`live` is a projection of this rather than a second walk of the directory:
    one glob, one set of rules. A caller that wants the names only should keep calling
    it — the extra elements are a cost the aggregate has no use for.
    """
    d = _dir()
    if not d.exists():
        return []
    out: list[tuple[str, float, bool]] = []
    now = time.time()
    for p in d.glob("*.json"):
        try:
            mtime = p.stat().st_mtime
            if now - mtime > PRUNE_SECONDS:
                p.unlink(missing_ok=True)      # a stray, long past anyone watching for it
                continue
            if exclude_token and p.stem == exclude_token:
                continue
            rec = json.loads(p.read_text())
            if kind is not None and _kind_of(rec) != kind:
                continue
            ts = rec.get("ts")
            started = float(ts) if isinstance(ts, (int, float)) else mtime
            out.append((rec.get("agent") or p.stem, started,
                        now - started > PRESUMED_DEAD_SECONDS))
        except (OSError, TypeError, ValueError):
            continue
    return out


def live(exclude_token: str | None = None, *,
         kind: str | None = DISPATCH) -> list[str]:
    """Agent names the tracker holds — presumed-dead ones included, since the aggregate
    this feeds counts records and the distinction is drawn per chip.

    ``exclude_token`` drops one specific record — the caller's own, so a dispatch
    never reports itself as a concurrent peer. *kind* is :func:`live_records`'.
    """
    return sorted(name for name, _, _ in live_records(exclude_token, kind=kind))


def still_running(exclude_token: str | None = None, *,
                  kind: str | None = DISPATCH) -> list[str]:
    """Agent names charter can still claim are *running* — presumed-dead ones dropped.

    For the callers that assert liveness rather than display it. The dispatch nudge says
    a peer "is already running", which stops being true at the presumed-dead threshold;
    keeping the record so a stuck dispatch stays visible must not turn that nudge into a
    nag that outlives the process by a day.

    **Its *kind* default is load-bearing rather than tidy.** This is the function whose
    output is read back to an operator as a sentence naming each peer, so a `clone`
    record reaching it produces *"`x` writes code and `clone` are already running"* —
    the wrong-and-confident failure #420 declined to ship. The nudge asks for nothing,
    and therefore gets dispatches.
    """
    return sorted(name for name, _, dead in live_records(exclude_token, kind=kind)
                  if not dead)


def start(agent: str, *, kind: str = DISPATCH) -> str | None:
    """Mark *agent* as in flight; returns an opaque token, or None on any failure.

    *kind* is what a reader filters on — :data:`DISPATCH` unless the caller says
    otherwise, which keeps every pre-#420 call site meaning exactly what it meant.

    **The token names exactly one record**, and a caller that can hold onto it should
    hand it back to :func:`finish` rather than let the name-and-kind search pick — see
    that function. ``None`` means nothing was written, so there is nothing to retire.
    """
    agent = (agent or "").strip()
    if not agent:
        return None
    try:
        from . import config

        d = _dir()
        config.private_mkdir(d)
        # mkstemp, not a timestamped name: two dispatches starting in the same
        # millisecond would collide and the second would overwrite the first —
        # losing exactly the overlap this exists to observe. The agent name stays
        # in the prefix so `finish` can still find its own records.
        fd, path = tempfile.mkstemp(prefix=f"{_safe_name(agent)}.", suffix=".json", dir=d)
        with os.fdopen(fd, "w") as fh:
            json.dump({"agent": agent, "kind": kind, "ts": time.time()}, fh)
        return Path(path).stem
    except OSError:
        return None


def finish(agent: str, *, kind: str = DISPATCH, token: str | None = None) -> None:
    """Clear one in-flight record for *agent* of *kind* — the oldest **still-running**
    one, since a repeat dispatch of the same persona should retire the run that started
    first.

    **A caller holding its own token retires that record and nothing else.** The search
    below picks a record by name, kind and age, and picking is a race the moment two
    workers of one agent run in the SAME process: both glob, both filter, both select the
    identical file, one unlink wins and the loser's record survives — drawn by the frame
    as ``⏳ 1 running`` for :data:`PRESUMED_DEAD_SECONDS` and then ``⋯ 1 stalled`` until
    :data:`PRUNE_SECONDS`, a full day after the work ended. Across processes the old
    dispatch and clone callers cannot do this — a process finishes what it started — but
    `frame.actions.ActionRegistry.invoke` starts a thread per invocation, so one process
    now holds several, and the palette makes two of them two keypresses away. *token* is
    what :func:`start` answered; it names one file, so there is nothing to pick.

    "Still running" is the qualification records surviving past the presumed-dead
    threshold made necessary. Oldest-first alone would hand a finishing dispatch the
    stuck record to retire and leave its own behind — deleting exactly what #308 exists
    to keep, and leaving a false live one in its place. Presumed-dead records are still
    eligible when there is nothing else, because a genuinely long dispatch does finish
    eventually and its record has to go when it does.

    **The kind is matched, not merely written.** The file NAME carries only the agent, so
    a clone of a repo called ``steward`` and a dispatch to a persona called ``steward``
    glob identically — and whichever finished first would retire the other's record,
    leaving a false live one behind and clearing a true one. Deciding it by reading each
    candidate rather than by renaming the files keeps the on-disk shape unchanged in both
    directions, so a record written by an older charter is still findable and one written
    by this charter is still findable by an older one.
    """
    if token is not None:
        # A name and nothing else: a token carrying a separator, ``.`` or ``..`` is not
        # something this ever wrote, and unlinking what it points at would be a delete
        # outside the tracker's own directory decided by a caller's string.
        if not token or Path(token).name != token:
            return
        try:
            (_dir() / f"{token}.json").unlink(missing_ok=True)
        except OSError:
            pass
        return
    agent = (agent or "").strip()
    if not agent:
        return
    try:
        now = time.time()
        matches = [p for p in sorted(_dir().glob(f"{_safe_name(agent)}.*.json"),
                                     key=lambda p: p.stat().st_mtime)
                   if _matches_kind(p, kind)]
        running = [p for p in matches
                   if now - p.stat().st_mtime <= PRESUMED_DEAD_SECONDS]
        for p in (running or matches)[:1]:
            p.unlink(missing_ok=True)
    except OSError:
        return


def _matches_kind(p: Path, kind: str) -> bool:
    """Is the record at *p* of *kind*? A file that cannot be read or parsed answers
    **False** — :func:`finish` deletes what this admits, and deleting a record charter
    could not read is deleting something it cannot claim is the caller's own. The
    unreadable file is pruned on its own schedule (:data:`PRUNE_SECONDS`, in
    :func:`live_records`), which is where a stray belongs."""
    try:
        return _kind_of(json.loads(p.read_text())) == kind
    except (OSError, TypeError, ValueError):
        return False


def prune_all() -> int:
    """Discard every record on this plane. How many were removed.

    **The one caller is `charter frame-quit`, and it is the one moment that can honestly do
    this.** Records clear only on :func:`finish`, which the worker that started them calls —
    so killing every harness on a plane strands every record it was holding:
    :func:`still_running` reports one for :data:`PRESUMED_DEAD_SECONDS` (30 minutes) and
    :func:`live` holds it for :data:`PRUNE_SECONDS` (24 hours). The frame's spinner reads
    `live(kind=None)`, so it would animate for a plane doing nothing, and the
    dispatch-overlap nudge reads :func:`still_running`, so it would name agents the quit
    itself killed.

    **Plane-scoped, because a record cannot say anything narrower.** Every record is
    ``{"agent", "kind", "ts"}`` — no fid, no chat, no workspace — so there is no filter a
    per-frame caller could apply, and inventing one here would be a second reading of a fact
    the file does not hold. That is exactly co-extensive with what a quit stops: `cmd_quit`
    kills every chat this PLANE has a directory for, which is every process that could have
    written one of these. The two scopes are the same scope, and that is why this is safe
    rather than merely convenient.

    **Not an age sweep, and not `PRUNE_SECONDS` brought forward.** :func:`live_records`
    already drops records past that horizon opportunistically, and #308 is explicit that
    deleting a record for being OLD destroys the single most interesting thing this tracker
    holds. This deletes for a different reason entirely: the work is over, because the caller
    just ended it.

    Everything here is best-effort, like the rest of this module: a record that could not be
    removed is one stale row in a frame that is about to stop existing, and a tracker that
    broke a quit would be worse than one that missed a file.
    """
    try:
        records = list(_dir().glob("*.json"))
    except OSError:
        return 0
    gone = 0
    for p in records:
        try:
            p.unlink(missing_ok=True)
            gone += 1
        except OSError:
            continue
    return gone


# --------------------------------------------------------------------------- #
# A CHAT'S OWN TURN.                                                          #
#                                                                             #
# Everything above is keyed by AGENT and says nothing about where the work is  #
# happening: a record is `{"agent", "kind", "ts"}` — "no fid, no chat, no      #
# workspace" (:func:`prune_all`). That is the right shape for what it feeds    #
# (the dispatch-overlap nudge, `bottom`'s ⏳ N) and the wrong shape for the     #
# question a tab strip asks, which is *is THIS chat's harness working*. So     #
# this is a second tracker rather than a fifth :data:`DISPATCH` kind: the      #
# records have a different key, a different horizon and a different reader,    #
# and folding them into one directory would put chat ids in front of the       #
# nudge that reads agent names back to an operator as a sentence — #420's own  #
# failure, arriving through the other axis.                                    #
#                                                                             #
# It shares the SHAPE and nothing else: one directory whose mtime is the       #
# cheap gate (:func:`turn_stamp`, :func:`stamp`'s twin), one small file per    #
# live record, an age past which charter stops claiming anything, and          #
# best-effort everywhere — a tracker that breaks a turn is worse than one that #
# misses a turn.                                                               #
# --------------------------------------------------------------------------- #

#: How long a turn charter never saw the end of keeps claiming to be working.
#:
#: **The falling edge is a `Stop` hook, and a turn can end without one.** Pressing Esc
#: mid-turn fires no `Stop` at all, so without this a chat that was interrupted would
#: spin for the rest of the plane's life. Every `pretooluse*`/`posttooluse*` handler
#: refreshes the mark (`hooks._turn_bump`), so this is not "how long may a turn take" —
#: it is **how long a live turn may go without touching a single tool**, which is a much
#: shorter thing. A turn issuing tool calls refreshes its own mark indefinitely and is
#: never cut off by this number.
#:
#: **Its own constant, not :data:`PRESUMED_DEAD_SECONDS`, for the reason that number's own
#: header gives about #308**: two jobs with opposite horizons sharing one number is what
#: made the old single TTL wrong. That one is a DISPLAY threshold — the record survives it
#: and is drawn differently (`45m?`) — because "a dispatch that has outlived every
#: reasonable expectation" is the most interesting thing that tracker holds. This one is
#: the opposite: past it charter does not know whether the harness is working, and the
#: honest picture for *does not know* is the same as for *is not* — nothing drawn at all,
#: which is `state.harness_session`'s rule one surface over.
#:
#: **Ten minutes, and the trade is stated rather than measured away.** A long TOOLLESS
#: think bumps nothing, so a TTL short enough to catch an abandoned turn also blinks off
#: during deep thinking; the two cannot both be had from this signal. Both errors cost the
#: operator the same single switch to look, and they differ in how they end: a blink-off
#: is repaired by the turn's very next tool call, while a stale spinner stands until this
#: number runs out. So it is set generously against the cadence it actually measures — ten
#: minutes with no tool call whatsoever is far outside what a working turn does — and no
#: further, because the direction charter errs in is *not claiming* (`inflight`'s own
#: `⋯ stalled` stops animating for exactly this reason).
TURN_STALE_SECONDS = 10 * 60


def _turn_dir() -> Path:
    from . import config
    return config.STATE_DIR / "chat-turns"


def _turn_file(chat: str | None) -> Path | None:
    """The one file that stands for *chat*'s turn, or ``None`` for a name that cannot
    have one.

    The name comes from ``$CHARTER_SESSION_ID`` inside a harness pane, which the frame
    launcher set (`commands_frame._session_id_env_argv`) and which anything in that shell
    can therefore also set to something else — so this is where a chosen string would
    become a chosen PATH, and it is the only place that has to answer for it.

    **:func:`_safe_name` is asked as a QUESTION here, never used as a mangling**, and that
    is what lets the file's NAME be the chat's id. It admits exactly the alphabet a chat id
    travels to tmux under (`frame/chats.ID_RE`), so a value it would change is a value that
    is not a chat id — refused — and one it would leave alone is a name this directory can
    hold losslessly. :func:`working_chats` therefore reads the id off the directory entry
    and never opens a file, and there is no mangled form for a reader to have to invert.
    ``../../.ssh/authorized_keys`` is refused rather than flattened, which is the same
    answer `chats.check` gives a name off an argv.

    Refusing rather than repairing costs one real case and it is stated rather than hidden:
    `_safe_name` truncates at 64 characters, so a chat id longer than that has no mark. A
    chat with no mark shows what every chat showed before this existed.

    ``.`` and ``..`` pass that question — they are made entirely of admitted characters —
    and are refused by name, because a chat "called" ``..`` would have :func:`turn_end`
    unlink the tracker's parent directory rather than a record.
    """
    # ``str | None`` rather than ``str``, and `(chat or "")` beside it, because
    # `hooks._chat_id` answers with `os.environ.get` unrepaired — an unset
    # ``$CHARTER_SESSION_ID`` is a `None` this seam is the right place to absorb.
    # :func:`start` spells the same line for the same reason one tracker up.
    chat = (chat or "").strip()
    if not chat or _safe_name(chat) != chat or chat in (".", ".."):
        return None
    return _turn_dir() / chat


def turn_stamp() -> int | None:
    """:func:`stamp` for the turn tracker — one ``stat``, or ``None`` for no directory.

    Same contract and the same two halves, so `frame/panel.py` can cache against it the
    way it already caches against the other: the mtime moves when a record is CREATED or
    REMOVED, which is the only way the SET of working chats can change, and it does NOT
    move when :func:`turn_bump` refreshes one — a refresh changes no answer, only a
    deadline. The deadline half is the caller's, exactly as it is for :func:`stamp`: a
    mark crossing :data:`TURN_STALE_SECONDS` changes what charter may claim while touching
    no file at all, so the panel also re-reads when the earliest such deadline passes.

    That split is what keeps an idle plane at one syscall per tick: no marks means no
    deadline to hold and nothing to compare against.
    """
    try:
        return _turn_dir().stat().st_mtime_ns
    except OSError:
        return None


def turn_begin(chat: str | None) -> None:
    """Mark *chat*'s harness as working. The RISING edge, and its only writer is the
    `UserPromptSubmit` hook.

    **Creating is deliberately separate from refreshing** (:func:`turn_bump`). A tool hook
    fires for a sub-agent's tools as well as the session's own, and it fires in whatever
    turn happens to be running; if a tool hook could CREATE a mark, a chat would start
    claiming to be working on evidence that a tool ran rather than on evidence that a turn
    began — and there would then be no single moment charter could point at as the start.
    A prompt is that moment, it is the one edge the harness reports unambiguously, and it
    is already on disk (`hooks.userpromptsubmit`).

    **The file is empty, and `config.touch_for` is what makes it.** A mark carries its
    meaning in EXISTING and in its mtime — which is that helper's own sentence, and here
    it is exact: the id is the file's name (:func:`_turn_file`), the TTL is its mtime, and
    there is nothing left for bytes to say. `touch_for` and not `Path.touch` because
    everything under `config.STATE_DIR` is charter's own state, and a bare `touch` creates
    at ``0o666 & ~umask`` in a directory charter does not always own (#331, #505).

    Best-effort: a tracker that cannot write is a strip that shows nothing, which is what
    it showed before this existed.
    """
    p = _turn_file(chat)
    if p is None:
        return
    try:
        from . import config
        config.private_mkdir(_turn_dir())
        config.touch_for(p)
    except OSError:
        return


def turn_bump(chat: str | None) -> None:
    """Refresh *chat*'s mark — the TTL only, never the mark itself.

    One ``utime`` on a file that already exists, and **nothing at all when it does not**:
    that missing-file case is the whole reason this is not :func:`turn_begin` with a
    different name. It is also why a refresh is free for the panel — `utime` does not move
    the directory's mtime, so :func:`turn_stamp` does not move and nothing re-reads.

    Called from every `pretooluse*` and `posttooluse*` handler, which is what makes
    :data:`TURN_STALE_SECONDS` a bound on a toolless STRETCH rather than on a turn.
    """
    p = _turn_file(chat)
    if p is None:
        return
    try:
        os.utime(p)
    except OSError:
        return


def turn_end(chat: str | None) -> None:
    """Clear *chat*'s mark. The FALLING edge — `hooks.stop`, and nothing else.

    `SubagentStop` deliberately does not call this: a sub-agent finishing does not end
    the turn that dispatched it, and clearing here would blink the tab off in the middle
    of work that is still going.
    """
    p = _turn_file(chat)
    if p is None:
        return
    try:
        p.unlink(missing_ok=True)
    except OSError:
        return


def working_chats() -> list[tuple[str, float]]:
    """``(chat, last_seen)`` per chat whose harness charter can still claim is working.

    :func:`live_records` one tracker over, and the same two readers: `frame/panel.py`
    takes the times, to know when its cached answer expires with no file having changed;
    `frame/slots.py` takes the names, to decide which tabs get a spinner.

    A mark past :data:`TURN_STALE_SECONDS` is **deleted here** rather than merely skipped,
    and that is the opposite of what :func:`live_records` does with a presumed-dead
    dispatch — on purpose. There, an outlived record is the most interesting thing the
    tracker holds and is kept so it can be drawn. Here there is nothing to draw: past the
    TTL charter does not know, so the record has no reader left, and deleting it moves the
    directory mtime — which turns an expiry that no file announced into an ordinary
    :func:`turn_stamp` change for every panel watching. A chat that never gets a `Stop`
    would otherwise leave one file behind for good.

    **One `stat` per entry and no file is ever opened**, because the id is the entry's own
    name — see :func:`_turn_file` for why that is lossless rather than convenient. So this
    costs a `readdir` and one `stat` per working chat, and a plane with nothing working
    does not get here at all.
    """
    d = _turn_dir()
    if not d.exists():
        return []
    out: list[tuple[str, float]] = []
    now = time.time()
    for p in d.iterdir():
        try:
            seen = p.stat().st_mtime
            # `>` and not `>=`, and the deletion sweep asks which. Only the DIRECTION is
            # pinned, deliberately: `now - seen` is a float measured against a ten-minute
            # constant, so the two spellings differ on one instant no run arrives at, and
            # both say the same thing about the chat — it has gone the whole TTL without a
            # tool call. A test asserting one of them would be pinning a coin-flip.
            if now - seen > TURN_STALE_SECONDS:
                p.unlink(missing_ok=True)
                continue
        except OSError:
            continue
        out.append((p.name, seen))
    return out
