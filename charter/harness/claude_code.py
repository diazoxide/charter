"""Claude Code as one harness among several.

Nothing here is new behaviour — it is the runtime charter was built inside, written down
as a peer of the others so that "which harness am I?" stops being a question every
function answers by sniffing for Claude-Code-shaped variables.
"""

from __future__ import annotations

import json
import os
from pathlib import Path

from .base import Harness, LayerPart

NAME = "claude-code"

#: The keys charter mirrors from the plane's own `.claude/settings.json` into a workspace
#: directory, and the reason there are two rather than one.
#:
#: Claude Code reads project settings from the session's working directory and **does not
#: walk up**, so a chat whose cwd is `workspaces/<ws>/` reads no settings at all. Agents,
#: skills and CLAUDE.md *do* walk up and stop at a git boundary — which a workspace
#: directory is not, being a plain directory inside the plane's own repo — so those
#: already arrive and are deliberately NOT copied here (#850). A second copy of `skills/`
#: would shadow the plugin's own non-deterministically; Claude Code says so itself, by
#: name: *"is already taken by X, which takes precedence"*.
#:
#: `enabledPlugins` alone is not enough, which is the correction this list records: `env`
#: is where `$CHARTER_HARNESS` comes from on a harness with no per-shell hook, so a
#: workspace given only the plugin loads charter's skills and hooks and still cannot say
#: which harness it is.
#:
#: **`statusLine` was the third and is gone (#895).** Charter no longer writes a status
#: line into Claude Code's settings at all, so there is nothing of charter's under that key
#: to mirror. An operator who wires one by hand owns it the way they own `permissions`:
#: charter neither copies it sideways nor reports on it.
#:
#: Nothing else. `permissions` is the plane's decision about the plane's own root, and
#: copying a grant sideways into a directory nobody granted it in puts a permission in
#: force where no one clicked for it.
#:
#: **That argument is about a GRANT, and `permissions` is not only grants (#942).** The key
#: was excluded whole, so `ask` and `deny` — which cannot put anything in force, only take
#: away — went with it, and `charter guard ask 'terraform apply *'` did not prompt in the
#: workspace chat that would run `terraform apply` while telling the operator it "applies to
#: everyone on this repo". The restrictive half travels now, and it travels beside these
#: keys rather than among them: see :data:`RESTRICTIVE_BUCKETS`, which is a filter INSIDE
#: `permissions` and not a fourth top-level key, because a top-level `permissions` entry
#: here would carry `allow` the day somebody widened the list without reading this.
WORKSPACE_KEYS = ("enabledPlugins", "env")

#: The `permissions` buckets a plane's own rules travel into a workspace in — never `allow`.
#:
#: A restrictive rule is the OPPOSITE of a grant: `ask` adds a prompt, `deny` adds a
#: refusal, and neither can make anything run that would not have run anyway. So the
#: sentence above — *"puts a permission in force where no one clicked for it"* — is
#: untouched by carrying these, and leaving them behind was what put a safety rule out of
#: force in the one directory where the guarded command actually gets typed.
#:
#: `deny` is here even though **charter never writes one** (`commands.py`: every `deny` is
#: an operator's own deliberate choice). That is the reason to carry it, not a reason to
#: skip it: a rule charter cannot re-derive is a rule that exists nowhere else, and it was
#: being dropped by the same key filter with nothing saying so.
RESTRICTIVE_BUCKETS = ("ask", "deny")

#: Where the mirrored document goes, relative to the workspace directory.
WORKSPACE_SETTINGS = ".claude/settings.json"

