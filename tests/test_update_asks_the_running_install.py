"""#537: `charter update` answers questions about the running install from the install.

Two halves of one defect, facing opposite directions, and both were reproduced on a real
machine rather than derived.

**Inside a plane it inferred the CLI from the cwd.** `cmd_update` asked
`doctor._is_charter_checkout(config.ROOT)` — *is the directory I am standing in a charter
clone* — and answered with it a different question: *is the charter the operator runs this
tree*. A maintainer has both, because charter documents both: `CONTRIBUTING.md` says
`python3 -m charter …` from the clone, and the dev channel says `uv tool install
git+…@main`. Standing in the clone with the second on `PATH`, the command printed *"the
charter you run is this checkout, moved by git"*, installed nothing, and left the CLI three
commits stale. The refutation is checkable and was checked: `charter --version` said `main
@ e17801c` while the clone's `HEAD` said `97163fb`, and the console script's shebang named
`~/.local/share/uv/tools/charter-cp/bin/python3`. If the running CLI were the tree, two
commits could not disagree.

**Outside a plane it inferred the CHANNEL from the plane's absence.** `cd /tmp && charter
update` found no `[update] channel`, read that absence as *stable*, resolved the published
version — which equals the number a dev build reports, because dev builds are never
published — installed nothing, and then failed its own verification, because `charter
--version` on a dev build ends in a commit rather than in that number:

    ✗ the install did not take: the installed `charter` reports charter 0.53.0+dev
      (main @ e17801c), expected 0.53.0

**The property is one sentence**: a question about the running install is answered from the
running install. `charter.__file__` says where this charter loaded from and the PEP 610
`direct_url.json` says what installed it — the same record `+dev (main @ …)` is already
produced from. Both are direct, and neither is adjacent to anything.

It is the shape this month keeps returning: a check matching a spelling instead of a
property (#547, #558, #576). Here the spellings were "the cwd looks like a clone" and "no
charter.toml above me", and the properties are "this module loaded from there" and "this
build came from git".
"""

from __future__ import annotations

import argparse
import contextlib
import io
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from unittest import mock

import charter
from charter import channel, commands_update, config, doctor
from tests._isolation import PersonaIso

#: A PEP 610 record of the install `docs/install.md` documents for the dev channel. Shaped
#: like the real file rather than like the minimum `_vcs_info` reads, so a change that
#: tightened the reader would be measured against a real record.
_GIT_INSTALL = {
    "url": "https://github.com/diazoxide/charter",
    "vcs_info": {"vcs": "git", "commit_id": "e17801c" + "0" * 33,
                 "requested_revision": "main"},
}


@contextlib.contextmanager
def installed_by(record):
    """Run the block with the PEP 610 record this charter reports as its own.

    Writes `channel`'s memo directly and resets it after. That memo is documented as a
    per-process cache with one writer and a named reset — an install replaces the
    interpreter's own package, so a running process is the build it started as — and this
    is the seam that exists for exactly this.
    """
    channel._direct_url = record
    try:
        yield
    finally:
        channel._reset_cache_for_tests()


