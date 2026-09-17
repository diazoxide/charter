#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = [
#   "charter-cp @ git+https://github.com/diazoxide/charter@50d31dc66835592ccea625bab8f5a0da313f2444",
#   "time-machine>=2.16",
# ]
# ///
"""Generate the fixture planes by running the Python charter.

The planes under this directory are not written by hand. They are what charter itself
writes, so they are true by construction: `docs/plane-format.md` in the charter repo
describes the format, and this script produces planes in it.

    ./generate.py            # regenerate every plane in place
    ./generate.py --check    # regenerate into a temp dir and diff (CI)

Determinism: the clock, the hostname, the user, the session id and the timezone are all
pinned (see `_env` and `sitecustomize.py` below), so a regeneration that changes nothing
produces no diff. The charter version is pinned in the script header above.
"""

from __future__ import annotations

import argparse
import json
import filecmp
import os
import shutil
import subprocess
import sys
import tempfile
from datetime import datetime, timedelta, timezone
from pathlib import Path

HERE = Path(__file__).resolve().parent

# Every timestamp charter writes comes from this clock. Each command advances it by a
# minute, so files that record "when" keep a believable order instead of collapsing.
EPOCH = datetime(2026, 3, 2, 9, 0, 0, tzinfo=timezone.utc)
STEP = timedelta(minutes=1)

HOSTNAME = "fixture-host"
USER = "fixture"
SESSION = "fixture-session-1"

# What `sitecustomize.py` pins that no environment variable can reach: charter reads the
# wall clock directly, and puts `socket.gethostname()` into filenames.
SITECUSTOMIZE = '''\
"""Pins the clock and the hostname for fixture generation (see generate.py)."""
import os
import socket

import time_machine

time_machine.travel(os.environ["FIXTURE_NOW"], tick=False).start()
socket.gethostname = lambda: os.environ["FIXTURE_HOST"]
'''


class Plane:
    """A plane being generated: runs charter commands against one directory."""

    def __init__(self, root: Path, home: Path, pins: Path) -> None:
        self.root = root
        self.home = home
        self.pins = pins
        self.clock = EPOCH

    def _env(self) -> dict[str, str]:
        self.clock += STEP
        # A FRESH dict, never `{**os.environ, ...}`. charter resolves its plane from
        # `$CHARTER_ROOT` before anything else, so an inherited one — every shell inside a
        # real plane has it — would point charter at the operator's plane and every write
        # would land there. Replacing the environment is what stops that; the ancestor walk
        # in `_refuse_enclosing_plane` only covers the cwd route. `CHARTER_ROOT` is then set
        # positively, so the plane is pinned rather than merely un-inherited.
        return {
            "CHARTER_ROOT": str(self.root),
            # A curated PATH, not the caller's: charter writes a DIFFERENT harness layer
            # when `claude` is on PATH (it installs its plugin and writes `enabledPlugins`)
            # than when it is not (it writes its own `hooks.PreToolUse` block). Both shapes
            # are legal; which one a fixture captures must not depend on whose machine ran
            # the generator. `git` lives in /usr/bin on macOS and on the CI images.
            "PATH": "/usr/bin:/bin",
            "HOME": str(self.home),
            "PYTHONPATH": str(self.pins),
            "PYTHONDONTWRITEBYTECODE": "1",
            "TZ": "UTC",
            "LC_ALL": "C.UTF-8",
            "USER": USER,
            "LOGNAME": USER,
            "FIXTURE_NOW": self.clock.isoformat(),
            "FIXTURE_HOST": HOSTNAME,
            "CHARTER_SESSION_ID": SESSION,
            "CHARTER_NO_BACKGROUND_CHECKS": "1",
            "GIT_AUTHOR_DATE": self.clock.isoformat(),
            "GIT_COMMITTER_DATE": self.clock.isoformat(),
            "TERM": "dumb",
            "NO_COLOR": "1",
        }

    def charter(self, *args: str, stdin: str | None = None, cwd: Path | None = None) -> str:
        """Run one charter command in the plane. Fails loudly: a fixture built from a
        command that silently failed would be a lie."""
        done = subprocess.run(
            [sys.executable, "-m", "charter", *args],
            cwd=cwd or self.root,
            env=self._env(),
            input=stdin,
            capture_output=True,
            text=True,
        )
        if done.returncode != 0:
            raise SystemExit(
                f"charter {' '.join(args)} failed ({done.returncode})\n"
                f"stdout:\n{done.stdout}\nstderr:\n{done.stderr}"
            )
        return done.stdout

    def git(self, *args: str, cwd: Path | None = None) -> None:
        # The system and global gitconfig are shut out: `core.autocrlf`, `init.templateDir`,
        # `core.hooksPath` or `commit.gpgsign` on somebody's machine would otherwise change
        # what is generated, or stop it generating at all.
        env = self._env() | {
            "GIT_CONFIG_NOSYSTEM": "1",
            "GIT_CONFIG_GLOBAL": os.devnull,
        }
        subprocess.run(
            ["git", *args], cwd=cwd or self.root, check=True, capture_output=True, env=env
        )

    def init_git(self, path: Path) -> None:
        path.mkdir(parents=True, exist_ok=True)
        self.git("init", "-q", "-b", "main", cwd=path)
        self.git("config", "user.name", "Fixture User", cwd=path)
        self.git("config", "user.email", "fixture@example.com", cwd=path)