#: Where a CHECKOUT's mirrored machine-local document goes — never a workspace directory.
#:
#: **Measured, not assumed; the first version of #942 assumed.** Claude Code 2.1.267, git
#: 2.50.1, `claude -p` against a `deny` on `mkdir` in throwaway repositories: a
#: `.claude/settings.local.json` at the GIT ROOT applies to a session started in a
#: subdirectory; one in that subdirectory applies too; in a linked worktree the MAIN
#: checkout's applies; and a session in a nested clone ignores the outer repository's and
#: reads its own. The docs date the git-root rule to v2.1.211.
#:
#: So `workspaces/<ws>/`, a directory inside the plane's repository, already reads the
#: plane's own local file, and a generated copy there is a leftover at best. A clone at
#: `workspaces/<ws>/<repo>/` is a git root of its own and reads nothing of the plane's —
#: the one place this is generated (:meth:`ClaudeCodeHarness.checkout_files`).
#:
#: **A separate file from the shared one, because the plane keeps two and they differ in
#: blast radius.** `charter guard ask --local` writes the plane's gitignored local file, so
#: that rule is one person's on one machine; folding it into the generated shared file would
#: publish it to every reader of a file that sits inside somebody else's repository.
#: `doctor.LANDING_PROMPT` is such a rule.
#:
#: **And it is not only charter's file** (:attr:`ClaudeCodeHarness.cowritten`): a clone's
#: git root is the clone, so this is exactly where "Yes, and don't ask again" saves a
#: standing approval for a session rooted there.
CHECKOUT_LOCAL_SETTINGS = ".claude/settings.local.json"

#: The settings files Claude Code resolves for a session, in the order it reads them.
#: ``~/.claude/settings.json`` is deliberately not among them: it is machine-global state
#: charter never writes, and this list answers what the REPO carries.
#:
#: **Two rules, not the one this used to state** ("from the session's own directory"). The
#: shared file is read from the session's directory and nowhere above it — measured on
#: 2.1.259 and again on 2.1.267. The local file is read from the session's directory AND
#: from the git root, the main checkout for a linked worktree — documented from 2.1.211,
#: measured on 2.1.267. The older reading predates #942: `doctor._settings_files` carried
#: it, so a hook or plugin declared only in the plane's local file went uncounted in a
#: workspace chat where the host runs it.
_PROJECT_SETTINGS = (".claude/settings.json", ".claude/settings.local.json")

#: Charter's layer, one part per discovery rule — measured on binary 2.1.259, the settings
#: rule re-measured on 2.1.267 for #942.
#:
#: **The `settings` part under-claims the local file, on purpose.** It looks for both files
#: in the session's own directory, which is true of both. What it cannot say is the second
#: place the local file is read — the git root — because `LayerPart` has two measured rules
#: and "this directory or the root" is neither; walking up would claim every directory in
#: between, which the host does not read. So a key only in the root's local file reads ✗
#: here while in force. That is the one direction `part_reaches` allows this row to be
#: wrong in, and charter never puts its own layer keys in that file.
#:
#: **Two parts and not one, because two rules.** `settings` is cwd-only; `skills+agents`
#: walk up and stop at the git boundary. `CLAUDE.md` is the third rule and is deliberately
#: NOT a part here: it walks up and is **not** git-bounded, so it arrives almost
#: everywhere — which is exactly why the reported chat read as half-configured rather than
#: as absent, and `doctor` says so in the row instead of checking for a file that is always
#: found.
#:
#: **There was a third part, `status line`, and #895 removed it.** It existed because two
#: facts that go missing separately have to be reported separately — but the two stopped
#: being separate the moment charter stopped writing `statusLine`. The part answered
#: *"does this directory carry charter's footer"* against a key charter no longer puts
#: anywhere, so on every plane charter sets up it could only ever answer ✗, and a row that
#: is always red about a surface nobody asked for is the cry-wolf failure `check_harness`
#: records. What a hand-wired footer does is its author's business, the same way
#: `permissions` is.
LAYER = (
    LayerPart(
        "settings", _PROJECT_SETTINGS, False,
        "project settings are read from the session's OWN directory and the host does not "
        "walk up, so nothing above it is in force",
        keys=WORKSPACE_KEYS),
    LayerPart(
        "skills+agents", (".claude/skills", ".claude/agents"), True,
        "skills and agents DO walk up, but the walk stops at the git root — anything "
        "charter wrote above that boundary is out of reach, while CLAUDE.md walks up and "
        "is NOT git-bounded, which is why a session like this reads as half-configured "
        "rather than empty"),
)