class WhereThisCharterLoadedFrom(unittest.TestCase):
    """`channel.package_dir` / `running_inside`, measured against the real running charter.

    Nothing is patched in this class on purpose. The suite imports charter from somewhere,
    and whatever that somewhere is, these have to be true of it — which is the only way to
    check a function whose whole job is to report a fact about this process.
    """

    def test_the_package_directory_is_the_one_holding_this_module(self):
        self.assertEqual(channel.package_dir(),
                         Path(charter.__file__).resolve().parent)
        self.assertTrue((channel.package_dir() / "channel.py").is_file())

    def test_the_tree_this_charter_came_from_contains_it(self):
        self.assertTrue(channel.running_inside(channel.package_dir().parent))
        self.assertTrue(channel.running_inside(channel.package_dir()))

    def test_and_a_directory_it_did_not_come_from_does_not(self):
        with tempfile.TemporaryDirectory() as d:
            self.assertFalse(channel.running_inside(d))
        self.assertFalse(channel.running_inside(Path("/nonexistent/charter")))

    def test_a_directory_whose_NAME_is_a_prefix_is_not_an_ancestor(self):
        """The `startswith` trap, and the fixture that actually springs it.

        A sibling — `<parent>/charter-old` beside `<parent>/charter` — is the obvious
        fixture and it does not catch anything: a string prefix answers it correctly, since
        the sibling's name is LONGER. What a prefix comparison gets wrong is the other
        direction, a directory whose whole path spells the opening of this one:
        `…/wt/chart` is a string prefix of `…/wt/charter` and is an ancestor of nothing.
        Written after the version without it survived being replaced by `startswith`.
        """
        here = channel.package_dir()
        self.assertFalse(channel.running_inside(str(here)[:-2]))
        self.assertFalse(channel.running_inside(here.parent / (here.name + "-old")))
        self.assertFalse(channel.running_inside(str(here) + "-old"))

    def test_a_plane_root_behind_a_symlink_is_still_the_tree(self):
        """Both ends resolve, for the reason `contain.within_data` gives one module over: a
        macOS temp directory lives under `/var/folders/…`, which is itself a link to
        `/private/var/…`, and any plane behind a linked mount is the same shape. Comparing
        a resolved package directory against an unresolved root answers "not the tree" for
        a plane that IS the tree — and this refusal's whole job is to fire there.

        Residue, stated: this pins the ROOT side. The package side is `Path(__file__)
        .resolve()`, and whether that resolve is load-bearing depends on where the suite
        was imported from, so no fixture here can decide it.
        """
        here = channel.package_dir()
        with tempfile.TemporaryDirectory() as d:
            link = Path(d) / "plane"
            link.symlink_to(here.parent, target_is_directory=True)
            self.assertTrue(channel.running_inside(link))
            self.assertFalse(channel.running_inside(Path(d) / "elsewhere"))

    def test_it_answers_rather_than_raises_on_a_path_it_cannot_resolve(self):
        """Same promise the refusal helpers in `contain` keep, and bounded the same way:
        `OSError` and `ValueError` are conditions a real root can be in — a NUL in the
        string, a resolve that the filesystem refuses — and they answer False. A `TypeError`
        from handing this an `int` is a defect in the caller and is left to raise, because
        catching it would make `running_inside` report "not the tree" for a bug."""
        for bad in ("", "\x00/x", Path("")):
            with self.subTest(root=repr(bad)):
                self.assertIsInstance(channel.running_inside(bad), bool)


class TheChannelOutsideAPlaneIsTheBuilds(unittest.TestCase):
    """`update_is_dev` — the plane's answer where there is a plane, the build's where not.

    `is_dev` and `is_dev_build` stay two functions, because they are two facts that
    routinely and legitimately disagree. This is the one place they are joined, and it is
    joined for a stated reason rather than by a caller reaching for whichever is handy.
    """

    @contextlib.contextmanager
    def _as(self, *, plane, declares="stable", build):
        with mock.patch.object(config, "HAS_CONTROL_PLANE", plane), \
                mock.patch.object(config, "UPDATE", {"channel": declares}), \
                installed_by(build):
            yield

    def test_with_no_plane_a_git_install_follows_the_dev_channel(self):
        with self._as(plane=False, build=_GIT_INSTALL):
            self.assertTrue(channel.is_dev_build())
            self.assertTrue(channel.update_is_dev())

    def test_with_no_plane_a_pypi_install_follows_the_release_channel(self):
        """PEP 610 writes no `direct_url.json` for an install resolved from an index, so
        the file's absence is the positive statement "this came from PyPI"."""
        with self._as(plane=False, build=None):
            self.assertFalse(channel.is_dev_build())
            self.assertFalse(channel.update_is_dev())

    def test_a_plane_that_exists_decides_it_whatever_is_installed(self):
        """A plane that says nothing about the channel has still SAID something: the
        default is stable and the plane keeps it. Only the absence of a plane is an absence
        of an answer, which is why this branches on whether a plane exists rather than on
        whether the key was found."""
        for declares, build, expected in (
            ("dev", None, True),            # opted in, still on the wheel — the ordinary
            ("dev", _GIT_INSTALL, True),    # first day of the dev channel
            ("stable", _GIT_INSTALL, False),  # the plane's word outranks the build
            ("stable", None, False),
        ):
            with self.subTest(declares=declares, git=bool(build)):
                with self._as(plane=True, declares=declares, build=build):
                    self.assertEqual(channel.update_is_dev(), expected)

    def test_a_non_git_direct_url_is_not_a_dev_build(self):
        """`uv tool install .` from a checkout writes a `dir_info` record, which
        `build_label` already reports as `+local`. It came from a direct URL and there is
        no git ref to follow, so it is not the dev channel."""
        with self._as(plane=False, build={"url": "file:///tmp/charter",
                                          "dir_info": {"editable": False}}):
            self.assertFalse(channel.is_dev_build())
            self.assertFalse(channel.update_is_dev())


