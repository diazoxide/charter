"""What an action is, what it is handed, and the two properties that are not negotiable.

An action is the half of the command surface that *does* rather than draws, so the
contract is stricter than a component's in exactly two ways, and both are pinned here
against the mutation that would remove them.

**Fire-and-report, never blocking** (§4g). `ActionRegistry.invoke` returns having STARTED
the work, and `Invocation` is the receipt. `TheCallerIsNeverBlocked` proves that with an
action that is deliberately still inside `run` when `invoke` returns — the assertion is
about the receipt reporting the work as running, taken at a moment the action provably
cannot have finished, rather than about how long anything took. A blocking action in a TUI
is indistinguishable from a hang, and the operator's only recourse would be the escape
hatch, for something working correctly.

**An action cannot reach a vault value.** `NoRouteFromAnActionToAVaultValue` installs a
REAL plain-file vault holding a real value and then goes looking for it two ways, neither
of which is a spelling check:

1. Everything *reachable* from the ctx charter built — instance dictionaries, containers,
   closure cells, bound methods' receivers — is collected and searched. A `spend()` that
   closed over the resolved value, or an inventory that cached its provider, is caught by
   this without the test knowing either exists.
2. Every public callable on what charter handed the action is CALLED, with the vault name
   and the key name it would need, and every answer searched the same way. A `value()`
   method added tomorrow is caught by this without the test naming it.

Both carry a positive control: the same walk, on the same object, finds the vault's NAME
and the key's NAME. Without that, a walk that traversed nothing would pass, which is the
shape where "the guard holds" and "the probe never looked" are the same green.

**An unavailable action carries WHY.** Never omitted from the listing, never listed with
an empty reason, and contained before it can reach a row (#472, and `[frame] hotkey`'s
newline is what a committed value reaching an overlay row costs).
"""

from __future__ import annotations

import threading
import unittest
from collections.abc import Mapping

from charter import inflight
from charter.frame import action, actions
from charter.secrets import registry as vaults
from tests._isolation import PersonaIso

#: How long a deliberately-blocked action stays inside `run` before giving up on its own.
#: Only ever reached when the property under test has been broken — a `join()` where the
#: implementation should have returned — so it is the bound on how long a RED takes, not
#: on how long a green does. Finite so that a broken implementation FAILS rather than
#: hangs: a suite that stops is not a suite that answers.
BLOCKED = 10.0

#: How long a test waits for a signal it has already caused. Generous, because it bounds a
#: pass and never a fail.
SOON = 5.0

SECRET = "s3cr3t-value-no-action-may-see"


def _snapshot(**kw):
    return {"repos": [], "todos": [], **kw}


def _act(aid="demo", **kw):
    """An action with charter's own defaults, for a case that is about one field."""
    kw.setdefault("title", "Demo")
    kw.setdefault("run", lambda ctx: None)
    return action.Action(id=aid, **kw)


def _reachable(obj, depth: int = 8) -> list[str]:
    """Every string reachable from *obj* — through attributes, containers and closures.

    A guard that matches a spelling gets walked past, so this asks the property instead:
    is the value anywhere in what charter handed over, by any route the handed object
    could later be asked to follow. Closure cells are walked because a function that
    resolved a secret once and kept it is the obvious way a "spender" would leak one, and
    `__self__` because a bound method carries its whole receiver with it.
    """
    seen: set[int] = set()
    out: list[str] = []

    def walk(o, d):
        if d < 0 or o is None or id(o) in seen:
            return
        seen.add(id(o))
        if isinstance(o, str):
            out.append(o)
            return
        if isinstance(o, (bytes, bytearray)):
            out.append(bytes(o).decode("utf-8", "replace"))
            return
        if isinstance(o, Mapping):
            for k, v in list(o.items()):
                walk(k, d - 1)
                walk(v, d - 1)
            return
        if isinstance(o, (list, tuple, set, frozenset)):
            for item in list(o):
                walk(item, d - 1)
            return
        for cell in getattr(o, "__closure__", None) or ():
            try:
                walk(cell.cell_contents, d - 1)
            except ValueError:          # an empty cell, mid-definition
                pass
        for name in ("__self__", "__func__", "__wrapped__", "__dict__"):
            walk(getattr(o, name, None), d - 1)
        for name in getattr(type(o), "__slots__", ()) or ():
            walk(getattr(o, name, None), d - 1)

    walk(obj, depth)
    return out


