"""`charter init` writes the marker that makes a directory a plane — and then has to be
able to see it.

`charter.config` resolves the plane ONCE, at import, through `root.find_root_or_cwd`. That
is right for every command except one: `init` is the command whose own first act changes
the answer. It wrote `charter.toml` and carried on with `config.HAS_CONTROL_PLANE` still
`False`, in a directory that was by then a plane (#858).

## Why nothing was visibly broken

`cmd_init`'s helpers all take *root* as an argument and are handed the right directory
explicitly — `_create_baseline_dirs(root)`, `_wire_harnesses(root)`, `_ensure_front_door`,
which says in its own docstring that it writes "through explicit paths under *root* rather
than `config.PERSONAS_DIR`". So the stale globals were never *read* on the path that
mattered, and the defect was one code change away from surfacing rather than a live fault.

It surfaced under exactly such a change. Gating `hooks.context_block` on
`HAS_CONTROL_PLANE` (#852/#857) broke a fresh `charter init`: the gate asked "am I in a
plane?" during `init`, the answer was `False` in a plane charter had just built, and the
generated opencode context file came out empty. That gate was reverted rather than shipped,
and this module is why it can be written now.

## What is asserted, and what is deliberately not

The claim is not "`HAS_CONTROL_PLANE` is True afterwards" — that is one name, and the next
reader will reach for a different one. It is that **nothing derived from the root is left
stale**: after `init`, every name in `config.DERIVED` equals what `config.derive` produces
for the directory `init` actually wrote into. A setting added to `derive` tomorrow is
covered by that the day it is added, the same way `DERIVED` itself is computed rather than
listed.

The other half is *how*. Re-resolving — `find_root_or_cwd()` again — is the tempting
spelling and is wrong: `root._outermost` hops outward through an enclosing plane's
`workspaces/`, so a plane scaffolded inside another plane's workspace would re-derive to
the OUTER one and `init` would spend the rest of its run reporting on a plane it did not
create. `init` acts on the root it chose, so the re-derivation is `config.use(root)`, and
`TheAnswerIsTakenAtTheRootInitChose` is the case that tells the two apart.
"""

from __future__ import annotations

import io
import shutil
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from charter import commands, config, root as _root
from charter.harness import base, registry
from tests import _envguard, _isolation


class InitSeesItsOwnPlane(unittest.TestCase):
    """A throwaway root, with every derived setting pointing into it and restored after.

    `config.use` rather than `mock.patch.object(config, "ROOT", …)`, which is what the
    other `init` fixtures did: patching the one name looked like isolation while the rest
    of `config` still pointed at the developer's real plane, and it cannot survive a
    command that re-derives — the patcher would put `ROOT` back and leave the other twenty
    names in the temp directory for every test that ran afterwards.
    """

    def setUp(self) -> None:
        # `cmd_init` reports what it wrote on stdout/stderr. Captured, so a run of this
        # module is a row of dots rather than seven scaffolding reports.
        self.enterContext(redirect_stdout(io.StringIO()))
        self.enterContext(redirect_stderr(io.StringIO()))
        # Outside a frame, with no session id and no pinned workspace: stated here rather
        # than inherited from the shell the suite was launched from (#519, #521, #528).
        _envguard.unset_all()
        self.root = Path(tempfile.mkdtemp(prefix="edm-test-init-sees-")).resolve()
        self.addCleanup(shutil.rmtree, self.root, True)
        _isolation.point_config_at(self, self.root)

    def init(self, **kw) -> int:
        return commands.cmd_init(SimpleNamespace(
            forge=kw.get("forge", "github"), owner=kw.get("owner", "acme"),
            host=kw.get("host", None)))


class TheMarkerLandingChangesThisProcessesAnswer(InitSeesItsOwnPlane):
    def test_the_directory_init_just_made_a_plane_reads_as_a_plane(self):
        """The one-line version of #858, and the one a future `HAS_CONTROL_PLANE` gate
        depends on."""
        self.assertFalse(config.HAS_CONTROL_PLANE, "the fixture starts outside a plane")
        self.init()
        self.assertTrue((self.root / _root.MARKER).is_file(), "init wrote no marker")
        self.assertTrue(
            config.HAS_CONTROL_PLANE,
            "`charter init` built a control plane and this process still reports there is "
            "none. Any code that asks 'am I in a plane?' is correct everywhere except "
            "inside `init`, and the failure is invisible until somebody runs it (#858)")

    def test_nothing_derived_from_the_root_is_left_stale(self):
        """The general claim, so the next setting is covered without anybody remembering
        this file exists. `HAS_CONTROL_PLANE` is the name #858 was found through; `GROUP`
        is a second one that was already wrong (`--owner` is written into `charter.toml`
        by this very command and the parsed copy never saw it), and there is no reason to
        think the third would be noticed either."""
        self.init(owner="acme")
        fresh = config.derive(self.root, start=self.root)
        stale = {k: (getattr(config, k), fresh[k]) for k in config.DERIVED
                 if getattr(config, k) != fresh[k]}
        self.assertEqual(
            stale, {},
            "`init` left settings disagreeing with the plane it had just created — "
            "`{name: (what config says, what the plane says)}`. Re-derive where the "
            "marker lands (#858)")

    def test_the_owner_this_run_declared_is_the_group_this_run_reads(self):
        """`GROUP` named, because the general case above would pass over it if `derive`
        ever stopped reading `[[forge]] owner` — and because it is the plainest evidence
        that the staleness was never only about one boolean."""
        self.init(owner="diazoxide")
        self.assertEqual(config.GROUP, "diazoxide")

    def test_running_it_twice_leaves_the_same_agreement(self):
        """`init` is additive and idempotent, and the second run takes the branch where
        the marker is already there. A re-derivation written only into the "I wrote it"
        branch has to leave the second run correct as well — it does, because the first
        run already made it true, and this is the case that says so rather than assuming
        it."""
        self.init()
        self.init()
        self.assertTrue(config.HAS_CONTROL_PLANE)
        self.assertEqual({k: getattr(config, k) for k in config.DERIVED},
                         config.derive(self.root, start=self.root))


