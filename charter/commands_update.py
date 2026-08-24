"""`charter update` — move charter, then say what the new version brings.

Three things get called "updating charter", and they have three different blast radii:

* the **CLI**, one machine-global install shared by every plane on this machine;
* the **harness artifact**, per project (see :meth:`charter.harness.base.Harness.upgrade`);
* the **pin** in ``charter.toml``, shared with every teammate once it is pushed.

This command converges them in the only order that is safe and stops at the one decision
that is not charter's to make. It is **idempotent**: run it when already current and it
goes straight to the news phase, which is what makes it safe for an agent to call without
first working out whether there is anything to do.

**CLI before the harness artifact.** Plugin-newer-than-CLI is the one direction that
breaks — the plugin dispatches ``charter hook <name>`` for handlers an older CLI does not
have, and `hooks.skew_message` is deliberately one-directional, so it stays quiet the other
way round. The release charter says this; the sequence here obeys it for the same reason.
"""

from __future__ import annotations

import shutil
import sys
from pathlib import Path

from . import config, util
from .update import SHARED_INSTALL_NOTE, _parse

#: The distribution, and the install commands that actually move it.
#:
#: NOT `uv tool upgrade`: it reports "Nothing to upgrade" for a git-installed charter and
#: leaves you pinned, which `commands._sync_cmd` already carries a comment about. Pinned
#: and `--refresh`ed rather than bare `--force`, because PyPI's simple index lags its
#: metadata endpoint and an unrefreshed install can succeed against a cached index while
#: leaving you on the old version.
_INSTALLERS = {
    "uv": ["uv", "tool", "install", "charter-cp=={version}", "--force", "--refresh"],
    "pipx": ["pipx", "install", "charter-cp=={version}", "--force"],
}

#: How an install identifies its owner. Path-shaped rather than "which binary exists",
#: because uv and pipx are frequently both installed and only one of them owns THIS
#: charter — asking the wrong one produces a confident no-op.
_MARKERS = (("uv", ("/uv/tools/",)), ("pipx", ("/pipx/venvs/",)))


def installer_for(executable: Path) -> tuple[str, list[str] | None]:
    """Which tool owns the install *executable* belongs to, and how it moves it.

    ``("unknown", None)`` is a real answer, not a failure: `docs/install.md` documents uv,
    pipx and pip, and there is no reliable way to move a plain `pip install` in somebody
    else's environment. Ambiguity resolves to *named, not run* — the same restraint charter
    keeps for a host's plugin command, for the same reason: a wrong guess here mutates the
    reader's machine.

    Not one of #390's self-relaunch sites, checked directly: *executable* here is
    STRING-MATCHED against `_MARKERS` to identify which tool owns the install
    (`uv`/`pipx`/unknown) — it is never handed to `-m` or otherwise used to import
    `charter`, so `-m`'s cwd-prepend hole (`util.self_relaunch_argv`'s own docstring)
    does not apply to this function at all.
    """
    p = str(executable)
    for name, markers in _MARKERS:
        if any(m in p for m in markers):
            return name, list(_INSTALLERS[name])
    return "unknown", None


def _baseline_file() -> Path:
    return config.STATE_DIR / "cache" / "update-baseline"


def read_baseline() -> str | None:
    """The version this plane last updated FROM, or ``None``.

    Per-developer and gitignored (it lives under ``STATE_DIR``): which version this laptop
    came from is not a fact about the plane, and ADR 0011 keeps the committed record to
    what git cannot know.
    """
    try:
        return _baseline_file().read_text().strip() or None
    except OSError:
        return None


def _stamp_baseline(version: str) -> None:
    try:
        p = _baseline_file()
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(version)
    except OSError:
        pass          # a missing baseline degrades the news RANGE, never the update


def _installed_version() -> str:
    from . import __version__

    return __version__


def _latest(live: bool = True) -> str | None:
    """The newest published version. A live read here, unlike the status line's.

    `update.maybe_spawn` exists so rendering never blocks on the network. This is an
    explicit command a person typed and is waiting on, so it asks — and falls back to the
    cache, saying so, rather than refusing to work offline.
    """
    from . import update

    if live:
        update.fetch_and_store()
    return (update.load().get("latest") or "").strip() or None


