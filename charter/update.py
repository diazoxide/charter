"""Is a newer charter published? — cached, and never on the status line's clock.

The status line renders on every turn, so it must never make a network call. This
mirrors :mod:`charter.glstate`: the renderer only ever *reads* a cache, and kicks
off a detached background refresh when that cache goes stale. A slow or offline
PyPI therefore costs a stale indicator, never a delayed prompt.

The check is deliberately unauthenticated and read-only — one GET of the JSON
metadata endpoint. Nothing is downloaded, installed or executed.

**The dev channel changes what "newer" means, and nothing else here.** On a plane that
declares ``[update] channel = "dev"`` there is no published version to compare against —
dev builds are never published (see :mod:`charter.channel` for why) — so "newer" becomes
*``main``'s head commit is not the commit this build was installed from*. That answer is
fetched, cached, TTL'd, cooled down and read on exactly the same terms as the PyPI one:
same cache file, same :data:`REFRESH_TTL`, same :data:`SPAWN_COOLDOWN`, same
:data:`NET_TIMEOUT`, same unauthenticated read-only GET, and the same absolute rule that
the render path only ever reads the cache. A second mechanism beside those brakes would
be a second thing to keep honest; there is one.
"""

from __future__ import annotations

import json
import os
import re
import subprocess
import time
from pathlib import Path
from typing import NamedTuple

from . import config, util

#: PyPI distribution name. The *command* is `charter`; the *package* is `charter-cp`
#: (PyPI would not allow `charter`), so the metadata lives under the latter.
DIST = "charter-cp"
_URL = f"https://pypi.org/pypi/{DIST}/json"

#: The repository the dev channel tracks, and the branch it tracks on it. **Constants,
#: not configuration.** ``charter.toml`` decides *whether* charter follows the dev channel
#: and never *what* it follows — a committed file that could name the repository would be
#: a committed file that decides which code your machine installs. Written out in two
#: pieces so the two URLs below are the only places they are joined, and so neither join
#: ever has a runtime value in it.
DEV_REPO = "diazoxide/charter"
DEV_BRANCH = "main"

#: One unauthenticated read-only GET, exactly like ``_URL`` above: the branch endpoint of
#: the public GitHub REST API, which answers with the head commit and needs no token for a
#: public repository. Nothing is downloaded, installed or executed by reading it.
_BRANCH_URL = f"https://api.github.com/repos/{DEV_REPO}/branches/{DEV_BRANCH}"

REFRESH_TTL = 24 * 3600   # re-check at most once a day — releases are not that frequent
SPAWN_COOLDOWN = 3600     # and at most one background attempt per hour, success or not
NET_TIMEOUT = 5           # a detached child, but still: never hang around


#: Said wherever a pin and an install disagree, and nowhere else.
#:
#: The pin is per control plane; the binary is one machine-global install. Two planes
#: pinning different versions cannot both be satisfied, and `version sync` does not fix
#: that — it picks a winner and puts the other plane into drift, which is how a plane that
#: nobody touched went from "in sync" to "drift" because of work done somewhere else.
#: charter is a control plane, not a version manager, so it says this rather than growing a
#: shim to resolve the pinned version per plane.
SHARED_INSTALL_NOTE = (
    "the `charter` binary is ONE machine-global install shared by every control plane on "
    "this machine, so syncing here can put another plane into drift (see: uv tool list). "
    "The per-plane version is the PLUGIN's — see `charter version`"
)

#: The per-project fix, and the whole of what #127 asked for.
#:
#: A Claude Code plugin is installed **per project**: `installed_plugins.json` records an
#: `installPath` into `~/.claude/plugins/cache/<marketplace>/<plugin>/<version>/`, and that
#: cache holds many versions side by side. Two projects on one machine were observed serving
#: two different charter versions at once — exactly what #127 reported as impossible.
#:
#: So this command is the one that honours a pin: it moves THIS project and no other, where
#: `version sync` conforms a binary every plane shares.
PLUGIN_SYNC_CMD = "claude plugin update charter@charter"


