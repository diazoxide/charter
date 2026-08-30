"""The charter a persona sub-agent obeys is a GENERATED copy, and nothing kept it honest.

Two defects, one shape.

**#677 — the copies drift, silently, in the direction that matters.** A persona is edited in
``personas/<name>/persona.md``; the sub-agent that is dispatched reads
``.claude/agents/<name>.md``, which `charter persona sync-agents` writes. Both are committed,
nothing asserted they agree, and on ``main`` at ``db87d2e`` all five were four days behind —
`.claude/agents/release.md` missing the bounded-release-body guidance in the same week an
over-long release body was the blocking defect. A drifted agent file does not fail. It runs an
older charter and reports success, which is the *convincing empty* this project refuses
everywhere else.

`persona lint` already noticed (`_agent_sync_issues`) and already named the command — as a
**warning**, so `lint` exits 0 and CI never ran it anyway. Noticing without failing is how a
guard gets waved through; this module is the same finding with an exit code.

**Why the check regenerates rather than diffing the prose.** The drift measured while fixing
this was not in the persona sources at all: `steward.md` was missing a *correction to the
generator* (the `uses:` wording that says a vault name is a rule you keep, not a wall charter
holds) and `reddit.md` carried a pre-#453 YAML shape for its MCP block. A test comparing the
committed agent against the committed charter would have passed over both. The property is
"this file is what `sync-agents` would write **today**", so the check runs `sync-agents`.

**#678 — regenerating wrote into the wrong tree.** `root._plane_of` redirects a linked
worktree to the tree it was cut from, so `sync-agents` run from a worktree edited tracked
files in the main clone. Every worker here owns a worktree and is told never to run git in
another; a command that reaches out of the caller's worktree defeats that rule from
underneath, and silently — the agent obeyed it and still dirtied someone else's tree.

The answer is not "which root" but **which operations follow the plane and which follow the
tree**: identity and machine-local state (the roster, the vault, the MCP approval record,
memory) follow the PLANE, because a worktree is a view of a plane's repo and not a second
plane. A *generation* — tracked files in, tracked files out — follows the TREE, because the
artifact belongs to the commit and the commit belongs to the branch. `root.tree_of` states it;
`config.in_tree` is the whole mechanism.

Not merged into `tests/test_freshness.py`: that module is about a *running session* being
behind the committed plane. This is about the committed plane disagreeing with itself.
"""

from __future__ import annotations

import difflib
import io
import json
import os
import shutil
import subprocess
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from charter import commands_persona as cp
from charter import config, root
from tests._isolation import PersonaIso

REPO = Path(__file__).resolve().parents[1]

#: The command a failure here must name. #666's closure test is the model: a guard that says
#: "these differ" and not "run this" is a guard the next reader waves through.
REGENERATE = "charter persona sync-agents"


def _generated(tree: Path) -> dict[str, str]:
    """Every ``.claude/agents/*.md`` under *tree*, by filename."""
    d = tree / ".claude" / "agents"
    return {p.name: p.read_text() for p in sorted(d.glob("*.md"))} if d.is_dir() else {}


def _diff(name: str, committed: str, regenerated: str, limit: int = 24) -> str:
    """The first *limit* lines of a unified diff, so a failure says WHAT drifted.

    Bounded because a persona charter is thousands of words: an unbounded diff of five of
    them buries the one line that says which command to run.
    """
    lines = list(difflib.unified_diff(
        committed.splitlines(), regenerated.splitlines(),
        fromfile=f"committed .claude/agents/{name}", tofile=f"what {REGENERATE} writes",
        lineterm="", n=1))
    shown = lines[:limit]
    if len(lines) > limit:
        shown.append(f"… {len(lines) - limit} more diff line(s)")
    return "\n".join(shown)


def _sync(**overrides):
    """Run the real command, with output captured. Returns (rc, transcript)."""
    args = SimpleNamespace(persona=None, approve_mcp=False, yes=False, dry_run=False)
    for k, v in overrides.items():
        setattr(args, k, v)
    out, err = io.StringIO(), io.StringIO()
    with redirect_stdout(out), redirect_stderr(err):
        rc = cp.cmd_persona_sync_agents(args)
    return rc, out.getvalue() + err.getvalue()


