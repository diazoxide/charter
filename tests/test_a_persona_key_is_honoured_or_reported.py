"""A persona frontmatter key is honoured or reported — never silently resolved (#575, #509).

Two issues, one property, and one of them is a permission grant.

**#575 — the key was not recognised.** `persona.parse` keeps a key exactly as written, so
``Vault:`` puts ``Vault`` in the dict, ``meta.get("vault")`` finds nothing, and the persona
declares a vault and has none. Every such key failed toward LESS — except one. ``borrows:``
is answered by ABSENCE: `borrows_of` returns None for an absent key on purpose, and None
means "keep the legacy `uses:` grant", which is the wider one. So an author writing
``Borrows: none`` to opt OUT of #257's grant got #257's grant, both borrowed personas' tools
auto-approved at `toolgate.decide`, with `structural_errors` empty.

**#509 — the key was recognised twice.** Two lines carrying one key collapse to the last,
because that is what building a dict does. ``vault: safe`` above ``vault: prod`` hands out
`prod` by LINE ORDER, and the first value is gone before any consumer sees the dict.

Both end in a value chosen by accident with no diagnostic, and the fix is one property.

**Refused, not folded — #573's call, kept.** Case-folding the LOOKUP would answer ``Vault:``
and leave ``vualt:``, ``borrow:`` and ``delegate_when:`` exactly as silent. The closed
vocabulary is what catches every one of them; `persona.misspelled_key` only sharpens the
SENTENCE and the SEVERITY for the sub-case where charter can name the word the author was
reaching for — and that is what lets the grant act on it. `test_folding_is_not_the_catch`
pins the difference.

**The sibling was wider than the named bug.** Three of the fields `_render_agent` emits are
enforced by being PRESENT, so misspelling one deletes the enforcement rather than narrowing
it: ``Agent-tools:`` emits no ``tools:`` line and no ``tools:`` line means the sub-agent
inherits EVERY tool; ``Disallowed-tools:`` drops the denylist; ``Draft:`` ships the
unfinished charter `sync-agents` refuses to ship. All three were reached through a run that
printed `✓ Synced 1 persona sub-agent(s)`.
"""

from __future__ import annotations

import io
import unittest
from contextlib import redirect_stderr, redirect_stdout
from types import SimpleNamespace

from charter import commands_persona as cp
from charter import config, persona
from tests._isolation import PersonaIso


def _front(**keys) -> str:
    """A persona file with these keys in this order — duplicates included, which is the
    whole point: `make_persona` takes **kwargs and a dict cannot hold one key twice."""
    return "".join(f"{k}: {v}\n" for k, v in keys.items())


class KeyCase(PersonaIso):
    def write(self, name: str, frontmatter: str, body: str = "charter body") -> str:
        d = config.PERSONAS_DIR / name
        d.mkdir(parents=True, exist_ok=True)
        (d / "persona.md").write_text(f"---\nname: {name}\n{frontmatter}---\n\n{body}\n")
        return name

    def errors(self, name: str) -> list[str]:
        return [m for lvl, m in persona.structural_errors(name) if lvl == "error"]

    def warns(self, name: str) -> list[str]:
        return [m for lvl, m in persona.lint(name, deep=False) if lvl == "warn"]