def plugin_version_here() -> str | None:
    """The version of the charter PLUGIN serving this project, or ``None``.

    ``$CLAUDE_PLUGIN_ROOT`` is a **documented** variable that Claude Code sets for the
    plugin's own processes, and it points at the versioned directory this project resolved
    to. That is the whole mechanism: no cache layout is parsed and `installed_plugins.json`
    is never read, because those are Claude Code internals — fine to look at by hand,
    never something to build on. Betting on an internal path is what `bin/edm` did, and it
    broke silently (#197).

    ``None`` outside a plugin process, which is the ordinary case for a `charter` typed in
    a terminal. Callers must say "not visible from here" rather than substituting the
    machine-global CLI's version and calling it this plane's — that conflation is #127.
    """
    root = os.environ.get("CLAUDE_PLUGIN_ROOT")
    if not root:
        return None
    try:
        doc = json.loads((Path(root) / ".claude-plugin" / "plugin.json").read_text())
    except (OSError, ValueError, UnicodeDecodeError):
        return None
    v = doc.get("version") if isinstance(doc, dict) else None
    return v.strip() if isinstance(v, str) and v.strip() else None


def _cache_file() -> Path:
    return config.STATE_DIR / "cache" / "update.json"


def _lock_file() -> Path:
    return config.STATE_DIR / "cache" / "update.checking"


def load() -> dict:
    try:
        return json.loads(_cache_file().read_text())
    except (OSError, ValueError):
        return {}


#: A version as PEP 440 spells it, from the regular expression in the PEP's Appendix B, and
#: nothing wider. Stdlib, because charter has no runtime dependencies (CONTRIBUTING), so
#: `packaging` is not there to ask.
#:
#: **``re.ASCII``, and a strip of only the six characters PEP 440 names.** Without the flag,
#: ``re.IGNORECASE`` folds Unicode: the KELVIN SIGN is a ``k`` in a local label and a DOTLESS
#: I (U+0131) completes ``preview``. `str.strip()` would also take a trailing U+2028 LINE
#: SEPARATOR off. Each would make a version out of a string no installer accepts as one.
#:
#: The local label is matched and then never read. See :func:`version_key` for why.
_PEP_440 = re.compile(r"""
    v?
    (?:(?P<epoch>[0-9]+)!)?
    (?P<release>[0-9]+(?:\.[0-9]+)*)
    (?:[-_.]?(?P<pre_l>alpha|a|beta|b|preview|pre|c|rc)[-_.]?(?P<pre_n>[0-9]+)?)?
    (?:-(?P<post_n1>[0-9]+)|[-_.]?(?P<post_l>post|rev|r)[-_.]?(?P<post_n2>[0-9]+)?)?
    (?:[-_.]?(?P<dev_l>dev)[-_.]?(?P<dev_n>[0-9]+)?)?
    (?:\+[a-z0-9]+(?:[-_.][a-z0-9]+)*)?
""", re.VERBOSE | re.IGNORECASE | re.ASCII)

#: PEP 440's spellings of the three pre-release phases, in the order the phases come.
_PRE_PHASE = {"a": 0, "alpha": 0, "b": 1, "beta": 1, "c": 2, "pre": 2, "preview": 2, "rc": 2}

#: What every string that is not a version keys as. Below every version, because an empty
#: tuple sorts before every tuple with something in it, and equal to every other
#: non-version, so no caller finds a direction between two strings that have none.
_NOT_A_VERSION = ()


