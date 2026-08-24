"""A denial charter cannot WRITE is not an allow (#438).

`_deny` refuses by printing one JSON object on stdout. A hook that prints nothing has said
nothing, and a `PreToolUse` hook that says nothing is an ALLOW — so every way the write can
fail was a way the guard failed **open**:

* `pretooluse_read` wrapped its whole body, `_deny` included, in `except Exception: return
  0`. A `BrokenPipeError` out of `print` is an `Exception`; the vault read went through.
* `pretooluse`, the sibling guard on the same invariant, had no wrapper — so the same
  exception propagated to `cli.main`, which quite correctly turns `BrokenPipeError` into
  141 for `charter … | head`. 141 is a *non-blocking* hook error: the tool call proceeds.

Two guards on one invariant failing in opposite directions, in a module that argues at
length that they must never disagree. Reachability is low (the bodies are dict gets, a
`str()` and a regex) and the direction is the whole point: the fix is not "make this body
not raise", it is "a decided denial reaches the harness by SOME channel or the process
exits refusing".

The other channel is the exit status. 2 blocks the tool call with stderr as the reason;
every other non-zero status is a non-blocking error and the call goes ahead — which is why
these assert on the number and not merely on "non-zero".

Property under test, stated so the tests can be checked against it: **whenever a guard
decides to deny and the JSON channel is unusable, the hook process refuses.** Not "the
vault guard does not crash".
"""

from __future__ import annotations

import io
import json
import sys
import unittest
from contextlib import redirect_stderr, redirect_stdout
from unittest import mock

from charter import config, hooks
from tests._isolation import PersonaIso

#: A payload that MUST be denied, per handler that can deny one.
#: The Bash guard's entry is the secret-leak rule, which needs no control plane and no repo
#: — `_leak_reason` is unconditional on purpose (see `pretooluse`), so this table stays
#: about the channel rather than about any one guard's preconditions.
DENIALS = {
    "pretooluse-read": {"tool_name": "Read",
                        "tool_input": {"file_path": ".charter/vaults/devops.json"},
                        "session_id": "s"},
    "pretooluse": {"tool_name": "Bash",
                   "tool_input": {"command": "cat .charter/vaults/devops.json"},
                   "session_id": "s"},
    "pretooluse-edit": {"tool_name": "Write",
                        "tool_input": {"file_path": "STATE_DIR/vaults.json",
                                       "content": "{}"},
                        "session_id": "s"},
}


class BrokenChannelCase(PersonaIso):
    """Every case here drives `hooks.dispatch`, the real process entrypoint, because the
    exit status is the thing under test and a handler's return value only becomes an exit
    status there."""

    def setUp(self) -> None:
        super().setUp()
        (self.tmp / "charter.toml").write_text("schema = 1\n")
        config.HAS_CONTROL_PLANE = True
        # Module state, and this suite drives the handlers in ONE process — a row left by
        # one case would turn an unrelated later hook into a refusal.
        hooks._undelivered_deny.clear()
        self.addCleanup(hooks._undelivered_deny.clear)

    def dispatch(self, name: str, payload: dict, broken: bool = False):
        """Run one hook the way `charter hook <name>` does; return (rc, stdout, stderr).

        *broken* replaces `builtins.print` — the lowest layer every emission in this module
        reaches, `_emit`'s own call — rather than `_emit` or `_deny`, so a future emitter
        that spells the write differently is still covered, and so the test cannot pass by
        stubbing out the code it is meant to exercise. Only **stdout** is broken, which is
        the real condition: a reader that went away closes the one pipe, and stderr is a
        different file descriptor that keeps working. A blunter stub that broke both would
        make the stderr assertion below unfalsifiable.
        """
        payload = dict(payload)
        ti = dict(payload.get("tool_input") or {})
        if str(ti.get("file_path", "")).startswith("STATE_DIR/"):
            ti["file_path"] = str(config.STATE_DIR / ti["file_path"].split("/", 1)[1])
            payload["tool_input"] = ti
        out, err = io.StringIO(), io.StringIO()
        old = sys.stdin
        sys.stdin = io.StringIO(json.dumps(payload))
        real_print = print
        def broken_print(*args, **kw):
            if kw.get("file") in (None, sys.stdout):
                raise BrokenPipeError(32, "Broken pipe")
            return real_print(*args, **kw)
        try:
            with redirect_stdout(out), redirect_stderr(err):
                if broken:
                    with mock.patch("builtins.print", side_effect=broken_print):
                        rc = hooks.dispatch(name, None)
                else:
                    rc = hooks.dispatch(name, None)
        finally:
            sys.stdin = old
        return rc, out.getvalue(), err.getvalue()


