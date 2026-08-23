"""opencode calls an MCP tool `<server>_<tool>`, so charter writes THAT name (#374).

`charter guard ask 'mcp__slack__send'` wrote the right rule for Claude Code (#365) and a
wrong one for opencode. `OpenCodeHarness.ask_rule` walks `TOOL_NAMES` looking for a
`Tool(pattern)` prefix and falls through to `return "bash", p` for everything else, so an
MCP pattern landed as `{"permission": {"bash": {"mcp__slack__send": "ask"}}}` — a rule over
a *bash command* literally named `mcp__slack__send`. The operator was told the guard was in
force and it could never fire, under any opencode configuration. That is #365's failure one
harness over, and it survived #365 because the fix was made in `commands._as_rule`, which
opencode does not go through.

`Harness.apply_ask_rule` documents the other honest answer — a harness that cannot express
a pattern returns `unsupported` and says why, because "the difference between naming that
limit and staying quiet is the difference between a limit and a lie". **That answer is not
available here, because opencode can express this.** Verified against opencode 1.18.21, the
binary on this machine, not inferred from the shape of the key:

* `permission` takes keys beyond the five it documents. `Permission.fromConfig` turns
  ``{"<key>": {"*": "ask"}}`` into ``{permission: "<key>", pattern: "*", action: "ask"}``,
  which `opencode debug agent build` prints back in the resolved rule list.
* Every MCP tool is registered under ``McpCatalog.toolName(server, tool)``, which is
  ``sanitize(server) + "_" + sanitize(tool)`` for ``sanitize = s =>
  s.replace(/[^a-zA-Z0-9_-]/g, "_")``, and the wrapper around it asks under exactly that
  id: ``ask({permission: <tool id>, patterns: ["*"]})``.
* `Permission.evaluate` glob-matches the permission NAME as well as the pattern
  (`Wildcard.match`) — which is how opencode's own ``{permission: "*"}`` default works — so
  a whole-server rule is ``<server>_*``.

The translation is therefore a rename, not an invented syntax: `mcp__slack__send` becomes
`slack_send` and `mcp__slack` becomes `slack_*`. `commands._MCP_RULE_RE` already confines
the pattern to ``[A-Za-z0-9_-]``, which is exactly the set opencode's `sanitize` leaves
alone, so no character in a rule charter accepts ever needs rewriting — and reusing that
regex rather than a second one keeps the two harnesses from disagreeing about what an MCP
rule even is.

What charter still cannot check is that the operator named the server the same way in
opencode's own `mcp` block. That is the same contract Claude Code's rule has — the name
comes from the operator, not from a guess — and the difference from the defect is total: a
mistyped server makes the rule inert for that server, while `bash` made it inert for every
possible one.

**And the rename can land on a name whose SHAPE is not the shape charter writes.** Five of
opencode's permissions take a bare action string and reject the ``{glob: decision}``
object — one of them, `doom_loop`, is reachable by the rename (`mcp__doom__loop`) and one,
`webfetch`, by `TOOL_NAMES`. Writing the object form there does not make one rule inert; it
makes the file invalid and opencode refuses to run in the project. That regression arrived
with this fix, in this file's own fixture: `mcp__doom__loop` acquired a new meaning the day
the rename shipped, and the two tests using it asserted only on the warning's wording, so
nothing was left watching the file. `TestTheFileOpencodeWillActuallyLoad` watches it now.
"""

from __future__ import annotations

import io
import json
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from types import SimpleNamespace

from charter import commands, config
from charter.harness import opencode as oc
from charter.harness import registry
from charter.harness.base import Harness
from tests._isolation import PersonaIso

#: The words the collision caveat is asserted to be ABSENT from, in one place so the
#: negative assertion and its anti-vacuity guard cannot drift apart.
_CAVEAT_PHRASE = "LAST matching rule"

