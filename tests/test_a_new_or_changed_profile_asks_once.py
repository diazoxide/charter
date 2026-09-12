"""A profile whose command is new or has changed shows it and asks once, before it runs.

Charter records each profile's `kind`, `command` and `env` as last launched, under
`.charter/`. A profile with no record, or one that no longer matches, is shown to the
operator and asked `run this? [y/N]` before anything starts. Built-ins never ask: their
command comes out of charter's own registry rather than out of a file (spec, *What guards
a launch*, item 2).

**Why there is an ask at all.** Once `charter.local.toml` is ignored an edit to it leaves
no diff; nothing stops a chat editing plane config; and the command goes to tmux rather
than through a harness's own permission prompt. Codex trusts hooks by hash for the same
reason.

**And where there is nobody to ask, there is a refusal instead.** A reopen, a restore, a
handoff and any open with no terminal on both stdin and stdout refuse where they would have
asked (review 3, ruling 23) — a question nobody can see is a pane that hangs, which is the
one outcome worse than a refusal.

This module is also where the two lines Task 2's sweep could not measure are pinned, for
the reason the sweep gives: each of those mutations hangs a real-tmux case instead of
failing it, and a run that timed out is neither green nor red. Both are a second of test
time here (:class:`ThePinsARealTmuxCaseCouldOnlyHangOn`).
"""

from __future__ import annotations

import io
import json
import os
import shutil
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from charter import commands_frame, config, profiles, profiletrust
from charter.frame import launcher, reopen as reopen_state, state
from tests import _gitguard
from tests._isolation import (APipe as _APipe, ATerminal as _ATerminal, PersonaIso,
                              Typed as _Typed, approve_profile, assert_approved,
                              declare_profiles, make_plane, wired_as_today)
from tests.test_a_profile_launch_is_refused_before_tmux import _ALaunchNamesAProfile


#: Ruling 10: a profile whose config folder does not carry charter's guard refuses to
#: launch, and every launch here is an approved or built-in profile the suite's `claude`
#: guard would otherwise read as unwired. This module is about the ASK; whether the ask's
#: yes still meets the wiring refusal behind it is
#: `tests/test_a_profile_is_wired_or_refuses.py`'s subject, where nothing stands in for it.
_WIRED = None


def setUpModule():
    global _WIRED
    _WIRED = wired_as_today()
    _WIRED.start()


def tearDownModule():
    if _WIRED is not None:
        _WIRED.stop()


class _ADeclaringPlane(PersonaIso):
    """A plane declaring `claude-work`, in a git repository that ignores the local file."""

    def setUp(self) -> None:
        super().setUp()
        make_plane(self)
        self.enterContext(mock.patch.dict(
            os.environ,
            {"PATH": os.environ.get("PATH", ""),
             "CHARTER_ROOT": str(config.ROOT), **_gitguard.environment()}, clear=True))
        # AFTER the clearing patch, so its `$HOME` and `$GIT_CEILING_DIRECTORIES` survive:
        # the ignore check is a real `git status` and both decide what it answers.
        self.local = declare_profiles(self)

    def _profile(self, name: str = "claude-work") -> profiles.Profile:
        return profiles.current().profiles[name]


