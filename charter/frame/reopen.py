"""What a quit wrote down, and what a reopen may believe of it.

**One FILE under `.charter/frame/`, and that is the whole trick.** `state.reap` skips
anything that is not a directory (``if not d.is_dir() or d.name in live``) and
`chats.of_workspace` scans with the same filter, so a plain file in the frame root is
invisible to both: it survives every reap, it is never mistaken for a chat, and — this is
the half that matters — **the chat DIRECTORY goes on being a liveness marker and goes on
being reaped exactly as it is today.** Nothing here inverts `reap`, which is stage 4 of
the IDE spec's own delivery order and six edits wide (`chat: close`, `max_chats`, the
ghost-tab collector, `cmd_workspace_remove`, `clear_claim`, and ~60 tests that assert the
current rule).

**Which is also why the manifest is not optional.** #757 keeps a chat's harness session id
in `session.durable` so `clear_shape` cannot delete it — but `reap` deletes the whole
directory when the chat's launcher pid is dead, and **after a restart every launcher pid is
dead**. Measured on the plane this was written for: the operator's own frame directories
were removed by the next launch. So the id #757 kept is gone by the time a reopen could ask
for it, and a resume that reads it off the chat's own directory reads nothing. The manifest
is the copy that outlives the directory it came from.

**Every value in here came off disk and is going onto a tmux argv, an `os.chdir` or a
`--resume` flag**, so every value is checked on the way OUT rather than trusted because
charter wrote it. That is `state.frame_workspace`'s rule (#442) and `chats.of_workspace`'s
(#475), applied at the one boundary this module owns: a hand-edited or half-written
`reopen.json` must degrade to "fewer chats reopened, and charter said which", never to a
name reaching a shell.

**No lock, deliberately** (§4d). Two quits may write this; last writer wins. A lock file is
a thing that gets stranded, and #685 spent a whole PR on that class of problem.

**Bounded like the chats are.** A transcript is one file per chat, replaced rather than
accumulated, and :func:`prune_transcripts` removes the ones no manifest names any more —
because a `.transcript` is a file in the frame root and therefore, by the same rule that
makes the manifest durable, has no other collector.
"""

from __future__ import annotations

import json
import os
import time
from pathlib import Path
from typing import NamedTuple

from .. import config, contain
from . import chats, state

#: The manifest's own name. A plain name beside the chat directories, not a dotfile, for
#: `state._CLAIM_FILE`'s reason: everything in the frame root is charter's own bookkeeping
#: and none of it hides from `ls`.
#:
#: **It cannot collide with a chat id**, and that is arithmetic rather than luck:
#: `state.new_chat_id` mints ``{prefix}.{digits}`` where the prefix is
#: `state.workspace_prefix`'s alphabet (no dots — a workspace called `api.2` mints `api_2`),
#: so ``reopen.json`` is not a shape the allocator can produce. `reap`'s and
#: `chats.of_workspace`'s `is_dir()` filters make that belt and braces.
MANIFEST = "reopen.json"

#: What a manifest this charter wrote says it is. Refused rather than guessed at on the way
#: back in: a directory under `.charter/frame/` can be months old (the IDE spec's §5 names
#: exactly this as a cost the durability redesign creates), and a reopen driven by a shape
#: it does not recognise would restore a plane out of fields it is inventing meanings for.
VERSION = 1

#: What a captured pane is written under — ``<chat id>.transcript`` in the frame root.
#:
#: The chat id is the whole name because a chat has at most ONE (§4f: *"the last capture per
#: chat, not a history of them"*), and because a reopen RENAMES the file onto the new chat's
#: id rather than writing a pointer to the old one. One naming rule, no second file to keep
#: in step, and `.transcript` cannot be an ordinal so it cannot be a chat id either.
TRANSCRIPT_SUFFIX = ".transcript"