def _answers(obj, *argsets) -> list:
    """Every public attribute of *obj*, and every answer it gives to *argsets*.

    Discovered with `dir`, never a literal list of names: the property is "nothing charter
    hands an action answers a vault value", and a literal list stops testing the day
    somebody adds a method to the object.
    """
    got = []
    for name in dir(obj):
        if name.startswith("_"):
            continue
        attr = getattr(obj, name)
        got.append(attr)
        if callable(attr):
            for args in argsets:
                try:
                    got.append(attr(*args))
                except Exception as exc:    # a wrong-arity guess is not a finding
                    got.append(str(exc))
    return got


class TheContract(unittest.TestCase):
    """An action declares an id, a title, availability with its reason, and what it does."""

    def test_an_action_declares_the_five_things_the_palette_needs(self):
        a = action.Action(id="clone", title="Clone a repo", run=lambda ctx: "started",
                          available=lambda ctx: True,
                          reason_unavailable=lambda ctx: "",
                          touches=("repos",))
        self.assertEqual((a.id, a.title, a.touches), ("clone", "Clone a repo", ("repos",)))
        self.assertTrue(a.available(None))
        self.assertEqual(a.reason_unavailable(None), "")
        self.assertEqual(a.run(None), "started")

    def test_an_action_is_available_and_silent_unless_it_says_otherwise(self):
        """The default is the common case, and it cannot be the ambiguous one."""
        a = _act()
        self.assertTrue(a.available(None))
        self.assertEqual(a.reason_unavailable(None), "")

    def test_an_id_that_could_reach_tmux_is_refused(self):
        """The same alphabet a component id is held to, and the same reason (#hotkey)."""
        for bad in ("Clone", "clone repo", "clone\n", "clone;kill-server", "#{x}",
                    "../clone", "", "a" * 40):
            with self.subTest(bad=bad):
                with self.assertRaises(action.ActionError):
                    _act(bad)

    def test_the_id_alphabet_is_the_component_one_rather_than_a_second_copy(self):
        """Two spellings of one alphabet is how two guards come to disagree."""
        from charter.frame import component
        self.assertTrue(component.usable_id("acme.deploy"))
        self.assertFalse(component.usable_id("acme.deploy\n"))
        self.assertEqual(_act("acme.deploy").id, "acme.deploy")

    def test_a_title_is_contained_rather_than_refused(self):
        """Display text: an open alphabet, closed to anything that can forge a row."""
        a = _act(title="Deploy\nrm -rf /")
        self.assertNotIn("\n", a.title)
        self.assertIn("\\x0a", a.title)

    def test_run_available_and_reason_must_be_callable(self):
        for field in ("run", "available", "reason_unavailable"):
            with self.subTest(field=field):
                with self.assertRaises(action.ActionError) as e:
                    _act(**{field: "not callable"})
                self.assertIn(field, str(e.exception))

    def test_an_unknown_touch_is_refused_and_names_what_charter_serves(self):
        with self.assertRaises(action.ActionError) as e:
            _act(touches=("gathr",))
        said = str(e.exception)
        self.assertIn("gathr", said)
        for served in action.TOUCHES:
            self.assertIn(served, said)

    def test_the_declared_vocabulary_is_exactly_what_is_served(self):
        """A name accepted here and served empty would be worse than a refusal — the
        action would declare it, do nothing, and be indistinguishable from a plane that
        genuinely has nothing. `component.NEEDS` is pinned to `ctx.SERVES` for the same
        reason, and this is that assertion for the other contract."""
        self.assertEqual(tuple(sorted(action.TOUCHES)), tuple(sorted(action.SERVES)))


class WhatAnActionIsHanded(PersonaIso):
    """Built FROM `touches`: absent, not disabled — the component rule, one contract over."""

    def test_a_ctx_carries_exactly_the_declaration_plus_the_identity(self):
        c = action.build(("repos",), fid="fr-1", snapshot=_snapshot())
        expected = {"fid", "repos"}
        self.assertEqual(set(vars(c)), expected)
        self.assertEqual({n for n in dir(c) if not n.startswith("_")}, expected)

    def test_an_undeclared_slice_is_absent_and_says_what_to_add(self):
        c = action.build((), fid="fr-1", snapshot=_snapshot())
        with self.assertRaises(AttributeError) as e:
            c.repos
        self.assertIn("touches", str(e.exception))

    def test_a_name_nothing_serves_is_told_apart_from_one_not_declared(self):
        c = action.build((), fid="fr-1", snapshot=_snapshot())
        with self.assertRaises(AttributeError) as e:
            c.subprocess
        said = str(e.exception)
        self.assertIn("subprocess", said)
        self.assertNotIn("touches to be handed it", said)

    def test_a_ctx_is_read_only(self):
        c = action.build(("repos",), fid="fr-1", snapshot=_snapshot())
        with self.assertRaises(AttributeError):
            c.repos = ()
        with self.assertRaises(AttributeError):
            del c.repos

    def test_the_slices_are_the_ones_a_component_gets_rather_than_a_second_reading(self):
        snap = _snapshot(repos=[{"name": "charter"}])
        c = action.build(("repos", "todos", "gather"), fid="fr-1", snapshot=snap)
        self.assertEqual(c.repos, ({"name": "charter"},))
        self.assertEqual(c.todos, ())
        self.assertEqual(dict(c.gather), snap)