class TheRecord(_ADeclaringPlane, unittest.TestCase):
    """What charter writes down when a profile runs, and what it makes of it next time."""

    def test_a_built_in_never_needs_approval(self):
        """The spec says so, and the reason is what the record is FOR: a built-in's
        command comes out of charter's own registry, which no chat can edit. Asking about
        it would train an operator to answer yes to a question that never carries risk."""
        self.assertEqual(profiletrust.approval_needed(self._profile("codex")), "")

    def test_a_declared_profile_with_no_record_is_new(self):
        self.assertEqual(profiletrust.approval_needed(self._profile()), profiletrust.NEW)

    def test_a_recorded_profile_is_approved(self):
        profiletrust.record_launched(self._profile())
        self.assertEqual(profiletrust.approval_needed(self._profile()), "")

    def test_a_changed_command_is_changed(self):
        profiletrust.record_launched(self._profile())
        self.local.write_text('[harness.claude-work]\nkind = "claude"\n'
                              'command = ["/opt/claude-2.1/bin/claude"]\n')
        self.assertEqual(profiletrust.approval_needed(self._profile()),
                         profiletrust.CHANGED)

    def test_a_changed_env_value_is_changed(self):
        """The `env` is half of what a profile IS — it is what moves the account — so a
        record that compared only the command would approve a profile pointed at somebody
        else's config folder."""
        profiletrust.record_launched(self._profile())
        self.local.write_text('[harness.claude-work]\nkind = "claude"\n'
                              'command = ["claude"]\n'
                              'env = { CLAUDE_CONFIG_DIR = "~/.someone-else" }\n')
        self.assertEqual(profiletrust.approval_needed(self._profile()),
                         profiletrust.CHANGED)

    def test_a_changed_kind_is_changed(self):
        profiletrust.record_launched(self._profile())
        self.local.write_text('[harness.claude-work]\nkind = "codex"\n'
                              'command = ["claude"]\n'
                              'env = { CLAUDE_CONFIG_DIR = "~/.cw" }\n')
        self.assertEqual(profiletrust.approval_needed(self._profile()),
                         profiletrust.CHANGED)

    def test_a_declared_replacement_of_a_built_in_asks(self):
        """`[harness.claude]` is a declaration, whatever it is called: the name is the
        built-in's and the command is the file's, and the command is what this is about."""
        self.local.write_text('[harness.claude]\nkind = "claude"\n'
                              'command = ["/opt/claude"]\n')
        self.assertEqual(profiletrust.approval_needed(self._profile("claude")),
                         profiletrust.NEW)

    def test_the_record_is_private_state_under_charter(self):
        """The name and the shape are spelled literally: a round trip through charter's own
        writer and reader cannot pin a key that both halves would rename together."""
        profiletrust.record_launched(self._profile())
        path = config.STATE_DIR / "harness-profiles-launched.json"
        self.assertEqual(path.stat().st_mode & 0o077, 0)
        self.assertEqual(json.loads(path.read_text()),
                         {"claude-work": {"kind": "claude", "command": ["claude"],
                                          "env": {"CLAUDE_CONFIG_DIR": "~/.cw"}}})

    def test_the_record_is_what_the_file_says_and_not_what_the_exec_gets(self):
        """As DECLARED, before `~` is expanded. The file is what an edit changes, and
        `$HOME` is not something a chat moves — a record of the expansion would read as
        *changed* for every profile the day somebody's home directory moved."""
        profiletrust.record_launched(self._profile())
        raw = json.loads((config.STATE_DIR / "harness-profiles-launched.json").read_text())
        self.assertEqual(raw["claude-work"]["env"]["CLAUDE_CONFIG_DIR"], "~/.cw")
        self.assertNotIn(str(self.home), json.dumps(raw))

    def test_recording_one_profile_leaves_the_others_recorded(self):
        """One object keyed by name, so approving `codex-pinned` today does not make
        `claude-work` ask again tomorrow."""
        profiletrust.record_launched(self._profile())
        profiletrust.record_launched(self._profile("codex-pinned"))
        self.assertEqual(profiletrust.approval_needed(self._profile()), "")
        self.assertEqual(profiletrust.approval_needed(self._profile("codex-pinned")), "")

    def test_the_first_record_makes_the_state_directory_it_lives_in(self):
        """A plane that has never written any state has no `.charter/` yet, and the first
        profile somebody approves is allowed to be the thing that creates it — at 0700,
        through `config.private_mkdir`, because the umask must not decide the mode of
        charter's own state directory (#437)."""
        shutil.rmtree(config.STATE_DIR, ignore_errors=True)
        self.assertEqual(profiletrust.record_launched(self._profile()), "")
        self.assertEqual(profiletrust.approval_needed(self._profile()), "")
        self.assertEqual(config.STATE_DIR.stat().st_mode & 0o077, 0)

    def test_an_unreadable_record_asks_again(self):
        """It fails towards ASKING, never towards running: a file charter cannot read says
        nothing about what the operator approved, and treating silence as a yes is exactly
        the state this record exists to keep out."""
        config.private_mkdir(config.STATE_DIR)
        (config.STATE_DIR / "harness-profiles-launched.json").write_text("{nope")
        self.assertEqual(profiletrust.approval_needed(self._profile()), profiletrust.NEW)

    def test_a_record_of_the_wrong_shape_asks_again(self):
        """Same rule one level down: the file is a plain JSON object a chat can write, so
        an entry that is a string rather than a fingerprint is not an approval either."""
        config.private_mkdir(config.STATE_DIR)
        (config.STATE_DIR / "harness-profiles-launched.json").write_text(
            '{"claude-work": "approved, honest"}')
        self.assertEqual(profiletrust.approval_needed(self._profile()), profiletrust.NEW)

    def test_a_record_that_is_not_an_object_at_all_asks_again(self):
        """And one level up from that: the whole file is JSON a chat can write, so a list
        where an object belongs reads as no records rather than as an `AttributeError` in
        the middle of a launch."""
        config.private_mkdir(config.STATE_DIR)
        (config.STATE_DIR / profiletrust.RECORD).write_text("[]")
        self.assertEqual(profiletrust.approval_needed(self._profile()), profiletrust.NEW)

    def test_a_record_that_cannot_be_written_says_why_and_never_raises(self):
        """`record_launched` is called from a launch that is about to `exec`, and from a
        hook-reachable press: it hands back the reason instead of raising it."""
        # Both shapes an `OSError` arrives in, because the sentence has to name something
        # either way: the filesystem's own `strerror`, and one raised with a message and no
        # errno at all — where an empty answer would read to `ask_in_terminal` as a write
        # that worked, and the launch would go ahead on a record that is not there.
        for raised, expected in ((OSError(28, "No space left on device"),
                                  "No space left on device"),
                                 (OSError("the disk is read-only"),
                                  "the disk is read-only")):
            with self.subTest(raised=raised):
                with mock.patch.object(profiletrust.config, "replace_for",
                                       side_effect=raised):
                    why = profiletrust.record_launched(self._profile())
                self.assertIn(expected, why)
                self.assertEqual(profiletrust.approval_needed(self._profile()),
                                 profiletrust.NEW)


