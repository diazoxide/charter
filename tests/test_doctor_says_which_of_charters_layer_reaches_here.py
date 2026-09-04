"""One row for the question no single row could answer — #869, and #859 beside it.

An operator opened a chat in a workspace directory and found no skills, no agents and no
plugin. Nothing in `charter doctor` said why, and every row in it was individually telling
the truth: the artefacts are discovered by **different rules**, so *"charter is set up"*
was never one fact. Measured on Claude Code 2.1.259:

* `.claude/settings.json` — the session's own directory, **no walk-up**
* `.claude/agents/`, `.claude/skills/` — walk up, **stopping at the git root**
* `CLAUDE.md` — walks up and is **not** git-bounded

Three rules, so three answers, and this file pins each of them separately: a test that
asserted only "the row mentions the workspace" would pass over a check that got two of the
three backwards.

#859 rides along because it is the same question one level down. *"A file this session
reads declares `charter hook pretooluse`"* and *"the guard will fire here"* are two facts,
and an untrusted directory answers yes to the first and no to the second. Charter asks the
half it owns — a directory with a git root of its own carries its own trust acceptance —
and the cases below pin that it is asked of git and never of `~/.claude.json`, and that it
is reported as a **condition** rather than as a verdict.

Every case writes only into a `PersonaIso` tmp plane, and `setUp` asserts that before it
writes anything. Nothing here may touch the developer's real plane.
"""

from __future__ import annotations

import ast
import inspect
import json
import os
import subprocess
import textwrap
from pathlib import Path
from unittest import mock

from charter import config, doctor
from charter.harness import base, claude_code, codex, opencode, registry

from tests import _isolation

OK, WARN = doctor.OK, doctor.WARN

#: The three keys charter's layer is made of, hand-spelled. Imported from
#: `claude_code.WORKSPACE_KEYS` this would assert nothing: a check that stopped looking at
#: any of them would still find the constant agreeing with itself.
_LAYER_DOC = {
    "enabledPlugins": {"charter@charter": True},
    "statusLine": {"type": "command", "command": "charter statusline"},
    "env": {"CHARTER_HARNESS": "claude-code"},
}


def _code(fn) -> str:
    """*fn*'s executable body — no docstring, no comments.

    Parsed rather than string-stripped, and both halves of that matter. `__doc__` is
    **dedented** from 3.13 on, so `getsource().replace(fn.__doc__, "")` silently removes
    nothing and every assertion built on it passes over the prose it meant to exclude —
    measured here, not assumed. And dropping comments is the point rather than a side
    effect: a comment naming Claude Code is exactly what a decision like this should carry,
    while a *literal* naming it is the hardcoded-per-harness fact these tests refuse.
    """
    tree = ast.parse(textwrap.dedent(inspect.getsource(fn)))
    node = tree.body[0]
    body = getattr(node, "body", [])
    if (body and isinstance(body[0], ast.Expr)
            and isinstance(body[0].value, ast.Constant)
            and isinstance(body[0].value.value, str)):
        node.body = body[1:]
    return ast.unparse(node)