class TheRefusalIsAboutTheRunningInstall(PersonaIso):
    """The first half, end to end through `cmd_update` with nothing patched to make it so.

    The plane root is built into a charter checkout on disk — `charter/docsrc.py` and a
    `pyproject.toml` naming the distribution, which is every test `doctor` applies — and
    the charter running the suite is elsewhere. That is the state the issue was filed from.
    """

    def setUp(self):
        super().setUp()
        (self.tmp / "charter").mkdir(parents=True, exist_ok=True)
        (self.tmp / "charter" / "docsrc.py").write_text("")
        (self.tmp / "pyproject.toml").write_text('name = "charter-cp"\n')
        (self.tmp / "charter.toml").write_text('schema = 1\n[update]\nchannel = "dev"\n')
        config.use(self.tmp)
        self.sync = self.enterContext(
            mock.patch.object(commands_update, "_sync_dev", return_value=(True, "spec")))
        self.enterContext(mock.patch.object(commands_update, "_move_harness"))
        self.enterContext(mock.patch.object(commands_update, "_refresh_plugin"))
        self.enterContext(
            mock.patch.object(commands_update, "_handoff_dev", return_value=(True, "ok")))

    def _run(self):
        err = io.StringIO()
        with redirect_stderr(err), redirect_stdout(io.StringIO()):
            code = commands_update.cmd_update(argparse.Namespace(to=None, bump=False))
        return code, err.getvalue()

    def test_the_two_questions_genuinely_disagree_here(self):
        """Stated as its own assertion, because every claim below rests on it. If these
        ever agree, the tests underneath pass for a reason that is not the one written."""
        self.assertTrue(doctor._is_charter_checkout(self.tmp))
        self.assertFalse(channel.running_inside(self.tmp))

    def test_standing_in_a_clone_no_longer_refuses_the_install(self):
        code, _err = self._run()
        self.assertEqual(code, 0)
        self.sync.assert_called_once()

    def test_and_it_names_the_install_it_moved(self):
        """The issue's second ask. Two cases printed nothing that told them apart, so an
        operator read "the charter you run is this checkout" about a `uv tool` install and
        copied the install command out of `commands_update.py` instead — a dozen times in
        two days.
        """
        _code, err = self._run()
        self.assertIn(str(channel.package_dir()), err)

    def test_it_does_not_consult_where_you_are_standing_at_all(self):
        """Not "it consults it and then ignores it". A second reader of the cwd is a second
        place for this to come back, so the update path has none."""
        with mock.patch.object(doctor, "_is_charter_checkout",
                               side_effect=AssertionError("cwd was read")):
            code, _err = self._run()
        self.assertEqual(code, 0)


class AVersionNumberCannotTellADevBuildFromItsRelease(PersonaIso):
    """The second half: `target != installed` compares two numbers a dev build shares.

    A dev build reports the version of the release it was built from — that is why they are
    never published — so on a plane that has one, an update to the published version found
    nothing to do and then failed to verify the thing it had not done.
    """

    def setUp(self):
        super().setUp()
        (self.tmp / "charter.toml").write_text('schema = 1\n[update]\nchannel = "stable"\n')
        config.use(self.tmp)
        self.enterContext(mock.patch.object(config, "HAS_CONTROL_PLANE", True))
        self.enterContext(
            mock.patch.object(commands_update, "_installed_version", lambda: "0.53.0"))
        self.enterContext(
            mock.patch.object(commands_update, "_latest", lambda live=True: "0.53.0"))
        self.enterContext(mock.patch.object(commands_update, "_move_harness"))
        self.moved: list[str] = []
        self.enterContext(mock.patch.object(
            commands_update, "_sync_to",
            side_effect=lambda v: (self.moved.append(v), (True, v))[1]))
        self.enterContext(
            mock.patch.object(commands_update, "_handoff", return_value=(True, "")))

    def _run(self):
        err = io.StringIO()
        with redirect_stderr(err), redirect_stdout(io.StringIO()):
            code = commands_update.cmd_update(argparse.Namespace(to=None, bump=False))
        return code, err.getvalue()

    def test_a_dev_build_under_a_stable_plane_is_moved_onto_the_wheel(self):
        with installed_by(_GIT_INSTALL):
            code, err = self._run()
        self.assertEqual(code, 0)
        self.assertEqual(self.moved, ["0.53.0"])
        # And it says which of the two "0.53.0"s it is installing, rather than printing
        # `installing charter 0.53.0 → 0.53.0 …` at somebody.
        self.assertIn("over the dev build", err)

    def test_and_a_wheel_already_on_the_target_still_installs_nothing(self):
        """The idempotence the module docstring promises. The new condition may only add
        the case a version number cannot see; it may not turn every re-run into an
        install."""
        with installed_by(None):
            code, _err = self._run()
        self.assertEqual(code, 0)
        self.assertEqual(self.moved, [])


if __name__ == "__main__":
    unittest.main()
