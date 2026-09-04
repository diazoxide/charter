"""What `os.replace` gives an atomic write, and what only the temp file's NAME can.

``os.replace`` is atomic about the moment the destination changes: a reader sees the whole
old file or the whole new one, never a prefix of either. It says nothing whatever about the
file being renamed. While every writer for one target agreed on one temp name — ``version``
published through ``version.tmp``, ``gather.json`` through ``gather.json.tmp`` — two writers
for that target were writing into ONE inode, and the sequence needs no unusual timing:

1. A opens the shared temp with ``"w"``, which truncates it to zero bytes.
2. B, a step ahead, ``os.replace``\\ s that same path onto the target. The target is now A's
   empty file, and B has been told its own content landed.
3. A writes its bytes into an inode that is already the target, so a reader between here
   and A's own rename sees whatever prefix has been flushed.

That is #893, and it was reached rather than reasoned about: on CI run 33854599000 a
frame's ``gather.json`` came out of `gather.refresh` existing and **zero bytes**, on a
branch touching none of this code, on one interpreter of four. `gather.cached` hands back
whatever parses and deliberately has no freshness check, so an empty scan is read as a true
one.

**These cases race rather than assert a name.** A case that read the temp file's name and
looked for a pid in it would pass against the defect verbatim — the bug is not that the name
lacks a pid, it is that two writers share a file — so what is asserted here is the property:
the target always holds ONE writer's whole content. Never empty, never a splice of two, at
every moment a reader can look and once the writers have stopped.

Threads and not processes, for what that buys and costs. It buys the interleaving being
inside one interpreter, where `sys.setswitchinterval` can make the scheduler switch on
almost every bytecode and turn a window measured in microseconds into one this suite hits
in under a second; it costs nothing in fidelity, because the shared-name defect is between
two *file descriptors* and does not care which process holds them. #845 already found this
shape from threads alone: `frame/record.py` writes the reopen manifest from a debounce
thread in the same process as everything else that writes it.
"""

from __future__ import annotations

import json
import os
import stat
import sys
import threading
import unittest
from pathlib import Path
from unittest import mock

from charter import config, workspace
from charter.frame import gather, state

from tests._isolation import PersonaIso

#: How many writers go at one target at once. Two is the defect's own shape and the number
#: the issue names; four is what a contended CI runner looks like and finds the window
#: sooner without making a green run slower in any way anyone notices.
WRITERS = 4

#: How many times each writer publishes. Chosen against the defect rather than for a round
#: number: with the shared temp name hand-applied, `charter/config.py`'s writer fails these
#: cases on every run measured at this depth, and the whole class costs well under a second
#: green.
ROUNDS = 60


def _payloads(n: int = WRITERS) -> list[str]:
    """One distinct, self-describing payload per writer, each a different LENGTH.

    Different lengths matter more than different values: a splice of two equal-length
    payloads can land on a byte boundary that still reads as one of them, and a short
    writer's content sitting inside a long writer's file is exactly what a shared temp
    inode produces. Trailing newline included, because every writer in `frame/state.py`
    writes one and a reader that strips is a reader that cannot see a truncation.
    """
    return [json.dumps({"writer": i, "pad": "p" * (256 * (i + 1))}) + "\n"
            for i in range(n)]