#: opencode's `permission` schema, transcribed here a SECOND time on purpose.
#:
#: `charter.harness.opencode.FLAT_ONLY_PERMISSIONS` is the same fact, and asserting
#: against it would make every test below circular: a mutation dropping `doom_loop` from
#: the production tuple would drop it from the expectation in the same breath and stay
#: green. So this copy comes from the other source — `$defs.PermissionConfig` in the
#: published `https://opencode.ai/config.json`, which types exactly these five named keys
#: `PermissionActionConfig` (``enum: ask|allow|deny``) and the rest `PermissionRuleConfig`
#: (``anyOf`` a bare action or a ``{pattern: action}`` object). The binary agrees: each of
#: opencode 1.18.21's 23 permission names was fed to `opencode debug agent build` in both
#: shapes, and only these five answer ``Expected PermissionActionConfig | undefined``.
_SCHEMA_FLAT_ONLY = ("doom_loop", "question", "todowrite", "webfetch", "websearch")

_SCHEMA_ACTIONS = ("ask", "allow", "deny")


def _schema_complaint(doc) -> str:
    """What opencode would refuse about *doc*, or ``""`` — the check the tests below make.

    Deliberately not a general JSON-Schema validator: charter carries no dependencies, and
    a validator vendored into the suite would be a third transcription to keep right. This
    is only the one rule that was broken — the shape a permission's VALUE may take —
    which is the rule `permission.doom_loop` violated while every test in this file
    stayed green.
    """
    perms = doc.get("permission", {})
    if not isinstance(perms, dict):
        return "`permission` is not an object"
    for name, value in perms.items():
        if isinstance(value, str):
            if value not in _SCHEMA_ACTIONS:
                return f"permission.{name} is {value!r}, not one of {_SCHEMA_ACTIONS}"
            continue
        if name in _SCHEMA_FLAT_ONLY:
            return (f"Expected PermissionActionConfig | undefined, got "
                    f"{json.dumps(value, separators=(',', ':'))} permission.{name}")
        if not isinstance(value, dict):
            return f"permission.{name} is neither an action nor a rule object"
        for pat, action in value.items():
            if action not in _SCHEMA_ACTIONS:
                return f"permission.{name}[{pat!r}] is {action!r}, not an action"
    return ""


class TestTheMcpNameOpencodeActuallyUses(unittest.TestCase):
    """The translation itself: charter's `mcp__` name → opencode's tool id."""

    def setUp(self) -> None:
        self.h = registry.get("opencode")

    def test_one_mcp_tool_becomes_that_tools_opencode_id(self):
        """The reported case. `bash` here matches a command nothing can run."""
        self.assertEqual(self.h.ask_rule("mcp__slack__send"), ("slack_send", "*"))

    def test_a_whole_server_becomes_a_glob_over_its_tools(self):
        """opencode glob-matches the permission name, so one key covers the server."""
        self.assertEqual(self.h.ask_rule("mcp__slack"), ("slack_*", "*"))

    def test_a_server_name_carrying_an_underscore_survives(self):
        """Only the FIRST `__` separates server from tool; `_` is legal in both halves and
        is a character opencode's own `sanitize` leaves alone."""
        self.assertEqual(self.h.ask_rule("mcp__my_server__do_thing"),
                         ("my_server_do_thing", "*"))

    def test_a_dashed_server_name_survives(self):
        """`-` is in opencode's keep-set too, so charter must not rewrite it."""
        self.assertEqual(self.h.ask_rule("mcp__my-server__do"), ("my-server_do", "*"))

    def test_allow_translates_the_same_way(self):
        """`allow_rule` defaults to `ask_rule` — the decision differs, the name must not."""
        self.assertEqual(self.h.allow_rule("mcp__slack__send"), ("slack_send", "*"))


