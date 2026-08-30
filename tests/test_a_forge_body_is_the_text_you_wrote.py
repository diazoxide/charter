"""A forge command that publishes prose may not carry a live command substitution (#703).

An agent filing an issue wrote ``--body "… `env -u PYTHONSAFEPATH` …"``, meaning the
backticks as a markdown code span. Inside double quotes they are command substitution, so
the shell ran `env` and pasted sixty-four variables — four 1Password service-account
tokens, a GitLab PAT, the session's own variables — into a **public** issue body. It was
live for forty minutes, and a forge keeps public edit history, so redaction does not undo
it: rotation was the only remedy and rotation is the operator's work.

Nineteen other issues filed the same night used the same ``--body "…"`` shape with
backticks and were harmless, because the backticked text was not a runnable command. **The
pattern was wrong in all twenty and nineteen were lucky**, which is the whole case for a
guard rather than a rule: what got published was decided by what the operator's shell
exports, not by what the agent meant to say.

**What is asserted here is the property, not the refusal.** `assertRaises`-shaped testing —
"something was denied" — is exactly what hides in a refusal-shaped change, so every denial
below is checked for the SENTENCE that explains it, and every allow is paired with the
denial it must not become. Three properties, in the order they can go wrong:

1. **The guard agrees with a real shell about what would run.** `LiveSubstitution` is a
   differential test: each case is executed by `bash`, with a sentinel file as the oracle
   for *did the substitution run*, and the guard's verdict must match. This is the only
   part of the change that is new shell reasoning, and a fix for a class of bug is
   unusually likely to contain that bug — so it is checked against the shell rather than
   against its author's belief about the shell. It already caught one: a here-string
   `<<<"… `x` …"` was being re-read as a heredoc whose delimiter was the quoted word, which
   classified a live substitution as an inert body. Review did not catch that; bash did.

2. **The guard does not refuse the path the working rule prescribes.** ``--body-file -``
   with a *quoted* heredoc is what shared persona memory now tells every agent to use. A
   guard that denied it would be switched off within a day and would then cover nothing, so
   `TheRuleItSteersToward` pins the quoted form allowed and the unquoted form — one
   apostrophe's difference, and the same defect — denied.

3. **The guard says only what it can see.** It reads the SHAPE of a command line. It never
   sees the value, cannot see one (at `PreToolUse` the substitution has not run; by
   `PostToolUse` the issue is public), and must not be described as keeping credentials off
   a forge. `WhatItDoesNotClaim` holds the denial text to that, because the overclaim is
   the failure this project refuses and a security sentence that is false is worse than no
   sentence.
"""

from __future__ import annotations

import os
import subprocess
import tempfile
import unittest

from charter import config, hooks
from tests._isolation import PersonaIso, run_hook

#: The incident, with the payload replaced. `env -u PYTHONSAFEPATH` is not written here in
#: any runnable form and no fixture reproduces it: a working reproduction of an
#: exfiltration in a committed file is the defect, not a test of it. `id -un` stands in —
#: it is a substitution, which is the only property under test.
INCIDENT = 'gh issue create --title "t" --body "run `id -un` first"'


def _decision(r) -> str | None:
    return (r or {}).get("hookSpecificOutput", {}).get("permissionDecision")


def _reason(r) -> str:
    return (r or {}).get("hookSpecificOutput", {}).get("permissionDecisionReason", "")


