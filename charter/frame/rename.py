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

**A leading `-` is NOT refused, and that is measured rather than assumed.** A title becomes
the front of `--name`'s value (`launcher.session_name`), so a strict option parser could read
`--name --test` as a flag with its argument missing and refuse to start — which would be a tab
whose harness cannot launch. Measured on Claude Code **2.1.273**, in a throwaway `HOME` and
`CLAUDE_CONFIG_DIR`, with no prompt typed and nothing spent:

===========================================  ====  =======================================
argv                                         rc    what happened
===========================================  ====  =======================================
``--name x --version``                       0     version printed (the control)
``--name --test --version``                  0     version printed — the value was taken
``--name -x --version``                      0     version printed
``--name --version``                         1     `Not logged in` — ``--version`` ITSELF
                                                   was taken as the value
===========================================  ====  =======================================

The last row is the one that settles it: `--name` takes the next argv element whatever it
looks like, so a dash-leading title is a value and never a flag. Charter passes argv as a list
through `os.execvpe` with no shell in between, so the parser was the only thing that could
have read it as anything else. A refusal here would cost an operator a title charter had no
reason to refuse.


**Renaming touches no harness** (ADR 0018, and ruling 3 of this task). There is no
`send-keys`, no `/rename` and no respawn anywhere in this module or in
`commands_frame.cmd_rename`: the new name reaches Claude Code at its next start or resume,
through `--name` again, and never reaches Codex or opencode at all — neither takes a name at
launch (`docs/harnesses.md`).

