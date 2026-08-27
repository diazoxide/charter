"""A shipped table's columns are sized from the values it holds, in CELLS (#592).

#508 fixed this for `persona stats` and left seven tables carrying the same shape. Three
of them ship it, and #592 enumerates them: **`worktree history`** pads a 25-character ISO
timestamp into 22, **`status`** pads `node-monorepo` (13) into 12, and **`recall`** sizes
its label column from the data — which is right — and then measures it with `len`, which
is not.

**Two of the three are wrong on values charter itself produces.** `pieces.record` writes
`datetime.isoformat(timespec="seconds")` on an aware datetime, which is 25 characters,
every time; `node-monorepo` is a stack name out of charter's own inventory vocabulary.
Neither needs an unusual name to misalign, which is why the fixtures below are mostly
ordinary and only some of them are awkward.

**Two probes, and they answer different questions.** The alignment probe asks whether the
columns line up; the readability probe asks whether the value is still there. #508 measured
why one cannot stand in for the other: `tui.pad` TRUNCATES, so restoring a mis-measured
width leaves every column lining up perfectly with the value quietly cut off inside one of
them. Every alignment assertion in #508's own suite passed against the restored constant.
So a table that is only checked for alignment is a table whose measuring is untested.

**And offsets are read in cells, never in characters.** `line.index(marker)` counts
characters, which is the unit that was wrong in the first place — a test using it reports
a CJK row as aligned while the terminal draws it eight columns out.

Fixtures are real names: one at each constant's boundary, one over it, one CJK, one
carrying a combining mark, one emoji, and a short ASCII control. The control is what says
a failure is about the awkward name rather than about the table.
"""

from __future__ import annotations

import io
import os
import re
import unicodedata
import unittest
from contextlib import redirect_stderr, redirect_stdout
from types import SimpleNamespace
from unittest import mock

from charter import (commands, commands_worktree, config, persona, pieces, tui,
                     workspace)
from tests._isolation import PersonaIso

#: A base character with a combining acute, built with `chr` so no editor and no
#: filesystem can quietly turn it into one codepoint.
#:
#: **`q` on purpose, and this is the whole fixture.** The obvious spelling —
#: `équipe-mémoire`, which is what #508's own roster fixture uses — has a PRECOMPOSED
#: `é` available, so the source file, the filesystem and `unicodedata.normalize` all
#: agree on one codepoint and `len` equals `tui.width`. That fixture is a silent control:
#: it passes against the very defect it is named for, measured, and it does. There is no
#: precomposed "q with acute" in Unicode, so this one stays two codepoints and one cell
#: wherever it is written down or read back.
#: :meth:`TableCase.assert_a_cell_and_a_character_disagree` keeps that true rather
#: than assumed.
COMBINING = "svc-q" + chr(0x0301) + "ueue"

#: The pair that makes `len`-sizing OBSERVABLE, and it took a mutation to find out that
#: nothing else does.
#:
#: A CJK name on its own does not catch a column sized with `len` and padded with
#: `tui.pad`: `pad` measures in cells, so every row is still padded to the same number of
#: cells and the table still lines up. And if some ASCII name is the widest by CHARACTERS,
#: the column it sizes is wide enough that the CJK value is not cut either — so the
#: readability probe passes too. Measured: a hand-check restored `len` here and the whole
#: file stayed green.
#:
#: What breaks is a value that is **widest in cells while another value is widest in
#: characters**. `len` then sizes the column to the ASCII name and `tui.pad` cuts the CJK
#: one down to it. So the fixture is a pair, and :meth:`TableCase.assert_the_widest_two
#: _disagree` is what keeps it one.
WIDEST_IN_CHARACTERS = "sixteen-char-svc"          # 16 characters, 16 cells
WIDEST_IN_CELLS = "日本語のリポジトリです"          # 11 characters, 22 cells