class LiveSubstitution(unittest.TestCase):
    """:func:`hooks._live_substitution` answers what `bash` does, on every spelling.

    The oracle is a **sentinel file**, not stdout, and getting there took two corrections
    that are the same defect this module's own guards are written against.

    The first oracle substituted `echo MARK` and asked whether `MARK` came back. The
    *unexpanded* text contains `MARK` too, so every case answered "expanded" — **an
    assertion sitting on the path that already satisfies it.** That is `assertRaises(
    SomeError)`'s error one level out: it passes on the state it means to detect and on the
    state it means to reject, so it discriminates nothing while looking like evidence.

    The second oracle asked whether the substitution's *output* reached stdout, which is a
    different question from whether it ran, and reported two mismatches that were not: a
    heredoc whose file descriptor is superseded by a second heredoc is still expanded, and
    a substitution feeding a command that then errors has still run. A reader who trusted
    that oracle would have "fixed" a correct guard into a fail-open one.

    `touch <sentinel>` answers the question the guard is actually about — *did the shell run
    it* — and the answer stops depending on where the output went.
    """

    #: `@S@` becomes the substitution that must be observed if the shell runs it; `@N@` a
    #: second, inert one, for the cases that need two substitutions to tell apart.
    CASES = (
        # the incident's own shape, and the one character that makes it safe
        'printf %s "a `@S@` b"',
        "printf %s 'a `@S@` b'",
        'printf %s "a \\`@S@\\` b"',
        'printf %s "a $(@S@) b"',
        "printf %s 'a $(@S@) b'",
        # an apostrophe inside double quotes opens nothing — prose in a body is full of them
        'printf %s "it\'s `@S@`"',
        "printf %s \"a '`@S@`' b\"",
        # heredocs: the delimiter's quoting decides, and ANY quoting in it counts
        'cat <<EOF\na `@S@` b\nEOF',
        "cat <<'EOF'\na `@S@` b\nEOF",
        'cat <<"EOF"\na `@S@` b\nEOF',
        'cat <<\\EOF\na `@S@` b\nEOF',
        "cat <<EO'F'\na `@S@` b\nEOF",
        "cat <<-'EOF'\n\ta `@S@` b\n\tEOF",
        'cat <<-EOF\n\ta `@S@` b\n\tEOF',
        # an expanding body whose only substitution is the modern spelling
        'cat <<EOF\nbuilt at $(@S@) today\nEOF',
        # a backslash-quoted delimiter must still terminate its body at the right line
        'cat <<\\EOF\nplain text\nEOF\nprintf %s "`@S@`"',
        # a quoted delimiter ends at its own closing quote, not the line's last one
        "cat <<'BODY'\nnotes\nBODY\nprintf %s \"`@S@`\" 'tail'",
        # an empty QUOTED delimiter is a heredoc that ends at the first blank line
        'cat <<""\na `@S@` b\n\n',
        # two on one line: the bodies follow in the order the headers did
        "cat <<'A' <<B\nx `@N@` x\nA\ny `@S@` y\nB",
        "cat <<A <<'B'\nx `@S@` x\nA\ny `@N@` y\nB",
        # a here-STRING is an ordinary word, not a heredoc (the `<<<` regression)
        'cat <<<"a `@S@` b"',
        "cat <<<'a `@S@` b'",
        # quoting that ends somewhere a naive scan would get wrong
        'printf %s $\'a\\\'b\' && printf %s " `@S@` "',
        "printf %s 'a' \"b `@S@`\"",
        'printf %s "nested $(echo `@S@`)"',
        'X=1; printf %s "$X `@S@`"',
        # and the overwhelmingly common case: nothing to substitute at all
        'printf %s "no substitution here"',
        "printf %s 'single quoted $HOME'",
    )

    def test_the_guard_and_bash_agree_on_every_spelling(self) -> None:
        for template in self.CASES:
            with self.subTest(template):
                with tempfile.TemporaryDirectory() as d:
                    sentinel = os.path.join(d, "ran")
                    cmd = (template.replace("@S@", f"touch {sentinel}")
                                   .replace("@N@", "true"))
                    subprocess.run(["bash", "-c", cmd], capture_output=True,
                                   text=True, timeout=20)
                    self.assertEqual(
                        os.path.exists(sentinel),
                        hooks._live_substitution(cmd) is not None,
                        f"the guard and bash disagree about whether this runs: {cmd!r}")

    def test_the_cases_cover_both_answers(self) -> None:
        """A differential test where every case falls on one side proves only that the
        oracle and the guard are both stuck. Both answers have to be represented, or the
        agreement above is vacuous."""
        verdicts = {hooks._live_substitution(c.replace("@S@", "true").replace("@N@", "true"))
                    is not None for c in self.CASES}
        self.assertEqual(verdicts, {True, False})

    def test_the_spelling_is_reported_not_just_the_fact(self) -> None:
        """The verdict carries WHICH spelling matched, so the trace field can say what
        tripped the guard — #289's finding, applied to the guard added after it."""
        self.assertEqual(hooks._live_substitution('x "`id`"'), "`")
        self.assertEqual(hooks._live_substitution('x "$(id)"'), "$(")


class ForgeGuardCase(PersonaIso):
    """Helpers only — the hook driven end to end, with a plane present.

    Split out so the classes below do not INHERIT each other's tests: subclassing a
    TestCase for its helpers re-runs every parent case once per child, which turns one
    real failure into four and makes the count say nothing.
    """

    def setUp(self) -> None:
        super().setUp()
        (self.tmp / "charter.toml").write_text("schema = 1\n")

    def deny_reason(self, cmd: str) -> str:
        r = run_hook(hooks.pretooluse, {"tool_name": "Bash",
                                        "tool_input": {"command": cmd},
                                        "cwd": str(config.ROOT)})
        self.assertEqual(_decision(r), "deny", f"expected a refusal for {cmd!r}")
        return _reason(r)

    def allowed(self, cmd: str) -> None:
        r = run_hook(hooks.pretooluse, {"tool_name": "Bash",
                                        "tool_input": {"command": cmd},
                                        "cwd": str(config.ROOT)})
        self.assertNotEqual(_decision(r), "deny", f"expected {cmd!r} to be allowed")


