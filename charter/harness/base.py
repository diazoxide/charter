"""What every harness has to answer, and nothing more.

A *harness* is the agent runtime charter runs inside — Claude Code, opencode, and
whatever comes next — as distinct from a *host*, which in this codebase is a forge
(``github.com``, a self-hosted GitLab). ADR 0015.

The interface is deliberately three members wide. Charter does not model a harness; it
models the three things it needs from one: what it calls itself, what it cannot carry,
and what has to be written on disk for charter to work inside it.
"""

from __future__ import annotations

from pathlib import Path
from typing import NamedTuple


class Deficit(NamedTuple):
    """One capability charter cannot offer on a harness, and why.

    ``detail`` is the whole point. A capability that is simply missing reads as a broken
    integration, and the person who reads it that way files a bug — so each names the
    limit and what stands in its place.
    """

    key: str
    detail: str
    #: A command that closes the gap, or "" when charter has no answer. An empty remedy
    #: is a claim too — inventing one sends somebody off to configure something that does
    #: not exist, which costs more than an honest gap.
    remedy: str = ""


class Harness:
    """One agent runtime charter can run inside."""

    #: The value this harness puts in ``$CHARTER_HARNESS``. Its identity everywhere.
    name: str = ""

    #: What charter cannot offer here. Empty is a claim, not a default — it says this
    #: harness carries everything, and `doctor` prints that as a clean row.
    deficits: tuple[Deficit, ...] = ()

    def detect(self) -> bool:
        """Is this harness live, judged by its own native evidence?

        Only consulted when ``$CHARTER_HARNESS`` is absent — which happens in a session
        that started before the plane was wired, and in any harness whose wiring charter
        has not written yet. A harness with no native signal returns ``False`` and is
        simply not detected, which is honest: it is not evidence of absence, and nothing
        downstream treats it as such.
        """
        return False

    def stale_wiring(self) -> str:
        """Which charter wrote this harness's installed wiring, when not the running one.

        ``""`` for a harness whose wiring charter does not generate — Claude Code's plugin
        is installed by the host and carries its own version check.
        """
        return ""

    def wire(self, root: Path) -> list[tuple[str, str]]:
        """Write what this harness needs under *root*, IF ABSENT.

        Returns ``(status, label)`` pairs — ``"created"`` or ``"present"`` — so `init`
        can report every harness the same way without knowing what any of them writes.

        The restraint is the contract, not an implementation detail: charter never
        repairs a file it finds, because the operator's content is in there and silently
        reverting a deliberate edit is worse than leaving a stale file for `doctor` to
        name.
        """
        return []

    def upgrade(self, root: Path) -> tuple[str, str]:
        """Move THIS harness's installed charter artifact to the running CLI's version.

        ``("moved", detail)`` — charter rewrote a file it authored.
        ``("current", version)`` — already on this version, nothing to do.
        ``("manual", command)`` — a host owns the artifact, so charter NAMES the command.
        ``("absent", why)`` — charter does not know how this harness updates.

        The default is ``absent`` rather than a plausible command, and that is the whole
        reason this member exists on the base class. Two code paths used to answer this
        question without knowing about each other — `update.plugin_version_here()` and
        :meth:`stale_wiring` — and the caller consulted one, then printed a Claude Code
        command to everybody. :class:`Deficit` already records why a guess is worse than a
        gap ("sends somebody off to configure something that does not exist"); here the
        guess would be *run*, not merely read.
        """
        return ("absent", f"charter has not pinned how {self.name or 'this harness'} "
                          f"updates its charter artifact — `charter harness list`")

    def ask_rule(self, pattern: str):
        """*pattern* in this harness's own rule syntax, or ``None`` if it has none.

        The operator types one sentence — `charter guard ask "git push *"` — and never
        learns three syntaxes. ADR 0014: charter writes the harness's rules and keeps no
        list of its own, which only holds if the translation lives here.
        """
        return None

    def apply_ask_rule(self, root: Path, pattern: str) -> tuple[str, str]:
        """Write the rule under *root*. ``(status, detail)``.

        ``"added"``, ``"present"``, ``"malformed"`` (refused, never repaired), or
        ``"unsupported"`` with a reason. A harness that cannot express command patterns
        says so: charter's own hook still guards the command, and the difference between
        naming that limit and staying quiet is the difference between a limit and a lie.
        """
        return "unsupported", f"{self.name} has no command-pattern permissions"
