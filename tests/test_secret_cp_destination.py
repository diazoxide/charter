"""`secret cp` writes a credential to a real file, or to nothing at all.

`docs/secrets.md` calls `cp` one of the two *safe* consumption paths — "prints only the
path, never the contents". That was true of the command and false of the destination,
which decided what "writing a file" meant:

    $ charter secret cp tv API_TOKEN /dev/stdout 2>&1 | cat
    FAKE-SEKRIT-…✓ Wrote 'tv/API_TOKEN' to /dev/stdout (0600). Value not shown.

The success line is false on its own output. `cmd_secret_get --reveal` refuses exactly
this channel (`sys.stdout.isatty()`); `cp` refused nothing — not a device, not a FIFO,
not a symlink aimed at a victim file, and not an existing `~/.ssh/config` it truncated
and chmodded 0600 without a word.

These tests are written against the *class*, not the one string in the report: every
non-regular kind, both spellings of "the agent's own pipe", the symlink that gets past a
naive `S_ISREG` check on the target, and the pre-existing file. The fabricated value
below is not a credential and appears in no assertion message.

Round one of that fix was bypassed, and the bypass is the reason for
`TestCharterOwnStreamsAreRefusedByIdentity` below. The first guard asked what a path was
CALLED — `os.lstat` on the path — and `/dev/fd/1` is not a symlink and not a device on
macOS, it is the underlying object, so it read as an ordinary existing file and fell into
the arm `--force` switches off. `charter secret cp v k /dev/fd/1 --force` wrote the value
into charter's own captured stdout and printed "Value not shown." after it. No list of
names closes that: `/dev/stdout`, `/dev/fd/1`, `/proc/self/fd/1`, the readlink'd log path
and any hardlink to it are one inode with five names. The guard now compares IDENTITY —
`(st_dev, st_ino)` against `os.fstat(0/1/2)` — which has one answer per object.
"""

from __future__ import annotations

import io
import json
import os
import shutil
import stat
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from charter import commands_secrets as cs
from charter import config
from charter.secrets import registry
from tests._isolation import PersonaIso

#: Fabricated. Long and distinctive so "did this reach stdout?" is a real question.
VALUE = "FABRICATED-not-a-real-credential-8f2a9c"


class CpCase(PersonaIso):
    def setUp(self) -> None:
        super().setUp()
        vf = config.ROOT / ".charter" / "vaults" / "v.json"
        vf.parent.mkdir(parents=True, exist_ok=True)
        vf.write_text(json.dumps({"k": VALUE}))
        registry.add_vault("v", "plain-file", {"file": str(vf)})
        # OUTSIDE the plane on purpose: that is where `cp`'s documented destination
        # lives (a kubeconfig under ~), and it keeps these cases clear of the
        # git-containment rule, which has its own class below.
        self.out_dir = Path(tempfile.mkdtemp(prefix="cp-dest-"))
        self.addCleanup(shutil.rmtree, self.out_dir, True)

    def cp(self, dest, force: bool = False):
        """Run `secret cp` capturing BOTH streams — the leak under test is on stdout."""
        args = SimpleNamespace(vault="v", key="k", dest=str(dest), force=force)
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            rc = cs.cmd_secret_cp(args)
        return rc, out.getvalue(), err.getvalue()

    def assertRefused(self, rc, out, err):
        self.assertEqual(rc, 2, "a refused destination must be a non-zero exit")
        self.assertNotIn(VALUE, out, "the secret reached stdout")
        self.assertNotIn(VALUE, err, "the secret reached stderr")
        self.assertIn("Refusing to write a secret", err)