class AnUnavailableActionCarriesWhy(PersonaIso):
    """§4h and #512's lesson: a row that explains beats a row that is silently absent."""

    def setUp(self):
        super().setUp()
        self.reg = actions.ActionRegistry()

    def _offers(self):
        return {o.id: o for o in self.reg.offers(fid="fr-1", snapshot=_snapshot())}

    def test_an_unavailable_action_is_listed_with_its_reason(self):
        self.reg.register(_act("switch", title="Switch workspace",
                               available=lambda ctx: False,
                               reason_unavailable=lambda ctx: "the session lock holds it"))
        offer = self._offers()["switch"]
        self.assertFalse(offer.available)
        self.assertEqual(offer.title, "Switch workspace")
        self.assertIn("session lock", offer.reason)

    def test_an_available_action_is_listed_without_one(self):
        self.reg.register(_act("go"))
        offer = self._offers()["go"]
        self.assertTrue(offer.available)
        self.assertEqual(offer.reason, "")

    def test_an_unavailable_action_that_says_nothing_still_says_something(self):
        """A provider that returns "" has not made the row honest — charter finishes it."""
        self.reg.register(_act("mute", available=lambda ctx: False,
                               reason_unavailable=lambda ctx: "   "))
        offer = self._offers()["mute"]
        self.assertFalse(offer.available)
        self.assertTrue(offer.reason.strip())
        self.assertIn("mute", offer.reason)

    def test_an_action_whose_availability_raises_is_unavailable_and_names_it(self):
        def boom(ctx):
            raise RuntimeError("no plane here")
        self.reg.register(_act("thrower", available=boom))
        offer = self._offers()["thrower"]
        self.assertFalse(offer.available)
        self.assertIn("RuntimeError", offer.reason)
        self.assertIn("no plane here", offer.reason)

    def test_an_action_whose_reason_raises_is_still_listed_with_a_reason(self):
        def boom(ctx):
            raise ValueError("and the reason broke too")
        self.reg.register(_act("thrower", available=lambda ctx: False,
                               reason_unavailable=boom))
        offer = self._offers()["thrower"]
        self.assertFalse(offer.available)
        self.assertIn("ValueError", offer.reason)

    def test_a_hostile_reason_renders_as_one_row(self):
        """A committed value reaching an overlay row is the `[frame] hotkey` class."""
        for hostile in ("two\nlines", "para graph", "esc\x1b[2Jape", "car\rriage"):
            with self.subTest(hostile=hostile):
                reg = actions.ActionRegistry()
                reg.register(_act("hostile", available=lambda ctx: False,
                                  reason_unavailable=lambda ctx, h=hostile: h))
                reason = reg.offers(fid="f", snapshot=_snapshot())[0].reason
                self.assertEqual(len(reason.splitlines()), 1)
                for ch in "\n\r \x1b":
                    self.assertNotIn(ch, reason)

    def test_each_action_is_asked_with_the_ctx_its_own_touches_declared(self):
        """The registry builds the ctx, so an action can never be handed more than it
        declared by a caller that did not read its declaration."""
        seen = {}
        self.reg.register(_act("narrow", touches=("todos",),
                               available=lambda ctx: seen.setdefault("narrow", ctx) or True))
        self.reg.register(_act("wide", touches=("repos", "todos"),
                                available=lambda ctx: seen.setdefault("wide", ctx) or True))
        self.reg.offers(fid="fr-1", snapshot=_snapshot())
        self.assertEqual(set(vars(seen["narrow"])), {"fid", "todos"})
        self.assertEqual(set(vars(seen["wide"])), {"fid", "todos", "repos"})

    def test_an_unavailable_action_is_not_run(self):
        ran = []
        self.reg.register(_act("locked", run=lambda ctx: ran.append(1),
                               available=lambda ctx: False,
                               reason_unavailable=lambda ctx: "the session lock holds it"))
        inv = self.reg.invoke("locked", fid="fr-1", snapshot=_snapshot())
        self.assertFalse(inv.started)
        self.assertIn("session lock", inv.reason)
        self.assertEqual(ran, [])

    def test_an_unknown_id_is_refused_rather_than_answered_with_a_quiet_nothing(self):
        with self.assertRaises(actions.ActionError) as e:
            self.reg.invoke("nope", fid="fr-1", snapshot=_snapshot())
        self.assertIn("nope", str(e.exception))


