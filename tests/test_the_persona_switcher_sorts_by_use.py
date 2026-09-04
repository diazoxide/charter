"""The persona switcher stops lifting the selected persona, and sorts by use — #882.

Two switchers reach one roster. `F2` opens a picker whose names come from
`frame/switch.personas`, and the frame's sidebar draws a clickable column of the same
names from `statusline._persona_chip_cells` (`frame/builtins._persona_events` turns a
click on one into `frame-switch --persona`). The column used to put the ACTIVE persona
first, so choosing a name re-laid the very list it had been chosen from: every other row
moved, and where a name sat depended on where the operator already was.

The order is now `persona.by_use` on both surfaces:

    **the plane's declared default first, then most-dispatched first, ties broken by the
    larger memory count, then by name.**

**Why dispatches and not memories, measured rather than argued.** Both numbers exist and
they rank differently — on charter's own plane the memory counts read `release 51`,
`steward 47`, `statusline 14`, `forge 13`, `reddit 7` while the dispatch counts read
`release 26`, `forge 18`, `steward 11`, `statusline 8`, `reddit 4`; `forge` is second on
one and fourth on the other. A memory count only ever grows, so it ossifies: a persona
worked heavily one month outranks one used daily the next, permanently. A dispatch count
measures use. `TheTwoKeysDisagree` below is the case that pins the choice, because it is
the only one a sweep swapping the two counts cannot pass.

**Every case here asserts an order in BOTH directions.** `sorted` over a set is not a
pinnable ordering guarantee — one assertion passes about half the time by luck — so a
case that says "more dispatches come first" has a twin that swaps the counts and demands
the other order out of the same code.
"""
from __future__ import annotations

import unittest
from unittest import mock

from charter import config, dispatch, persona, statusline
from charter.frame import choose, switch
from tests._isolation import PersonaIso


class Roster(PersonaIso):
    """A plane whose personas have exactly the dispatch and memory counts a case states."""

    def persona_with(self, name: str, *, dispatches: int = 0, memories: int = 0) -> str:
        """One persona carrying *dispatches* dispatch rows and *memories* memory files.

        Written through `dispatch.record` and `persona.remember` — the real writers — so a
        case measures the store the switcher reads rather than a shape a fixture invented.
        """
        self.make_persona(name, role=name.title(), vault="none")
        for _ in range(dispatches):
            self.assertIsNotNone(dispatch.record(name), f"{name}: a dispatch was not recorded")
        for i in range(memories):
            persona.remember(name, f"# {name} note {i}\n\nbody {i}\n", title=f"{name} {i}")
        self.assertEqual(dispatch.tally().get(name, 0), dispatches, name)
        self.assertEqual(persona.memory_count(name), memories, name)
        return name

    def declare_default(self, name: str) -> None:
        """`charter.toml`'s `[persona] default` — the rung `persona.plane_default` reads
        first, and the one a consumer can find.

        No `config.use` afterwards, deliberately: `declared_default` re-reads the file
        through `instance.load` on every call, and re-deriving would move
        `config.PERSONAS_DIR` under personas this fixture has already written.
        """
        (config.ROOT / "charter.toml").write_text(
            f'schema = 1\n\n[persona]\ndefault = "{name}"\n')

    def column(self) -> list[str]:
        """The sidebar persona column's names, in the order it draws them."""
        return [c.name for c in statusline._persona_chip_cells(None) if c.name]