class Chat(NamedTuple):
    """One chat a quit recorded, as a reopen reads it back.

    Carried as a record rather than a dict so the writer and the reader cannot come to
    disagree about what a chat's restore items are — `chats.Chat`'s reason, one lifetime
    further out.

    Every field is the value at the moment of the QUIT. None of them is re-derived at
    reopen time, because the plane the reopen runs against no longer holds the chat that
    would answer.
    """

    #: The chat id this WAS. Not the id it comes back as: `state.new_chat_id` allocates a
    #: fresh ordinal on the way in (the old directory is reaped — see the module
    #: docstring), so this is a name for the transcript and for what charter says on
    #: screen, never a directory a reopen writes into.
    chat: str
    #: The workspace it belonged to — `state.own_workspace`, the membership question
    #: (#733), never `workspace_for`. **It is the authoritative answer on the way back, and
    #: #791 is what makes that true rather than hopeful**: that change took the per-session
    #: `.charter/sessions/<fid>.workspace` pointer out of `own_workspace`'s ladder, which
    #: matters here for a reason that is easy to miss — a reopen gets a FRESH ordinal, but
    #: `new_chat_id` walks upward from 1 and `reap` frees the ordinals a quit's chats held,
    #: so it very often gets the same NAME back. While the pointer was a rung, a previous
    #: chat's `charter workspace use` would have outranked this record for the chat that
    #: inherited its ordinal.
    workspace: str
    #: The persona resolved for it, or ``""``. Its own per-session pointer, which a reopen
    #: re-writes under the NEW id — an unpinned chat's persona lives nowhere else, and the
    #: pointer is keyed on a fid that is about to stop existing.
    persona: str
    #: `harness.base.name` — ``claude-code``, ``opencode``, ``codex`` — or ``""``. The
    #: harness's own identity and not its `cli_name`, because that is what
    #: `state.identity` recorded and what decides whether a resume is even possible.
    harness: str
    #: Where the harness was started, or ``""`` when charter never recorded one (a chat
    #: launched by a charter that predates `state.record_cwd`).
    cwd: str
    #: The harness's own session id, or ``""``. Claude Code only: nothing else writes one
    #: (§2.8), so nothing else can be asked for its conversation back.
    resume: str
    #: The captured scrollback's file name in the frame root, or ``""``.
    transcript: str
    #: Whether this chat was the one on screen in its tmux session when the quit ran, so a
    #: reopen can put the operator back on it rather than on whichever window it created
    #: first.
    active: bool


class Frame(NamedTuple):
    """One workspace's worth of chats — a tmux session, before the quit killed it.

    Grouped rather than flat because a reopen has to rebuild them one session at a time:
    the first chat of a workspace CREATES the session and every later one joins it
    (`cmd_launch`'s `joining` branch), and interleaving two workspaces' chats would make
    that ordering depend on the manifest's sort.
    """

    workspace: str
    chats: tuple[Chat, ...]


class Manifest(NamedTuple):
    """Everything one quit wrote, ready to be acted on.

    *focus* is the workspace whose session the reopen attaches to — the one the quit was
    invoked FROM, which is where the operator was standing when they asked to stop. It is
    recorded rather than derived because by reopen time there is no client anywhere to ask.
    """

    at: int
    focus: str
    frames: tuple[Frame, ...]

    def all_chats(self) -> tuple[Chat, ...]:
        """Every chat in every frame, in the order a reopen rebuilds them."""
        return tuple(c for f in self.frames for c in f.chats)


def path() -> Path:
    """Where the manifest lives. One expression, so the writer and the reader cannot
    disagree — and never exported as a bare constant, because `state._root()` reads
    `config.STATE_DIR` at CALL time (see that module's docstring: the test harness
    repoints it after import)."""
    return state._root() / MANIFEST


def transcript_path(fid: str) -> Path | None:
    """Where *fid*'s captured scrollback lives, or ``None`` when *fid* cannot name one.

    Through `contain.child`, exactly like `state.frame_dir`: a chat id may have come off
    `os.scandir` or out of the manifest, and this path is about to be opened for writing
    and later handed to a pager. Resolved, never sanitised — rewriting a hostile name into
    a safe-looking one invents a second identity for it (the failure `contain.child`
    documents).

    The id is held to `chats.ID_RE` as well, because `contain.child` bounds *shape* and not
    alphabet, and a name that cannot be a chat has no transcript by definition.
    """
    if not chats.ID_RE.fullmatch(fid or ""):
        return None
    return contain.child(state._root(), f"{fid}{TRANSCRIPT_SUFFIX}")