# --------------------------------------------------------------------------- #
# #575 — the fail-open. The priority: `borrows_of` must never fall through to  #
# a wider grant than the author wrote.                                         #
# --------------------------------------------------------------------------- #
class BorrowsFailsOpen(KeyCase):
    def grant_world(self, front_key: str = "Borrows", front_val: str = "none") -> None:
        self.write("forge", "role: f\nvault: none\ntools: Bash(gh:*)\n")
        self.write("release", "role: r\nvault: none\ntools: Bash(git push:*)\n")
        self.write("front", f"role: fr\nvault: none\nuses: forge, release\n"
                            f"{front_key}: {front_val}\ntools: Bash(ls:*)\n")

    def test_the_reproduction_from_the_issue(self):
        """`Borrows: none` — the word that grants nothing — granted both personas' tools."""
        self.grant_world()
        self.assertEqual(persona.borrows_of("front"), [],
                         "a `borrows:` charter could not read must not answer 'absent'")
        self.assertEqual(persona.effective_tools("front"), {"Bash(ls:*)"},
                         "the author opted OUT; the borrowed tools must not be granted")

    def test_the_fallback_cannot_come_back(self):
        """The regression this exists to stop: None here IS the wide grant.

        Asserted on the sentinel and not only on the tool set, because the two are one
        line apart in `effective_tools` and a future edit could restore the fallback while
        some other narrowing kept this fixture's tools looking right.
        """
        self.grant_world()
        self.assertIsNotNone(persona.borrows_of("front"))
        self.assertNotIn("Bash(gh:*)", persona.effective_tools("front"))
        self.assertNotIn("Bash(git push:*)", persona.effective_tools("front"))

    def test_a_miscased_borrows_naming_personas_also_fails_closed(self):
        """Narrower than the author asked for, which is the direction this may be wrong in.

        `Borrows: forge` meant "forge only". Charter cannot read the key, so it grants
        nothing and says which line to fix — one edit. Reading it as the legacy grant costs
        `release`'s tools as well, which is what the file was written to stop.
        """
        self.grant_world(front_val="forge")
        self.assertEqual(persona.borrows_of("front"), [])
        self.assertEqual(persona.effective_tools("front"), {"Bash(ls:*)"})

    def test_borrows_declared_twice_fails_closed(self):
        """#509 reaching the grant: `borrows: none` above `borrows: forge` granted forge."""
        self.write("forge", "role: f\nvault: none\ntools: Bash(gh:*)\n")
        self.write("front", "role: fr\nvault: none\nuses: forge\ntools: Bash(ls:*)\n"
                            + _front(borrows="none") + "borrows: forge\n")
        self.assertEqual(persona.borrows_of("front"), [])
        self.assertEqual(persona.effective_tools("front"), {"Bash(ls:*)"})

    def test_an_inherited_miscased_borrows_does_not_widen_the_child(self):
        """A report belongs to the file being edited; a GRANT belongs to the persona the
        tool gate is deciding about. A parent charter could not read must not hand its
        children the wide grant either."""
        self.write("forge", "role: f\nvault: none\ntools: Bash(gh:*)\n")
        self.write("parent", "role: p\nvault: none\nuses: forge\nBorrows: none\n")
        self.write("kid", "role: k\nvault: none\nextends: parent\ntools: Bash(ls:*)\n")
        self.assertEqual(persona.borrows_of("kid"), [])
        self.assertNotIn("Bash(gh:*)", persona.effective_tools("kid"))

    def test_an_inherited_duplicate_borrows_does_not_widen_the_child(self):
        """The half `resolve` cannot see: a dict merged from an ancestor carries that
        ancestor's LAST line and no trace that there were two."""
        self.write("forge", "role: f\nvault: none\ntools: Bash(gh:*)\n")
        self.write("parent", "role: p\nvault: none\nuses: forge\n"
                             "borrows: none\nborrows: forge\n")
        self.write("kid", "role: k\nvault: none\nextends: parent\ntools: Bash(ls:*)\n")
        self.assertEqual(persona.borrows_of("kid"), [])
        self.assertNotIn("Bash(gh:*)", persona.effective_tools("kid"))

    def test_an_absent_borrows_still_means_the_legacy_grant(self):
        """The back-compat #257 was built on, and the thing narrowing must not break:
        opting one persona in must never alter a persona that declared nothing."""
        self.write("forge", "role: f\nvault: none\ntools: Bash(gh:*)\n")
        self.write("plain", "role: p\nvault: none\nuses: forge\ntools: Bash(ls:*)\n")
        self.assertIsNone(persona.borrows_of("plain"))
        self.assertEqual(persona.effective_tools("plain"), {"Bash(ls:*)", "Bash(gh:*)"})

    def test_a_correctly_spelled_borrows_none_still_borrows_nothing(self):
        self.write("forge", "role: f\nvault: none\ntools: Bash(gh:*)\n")
        self.write("opted", "role: o\nvault: none\nuses: forge\nborrows: none\n"
                            "tools: Bash(ls:*)\n")
        self.assertEqual(persona.borrows_of("opted"), [])
        self.assertEqual(persona.effective_tools("opted"), {"Bash(ls:*)"})

    def test_a_correctly_spelled_borrows_still_grants_what_it_names(self):
        self.write("forge", "role: f\nvault: none\ntools: Bash(gh:*)\n")
        self.write("release", "role: r\nvault: none\ntools: Bash(git push:*)\n")
        self.write("opted", "role: o\nvault: none\nuses: forge, release\nborrows: forge\n"
                            "tools: Bash(ls:*)\n")
        self.assertEqual(persona.borrows_of("opted"), ["forge"])
        self.assertEqual(persona.effective_tools("opted"), {"Bash(ls:*)", "Bash(gh:*)"})