class TheCallerIsNeverBlocked(PersonaIso):
    """§4g: an action returns immediately having started work. Progress is `inflight`."""

    def setUp(self):
        super().setUp()
        self.reg = actions.ActionRegistry()

    def test_invoke_returns_while_the_action_is_still_inside_run(self):
        """The receipt reports the work as running at a moment `run` provably cannot have
        returned: it is blocked on a gate this test has not opened yet."""
        gate, entered = threading.Event(), threading.Event()
        self.addCleanup(gate.set)

        def slow(ctx):
            entered.set()
            gate.wait(timeout=BLOCKED)
            return "done"

        self.reg.register(_act("slow", run=slow))
        inv = self.reg.invoke("slow", fid="fr-1", snapshot=_snapshot())
        self.assertTrue(entered.wait(SOON), "the action never started")
        self.assertTrue(inv.started)
        self.assertTrue(inv.running, "invoke waited for the action to finish")
        gate.set()
        self.assertTrue(inv.join(SOON))
        self.assertFalse(inv.running)
        self.assertEqual(inv.note, "done")

    def test_work_in_flight_is_visible_while_it_runs_and_gone_after(self):
        """"Progress surfaces through `inflight`" — the tracker, not a second clock."""
        gate, entered = threading.Event(), threading.Event()
        self.addCleanup(gate.set)

        def slow(ctx):
            entered.set()
            gate.wait(timeout=BLOCKED)

        self.reg.register(_act("slow", run=slow))
        inv = self.reg.invoke("slow", fid="fr-1", snapshot=_snapshot())
        self.assertTrue(entered.wait(SOON))
        self.assertIn("slow", inflight.live(kind=inflight.ACTION))
        gate.set()
        self.assertTrue(inv.join(SOON))
        self.assertEqual(inflight.live(kind=inflight.ACTION), [])

    def test_an_action_does_not_reach_the_readers_that_did_not_ask_for_it(self):
        """`inflight`'s own rule: a new kind must not leak into the dispatch nudge."""
        gate, entered = threading.Event(), threading.Event()
        self.addCleanup(gate.set)
        self.reg.register(_act("slow", run=lambda ctx: (entered.set(),
                                                        gate.wait(timeout=BLOCKED))))
        inv = self.reg.invoke("slow", fid="fr-1", snapshot=_snapshot())
        self.assertTrue(entered.wait(SOON))
        self.assertEqual(inflight.live(), [])
        gate.set()
        inv.join(SOON)

    def test_an_action_that_raises_costs_its_own_row_and_names_what_it_raised(self):
        def boom(ctx):
            raise ZeroDivisionError("nothing left to divide")
        self.reg.register(_act("boom", run=boom))
        inv = self.reg.invoke("boom", fid="fr-1", snapshot=_snapshot())
        self.assertTrue(inv.join(SOON))
        self.assertTrue(inv.started)
        self.assertFalse(inv.ok)
        self.assertIn("ZeroDivisionError", inv.error)
        self.assertIn("nothing left to divide", inv.error)

    def test_a_failed_action_clears_its_in_flight_record(self):
        self.reg.register(_act("boom", run=lambda ctx: 1 / 0))
        self.assertTrue(self.reg.invoke("boom", fid="f", snapshot=_snapshot()).join(SOON))
        self.assertEqual(inflight.live(kind=inflight.ACTION), [])

    def test_a_note_a_provider_wrote_is_contained_before_it_can_reach_a_row(self):
        self.reg.register(_act("noisy", run=lambda ctx: "started\nrm -rf /"))
        inv = self.reg.invoke("noisy", fid="f", snapshot=_snapshot())
        self.assertTrue(inv.join(SOON))
        self.assertEqual(len(inv.note.splitlines()), 1)
        self.assertNotIn("\n", inv.note)


