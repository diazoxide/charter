"""The directories charter creates for a vault do not list vault names to other accounts.

Round one of #437 fixed the vault *file* and wrote a news entry saying `.charter/vaults/`
"no longer lists every vault name you have to every account on the machine". The code
under that sentence was ``p.parent.mkdir(parents=True, exist_ok=True, mode=0o700)``, and
the sentence was false twice over.

**Once because `mode` reaches the leaf only.** CPython's ``pathlib.Path.mkdir`` creates a
missing parent with a bare recursive ``self.parent.mkdir(parents=True, exist_ok=True)`` —
no *mode* — so every level above the last comes out at ``0o777 & ~umask``. Measured on a
plane where charter created every level itself, vault at ``.charter/vaults/team/prod.json``::

    .charter               0755
    .charter/vaults        0755      <-- the directory the fix was about
    .charter/vaults/team   0700

**Once because a directory that already exists ignores `mode` entirely.** A
``.charter/vaults/`` made by an older charter or by ``mkdir -p`` is 0755 before
``secret set`` and 0755 after, and nothing reported it.

**The property is not "the leaf is 0700".** That is a spelling, and this file exists
because the leaf *was* 0700 while the claim was false. The property is:

    every directory **the vault writers** create on the way to a vault file satisfies
    ``mode & 0o077 == 0``

— asserted over the whole chain, at whatever depth the vault is configured, so a fix that
covers two levels and not three goes RED. Modes are tested through that mask and never
against a list of known-bad values: 0755 is the one everybody pictures, while 0705, 0711,
0730 and 0701 list or traverse just as well and appear on no such list.

**"The vault writers", and who else gets there first.** The 0700 walk is
:func:`base.make_private_dir`, which the three secrets writers call — and it is
:func:`config.private_mkdir` under another name, which is what every other state writer
calls since #470. That distinction used to matter: on the default CLI flow the vault
writers are not what creates ``.charter/`` (``charter vault add`` writes the local
registry first, through a bare ``path.parent.mkdir(parents=True, exist_ok=True)``), so the
state directory was already there, at the umask default, before any vault file was
written. Every class below except :class:`TheOrderTheCliActuallyUses` gives the vault
writer the first move, which is the order a direct ``PlainFileProvider(...).set()``
produces and not the order the CLI produces. That class exists so the difference is pinned
rather than hidden: it reproduces the registry-first ordering and asserts that the state
directory comes out 0700 under it too. The whole-CLI sweep — real binary, real plane,
three umasks, three different first writers — is
`tests/test_the_state_directory_is_charters_to_choose.py`.

The second property is the honest half, and it is asserted rather than merely documented:

    a directory charter did NOT create keeps its mode exactly, and is reported

Both halves matter. Tightening whatever directory a vault lands in is not an improvement —
a vault's ``file`` may name any path on this machine, so it is how charter would come to
chmod a home directory or a shared team directory unprompted (#331). Reporting is the
only alternative that does not require charter to be wrong about one of the two.
"""

from __future__ import annotations

import os
import stat
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from charter import config
from charter.secrets import fingerprint
from charter.secrets.base import loose_dirs, make_private_dir
from charter.secrets.plain_file import PlainFileProvider
from charter.secrets.reference import ReferenceProvider

from tests._isolation import PersonaIso

#: Modes that let *some* other account do *something* with a directory. Deliberately more
#: than "0755": 0705 and 0701 grant other without group, 0711 grants traverse without
#: read, 0730 grants a group write. A guard written against a literal list is the failure
#: this suite keeps finding, so these exist to prove the guard is the mask and not a list.
LOOSE_MODES = (0o755, 0o750, 0o705, 0o701, 0o711, 0o730, 0o775, 0o777, 0o707)

#: Modes reachable only by the owner. 0700 is the target; the others must not be reported
#: as loose merely for differing from it.
PRIVATE_MODES = (0o700, 0o500, 0o300, 0o100)


def modes_up_to(leaf, stop) -> dict:
    """``{path: mode}`` for every directory from *leaf* up to and including *stop*."""
    out, cur, stop_rp = {}, Path(leaf), Path(stop).resolve()
    while True:
        out[cur] = stat.S_IMODE(cur.stat().st_mode)
        if cur.resolve() == stop_rp or cur.parent == cur:
            return out
        cur = cur.parent