def _write_pins(pins: Path) -> None:
    pins.mkdir(parents=True, exist_ok=True)
    (pins / "sitecustomize.py").write_text(SITECUSTOMIZE)


def _generate(name: str, out: Path) -> None:
    """Build one plane in a scratch directory, then move it to `out`.

    It is built OUTSIDE this repository on purpose. charter finds its plane by walking up
    from the working directory, and this repository is normally cloned inside a real plane —
    building here would make every command act on the operator's own plane instead.
    """
    builder = PLANES[name]
    with tempfile.TemporaryDirectory() as tmp:
        scratch = Path(tmp)
        home = scratch / "home"
        pins = scratch / "pins"
        root = scratch / "plane"
        home.mkdir()
        _write_pins(pins)
        _refuse_enclosing_plane(root)
        plane = Plane(root, home, pins)
        plane.init_git(root)
        builder(plane)
        if not (root / "charter.toml").is_file():
            raise SystemExit(f"{name}: no charter.toml — charter did not write this plane")
        _prune(root)
        dest = out / name
        if dest.exists():
            shutil.rmtree(dest)
        dest.parent.mkdir(parents=True, exist_ok=True)
        shutil.copytree(root, dest)
        _write_empty_dirs(dest)


def _empty_dirs(plane: Path) -> list[str]:
    """Directories of the plane that hold no file at any depth, relative and sorted.

    A fresh plane has `inventory/` and `workspaces/` and nothing in them, and git cannot
    carry an empty directory. They are part of the format all the same, so they are
    recorded beside the plane instead of inside it.
    """
    empty = [
        str(d.relative_to(plane))
        for d in plane.rglob("*")
        if d.is_dir() and not any(p.is_file() for p in d.rglob("*"))
    ]
    return sorted(empty)


def _write_empty_dirs(plane: Path) -> None:
    listing = plane.parent / f"{plane.name}.empty-dirs"
    dirs = _empty_dirs(plane)
    if dirs:
        listing.write_text("\n".join(dirs) + "\n")
    else:
        listing.unlink(missing_ok=True)


