"""A read of plane data must finish, and must not grow without limit.

#336(b): nothing bounded these reads — no size cap, no deadline, and no check that the
thing being opened is a file at all. A committed symlink to a FIFO at
`personas/<active>/persona.md` hung `doctor`, `statusline`, `hook sessionstart` and
`persona show` on 0.47.2; `/dev/zero` is the memory-exhaustion variant of the same
omission. `hooks/hooks.json` caps the SessionStart hooks at 5s and 20s, so a session
degrades to a lost briefing — the status line and `persona show` have no charter-side
bound at all.

**One check kills both, and it is the check that was already there for containment.**
`os.lstat` says regular-or-not and how big in the same syscall, and — this is the part
that matters — `stat` never *opens* anything, so it answers about a FIFO without
blocking on it. A FIFO is not `S_ISREG`; neither is a character device; neither is a
directory. The hang, the endless yield and the oversized read all die at the gate the
redirection check already pays for, which is why this half costs no new syscall.

**The cap is 1 MiB, and it is meant never to fire on anything a human wrote.** The
largest persona charter in charter's own plane is 6.8 KB, the largest memory index 5 KB,
the largest document under `docs/` 34 KB. 1 MiB is ~150× the first and still small
enough that one read cannot exhaust anything. A cap tuned close to real content would
fire on the first long runbook somebody curates, and a cap that never fires on anything
at all is decoration — this one fires only on a file no editor produced.

**Preconditions are asserted, not assumed.** Each hang case proves the fixture really
blocks a reader — a thread that opens it is still alive when charter has already
returned — before asserting that charter did not block on it. Without that, a passing
test proves only that `mkfifo` was spelled wrong.
"""

from __future__ import annotations

import os
import stat
import threading
import unittest
from pathlib import Path
from unittest import mock

from charter import config, contain, hooks, memstore, persona, recall, todos, workspace
from tests._isolation import PersonaIso

#: Long enough that a hang is unambiguous, short enough that a regression does not look
#: like a wedged machine. Every case here returns in microseconds when the gate holds.
WATCHDOG = 5.0