class TheAsk(_ADeclaringPlane, unittest.TestCase):
    """The prompt itself: what it shows, what it accepts, and what it writes down."""

    def _ask(self, answer: str = "y\n", name: str = "claude-work"):
        stdin, out = _Typed(answer), io.StringIO()
        got = profiletrust.ask_in_terminal(self._profile(name), stdin=stdin, stdout=out)
        return got, out.getvalue(), stdin

    def test_it_shows_the_command_and_the_env_before_asking(self):
        _got, said, _stdin = self._ask()
        self.assertIn("command  claude", said)
        self.assertIn("CLAUDE_CONFIG_DIR=~/.cw", said)
        self.assertTrue(said.rstrip().endswith("run this? [y/N]"), said)

    def test_it_says_which_profile_and_why_it_is_asking(self):
        _got, said, _stdin = self._ask()
        self.assertIn("claude-work", said)
        self.assertIn("has not run on this machine before", said)

    def test_yes_records_and_runs(self):
        got, _said, stdin = self._ask()
        self.assertEqual(got, profiletrust.Answer(True, ""))
        self.assertEqual(profiletrust.approval_needed(self._profile()), "")
        self.assertEqual(stdin.reads, 1, "one question, read once")

    def test_yes_spelled_out_and_in_capitals_is_still_yes(self):
        for answer in ("yes\n", "Y\n", "  y  \n"):
            with self.subTest(answer=answer):
                # Back to a plane that has never run this profile: the previous answer
                # recorded one, and a second question is never asked of an approved one.
                (config.STATE_DIR / profiletrust.RECORD).unlink(missing_ok=True)
                got, _said, _stdin = self._ask(answer)
                self.assertTrue(got.yes)

    def test_anything_else_declines_and_records_nothing(self):
        """A default of no is the whole point, so end-of-input and a fat finger both land
        on it. `""` is the closed stream: a launch driven from a script must decline rather
        than loop asking."""
        for answer in ("\n", "n\n", "no\n", "yess\n", ""):
            with self.subTest(answer=answer):
                got, _said, _stdin = self._ask(answer)
                self.assertEqual(got, profiletrust.Answer(False, ""))
                self.assertEqual(profiletrust.approval_needed(self._profile()),
                                 profiletrust.NEW)

    def test_a_profile_approved_since_the_question_was_settled_is_not_asked_about(self):
        """**The two reads are two moments, and the record can move between them** (S1).
        `refusal` reads it to decide there is a question; this reads it again to build the
        prompt — and in between a second terminal, the pane a `+` opened, or the chat this
        module's own docstring names can have written it. A profile that matches by then is
        APPROVED: it starts, with no question and no prompt.

        Both streams refuse to be touched, so this is "nothing was asked" rather than
        "something was asked quickly" — and before the fix it was neither: `HEADLINE[""]`
        raised `KeyError('')` out of a launcher, in a pane, which is the one thing this
        function's own rule forbids.
        """
        profiletrust.record_launched(self._profile())
        got = profiletrust.ask_in_terminal(self._profile(), stdin=_APipe(), stdout=_APipe())
        self.assertEqual(got, profiletrust.Answer(True, ""))

    def test_a_control_c_at_the_prompt_declines(self):
        """^C at a question means "not this", and it must not come out of here as a
        traceback in a pane the operator is looking at.

        **Caught here rather than allowed to escape**, and that is a decision about the
        SWEEP rather than about the prompt. `unittest` reads a `KeyboardInterrupt` out of a
        test as the operator interrupting the run: it stops the whole run and the process
        dies on SIGINT. So an escaping one is not a red — it is a run that ended — and
        `tools/sweep.py` could not resolve this guard at all while it was: CI answered
        `unresolved` for narrowing it, which is neither green nor red and leaves the line
        with no verdict. Caught, the same mutation is an ordinary failing assertion.
        """
        stdin = mock.Mock(readline=mock.Mock(side_effect=KeyboardInterrupt))
        try:
            got = profiletrust.ask_in_terminal(self._profile(), stdin=stdin,
                                               stdout=io.StringIO())
        except KeyboardInterrupt:
            self.fail("^C at the prompt came out as a traceback instead of a decline")
        self.assertEqual(got, profiletrust.Answer(False, ""))
        self.assertEqual(profiletrust.approval_needed(self._profile()), profiletrust.NEW)

    def test_control_bytes_in_a_command_are_shown_escaped_in_the_prompt(self):
        """Ruling 35, and the reason this prompt exists at all: `charter.local.toml` is a
        file a chat can write, so a `\\r` or an ESC in a `command` could redraw this very
        line to show a harmless command while the operator approves another one."""
        self.local.write_text('[harness.sneaky]\nkind = "claude"\n'
                              'command = ["rm\\r\\u001b[2Kclaude"]\n')
        _got, said, _stdin = self._ask(name="sneaky")
        self.assertNotIn("\r", said)
        self.assertNotIn("\x1b", said)
        # `contain.readable`'s own spelling for a byte that is not printable ASCII.
        self.assertIn("\\u000d", said)

    def test_a_name_that_never_passed_the_name_rule_is_still_escaped_in_the_prompt(self):
        """`_prompt` contains the NAME too, and every name that reaches it today passed
        `profiles.NAME_RE` — so a sweep cannot tell that containment from its absence. It is
        kept for ruling 35's reason (the prompt is the one line that must never be redrawn)
        and pinned here with a `Profile` built by hand, past the rule that would refuse it."""
        p = profiles.Profile(name="evil\x1b[2Kname", kind="claude", harness="claude-code",
                             command=("claude",), env=(), source=profiles.LOCAL_FILE)
        said = profiletrust._prompt(p, profiletrust.NEW, {})
        self.assertNotIn("\x1b", said)
        self.assertIn("evil\\u001b[2Kname", said)

    def test_a_control_byte_in_an_env_value_is_shown_escaped_too(self):
        """The `env` is on the prompt beside the command, and it comes out of the same
        file — so it is the same hazard and it gets the same containment."""
        self.local.write_text('[harness.sneaky]\nkind = "claude"\n'
                              'command = ["claude"]\n'
                              'env = { CLAUDE_CONFIG_DIR = "~/a\\u001b[2Kb" }\n')
        _got, said, _stdin = self._ask(name="sneaky")
        self.assertNotIn("\x1b", said)
        self.assertIn("\\u001b", said)

    def test_a_new_profile_has_nothing_it_was(self):
        """The `was` row belongs to a CHANGE. On a profile that has never run there is
        nothing to compare against, and a row saying so — blank, or repeating the command
        back — would make the first question look like the second one."""
        _got, said, _stdin = self._ask()
        self.assertNotIn("was ", said)

    def test_a_profile_with_no_environment_gets_no_env_row(self):
        """A blank `env` row is a question about nothing, and it is the row that decides
        WHICH ACCOUNT a chat runs as — so an empty one reads as "no account set" on a
        profile that genuinely has none, and as noise everywhere else."""
        self.local.write_text('[harness.plain]\nkind = "claude"\ncommand = ["claude"]\n')
        _got, said, _stdin = self._ask(name="plain")
        self.assertIn("command  claude", said)
        self.assertNotIn("env ", said)

    def test_the_prompt_shows_a_long_command_whole(self):
        """**A sentence clips; a prompt does not** (F1). `contain.readable` stops each word
        at 160 characters, and a command approved with its tail unseen is the one outcome
        this whole feature is against: `--settings <somewhere else>` past that clip reads as
        an ordinary `claude`. It wraps rather than hiding anything, and nothing on this
        prompt ends in the `...` that would mean charter kept some of it back."""
        tail = "z" * 400
        self.local.write_text('[harness.wordy]\nkind = "claude"\n'
                              f'command = ["claude", "--settings", "/tmp/{tail}"]\n')
        _got, said, _stdin = self._ask(name="wordy")
        self.assertIn(f"command  claude --settings /tmp/{tail}", said)
        self.assertNotIn("...", said)

    def test_a_long_environment_value_is_whole_on_the_prompt_too(self):
        """The `env` is the half that decides WHICH ACCOUNT, so a config folder clipped to
        its first 160 characters is exactly the value an operator must be able to read to
        the end before saying yes."""
        tail = "z" * 400
        self.local.write_text('[harness.wordy]\nkind = "claude"\n'
                              'command = ["claude"]\n'
                              f'env = {{ CLAUDE_CONFIG_DIR = "/tmp/{tail}" }}\n')
        _got, said, _stdin = self._ask(name="wordy")
        self.assertIn(f"env      CLAUDE_CONFIG_DIR=/tmp/{tail}", said)
        self.assertNotIn("...", said)

    def test_a_long_command_is_whole_and_still_escaped(self):
        """Whole is not raw: every byte outside printable ASCII is still an escape, at any
        length. Not clipping and not escaping are two different decisions and only one of
        them was made."""
        tail = "z" * 400
        self.local.write_text('[harness.wordy]\nkind = "claude"\n'
                              f'command = ["cl\\raude{tail}"]\n')
        _got, said, _stdin = self._ask(name="wordy")
        self.assertIn(f"command  cl\\u000daude{tail}", said)
        self.assertNotIn("\r", said)

    def test_the_profiles_name_on_the_prompt_is_whole_as_well(self):
        """Same rule for the headline: the name is what the last line tells somebody to
        type, so a clipped one is a remedy nobody can run. The one-line REFUSALS still clip
        it — `TheRefusalsWhereNobodyCanBeAsked` pins that — because a sentence that ends in
        its remedy is the surface a 200-character name really does spoil."""
        long_name = "w" * 200
        self.local.write_text(f'[harness.{long_name}]\nkind = "claude"\n'
                              'command = ["claude"]\n')
        _got, said, _stdin = self._ask(name=long_name)
        self.assertIn(long_name, said)
        self.assertNotIn("...", said)

    def test_what_it_was_reads_in_a_settled_order(self):
        """Two variables in one record come back in whatever order JSON kept them, and a
        `was` line that reordered itself between two asks would read as a change the
        operator did not make. Sorted by name, like `profiles.Profile.env` itself."""
        config.private_mkdir(config.STATE_DIR)
        (config.STATE_DIR / profiletrust.RECORD).write_text(json.dumps(
            {"claude-work": {"kind": "claude", "command": ["claude"],
                             "env": {"ZZ_LAST": "z", "AA_FIRST": "a"}}}))
        _got, said, _stdin = self._ask()
        self.assertIn("was      AA_FIRST=a ZZ_LAST=z claude", said)

    def test_a_record_missing_half_its_fingerprint_still_shows_a_was_row(self):
        """The record is a file a chat can write, so an entry with no `env` — or no
        `command` — is ordinary rather than exotic, and neither may come out of here as a
        `KeyError` in the middle of a launch. The row shows what is there, with no leading
        or trailing space where the missing half would have been: a `was` line that begins
        with one reads as a command that begins with one.

        Both halves, because they are two lookups and a test that exercised one would leave
        the other free to be written as `was["…"]`.
        """
        for entry, expected in (
                ('{"kind": "claude", "command": ["elsewhere"]}', "  was      elsewhere\n"),
                ('{"kind": "claude", "env": {"CLAUDE_CONFIG_DIR": "~/.old"}}',
                 "  was      CLAUDE_CONFIG_DIR=~/.old\n")):
            with self.subTest(entry=entry):
                config.private_mkdir(config.STATE_DIR)
                (config.STATE_DIR / profiletrust.RECORD).write_text(
                    '{"claude-work": ' + entry + '}')
                _got, said, _stdin = self._ask()
                self.assertIn(expected, said)

    def test_a_word_that_renders_as_nothing_is_still_shown_as_a_word(self):
        """Not clipping is not the same as printing nothing. After the escape the prompt
        holds only printable ASCII, so "renders as nothing" is exactly "is spaces" — and an
        argument of three spaces that reached the screen as three spaces would be a word the
        operator approved without being able to see that it was there at all."""
        self.local.write_text('[harness.blankish]\nkind = "claude"\n'
                              'command = ["claude", "   "]\n')
        _got, said, _stdin = self._ask(name="blankish")
        self.assertIn('command  claude ""', said)

    def test_an_argument_that_merely_contains_a_space_is_shown_as_it_is(self):
        """The other side of the same rule, and an ordinary profile rather than an odd one:
        `command` is an argv list, so one element holding a space is a single argument with
        a space in it — `--append-system-prompt "be terse"` — and it is exactly the argument
        an operator most needs to read before saying yes. "Renders as nothing" is *every*
        character is a space, not *any*."""
        self.local.write_text('[harness.spaced]\nkind = "claude"\ncommand = '
                              '["claude", "--append-system-prompt", "be terse"]\n')
        _got, said, _stdin = self._ask(name="spaced")
        self.assertIn("command  claude --append-system-prompt be terse", said)

    def test_a_changed_profile_shows_what_it_was(self):
        """A change is only readable against what it changed FROM: "is this the command
        you meant" is a different question from "did you mean to swap these two"."""
        profiletrust.record_launched(self._profile())
        self.local.write_text('[harness.claude-work]\nkind = "claude"\n'
                              'command = ["claude", "--elsewhere"]\n')
        _got, said, _stdin = self._ask()
        self.assertIn("has changed since it last ran", said)
        self.assertIn("was ", said)
        self.assertIn("CLAUDE_CONFIG_DIR=~/.cw", said)

    def test_what_it_was_is_shown_escaped_as_well(self):
        """The old fingerprint is read back out of `.charter/`, which is as writable by a
        chat as the local file is (ruling 13) — so it reaches this terminal by the same
        route and is contained by the same rule."""
        config.private_mkdir(config.STATE_DIR)
        (config.STATE_DIR / "harness-profiles-launched.json").write_text(json.dumps(
            {"claude-work": {"kind": "claude", "command": ["rm\r[2Kclaude"],
                             "env": {}}}))
        _got, said, _stdin = self._ask()
        self.assertNotIn("\r", said)
        self.assertNotIn("\x1b", said)
        self.assertIn("\\u000d", said)

    def test_a_yes_that_cannot_be_recorded_comes_back_as_the_write_error(self):
        """N2b's nit, and the reason this returns more than a `bool`: a yes charter could
        not write down must refuse the launch rather than run it, because the very next
        open would find no record and ask the same question again."""
        with mock.patch.object(profiletrust.config, "replace_for",
                               side_effect=OSError(28, "No space left on device")):
            got, _said, _stdin = self._ask()
        self.assertEqual(got, profiletrust.Answer(True, "No space left on device"))

    def test_the_question_is_on_the_screen_before_anything_is_read(self):
        """The prompt's last line ends WITHOUT a newline — `run this? [y/N] ` leaves the
        cursor beside it, which is what makes it look like a question — so nothing flushes
        it on its own. Unflushed, the operator is shown a blank pane and charter waits
        there for an answer to a question they cannot see."""
        flushed: list[int] = []

        class _Screen(_ATerminal):
            def flush(self) -> None:
                flushed.append(len(self.getvalue()))

        class _LooksBeforeItAnswers(_Typed):
            def readline(self) -> str:
                if not flushed:
                    raise AssertionError("asked for an answer before the question was "
                                         "flushed to the screen")
                return super().readline()

        got = profiletrust.ask_in_terminal(self._profile(), stdin=_LooksBeforeItAnswers("y\n"),
                                           stdout=_Screen())
        self.assertTrue(got.yes)

    def test_the_two_states_are_the_words_the_sentences_use(self):
        """`new` and `changed` are read straight into three sentences, so they are an
        interface rather than two flags: spelled out here, and asserted IN the sentences,
        so a rename is a red test rather than prose that stops making sense."""
        self.assertEqual((profiletrust.NEW, profiletrust.CHANGED), ("new", "changed"))
        for attended in (True, False):
            self.assertIn("is new", profiletrust.refusal(self._profile(),
                                                         attended=attended).text)
        profiletrust.record_launched(self._profile())
        self.local.write_text('[harness.claude-work]\nkind = "claude"\n'
                              'command = ["elsewhere"]\n')
        for attended in (True, False):
            self.assertIn("is changed", profiletrust.refusal(self._profile(),
                                                             attended=attended).text)

    def test_a_decline_is_the_workspace_pickers_own_cancel_code(self):
        """130, written out. Every neighbouring number already means something else to
        whoever reads `$?` — 127 and 126 are the shell's words for a command that could not
        be found or run, 2 is charter's usage error, 1 is a harness that ran and failed, 0
        is one that ran and did not — and 130 is the shell's "ended by SIGINT", which is
        what answering no to a question is. The same number `_choose_workspace` returns for
        a cancelled picker, so one `$?` means one thing across both prompts."""
        self.assertEqual(profiletrust.DECLINED_EXIT, 130)
        self.assertEqual(profiletrust.DECLINED_EXIT, commands_frame._PICKER_CANCELLED)

    def test_asking_needs_a_terminal_on_both_sides(self):
        """Ruling 23: a terminal on one side only is not one. `charter claude-work > log`
        has somebody at the keyboard and nowhere to print the question; `charter claude-work
        < /dev/null` has the screen and nothing to read from. Both are a hang if asked."""
        for stdin_tty, stdout_tty in ((True, False), (False, True), (False, False)):
            with self.subTest(stdin=stdin_tty, stdout=stdout_tty):
                self.assertFalse(profiletrust.can_ask(
                    SimpleNamespace(isatty=lambda t=stdin_tty: t),
                    SimpleNamespace(isatty=lambda t=stdout_tty: t)))
        self.assertTrue(profiletrust.can_ask(SimpleNamespace(isatty=lambda: True),
                                             SimpleNamespace(isatty=lambda: True)))