class TheGenerationThisRepoCommits(PersonaIso):
    """This repository's own persona sources, regenerated into a throwaway tree.

    A COPY, never the checkout: the whole point is a check that runs in the suite on every
    push, and one that rewrites the working tree it is checking would launder the very drift
    it exists to catch — green because it just fixed it, with the fix uncommitted.

    Copied rather than read in place because `sync-agents` also PRUNES: a generated agent
    whose persona is gone is deleted, and a check that never exercised the prune would miss
    the orphan half of the same defect.
    """

    def setUp(self) -> None:
        super().setUp()
        self.tree = self.tmp / "regenerated"
        self.tree.mkdir(parents=True, exist_ok=True)
        shutil.copytree(REPO / "personas", self.tree / "personas")
        shutil.copy2(REPO / "charter.toml", self.tree / "charter.toml")
        # `charter.toml` comes along because the generated prose is worded for THIS plane's
        # declared forge (`_credential_rule`); without it every agent would render the
        # multi-forge fallback and the comparison would be against a plane that isn't this
        # one.
        shutil.copytree(REPO / ".claude" / "agents", self.tree / ".claude" / "agents")
        with config.in_tree(self.tree):
            self.rc, self.said = _sync()

    def regenerated(self) -> dict[str, str]:
        return _generated(self.tree)

    def committed(self) -> dict[str, str]:
        return _generated(REPO)


class TestTheCommittedSubAgentIsWhatSyncAgentsWouldWrite(TheGenerationThisRepoCommits):
    def test_the_regeneration_itself_succeeds(self):
        """Precondition. If `sync-agents` failed, every assertion below is comparing two
        copies of the committed files and passing for the wrong reason."""
        self.assertEqual(self.rc, 0, self.said)
        self.assertIn("Synced", self.said)

    def test_every_persona_has_the_sub_agent_it_is_dispatched_as(self):
        missing = sorted(set(self.regenerated()) - set(self.committed()))
        self.assertFalse(missing, self._msg(
            f"{len(missing)} persona(s) have no committed sub-agent: {', '.join(missing)}. "
            f"Until the file exists the persona cannot be dispatched at all."))

    def test_no_generated_sub_agent_outlives_the_persona_it_names(self):
        """The prune half. A generated agent whose persona is gone keeps that persona
        dispatchable under whatever its charter said before it was removed."""
        orphans = sorted(set(self.committed()) - set(self.regenerated()))
        self.assertFalse(orphans, self._msg(
            f"{len(orphans)} committed sub-agent(s) no longer have a persona: "
            f"{', '.join(orphans)}."))

    def test_every_generated_sub_agent_is_what_its_persona_says_today(self):
        """The defect itself, and the one that cannot be seen by reading either file."""
        regenerated, committed = self.regenerated(), self.committed()
        drifted = [n for n in sorted(set(regenerated) & set(committed))
                   if regenerated[n] != committed[n]]
        if not drifted:
            return
        detail = "\n\n".join(_diff(n, committed[n], regenerated[n]) for n in drifted)
        self.fail(self._msg(
            f"{len(drifted)} generated sub-agent(s) no longer match their persona: "
            f"{', '.join(drifted)}.\n\n{detail}"))

    def _msg(self, what: str) -> str:
        return (
            f"{what}\n\n"
            f"`.claude/agents/<persona>.md` is the file a dispatched sub-agent actually "
            f"reads — `personas/<persona>/persona.md` is the file a human edits. A stale "
            f"copy does not fail; it silently runs an older charter. Regenerate and commit "
            f"the result:\n\n    {REGENERATE}\n")


class TestTheGenerationDoesNotDependOnWhoRanIt(TheGenerationThisRepoCommits):
    """A committed generated file has to be reproducible, or the check above is a trap.

    `persona.mcp_render_entry` wraps a declared MCP server in `charter secret exec` only when
    that server's fingerprint has been **approved on this machine** — a record under
    `.charter/`, gitignored by design. A sidecar declaring `secrets:` therefore renders one
    way on the approver's laptop and another in CI, so committing either answer makes the
    freshness check flip on whoever runs it and ping-pong the file between two spellings.

    Charter's own personas declare no such server, and this is the assertion that says so
    rather than the assumption that it stays true.
    """

    def test_no_committed_sidecar_makes_the_generated_agent_machine_dependent(self):
        offenders = []
        for sidecar in sorted((REPO / "personas").glob("*/mcp.json")):
            try:
                servers = (json.loads(sidecar.read_text()) or {}).get("mcpServers") or {}
            except (OSError, ValueError):
                continue          # a malformed sidecar is `persona lint`'s finding, not this
            for server, entry in servers.items():
                if isinstance(entry, dict) and (entry.get("secrets") or entry.get("secret_files")):
                    offenders.append(f"{sidecar.relative_to(REPO)} → {server}")
        self.assertFalse(offenders, (
            f"{', '.join(offenders)} declares a credential-bearing MCP server, so what "
            f"`{REGENERATE}` writes depends on whether that server was approved on the "
            f"machine that ran it (`charter persona sync-agents --approve-mcp`). The "
            f"committed sub-agent can then only match one of the two answers, and the "
            f"freshness check above would flip per developer. Either keep the credential "
            f"hand-off out of a committed generated file, or teach that check which answer "
            f"is the committed one."))