#: The plane-root paths a checkout of its own cuts a Claude Code session off from.
#:
#: The same two directories :data:`LAYER`'s walking part is about, said for a different
#: purpose: that part answers whether a session HERE finds them, this one is the spelling
#: charter mirrors into `workspaces/<ws>/<repo>/`. Not derived from `LAYER` because the two
#: only coincide for this harness — opencode's in-repo surface is resolved by a rule
#: `LayerPart` cannot express, so deriving one from the other would have kept opencode's
#: paths out of a clone for as long as its discovery rule went unmeasured.
#:
#: **`.claude/settings.json` is deliberately not here**, and neither is `CLAUDE.md`.
#: Settings arrive by :meth:`ClaudeCodeHarness.workspace_files`, which every checkout gets
#: through `_harness_files`; listing it here as well would mirror the plane's file over the
#: generated one. `CLAUDE.md` is the project-instructions line `Harness.inherited_paths`
#: draws, and it is left behind on purpose — but **not** on the rule the two below follow,
#: which is what this comment used to say. :data:`LAYER` has the measurement: it walks up
#: and is NOT git-bounded, so the plane's own copy already reaches a checkout inside the
#: plane and there is no gap here for a mirror to close. What a mirror would add is the
#: plane's instructions inside somebody else's repository, read there as that repository's.
WALKUP_DIRS = (".claude/agents", ".claude/skills")


def _restrictive(doc) -> dict[str, list[str]]:
    """*doc*'s :data:`RESTRICTIVE_BUCKETS`, empty ones dropped. ``{}`` for anything else.

    Read through `commands._rules_in` rather than by hand. That reader already tolerates
    every shape a hand-edited `permissions` can take — a block that is not an object, a
    bucket that is not a list, an entry that is not a string — and charter must not grow a
    second opinion about one file: the writer (`add_permission_rule`) and this mirror would
    then disagree about what is in the plane's settings, and only one of them prints.

    Empty buckets are dropped so that a plane whose `permissions` holds nothing but grants
    contributes no `permissions` key at all. An empty block in the generated file would read
    as policy — `workspace_files`' own *"writing an empty `{}` would look like a layer"*, one
    level in.
    """
    from .. import commands

    found = {b: commands._rules_in(doc, b) for b in RESTRICTIVE_BUCKETS}
    return {b: rules for b, rules in found.items() if rules}