def version_key(v: str | None) -> tuple:
    """The key that orders charter versions, as PEP 440 orders them. Every comparison of two
    charter versions goes through this one function, so they cannot disagree.

    It replaced `_parse`, which kept the digits of each dot-separated part and dropped the
    rest. ``0.60.0rc1`` became ``(0, 60, 1)``, so a release candidate compared newer than
    its release, and so did ``a``, ``b`` and ``.dev`` (#1050). Nothing had published a
    pre-release, so no comparison had yet met one. When one did, `charter update`, `version
    bump`, the status line's arrow and SessionStart's pin would all have got it backwards.

    Numbers compare as numbers (0.10.0 is newer than 0.2.0), and trailing zeros do not
    count (``0.60`` is ``0.60.0``). Around a release, PEP 440's order:
    ``X.devN < XaN.devN < XaN < XbN < XrcN < X < X.postN.devN < X.postN``.

    **A local label (``+dev``, ``+local``) does not move a version**, which is not PEP 440's
    order. `channel.build_label` appends one to the SAME wheel's number to say where the
    build came from, not that it is a later release, and `_parse` read ``0.61.0+dev`` as
    ``0.61.0``. No caller is handed a label today, because ``__version__`` carries none; the
    day one is, it gets the answer it always had.

    **Anything that is not a version sorts below every version and ties with every other
    one**, ``None`` included. `_parse` promised that too ("an unparseable version must never
    make the indicator claim an update") and kept it only for strings with no digits in
    them: ``build7`` read as ``(7,)``. Below, so that a cache or a pin holding junk never
    reads as newer, and every caller refuses in the direction it already did.
    """
    m = _PEP_440.fullmatch((v or "").strip(" \t\n\r\f\v"))
    if m is None:
        return _NOT_A_VERSION
    release = [int(n) for n in m["release"].split(".")]
    while release and release[-1] == 0:
        release.pop()
    if m["post_n1"] is not None:
        post = int(m["post_n1"])       # ``1.0-1``, PEP 440's implicit post-release
    elif m["post_l"]:
        post = int(m["post_n2"] or 0)
    else:
        post = None
    if m["pre_l"]:
        pre = (_PRE_PHASE[m["pre_l"].lower()], int(m["pre_n"] or 0))
    elif post is None and m["dev_l"]:
        pre = (-1,)    # ``X.devN`` comes before ``X``'s first alpha, not after its rc
    else:
        pre = (3,)     # a release, or its post-release, comes after every one of its phases
    # Absent sorts below any number for a post-release and above any number for a
    # development release: ``X < X.post0``, and ``X.dev0 < X``.
    return (int(m["epoch"] or 0), tuple(release), pre,
            -1 if post is None else post,
            (0, int(m["dev_n"] or 0)) if m["dev_l"] else (1,))


#: What the dev channel may say about a build that records no commit, and all it may say.
#:
#: `newer_head` nudges such a build on purpose, and its docstring says why. A nudge is not
#: a comparison, though. A cached head of `main` and a wheel's version number are not on
#: one axis, so charter cannot tell which is newer. `charter version` printed the cached
#: PyPI number as "published" and newer, and `report send` said "9d18d55 is out — this may
#: already be fixed", both to a 0.60.0 wheel that already contained `9d18d55` (#937). This
#: sentence claims only what the install record shows. Both surfaces print it, so they
#: cannot drift into describing one state two different ways.
NOT_INSTALLED_FROM_MAIN = (f"this plane follows `{DEV_BRANCH}`, but this build was not "
                           f"installed from a commit of it")


def dev_verdict(head: str) -> str:
    """What a dev plane may say about *head*, the short commit :func:`newer_head` returned.

    One comparison, and never a direction. `charter version` and `charter report send` both
    print this: one state described two different ways on two surfaces is how #937 started,
    and a constant shared by only ONE of the two cases below would leave the other free to
    drift back.

    **The build that has a commit is the case that looks safe and is not.** "``<head>`` is
    out" reads as *``main`` has moved past you*, which charter has not checked. The cache is
    per plane (:data:`config.STATE_DIR`) while the binary is one machine-global install
    (#127), so ``charter update`` run in plane A moves this build to a commit that plane B's
    cache has never heard of. B's cached head is then an ANCESTOR of what is running, and
    "is out" is exactly backwards — the same claim, in the same direction, that #937 is
    about. Unequal is all that was measured, so unequal is all this says.
    """
    from . import channel

    mine = channel.installed_commit()
    if not mine:
        return NOT_INSTALLED_FROM_MAIN
    return (f"this plane follows `{DEV_BRANCH}`, and the head cached for it ({head}) is "
            f"not the commit this build was installed from ({mine[:7]})")