def _usable(chat: Chat) -> bool:
    """Whether *chat* is one a reopen may act on.

    **Two names and nothing else**, because those two are the ones that become paths and
    tmux targets: the chat id (a transcript file name, and what charter prints) and the
    workspace (a `-t` session target, a `workspace_dir()` join — #442's own position).
    Everything else on the record degrades on its own terms: an unusable `cwd` falls back
    and says so, a `resume` that no longer names a conversation is the harness's answer to
    give, a missing `transcript` is simply not offered, and a `persona` that no longer
    exists is refused by `switch.to_persona`'s own name check.

    A chat this refuses is REPORTED and skipped, never silently dropped — see
    `commands_frame.cmd_reopen`.
    """
    from .. import workspace as ws_mod
    return bool(chats.ID_RE.fullmatch(chat.chat or "")
                and ws_mod.valid_name(chat.workspace))


def write(frames, *, focus: str, at: int | None = None) -> bool:
    """Record *frames* as the plane to put back. ``True`` when it landed.

    **Called BEFORE anything is killed**, and that ordering is the record rather than a
    tidiness — the same rule `trace._trace_secret_use` keeps for the same reason: a record
    that depends on the thing it records succeeding is not a record. A quit that wrote this
    after its kills would lose the whole plane to any failure in between, and the failure
    it would lose it to is precisely the one an operator cannot undo.

    Atomic: written to a sibling temp file and `os.replace`d, so a reader either sees the
    previous manifest whole or this one whole. `config.write_for` for the temp file and not
    `Path.write_text`, because `os.replace` carries the SOURCE's mode onto the target
    (#582) and this is a state path.

    **The temp file's NAME is `config.temp_beside`'s and not this module's, and #845 is
    why.** ``os.replace`` is atomic; the file it renames is not. While there was one temp
    name, two writers wrote into the same bytes and the first to rename put the SECOND
    writer's content in place — reporting, to the first, that its own record had landed.
    That was already reachable (a quit races another quit; `_forget_transcript` races
    either), and #845 makes it ordinary: the frame process now records on a debounce, from
    a thread, in the same process as `cmd_reopen`'s own `_consume`. This was `mkstemp`
    here first, alone in the package; #893 found the same defect in the other twenty
    atomic writers and answered all of them in `config.replace_for`, so what was a rule
    this module kept for itself is now the one every writer gets — a pid and a random tail,
    unique across processes and threads by construction rather than re-derived out of a
    counter, at the mode `config.write_for` picks for where the path is.

    **Removed when the rename does not happen**, because a name nothing can predict is a
    name nothing can collect: `prune_transcripts` is the only sweep the frame root has and
    it touches nothing but `*.transcript`, so a temp left behind would stay for good. That
    removal is `replace_for`'s now, and it is the reason this module no longer spells one.

    ``False`` for every way it can fail to land — no frame root, a filesystem that refuses
    the write, a value `json` cannot serialise. One answer, because the caller does the same
    thing with each: tell the operator the plane was not recorded, and let them decide
    whether to quit anyway.
    """
    root = state._root()
    try:
        config.private_mkdir(root)
    except OSError:
        return False
    payload = {
        "version": VERSION,
        "at": int(time.time() if at is None else at),
        "focus": focus,
        "frames": [{"workspace": f.workspace,
                    "chats": [c._asdict() for c in f.chats]} for f in frames],
    }
    try:
        config.replace_for(root / MANIFEST,
                           json.dumps(payload, indent=2, sort_keys=True) + "\n")
    except (OSError, TypeError, ValueError):
        return False
    return True