class _Race:
    """WRITERS threads publishing to one target, and one reader watching it throughout.

    The reader is the half that catches step 3 above — a reader between the wrong rename
    and the writer that is still filling the inode — and the final assertion is the half
    that catches step 2. Neither subsumes the other: a defect that only ever published
    empty files would be invisible to a run that happened to end on a good write.
    """

    def __init__(self, target: Path, publish, payloads: list[str]) -> None:
        self.target, self.publish, self.payloads = target, publish, payloads
        self.bad: list[str] = []
        self.errors: list[BaseException] = []
        self.stop = threading.Event()
        self.start = threading.Barrier(WRITERS + 1)

    def _write(self, i: int) -> None:
        self.start.wait()
        try:
            for _ in range(ROUNDS):
                self.publish(self.payloads[i])
        except BaseException as exc:       # noqa: BLE001 — reported, not swallowed
            self.errors.append(exc)

    def _read(self) -> None:
        self.start.wait()
        while not self.stop.is_set():
            try:
                text = self.target.read_text()
            except FileNotFoundError:
                continue                    # before the first publish lands
            except OSError as exc:
                self.errors.append(exc)
                return
            if text not in self.payloads:
                self.bad.append(text)
                return                      # one witness is the whole finding

    def run(self) -> None:
        # An aggressive switch interval, restored afterwards. The window this is about is
        # one `open()` wide, and the default 5ms interval means a thread usually runs
        # straight through it; at a microsecond the interpreter yields inside the window
        # instead. It changes how OFTEN the defect is hit and nothing about whether it is
        # there — a test that needed a `sleep` to find a race would be a test that stops
        # finding it (`reopen.write`'s own note, and the issue's).
        old = sys.getswitchinterval()
        sys.setswitchinterval(1e-6)
        threads = [threading.Thread(target=self._write, args=(i,))
                   for i in range(WRITERS)]
        reader = threading.Thread(target=self._read)
        try:
            for t in (*threads, reader):
                t.start()
            for t in threads:
                t.join()
            self.stop.set()
            reader.join()
        finally:
            sys.setswitchinterval(old)


class TwoWritersForOneTarget(PersonaIso):
    """`config.replace_for` under concurrency, on both sides of its own dispatch."""

    def _assert_whole(self, race: _Race, target: Path) -> None:
        self.assertEqual(race.errors, [], "a writer raised rather than publishing")
        self.assertEqual(race.bad, [],
                         "a reader saw a file that is no writer's content — two writers "
                         "shared one temp inode, which is what a private temp NAME is for")
        self.assertIn(target.read_text(), race.payloads,
                      "the target ended up holding something no writer wrote whole")

    def test_the_target_only_ever_holds_one_writers_whole_content(self) -> None:
        target = Path(config.STATE_DIR) / "frame" / "alpha.1" / "gather.json"
        config.private_mkdir(target.parent)
        payloads = _payloads()
        race = _Race(target, lambda text: config.replace_for(target, text), payloads)
        race.run()
        self._assert_whole(race, target)

    def test_it_holds_for_a_committed_target_too(self) -> None:
        """The other side of `config.write_for`'s dispatch — `workspace._write_manifest`
        publishes ``workspaces/<n>/workspace.json`` through this, and that file's readers
        include `git add`. A race that only ever ran under `.charter/` would leave the
        branch a committed file takes unmeasured."""
        target = self.tmp / "committed" / "workspace.json"
        target.parent.mkdir(parents=True)
        payloads = _payloads()
        race = _Race(target, lambda text: config.replace_for(target, text), payloads)
        race.run()
        self._assert_whole(race, target)

    def test_no_temp_file_survives_the_race(self) -> None:
        """Every publish that landed took its temp file with it. Asserted after the race
        rather than during it, because during it there ARE temp files — that is the point
        of them — and a case that could not tell the two apart would be asserting that the
        writer does not work."""
        target = Path(config.STATE_DIR) / "frame" / "alpha.1" / "gather.json"
        config.private_mkdir(target.parent)
        race = _Race(target, lambda text: config.replace_for(target, text), _payloads())
        race.run()
        self.assertEqual(sorted(p.name for p in target.parent.iterdir()),
                         ["gather.json"])