class WorktreeCase(PersonaIso):
    """A real plane repository with a real linked worktree over it.

    Real git, for `tests/test_piece_claim.py`'s reason: the redirect under test is decided by
    a `.git` FILE that only git writes, and a fixture that wrote one by hand would be
    asserting against this module's idea of git's layout rather than against git's.
    """

    #: Does the plane hold a persona at all? A plane that holds none is the case the
    #: refusal below must NOT fire on — see `TestAPlaneWithNoPersonasIsNotARefusal`.
    SEED_PERSONA = True

    def setUp(self) -> None:
        super().setUp()
        self.plane = self.tmp / "plane"
        self.plane.mkdir(parents=True, exist_ok=True)
        self.addCleanup(config.restore, config.use(self.plane))
        (self.plane / "charter.toml").write_text("schema = 1\n\n[[forge]]\nkind = \"github\"\n")
        if self.SEED_PERSONA:
            self.make_persona("scribe", role="Scribe of the plane", vault="none")
        self.git("init", "-q", "-b", "main", ".")
        self.git("add", "-A")
        self.git("commit", "-qm", "base")

        # The plane's own copy, written the ordinary way. Also the regression half: this is
        # the path every non-worktree caller takes and it must be unchanged.
        self.rc_plane, self.said_plane = _sync()
        self.plane_agent = self.plane / ".claude" / "agents" / "scribe.md"

        self.worktree = self.tmp / "wt"
        self.git("worktree", "add", "-q", "-b", "branch", str(self.worktree))
        if self.SEED_PERSONA:
            # The branch edits the persona. Without this the two trees render identically
            # and a write into the wrong one is invisible — exactly how #678 survived.
            edited = (self.worktree / "personas" / "scribe" / "persona.md")
            edited.write_text(edited.read_text().replace("Scribe of the plane",
                                                         "Scribe of the branch"))

    def git(self, *argv: str) -> None:
        subprocess.run(["git", "-C", str(self.plane), *argv], check=True, capture_output=True)

    def sync_from(self, cwd: Path):
        here = Path.cwd()
        os.chdir(cwd)
        # Registered rather than a `finally`, and BEFORE `PersonaIso`'s rmtree can run: a cwd
        # left inside a deleted temp directory makes `Path.cwd()` raise for every test that
        # comes after.
        self.addCleanup(os.chdir, here)
        try:
            return _sync()
        finally:
            os.chdir(here)


