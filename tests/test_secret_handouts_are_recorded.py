"""Every credential charter hands out is recorded, and no record holds a value (#441).

`charter trace` knew about `secret-warn` — the scanner that spots a value in a file about
to be committed — and about nothing charter itself handed over. So the question an
operator asks after a bad afternoon, *"which command received the prod token"*, had no
answer anywhere in the plane. `secret audit` is a rotation-age report, not access logging.

**Two properties, and they pull in opposite directions**, which is why both are pinned here
rather than one being assumed from the other:

1. *Completeness* — every route by which a plaintext leaves charter's process records that
   it did. There are three: a child's environment or a temp file (`exec`, in each of its
   three launch modes), a file on disk (`cp`), and a terminal (`get --reveal`).
2. *Emptiness* — no record holds a value. A trace file lives inside the plane and is
   readable by the agent whose behaviour it records, so a record of a secret that copied
   the secret would be a fresh leak wearing an audit trail's clothes.

**Where the assertions are made.** Emptiness is asserted against the **bytes of the trace
file**, never against the parsed fields. A field-by-field check is a list of the spellings
somebody thought of, and the next field added is the one that is not on it; the file is the
one place every spelling must arrive. `TraceHoldsNoValue` therefore also drives the
recorder directly with a value buried in a nested dict, a tuple and a list — shapes no
caller passes today, so that the rule under test is about shapes and not about the fields
`cmd_secret_exec` happens to record this month.

**The values here are fabricated** and are constructed to be unmissable in a diff of the
trace file. Nothing in this module asserts on a whole environment or a whole argv — that is
how a suite log came to hold live 1Password tokens — and no assertion message is given the
file's contents to print.

`PersonaIso` throughout: the trace writes through `config.PERSONA_STATE_DIR`, and a fixture
that does not redirect it appends to the developer's own plane (#372, #402).
"""

from __future__ import annotations

import json
import os
import stat
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from charter import commands_secrets, config, trace
from tests._isolation import PersonaIso

#: Fabricated. Long, unique, and shaped so a partial copy is still recognisable.
_VALUE = "FABRICATED-SECRET-VALUE-0f1e2d3c4b5a-DO-NOT-LOG"
_OTHER = "FABRICATED-SECOND-VALUE-9a8b7c6d5e4f-DO-NOT-LOG"

#: The one session every test in this file writes to and reads back.
_SESSION = "handout-session"


class _StubProvider:
    """A vault that answers with fabricated values and remembers what was asked."""

    def __init__(self, values: dict[str, str]):
        self.values = values
        self.asked: list[str] = []

    def get(self, key: str) -> str:
        self.asked.append(key)
        return self.values[key]

    def keys(self):
        return sorted(self.values)


class _HandoutCase(PersonaIso):
    def setUp(self) -> None:
        super().setUp()
        self.provider = _StubProvider({"prod-token": _VALUE, "prod-user": _OTHER})
        self.enterContext(mock.patch.object(commands_secrets, "_provider",
                                            lambda _name: self.provider))
        # A fixed session id so `trace.read` looks in the file the handlers wrote, without
        # depending on whatever `$CHARTER_SESSION_ID` the developer's shell carries — an
        # ambient value is how a test comes to pass for a reason it does not state.
        self.enterContext(mock.patch.dict(os.environ,
                                          {"CHARTER_SESSION_ID": _SESSION,
                                           "CLAUDE_CODE_SESSION_ID": _SESSION}))

    # -- reading the record ------------------------------------------------- #
    def events(self, kind: str | None = None) -> list[dict]:
        evs = trace.read(_SESSION)
        return [e for e in evs if kind is None or e["event"] == kind]

    def trace_bytes(self) -> str:
        """The raw trace file. The lowest layer every recorded field must pass through."""
        f = config.PERSONA_STATE_DIR / "trace" / f"{_SESSION}.jsonl"
        return f.read_text() if f.exists() else ""

    def assert_no_value_in_the_trace(self) -> None:
        """Neither fabricated value appears anywhere in the file.

        Asserted with `assertNotIn`'s message suppressed: a failing `assertNotIn` prints
        the haystack, and the haystack in the real defect this guards is a live credential.
        """
        text = self.trace_bytes()
        for name, value in (("prod-token", _VALUE), ("prod-user", _OTHER)):
            self.assertTrue(
                value not in text,
                f"the value of '{name}' reached the trace file — "
                f"contents deliberately not printed")

    @staticmethod
    def exec_args(**kw):
        base = {"vault": "prod", "env": None, "file": None, "dotenv": None,
                "command": [], "exec_mode": False, "stream_mode": False}
        base.update(kw)
        return SimpleNamespace(**base)


