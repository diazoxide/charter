"""**Every row that makes a kind of thing has a row that unmakes the same kind — #921.**

*"now for example no way to reactivelly close harness session, we have plus button - that
adding new session tab, but no any option to delete/stop, no any modal/drawer for
confirmation."*

Closing a chat had been possible the whole time — `F2 → chat: close`, or right-click a tab.
The operator could not find either and concluded, from the evidence on screen, that it was
not possible, and **that conclusion was correct given what the frame said about itself**:
the frame advertises exactly two things, `F2 palette` on the attention strip and `+` on the
chats strip. An unadvertised *create* costs an operator a feature they did not know about;
an unadvertised *destroy* makes them conclude the system will not let them do it, which is
strictly worse and is what happened here.

**The rule is asked of the CATALOGUE and not of drawn glyphs**, and that is the decision
this file rests on rather than a convenience. The rows are data
(`commands_frame._palette_catalogue`); `[frame] mouse` ships false, so on charter's own
default plane the palette is not merely the primary surface, it is the only one; and it is
the surface that works at every width, including the widths where a strip degrades to `2/3`
and then to nothing. A rule stated over what a bar happened to draw would be a rule that
switches itself off on the plane it matters most on.

**Stated as an EQUALITY of the two sides, which is what makes it a tripwire in both
directions.** On `main` the catalogue held `chat: close` and no `chat: new` — the create
half was reachable only through a pointer affordance most planes never draw
(`frame/events.py`: *"give every pointer affordance a key as well"*) — so the failure this
file was written to show is the destroy side standing alone::

    AssertionError: Items in the second set but not the first:
    'chat' : the palette unmakes a kind of thing it does not offer to make …

and the failure it exists to catch on some future day is the create side standing alone,
which is #921's own report. One assertion covers both because it is one rule.

**`charter: quit` is deliberately in neither verb set.** Quit stops every harness on the
plane and takes nothing away that comes back differently — `leave._close_summary` is the
one verb that says *forget* — and there is no `charter: new`, because a plane is not
something you make from inside one. A rule that demanded a counterpart for it would be
demanding a row that cannot exist, which is how an invariant gets suppressed rather than
kept.
"""

from __future__ import annotations

import os
import unittest
from unittest import mock

from charter import commands_frame, util
from charter.frame import builtin_actions, choose, leave, state

from tests._isolation import PersonaIso
from tests.test_frame_pickers import _plane_personas, _plane_workspaces

#: The title verbs that MAKE a kind of thing, and the ones that UNMAKE the same kind.
#:
#: Closed sets, and small on purpose: what they are asserted against is charter's own
#: catalogue, whose titles are charter's own sentences in one shape — `<noun>: <verb> —
#: <what it does>`. A rule that tried to read intent out of open prose would be a rule that
#: goes quiet the first time somebody writes a title slightly differently, which is the
#: failure mode this file is guarding *against*.
#:
#: A verb added here is a claim that charter has started making or unmaking a new kind of
#: thing, and the assertion below is what then asks for its counterpart.
_MAKES = frozenset({"new", "create"})
_UNMAKES = frozenset({"close", "delete", "remove"})


def _noun_and_verb(row) -> tuple[str, str] | None:
    """The `<noun>: <verb>` a catalogue row leads with, or ``None`` for a row that leads
    with neither.

    `detach — leave the harness running` and `refresh — gather this workspace's repos…`
    are the two rows that carry no noun at all, and they are answered ``None`` here rather
    than being special-cased at the call site: neither makes nor unmakes anything, and a
    row with no noun cannot be asked for a counterpart of a kind it never named.
    """
    noun, sep, rest = row.title.partition(":")
    words = rest.split()
    if not sep or not words or " " in noun.strip():
        return None
    return noun.strip(), words[0]


def _catalogue_rows(fid: str):
    """The palette's own catalogue for *fid*, with the name pickers dropped.

    **`choose.noun_of` is what drops them, and never a title test.** A picker row's title
    is `<noun>: <the name this frame is on> — pick another`, so a plane holding a workspace
    called `new` would put `workspace: new` in front of this rule — a NAME read as a verb.
    `choose.noun_of` is the seam `commands_frame._draw_palette` itself uses to tell a
    doorway onto a list of names from a row that does something, matched against the ids
    that module mints rather than by splitting on a colon, so this asks the question
    charter already answers instead of inventing a second answer that a name can fool.
    """
    reg = builtin_actions.build(fid, current_density="normal", current_chrome="off")
    rows = commands_frame._palette_catalogue(fid, reg, snapshot={})
    return [r for r in rows if choose.noun_of(r) is None]


class _Palette(PersonaIso):
    """One frame on an isolated plane, with the palette's catalogue reachable."""

    FID = "f-pair"

    def setUp(self) -> None:
        super().setUp()
        self.enterContext(mock.patch.dict(
            os.environ, {"CHARTER_SESSION_ID": self.FID}, clear=True))
        _plane_workspaces("alpha")
        _plane_personas("steward")
        state.frame_dir(self.FID, create=True)
        state.record_server(self.FID, "charter")
        state.record_harness_pane(self.FID, "%3")
        state.record_workspace(self.FID, "alpha")