class TwoSaversForOneFrame(PersonaIso):
    """The same property through the function the defect was OBSERVED in.

    `config.replace_for` being correct is not the same claim as `gather.save` calling it:
    the cache file that came out of CI empty is this one, and a case that only exercised
    the shared writer would go green with `gather.save` still spelling its own temp name.
    """

    FID = "alpha.1"

    def test_a_cache_read_during_a_race_always_parses_and_is_never_empty(self) -> None:
        """The reader reads the FILE, not `gather.cached`, and the difference is the whole
        CI failure. `cached` answers ``None`` for a file that is missing and for one that
        does not parse — it cannot tell them apart and does not try — so a reader watching
        through it sees the zero-byte cache as "no cache yet" and reports nothing. What the
        panel then draws is a frame with no branch on it, which is what run 33854599000
        actually asserted against. So the witness is the bytes on disk: every reading of
        this file, at every moment, is one saver's whole JSON.
        """
        state.frame_dir(self.FID, create=True)
        scans = [{"repos": [{"name": f"r{j}"} for j in range(i + 1)], "worktrees": [],
                  "writer": i} for i in range(WRITERS)]
        # `json.dumps` is deterministic for one dict within one process, so this is the
        # exact byte string `gather.save` will write for each scan — which is what lets the
        # race compare a raw read against "what some writer wrote whole".
        payloads = [json.dumps(s) for s in scans]
        target = Path(config.STATE_DIR) / "frame" / self.FID / "gather.json"

        race = _Race(target, lambda text: gather.save(self.FID, json.loads(text)), payloads)
        race.run()

        self.assertEqual(race.errors, [], "a saver raised rather than saving")
        self.assertEqual(race.bad, [],
                         "a panel repaint read a cache nobody saved — an empty file among "
                         "these is the CI failure this issue was filed from, verbatim")
        self.assertNotEqual(target.read_bytes(), b"", "the cache file is zero bytes")
        self.assertIn(gather.cached(self.FID), scans)


class TheTempFileIsThisWritersOwn(PersonaIso):
    """`config.temp_beside` on its own. These cases cannot replace the racing ones above —
    a name carrying a pid is not the property, it is one way of getting it — but a name
    that stopped being unique would fail here first and say why in one line."""

    def test_two_asks_for_one_target_never_name_the_same_file(self) -> None:
        target = Path(config.STATE_DIR) / "frame" / "alpha.1" / "gather.json"
        names = {config.temp_beside(target) for _ in range(200)}
        self.assertEqual(len(names), 200)

    def test_it_is_beside_the_target_so_the_rename_stays_a_rename(self) -> None:
        """``os.replace`` cannot cross filesystems, and `.charter/` may be a mount of its
        own (`$CHARTER_HOME` puts it anywhere on the machine). A temp file in the system
        temp directory would turn every atomic write into an `EXDEV`."""
        target = Path(config.STATE_DIR) / "frame" / "alpha.1" / "gather.json"
        tmp = config.temp_beside(target)
        self.assertEqual(tmp.parent, target.parent)
        self.assertNotEqual(tmp.name, target.name)
        self.assertTrue(tmp.name.startswith(f"{target.name}."),
                        "a temp file that does not say which target it belongs to is one "
                        "nobody can attribute when it is found beside four of them")
        self.assertTrue(tmp.name.endswith(config.TEMP_SUFFIX))

    def test_it_carries_the_writing_process(self) -> None:
        """The pid separates processes and the random tail separates threads within one.
        Both are needed and neither is decoration: `notify.plane_changed_everywhere` writes
        a gather cache for every frame on the plane, from hooks, panels and CLI commands
        that run at once."""
        target = Path(config.STATE_DIR) / "frame" / "alpha.1" / "gather.json"
        self.assertIn(str(os.getpid()), config.temp_beside(target).name)


