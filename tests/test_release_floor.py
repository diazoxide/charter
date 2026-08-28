"""Publishing is on the floor: an unattended run may not cut a release.

0.46.0 taught `_ask` to fall back to `allow` under `bypassPermissions`, so a workflow nudge
stops hanging a run with nobody at the keyboard. Right for a nudge — wrong for the one
command in this repo that cannot be undone, and the change silently removed the cover that
was there:

    git tag v9.9.9  (default)            -> ask     # a hook `ask` floors at a prompt in
    git tag v9.9.9  (bypassPermissions)  -> allow   # EVERY mode, so this used to stop

The cover was the clone-commit nudge, whose git-write pattern happened to include `tag` and
`push`. It was never designed to guard releases; it just did, and pushing a tag fires
`release.yml` → an irreversible PyPI publish (Trusted Publishing, no token, nothing to
retract). `gh pr merge` and `gh release create` were never covered at all, being no kind of
`git`. That nudge has since been deleted outright (#371) — which is exactly why this guard
had to stop depending on it, and why the cases below assert the floor directly rather than
through anything the nudge happens to catch.

Two properties carry the whole design and are asserted with equal weight:

* **Unattended DENIES.** Not asks — 0.46.0 made an unattended ask into an allow, so an ask
  here would be indistinguishable from no guard. Deny also cannot hang: the run gets an
  immediate refusal naming a remedy.
* **Attended is UNCHANGED.** A person cutting a release must see exactly what they saw
  before. A guard that made releases harder for the operator is the cage the plane-root
  guard's docstring warns about, and the fix people reach for then is to disable it forever.
"""

import unittest

from tests._isolation import run_hook
from tests.test_hooks import InAControlPlane
from charter import hooks

UNATTENDED = "bypassPermissions"


class FloorCase(InAControlPlane):
    def decide(self, cmd: str, mode: str | None = None, in_clone: bool = False):
        cwd = str(self.tmp / "workspaces" / "w" / "r") if in_clone else str(self.tmp)
        (self.tmp / "workspaces" / "w" / "r").mkdir(parents=True, exist_ok=True)
        payload = {"tool_input": {"command": cmd}, "cwd": cwd,
                   "session_id": "s", "tool_use_id": "t"}
        if mode is not None:
            payload["permission_mode"] = mode
        r = run_hook(hooks.pretooluse, payload)
        return None if r is None else r["hookSpecificOutput"]["permissionDecision"]


class TestUnattendedCannotCutARelease(FloorCase):
    def test_creating_a_tag_is_denied(self):
        """The choke point: without a local tag there is nothing to push."""
        self.assertEqual("deny", self.decide("git tag v9.9.9", UNATTENDED))

    def test_creating_a_tag_is_denied_whatever_it_is_called(self):
        """Keyed on tagging, not on `v*`. Shape-matching is narrower and more 'correct',
        and is walked past by naming the tag `release-1`."""
        self.assertEqual("deny", self.decide("git tag release-1", UNATTENDED))
        self.assertEqual("deny", self.decide("git tag -a 2026-08 -m x", UNATTENDED))

    def test_the_program_name_is_case_folded(self):
        """`GIT` and `git` are the same binary on APFS and NTFS. This guard took the
        program name with `rsplit("/", 1)[-1]` and no fold, so `GIT tag v1` — and the
        same Shift key on the one-credential guard — walked past both."""
        self.assertEqual("deny", self.decide("GIT tag v9.9.9", UNATTENDED))
        self.assertEqual("deny", self.decide("/usr/bin/GIT push --tags", UNATTENDED))

    def test_a_substitution_does_not_stand_the_tag_down(self):
        """`(`/`)` as plain segment boundaries stranded the operand: the tag NAME ended up
        in a segment of its own, and `git tag` alone only LISTS."""
        self.assertEqual("deny", self.decide("git tag $(cat VERSION)", UNATTENDED))

    def test_pushing_tags_is_denied(self):
        """Defence in depth — a tag that already exists locally could still be pushed."""
        for cmd in ("git push --tags",
                    "git push --follow-tags origin main",
                    "git push origin refs/tags/v9.9.9",
                    "git push origin v9.9.9"):
            self.assertEqual("deny", self.decide(cmd, UNATTENDED), cmd)

    def test_forge_release_and_merge_are_denied(self):
        """Never covered before — no kind of `git`, so the old nudge never saw them."""
        for cmd in ("gh release create v9.9.9",
                    "gh pr merge 1 --squash",
                    "glab release create v9.9.9",
                    "glab mr merge 1"):
            self.assertEqual("deny", self.decide(cmd, UNATTENDED), cmd)

    def test_the_denial_names_a_remedy(self):
        r = run_hook(hooks.pretooluse,
                     {"tool_input": {"command": "git tag v9.9.9"}, "cwd": str(self.tmp),
                      "session_id": "s", "permission_mode": UNATTENDED})
        reason = r["hookSpecificOutput"]["permissionDecisionReason"]
        self.assertIn("attended", reason)

    def test_it_denies_outside_a_clone_too(self):
        """Unlike the clone nudge it accidentally replaced, this is not about where you are."""
        self.assertEqual("deny", self.decide("git tag v9.9.9", UNATTENDED, in_clone=False))
        self.assertEqual("deny", self.decide("git tag v9.9.9", UNATTENDED, in_clone=True))