class TestABrokenPipeDoesNotTurnADenyIntoAnAllow(BrokenChannelCase):
    def test_every_denying_hook_refuses_by_exit_status(self):
        """The load-bearing one, and a table rather than one case: the audit found this in
        the vault guard, and the defect is the shape, not the guard."""
        for name, payload in DENIALS.items():
            with self.subTest(hook=name):
                hooks._undelivered_deny.clear()
                rc, _, _ = self.dispatch(name, payload, broken=True)
                self.assertEqual(rc, hooks.DENY_EXIT, name)

    def test_the_status_is_the_one_the_harness_reads_as_blocking(self):
        """A contract pin, not a behaviour test, and said plainly rather than dressed up:
        the number cannot be derived from anything in this repo, it is the harness's. 2
        blocks the tool call with stderr as the reason; **every** other non-zero status is a
        non-blocking error and the call proceeds — so an assertion of merely "non-zero"
        would have passed on 1, on the 120 a failed shutdown flush produces, and on
        `cli.main`'s 141-for-SIGPIPE, which are the three numbers this issue is about."""
        self.assertEqual(hooks.DENY_EXIT, 2)
        self.assertNotIn(hooks.DENY_EXIT, (0, 1, 120, 141))

    def test_the_backstop_covers_a_call_site_that_drops_the_status(self):
        """The redundancy, tested as itself. Returning `_deny`'s status is a discipline
        every call site has to keep; `dispatch` refusing on an undelivered denial is the
        part that holds when one does not — and a guard written after this line is exactly
        the one nobody will remember to check."""
        def forgetful():
            hooks._deny("PreToolUse", "a guard that drops the status")
            return 0

        with mock.patch.dict(hooks._HANDLERS, {"pretooluse-read": forgetful}):
            rc, _, _ = self.dispatch("pretooluse-read", DENIALS["pretooluse-read"],
                                     broken=True)
        self.assertEqual(rc, hooks.DENY_EXIT)

    def test_the_reason_still_reaches_the_model_on_stderr(self):
        """Exit 2 hands stderr to the model. A refusal with no reason is the bare "no" this
        module's guards are written not to give."""
        rc, _, err = self.dispatch("pretooluse-read", DENIALS["pretooluse-read"], broken=True)
        self.assertEqual(rc, hooks.DENY_EXIT)
        self.assertIn("secret exec", err)

    def test_the_deny_helper_reports_the_status_to_its_caller(self):
        """One layer down: `_deny` is what every guard calls, and its return value is what a
        handler propagates. Pinned separately so a handler that starts discarding it fails
        here rather than only through the process boundary."""
        with redirect_stdout(io.StringIO()), redirect_stderr(io.StringIO()):
            self.assertEqual(hooks._deny("PreToolUse", "x"), 0)
            with mock.patch("builtins.print",
                            side_effect=BrokenPipeError(32, "Broken pipe")):
                self.assertEqual(hooks._deny("PreToolUse", "x"), hooks.DENY_EXIT)