class TestASyncFollowsTheTreeItWasRunFrom(WorktreeCase):
    def test_the_plane_still_generates_its_own_agents(self):
        """The regression. `tree_of` answers None for a caller standing in the plane, so the
        ordinary path must be untouched — including the report, which says nothing about a
        worktree because there is none."""
        self.assertEqual(self.rc_plane, 0, self.said_plane)
        self.assertIn("Scribe of the plane", self.plane_agent.read_text())
        self.assertNotIn("worktree you ran from", self.said_plane)

    def test_it_writes_into_the_worktree(self):
        rc, said = self.sync_from(self.worktree)
        self.assertEqual(rc, 0, said)
        wrote = self.worktree / ".claude" / "agents" / "scribe.md"
        self.assertTrue(wrote.is_file(), f"nothing was written into the worktree: {said}")
        self.assertIn("Scribe of the branch", wrote.read_text())

    def test_it_does_not_touch_the_clone_it_was_cut_from(self):
        """#678 as reported: the write landed in a tree the caller does not own, with no
        conflict and no message, while five agents worked in parallel."""
        before = self.plane_agent.read_text()
        self.sync_from(self.worktree)
        self.assertEqual(self.plane_agent.read_text(), before,
                         "sync-agents edited a tracked file in the main clone")

    def test_it_reads_the_worktrees_sources_not_the_planes(self):
        """The half that would be a worse bug than #678 if it were got wrong: moving only
        the OUTPUT would render the plane's persona over the branch's own edit, and the
        author would commit the reversion."""
        self.sync_from(self.worktree)
        wrote = (self.worktree / ".claude" / "agents" / "scribe.md").read_text()
        self.assertNotIn("Scribe of the plane", wrote)

    def test_it_says_where_it_wrote(self):
        """A redirect charter can see is a redirect charter names (ADR 0013). Otherwise a
        worker goes looking for the change in the clone — or assumes it landed there."""
        _rc, said = self.sync_from(self.worktree)
        self.assertIn(str(self.worktree), said)
        self.assertIn(str(self.plane), said)

    def test_the_planes_state_is_still_the_planes(self):
        """The split, from the other side. Sources and outputs follow the tree; the
        machine-local state directory — the vault registry, the MCP approval record, the
        ephemeral persona state — does not fork per worktree."""
        state = config.STATE_DIR
        with config.in_tree(self.worktree):
            self.assertEqual(config.ROOT, self.worktree)
            self.assertEqual(config.PERSONAS_DIR, self.worktree / "personas")
            self.assertEqual(config.STATE_DIR, state)
            self.assertEqual(config.VAULTS_REGISTRY, state / "vaults.json")
        self.assertEqual(config.ROOT, self.plane)
        self.assertEqual(config.STATE_DIR, state)

    def test_a_developer_who_relocated_their_state_keeps_it_afterwards(self):
        """The pin is an argument to one `derive` call, not a change to the process — so a
        developer who put their state somewhere with `$CHARTER_HOME` must find it still set,
        to their value, once the generation is over.

        `in_tree` sets that variable to make the plane's state survive the re-derivation. A
        restore that always *unset* it would look correct in every test where it started
        unset, and silently move the next command's vault back into the plane for the one
        developer who had relocated theirs."""
        chosen, state = str(self.tmp / "elsewhere"), config.STATE_DIR
        with mock.patch.dict(os.environ, {config.STATE_HOME_VAR: chosen}):
            with config.in_tree(self.worktree):
                # The pin carries the state directory this session ALREADY resolved, not
                # whatever the variable says now — the plane's answer, whichever way it was
                # reached.
                self.assertEqual(config.STATE_DIR, state)
            self.assertEqual(os.environ.get(config.STATE_HOME_VAR), chosen)

    def test_a_developer_who_never_set_it_does_not_acquire_it(self):
        """The other side of the same branch: the pin is lifted, not left behind."""
        os.environ.pop(config.STATE_HOME_VAR, None)
        with config.in_tree(self.worktree):
            pass
        self.assertNotIn(config.STATE_HOME_VAR, os.environ)

    def test_reading_a_worktree_never_writes_a_state_directory_into_it(self):
        """`config.derive` migrates a legacy state directory as a side effect of being
        called. Re-deriving for a worktree must not make charter create — or move — anything
        in a tree it was only asked to generate into."""
        with config.in_tree(self.worktree):
            pass
        self.assertFalse((self.worktree / ".charter").exists())


class TestASyncRefusesAWorktreeThatCarriesNoSources(WorktreeCase):
    def test_it_refuses_rather_than_reaching_back_into_the_plane(self):
        """A worktree of the plane on a branch with no `personas/` — cut before the plane was
        committed, or from a repo whose `charter.toml` was never staged. Generating there
        writes nothing; generating into the plane instead is #678 again. So: refuse, and name
        the tree that has the sources."""
        shutil.rmtree(self.worktree / "personas")
        rc, said = self.sync_from(self.worktree)
        self.assertEqual(rc, 1, said)
        self.assertIn(str(self.plane), said)
        self.assertIn("personas", said)
        self.assertIn(str(self.worktree), said)

    def test_it_writes_nothing_anywhere(self):
        before = self.plane_agent.read_text()
        shutil.rmtree(self.worktree / "personas")
        self.sync_from(self.worktree)
        self.assertEqual(self.plane_agent.read_text(), before)
        self.assertFalse((self.worktree / ".claude" / "agents").exists())