class TestAttendedIsCompletelyUnchanged(FloorCase):
    """The regression this guard must not become."""

    def test_a_person_tagging_at_the_plane_is_not_stopped(self):
        self.assertIsNone(self.decide("git tag v9.9.9", "default"))

    def test_a_person_tagging_in_a_clone_is_not_stopped_either(self):
        """Was `ask` while the clone-commit nudge existed; that nudge is gone (#371), so
        this is now silent for the same reason the plane-root case is. The floor is what
        matters here and it is asserted directly above — attended tagging never denies."""
        self.assertIsNone(self.decide("git tag v9.9.9", "default", in_clone=True))

    def test_auto_mode_is_attended(self):
        """`auto` usually has a human watching — same boundary `_ask` already draws."""
        self.assertIsNone(self.decide("git tag v9.9.9", "auto"))

    def test_a_missing_mode_is_attended(self):
        """A host that sends no `permission_mode` must never be read as unattended."""
        self.assertIsNone(self.decide("git tag v9.9.9", None))

    def test_forge_release_is_untouched_when_attended(self):
        for cmd in ("gh release create v9.9.9", "gh pr merge 1 --squash"):
            self.assertIsNone(self.decide(cmd, "default"), cmd)


class TestItDoesNotOverreach(FloorCase):
    """Reading and inspecting must stay free, unattended included — an autonomous run
    legitimately needs to know what the tags ARE."""

    def test_listing_tags_is_allowed(self):
        for cmd in ("git tag", "git tag -l", "git tag --list", "git tag -l 'v0.4*'"):
            self.assertIsNone(self.decide(cmd, UNATTENDED), cmd)

    def test_deleting_a_local_tag_is_allowed(self):
        """Local only — it publishes nothing."""
        self.assertIsNone(self.decide("git tag -d v9.9.9", UNATTENDED))

    def test_an_ordinary_push_is_allowed(self):
        self.assertIsNone(self.decide("git push origin main", UNATTENDED))

    def test_reading_forge_state_is_allowed(self):
        for cmd in ("gh release list", "gh pr view 1", "gh run list", "glab mr list"):
            self.assertIsNone(self.decide(cmd, UNATTENDED), cmd)

    def test_prose_mentioning_a_tag_is_not_a_tag(self):
        self.assertIsNone(
            self.decide('git commit -m "prepare git tag v9.9.9 notes"', UNATTENDED))