class EveryLevelCharterCreates(PersonaIso):
    """The property, over the whole chain, at several depths."""

    def test_a_plane_charter_builds_from_nothing_has_no_reachable_level(self) -> None:
        """The headline case: `.charter/` itself does not exist yet, so every level in the
        chain is charter's own choice and none of it is inherited from the fixture."""
        sd = Path(config.STATE_DIR)
        self.assertFalse(sd.exists(), "precondition: charter creates every level here")
        vf = sd / "vaults" / "devops.json"
        PlainFileProvider("v", {"file": str(vf)}).set("K", "value-one")

        chain = modes_up_to(vf.parent, sd)
        self.assertGreaterEqual(
            len(chain), 2,
            "a one-element chain makes this vacuous — the defect was the level ABOVE the "
            "leaf, so a chain that does not contain one proves nothing")
        for p, mode in chain.items():
            self.assertEqual(mode & 0o077, 0,
                             f"{p} is {oct(mode)[-3:]}: another account on this machine "
                             f"can reach the directory holding the vault names")

    def test_depth_does_not_decide_it(self) -> None:
        """Three, four and six levels below the plane. The round-one fix passes at depth
        one and fails at every depth beyond it, so depth is the axis that separates a fix
        for the property from a fix for the example."""
        sd = Path(config.STATE_DIR)
        for rel in ("vaults/devops.json",
                    "vaults/team/prod.json",
                    "vaults/a/b/c/deep.json"):
            with self.subTest(rel=rel):
                vf = sd / rel
                PlainFileProvider("v", {"file": str(vf)}).set("K", "value-one")
                for p, mode in modes_up_to(vf.parent, sd).items():
                    self.assertEqual(mode & 0o077, 0, f"{p} is {oct(mode)[-3:]}")

    def test_the_intermediate_level_specifically(self) -> None:
        """The exact measurement from the report, kept as its own case.

        The tests above cover it, but they cover it alongside the leaf — and a regression
        restoring ``mkdir(parents=True, mode=…)`` leaves the leaf green. Naming the
        intermediate level makes the failure say which level went wrong.
        """
        vf = Path(config.STATE_DIR) / "vaults" / "team" / "prod.json"
        PlainFileProvider("v", {"file": str(vf)}).set("K", "value-one")
        vaults = Path(config.STATE_DIR) / "vaults"
        self.assertEqual(
            stat.S_IMODE(vaults.stat().st_mode) & 0o077, 0,
            f"{vaults} is {oct(stat.S_IMODE(vaults.stat().st_mode))[-3:]} — it is the "
            f"directory that lists every vault name, which is what the fix was about")

    def test_a_reference_vault_gets_the_same_directory(self) -> None:
        """A reference vault holds no plaintext — and the same directory listing.

        Its file is `op://…` URIs, so its own mode is a smaller matter; the directory is
        not, because it names your vault layout either way. It went through a plain
        ``mkdir(parents=True, exist_ok=True)`` with no mode argument at all.
        """
        vf = Path(config.STATE_DIR) / "vaults" / "team.json"
        ReferenceProvider("t", {"file": str(vf)}).set("K", "op://Eng/deploy/token")
        for p, mode in modes_up_to(vf.parent, config.STATE_DIR).items():
            self.assertEqual(mode & 0o077, 0, f"{p} is {oct(mode)[-3:]}")

    def test_the_fingerprint_key_directory_too(self) -> None:
        """`.charter/` holds `fingerprint.key`, whose secrecy is the whole of #436. The
        key file is 0600, but a 0755 directory around it is still the plane advertising
        that the file is there, and it is created by the same call."""
        self.assertIsNotNone(fingerprint.fingerprint("value-one"),
                             "no key was made, so this asserts nothing about its home")
        sd = Path(config.STATE_DIR)
        self.assertEqual(stat.S_IMODE(sd.stat().st_mode) & 0o077, 0,
                         f"{sd} is {oct(stat.S_IMODE(sd.stat().st_mode))[-3:]}")


