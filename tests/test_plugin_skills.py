"""The plugin ships skills, and every command they name is real.

Charter's knowledge used to reach agents only two ways: a SessionStart injection that
costs every session whether or not it is needed, and `--help`, which answers "what are the
flags" rather than "what must I not get wrong". Anything in between got written down in
each control plane instead — and a plane's copy drifts, silently, because nothing compares
it to the CLI.

The concrete case: a plane carried a `setup` skill telling engineers to authenticate over
SSH and add an SSH key, months after charter's rule became token-only-over-HTTPS and its
own guard began *denying* exactly that. The skill still looked wired. Nothing read it
against the thing it described.

So the check that matters here is not that the files exist — it is that every `charter …`
command a skill puts in front of an agent still resolves against the parser in this same
commit. A skill that names a removed or renamed command is the same failure again, and it
would ship to every user of the plugin rather than one repo.
"""
from __future__ import annotations

import re
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SKILLS = ROOT / "skills"

#: Commands appear in `backticks` or fenced blocks. Prose does not: "a charter alone loses
#: to a general-purpose agent" is a sentence about charters, not an invocation, and
#: scanning raw prose would read `alone` as a subcommand.
_CODE_SPAN = re.compile(r"`([^`\n]+)`")
_CODE_FENCE = re.compile(r"```[a-z]*\n(.*?)```", re.S)
_INVOCATION = re.compile(r"\bcharter\s+([a-z][a-z-]*)")


def _skill_files() -> list[Path]:
    return sorted(SKILLS.glob("*/SKILL.md"))


def _frontmatter(text: str) -> dict[str, str]:
    lines = text.splitlines()
    if not lines or lines[0].strip() != "---":
        return {}
    out = {}
    for ln in lines[1:]:
        if ln.strip() == "---":
            break
        if ":" in ln and not ln.startswith((" ", "\t")):
            k, v = ln.split(":", 1)
            out[k.strip()] = v.strip().strip("\"'")
    return out


def _top_level_commands() -> set[str]:
    """Every subcommand the CLI actually accepts, read from the parser rather than a
    hand-kept list — a list would be one more thing to drift."""
    import argparse

    from charter import cli

    found: set[str] = set()
    for action in cli.build_parser()._actions:
        if isinstance(action, argparse._SubParsersAction):
            found.update(action.choices)
    return found


class TestSkillsShip(unittest.TestCase):
    def test_the_plugin_has_a_skills_directory(self):
        self.assertTrue(SKILLS.is_dir(), "the plugin ships no skills/ directory")
        self.assertTrue(_skill_files(), "skills/ contains no SKILL.md")

    def test_each_skill_declares_a_name_matching_its_directory(self):
        """Claude Code addresses a plugin skill as `<plugin>:<name>`. A name that
        disagrees with its directory produces a handle nobody can guess, and the
        charter lint that validates `charter:<skill>` references would reject it."""
        for path in _skill_files():
            fm = _frontmatter(path.read_text())
            self.assertEqual(fm.get("name"), path.parent.name, str(path))

    def test_each_skill_describes_when_to_use_it(self):
        """The description is the only thing an agent sees before deciding to load a
        skill. One that describes the topic rather than the trigger never gets picked."""
        for path in _skill_files():
            fm = _frontmatter(path.read_text())
            desc = fm.get("description", "")
            self.assertGreater(len(desc), 40, str(path))
            self.assertIn("use when", desc.lower(), str(path))

    def test_every_skill_is_model_invokable(self):
        """These exist to be loaded by an agent mid-task. `disable-model-invocation`
        would make them human-only slash commands, which a sub-agent cannot reach."""
        for path in _skill_files():
            fm = _frontmatter(path.read_text())
            self.assertNotEqual(fm.get("disable-model-invocation", "").lower(), "true",
                                str(path))


class TestSkillsNameRealCommands(unittest.TestCase):
    def test_every_charter_command_a_skill_names_exists(self):
        """The `setup`-skill failure, generalised: a skill that instructs an agent to run
        a command the CLI does not have is wrong in a way only its reader discovers."""
        valid = _top_level_commands()
        self.assertIn("workspace", valid, "sanity: the parser was read correctly")

        for path in _skill_files():
            text = path.read_text()
            snippets = _CODE_SPAN.findall(text) + _CODE_FENCE.findall(text)
            for snippet in snippets:
                for cmd in _INVOCATION.findall(snippet):
                    self.assertIn(cmd, valid,
                                  f"{path.parent.name} names `charter {cmd}`, "
                                  f"which the CLI does not accept")

    def test_a_skill_names_at_least_one_command(self):
        """Guards the check above from passing vacuously if the extraction regexes are
        ever narrowed — a green suite with nothing scanned is the worst outcome here."""
        valid = _top_level_commands()
        seen = set()
        for path in _skill_files():
            text = path.read_text()
            for snippet in _CODE_SPAN.findall(text) + _CODE_FENCE.findall(text):
                seen.update(_INVOCATION.findall(snippet))
        self.assertTrue(seen & valid, "no charter invocations were scanned at all")


if __name__ == "__main__":
    unittest.main()
