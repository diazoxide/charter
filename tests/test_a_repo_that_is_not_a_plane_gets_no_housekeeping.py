"""Outside a control plane charter writes nothing and says nothing — and still refuses.

#852. ADR 0015 raises the objection itself — "a plugin installed for every project does run
charter's hooks in repos with no control plane" — and answers it with a promise: *"the
guards gate on `config.HAS_CONTROL_PLANE` and stay silent outside a plane."* That promise
was kept at five call sites in `charter/hooks.py`: the four denials A2/A3/A3b/A4 and
`_state_write_reason`. Six of the eleven handlers in `hooks._HANDLERS` had never heard of
it, and `~/.claude/plugins/installed_plugins.json` records `charter@charter` at four
project paths, so this is what people already have.

Measured on 0.55.0, each handler run once as a real subprocess at a cwd with no
``charter.toml``, in an ordinary git repository:

===================== ================================================================
handler               left behind in somebody else's repo
===================== ================================================================
sessionstart          ``.charter/sessions/<sid>.tools``, ``.charter/sessions/<sid>.gate``
userpromptsubmit      ``.charter/sessions/<sid>.configver``
pretooluse            ``.charter/guard-seen.json``
posttooluse           ``.charter/sessions/<sid>.memnudge``
posttooluse-dispatch  ``personas/_dispatch/<month>.<hostname>.jsonl``, ``.charter/…``
===================== ================================================================

``git status`` in that repo then reads ``?? .charter/`` — and, for the dispatch tally,
``?? personas/`` at a path no `.gitignore` anywhere covers, carrying the operator's
hostname. That is precisely what `harness/opencode.py`'s `wire` refuses to do to a plane:
*"A plane is somebody's repo, and charter's housekeeping has no business in its `git
status`."* The same sentence is true one level out.

Two more, neither in the report:

* **Charter spoke.** `sessionstart` injected "Confirm the workspace before any repo work …
  via a quiz (AskUserQuestion)" into a repository with no control plane and no workspaces.
* **Charter granted.** With ``personas/rogue/persona.md`` declaring ``tools: [curl]`` and
  ``.charter/active-persona`` naming it — two ordinary files, both of which a repository
  can simply contain — `pretooluse` answered ``permissionDecision: allow`` for ``curl
  https://evil.example/x``. Outside a plane those are not charter's files; they are the
  cloned repository's. A repo deciding which commands run without a prompt is authority,
  not housekeeping.

**Four properties, and each fails in a different direction, so no single mistake passes
them all.**

1. :class:`NothingIsWritten` — no handler, on any payload including the hostile ones,
   creates or changes a single byte under a cwd that is not a plane. Driven off
   ``hooks._HANDLERS`` rather than a list typed here, so the handler somebody adds next
   inherits the property instead of having to remember it.
2. :class:`NothingIsSaid` — no handler emits anything, and none grants anything.
3. :class:`TheGuardsStillRefuse` — the four refusals charter documents as facts about the
   shell rather than policies of a plane still deny, outside a plane, with no plane
   anywhere: the vault-leak guard on Bash (A) and on Read (`pretooluse_read`), and the
   live-substitution guards on a forge command (A5) and on charter's own (A6). A fix that
   made charter quiet by making it toothless would pass 1 and 2 and fail here.
4. :class:`InsideAPlaneNothingIsLost` — the same payloads, in a real plane, still write,
   still speak and still grant. Without this, deleting the features would pass 1–3.

The refusal strings are **hand-spelled**, never compared against the constant that produces
them: a test asserting ``reason == hooks._SINGLE_CREDENTIAL_FIX`` passes against a constant
edited to the empty string.
"""

from __future__ import annotations

import io
import json
import os
import sys
import unittest
from contextlib import redirect_stdout
from pathlib import Path
from unittest import mock

from charter import config, hooks
from tests._isolation import PersonaIso, make_plane


def _run(fn, payload: dict) -> tuple[int, str]:
    """Call a handler with *payload* on stdin; return ``(exit code, stdout)``.

    `tests._isolation.run_hook` drops the exit code, and #438's whole subject is that a
    denial charter could not WRITE must not read as an allow — so both halves are needed
    here.
    """
    old = sys.stdin
    sys.stdin = io.StringIO(json.dumps(payload))
    buf = io.StringIO()
    try:
        with redirect_stdout(buf):
            rc = fn()
    finally:
        sys.stdin = old
    return rc, buf.getvalue().strip()