class NoInstantAtWhichItIsWider(PersonaIso):
    """The directory is private *when it is created*, not private a moment afterwards.

    Final-state assertions cannot tell ``mkdir(0o700)`` from ``mkdir()`` followed by a
    chmod: both end at 0700, and only one of them is ever 0755. That gap is the exact
    shape #437 was filed about one level down — the vault file was opened at the wrong
    mode and chmod-ed after the plaintext was already in it — so a directory fix that
    reproduces it while passing the tests would be the same defect wearing the fix's
    clothes.

    Observed at the syscall, not inferred: every ``os.mkdir`` charter issues is required
    to name a mode with no group or other bits. And the watch **asserts it saw something**
    — a future implementation that creates directories through some call this does not
    wrap makes this test say so rather than pass on having observed nothing.
    """

    def test_every_mkdir_charter_issues_names_a_private_mode(self) -> None:
        seen = []
        real = os.mkdir

        def spy(path, mode=0o777, *a, **kw):
            seen.append((str(path), mode))
            return real(path, mode, *a, **kw)

        vf = Path(config.STATE_DIR) / "vaults" / "team" / "prod.json"
        with mock.patch.object(os, "mkdir", spy):
            PlainFileProvider("v", {"file": str(vf)}).set("K", "value-one")

        self.assertTrue(seen, "no os.mkdir was observed at all — charter created these "
                              "directories through a call this watch does not wrap, so "
                              "the assertion below would have been vacuous")
        made = {p for p, _ in seen}
        for level in (config.STATE_DIR, str(vf.parent.parent), str(vf.parent)):
            self.assertIn(str(level), made,
                          "a level charter created was not seen being created")
        for path, mode in seen:
            self.assertEqual(
                mode & 0o077, 0,
                f"os.mkdir({path}, {oct(mode)}) — the directory exists at a mode another "
                f"account can reach before any chmod could narrow it")


class UmaskCannotDecideThis(PersonaIso):
    """The mode the vault writer asks for is the mode it gets, whatever the umask is.

    ``os.mkdir``'s *mode* argument is masked by the umask, which makes the result a
    property of the ambient environment rather than of charter. Both directions matter: a
    permissive umask must not widen the directory, and a restrictive one must not leave
    charter a directory it cannot then use.

    Scoped to the levels :func:`make_private_dir` creates, and the scope is not a detail:
    the chain below starts at a per-umask subdirectory *underneath* ``.charter/`` for
    exactly that reason. On the real CLI flow the umask does decide ``.charter/`` itself,
    because the registry write creates it first (#470) — asserted in
    :class:`TheOrderTheCliActuallyUses`, which is where the honest version of this class
    name lives.
    """

    def test_every_umask_yields_exactly_0700(self) -> None:
        # 0o000: mkdir(0o700) survives this, but a regression to `mkdir()` with no mode
        #        gives 0o777 — so this is the case that catches "stopped passing a mode".
        # 0o377: strips owner-write and owner-execute from mkdir's own argument, leaving
        #        0o400, a directory charter cannot write the vault into. The explicit
        #        chmod is what makes the outcome charter's decision rather than the shell's.
        sd = Path(config.STATE_DIR)
        for um in (0o000, 0o022, 0o077, 0o377):
            with self.subTest(umask=oct(um)):
                old = os.umask(um)
                try:
                    vf = sd / f"u{um:03o}" / "vaults" / "team" / "prod.json"
                    PlainFileProvider("v", {"file": str(vf)}).set("K", "value-one")
                    chain = modes_up_to(vf.parent, sd / f"u{um:03o}")
                finally:
                    os.umask(old)
                for p, mode in chain.items():
                    self.assertEqual(
                        mode, 0o700,
                        f"under umask {oct(um)}, {p} came out {oct(mode)[-3:]}")