class TableCase(PersonaIso):
    """The two probes, and the environment every table below is rendered in."""

    def assert_a_cell_and_a_character_disagree(self, value: str) -> None:
        """Fixture guard: *value* must be one `len` and a different `tui.width`.

        A case named for the character-versus-cell defect that measures the same either
        way is not a case, it is a control that reads like one — and both directions have
        shipped in this repo (#588's UTF-8 round trip that passed on any UTF-8 machine).
        The value has to have gone through whatever normalisation the filesystem does
        before this is asked, which is why callers pass what they READ BACK.
        """
        self.assertNotEqual(
            len(value), tui.width(value),
            f"fixture: {value!r} is {len(value)} characters AND {tui.width(value)} cells, "
            "so `len` and `tui.width` agree about it and this case cannot fail")

    def assert_the_widest_two_disagree(self, by_chars: str, by_cells: str) -> None:
        """Fixture guard for the pair that makes `len`-sizing observable at all.

        *by_chars* must be the longer by `len` and *by_cells* the wider by `tui.width`.
        Only then does sizing with `len` produce a column too narrow for the value it
        holds — with any other pairing the column is wide enough by accident and the
        readability probe passes against the defect. See :data:`WIDEST_IN_CELLS`.
        """
        self.assertGreater(len(by_chars), len(by_cells), "fixture: wrong way round")
        self.assertGreater(tui.width(by_cells), tui.width(by_chars),
                           "fixture: wrong way round")

    def setUp(self) -> None:
        super().setUp()
        # Pinned rather than inherited: `$COLUMNS` is read by `tui` and has flipped five
        # tests in this suite before (#544). None of these tables clips to the terminal,
        # and pinning it is what keeps that true by detection rather than by hope.
        self.enterContext(mock.patch.dict(os.environ, {"COLUMNS": "220"}))

    @staticmethod
    def run_cmd(fn, args) -> list[str]:
        out = io.StringIO()
        with redirect_stdout(out), redirect_stderr(io.StringIO()):
            fn(args)
        return out.getvalue().splitlines()

    @staticmethod
    def cells_before(line: str, marker: str) -> int:
        """How many terminal CELLS precede *marker* on *line*.

        Not `line.index(marker)`. That is a character count, and a character count is the
        defect: it reports a CJK row as starting its tail where the ASCII row starts its
        own while the terminal draws them eight columns apart.
        """
        return tui.width(tui.strip_ansi(line)[:tui.strip_ansi(line).index(marker)])

    def assert_aligned(self, rows: list[str], marker: str, *, least: int = 2) -> None:
        """*marker* begins in the same cell on every row that carries it."""
        data = [r for r in rows if marker in tui.strip_ansi(r)]
        self.assertGreaterEqual(len(data), least,
                                f"{marker!r} is on {len(data)} row(s), needed {least} to "
                                f"compare offsets:\n" + "\n".join(rows))
        at = {self.cells_before(r, marker) for r in data}
        self.assertEqual(
            len(at), 1,
            f"the column holding {marker!r} starts at cells {sorted(at)} — different on "
            f"different rows, so it was sized by a guess or measured in characters:\n"
            + "\n".join(data))

    def assert_readable(self, rows: list[str], value: str) -> None:
        """*value* appears IN FULL on some row.

        The half alignment cannot see. `tui.pad` truncates whatever does not fit, so a
        column sized too small stays perfectly aligned and loses the end of its value —
        and a repo, piece or persona name a reader cannot read back off the row is one
        they cannot go and act on.
        """
        self.assertTrue(any(value in tui.strip_ansi(r) for r in rows),
                        f"{value!r} appears in full on no row — the column was sized by a "
                        f"guess and the value was cut to fit it:\n" + "\n".join(rows))