class AFailedPublishLeavesNothingBehind(PersonaIso):
    """A temp name nothing can predict is a temp name nothing can collect. None of the
    directories charter publishes into has a sweep that would find one: `.charter/frame/`
    has `reopen.prune_transcripts`, which touches nothing but `*.transcript`, and
    `workspaces/<n>/` has `git status`, where litter is a teammate's problem."""

    def test_a_rename_that_fails_removes_the_temp_file(self) -> None:
        target = Path(config.STATE_DIR) / "frame" / "alpha.1" / "version"
        config.private_mkdir(target.parent)
        with mock.patch("os.replace", side_effect=OSError("no")):
            with self.assertRaises(OSError):
                config.replace_for(target, "1\n")
        self.assertEqual(list(target.parent.iterdir()), [])

    def test_a_write_that_fails_removes_the_temp_file(self) -> None:
        """The earlier half of the same clause. `write_for` can create the file and then
        fail filling it — a full filesystem answers at the `write`, not at the `open` —
        and the temp file exists at that point exactly as it does after a failed rename."""
        target = Path(config.STATE_DIR) / "frame" / "alpha.1" / "version"
        config.private_mkdir(target.parent)
        real = config.open_for

        class _Full:
            def __init__(self, f):
                self._f = f

            def write(self, _data):
                raise OSError(28, "No space left on device")

        import contextlib

        @contextlib.contextmanager
        def refusing(p, mode="w", **kw):
            with real(p, mode, **kw) as f:
                yield _Full(f)

        with mock.patch.object(config, "open_for", refusing):
            with self.assertRaises(OSError):
                config.replace_for(target, "1\n")
        self.assertEqual(list(target.parent.iterdir()), [])

    def test_a_temp_file_that_cannot_be_removed_is_still_the_real_failure(self) -> None:
        """The filesystem that refused the rename can refuse the tidy-up too. What the
        caller is owed is why its WRITE failed; replacing that with the unlink's error
        would send every reader of the traceback after the wrong fault."""
        target = Path(config.STATE_DIR) / "frame" / "alpha.1" / "version"
        config.private_mkdir(target.parent)
        with mock.patch("os.replace", side_effect=OSError(28, "No space left on device")), \
                mock.patch("pathlib.Path.unlink", side_effect=OSError(13, "read-only")):
            with self.assertRaises(OSError) as caught:
                config.replace_for(target, "1\n")
        self.assertEqual(caught.exception.errno, 28)

    def test_a_failed_publish_leaves_the_previous_content_whole(self) -> None:
        target = Path(config.STATE_DIR) / "frame" / "alpha.1" / "version"
        config.private_mkdir(target.parent)
        config.replace_for(target, "first\n")
        with mock.patch("os.replace", side_effect=OSError("no")):
            with self.assertRaises(OSError):
                config.replace_for(target, "second\n")
        self.assertEqual(target.read_text(), "first\n")


class ThePublishedFileKeepsTheModeItsPlaceAsksFor(PersonaIso):
    """``os.replace`` carries the SOURCE's mode onto the target (#582), so the temp file
    decides what the destination comes out at — and the temp file is created by
    `config.write_for`, which asks where the path is rather than assuming.

    This is why `config.replace_for` does NOT use `tempfile.mkstemp`, which
    `frame/reopen.py` used to and every other atomic writer in the wild does: mkstemp
    creates at 0600 unconditionally, which is the right mode for `.charter/` and the wrong
    one for a file in the operator's own git tree. Charter tightens what is its own and
    reports what is not (#331); a manifest that came back 0600 from a `charter workspace`
    command would be charter tightening somebody's committed file without being asked.
    """

    def test_a_state_file_is_private_however_loose_the_umask(self) -> None:
        target = Path(config.STATE_DIR) / "frame" / "alpha.1" / "version"
        config.private_mkdir(target.parent)
        old = os.umask(0o000)
        try:
            config.replace_for(target, "1\n")
        finally:
            os.umask(old)
        self.assertEqual(stat.S_IMODE(target.stat().st_mode) & 0o077, 0)

    def test_a_committed_file_is_left_at_the_umasks_answer(self) -> None:
        workspace.ensure("alpha")
        old = os.umask(0o022)
        try:
            workspace.write_manifest("alpha", {"name": "alpha", "repos": []})
            control = self.tmp / "control"
            control.write_text("x")
        finally:
            os.umask(old)
        self.assertEqual(stat.S_IMODE(workspace.manifest_path("alpha").stat().st_mode),
                         stat.S_IMODE(control.stat().st_mode),
                         "a committed manifest was tightened; it is the operator's")


if __name__ == "__main__":
    unittest.main()