# --------------------------------------------------------------------------- #
# #575 — the report                                                            #
# --------------------------------------------------------------------------- #
class AMiscasedKeyIsReported(KeyCase):
    def test_structural_errors_names_the_key_and_the_spelling(self):
        """The signal on screen every turn, not a command the operator may never run."""
        self.write("demo", "role: A demo\nVault: devops\nExtends: base\n")
        msgs = self.errors("demo")
        self.assertTrue(any("'Vault'" in m and "`vault:`" in m for m in msgs), msgs)
        self.assertTrue(any("'Extends'" in m and "`extends:`" in m for m in msgs), msgs)

    def test_it_is_an_error_not_a_warning(self):
        """A warning is what this had, and it read `it does nothing (typo?)` — the wrong
        sentence for `Borrows:`, whose absence does a great deal."""
        self.write("demo", "role: A demo\nVault: devops\n")
        self.assertEqual([lvl for lvl, m in persona.lint("demo", deep=False)
                          if "'Vault'" in m], ["error"])

    def test_the_milder_sentence_is_not_also_printed(self):
        """One line, one finding. A reader who acts on the milder of two sentences about
        one line acts on the wrong one."""
        self.write("demo", "role: A demo\nBorrows: none\n")
        about = [m for _l, m in persona.lint("demo", deep=False) if "'Borrows'" in m]
        self.assertEqual(len(about), 1, about)
        self.assertNotIn("does nothing", about[0])

    def test_folding_is_not_the_catch(self):
        """#573's argument, pinned. `vualt:` is caught by the vocabulary being CLOSED —
        the same mechanism, unchanged. What the fold adds is one sentence, for the one
        sub-case where charter can name the word the author meant."""
        self.write("typo", "role: t\nvault: none\nvualt: devops\n")
        reported = [m for _l, m in persona.lint("typo", deep=False) if "vualt" in m]
        self.assertEqual(len(reported), 1, "a key charter does not read is still caught")
        self.assertIsNone(persona.misspelled_key("vualt"))

    def test_an_unknown_key_stays_a_warning(self):
        """The blast-radius decision, stated as a test. Charter has no claim about
        `modell:` — a harness's own field is a legitimate thing to carry in a committed
        file — so this fires as a warning and nothing narrows."""
        self.write("hk", "role: h\nvault: none\nmodell: opus\nuses: hk2\n")
        self.write("hk2", "role: h2\nvault: none\ntools: Bash(ls:*)\n")
        levels = [lvl for lvl, m in persona.lint("hk", deep=False) if "modell" in m]
        self.assertEqual(levels, ["warn"])
        self.assertEqual(self.errors("hk"), [])
        self.assertEqual(persona.effective_tools("hk"), {"Bash(ls:*)"},
                         "an unknown key must not touch the grant")

    def test_the_value_is_never_read_out_of_the_miscased_key(self):
        """Not case-folding: charter refuses, it does not guess."""
        self.write("demo", "role: A demo\nVault: devops\n")
        self.assertIsNone(persona.vault_of("demo"))

    def test_misspelled_key_answers_only_for_a_case_variant(self):
        self.assertIsNone(persona.misspelled_key("vault"), "a key charter reads is fine")
        self.assertEqual(persona.misspelled_key("VAULT"), "vault")
        self.assertEqual(persona.misspelled_key("Delegate-When"), "delegate-when")
        self.assertIsNone(persona.misspelled_key("delegate_when"), "underscore is not case")
        self.assertIsNone(persona.misspelled_key("borrow"), "a shorter word is not case")

    def test_a_key_that_renders_as_nothing_is_named_by_its_escape(self):
        """#498's finding on this row. U+3164 HANGUL FILLER is `Lo`, survives `strip`, and
        prints as nothing — so an unescaped row said `frontmatter key '' …`, telling
        somebody to go and edit a key it does not name."""
        self.write("blank", "role: b\nvault: none\nㅤvault: devops\n")
        msgs = [m for _l, m in persona.lint("blank", deep=False) if "frontmatter key" in m]
        self.assertTrue(msgs, "the key is reported at all")
        self.assertFalse(any("key ''" in m for m in msgs), msgs)
        self.assertTrue(any("3164" in m for m in msgs), msgs)