def _git_init(d: Path) -> None:
    """A repository, so `git rev-parse --show-toplevel` answers with *d*.

    No commit is made and none is needed: the boundary an upward search stops at, and the
    unit trust is inherited to, both exist the moment `.git` does.
    """
    subprocess.run(["git", "init", "-q", str(d)], check=True,
                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


class LayerCase(_isolation.PersonaIso):
    """A plane wired the way `charter init` leaves one, with a workspace under it."""

    def setUp(self) -> None:
        super().setUp()
        # The tripwire every write below depends on.
        self.assertIn("edm-test-", str(config.STATE_DIR))
        (config.ROOT / "charter.toml").write_text("schema = 1\n")
        self.enterContext(mock.patch.object(config, "HAS_CONTROL_PLANE", True))
        # A HOME with no user-level settings, so nothing here can pass on the developer's
        # own machine config and prove nothing on CI.
        self.home = self.tmp / "home"
        (self.home / ".claude").mkdir(parents=True, exist_ok=True)
        self.enterContext(mock.patch.dict(os.environ, {"HOME": str(self.home)}))
        os.environ.pop("CLAUDE_PLUGIN_ROOT", None)
        # The plane is a git repository, which is what makes `workspaces/<ws>/` ride its
        # trust and a clone under it not. `config.ROOT` is resolved because `os.getcwd()`
        # is and because git answers with the physical path — macOS reaches its temp
        # directory through `/var` → `/private/var`, and an unresolved fixture path fails
        # on the developer's machine while passing on CI.
        self.plane = Path(config.ROOT).resolve()
        _git_init(self.plane)
        self.workspace = self.plane / "workspaces" / "fleet"
        self.workspace.mkdir(parents=True, exist_ok=True)
        # Registered AFTER `PersonaIso`'s own cleanup, so it runs BEFORE it (LIFO): the
        # tmp tree cannot be removed out from under the process's own cwd.
        self.addCleanup(os.chdir, os.getcwd())
        # Claude Code unless a case says otherwise. `_envguard` clears `$CHARTER_HARNESS`,
        # and a row that answered for three harnesses would make every assertion below
        # ambiguous about which one it caught.
        self.enterContext(mock.patch.dict(os.environ, {"CHARTER_HARNESS": "claude-code"}))

    def rooted_at(self, where: Path) -> None:
        os.chdir(where)

    def settings(self, where: Path, doc: dict | None = None) -> Path:
        p = where / ".claude" / "settings.json"
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(json.dumps(doc if doc is not None else _LAYER_DOC))
        return p

    def agents(self, where: Path) -> Path:
        d = where / ".claude" / "agents"
        d.mkdir(parents=True, exist_ok=True)
        return d

    def clone(self, name: str = "api") -> Path:
        """A repo checked out inside the workspace — a git root of its own."""
        d = self.workspace / name
        d.mkdir(parents=True, exist_ok=True)
        _git_init(d)
        return d.resolve()

    def detail(self) -> str:
        return doctor.check_session_layer().detail


class TheThreeRulesAreThreeAnswers(LayerCase):
    """#869: settings, skills+agents and the status line go missing separately."""

    def test_settings_do_not_walk_up_from_a_workspace_directory(self):
        """The reported chat, exactly. The plane is wired and the session is not."""
        self.settings(self.plane)
        self.rooted_at(self.workspace)
        self.assertIn("settings ✗", self.detail())

    def test_it_names_the_rule_that_decided_rather_than_only_the_miss(self):
        """"missing" without the rule sends the reader to the plane's file, finds it
        wired, and costs the investigation this row exists to end (#851's own lesson)."""
        self.settings(self.plane)
        self.rooted_at(self.workspace)
        self.assertIn("does not walk up", self.detail())

    def test_settings_written_into_the_workspace_do_reach_it(self):
        """What #850 writes. The row has to go green on the fix or it is not measuring
        the thing the fix changes."""
        self.settings(self.workspace)
        self.rooted_at(self.workspace)
        self.assertIn("settings ✓", self.detail())

    def test_agents_walk_up_to_a_workspace_inside_the_planes_repository(self):
        """The rule that differs. `workspaces/<ws>/` is a plain directory inside the
        plane's own git repo, so the walk reaches the plane's `.claude/agents/` — which
        is why #850 deliberately copies no agents into a workspace."""
        self.agents(self.plane)
        self.rooted_at(self.workspace)
        self.assertIn("skills+agents ✓", self.detail())

    def test_the_walk_stops_at_the_git_root_of_a_clone(self):
        """A clone under the workspace has a `.git` of its own, so the walk never reaches
        the plane's agents however far above they are."""
        self.agents(self.plane)
        here = self.clone()
        self.rooted_at(here)
        self.assertIn("skills+agents ✗", self.detail())

    def test_the_walk_that_stops_says_it_stopped_at_the_git_root(self):
        self.agents(self.plane)
        self.rooted_at(self.clone())
        self.assertIn("stops at the git root", self.detail())

    def test_skills_alone_satisfy_the_walking_part(self):
        """Either directory answers it — a plane carrying skills and no agents is not a
        plane whose walking artefacts are missing."""
        (self.plane / ".claude" / "skills").mkdir(parents=True)
        self.rooted_at(self.workspace)
        self.assertIn("skills+agents ✓", self.detail())

    def test_the_status_line_is_reported_apart_from_the_settings_that_carry_it(self):
        """`WORKSPACE_KEYS`' own correction: a directory given only the plugin loads
        charter's hooks and skills and still renders no status line. Folded into one
        answer, the half that is present hides the half that is not."""
        doc = {k: v for k, v in _LAYER_DOC.items() if k != "statusLine"}
        self.settings(self.workspace, doc)
        self.rooted_at(self.workspace)
        detail = self.detail()
        self.assertIn("settings ✓", detail)
        self.assertIn("status line ✗", detail)

    def test_claude_md_is_named_as_why_such_a_session_feels_half_configured(self):
        """The fourth rule, and the one that explains the symptom: CLAUDE.md walks up and
        is NOT git-bounded, so charter's prose arrives where none of its machinery does.
        Without it the row reads as "nothing is here", which is not what the operator
        saw."""
        self.rooted_at(self.clone())
        self.assertIn("CLAUDE.md", self.detail())

    def test_a_settings_file_that_is_not_charters_is_not_charters_layer(self):
        """A directory can hold somebody else's `.claude/settings.json`. Counting it
        would report a layer that is not there — the one direction this row must never
        be wrong in."""
        self.settings(self.workspace, {"permissions": {"allow": ["Bash(ls:*)"]}})
        self.rooted_at(self.workspace)
        self.assertIn("settings ✗", self.detail())

    def test_an_unparseable_settings_file_is_not_a_declaration(self):
        p = self.workspace / ".claude" / "settings.json"
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text("{ not json")
        self.rooted_at(self.workspace)
        self.assertIn("settings ✗", self.detail())


class ItIsAFactAndNeverAVerdict(LayerCase):
    """`check_session_root`'s discipline, for its reason: a chat rooted in a clone reaches
    none of this and that is the designed workflow — charter writes nothing inside
    `workspaces/<ws>/<repo>/` on purpose. A row that warned there is a row operators learn
    to skip, and the remedies belong to `workspace layer` and `plane-root guard`, which
    have them."""

    def test_a_clone_with_nothing_in_it_is_still_ok(self):
        self.rooted_at(self.clone())
        self.assertEqual(doctor.check_session_layer().status, OK)

    def test_a_fully_wired_session_is_ok_too(self):
        self.settings(self.plane)
        self.agents(self.plane)
        self.rooted_at(self.plane)
        self.assertEqual(doctor.check_session_layer().status, OK)

    def test_it_offers_no_hint_to_follow_into_the_wrong_directory(self):
        """The row diagnoses; it does not send anybody anywhere. `charter reinit` writes
        the PLANE's settings and would not change a session rooted in a clone — a remedy
        that looks like a fix and is not is #851's own defect one level down."""
        self.rooted_at(self.clone())
        self.assertEqual(doctor.check_session_layer().hint, "")

    def test_it_says_which_directory_answered(self):
        here = self.clone()
        self.rooted_at(here)
        self.assertIn(str(here), self.detail())

    def test_no_plane_no_row(self):
        with mock.patch.object(config, "HAS_CONTROL_PLANE", False):
            r = doctor.check_session_layer()
        self.assertEqual(r.status, OK)
        self.assertIn("no control plane", r.detail)


class TrustIsAConditionNotAVerdict(LayerCase):
    """#859. The gate is on the DIRECTORY and it is global — it takes no argument saying
    which settings source declared the hook — so a declaration and a firing are two
    facts."""

    def test_a_workspace_inside_the_planes_repository_is_not_flagged(self):
        """Trust is inherited up to the git root, and `workspaces/<ws>/` is inside the
        plane's own repo. The `+` button and every workspace tab put a chat there, so a
        trust clause on the common case would be the cry-wolf failure this file's
        neighbours keep recording."""
        self.rooted_at(self.workspace)
        self.assertNotIn("trust:", self.detail())

    def test_a_clone_with_its_own_git_root_is_flagged(self):
        self.rooted_at(self.clone())
        self.assertIn("trust:", self.detail())

    def test_it_names_that_directorys_git_root(self):
        here = self.clone()
        self.rooted_at(here)
        self.assertIn(str(here), self.detail())

    def test_it_says_a_declaration_is_not_a_firing(self):
        """The whole point. The clone can declare `charter hook pretooluse` in its own
        settings and still run nothing, because the gate is on the directory."""
        self.settings(self.clone(), _LAYER_DOC)
        self.rooted_at(self.clone())
        self.assertIn("whatever any settings file declares", self.detail())

    def test_a_flagged_directory_is_still_reported_as_ok(self):
        """It names a condition, not a verdict — charter cannot know the directory has
        NOT been accepted, and warning as though it had not is exactly the guess #859
        refuses to make."""
        self.rooted_at(self.clone())
        self.assertEqual(doctor.check_session_layer().status, OK)

    def test_the_settings_being_present_does_not_suppress_the_condition(self):
        """A clone that IS wired is the case the row most needs to qualify: everything
        reads green and nothing runs."""
        self.settings(self.clone(), _LAYER_DOC)
        self.agents(self.clone())
        self.rooted_at(self.clone())
        detail = self.detail()
        self.assertIn("settings ✓", detail)
        self.assertIn("trust:", detail)

    def test_it_says_guard_seen_answers_per_plane_and_cannot_stand_in(self):
        """`guardseen` state lives under `config.STATE_DIR`, so a guard that fired in the
        plane last week still shows a recent sighting for a session rooted in a clone
        where nothing has ever dispatched."""
        self.rooted_at(self.clone())
        detail = self.detail()
        self.assertIn("per PLANE", detail)
        self.assertIn(str(config.STATE_DIR), detail)

    def test_the_host_s_own_trust_record_is_never_read(self):
        """The decision #859 argues for, pinned. Reading `hasTrustDialogAccepted` out of
        `~/.claude.json` would need two things nobody has measured — that a MISSING entry
        means "not trusted" rather than "never opened", and that the flag charter reads is
        the one the host gates on across versions — and reading absence as refusal warns
        at planes that are fine. A condition asked of git cannot be wrong that way."""
        code = _code(doctor.check_session_layer)
        self.assertNotIn("hasTrustDialogAccepted", code)
        self.assertNotIn(".claude.json", code)

    def test_a_directory_in_no_repository_at_all_says_nothing_about_trust(self):
        """No git root, no claim. Where an unbounded walk stops has not been measured and
        neither has what a trust boundary is without a repository."""
        loose = self.tmp / "loose"
        loose.mkdir()
        with mock.patch.object(doctor, "_git_root_of", return_value=None):
            self.rooted_at(loose)
            self.assertNotIn("trust:", self.detail())


class EveryHarnessAnswersForItself(LayerCase):
    """The rules are per harness AND per artefact, so they live on the harness. Written
    into `doctor` they would be the hardcoded-literal-per-harness failure
    `harness/registry.py` exists to end."""

    def as_harness(self, name: str) -> None:
        self.enterContext(mock.patch.dict(os.environ, {"CHARTER_HARNESS": name}))

    def test_opencode_gets_its_own_sentence_and_not_claude_codes_rules(self):
        """charter writes no in-repo layer for opencode, so looking for `.claude/` there
        and reporting it absent would be a confident answer to a question opencode was
        never asked."""
        self.as_harness("opencode")
        self.rooted_at(self.clone())
        detail = self.detail()
        self.assertIn("opencode:", detail)
        self.assertNotIn("skills+agents ✗", detail)

    def test_opencodes_sentence_records_the_in_repo_surface_it_does_read(self):
        """Measured against 1.18.23 with a real session: `opencode.json` at the repository
        root IS read — a malformed one fails the run outright — and `.opencode/agent/` is
        read from the project. A management CLI is not a session, and probing with one
        yields a confident false negative."""
        self.as_harness("opencode")
        self.rooted_at(self.workspace)
        detail = self.detail()
        self.assertIn("opencode.json", detail)
        self.assertIn(".opencode/agent", detail)

    def test_codexs_sentence_records_the_in_repo_surface_it_does_read(self):
        """Measured against 0.147.0 with `codex exec`: a project `.codex/config.toml` is
        ignored, and `.codex/skills/` is NOT — a sentinel skill reaches the model's
        context with zero tool calls."""
        self.as_harness("codex")
        self.rooted_at(self.workspace)
        detail = self.detail()
        self.assertIn(".codex/skills", detail)

    def test_a_harness_with_no_measured_trust_gate_gets_no_trust_clause(self):
        """Claude Code's gate is measured; opencode's is not. Printing the sentence under
        opencode's name would be exactly the borrowed answer this row refuses."""
        self.as_harness("opencode")
        self.rooted_at(self.clone())
        self.assertNotIn("trust:", self.detail())

    def test_an_unregistered_harness_is_not_answered_for(self):
        """"No known gaps" and "no knowledge" read identically otherwise — the
        distinction `registry.deficits` already draws, one row over."""
        self.as_harness("something-else")
        self.rooted_at(self.workspace)
        detail = self.detail()
        self.assertIn("no record", detail)
        self.assertNotIn("settings ✗", detail)

    def test_with_no_harness_named_every_registered_one_answers(self):
        """A `charter doctor` typed in a plain terminal has no harness to report for, and
        picking one would be a guess."""
        self.enterContext(mock.patch.dict(os.environ, {}, clear=False))
        os.environ.pop("CHARTER_HARNESS", None)
        self.rooted_at(self.workspace)
        detail = self.detail()
        for h in registry.all():
            self.assertIn(f"{h.name}:", detail)

    def test_doctor_names_no_harness_and_no_harness_path_of_its_own(self):
        """The pin that keeps the rules on the harness. A fourth harness registered in
        `KINDS` is covered the day it is registered, and cannot be silently reported under
        Claude Code's discovery rules."""
        code = _code(doctor.check_session_layer)
        for literal in (".claude", "claude-code", "opencode", "codex", "CLAUDE.md"):
            self.assertNotIn(literal, code,
                             f"`check_session_layer` names {literal!r} itself; that fact "
                             f"belongs on the harness")


class TheHarnessesDeclareWhatWasMeasured(_isolation.PersonaIso):
    """Properties of the declarations themselves, so a harness cannot be registered with a
    layer nobody can render."""

    def test_claude_code_declares_all_three_parts(self):
        got = [p.what for p in claude_code.ClaudeCodeHarness().layer]
        self.assertEqual(got, ["settings", "skills+agents", "status line"])

    def test_the_two_rules_are_both_represented(self):
        """One walking part and two that do not walk. A layer whose parts all shared a
        rule would not have needed this row at all."""
        parts = claude_code.ClaudeCodeHarness().layer
        self.assertEqual([p.walks for p in parts], [False, True, False])

    def test_every_part_says_why_it_would_be_missing(self):
        for h in registry.all():
            for part in h.layer:
                self.assertTrue(part.why.strip(),
                                f"{h.name}/{part.what} would print a bare 'missing'")

    def test_a_harness_with_no_layer_says_where_its_layer_comes_from(self):
        """Empty means charter writes no in-repo layer, NOT that the harness has no
        in-repo surface — `Deficit.detail`'s reason: a capability that is simply absent
        reads as a broken integration and gets filed as a bug."""
        for h in registry.all():
            if not h.layer:
                self.assertTrue(h.layer_note.strip(),
                                f"{h.name} declares neither a layer nor a note")

    def test_only_claude_code_claims_a_measured_trust_gate(self):
        gated = sorted(h.name for h in registry.all() if h.trust_gate)
        self.assertEqual(gated, ["claude-code"])

    def test_the_base_class_claims_nothing(self):
        """A harness registered tomorrow inherits silence, not Claude Code's answers."""
        h = base.Harness()
        self.assertEqual((h.layer, h.layer_note, h.trust_gate), ((), "", ""))

    def test_neither_note_claims_a_project_has_no_config_surface(self):
        """The claim that had to be retracted. `.codex/skills/` and `opencode.json` are
        both read from a project, measured with real sessions — so a note saying the
        project carries nothing would be false in the direction that stops charter using
        a surface it has."""
        for h in (opencode.OpenCodeHarness(), codex.CodexHarness()):
            self.assertNotIn("no project-level config exists at all", h.layer_note)
            self.assertIn("DOES read an in-repo", h.layer_note)

    def test_no_shipped_deficit_says_a_harness_ignores_the_project(self):
        """The retraction, pinned where it can regress. The two `workspace-scope` deficits
        kept their conclusion — a workspace **directory** is not a config scope for either
        harness — and lost the grounds they had for it. *"Config is machine-global"* is
        false of opencode (`opencode.json` at the repository root is read, and a malformed
        one fails the run outright) and *"no project-level config exists at all"* was read
        as "Codex ignores the project", which `.codex/skills/` refutes.

        Both sentences reached `docs/harnesses.md`, `docs/workspaces.md`, a news entry and
        `doctor`'s own aside before anybody re-measured. A wrong reason travels exactly as
        far as a wrong conclusion, and costs the same: somebody stops looking."""
        for h in registry.all():
            for d in h.deficits:
                self.assertNotIn("no project-level config exists at all", d.detail,
                                 f"{h.name} still claims the project carries nothing")
                self.assertNotIn("config is machine-global", d.detail,
                                 f"{h.name}'s ceiling rests on a refuted measurement")


class ThePartResolver(_isolation.PersonaIso):
    """`base.part_reaches` on its own, where the walk boundary can be stated exactly."""

    def setUp(self) -> None:
        super().setUp()
        self.top = (self.tmp / "repo").resolve()
        self.here = self.top / "a" / "b"
        self.here.mkdir(parents=True)

    def part(self, walks: bool, *paths: str, keys=()) -> base.LayerPart:
        return base.LayerPart("x", paths, walks, "why", keys=keys)

    def test_a_walking_part_finds_a_directory_above_within_the_bound(self):
        (self.top / ".claude" / "agents").mkdir(parents=True)
        self.assertTrue(base.part_reaches(
            self.part(True, ".claude/agents"), self.here, self.top))

    def test_a_walking_part_stops_at_the_bound(self):
        (self.top.parent / ".claude" / "agents").mkdir(parents=True)
        self.assertFalse(base.part_reaches(
            self.part(True, ".claude/agents"), self.here, self.top))

    def test_a_non_walking_part_ignores_the_bound_entirely(self):
        (self.top / ".claude").mkdir(parents=True)
        (self.top / ".claude" / "settings.json").write_text("{}")
        self.assertFalse(base.part_reaches(
            self.part(False, ".claude/settings.json"), self.here, self.top))

    def test_no_bound_means_no_walk(self):
        """A directory in no repository at all. Where the walk would stop has not been
        measured, so it covers that directory alone — under-claiming, which is the
        direction that cannot mislead."""
        (self.top / ".claude" / "agents").mkdir(parents=True)
        self.assertFalse(base.part_reaches(
            self.part(True, ".claude/agents"), self.here, None))

    def test_a_bound_that_is_not_an_ancestor_is_not_walked_to(self):
        """A caller handing in an unrelated root must not turn the walk loose on the
        filesystem."""
        (self.top / ".claude" / "agents").mkdir(parents=True)
        elsewhere = (self.tmp / "elsewhere").resolve()
        elsewhere.mkdir()
        self.assertFalse(base.part_reaches(
            self.part(True, ".claude/agents"), self.here, elsewhere))

    def test_a_bound_equal_to_the_directory_is_that_directory(self):
        (self.here / ".claude" / "agents").mkdir(parents=True)
        self.assertTrue(base.part_reaches(
            self.part(True, ".claude/agents"), self.here, self.here))

    def test_any_of_the_paths_satisfies_a_part(self):
        (self.here / ".claude" / "skills").mkdir(parents=True)
        self.assertTrue(base.part_reaches(
            self.part(False, ".claude/agents", ".claude/skills"), self.here, None))

    def test_keys_narrow_presence_to_a_document_that_carries_one(self):
        p = self.here / ".claude" / "settings.json"
        p.parent.mkdir(parents=True)
        p.write_text(json.dumps({"permissions": {}}))
        part = self.part(False, ".claude/settings.json", keys=("statusLine",))
        self.assertFalse(base.part_reaches(part, self.here, None))
        p.write_text(json.dumps({"statusLine": {"command": "charter statusline"}}))
        self.assertTrue(base.part_reaches(part, self.here, None))

    def test_any_of_the_keys_satisfies_a_part(self):
        p = self.here / ".claude" / "settings.json"
        p.parent.mkdir(parents=True)
        p.write_text(json.dumps({"env": {"CHARTER_HARNESS": "claude-code"}}))
        self.assertTrue(base.part_reaches(
            self.part(False, ".claude/settings.json", keys=("statusLine", "env")),
            self.here, None))

    def test_a_json_document_that_is_not_an_object_carries_no_keys(self):
        """The array **holds the key name**, and that is the whole test. `[]` proves
        nothing: `"env" in []` is already False, so a check that had dropped the type
        filter entirely would pass over it — the deletion sweep found exactly that and
        was right. `["env"]` separates "this document declares env" from "this document
        happens to contain the string"."""
        p = self.here / ".claude" / "settings.json"
        p.parent.mkdir(parents=True)
        p.write_text(json.dumps(["env"]))
        self.assertFalse(base.part_reaches(
            self.part(False, ".claude/settings.json", keys=("env",)), self.here, None))

    def test_a_path_that_cannot_be_read_is_not_found(self):
        """`Path.exists` swallows ENOENT and ENOTDIR and does NOT swallow EACCES, so a
        `.claude/` under an ancestor this process cannot traverse raises here. `doctor`
        runs from the SessionStart hook: a row must render something."""
        with mock.patch.object(Path, "exists", side_effect=PermissionError("nope")):
            self.assertFalse(base.part_reaches(
                self.part(False, ".claude/agents"), self.here, None))

    def test_an_unreadable_document_is_not_a_declaration(self):
        p = self.here / ".claude" / "settings.json"
        p.parent.mkdir(parents=True)
        p.write_text("{ not json")
        self.assertFalse(base.part_reaches(
            self.part(False, ".claude/settings.json", keys=("env",)), self.here, None))

    def test_a_keyless_part_asks_only_whether_the_path_is_there(self):
        """Directories carry no keys, and `.claude/agents/` is a directory."""
        (self.here / ".claude" / "agents").mkdir(parents=True)
        self.assertTrue(base.part_reaches(
            self.part(False, ".claude/agents"), self.here, None))


class WhereTheWalkStopsIsAskedOfGit(LayerCase):
    """`_git_root_of` on its own. Everything above it — which directories the walk covers,
    whether the trust condition fires — is decided by what this function answers, so its
    refusals are stated here rather than inferred through a row."""

    def _proc(self, returncode: int, stdout: str):
        from types import SimpleNamespace

        return SimpleNamespace(returncode=returncode, stdout=stdout, stderr="")

    def test_a_directory_in_no_repository_has_no_git_root(self):
        """git exits non-zero outside a repository. Read as an answer instead, `Path("")`
        resolves to the CURRENT directory — so every loose directory would report itself
        as its own git root and the trust condition would fire everywhere."""
        with mock.patch.object(doctor, "_git_in", return_value=self._proc(128, "")):
            self.assertIsNone(doctor._git_root_of(self.plane))

    def test_an_empty_answer_is_not_a_root_either(self):
        """The other half of the same refusal, and not reachable through the first: a
        zero exit with nothing on stdout is what a git built without the subcommand, or
        one whose output was swallowed, leaves behind."""
        with mock.patch.object(doctor, "_git_in", return_value=self._proc(0, "  \n")):
            self.assertIsNone(doctor._git_root_of(self.plane))

    def test_a_real_repository_answers_with_itself(self):
        self.assertEqual(doctor._git_root_of(self.plane), self.plane)

    def test_a_clone_answers_with_the_clone_and_not_the_plane(self):
        here = self.clone()
        self.assertEqual(doctor._git_root_of(here), here)

    def test_a_git_that_hangs_or_is_missing_does_not_take_the_row_with_it(self):
        """`check_plane_root`'s two, for its reasons: a stalled network mount raises
        `ProcTimeout`, and a missing git raises OSError out of the exec. This runs from
        the SessionStart hook."""
        from charter import util

        for boom in (util.ProcTimeout(["git"], 5.0), OSError("no git")):
            with self.subTest(boom=type(boom).__name__):
                with mock.patch.object(doctor, "_git_in", side_effect=boom):
                    self.assertIsNone(doctor._git_root_of(self.plane))


class SettingsAreReadOnceAndDefensively(LayerCase):
    """`_settings_docs` is the single reader every structured question about the session's
    settings goes through, so what it refuses decides what several rows can claim."""

    def test_a_settings_file_that_is_not_an_object_yields_no_plugin_ids(self):
        """A JSON array parses fine and has no `.get`. Without the type filter this is an
        `AttributeError` out of a preflight check — the row does not warn, it crashes."""
        p = self.workspace / ".claude" / "settings.json"
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(json.dumps([{"enabledPlugins": {"charter@charter": True}}]))
        self.rooted_at(self.workspace)
        self.assertEqual(doctor._enabled_plugin_ids(), set())

    def test_an_unreadable_settings_file_is_skipped_not_raised(self):
        p = self.workspace / ".claude" / "settings.json"
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text("{ not json")
        self.rooted_at(self.workspace)
        self.assertEqual(doctor._settings_docs(), [])

    def test_the_documents_come_back_in_the_order_the_host_reads_them(self):
        self.settings(self.workspace, {"env": {"A": "1"}})
        (self.workspace / ".claude" / "settings.local.json").write_text(
            json.dumps({"env": {"B": "2"}}))
        self.rooted_at(self.workspace)
        self.assertEqual([d["env"] for d in doctor._settings_docs()],
                         [{"A": "1"}, {"B": "2"}])


class _StubHarness(base.Harness):
    """A harness charter has never met, so the row's per-harness rendering can be driven
    without waiting for a fourth real one to be registered."""

    def __init__(self, name: str, gate: str) -> None:
        self.name = name
        self.trust_gate = gate
        self.layer = (base.LayerPart("settings", (".nowhere",), False, "why"),)


class TheTrustClauseIsAssembledDeterministically(LayerCase):
    """Two harnesses can gate on two different things, and the sentence that joins them
    must read the same on every run. A `set` has no order a reader or a test can rely on
    — string hashing is randomised per process — so the row's own text would move between
    runs, which is the defect this repo already pins column widths and roster order
    against."""

    def _detail_for(self, stubs) -> str:
        with mock.patch.object(registry, "all", return_value=stubs):
            os.environ.pop("CHARTER_HARNESS", None)
            self.rooted_at(self.clone())
            return self.detail()

    def test_two_gates_are_named_in_registration_order(self):
        detail = self._detail_for([_StubHarness("z-harness", "zeta"),
                                   _StubHarness("a-harness", "alpha")])
        self.assertIn("zeta / alpha", detail)

    def test_registration_order_and_not_alphabetical_order(self):
        """The same two, registered the other way round. Sorting would render both cases
        identically and this pair is what tells them apart."""
        detail = self._detail_for([_StubHarness("a-harness", "alpha"),
                                   _StubHarness("z-harness", "zeta")])
        self.assertIn("alpha / zeta", detail)

    def test_two_harnesses_gating_on_the_same_thing_say_it_once(self):
        detail = self._detail_for([_StubHarness("one", "hooks"),
                                   _StubHarness("two", "hooks")])
        self.assertIn("until that is given, hooks do not run", detail)

    def test_one_gate_is_named_alone(self):
        self.rooted_at(self.clone())
        self.assertIn("hooks or the status line do not run here", self.detail())


class TheRowIsInTheReport(LayerCase):
    def test_the_name_is_declared_before_the_run(self):
        """`_FIXED_CHECK_NAMES` sizes the column without running a check, and is pinned by
        equality against what `run_all` produces."""
        self.assertIn("session layer", doctor.check_names())

    def test_it_sits_beside_the_row_that_says_which_directory_answered(self):
        """`session root` names the directory; this one says what that directory can
        reach. Read in the other order the second row is trivia."""
        names = doctor.check_names()
        self.assertEqual(names[names.index("session root") + 1], "session layer")


class TheTwoRowsNarrateOneSetOfRules(LayerCase):
    """#879: `session root` and `session layer` are printed one under the other and told an
    operator opposite things about the same directory.

    The older row said the host reads *"project settings, agents, skills and commands from
    the session's own directory and does not walk up"*. The newer one — added in the same
    release, from the measurement — said `.claude/agents` and `.claude/skills` walk up and
    stop at the git boundary. Both were describing Claude Code 2.1.259, and an operator in a
    workspace clone was told on one line that their agents were out of reach and on the next
    that they were not.

    The words were fixable; the shape was the defect. Two rows restating one discovery rule
    from two independent sources drift by default, and nothing renders them together, so
    nobody sees it. These cases hold both rows to `Harness.layer` — the source
    `check_session_layer` already reads — and to each other.
    """

    def root(self) -> str:
        self.rooted_at(self.workspace)
        return doctor.check_session_root().detail

    def parts(self) -> tuple[list[str], list[str]]:
        """What the harness declares, split by rule. Hand-spelling these would be a second
        copy of the thing under test; `TheHarnessesDeclareWhatWasMeasured` is where the
        declaration itself is pinned against the measurement."""
        layer = claude_code.ClaudeCodeHarness().layer
        return ([p.what for p in layer if not p.walks],
                [p.what for p in layer if p.walks])

    def test_no_walking_part_is_named_in_the_clause_that_says_the_host_does_not_walk_up(self):
        """The defect itself, in the one direction it did harm. `agents` and `skills` stood
        inside the sentence ending "and does not walk up"."""
        cwd_only, walking = self.parts()
        clause = next(ln for ln in self.root().splitlines() if "does not walk up" in ln)
        for what in walking:
            self.assertNotIn(
                what, clause,
                f"`{what}` walks up per the harness and this row says it does not: "
                f"{clause.strip()}")
        for what in cwd_only:
            self.assertIn(what, clause,
                          f"`{what}` is cwd-only per the harness and the row that explains "
                          f"why the plane's .claude/ is not in force does not name it")

    def test_the_walking_parts_are_named_as_walking_and_handed_to_the_row_below(self):
        """Naming only the cwd-only half would leave "the plane's .claude/ is not in force"
        reading as "none of it reaches", which is the same wrong answer with the false
        sentence removed rather than corrected. The row says what walks and then declines to
        say whether the walk arrives — that is a fact about this directory's git root, and
        `session layer` is the row that resolves it."""
        detail = self.root()
        for what in self.parts()[1]:
            self.assertIn(what, detail)
        self.assertIn("DO walk up", detail)
        self.assertIn("git root", detail)
        self.assertIn("session layer", detail)

    def test_the_two_rows_name_the_same_parts(self):
        """Whatever a harness declares, both rows account for all of it. A part added to
        `Harness.layer` and rendered by only one of them is how they came apart."""
        self.rooted_at(self.workspace)
        root, layer = doctor.check_session_root().detail, self.detail()
        for what in sum(self.parts(), []):
            with self.subTest(what=what):
                self.assertIn(what, root)
                self.assertIn(what, layer)

    def test_the_row_restates_no_discovery_rule_of_its_own(self):
        """The structural half, and the reason the words above stay fixed. No artefact is
        named in this row's code — the names come from the harness — so there is no second
        copy of the rules here to drift from the measurement again.

        Comments are stripped, deliberately, per `_code`: a comment naming `agents` is
        exactly what a decision like this should carry, while a literal is the thing being
        refused.
        """
        code = _code(doctor.check_session_root) + _code(doctor._discovery_rules)
        for artefact in ("agents", "skills", "commands", "CLAUDE.md", "settings.json"):
            self.assertNotIn(
                artefact, code,
                f"`{artefact}` is spelled into the session root row rather than read from "
                f"the harness, which is how the two rows disagreed")

    def test_a_harness_charter_has_not_met_borrows_no_rules_from_one_it_has(self):
        """`check_session_layer` refuses to report an unregistered runtime under Claude
        Code's rules. Reporting it one row up would be the same borrowed answer, in the row
        an operator reads first."""
        with mock.patch.dict(os.environ, {"CHARTER_HARNESS": "somethingelse"}):
            detail = self.root()
        self.assertNotIn("walk up", detail)
        for what in sum(self.parts(), []):
            self.assertNotIn(what, detail)
        self.assertIn("not the plane", detail)
        self.assertIn("identity", detail,
                      "the row dropped the half that is true of every harness")


if __name__ == "__main__":
    import unittest

    unittest.main()