class ADirectoryCharterDidNotCreate(PersonaIso):
    """It keeps its mode, and it is named. Neither half is optional."""

    def _scaffold(self, tag: str, mode: int) -> Path:
        """A fresh vault directory whose every ancestor inside the plane is 0700.

        The fixture has to own the whole chain, because the thing under test *walks* the
        chain: leave `.charter/` at the mkdir default and the report names it, and the
        test would be reading its own scaffolding back rather than the directory it set.
        The first version of this file did exactly that and passed the loose cases for
        the wrong reason.
        """
        vd = Path(config.STATE_DIR) / f"{tag}{mode:03o}" / "vaults"
        vd.mkdir(parents=True)
        cur = vd
        while True:
            os.chmod(cur, 0o700)
            if cur.resolve() == Path(config.STATE_DIR).resolve():
                break
            cur = cur.parent
        return vd

    def _preexisting(self, mode: int, tag: str = "d"):
        """A vault directory that was already there, at *mode*, when charter arrived —
        so what follows measures what ``secret set`` does to a directory it did not make.

        Only used with modes the owner can still write, which is every mode in
        `LOOSE_MODES`; `_then_chmod` covers the rest.
        """
        vd = self._scaffold(tag, mode)
        os.chmod(vd, mode)
        self.addCleanup(lambda p=vd: os.chmod(p, 0o700))
        prov = PlainFileProvider("demo", {"file": str(vd / "demo.json")})
        prov.set("K", "value-one")
        return vd, prov

    def _then_chmod(self, mode: int, tag: str = "c"):
        """Vault written into a private directory, which is then set to *mode*.

        The corpus of owner-only modes includes 0500, 0300 and 0100 — directories charter
        cannot write into at all — so the write has to happen first. `health()` only
        reads, and every one of those modes still permits the traverse it needs.
        """
        vd = self._scaffold(tag, mode)
        prov = PlainFileProvider("demo", {"file": str(vd / "demo.json")})
        prov.set("K", "value-one")
        os.chmod(vd, mode)
        self.addCleanup(lambda p=vd: os.chmod(p, 0o700))
        return vd, prov

    def test_charter_does_not_chmod_it(self) -> None:
        for mode in LOOSE_MODES:
            with self.subTest(mode=oct(mode)):
                vd, _ = self._preexisting(mode)
                self.assertEqual(
                    stat.S_IMODE(vd.stat().st_mode), mode,
                    "charter must not silently chmod a directory it did not create — a "
                    "vault's --file can name any path on this machine (#331)")

    def test_health_names_every_loose_mode(self) -> None:
        """Reported for the *property*, not for a list of known-bad modes."""
        for mode in LOOSE_MODES:
            with self.subTest(mode=oct(mode)):
                _, prov = self._then_chmod(mode, tag="h")
                ok, detail = prov.health()
                self.assertTrue(ok, "a loose directory is a warning, not an unreachable "
                                    "vault — doctor runs health() from SessionStart and "
                                    "must not turn this into a red line")
                self.assertIn("listed by other accounts", detail)
                self.assertIn(oct(mode)[-3:], detail,
                              "the report must name the mode it found, or the operator "
                              "cannot tell it from the mode charter wants")

    def test_health_is_silent_when_no_other_account_can_reach_it(self) -> None:
        """Otherwise the check is a constant, and a constant reports nothing."""
        for mode in PRIVATE_MODES:
            with self.subTest(mode=oct(mode)):
                _, prov = self._then_chmod(mode, tag="q")
                _, detail = prov.health()
                self.assertNotIn("listed by other accounts", detail)

    def test_the_vault_file_is_still_0600_beside_a_loose_directory(self) -> None:
        """The directory finding must not have displaced the #437 fix it sits beside."""
        vd, _ = self._preexisting(0o755, tag="f")
        self.assertEqual(stat.S_IMODE((vd / "demo.json").stat().st_mode), 0o600)


class LooseDirsIsBounded(PersonaIso):
    """`loose_dirs` reports the plane, not the machine.

    ``/``, ``/Users`` and ``/tmp`` are 0755 everywhere and are nobody's defect; a check
    that names them is a check operators learn to ignore, which is what #171 and #55 cost.
    """

    def test_it_stops_at_the_plane_state_directory(self) -> None:
        vd = Path(config.VAULTS_DIR)
        vd.mkdir(parents=True)
        sd = Path(config.STATE_DIR)
        self.addCleanup(lambda: os.chmod(sd, 0o700))
        os.chmod(sd, 0o755)
        os.chmod(vd, 0o755)
        found = {p.resolve() for p, _ in loose_dirs(vd, sd)}
        self.assertIn(vd.resolve(), found)
        self.assertIn(sd.resolve(), found, "the bound is inclusive: .charter itself is a "
                                           "directory charter creates and owns")
        self.assertNotIn(sd.resolve().parent, found,
                         "the walk must stop at the plane, not climb to the filesystem "
                         "root naming every 0755 directory on the way")

    def test_a_vault_outside_the_plane_reports_only_its_own_directory(self) -> None:
        """Pointing --file outside the plane is a configuration charter recommends.

        charter has an opinion about the directory holding a vault and none at all about
        that directory's ancestors on someone else's filesystem layout — so a walk that
        never meets the plane must not run all the way to ``/``.
        """
        outside = Path(tempfile.mkdtemp(prefix="edm-outside-"))
        self.addCleanup(lambda: os.chmod(outside, 0o700))
        os.chmod(outside, 0o755)
        found = [p for p, _ in loose_dirs(outside, config.STATE_DIR)]
        self.assertEqual([p.resolve() for p in found], [outside.resolve()])