def _prune(root: Path) -> None:
    """Drop what a committed fixture cannot or should not carry.

    Each of these is documented in README.md, because "what the fixture leaves out" is as
    much a part of using it as what it holds.
    """
    # Every git directory, the plane's own and each clone's. Git cannot track a path
    # inside a `.git` directory, so a fixture that kept one could not be committed. This
    # takes `<clone>/.git/info/exclude` with it — `docs/plane-format.md` specifies that
    # block, and a test that needs it makes a clone and lets charter write it.
    for gitdir in sorted(root.rglob(".git"), reverse=True):
        shutil.rmtree(gitdir) if gitdir.is_dir() else gitdir.unlink()

    # Caches are keyed by absolute path and carry the generator's scratch directory, which
    # is a different directory on every run and on every machine. They are `internal` in
    # the format, so nothing reads them for meaning.
    cache = root / ".charter" / "cache"
    if cache.exists():
        shutil.rmtree(cache)

    # Random key material: `.charter/fingerprint.key` is freshly generated per plane, so it
    # can never be reproduced, and a fixture has no business carrying key bytes at all.
    (root / ".charter" / "fingerprint.key").unlink(missing_ok=True)


def _refuse_enclosing_plane(root: Path) -> None:
    """Stop before writing if some directory above the scratch plane is itself a plane."""
    for parent in root.resolve().parents:
        if (parent / "charter.toml").is_file():
            raise SystemExit(
                f"refusing to generate: {parent} is a control plane, and charter would "
                f"act on it instead of the fixture"
            )


def build_minimal(plane: Plane) -> None:
    """The plane a stranger gets from `charter init`, and nothing else."""
    plane.charter("init", "--forge", "github", "--owner", "acme")


def build_daily(plane: Plane) -> None:
    """A plane in use: two workspaces (one LIVE, one local), a cloned repo, memory, todos,
    a second persona with memory of its own, a vault registry, and the session state a
    harness leaves behind."""
    plane.charter("init", "--forge", "github", "--owner", "acme")

    # A repo to clone. It is a plain local git repo, so no forge and no network are needed.
    upstream = plane.root.parent / "upstream" / "svc"
    plane.init_git(upstream)
    (upstream / "README.md").write_text("# svc\n")
    plane.git("add", "-A", cwd=upstream)
    plane.git(
        "commit", "-qm", "first", cwd=upstream
    )

    # A LIVE workspace: committed, so its files carry the un-ignore block in .gitignore.
    plane.charter("workspace", "create", "alpha", "--vision", "Ship the widget")
    plane.charter("workspace", "live", "alpha")
    plane.charter("workspace", "remember", "-w", "alpha", "The API returns 418 on Mondays")
    plane.charter("ws", "todo", "-w", "alpha", "Write the migration")
    plane.charter("ws", "todo", "-w", "alpha", "Review the rollout plan")
    plane.charter("ws", "todo", "-w", "alpha", "done", "write-the-migration")

    # A second repo that carries a `charter.toml` of its own — charter's own repo is one,
    # and cloning it into a workspace is ordinary. It is the case charter bug #200 was
    # about: the nearest manifest is NOT the plane, and a reader that stops at the nearest
    # one lands in the clone, with the wrong personas and the wrong memory.
    nested = plane.root.parent / "upstream" / "tool"
    plane.init_git(nested)
    (nested / "charter.toml").write_text('schema = 1\n\n[[forge]]\nkind = "github"\n')
    plane.git("add", "-A", cwd=nested)
    plane.git("commit", "-qm", "first", cwd=nested)

    # Clones inside the workspace, which get the generated harness layer.
    plane.git("clone", "-q", str(upstream), str(plane.root / "workspaces/alpha/svc"))
    plane.git("clone", "-q", str(nested), str(plane.root / "workspaces/alpha/tool"))
    plane.charter("workspace", "reinit", "alpha")
    plane.charter("workspace", "snapshot", "alpha")

    # A second workspace, left local (not committed): the other half of the LIVE/local pair.
    plane.charter("workspace", "create", "beta", "--vision", "Retire the old importer")

    # A second persona, with memory of its own and one shared memory.
    plane.charter(
        "persona", "create", "devops",
        "--role", "DevOps Engineer",
        "--delegate-when", "CI/CD pipelines, k8s deploys",
    )
    plane.charter("persona", "remember", "devops", "Cluster prod-1 lives in eu-west-1")
    plane.charter("persona", "remember", "devops", "The plane is the unit of work", "--shared")

    # The vault registry. The only value in it is an obvious non-secret: fixtures never
    # carry a credential, and `docs/plane-format.md` specifies the registry's shape only.
    plane.charter("vault", "add", "fixture", "--provider", "plain-file", "--persona", "steward")
    plane.charter("secret", "set", "fixture", "API_TOKEN", "--stdin", stdin="fixture-not-a-secret")
    plane.charter("secret", "get", "fixture", "API_TOKEN")  # mints .charter/fingerprint.key

    # Guard rules, in each harness's own file.
    plane.charter("guard", "ask", "terraform apply *")

    # A dispatch, which is what puts the hostname into a filename: the tally is
    # `personas/_dispatch/<YYYY-MM>.<host>.jsonl`. Without one, nothing in the fixtures
    # would exercise the hostname pin, and a break in it would go unseen.
    plane.charter(
        "hook", "posttooluse-dispatch",
        stdin=json.dumps(
            {
                "tool_name": "Task",
                "tool_input": {"subagent_type": "devops", "prompt": "check the rollout"},
                "tool_response": "agentId: a1b2c3d4",
                "session_id": SESSION,
                "cwd": str(plane.root),
            }
        ),
    )

    # What a harness session leaves behind: the per-session state a hook writes.
    plane.charter("workspace", "use", "alpha")
    plane.charter(
        "hook", "userpromptsubmit",
        stdin=json.dumps(
            {
                "cwd": str(plane.root / "workspaces/alpha/svc"),
                "session_id": SESSION,
                "hook_event_name": "UserPromptSubmit",
                "prompt": "add the migration",
            }
        ),
    )