class TheShapeThatWasPublished(ForgeGuardCase):
    """The incident is refused, and the refusal explains itself."""

    def test_the_incident_is_refused(self) -> None:
        why = self.deny_reason(INCIDENT)
        # The three things the reader needs, none of which is "denied": what was about to
        # happen, where, and what to type instead.
        self.assertIn("gh issue create", why)
        self.assertIn("substitution", why)
        self.assertIn("--body-file", why)
        # and it names the spelling it actually saw — a denial that always said `$(…)`
        # would be describing a command the agent did not write
        self.assertIn("`\u2026`", why)

    def test_the_refusal_says_the_shell_runs_it_before_gh_starts(self) -> None:
        """The mechanism, not the outcome. An agent that reads 'denied: unsafe' learns
        nothing and writes the same line again with a different flag; the one fact that
        stops it recurring is that the expansion happens BEFORE the forge command exists."""
        why = self.deny_reason(INCIDENT)
        self.assertIn("before", why.lower())
        self.assertIn("output", why.lower())

    def test_the_refusal_names_the_markdown_collision(self) -> None:
        """Why the mistake is not exotic: a body is the one argument where markdown code
        spans and shell substitution are the same character. A denial that did not say so
        leaves the agent believing it typed something strange."""
        self.assertIn("markdown", self.deny_reason(INCIDENT).lower())

    def test_the_modern_spelling_is_refused_too(self) -> None:
        self.assertIn("$(", self.deny_reason(
            'gh issue create --title "t" --body "run $(id -un) first"'))

    def test_a_single_quoted_body_publishes_the_backticks(self) -> None:
        """The same body, one character different, is exactly what an agent should write —
        and the shell agrees (`LiveSubstitution` runs this spelling)."""
        self.allowed("gh issue create --title 't' --body 'run `id -un` first'")

    def test_a_double_quoted_title_does_not_make_the_single_quoted_body_live(self) -> None:
        """The realistic mixed-quoting shape, and the one that says the scanner **leaves**
        double-quote state as well as entering it. A scanner that entered at the title's
        quote and never came out would read the body's single quotes as ordinary characters
        and refuse a command that publishes its backticks exactly as written."""
        self.allowed('gh issue create --title "a fix" --body \'see `id -un`\'')

    def test_a_body_file_is_untouched(self) -> None:
        self.allowed("gh pr create --title 't' --body-file /tmp/body.md")

    def test_reading_the_forge_is_not_publishing(self) -> None:
        """The guard's subject is published prose. A read publishes nothing, so a
        substitution in one is none of its business — and `gh issue list` is far more
        common than `gh issue create`."""
        self.allowed('gh issue view "$(cat /tmp/n)"')
        self.allowed('gh pr list --search "$(cat /tmp/q)"')

    def test_a_substitution_with_no_forge_write_is_not_this_guards_business(self) -> None:
        self.allowed('printf %s "today is $(date)"')

    def test_a_forge_write_with_no_substitution_is_not_touched(self) -> None:
        self.allowed('gh issue comment 703 --body "no backticks here at all"')

    def test_the_quoted_substitution_that_the_leak_guard_lists_as_open(self) -> None:
        """`_leak_reason` documents a quoted command substitution as a known-open bypass —
        shlex keeps `"$(cat <vault>)"` as one word and no vault predicate looks inside it.
        For this one command family that hole is now closed, which is worth pinning because
        it is the only place the two guards overlap."""
        vault = str(config.ROOT / ".charter" / "vaults" / "db.json")
        self.assertEqual(
            _decision(run_hook(hooks.pretooluse,
                               {"tool_name": "Bash",
                                "tool_input": {"command":
                                               f'gh issue create --body "$(cat {vault})"'},
                                "cwd": str(config.ROOT)})),
            "deny")