class TheWiringThatRunsAfterTheMarkerSeesAPlane(InitSeesItsOwnPlane):
    """The reported symptom, isolated from the harness it was found in.

    #857 gated a harness's generated context file on `HAS_CONTROL_PLANE`. `_wire_harnesses`
    runs inside `cmd_init`, *after* the marker is written, so the gate saw `False` and the
    file came out empty. Asserted through the registry rather than through opencode: the
    property is "everything `init` runs after the marker sees a plane", and pinning it to
    one harness would leave the next one to rediscover it.
    """

    def test_a_harness_being_wired_is_told_there_is_a_plane(self):
        seen = {}

        class Watcher(base.Harness):
            name = "watcher"

            def wire(self, root):
                seen["plane"] = config.HAS_CONTROL_PLANE
                seen["root"] = config.ROOT
                return [("created", ".watcher")]

        with mock.patch.dict(registry.KINDS, {"watcher": Watcher}):
            self.init()
        self.assertEqual(seen.get("root"), self.root,
                         "the harness was wired against a root that is not the one init "
                         "wrote the marker into")
        self.assertIs(
            seen.get("plane"), True,
            "a harness wired by `charter init` is told it is outside a control plane, in "
            "the plane `init` created two statements earlier. That is #858, and it is how "
            "a gated context file shipped empty from a fresh init (#857)")


class TheAnswerIsTakenAtTheRootInitChose(InitSeesItsOwnPlane):
    """Not re-resolved — and the difference is a whole plane.

    `root.find_root` hops outward through an enclosing plane's `workspaces/` and keeps
    hopping "until the answer stops moving" (`root._outermost`), because the plane holding
    the vault is the one every other command means. `init` means something else: the
    directory it was told to build in. Re-deriving by resolution rather than from that
    directory would leave `init` reporting on, and deriving state paths for, a plane it did
    not create — silently, and only for the nested case, which is the one charter's own
    dogfooding produces (`workspaces/<ws>/charter`).
    """

    def test_a_plane_scaffolded_inside_another_planes_workspace_is_the_one_init_built(self):
        outer = Path(tempfile.mkdtemp(prefix="edm-test-outer-")).resolve()
        self.addCleanup(shutil.rmtree, outer, True)
        (outer / _root.MARKER).write_text("schema = 1\n")
        inner = outer / "workspaces" / "dev" / "plane"
        inner.mkdir(parents=True)

        # Point config at `inner` the way a shell standing there has to be pointed
        # ($CHARTER_ROOT, which wins outright in `find_root`), since resolution on its own
        # answers `outer`.
        _isolation.point_config_at(self, inner)
        commands.cmd_init(SimpleNamespace(forge="github", owner="acme", host=None))

        # THE PREMISE, asserted after `init` rather than before it, because that is where
        # it is load-bearing. Before, `inner` has no marker and resolution answers `outer`
        # for the uninteresting reason. After, `inner` carries its own `charter.toml` and
        # resolution STILL answers `outer` — that is `_outermost`'s hop, and it is the only
        # thing that makes `config.use(root)` and a re-resolution distinguishable here.
        self.assertEqual(
            _root.find_root(inner), outer,
            "resolution no longer hops outward past a plane inside another plane's "
            "workspaces/, so this case can no longer tell the two spellings of the "
            "re-derivation apart — re-read the class before trusting it")

        self.assertEqual(
            config.ROOT, inner,
            "after `init`, config points at the ENCLOSING plane rather than the one init "
            "just built. The re-derivation re-ran resolution instead of deriving from the "
            "root init chose, so every state path init reports now belongs to a plane it "
            "did not create")
        self.assertTrue(config.HAS_CONTROL_PLANE)
        self.assertEqual(config.NESTED_ORIGIN, inner,
                         "and the nesting itself goes unrecorded, so nothing downstream "
                         "can say charter is standing in a plane inside a plane")


if __name__ == "__main__":
    unittest.main()
