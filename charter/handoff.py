"""The facts a handoff is made of: the stamp, the first message, the brief, the todo text,
and the command to run when there is no frame to open a chat in.

**No I/O beyond the one stream :func:`read_brief` is handed**, and no plane, so every
string a handoff produces can be checked without a control plane, a tmux or a workspace on
disk. `charter/commands_handoff.py` is where the refusals are ordered and the writes
happen; this module only knows how each piece is spelled.

A handoff opens a chat whose first message is a brief the operator approved
(`docs/handoff.md`). That message travels to the harness as a command-line argument, so it
is world-readable on this machine while the harness starts — which is why
`cmd_handoff` refuses a credential-shaped brief before any of this runs.
"""

from __future__ import annotations

import datetime
import shlex
import sys

from . import contain

#: What :func:`_shown` passes as `contain.one_line`'s *limit* to mean **do not clip**. That
#: function's own budget is 160 characters, which is right for a report row and wrong for a
#: line whose whole point is that it can be pasted and run: a 12 KB brief would arrive
#: ending in `…`.
#:
#: **A ceiling rather than a width computed per character, because the width was wrong.**
#: The first spelling multiplied the input by the widest escape it believed in — 6, for the
#: `\\uXXXX` form. `one_line` formats with `:04x`, which is a MINIMUM width, so a codepoint
#: outside the BMP renders as SEVEN characters: `\\ue0001` for the language tag, and the
#: same for the musical format controls. At about a thousand of those the budget
#: under-shot, the helper clipped, and the pasted line ended in `…` — the exact failure
#: this exists to prevent. Counting to seven instead would be the same mistake with a
#: better number: it is a claim about which categories Unicode has assigned where, and
#: `contain._INVISIBLE`'s own docstring is about why charter does not make those.
_NO_CLIP = sys.maxsize

#: The first line of every handoff's first message. Facts charter can observe and no
#: instruction: where it came from, which workspace that was, and when. The new chat — and
#: whoever reads the transcript a month later — can tell this message was not typed there.
#: The brackets are ⟨⟩ rather than <> so the line cannot be read as markup by anything that
#: renders the transcript.
STAMP = "⟨handoff from chat {chat} · workspace {workspace} · {when}⟩"

#: What the stamp calls a source that has no chat id — a handoff proposed from a shell
#: outside any frame, which is refused, but whose printed command still carries the stamp
#: so the chat it opens is marked the same way (plan Open question 12).
NO_CHAT = "none"

#: Stands where the harness's own word goes when nothing says which harness this is: no
#: ``$CHARTER_HARNESS``, no native evidence, and no ``[harness] default`` on the plane.
#: Printed literally, because a guess here starts the wrong tool with the operator's brief
#: already in its argv.
UNKNOWN_HARNESS = "<harness>"

NO_STDIN = (
    "charter handoff: this shell has no stdin at all — the command was run with its input "
    "closed, so there is nothing to read a brief from and nothing was opened. Pass the "
    "brief as a quoted heredoc in the same call:\n"
    "  charter handoff {ws} <<'BRIEF'\n  <the brief>\n  BRIEF")
TTY_BRIEF = (
    "charter handoff: reads its brief from stdin, and stdin here is a terminal — nothing "
    "was opened. Pass the brief as a quoted heredoc in the same call:\n"
    "  charter handoff {ws} <<'BRIEF'\n  <the brief>\n  BRIEF")
NOT_UTF8_BRIEF = (
    "charter handoff: the brief on stdin is not UTF-8 text — nothing was opened.")
EMPTY_BRIEF = (
    "charter handoff: the brief on stdin is empty — nothing was opened. Pass it as a "
    "quoted heredoc in the same call:\n"
    "  charter handoff {ws} <<'BRIEF'\n  <the brief>\n  BRIEF")


def _now() -> datetime.datetime:
    """Local time. Its own function so a test can freeze it without freezing the clock."""
    return datetime.datetime.now()


def stamp(source_chat: str, source_workspace: str,
          when: datetime.datetime | None = None) -> str:
    """The stamp line for a handoff leaving *source_chat* in *source_workspace*.

    Minutes, not seconds: the stamp is read by a person deciding whether this message is
    the one they approved a moment ago, and a second's precision answers no question they
    have. Local time, because the reader is in it.
    """
    when = when or _now()
    return STAMP.format(chat=source_chat, workspace=source_workspace,
                        when=when.strftime("%Y-%m-%d %H:%M"))


def first_message(stamp_line: str, brief: str) -> str:
    """The whole of what the new chat is sent: the stamp, a blank line, the brief verbatim.

    The blank line is what keeps the stamp from reading as the brief's first line — which
    is also the line :func:`title` turns into a todo.
    """
    return f"{stamp_line}\n\n{brief}"


def title(brief: str) -> str:
    """*brief*'s first non-blank line, stripped — the todo's title and nothing else.

    Non-blank rather than ``[0]``: a heredoc written with the delimiter on its own line
    often opens with a newline, and a todo titled by an empty string is one
    `memstore.write` refuses.

    **Split on ``\\n`` and nothing else.** `str.splitlines` also breaks on ``\\r``,
    ``\\x0b``, ``\\x0c``, ``\\x1c``–``\\x1e``, U+2028 and U+2085, so a brief whose first
    line carried any of those was titled by a PREFIX of the line the operator wrote —
    charter's "first line" and theirs meaning different things, silently. A shell heredoc
    ends a line at ``\\n``, which is what the operator typed into. What a control character
    then does on screen is a rendering question, answered where the rendering is
    (`contain.one_line`), not by cutting the text short here.
    """
    for line in brief.split("\n"):
        # Stripped ONCE and bound: two `strip()` calls on one line are two places for the
        # deletion sweep to ask whether `lstrip` would do, and only one of them can be
        # answered — for a whitespace-only line the test above is the same either way.
        stripped = line.strip()
        if stripped:
            return stripped
    return ""


