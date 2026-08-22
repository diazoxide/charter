"""A committed symlink must not redirect a WRITE charter performs on its own.

#349, the other half of #336. #348 gated every *read* of plane data behind
`contain.file_refusal`/`dir_refusal`; the write side never went through `memstore.files`
and inherited none of it. `ensure_index`, `write`, `index_append` and `_drop_index_line`
opened their target directly, and `Path.write_text` / `open(…, "a")` follow a symlink.

**Two properties make this sharper than the read half.**

``MEMORY.md`` is a **fixed name**, so there is nothing to guess and no race to win: the
file charter will write to is known before the attacker commits, and it is the one they
replace with a link. And the target need not be a charter file — pointed at
``.charter/vaults/<name>.json`` an append *corrupts* a credential store, where the read
half merely leaked one. Reproduced against the real CLI on 0.47.2 and on #348's branch:

    $ charter persona remember victim "the sky is blue"
    ✓ Remembered (persistent) → personas/victim/memory/the-sky-is-blue.md
    # …and the vault is now two lines and no longer parses as JSON.

**ENOENT is the one thing the write side must read differently.** On a read, a path that
is not there is a refusal. On a write it is the *normal* case — the file is about to be
created — so `contain.write_refusal` answers ``None`` for it. Getting that backwards
breaks every first write on a fresh plane, which is why
:meth:`test_a_first_write_on_a_fresh_plane_is_not_refused` and
:meth:`test_a_dangling_link_that_escapes_does_not_create_its_target` are both here: they
fail in opposite directions, and no single mistake passes both.

**Preconditions are asserted, not assumed.** Every case plants a real link or FIFO,
proves the OS follows or blocks on it, and records the target's exact bytes *before*
asking charter to refuse. A test that passed because the fixture was a copy, or because
the target happened not to exist, would prove nothing — this audit has produced seven
vacuous passes already.
"""

from __future__ import annotations

import json
import os
import shutil
import stat
import tempfile
import threading
import unittest
from pathlib import Path

from charter import config, contain, curate, memstore, persona, todos, workspace
from tests._isolation import PersonaIso

#: The value an append must never reach. Same shape as the read half's canary so a grep
#: from either report lands in both files.
CANARY = "CANARY-VAULT-SECRET-9f31"

#: Long enough that a block is unambiguous, short enough that a regression does not look
#: like a wedged machine. Every case here returns in microseconds when the gate holds.
WATCHDOG = 5.0