class TheRuleItSteersToward(ForgeGuardCase):
    """`--body-file -` with a QUOTED heredoc is what shared persona memory prescribes.

    This is the calibration that decides whether the guard survives contact. A guard whose
    firing condition is the prescribed workflow is not miscalibrated but inverted — the
    argument #371 used to delete the clone-commit nudge outright — so the prescribed form
    must be allowed with a body full of the exact characters that caused the incident.

    The unquoted twin is the same defect with one apostrophe's less typing, and it is the
    failure mode the rule itself creates: an agent that has learned "use a heredoc" and
    forgets the quotes is back at #703 with no guard, which is why covering only `--body`
    would have been worse than covering nothing.
    """

    BODY = "See `id -un` and $(date) for the version."

    def test_a_quoted_heredoc_body_is_allowed(self) -> None:
        for delim in ("'BODY'", '"BODY"', "\\BODY"):
            with self.subTest(delim):
                self.allowed(f"gh issue comment 703 --body-file - <<{delim}\n"
                             f"{self.BODY}\nBODY")

    def test_a_quoted_heredoc_survives_the_tab_stripping_form(self) -> None:
        self.allowed(f"gh pr create --body-file - <<-'BODY'\n\t{self.BODY}\n\tBODY")

    def test_an_unquoted_heredoc_is_the_same_defect_and_is_refused(self) -> None:
        why = self.deny_reason(f"gh issue comment 703 --body-file - <<BODY\n"
                               f"{self.BODY}\nBODY")
        self.assertIn("--body-file", why)
        self.assertIn("<<'BODY'", why)

    def test_the_unquoted_tab_stripping_form_is_refused_too(self) -> None:
        self.deny_reason(f"gh pr create --body-file - <<-BODY\n\t{self.BODY}\n\tBODY")

    def test_quotes_inside_an_expanding_body_do_not_save_it(self) -> None:
        """A heredoc body has no quoting rules but the backslash, so single-quoting the
        code span inside an unquoted heredoc still runs it — checked against bash in
        `LiveSubstitution`. Reusing the command-line scanner here would have applied
        single-quote protection a shell does not offer, on the exact path the rule steers
        agents onto."""
        self.deny_reason("gh issue create --body-file - <<BODY\nsee '`id -un`'\nBODY")

    def test_a_backslash_escaped_span_in_an_expanding_body_is_allowed(self) -> None:
        self.allowed("gh issue create --body-file - <<BODY\nsee \\`id -un\\`\nBODY")

    def test_a_dollar_substitution_alone_in_an_expanding_body_is_caught(self) -> None:
        """The modern spelling with **no backtick anywhere**, which is the point.

        The deletion sweep found this: every other heredoc case here carries a backtick, so
        the backtick arm answered first and the `$(` arm of the body scanner was never the
        thing under test. Disabling it left an expanding heredoc whose body runs `$(…)`
        allowed — confirmed against bash, which really does execute it. A case that two
        arms can both satisfy tests neither.
        """
        self.deny_reason("gh issue create --body-file - <<BODY\nbuilt at $(date) today\nBODY")

    def test_a_backslash_quoted_delimiter_still_ends_its_own_body(self) -> None:
        """`<<\\EOF` does not expand, so nothing in its body is scanned — but the scanner
        still has to know **where that body stops**, or everything after it is swallowed as
        more inert body.

        Also from the sweep. The delimiter of `<<\\EOF` is `EOF`: the backslash quotes the
        heredoc and the character it escapes is still part of the name. Losing that
        character leaves the scanner hunting a terminator that never arrives, so it consumes
        the rest of the command — and the live substitution after the heredoc was allowed.
        Every other backslash-delimiter case here ends at the end of the string, where a
        swallowed remainder is empty and the bug is invisible.
        """
        self.deny_reason("gh issue create --body-file - <<\\EOF\nplain text\nEOF\n"
                         'echo "`id -un`"')

    def test_a_quoted_delimiter_ends_at_its_OWN_closing_quote(self) -> None:
        """`<<'BODY'` closes at the next quote, not at the last one on the line.

        The third finding the sweep produced here, and the subtlest: the delimiter scan
        reaches for the closing quote, and reaching for the *wrong* one swallows everything
        between into the delimiter — including a live substitution in a later command, which
        is then never scanned at all. Every other quoted-delimiter case in this file has no
        second quote anywhere after it, so the two spellings agree and the bug is invisible.

        The shape below is ordinary: file an issue from a heredoc, then comment on a request
        with a substitution in it and a quoted `--repo`. bash runs that substitution.
        """
        self.deny_reason("gh issue create --body-file - <<'BODY'\nnotes\nBODY\n"
                         "gh pr comment 1 --body \"`id -un`\" --repo 'o/r'")


