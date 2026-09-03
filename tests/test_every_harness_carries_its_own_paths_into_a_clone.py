"""A clone gets EVERY harness's in-repo surface, in that harness's own spelling — #868.

#870 gave a guest checkout at `workspaces/<ws>/<repo>/` the plane's layer, and the list of
what to carry was two literals in `workspace.py`:

```python
WALKUP_DIRS = (".claude/agents", ".claude/skills")
```

#872 recorded that as a stated limit with the measured facts beside it, which is honest,
and an honest limit is still a limit: an operator on opencode or Codex got a workspace with
none of the plane's agents or skills. The three surfaces, each measured against the
installed binary rather than its documentation:

| harness      | in-repo surface                      | binary   |
|--------------|--------------------------------------|----------|
| Claude Code  | `.claude/agents`, `.claude/skills`    | 2.1.259  |
| opencode     | `.opencode/agent`, `opencode.json`    | 1.18.23  |
| Codex        | `.codex/skills`                       | 0.147.0  |

So the spelling moved onto the harness, beside `Harness.layer` and `layer_note`, and
`workspace._inherited_files` asks the registry. Two things that makes true are what this
file is about — `workspace.py` names no harness and no harness path of its own, and a
surface a harness declares reaches a real clone — plus the invariants that keep the new
member from being used for the wrong thing: it is not where a harness's GENERATED files
go, and it is not where the plane's project instructions go.

Every case writes only into a `PersonaIso` tmp plane, and the fixture asserts that before
it writes anything. Nothing here touches the developer's real `workspaces/`.
"""

from __future__ import annotations

import json
import unittest
from unittest import mock

from charter import config, workspace
from charter.harness import base, claude_code, codex, opencode, registry

from tests.test_a_clone_gets_the_layer_and_hides_it import CloneLayer
from tests.test_doctor_says_which_of_charters_layer_reaches_here import _code


def _harness(name: str, *paths: str) -> base.Harness:
    """A registered-looking harness that declares *paths* and generates nothing.

    A real :class:`base.Harness` subclass rather than a `SimpleNamespace`, because these
    are handed to `registry.all()`'s callers and every one of them asks a harness more than
    one question. A stub that answers only the question a test happens to ask is how a
    fake drifts from the interface it stands for.
    """
    return type("Fake", (base.Harness,), {"name": name, "inherited_paths": paths})()


class TheSpellingIsTheHarnesses(unittest.TestCase):
    """Properties of the declarations, so a harness cannot be registered with a surface
    nothing carries — or with one carrying something it must not."""

    def test_workspace_names_no_harness_and_no_harness_path_of_its_own(self):
        """The pin that keeps the spelling on the harness, and the same pin
        `check_session_layer` already carries one module over: reporting a harness under
        another's rules and WRITING another harness's files are the same mistake, and only
        the second one lands on disk.

        Docstrings and comments are stripped, so the prose above `_inherited_files` may go
        on naming what was measured. A *literal* is the thing refused.
        """
        for fn in (workspace._inherited_files, workspace._guest_files):
            code = _code(fn)
            for literal in (".claude", "claude-code", "opencode", "codex", "CLAUDE.md"):
                self.assertNotIn(literal, code,
                                 f"`{fn.__name__}` names {literal!r} itself; that fact "
                                 f"belongs on the harness")

    def test_every_shipped_harness_declares_what_a_clone_cuts_it_off_from(self):
        """All three were measured to read something from a project. A harness left at the
        default here is one whose operator gets a workspace with none of the plane's
        agents or skills, which is the whole of #868."""
        for h in registry.all():
            self.assertTrue(h.inherited_paths,
                            f"{h.name} declares no in-repo surface, so a clone carries "
                            f"nothing for it")

    def test_each_harness_declares_the_surface_that_was_measured(self):
        """Spelled out per harness rather than compared to the registry's own answer: a
        test that asked the registry what it holds would agree with itself no matter which
        harness had quietly stopped declaring anything."""
        self.assertEqual(claude_code.ClaudeCodeHarness().inherited_paths,
                         (".claude/agents", ".claude/skills"))
        self.assertEqual(opencode.OpenCodeHarness().inherited_paths,
                         (".opencode/agent", "opencode.json"))
        self.assertEqual(codex.CodexHarness().inherited_paths, (".codex/skills",))

    def test_the_base_class_declares_nothing(self):
        """A harness registered tomorrow inherits silence, not Claude Code's paths — the
        restraint `layer`, `layer_note` and `trust_gate` already keep."""
        self.assertEqual(base.Harness().inherited_paths, ())

    def test_no_harness_carries_the_planes_project_instructions(self):
        """The line `Harness.inherited_paths` draws. `CLAUDE.md` walks up on the same rule
        as `.claude/agents/`, so the gap is real for it too — and it is the one file a repo
        of its own is most likely to have opinions about. Dropping the plane's instructions
        into somebody else's repo, to be read there as that repo's, is a claim of a
        different size from mirroring a settings key."""
        for h in registry.all():
            for p in h.inherited_paths:
                self.assertNotIn(p.rsplit("/", 1)[-1],
                                 ("CLAUDE.md", "AGENTS.md", "AGENT.md", ".cursorrules"),
                                 f"{h.name} would narrate the host's repo with {p}")

    def test_no_harness_mirrors_a_file_it_also_generates(self):
        """The two questions `_guest_files` asks must not overlap. `.claude/settings.json`
        is GENERATED for a checkout — the plane's three keys, composed — and a harness that
        also listed `.claude` here would mirror the plane's whole settings file over the
        generated one, silently putting the plane's `permissions` in force in a repo
        charter is a guest in."""
        for h in registry.all():
            for gen in h.workspace_files():
                for got in h.inherited_paths:
                    self.assertFalse(gen == got or gen.startswith(f"{got}/"),
                                     f"{h.name} both generates and mirrors {gen}")