class TheLimitOfAPosixMode(PersonaIso):
    """The next spelling, named and pinned: an ACL the mode does not show.

    ``mode & 0o077`` is the property this file tests everywhere else, and it is the right
    property for a POSIX mode. It is not the whole of "can another account reach this
    directory". macOS extended ACLs and Linux POSIX.1e ACLs both grant access that
    ``st_mode`` does not reflect: measured here, a directory at 0700 with
    ``chmod +a "everyone allow read,list,search"`` reports ``st_mode`` 0700 while every
    account on the machine can list it — which is precisely the exposure `make_private_dir`
    and `loose_dirs` exist to prevent, arriving by a spelling neither of them can read.

    **This test asserts the gap rather than the fix.** Reading an ACL needs `acl_get_file`,
    which the standard library does not expose on either platform, and shelling out to
    `ls -le` from a health check that `doctor` runs at every session start costs more than
    the gap does. So the limit is accepted, documented on `base._OTHERS`, and pinned here
    so it cannot quietly stop being true in either direction: if someone teaches
    `loose_dirs` to read ACLs, this goes RED and the docstring saying it cannot is the
    thing to fix.
    """

    def _acl(self, d: Path) -> None:
        r = subprocess.run(["chmod", "+a", "everyone allow read,list,search", str(d)],
                           capture_output=True, text=True)
        if r.returncode != 0:
            self.skipTest(f"no extended-ACL support here: {r.stderr.strip()[:80]}")

    @unittest.skipUnless(sys.platform == "darwin", "chmod +a is macOS's ACL spelling")
    def test_an_acl_grants_access_the_mode_does_not_show(self) -> None:
        vd = Path(config.VAULTS_DIR)
        PlainFileProvider("v", {"file": str(vd / "demo.json")}).set("K", "value-one")
        self.assertEqual(stat.S_IMODE(vd.stat().st_mode), 0o700, "charter made this one")

        self._acl(vd)
        listing = subprocess.run(["ls", "-led", str(vd)], capture_output=True, text=True)
        self.assertIn("everyone", listing.stdout,
                      "the ACL did not take, so nothing below is being measured")

        self.assertEqual(
            stat.S_IMODE(vd.stat().st_mode) & 0o077, 0,
            "st_mode still reads private — this is the gap, and if this line ever fails "
            "it means the platform started reflecting ACLs in the mode, which would be "
            "good news and would make base._OTHERS's docstring wrong")
        self.assertEqual(
            loose_dirs(vd, config.STATE_DIR), [],
            "loose_dirs reports nothing, because a POSIX mode is all it reads. Documented "
            "on base._OTHERS as an accepted limit; if this list stops being empty, ACL "
            "awareness has arrived and that docstring needs updating with it.")


class MakePrivateDirItself(PersonaIso):
    """Two behaviours of the helper the provider tests cannot show directly."""

    def test_it_leaves_an_existing_leaf_alone(self) -> None:
        d = Path(config.STATE_DIR) / "pre"
        d.mkdir(parents=True)
        self.addCleanup(lambda: os.chmod(d, 0o700))
        os.chmod(d, 0o755)
        make_private_dir(d)
        self.assertEqual(stat.S_IMODE(d.stat().st_mode), 0o755)

    def test_a_directory_that_appears_mid_walk_is_left_alone(self) -> None:
        """The race branch: the walk said "missing", the mkdir said "already there".

        Reachable only by losing a race with another process, so it is reached here
        through the one seam that makes the race deterministic — `Path.exists` is made to
        answer False for a directory that is really there at 0755, which is precisely what
        the losing side of the race observes.

        The seam manufactures the *timing*, not the outcome under test: what is asserted
        is what `make_private_dir` does once `mkdir` has raised `FileExistsError`, and it
        must be the same thing it does for any other directory it did not create — leave
        the mode alone. A chmod here would let a concurrent charter, or an operator's
        `mkdir -p` landing at the wrong moment, be enough to make charter tighten a
        directory it was never asked to touch.
        """
        d = Path(config.STATE_DIR) / "raced"
        d.mkdir(parents=True)
        self.addCleanup(lambda: os.chmod(d, 0o700))
        os.chmod(d, 0o755)

        real_exists = Path.exists

        def blind(self, *a, **kw):
            return False if self == d else real_exists(self, *a, **kw)

        with mock.patch.object(Path, "exists", blind):
            make_private_dir(d)

        self.assertEqual(
            stat.S_IMODE(d.stat().st_mode), 0o755,
            "mkdir raised FileExistsError, so this directory is one charter did NOT "
            "create, and charter does not chmod those (#331)")

    def test_it_creates_the_levels_below_an_existing_one(self) -> None:
        """The mixed case: part of the chain is the operator's, the rest is charter's."""
        pre = Path(config.STATE_DIR) / "pre"
        pre.mkdir(parents=True)
        self.addCleanup(lambda: os.chmod(pre, 0o700))
        os.chmod(pre, 0o755)
        make_private_dir(pre / "a" / "b")
        self.assertEqual(stat.S_IMODE(pre.stat().st_mode), 0o755, "not charter's to fix")
        for d in (pre / "a", pre / "a" / "b"):
            self.assertEqual(stat.S_IMODE(d.stat().st_mode), 0o700, f"{d} is charter's")