class TestANonRegularDestinationIsRefused(CpCase):
    def test_a_character_device_destination_is_refused(self):
        """The headline case, in its most literal form."""
        rc, out, err = self.cp("/dev/null")
        self.assertRefused(rc, out, err)
        self.assertIn("character device", err)

    def test_dev_stdout_is_refused(self):
        """`/dev/stdout` is charter's own stdout — the agent's captured pipe. Whatever
        it resolves to on this platform (a symlink on macOS and Linux, a character
        device elsewhere), it is not a file charter may write a credential to."""
        rc, out, err = self.cp("/dev/stdout")
        self.assertRefused(rc, out, err)

    def test_dev_stderr_and_dev_fd_are_the_same_channel(self):
        """Named separately because a guard written against the one string in the report
        would let these two through, and they are the same pipe."""
        for spelling in ("/dev/stderr", "/dev/fd/1", "/dev/fd/2"):
            with self.subTest(dest=spelling):
                rc, out, err = self.cp(spelling)
                self.assertRefused(rc, out, err)

    def test_a_fifo_is_refused(self):
        """Worse than a device: opening a FIFO for writing BLOCKS until a reader
        appears, with the plaintext already resolved and charter hung holding it."""
        fifo = self.out_dir / "pipe"
        os.mkfifo(fifo)
        rc, out, err = self.cp(fifo)
        self.assertRefused(rc, out, err)
        self.assertIn("FIFO", err)

    def test_a_directory_is_refused(self):
        rc, out, err = self.cp(self.out_dir)
        self.assertRefused(rc, out, err)
        self.assertIn("directory", err)