def dev_remedy() -> str:
    """The command that moves THIS charter onto ``main`` — one answer for every surface.

    Beside :func:`dev_verdict`, and for the same reason. Two surfaces that describe one
    state with one sentence and then prescribe two different next steps have the same defect
    one line further down the message — which is exactly how it shipped: `charter version`
    learned about the checkout case and `report send` went on naming the installer.

    ``charter update`` refuses to install over the tree it is running from. It answers "the
    charter you are running IS this tree … it moves by git rather than by an installer:
    charter version", which is `commands.cmd_version` — so a reader working in a charter
    clone was handed a loop between two commands, each naming the other. The gate is the
    question `commands_update` already asks, :func:`channel.running_inside`, and that answer
    names the tree, because a bare ``git pull`` typed somewhere else moves something else.
    """
    from . import channel

    if channel.running_inside(config.ROOT):
        return f"git -C {channel.package_dir().parent} pull"
    return "charter update"


#: What a pin beside the dev channel amounts to, spelled once. The long form and the brief
#: one below both carry it, so a reader who meets the status line's row and then
#: `charter version`'s refusal reads the same words in both.
_TWO_CHARTERS = "two different charters"


class PinBesideDev(NamedTuple):
    """:func:`pin_beside_dev`'s answer, one field per kind of surface."""

    #: The conflict itself, for a surface with room for a sentence.
    conflict: str
    #: The two ways out: toward a pinned release, then toward ``main``.
    ways: tuple[str, str]
    #: The conflict and where both ways out are printed, for a surface with one short row.
    brief: str


def pin_beside_dev() -> PinBesideDev:
    """What every surface says about a pin beside ``[update] channel = "dev"``.

    Beside :func:`dev_remedy`, and for the same reason. The pair is ONE refused state, and it
    had six readers saying five things: session start refused it, `version bump` refused to
    write it, `version sync` installed the pin over a plane following ``main``, `charter
    version` and the status line recommended that sync, and `doctor` called it in sync or
    named a plugin update (#1018). Each caller says what IT did (nothing installed, nothing
    written); what the state is and how to leave it comes from here, so one surface cannot
    grow a third way out or quietly lose one.

    **The brief form is a field of this answer, not a sentence of the status line's.** A
    status line row has no room for both ways out, and a short wording written where it is
    drawn is exactly the second description of one state this function exists to prevent.
    So it is built here, from the same :data:`_TWO_CHARTERS`, and names the command whose
    output carries both ways out.

    **Neither way out is chosen for the operator.** Which charter the plane wants is theirs
    to say, and each is one edit. The second is worded to hold whether or not the plane
    already carries a pin, because `version bump` refuses on a pin-less dev plane too.
    """
    return PinBesideDev(
        conflict=(f"a `[charter] version` pin and `[update] channel = \"dev\"` ask for "
                  f"{_TWO_CHARTERS}"),
        ways=(f"to follow a pinned release, drop `[update] channel = \"dev\"` from the plane's "
              f"`charter.toml`",
              f"to stay on `{DEV_BRANCH}`, keep no `[charter] version` in the plane's "
              f"`charter.toml` and move this charter onto it:  {dev_remedy()}"),
        brief=f"pin + dev channel: {_TWO_CHARTERS} · charter version",
    )