# --------------------------------------------------------------------------- #
# #509 — the key recognised twice                                             #
# --------------------------------------------------------------------------- #
class AKeyWrittenTwice(KeyCase):
    def test_duplicate_keys_reads_the_text(self):
        text = ("---\nversion: 0.60.0\nheadline: important\n"
                "security: true\nsecurity: false\n---\nbody\n")
        self.assertEqual(persona.duplicate_keys(text), ["security"])

    def test_parse_still_returns_the_dict_and_the_body(self):
        """#509 read the fix as 'either change what `parse` returns or give it a second
        output', both landing on every caller in the tree. Neither happened."""
        meta, body = persona.parse("---\na: 1\nb: 2\nb: 3\n---\nbody\n")
        self.assertEqual(meta, {"a": "1", "b": "3"})
        self.assertEqual(body, "body")

    def test_load_carries_the_answer_the_dict_cannot(self):
        self.write("dup", "role: d\nvault: alpha\nvault: beta\n")
        d = persona.load("dup")
        self.assertEqual(d["dupes"], ["vault"])
        self.assertEqual(d["meta"]["vault"], "beta", "the dict is unchanged; the report is new")

    def test_it_is_reported_by_name(self):
        self.write("dup", "role: d\nvault: alpha\nvault: beta\n"
                          "tools: Bash(ls:*)\ntools: Bash(rm:*)\n")
        msgs = self.errors("dup")
        self.assertTrue(any("'vault'" in m and "more than once" in m for m in msgs), msgs)
        self.assertTrue(any("'tools'" in m and "more than once" in m for m in msgs), msgs)

    def test_the_report_says_which_line_charter_obeyed(self):
        """Half the value of the sentence: the author's first line is the one they meant."""
        self.write("dup", "role: d\nvault: alpha\nvault: beta\n")
        self.assertTrue(any("LAST" in m for m in self.errors("dup")), self.errors("dup"))

    def test_a_key_written_once_is_never_reported(self):
        """A false positive here would train people to scroll past the row."""
        self.write("ok", "role: o\nvault: v\nuses: other\ntools: Bash(ls:*)\n")
        self.write("other", "role: o2\nvault: none\n")
        self.assertEqual(persona.load("ok")["dupes"], [])
        self.assertEqual(self.errors("ok"), [])

    def test_whitespace_around_a_key_is_the_same_key(self):
        """#509 asks which spellings collide. `parse` strips the key, so `vault :` IS
        `vault` and always did collide — stated here rather than left to be rediscovered."""
        self.assertEqual(persona.duplicate_keys("---\nvault: a\nvault : b\n---\nx\n"),
                         ["vault"])

    def test_case_is_not_the_same_key(self):
        """The other half of that question. `Vault:` does NOT collide with `vault:` — it is
        a different key, reported as a miscased one rather than as a duplicate."""
        self.assertEqual(persona.duplicate_keys("---\nvault: a\nVault: b\n---\nx\n"), [])
        self.write("both", "role: b\nvault: a\nVault: b\n")
        msgs = self.errors("both")
        self.assertTrue(any("'Vault'" in m for m in msgs), msgs)
        self.assertFalse(any("more than once" in m for m in msgs), msgs)