class TestCharterOwnStreamsAreRefusedByIdentity(CpCase):
    """The bypass that survived round one, and the reason it survived.

    Round one asked what the destination was CALLED. `os.lstat('/dev/fd/1')` on macOS
    does not report a symlink and does not report a device — `/dev/fd/N` there is the
    underlying object re-opened, so it reported mode 0o100200, a plain regular file. The
    guard therefore fell through to its "already exists" arm, whose whole purpose is to
    be switched off by `--force`. Measured against the branch::

        charter secret cp v k /dev/fd/1 --force
        -> rc=0, the value in the file behind fd 1, and
           "✓ Wrote 'v/k' to /dev/fd/1 (0600). Value not shown." on top of it

    which is issue #421's headline symptom reproduced through its own fix. Worse, the
    no-`--force` refusal read "Pass --force to overwrite it deliberately": the guard
    printing the recipe for its own bypass, the pattern #421 was filed about.

    These cases all pin IDENTITY — `(st_dev, st_ino)` against `os.fstat(0/1/2)` — which
    has one answer per object no matter how many names point at it. They run with fd 1
    and fd 2 pointed at regular files this test owns, because that is the reviewer's
    condition (and this harness's): the pre-existing `/dev/fd/1` case passed only because
    the runner's fd 1 happened to be a pipe, and `assertRefused` accepted the "already
    exists" message as though it were the device refusal.
    """

    def setUp(self) -> None:
        super().setUp()
        self.transcript = self.out_dir / "transcript.log"
        self.transcript.write_text("earlier conversation\n")
        os.chmod(self.transcript, 0o644)
        self.saved = {}
        for fd in (1, 2):
            opened = os.open(self.transcript, os.O_WRONLY | os.O_APPEND)
            self.saved[fd] = os.dup(fd)
            os.dup2(opened, fd)
            os.close(opened)
        # Named for what it restores, which is now a style point rather than a trap:
        # `PersonaIso` used to register a cleanup called `_restore`, so a subclass method
        # of that name replaced it — this fixture would have run twice (EBADF on the
        # second) and the plane isolation would never have been undone. That cleanup is
        # name-mangled since #459 and cannot be shadowed.
        self.addCleanup(self._restore_streams)

    def _restore_streams(self) -> None:
        for fd, saved in self.saved.items():
            os.dup2(saved, fd)
            os.close(saved)

    def assertTranscriptIntact(self):
        """Nothing written, nothing truncated, and the mode not quietly changed.

        The last one is its own assertion because the broken version's `fchmod` landed
        on the descriptor it had opened — charter's own stdout — and left the harness's
        transcript at 0600.
        """
        self.assertNotIn(VALUE, self.transcript.read_text(),
                         "the secret was written into charter's own stream")
        self.assertEqual(self.transcript.read_text(), "earlier conversation\n",
                         "charter's own stream was truncated or appended to")
        self.assertEqual(stat.S_IMODE(self.transcript.stat().st_mode), 0o644,
                         "a refused write still chmodded charter's own stream")

    def test_the_file_behind_stdout_is_refused_under_its_own_name(self):
        """The plainest spelling and the one no name-list would ever contain: the real
        path of the file charter's stdout is redirected to."""
        rc, out, err = self.cp(self.transcript, force=True)
        self.assertRefused(rc, out, err)
        self.assertIn("charter's own standard output", err)
        self.assertTranscriptIntact()

    def test_dev_fd_1_backed_by_a_regular_file_is_refused(self):
        """The reviewer's exact input, without `--force`."""
        rc, out, err = self.cp("/dev/fd/1")
        self.assertRefused(rc, out, err)
        self.assertTranscriptIntact()

    def test_dev_fd_1_backed_by_a_regular_file_is_refused_under_force(self):
        """The reviewer's exact input. `--force` was the bypass; it must now buy nothing
        but a different refusal path to the same answer."""
        rc, out, err = self.cp("/dev/fd/1", force=True)
        self.assertRefused(rc, out, err)
        self.assertTranscriptIntact()

    def test_dev_fd_2_is_the_same_channel_by_another_number(self):
        rc, out, err = self.cp("/dev/fd/2", force=True)
        self.assertRefused(rc, out, err)
        self.assertTranscriptIntact()

    def test_a_second_hard_name_for_the_transcript_is_refused(self):
        """One inode, two names, and the second one is in no table of spellings. Identity
        is what makes this case free; a path check would have to enumerate it."""
        other = self.out_dir / "not-obviously-the-transcript"
        os.link(self.transcript, other)
        rc, out, err = self.cp(other, force=True)
        self.assertRefused(rc, out, err)
        self.assertIn("charter's own standard output", err)
        self.assertTranscriptIntact()

    def test_the_refusal_never_names_a_flag_that_would_get_past_it(self):
        """#421 in one sentence: a guard must not print its own bypass. The refusal this
        replaced ended "Pass --force to overwrite it deliberately", and `--force` was the
        bypass."""
        for dest, force in ((self.transcript, False), ("/dev/fd/1", False),
                            (self.transcript, True), ("/dev/fd/1", True)):
            with self.subTest(dest=str(dest), force=force):
                _, _, err = self.cp(dest, force=force)
                self.assertNotIn("--force", err,
                                 "the refusal suggests the flag that used to bypass it")
                self.assertNotIn("--reveal", err)

    def test_the_vault_is_never_read_for_one_of_our_own_streams(self):
        """The value must not exist in this process for the case that was about to print
        it — refusing after resolving would still put the plaintext one bug from the
        stream it was refused from."""
        spy = _SpyProvider()
        with mock.patch.object(cs, "_provider", lambda _name: spy):
            rc, _, _ = self.cp("/dev/fd/1", force=True)
        self.assertEqual(rc, 2)
        self.assertEqual(spy.reads, [], "a refused stream still read the vault")

    def test_a_name_is_recognised_without_opening_anything(self):
        """Two lookups answer two questions, so each needs its own case or one of them
        is decoration. `lstat` alone recognises a second NAME for the inode — a hardlink,
        or the path `readlink` gives for the log — and costs no open; the descriptor is
        what `/dev/fd/N` needs, because macOS reports devfs's `st_dev` there rather than
        the file's. With the descriptor half stubbed out, the cheap half must still hold."""
        other = self.out_dir / "another-name"
        os.link(self.transcript, other)
        with mock.patch.object(cs, "_identify_dest", lambda _raw: None):
            rc, out, err = self.cp(other)
        self.assertRefused(rc, out, err)
        self.assertIn("charter's own standard output", err)
        self.assertNotIn("--force", err)

    def test_the_open_refuses_even_when_the_path_check_is_bypassed(self):
        """The guarantee, separated from the courtesy. Everything the path check sees can
        be swapped between the `lstat` and the `open`; only the descriptor cannot. With
        the path check stubbed out — the shape of a lost race — the `fstat` after the
        open is the whole defence, and it has to hold on its own.

        Both spellings, because they fail differently: `/dev/fd/1` is the input the
        reviewer used, and the real path is the one that proves nothing was destroyed on
        the way to the refusal. macOS devfs quietly ignores `O_TRUNC` on `/dev/fd/N`, so
        an open that truncated before deciding would look harmless through that name and
        empty the file through this one.
        """
        for dest in ("/dev/fd/1", self.transcript):
            with self.subTest(dest=str(dest)):
                with mock.patch.object(cs, "_cp_dest_refusal", lambda *a, **k: None):
                    rc, out, err = self.cp(dest, force=True)
                self.assertRefused(rc, out, err)
                self.assertTranscriptIntact()

    def test_an_ordinary_destination_is_still_written(self):
        """The guard must refuse charter's streams, not every file — with fd 1 on a
        regular file, an over-broad identity test would refuse everything."""
        dest = self.out_dir / "kubeconfig"
        rc, out, err = self.cp(dest)
        self.assertEqual(rc, 0)
        self.assertEqual(dest.read_text(), VALUE)
        self.assertNotIn(VALUE, out + err)


