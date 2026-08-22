"""Four load-bearing guards, written down as decisions rather than left as behaviour.

#340, from the authority audit of 0.47.2. These four surfaces were swept and came back
**clean** — and in each case the reason they are clean is a deliberate choice that exists
only in the code's behaviour. Each has a comment saying why. A comment survives a refactor
only if the person doing the refactor reads it, and every failure here is silent: the
guard's absence looks exactly like the guard being present and never firing.

So each test below is named after the DECISION, not the mechanism, and asserts the
consequence the decision exists to prevent. If someone simplifies one of these away, the
test that goes red says what they took.

**1. `registry.resolve_host` fails closed.** A URL crafted to LOOK like a managed forge —
`https://user@github.com@evil.example/…`, `git@github.com.evil.example:…` — resolves to
unmanaged (`None`), never to github.com or to a declared self-hosted host. `resolve_host`
is what `gitpolicy.forge_for` and `hooks._known_forges` both read, so a lookalike that
resolved to a managed forge would hand that clone the managed forge's credential helper
and `insteadOf` rewrite, and would put the SSH guard's decision on the wrong host.
`tests/test_forge_registry.py` already covers the ACCIDENTAL half of this (finding 1: a
legitimate self-hosted remote whose path contains another forge's name). This is the
hostile half: not "is a friendly URL read correctly" but "does an unfriendly one acquire
authority". Nine of them, all unmanaged.

**2. `news._pass_through` is read off the parser, not named.** It finds open-ended
positionals by inspecting `parser._actions`, explicitly so the #317 shape "cannot come
back under a different command's name". The test that proves this is the one where the
command is called something else entirely: a synthetic `deploy` with `nargs="*"` is found
by a guard that has never heard of it. Naming `secret exec` passes every test written
against `secret exec` and none written against the command added next quarter.

**3. `sync-agents` filenames come from the directory listing.** `names` is
`persona.list_personas()` — a directory read — so `name:` inside a committed persona file
cannot steer where the generated sub-agent is written. The generated file IS a sub-agent's
system prompt, so where it lands is authority.

**4. `clamp_share` fails to `local`.** The VALUE half of this decision is already written
down, in `tests/test_committed_settings_are_validated.py`
(`test_an_unrecognised_share_posture_falls_back_to_local`, #339) — it is not duplicated
here. What was NOT written down is the other half the docstring claims: every reactive
committer re-clamps `config.MEMORY_SHARE` itself rather than trusting it was clamped
upstream. A caller that dropped the re-clamp for a bare `!= "local"` would push on a typo,
and the value-clamp test would still be green. Both halves are needed: one says an
unrecognised value becomes `local`, the other says nothing reaches a remote without asking.

**Preconditions are asserted.** Every refusal is paired with the benign input that still
works, so a guard that stopped running cannot be mistaken for a guard that refused.
"""

from __future__ import annotations

import argparse
import ast
import shutil
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from charter import commands_persona, config, hooks, news, persona, planegit
from charter.forge import registry
from tests._isolation import PersonaIso


# --------------------------------------------------------------------------- #
# 1 — resolve_host fails closed on a lookalike host                            #
# --------------------------------------------------------------------------- #
#: Host-confusion URLs: each one contains a managed host's name, and in each one the host
#: component git would actually connect to is `evil.example` or a subdomain of it. They
#: cover the shapes that fool a parser rather than a reader — userinfo before the real
#: host (`user@github.com@evil.example`), a credential-shaped userinfo
#: (`github.com:token@`), a suffix that only looks like a subdomain
#: (`github.com.evil.example`), and the managed name pushed into the path.
_LOOKALIKES = (
    "https://user@github.com@evil.example/o/r.git",
    "https://github.com@evil.example/o/r.git",
    "https://github.com:token@evil.example/o/r.git",
    "https://github.com.evil.example/o/r.git",
    "git@github.com.evil.example:o/r.git",
    "ssh://git@github.com.evil.example/o/r.git",
    "https://evil.example/github.com/o/r.git",
    "git@evil.example:git.internal/o/r.git",
    "git@git.internal.evil.example:o/r.git",
)