# --------------------------------------------------------------------------- #
# The sibling: three fields enforced by their PRESENCE                        #
# --------------------------------------------------------------------------- #
class TheSubAgentIsNotGeneratedFromAKeyCharterCannotRead(KeyCase):
    def sync(self) -> str:
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            cp.cmd_persona_sync_agents(SimpleNamespace(persona=None, dry_run=False,
                                                       approve_mcp=False, yes=False))
        return out.getvalue() + err.getvalue()

    def agent_path(self, name: str):
        return config.ROOT / ".claude" / "agents" / f"{name}.md"

    def test_a_miscased_agent_tools_does_not_ship_an_agent_with_every_tool(self):
        """The widest of the three. No `tools:` line in a generated agent does not mean a
        narrow agent — it means the sub-agent inherits EVERY tool, so a misspelling here
        deletes the allowlist rather than narrowing it."""
        self.write("gated", "role: g\nvault: none\nAgent-tools: Read, Grep\n")
        self.assertEqual(cp._write_agent("gated"), "unreadable")
        self.assertFalse(self.agent_path("gated").exists())

    def test_a_miscased_disallowed_tools_does_not_ship_an_agent_with_no_denylist(self):
        self.write("gated", "role: g\nvault: none\nDisallowed-tools: Bash\n")
        self.assertEqual(cp._write_agent("gated"), "unreadable")

    def test_a_miscased_draft_does_not_ship_the_unfinished_charter(self):
        """`sync-agents` refuses to make a draft into a system prompt. `Draft: true` made
        `is_draft` False and shipped it anyway."""
        self.write("wip", "role: w\nvault: none\nDraft: true\n")
        self.assertFalse(persona.is_draft("wip"), "the key is still read by nothing")
        self.assertEqual(cp._write_agent("wip"), "unreadable")
        self.assertFalse(self.agent_path("wip").exists())

    def test_a_duplicated_agent_tools_does_not_ship_the_lower_line(self):
        self.write("dup", "role: d\nvault: none\n"
                          "agent-tools: Read\nagent-tools: Read, Bash\n")
        self.assertEqual(cp._write_agent("dup"), "unreadable")

    def test_a_stale_agent_is_removed_rather_than_left_dispatchable(self):
        """The same call the draft branch makes. Leaving the old file keeps the persona
        dispatchable under whatever it said BEFORE — which is the grant the author was
        editing the file to remove."""
        self.write("gated", "role: g\nvault: none\nagent-tools: Read\n")
        self.assertEqual(cp._write_agent("gated"), "written")
        self.assertTrue(self.agent_path("gated").exists())
        self.write("gated", "role: g\nvault: none\nAgent-tools: Read\n")
        self.assertEqual(cp._write_agent("gated"), "unreadable")
        self.assertFalse(self.agent_path("gated").exists())

    def test_the_run_that_would_have_written_it_says_so(self):
        """Not left to `lint`. A green tick over a dropped allowlist is the shape #453
        keeps arriving in."""
        self.write("gated", "role: g\nvault: none\nAgent-tools: Read\n")
        out = self.sync()
        self.assertIn("gated", out)
        self.assertIn("'Agent-tools'", out, "the report names the key to go and edit")
        self.assertIn("agent-tools:", out, "and the spelling that would have worked")
        self.assertNotIn("Synced 1 persona", out, "no green tick over a dropped allowlist")

    def test_an_unknown_key_still_generates_an_agent(self):
        """The narrow gate, pinned from the other side: `modell:` is not `key_issues`, so
        a plane carrying a harness field keeps its sub-agent."""
        self.write("hk", "role: h\nvault: none\nmodell: opus\n")
        self.assertEqual(cp._write_agent("hk"), "written")
        self.assertTrue(self.agent_path("hk").exists())


