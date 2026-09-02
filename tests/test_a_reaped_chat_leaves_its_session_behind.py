"""#731, the surviving half: a recycled chat ordinal inherits the previous frame's
per-session state.

`state.new_chat_id` counts up from 1 and hands out the lowest FREE ordinal, and an
ordinal is free the moment `state.reap` removes its directory — so a "fresh" chat id is
very often a recycled NAME. Minting a new id is therefore not isolation; what makes the
name mean a new chat is reaping the state keyed on the old one.

`reap` removed `.charter/frame/<fid>/` and nothing else. Everything charter keys on the
charter session id lives one directory over, in `.charter/sessions/<fid>.*` — and inside
a frame the frame **is** the charter session (ADR 0019), so `<fid>` is that key. The
measured result, reproduced below without tmux: a relaunched `alpha.1` whose predecessor
had selected `gamma` comes up `is_locked: gamma`, `workspace.resolve: gamma`, and
`charter workspace use alpha` — typed by the operator who just launched with
`--workspace alpha` — is **refused as locked**.

#794 closed the visible half: the panels draw `alpha` and the chat belongs to `alpha`,
because `state.own_workspace` no longer reads that pointer. What that leaves is worse to
read than a wrong label — the panels and the commands now disagree, and the operator is
told `locked` by a lock nobody in this chat set.

**The sweep is the directory prefix, not a list of two suffixes**, and that is
`workspace._prune`'s lesson taken at its word rather than re-learned (#366): its allowlist
of marker families drifted three times before it was replaced by "every file in the
directory". `<fid>.workspace` and `<fid>.lock` are the two #731 measured; `<fid>.persona`,
`<fid>.usage`, `<fid>.tools`, `<fid>.gate`, `<fid>.configver`, `<fid>.memnudge`,
`<fid>.route-pending` and `<fid>.<tuid>.<kind>.ask-pending` are keyed the same way by four
other modules, and every one of them is inherited by the same recycled name. A remedy that
names two of eight is a remedy that has to be edited every time a ninth is written.

**The other fix #731 offered is rejected here for the reason its own comment thread
gives.** Letting the launcher's `--workspace` outrank a pointer it did not write only ever
helps a launch that carries the flag, and bare `charter` is now the ordinary way in; it
also leaves the lock — which no flag addresses — exactly where it is.
"""

from __future__ import annotations

import unittest
from unittest import mock

from charter import config, workspace
from charter.frame import state
from tests._isolation import PersonaIso

SERVER = "charter-test-731"


class _ReapedChat(PersonaIso):
    """One chat, one workspace selection inside it, and then a real reap.

    The launcher is patched dead rather than a pid being hand-written into the claim
    file: `reap`'s third keep-rule is "is the process that claimed this directory still
    running", and in this test that process is the test runner itself. Patching the
    predicate is what a real exit answers; writing a number is a guess about which pids
    the machine has free.
    """

    def open_chat(self, ws: str = "alpha") -> str:
        fid = state.new_chat_id(ws)
        assert fid is not None
        state.record_server(fid, SERVER)
        state.record_workspace(fid, ws)
        return fid

    def reap_all(self) -> list[str]:
        # `config.STATE_DIR` is asserted, not assumed: this test's whole subject is a
        # recursive delete under the state directory, and `PersonaIso` repointing it is
        # the only thing between that and the developer's own plane.
        self.assertIn("edm-test-", str(config.STATE_DIR))
        with mock.patch.object(state, "_launcher_is_alive", return_value=False):
            return state.reap(set(), server=SERVER)