class TheRefusalsWhereNobodyCanBeAsked(_ADeclaringPlane, unittest.TestCase):
    """`profiletrust.refusal` — the seam the launcher's chain ends in."""

    def test_an_approved_profile_is_not_refused(self):
        profiletrust.record_launched(self._profile())
        self.assertIsNone(profiletrust.refusal(self._profile(), attended=True))
        self.assertIsNone(profiletrust.refusal(self._profile(), attended=False))

    def test_an_attended_open_asks_and_an_unattended_one_refuses(self):
        """Two kinds, not two sentences: the caller decides what to do by the KIND (ruling
        27), because a sentence gets reworded and a branch on one stops branching."""
        self.assertEqual(profiletrust.refusal(self._profile(), attended=True).kind,
                         launcher.KIND_ASK)
        self.assertEqual(profiletrust.refusal(self._profile(), attended=False).kind,
                         launcher.KIND_UNATTENDED)

    def test_each_refusal_names_the_command_that_would_approve_it(self):
        for attended in (True, False):
            with self.subTest(attended=attended):
                r = profiletrust.refusal(self._profile(), attended=attended)
                self.assertIn("charter claude-work", r.text)
                self.assertEqual(r.exit, launcher.REFUSED_EXIT)

    def test_a_refusal_repeats_the_profiles_name_back_bounded(self):
        """`profiles.NAME_RE` bounds a declared name's shape and not its length, so this is
        the one value in the sentence that arrives printable and as long as the file liked
        — and each of these sentences ends in the remedy."""
        long_name = "w" * 200
        self.local.write_text(f'[harness.{long_name}]\nkind = "claude"\n'
                              'command = ["claude"]\n')
        for attended in (True, False):
            with self.subTest(attended=attended):
                r = profiletrust.refusal(self._profile(long_name), attended=attended)
                self.assertIn("w" * 160 + "...", r.text)
                self.assertNotIn(long_name, r.text)

    def test_a_built_in_is_refused_nowhere(self):
        self.assertIsNone(profiletrust.refusal(self._profile("codex"), attended=False))


