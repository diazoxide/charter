"""Removing a memory propagates as far as adding one, and the hooks stop guessing how
far that is (issue #82).

Two halves of one defect.

**Removal was not symmetric with addition.** `remember` routes through
`commit_memory_reactive`; `forget` only unlinked the file. A purge therefore stayed on
the machine that ran it while the memory it removed lived on in the remote and returned
with the next pull. That is backwards: removal is the operation that most needs to
propagate, because it is how a poisoned or secret-bearing memory is taken out of
circulation. The incident behind the report was a classifier-bypass note that had to be
purged, and the workaround was a manual commit and push.

**And the hooks described a propagation they never checked.** `_mem_cadence_nudge` chose
its wording from workspace LIVENESS and announced "committed + shared … reactive (commits
+ pushes immediately)". What actually commits is `config.MEMORY_SHARE`, which defaults to
`local` — where nothing is committed at all, deliberately, so that nothing reaches a
remote without a human between writing and disclosure. Meanwhile `_uncommitted_memory_nudge`
scanned `personas/` only, so the one mechanism that could have contradicted the claim was
blind to the store the other hook was recommending.

One hook said it was shared; the other could not see that it wasn't. Under `local` — the
default — both were wrong at once.
"""
from __future__ import annotations

import unittest
from types import SimpleNamespace
from unittest import mock

from charter import commands_persona as cp
from charter import commands_workspace as cw
from charter import config, hooks, memstore, persona, workspace
from tests._isolation import PersonaIso


class ForgetCase(PersonaIso):
    def calls(self):
        """Records what was handed to the reactive committer, without running git."""
        return mock.patch("charter.commands.commit_memory_reactive")

    def _persona(self, name="steward"):
        """`forget` refuses on a persona that does not exist, so the memory store alone
        is not enough of a fixture."""
        d = config.PERSONAS_DIR / name
        d.mkdir(parents=True, exist_ok=True)
        (d / "persona.md").write_text(f"---\nname: {name}\n---\n\nCharter.\n")
        persona.scaffold_memory(name)


class TestMemstoreReportsWhatItRemoved(PersonaIso):
    """The commands can only stage a deletion they are told about."""

    def test_it_returns_the_removed_path(self):
        d = persona.memory_dir("forgettest")
        d.mkdir(parents=True, exist_ok=True)
        (d / "a-fact.md").write_text("x")
        self.assertEqual(memstore.forget(d, "a-fact.md"), d / "a-fact.md")

    def test_a_miss_is_still_falsy(self):
        """Callers test the result for truthiness; a Path is truthy and None is not, so
        the change of type must not change the branch anyone takes."""
        d = persona.memory_dir("forgettest")
        d.mkdir(parents=True, exist_ok=True)
        self.assertFalse(memstore.forget(d, "nope"))

    def test_the_file_is_actually_gone(self):
        d = persona.memory_dir("forgettest")
        d.mkdir(parents=True, exist_ok=True)
        (d / "a-fact.md").write_text("x")
        memstore.forget(d, "a-fact.md")
        self.assertFalse((d / "a-fact.md").exists())


class TestPersonaForgetIsReactive(ForgetCase):
    def forget(self, **kw):
        args = SimpleNamespace(name="steward", slug="a-fact", shared=False,
                               ephemeral=False, **kw)
        with self.calls() as m:
            rc = cp.cmd_persona_forget(args)
        return rc, m

    def setUp(self):
        super().setUp()
        self._persona()
        persona.remember("steward", "a durable fact", title="a fact")

    def test_the_deletion_is_handed_to_the_reactive_committer(self):
        rc, m = self.forget()
        self.assertEqual(rc, 0)
        self.assertTrue(m.called)

    def test_it_stages_the_memory_directory_not_the_deleted_file(self):
        """`git add` on a path that no longer exists and was never tracked fails the
        whole call — which would take the index update down with it. The directory
        always exists, and `git add <dir>` stages deletions inside it."""
        _, m = self.forget()
        staged = m.call_args[0][0]
        self.assertTrue(any(s.endswith("memory") for s in staged), staged)

    def test_the_commit_message_names_the_removal(self):
        _, m = self.forget()
        self.assertIn("forget", m.call_args[0][1])

    def test_an_ephemeral_forget_commits_nothing(self):
        """Ephemeral memory is session scratch and gitignored — `remember` skips the
        commit for it, and so must `forget`."""
        persona.remember("steward", "scratch", title="scratch", ephemeral=True)
        args = SimpleNamespace(name="steward", slug="scratch", shared=False, ephemeral=True)
        with self.calls() as m:
            cp.cmd_persona_forget(args)
        self.assertFalse(m.called)

    def test_a_forget_that_matched_nothing_commits_nothing(self):
        args = SimpleNamespace(name="steward", slug="never-existed", shared=False,
                               ephemeral=False)
        with self.calls() as m:
            rc = cp.cmd_persona_forget(args)
        self.assertEqual(rc, 1)
        self.assertFalse(m.called)