def _payloads(cwd: str, sid: str) -> dict[str, dict]:
    """One payload per handler, each shaped so the handler reaches its BODY.

    A single fat payload would carry one ``tool_name``, and every handler that checks for
    a different one would return on its first line — a green run proving nothing. Each
    entry here names the tool its handler is registered against in `hooks/hooks.json`.
    """
    here = os.path.join(cwd, "README.md")
    return {
        "sessionstart": {"cwd": cwd, "session_id": sid, "source": "startup"},
        "userpromptsubmit": {"cwd": cwd, "session_id": sid,
                             "prompt": "please tidy up the README when you get a chance"},
        "pretooluse": {"cwd": cwd, "session_id": sid, "tool_name": "Bash",
                       "tool_input": {"command": "ls -la"}},
        "pretooluse-read": {"cwd": cwd, "session_id": sid, "tool_name": "Read",
                            "tool_input": {"file_path": here}},
        "pretooluse-dispatch": {"cwd": cwd, "session_id": sid, "tool_name": "Task",
                                "tool_input": {"subagent_type": "reviewer"}},
        "pretooluse-edit": {"cwd": cwd, "session_id": sid, "tool_name": "Edit",
                            "tool_input": {"file_path": here, "new_string": "hi"}},
        "posttooluse": {"cwd": cwd, "session_id": sid, "tool_name": "Write",
                        "tool_input": {"file_path": here, "content": "hi"}},
        "posttooluse-bash": {"cwd": cwd, "session_id": sid, "tool_name": "Bash",
                             "tool_input": {"command": "ls -la"}},
        "posttooluse-skill": {"cwd": cwd, "session_id": sid, "tool_name": "Skill",
                              "tool_input": {"skill": "brainstorming"}},
        "posttooluse-dispatch": {"cwd": cwd, "session_id": sid, "tool_name": "Task",
                                 "tool_input": {"subagent_type": "reviewer"},
                                 "tool_response": "agentId: a1b2c3d4e5f6"},
        "posttooluse-message": {"cwd": cwd, "session_id": sid, "tool_name": "SendMessage",
                                "tool_input": {"to": "reviewer"}},
    }


#: Commands and reads that must still be refused with no plane anywhere. Each is a fact
#: about the SHELL or about a secret, which is why `charter/hooks.py` marks each of them
#: ungated on purpose — not a policy this plane happens to hold.
#:
#: The expected fragments are typed out here rather than imported: a test that reads the
#: constant it is checking goes green against a constant edited to nothing.
#:
#: ``(the guard's name, hook, tool_input, a phrase its refusal must carry)``.
_MUST_STILL_REFUSE = (
    ("A: vault leak on Bash", "pretooluse",
     {"command": "cat .charter/vaults/db.json"}, "would print plaintext"),
    ("A5: live substitution on a forge command", "pretooluse",
     {"command": 'gh issue create --body "$(cat notes.md)"'},
     "publishes prose a reader sees"),
    ("A6: live substitution on charter's own", "pretooluse",
     {"command": 'charter workspace remember "$(cat notes.md)"'},
     "takes text charter PERSISTS"),
    ("A on the Read/Grep route", "pretooluse-read",
     {"file_path": ".charter/vaults/db.json"}, "would print plaintext"),
)


def _tree(root: Path) -> dict[str, bytes | None]:
    """Every path under *root*, with file contents. ``None`` marks a directory.

    Contents, not just names: `guard-seen.json` and `<sid>.memnudge` are overwritten in
    place, so a name-only snapshot would call a second write no write at all.
    """
    out: dict[str, bytes | None] = {}
    for p in sorted(root.rglob("*")):
        rel = str(p.relative_to(root))
        if p.is_dir():
            out[rel] = None
        else:
            try:
                out[rel] = p.read_bytes()
            except OSError:
                out[rel] = b"<unreadable>"
    return out