class WhatItDoesNotClaim(ForgeGuardCase):
    """The stated limits, pinned so a later edit cannot quietly widen the claim.

    #370's ruling is that a guard which cannot verify what it claims is worse than a
    documented boundary. This guard verifies one thing — that a live substitution stands on
    the line of a prose-publishing forge command — and the denial has to stay inside that.
    """

    def test_the_denial_does_not_promise_to_keep_a_credential_off_a_forge(self) -> None:
        why = self.deny_reason(INCIDENT)
        self.assertIn("does not claim", why.lower())
        self.assertIn("shape", why.lower())

    def test_the_scope_is_the_whole_call_and_the_denial_is_honest_about_it(self) -> None:
        """Substitution attribution is not attempted: a substitution anywhere on a call
        that also publishes is refused, including where it lands nowhere near the body.
        Denying more is the direction to be wrong in, and it is asserted rather than
        discovered by whoever hits it."""
        self.deny_reason('cd "$(git rev-parse --show-toplevel)" && '
                         "gh pr create --body-file b.md")

    def test_a_separate_bash_call_is_the_escape_hatch(self) -> None:
        """Each call is judged alone, so computing the value first and publishing second
        is allowed — the remedy the denial names has to actually work."""
        self.allowed('B="$(git branch --show-current)"')
        self.allowed("gh pr create --head main --body-file b.md")

    def test_a_body_file_holding_the_same_text_is_not_covered(self) -> None:
        """The gap #703 named, stated as a test so it cannot be forgotten into a claim of
        coverage. A file written earlier holds whatever it holds; the shell expands nothing
        on its way through `--body-file`, so there is no substitution to see and this guard
        is silent. That is a limit, not a hole to be plugged here: the file's CONTENT is a
        different question from the command line's SHAPE."""
        self.allowed("gh issue create --title 't' --body-file /tmp/anything.md")

    def test_git_commit_is_a_sibling_this_guard_does_not_cover(self) -> None:
        """A commit message is outward-facing too, and the same substitution lands in it.

        **Not excluded because it is less severe — it is MORE severe.** The first version of
        this docstring said a commit is visible in `git log` and in the request before it
        reaches anyone, which confuses visibility with reversibility. On the axis #703 turns
        on, *can this be undone*, a commit message is strictly worse than an issue body: a
        body is replaced in one call, while a pushed commit message needs a history rewrite
        and a rewrite does not reach forks, existing clones, or the forge's own caches.

        It is out of scope because of **calibration on an unverified surface**. The nineteen
        `(tool, noun, verb)` rows were each checked against `gh`/`glab` `--help`; nothing
        equivalent has been done for the commit surface. And that surface is dense with the
        exact character this guard keys on — measured on this repository, **25 of the last
        30 commit messages on `main` contain a backtick**, 10 of them around text that reads
        as a runnable command. Inside `-m "…"` every one of those is live. A security guard
        that fires constantly on legitimate work is how a guard gets switched off, and then
        it protects nothing (`_BRANCH_MOVERS` and #371 both make this argument).

        So the boundary is named here rather than implied by the guard's silence, and it is
        filed as its own issue rather than left as an omission.
        """
        self.allowed('git commit -m "see `id -un`"')


class AMalformedCommandStillGetsAnAnswer(ForgeGuardCase):
    """The scanner returns a verdict for any string, and raising is not a verdict.

    **An exception here is worse than either answer.** `hooks.dispatch` calls the handler
    as `rc = fn()` with nothing around it, so a scanner that raised would take the turn
    down — and this guard runs on *every* Bash call, where most strings are not commands
    anybody was careful about. Every unterminated construct therefore has a defined end:
    the scanner treats the rest of the string as literal, which is also what a shell does
    with the whole command, since it refuses to run it at all.

    Where a malformed command *does* still resolve to a live substitution, it is **refused**
    rather than waved through. That is `_leak_reason`'s own posture on unparseable input,
    quoted here because the same argument decides it: *"a false deny on an already-malformed
    command is survivable; printing a credential is not."*
    """

    #: Each of these ends inside something — a quote, a heredoc, an escape — and the last
    #: two end mid-construct at the final character, which is where an off-by-one lives.
    MALFORMED = (
        "",
        "gh issue create --body ",
        'gh issue create --body "unterminated',
        "gh issue create --body 'unterminated",
        "gh issue create --body-file - <<",
        "gh issue create --body-file - <<'UNCLOSED\nbody text\n",
        "gh issue create --body-file - <<UNCLOSED\nbody text\n",
        "gh issue create --body \"trailing escape \\",
        'gh issue create --body "$',
        'gh issue create --body "x`',
        "gh issue create --body $'abc",
        'gh issue create --body-file - <<<<"`id -un`"',
        # a heredoc delimiter that ends in a backslash: the escape has nothing to escape
        "gh issue create --body-file - <<\\",
    )

    def test_no_input_makes_the_scanner_raise(self) -> None:
        for cmd in self.MALFORMED:
            with self.subTest(cmd):
                self.assertIn(hooks._live_substitution(cmd), (None, "`", "$("))

    def test_the_scanner_answers_for_a_command_that_is_not_a_string(self) -> None:
        """`None` is not a command, and the answer is still an answer.

        `_forge_substitution_hit` normalises before it calls, so this fallback looks
        redundant from the one call site that exists today. The next caller is the reason it
        is here, and an exception is the one outcome this module may not have: `dispatch`
        runs the handler as a bare `rc = fn()`, so a raise takes the turn down rather than
        producing a verdict."""
        self.assertIsNone(hooks._live_substitution(None))
        # and the same contract one level out, where the hot-path filter would be the
        # thing that raises
        self.assertIsNone(hooks._forge_substitution_hit(None))

    def test_no_input_makes_the_guard_raise(self) -> None:
        for cmd in self.MALFORMED:
            with self.subTest(cmd):
                hit = hooks._forge_substitution_hit(cmd)
                self.assertTrue(hit is None or isinstance(hit, tuple))

    def test_a_live_tick_at_the_very_last_character_is_still_seen(self) -> None:
        """The final character of the string, inside an unterminated double quote. A scan
        that stopped one short of the end would answer *allow* here, and the same
        off-by-one is what decides a body whose backtick happens to land last."""
        self.deny_reason('gh issue create --body "x`')

    def test_an_empty_quoted_delimiter_is_still_a_heredoc(self) -> None:
        """`<<""` names the empty string: it does not expand, and it ends at the first
        empty line. `<<` with no word after it is not a heredoc at all.

        Collapsing those two made the quoted-empty form untracked, so its body was read as
        command text and charter refused a body bash does not expand — a false REFUSAL
        rather than a miss, which is why nothing caught it until the sweep did. Settled
        against bash, which runs neither of these."""
        self.assertIsNone(hooks._live_substitution('cat <<""\n`id -un`\n\n'))
        # And the other half: `<<` with no word is not a heredoc, so what follows is
        # COMMAND text and its quoting applies. Tracking it as an empty-delimiter heredoc
        # instead would scan those lines under heredoc rules, where a single quote protects
        # nothing — refusing a backtick the shell keeps literal.
        self.assertIsNone(hooks._live_substitution("cat <<\n'`id -un`'"))
        self.assertEqual(hooks._live_substitution("cat <<\nx\n`id -un`"), "`")

    def test_the_last_line_of_an_unterminated_body_is_not_truncated(self) -> None:
        """A heredoc body's final line has no trailing newline to stop at, and reading it
        as \"up to the newline\" silently drops its last character.

        Every input that distinguishes this is one bash rejects — for anything it will
        actually run, the final line holds a complete substitution and losing one character
        still leaves the opener visible. It is asserted anyway, on the same footing as the
        here-string case: the cost is a refusal of something that would not have run."""
        self.assertEqual(hooks._live_substitution("cat <<BODY\n`"), "`")

    def test_a_here_string_is_never_read_as_a_heredoc_header(self) -> None:
        """`<<<` is stepped over whole. Reading it one character at a time leaves a `<<`
        whose "delimiter" is the quoted word, which files a live substitution away as an
        inert heredoc body — the fail-open a real bash caught during review.

        Bash rejects this input outright (`<<<<` is a syntax error) and it is asserted
        anyway, for the reason `_leak_reason` gives about unparseable commands: a false
        refusal of something that would not have run is survivable, and the other direction
        is not. It is the only input that distinguishes the branch, which is worth knowing
        rather than discovering when somebody deletes it."""
        self.assertEqual(hooks._live_substitution('cat <<<<"`id -un`"'), "`")

    def test_an_unterminated_single_quote_leaves_the_rest_literal(self) -> None:
        """The opposite end of the same property: what a shell would refuse to run, this
        reads as literal rather than guessing a second meaning for it."""
        self.assertIsNone(hooks._live_substitution("gh issue create --body 'x`"))