class TestAPlaneWithNoPersonasIsNotARefusal(WorktreeCase):
    """The refusal is narrow, and this is the half that makes it narrow.

    "The worktree has no `personas/`" is not enough to refuse on: a plane that has no
    personas *at all* has nothing for either tree to generate, and `sync-agents` already has
    a true answer for that — "No personas to sync. Create one first." Refusing there would
    hand a worker a message about worktrees for a plane that simply has no roster yet, and
    would make `charter persona create` in a worktree unreachable through the report that
    tells you to run this next.
    """

    SEED_PERSONA = False

    def test_a_worktree_of_an_empty_plane_is_told_the_plane_is_empty(self):
        rc, said = self.sync_from(self.worktree)
        self.assertEqual(rc, 0, said)
        self.assertIn("No personas to sync", said)
        self.assertNotIn("carries no", said)


class TestTreeOfIsTheNarrowQuestion(WorktreeCase):
    """`root.tree_of` answers "is this caller standing in a linked worktree of THIS plane",
    and nothing wider. A broader answer would redirect writes for callers whose trees have
    nothing to do with the plane's generated files."""

    def test_a_worktree_of_the_plane_is_the_tree(self):
        # `.resolve()` on the expectation, not on the answer: the function canonicalises,
        # which on macOS is the difference between `/var/…` and `/private/var/…`. A test
        # that compared the raw fixture path would pass on Linux and fail on the machine
        # #678 was reported from.
        self.assertEqual(root.tree_of(self.plane, self.worktree), self.worktree.resolve())

    def test_a_directory_inside_the_worktree_is_still_that_worktree(self):
        deep = self.worktree / "personas" / "scribe"
        self.assertEqual(root.tree_of(self.plane, deep), self.worktree.resolve())

    def test_the_plane_itself_is_not_a_worktree_of_itself(self):
        """None rather than the plane, so a caller has to say what it wants for the ordinary
        case instead of inheriting an answer."""
        self.assertIsNone(root.tree_of(self.plane, self.plane))

    def test_a_worktree_of_some_other_repo_is_not_the_planes(self):
        """A clone under `workspaces/<ws>/<repo>` resolves the plane by walking up, and its
        worktrees are views of that clone. Redirecting the plane's generated files into one
        would be a new bug wearing this fix's clothes."""
        other = self.tmp / "other"
        other.mkdir()
        subprocess.run(["git", "init", "-q", "-b", "main", str(other)],
                       check=True, capture_output=True)
        (other / "f").write_text("x\n")
        subprocess.run(["git", "-C", str(other), "add", "-A"], check=True, capture_output=True)
        subprocess.run(["git", "-C", str(other), "commit", "-qm", "base"],
                       check=True, capture_output=True)
        other_wt = self.tmp / "other-wt"
        subprocess.run(["git", "-C", str(other), "worktree", "add", "-q", "-b", "b",
                        str(other_wt)], check=True, capture_output=True)
        self.assertIsNone(root.tree_of(self.plane, other_wt))

    def test_a_plain_directory_is_not_a_tree(self):
        plain = self.tmp / "plain"
        plain.mkdir()
        self.assertIsNone(root.tree_of(self.plane, plain))

    def test_a_start_that_does_not_exist_answers_none(self):
        self.assertIsNone(root.tree_of(self.plane, self.tmp / "gone" / "deeper"))

    def test_a_working_directory_that_no_longer_exists_answers_none(self):
        """It sits on a command path and must never raise — and this is the way that
        actually happens, which no non-existent *argument* reaches: with no `start`, the
        question is asked of `Path.cwd()`, and a process whose working directory was deleted
        out from under it gets `FileNotFoundError` there. `find_root_or_cwd` catches the same
        pair for the same reason and says so in its own docstring; without the catch,
        `sync-agents` would traceback instead of generating.

        Both spellings, because `.resolve()` answers a symlink loop with `RuntimeError`
        rather than an `OSError`, and a catch narrowed to one of the two is a catch that
        holds for one of the two.
        """
        for boom in (FileNotFoundError("cwd deleted"), RuntimeError("symlink loop")):
            with self.subTest(raises=type(boom).__name__):
                with mock.patch.object(Path, "cwd", side_effect=boom):
                    self.assertIsNone(root.tree_of(self.plane))


if __name__ == "__main__":
    unittest.main()