class TestALookalikeHostNeverAcquiresAManagedForge(unittest.TestCase):
    """`resolve_host` is the single question "whose policy governs this remote", asked by
    `gitpolicy.forge_for` (which then writes a credential helper and an `insteadOf`
    rewrite into that clone) and by `hooks._known_forges` (the SSH guard's host set). The
    decision recorded here is that it answers "nobody's" rather than guess: an unrecognised
    host is UNMANAGED, and a URL that merely mentions a managed host is unrecognised."""

    def setUp(self):
        self.root = Path(tempfile.mkdtemp(prefix="edm-lookalike-"))
        self.addCleanup(lambda: shutil.rmtree(self.root, ignore_errors=True))
        # A declared self-hosted host, so both halves of the host set are in play: the
        # class defaults (github.com, gitlab.com) and this plane's own declaration.
        (self.root / "charter.toml").write_text(
            '[[forge]]\nkind = "gitlab"\nhost = "git.internal"\ngroup = "acme"\n')

    def test_a_lookalike_host_resolves_to_unmanaged_never_to_a_managed_forge(self):
        for url in _LOOKALIKES:
            with self.subTest(url=url):
                self.assertIsNone(
                    registry.resolve_host(url, self.root),
                    f"{url!r} was handed a managed forge's policy — the host component "
                    "git connects to is evil.example, not the managed name inside it")

    def test_the_real_hosts_still_resolve(self):
        """The precondition. Every URL above returning None would also be true of a
        `resolve_host` that had stopped resolving anything at all."""
        self.assertEqual(registry.resolve_host("https://github.com/o/r.git", self.root).kind,
                         "github")
        self.assertEqual(registry.resolve_host("git@git.internal:o/r.git", self.root).host,
                         "git.internal")

    def test_a_differently_cased_managed_host_is_still_managed(self):
        """git treats hostnames case-insensitively, so a guard that recognised only the
        canonical spelling would look present while `GITHUB.COM` walked through it."""
        self.assertEqual(registry.resolve_host("https://GITHUB.COM/o/r.git", self.root).host,
                         "github.com")

    def test_an_unparseable_remote_is_unmanaged_rather_than_an_error(self):
        """`resolve_host` runs on every Bash PreToolUse call. Garbage in is `None`, not a
        raise — and `None` is the safe answer, not a fallback to some forge."""
        for url in ("", "   ", "not a url", "://", "file:///srv/repo.git"):
            with self.subTest(url=url):
                self.assertIsNone(registry.resolve_host(url, self.root))


# --------------------------------------------------------------------------- #
# 2 — the pass-through guard is read off the parser, not named                 #
# --------------------------------------------------------------------------- #
class TestPassThroughIsReadOffTheParser(unittest.TestCase):
    """#317's shape was `secret exec`'s `command` positional (`nargs="*"`) becoming an
    argv for `subprocess.run`. The fix does not name that command. It asks argparse which
    positionals swallow an open-ended list, so a command added later inherits the guard
    instead of re-opening the hole under a new name."""

    @staticmethod
    def _parser_with(name: str, *args) -> argparse.ArgumentParser:
        p = argparse.ArgumentParser(prog="charter")
        sub = p.add_subparsers(dest="cmd")
        child = sub.add_parser(name)
        for a, kw in args:
            child.add_argument(a, **kw)
        return child

    def test_an_open_ended_positional_is_found_under_a_command_name_charter_has_never_had(self):
        """The whole decision in one assertion: `deploy` does not exist, has never existed,
        and is not `secret exec` — and its open-ended positional is still reported. A guard
        that named the command would return nothing here."""
        parser = self._parser_with(
            "deploy", ("target", {}), ("argv", {"nargs": "*"}))
        self.assertEqual(news._pass_through(parser), ["argv"])

    def test_a_remainder_positional_counts_too(self):
        """`nargs=argparse.REMAINDER` swallows an open-ended list the same way `"*"` does,
        and is the shape someone reaches for when adding a wrapper command."""
        parser = self._parser_with("wrap", ("rest", {"nargs": argparse.REMAINDER}))
        self.assertEqual(news._pass_through(parser), ["rest"])

    def test_a_command_that_takes_no_open_ended_list_reports_nothing(self):
        """The precondition, and the reason this guard is usable: it does not flag every
        command. A `_pass_through` that returned a positional here would make the whole
        signal meaningless."""
        parser = self._parser_with("quiet", ("name", {}), ("count", {"nargs": "?"}))
        self.assertEqual(news._pass_through(parser), [])

    def test_the_real_cli_reports_commands_317_never_named(self):
        """Read against charter's own parser: the guard covers commands that have nothing
        to do with `secret exec`. If this ever returns only `secret exec`'s positional, the
        guard was narrowed to the one command it was written for."""
        found = {" ".join(path): news._pass_through(news._parser_at(path))
                 for path in news._all_command_paths()}
        found = {k: v for k, v in found.items() if v}
        self.assertEqual(found.get("secret exec"), ["command"],
                         "precondition: #317's own command is no longer reported")
        self.assertEqual(found.get("clone"), ["repos"])
        self.assertEqual(found.get("persona secret exec"), ["command"])
        self.assertGreater(
            len([k for k in found if not k.endswith("secret exec")]), 1,
            f"only `secret exec` is covered — the guard was renamed back into a name: {found}")

    def test_a_leaf_with_no_parser_is_empty_rather_than_a_crash(self):
        self.assertEqual(news._pass_through(None), [])