# --------------------------------------------------------------------------- #
# worktree history — `{ts:<22} {repo:<16} {piece:<24} {event:<10}`             #
# --------------------------------------------------------------------------- #
class WorktreeHistoryCase(TableCase):
    """`charter worktree history` reads ONLY the log, never git (its own docstring), so
    these rows are recorded rather than produced by a real worktree — the table under
    test is the rendering, and `tests/test_piece_history.py` covers the record."""

    WS = "alpha"

    #: One value per column shape the constants get wrong. `svc` and `slice` are the
    #: CONTROLS: they fit every constant, so they are drawn identically with the fix and
    #: without it, and a failure that names them is about the table rather than the name.
    REPOS = {
        "short ascii control": "svc",
        "exactly the old 16-char boundary": "sixteen-char-svc",
        "one character over the boundary": "seventeen-char-sv",
        "well over the boundary": "charter-control-plane-service",
        "CJK — two cells per glyph": "日本語のリポジトリ",
        "combining mark — zero cells": COMBINING,
        "emoji — two cells, one character": "🚀-launcher",
    }

    def setUp(self) -> None:
        super().setUp()
        workspace.ensure(self.WS)
        workspace.scaffold(self.WS)

    def record(self, repo: str, piece: str, *, event: str = "claimed",
               reason: str = "SO-SAID", when=None):
        pieces.record(self.WS, event, repo, piece, reason=reason, when=when)

    def history(self, **kw) -> list[str]:
        args = SimpleNamespace(repo=None, piece=None, workspace=self.WS, **kw)
        return self.run_cmd(commands_worktree.cmd_worktree_history, args)

    #: The tail every row ends with. `pieces.claimant` answers the same string for every
    #: row here (no persona, no session — the host), so a reason identical across rows
    #: gives one marker whose offset is the SUM of every width the renderer computed.
    TAIL = "— SO-SAID"


class TestWorktreeHistoryColumnsLineUp(WorktreeHistoryCase):
    def test_a_repo_name_over_the_constant_does_not_push_its_row(self):
        """One case per name, so a failure says WHICH kind of name broke the table."""
        for label, repo in self.REPOS.items():
            with self.subTest(name=label):
                pieces.dir_for(self.WS).exists() and [
                    f.unlink() for f in pieces.dir_for(self.WS).glob("*.jsonl")]
                self.record("svc", "slice")
                self.record(repo, "slice")
                self.assert_aligned(self.history(), self.TAIL)

    def test_the_whole_awkward_log_lines_up_at_once(self):
        """Together, not one at a time: the column is sized from the widest value in the
        table, so the interesting row is the one that is NOT the widest and still has to
        be padded to a width somebody else's name decided."""
        for repo in self.REPOS.values():
            self.record(repo, "slice")
        self.assert_aligned(self.history(), self.TAIL, least=len(self.REPOS))

    def test_a_piece_name_over_the_constant_does_not_push_its_row(self):
        """`{piece:<24}`, and a piece is named like a branch — `charter worktree add`
        names the branch after it — so twenty-four characters is a guess that ordinary
        work walks past. This branch's own name is thirty-four."""
        self.record("svc", "slice")
        self.record("svc", "fix-table-cells-and-frame-workspace")
        self.assert_aligned(self.history(), self.TAIL)

    def test_every_name_is_still_readable_off_its_row(self):
        """Sizing the column, not clipping to it — the half the alignment cases cannot
        see, because `tui.pad` truncates and the columns go on lining up while it does.

        Measured — with only the awkward names in here, restoring `len` left every case
        in this file green, because the widest name by characters was also wide enough in
        cells for every other value to fit inside it. The case that catches `len` is the
        one below, and it is a PAIR and nothing else."""
        for repo in self.REPOS.values():
            self.record(repo, "slice")
        rows = self.history()
        for repo in self.REPOS.values():
            with self.subTest(repo=repo):
                self.assert_readable(rows, repo)

    def test_the_widest_in_cells_is_readable_beside_the_widest_in_characters(self):
        """The two-value fixture, ALONE in its table — and alone is load-bearing.

        `len`-sizing is only observable when the value that decides the width and the
        value that overflows it are different values. Put a sixty-character ASCII name in
        the same table and it decides the width for both, the CJK one fits inside it with
        room to spare, and the case passes against exactly the defect it is named for.
        Measured: it did, in the sibling case above, until this one was split out."""
        self.assert_the_widest_two_disagree(WIDEST_IN_CHARACTERS, WIDEST_IN_CELLS)
        for repo in (WIDEST_IN_CHARACTERS, WIDEST_IN_CELLS):
            self.record(repo, "slice")
        rows = self.history()
        self.assert_readable(rows, WIDEST_IN_CELLS)
        self.assert_aligned(rows, self.TAIL)

    def test_a_log_line_missing_a_field_still_draws_a_row(self):
        """`events()` skips a line it cannot parse and keeps every line that has a
        `piece` — so a half-written record, or one from a charter that wrote fewer
        fields, arrives here with `ts`, `repo` or `event` simply absent. The `""`
        fallbacks are what stand between that and `tui.pad(None, w)`, which raises and
        takes the whole command down.

        A killed process mid-append is exactly what an append-only log collects, which is
        `events()`' own stated reason for tolerating it. Nothing was asserting that the
        renderer tolerates it too — found by the deletion sweep."""
        self.record("svc", "slice")
        log = pieces.log_path(self.WS)
        log.write_text(log.read_text() + '{"piece": "a-piece-and-nothing-else"}\n')
        rows = self.history()
        self.assertTrue(any("a-piece-and-nothing-else" in tui.strip_ansi(r)
                            for r in rows), "\n".join(rows))
        self.assert_aligned(rows, "slice", least=1)

    def test_the_timestamp_column_is_wider_than_the_timestamp_charter_writes(self):
        """`{ts:<22}` on the 25-character stamp `pieces.record` writes on every event.

        `str.format` pads and never truncates, so what a too-small field costs here is
        not a cut value — it is the SEPARATION, which is the only thing telling a reader
        where one column ends and the next begins. Every row of this table ran its
        timestamp into a single space in front of the repo name, and a single space is a
        word space rather than a column boundary.

        The fixture is charter's OWN stamp (`pieces.record`'s default `when`), not a
        constructed one, because that is #592's point: the constant was already wrong on
        the only value this column ever holds.
        """
        self.record("svc", "slice")
        row = next(r for r in self.history() if self.TAIL in r)
        plain = tui.strip_ansi(row)
        ts = plain.split()[0]
        self.assertGreaterEqual(len(ts), 25, f"fixture: {ts!r} is not the 25-character "
                                             "stamp #592 is about")
        after = plain[plain.index(ts) + len(ts):]
        self.assertGreaterEqual(
            len(after) - len(after.lstrip(" ")), 2,
            f"the timestamp column is narrower than the timestamp it holds, so it "
            f"degraded to a word space in front of the next column:\n{plain}")

    def test_a_newline_in_a_recorded_value_cannot_write_its_own_row(self):
        """The log is COMMITTED and append-only, so a value in it is somebody else's
        machine's. A format string counts a newline as one character and prints it as a
        line break, which shears every column below it; going through `tui.pad` means the
        kit's own `sanitize` sees it first."""
        self.record("svc\nFORGED  fake  fake  fake", "slice")
        rows = self.history()
        self.assertEqual([r for r in rows if tui.strip_ansi(r).startswith("FORGED")], [],
                         "\n".join(rows))