# --------------------------------------------------------------------------- #
# the vocabulary                                                              #
# --------------------------------------------------------------------------- #
class TheVocabulary(unittest.TestCase):
    def test_every_key_a_real_charter_uses_is_in_it(self):
        """A false positive here is now an ERROR and a withheld sub-agent, so this matters
        more than it did when the same list only produced a warning."""
        for key in ("name", "role", "vault", "extends", "uses", "delegate-when", "tools",
                    "agent-tools", "model", "color", "memory", "activity", "borrows",
                    "draft", "skills", "disallowed-tools", "routing", "routes-to",
                    "description", "agent-description", "dispatch-isolation"):
            self.assertIn(key, persona.KNOWN_KEYS, f"real charters use {key!r}")

    def test_the_two_sets_do_not_overlap(self):
        self.assertEqual(set(persona.AGENT_PASSTHROUGH_KEYS)
                         & set(persona.CHARTER_OWN_KEYS), set())

    def test_every_key_charter_actually_reads_is_declared(self):
        """The failure this cannot have: a key charter READS that the vocabulary omits.

        That key is spelled correctly in a correct file, `misspelled_key` answers None for
        it — so it lands as an unknown-key warning on a persona that is right. Read off the
        source rather than a hand-kept list, so a `meta.get("newfield")` added tomorrow
        fails here rather than on somebody's roster.
        """
        import re
        from pathlib import Path
        src = Path(persona.__file__).parent
        found = set()
        for mod in ("persona.py", "commands_persona.py"):
            text = (src / mod).read_text()
            found |= set(re.findall(r'meta(?:\[|\.get\()"([a-z][a-z-]*)"', text))
        missing = found - set(persona.KNOWN_KEYS)
        self.assertEqual(missing, set(), f"read by charter but not in KNOWN_KEYS: {missing}")

    def test_commands_persona_still_exports_the_old_names(self):
        """Moved to the lower layer for `structural_errors`, which says in its own
        docstring that it cannot afford to import the command module."""
        self.assertEqual(cp._AGENT_PASSTHROUGH_KEYS, persona.AGENT_PASSTHROUGH_KEYS)
        self.assertEqual(cp._CHARTER_OWN_KEYS, persona.CHARTER_OWN_KEYS)

    def test_the_status_line_path_imports_no_command_module(self):
        """The cost that kept this check out of the status line's signal in the first
        place: the vocabulary lived in `commands_persona`, and `structural_errors` runs on
        every turn. Asserted on the STATEMENTS rather than the text — the docstring names
        the module it is careful not to import.
        """
        import ast
        import inspect
        import textwrap
        for fn in (persona.structural_errors, persona.key_issues, persona.misspelled_key):
            tree = ast.parse(textwrap.dedent(inspect.getsource(fn)))
            names = [a.name for n in ast.walk(tree) if isinstance(n, ast.Import)
                     for a in n.names]
            names += [n.module or "" for n in ast.walk(tree)
                      if isinstance(n, ast.ImportFrom)]
            self.assertEqual([n for n in names if "commands" in n], [],
                             f"{fn.__name__} imports a command module")


if __name__ == "__main__":
    unittest.main()