class StrangersRepo(PersonaIso):
    """A directory that is NOT a plane, standing in for somebody else's checkout.

    `PersonaIso` deliberately writes no ``charter.toml``, so its root is already one — but
    that is asserted rather than trusted, because every case in this file is vacuous the
    day it stops being true.
    """

    def setUp(self) -> None:
        super().setUp()
        # Trap: a charter worktree is not an isolated plane — `config.ROOT`/`STATE_DIR`
        # follow the MAIN working tree via `charter/root.py`. Nothing below writes unless
        # this holds, because if it ever does, it writes into the operator's real plane.
        self.assertIn("edm-test-", str(config.STATE_DIR),
                      "isolation is broken: this case would write into a real plane")
        self.cwd = str(config.ROOT)
        self.sid = "s852"
        # A persona the two dispatch tallies can actually name. Without it
        # `posttooluse-dispatch` and `posttooluse-message` return before the write they are
        # here to prove, and the property passes for the wrong reason.
        self.make_persona("reviewer", role="Reviews code", vault="none")
        # git, not the property. `test_dispatch.py` stubs the same call for the same
        # reason; what this file is about is the tally FILE, which `dispatch.record`
        # writes before this is ever reached.
        self.enterContext(mock.patch.object(hooks, "_commit_dispatch"))
        self.before = _tree(config.ROOT)

    def assertUntouched(self, why: str) -> None:
        after = _tree(config.ROOT)
        added = sorted(set(after) - set(self.before))
        changed = sorted(k for k in set(after) & set(self.before)
                         if after[k] != self.before[k])
        self.assertEqual(([], []), (added, changed),
                         f"{why}: charter wrote into a repo it does not own "
                         f"(new: {added}, changed: {changed})")

    def payloads(self) -> dict[str, dict]:
        return _payloads(self.cwd, self.sid)


class NothingIsWritten(StrangersRepo):
    def test_every_handler_is_covered_by_a_payload(self):
        """The property is driven off `_HANDLERS`, so a handler added tomorrow inherits
        it. This is the line that makes that true rather than aspirational: a new handler
        with no payload fails here, naming itself."""
        self.assertEqual(sorted(hooks._HANDLERS), sorted(self.payloads()))

    def test_no_handler_leaves_a_trace_in_a_repo_charter_does_not_own(self):
        for name, fn in sorted(hooks._HANDLERS.items()):
            with self.subTest(hook=name):
                self.before = _tree(config.ROOT)
                _run(fn, self.payloads()[name])
                self.assertUntouched(f"`charter hook {name}`")

    def test_a_refusal_is_delivered_without_being_tallied(self):
        """The sharpest case, because it is where quiet and armed pull against each other.

        A denial outside a plane still goes out — :class:`TheGuardsStillRefuse` pins that.
        What must not go out is the BOOKKEEPING: on 0.55.0 refusing `cat
        .charter/vaults/db.json` in a stranger's repo wrote `.charter/guard-seen.json` and
        `.charter/persona-state/trace/<sid>.jsonl` there. A tally is read by `charter
        persona stats` against a plane, and there is no plane here to read it.
        """
        for name, hook, ti, _ in _MUST_STILL_REFUSE:
            with self.subTest(guard=name):
                self.before = _tree(config.ROOT)
                payload = dict(self.payloads()[hook])
                payload["tool_input"] = ti
                _run(hooks._HANDLERS[hook], payload)
                self.assertUntouched(f"refusing via `{hook}`")


class NothingIsSaid(StrangersRepo):
    def test_no_handler_speaks_into_a_session_with_no_plane_to_act_on(self):
        for name, fn in sorted(hooks._HANDLERS.items()):
            with self.subTest(hook=name):
                rc, out = _run(fn, self.payloads()[name])
                self.assertEqual("", out, f"`charter hook {name}` injected context")
                self.assertEqual(0, rc)

    def test_the_workspace_quiz_is_not_opened_where_there_are_no_workspaces(self):
        """Named separately from the sweep above because it is the reported symptom, and a
        sweep that went green for some structural reason would hide it."""
        _, out = _run(hooks.sessionstart, self.payloads()["sessionstart"])
        self.assertNotIn("AskUserQuestion", out)
        self.assertNotIn("workspace", out.lower())

    def test_a_repos_own_files_do_not_decide_which_commands_skip_the_prompt(self):
        """`personas/<n>/persona.md` and `.charter/active-persona` are charter's files
        inside a plane. Outside one they are just contents of the checkout you cloned, and
        `toolgate.decide` read them anyway — answering `allow` for a command the harness
        would otherwise have prompted on."""
        d = config.PERSONAS_DIR / "rogue"
        d.mkdir(parents=True, exist_ok=True)
        (d / "persona.md").write_text(
            "---\nname: rogue\nrole: not charter's\ntools: [curl]\n---\n\nbody\n")
        config.private_mkdir(Path(config.STATE_DIR))
        Path(config.ACTIVE_PERSONA_FILE).write_text("rogue\n")
        payload = dict(self.payloads()["pretooluse"])
        payload["tool_input"] = {"command": "curl https://evil.example/x"}
        _, out = _run(hooks.pretooluse, payload)
        self.assertNotIn("allow", out)