class SecretExecIsRecorded(_HandoutCase):
    """#441's own case: `cmd_secret_exec` against a fabricated vault."""

    def setUp(self) -> None:
        super().setUp()
        self.runs: list[list[str]] = []

        def fake_run(argv, **kw):
            self.runs.append(list(argv))
            return SimpleNamespace(stdout="", stderr="", returncode=0)

        self.enterContext(mock.patch.object(commands_secrets.subprocess, "run", fake_run))

    def test_the_trace_holds_the_vault_the_key_names_and_argv0(self):
        commands_secrets.cmd_secret_exec(self.exec_args(
            env=["PROD_TOKEN=prod-token"], command=["--", "/usr/bin/env"]))
        evs = self.events("secret-exec")
        self.assertEqual(len(evs), 1, "exactly one hand-out event per run")
        e = evs[0]
        self.assertEqual(e["vault"], "prod")
        self.assertEqual(e["key_names"], ["prod-token"])
        self.assertEqual(e["env_names"], ["PROD_TOKEN"])
        self.assertEqual(e["argv0"], "/usr/bin/env")
        self.assertEqual(e["mode"], "capture")

    def test_the_trace_does_not_hold_the_value(self):
        commands_secrets.cmd_secret_exec(self.exec_args(
            env=["PROD_TOKEN=prod-token", "PROD_USER=prod-user"],
            command=["--", "/usr/bin/env"]))
        self.assertTrue(self.provider.asked,
                        "PRECONDITION: the vault must actually have been read, or "
                        "'no value in the trace' proves only that none was resolved")
        self.assert_no_value_in_the_trace()

    def test_the_rest_of_argv_is_not_recorded(self):
        """charter never substitutes a value into a command line — but a caller may have
        typed one there, and a record whose purpose is to hold no values must not copy a
        line that might."""
        argv = ["/bin/sh", "-c", f"echo {_VALUE}"]
        commands_secrets.cmd_secret_exec(self.exec_args(
            env=["PROD_TOKEN=prod-token"], command=["--", *argv]))
        self.assertEqual(self.runs, [argv],
                         "PRECONDITION: the whole command must really have been run")
        self.assert_no_value_in_the_trace()
        e = self.events("secret-exec")[0]
        self.assertEqual(e["argv0"], argv[0])
        # Every element PAST argv[0], asserted absent from the whole record rather than
        # from a field named in advance — the point is that none of them is anywhere.
        recorded = json.dumps(e)
        for element in argv[1:]:
            self.assertTrue(element not in recorded,
                            f"argv[{argv.index(element)}] reached the trace record")

    def test_a_run_that_resolves_nothing_is_still_recorded(self):
        """A `secret exec` with no `--env`/`--file`/`--dotenv` hands out no credential, but
        it does run a command inside the vault machinery. A trace that showed the
        credentialed runs and hid the others answers "what did this vault do" with a
        filtered list that reads as a whole one."""
        commands_secrets.cmd_secret_exec(self.exec_args(command=["--", "/bin/true"]))
        evs = self.events("secret-exec")
        self.assertEqual([e["key_names"] for e in evs], [[]])
        self.assertEqual(evs[0]["argv0"], "/bin/true")

    def test_a_vault_that_refuses_records_no_handout(self):
        """Nothing was handed out, so nothing is claimed. The event means "this command
        received these credentials", and a run that resolved none did not."""
        from charter.secrets import base

        def boom(_key):
            raise base.VaultError("no such key")

        with mock.patch.object(self.provider, "get", boom):
            rc = commands_secrets.cmd_secret_exec(self.exec_args(
                env=["PROD_TOKEN=prod-token"], command=["--", "/bin/true"]))
        self.assertEqual(rc, 1)
        self.assertEqual(self.events("secret-exec"), [])

    def test_the_record_is_written_before_the_child_runs(self):
        """Placement, asserted rather than assumed. `--exec` replaces the process, so a
        record written after the launch is a record never written; and a child that dies on
        its first instruction still received the credentials."""
        seen: list[int] = []

        def fake_run(argv, **kw):
            seen.append(len(self.events("secret-exec")))
            return SimpleNamespace(stdout="", stderr="", returncode=0)

        with mock.patch.object(commands_secrets.subprocess, "run", fake_run):
            commands_secrets.cmd_secret_exec(self.exec_args(
                env=["PROD_TOKEN=prod-token"], command=["--", "/bin/true"]))
        self.assertEqual(seen, [1],
                         "the hand-out must be on record before the child is launched")