class TestWhatIsStillABashCommand(unittest.TestCase):
    """The mirror-image failure a prefix test would introduce, which #365 already paid for
    once: a binary whose name merely begins with `mcp` is a command, not an MCP rule."""

    def test_a_bare_command_is_untouched(self):
        self.assertEqual(registry.get("opencode").ask_rule("git push *"),
                         ("bash", "git push *"))

    def test_a_binary_called_mcp_something_is_a_command(self):
        self.assertEqual(registry.get("opencode").ask_rule("mcp-inspector --list"),
                         ("bash", "mcp-inspector --list"))

    def test_a_tool_rule_still_comes_apart_into_tool_and_glob(self):
        self.assertEqual(registry.get("opencode").ask_rule("Read(./secrets/**)"),
                         ("read", "./secrets/**"))

    def test_a_wildcarded_mcp_pattern_is_not_quietly_given_an_opencode_name(self):
        """`commands` refuses `mcp__slack__send *` before any harness is asked (#369), and
        this is what keeps that refusal meaningful if the order ever changes: the test is on
        the SHAPE `_MCP_RULE_RE` accepts, not on a `mcp__` prefix. A prefix test would coin
        the opencode tool id `slack_send *` — a name with a space in it, which no tool can
        have — which is the same silent failure wearing the other harness's clothes."""
        self.assertEqual(registry.get("opencode").ask_rule("mcp__slack__send *"),
                         ("bash", "mcp__slack__send *"))


class TestWhatLandsInOpencodeJson(PersonaIso):
    """The file, because the translation is only worth anything once it is written."""

    def config_path(self) -> Path:
        return Path(config.ROOT) / "opencode.json"

    def test_the_rule_is_keyed_by_the_mcp_tool_not_by_bash(self):
        status, _ = registry.get("opencode").apply_ask_rule(Path(config.ROOT),
                                                            "mcp__slack__send")
        self.assertEqual(status, "added")
        perms = json.loads(self.config_path().read_text())["permission"]
        self.assertEqual(perms["slack_send"], {"*": "ask"})
        self.assertNotIn("bash", perms)

    def test_allow_writes_the_same_key_with_the_other_verb(self):
        registry.get("opencode").apply_allow_rule(Path(config.ROOT), "mcp__slack__send")
        perms = json.loads(self.config_path().read_text())["permission"]
        self.assertEqual(perms["slack_send"], {"*": "allow"})

    def test_writing_it_twice_is_not_an_edit(self):
        """Idempotence on the TRANSLATED key — it held for the wrong key too, so the
        assertion has to name what the file ends up holding or it proves nothing."""
        h = registry.get("opencode")
        h.apply_ask_rule(Path(config.ROOT), "mcp__slack__send")
        before = self.config_path().read_text()
        self.assertIn('"slack_send"', before)
        self.assertEqual(h.apply_ask_rule(Path(config.ROOT), "mcp__slack__send")[0],
                         "present")
        self.assertEqual(self.config_path().read_text(), before)