def _sync_to(version: str) -> tuple[bool, str]:
    """Install exactly *version* through whichever tool owns this install."""
    name, argv = installer_for(Path(sys.executable))
    if argv is None:
        return False, (f"charter was not installed by uv or pipx, so charter will not "
                       f"guess how to move it — run the equivalent of: "
                       f"pip install --upgrade charter-cp=={version}")
    if not shutil.which(argv[0]):
        return False, f"{name} owns this install but is not on PATH"
    cmd = [a.format(version=version) for a in argv]
    proc = util.run(cmd, check=False)
    if proc.returncode != 0:
        why = (proc.stderr or proc.stdout or "").strip().splitlines()
        return False, (why[-1][:200] if why else f"exit {proc.returncode}")
    return True, version


def _handoff(target: str, baseline: str) -> tuple[bool, str]:
    """Run the news phase in a fresh process of the NEWLY INSTALLED binary.

    Two jobs in one subprocess, which is why it is not optional. The new version's entries
    and probes exist only in the new wheel — this process is still the old build, as
    `cmd_version_sync` already says out loud — and a binary that answers with the target
    version IS the proof the install took. Without that, an install that succeeded against
    a cached index reports success while leaving you where you started.

    **This is the charter process that made #314 a loop**, and what used to bound it is
    worth naming rather than leaving to be rediscovered. `charter news --since` probes, and
    a probe can dispatch `update`, so the chain comes back here — in a new interpreter,
    where a counter in `news` starts at zero again. It terminated only because `cmd_update`
    stamps its baseline BEFORE it moves and hands the pre-install version down as *baseline*:
    once the target is installed the child asks for `between(installed, installed)`, which
    is empty because `news.between` is exclusive at the low end, so it probes nothing. True,
    and an accident — arithmetic in another module, one refactor from silently going away.
    The guard no longer rests on it: `news._ENV` marks the environment for the length of a
    probe and the charter started here inherits it, so it declines to probe whatever its
    range says. `tests/test_news_cross_process.py` pins both.
    """
    binary = shutil.which("charter")
    if not binary:
        return False, "the `charter` command is not on PATH from here"
    got = util.run([binary, "--version"], check=False)
    said = (got.stdout or got.stderr or "").strip().split()
    if got.returncode != 0 or not said or said[-1] != target:
        return False, f"the installed `charter` reports {' '.join(said) or '?'}, expected {target}"
    news = util.run([binary, "news", "--since", baseline], check=False)
    out = (news.stdout or "").strip()
    if out:
        print(out)
    return True, ""


def _bump_pin(target: str) -> bool:
    """Write the pin and land it. Team-affecting, so it happens only after verification.

    `charter save` rather than raw git: the pin lives in the PLANE ROOT, and on a plane
    whose repo requires pull requests that is the one tree #157 forbids branching — `save`
    knows how to land it either way (#167).
    """
    from . import commands, instance

    if not instance.set_locked_version(config.ROOT, target):
        util.err(f"could not write the lock into {config.ROOT / 'charter.toml'}")
        return False
    util.ok(f"pinned this control plane to charter {target}.")
    util.info(f"  share it: charter save 'charter: pin to {target}'")
    return True


def _resolve_target(args, installed: str, locked: str | None) -> tuple[str | None, bool, str | None]:
    """``(target, proposed, latest)``. *proposed* means charter stopped and asked.

    Moving the machine PAST a pin manufactures the drift `charter version` reports as an
    error, so the pin decides the target rather than being an afterthought to it.

    *latest* travels back out because :func:`_latest` is a LIVE read — this is a command a
    person is waiting on, not the status line's cached glance — and the caller needs the
    same number to explain what it is proposing. Asking PyPI twice to answer one question
    doubles that wait for nothing.
    """
    explicit = (getattr(args, "to", None) or "").strip()
    if explicit:
        return explicit, False, None
    if locked and _parse(installed) < _parse(locked):
        return locked, False, None    # conforming to a pin somebody chose affects nobody
    latest = _latest()
    if not locked:
        return latest or installed, False, latest
    if latest and _parse(latest) > _parse(installed):
        if not getattr(args, "bump", False):
            return None, True, latest  # moving past the pin moves the TEAM
        return latest, False, latest
    return installed, False, latest