class WhereItAsks(_ALaunchNamesAProfile, unittest.TestCase):
    """Every open charter has, and what each of them does with a profile nobody approved.

    The table in the plan's Task 3, as cases: a terminal launch asks before tmux; a press
    with no terminal defers to the pane, which has one on both ends; and a reopen, a
    handoff or a launch with nowhere to ask refuses instead.
    """

    def test_a_terminal_launch_asks_before_anything_is_allocated(self):
        """A refusal before tmux is a `return` with nothing to tear down — no session, no
        window, no chat directory — which is why the question is asked here and not after
        the frame is built."""
        screen, said = _ATerminal(), []
        with mock.patch.object(commands_frame.util, "err", side_effect=said.append):
            rc = self._launch(profile="claude-work", stdin=_Typed("n\n"), stdout=screen)
        self.assertEqual(rc, profiletrust.DECLINED_EXIT)
        self.assertEqual(self.argvs, [], "tmux was asked for something anyway")
        self.assertFalse((config.STATE_DIR / "frame" / "beta.1").exists())
        # Said on the terminal the question went to, in `_choose_workspace`'s own words for
        # a cancel — and said NOWHERE ELSE: an operator's own "no" reported a second time
        # under charter's red ✗ reads as a rule firing on them.
        self.assertIn("charter: nothing started.", screen.getvalue())
        self.assertEqual(said, [])

    def test_the_question_is_the_profiles_own_command(self):
        screen = _ATerminal()
        self._launch(profile="claude-work", stdin=_Typed("n\n"), stdout=screen)
        self.assertIn("command  claude", screen.getvalue())
        self.assertIn("CLAUDE_CONFIG_DIR=~/.cw", screen.getvalue())
        self.assertIn("run this? [y/N]", screen.getvalue())

    def test_a_yes_at_the_terminal_launches_and_is_not_asked_again_in_the_pane(self):
        """The record is what the pane reads, so the second run of the chain — the one
        immediately before the `exec` — finds an approval rather than a question. Asking
        twice for one launch is the failure this pins."""
        self.assertEqual(self._launch(profile="claude-work", stdin=_Typed("y\n")), 0)
        self.assertTrue(any("new-window" in a for a in self.argvs), self.argvs)
        assert_approved("claude-work")
        # And the pane's own launcher, in this process, with a stdin that would raise if
        # it were read: the approval is already there to be found.
        self.assertEqual(launcher.start(self._profile(), [], fid=None, attended=True), 0)
        self.assertEqual(len(self.execs), 1, self.execs)

    def test_only_asking_is_deferred_to_the_pane(self):
        """N2a's nit. A press has no terminal, so the ASK waits for the pane — but every
        other refusal still happens here, where there is nothing to tear down. A `PATH`
        refusal carried into tmux would open a window to say it in and close it again."""
        rc, said = self._said(profile="claude-work", which=None, stdin=_APipe())
        self.assertEqual(rc, launcher.MISSING_EXIT)
        self.assertIn("not on PATH", said)
        self.assertEqual(self.argvs, [])

    def test_a_press_with_no_terminal_defers_to_the_pane(self):
        """The `+`, a workspace tab, the palette's new chat: nobody typed anything, the
        process behind the click has `/dev/null` for streams, and the pane it opens has a
        terminal on both ends. So it launches, with `--attended`, and the pane asks."""
        self.assertEqual(self._launch(profile="claude-work", stdin=_APipe()), 0)
        start = next(a for a in self.argvs if "new-window" in a)
        self.assertIn("--attended", start)

    def test_a_press_says_the_refusal_it_is_not_deferring(self):
        """The other half of the same rule, on the surface a press actually reports on:
        `_launch_refusal` hands a `+` the sentence to put on the attention row, and it
        must hand back nothing for the one question the pane is going to ask itself."""
        with mock.patch("sys.stdin", _APipe()):
            self.assertEqual(
                commands_frame._launch_refusal(self._profile(), attended=True), "")
            with mock.patch.object(launcher.shutil, "which", return_value=None):
                self.assertIn("not on PATH", commands_frame._launch_refusal(
                    self._profile(), attended=True))

    def test_an_attended_launch_with_no_terminal_refuses_with_needs_asking(self):
        """Review 3: somebody typed this, and there is nowhere to put the question. It is
        refused with its own sentence rather than asked into a pipe — and stdin is never
        read, which is the stronger claim: a read on a closed stream returns at once and
        looks exactly like a guard that is there."""
        said: list[str] = []
        with mock.patch("sys.stdin", _APipe()), \
                mock.patch("sys.stdout", io.StringIO()), \
                mock.patch.object(launcher.util, "err", side_effect=said.append):
            rc = launcher.start(self._profile(), [], fid=None, attended=True)
        self.assertEqual(rc, launcher.REFUSED_EXIT)
        self.assertTrue(any("no terminal here to ask in" in s for s in said), said)

    def test_an_attended_launch_with_a_terminal_on_both_asks(self):
        stdin = _Typed("y\n")
        with mock.patch("sys.stdin", stdin), \
                mock.patch("sys.stdout", _ATerminal()):
            self.assertEqual(launcher.start(self._profile(), [], fid=None, attended=True),
                             0)
        self.assertEqual(len(self.execs), 1, self.execs)
        self.assertEqual(stdin.reads, 1)

    def test_a_decline_is_not_reported_a_second_time_as_a_refusal(self):
        """The same rule one surface along — `launcher.start`, which is `--no-frame` and
        every launch whose output is a pipe. `ask_in_terminal` has already answered on the
        terminal the question went to; charter's red ✗ over the top of that would report
        the operator's own answer as a rule they broke."""
        said: list[str] = []
        with mock.patch("sys.stdin", _Typed("n\n")), \
                mock.patch("sys.stdout", (screen := _ATerminal())), \
                mock.patch.object(launcher.util, "err", side_effect=said.append):
            rc = launcher.start(self._profile(), [], fid=None, attended=True)
        self.assertEqual(rc, profiletrust.DECLINED_EXIT)
        self.assertEqual(said, [])
        self.assertIn("charter: nothing started.", screen.getvalue())
        self.assertEqual(self.execs, [])

    def test_a_yes_runs_the_whole_chain_again_before_the_exec(self):
        """Ruling 27: a yes answers the approval question and nothing else. The plane can
        move while the operator is reading the prompt — and a yes that walked straight past
        the checks behind it would be the one path in charter where saying yes to one
        question waives the rest."""
        gone: list[str] = []

        def _which(program, path=None):
            # On the ask's own read: the command leaves `PATH` while the question is on
            # screen. The first call is the chain before the ask, the second is after it.
            return None if gone else "/nowhere/claude"

        class _TypesAndRemovesTheCommand(_Typed):
            def readline(self) -> str:
                gone.append("yes")
                return super().readline()

        said: list[str] = []
        with mock.patch.object(launcher.shutil, "which", side_effect=_which), \
                mock.patch("sys.stdin", _TypesAndRemovesTheCommand("y\n")), \
                mock.patch("sys.stdout", _ATerminal()), \
                mock.patch.object(launcher.util, "err", side_effect=said.append):
            rc = launcher.start(self._profile(), [], fid=None, attended=True)
        self.assertEqual(rc, launcher.MISSING_EXIT)
        self.assertEqual(self.execs, [], "a yes walked past the check behind it")

    def test_a_record_lost_between_the_yes_and_the_recheck_says_what_happened(self):
        """S1's other half. After a yes the whole chain runs again (ruling 27), and the
        record it just wrote is under `.charter/`, which a chat can write (ruling 13) — so
        the re-run CAN come back asking. Asking a second time is what N2b rules out, and the
        sentence that used to come out here was `NEEDS_ASKING` — *there is no terminal here
        to ask in* — said on a terminal that plainly had one. It says what is true instead.
        """
        real = profiletrust.record_launched

        def _records_it_then_loses_it(p):
            why = real(p)
            (config.STATE_DIR / profiletrust.RECORD).write_text("{}")
            return why

        stdin, said = _Typed("y\n"), []
        with mock.patch("sys.stdin", stdin), \
                mock.patch("sys.stdout", _ATerminal()), \
                mock.patch.object(profiletrust, "record_launched",
                                  side_effect=_records_it_then_loses_it), \
                mock.patch.object(launcher.util, "err", side_effect=said.append):
            rc = launcher.start(self._profile(), [], fid=None, attended=True)
        self.assertEqual(rc, launcher.REFUSED_EXIT)
        self.assertEqual(stdin.reads, 1, "the same question was asked twice")
        self.assertEqual(self.execs, [], "a command nobody approved was started")
        self.assertTrue(any("moved while that question was on screen" in s
                            for s in said), said)
        self.assertFalse(any("no terminal here to ask in" in s for s in said), said)

    def test_the_moved_refusal_repeats_the_profiles_name_back_bounded(self):
        """The fourth one-line sentence that quotes a name, and it ends in the remedy
        `charter <name>` like its siblings — so it clips where the PROMPT does not. A
        200-character name in the middle of it is a remedy nobody reads to."""
        long_name = "w" * 200
        self.local.write_text(f'[harness.{long_name}]\nkind = "claude"\n'
                              'command = ["claude"]\n')
        real = profiletrust.record_launched

        def _records_it_then_loses_it(p):
            why = real(p)
            (config.STATE_DIR / profiletrust.RECORD).write_text("{}")
            return why

        said: list[str] = []
        with mock.patch("sys.stdin", _Typed("y\n")), \
                mock.patch("sys.stdout", _ATerminal()), \
                mock.patch.object(profiletrust, "record_launched",
                                  side_effect=_records_it_then_loses_it), \
                mock.patch.object(launcher.util, "err", side_effect=said.append):
            rc = launcher.start(self._profile(long_name), [], fid=None, attended=True)
        self.assertEqual(rc, launcher.REFUSED_EXIT)
        self.assertIn("w" * 160 + "...", said[0])
        self.assertNotIn(long_name, said[0])

    def test_a_record_that_cannot_be_written_refuses_and_never_asks_again(self):
        """N2b's nit: re-running the chain after a yes charter could not write down would
        find no record and ask the very same question a second time. It refuses instead,
        naming the path and what the filesystem said."""
        stdin = _Typed("y\n")
        said: list[str] = []
        with mock.patch("sys.stdin", stdin), \
                mock.patch("sys.stdout", _ATerminal()), \
                mock.patch.object(profiletrust.config, "replace_for",
                                  side_effect=OSError(28, "No space left on device")), \
                mock.patch.object(launcher.util, "err", side_effect=said.append):
            rc = launcher.start(self._profile(), [], fid=None, attended=True)
        self.assertEqual(rc, launcher.REFUSED_EXIT)
        self.assertEqual(stdin.reads, 1, "the same question was asked twice")
        self.assertTrue(any("No space left on device" in s for s in said), said)
        self.assertTrue(any(str(config.STATE_DIR) in s for s in said), said)

    def test_the_record_path_a_refusal_quotes_is_contained(self):
        """Ruling 35 on the one value in this sentence charter did not mint: the path is
        built from `config.STATE_DIR`, which is built from the plane root — a directory an
        operator named and a chat may have made — and this sentence goes to a terminal."""
        said: list[str] = []
        with mock.patch("sys.stdin", _Typed("y\n")), \
                mock.patch("sys.stdout", _ATerminal()), \
                mock.patch.object(profiletrust.config, "STATE_DIR",
                                  self.tmp / "pl\x1bane"), \
                mock.patch.object(profiletrust.config, "replace_for",
                                  side_effect=OSError(28, "no spa\x1b[2Kce left")), \
                mock.patch.object(launcher.util, "err", side_effect=said.append):
            rc = launcher.start(self._profile(), [], fid=None, attended=True)
        self.assertEqual(rc, launcher.REFUSED_EXIT)
        self.assertEqual(len(said), 1, said)
        # Both halves: the path, and what the filesystem said about it — which is not
        # charter's own words either, and arrives about a path the FILE chose.
        self.assertNotIn("\x1b", said[0])
        self.assertEqual(said[0].count("\\u001b"), 2, said[0])

    def test_the_record_path_a_refusal_quotes_is_not_clipped_short(self):
        """S2: the sentence ends in *fix that path and run it again*, so the path has to be
        one the reader can act on. `contain.PATH_DISPLAY_LIMIT` (1024) and not
        `DISPLAY_LIMIT` (160) — `contain` says why in as many words, and a plane's paths are
        legitimately long: this one is under a temp directory and 200 characters of name."""
        deep = self.tmp / ("d" * 200)
        said: list[str] = []
        with mock.patch("sys.stdin", _Typed("y\n")), \
                mock.patch("sys.stdout", _ATerminal()), \
                mock.patch.object(profiletrust.config, "STATE_DIR", deep), \
                mock.patch.object(profiletrust.config, "replace_for",
                                  side_effect=OSError(28, "No space left on device")), \
                mock.patch.object(launcher.util, "err", side_effect=said.append):
            rc = launcher.start(self._profile(), [], fid=None, attended=True)
        self.assertEqual(rc, launcher.REFUSED_EXIT)
        self.assertIn(str(deep / profiletrust.RECORD), said[0])

    def test_the_pane_asks_and_a_no_exits_130(self):
        """In the pane, whose stdin and stdout are its own tty. A no closes the window the
        way any other exit does — there is nothing to read afterwards, because the operator
        just answered the question themselves."""
        state.frame_dir("beta.1", create=True)
        waited: list[int] = []
        with mock.patch("sys.stdin", _Typed("n\n")), \
                mock.patch("sys.stdout", _ATerminal()), \
                mock.patch.object(launcher, "framed_chat", return_value="beta.1"), \
                mock.patch.object(launcher, "_wait_for_the_operator",
                                  side_effect=lambda: waited.append(1)):
            rc = launcher.cmd_frame_launch(SimpleNamespace(
                profile="claude-work", attended=True, rest=[]))
        self.assertEqual(rc, profiletrust.DECLINED_EXIT)
        self.assertEqual(self.execs, [])
        self.assertEqual(waited, [], "the operator answered; there is no key to wait for")

    def test_an_unattended_pane_refuses_rather_than_asks(self):
        """A reopen's pane and a handoff's pane carry no `--attended`, so the question is
        never put: there is nobody there to see it, and a pane waiting on one is a chat
        that never starts and never says why."""
        state.frame_dir("beta.1", create=True)
        said: list[str] = []
        with mock.patch("sys.stdin", _APipe()), \
                mock.patch.object(launcher, "framed_chat", return_value="beta.1"), \
                mock.patch.object(launcher.util, "err", side_effect=said.append):
            rc = launcher.cmd_frame_launch(SimpleNamespace(
                profile="claude-work", attended=False, rest=[]))
        self.assertEqual(rc, launcher.REFUSED_EXIT)
        self.assertTrue(any("nobody is at this open" in s for s in said), said)

    def test_a_built_in_never_asks_anywhere(self):
        """With a stdin that raises on a read, so this is "never asked" rather than
        "answered quickly".

        **The command is stood in for**, because which harnesses a machine happens to have
        installed is not what this case is about — and it decided it: with the real
        `shutil.which` this passed here and came back 127 on CI, where `codex` is not
        installed (`CONTRIBUTING.md`, *your machine is not the runner*).
        """
        with mock.patch("sys.stdin", _APipe()), \
                mock.patch.object(launcher.shutil, "which", return_value="/nowhere/codex"):
            self.assertEqual(launcher.start(self._profile("codex"), [], fid=None,
                                            attended=True), 0)
        self.assertEqual(len(self.execs), 1, self.execs)

    def test_each_refusal_says_something_different(self):
        """A reader has to be able to tell which rule fired. Four sentences for one
        profile, one fixture — so a copied message is a red test rather than a shrug."""
        p = self._profile()
        sentences = {profiletrust.refusal(p, attended=True).text,
                     profiletrust.refusal(p, attended=False).text}
        with mock.patch.object(launcher.shutil, "which", return_value=None):
            sentences.add(launcher.refusal(p, root=config.ROOT, attended=True,
                                           env=dict(os.environ)).text)
        (config.ROOT / ".gitignore").write_text("")
        sentences.add(launcher.refusal(p, root=config.ROOT, attended=True,
                                       env=dict(os.environ)).text)
        self.assertEqual(len(sentences), 4, sentences)