class TheArmsThatOtherArmsWereAnswering(ForgeGuardCase):
    """Cases where a second, easier arm had been answering for the one under test.

    Every one of these came from the deletion sweep rather than from review, and they share
    a shape with the oracle defect this module's docstring names: **a case that two arms can
    both satisfy tests neither.** Each test below is written so exactly one arm can answer
    it — the other spelling removed, the other position moved, or the other quoting dropped.
    """

    def test_an_unquoted_substitution_is_caught(self) -> None:
        """`--body $(cat …)` with no quotes at all.

        Every other `$(` case in this file sits inside double quotes, so they were answered
        by the double-quoted scanner and the unquoted arm of the main loop was never the
        thing under test. Removing that arm left this allowed — and bash runs it."""
        self.deny_reason("gh issue create --title t --body $(cat /tmp/b.md)")

    def test_a_bare_dollar_in_an_expanding_body_is_not_a_substitution(self) -> None:
        """`$` alone is not `$(`, and an expanding heredoc full of prices or variables is
        prose.

        Asserted on the scanner, and that is not a style choice: routed through the hook
        this command never reaches the code under test, because it holds no backtick and
        no `$(` and the hot-path filter answers first. It would pass on a guard with no
        heredoc scanner at all — an assertion sitting on the path that already satisfies
        it, which is the failure this file keeps finding."""
        self.assertIsNone(hooks._live_substitution(
            "cat <<BODY\nset $HOME first\nBODY"))

    def test_a_single_quoted_run_ends_at_its_own_closing_quote(self) -> None:
        """The single-quote scan reaches for the *next* quote, not the line's last one.

        Reaching for the last one swallows everything between two quoted arguments — here a
        live substitution in the `--title` between `--body 'plain'` and `--repo 'o/r'`,
        which bash really runs."""
        self.deny_reason("gh issue create --body 'plain' --title \"`id -un`\" --repo 'o/r'")

    def test_a_tab_stripped_heredoc_still_ends_where_it_says(self) -> None:
        """`<<-'BODY'` strips leading tabs from the terminator, so a tab-indented `BODY`
        closes it. Not stripping means it never closes, the rest of the command is eaten as
        inert body, and the substitution in the command after it is never seen."""
        self.deny_reason("gh issue create --body-file - <<-'BODY'\n\tnotes\n\tBODY\n"
                         "gh pr comment 1 --body \"`id -un`\"")

    def test_a_plain_heredoc_does_not_strip_tabs_from_its_terminator(self) -> None:
        """The other direction, and the one that costs a false refusal. `<<'BODY'` without
        the dash does **not** strip, so a tab-indented `BODY` inside the body is body text.
        Stripping anyway ends the heredoc early and reads the rest of its own body as
        commands — including a `$(…)` that is inert where it actually sits."""
        self.allowed("gh issue create --body-file - <<'BODY'\n\tBODY\n$(id -un)\nBODY")

    def test_a_backslash_escapes_outside_quotes_too(self) -> None:
        """`\\`` is a literal backtick wherever it stands, not only inside double quotes.

        Every other escape case in this file sits inside a heredoc body or a double-quoted
        run, so the main loop's own backslash branch was never the thing answering. Removing
        it turns an escaped backtick into a live one and refuses a command bash runs
        literally."""
        self.allowed("gh issue create --title t --body x\\`y")

    def test_a_bare_dollar_outside_quotes_is_not_a_substitution(self) -> None:
        """`$HOME` is not `$(`, unquoted just as much as quoted.

        The inert backtick in the title is load-bearing: without a substitution somewhere
        the hot-path filter answers first and the case proves nothing about the `$` at
        all."""
        self.allowed("gh issue create --title 'a `b`' --body $HOME/x")

    def test_a_pending_heredoc_body_is_consumed_at_the_NEWLINE(self) -> None:
        """A heredoc's body starts on the next line, not at the next character.

        Consuming it the moment a header is seen eats the rest of the header's own line —
        here a `--title` carrying a live substitution — and an inert (`<<'BODY'`) heredoc
        never rescans what it swallowed, so the substitution is simply lost. bash runs it."""
        self.deny_reason("gh issue create --body-file - <<'BODY' --title \"`id -un`\"\n"
                         "x\nBODY")

    def test_a_bare_dollar_does_not_open_an_ansi_c_quotation(self) -> None:
        """`$USER` is not `$'…'`, and reading it as one skips to the next single quote —
        over a live substitution on the way.

        `_ansi_c_end` exists because `$'a\\'b'` ends at the LAST quote, so a plain
        single-quote scan ends it in the wrong place. Entering it on any `$` instead of on
        `$'` turns that repair into a much larger hole: an unquoted `$VAR` anywhere before
        the body swallows the rest of the line. bash runs the substitution here."""
        self.deny_reason('gh issue create --title $USER --body "`id -un`"')

    def test_a_plain_redirection_is_not_a_heredoc(self) -> None:
        """`<` is one character and `<<` is two, and treating the first as the second reads
        the redirection's filename as a heredoc delimiter.

        A QUOTED filename is what makes this bite: the delimiter comes back \"quoted\", so
        the invented heredoc does not expand, and everything after the redirection — the
        next command included — is swallowed as inert body. bash runs the substitution that
        is then never looked at."""
        self.deny_reason("gh issue create --body-file - < 'notes.md'\necho \"`id -un`\"")

    def test_a_flag_value_cannot_supply_the_noun_and_the_verb(self) -> None:
        """Flag values are words too, and two of them can read as `issue create`.

        The scan used to drop `-…` tokens before pairing, which joined words a flag stood
        between — and made `--label issue --state create` a match on a command that
        publishes nothing. Pairing over every token instead keeps the real cases (a global
        flag's value still sits in front of the noun) and drops that false one."""
        # A LIVE substitution must be present, or nothing reaches the pairing at all: the
        # hot-path filter answers first on a command with none, and `_forge_prose_command`
        # is only consulted once a live one has been found. An inert backtick here made the
        # case vacuous in a second way, which is the same defect one layer along.
        self.allowed('gh issue list --label issue --state create --jq "$(cat /tmp/f)"')
        self.deny_reason('gh --repo o/r issue create --body "`id -un`"')



