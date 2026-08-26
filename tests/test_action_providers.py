"""The second seam a stranger's code arrives through: `charter.actions`.

§4d gives actions their own entry-point group and a stricter contract than components: "a
provider that adds a CI panel but cannot add *rerun failed job* is half a plugin", and an
action *does* rather than draws. The four refusals are the component registry's, which is
why `ActionProviders` subclasses `Providers` rather than repeating it — and why the cases
below assert them again from the outside: a shared implementation is only safe while
something proves both contracts still get the answer.

**Every distribution here is real**, on the same terms as `test_component_providers`:
`importlib.metadata` is never stubbed, and the fixture that writes a `.dist-info` is that
file's, imported rather than copied.

**A failed action is a ROW that says why, not a missing row.** That is §4b property 4 for
the command surface — the surface the component registry was waiting for. A palette that
silently omits an option is worse than one that explains (#512), and it is worse in a way
the operator cannot even ask about.
"""

from __future__ import annotations

import unittest

from charter.frame import action, actions, component
from tests._isolation import PersonaIso
from tests.test_component_providers import _SitePackages

AID = "acme.deploy"
MODULE = "acme_deploy"
ENTRY = f"{MODULE}:deploy"
GROUP = "charter.actions"


#: "say nothing about the version and let the fixture pick charter's own" — an object
#: rather than ``None``, because ``None`` is one of the values a case below installs AS the
#: declared version, and a sentinel that is also a fixture value is a test that quietly
#: stops testing. `test_component_providers` learned this first and says so at length.
_CHARTERS = object()


def _source(*, aid: str = AID, api: object = _CHARTERS,
            run: str = "lambda ctx: 'started'",
            available: str = "lambda ctx: True",
            reason: str = "lambda ctx: ''", head: str = "") -> str:
    """A provider module declaring one action, and a record of whether it built it."""
    api = action.API_VERSION if api is _CHARTERS else api
    return (
        "from charter.frame import action\n"
        f"{head}\n"
        f"ACTION_API_VERSION = {api!r}\n"
        "built = False\n"
        "\n"
        "\n"
        "def deploy():\n"
        "    global built\n"
        "    built = True\n"
        "    return action.Action(\n"
        f"        id={aid!r}, title='Deploy', run={run},\n"
        f"        available={available}, reason_unavailable={reason})\n"
    )


class TheGroupIsItsOwn(PersonaIso):
    """§4d: `charter.components` and `charter.actions`, separate. Not one group with a tag."""

    def setUp(self):
        super().setUp()
        self.site = _SitePackages(self)

    def test_an_installed_action_provider_is_listed(self):
        self.site.install("acme-charter", "1.4.0", {AID: ENTRY}, {MODULE: _source()},
                          group=GROUP)
        self.assertIn(AID, actions.ActionProviders().ids())

    def test_a_component_provider_is_not_an_action_provider(self):
        """The whole point of two groups: a component cannot be invoked, and an action
        cannot be drawn, so an entry point in the wrong group must not be found by the
        other contract."""
        self.site.install("acme-charter", "1.4.0", {"acme.metrics": "acme_metrics:m"},
                          {"acme_metrics": "API_VERSION = 1\nm = None\n"})
        self.assertNotIn("acme.metrics", actions.ActionProviders().ids())

    def test_an_action_provider_is_not_a_component_provider(self):
        from charter.frame import registry
        self.site.install("acme-charter", "1.4.0", {AID: ENTRY}, {MODULE: _source()},
                          group=GROUP)
        self.assertNotIn(AID, registry.Providers().ids())

    def test_listing_does_not_import_the_provider(self):
        self.site.install("boom-charter", "2.0", {"boom.deploy": "boom_deploy:deploy"},
                          {"boom_deploy": "raise RuntimeError('imported at discovery')\n"},
                          group=GROUP)
        providers = actions.ActionProviders()
        self.assertIn("boom.deploy", providers.ids())
        self.assertFalse(self.site.imported("boom_deploy"))

    def test_adding_it_imports_it_and_offers_it(self):
        self.site.install("acme-charter", "1.4.0", {AID: ENTRY}, {MODULE: _source()},
                          group=GROUP)
        reg = actions.ActionRegistry()
        added = reg.add(AID)
        self.assertTrue(self.site.imported(MODULE))
        self.assertEqual(added.id, AID)
        offer = reg.offers(fid="fr-1", snapshot={})[0]
        self.assertEqual((offer.id, offer.title, offer.available), (AID, "Deploy", True))
        self.assertEqual(reg.failures, {})

    def test_a_provider_may_name_the_action_itself_rather_than_a_factory(self):
        self.site.install("acme-charter", "1.4.0", {AID: f"{MODULE}:widget"},
                          {MODULE: _source() + "\nwidget = deploy()\n"}, group=GROUP)
        self.assertEqual(actions.ActionRegistry().add(AID).id, AID)

    def test_a_providers_action_actually_runs(self):
        self.site.install("acme-charter", "1.4.0", {AID: ENTRY},
                          {MODULE: _source(run="lambda ctx: 'deploying 3 repos'")},
                          group=GROUP)
        reg = actions.ActionRegistry()
        reg.add(AID)
        inv = reg.invoke(AID, fid="fr-1", snapshot={})
        self.assertTrue(inv.join(5.0))
        self.assertEqual(inv.note, "deploying 3 repos")
        self.assertEqual(inv.error, "")


