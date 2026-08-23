"""What every harness has to answer, and nothing more.

A *harness* is the agent runtime charter runs inside — Claude Code, opencode, and
whatever comes next — as distinct from a *host*, which in this codebase is a forge
(``github.com``, a self-hosted GitLab). ADR 0015.

Charter does not model a harness; it models what it needs from one, and the interface
grows only when that need does. Three members answered while charter only lived inside a
harness: what it calls itself, what it cannot carry, and what has to be written on disk
for charter to work inside it. `charter <harness>` adds a fourth, because charter now
runs the harness rather than only living inside it, so it also needs to know how to
start one — what an operator types, and what argv to exec (ADR 0018, issue #345). That
fact lives here, on the harness, for the same reason the first three do: anywhere else
it is a hardcoded literal per harness, the exact failure `registry.py` iterating
``KINDS`` exists to end. A fifth member needs the same kind of argument, not just a use.
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

    #: The word an operator types after ``charter`` to run this harness in a frame.
    #: Distinct from :attr:`name`, which is the harness's own identity in
    #: ``$CHARTER_HARNESS``: ``claude-code`` names the harness, ``claude`` is the binary
    #: and what a hand types. Empty means charter cannot launch this harness.
    cli_name: str = ""

    #: The binary to exec. All three shipped harnesses set this to the SAME string as
    #: :attr:`cli_name` — the split is not there because they differ today, and saying
    #: so was a lie a reader could check in thirty seconds. It is kept because the two
    #: answer to different owners: `cli_name` is charter's own command surface, checked
    #: for collisions against every core `charter` command at parser-construction time
    #: (see `cli._wire`), while `binary` is whatever the harness's vendor happens to
    #: install. Either can move without the other — a harness renaming its binary, or
    #: charter having to rename a subcommand that collided — and collapsing them into
    #: one attribute would make each of those a change to the other's meaning.
    binary: str = ""

    def launch_argv(self, extra: list[str]) -> list[str]:
        """Argv for starting this harness, with the operator's arguments appended.

        A **list**, never a joined string, and that is a security property rather than a
        style preference: tmux does not shell-interpret separate arguments and does
        interpret a joined one (pinned against 3.7c). Returning a string here would put
        command injection back into every launch.
        """
        return [self.binary, *extra]

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

    def apply_ask_rule(self, root: Path, pattern: str,
                       local: bool = False) -> tuple[str, str]:
        """Write the rule under *root*. ``(status, detail)``.

        ``"added"``, ``"present"``, ``"malformed"`` (refused, never repaired), or
        ``"unsupported"`` with a reason. A harness that cannot express command patterns
        says so: charter's own hook still guards the command, and the difference between
        naming that limit and staying quiet is the difference between a limit and a lie.
        """
        return "unsupported", f"{self.name} has no command-pattern permissions"

    def allow_rule(self, pattern: str):
        """*pattern* as an ALLOW rule in this harness's syntax, or ``None``.

        Defaults to :meth:`ask_rule`, because every harness charter knows encodes the
        pattern the same way for both verbs — only the decision differs. A harness where
        that stops being true overrides this; sharing the default is what keeps one
        operator sentence from acquiring two spellings.
        """
        return self.ask_rule(pattern)

    def apply_allow_rule(self, root: Path, pattern: str,
                         local: bool = False) -> tuple[str, str]:
        """Write the allow rule under *root*. ``(status, detail)``, as
        :meth:`apply_ask_rule`.

        ``local=True`` asks for the harness's MACHINE-LOCAL file — a rule that is one
        person's, on one machine. A harness with no such file must return ``unsupported``
        and say why rather than falling back to a shared one: an `allow` rule widens what
        runs unprompted, so a silent fallback would publish a personal trust decision to
        everybody, which is the failure the flag exists to prevent.
        """
        return "unsupported", f"{self.name} has no command-pattern permissions"