class TheFalsePositivesValueMatchingWouldHaveHad(ForgeGuardCase):
    """The option #703 weighed first was matching a body against the exported environment.

    It has a false-positive problem — a body legitimately quoting `$HOME`, or a version
    string — and a worse property: it needs charter to *read* the environment it is
    protecting, holding every credential on the machine in guard code on every Bash call.
    Both are moot, and that is the finding rather than the preference: **at `PreToolUse`
    there is no value to match.** The hook is handed ``--body "`env`"``, not the sixty-four
    variables it becomes, and by `PostToolUse` the issue is public. Value-matching is not
    an option that was rejected here; it is one that does not exist at either end.

    Matching the SHAPE has neither problem, and these are the cases that show it: a body
    quoting an environment variable is prose, and the guard is silent on it.
    """

    def test_a_body_quoting_an_environment_variable_is_prose(self) -> None:
        self.allowed('gh issue create --body "set $HOME before running"')

    def test_a_version_string_in_a_body_is_prose(self) -> None:
        self.allowed('gh pr comment 1 --body "version 1.2.3 (build $BUILD)"')

    def test_a_dollar_that_is_not_a_substitution_is_not_one(self) -> None:
        """`$VAR` expands too, and is deliberately NOT refused. It carries one value the
        agent named, which is a different act from splicing in the output of a command it
        did not — and refusing it would deny most prose about shell usage."""
        self.assertIsNone(hooks._live_substitution('x "$HOME/$USER ${PATH}"'))


