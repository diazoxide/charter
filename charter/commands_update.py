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

**Every question about the running install is asked OF the running install** (#537). This
command decides two things before it does anything — *would an install land on a tree
somebody is editing*, and *which channel does this run follow* — and it used to answer both
by looking somewhere adjacent. The first read the CWD: a maintainer standing in a charter
clone was told "the charter you run is this checkout, moved by git" while their charter was
the ``uv tool`` install the dev channel documents, three commits behind, with ``charter
--version`` and the clone's ``HEAD`` naming two different commits that could not disagree
if the claim were true. The second read the ABSENCE of a plane: ``cd /tmp && charter
update`` found no ``[update] channel``, took that for *stable*, and failed a version
comparison against a dev build. ``charter.__file__`` and the PEP 610 ``direct_url.json``
answer both directly, and `charter.channel` is where they are asked.
"""

from __future__ import annotations

import shutil
import sys
from pathlib import Path

from . import config, update as _update, util
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

#: The requirement a dev install resolves — **a module constant, with no interpolation
#: point in it at all**, and that is the security property this whole feature turns on.
#:
#: `charter.toml` is committed and arrives from someone else's machine. It decides WHETHER
#: charter follows the dev channel (`instance.UPDATE_CHANNELS`, a closed set of two) and it
#: never decides WHAT charter installs: this string is written here, joined from
#: `update.DEV_REPO` and `update.DEV_BRANCH` which are also constants, and it reaches
#: `util.run` as one element of a list argv — no shell, no format call, no value read from
#: anywhere. The two incidents behind that rule are named in `instance.UPDATE_CHANNELS`.
#:
#: The literal spelling, so it can be typed by hand and be the same thing:
#: ``uv tool install --force git+https://github.com/diazoxide/charter@main``.
DEV_SPEC = f"git+https://github.com/{_update.DEV_REPO}@{_update.DEV_BRANCH}"

#: The dev half of :data:`_INSTALLERS`, keyed the same way and moved by the same
#: `installer_for` lookup.
#:
#: No ``--refresh``, unlike the pinned installers above: that flag exists to defeat a
#: *PyPI index cache* that lags the metadata endpoint, and there is no index in this path —
#: uv resolves a git ref and clones it. ``--force`` is what makes the install replace the
#: charter already there, which for a spec whose resolved commit changes under a fixed name
#: is the whole job.
_DEV_INSTALLERS = {
    "uv": ["uv", "tool", "install", "--force", DEV_SPEC],
    "pipx": ["pipx", "install", "--force", DEV_SPEC],
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
        config.private_mkdir(p.parent)
        config.write_for(p, version)
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


def dev_install_argv() -> tuple[str, list[str] | None]:
    """``(installer name, argv)`` for a dev install — a fresh list of module constants.

    The mirror of :func:`installer_for`, and deliberately a separate function rather than a
    ``{version}`` substituted into that one: :func:`_sync_to` builds its command with
    ``a.format(version=…)``, and a dev spec run through the same line would put a
    ``str.format`` call between a committed config file and an install command. There is no
    format call on this path, so there is nothing to reach through.

    "This path" means both ends of it — the argv is built here without interpolation and
    :func:`_sync_dev` runs it without any either. Saying it of the build site alone is what
    #455 was: true, and not the sentence anybody was relying on.

    A fresh list per call, never the table's own: the caller hands it to `util.run`, and a
    module-level list handed out is a module-level list something can edit for the life of
    the process (same reason `instance.density_slots` copies).
    """
    name, _pypi = installer_for(Path(sys.executable))
    argv = _DEV_INSTALLERS.get(name)
    return name, (list(argv) if argv else None)


def _sync_dev() -> tuple[bool, str]:
    """Install :data:`DEV_SPEC` through whichever tool owns this install.

    **The run site of the no-interpolation rule, and it is pinned here too (#455).**
    :func:`dev_install_argv` builds the argv with no format call; this function hands that
    argv to `util.run` element for element — nothing interpolated, nothing appended,
    nothing re-split, no shell. Both halves matter and only the first used to be pinned: a
    `str.format` added *here* survived the whole suite, because the build-site canary never
    reaches this line. `tests/test_dev_update_command.py` now watches the argv at the
    subprocess boundary, which is where every spelling of "run it" has to arrive.

    Compare :func:`_sync_to`, which legitimately DOES interpolate — ``a.format(version=…)``
    with a version resolved from PyPI. The dev spec must never travel that line, which is
    why these are two functions and not one with a flag.
    """
    name, argv = dev_install_argv()
    if argv is None:
        return False, (f"charter was not installed by uv or pipx, so charter will not "
                       f"guess how to move it — run the equivalent of: "
                       f"uv tool install --force {DEV_SPEC}")
    if not shutil.which(argv[0]):
        return False, f"{name} owns this install but is not on PATH"
    proc = util.run(argv, check=False)
    if proc.returncode != 0:
        why = (proc.stderr or proc.stdout or "").strip().splitlines()
        return False, (why[-1][:200] if why else f"exit {proc.returncode}")
    return True, DEV_SPEC


def _handoff_dev(baseline: str) -> tuple[bool, str]:
    """The dev channel's :func:`_handoff`: verify the install took, then run the news phase.

    The stable path proves an install by comparing ``charter --version``'s last word to the
    version it asked for. A dev install cannot use that test — the version number does not
    move, which is the entire reason dev builds are not published — so the proof is the
    thing that DOES change: the new binary reports a PEP 610 dev build. A ``charter
    --version`` still saying plain ``0.51.0`` means the PyPI wheel is still installed and
    ``uv`` exited 0 against something else.

    The news phase runs on the same terms and for the same reason (`_handoff`'s docstring
    owns the loop story, including why `news._ENV` and not arithmetic is what bounds it).
    Its range is empty here, because dev does not move the version — kept anyway rather
    than special-cased away, so a dev build installed over an older CLI still reports what
    the versions in between brought.
    """
    binary = shutil.which("charter")
    if not binary:
        return False, "the `charter` command is not on PATH from here"
    got = util.run([binary, "--version"], check=False)
    said = (got.stdout or got.stderr or "").strip()
    if got.returncode != 0 or "+dev" not in said:
        return False, (f"the installed `charter` reports {said or '?'}, which is not a dev "
                       f"build — the install did not replace what is on PATH")
    news = util.run([binary, "news", "--since", baseline], check=False)
    out = (news.stdout or "").strip()
    if out:
        print(out)
    return True, said


def _move_harness() -> None:
    """Move this harness's charter artifact, and say what happened. Never raises.

    Shared by both channels because it is the same job on both: the artifact tracks the
    CLI, and which channel the CLI came from is not something it has an opinion about.
    Extracted when the dev path was added rather than copied — two copies of a block that
    ends in `stale_wiring()` is two places for a status branch to be forgotten.
    """
    from . import harness

    h = harness.get(harness.current())
    if h is None:
        # Never "nothing to do": absence of information is not evidence of health, and a
        # terminal-run update is the likeliest place for that to mislead.
        util.warn("no harness detected, so its charter artifact was not checked — "
                  "`charter harness list`.")
        return
    status, detail = h.upgrade(config.ROOT)
    if status == "moved":
        util.ok(f"moved: {detail}")
    elif status == "current":
        util.ok(f"the {h.name} artifact is already on {detail}.")
    elif status == "manual":
        # Not "run: <detail>". `manual` means charter is declining to touch a file — the
        # detail says what is wrong and what would fix it, and prefixing a sentence with
        # "run:" told operators to paste a paragraph into a shell.
        util.info(f"the {h.name} artifact is not charter's to move:")
        util.info(f"  {detail}")
    else:
        util.warn(detail)
    stale = h.stale_wiring()
    if stale:
        # NOT "`charter reinit` adds what is missing": nothing is missing in any of these
        # states — a file is there and charter cannot vouch for it, or something charter
        # did not write loads beside it. reinit is additive and would have printed
        # "nothing to do", which is how this line sent operators to a command that
        # reported success and changed nothing. The remedy comes from the harness, so
        # doctor, init, reinit and update cannot contradict each other about it.
        util.warn(f"this plane's {h.name} wiring is {stale}.")
        remedy = h.wiring_remedy()
        # `manual` above already printed exactly this; saying it twice in one run is how a
        # reader learns the second half of the output is boilerplate.
        if remedy and not (status == "manual" and remedy == detail):
            util.info(f"  {remedy}")


def _refresh_plugin() -> None:
    """Force the Claude Code plugin back into step with the marketplace clone.

    **Only on the dev channel, and only because a version-keyed update cannot do it.**
    `claude plugin update charter@charter` compares version strings; charter's plugin
    version moves once per release; so between releases it correctly answers *already at
    the latest version* while the clone it came from has moved on. `plugincache` owns the
    mechanism and the evidence.

    Best-effort and loud about failing, never fatal: charter's CLI has just been replaced
    successfully at this point, and a plugin that could not be refreshed is a smaller
    problem than an update that reports failure over one.
    """
    from . import plugincache

    if not plugincache.available():
        return          # no Claude Code here — opencode and Codex have no plugin cache
    try:
        ok, detail = plugincache.force_refresh(config.ROOT)
    except Exception as e:
        # "Never fatal" has to be enforced, not asserted. `force_refresh` returns rather
        # than raises on every path it owns; this keeps the promise true for the ones it
        # does not — and the promise matters more here than anywhere else, because by this
        # line the CLI has already been replaced and the harness artifact already moved.
        # An exception escaping would end a SUCCESSFUL update in a traceback.
        util.warn(f"the plugin was not refreshed: {e}")
        return
    if ok:
        util.ok(f"plugin: {detail} — it loads on the NEXT session.")
    else:
        util.warn(f"the plugin was not refreshed: {detail}")


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


def _update_dev(args, installed: str) -> int:
    """`charter update` on the dev channel: install from git, then the two artifacts.

    The same command with the same shape as the stable path — CLI, then this harness's
    artifact, then verification — with three differences, each forced by what a dev build
    is rather than chosen:

    * **No target resolution.** There is nothing published to resolve against; the target
      is ``main``, which is what :data:`DEV_SPEC` says and all it says.
    * **The plugin is force-refreshed.** On stable, `claude plugin update` handles it at
      release time. On dev the version never moves, so a version-keyed update is a no-op
      by construction and the only mechanism left is uninstall + reinstall. See
      `_refresh_plugin`.
    * **No pin.** ``[charter] version`` is a published version a team conforms to; a commit
      of ``main`` is not one, and writing one into a committed file would put every
      teammate onto an unreviewed merge. ``--bump`` says so rather than doing nothing.

    **Nothing here installs by itself.** The status line nudges when ``main`` moves and the
    operator types this command — auto-installing unreviewed merges is committed content
    reaching execution without a moment of consent, which is the shape of #453.
    """
    from . import channel

    # BEFORE anything moves, so an interrupted update still knows where it started.
    _stamp_baseline(installed)
    before = channel.installed_commit()

    util.warn(SHARED_INSTALL_NOTE)
    util.info(f"installing charter from {DEV_SPEC} …")
    # Which install is being replaced, named rather than left to `readlink -f $(which
    # charter)`. #537's whole cost was that the two cases — the CLI IS this tree, and the
    # CLI is an install standing next to it — printed nothing that told them apart, so an
    # operator in a charter clone read "the charter you run is this checkout" about a `uv
    # tool` install and copied the install command out of `commands_update.py` instead.
    util.info(f"  over the charter this process is running: {channel.package_dir()}")
    ok, detail = _sync_dev()
    if not ok:
        util.err(f"could not install the dev build: {detail}")
        return 1

    _move_harness()
    _refresh_plugin()

    ok, said = _handoff_dev(installed)
    if not ok:
        util.err(f"the install did not take: {said}")
        util.info("  nothing was reported about the new build, because charter could not "
                  "confirm it is the one running.")
        return 1
    util.ok(f"on the dev channel: {said}")
    if before and before[:7] in said:
        util.info("  same commit as before — main had not moved.")
    if getattr(args, "bump", False):
        util.warn("--bump moves this plane's `[charter] version` pin, which names a "
                  "PUBLISHED release. A dev build has no such number, so the pin was left "
                  "alone.")
    return 0


def _update_dev_on_a_checkout(args) -> int:
    """The half of :func:`_update_dev` that is safe over the tree you are editing.

    `cmd_update` does two independent things on the dev channel: it moves the **CLI**, and
    it force-refreshes the **plugin**. Only the first is unsafe on a charter checkout. The
    plugin is a separate artifact under ``~/.claude/plugins/`` — outside this tree, and
    refreshed from a marketplace clone that is not this tree either.

    Refusing both was #456. `doctor`'s `plugin files` row names ``charter update`` as the
    fix; on a checkout that command refused — on the machine most likely to be tracking the
    dev channel at all, because a maintainer is who wants it. A remedy that refuses when
    followed costs the reader the next hint too, so the hint is made true here rather than
    replaced with a second command to know about.

    **The CLI refusal is not weakened, it is stated.** Nothing on this path installs
    anything: the operator's charter already IS this tree, moved by git.

    **And "already IS" is now measured** (#537). This branch is reached because
    `channel.running_inside` said the charter running this process loaded from under the
    plane root — not because the plane root looks like a charter clone. Standing in the
    clone with the dev channel's documented ``uv tool`` install on ``PATH``, the old
    condition printed the sentence above about a binary `git pull` cannot reach, and the
    two versions said so out loud: ``charter --version`` reported ``main @ e17801c`` while
    the clone's ``HEAD`` was ``97163fb``.

    Two more things it deliberately does not do. It does not move the harness artifact —
    `_move_harness` writes into the plane root, and on a charter checkout the plane root is
    the tree being edited, which is the same objection one directory over. And it does not
    stamp an update baseline, because nothing moved for a news range to be measured from.
    """
    from . import channel, plugincache

    util.warn(f"the charter you are running IS this tree ({channel.package_dir()}), so "
              f"nothing was installed over it.")
    util.info("  it moves by git rather than by an installer:  charter version")
    if not plugincache.available():
        util.info("  no `claude` on PATH either, so there is no plugin to refresh — "
                  "there is nothing this command can do from here.")
        return 0
    util.info("  the plugin lives outside this tree, so that half still runs:")
    _refresh_plugin()
    if getattr(args, "bump", False):
        util.warn("--bump moves this plane's `[charter] version` pin, which names a "
                  "PUBLISHED release. A dev build has no such number, so the pin was left "
                  "alone.")
    return 0


def cmd_update(args) -> int:
    from . import channel, instance, news

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

    explicit = (getattr(args, "to", None) or "").strip()

    if channel.running_inside(config.ROOT):
        # `CONTRIBUTING.md` tells contributors to run `python3 -m charter …` from the
        # clone. Installing over that is never what "let me try the update command" meant,
        # and the failure is silent: the news phase would hand off to a binary that is not
        # the tree being edited, and report on it as though it were.
        #
        # **Asked of the running install, not of the cwd** (#537). `_is_charter_checkout`
        # used to stand here, and it answers a question about the DIRECTORY — so a
        # maintainer who has both a clone and the `uv tool` install the dev channel
        # documents was told "the charter you run is this checkout, moved by git" about a
        # binary `git pull` cannot reach, and left three commits stale. Being in a charter
        # checkout does not mean running it; `charter.__file__` is what does.
        #
        # The CLI half. NOT the plugin half — see `_update_dev_on_a_checkout`, which runs
        # for the one shape of this command that has something left to do here.
        if channel.update_is_dev() and not explicit:
            return _update_dev_on_a_checkout(args)
        util.err(f"the charter you are running IS this tree ({channel.package_dir()}) — "
                 f"refusing to install over the tree you are editing.")
        util.info("  what you probably want:  charter version")
        return 2

    installed = _installed_version()
    if channel.update_is_dev() and not explicit:
        return _update_dev(args, installed)
    if channel.update_is_dev():
        # `--to X.Y.Z` names a PUBLISHED version, which is the one thing the dev channel
        # does not have. Rather than refuse it, honour it and say what just happened: this
        # is how somebody goes back to a release without first editing charter.toml, and
        # an update that silently installed `main` because the plane said so would be
        # ignoring the version the operator typed on the line in front of them.
        util.info(f"this plane tracks the dev channel; --to {explicit} overrides it for "
                  f"this run and installs the published release.")

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

    # `target != installed` compares two version NUMBERS, and a dev build carries the same
    # number as the release it was built from — that is the whole reason dev builds are
    # never published (`channel`'s docstring). So "is the running install already the
    # target" is asked of the install RECORD as well: a git install is not the published
    # wheel, whatever number it prints. Without that, a plane on the stable channel with a
    # dev build on it installed nothing, and then `_handoff` reported *the install did not
    # take* — because `charter --version` ends in a commit rather than in the target
    # (#537). Nothing to install and a failed verification of it is the worst pair.
    on_a_dev_build = channel.is_dev_build()
    if target != installed or on_a_dev_build:
        util.warn(SHARED_INSTALL_NOTE)
        if target == installed:
            util.info(f"installing the published charter {target} over the dev build this "
                      f"process is running …")
        else:
            util.info(f"installing charter {installed} → {target} …")
        ok, detail = _sync_to(target)
        if not ok:
            util.err(f"could not install {target}: {detail}")
            return 1

    _move_harness()

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