# --------------------------------------------------------------------------- #
# 3 — a generated agent's filename comes from the directory, not frontmatter   #
# --------------------------------------------------------------------------- #
class TestGeneratedAgentFilenameComesFromTheDirectory(PersonaIso):
    """`sync-agents` writes one file per persona into `.claude/agents/`, and that file IS
    the sub-agent's system prompt — so choosing its path is choosing what a future session
    executes as. The names come from `persona.list_personas()`, a directory read. A `name:`
    key inside `persona.md` is committed, shared data; it names nothing on disk.

    The interesting hostile value is not a traversal — `persona.reference_ok` refuses those
    on the way in — it is **another persona's name**. A persona file claiming `name: forge`
    would, if frontmatter chose the path, land on `forge.md` and replace the forge
    sub-agent's system prompt with its own, for every later session. That is the assertion
    below: the file named after a persona contains THAT persona's charter."""

    #: A sentinel per persona, so "which charter ended up in this file" is checkable.
    def _persona_named(self, dirname: str, frontmatter_name: str) -> Path:
        d = config.PERSONAS_DIR / dirname
        d.mkdir(parents=True, exist_ok=True)
        (d / "persona.md").write_text(
            f"---\nname: {frontmatter_name}\nrole: Role of {dirname}\n---\n\n"
            f"# {dirname}\n\ncharter body of {dirname}\n")
        persona.scaffold_memory(dirname)
        return d

    def _sync(self) -> int:
        return commands_persona.cmd_persona_sync_agents(SimpleNamespace(persona=None))

    @property
    def agents(self) -> Path:
        return config.ROOT / ".claude" / "agents"

    def test_a_persona_file_cannot_choose_which_agent_file_it_is_written_to(self):
        """`helper` claims to be `forge`. `forge.md` must still be forge's own charter —
        the sub-agent a later session dispatches by that name is unchanged."""
        self._persona_named("forge", "forge")
        self._persona_named("helper", "forge")
        self.assertEqual(self._sync(), 0)
        self.assertTrue((self.agents / "helper.md").exists(),
                        "precondition: `helper` was given no sub-agent at all")
        self.assertIn("charter body of forge", (self.agents / "forge.md").read_text())
        self.assertNotIn("charter body of helper", (self.agents / "forge.md").read_text())
        self.assertIn("charter body of helper", (self.agents / "helper.md").read_text())

    def test_every_persona_on_disk_gets_the_file_its_directory_names(self):
        """The set comes from the listing, so the files and the personas cannot disagree —
        no persona is skipped because another one claimed its name."""
        self._persona_named("helper", "second")
        self._persona_named("second", "helper")   # a swap, if frontmatter chose
        self.assertEqual(self._sync(), 0)
        self.assertEqual({p.stem for p in self.agents.glob("*.md")}, set(persona.list_personas()))
        self.assertEqual({p.stem for p in self.agents.glob("*.md")}, {"helper", "second"})
        self.assertIn("charter body of helper", (self.agents / "helper.md").read_text())

    def test_nothing_is_written_outside_the_agents_directory(self):
        """Stated as the consequence rather than the mechanism: whatever the frontmatter
        says, the only files this run creates are under `.claude/agents/`."""
        self._persona_named("helper", "../../../../pwned")
        before = {p for p in config.ROOT.rglob("*") if p.is_file()}
        self.assertEqual(self._sync(), 0)
        created = {p for p in config.ROOT.rglob("*") if p.is_file()} - before
        self.assertTrue(created, "precondition: sync-agents created no files")
        for p in created:
            self.assertEqual(p.parent, self.agents, f"{p} was written outside {self.agents}")
        self.assertFalse(list(config.ROOT.parent.glob("pwned*")))

    def test_the_resolved_persona_is_stamped_with_the_directory_name(self):
        """The same decision one hop in, and the reason the hop above stays safe even if
        someone reaches for `meta["name"]`: `persona.resolve` overwrites `name` with the
        name it was ASKED for. `persona.load` keeps the file's own value (it is the raw
        definition), so this stamp is what stands between committed frontmatter and every
        consumer of a resolved persona — including the `name:` a generated agent is
        dispatched by."""
        self._persona_named("helper", "forge")
        self.assertEqual(persona.load("helper")["meta"]["name"], "forge",
                         "precondition: the frontmatter value never reached `load`")
        self.assertEqual(persona.resolve("helper")["meta"]["name"], "helper")
        self.assertEqual(self._sync(), 0)
        self.assertIn("name: helper", (self.agents / "helper.md").read_text())