class TestASymlinkIsNeverFollowed(CpCase):
    def test_a_symlink_to_a_regular_file_is_refused(self):
        """The input that gets past a check that stats the TARGET: the target is a
        perfectly ordinary regular file, and the link is what decides where the
        plaintext lands."""
        victim = self.out_dir / "victim"
        victim.write_text("original-config\n")
        link = self.out_dir / "link"
        link.symlink_to(victim)

        rc, out, err = self.cp(link)
        self.assertRefused(rc, out, err)
        self.assertIn("symlink", err)
        self.assertEqual(victim.read_text(), "original-config\n")

    def test_a_symlink_to_a_device_is_refused_as_a_symlink(self):
        link = self.out_dir / "to-null"
        link.symlink_to("/dev/null")
        rc, out, err = self.cp(link)
        self.assertRefused(rc, out, err)

    def test_a_dangling_symlink_is_refused(self):
        """`lstat` succeeds and `stat` does not — the ordering that lets a link create a
        file out of nothing if the check is written the other way round."""
        link = self.out_dir / "dangling"
        link.symlink_to(self.out_dir / "nothing-here")
        rc, out, err = self.cp(link)
        self.assertRefused(rc, out, err)
        self.assertIn("symlink", err,
                      "refused, but for the wrong reason — the link is the finding")
        self.assertFalse((self.out_dir / "nothing-here").exists())

    def test_force_does_not_buy_a_symlink(self):
        """--force is consent to destroy a file the operator NAMED, not consent to let a
        link choose the file."""
        victim = self.out_dir / "victim"
        victim.write_text("original-config\n")
        link = self.out_dir / "link"
        link.symlink_to(victim)
        rc, out, err = self.cp(link, force=True)
        self.assertRefused(rc, out, err)
        self.assertEqual(victim.read_text(), "original-config\n")


class TestAnExistingFileIsNotClobbered(CpCase):
    def test_an_existing_file_is_not_clobbered_without_force(self):
        victim = self.out_dir / "config"
        victim.write_text("original-config\n")
        os.chmod(victim, 0o644)

        rc, out, err = self.cp(victim)
        self.assertRefused(rc, out, err)
        self.assertIn("--force", err)
        self.assertEqual(victim.read_text(), "original-config\n")
        self.assertEqual(stat.S_IMODE(victim.stat().st_mode), 0o644,
                         "a refused write must not change the victim's mode either")

    def test_force_overwrites_and_says_so(self):
        victim = self.out_dir / "config"
        victim.write_text("original-config\n")
        os.chmod(victim, 0o644)

        rc, out, err = self.cp(victim, force=True)
        self.assertEqual(rc, 0)
        self.assertEqual(victim.read_text(), VALUE)
        self.assertEqual(stat.S_IMODE(victim.stat().st_mode), 0o600)
        self.assertIn("Overwrote", err)
        self.assertNotIn(VALUE, out)