class EveryLaunchModeRecords(_HandoutCase):
    """The completeness half. Three modes launch a child, and all three go through one
    recording site — so a fourth mode added below that site is recorded by construction
    rather than by somebody remembering."""

    def _dotenv_dir(self):
        return str(self.tmp)

    def test_capture_mode(self):
        with mock.patch.object(commands_secrets.subprocess, "run",
                               lambda argv, **kw: SimpleNamespace(
                                   stdout="", stderr="", returncode=0)):
            commands_secrets.cmd_secret_exec(self.exec_args(
                env=["PROD_TOKEN=prod-token"], command=["--", "/bin/true"]))
        self.assertEqual([e["mode"] for e in self.events("secret-exec")], ["capture"])
        self.assert_no_value_in_the_trace()

    def test_stream_mode(self):
        with mock.patch.object(commands_secrets.subprocess, "run",
                               lambda argv, **kw: SimpleNamespace(returncode=0)):
            commands_secrets.cmd_secret_exec(self.exec_args(
                env=["PROD_TOKEN=prod-token"], command=["--", "/bin/true"],
                stream_mode=True))
        self.assertEqual([e["mode"] for e in self.events("secret-exec")], ["stream"])
        self.assert_no_value_in_the_trace()

    def test_exec_mode(self):
        with mock.patch.object(commands_secrets.os, "execvpe", lambda f, a, e: None):
            commands_secrets.cmd_secret_exec(self.exec_args(
                env=["PROD_TOKEN=prod-token"], command=["--", "/bin/true"],
                exec_mode=True))
        self.assertEqual([e["mode"] for e in self.events("secret-exec")], ["exec"])
        self.assert_no_value_in_the_trace()

    def test_a_file_credential_names_its_key_and_its_variable(self):
        with mock.patch.object(commands_secrets.subprocess, "run",
                               lambda argv, **kw: SimpleNamespace(
                                   stdout="", stderr="", returncode=0)):
            commands_secrets.cmd_secret_exec(self.exec_args(
                file=["GOOGLE_APPLICATION_CREDENTIALS=prod-token"],
                command=["--", "/bin/true"]))
        e = self.events("secret-exec")[0]
        self.assertEqual(e["key_names"], ["prod-token"])
        self.assertEqual(e["env_names"], ["GOOGLE_APPLICATION_CREDENTIALS"])
        self.assert_no_value_in_the_trace()

    def test_a_dotenv_credential_names_every_key_it_merged(self):
        with mock.patch.object(commands_secrets.subprocess, "run",
                               lambda argv, **kw: SimpleNamespace(
                                   stdout="", stderr="", returncode=0)):
            commands_secrets.cmd_secret_exec(self.exec_args(
                dotenv=["SECRETS_FILE=USER:prod-user", "SECRETS_FILE=TOKEN:prod-token"],
                command=["--", "/bin/true"]))
        e = self.events("secret-exec")[0]
        self.assertEqual(e["key_names"], ["prod-token", "prod-user"])
        self.assertEqual(e["env_names"], ["SECRETS_FILE"])
        self.assert_no_value_in_the_trace()


class SecretCpIsRecorded(_HandoutCase):
    """`secret cp` materialises a value as a file. The record names the destination —
    which is the thing an operator has to go and delete."""

    def test_a_written_credential_is_recorded_with_its_destination(self):
        dest = self.tmp / "out" / "kubeconfig"
        rc = commands_secrets.cmd_secret_cp(SimpleNamespace(
            vault="prod", key="prod-token", dest=str(dest), force=False))
        self.assertEqual(rc, 0)
        self.assertEqual(dest.read_text(), _VALUE,
                         "PRECONDITION: the credential must really have been written")
        self.assertEqual(stat.S_IMODE(dest.stat().st_mode), 0o600)
        e = self.events("secret-cp")
        self.assertEqual(len(e), 1)
        self.assertEqual((e[0]["vault"], e[0]["key_names"]), ("prod", ["prod-token"]))
        self.assertEqual(e[0]["dest"], str(dest))
        self.assert_no_value_in_the_trace()

    def test_a_refused_destination_records_nothing(self):
        """The refusal path never resolves the value, so there is no hand-out to record —
        and a record here would report a credential written to a file that does not
        contain one."""
        target = self.tmp / "already-there"
        target.write_text("existing\n")
        rc = commands_secrets.cmd_secret_cp(SimpleNamespace(
            vault="prod", key="prod-token", dest=str(target), force=False))
        self.assertEqual(rc, 2)
        self.assertEqual(self.events("secret-cp"), [])
        self.assertEqual(self.provider.asked, [],
                         "a refused destination must not even read the vault")