class TestTheFileOpencodeWillActuallyLoad(PersonaIso):
    """The subject is the JSON on disk, not the sentence printed beside it.

    #374's fix gave `mcp__doom__loop` a new meaning — it stopped being an inert `bash`
    key and became the literal opencode permission name `doom_loop` — and two tests went
    on using it as a fixture while asserting only on the warning's prose. Both stayed
    green over a rule that does not merely fail to fire: `{"doom_loop": {"*": "ask"}}` is
    the object form of a name opencode types as a bare action, so opencode rejects the
    WHOLE file and will not run in the project.

        $ opencode run 'hi'
        Error: Configuration is invalid at .../opencode.json
          ↳ Expected PermissionActionConfig | undefined, got {"*":"ask"} permission.doom_loop

    That is #374's own defect one turn further on — a tick over a rule that cannot fire,
    now over a config that cannot load — and it is charter's and new: at the branch point
    the same command wrote `{"bash": {"mcp__doom__loop": "ask"}}`, inert but loadable.

    So these tests read the file back and check its SHAPE against `_schema_complaint`,
    which is opencode's rule transcribed from the published schema rather than from
    charter's own table. A test that had done this would have caught it on the day.
    """

    def config_path(self) -> Path:
        return Path(config.ROOT) / "opencode.json"

    def written(self) -> dict:
        return json.loads(self.config_path().read_text())

    def test_the_checker_these_tests_lean_on_rejects_what_opencode_rejects(self):
        """Anti-vacuity, first, because everything below is worthless if this returns ""
        for everything. The argument is the exact document charter used to write."""
        broken = {"permission": {"doom_loop": {"*": "ask"}}}
        self.assertIn("doom_loop", _schema_complaint(broken))
        self.assertEqual(_schema_complaint({"permission": {"doom_loop": "ask"}}), "")
        self.assertEqual(_schema_complaint({"permission": {"bash": {"*": "ask"}}}), "")

    def test_a_tool_whose_id_is_a_flat_only_permission_is_written_flat(self):
        status, _ = registry.get("opencode").apply_ask_rule(Path(config.ROOT),
                                                            "mcp__doom__loop")
        self.assertEqual(status, "added")
        self.assertEqual(self.written()["permission"], {"doom_loop": "ask"})
        self.assertEqual(_schema_complaint(self.written()), "")

    def test_the_flat_form_is_the_same_rule_and_not_a_weaker_one(self):
        """Nothing is traded away by dropping the object. Measured on opencode 1.18.21:
        `{"doom_loop": "ask"}` resolves to ``{permission: doom_loop, pattern: '*',
        action: 'ask'}`` — the same entry, in the same last-wins position, that the
        object form was reaching for. Which is why this key is written rather than
        refused, while a key carrying a real pattern is refused rather than flattened."""
        h = registry.get("opencode")
        self.assertEqual(h.ask_rule("mcp__doom__loop"), ("doom_loop", "*"))
        h.apply_ask_rule(Path(config.ROOT), "mcp__doom__loop")
        self.assertEqual(self.written()["permission"]["doom_loop"], "ask")

    def test_allow_writes_the_flat_form_too(self):
        registry.get("opencode").apply_allow_rule(Path(config.ROOT), "mcp__doom__loop")
        self.assertEqual(self.written()["permission"], {"doom_loop": "allow"})
        self.assertEqual(_schema_complaint(self.written()), "")

    def test_writing_the_flat_form_twice_is_not_an_edit(self):
        h = registry.get("opencode")
        h.apply_ask_rule(Path(config.ROOT), "mcp__doom__loop")
        before = self.config_path().read_text()
        self.assertEqual(h.apply_ask_rule(Path(config.ROOT), "mcp__doom__loop")[0],
                         "present")
        self.assertEqual(self.config_path().read_text(), before)

    def test_a_flat_only_key_someone_shaped_by_hand_is_refused_not_repaired(self):
        """The same restraint the object branch keeps. An operator who wrote an object
        under `doom_loop` has a broken file, but it is theirs and charter does not know
        what they meant — reporting beats overwriting."""
        self.config_path().write_text(
            json.dumps({"permission": {"doom_loop": {"*": "deny"}}}))
        before = self.config_path().read_text()
        status, detail = registry.get("opencode").apply_ask_rule(Path(config.ROOT),
                                                                 "mcp__doom__loop")
        self.assertEqual(status, "malformed")
        self.assertIn("doom_loop", detail)
        self.assertEqual(self.config_path().read_text(), before)

    def test_a_glob_over_a_flat_only_name_still_takes_the_object_form(self):
        """`doom_*` is not `doom_loop`. It reaches opencode through the schema's
        `additionalProperties`, which is `PermissionRuleConfig` — the object form is legal
        there, and flattening it would be a different rule. Confirmed against the binary:
        `{"doom_*": {"*": "ask"}}` loads."""
        registry.get("opencode").apply_ask_rule(Path(config.ROOT), "mcp__doom")
        self.assertEqual(self.written()["permission"], {"doom_*": {"*": "ask"}})
        self.assertEqual(_schema_complaint(self.written()), "")

    def test_a_flat_only_key_with_a_real_pattern_is_refused_rather_than_widened(self):
        """`WebFetch(https://example.com/*)` is keyed `webfetch` through `TOOL_NAMES`, and
        `webfetch` is flat-only too. Writing the flat form here would silently trade the
        one URL the operator named for every fetch there is — an `allow` widened past
        what was asked, which is the failure `--local`'s refusal already exists to
        prevent. `Harness.apply_ask_rule` keeps `unsupported` for exactly this."""
        status, detail = registry.get("opencode").apply_ask_rule(
            Path(config.ROOT), "WebFetch(https://example.com/*)")
        self.assertEqual(status, "unsupported")
        self.assertIn("webfetch", detail)
        self.assertIn("https://example.com/*", detail)
        self.assertFalse(self.config_path().exists(),
                         "a refused rule must not leave a file behind")

    def test_the_same_key_with_no_pattern_at_all_is_written_rather_than_refused(self):
        """`WebFetch(*)` asks for every fetch, which is what the flat form says. Refusing
        it would name a limit opencode does not have."""
        status, _ = registry.get("opencode").apply_ask_rule(Path(config.ROOT),
                                                            "WebFetch(*)")
        self.assertEqual(status, "added")
        self.assertEqual(self.written()["permission"], {"webfetch": "ask"})

    def test_every_rule_this_harness_writes_is_a_file_opencode_can_load(self):
        """The catch-all the two prose tests were standing in for. One fresh plane per
        pattern, so an earlier key cannot carry a later one."""
        h = registry.get("opencode")
        for pattern in ("mcp__slack__send", "mcp__slack", "mcp__doom", "mcp__doom__loop",
                        "mcp__plan", "mcp__list", "mcp__read_mcp", "git push *",
                        "Read(./secrets/**)", "WebFetch(*)", "Task(*)"):
            with self.subTest(pattern=pattern):
                root = Path(config.ROOT) / "planes" / pattern.replace("*", "_star")
                root.mkdir(parents=True, exist_ok=True)
                status, detail = h.apply_ask_rule(root, pattern)
                self.assertIn(status, ("added", "unsupported"), detail)
                if status == "added":
                    doc = json.loads((root / "opencode.json").read_text())
                    self.assertEqual(_schema_complaint(doc), "", f"{pattern} → {doc}")