class ClaudeCodeHarness(Harness):
    name = NAME

    layer = LAYER

    inherited_paths = WALKUP_DIRS

    #: Measured, and the reason `doctor` names a condition rather than a verdict (#859).
    #: The gate is on the DIRECTORY and is global — it takes no argument saying which
    #: settings source declared the hook — and it is inherited up to the git root. So
    #: `workspaces/<ws>/` rides the plane's acceptance (it is inside the plane's own
    #: repository) while a clone at `workspaces/<ws>/<repo>` or a linked worktree has a
    #: git root of its own and needs its own.
    #:
    #: This read "hooks or the status line" until #895. The binary still gates both — that
    #: measurement is unchanged — but charter now writes only one of them, and naming a
    #: surface charter no longer wires would send an operator looking for a footer that is
    #: not there. **Hooks are the half that still matters and the half that is still
    #: named**: an untrusted directory is a plane-root guard that does not fire, which is
    #: the whole reason this condition is printed at all.
    trust_gate = "hooks"

    #: Empty on purpose. Claude Code carries every surface charter has — it is the
    #: runtime charter grew inside, so it is the reference ceiling rather than a harness
    #: that happens to have no gaps recorded yet.
    deficits = ()

    cli_name = "claude"
    binary = "claude"

    def detect(self) -> bool:
        """``$CLAUDE_PLUGIN_ROOT`` is set for the plugin's own processes.

        The fallback is not tidiness: ``$CHARTER_HARNESS`` reaches Claude Code only once
        `init` has written it into settings, so without this every session already
        running — and every plane not yet reinitialised — would answer "no harness" the
        day the neutral variable ships.
        """
        return bool(os.environ.get("CLAUDE_PLUGIN_ROOT"))

    def wire(self, root: Path) -> list[tuple[str, str]]:
        """One static key in ``.claude/settings.json``'s ``env``.

        Claude Code has no per-shell hook the way opencode does, but ``env`` *"sets
        environment variables that apply to every session"* and this harness's name is a
        constant, so one key does the same job. Its session id needs no wiring: it keeps
        arriving as ``$CLAUDE_CODE_SESSION_ID``, which `session.current` still reads.

        `settings.json` is `commands.py`'s territory — it already owns the status line,
        the guard hook and the ask rules in that same file — so the plumbing stays there
        and this asks for it. Imported inside the method because `commands` imports the
        registry.
        """
        from .. import commands

        status, _path = commands.ensure_env_var(root, "CHARTER_HARNESS", self.name)
        return [(status, ".claude/settings.json (env)")]

    def provision(self, root: Path) -> list[tuple[str, str]]:
        """Install charter's own Claude Code plugin for the plane at *root* (#881).

        The CLI is the front door: it is what sits on ``PATH``, what every hook in
        ``hooks/hooks.json`` dispatches to, and what already writes `.claude/settings.json`
        — so it exists before a harness session does. The inverse shape, a plugin that
        bootstraps a CLI, has to guess at a Python environment it does not own.

        **Project scope**, from the plane's own directory, for the reason the README states
        as a feature: the plugin is what carries a plane's pinned version, so two planes on
        one laptop can sit on different charters without fighting. A ``user``-scope install
        would collapse that to one version per machine and would put charter's hooks into
        repositories nobody pointed charter at.

        A machine with no `claude` gets **no row at all** rather than a warning. An opencode
        or Codex plane is a supported install and has no Claude Code plugin to be missing;
        `check_harness` already states what each harness cannot carry, and a warning here
        would be the cry-wolf failure `doctor.py` keeps returning to.
        """
        from .. import plugincache

        status, detail = plugincache.install(root)
        if status == "installed":
            # Shaped for `commands._fold_entries`, which splits a label on `" ("`: the
            # head is the thing, the parenthesis is the note. `init` lists one line per
            # FILE and folds several notes onto it, and a label with no fold point makes
            # that line grow instead — the 254-column headline #231 capped.
            return [("created", f"the Claude Code plugin ({plugincache.PLUGIN_ID}, "
                                f"{plugincache.INSTALL_SCOPE} scope)")]
        if status == "present":
            # No parenthetical. Every caller here already says "already present" or
            # "Already there", so `(charter@charter is already installed …)` would be the
            # same word twice in one line.
            return [("present", "the Claude Code plugin")]
        if status == "unavailable":
            return []
        # `unknown` and `failed`. A SENTENCE, in the bucket `init` warns about rather than
        # lists: "the plugin is not installed" is exactly the state a reader must not take
        # away from a line under "already present" (#433, one level up).
        return [("unvouched",
                 f"The Claude Code plugin was not installed — {detail}. Without it charter's "
                 f"hooks do not run in this plane: no session context, no plane-root guard, "
                 f"no auto-save. Retry with `charter doctor --fix`, or install it by hand: "
                 f"`claude plugin marketplace add {plugincache.MARKETPLACE_SOURCE}` then "
                 f"`claude plugin install {plugincache.PLUGIN_ID} --scope "
                 f"{plugincache.INSTALL_SCOPE}`.")]

    #: The generated file Claude Code also writes into itself — a checkout's local settings,
    #: where "Yes, and don't ask again" lands. See :attr:`Harness.cowritten`.
    cowritten = (CHECKOUT_LOCAL_SETTINGS,)

    def _plane_documents(self) -> tuple[dict | None, dict | None]:
        """The plane's two settings documents — ``(shared, machine-local)``.

        ``None`` for either one that exists and cannot be parsed. That is NOT "nothing to
        mirror": :meth:`held_files` names it, and charter keeps the last good generated copy
        rather than withdrawing it over a file somebody is holding.

        One function because every caller needs the same pair, and asking twice is how a
        generator and a reporter come to disagree about what the plane says.
        """
        from .. import commands, config

        root = Path(config.ROOT)
        return (commands._load_settings(root)[0],
                commands._load_json_settings(root / commands.LOCAL_SETTINGS)[0])

    def held_files(self) -> dict[str, str]:
        """The generated file each unreadable plane file feeds, and that plane file's path.

        The shared file feeds `.claude/settings.json` in a workspace and a checkout alike; the
        local file feeds only a checkout's `.claude/settings.local.json`.
        """
        from .. import commands, config

        root = Path(config.ROOT)
        shared, local = self._plane_documents()
        held: dict[str, str] = {}
        if shared is None:
            held[WORKSPACE_SETTINGS] = str(commands._settings_path(root))
        if local is None:
            held[CHECKOUT_LOCAL_SETTINGS] = str(root / commands.LOCAL_SETTINGS)
        return held

    def restrictive_rules(self) -> dict[str, tuple[str, ...]]:
        """The plane's ask/deny rules, keyed by the generated file each rides in.

        Flat within a file, because the caller counts rather than renders. The shared file's
        rules ride in `.claude/settings.json` wherever it is generated; the local file's ride
        only in a checkout's `.claude/settings.local.json`. A file declaring none is absent
        rather than present and empty — a zero is not a count worth a sentence.
        """
        out: dict[str, tuple[str, ...]] = {}
        for rel, doc in zip((WORKSPACE_SETTINGS, CHECKOUT_LOCAL_SETTINGS),
                            self._plane_documents()):
            rules = tuple(rule for bucket in _restrictive(doc).values() for rule in bucket)
            if rules:
                out[rel] = rules
        return out

    def workspace_files(self) -> dict[str, str]:
        """The plane's own settings keys, as one document for a workspace to hold.

        **A 1:1 sync, and v1 has no overrides on purpose.** The whole of the requirement
        is that a chat standing in `workspaces/<ws>/` gets the layer a chat standing in
        the plane root gets; a `workspace.json` key that makes one workspace differ is a
        second feature, and building the divergence before anybody has asked for a
        specific one would ship a mechanism nothing exercises — ADR 0007's objection.

        **Read from the plane's committed file rather than composed from charter's own
        constants**, and that is what makes it a sync rather than a second generator. The
        argument survived #895 unchanged even though the constant it used to name did not:
        a plane whose operator edited one of these keys by hand would otherwise get
        charter's default mirrored into every workspace, silently reverting a deliberate
        choice in a new place. Reading the file cannot do that; composing from constants
        always can.

        Empty for a plane with no settings of its own, and empty for one whose settings
        are not parseable: `_load_settings` returns ``None`` for the second, and charter
        does not guess over a file somebody is holding. Empty here means the workspace
        gets no file and no marker at all, which is the honest rendering of "there is
        nothing to mirror" — writing an empty `{}` would look like a layer. An unparseable
        file is also HELD (:meth:`held_files`), so a workspace that already has a copy keeps
        it.

        **The shared file only.** Its restrictive half travels since #942; the plane's local
        file does not travel here at all, because Claude Code reads that file at the git root
        and a workspace directory is inside the plane's repository — see
        :data:`CHECKOUT_LOCAL_SETTINGS` for the measurement and :meth:`checkout_files` for
        the one place it is generated.
        """
        settings, _local = self._plane_documents()
        doc = {k: settings[k] for k in WORKSPACE_KEYS if k in settings} if settings else {}
        restrictions = _restrictive(settings)
        if restrictions:
            # Last, so the two mirrored keys keep the order they have always had in this
            # file and an existing workspace goes `stale` only where the plane really moved.
            doc["permissions"] = restrictions
        return {WORKSPACE_SETTINGS: json.dumps(doc, indent=2) + "\n"} if doc else {}

    def checkout_files(self) -> dict[str, str]:
        """A checkout's `.claude/settings.local.json`: the plane's local ask/deny rules.

        Only in a checkout, because only there has the plane's own local file stopped
        reaching the session — measured on 2.1.267. Only the restrictive half, for
        `WORKSPACE_KEYS`' reason. ``{}`` when the plane's local file declares none.
        """
        _settings, local = self._plane_documents()
        restrictions = _restrictive(local)
        if not restrictions:
            return {}
        return {CHECKOUT_LOCAL_SETTINGS:
                json.dumps({"permissions": restrictions}, indent=2) + "\n"}

    def upgrade(self, root: Path) -> tuple[str, str]:
        """Named, never run — the restraint `cmd_version_sync` already keeps.

        `claude` may be absent, may prompt for a scope, and the command mutates the
        reader's editor install. What changes here is only that this answer is now Claude
        Code's answer rather than the answer charter gave every harness.
        """
        from .. import update

        return "manual", update.PLUGIN_SYNC_CMD

    def ask_rule(self, pattern: str) -> str:
        from .. import commands

        return commands._as_rule(pattern)

    def apply_ask_rule(self, root: Path, pattern: str, local: bool = False,
                       dry_run: bool = False) -> tuple[str, str]:
        from .. import commands

        return commands.add_ask_rule(root, self.ask_rule(pattern), local=local,
                                     dry_run=dry_run)

    def apply_allow_rule(self, root: Path, pattern: str, local: bool = False,
                         dry_run: bool = False) -> tuple[str, str]:
        from .. import commands

        return commands.add_allow_rule(root, self.allow_rule(pattern), local=local,
                                       dry_run=dry_run)