class TheApiVersionIsItsOwnInteger(PersonaIso):
    """§4g's refusal, on the action contract's own integer.

    Separate from the component one because a single module may legitimately supply both,
    and one attribute cannot mean two contracts whose versions move independently — a
    component bump would otherwise refuse every action on the machine for a contract it
    did not touch.
    """

    def setUp(self):
        super().setUp()
        self.site = _SitePackages(self)
        self.reg = actions.ActionRegistry()
        self.other = action.API_VERSION + 6

    def _install(self, api):
        self.site.install("acme-charter", "9.9.9", {AID: ENTRY},
                          {MODULE: _source(api=api)}, group=GROUP)

    def test_a_provider_speaking_another_version_is_a_row_that_says_so(self):
        self._install(self.other)
        self.reg.add(AID)
        offer = self.reg.offers(fid="fr-1", snapshot={})[0]
        self.assertFalse(offer.available)
        self.assertIn("acme-charter 9.9.9", offer.reason)
        self.assertIn(f"version {self.other}", offer.reason)
        self.assertIn(f"charter speaks {action.API_VERSION}", offer.reason)

    def test_it_is_refused_before_the_provider_builds_anything(self):
        self._install(self.other)
        self.reg.add(AID)
        import sys
        self.assertFalse(getattr(sys.modules[MODULE], "built"))

    def test_a_version_that_is_not_one_integer_is_refused(self):
        """``"1"`` and ``True`` are the two a permissive check lets through: ``True == 1``
        in Python, so a bare equality would load a provider declaring a boolean."""
        for api in ("1", 1.0, True, None):
            with self.subTest(api=api):
                self._install(api)          # into the one site, replacing what was there
                reg = actions.ActionRegistry()
                reg.add(AID)
                self.assertIn("ACTION_API_VERSION", reg.failures[AID])

    def test_the_component_integer_is_not_the_one_that_is_read(self):
        """A module declaring only `API_VERSION` has said nothing about this contract."""
        self.site.install("acme-charter", "9.9.9", {AID: ENTRY},
                          {MODULE: _source().replace("ACTION_API_VERSION",
                                                     "API_VERSION")}, group=GROUP)
        self.reg.add(AID)
        self.assertIn("ACTION_API_VERSION", self.reg.failures[AID])


class TwoProvidersClaimingOneIdLoadNeither(PersonaIso):
    """§4h, on the action group: a row whose origin cannot be determined is worse."""

    def setUp(self):
        super().setUp()
        self.site = _SitePackages(self)
        self.reg = actions.ActionRegistry()
        self.site.install("acme-charter", "1.4.0", {AID: ENTRY}, {MODULE: _source()},
                          group=GROUP)
        self.site.install("other-charter", "0.2", {AID: "other_deploy:deploy"},
                          {"other_deploy": _source()}, group=GROUP)

    def test_neither_loads_and_both_are_named(self):
        self.reg.add(AID)
        said = self.reg.failures[AID]
        self.assertIn("acme-charter 1.4.0", said)
        self.assertIn("other-charter 0.2", said)
        self.assertIn("NEITHER", said)

    def test_the_rest_of_the_palette_is_offered(self):
        self.reg.register(action.Action(id="refresh", title="Refresh",
                                        run=lambda ctx: None))
        self.reg.add(AID)
        offered = {o.id: o for o in self.reg.offers(fid="fr-1", snapshot={})}
        self.assertTrue(offered["refresh"].available)
        self.assertFalse(offered[AID].available)
        self.assertIn("acme-charter", offered[AID].reason)