class TestThroughTheCommandBothHarnessesGetIt(PersonaIso):
    """One operator sentence, and neither harness is left holding a rule that cannot fire."""

    def invoke(self, fn, **kw) -> tuple[int, str]:
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            rc = fn(SimpleNamespace(**kw))
        return rc, out.getvalue() + err.getvalue()

    def test_guard_ask_writes_the_mcp_tool_id_for_opencode(self):
        rc, _ = self.invoke(commands.cmd_guard_ask, pattern="mcp__slack__send",
                            local=False)
        self.assertEqual(rc, 0)
        perms = json.loads((Path(config.ROOT) / "opencode.json").read_text())["permission"]
        self.assertEqual(perms["slack_send"], {"*": "ask"})
        self.assertNotIn("bash", perms)

    def test_and_claude_code_still_gets_its_own_verbatim_rule(self):
        """The half that already worked (#365) must not move to fix the half that did not."""
        self.invoke(commands.cmd_guard_ask, pattern="mcp__slack__send", local=False)
        rules = json.loads(
            (Path(config.ROOT) / ".claude" / "settings.json").read_text()
        )["permissions"]["ask"]
        self.assertIn("mcp__slack__send", rules)

    def test_the_operator_is_shown_the_name_that_was_written(self):
        """`guard` says what it wrote (0.49.0). A translated name is exactly the case where
        the operator needs to read it back and check their server is called that."""
        _rc, out = self.invoke(commands.cmd_guard_ask, pattern="mcp__slack__send",
                               local=False)
        self.assertIn("slack_send", out)

    def test_the_whole_command_leaves_a_file_opencode_can_load(self):
        """End to end, over the fixture that broke. The tick and the collision warning
        were both already right about `doom_loop`; the file underneath them was not."""
        rc, out = self.invoke(commands.cmd_guard_ask, pattern="mcp__doom__loop",
                              local=False)
        self.assertEqual(rc, 0)
        self.assertIn("doom_loop", out)
        doc = json.loads((Path(config.ROOT) / "opencode.json").read_text())
        self.assertEqual(_schema_complaint(doc), "")

    def test_a_rule_opencode_cannot_shape_is_reported_not_ticked(self):
        """The `unsupported` half through the command. Claude Code takes the rule
        verbatim, opencode says why it cannot, and no `✓` is printed for a file that was
        never written — the distinction #374 is about."""
        rc, out = self.invoke(commands.cmd_guard_ask,
                              pattern="WebFetch(https://example.com/*)", local=False)
        self.assertEqual(rc, 0)
        self.assertIn("webfetch", out)
        self.assertNotIn("opencode: asking for", out)
        rules = json.loads(
            (Path(config.ROOT) / ".claude" / "settings.json").read_text()
        )["permissions"]["ask"]
        self.assertIn("WebFetch(https://example.com/*)", rules)