# --------------------------------------------------------------------------- #
# status — `  {repo:<38} {stack:<12} {note}`                                  #
# --------------------------------------------------------------------------- #
class StatusTableCase(TableCase):
    WS = "alpha"
    #: The third column, identical on every row, so its offset is the sum of the two
    #: measured widths in front of it. Stubbed because it shells out to git per clone and
    #: what is under test is the arithmetic, not the note.
    NOTE = "main · clean"

    def setUp(self) -> None:
        super().setUp()
        workspace.ensure(self.WS)
        workspace.scaffold(self.WS)
        self.enterContext(mock.patch.object(commands, "_clone_note",
                                            return_value=self.NOTE))

    def clone(self, name: str, stack: str = "python") -> str:
        # `workspace.clones` counts a directory as a clone only if it has a `.git` — that
        # is how `memory/` and `refs/` stay out of the list — so the fixture needs one.
        d = workspace.workspace_dir(self.WS) / name
        try:
            (d / ".git").mkdir(parents=True, exist_ok=True)
        except OSError:
            self.skipTest(f"filesystem refuses {name!r}")
        self.stacks[name] = {"name": name, "stack": stack}
        return name

    def on_disk(self, name: str) -> str:
        """The clone's name AS THE RENDERER WILL SEE IT — read back off the filesystem.

        `workspace.clones` lists directories, so a filesystem that normalises a name on
        the way in hands the table a different string than the fixture wrote. Asking the
        directory is the only way a fixture guard can speak for the value under test.

        Matched under NFC rather than by equality, because normalising is exactly the
        thing being looked for: an equality match would fail to find the very entry whose
        recomposition is the case worth catching."""
        want = unicodedata.normalize("NFC", name)
        d = workspace.workspace_dir(self.WS)
        return next((p.name for p in d.iterdir()
                     if unicodedata.normalize("NFC", p.name) == want), name)

    def status(self) -> list[str]:
        self.stacks = getattr(self, "stacks", {})
        with mock.patch.object(commands.inventory, "repos",
                               return_value=list(self.stacks.values())):
            return self.run_cmd(commands.cmd_status,
                                SimpleNamespace(workspace=self.WS, all=False))