def newer_head() -> str | None:
    """The dev channel's answer to "is there anything newer?" — a short commit, or None.

    Cache only. Like :func:`newer_than`, whose dev branch this is, it is called from the
    status line's render path and must never reach the network; `maybe_spawn` is what
    fills the cache, in a detached child.

    Three states, and the middle one is the reason this is not a plain equality test:

    * no cached head — nothing has been fetched yet, or the fetch failed. Say nothing.
      An indicator that appears because a check did not happen is worse than no indicator.
    * head equals the installed commit — current. Say nothing.
    * anything else — behind, and that deliberately INCLUDES an install with no commit at
      all. A plane that declares the dev channel while running the PyPI wheel has not got
      what it asked for, and the nudge is how it finds out; ``charter update`` moves it.
    """
    from . import channel

    head = (load().get("head") or "").strip()
    if not head:
        return None
    mine = channel.installed_commit()
    return None if mine and mine == head else head[:7]


def newer_than(current: str) -> str | None:
    """The cached latest version if it is strictly newer than *current*, else None.

    On the dev channel this hands off to :func:`newer_head` instead: there is no published
    version to be newer than, so the comparison is against ``main``'s head commit. The
    channel is read from `charter.channel`, which reads it from the config boundary where
    it has already been clamped to a closed set — this function never sees an operator's
    string, only a branch on one of two constants.
    """
    from . import channel

    if channel.is_dev():
        return newer_head()
    latest = (load().get("latest") or "").strip()
    if not latest or not current:
        return None
    try:
        return latest if version_key(latest) > version_key(current) else None
    except Exception:
        return None


def checked() -> bool:
    """Whether the cache holds the answer :func:`newer_than` compares on this channel.

    ``latest`` on the stable channel, ``head`` on dev, because a PyPI number is not what a
    dev plane is compared against. Without it `newer_than` answers None for "nothing
    newer" and for "nothing known" alike, and `charter version` read both as up to date.
    That was true for the hour before a first background check landed. With
    ``$CHARTER_NO_BACKGROUND_CHECKS`` set it is true for good, so the two are told apart
    here (#945).
    """
    from . import channel

    return bool((load().get("head" if channel.is_dev() else "latest") or "").strip())


def latest_display(installed: str) -> str:
    """The `latest` line, honest about what it is.

    `latest` is a reading of PyPI cached for up to :data:`REFRESH_TTL`, not a live answer.
    Usually the distinction does not matter. It matters completely in one case: when the
    INSTALLED version is newer than the cached one, the cache is *provably* out of date —
    you cannot be running something PyPI has not published — and printing the lower number
    beside it produced `installed 0.27.2 / latest 0.26.0`, a contradiction on one screen
    that invites the reader to distrust every other line in the output.

    So that case reports the staleness instead of the number. ADR 0013: do not present as
    checked what was not checked.
    """
    latest = (load().get("latest") or "").strip()
    if not latest:
        return "— (not checked yet)"
    try:
        stale = bool(installed) and version_key(latest) < version_key(installed)
    except Exception:
        stale = False
    if stale:
        return f"— (cached {latest} is stale: it predates the {installed} you are running)"
    return latest


def _fetch_latest() -> str | None:
    """One unauthenticated GET of PyPI's JSON metadata endpoint.

    Through `urlopen`'s DEFAULT opener, which honours ``$https_proxy`` — the setting a
    machine behind a proxy already has, and so the one this should keep reading.
    """
    import urllib.request
    try:
        with urllib.request.urlopen(_URL, timeout=NET_TIMEOUT) as r:
            return json.load(r)["info"]["version"]
    except Exception:
        return None


