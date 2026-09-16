"""Giving a tab a name a person chose — the bound on one, and the surface that takes one.

**A title is for people; the id goes on doing the linking** (decision 11, ruling 1). Nothing
here replaces a chat id anywhere: every link, record, kill, reap and claim is still aimed at
`<workspace>.<n>`, and a chat with no title behaves exactly as it did before this module
existed. What a title changes is what is DRAWN wherever a chat is named to a person — the
strip, the tab menu, the quit and close rows, the ended tab's resume row, the chat picker —
and what Claude Code is started under (`launcher.session_name`).

**A title is text a person wrote, so it is bounded on the way in and contained on the way
out** (ruling 35). :func:`normalized` is the bound and it is the ONLY one: `state
.record_title` asks it rather than spelling the rule again, so what charter refuses to take
and what charter writes down cannot come from two readings. The containment on the way out is
`chats.title_of`, because the file is one a chat can write.

**The bound is one line, printable, and at most `state.TITLE_MAX`.**

* *One line* is `" ".join(text.split())`: every run of whitespace — a newline, a tab, a
  vertical tab, U+2028, U+0085, a non-breaking space — becomes one ASCII space. A title
  cannot forge a second row of a strip or a second row of a menu, because it cannot hold a
  line break at all.
* *Printable* is `str.isprintable()` asked of what is left, and it REFUSES rather than
  escaping. That is the one place this differs from `contain.one_line`, which would write an
  ESC into the record as the six characters ``\\x1b`` and call it contained — true, and not
  what an operator who typed a control byte into `charter frame-rename` meant. Charter says
  the rule worked and renames nothing.

  **`contain.one_line` would be a no-op here and is deliberately not called.** Measured over
  its own definition: after the whitespace collapse the only characters it still escapes are
  `Cc` that are not whitespace, `Cf` and `Cs` — and every one of those answers
  `isprintable()` False, so the bound below already refuses each of them. A second
  containment no input could make observable is the masked line `frame/chats.py`'s docstring
  records and the survivor `tools/sweep.py` reports.
* *At most 60* is `state.TITLE_MAX`, and the refusal SAYS how long the title was
  (:data:`TOO_LONG`), because "too long" without a number is a refusal the operator cannot
  act on.

**Renaming touches no harness** (ADR 0018, and ruling 3 of this task). There is no
`send-keys`, no `/rename` and no respawn anywhere in this module or in
`commands_frame.cmd_rename`: the new name reaches Claude Code at its next start or resume,
through `--name` again, and never reaches Codex or opencode at all — neither takes a name at
launch (`docs/harnesses.md`).

**Nothing here is on the import path** (ruling 43). `frame/state.py` imports this module
inside :func:`state.record_title`, and `commands_frame` reaches it at call time.
"""

from __future__ import annotations

from .. import contain
from . import state

#: What a title longer than `state.TITLE_MAX` is refused with. It names the rule and the fix
#: in one breath (CONTEXT.md's *A refusal is the rule working*) and it counts, because a
#: refusal that says only "too long" leaves the operator guessing at how much to cut.
#:
#: The number it reports is the length of the NORMALISED title, which is what was measured —
#: a sentence counting the raw text would be counting whitespace this module has already
#: collapsed.
TOO_LONG = "a title is at most {max} characters — this one is {n}; nothing was renamed"

#: What a title holding something with no glyph is refused with. One sentence for every such
#: character — an ESC, a BEL, a zero-width joiner, a lone surrogate — because they are one
#: rule and an operator acts on all of them the same way: type a title.
NOT_PRINTABLE = "a title is one line of printable text — nothing was renamed"


def normalized(text: str) -> tuple[str | None, str]:
    """*text* as a title, or ``(None, why it cannot be one)``.

    ``("", "")`` is *no title*, which is how a rename REMOVES one: `state.record_title`
    deletes the file for it, and every surface goes back to drawing the id.

    Two answers rather than one, because the two callers want different halves: the gate
    (`state.record_title`) acts on the title and never says anything, and the surface
    (`rename.Rename`, `commands_frame.cmd_rename`) has an operator in front of it who has to
    be told which rule fired. A single ``None`` would make *removed* and *refused*
    indistinguishable at the gate, which is the one distinction a rename turns on.

    The order is the order the rules cost and the order they are about: collapse first,
    because both bounds below are asked of the collapsed string and a title counted before
    its whitespace was collapsed would be refused for length it does not have.
    """
    shown = " ".join(str(text).split())
    if not shown:
        return "", ""
    if not shown.isprintable():
        return None, NOT_PRINTABLE
    if len(shown) > state.TITLE_MAX:
        return None, TOO_LONG.format(max=state.TITLE_MAX, n=len(shown))
    return shown, ""


def first_line_title(brief: str) -> str:
    """The title a handoff's *brief* gives its chat — the first non-blank line, cut to fit.

    **Cut and never refused**, which is the one place a title is clipped rather than bounded.
    A brief is not a label somebody typed: it is the whole message a model wrote and the
    operator approved at the harness prompt (ADR 0021), and refusing to title the chat
    because that message is long would leave the tab bare for a reason nobody can act on. A
    typed title is the other case and is refused with its length (:data:`TOO_LONG`).

    **`contain.readable` and not `contain.one_line`, because the answer has to survive
    :func:`normalized`.** `one_line` escapes five Unicode categories, so a private-use or
    unassigned codepoint in a brief would come through it unchanged, answer
    `str.isprintable()` False, and be REFUSED by the gate — a handoff whose chat is silently
    untitled, which is exactly what "never refused" promises does not happen. `readable`
    says instead what a title MAY hold — U+0020..U+007E — and escapes everything else
    whatever its category, so what comes back is printable ASCII by construction.

    Cut at ``TITLE_MAX - 3`` so the whole answer, marker included, is at most
    `state.TITLE_MAX` characters — `contain.readable`'s own ``...``, which is ASCII like the
    rest of what it returns, rather than an ellipsis a clipped title would be the only user
    of.

    ``""`` for a brief that is empty or all blank lines, which is every open that is not a
    handoff: `state.record_title` reads it as *no title* and writes nothing at all.
    """
    line = next((l for l in brief.splitlines() if l.strip()), "")
    if not line.strip():
        # Before `contain.readable`, which answers `contain.BLANK` — the two characters `""`
        # — for a value whose whole rendering is spaces. That marker is right for a sentence
        # naming a value and wrong for a tab, which would draw a pair of quotes as its name.
        return ""
    return contain.readable(line.strip(), state.TITLE_MAX - len("..."))