class ThePaletteAdvertisesUnmakingWhatItMakes(_Palette, unittest.TestCase):
    """The rule itself, over the rows `F2` is built from."""

    def test_every_kind_the_palette_makes_it_also_unmakes(self):
        """**The whole rule, as one equality.**

        Read the failure in the direction it fires. A noun on the *makes* side alone is
        #921's report — a surface offering to create something with no visible way to
        remove it, which an operator reads as a refusal rather than an omission. A noun on
        the *unmakes* side alone is what `main` shipped: `chat: close` in the catalogue
        with no `chat: new` beside it, so the only route to a new chat was a `+` that
        `[frame] mouse = false` never draws.
        """
        makes, unmakes = set(), set()
        for row in _catalogue_rows(self.FID):
            pair = _noun_and_verb(row)
            if pair is None:
                continue
            noun, verb = pair
            if verb in _MAKES:
                makes.add(noun)
            if verb in _UNMAKES:
                unmakes.add(noun)
        self.assertEqual(
            makes, unmakes,
            f"the palette makes {sorted(makes)} and unmakes {sorted(unmakes)} — every "
            "kind of thing a surface offers to make must be a kind it offers to unmake, "
            "at the same weight and in the same place (#921)")

    def test_and_chat_is_the_kind_it_is_about(self):
        """The rule above is satisfiable by removing rows, and this is what says which
        answer #921 asked for. `chat` is on both sides, by name."""
        pairs = [p for p in map(_noun_and_verb, _catalogue_rows(self.FID)) if p]
        self.assertIn(("chat", "new"), pairs, "no palette row makes a chat")
        self.assertIn(("chat", "close"), pairs, "no palette row unmakes a chat")


class TheNewChatRowStartsTheSameCommandThePlusDoes(_Palette, unittest.TestCase):
    """**One command, two routes, and neither is the other's fallback.**

    `builtins._NEW_CHAT` is what the chat strip's `+` spawns and this is what the palette
    row spawns; they must be the same `charter frame-new-chat`, or a plane with a pointer
    and a plane without would be making chats two different ways.
    """

    def _run(self):
        reg = builtin_actions.build(self.FID, current_density="normal",
                                    current_chrome="off")
        with mock.patch.object(builtin_actions, "_spawn") as spawn:
            inv = reg.invoke("chat.new", fid=self.FID, snapshot={})
            inv.join(timeout=5)
        return inv, spawn

    def test_it_starts_frame_new_chat_for_this_frame(self):
        inv, spawn = self._run()
        self.assertTrue(inv.started, inv.reason)
        argv = spawn.call_args.args[0]
        self.assertEqual(argv, util.self_relaunch_argv("frame-new-chat",
                                                       "--chat", self.FID))
        self.assertEqual(spawn.call_args.kwargs, {"fid": self.FID})

    def test_it_is_offered_even_where_the_command_will_refuse(self):
        """**`cmd_new_chat` owns its four refusals and says each on the attention row.**

        A second reading of them here would refuse a row the `+` beside it still offers —
        two routes to one command degrading differently, which is exactly what
        `builtins._bar_events` keeps `add` as data to prevent. #512's rule points the same
        way: an option you cannot see is one you cannot ask about.
        """
        reg = builtin_actions.build(self.FID, current_density="normal",
                                    current_chrome="off")
        row = [o for o in reg.offers(fid=self.FID, snapshot={}) if o.id == "chat.new"]
        self.assertEqual(len(row), 1, "the palette lost its new-chat row")
        self.assertTrue(row[0].available)
        self.assertFalse(row[0].reason)


class TheNewChatRowKeepsTheHarmlessHalfOfTheList(_Palette, unittest.TestCase):
    """**Placement is a guard, not a taste** (`leave.open_rows`).

    The cursor starts on the first row that can run, so the destructive rows are appended
    last and a new row must not disturb that. `chat: new` destroys nothing, so it belongs
    with the harmless rows — and this is what says it stayed there.
    """

    def test_it_is_above_both_destructive_rows(self):
        titles = [r.title for r in _catalogue_rows(self.FID)]
        self.assertLess(titles.index("chat: new — another chat in this workspace"),
                        titles.index(leave.OPEN_QUIT))
        self.assertLess(titles.index("chat: new — another chat in this workspace"),
                        titles.index(leave.OPEN_CLOSE))

    def test_and_the_destructive_rows_are_still_the_last_two(self):
        titles = [r.title for r in _catalogue_rows(self.FID)]
        self.assertEqual(titles[-2:], [leave.OPEN_QUIT, leave.OPEN_CLOSE])


if __name__ == "__main__":  # pragma: no cover
    unittest.main()