class TestWhenTheTranslatedNameIsOpencodesOwn(unittest.TestCase):
    """The expensive half of the limit, which the first pass named only the cheap half of.

    `<server>_*` covering a sibling server called `slack_admin` costs the operator a rule
    that is wider than they asked. `<server>_*` covering opencode's OWN permission names
    costs them one of opencode's decisions: `evaluate` is
    ``findLast(r => match(name, r.permission) && match(pattern, r.pattern))`` and config
    resolves after the built-in ruleset, so charter's entry is the last match and wins.

    Verified against opencode 1.18.21 rather than reasoned: with
    ``{"permission": {"plan_*": {"*": "allow"}}}`` in `opencode.json`,
    `opencode debug agent build` lists ``plan_enter``/``plan_exit`` deny at indices 9 and
    10 and ``plan_*`` allow at 17 — so both denies are gone. Before #374 the same command
    wrote an inert `bash` rule and nothing happened, which makes this widening charter's
    own and new, and the reason a warning had to come with the translation.
    """

    def setUp(self) -> None:
        self.h = registry.get("opencode")

    def test_a_whole_server_glob_over_opencodes_own_names_is_named(self):
        said = self.h.rule_outranks("mcp__plan")
        self.assertIn("plan_enter", said)
        self.assertIn("plan_exit", said)

    def test_the_caveat_says_which_way_opencode_resolves_the_collision(self):
        """Naming the collision without naming the direction leaves the operator to
        guess whether their rule wins or opencode's does — which is the whole point."""
        self.assertIn("LAST", self.h.rule_outranks("mcp__plan"))

    def test_a_server_colliding_with_nothing_gets_no_caveat(self):
        """Silence is the common case and has to stay silent, or the line stops being read
        by the time it matters."""
        self.assertEqual(self.h.rule_outranks("mcp__slack"), "")
        self.assertEqual(self.h.rule_outranks("mcp__slack__send"), "")

    def test_a_single_tool_can_collide_exactly_and_is_named_too(self):
        """`mcp__doom__loop` translates to `doom_loop`, which IS opencode's own permission
        — no glob involved. A warning that only looked at `_*` would miss it."""
        said = self.h.rule_outranks("mcp__doom__loop")
        self.assertIn("doom_loop", said)

    def test_an_exact_collision_offers_no_narrower_form_because_there_is_none(self):
        """The remedy for `plan_*` is "name the tool". For `doom_loop` there is no
        narrower name, and offering one would send the operator to type something that
        changes nothing."""
        self.assertNotIn("Naming the tool", self.h.rule_outranks("mcp__doom__loop"))
        self.assertIn("Naming the tool", self.h.rule_outranks("mcp__plan"))

    def test_the_narrower_form_offered_is_one_charter_would_accept(self):
        """`_MCP_RULE_RE` admits a trailing separator, so `mcp__plan__` translates to the
        same `plan_*` as `mcp__plan`. Echoing the operator's pattern back into the remedy
        would advise `mcp__plan____<tool>` — four separators, which charter itself refuses
        — so the remedy is rebuilt from the name that was written."""
        said = self.h.rule_outranks("mcp__plan__")
        self.assertIn("`mcp__plan__<tool>`", said)
        self.assertNotIn("___", said)

    def test_the_narrower_form_keeps_a_server_name_that_has_an_underscore(self):
        """Rebuilding from the written name must not re-split it. A server called
        `read_mcp` translates to `read_mcp_*`, which collides with opencode's
        `read_mcp_resource` — and the remedy is `mcp__read_mcp__<tool>`, not
        `mcp__read__<tool>`, which names a different server entirely."""
        said = self.h.rule_outranks("mcp__read_mcp")
        self.assertIn("read_mcp_resource", said)
        self.assertIn("`mcp__read_mcp__<tool>`", said)

    def test_an_ordinary_command_rule_is_not_warned_about(self):
        """`git push *` is keyed `bash`, and `bash` is one of opencode's own permission
        names — ON PURPOSE. Warning here would fire on nearly every invocation and train
        the operator to skip the line that matters."""
        self.assertEqual(self.h.rule_outranks("git push *"), "")
        self.assertEqual(self.h.rule_outranks("Read(./secrets/**)"), "")

    def test_a_harness_with_no_collision_to_report_says_nothing(self):
        """The base default is a claim, not a stub: Claude Code namespaces MCP rules under
        `mcp__`, so nothing charter writes there can land on a built-in tool name."""
        self.assertEqual(Harness().rule_outranks("mcp__plan"), "")
        cc = registry.get(registry.CLAUDE_CODE)
        self.assertEqual(cc.rule_outranks("mcp__plan"), "")