def read_brief(stream, ws: str) -> tuple[str, str]:
    """``(brief, refusal)`` read from *stream* — the brief is ``""`` whenever it refuses.

    **`isatty()` before any read, and that order is the whole function.** A read on a
    terminal blocks forever with nothing on screen to say why, which inside an agent's Bash
    tool is a turn that never ends. The refusals name the heredoc in the same breath,
    because a chat that reached here typed the command without one.

    **A CLOSED stdin is a refusal, not a traceback.** Python hands `sys.stdin` as ``None``
    when fd 0 is closed, so `charter handoff beta 0<&-` — a spelling the Bash guard allows,
    because the two words in front of it are the exact ones — reached `isatty()` on
    ``None`` and drafted a crash report. Charter classifies what it refuses (ADR 0009), and
    nothing about this is a surprise to the process it happened in.

    The brief is kept **verbatim** — leading blank lines, trailing newline and all. It is
    what the new chat is sent, and a handoff that tidied it would send text nobody approved.
    """
    if stream is None:
        return "", NO_STDIN.format(ws=ws)
    if stream.isatty():
        return "", TTY_BRIEF.format(ws=ws)
    # `.buffer`, not `.read()`: the decision "is this UTF-8" has to be charter's, and a
    # text stream has already made it — with whatever errors= and encoding the environment
    # handed it, which on a machine with a non-UTF-8 locale is a different answer.
    try:
        text = stream.buffer.read().decode("utf-8")
    except UnicodeDecodeError:
        return "", NOT_UTF8_BRIEF
    # `split()` and never `strip()`, which `commands_frame.background_refusal` settled one
    # seam over: `not text.strip()` and `not text.lstrip()` answer alike for every string,
    # so the strip is a line nothing can turn red. No words is an empty brief.
    if not text.split():
        return "", EMPTY_BRIEF.format(ws=ws)
    return text, ""


def todo_text(brief: str, *, source_chat: str, source_workspace: str) -> str:
    """The todo a handoff records in the TARGET workspace: the title and where it came from.

    **The brief is not in it, and that is not brevity.** A LIVE workspace commits
    `todos/**`, and a brief never reaches a committed file (spec: Limits). The title is
    enough for the person reading the list to recognise the work, and the chat that was
    opened is holding the rest.
    """
    return (f"{title(brief)}\n\nHanded off from chat {source_chat} · workspace "
            f"{source_workspace}. The full brief is private to the chat it opened.")


def terminal_command(*, cli_name: str, workspace: str, extra: list[str],
                     create_vision: str | None, persona: str | None) -> str:
    """The command to run in a new terminal, when there is no frame to open a chat in.

    A handoff needs a frame — charter's own tmux server, with a launcher that can go away
    once the chat exists. Outside one, and inside the operator's own tmux (where the
    launcher stays awake for the life of the harness it starts), there is nothing to open a
    chat in the background OF. So charter prints the equivalent command instead of
    refusing and stopping: everything the handoff would have done, minus the background.

    Every word goes through `shlex.quote`, including the message — it carries the brief,
    which is arbitrary approved prose with quotes and newlines in it.

    *persona* rides as a ``CHARTER_PERSONA=`` prefix rather than a flag, because that
    variable is the pin `charter <harness>` already reads (plan Open question 12), and
    *create_vision* rides as a `charter workspace create … &&` in front, because the
    workspace has to exist before the launcher resolves it.

    **One parameter for "create it, and this is its vision", not two.** `--create` without
    `--vision` is refused before this is reached, so a `create` flag beside an optional
    *vision* is a pair that can only be wrong together — and the `or ""` that stood in for
    the impossible half was a fallback no input could reach (`shlex.quote(None)` answers
    ``"''"`` anyway, so it could not even be seen). ``None`` means the workspace is there
    already.
    """
    q = shlex.quote
    head = ""
    if create_vision is not None:
        head += f"charter workspace create {q(workspace)} --vision {q(create_vision)} && "
    if persona:
        head += f"CHARTER_PERSONA={q(persona)} "
    # `cli_name` unquoted: it is a registered harness's own word, or the literal
    # `<harness>` placeholder, and quoting the placeholder would hide that it is one.
    words = [f"charter {cli_name}", "--workspace", q(workspace), *(q(a) for a in extra)]
    return _shown(head + " ".join(words))


def _shown(command: str) -> str:
    """*command*, safe to PRINT on the terminal charter is printing to.

    **`shlex.quote` escapes nothing.** It wraps a word in single quotes, which is what the
    SHELL needs and says nothing about what a terminal does with the bytes inside them —
    measured: ESC, `\\r`, backspace and a bidirectional override all pass through it
    untouched. And the word this line carries is the whole brief, which is arbitrary text
    the operator approved for a model to read, not for a terminal to execute.

    That matters most here of anywhere, because this is the one refusal that tells the
    operator to **paste the line into a new terminal**: a brief opening
    `\\r✓ opened chat beta.9` repaints charter's own refusal as a success, on the line the
    operator is about to trust. `commands_handoff.BAD_NAME` has contained a workspace name
    for the same reason since this command shipped; the brief is the same defect on a
    surface a thousand times larger.

    **The cost, stated:** a brief carrying a control character is pasted as its escape
    rather than as the character, so the chat that command opens is sent `\\x1b` as four
    characters. That is the right way round — a brief has nothing to say in ESC — and the
    operator can see on screen exactly what the command would send.

    The escape is asked for without `one_line`'s report clip (:data:`_NO_CLIP`), because a
    command cut off after 160 characters is not a command.
    """
    return contain.one_line(command, limit=_NO_CLIP)
