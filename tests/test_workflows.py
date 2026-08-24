"""Every action CI runs is named by something nobody else can move (#443).

`release.yml`'s `publish` job holds `id-token: write`, and PyPI's Trusted Publishing
verifies the token minted there as charter's genuine publisher — the claim is real, so
whatever code runs in that job can publish charter. A `uses:` on a floating ref is that
code, chosen by whoever can move the ref. `pypa/gh-action-pypi-publish@release/v1` is a
BRANCH head, so it is a force-push away from being different code; `@v4` is a tag its owner
can retarget. That is the whole of CVE-2025-30066.

**The property, and it is not "the string does not say v4".** A ref is acceptable when
naming it a second time cannot get you different bytes: a full commit SHA (a content
address), an image digest, or a path inside this repository, which moves only with a commit
to this repository. Everything else is a promise by a third party, whatever it is spelled.

**What this cannot check**, stated because a guard that overclaims is the defect twice
over. That a pinned SHA is a real commit of the repository beside it, or that its trailing
`# v1.2.3` is that commit's tag — both need the network, and no test in this suite makes a
network call; a wrong-but-well-formed SHA fails in CI on the next run, loudly, which is the
failure mode you want from an unresolvable ref. That a ref which *is* 40 hex characters is
an object name rather than a branch somebody named after one — GitHub resolves it as a
commit, and the case is theoretical, but it is not excluded here. And nothing at all about
a `run:` step that pipes a script off the internet: pinning is not a defence against one,
and there is none in these files today.

**Fail-closed on anything unparsed.** The scanner counts `uses:` tokens at the byte level
and requires that it produced a ref for every one of them, so a spelling it does not
understand — a flow mapping, a folded scalar, a new file type — fails the suite instead of
being silently skipped. That is the shape of failure this repository keeps re-learning: a
guard that scans for one spelling passes happily on the next one.
"""

from __future__ import annotations

import os
import re
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
GITHUB = REPO / ".github"

#: A full git object name: the only ref that is a content address rather than a promise.
#: Lowercase, because that is what every tool that prints one emits — an uppercase variant
#: would be a second spelling of the same pin, and two spellings is how audits drift.
_SHA = re.compile(r"^[0-9a-f]{40}$")

#: An OCI digest, for a `docker://` step. None today; the predicate answers for one anyway,
#: so adding one does not require also remembering to teach this file about it.
_DIGEST = re.compile(r"^sha256:[0-9a-f]{64}$")

#: `uses:` as a block-mapping key, with or without a leading sequence dash.
_USES = re.compile(r"^\s*(?:-\s+)?uses\s*:\s*(.+?)\s*$")


def _strip_comment(line: str) -> str:
    """Everything before the first ``#``.

    Crude on purpose. It is applied to the ref scan AND to the token count, so the two
    always see the same bytes — a `uses:` inside a comment is invisible to both rather than
    a phantom the parity check would trip over. The trailing `# v4.4.0` on every pin is
    exactly what it is here to remove.
    """
    return line.split("#", 1)[0]


#: Directories with nothing CI runs in them, and every reason not to walk them: git's
#: object store, and the worktrees other agents are working in — each a full second copy of
#: this repository, whose `.github/` is not the one that runs.
_PRUNE = {".git", "__pycache__", "node_modules", "dist", "worktrees", ".venv"}


def _yaml_files() -> list[Path]:
    """Every file CI can take a `uses:` from, found by walking rather than by name.

    Two populations, and missing either one is a hole:

    * everything under `.github/` — a list of the two workflows that exist today is a list
      somebody adds a third file beside;
    * every `action.yml` / `action.yaml` in the tree, because a **local** composite action
      is `uses: ./tools/thing`, which this file rightly calls immutable — it moves only with
      a commit here — and whatever *that* file runs is executing in the calling job with the
      calling job's token. Trusting the path and never reading the file is how a pinned
      workflow ends up running `evil/action@main` one hop away.
    """
    found = {p for p in GITHUB.rglob("*") if p.is_file() and p.suffix in (".yml", ".yaml")}
    for dirpath, dirnames, filenames in os.walk(REPO):
        dirnames[:] = [d for d in dirnames if d not in _PRUNE]
        for name in filenames:
            if name in ("action.yml", "action.yaml"):
                found.add(Path(dirpath) / name)
    return sorted(found)


def _scan(path: Path) -> tuple[list[tuple[int, str]], int]:
    """``([(line number, ref)], number of `uses:` tokens seen)``."""
    refs: list[tuple[int, str]] = []
    tokens = 0
    for n, raw in enumerate(path.read_text().splitlines(), 1):
        code = _strip_comment(raw)
        tokens += code.count("uses:")
        m = _USES.match(code)
        if m:
            refs.append((n, m.group(1)))
    return refs, tokens


def immutable(ref: str) -> bool:
    """Can this ref be made to mean different bytes by someone other than us?

    True when it cannot: a path inside this repository, an image digest, or `owner/repo`
    (optionally with a subdirectory) at a full commit SHA.
    """
    ref = ref.strip()
    if ref.startswith("'") and ref.endswith("'") and len(ref) >= 2:
        ref = ref[1:-1]
    elif ref.startswith('"') and ref.endswith('"') and len(ref) >= 2:
        ref = ref[1:-1]
    if ref.startswith("./") or ref.startswith("../"):
        return True                       # in this tree; moves only with a commit here
    if ref.startswith("docker://"):
        _, _, tail = ref.partition("docker://")
        _, sep, digest = tail.rpartition("@")
        return bool(sep) and bool(_DIGEST.match(digest))
    owner, sep, rev = ref.rpartition("@")
    if not sep or owner.count("/") < 1:
        return False                      # no ref at all, or not `owner/repo`
    return bool(_SHA.match(rev))