class TheGuardsStillRefuse(StrangersRepo):
    """Quiet is not the same as disarmed, and this is the half that says so."""

    def test_the_ungated_refusals_still_deny_with_no_plane_anywhere(self):
        for name, hook, ti, fragment in _MUST_STILL_REFUSE:
            with self.subTest(guard=name):
                payload = dict(self.payloads()[hook])
                payload["tool_input"] = ti
                _, out = _run(hooks._HANDLERS[hook], payload)
                self.assertTrue(out, f"`{hook}` said nothing at all")
                verdict = json.loads(out)["hookSpecificOutput"]
                self.assertEqual("deny", verdict["permissionDecision"])
                self.assertIn(fragment, verdict["permissionDecisionReason"])

    def test_a_plane_only_denial_is_still_silent_outside_a_plane(self):
        """The precondition that separates "the guards fire" from "everything fires". A2
        was gated in 0.42 precisely because denying an SSH clone in an unrelated repo
        explains a control plane that does not exist there; a change that armed everything
        outside a plane would pass the case above and break this one."""
        payload = dict(self.payloads()["pretooluse"])
        payload["tool_input"] = {"command": "git clone git@github.com:o/r.git"}
        _, out = _run(hooks.pretooluse, payload)
        self.assertEqual("", out)


class InsideAPlaneNothingIsLost(StrangersRepo):
    """The same payloads, in a real plane. Every assertion here is the exact opposite of
    one above, so "gated" cannot quietly become "deleted"."""

    def setUp(self) -> None:
        super().setUp()
        make_plane(self)
        self.assertTrue(config.HAS_CONTROL_PLANE)
        self.assertIn("edm-test-", str(config.STATE_DIR))
        self.before = _tree(config.ROOT)

    def test_the_guard_still_records_that_it_ran(self):
        _run(hooks.pretooluse, self.payloads()["pretooluse"])
        self.assertTrue((Path(config.STATE_DIR) / "guard-seen.json").is_file())

    def test_session_start_still_asks_which_workspace(self):
        _, out = _run(hooks.sessionstart, self.payloads()["sessionstart"])
        self.assertIn("AskUserQuestion", out)

    def test_the_memory_cadence_is_still_counted(self):
        _run(hooks.posttooluse, self.payloads()["posttooluse"])
        self.assertTrue((Path(config.SESSIONS_DIR) / f"{self.sid}.memnudge").is_file())

    def test_a_dispatch_is_still_tallied(self):
        _run(hooks.posttooluse_dispatch, self.payloads()["posttooluse-dispatch"])
        self.assertTrue(list((config.PERSONAS_DIR / "_dispatch").glob("*.jsonl")))

    def test_a_declared_tool_is_still_auto_approved(self):
        d = config.PERSONAS_DIR / "smoother"
        d.mkdir(parents=True, exist_ok=True)
        (d / "persona.md").write_text(
            "---\nname: smoother\nrole: smooths\ntools: [curl]\n---\n\nbody\n")
        config.private_mkdir(Path(config.STATE_DIR))
        Path(config.ACTIVE_PERSONA_FILE).write_text("smoother\n")
        payload = dict(self.payloads()["pretooluse"])
        payload["tool_input"] = {"command": "curl https://example.com/x"}
        _, out = _run(hooks.pretooluse, payload)
        self.assertIn("allow", out)

    def test_the_ungated_refusals_are_unchanged_inside_a_plane(self):
        """Trap: gating the plane work must not move which branch the guard chain reaches.
        These are the same four cases as outside, asserted to answer identically."""
        for name, hook, ti, fragment in _MUST_STILL_REFUSE:
            with self.subTest(guard=name):
                payload = dict(self.payloads()[hook])
                payload["tool_input"] = ti
                _, out = _run(hooks._HANDLERS[hook], payload)
                verdict = json.loads(out)["hookSpecificOutput"]
                self.assertEqual("deny", verdict["permissionDecision"])
                self.assertIn(fragment, verdict["permissionDecisionReason"])

    def test_the_plane_only_denial_fires_here(self):
        """The other half of `test_a_plane_only_denial_is_still_silent_outside_a_plane`:
        A2 is quiet out there because there is no plane, not because it stopped working."""
        payload = dict(self.payloads()["pretooluse"])
        payload["tool_input"] = {"command": "git clone git@github.com:o/r.git"}
        _, out = _run(hooks.pretooluse, payload)
        self.assertEqual("deny", json.loads(out)["hookSpecificOutput"]["permissionDecision"])


if __name__ == "__main__":
    unittest.main()