class WriteFixtures(PersonaIso):
    """Plane, vault canary and link/FIFO fixtures — shared, so neither case class
    inherits the other's tests and runs them twice."""

    def setUp(self) -> None:
        super().setUp()
        # A plain-file vault at the fixed place relative to the plane — what makes the
        # attack portable to a machine whose layout the attacker does not know.
        self.vault = config.VAULTS_DIR / "devops.json"
        self.vault.parent.mkdir(parents=True, exist_ok=True)
        # Two properties, both load-bearing, both learned from a vacuous pass.
        #
        # It carries an index-shaped `(…md)` link, so `index_drift` reading this file
        # *shows up* in what it returns. Without one the drift assertion passed against
        # unfixed code: a vault with no links yields no links whether charter read it or
        # not, which proves only that JSON is not markdown.
        #
        # It has **no trailing newline**, so `_drop_index_line`'s rewrite
        # (`"\n".join(keep) + "\n"`) is visible in the bytes. With one, a single-line
        # target round-trips through that filter unchanged and `forget` also passed
        # against unfixed code — the write happened and the file looked untouched.
        self.vault.write_text(
            '{"token": "%s", "note": "rotate: - [stolen](outside-canary.md)"}' % CANARY)
        self.vault_bytes = self.vault.read_bytes()
        self.vault_mtime = self.vault.stat().st_mtime_ns
        self.outside = Path(tempfile.mkdtemp(prefix="edm-test-outside-"))
        self.addCleanup(shutil.rmtree, self.outside, ignore_errors=True)

    # ------------------------------------------------------------------ helpers
    def link(self, link: Path, target: Path) -> str:
        """Plant *link* → *target* as a **relative** symlink and prove it is one."""
        link.parent.mkdir(parents=True, exist_ok=True)
        rel = os.path.relpath(target, link.parent)
        if link.is_symlink() or link.exists():
            link.unlink()
        os.symlink(rel, link)
        self.assertTrue(link.is_symlink(), f"fixture must be a symlink, not a copy: {link}")
        return rel

    def escaping_link(self, link: Path, target: Path) -> str:
        rel = self.link(link, target)
        self.assertTrue(rel.startswith(".."),
                        f"fixture must escape its own directory, got {rel!r}")
        return rel

    def live_vault_link(self, link: Path) -> Path:
        """*link* → the vault, proven live: the OS reads the secret through it.

        This is the precondition that separates "charter refused" from "the fixture never
        worked". Without it a passing test proves only that `os.symlink` was misspelled.
        """
        self.escaping_link(link, self.vault)
        self.assertIn(CANARY, link.read_text(),
                      "precondition: the OS must follow the link to the vault")
        return link

    def assertVaultIntact(self, why: str) -> None:
        """The vault must be untouched: same bytes, same mtime, still valid JSON.

        `mtime` is here because content equality is not proof that nothing was written —
        `_drop_index_line` rewrites the file whole, and a target it happens to reproduce
        byte for byte looks identical to one charter never opened. That is precisely how
        this file's first draft passed against unfixed code.
        """
        self.assertEqual(self.vault_bytes, self.vault.read_bytes(), why)
        self.assertEqual(self.vault_mtime, self.vault.stat().st_mtime_ns,
                         f"{why} (the bytes survived, but the file was rewritten)")
        json.loads(self.vault.read_text())          # raises if an append corrupted it

    def _fifo(self, path: Path) -> Path:
        path.parent.mkdir(parents=True, exist_ok=True)
        if path.is_symlink() or path.exists():
            path.unlink()
        os.mkfifo(path)
        self.assertTrue(stat.S_ISFIFO(os.lstat(path).st_mode),
                        f"precondition: {path} must be a FIFO")
        return path

    def write_blocking_fifo(self, path: Path) -> Path:
        """A FIFO with **no reader**, proven to block a writer.

        Deliberately not the same fixture as :meth:`read_blocking_fifo`, and the two
        cannot be merged: a FIFO blocks a writer only while nobody is reading and blocks
        a reader only while nobody is writing, so the thread that *proves* one case is
        exactly what makes the other case fail to block. A single shared helper here
        passed the read case against unfixed code, because its own blocked writer had
        already satisfied the reader.
        """
        self._fifo(path)
        started = threading.Event()

        def _write():
            started.set()
            with open(path, "a") as f:              # blocks here until a reader appears
                f.write("x")
        writer = threading.Thread(target=_write, daemon=True)
        writer.start()
        started.wait(1.0)
        writer.join(0.3)
        self.assertTrue(writer.is_alive(),
                        f"precondition: writing to {path} must block — it did not")
        self.addCleanup(self._release, path, os.O_RDONLY)
        return path

    def read_blocking_fifo(self, path: Path) -> Path:
        """A FIFO with **no writer**, proven to block a reader."""
        self._fifo(path)
        started = threading.Event()

        def _read():
            started.set()
            with open(path) as f:                   # blocks here until a writer appears
                f.read()
        reader = threading.Thread(target=_read, daemon=True)
        reader.start()
        started.wait(1.0)
        reader.join(0.3)
        self.assertTrue(reader.is_alive(),
                        f"precondition: reading {path} must block — it did not")
        self.addCleanup(self._release, path, os.O_WRONLY)
        return path

    @staticmethod
    def _release(path: Path, how: int) -> None:
        try:
            os.close(os.open(path, how | os.O_NONBLOCK))
        except OSError:
            pass                                 # nobody waiting — nothing to release

    def completes(self, fn, label: str):
        """Call *fn* on a watchdog. Fails if it is still running when the clock runs out."""
        box = {}

        def _run():
            try:
                box["r"] = fn()
            except BaseException as e:            # noqa: BLE001 — the watchdog wants the
                box["e"] = e                      # outcome, whatever shape it arrived in
        t = threading.Thread(target=_run, daemon=True)
        t.start()
        t.join(WATCHDOG)
        self.assertFalse(t.is_alive(), f"{label} blocked for more than {WATCHDOG}s")
        return box