class TheScannerSeesEverything(unittest.TestCase):
    """A checker that found nothing would pass every test below it."""

    def test_the_workflows_are_where_this_thinks_they_are(self):
        names = {p.relative_to(REPO).as_posix() for p in _yaml_files()}
        self.assertIn(".github/workflows/release.yml", names)
        self.assertIn(".github/workflows/test.yml", names)

    def test_it_finds_actions_to_check_at_all(self):
        found = sum(len(_scan(p)[0]) for p in _yaml_files())
        self.assertGreater(found, 0, "no `uses:` found anywhere — the scanner is broken, "
                                     "and a broken scanner passes every other test here")

    def test_every_uses_token_produced_a_ref(self):
        """Fail-closed. A `uses:` written in a shape the scanner does not parse — a flow
        mapping, a folded scalar — must fail the suite, not be skipped by it."""
        for p in _yaml_files():
            refs, tokens = _scan(p)
            with self.subTest(file=p.relative_to(REPO).as_posix()):
                self.assertEqual(len(refs), tokens,
                                 f"{tokens} `uses:` tokens but {len(refs)} parsed — an "
                                 f"unrecognised spelling in {p.name} would go unchecked")


class EveryActionIsPinnedToSomethingImmutable(unittest.TestCase):
    def test_no_workflow_runs_a_ref_its_owner_can_move(self):
        for p in _yaml_files():
            for line, ref in _scan(p)[0]:
                with self.subTest(file=p.relative_to(REPO).as_posix(), line=line):
                    self.assertTrue(
                        immutable(ref),
                        f"{ref} is a ref somebody else can move. Pin it to a full commit "
                        f"SHA with the tag in a trailing comment — see release.yml's "
                        f"header for why (#443).")

    def test_every_pin_says_which_version_it_is(self):
        """The SHA is the security property; the trailing tag is what makes it
        maintainable. Without it nobody can tell whether the pin is a year stale, and a pin
        nobody dares move is how an unpatched action outlives the reason it was pinned."""
        for p in _yaml_files():
            for n, raw in enumerate(p.read_text().splitlines(), 1):
                m = _USES.match(_strip_comment(raw))
                if not m or not _SHA.match(m.group(1).rpartition("@")[2]):
                    continue
                with self.subTest(file=p.relative_to(REPO).as_posix(), line=n):
                    _, sep, comment = raw.partition("#")
                    self.assertTrue(sep and comment.strip(),
                                    "a SHA pin with no `# <tag>` beside it")

    def test_something_is_watching_the_pins_for_updates(self):
        """A pin freezes a security fix out as effectively as it freezes an attacker out.
        Dependabot opens the pull request that moves one; a human still merges it."""
        cfg = GITHUB / "dependabot.yml"
        self.assertTrue(cfg.is_file(), "SHA-pinned actions with nothing watching them")
        self.assertIn("github-actions", cfg.read_text())


class TheRuleIsAboutMovability(unittest.TestCase):
    """The predicate itself, on the spellings a "does it say v4" check would wave through.

    Not a denylist of bad strings — that is the guard this repository has now watched fail
    four times. Each case below asks the same one question: *given only this ref, can the
    bytes it resolves to change without a commit landing here?*
    """

    def test_a_full_sha_is_the_only_accepted_third_party_ref(self):
        self.assertTrue(immutable("actions/checkout@" + "a" * 40))
        self.assertTrue(immutable("owner/repo/sub/dir@" + "0" * 40))

    def test_near_misses_are_not_pins(self):
        for ref in (
            "actions/checkout@v4",                     # a tag its owner can retarget
            "actions/checkout@v4.4.0",                 # an exact tag is still a tag
            "pypa/gh-action-pypi-publish@release/v1",  # a branch head
            "actions/checkout@main",
            "actions/checkout@" + "a" * 39,            # short
            "actions/checkout@" + "a" * 41,            # long
            "actions/checkout@" + "A" * 40,            # not the spelling any tool emits
            "actions/checkout@" + "g" * 40,            # 40 chars, not hex
            "actions/checkout",                        # no ref at all
            "actions/checkout@${{ env.PIN }}",         # resolved at run time, elsewhere
            "docker://alpine:3.20",                    # a tag on an image is a tag
            "docker://alpine@sha256:" + "a" * 63,      # short digest
        ):
            with self.subTest(ref=ref):
                self.assertFalse(immutable(ref), ref)

    def test_a_local_action_is_ours_to_move_and_nobody_elses(self):
        self.assertTrue(immutable("./.github/actions/setup"))
        self.assertTrue(immutable("./.github/workflows/reusable.yml"))

    def test_an_image_digest_is_a_content_address_too(self):
        self.assertTrue(immutable("docker://alpine@sha256:" + "b" * 64))

    def test_quoting_does_not_change_the_answer(self):
        """YAML lets the same value be written three ways, and a scanner that only knows
        one of them is a scanner with two holes in it."""
        self.assertTrue(immutable('"actions/checkout@' + "a" * 40 + '"'))
        self.assertFalse(immutable("'actions/checkout@v4'"))


if __name__ == "__main__":
    unittest.main()