class TestTheOrdinaryCaseStillWorks(CpCase):
    def test_a_new_file_is_written_at_0600_and_the_value_is_not_printed(self):
        dest = self.out_dir / "kubeconfig"
        rc, out, err = self.cp(dest)
        self.assertEqual(rc, 0)
        self.assertEqual(dest.read_text(), VALUE)
        self.assertEqual(stat.S_IMODE(dest.stat().st_mode), 0o600)
        self.assertNotIn(VALUE, out + err)
        self.assertIn("Value not shown.", err + out)

    def test_one_missing_parent_directory_is_created_at_0700(self):
        """The documented case — `~/.kube/config` where `~/.kube` does not exist yet."""
        dest = self.out_dir / "kube" / "config"
        rc, _, _ = self.cp(dest)
        self.assertEqual(rc, 0)
        self.assertEqual(dest.read_text(), VALUE)
        self.assertEqual(stat.S_IMODE(dest.parent.stat().st_mode), 0o700)

    def test_a_missing_tree_is_refused_rather_than_built(self):
        """`mkdir -p` on a caller-supplied path builds arbitrary directories anywhere the
        user can write, and turns a typo'd destination into a silent success."""
        dest = self.out_dir / "a" / "b" / "c" / "config"
        rc, out, err = self.cp(dest)
        self.assertEqual(rc, 2)
        self.assertNotIn(VALUE, out + err)
        self.assertFalse((self.out_dir / "a").exists())


class TestTheOpenIsTheSecondLineOfDefence(CpCase):
    """The `lstat` above and the `open` below are two syscalls with a gap between them,
    and the gap is where a plant goes. `O_EXCL` and `O_NOFOLLOW` close it — but a
    single-threaded test can never lose that race, so the check is stubbed out to
    simulate having seen an empty path, and the OPEN is what has to refuse.

    Without this, both flags are unfalsifiable belt-and-braces: removing either one
    leaves every other test in this file green.
    """

    def cp_as_if_the_path_were_empty(self, dest, force: bool = False):
        with mock.patch.object(cs, "_cp_dest_refusal", lambda *a, **k: None):
            return self.cp(dest, force=force)

    def test_a_file_that_appears_after_the_check_is_not_truncated(self):
        victim = self.out_dir / "planted"
        victim.write_text("original-config\n")
        rc, out, err = self.cp_as_if_the_path_were_empty(victim)
        self.assertEqual(rc, 2)
        self.assertNotIn(VALUE, out + err)
        self.assertEqual(victim.read_text(), "original-config\n")

    def test_a_symlink_that_appears_after_the_check_is_not_followed(self):
        victim = self.out_dir / "victim"
        victim.write_text("original-config\n")
        link = self.out_dir / "planted-link"
        link.symlink_to(victim)
        rc, out, err = self.cp_as_if_the_path_were_empty(link)
        self.assertEqual(rc, 2)
        self.assertNotIn(VALUE, out + err)
        self.assertEqual(victim.read_text(), "original-config\n")

    def test_a_device_that_appears_after_the_check_is_refused_by_the_descriptor(self):
        """`O_EXCL` and `O_NOFOLLOW` cover the plants they cover; neither says anything
        about *kind*. Under `--force` there is no `O_EXCL`, so a character device swapped
        in after the check opens cleanly and takes the write. The `fstat` asks the
        descriptor what it is, which is the same question the path check asked and the
        only version of it nothing can race."""
        rc, out, err = self.cp_as_if_the_path_were_empty("/dev/null", force=True)
        self.assertEqual(rc, 2)
        self.assertNotIn(VALUE, out + err)
        self.assertIn("character device", err)

    def test_a_fifo_with_a_reader_is_refused_by_the_descriptor(self):
        """The case the kind check exists for. A FIFO nobody is reading fails the open
        outright (ENXIO under `O_NONBLOCK`) — but a FIFO with a reader already attached
        opens straight through, and the reader on the other end is whoever planted it."""
        fifo = self.out_dir / "planted-pipe"
        os.mkfifo(fifo)
        reader = os.open(fifo, os.O_RDONLY | os.O_NONBLOCK)
        self.addCleanup(os.close, reader)

        rc, out, err = self.cp_as_if_the_path_were_empty(fifo, force=True)
        self.assertEqual(rc, 2)
        self.assertNotIn(VALUE, out + err)
        self.assertIn("FIFO", err)
        try:
            drained = os.read(reader, 4096).decode("utf-8", "replace")
        except BlockingIOError:
            drained = ""
        self.assertNotIn(VALUE, drained, "the reader on the pipe got the credential")

    def test_a_planted_symlink_is_not_followed_under_force_either(self):
        """The case `O_EXCL` cannot cover: `--force` opens with `O_TRUNC`, so the plant
        succeeds unless the open itself refuses to traverse a link. `--force` is consent
        to destroy the file the operator NAMED."""
        victim = self.out_dir / "victim"
        victim.write_text("original-config\n")
        link = self.out_dir / "planted-link"
        link.symlink_to(victim)
        rc, out, err = self.cp_as_if_the_path_were_empty(link, force=True)
        self.assertEqual(rc, 2)
        self.assertNotIn(VALUE, out + err)
        self.assertEqual(victim.read_text(), "original-config\n")