class TestStatusColumnsLineUp(StatusTableCase):
    def setUp(self) -> None:
        super().setUp()
        self.stacks: dict[str, dict] = {}

    def test_the_stack_charter_itself_names_does_not_push_its_row(self):
        """`node-monorepo` is thirteen characters in a column of twelve, and it is one of
        charter's OWN stack names — so this table has never lined up on a plane holding a
        node monorepo. The `go` row is what it fails to line up WITH."""
        self.clone("svc", stack="go")
        self.clone("web", stack="node-monorepo")
        self.assert_aligned(self.status(), self.NOTE)

    def test_the_header_lines_up_with_the_rows_it_heads(self):
        """The header used to go through the same format string as the rows, which looks
        like one code path and is not: `{:<12}` agrees with itself only while every value
        is inside the constant. A header sitting one column left of the values under it is
        the tell #508 names — the thing a reader notices before any row does."""
        self.clone("web", stack="node-monorepo")
        rows = self.status()
        head = next(r for r in rows if "BRANCH / NOTE" in r)
        row = next(r for r in rows if self.NOTE in r)
        self.assertEqual(self.cells_before(head, "BRANCH / NOTE"),
                         self.cells_before(row, self.NOTE),
                         "\n".join([head, row]))

    def test_a_clone_the_inventory_does_not_know_still_gets_a_stack_cell(self):
        """`inv_by_name.get(name, {}).get("stack", "?")` — two fallbacks, and both are
        reachable: a clone made by hand, or one whose repo has left the inventory, has no
        entry at all. Without them the cell is `None`, `tui.pad` raises on it, and
        `charter status` — the command whose whole job is "where am I" — dies on the one
        plane where the question is hardest to answer another way.

        Found by the deletion sweep: nothing asserted either fallback.
        """
        self.clone("svc", stack="go")
        unknown = self.clone("cloned-by-hand")
        del self.stacks[unknown]          # in the workspace, absent from the inventory
        rows = self.status()
        self.assert_readable(rows, "?")
        self.assert_aligned(rows, self.NOTE)

    def test_a_repo_name_past_the_constant_does_not_push_its_row(self):
        self.clone("svc")
        self.clone("a-repo-name-far-past-any-fixed-column-width-somebody-guessed")
        self.assert_aligned(self.status(), self.NOTE)

    def test_a_cjk_repo_name_does_not_push_its_row(self):
        """The `len` half, which no constant can fix: two cells per glyph, so a name well
        inside any budget still shifts its row by its own length."""
        name = self.clone("日本語のリポジトリ")
        self.clone("svc")
        rows = self.status()
        self.assert_a_cell_and_a_character_disagree(self.on_disk(name))
        self.assert_aligned(rows, self.NOTE)

    def test_a_combining_mark_does_not_pull_its_row(self):
        """The other direction, and the one a bigger constant makes WORSE: a combining
        mark is ZERO cells, so `len` over-pads and the row drifts right.

        The fixture is checked as it came back OFF THE FILESYSTEM, because that is where
        this one can quietly stop being a fixture — a filesystem that composes the pair
        into one codepoint hands the renderer a value `len` and `tui.width` agree about,
        and the case then passes against the defect it is named for."""
        name = self.clone(COMBINING)
        self.clone("svc")
        rows = self.status()
        self.assert_a_cell_and_a_character_disagree(self.on_disk(name))
        self.assert_aligned(rows, self.NOTE)

    def test_every_repo_and_stack_is_still_readable_off_its_row(self):
        """A constant's half: a name and a stack past the width somebody guessed."""
        long_repo = "a-repo-name-far-past-any-fixed-column-width-somebody-guessed"
        self.clone("svc", stack="go")
        self.clone(long_repo, stack="node-monorepo")
        rows = self.status()
        for value in (long_repo, "node-monorepo"):
            with self.subTest(value=value):
                self.assert_readable(rows, value)

    def test_the_widest_in_cells_is_readable_beside_the_widest_in_characters(self):
        """`len`'s half, and it needs the pair ALONE in the table.

        The width has to be decided by one value and overflowed by another. A sixty-
        character ASCII repo name in the same table decides it for both, the CJK name fits
        inside with room to spare, and the case passes against the defect — measured, in
        the sibling case above, until this one was split out of it."""
        self.assert_the_widest_two_disagree(WIDEST_IN_CHARACTERS, WIDEST_IN_CELLS)
        self.clone(WIDEST_IN_CHARACTERS, stack="go")
        self.clone(WIDEST_IN_CELLS, stack="python")
        rows = self.status()
        self.assert_readable(rows, WIDEST_IN_CELLS)
        self.assert_aligned(rows, self.NOTE)