class TestItDidNotBreakTheWorkingCase(BrokenChannelCase):
    def test_an_intact_channel_still_denies_as_json_at_exit_zero(self):
        """Guards the guard. A fix that made every denial exit 2 would pass every test
        above and change what a denial IS — the JSON verdict is the normal channel, and it
        carries the reason the agent reads."""
        for name, payload in DENIALS.items():
            with self.subTest(hook=name):
                hooks._undelivered_deny.clear()
                rc, out, _ = self.dispatch(name, payload)
                self.assertEqual(rc, 0, name)
                decision = json.loads(out)["hookSpecificOutput"]["permissionDecision"]
                self.assertEqual(decision, "deny", name)

    def test_an_allowed_call_is_untouched_by_a_broken_channel(self):
        """The other half of the direction: a hook with nothing to say must not start
        refusing because a write it never made would have failed."""
        rc, _, _ = self.dispatch("pretooluse-read",
                                 {"tool_name": "Read",
                                  "tool_input": {"file_path": "README.md"},
                                  "session_id": "s"},
                                 broken=True)
        self.assertEqual(rc, 0)

    def test_a_later_hook_does_not_inherit_an_earlier_refusal(self):
        """`_undelivered_deny` is module state. If it leaked between dispatches, the test
        above would pass for the wrong reason and a real session's next hook would refuse
        an unrelated tool call."""
        self.dispatch("pretooluse-read", DENIALS["pretooluse-read"], broken=True)
        rc, _, _ = self.dispatch("pretooluse-read",
                                 {"tool_name": "Read",
                                  "tool_input": {"file_path": "README.md"},
                                  "session_id": "s"})
        self.assertEqual(rc, 0)


class TestTheParseIsStillAllowedToFail(BrokenChannelCase):
    """The `except` in `pretooluse_read` was not deleted, it was NARROWED — deciding is
    fallible and stays excused; refusing is not. A fix that removed the wrapper outright
    would make a malformed payload break the turn, which is the failure mode the rest of
    this module is written to avoid."""

    def test_a_payload_that_cannot_be_read_is_not_a_broken_turn(self):
        class Hostile:
            def __contains__(self, _):
                raise RuntimeError("boom")

            def get(self, *_a, **_k):
                raise RuntimeError("boom")

        with mock.patch.object(hooks, "_CONTENT_TOOLS", Hostile()):
            rc, out, _ = self.dispatch("pretooluse-read", DENIALS["pretooluse-read"])
        self.assertEqual(rc, 0)
        self.assertEqual(out.strip(), "")


class TestNoOtherHookHidesADenyInsideAnExcept(unittest.TestCase):
    """`pretooluse_read` was the only one, and this is what keeps that true.

    Structural rather than behavioural on purpose: a fail-open is invisible from the
    outside — it looks exactly like the guard being present and never firing — so the shape
    is pinned where it can be introduced. `dispatch`'s backstop covers a `_deny` whose
    status a new call site forgets to return; this covers the other spelling, a `_deny`
    sitting inside a `try` that swallows it.
    """

    def test_every_deny_call_is_inside_a_function_that_returns_its_status(self):
        import ast
        import inspect

        src = inspect.getsource(hooks)
        tree = ast.parse(src)
        # Collect the `_deny(...)` calls that are NOT bound to a name — a bare expression
        # statement discards the status, which is the shape that fails open.
        discarded = [n.lineno for n in ast.walk(tree)
                     if isinstance(n, ast.Expr) and isinstance(n.value, ast.Call)
                     and isinstance(n.value.func, ast.Name) and n.value.func.id == "_deny"]
        self.assertEqual(discarded, [], f"_deny status discarded at lines {discarded}")

    def test_no_try_block_encloses_a_deny_call(self):
        import ast
        import inspect

        tree = ast.parse(inspect.getsource(hooks))
        bad = []
        for node in ast.walk(tree):
            if not isinstance(node, ast.Try):
                continue
            for handler in node.handlers:
                del handler
            for child in ast.walk(ast.Module(body=node.body, type_ignores=[])):
                if (isinstance(child, ast.Call) and isinstance(child.func, ast.Name)
                        and child.func.id == "_deny"):
                    bad.append(child.lineno)
        self.assertEqual(bad, [], f"_deny called inside a try/except at lines {bad}")


if __name__ == "__main__":
    unittest.main()