PLANES = {"minimal": build_minimal, "daily": build_daily}


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--check", action="store_true", help="regenerate elsewhere and diff")
    ap.add_argument("planes", nargs="*", choices=list(PLANES) + [[]], default=list(PLANES),
                    help="which planes")
    args = ap.parse_args()
    names = args.planes or list(PLANES)

    if not args.check:
        for name in names:
            _generate(name, HERE)
            print(f"wrote {HERE / name}")
        # A plane carries a .gitignore that hides its own workspaces and state directory,
        # and it hides them here too. Tracked files stay tracked, so this is only needed
        # for paths git has not seen before.
        print("\nstage new files with:\n  git add -f tests/fixtures/planes")
        return 0

    with tempfile.TemporaryDirectory() as tmp:
        for name in names:
            _generate(name, Path(tmp))
        drift = []
        for name in names:
            same = _same_tree(HERE / name, Path(tmp) / name)
            print(("ok   " if same else "DRIFT") + f" {name}")
            if not same:
                drift.append(name)
    return 1 if drift else 0


def _same_tree(committed: Path, fresh: Path) -> bool:
    """Compare two planes by path and by CONTENT.

    Content, not `filecmp`'s default stat signature: two files of the same size written a
    moment apart compare equal under that default, which is every file here.
    """
    # Files only. A committed fixture cannot carry an empty directory, so a fresh plane
    # always has directories the committed one does not; they are compared through the
    # `.empty-dirs` listing below instead.
    def paths(root: Path) -> set[str]:
        return {str(p.relative_to(root)) for p in root.rglob("*") if p.is_file()}

    left, right = paths(committed), paths(fresh)
    same = True
    for missing in sorted(left - right):
        print(f"    only in committed: {missing}")
        same = False
    for added in sorted(right - left):
        print(f"    only in fresh: {added}")
        same = False
    for rel in sorted(left & right):
        a, b = committed / rel, fresh / rel
        if not filecmp.cmp(a, b, shallow=False):
            print(f"    differs: {rel}")
            same = False

    recorded = committed.parent / f"{committed.name}.empty-dirs"
    was = recorded.read_text().splitlines() if recorded.exists() else []
    now = _empty_dirs(fresh)
    if was != now:
        print(f"    empty directories differ: recorded {was}, fresh {now}")
        same = False
    return same


if __name__ == "__main__":
    raise SystemExit(main())