class TheOrderDoesNotDependOnWhereYouAre(Roster):
    """#882's whole complaint, on both surfaces."""

    def setUp(self) -> None:
        super().setUp()
        self.persona_with("alfa", dispatches=9)
        self.persona_with("bravo", dispatches=5)
        self.persona_with("charlie", dispatches=1)

    def test_the_picker_reads_the_same_whichever_persona_is_active(self):
        """Two opposite standpoints, one list. Without this the list is a function of the
        cursor and no operator can learn where a name lives."""
        persona.set_active("charlie")
        from_charlie = switch.personas()
        persona.set_active("alfa")
        self.assertEqual(switch.personas(), from_charlie)
        self.assertEqual(from_charlie, ["alfa", "bravo", "charlie"])

    def test_the_sidebar_column_reads_the_same_whichever_persona_is_active(self):
        """The surface the lift actually lived on (`statusline.py`'s `order = [active] +
        …`). `charlie` is last by use and was first by the old rule, so this case would
        have failed before #882 and cannot pass by accident now."""
        persona.set_active("charlie")
        from_charlie = self.column()
        persona.set_active("alfa")
        self.assertEqual(self.column(), from_charlie)
        self.assertEqual(from_charlie, ["alfa", "bravo", "charlie"])

    def test_the_least_used_persona_does_not_lead_by_being_chosen(self):
        """Stated as its own case rather than left implicit in the two above: the defect
        was specifically that the chosen name went to index 0."""
        persona.set_active("charlie")
        self.assertEqual(switch.personas()[0], "alfa")
        self.assertEqual(self.column()[0], "alfa")

    def test_the_active_persona_is_still_marked(self):
        """What replaces the lift, and the reason removing it loses nothing: the picker
        still says which row you are on — it just does not move it.

        Selected for the FRAME's id with no terminal, which is `switch.to_persona`'s own
        call: `switch.current_persona` reads the frame's rungs and not this process's, so
        a plane-wide `set_active` would mark nothing here (#411).
        """
        persona.set_active("charlie", session_id="f-882", terminal_id="")
        roster = choose.roster(choose.PERSONA, "f-882")
        self.assertEqual([r.title for r in roster.rows], ["alfa", "bravo", "charlie"])
        self.assertEqual([r.title for r in roster.rows if r.mark], ["charlie"])

    def test_the_two_switchers_offer_one_order(self):
        """A picker and the column beside it drawing the same roster two ways is the drift
        one shared `persona.by_use` exists to make impossible."""
        self.assertEqual(self.column(), switch.personas())


class TheDefaultIsPinnedFirst(Roster):
    def test_the_declared_default_leads_a_persona_with_more_dispatches(self):
        self.persona_with("front", dispatches=0)
        self.persona_with("worker", dispatches=40)
        self.declare_default("front")
        self.assertEqual(switch.personas(), ["front", "worker"])

    def test_and_the_pin_moves_when_the_declaration_does(self):
        """The twin. Same two personas, same counts, the other name declared — so the pin
        is read from the declaration rather than from anything about `front` itself."""
        self.persona_with("front", dispatches=0)
        self.persona_with("worker", dispatches=40)
        self.declare_default("worker")
        self.assertEqual(switch.personas(), ["worker", "front"])

    def test_the_legacy_dotfile_pins_too(self):
        """`personas/.default` keeps resolving — `persona.plane_default` reads both rungs,
        so a plane that adopted the dotfile does not lose its front door to #882."""
        self.persona_with("front", dispatches=0)
        self.persona_with("worker", dispatches=40)
        (config.PERSONAS_DIR / ".default").write_text("front\n")
        self.assertEqual(switch.personas(), ["front", "worker"])

    def test_a_plane_with_no_default_starts_with_the_most_dispatched(self):
        """No pin is a real and ordinary answer, not a missing one."""
        self.persona_with("front", dispatches=0)
        self.persona_with("worker", dispatches=40)
        self.assertIsNone(persona.plane_default())
        self.assertEqual(switch.personas(), ["worker", "front"])

    def test_a_default_naming_a_persona_that_is_not_here_pins_nothing(self):
        """`plane_default` validates against what exists, so a declaration left behind by a
        rename leads to a roster in use order rather than to a phantom first row."""
        self.persona_with("worker", dispatches=40)
        self.persona_with("other", dispatches=1)
        self.declare_default("renamed-away")
        self.assertEqual(switch.personas(), ["worker", "other"])

    def test_the_pinned_persona_appears_exactly_once(self):
        """The pin is a MOVE and not a copy — a roster that listed its front door twice
        would give one persona two rows and `choose.Roster` two ids for one name."""
        self.persona_with("front", dispatches=3)
        self.persona_with("worker", dispatches=40)
        self.declare_default("front")
        names = switch.personas()
        self.assertEqual(names, ["front", "worker"])
        self.assertEqual(len(names), len(set(names)))