class TheRegistryAnswersForAllOfThem(unittest.TestCase):
    """`registry.inherited_paths()` — the one place `workspace.py` asks."""

    def test_every_harnesss_surface_is_in_the_answer(self):
        got = registry.inherited_paths()
        for p in (".claude/agents", ".claude/skills", ".opencode/agent", "opencode.json",
                  ".codex/skills"):
            self.assertIn(p, got)

    def test_a_path_two_harnesses_name_is_carried_once(self):
        """Two harnesses can agree on a directory — nothing stops a future one adopting
        `.claude/skills`. Carried twice it is two keys for one file, and the second pass
        would overwrite the first's marker entry with an identical digest for a path the
        exclude block then lists twice."""
        with mock.patch.object(registry, "all", return_value=[
                _harness("a", ".shared/skills", ".a/agents"),
                _harness("b", ".shared/skills", ".b/agents")]):
            self.assertEqual(registry.inherited_paths(),
                             (".shared/skills", ".a/agents", ".b/agents"))

    def test_the_order_is_registration_order_and_not_a_hash(self):
        """Two opposite registrations of the SAME pair, because one assertion would pass
        about half the time by luck: `sorted` over a set, or a set at all, orders by hash,
        and a guarantee that holds by coincidence is one nothing can pin. Order is what
        decides which harness's copy wins when two name one path and the plane holds a file
        at it, so it is a real answer rather than cosmetics."""
        first, second = _harness("a", ".a/x"), _harness("b", ".b/x")
        with mock.patch.object(registry, "all", return_value=[first, second]):
            self.assertEqual(registry.inherited_paths(), (".a/x", ".b/x"))
        with mock.patch.object(registry, "all", return_value=[second, first]):
            self.assertEqual(registry.inherited_paths(), (".b/x", ".a/x"))

    def test_a_harness_that_declares_nothing_removes_nobody_elses(self):
        with mock.patch.object(registry, "all",
                               return_value=[_harness("quiet"), _harness("loud", ".l/x")]):
            self.assertEqual(registry.inherited_paths(), (".l/x",))