class TheSpellingsThatAreStillTheSameDefect(ForgeGuardCase):
    """Shapes an agent really writes that are the incident wearing different punctuation."""

    def test_a_line_continuation_does_not_hide_it(self) -> None:
        """Agents wrap long forge commands. A scanner that stopped at the first newline
        would see a `--title` and miss the `--body` two lines down."""
        self.deny_reason('gh issue create --title "t" \\\n  --body "see `id -un`"')

    def test_a_process_substitution_feeding_body_file_is_caught(self) -> None:
        """`--body-file <(…)` looks like the safe path and is not: the shell expands the
        double-quoted word inside it exactly as it would in `--body`."""
        self.deny_reason('gh issue create --body-file <(echo "`id -un`")')

    def test_a_substitution_in_the_title_counts(self) -> None:
        """A title is published prose too, so the body being a file saves nothing."""
        self.deny_reason('gh issue create --title "`id -un`" --body-file b.md')

    def test_the_common_report_posting_shape_is_refused_with_a_one_word_remedy(self) -> None:
        """`--body "$(cat report.md)"` is the pattern an agent reaches for after generating
        a file, and `--body-file report.md` is the same command shorter. This is the denial
        most likely to be seen, so the remedy has to be in it."""
        self.assertIn("--body-file", self.deny_reason('gh pr comment 1 --body "$(cat r.md)"'))


class TheTableIsLoadBearing(ForgeGuardCase):
    """Every row of :data:`hooks._FORGE_PROSE` answers, and the branch that reaches it is
    not what decides.

    `_PUBLISH_FORGE` sits ten lines above the new table and looks like the same thing. It
    is not: it answers "does this publish or land CODE", and `gh pr create` is
    *deliberately absent* from it while being the single most likely command to carry this
    defect. One constant answering two questions is the #555 defect this module already has
    a name for, so the tables are separate and this pins both directions of that — the new
    table covering what the old one excludes, and the old one still excluding it.
    """

    def test_every_row_denies(self) -> None:
        for tool, noun, verb in sorted(hooks._FORGE_PROSE):
            with self.subTest(f"{tool} {noun} {verb}"):
                self.deny_reason(f'{tool} {noun} {verb} x --body "`id -un`"')

    def test_a_verb_that_is_not_in_the_table_is_allowed(self) -> None:
        for cmd in ("gh issue list", "gh pr checkout", "gh pr merge",
                    "glab mr merge", "glab issue view"):
            with self.subTest(cmd):
                self.allowed(f'{cmd} x --body "`id -un`"')

    def test_gh_pr_create_is_covered_here_and_still_absent_from_the_release_floor(self) -> None:
        self.assertIn(("gh", "pr", "create"), hooks._FORGE_PROSE)
        self.assertNotIn(("gh", "pr", "create"), hooks._PUBLISH_FORGE)

    def test_the_noun_is_found_past_a_global_flags_value(self) -> None:
        """`gh --repo o/r issue create` puts the repo in front of the noun. Reading the
        first two non-flag words — which is where A4's own reader stops — would see
        `("gh", "o/r", "issue")` and allow the command."""
        self.deny_reason('gh --repo o/r issue create --body "`id -un`"')

    def test_a_quoted_argument_cannot_supply_the_pair(self) -> None:
        """shlex keeps a quoted value as ONE word, so prose that merely mentions a verb is
        prose — the false positive `_leak_reason` was rewritten to stop having."""
        self.allowed('gh issue list --search "pr create" --json number')


class ItIsNotGatedOnAControlPlane(unittest.TestCase):
    """Ungated, with `_leak_reason` and unlike A2/A3/A4.

    Those are gated because denying them outside a plane explains a control plane that does
    not exist on that machine — `git clone git@…` refused in an unrelated repo. This one
    explains a SHELL, which is present wherever the harness runs, and its remedy is plain
    `gh` usage naming nothing of charter's. A plane-gated version would publish the same
    body from one directory and refuse it from another, which is the guard-that-looks-
    present failure this module argues against everywhere else.
    """

    def test_it_fires_with_no_control_plane(self) -> None:
        old = config.HAS_CONTROL_PLANE
        config.HAS_CONTROL_PLANE = False
        try:
            self.assertIsNotNone(hooks._forge_substitution_hit(INCIDENT))
        finally:
            config.HAS_CONTROL_PLANE = old


if __name__ == "__main__":
    unittest.main()