def _fetch_head() -> str | None:
    """One unauthenticated GET of the public branch endpoint — ``main``'s head commit.

    The URL is :data:`_BRANCH_URL`, built at import from two module constants. Nothing
    from ``charter.toml`` reaches it, on any path: the channel decides *whether* this runs
    and never *where* it points. Only a full 40-character hex commit id is accepted, so a
    surprising response body cannot put arbitrary text into the cache that the status line
    then renders.
    """
    import urllib.request
    try:
        with urllib.request.urlopen(_BRANCH_URL, timeout=NET_TIMEOUT) as r:
            sha = json.load(r)["commit"]["sha"]
    except Exception:
        return None
    if not isinstance(sha, str):
        return None
    sha = sha.strip()
    return sha if len(sha) == 40 and all(c in "0123456789abcdef" for c in sha.lower()) else None


def fetch_and_store() -> str | None:
    """Query PyPI — and on the dev channel, ``main``'s head — and cache the result.
    Runs in the detached child, never inline. Returns the published version, as before.

    **The record is merged, not replaced.** Two answers now live in one cache file, and a
    write that rebuilt the dict from scratch would drop whichever of them this call could
    not fetch: an offline moment on a dev plane would erase the published version the
    version-lock rows read, for no better reason than that the other GET failed.

    **``ts`` is stamped only when everything this channel asked for arrived.** ``ts`` is
    what :func:`maybe_spawn` measures :data:`REFRESH_TTL` against, so stamping it after a
    partial fetch would hold a half-filled cache for a day. On the stable channel that
    condition reads exactly as it always did — the PyPI call succeeded — because the head
    is not fetched there at all.
    """
    from . import channel

    dev = channel.is_dev()
    latest = _fetch_latest()
    head = _fetch_head() if dev else None
    if latest is None and head is None:
        return None
    record = dict(load())
    if latest is not None:
        record["latest"] = latest
    if head is not None:
        record["head"] = head
    if latest is not None and (head is not None or not dev):
        record["ts"] = time.time()
    try:
        p = _cache_file()
        config.private_mkdir(p.parent)
        config.write_for(p, json.dumps(record))
    except OSError:
        return None
    return latest


def maybe_spawn() -> None:
    """Kick off a detached refresh if the cache is stale. Non-blocking, best-effort.

    Two independent brakes, because every caller is on a hot path: the cache TTL, and a
    spawn cooldown that also covers *failed* attempts — otherwise an offline machine would
    fork a doomed child on every single render.

    Three callers, the same three the CI-state cache has: the status line's render
    (`statusline._brand`), the frame's gather, which a panel with no cache runs on every
    repaint, and the SessionStart hook. Until #938 the render was the only one, and no
    Claude Code chat reached it any more, so the cache sat for days on the plane that
    reported it. A new caller gets both brakes for free and needs no throttle of its own.

    ``$CHARTER_NO_BACKGROUND_CHECKS`` is the brake that does not wait for a cooldown, and it
    comes first: before the lock is stat'ed, the cache read or ``.charter/cache/`` created
    (#945). Somebody who said no to background checks gets no trace of one. `charter update`
    and `charter version bump` call `fetch_and_store` themselves and are not stopped.
    """
    if util.background_checks_off():
        return
    if not config.HAS_CONTROL_PLANE:
        return                # see `glstate.maybe_spawn` — no plane, nowhere to cache
    now = time.time()
    lock = _lock_file()
    try:
        if lock.exists() and now - lock.stat().st_mtime < SPAWN_COOLDOWN:
            return
        if now - (load().get("ts") or 0) < REFRESH_TTL:
            return
    except Exception:
        return
    try:
        config.private_mkdir(lock.parent)
        config.touch_for(lock)  # touch FIRST: a spawn storm is worse than a missed check
        # -P (util.self_relaunch_argv, #390): this ALSO runs on the status line's own
        # render path (see the module docstring's mirror of glstate) — without it, a
        # project directory with its own `charter/` package would shadow the installed
        # one on every render, exactly as quietly as glstate's own gl-refresh did.
        subprocess.Popen(
            util.self_relaunch_argv("_version-check"),
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
            stdin=subprocess.DEVNULL, start_new_session=True, env=util.child_env(),
        )
    except Exception:
        return