# --------------------------------------------------------------------------- #
# 4 — every reader of the declared share posture re-clamps it                  #
# --------------------------------------------------------------------------- #
#: `charter.config` is where MEMORY_SHARE is DEFINED (`instance.share_of`, which clamps).
#: Every other module reads it, and every read is a decision about whether something
#: leaves this machine.
_SHARE_DEFINED_IN = "config.py"


def _unclamped_share_reads(path: Path) -> list[int]:
    """Line numbers in *path* where `MEMORY_SHARE` is read without `clamp_share` around it.

    Parsed rather than grepped: every one of these modules also DISCUSSES the setting in a
    docstring, and a docstring is not a read.
    """
    tree = ast.parse(path.read_text())
    bad: list[int] = []
    for node in ast.walk(tree):
        if not isinstance(node, ast.Call):
            continue
        func = node.func
        name = func.attr if isinstance(func, ast.Attribute) else getattr(func, "id", None)
        if name != "clamp_share":
            continue
        for arg in node.args:                     # a clamped read — mark it seen
            for sub in ast.walk(arg):
                if isinstance(sub, ast.Attribute) and sub.attr == "MEMORY_SHARE":
                    sub._clamped = True           # type: ignore[attr-defined]
    for node in ast.walk(tree):
        if (isinstance(node, ast.Attribute) and node.attr == "MEMORY_SHARE"
                and not getattr(node, "_clamped", False)):
            bad.append(node.lineno)
    return bad


class TestEveryReadOfTheDeclaredPostureIsReClamped(PersonaIso):
    """`[memory] share` is committed, shared config that switches on an unattended
    commit-and-push. `instance.clamp_share` maps anything unrecognised to `local`, and the
    value half of that is asserted in `test_committed_settings_are_validated.py`. This is
    the caller half: no reactive committer trusts that the clamp already happened. It costs
    one call and it means a single unclamped path cannot publish an agent's notes."""

    def test_no_module_reads_the_posture_without_clamping_it(self):
        offenders = {}
        for path in sorted(Path(config.__file__).parent.glob("*.py")):
            if path.name == _SHARE_DEFINED_IN:
                continue
            lines = _unclamped_share_reads(path)
            if lines:
                offenders[path.name] = lines
        self.assertEqual(offenders, {},
                         "a read of the share posture skips `instance.clamp_share` — an "
                         "unrecognised committed value would reach the push decision")

    def test_the_scan_finds_the_reads_it_is_scanning(self):
        """The precondition. An AST walk that matched nothing would report a clean repo
        forever, including after someone deleted every clamp."""
        seen = 0
        for path in sorted(Path(config.__file__).parent.glob("*.py")):
            seen += path.read_text().count("clamp_share(")
        self.assertGreaterEqual(seen, 5, "precondition: the clamp call sites vanished")

    def test_a_typo_in_the_committed_posture_does_not_commit_or_push(self):
        """Behaviour, not source: the two reactive committers that actually reach a remote
        stay silent on a value that is not a posture."""
        for value in ("push ", "PUSH", "publish", "push\n", "commit;push", None):
            with self.subTest(value=value):
                with mock.patch.object(config, "MEMORY_SHARE", value), \
                     mock.patch.object(planegit, "commit_push") as pushed:
                    self.assertEqual(planegit.commit_memory_reactive(["personas"], "t"), 0)
                    pushed.assert_not_called()
                with mock.patch.object(config, "MEMORY_SHARE", value), \
                     mock.patch("charter.commands.commit_push") as pushed:
                    hooks._commit_dispatch(config.ROOT / "workspaces" / "w" / "x.md", "a")
                    pushed.assert_not_called()

    def test_a_real_posture_still_reaches_the_committer(self):
        """The precondition again, and the one that matters most: a clamp that refused
        EVERYTHING would pass the test above while quietly disabling the feature."""
        with mock.patch.object(config, "MEMORY_SHARE", "push"), \
             mock.patch.object(planegit, "commit_push", return_value=0) as pushed:
            planegit.commit_memory_reactive(["personas"], "t")
            pushed.assert_called_once()
        with mock.patch.object(config, "MEMORY_SHARE", "push"), \
             mock.patch("charter.commands.commit_push", return_value=0) as pushed:
            hooks._commit_dispatch(config.ROOT / "workspaces" / "w" / "x.md", "a")
            pushed.assert_called_once()


if __name__ == "__main__":
    unittest.main()