class TestTheCollisionMatcherIsOpencodesOwn(unittest.TestCase):
    """`Wildcard.match` anchors ``^…$``. A substring test would be wrong in the
    reassuring direction for one half and the alarming direction for the other."""

    def test_a_glob_matches_only_names_that_start_with_the_server(self):
        self.assertEqual(oc._shadowed_builtins("plan_*"), ("plan_enter", "plan_exit"))

    def test_a_name_that_is_merely_a_substring_of_a_builtin_does_not_match(self):
        """Unanchored at the front, `lan_*` would "match" `plan_enter` and warn about a
        collision that cannot happen."""
        self.assertEqual(oc._shadowed_builtins("lan_*"), ())

    def test_a_prefix_of_a_builtin_with_no_glob_does_not_match(self):
        """Unanchored at the back, `plan_ent` would "match" `plan_enter` — and a rule
        keyed `plan_ent` matches nothing at all in opencode."""
        self.assertEqual(oc._shadowed_builtins("plan_ent"), ())

    def test_an_ordinary_server_name_matches_nothing(self):
        self.assertEqual(oc._shadowed_builtins("slack_*"), ())
        self.assertEqual(oc._shadowed_builtins("slack_send"), ())


class TestTheOperatorHearsAboutItAtWriteTime(PersonaIso):
    """At write time, because that is the last moment they can change their mind — the
    same argument `_warn_if_shadowing` already makes for the persona tool-gate."""

    def invoke(self, fn, **kw) -> tuple[int, str]:
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            rc = fn(SimpleNamespace(**kw))
        return rc, out.getvalue() + err.getvalue()

    def test_guard_allow_warns_that_it_just_relaxed_two_of_opencodes_denies(self):
        """The reported shape: `allow` over a whole server whose name is opencode's."""
        rc, out = self.invoke(commands.cmd_guard_allow, pattern="mcp__plan", local=False)
        self.assertEqual(rc, 0)
        self.assertIn("plan_enter", out)
        self.assertIn("plan_exit", out)

    def test_guard_ask_warns_too_because_deny_to_ask_is_also_a_widening(self):
        """opencode's default for `plan_enter` is `deny`. An `ask` rule keyed `plan_*`
        outranks it, so a thing that could not happen becomes a thing one click away."""
        _rc, out = self.invoke(commands.cmd_guard_ask, pattern="mcp__plan", local=False)
        self.assertIn("plan_enter", out)

    def test_an_ordinary_rule_gets_no_such_line(self):
        _rc, out = self.invoke(commands.cmd_guard_ask, pattern="mcp__slack__send",
                               local=False)
        self.assertNotIn(_CAVEAT_PHRASE, out)

    def test_the_phrase_asserted_absent_above_is_one_the_caveat_really_says(self):
        """Anti-vacuity for the test above it, which is otherwise reworded into silence.

        `assertNotIn("LAST matching rule", …)` passes for two different reasons — the
        caveat did not fire, or the caveat no longer uses those words — and only the
        first is the subject. Rewording the sentence to "the LAST rule that matches"
        leaves the whole guard suite green today. Pinning the same literal against a
        pattern that DOES collide makes a rewording redden here instead of quietly
        emptying the negative assertion.
        """
        self.assertIn(_CAVEAT_PHRASE, registry.get("opencode").rule_outranks("mcp__plan"))

    def test_nothing_is_warned_about_where_nothing_was_written(self):
        """`--local` is `unsupported` under opencode — its only uncommitted config is
        machine-wide — so no opencode rule exists to outrank anything. Describing the
        consequence of a rule that was refused is the same species of lie as #374."""
        _rc, out = self.invoke(commands.cmd_guard_ask, pattern="mcp__plan", local=True)
        self.assertNotIn("plan_enter", out)