class TestCharterOwnLandingIsOnTheFloor(FloorCase):
    """`charter change land` merges one member of a cross-repo change into a repository,
    which is the same act `gh pr merge` is — so it sits on the same floor.

    **The split is attended versus unattended, and it is the standing rule read in the two
    modes the harness reports.** Attended, an agent may land one member, because that is
    the merge the standing rule already permits for a single repo. Unattended it may not
    land at all, because that is where this floor already sat: the project's line runs
    between *opening* a request and *merging* one, and `gh pr create` is deliberately
    absent from `_PUBLISH_FORGE`. Without this entry, `charter change land` would be a
    documented way around a floor charter itself wrote.
    """

    def test_landing_a_member_unattended_is_denied(self):
        self.assertEqual("deny", self.decide(
            "charter change land component-api-2 --repo charter", UNATTENDED))

    def test_the_denial_names_the_command_and_a_remedy(self):
        r = run_hook(hooks.pretooluse,
                     {"tool_input": {"command": "charter change land c --repo r"},
                      "cwd": str(self.tmp), "session_id": "s",
                      "permission_mode": UNATTENDED})
        reason = r["hookSpecificOutput"]["permissionDecisionReason"]
        self.assertIn("charter change land", reason)
        self.assertIn("attended", reason)

    def test_the_entry_is_what_answers_and_not_the_branch_that_reaches_it(self):
        """**Step 3b, and it is the mutation that would have caught a dead line.** The
        `_PUBLISH_FORGE` lookup used to live under `elif base in ("gh", "glab")`, so
        adding the tuple alone would have shipped a set member nothing could reach and a
        mutation deleting it would have stayed green. Asked here of the SET, one layer
        below the hook, where nothing else can answer: delete the tuple and this is red
        while every gh/glab case above stays green."""
        self.assertIn(("charter", "change", "land"), hooks._PUBLISH_FORGE)

    def test_the_reader_reaches_charter_and_not_only_the_two_forge_clis(self):
        """The other half of the same pair. Revert the base check to `("gh", "glab")` and
        keep the entry, and THIS is red while the assertion above stays green — which is
        why both exist rather than one."""
        self.assertEqual("deny", self.decide("charter change land c --repo r", UNATTENDED))

    def test_the_pre_rename_binary_is_the_same_command(self):
        """`edm` is charter's pre-rename name. `hooks._CHARTER_PROGS` keeps it for the
        reason a security guard keeps an alternative: one extra string against a silent gap
        on a machine still running the old binary."""
        self.assertEqual("deny", self.decide("edm change land c --repo r", UNATTENDED))

    def test_the_module_spelling_is_the_same_command_too(self):
        """`python3 -m charter change land` puts charter's own NAME in argv instead of in
        the program, and `-m` drops out with the other flags — so the verb pair sits one
        place further along. A guard that matched one spelling and missed the other is a
        guard with a `python3 -m` for a bypass."""
        self.assertEqual("deny", self.decide(
            "python3 -m charter change land c --repo r", UNATTENDED))

    def test_landing_attended_is_untouched(self):
        """The standing rule: implement, PR, merge — but never release alone. A merge is
        undone by another commit; this floor is about the mode, not about the verb."""
        for mode in ("default", "auto", None):
            self.assertIsNone(
                self.decide("charter change land c --repo r", mode), repr(mode))

    def test_every_other_change_verb_is_untouched_even_unattended(self):
        """A cross-repo landing is N merges, each individually revertible — it is not a
        release, and treating the whole surface as one would be stricter than the standing
        rule and would push agents onto raw `gh pr merge`, outside every guard here."""
        for cmd in ("charter change show component-api-2",
                    "charter change list",
                    "charter change create c --why x",
                    "charter change add c r",
                    "charter change push c",
                    "charter change revert c"):
            self.assertIsNone(self.decide(cmd, UNATTENDED), cmd)

    def test_an_unrelated_charter_command_is_untouched(self):
        for cmd in ("charter doctor", "charter workspace status", "charter land"):
            self.assertIsNone(self.decide(cmd, UNATTENDED), cmd)


class TestItIsAFloorNotANudge(FloorCase):
    """`bypassPermissions` means stop asking me, not stop knowing things."""

    def test_the_other_floor_guards_still_deny(self):
        self.assertEqual("deny", self.decide("git clone git@github.com:a/b.git", UNATTENDED))

    def test_it_outranks_the_clone_nudge(self):
        """In a clone, unattended, the release verb must DENY rather than fall through to
        the nudge's allow — that fall-through is the whole bug."""
        self.assertEqual("deny", self.decide("git tag v9.9.9", UNATTENDED, in_clone=True))


if __name__ == "__main__":
    unittest.main()