class _SpyProvider:
    """Records every key read. Returns the fabricated value, never a real vault read."""

    def __init__(self) -> None:
        self.reads: list[str] = []

    def get(self, key: str) -> str:
        self.reads.append(key)
        return VALUE


class TestTheValueIsNotResolvedForARefusedDestination(CpCase):
    def test_a_refused_destination_never_reads_the_vault(self):
        """Ordering, not decoration: for the case that was about to print the plaintext,
        the plaintext must never enter this process at all."""
        spy = _SpyProvider()
        with mock.patch.object(cs, "_provider", lambda _name: spy):
            rc, _, _ = self.cp("/dev/null")
        self.assertEqual(rc, 2)
        self.assertEqual(spy.reads, [],
                         "the destination is checked before the vault is read")

    def test_an_allowed_destination_does_read_it(self):
        """The sibling that keeps the assertion above from passing for the wrong
        reason — a `cp` that never reads anything would also record no reads."""
        spy = _SpyProvider()
        with mock.patch.object(cs, "_provider", lambda _name: spy):
            rc, _, _ = self.cp(self.out_dir / "fresh")
        self.assertEqual(rc, 0)
        self.assertEqual(spy.reads, ["k"])


class TestThePlaneIsNotADropBox(CpCase):
    """ADR-0017, the rule `vault add` already applies: plaintext written somewhere git
    tracks is committed by the next `charter save`. `cp` writes the same plaintext to the
    same kind of path and asked nothing.

    `git_ignores` is stubbed rather than a real repo being built: the three answers it
    can give (tracked / ignored / not a repo) are the whole decision, and the real thing
    is `git check-ignore`, already exercised by the `vault add` tests.
    """

    def cp_with_git(self, dest, answer):
        with mock.patch("charter.util.git_ignores", lambda root, path: answer):
            return self.cp(dest)

    def test_a_tracked_path_inside_the_plane_is_refused(self):
        dest = Path(config.ROOT) / "tracked.txt"
        rc, out, err = self.cp_with_git(dest, False)     # git would take this file
        self.assertRefused(rc, out, err)
        self.assertIn("charter save", err)
        self.assertFalse(dest.exists())

    def test_a_gitignored_path_inside_the_plane_is_allowed(self):
        """`.charter/` is gitignored by `charter init`, and materialising there is a
        thing personas legitimately do — the rule is about git, not about the plane."""
        dest = Path(config.ROOT) / ".charter" / "materialised"
        rc, _, _ = self.cp_with_git(dest, True)
        self.assertEqual(rc, 0)

    def test_a_path_outside_the_plane_is_never_asked_about(self):
        """Outside the plane git has no say, and `cp`'s whole documented use — a
        kubeconfig under ~ — lives there."""
        asked: list[str] = []

        def spy(root, path):
            asked.append(str(path))
            return False

        with mock.patch("charter.util.git_ignores", spy):
            rc, _, _ = self.cp(self.out_dir / "outside")
        self.assertEqual(rc, 0)
        self.assertEqual(asked, [])


if __name__ == "__main__":
    unittest.main()