class PlaneWritesAreContained(WriteFixtures):

    # --------------------------------------- contain.write_refusal, on its own
    def test_a_path_that_is_not_there_is_not_a_refusal(self):
        """The one rule the write side does not share with the read side.

        Every first write on a fresh plane lands on a path that does not exist. Reading
        ENOENT as a refusal — which `file_refusal` correctly does — would refuse all of
        them, so the two functions have to answer this differently and deliberately.
        """
        fresh = persona.memory_dir("nobody") / "MEMORY.md"
        self.assertFalse(fresh.exists(), "precondition: nothing is there yet")
        self.assertIsNone(contain.write_refusal(fresh))
        self.assertIsNotNone(contain.file_refusal(fresh),
                             "the READ side must still refuse a path that is not there")

    def test_an_ordinary_file_inside_the_data_roots_is_writable(self):
        self.make_persona("real")
        idx = persona.index_of(persona.memory_dir("real"))
        self.assertTrue(idx.is_file(), "precondition: scaffolding wrote a real index")
        self.assertIsNone(contain.write_refusal(idx))

    def test_a_link_that_escapes_the_data_roots_is_refused(self):
        self.make_persona("victim")
        idx = self.live_vault_link(persona.index_of(persona.memory_dir("victim")))
        refusal = contain.write_refusal(idx)
        self.assertIsNotNone(refusal, "an escaping link must be refused on the write side")
        self.assertIn("outside the directories", refusal)
        self.assertIn("write", refusal,
                      "the refusal must say a WRITE was redirected, not a read")

    def test_a_link_that_lands_inside_the_data_roots_is_followed(self):
        """The benign half — what catches a "fix" that contains writes by refusing all
        of them. #342 stayed lexical partly so a plane that legitimately links a persona
        directory keeps working, and resolving must not take that away."""
        self.make_persona("real")
        self.make_persona("alias")
        target = persona.memory_dir("real") / "MEMORY.md"
        self.assertTrue(target.is_file(), "precondition: the target is a real index")
        link = persona.memory_dir("alias") / "MEMORY.md"
        self.link(link, target)
        self.assertIsNone(contain.write_refusal(link),
                          "a link landing inside the data roots must still be written")

    def test_a_dangling_link_inside_the_data_roots_is_writable(self):
        """A contained link whose target is not there **yet** is the ENOENT case one
        level down: charter is about to create the file the link names, and it names a
        place charter may write."""
        self.make_persona("real")
        link = persona.memory_dir("real") / "later.md"
        self.link(link, persona.memory_dir("real") / "not-yet.md")
        self.assertTrue(link.is_symlink() and not link.exists(),
                        "precondition: a live link to a file that does not exist")
        self.assertIsNone(contain.write_refusal(link))

    def test_a_fifo_is_refused_before_anything_opens_it(self):
        self.make_persona("victim")
        idx = self.write_blocking_fifo(persona.index_of(persona.memory_dir("victim")))
        self.assertIsNotNone(contain.write_refusal(idx))

    def test_a_directory_is_not_a_writable_plane_file(self):
        self.make_persona("victim")
        d = persona.memory_dir("victim") / "archive"
        d.mkdir(parents=True, exist_ok=True)
        self.assertIsNotNone(contain.write_refusal(d))

    def test_a_write_refusal_never_raises(self):
        """`contain`'s standing promise, extended to the new function. `os.lstat` answers
        a path holding a NUL with `ValueError`, not `OSError` — the one input shaped to
        reach past a check, and the bug that survived #348's first push."""
        for bad in ("a\x00b", ""):
            with self.subTest(path=bad):
                self.assertIsNotNone(contain.write_refusal(bad))

    # ------------------------------------------------- the flagship: remember
    def test_remember_does_not_append_through_a_linked_index(self):
        """#349's demonstration, end to end. `MEMORY.md` is a fixed name, so this needs
        no guess: the attacker commits the link and waits for the next `remember`."""
        self.make_persona("victim")
        self.live_vault_link(persona.index_of(persona.memory_dir("victim")))

        with self.assertRaises(contain.Refused):
            persona.remember("victim", "the sky is blue")
        self.assertVaultIntact("charter appended its index line through the link")

    def test_remember_says_why_it_refused(self):
        """A refusal is data. The operator ran a command that did not do what it said;
        "✓ Remembered" over a corrupted vault is the failure, and silence is only
        marginally better. The sentence has to name both ends — the path charter opened
        is not the path it was given, which is the whole defect."""
        self.make_persona("victim")
        idx = self.live_vault_link(persona.index_of(persona.memory_dir("victim")))
        try:
            persona.remember("victim", "the sky is blue")
            self.fail("the write was not refused")
        except contain.Refused as e:
            said = str(e)
        self.assertIn(str(idx), said)
        self.assertIn(str(self.vault.resolve()), said,
                      "the refusal must name where the link actually goes")

    def test_remember_does_not_write_its_memory_file_through_a_linked_directory(self):
        """The variant a per-file check structurally cannot see: when the *directory* is
        the link, every path inside it is an ordinary name with nothing to object to."""
        self.make_persona("victim")
        mem = persona.memory_dir("victim")
        shutil.rmtree(mem, ignore_errors=True)
        elsewhere = self.outside / "stolen"
        elsewhere.mkdir(parents=True)
        self.link(mem, elsewhere)
        self.assertTrue(mem.is_dir() and mem.is_symlink(),
                        "precondition: a live directory link")

        with self.assertRaises(contain.Refused):
            persona.remember("victim", "the sky is blue")
        self.assertEqual([], sorted(p.name for p in elsewhere.iterdir()),
                         "charter wrote a memory into a directory outside the plane")

    def test_a_dangling_link_that_escapes_does_not_create_its_target(self):
        """The half that fails if ENOENT is read as "nothing to object to" *before* the
        link is resolved. `write_text` through a dangling link **creates the target**, so
        a link at a predictable name is a write-anywhere primitive with attacker-chosen
        content — strictly worse than the append, and invisible to `exists()`."""
        self.make_persona("victim")
        victim_file = self.outside / "not-yet-there.json"
        self.assertFalse(victim_file.exists(), "precondition: the target does not exist")
        link = persona.index_of(persona.memory_dir("victim"))
        self.escaping_link(link, victim_file)
        self.assertTrue(link.is_symlink() and not link.exists(),
                        "precondition: a live link to a file that is not there")

        with self.assertRaises(contain.Refused):
            persona.remember("victim", "the sky is blue")
        self.assertFalse(victim_file.exists(),
                         "charter created a file outside the plane through a dangling link")

    def test_a_fifo_index_does_not_block_remember(self):
        """The liveness half on the write side. `open(fifo, "a")` waits for a reader for
        ever, and unlike the read half there is no `hooks.json` timeout above this — a
        human is sitting at the command."""
        self.make_persona("victim")
        self.write_blocking_fifo(persona.index_of(persona.memory_dir("victim")))
        box = self.completes(lambda: persona.remember("victim", "the sky is blue"),
                             "persona.remember onto a FIFO index")
        self.assertIsInstance(box.get("e"), contain.Refused)

    # ---------------------------------------------------- the benign half, end to end
    def test_a_first_write_on_a_fresh_plane_is_not_refused(self):
        """The case that breaks if ENOENT is treated as a refusal. Deliberately the
        whole path — scaffold, write, index — because each step lands on something that
        is not there yet."""
        self.make_persona("fresh")
        p = persona.remember("fresh", "the sky is blue")
        self.assertTrue(p.is_file(), "the memory file must be written")
        idx = persona.index_of(persona.memory_dir("fresh"))
        self.assertIn("the sky is blue", idx.read_text(),
                      "the index line must be appended")

    def test_a_second_write_still_appends(self):
        """A guard that refuses an *existing* regular file would pass every test above
        and break the plane on its second memory."""
        self.make_persona("fresh")
        persona.remember("fresh", "the sky is blue")
        persona.remember("fresh", "water is wet")
        idx = persona.index_of(persona.memory_dir("fresh")).read_text()
        self.assertIn("the sky is blue", idx)
        self.assertIn("water is wet", idx)

    def test_an_index_linked_inside_the_plane_is_still_appended(self):
        """The benign link, end to end: a plane that shares one index between two
        personas keeps working, because the link lands inside `personas/`."""
        self.make_persona("real")
        self.make_persona("alias")
        target = persona.index_of(persona.memory_dir("real"))
        self.link(persona.index_of(persona.memory_dir("alias")), target)
        persona.remember("alias", "the sky is blue")
        self.assertIn("the sky is blue", target.read_text(),
                      "a contained link must still be followed")

    # ------------------------------------------------------ the other write paths
    def test_forget_does_not_truncate_through_a_linked_index(self):
        """`_drop_index_line` **rewrites** the index whole, so this is the destructive
        variant: where `remember` appends two lines to a vault, `forget` replaces it with
        whatever survived a filter for one filename."""
        self.make_persona("victim")
        persona.remember("victim", "the sky is blue")
        self.live_vault_link(persona.index_of(persona.memory_dir("victim")))

        self.completes(lambda: persona.forget("victim", "the-sky-is-blue"),
                       "persona.forget onto a linked index")
        self.assertVaultIntact("charter truncated the vault through the index link")

    def test_index_drift_does_not_read_a_linked_index(self):
        """The read `files()` never covered, because it is the one file `files()` filters
        out by name. `doctor` calls `index_drift` for every memory base on every
        SessionStart, so a FIFO here hangs the briefing and a link here reads whatever it
        points at."""
        self.make_persona("victim")
        self.live_vault_link(persona.index_of(persona.memory_dir("victim")))
        drift = memstore.index_drift(persona.memory_dir("victim"))
        self.assertEqual([], drift["dangling"],
                         "charter parsed a file outside the plane as its index")

    def test_a_fifo_index_does_not_block_the_session_briefing(self):
        self.make_persona("victim")
        self.read_blocking_fifo(persona.index_of(persona.memory_dir("victim")))
        self.completes(lambda: memstore.index_drift(persona.memory_dir("victim")),
                       "index_drift over a FIFO index")

    def test_a_todo_is_not_recorded_through_a_linked_index(self):
        """The todo store is the same `memstore` with a different base, so one gate must
        cover it — asserted rather than assumed, which is what caught `curate` on the
        read side."""
        workspace.ensure("task")
        todos.scaffold("task")
        self.live_vault_link(memstore.index_path(todos.todos_dir("task")))
        with self.assertRaises(contain.Refused):
            todos.add("task", "ship the thing")
        self.assertVaultIntact("a todo was appended into the vault")

    def test_a_workspace_note_is_not_recorded_through_a_linked_index(self):
        workspace.ensure("task")
        workspace.scaffold_memory("task")
        self.live_vault_link(memstore.index_path(workspace.memory_dir("task")))
        with self.assertRaises(contain.Refused):
            workspace.note("task", "the sky is blue")
        self.assertVaultIntact("a workspace note was appended into the vault")

    def test_curate_does_not_repair_an_index_through_a_link(self):
        """`curate --apply` re-appends every unindexed file, so a linked index turns one
        operator command into N appends. It must refuse, and — because `apply_safe`
        returns a log the operator reads — it must say so there rather than raise out of
        a batch that has already archived files."""
        self.make_persona("victim")
        mem = persona.memory_dir("victim")
        memstore.write(mem, "a real memory", "kept", index=False)
        self.live_vault_link(persona.index_of(mem))

        actions = self.completes(lambda: curate.apply_safe(mem), "curate.apply_safe")
        self.assertIsNone(actions.get("e"),
                          f"apply_safe must not raise out of a batch: {actions.get('e')}")
        self.assertVaultIntact("curate repaired the index into the vault")
        self.assertTrue(any("outside the directories" in a for a in actions.get("r") or []),
                        f"the refusal must reach the operator's log, got {actions.get('r')}")

    def test_archive_does_not_move_a_memory_out_through_a_linked_directory(self):
        """`archive/` is the fourth fixed name in the store, and `rename` follows a link
        on the destination directory exactly as `open` does on a file."""
        self.make_persona("victim")
        mem = persona.memory_dir("victim")
        persona.remember("victim", "the sky is blue")
        elsewhere = self.outside / "stolen"
        elsewhere.mkdir(parents=True)
        self.link(mem / "archive", elsewhere)

        self.completes(lambda: memstore.archive(mem, "the-sky-is-blue"),
                       "memstore.archive into a linked directory")
        self.assertEqual([], sorted(p.name for p in elsewhere.iterdir()),
                         "charter moved a memory out of the plane")

    def test_doctor_says_when_it_will_not_touch_an_index(self):
        """A refusal nobody can see is how #337 happened, and the store is the one place
        where refusing is *quiet*: `files()` and `_listed` both answer "nothing", which a
        base with no memories is indistinguishable from. `doctor` reported
        "3 base(s) consistent" over an index pointing at a vault.

        ADR 0009 — name what was actually checked. "1 unindexed" would send the operator
        to `charter persona optimize`, which cannot repair an index charter refuses to
        write, so the hint that looked helpful would be the one that wasted their time.
        """
        from charter import doctor
        self.make_persona("victim")
        memstore.write(persona.memory_dir("victim"), "a real memory", "kept", index=False)
        self.live_vault_link(persona.index_of(persona.memory_dir("victim")))

        r = doctor.check_memory_indexes()
        self.assertNotEqual(doctor.OK, r.status,
                            "doctor reported a refused index as consistent")
        said = f"{r.detail} {r.hint or ''}"
        self.assertIn("victim", said, "the refusal must name WHICH base")
        self.assertIn("outside the directories", said,
                      f"doctor must say why charter will not touch it, got {said!r}")

    def test_doctor_does_not_flag_a_base_whose_index_is_simply_absent(self):
        """The ENOENT rule again, one layer up — and the regression the first draft of
        this check shipped. `charter init` scaffolds `personas/<front-door>/memory/` with
        a `.gitkeep` and **no MEMORY.md**, so asking the READ-side gate here ("a path that
        is not there is a refusal") reported every fresh plane as holding an index charter
        refuses to touch. Found against the real CLI, not in the suite.
        """
        from charter import doctor
        self.make_persona("fresh")
        idx = persona.index_of(persona.memory_dir("fresh"))
        idx.unlink(missing_ok=True)
        self.assertTrue(idx.parent.is_dir() and not idx.exists(),
                        "precondition: the base exists, its index does not")

        r = doctor.check_memory_indexes()
        self.assertEqual(doctor.OK, r.status,
                         f"an absent index is not a defect, got {r.detail!r} {r.hint!r}")