class TestTheCaveatClaimsOnlyTheLastWord(unittest.TestCase):
    """It says charter's rule decides those names. Not that it replaced a rule opencode
    wrote for each of them, which is only sometimes true.

    Measured on opencode 1.18.21 with an empty `opencode.json`: the seeded ruleset carries
    name-specific entries for `*`, `doom_loop`, `external_directory`, `question`,
    `plan_enter`, `plan_exit` and `read` and nothing else, so `plan_*` really does outrank
    two denies while `list_*` only outranks the index-0 ``{permission: "*"}`` allow. The
    difference is not charter's to spell out — part of that seed is machine-specific
    (`external_directory` carries this machine's own paths) — so the sentence claims the
    half that is true either way.
    """

    def setUp(self) -> None:
        self.h = registry.get("opencode")

    def test_it_does_not_claim_to_have_replaced_a_rule_opencode_wrote(self):
        said = self.h.rule_outranks("mcp__list")
        self.assertIn("list_mcp_resources", said)
        self.assertNotIn("replacing", said)

    def test_it_still_says_charters_rule_is_the_one_that_decides(self):
        self.assertIn("decides those too", self.h.rule_outranks("mcp__list"))

    def test_the_pronoun_follows_the_number_of_names_it_just_listed(self):
        """One `permission`/`that`/`it`, several `permissions`/`those`/`them`. The
        plural branch used to end on a singular "opencode's built-in one"."""
        one = self.h.rule_outranks("mcp__doom__loop")
        many = self.h.rule_outranks("mcp__plan")
        self.assertIn("decides that too, ahead of whatever opencode had decided for it",
                      one)
        self.assertIn("decides those too, ahead of whatever opencode had decided for "
                      "them", many)


class TestBothVerbsTranslateTheSameWay(unittest.TestCase):
    """`rule_outranks` takes a pattern and no verb, and reads the name back through
    `ask_rule` for the allow path as well as the ask path.

    Correct only while opencode keeps `base.allow_rule`'s shared default — which that
    default exists to guarantee ("keeps one operator sentence from acquiring two
    spellings"). If opencode ever overrides `allow_rule`, `guard allow` would start
    warning about the collisions of a name it did not write, and nothing else in the
    suite would notice. This fails on that day and names the caller.
    """

    def test_opencode_answers_both_verbs_with_the_same_name(self):
        h = registry.get("opencode")
        for p in ("mcp__plan", "mcp__doom__loop", "mcp__slack__send", "git push *",
                  "WebFetch(*)"):
            with self.subTest(pattern=p):
                self.assertEqual(h.allow_rule(p), h.ask_rule(p),
                                 "OpenCodeHarness.rule_outranks reads the rule name back "
                                 "through ask_rule for BOTH verbs — override allow_rule "
                                 "and it must be taught the difference")


if __name__ == "__main__":
    unittest.main()