def cmd_update(args) -> int:
    from . import doctor, harness, instance, news

    if news.probing():
        # FIRST, before the checkout refusal below and before anything is read, because
        # this one is true wherever it is typed. A news entry's `check:` is dispatched as a
        # charter subcommand, so `check: update --to X` reaches the installer below and
        # reinstalls the machine to answer a question about it (#314) — a probe that
        # installs software is a worse bug than a probe that hangs. The environment marker
        # cannot stop this: it only reaches processes charter SPAWNS, and this one is the
        # probe itself, running at the depth the guard permits by design.
        news.refuse_mutation()
        util.err("refusing to update from inside a news probe — a `check:` asks whether "
                 "this plane already has something; it cannot be the thing that gets it.")
        util.info("  the entry naming `update` is the defect:  charter news --pending")
        return 2

    if doctor._is_charter_checkout(config.ROOT):
        # `CONTRIBUTING.md` tells contributors to run `python3 -m charter …` from the
        # clone. Installing over that is never what "let me try the update command" meant,
        # and the failure is silent: the news phase would hand off to a binary that is not
        # the tree being edited, and report on it as though it were.
        util.err("this is a charter checkout — refusing to install over the tree you are "
                 "editing.")
        util.info("  what you probably want:  charter version")
        return 2

    installed = _installed_version()
    locked = instance.locked_version(instance.load(config.ROOT))
    target, proposed, latest = _resolve_target(args, installed, locked)
    if proposed:
        util.info(f"this plane pins {locked}, and {latest} is published.")
        util.info("  moving past the pin moves every teammate on their next session.")
        util.info("  do it:  charter update --bump")
        return 0
    if target is None:
        util.err("could not determine a target version (offline?). "
                 "Pass one explicitly: charter update --to X.Y.Z")
        return 1

    # BEFORE anything moves, so an interrupted update still knows where it started.
    _stamp_baseline(installed)

    if target != installed:
        util.warn(SHARED_INSTALL_NOTE)
        util.info(f"installing charter {installed} → {target} …")
        ok, detail = _sync_to(target)
        if not ok:
            util.err(f"could not install {target}: {detail}")
            return 1

    h = harness.get(harness.current())
    if h is None:
        # Never "nothing to do": absence of information is not evidence of health, and a
        # terminal-run update is the likeliest place for that to mislead.
        util.warn("no harness detected, so its charter artifact was not checked — "
                  "`charter harness list`.")
    else:
        status, detail = h.upgrade(config.ROOT)
        if status == "moved":
            util.ok(f"moved: {detail}")
        elif status == "current":
            util.ok(f"the {h.name} artifact is already on {detail}.")
        elif status == "manual":
            # Not "run: <detail>". `manual` means charter is declining to touch a file —
            # the detail says what is wrong and what would fix it, and prefixing a
            # sentence with "run:" told operators to paste a paragraph into a shell.
            util.info(f"the {h.name} artifact is not charter's to move:")
            util.info(f"  {detail}")
        else:
            util.warn(detail)
        stale = h.stale_wiring()
        if stale:
            # NOT "`charter reinit` adds what is missing": nothing is missing in any of
            # these states — a file is there and charter cannot vouch for it, or something
            # charter did not write loads beside it. reinit is additive and would have
            # printed "nothing to do", which is how this line sent operators to a command
            # that reported success and changed nothing. The remedy comes from the harness.
            util.warn(f"this plane's {h.name} wiring is {stale}.")
            remedy = h.wiring_remedy()
            # `manual` above already printed exactly this; saying it twice in one run is
            # how a reader learns the second half of the output is boilerplate.
            if remedy and not (status == "manual" and remedy == detail):
                util.info(f"  {remedy}")

    ok, why = _handoff(target, installed)
    if not ok:
        util.err(f"the install did not take: {why}")
        util.info("  nothing was reported about the new version, because charter could "
                  "not confirm it is the one running.")
        return 1

    if getattr(args, "bump", False) and target != locked:
        if not config.HAS_CONTROL_PLANE:
            # Nothing to pin TO. Writing a charter.toml here would conjure a control plane
            # out of a version bump, in whatever directory the command was typed.
            util.warn("no control plane here, so there is no pin to move.")
        else:
            _bump_pin(target)
    return 0