class ARecycledOrdinalStartsFromNothing(_ReapedChat):
    def test_the_next_chat_takes_the_reaped_ordinal_back(self):
        """The premise every case below rests on, stated once so the rest are about
        consequences rather than about whether the ordinal really is recycled."""
        first = self.open_chat()
        self.assertEqual(first, "alpha.1")
        self.assertEqual(self.reap_all(), ["alpha.1"])
        self.assertEqual(self.open_chat(), "alpha.1")

    def test_the_workspace_pointer_does_not_outlive_the_frame(self):
        fid = self.open_chat()
        workspace.set_active("gamma", session_id=fid, terminal_id="")
        self.assertEqual(workspace.for_session(fid), "gamma")

        self.reap_all()

        again = self.open_chat()
        self.assertEqual(again, fid, "the ordinal must be recycled or this measures nothing")
        self.assertIsNone(workspace.for_session(again))

    def test_the_lock_does_not_outlive_the_frame(self):
        """The half that survived #794, and the one the operator meets as a refusal."""
        fid = self.open_chat()
        workspace.set_active("gamma", session_id=fid, terminal_id="")
        self.assertEqual(workspace.is_locked(fid), "gamma")

        self.reap_all()
        again = self.open_chat()

        self.assertIsNone(workspace.is_locked(again))

    def test_the_new_chat_can_select_the_workspace_it_was_launched_with(self):
        """`charter claude --workspace alpha`, then `charter workspace use alpha` — the
        exact sequence #731 reports, and the one that answered `locked`."""
        fid = self.open_chat()
        workspace.set_active("gamma", session_id=fid, terminal_id="")

        self.reap_all()
        again = self.open_chat()

        self.assertEqual(
            workspace.set_active("alpha", session_id=again, terminal_id=""), "session")

    def test_resolution_in_the_new_chats_shell_is_not_the_old_chats_choice(self):
        """`workspace.resolve` is what every `charter` command in the frame's own shell
        acts on — `charter clone`, `charter repos`, `charter ws current`."""
        fid = self.open_chat()
        workspace.set_active("gamma", session_id=fid, terminal_id="")
        self.assertEqual(workspace.resolve(session_id=fid), "gamma")

        self.reap_all()
        again = self.open_chat()

        self.assertNotEqual(workspace.resolve(session_id=again), "gamma")


class TheWholeFamilyGoesNotTwoSuffixes(_ReapedChat):
    """`.workspace` and `.lock` are the two #731 measured; eight modules key on this id.

    Written as a loop over the families that exist today **and** as a guard that the loop
    is not the list: a marker family added tomorrow is covered the day it is written,
    which is the property `workspace._prune` was rewritten to have (#366).
    """

    #: Every suffix charter writes into `SESSIONS_DIR` keyed on a session id, with the
    #: module that owns it. Spelled here by hand rather than imported: a test that asked
    #: the same constant the writer asks would move with a rename and pin nothing.
    FAMILIES = [
        ("workspace", "charter.workspace"),
        ("lock", "charter.workspace"),
        ("persona", "charter.persona"),
        ("usage", "charter.statusline"),
        ("tools", "charter.toolgate"),
        ("gate", "charter.toolgate"),
        ("configver", "charter.hooks"),
        ("memnudge", "charter.hooks"),
        ("route-pending", "charter.hooks"),
        ("toolu_a1.PreToolUse.ask-pending", "charter.hooks"),
    ]

    def test_every_marker_keyed_on_the_chat_id_goes_with_it(self):
        fid = self.open_chat()
        config.private_mkdir(config.SESSIONS_DIR)
        written = []
        for suffix, owner in self.FAMILIES:
            p = config.SESSIONS_DIR / f"{fid}.{suffix}"
            p.write_text(f"{owner}\n")
            written.append(p)

        self.reap_all()

        still_there = sorted(p.name for p in written if p.exists())
        self.assertEqual(still_there, [])

    def test_a_family_nobody_has_written_yet_is_covered_too(self):
        """The point of sweeping the prefix rather than the suffixes: this name is not in
        `FAMILIES`, is not in charter, and is reaped anyway."""
        fid = self.open_chat()
        config.private_mkdir(config.SESSIONS_DIR)
        invented = config.SESSIONS_DIR / f"{fid}.a-marker-written-next-year"
        invented.write_text("x\n")

        self.reap_all()

        self.assertFalse(invented.exists())

    def test_another_sessions_markers_are_left_alone(self):
        """The match is anchored AND dot-terminated, and both halves are reachable.

        `alpha.10` is the tail half: `state.new_chat_id` counts to `_CHAT_ORDINAL_MAX`,
        so a plane with ten chats in one workspace has `alpha.1` and `alpha.10` side by
        side and one of them is live.

        `xalpha.1` is the head half, and it is why `startswith` rather than `in`: a chat
        id is `{workspace_prefix}.{n}`, `workspace_prefix` only maps characters into
        ``[A-Za-z0-9_-]`` and never inserts a boundary, so two workspaces whose names end
        the same way — `alpha` and `xalpha`, `api` and `legacy-api` — mint ids where one
        is a suffix of the other. An unanchored match reaps the live one's pointer.
        """
        fid = self.open_chat()          # alpha.1
        config.private_mkdir(config.SESSIONS_DIR)
        neighbour = config.SESSIONS_DIR / "alpha.10.workspace"
        neighbour.write_text("beta\n")
        sibling_workspace = config.SESSIONS_DIR / "xalpha.1.workspace"
        sibling_workspace.write_text("beta\n")
        stranger = config.SESSIONS_DIR / "8f14e45f-ea3a-4b71-9a2c-000000000001.workspace"
        stranger.write_text("beta\n")
        prefixless = config.SESSIONS_DIR / "alpha.1"
        prefixless.write_text("beta\n")

        self.assertEqual(self.reap_all(), [fid])

        self.assertTrue(neighbour.exists(), "`alpha.1.` must not match `alpha.10.…`")
        self.assertTrue(sibling_workspace.exists(),
                        "the match is anchored: `xalpha.1.workspace` CONTAINS `alpha.1.`")
        self.assertTrue(stranger.exists())
        self.assertTrue(prefixless.exists(),
                        "the id itself is not a marker for the id — only `<fid>.<suffix>`")

    def test_a_chat_that_is_still_live_keeps_its_selection(self):
        """The direction that would cost the operator their working state: reaping is
        keyed on the directory that was actually removed, never on the sweep running."""
        kept = self.open_chat()
        workspace.set_active("gamma", session_id=kept, terminal_id="")

        self.assertIn("edm-test-", str(config.STATE_DIR))
        with mock.patch.object(state, "_launcher_is_alive", return_value=False):
            self.assertEqual(state.reap({kept}, server=SERVER), [])

        self.assertEqual(workspace.for_session(kept), "gamma")
        self.assertEqual(workspace.is_locked(kept), "gamma")

    def test_a_frame_on_another_server_keeps_its_selection(self):
        """`reap` is scoped to one server (#381); the marker sweep inherits that scope
        because it happens only where the directory was removed."""
        fid = self.open_chat()
        workspace.set_active("gamma", session_id=fid, terminal_id="")

        self.assertIn("edm-test-", str(config.STATE_DIR))
        with mock.patch.object(state, "_launcher_is_alive", return_value=False):
            self.assertEqual(state.reap(set(), server="a-different-server"), [])

        self.assertEqual(workspace.for_session(fid), "gamma")


