""""Upgrade it for the newest hooks" did not tell anyone how to upgrade it.

`check_plugin_skew` reports an older plugin as OK-with-a-note, which is right — an older
plugin wires fewer hooks, and that is benign in a way a newer one is not. What it did not
do is say what to type. Following the note the obvious way fails twice over:

    $ claude plugin update charter@charter
    ✘ Failed to update plugin "charter@charter": Plugin "charter" is not installed at
      scope user

— because `update` defaults to `user` scope and the plugin is usually installed at
`project` scope; and because the marketplace is a git clone that advertises whatever
version it last fetched, so without `marketplace update` first the command finds the
installed version already current and correctly does nothing. A no-op that reports
success-shaped output is how a plugin sat two minor versions behind while `doctor` was run
repeatedly.

Both names are read from the manifests charter is already given — the installed plugin
directory carries `plugin.json` AND `marketplace.json` — so the id is exact rather than a
placeholder, with no dependency on the shape of Claude Code's cache path.

The assertions run against `Result.render()`, not against `.hint`. `render()` suppresses
the hint field entirely when the status is OK, so a fix written into `hint` would have
rendered nothing at all while looking done — the failure ADR 0013 exists to name.
"""

from __future__ import annotations

import json
import unittest.mock
import unittest
from pathlib import Path

from charter import doctor
from tests._isolation import PersonaIso


class PluginHintBase(PersonaIso):
    def _install(self, version: str, plugin="charter", marketplace="charter",
                 with_marketplace=True):
        root = self.tmp / "plugin-root"
        (root / ".claude-plugin").mkdir(parents=True, exist_ok=True)
        (root / ".claude-plugin" / "plugin.json").write_text(
            json.dumps({"name": plugin, "version": version}))
        if with_marketplace:
            (root / ".claude-plugin" / "marketplace.json").write_text(
                json.dumps({"name": marketplace}))
        self.enterContext(unittest.mock.patch.dict(
            "os.environ", {"CLAUDE_PLUGIN_ROOT": str(root)}))
        return root


class TestAnOlderPluginIsToldHowToUpgrade(PluginHintBase):
    def _rendered(self, version="0.25.0", **kw):
        self._install(version, **kw)
        with unittest.mock.patch("charter.hooks.MIN_PLUGIN_VERSION", "0.28.0"):
            return doctor.check_plugin_skew().render()

    def test_it_names_the_marketplace_step(self):
        """The step whose absence makes the next line a silent no-op."""
        self.assertIn("marketplace update", self._rendered())

    def test_it_names_the_update_step_with_the_exact_plugin_id(self):
        out = self._rendered()
        self.assertIn("plugin update charter@charter", out)

    def test_it_names_the_scope(self):
        """`update` defaults to user scope; the plugin is usually installed per project."""
        self.assertIn("--scope", self._rendered())

    def test_the_marketplace_step_comes_first(self):
        out = self._rendered()
        self.assertLess(out.index("marketplace update"), out.index("plugin update"))

    def test_it_still_reports_ok(self):
        """An older plugin is supported. This changes the advice, not the severity."""
        self._install("0.25.0")
        with unittest.mock.patch("charter.hooks.MIN_PLUGIN_VERSION", "0.28.0"):
            self.assertEqual(doctor.check_plugin_skew().status, doctor.OK)

    def test_the_advice_survives_rendering(self):
        """render() drops `hint` when the status is OK, so advice written there would be
        invisible — shipped-looking and doing nothing."""
        self._install("0.25.0")
        with unittest.mock.patch("charter.hooks.MIN_PLUGIN_VERSION", "0.28.0"):
            r = doctor.check_plugin_skew()
        self.assertIn("marketplace update", r.render())

    def test_a_matching_plugin_says_none_of_it(self):
        self._install("0.28.0")
        with unittest.mock.patch("charter.hooks.MIN_PLUGIN_VERSION", "0.28.0"):
            out = doctor.check_plugin_skew().render()
        self.assertNotIn("marketplace update", out)

    def test_a_missing_marketplace_manifest_does_not_crash(self):
        """Read defensively: the id is a convenience, not a guarantee."""
        out = self._rendered(with_marketplace=False)
        self.assertIn("marketplace update", out)

    def test_it_uses_the_names_it_was_given(self):
        out = self._rendered(plugin="other", marketplace="mkt")
        self.assertIn("other@mkt", out)


if __name__ == "__main__":
    unittest.main()