# --------------------------------------------------------------------------- #
# recall — `  {date:<10}  {label:<len(widest)}  {title}`                       #
# --------------------------------------------------------------------------- #
class RecallCase(TableCase):
    """`recall` is the one of the three that already sized its column from the data.

    Which is why it is here: sizing is half the answer, and this half is the one a reader
    of the code would call done. `max(len(h.label) …)` measures CHARACTERS, so a base
    label carrying a persona's own directory name misaligns every row of the report
    without any value going near a constant — there is no constant left to go near.
    """

    WS = "w"
    QUERY = "keycloak"

    def setUp(self) -> None:
        super().setUp()
        workspace.ensure(self.WS)
        workspace.scaffold(self.WS)
        from charter import memstore
        memstore.write(workspace.memory_dir(self.WS),
                       "the keycloak token rotates every ninety days",
                       title="keycloak token policy", timestamped=True)

    def with_persona(self, name: str) -> str:
        try:
            self.make_persona(name, role="Role")
        except OSError:
            self.skipTest(f"filesystem refuses {name!r}")
        persona.remember(name, "keycloak deploys need the staging realm first")
        return name

    def persona_on_disk(self, name: str) -> str:
        """The persona DIRECTORY name as `recall` will label it — read back, matched
        under NFC, for the reason `StatusTableCase.on_disk` gives."""
        want = unicodedata.normalize("NFC", name)
        return next((p.name for p in config.PERSONAS_DIR.iterdir()
                     if unicodedata.normalize("NFC", p.name) == want), name)

    def recall(self, name: str) -> list[str]:
        return self.run_cmd(commands.cmd_recall, SimpleNamespace(
            query=self.QUERY, scope=None, ephemeral=False, persona=name,
            workspace=self.WS, all_workspaces=False, since=None, limit=8, full=False))

    @staticmethod
    def hit_rows_or_undated(rows: list[str]) -> list[str]:
        """Hit rows whose date column holds the `—` an undated memory renders."""
        return [r for r in rows if re.match(r"^ {2}—", tui.strip_ansi(r))]

    @staticmethod
    def hit_rows(rows: list[str]) -> list[str]:
        """Only the `date · label · title` rows.

        The report interleaves each hit with its ADDRESS line, and a memory's filename is
        slugged from its title — so the query word appears on both, and an offset probe
        that took the address rows for hit rows would compare a hanging indent against a
        column. The two are different kinds of line; only one of them is the table.
        """
        return [r for r in rows
                if re.match(r"^ {2}\d{4}-\d{2}-\d{2} ", tui.strip_ansi(r))]