class AFilesystemThatRefuses(_ReapedChat):
    """The two `except OSError` clauses in `_forget_session`, entered on purpose.

    `chmod` is not the seam — CI runs some jobs as a user for whom mode bits are advice —
    so the failure is injected at the exact call that can raise it. Both clauses are
    asserted by their CONSEQUENCE rather than by "it did not raise": `reap` still reports
    the directory it removed, and the caller still gets a list.
    """

    def test_a_sessions_directory_that_cannot_be_listed_costs_nothing_else(self):
        fid = self.open_chat()
        workspace.set_active("gamma", session_id=fid, terminal_id="")

        self.assertIn("edm-test-", str(config.STATE_DIR))
        real = state.Path.iterdir

        def refuse(self_):
            if self_ == state.Path(config.SESSIONS_DIR):
                raise PermissionError(13, "refused")
            return real(self_)

        with mock.patch.object(state, "_launcher_is_alive", return_value=False), \
                mock.patch.object(state.Path, "iterdir", refuse):
            self.assertEqual(state.reap(set(), server=SERVER), [fid])

        self.assertFalse((config.STATE_DIR / "frame" / fid).exists(),
                         "the frame's own directory still goes")
        self.assertEqual(workspace.for_session(fid), "gamma",
                         "no proof, no deletion — the marker is left where it was")

    def test_one_marker_that_cannot_be_unlinked_does_not_strand_the_rest(self):
        fid = self.open_chat()
        workspace.set_active("gamma", session_id=fid, terminal_id="")
        stuck = config.SESSIONS_DIR / f"{fid}.lock"
        real = state.Path.unlink

        def refuse(self_, *a, **kw):
            if self_.name == stuck.name:
                raise PermissionError(13, "refused")
            return real(self_, *a, **kw)

        self.assertIn("edm-test-", str(config.STATE_DIR))
        with mock.patch.object(state, "_launcher_is_alive", return_value=False), \
                mock.patch.object(state.Path, "unlink", refuse):
            self.assertEqual(state.reap(set(), server=SERVER), [fid])

        self.assertTrue(stuck.exists())
        self.assertIsNone(workspace.for_session(fid),
                          "the loop continues past the one it could not remove")


if __name__ == "__main__":
    unittest.main()