class TheRestSortByDispatchCount(Roster):
    def test_more_dispatches_come_first(self):
        self.persona_with("aa", dispatches=2)
        self.persona_with("zz", dispatches=9)
        self.assertEqual(switch.personas(), ["zz", "aa"])

    def test_and_the_order_flips_when_the_counts_do(self):
        """The twin, and the reason it is not optional: the names are `aa` and `zz`, so
        one of these two orders is also the alphabetical one and a switcher that had
        stopped sorting at all would still pass the case above."""
        self.persona_with("aa", dispatches=9)
        self.persona_with("zz", dispatches=2)
        self.assertEqual(switch.personas(), ["aa", "zz"])

    def test_a_persona_never_dispatched_sorts_last_rather_than_missing(self):
        """Zero is a count, not an absence — `dispatch.tally` has no row for a persona
        nobody has dispatched, and reading that as "no data, leave it where it was" is how
        an unused persona keeps a position it has not earned."""
        self.persona_with("used", dispatches=4)
        self.persona_with("never", dispatches=0)
        self.assertEqual(switch.personas(), ["used", "never"])


class TheTieBreakIsTheMemoryCount(Roster):
    """The tie-break is exactly what a `drop-conjunct` sweep deletes and what a
    `swap-synonym` sweep points at the other count, so both directions are pinned."""

    def test_a_tie_on_dispatches_is_broken_by_the_larger_memory_count(self):
        self.persona_with("aa", dispatches=3, memories=1)
        self.persona_with("zz", dispatches=3, memories=6)
        self.assertEqual(switch.personas(), ["zz", "aa"])

    def test_and_the_tie_break_flips_when_the_memory_counts_do(self):
        """The twin. Same dispatch counts, memories swapped — so a switcher that had
        dropped the tie-break and fallen through to the name would fail here while
        passing the case above."""
        self.persona_with("aa", dispatches=3, memories=6)
        self.persona_with("zz", dispatches=3, memories=1)
        self.assertEqual(switch.personas(), ["aa", "zz"])

    def test_a_tie_on_both_counts_falls_back_to_the_name(self):
        """The order is TOTAL. Two personas alike in both numbers still have one order,
        the same one on every machine — `sorted` over a set would have been an ordering
        that is right about half the time by luck."""
        self.persona_with("zz", dispatches=2, memories=2)
        self.persona_with("aa", dispatches=2, memories=2)
        self.assertEqual(switch.personas(), ["aa", "zz"])
        self.assertEqual(switch.personas(), switch.personas())


class TheTwoKeysDisagree(Roster):
    """The case #882 was actually decided on, and the only one that can tell the two
    counts apart.

    Every other case above would pass just as well with the keys exchanged, because on a
    plane where the two numbers agree there is nothing to choose between them. Charter's
    own roster is not such a plane — `forge` is second by dispatch and fourth by memory —
    and this is that shape reduced to two personas.
    """

    def test_the_persona_dispatched_more_leads_the_one_remembered_more(self):
        self.persona_with("used", dispatches=20, memories=1)
        self.persona_with("remembered", dispatches=2, memories=50)
        self.assertEqual(switch.personas(), ["used", "remembered"])

    def test_and_it_still_does_when_the_names_would_say_otherwise(self):
        """The twin: alphabetical order now agrees with the memory count and disagrees
        with the dispatch count, so neither a fallback to the name nor a swap to memories
        can pass both halves."""
        self.persona_with("aa", dispatches=2, memories=50)
        self.persona_with("zz", dispatches=20, memories=1)
        self.assertEqual(switch.personas(), ["zz", "aa"])

    def test_growing_a_memory_count_does_not_reorder_the_roster(self):
        """The ossification argument, measured rather than asserted in a docstring: a
        memory count only ever grows, so if it ranked, a persona could take a position it
        had stopped earning and never give it back."""
        self.persona_with("used", dispatches=20, memories=0)
        self.persona_with("remembered", dispatches=2, memories=0)
        before = switch.personas()
        for i in range(30):
            persona.remember("remembered", f"# more {i}\n\nbody\n", title=f"more {i}")
        self.assertEqual(switch.personas(), before)
        self.assertEqual(before, ["used", "remembered"])