class TheOrderTheCliActuallyUses(PersonaIso):
    """The registry write goes first, so ``.charter/`` is not the vault writer's to choose.

    Every other class here calls a provider's ``set()`` against a plane where
    ``.charter/`` does not exist yet, so :func:`make_private_dir` wins the race to create
    it and the state directory comes out 0700. That is the fixture manufacturing the
    condition it claims to observe, in the favourable direction: on the flow ``charter
    vault add`` prints as its own first step, the local registry is written *before* any
    secret is set, and ``registry._write`` does ``path.parent.mkdir(parents=True,
    exist_ok=True)`` with no mode. Under ``umask 022`` that leaves ``.charter`` at 0755,
    and no later ``secret set`` touches it — charter does not chmod a directory it did not
    create, which is the same rule that protects a home directory (#331).

    So this class drives ``registry.add_vault`` first, which is the real call, and pins
    all three halves of what the documents now say:

    * the levels the vault writers create are 0700 whatever the umask is;
    * ``.charter/`` itself is 0700 too, whatever the umask is — the registry write goes
      through the same walk since #470, so the mode no longer depends on which command
      somebody happened to run first;
    * a directory charter did **not** create keeps its mode and is *reported* — on the
      health line ``charter vault list`` prints, and on ``charter doctor``'s vaults line
      (#471).

    The middle one used to be a defect pinned on purpose, with the residual written into
    `docs/secrets.md` and the news entry. Both now describe the fix.
    """

    def _fresh_plane(self, tag: str, um: int):
        """Enter a brand-new plane root, with *um* in force for everything created in it.

        Both have to be per-case: a umask set after ``.charter/`` exists changes nothing,
        which would make a loop over umasks assert the first iteration four times.
        """
        root = self.tmp / f"plane-{tag}"
        root.mkdir()
        prev = config.use(root)
        self.addCleanup(config.restore, prev)
        old = os.umask(um)
        self.addCleanup(os.umask, old)
        return root

    @staticmethod
    def _add_vault(name: str, rel: str) -> Path:
        """Register a vault through the call ``charter vault add`` makes.

        Not a hand-rolled ``mkdir``: the point of this class is that the *registry* is
        what creates ``.charter/``, so it has to be the registry doing it. If that ever
        stops being true, these cases stop reproducing the CLI and say so rather than
        quietly asserting something else.
        """
        from charter.secrets import registry

        vf = Path(config.STATE_DIR) / rel
        registry.add_vault(name, "plain-file", {"file": str(vf)})
        return vf

    def test_the_vault_directories_are_private_even_when_the_registry_went_first(self):
        """The fix, in the ordering the CLI produces rather than the one the fixture did."""
        for um in (0o000, 0o022, 0o077):
            with self.subTest(umask=oct(um)):
                self._fresh_plane(f"v{um:03o}", um)
                sd = Path(config.STATE_DIR)
                self.assertFalse(sd.exists(), "precondition: nothing has made it yet")
                vf = self._add_vault("devops", "vaults/team/prod.json")
                self.assertTrue(
                    sd.is_dir(),
                    "the registry write did not create STATE_DIR, so this case no longer "
                    "reproduces the CLI ordering and everything below it is vacuous")
                self.assertFalse(
                    vf.parent.exists(),
                    "the vault directory already exists, so make_private_dir will not be "
                    "the thing that creates it and the assertion below proves nothing")

                PlainFileProvider("devops", {"file": str(vf)}).set("K", "value-one")

                chain = modes_up_to(vf.parent, sd / "vaults")
                self.assertGreaterEqual(len(chain), 2, "a one-element chain is vacuous")
                for p, mode in chain.items():
                    self.assertEqual(
                        mode, 0o700,
                        f"under umask {oct(um)}, {p} came out {oct(mode)[-3:]} — the "
                        f"registry going first must not cost the levels below it")
                self.assertEqual(stat.S_IMODE(vf.stat().st_mode), 0o600, "and the file")

    def test_the_state_directory_is_charters_to_choose(self):
        """The residual, closed (#470). `.charter/` is the registry's to create — and the
        registry creates it through the same walk now.

        Asserted as *the umask does not decide it* — three umasks, one mode — and not
        merely as "0700 under 022". The property is the independence; a fix that only held
        under the umask it was written for would satisfy the weaker reading. The CLI-level
        sweep, in a plane built by `charter init` and driven through the real binary, is
        `tests/test_the_state_directory_is_charters_to_choose.py`; this case is the
        in-process pin on the ordering `charter vault add` produces.
        """
        seen = {}
        for um in (0o000, 0o022, 0o077):
            self._fresh_plane(f"s{um:03o}", um)
            self._add_vault("devops", "vaults/team/prod.json")
            seen[um] = stat.S_IMODE(Path(config.STATE_DIR).stat().st_mode)

        self.assertEqual(
            set(seen.values()), {0o700},
            f"the umask still decides `.charter/`: "
            f"{[(oct(u), oct(m)[-3:]) for u, m in seen.items()]}")

    def test_the_loose_state_directory_is_named_where_the_docs_say_it_is(self):
        """Reported, not silently accepted — and reported in `charter vault list`.

        The two claims travel together: charter declines to chmod a directory it did not
        create *because* it names it instead. A residual nothing reports is just a hole.
        """
        import argparse
        import io
        from contextlib import redirect_stdout

        from charter.commands_secrets import cmd_vault_list
        from charter.secrets import registry

        self._fresh_plane("report", 0o022)
        # Made by hand, before charter gets there: since #470 a `.charter/` charter creates
        # is 0700, so the directory that has to be REPORTED is the one that predates it —
        # an older charter's, or a `mkdir -p` at the umask default.
        sd = Path(config.STATE_DIR)
        sd.mkdir(parents=True)
        os.chmod(sd, 0o755)
        vf = self._add_vault("devops", "vaults/team/prod.json")
        PlainFileProvider("devops", {"file": str(vf)}).set("K", "value-one")

        self.assertEqual(stat.S_IMODE(sd.stat().st_mode) & 0o077, 0o055,
                         "precondition: there is something to report")

        _, detail = registry.provider_for("devops").health()
        self.assertIn("listed by other accounts", detail)
        self.assertIn(".charter 755", detail,
                      f"the health line must name the level that is loose, not merely "
                      f"that one is: {detail!r}")

        buf = io.StringIO()
        with redirect_stdout(buf):
            cmd_vault_list(argparse.Namespace())
        out = buf.getvalue()
        self.assertIn("listed by other accounts: .charter 755", out,
                      "`charter vault list`'s STATUS column is where docs/secrets.md now "
                      "says this appears, so that is where it has to appear")

    def test_the_note_is_where_the_docs_say_it_is_in_doctor_too(self):
        """#471, closed: `check_vaults` asks `loose_dirs()` and renders the same note.

        Kept here as the pair to the case above — the two surfaces are one claim, and a
        test file that pins only one of them is how they came apart in the first place.
        The rest of doctor's behaviour around it (the JSON, the WARN paths, one line per
        directory) is `tests/test_doctor_names_a_loose_state_directory.py`.
        """
        from charter import doctor

        self._fresh_plane("doctor", 0o022)
        sd = Path(config.STATE_DIR)
        sd.mkdir(parents=True)
        os.chmod(sd, 0o755)
        vf = self._add_vault("devops", "vaults/team/prod.json")
        PlainFileProvider("devops", {"file": str(vf)}).set("K", "value-one")

        res = doctor.check_vaults()
        self.assertIn("listed by other accounts: .charter 755", res.render())
        self.assertEqual(res.status, doctor.OK,
                         "and it stays a green line: a loose directory is an operator's "
                         "decision, not an unreachable vault")


if __name__ == "__main__":       # pragma: no cover
    unittest.main()