def read() -> Manifest | None:
    """The last quit's manifest, or ``None`` when there is nothing to reopen.

    ``None`` for a plane that has never quit, for a file that cannot be read, for one that
    is not JSON, for one whose ``version`` this charter does not speak, and for one whose
    shape is not the two nested lists this writes. Five reasons, one answer, because the
    caller does the same thing with all of them: say there is nothing recorded to put back.
    **Never a partial guess** — a manifest charter cannot read whole is not a plane charter
    may half-restore.

    A chat whose two load-bearing names do not check out is dropped from the frame it was
    in rather than taking the manifest down with it (:func:`_usable`), and a frame left with
    no chats is dropped in turn. `commands_frame.cmd_reopen` reports the difference between
    what was recorded and what came back, so a dropped chat is a sentence rather than a
    silence.
    """
    try:
        raw = json.loads(path().read_text())
    except (OSError, ValueError):
        return None
    if not isinstance(raw, dict) or raw.get("version") != VERSION:
        return None
    frames_raw = raw.get("frames")
    if not isinstance(frames_raw, list):
        return None
    frames: list[Frame] = []
    for f in frames_raw:
        if not isinstance(f, dict) or not isinstance(f.get("chats"), list):
            return None
        kept = tuple(c for c in (_chat(x) for x in f["chats"])
                     if c is not None and _usable(c))
        if kept:
            frames.append(Frame(workspace=str(f.get("workspace") or ""), chats=kept))
    focus = raw.get("focus")
    at = raw.get("at")
    return Manifest(at=at if isinstance(at, int) else 0,
                    focus=focus if isinstance(focus, str) else "",
                    frames=tuple(frames))


def _chat(raw) -> Chat | None:
    """One recorded chat, or ``None`` when the entry is not one.

    Field by field with a type per field, rather than ``Chat(**raw)``: the mapping comes
    off disk, and splatting it would let an unexpected key raise `TypeError` out of a
    reader whose whole contract is that it degrades. A missing key is the migration case —
    a manifest written by a charter one field older — and answers with that field's own
    empty value, which every caller already handles (a chat with no `resume` reopens empty
    and says so).
    """
    if not isinstance(raw, dict):
        return None
    text = {k: (raw.get(k) if isinstance(raw.get(k), str) else "")
            for k in ("chat", "workspace", "persona", "harness", "cwd", "resume",
                      "transcript")}
    return Chat(active=raw.get("active") is True, **text)


def forget() -> None:
    """Drop the manifest, because it has been acted on.

    **A manifest describes one quit and is consumed by one reopen.** Left in place, a second
    `charter reopen` would open every chat a second time — and the operator would have no
    way to tell the duplicate tabs from the real ones, because a reopened chat is a fresh
    ordinal either way.

    Never raises: a manifest that could not be removed costs a duplicated tab the operator
    can close, and a reopen that had already relaunched every harness must not report
    failure over a file.
    """
    try:
        path().unlink(missing_ok=True)
    except OSError:
        return


def prune_transcripts(keep) -> None:
    """Remove every ``*.transcript`` in the frame root whose chat is not in *keep*.

    **The collector a file in the frame root does not otherwise have.** The same rule that
    makes the manifest survive `reap` — it is not a directory — means a transcript survives
    it too, so the thing that bounds them has to be the thing that writes them. §4f asks
    for *"the last capture per chat, not a history of them"*; naming the file after the
    chat gives that per chat, and this gives it across chats that no longer exist.

    Called with the ids the manifest now names, from the quit that just wrote it, and with
    the one id being closed removed, from `chat: close`. Never raises, and never touches
    anything but that suffix: the frame root also holds the manifest and every chat's
    directory, and a sweep that reached wider would be `state.reap`'s job being done twice
    by something that does not know `reap`'s four keep-rules.
    """
    root = state._root()
    try:
        entries = list(os.scandir(root))
    except OSError:
        return
    wanted = {str(k) for k in keep}
    for e in entries:
        if e.is_dir() or not e.name.endswith(TRANSCRIPT_SUFFIX):
            continue
        if e.name[:-len(TRANSCRIPT_SUFFIX)] in wanted:
            continue
        try:
            os.unlink(e.path)
        except OSError:
            continue