class FixedNameWritesAreContained(WriteFixtures):
    """Every other write charter aims at a name an attacker can predict.

    #349 names four functions in `memstore`; the shape is not theirs. Any write to a name
    charter chooses, inside a directory a commit controls, can be pre-replaced with a link
    — and `git checkout` materialises symlinks. Table-driven for the reason #342 gave:
    the next fixed-name write that skips containment should fail here rather than ship.

    **The boundary is `contain.data_roots()`.** These are the sites whose targets already
    sit inside the directories the read half defends — `personas/` and `workspaces/`.
    Charter writes to plenty of committed names *outside* them (`charter.toml`,
    `.gitignore`, `.claude/settings.json`, `inventory/repos.json`, `vaults.json`), and
    those have the same defect but no boundary to check against: `data_roots()` excludes
    them by design, because `.charter/` sits under ROOT and must stay excluded. Extending
    the boundary is a separate decision, and inventing one here would have drawn it
    silently.

    **Two contracts, and mixing them up would be the bug.** A write on a CLI path raises,
    so nothing is lost silently. A write from a *hook* — `dispatch`, `skilluse`, `pieces`
    — is a tally that already promises "best-effort: never break a turn", so it refuses by
    declining to write and answering None, exactly as it already does for `OSError`.
    """

    def outside_target(self, name: str) -> Path:
        """A file outside the plane, with known bytes, to catch a redirected write."""
        t = self.outside / name
        t.write_text("UNTOUCHED")
        return t

    def assertRedirectRefused(self, target: Path, label: str) -> None:
        self.assertEqual("UNTOUCHED", target.read_text(),
                         f"{label} was redirected through a committed link")

    # ------------------------------------------------- unattended: refuse, never raise
    def test_a_dispatch_tally_is_not_appended_through_a_link(self):
        """The highest-reachability site in the audit: driven by the PostToolUse hook on
        every sub-agent dispatch, with nobody typing anything. The filename is not even a
        guess — `personas/_dispatch/<YYYY-MM>.<host>.jsonl` is a committed file, so the
        attacker edits the one already in the repo."""
        from charter import dispatch
        target = self.outside_target("dispatch-victim.json")
        self.escaping_link(dispatch.path_for(), target)
        self.assertIsNone(dispatch.record("forge"), "a refused tally answers None")
        self.assertRedirectRefused(target, "dispatch.record")

    def test_dispatch_advice_is_not_appended_through_a_link(self):
        from charter import dispatch
        target = self.outside_target("advice-victim.json")
        self.escaping_link(dispatch.path_for(), target)
        self.assertIsNone(dispatch.record_advice())
        self.assertRedirectRefused(target, "dispatch.record_advice")

    def test_a_skill_tally_is_not_appended_through_a_link(self):
        from charter import skilluse
        target = self.outside_target("skills-victim.json")
        self.escaping_link(skilluse.path_for(), target)
        self.assertIsNone(skilluse.record("brainstorming"))
        self.assertRedirectRefused(target, "skilluse.record")

    def test_a_piece_event_is_not_appended_through_a_link(self):
        from charter import pieces
        workspace.ensure("task")
        target = self.outside_target("pieces-victim.json")
        self.escaping_link(pieces.log_path("task"), target)
        self.assertIsNone(pieces.record("task", "claimed", "repo", "p1"))
        self.assertRedirectRefused(target, "pieces.record")

    def test_a_presence_beat_is_not_written_through_a_link(self):
        """`seen` **overwrites** rather than appends, so a redirect here destroys the
        target outright — and it is hook-driven too."""
        from charter import pieces
        workspace.ensure("task")
        target = self.outside_target("seen-victim.json")
        self.escaping_link(pieces.seen_path("task", "repo", None), target)
        self.assertIsNone(pieces.seen("task", "repo", None, session="s"))
        self.assertRedirectRefused(target, "pieces.seen")

    def test_an_unattended_tally_never_raises_on_a_refusal(self):
        """The contract these three already carry, asserted rather than assumed: a hook
        may cost a session its briefing and never its turn. `contain.Refused` escaping
        into `hooks.pretooluse` would break every dispatch on a plane with one bad link."""
        from charter import dispatch, pieces, skilluse
        workspace.ensure("task")
        for label, path, call in (
            ("dispatch", dispatch.path_for(), lambda: dispatch.record("forge")),
            ("skilluse", skilluse.path_for(), lambda: skilluse.record("s")),
            ("pieces", pieces.log_path("task"), lambda: pieces.record("task", "claimed", "r", "p1")),
        ):
            with self.subTest(site=label):
                self.escaping_link(path, self.vault)
                try:
                    self.assertIsNone(call())
                except contain.Refused as e:
                    self.fail(f"{label} raised into a hook path: {e}")
                self.assertVaultIntact(f"{label} wrote into the vault")

    # ------------------------------------------------------ CLI paths: refuse loudly
    def test_persona_scaffolding_does_not_write_through_a_link(self):
        """`persona.scaffold_memory` writes `MEMORY.md` itself rather than through
        `memstore.ensure_index`, so gating the store alone leaves this one open — the same
        fixed name, one function over."""
        d = config.PERSONAS_DIR / "victim"
        d.mkdir(parents=True, exist_ok=True)
        target = self.outside_target("scaffold-victim.json")
        self.escaping_link(persona.memory_dir("victim") / "MEMORY.md", target)
        with self.assertRaises(contain.Refused):
            persona.scaffold_memory("victim")
        self.assertRedirectRefused(target, "persona.scaffold_memory")

    def test_persona_refs_readme_is_not_written_through_a_link(self):
        d = config.PERSONAS_DIR / "victim"
        d.mkdir(parents=True, exist_ok=True)
        target = self.outside_target("refs-victim.json")
        self.escaping_link(persona.refs_dir("victim") / "README.md", target)
        with self.assertRaises(contain.Refused):
            persona.scaffold_memory("victim")
        self.assertRedirectRefused(target, "persona.scaffold_memory refs/README.md")

    def test_a_workspace_vision_is_not_written_through_a_link(self):
        """`workspace.read_charter` already calls `contain.file_refusal`; `set_vision`
        reads *and* rewrites the same file with nothing in between. One file, one name,
        a guard on one side of it."""
        workspace.ensure("task")
        target = self.outside_target("vision-victim.json")
        self.escaping_link(workspace.charter_file("task"), target)
        with self.assertRaises(contain.Refused):
            workspace.set_vision("task", "ship it")
        self.assertRedirectRefused(target, "workspace.set_vision")

    def test_a_workspace_manifest_is_not_written_through_a_link(self):
        workspace.ensure("task")
        target = self.outside_target("manifest-victim.json")
        self.escaping_link(workspace.manifest_path("task"), target)
        with self.assertRaises(contain.Refused):
            workspace.write_manifest("task", {"repos": []})
        self.assertRedirectRefused(target, "workspace.write_manifest")

    def test_the_declared_default_workspace_is_not_written_through_a_link(self):
        workspace.ensure("task")
        target = self.outside_target("default-victim.json")
        self.escaping_link(config.WORKSPACES_DIR / ".default", target)
        with self.assertRaises(contain.Refused):
            workspace.set_declared_default("task")
        self.assertRedirectRefused(target, "workspace.set_declared_default")

    # ------------------------------------------------------------- the benign half
    def test_every_gated_site_still_works_on_an_ordinary_plane(self):
        """The half that catches a fix which contains writes by refusing them all —
        every site above, with nothing planted."""
        from charter import dispatch, pieces, skilluse
        self.make_persona("real")
        workspace.ensure("task")
        self.assertIsNotNone(dispatch.record("real"))
        self.assertIsNotNone(dispatch.record_advice())
        self.assertIsNotNone(skilluse.record("brainstorming"))
        self.assertIsNotNone(pieces.record("task", "claimed", "repo", "p1"))
        self.assertIsNotNone(pieces.seen("task", "repo", None, session="s"))
        persona.scaffold_memory("real")
        self.assertTrue(persona.index_of(persona.memory_dir("real")).is_file())
        workspace.set_vision("task", "ship it")
        self.assertIn("ship it", workspace.charter_file("task").read_text())
        workspace.write_manifest("task", {"repos": []})
        self.assertTrue(workspace.manifest_path("task").is_file())
        workspace.set_declared_default("task")
        self.assertEqual("task", (config.WORKSPACES_DIR / ".default").read_text().strip())


if __name__ == "__main__":
    unittest.main()