class TheMemoryCountIsReadOnlyOnATie(Roster):
    """The cost half. A switcher opens on a keypress and `persona.memory_count` is a
    directory glob per persona, so the tie-break is a second pass over the tied runs
    rather than a second term in one sort key — a key function is called once per element,
    which would pay one glob per persona on every open to decide an order the dispatch
    count has almost always already decided.
    """

    def _counted(self):
        calls = []
        real = persona.memory_count
        patcher = mock.patch.object(
            persona, "memory_count", side_effect=lambda n: (calls.append(n), real(n))[1])
        self.enterContext(patcher)
        return calls

    def test_distinct_dispatch_counts_read_no_memory_directory_at_all(self):
        self.persona_with("aa", dispatches=1, memories=3)
        self.persona_with("bb", dispatches=2, memories=3)
        self.persona_with("cc", dispatches=3, memories=3)
        calls = self._counted()
        self.assertEqual(switch.personas(), ["cc", "bb", "aa"])
        self.assertEqual(calls, [], "a memory directory was walked to break no tie")

    def test_only_the_tied_personas_are_asked(self):
        """The twin, and the one that keeps the case above from being satisfied by a
        tie-break that never runs: `bb` and `cc` tie and are read, `aa` stands alone and
        is not."""
        self.persona_with("aa", dispatches=1, memories=3)
        self.persona_with("bb", dispatches=5, memories=1)
        self.persona_with("cc", dispatches=5, memories=9)
        calls = self._counted()
        self.assertEqual(switch.personas(), ["cc", "bb", "aa"])
        self.assertEqual(sorted(set(calls)), ["bb", "cc"])
        self.assertNotIn("aa", calls)

    def test_the_dispatch_store_is_read_once_for_the_whole_roster(self):
        """One glob of `personas/_dispatch/` per open, never one per persona: the store is
        a handful of month-and-host files, and asking it by name would re-walk all of them
        for every row."""
        for i in range(6):
            self.persona_with(f"p{i}", dispatches=i)
        with mock.patch.object(dispatch, "tally", wraps=dispatch.tally) as tally:
            switch.personas()
        self.assertEqual(tally.call_count, 1)


class ItDegradesRatherThanRaising(Roster):
    """A switcher opens on a keypress and the sidebar repaints several times a second;
    neither may be taken down by a number that could not be read."""

    def test_an_unreadable_dispatch_store_leaves_an_alphabetical_roster(self):
        self.persona_with("zz", dispatches=9)
        self.persona_with("aa", dispatches=1)
        with mock.patch.object(dispatch, "tally", side_effect=OSError("nope")):
            self.assertEqual(switch.personas(), ["aa", "zz"])

    def test_an_unreadable_memory_directory_counts_as_zero(self):
        """The tie-break's own fail-toward-no-change: the persona charter cannot read is
        demoted within its tie group and nothing raises out of a repaint."""
        self.persona_with("aa", dispatches=3, memories=0)
        self.persona_with("zz", dispatches=3, memories=4)
        with mock.patch.object(persona, "memories", side_effect=OSError("nope")):
            self.assertEqual(persona.memory_count("zz"), 0)
            self.assertEqual(switch.personas(), ["aa", "zz"])

    def test_a_plane_with_no_personas_answers_an_empty_list(self):
        self.assertEqual(persona.by_use(), [])
        self.assertEqual(switch.personas(), [])


class TheNameCheckHappensBeforeTheOrdering(Roster):
    """`persona.valid_name` is the rule `persona.dir_of`'s join depends on (#442), and
    `by_use` reads a memory directory per tied name — so a name the switcher may not draw
    must not reach it."""

    def test_a_name_the_switcher_refuses_is_never_counted(self):
        self.persona_with("ok", dispatches=1)
        with mock.patch.object(persona, "list_personas",
                               return_value=["ok", "../escape", "with space"]), \
             mock.patch.object(persona, "memory_count", wraps=persona.memory_count) as mc:
            self.assertEqual(switch.personas(), ["ok"])
        self.assertNotIn("../escape", [c.args[0] for c in mc.call_args_list])


if __name__ == "__main__":
    unittest.main()