class EveryHarnessesSurfaceReachesTheClone(CloneLayer):
    """The behaviour, in a real clone inside a real workspace."""

    def plane(self, rel: str, text: str) -> None:
        p = config.ROOT / rel
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(text)

    def test_a_codex_skill_reaches_the_clone(self):
        """Measured against codex-cli 0.147.0 with a real `codex exec` session: a sentinel
        at `<repo>/.codex/skills/<name>/SKILL.md` reaches the model's context with zero
        tool calls, where a control repository answers NONE. So this is the file that
        decides whether a Codex chat in a clone has the plane's skills."""
        self.plane(".codex/skills/deploy/SKILL.md", "# deploy\n")
        self.wire()
        self.assertEqual((self.clone / ".codex/skills/deploy/SKILL.md").read_text(),
                         "# deploy\n")

    def test_an_opencode_project_agent_reaches_the_clone(self):
        """Measured against opencode 1.18.23: a sentinel at
        `<repo>/.opencode/agent/probe.md` is a project agent that a control repository does
        not have."""
        self.plane(".opencode/agent/steward.md", "---\nname: steward\n---\nroute it\n")
        self.wire()
        self.assertEqual((self.clone / ".opencode/agent/steward.md").read_text(),
                         "---\nname: steward\n---\nroute it\n")

    def test_the_opencode_project_config_reaches_the_clone_as_ONE_file(self):
        """The path that is a FILE and not a directory, which is why the mirror reads the
        declared path itself before it reads anything under it. `rglob` on a file yields
        nothing, so before #868 this would have been carried as nothing at all — and a
        mirror that used the relative path unconditionally would have written it to the key
        `opencode.json/.`, a path that names nothing and hides nothing.

        It is load-bearing rather than an example: malformed JSON at `<repo>/opencode.json`
        fails the run OUTRIGHT (`Error: Config file at <repo>/opencode.json is not valid
        JSON(C)`), so this is the file that proves opencode reads a project at all."""
        self.plane("opencode.json", '{"instructions": ["AGENTS.md"]}\n')
        self.wire()
        self.assertEqual((self.clone / "opencode.json").read_text(),
                         '{"instructions": ["AGENTS.md"]}\n')
        self.assertFalse((self.clone / "opencode.json.").exists())

    def test_the_three_surfaces_arrive_together(self):
        """One `wire`, not one per harness. A workspace is not a harness's workspace."""
        self.plane(".claude/agents/steward.md", "cc\n")
        self.plane(".opencode/agent/steward.md", "oc\n")
        self.plane(".codex/skills/s/SKILL.md", "cx\n")
        rows = self.wire()
        for rel in (".claude/agents/steward.md", ".opencode/agent/steward.md",
                    ".codex/skills/s/SKILL.md"):
            self.assertEqual(rows[rel], "created", rel)

    def test_the_workspace_directory_still_gets_none_of_them(self):
        """The boundary did not move. Every one of these paths is resolved from inside the
        plane's own repository — by a walk that stops at the git root, or by resolving the
        repository root itself — so a workspace directory already reads the plane's copies
        and a second copy would shadow the first."""
        self.plane(".opencode/agent/steward.md", "oc\n")
        self.plane(".codex/skills/s/SKILL.md", "cx\n")
        self.plane("opencode.json", "{}\n")
        workspace.wire_harnesses(self.ws)
        wd = workspace.workspace_dir(self.ws)
        for unwanted in (".opencode", ".codex", "opencode.json"):
            self.assertFalse((wd / unwanted).exists(), f"{unwanted} should not be written")

    def test_the_plane_charter_file_is_still_left_behind(self):
        """Carried in the same pass as everything else if any harness had declared it.
        None does, and this is where that stays true."""
        self.plane("CLAUDE.md", "the plane's instructions\n")
        self.plane("AGENTS.md", "also the plane's\n")
        self.wire()
        self.assertFalse((self.clone / "CLAUDE.md").exists())
        self.assertFalse((self.clone / "AGENTS.md").exists())