class TestWorkspaceForgetIsReactive(ForgetCase):
    def setUp(self):
        super().setUp()
        workspace.ensure("ws")
        workspace.remember("ws", "a durable fact", title="a fact")

    def test_the_deletion_is_handed_to_the_reactive_committer(self):
        with mock.patch("charter.commands_workspace.commit_memory_reactive") as m:
            rc = cw.cmd_workspace_forget(SimpleNamespace(workspace="ws", slug="a-fact"))
        self.assertEqual(rc, 0)
        self.assertTrue(m.called)

    def test_a_forget_that_matched_nothing_commits_nothing(self):
        with mock.patch("charter.commands_workspace.commit_memory_reactive") as m:
            rc = cw.cmd_workspace_forget(SimpleNamespace(workspace="ws", slug="nope"))
        self.assertEqual(rc, 1)
        self.assertFalse(m.called)


class ShareCase(PersonaIso):
    def note(self, share: str) -> str:
        with mock.patch.object(config, "MEMORY_SHARE", share):
            return hooks.memory_share_note()

    def cadence(self, share: str) -> str:
        with mock.patch.object(config, "MEMORY_SHARE", share):
            return hooks._mem_cadence_nudge(None, 12)


class TestTheNudgeStatesTheRealPosture(ShareCase):
    def test_local_says_it_stays_on_this_machine(self):
        self.assertIn("THIS MACHINE", self.note("local"))

    def test_local_does_not_claim_it_is_shared(self):
        """The exact false claim: a memory recorded under `local` reached nobody while
        the agent was told it had reached the team."""
        n = self.cadence("local").lower()
        self.assertNotIn("committed + shared", n)
        self.assertNotIn("pushes immediately", n)

    def test_local_says_what_to_do_instead(self):
        self.assertIn("yourself", self.note("local"))

    def test_commit_says_committed_but_not_pushed(self):
        n = self.note("commit")
        self.assertIn("NOT pushed", n)

    def test_push_says_it_reaches_the_team(self):
        self.assertIn("pushed", self.note("push"))

    def test_the_cadence_nudge_carries_the_posture(self):
        self.assertIn("THIS MACHINE", self.cadence("local"))

    def test_an_unreadable_posture_says_nothing_rather_than_guessing(self):
        with mock.patch("charter.instance.clamp_share", side_effect=RuntimeError):
            self.assertEqual(hooks.memory_share_note(), "")


class TestTheUncommittedNetSeesBothStores(ShareCase):
    def scan(self, share: str, porcelain: str) -> str:
        with mock.patch.object(config, "MEMORY_SHARE", share), \
             mock.patch("subprocess.run",
                        return_value=SimpleNamespace(stdout=porcelain, returncode=0)):
            return hooks._uncommitted_memory_nudge()

    WS = "?? workspaces/plane-shape/memory/20260813-a-fact.md\n"
    PERSONA = "?? personas/steward/memory/a-fact.md\n"

    def test_an_uncommitted_workspace_memory_is_now_reported(self):
        """The file this whole issue was found through was invisible here."""
        self.assertIn("workspace", self.scan("push", self.WS))

    def test_an_uncommitted_persona_memory_is_still_reported(self):
        self.assertIn("persona", self.scan("push", self.PERSONA))

    def test_both_stores_are_counted_together(self):
        self.assertIn("2", self.scan("push", self.WS + self.PERSONA))

    def test_it_names_the_command_that_fits_the_store(self):
        self.assertIn("workspace save", self.scan("push", self.WS))
        self.assertIn("memory-sync", self.scan("push", self.PERSONA))

    def test_it_is_silent_under_local(self):
        """Under `local`, uncommitted memory is the design, not a backlog. Nagging about
        the intended state — and naming a sync command charter deliberately did not run —
        would be a new false alarm in a change made to remove one."""
        self.assertEqual(self.scan("local", self.WS + self.PERSONA), "")

    def test_nothing_uncommitted_says_nothing(self):
        self.assertEqual(self.scan("push", ""), "")


if __name__ == "__main__":
    unittest.main()
