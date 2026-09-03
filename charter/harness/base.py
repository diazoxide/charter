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

:attr:`Harness.layer`, :attr:`Harness.layer_note` and :attr:`Harness.trust_gate` are that
argument for a fifth, sixth and seventh, and it is the same one. `doctor`'s `session
layer` row answers *"can a session started in this directory see charter's layer?"* (#869)
— and the rules that decide differ per harness AND per artefact, measured rather than
documented. Written in `doctor` they would be exactly the hardcoded literal per harness
this file exists to stop: a harness added to ``KINDS`` would silently be reported under
Claude Code's discovery rules, which is the "verified a proxy instead of the fact"
failure #168, #177, #261 and #851 are each an instance of.
"""

from __future__ import annotations

import json
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


#: The :attr:`Deficit.key` a harness uses to say it cannot hold per-workspace
#: configuration — a workspace directory is not a config scope for it, so two workspaces on
#: one machine cannot be made to differ. NOT a claim that the harness ignores a project:
#: opencode and Codex both declare this key and both read something from a repository
#: (:attr:`Harness.layer_note` says what). Config scope and project scope are two things,
#: and conflating them is what sent an earlier reading of this key into the docs as "no
#: project-level config at all".
#:
#: A named constant rather than a string each side spells, because the two sides are far
#: apart: the harness declares it, and `workspace.harness_layer` reads it to decide
#: whether to ask that harness for files. A misspelling on either side would report a
#: workspace as carrying a layer it has never had — the "looks wired and is not" shape
#: #177 and #433 already cost this repo twice.
WORKSPACE_SCOPE = "workspace-scope"


class LayerPart(NamedTuple):
    """One artefact of charter's IN-REPO layer, and the rule a harness discovers it by.

    Data rather than a method, because the interesting thing about these is that **two
    parts of one harness's layer follow different rules**. Claude Code reads
    ``.claude/settings.json`` from the session's own directory and does not walk up, while
    ``.claude/agents/`` and ``.claude/skills/`` walk up and stop at the git boundary — so
    "charter is set up here" was never one fact, and an operator staring at a chat with no
    skills could not infer it from any single row (#869).

    ``paths`` are relative to the directory being asked about; **any** of them satisfies
    the part. ``keys`` narrows that to a JSON document carrying one of the named top-level
    keys, which is what makes the answer about *charter's* layer rather than about a file
    that happens to share its name: a directory can hold a ``.claude/settings.json`` that
    is entirely somebody else's.

    ``why`` is printed only when the part does **not** reach, and it names the measured
    rule rather than a remedy. A row that says "missing" without saying which rule decided
    sends the reader to the wrong directory, which is the whole cost #851 recorded.
    """

    what: str
    paths: tuple[str, ...]
    #: Walk up from the directory, stopping at the git root — or read the directory alone.
    #: A bool and not an enum because charter has measured exactly two rules; a third
    #: belongs here the day something measures it, not before.
    walks: bool
    why: str
    keys: tuple[str, ...] = ()


def _search_dirs(here: Path, bound: Path | None) -> list[Path]:
    """The directories a lookup from *here* covers, stopping at *bound* inclusive.

    ``None`` bound means no walk at all — one directory. That is also the answer for a
    walking part whose directory is in **no** repository: where the walk stops there has
    not been measured, and a guess would put an unmeasured claim in the row whose entire
    job is to report measured rules. Under-claiming is the direction that cannot mislead.
    """
    if bound is None or bound == here or bound not in here.parents:
        return [here]
    out = [here]
    for parent in here.parents:
        out.append(parent)
        if parent == bound:
            break
    return out


def _json_object(p: Path) -> dict | None:
    try:
        doc = json.loads(p.read_text())
    except (OSError, ValueError, UnicodeDecodeError):
        return None
    return doc if isinstance(doc, dict) else None


def part_reaches(part: LayerPart, here: Path, bound: Path | None) -> bool:
    """Would a session started in *here* find *part*? *bound* is where a walk stops.

    *bound* is handed in rather than derived, and that is deliberate: `doctor` already
    asks git for the repository root, and a second spelling of "where does the walk stop"
    is how two answers about one directory come to disagree.

    A path that cannot be read is not found. Absent, unreadable and "not a JSON object"
    are one answer here — the host would resolve nothing from it either way — and treating
    a malformed file as a declaration would report a layer that is not there, which is the
    only direction this row must never be wrong in.
    """
    for d in _search_dirs(here, bound if part.walks else None):
        for rel in part.paths:
            p = d / rel
            if not part.keys:
                try:
                    if p.exists():
                        return True
                except OSError:
                    pass
                continue
            doc = _json_object(p)
            if doc is not None and any(k in doc for k in part.keys):
                return True
    return False


class Harness:
    """One agent runtime charter can run inside."""

    #: Every part of charter's in-repo layer, with the rule this harness finds each by.
    #:
    #: Empty means charter writes **no** in-repo layer for this harness — which is not the
    #: same as the harness having no in-repo surface, and :attr:`layer_note` is where that
    #: difference gets said. Empty with an empty note means charter has not measured this
    #: harness at all, and `doctor` prints that rather than a clean row: "no known gaps"
    #: and "no knowledge" read identically otherwise, the distinction `registry.deficits`
    #: already draws.
    layer: tuple[LayerPart, ...] = ()

    #: Where charter's layer comes from on a harness whose :attr:`layer` is empty, and
    #: which in-repo surfaces that harness reads that charter does not write.
    #:
    #: A sentence rather than a flag, for `Deficit.detail`'s reason: a capability that is
    #: simply absent reads as a broken integration, and the person who reads it that way
    #: files a bug. It is also the honest place for a measurement that has moved — what a
    #: harness reads from a project is a fact about a binary version, and it changes.
    layer_note: str = ""

    #: What this harness refuses to run in a directory that has not been TRUSTED, or ``""``
    #: when charter has not measured a trust gate here.
    #:
    #: The gate takes no argument saying which settings source declared the thing it is
    #: gating, so *"a file this session reads declares `charter hook pretooluse`"* and
    #: *"the guard will fire here"* are two different facts and an untrusted directory
    #: answers yes to the first and no to the second (#859). A harness charter has not
    #: measured says nothing, which is not a claim that it has no gate.
    trust_gate: str = ""

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

    def wiring_remedy(self) -> str:
        """What an operator can DO about :meth:`stale_wiring`, or ``""``.

        READ-ONLY. :meth:`upgrade` composes the same sentence, but writes on the way to it,
        and `doctor` needs the sentence without the write.

        It exists because the caller invented one. `doctor`'s harness row ended
        "→ charter reinit" — a command it chose, not one any harness had named — and
        `charter reinit` then printed "Up to date — nothing to do" over the very file the
        row was warning about. A remedy the reporter makes up is a remedy nothing tests.
        """
        return ""

    def wire(self, root: Path) -> list[tuple[str, str]]:
        """Write what this harness needs under *root*, IF ABSENT.

        Returns ``(status, label)`` pairs so `init` can report every harness the same way
        without knowing what any of them writes:

        * ``"created"`` / ``"present"`` (and any other write status) — *label* is a PATH,
          listed in the summary.
        * ``"unvouched"`` — *label* is a SENTENCE, warned about. Something in this
          harness's wiring that charter did not write and will not touch, plus what to do
          about it. A path in this bucket used to be listed as "already present", which is
          true about the filename and false about everything a reader takes from it: #433
          shipped a shim with its routing cut out under exactly that line.

        The restraint is the contract, not an implementation detail: charter never
        repairs a file it finds, because the operator's content is in there and silently
        reverting a deliberate edit is worse than leaving a stale file for `doctor` to
        name.
        """
        return []

    def workspace_files(self) -> dict[str, str]:
        """What this harness needs inside a directory charter OWNS — ``{relpath: text}``.

        The fifth member, and the argument for it is that :meth:`wire` answers a different
        question about a different file. `wire` writes into somebody else's tree — the
        plane root, whose `.claude/settings.json` is user-owned and git-tracked — so its
        contract is *if absent, never repair*, and its answer is a write. A workspace
        directory is charter's own (`workspace.ensure` makes it, `.charter-structure`
        stamps it), so its files are **generated**: they must be regenerable when the
        plane moves, and they must be READABLE without writing, because `doctor` reports
        their staleness from the SessionStart hook and a check that writes is not a check.
        A `dry_run` flag on `wire` would have bought the second of those and neither of
        the first two, and it would have put a branch inside every harness's writer for a
        root only one of them can use.

        So this returns the CONTENT and nothing else. Materialising it — the ownership
        marker, the digest, refusing a file charter did not write — is one generic loop in
        `charter/workspace.py` rather than a copy per harness, which is the same reason
        :meth:`ask_rule` returns a rule instead of writing one.

        **Empty means "nothing to put here", and a harness that empties it because it
        CANNOT must also declare :data:`WORKSPACE_SCOPE`.** Those are opposite facts that
        an empty dict alone spells identically — `registry.deficits`' own complaint — and
        the caller renders them differently: nothing, versus a named ceiling. The base
        default is empty because charter has no idea what an unmet harness needs, and
        `TestEveryRegisteredHarnessEitherCarriesFilesOrNamesTheCeiling` is what fails on
        the day one answers neither.

        Paths are RELATIVE and must stay inside the directory: they are joined onto a
        workspace root, and `..` in one would put charter's writes outside the boundary
        this whole mechanism exists to draw.
        """
        return {}

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

        The RETURN TYPE is the harness's, not charter's: a string where the harness's
        rules are strings, a structure where they are structures. :meth:`rule_text` is
        what a human is shown, so the writer never has to trade the shape it needs for
        one that happens to print.
        """
        return None

    def rule_text(self, pattern: str) -> str:
        """*pattern* as this harness's own FILE spells it — the line `guard` prints.

        `guard` prints what it wrote so the operator can read it back against the file,
        and 0.49.0's argument for that is a guard's whole worth: *"a guard whose output
        cannot be trusted is worse than no guard, because the tick is what stops you
        checking"*. Since #374 the read-back also carries a name the operator never
        typed — charter TRANSLATES an MCP pattern for opencode, and cannot check that
        their `mcp` block spells the server the same way — so this line is the only thing
        standing between a mistyped server and a rule that is inert for it.

        Separate from :meth:`ask_rule` because the two answer different questions.
        `ask_rule` answers the writer, in whatever shape that harness's file needs;
        this answers a human, in the spelling they will go looking for. Interpolating
        the first into a sentence is how opencode's `(tool, glob)` pair reached the
        operator as ``('slack_send', '*')`` — Python's repr of a 2-tuple, a thing that
        appears in no `opencode.json` ever written (#395).

        The default covers a harness whose rule already IS its spelling. A harness whose
        rule is structural must override: the base class does not know how that file
        writes it down, and a generic join would print a plausible shape nothing holds —
        the same defect in a different costume. ``""`` when there is no rule at all, and
        `guard` never prints this for a rule that was not written.

        One rendering serves both verbs, because it names WHERE the rule lives and the
        surrounding sentence carries the decision ("asking for" / "allowing"). That holds
        while :meth:`allow_rule` keeps its shared default; `TestBothVerbsTranslateTheSameWay`
        is what fails the day a harness overrides it.
        """
        rule = self.ask_rule(pattern)
        return rule if isinstance(rule, str) else ""

    def apply_ask_rule(self, root: Path, pattern: str, local: bool = False,
                       dry_run: bool = False) -> tuple[str, str]:
        """Write the rule under *root*. ``(status, detail)``.

        ``"added"``, ``"present"``, ``"malformed"`` (refused, never repaired), or
        ``"unsupported"`` with a reason. A harness that cannot express command patterns
        says so: charter's own hook still guards the command, and the difference between
        naming that limit and staying quiet is the difference between a limit and a lie.

        Two of those four are refusals, and they are **not the same refusal**. That
        distinction is load-bearing rather than descriptive, because `charter guard` is
        all-or-nothing across harnesses (#376) and has to decide which refusals stop it:

        * ``malformed`` is a condition of a FILE. Somebody can fix it, and until they do,
          writing the other harnesses is what leaves the plane holding a rule here and not
          there from one command. It stops the whole command.
        * ``unsupported`` is a standing property of the HARNESS **and this pattern**. Re-
          running never changes it — Codex answers it to every pattern there is, opencode
          to every `--local` one, and since #374 to a URL glob under one of the five
          permissions opencode will only take a bare action for — so treating it as a
          failure would mean charter could never write a guard rule anywhere. It is
          reported and stepped over, and the rule is honestly not in force there.

          "And this pattern" is worth the words. It read as harness-wide when only whole
          harnesses answered it, and a transaction keyed off "does this harness support
          anything" would still have passed every test then and be wrong now. What the
          caller must key off is this answer, to this pattern, on this call.

        ``dry_run=True`` answers the same question and writes nothing: the status is what
        a real call would return, so the caller can ask every harness before committing to
        any of them. It has to be the write path minus the write, never a validator of its
        own — a second implementation of "can this harness take the rule" eventually
        answers differently from the one that writes, and a transaction whose check
        disagrees with its commit prints a tick either way.

        Those four are the whole of what a harness answers. A write that RAISES is not a
        fifth answer to add here: an implementation cannot know whether the command has
        already written somebody else, which is the only thing that makes the failure worth
        distinguishing. `commands._guard_apply` catches the `OSError` and reports it under a
        status of its own, so a harness is free to let one out.
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

    def apply_allow_rule(self, root: Path, pattern: str, local: bool = False,
                         dry_run: bool = False) -> tuple[str, str]:
        """Write the allow rule under *root*. ``(status, detail)``, as
        :meth:`apply_ask_rule` — same four answers, same meaning, same ``dry_run``.

        ``local=True`` asks for the harness's MACHINE-LOCAL file — a rule that is one
        person's, on one machine. A harness with no such file must return ``unsupported``
        and say why rather than falling back to a shared one: an `allow` rule widens what
        runs unprompted, so a silent fallback would publish a personal trust decision to
        everybody, which is the failure the flag exists to prevent.

        That is also the sharpest case for ``unsupported`` not blocking anybody else:
        opencode answers it to every ``--local`` rule, so a transaction that stopped on it
        would turn the flag into a command that writes nothing at all.
        """
        return "unsupported", f"{self.name} has no command-pattern permissions"

    def rule_outranks(self, pattern: str) -> str:
        """What of THIS harness's own built-in decisions the rule will outrank, or ``""``.

        A harness has permission names of its own and its own defaults over them, and
        charter's rule is one more entry in that same table. Where the two collide the
        operator's sentence quietly decides something they did not name — the shape ADR
        0014 accepts by writing the host's rules rather than keeping charter's own, and
        the cost of accepting it is that somebody has to say when it happens.

        Returned as a finished sentence rather than a list of names, for the same reason
        :attr:`Deficit.detail` is: the *why* is the harness's, not the caller's — one
        harness resolves rules last-wins, the next may not — and a caller assembling the
        sentence would have to know which.

        Empty is the honest default, not a stub: a harness whose names charter cannot
        collide with has nothing to warn about, and inventing a caveat for it would train
        the operator to skip the ones that are real.
        """
        return ""