class SecretRevealIsRecorded(_HandoutCase):
    """The third route out, and the one an incomplete fix would have left silent.

    Recording `exec` and `cp` but not `--reveal` would be worse than recording none: an
    operator who greps for the two, finds nothing and concludes the token never left the
    vault has been misled by a record that looks complete.
    """

    @staticmethod
    def _args(**kw):
        base = {"vault": "prod", "key": "prod-token", "reveal": True, "force": True}
        base.update(kw)
        return SimpleNamespace(**base)

    def test_reveal_records_the_handout(self):
        commands_secrets.cmd_secret_get(self._args())
        e = self.events("secret-reveal")
        self.assertEqual(len(e), 1)
        self.assertEqual((e[0]["vault"], e[0]["key_names"]), ("prod", ["prod-token"]))
        self.assert_no_value_in_the_trace()

    def test_the_masked_form_records_nothing(self):
        """`secret get` without `--reveal` prints a length and a digest. Nothing left, so
        nothing is recorded — an event here would report a hand-out that did not happen."""
        commands_secrets.cmd_secret_get(self._args(reveal=False))
        self.assertEqual(self.events("secret-reveal"), [])

    def test_a_refused_reveal_records_nothing(self):
        """Refused for non-interactive stdout: the value was resolved but never left."""
        commands_secrets.cmd_secret_get(self._args(force=False))
        self.assertEqual(self.events("secret-reveal"), [])


class TraceHoldsNoValue(_HandoutCase):
    """The emptiness half, tested at the recorder rather than at its callers.

    Everything above pins the fields `cmd_secret_exec` records TODAY. This pins the rule
    that will still be here when somebody adds a field: whatever shape is handed to
    `_trace_secret_use`, a value this call resolved does not reach the file.
    """

    def test_a_value_buried_in_any_shape_is_removed(self):
        for label, payload in (
            ("a plain string", _VALUE),
            ("inside a longer string", f"Authorization: Bearer {_VALUE}"),
            ("a list", ["a", _VALUE, "b"]),
            ("a tuple", ("a", _VALUE)),
            ("a dict value", {"header": _VALUE}),
            ("a dict key", {_VALUE: "header"}),
            ("nested two deep", {"outer": [{"inner": (_VALUE,)}]}),
        ):
            with self.subTest(shape=label):
                commands_secrets._trace_secret_use(
                    "secret-exec", [_VALUE], vault="prod", probe=payload)
                self.assert_no_value_in_the_trace()

    def test_the_scrub_is_not_vacuous(self):
        """The positive control. Without it every case above would also pass against a
        recorder that wrote nothing at all, or against a `_value_free` that returned the
        empty string for everything."""
        commands_secrets._trace_secret_use(
            "secret-exec", [_VALUE], vault="prod", probe={"outer": [{"inner": [_VALUE]}]})
        e = self.events("secret-exec")[0]
        self.assertEqual(e["probe"], {"outer": [{"inner": ["***"]}]},
                         "the shape must survive; only the value is removed")
        self.assertEqual(e["vault"], "prod")

    def test_a_field_carrying_no_value_is_left_alone(self):
        commands_secrets._trace_secret_use(
            "secret-exec", [_VALUE], vault="prod", key_names=["prod-token"], argv0="kubectl")
        e = self.events("secret-exec")[0]
        self.assertEqual((e["key_names"], e["argv0"]), (["prod-token"], "kubectl"))

    def test_a_recorder_failure_never_fails_the_command(self):
        """Observability must not break the thing it observes: a credential successfully
        delivered must not become a failed command because the bookkeeping threw."""
        with mock.patch.object(trace, "record", side_effect=RuntimeError("disk full")), \
             mock.patch.object(commands_secrets.subprocess, "run",
                               lambda argv, **kw: SimpleNamespace(
                                   stdout="", stderr="", returncode=0)):
            rc = commands_secrets.cmd_secret_exec(self.exec_args(
                env=["PROD_TOKEN=prod-token"], command=["--", "/bin/true"]))
        self.assertEqual(rc, 0)


class SummaryNamesTheHandouts(_HandoutCase):
    """`charter trace --summary` gives hand-outs the same billing as guard denials.

    A summary that called out denials and secret warnings while leaving credential
    hand-outs to the generic per-event tally would imply they are the less notable thing.
    """

    def test_the_summary_names_the_command_and_the_key_but_no_value(self):
        import io
        from contextlib import redirect_stdout

        from charter import commands_persona

        with mock.patch.object(commands_secrets.subprocess, "run",
                               lambda argv, **kw: SimpleNamespace(
                                   stdout="", stderr="", returncode=0)):
            commands_secrets.cmd_secret_exec(self.exec_args(
                env=["PROD_TOKEN=prod-token"], command=["--", "/usr/bin/kubectl"]))
        buf = io.StringIO()
        with redirect_stdout(buf):
            commands_persona.cmd_trace(SimpleNamespace(session=_SESSION, summary=True, n=0))
        out = buf.getvalue()
        self.assertIn("credentials handed out", out)
        self.assertIn("prod/prod-token", out)
        self.assertIn("/usr/bin/kubectl", out)
        self.assertTrue(_VALUE not in out, "the summary printed a value")


if __name__ == "__main__":
    unittest.main()