class AReopenAndAHandoffRefuseWhereTheyWouldHaveAsked(PersonaIso, unittest.TestCase):
    """Nobody is at either of these, and both know it before they start anything."""

    def setUp(self) -> None:
        super().setUp()
        make_plane(self)
        self.enterContext(mock.patch.dict(
            os.environ,
            {"PATH": os.environ.get("PATH", ""),
             "CHARTER_ROOT": str(config.ROOT), **_gitguard.environment()}, clear=True))
        self.local = declare_profiles(self)
        (config.WORKSPACES_DIR / "alpha").mkdir(parents=True, exist_ok=True)
        self.launched: list = []

    def _chat(self, **kw):
        fields = dict(chat="alpha.1", workspace="alpha", persona="", harness="claude-code",
                      cwd="", resume="", transcript="", active=True, profile="claude-work")
        return reopen_state.Chat(**{**fields, **kw})

    def _reopen(self, c) -> str:
        def fake_launch(args):
            self.launched.append(args)
            args.reopening.fid = "alpha.1"
            return 0

        said: list[str] = []
        with mock.patch.object(commands_frame.util, "warn", side_effect=said.append), \
                mock.patch("charter.commands_frame.cmd_launch", side_effect=fake_launch):
            commands_frame._reopen_one(c)
        return "\n".join(said)

    def test_a_reopen_skips_an_unapproved_profile(self):
        """A reopen has nobody to ask, and it says so by name rather than starting a chat
        whose pane would refuse a moment later. The chat stays in the manifest, so
        approving the profile once and running `charter reopen` again brings it back."""
        said = self._reopen(self._chat())
        self.assertEqual(self.launched, [])
        self.assertIn("a reopen has nobody to ask", said)
        self.assertIn("alpha.1", said)
        self.assertIn("charter claude-work", said)

    def test_the_skipped_chats_profile_is_repeated_back_bounded(self):
        """`profiles.NAME_RE` bounds a declared name's shape and not its length, so this is
        the one value in the sentence that arrives printable and as long as the file liked
        — and the sentence ends in the remedy, `Run charter <name> once to approve it`,
        which an unbounded name in the middle of it scrolls off the screen."""
        long_name = "w" * 200
        self.local.write_text(f'[harness.{long_name}]\nkind = "claude"\n'
                              'command = ["claude"]\n')
        said = self._reopen(self._chat(profile=long_name))
        self.assertIn("w" * 160 + "...", said)
        self.assertNotIn(long_name, said)
        self.assertEqual(self.launched, [])

    def test_a_reopen_of_an_approved_profile_goes_ahead(self):
        approve_profile(self)
        self.assertNotIn("nobody to ask", self._reopen(self._chat()))
        self.assertEqual(len(self.launched), 1)

    def test_a_reopen_of_a_built_in_is_never_stopped_by_this(self):
        self.assertNotIn("nobody to ask", self._reopen(self._chat(profile="codex")))
        self.assertEqual(len(self.launched), 1)