class PlaneReadsAreBounded(PersonaIso):

    # ------------------------------------------------------------------ helpers
    def fifo(self, path: Path) -> Path:
        """A FIFO at *path*, proven to be one and proven to block a reader.

        The proof is a live reader: a thread that opens it is still running when the
        assertions below say charter came back. Cleanup opens the write end, which is
        what releases that reader — a blocked `open()` on a FIFO waits for a writer.
        """
        path.parent.mkdir(parents=True, exist_ok=True)
        os.mkfifo(path)
        self.assertTrue(stat.S_ISFIFO(os.lstat(path).st_mode),
                        f"precondition: {path} must be a FIFO")

        opened = threading.Event()
        def _read():
            opened.set()
            with open(path) as f:               # blocks here until a writer appears
                f.read()
        reader = threading.Thread(target=_read, daemon=True)
        reader.start()
        opened.wait(1.0)
        reader.join(0.3)
        self.assertTrue(reader.is_alive(),
                        f"precondition: reading {path} must block — it did not")
        self.addCleanup(self._release, path)
        return path

    @staticmethod
    def _release(path: Path) -> None:
        try:
            os.close(os.open(path, os.O_WRONLY | os.O_NONBLOCK))
        except OSError:
            pass                                 # nobody waiting — nothing to release

    def completes(self, fn, label: str):
        """Call *fn* on a watchdog. Fails if it is still running when the clock runs out."""
        box = {}
        t = threading.Thread(target=lambda: box.update(r=fn()), daemon=True)
        t.start()
        t.join(WATCHDOG)
        self.assertFalse(t.is_alive(), f"{label} blocked for more than {WATCHDOG}s")
        return box.get("r")

    def oversized(self, path: Path) -> Path:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text("# big\n\nbadger " * ((contain.MAX_BYTES // 7) + 1))
        self.assertGreater(path.stat().st_size, contain.MAX_BYTES, "precondition")
        return path

    # ---------------------------------------------------------- (b) it must finish
    def test_a_fifo_persona_charter_does_not_block_the_read(self):
        d = config.PERSONAS_DIR / "blocked"
        d.mkdir(parents=True)
        self.fifo(d / "persona.md")
        self.assertIn("blocked", persona.list_personas(),
                      "precondition: charter still finds this persona")

        self.assertIsNone(self.completes(lambda: persona.load("blocked"),
                                         "persona.load on a FIFO charter"))
        self.completes(lambda: persona.resolve("blocked"), "persona.resolve on a FIFO")
        self.completes(lambda: persona.structural_errors("blocked"), "lint on a FIFO")

    def test_a_fifo_memory_does_not_block_recall(self):
        self.make_persona("reader")
        mem = persona.memory_dir("reader")
        (mem / "kept.md").write_text("# a real memory\n\nbadger\n")
        self.fifo(mem / "blocked.md")

        hits = self.completes(
            lambda: recall.recall("badger", persona_name="reader", scopes=("persona",)).hits,
            "recall over a FIFO memory")
        self.assertEqual(["kept.md"], [h.path.name for h in hits])

    def test_a_fifo_todo_does_not_block_the_count(self):
        workspace.ensure("other")
        self.fifo(todos.todos_dir("other") / "blocked.md")
        self.assertEqual(0, self.completes(lambda: todos.count_open("other"),
                                           "todos.count_open over a FIFO"))
        self.assertEqual([], self.completes(lambda: todos.open_todos("other"),
                                            "todos.open_todos over a FIFO"))

    def test_a_fifo_default_persona_pointer_does_not_block(self):
        """`personas/.default` is committed, and `resolve_active` reads it on every turn
        to answer "who am I" — the status line's first question."""
        self.fifo(config.PERSONAS_DIR / ".default")
        self.completes(persona.resolve_active, "resolve_active on a FIFO .default")

    def test_a_fifo_workspace_charter_does_not_block_the_session_briefing(self):
        """The SessionStart neighbour digest reads every other workspace's `workspace.md`
        for its vision line. `hooks.json` caps that hook at 5s, so a block here costs the
        session its briefing rather than the turn — a bound, but not charter's."""
        workspace.ensure("other")
        charter_file = workspace.charter_file("other")
        charter_file.unlink(missing_ok=True)
        self.fifo(charter_file)
        self.assertEqual("", self.completes(lambda: workspace.read_vision("other"),
                                            "read_vision on a FIFO workspace.md"))

    def test_a_fifo_mcp_declaration_does_not_block_sync_agents(self):
        """`personas/<name>/mcp.json` is the third committed file `persona.py` opens by
        listing rather than by asking. `mcp_servers` already refuses to raise on a stray
        comma; blocking for ever is the same promise broken a different way."""
        self.make_persona("reader")
        self.fifo(persona.dir_of("reader") / persona.MCP_FILE)
        self.assertEqual({}, self.completes(lambda: persona.mcp_servers("reader"),
                                            "mcp_servers on a FIFO mcp.json"))

    @unittest.skipUnless(os.path.exists("/dev/zero"), "no /dev/zero on this platform")
    def test_a_character_device_is_not_a_plane_file(self):
        """The memory-exhaustion variant: a link to a device that yields for ever. It is
        refused by the same S_ISREG test that stops the FIFO, without reading a byte."""
        self.assertTrue(stat.S_ISCHR(os.stat("/dev/zero").st_mode), "precondition")
        self.assertIsNotNone(contain.file_refusal(Path("/dev/zero")))

    # -------------------------------------------------------- (b) it must stay small
    def test_an_oversized_memory_is_not_read(self):
        self.make_persona("reader")
        mem = persona.memory_dir("reader")
        (mem / "kept.md").write_text("# a real memory\n\nbadger\n")
        self.oversized(mem / "huge.md")

        self.assertEqual(["kept.md"], [p.name for p in memstore.files(mem)])
        hits = recall.recall("badger", persona_name="reader", scopes=("persona",)).hits
        self.assertEqual(["kept.md"], [h.path.name for h in hits])

    def test_a_file_just_under_the_cap_is_still_read(self):
        """The half that catches a cap so tight it refuses real content."""
        self.make_persona("reader")
        big = persona.memory_dir("reader") / "long.md"
        big.write_text("# long\n\n" + "badger " * ((contain.MAX_BYTES // 7) - 2))
        self.assertLess(big.stat().st_size, contain.MAX_BYTES, "precondition")
        self.assertGreater(big.stat().st_size, contain.MAX_BYTES // 2, "precondition")

        hits = recall.recall("badger", persona_name="reader", scopes=("persona",)).hits
        self.assertEqual(["long.md"], [h.path.name for h in hits])

    def test_an_oversized_persona_charter_is_not_loaded(self):
        d = config.PERSONAS_DIR / "huge"
        d.mkdir(parents=True)
        self.oversized(d / "persona.md")
        self.assertIsNone(persona.load("huge"))

    def test_the_session_briefing_counts_todos_without_reading_them(self):
        """`hooks` read every todo of every workspace in full to print a number, while
        `todos.count_open` — which the status line already uses — counts the listing. One
        question, two implementations, and the expensive one ran at SessionStart."""
        workspace.ensure("other")
        todos.add("other", "something this workspace still means to do")

        with mock.patch.object(memstore, "entries", wraps=memstore.entries) as spy:
            digest = hooks._other_workspaces_digest("s1")
        read_dirs = [str(c.args[0]) for c in spy.call_args_list]
        self.assertEqual([], [d for d in read_dirs if d.endswith("todos")],
                         f"the digest read todo bodies to count them: {read_dirs}")
        self.assertIn("1 todo", digest)


if __name__ == "__main__":
    unittest.main()