class TestRecallColumnsLineUp(RecallCase):
    def test_a_cjk_persona_label_does_not_push_its_row(self):
        """`persona:日本語のペルソナ` beside `workspace:w`: eight glyphs the terminal draws
        as sixteen cells, padded by `len` to eight — so the TITLE column on that row lands
        eight columns left of every other row's."""
        name = self.with_persona("日本語のペルソナ")
        self.assert_aligned(self.hit_rows(self.recall(name)), "keycloak", least=2)

    def test_a_combining_mark_in_a_label_does_not_pull_its_row(self):
        """The direction a bigger constant makes worse — zero cells, one character, so
        `len` over-pads and the row drifts RIGHT of every other. The fixture is guarded as
        the persona directory came back off the filesystem: a normalising filesystem hands
        the label one codepoint, `len` and `tui.width` then agree, and the case would pass
        against exactly the defect it is named for."""
        name = self.with_persona(COMBINING)
        rows = self.hit_rows(self.recall(name))
        self.assert_a_cell_and_a_character_disagree(self.persona_on_disk(name))
        self.assert_aligned(rows, "keycloak", least=2)

    def test_an_ascii_roster_is_the_control(self):
        """Named a control so a failure here reads as "the table broke", not "the awkward
        name broke it". `len` and `tui.width` agree on every label in this case."""
        self.assert_aligned(self.hit_rows(self.recall(self.with_persona("dev"))),
                            "keycloak", least=2)

    def test_every_label_is_still_readable_off_its_row(self):
        name = self.with_persona("日本語のペルソナ")
        rows = self.recall(name)
        self.assert_readable(rows, f"persona:{name}")
        self.assert_readable(rows, f"workspace:{self.WS}")

    def test_the_address_hangs_under_the_label_it_addresses(self):
        """The address line's indent used to be `14` written out, which is `2 + 10 + 2` —
        correct only while the date column is exactly ten wide. Pinned as a relationship
        so the two cannot drift apart the first time either end moves."""
        rows = self.recall(self.with_persona("dev"))
        hit = next(i for i, r in enumerate(rows) if "keycloak" in tui.strip_ansi(r))
        addr = tui.strip_ansi(rows[hit + 1])
        row = tui.strip_ansi(rows[hit])
        date = row.split()[0]
        self.assertEqual(len(addr) - len(addr.lstrip(" ")),
                         row.index(date) + len(date) + 2,
                         "\n".join([row, addr]))

    def test_an_undated_result_narrows_the_date_column_and_the_indent_with_it(self):
        """The case that makes both of those measurable at all.

        Every dated hit renders a ten-character ISO date, so a `10`-wide constant and a
        `14`-wide indent are correct for every dated report — a hand-check restored both
        and nothing went red. An UNDATED memory renders `—`, one cell, and a column sized
        from the values is then three wide and the indent five. That is the difference
        between a column that is measured and one that happens to be right.

        Undated is ordinary rather than exotic: `recall` counts them (`got.undated`) and
        a memory file with no recorded date is what `memstore` writes when nothing dated
        it.
        """
        name = self.with_persona("dev")
        # A memory's date is its in-body `_YYYY-MM-DD …_` stamp, falling back to a
        # `YYYYMMDD-` filename prefix (`memstore.memory_date`). BOTH have to go, and the
        # filename one is why the workspace journal file is renamed rather than edited —
        # `memstore.write(timestamped=True)` puts the date in the name as well as in the
        # body, so stripping only the stamp leaves the hit dated and the fixture inert.
        for base in (persona.memory_dir(name), workspace.memory_dir(self.WS)):
            for f in list(base.glob("*.md")):
                body = re.sub(r"^_\d{4}-\d{2}-\d{2}[^\n]*$", "_undated_", f.read_text(),
                              flags=re.M)
                f.write_text(body)
                f.rename(base / re.sub(r"^\d{8}-\d{6}-", "", f.name))
        rows = self.hit_rows_or_undated(self.recall(name))
        self.assertTrue(rows, "fixture: no hits at all")
        self.assertTrue(all(tui.strip_ansi(r).lstrip().startswith("—") for r in rows),
                        "fixture: a hit still carries a date, so the date column is ten "
                        "wide and this case cannot fail:\n" + "\n".join(rows))
        row = tui.strip_ansi(rows[0])
        self.assertEqual(row.index("—"), 2, row)
        # The label starts one gap past a column holding a single cell — not eleven.
        self.assertLess(tui.width(row[:row.index("persona:")]), 10, row)
        # And the address line follows the column it hangs under. This is the assertion
        # that makes the hardcoded `14` fail: with every hit dated, `2 + dw` IS 14 and a
        # literal is indistinguishable from the expression.
        addr = tui.strip_ansi(next(ln for ln in self.recall(name)
                                   if "/memory/" in ln))
        self.assertEqual(len(addr) - len(addr.lstrip(" ")),
                         row.index("persona:"), "\n".join([row, addr]))


if __name__ == "__main__":  # pragma: no cover
    unittest.main()
