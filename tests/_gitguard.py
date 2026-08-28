"""The git config every child of this suite reads is written here, not by the operator.

`_isolation` redirects the config directories charter writes outside the plane. This is the
same move for the one config file charter never writes and every `git` it spawns reads:
``~/.gitconfig``.

**The defect it closes is a HANG, and it is the operator's thumb.** A fixture repository
created by a test inherits ``commit.gpgsign = true`` from the machine's own global config.
With 1Password's ``op-ssh-sign`` as ``gpg.ssh.program`` — the setup on the machine this was
written on — ``git commit`` parks on a biometric prompt and the suite never returns. Not a
pass, not a fail, no verdict and no line saying why (#641). Measured on that machine, in a
bare temp repository with charter's own checkout out of the picture::

    $ git init -q . && echo x > a && git add a && git commit -m probe
    error: 1Password: failed to fill whole buffer
    fatal: failed to write commit object
    git commit -m probe  0.01s user 0.01s system 0% cpu 1:00.36 total

Sixty seconds and a failure with stdin closed; an indefinite prompt with a terminal
attached. **CI can never see this** — a runner has no signing config — so it is invisible
in the one place everybody looks, exactly as #545's 122 blocked `input()` calls were.

**Thirty-one modules run ``git commit``**, 1,384 children in one green run, and every one of
them was neutralised by hand or not at all. Three different spellings were in use:
``git -c commit.gpgsign=false commit`` on the argv (1,068 of the 1,384), a repo-local
``git config commit.gpgsign false`` in the fixture's ``setUp`` (208 more, all in one
module), and ``GIT_CONFIG_GLOBAL=/dev/null`` in a hand-built child environment. A test that
remembers none of the three is not refused, not reported and not slow — it hangs, and the
thirty-second module to be written is the one that will.

**So the answer is not thirty-one patches, it is one redirect.** ``$GIT_CONFIG_GLOBAL``
points at a file this package writes and ``$GIT_CONFIG_SYSTEM`` at ``os.devnull``, before
any test module is collected, so every `git` this process or its children run reads a
config the repository controls. Signing is off there because nothing here signs; the
identity, the default branch name and the rest are stated for the same reason `_ttyguard`
states what the terminal answers — so a green run means the same thing on a laptop with
1Password, on a laptop without it, and on a runner with no config at all.

**Redirected, not merely defaulted**, and that is `_isolation`'s rule for the same reason:
a developer who already has these set in their shell is exactly the case that hangs, and
inheriting theirs would keep the hole open for them.

What the redirect does NOT do is stop a test from spelling its own environment and dropping
it. `tests._planeguard.AmbientGitConfig` refuses that at the `Popen`, which is what makes
this a property of the suite rather than a default a new module can walk past.
"""

from __future__ import annotations

import os
import tempfile

#: The environment variables that decide which git config files a child reads, and what
#: this package sets them to. `_planeguard` asks for these names rather than spelling them
#: again, so the refusal and the redirect cannot disagree about what "neutralised" means.
#:
#: ``GIT_CONFIG_SYSTEM`` and ``GIT_CONFIG_NOSYSTEM`` are both set. Either alone would do;
#: together they cover git's two spellings of the same instruction, and a child that
#: rebuilds one of them by hand is still covered by the other.
NAMES: tuple[str, ...] = ("GIT_CONFIG_GLOBAL", "GIT_CONFIG_SYSTEM", "GIT_CONFIG_NOSYSTEM")

#: What the suite's global git config says. Every line is here because a git child would
#: otherwise read the operator's answer to the same question:
#:
#: * ``user`` — a commit needs an identity, and 1,384 of them were being made under
#:   whoever ran the suite. Ends in ``.invalid``, which RFC 2606 reserves, so an address
#:   that escapes into a fixture's output can never be a real person's.
#: * ``commit.gpgsign``/``tag.gpgsign`` — the hang.
#: * ``init.defaultBranch`` — a fixture repo's first branch is ``main`` on every machine
#:   rather than ``main`` or ``master`` depending on whose config it read.
#: * ``core.hooksPath`` — an operator with a global hooks path was running their own hooks
#:   inside every fixture repository this suite creates.
#: * ``filter``/``diff``/``merge`` drivers are simply absent, which is the point of
#:   redirecting the whole file rather than overriding four keys: the machine this was
#:   written on has ``filter.lfs.*`` globally, and a fixture repo that shells out to
#:   ``git-lfs`` is reading the machine again.
CONFIG = """\
[user]
\tname = charter test suite
\temail = suite@charter.invalid
[commit]
\tgpgsign = false
[tag]
\tgpgsign = false
[init]
\tdefaultBranch = main
[core]
\thooksPath =
"""

#: The file :func:`install` wrote, so a case can read it back rather than guess at it.
#: Named for what it is rather than ``PATH``, which in a file about child environments is
#: already taken by something else entirely.
FILE: str = ""

_INSTALLED = False


def environment() -> dict[str, str]:
    """The three variables, as a child of this suite must carry them.

    A mapping rather than the names alone, so a case that really does build its whole
    environment can splat it in — ``{"PATH": ..., **_gitguard.environment()}`` — which is
    the way out `_planeguard.AmbientGitConfig` names and `test_frame_owns_the_surface`
    takes. Raises rather than filtering if :func:`install` has not run: an empty answer
    would be a way out that quietly does nothing.
    """
    return {name: os.environ[name] for name in NAMES}


def install() -> None:
    """Write the config and point every git child at it. Idempotent.

    Called from ``tests/__init__.py`` **above the import that pulls charter in**, for
    `_ttyguard`'s reason one tool over: `charter.config` resolves a plane at import and
    `charter.root` reads a git worktree pointer while doing it, so the redirect has to be
    in force before charter is a module rather than after.
    """
    global _INSTALLED, FILE
    if _INSTALLED:
        return
    _INSTALLED = True
    directory = tempfile.mkdtemp(prefix="charter-suite-gitconfig-")
    FILE = os.path.join(directory, "config")
    with open(FILE, "w", encoding="utf-8") as fh:
        fh.write(CONFIG)
    os.environ["GIT_CONFIG_GLOBAL"] = FILE
    os.environ["GIT_CONFIG_SYSTEM"] = os.devnull
    os.environ["GIT_CONFIG_NOSYSTEM"] = "1"