class NoRouteFromAnActionToAVaultValue(PersonaIso):
    """§4d: an action reaching a vault goes THROUGH the guards, never around them.

    The vault this registers is real and the value in it is real, so every assertion
    below is about what charter did with an actual secret rather than about what a stub
    was asked to pretend.
    """

    def setUp(self):
        super().setUp()
        vaults.add_vault("devops", "plain-file",
                         {"file": str(self.tmp / "devops.json")})
        vaults.provider_for("devops").set("deploy_token", SECRET)
        self.reg = actions.ActionRegistry()
        self.seen = []

    def _ctx(self, touches=("vault",)):
        self.reg.register(_act("probe", touches=touches,
                               run=lambda ctx: self.seen.append(ctx)))
        inv = self.reg.invoke("probe", fid="fr-1", snapshot=_snapshot())
        self.assertTrue(inv.join(SOON))
        self.assertEqual(inv.error, "")
        return self.seen[0]

    def test_the_value_is_really_there_to_be_found(self):
        """The control on the whole class: this file, read directly, holds the secret."""
        self.assertEqual(vaults.provider_for("devops").get("deploy_token"), SECRET)
        self.assertIn(SECRET, (self.tmp / "devops.json").read_text())

    def test_nothing_reachable_from_the_ctx_carries_the_value(self):
        c = self._ctx()
        found = _reachable(c)
        self.assertIn("devops", found, "the walk never reached the vault inventory")
        self.assertNotIn(SECRET, found)
        self.assertFalse([s for s in found if SECRET in s])

    def test_nothing_the_vault_object_answers_carries_the_value(self):
        c = self._ctx()
        answers = _answers(c.vault, (), ("devops",), ("devops", "deploy_token"))
        found = [s for a in answers for s in _reachable(a)]
        self.assertIn("devops", found, "the probe called nothing that answered")
        self.assertIn("deploy_token", found, "the key names were never reached")
        self.assertFalse([s for s in found if SECRET in s])

    def test_what_it_does_answer_is_the_names(self):
        """`touches = ("vault",)` is a real capability and not a no-op: an action can ask
        WHICH vaults exist and WHAT they are keyed by, which is what an action offering to
        spend one needs — and is the whole of what charter hands over."""
        c = self._ctx()
        self.assertEqual(c.vault.names(), ("devops",))
        self.assertEqual(c.vault.keys("devops"), ("deploy_token",))

    def test_an_action_that_did_not_declare_vault_has_no_vault_at_all(self):
        c = self._ctx(touches=())
        self.assertEqual(set(vars(c)), {"fid"})
        with self.assertRaises(AttributeError) as e:
            c.vault
        self.assertIn("touches", str(e.exception))

    def test_the_inventory_keeps_no_provider_between_calls(self):
        """A cached provider is how the value would arrive without anybody writing a
        method for it — the object would simply be holding the file it read."""
        c = self._ctx()
        c.vault.keys("devops")
        self.assertFalse([s for s in _reachable(c.vault) if SECRET in s])

    def test_an_unknown_vault_is_a_refusal_rather_than_an_empty_answer(self):
        c = self._ctx()
        with self.assertRaises(Exception) as e:
            c.vault.keys("no-such-vault")
        self.assertIn("no-such-vault", str(e.exception))


class TheRegistry(PersonaIso):
    """Registration, order, and the refusals that mirror the component registry's."""

    def setUp(self):
        super().setUp()
        self.reg = actions.ActionRegistry()

    def test_actions_are_listed_in_the_order_they_were_registered(self):
        for aid in ("c", "a", "b"):
            self.reg.register(_act(aid))
        self.assertEqual([a.id for a in self.reg.all()], ["c", "a", "b"])
        self.assertEqual([o.id for o in self.reg.offers(fid="f", snapshot=_snapshot())],
                         ["c", "a", "b"])

    def test_two_actions_claiming_one_id_refuses_and_names_both(self):
        self.reg.register(_act("deploy", title="Deploy the plane"))
        with self.assertRaises(actions.ActionError) as e:
            self.reg.register(_act("deploy", title="Deploy the docs"))
        said = str(e.exception)
        self.assertIn("Deploy the plane", said)
        self.assertIn("Deploy the docs", said)

    def test_a_collision_leaves_the_registry_as_it_was(self):
        self.reg.register(_act("deploy", title="Deploy the plane"))
        with self.assertRaises(actions.ActionError):
            self.reg.register(_act("deploy", title="Deploy the docs"))
        self.assertEqual(self.reg.get("deploy").title, "Deploy the plane")
        self.assertEqual(len(self.reg.all()), 1)

    def test_registering_something_that_is_not_an_action_is_refused(self):
        with self.assertRaises(actions.ActionError):
            self.reg.register("deploy")

    def test_an_unknown_id_names_itself(self):
        with self.assertRaises(actions.ActionError) as e:
            self.reg.get("ghost")
        self.assertIn("ghost", str(e.exception))

    def test_the_registries_do_not_share_state(self):
        self.reg.register(_act("solo"))
        self.assertEqual(actions.ActionRegistry().all(), ())


if __name__ == "__main__":
    unittest.main()