class ThePinsARealTmuxCaseCouldOnlyHangOn(PersonaIso, unittest.TestCase):
    """Two of Task 2's lines its own sweep could not measure, pinned in a second here.

    Both mutations hang a real-tmux case rather than reddening it, and a sweep run that
    timed out is neither green nor red — it is *unresolved*, so the line comes back with no
    verdict at all (`tools/sweep.py`, `SUBSET_TIMEOUT`). Task 3 owns the attended path and
    the ask, so each gets a pin that runs in this process with no tmux anywhere near it.
    """

    def setUp(self) -> None:
        super().setUp()
        make_plane(self)
        state.frame_dir("beta.1", create=True)

    def test_the_wait_for_a_launcher_is_bounded_by_a_deadline(self):
        """**Line one: `_await_the_launcher`'s deadline.** A pane charter cannot prove a
        chat for records nothing at all, so without the budget this wait never ends — and a
        reopen of four chats stops on the first of them, forever. The sleep counter is the
        bound: a loop with its deadline gone reaches it instead of hanging the run."""
        slept: list[float] = []

        def _sleep(seconds: float) -> None:
            slept.append(seconds)
            if len(slept) > 50:
                raise AssertionError("the wait is not bounded by its deadline")

        ticks = iter([0.0, 0.0, commands_frame._LAUNCHER_SECONDS + 1])
        with mock.patch.object(commands_frame.time, "sleep", side_effect=_sleep), \
                mock.patch.object(commands_frame.time, "monotonic",
                                  side_effect=lambda: next(ticks)), \
                mock.patch.object(commands_frame, "_pane_state",
                                  return_value=(commands_frame._ALIVE, None)):
            self.assertEqual(commands_frame._await_the_launcher("sock", "beta.1", "%1"),
                             (None, ""))

    def test_only_an_attended_open_puts_attended_on_the_argv(self):
        """**Line two: `("--attended",) if attended else ()`.** Collapsed, every pane on
        the plane is attended — including a reopen's and a handoff's, which have nobody in
        front of them — and each of those panes then stops on a question until the suite's
        own watchdog. Asserted in both directions, because only one of them is the hang."""
        self.assertIn("--attended", launcher.argv("claude-work", [], attended=True))
        self.assertNotIn("--attended", launcher.argv("claude-work", [], attended=False))


if __name__ == "__main__":
    unittest.main()