**Nothing here is on the import path** (ruling 43). `frame/state.py` imports this module
inside :func:`state.record_title`, and `commands_frame` reaches it at call time.
"""

from __future__ import annotations

from dataclasses import dataclass

from .. import contain, util
from . import overlay, palette, state

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


# --------------------------------------------------------------------------- #
# The surface: one line of input, in the pane the operator is already looking at.
# --------------------------------------------------------------------------- #

#: The row that OPENS the input from `F2`, and the row inside it that Enter acts on. Both
#: carry a `:`, which is `frame/choose.py`'s trick and the whole of why they cannot collide
#: with an action: `frame/action.py` holds every action id to `component.usable_id` — lower
#: case letters, digits, underscores and at most one dot — so a provider cannot ship an action
#: called `rename:go` and take this keypress. Neither id is ever drawn.
OPEN_ID = "rename:open"
GO_ID = "rename:go"

#: The `F2` doorway's title. `chat:` because that is the noun it is about, exactly as
#: `leave.OPEN_CLOSE` and `choose.open_rows` are read by an operator scanning left edges.
OPEN_RENAME = "chat: rename — give this tab a title"

#: What the doorway says on a frame whose own chat charter cannot resolve — `leave
#: .NO_CHAT_HERE`'s case one verb over, and listed with its reason rather than dropped
#: (#512: an option you cannot see is one you cannot ask about).
NO_CHAT_HERE = ("charter cannot tell which chat this palette was opened in, so it has no tab "
                "to rename — `charter frame-rename <chat> -- <title>` names one")

#: What `charter frame-rename` says for a name that is not a chat of this plane. It names the
#: rule and the fix in one breath, and the value in it is contained: it comes off an argv.
NOT_A_CHAT = ("no chat '{chat}' on this plane, so nothing was renamed — `charter frame-rename "
              "<chat> -- <title>` takes a chat id as charter spells it")

#: The one row the input draws, and what it says before anything is typed. It names what an
#: empty Enter does, because *take the title off* is the one outcome an operator cannot guess
#: from a blank line.
TYPED = "title: {text}"
NOTHING_TYPED = "(none) — Enter takes this tab's title off"

#: The input's bottom line. `enter rename` rather than `enter choose`: there is one row and it
#: is not a list, so the only thing worth spelling is what the two keys do.
FOOTER = "  type a title   enter rename   esc cancel"

#: What the notice says after a rename landed. Two sentences, because a rename means two
#: different things depending on the harness: Claude Code is started under the name charter
#: composes, so the operator is told WHEN it will see it; Codex and opencode never do, so
#: promising them anything would be charter claiming a parity it does not have (ruling 4).
RENAMED = "renamed"
RENAMED_AT_NEXT_START = ("renamed — {harness} sees the new name the next time it starts or "
                         "resumes")


def label(target: str) -> str:
    """The input's heading. Named, for `tabmenu.label`'s reason: this surface may be about a
    tab the frame is NOT on, and a bare `charter` over a one-line input would leave the
    operator typing a name for a chat they cannot see."""
    return f"rename {target}"


def takes_a_name(harness: str) -> bool:
    """Whether *harness* is started under the name charter composes.

    **Asked of the registry's own argv builders, never by naming Claude Code here** (ruling
    4, and `leave.resumable_harness`'s rule one question over). Codex names no flag at all
    (openai/codex#14482 is open) and opencode's `--title` exists only on `opencode run`, so
    both answer ``False`` — and a harness that grows a name flag tomorrow answers ``True``
    here on the day its `new_session_argv` does, rather than on the day somebody remembers
    this sentence.

    ``False`` for a harness charter has no record of, which is the migration case: the notice
    then promises nothing, which is the honest half of not knowing.
    """
    from ..harness import registry
    h = registry.get(harness)
    if h is None:
        return False
    return (h.new_session_argv("s", "n") != h.new_session_argv("s", "")
            or h.resume_argv("s", "n") != h.resume_argv("s", ""))


def renamed_note(harness: str) -> str:
    """What the frame's attention row says after a rename landed.

    **The harness name is NOT contained here, and the sweep is what asked.** It is read off
    `state.identity`, which is a file a chat can write — so ruling 35 would ordinarily apply.
    It cannot be reached: the only branch that puts *harness* in a sentence is the one
    :func:`takes_a_name` answers ``True`` for, and that function answers from
    `harness.registry`, whose names are charter's own literals. A value off disk that is not
    one of them answers ``False`` and reaches :data:`RENAMED`, which names nothing. A
    containment no input can make observable is the survivor `tools/sweep.py` reports, and
    this one was reported.
    """
    return (RENAMED_AT_NEXT_START.format(harness=harness)
            if takes_a_name(harness) else RENAMED)


def open_rows(fid: str) -> tuple[overlay.Row, ...]:
    """The `F2` doorway row, and it costs no scan.

    **A doorway and not an action**, for `leave.open_rows`' reason exactly: an `Action`'s
    contract is *fire-and-report*, and opening an input starts nothing — it replaces the
    surface in the pane the operator is already looking at.

    **Before `leave.open_rows` and after everything else** (`commands_frame
    ._palette_catalogue`). Renaming is harmless and the leaving rows are not, so the guard
    that keeps a destructive row off the first row that can run is untouched: every row above
    `charter: quit` can still run on an ordinary plane.

    *fid* decides one thing: a palette that cannot resolve its own chat has no tab to rename,
    so the row is listed with its reason rather than dropped.
    """
    return (overlay.Row(id=OPEN_ID, title=OPEN_RENAME,
                        note="" if fid else NO_CHAT_HERE, refused=not fid),)


def is_row(row) -> bool:
    """Whether *row* belongs to this module at all.

    What tells `commands_frame._draw_palette` that a chosen row must NOT be handed to
    `ActionRegistry.invoke` as an action id — `leave.is_row`'s job, and the reason that
    function exists is the defect this one avoids: an id nothing recognised fell through to
    `invoke`, and the operator was told an action had failed instead of getting what they
    pressed.
    """
    return row.id in (OPEN_ID, GO_ID)


@dataclass
class Rename(palette.Palette):
    """A one-line input, drawn as the palette it is.

    **`palette.Palette` with the filter turned off, and that is the whole design.** The
    palette already reads one printable character at a time out of `overlay.decode`
    (`Palette.handle`), already has backspace, already leaves on Escape, and already owns a
    pane modally — so a title typed here cannot hold a newline or an escape sequence by the
    route it is built, whatever :func:`normalized` then says about it. What this changes is
    :meth:`_refilter`: the query is not a filter over rows, it IS the value, and the one row
    shows it back.

    **Nothing here reads or writes a chat's state.** Enter hands the typed text to `charter
    frame-rename` through `builtin_actions._spawn` — a `Popen` argv, no tmux — for
    `tabmenu.chose`'s measured reason: the surface's pane is killed the instant a row is
    chosen, and `kill-pane` hands SIGHUP to that pane's process group.
    """

    #: The tab this input is about. Not always the chat the surface is drawn in: the tab menu
    #: opens it over the tab the pointer landed on.
    target: str = ""

    #: A row here is not chosen from a list, so there is nothing for a cut to hide — but the
    #: value being typed IS arbitrary text, and a pane too narrow to show all of it must say
    #: so rather than let the operator believe they typed less (ruling 45).
    says_what_it_hid: bool = True

    footer: str | None = FOOTER

    def typed(self) -> str:
        """What has been typed, raw. `normalized` is what bounds it, and it is asked by the
        one gate (`state.record_title`) rather than here — a surface that pre-normalised
        would be a second reading of the same rule."""
        return self.query

    def refuse(self, why: str) -> None:
        """Say *why* in the footer and leave everything else alone.

        **The refusal stays in the footer and the surface stays up**, which is
        `frame/selector.py`'s ruling 6 one surface over and for a sharper reason: what the
        operator typed is in the query, and a surface that closed on a refusal would throw
        away sixty characters to tell them one of them was wrong. The next keystroke clears
        it (:meth:`_refilter`), because a reason that outlived its own occasion would be
        attributed to whatever is typed next.
        """
        self.footer = why

    def _refilter(self) -> None:
        # **The one place this stops being a filter.** `Palette._refilter` narrows the
        # catalogue by the query, which on a one-row surface would make the row vanish the
        # moment the operator types anything that is not in its title — an input that erases
        # itself. The row is composed from the query instead, so what is on screen is what
        # will be recorded.
        self.rows = (overlay.Row(id=GO_ID,
                                 title=TYPED.format(text=self.query or NOTHING_TYPED)),)
        self._sel = 0
        self._top = 0
        self.said = ""
        self.footer = FOOTER
        self._headline()


def opens(row, target: str) -> "Rename | None":
    """The surface *row* opens, or ``None`` when it opens none.

    `commands_frame._picker`'s job for this row, and the same two-line shape: a doorway is
    told apart by its id, and what comes back replaces the surface in the pane the operator is
    already looking at (`palette.own_the_tty`'s *then*).

    A REFUSED doorway opens nothing — the palette that could not name its own chat has no tab
    to rename, and a surface over a target charter cannot name would be an offer it already
    knows it cannot honour (`choose.open_rows`' rule).
    """
    if row.id != OPEN_ID or row.refused:
        return None
    return Rename(target=target, label=label(target), mouse=True)


def again(row, opened: "Rename | None") -> "Rename | None":
    """The input handed back with its refusal showing, or ``None`` when Enter may go through.

    **The bound is asked HERE as well as at the gate, and the second ask is not a duplicate
    rule — it is the same rule asked where there is somebody to tell.** `state.record_title`
    refuses silently, because a handoff and a restore have no operator in front of them; this
    is the surface that does, and ruling 6's whole point is that a refusal an operator can
    read costs them nothing they typed.
    """
    if opened is None or row is None or row.id != GO_ID:
        return None
    shown, why = normalized(opened.typed())
    if shown is not None:
        return None
    opened.refuse(why)
    return opened


def chose(row, target: str, *, fid: str, text: str) -> bool:
    """Act on the row Enter landed on. Answers whether anything was started.

    **`builtin_actions._spawn`, never a bare `Popen`**, for `tabmenu.chose`'s measured
    reason: the surface closes the instant a row has been chosen, and `kill-pane` hands
    SIGHUP to this process's group, so work started in-process dies with the pane it was
    started from.

    **The title travels on an ARGV and never through tmux** (the plan's global constraint:
    `layout.CARRIABLE` is unchanged and nothing new crosses tmux but flags and closed-alphabet
    ids). `_spawn` is a `Popen` with a list argv — no shell, no `split-window`, no
    `set-environment` — so the one place a person's words go is the child's own `sys.argv`.

    *target* is the tab and *fid* is the chat this surface was opened over: `charter
    frame-rename <target>` says which tab to rename, and `--chat <fid>` says where the
    keypress came from, which is what puts the notice on the screen the operator is actually
    looking at. That is `tabmenu.chose`'s own split, made for the same reason.

    **`--chat` goes BEFORE the tab, and that ordering is measured rather than stylistic.**
    The title is `nargs=REMAINDER`, which swallows everything from the first token it reaches
    — options included. Measured on this tree: `frame-rename <tab> --chat <fid> -- <text>`
    parses `--chat`, `<fid>` and the separator INTO THE TITLE and leaves `args.chat` empty, so
    the notice would be written to no frame at all. With the option in front, the positional
    and the remainder each get what they are for.
    """
    if row is None or row.id != GO_ID:
        return False
    from .builtin_actions import _spawn
    _spawn(util.self_relaunch_argv("frame-rename", "--chat", fid, target, "--", text),
           fid=fid)
    return True