class AFailedProviderIsARowNotASilence(PersonaIso):
    """A missing or broken provider becomes a permanently-unavailable offer WITH the why.

    This is the surface `registry.STANDIN_EDGE`'s note was waiting for: in Phase 1 an
    arrangement charter could not honour had nowhere to say so, so it was refused whole.
    An action has somewhere — the row itself.
    """

    def setUp(self):
        super().setUp()
        self.site = _SitePackages(self)
        self.reg = actions.ActionRegistry()

    def _reason(self, aid=AID):
        self.reg.add(aid)
        return {o.id: o for o in self.reg.offers(fid="fr-1", snapshot={})}[aid].reason

    def test_a_missing_provider_is_a_row_that_says_it_is_missing(self):
        said = self._reason()
        self.assertIn(AID, said)
        self.assertIn("no installed provider", said)
        self.assertIn(GROUP, said)

    def test_an_import_that_raises_is_a_row_that_names_it(self):
        self.site.install("acme-charter", "1.4.0", {AID: ENTRY},
                          {MODULE: "raise RuntimeError('no such database')\n"},
                          group=GROUP)
        said = self._reason()
        self.assertIn("RuntimeError", said)
        self.assertIn("no such database", said)

    def test_building_that_raises_is_a_row_that_names_it(self):
        self.site.install("acme-charter", "1.4.0", {AID: ENTRY},
                          {MODULE: _source(head="raise_it = 1") .replace(
                              "    return action.Action(",
                              "    raise ValueError('bad config')\n    return action.Action(")},
                          group=GROUP)
        said = self._reason()
        self.assertIn("ValueError", said)
        self.assertIn("bad config", said)

    def test_an_entry_point_answering_a_component_is_refused(self):
        """The two contracts are not interchangeable, and the refusal says which is wanted."""
        self.site.install("acme-charter", "1.4.0", {AID: "acme_mixed:thing"},
                          {"acme_mixed": (
                              "from charter.frame import component\n"
                              f"ACTION_API_VERSION = {action.API_VERSION}\n"
                              "thing = component.Component(\n"
                              f"    id={AID!r}, title='Metrics', edge='right',\n"
                              "    size=component.Fixed(12), render=lambda ctx: [])\n")},
                          group=GROUP)
        said = self._reason()
        self.assertIn("frame action", said)

    def test_an_entry_point_name_that_is_not_the_actions_id_is_refused(self):
        self.site.install("acme-charter", "1.4.0", {AID: ENTRY},
                          {MODULE: _source(aid="acme.other")}, group=GROUP)
        said = self._reason()
        self.assertIn("acme.other", said)
        self.assertIn(AID, said)

    def test_a_bare_name_no_builtin_claims_says_what_a_provider_id_looks_like(self):
        said = self._reason("deployy")
        self.assertIn("namespaced", said)

    def test_a_failed_action_refuses_to_run_rather_than_running_something_else(self):
        self.reg.add(AID)
        inv = self.reg.invoke(AID, fid="fr-1", snapshot={})
        self.assertFalse(inv.started)
        self.assertIn("no installed provider", inv.reason)

    def test_adding_something_already_registered_answers_what_is_registered(self):
        first = self.reg.register(action.Action(id="refresh", title="Refresh",
                                                run=lambda ctx: None))
        self.assertIs(self.reg.add("refresh"), first)


class TheProviderContractIsOneImplementation(unittest.TestCase):
    """`ActionProviders` mirrors `Providers` by SUBCLASSING it — #547's lesson.

    A copy would drift, and the drift would be a refusal that stopped holding on one of
    the two contracts with nothing failing. What differs is data, and this asserts that
    the data is what differs.
    """

    def test_it_is_the_component_loader_with_this_contracts_answers(self):
        from charter.frame import registry
        self.assertTrue(issubclass(actions.ActionProviders, registry.Providers))
        p = actions.ActionProviders()
        self.assertEqual(p.group, GROUP)
        self.assertEqual(p.version_attr, "ACTION_API_VERSION")
        self.assertEqual(p.speaks, action.API_VERSION)
        self.assertIs(p.kind, action.Action)
        self.assertIs(p.error, action.ActionError)

    def test_the_two_groups_are_different_strings(self):
        from charter.frame import registry
        self.assertNotEqual(actions.ACTION_GROUP, registry.PROVIDER_GROUP)


if __name__ == "__main__":
    unittest.main()