class TheGuestContractHoldsForAllOfThem(CloneLayer):
    """The four restraints #870 bought, asked of the paths #868 added. A mechanism that
    held only for `.claude/` would be a mechanism that never held."""

    def plane(self, rel: str, text: str) -> None:
        p = config.ROOT / rel
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(text)

    def seed(self) -> None:
        self.plane(".opencode/agent/steward.md", "oc\n")
        self.plane(".codex/skills/s/SKILL.md", "cx\n")
        self.plane("opencode.json", "{}\n")

    def test_the_exact_paths_are_hidden_never_a_directory(self):
        """A guest repo may have its own `.codex/` and its own `opencode.json`. Hiding a
        directory would hide the operator's files inside it from their own `git status`,
        which is a worse failure than the noise this prevents."""
        self.seed()
        self.wire()
        lines = self.excludes().splitlines()
        for rel in ("/.opencode/agent/steward.md", "/.codex/skills/s/SKILL.md",
                    "/opencode.json"):
            self.assertIn(rel, lines)
        for glob in ("/.opencode/", "/.codex/", "/.opencode", "/.codex"):
            self.assertNotIn(glob, lines)

    def test_the_status_stays_clean_and_nothing_can_be_staged(self):
        """Against real `git`, not against charter's model of it."""
        self.seed()
        self.wire()
        self.assertEqual(self.status(), "")

    def test_re_wiring_does_not_duplicate_the_block(self):
        """An `ensure` runs on every launch. Appending would leave `git status` clean while
        `info/exclude` grew without bound — the failure nobody would ever look for."""
        self.seed()
        self.wire()
        self.wire()
        text = self.excludes()
        self.assertEqual(text.count("/opencode.json"), 1)
        self.assertEqual(text.count("# <<< charter <<<"), 1)

    def test_a_file_the_operator_already_owns_is_never_overwritten(self):
        """The guest's own `opencode.json` is the sharpest case in the set: it is a file at
        the repository root that a repo is quite likely to have, and overwriting it would
        change how their opencode runs."""
        self.seed()
        (self.clone / "opencode.json").write_text('{"theirs": true}\n')
        rows = self.wire()
        self.assertEqual(rows["opencode.json"], "foreign")
        self.assertEqual((self.clone / "opencode.json").read_text(), '{"theirs": true}\n')
        # And not hidden either — the quieter half of the same restraint.
        self.assertNotIn("/opencode.json", self.excludes().splitlines())

    def test_the_marker_vouches_for_every_one_of_them(self):
        """`doctor` tells `stale` from `foreign` by this file and nothing else."""
        self.seed()
        self.wire()
        marker = json.loads((self.clone / workspace.GENERATED_MARKER).read_text())
        for rel in (".opencode/agent/steward.md", ".codex/skills/s/SKILL.md",
                    "opencode.json"):
            self.assertEqual(marker[rel],
                             workspace.content_digest((self.clone / rel).read_text()))

    def test_a_stale_copy_is_refreshed_and_a_taken_over_one_is_not(self):
        """Refresh, not create-once — a copy an older charter wrote must not survive every
        upgrade while `doctor` reports the checkout wired (ADR 0015). The distinction is
        the marker's, and it has to hold for the paths #868 added or those two harnesses
        get the create-once bug the `.claude/` ones already lost."""
        self.seed()
        self.wire()
        self.plane("opencode.json", '{"moved": true}\n')
        self.plane(".codex/skills/s/SKILL.md", "cx moved\n")
        # The operator's now. Charter cannot vouch for it, so it never rewrites it — even
        # though the plane's copy moved in exactly the same way.
        (self.clone / ".codex/skills/s/SKILL.md").write_text("theirs\n")
        rows = self.wire()
        self.assertEqual(rows["opencode.json"], "refreshed")
        self.assertEqual((self.clone / "opencode.json").read_text(), '{"moved": true}\n')
        self.assertEqual(rows[".codex/skills/s/SKILL.md"], "foreign")
        self.assertEqual((self.clone / ".codex/skills/s/SKILL.md").read_text(), "theirs\n")

    def test_unwiring_takes_them_back_out_and_leaves_no_empty_directories(self):
        """`workspace remove` unwires before the `rmtree`, because a linked worktree's
        exclude lives in a main repo outside the directory. A `.codex/skills/s/` left
        standing after its last generated file is charter still visible in a repo it no
        longer has anything in."""
        self.seed()
        self.wire()
        removed = workspace.unwire_guest(self.clone)
        for rel in (".opencode/agent/steward.md", ".codex/skills/s/SKILL.md",
                    "opencode.json"):
            self.assertIn(rel, removed)
            self.assertFalse((self.clone / rel).exists())
        self.assertFalse((self.clone / ".codex").exists())
        self.assertFalse((self.clone / ".opencode").exists())
        # git's own commented header stays; charter's block is what goes. Asserting the
        # file is EMPTY would pass for a charter that had truncated somebody else's file.
        left = self.excludes()
        self.assertNotIn("charter", left)
        self.assertNotIn("opencode.json", left)

    def test_a_binary_in_one_surface_does_not_empty_the_others(self):
        """One unreadable file must not take the layer down with it — the restraint
        `_inherited_files` records, now across three harnesses rather than one."""
        self.seed()
        (config.ROOT / ".codex/skills/s/logo.png").write_bytes(b"\x89PNG\r\n\x1a\n\xff\xfe")
        self.wire()
        self.assertFalse((self.clone / ".codex/skills/s/logo.png").exists())
        self.assertTrue((self.clone / ".opencode/agent/steward.md").is_file())
        self.assertTrue((self.clone / "opencode.json").is_file())


if __name__ == "__main__":
    unittest.main()
