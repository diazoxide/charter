"""Every action CI runs is named by something nobody else can move (#443).

`release.yml`'s `publish` job holds `id-token: write`, and PyPI's Trusted Publishing
verifies the token minted there as charter's genuine publisher — the claim is real, so
whatever code runs in that job can publish charter. A `uses:` on a floating ref is that
code, chosen by whoever can move the ref. `pypa/gh-action-pypi-publish@release/v1` is a
BRANCH head, so it is a force-push away from being different code; `@v4` is a tag its owner
can retarget. That is the whole of CVE-2025-30066.

**The property, and it is not "the string does not say v4".** A reference is acceptable
when naming it a second time cannot get you different bytes: a full commit SHA (a content
address), an image digest, or a path to a file **tracked in this repository**, which moves
only with a commit here. Everything else is a promise by a third party, whatever it is
spelled — and "in the working tree" is not the same claim as "tracked": `./node_modules/x`
is a path in this tree that an `npm install` rewrites. `uses:` is not the only key that
names somebody else's code, so `container:`/`services:`/`runs.image` are held to the same
rule: `image: node:18` runs third-party bytes inside the job exactly as a step does.

**And a container key has two shapes, which is the spelling problem again one level in.**
`container:` takes a mapping with an `image:` in it *or* a bare scalar — GitHub: "when you
only specify a container image, you can omit the `image` keyword" — and `services:` gives
every child the same pair. A key set of `("uses", "image")` knew the long form only, so
`container: evil/img:latest` on the `publish` job read as no reference at all and left this
whole file green. Both shapes are read now, and `Ref.kind` says what a reference *is*
(`ACTION` or `IMAGE`) so the rule tests select on that rather than on the key that spelled
it. The corpus below holds every shape, in both directions.

**Why this reads YAML instead of grepping for `uses:`.** The first version of this file
matched the key with `^\\s*(?:-\\s+)?uses\\s*:` and counted the literal bytes `uses:` as its
fail-closed cross-check. Both are one spelling of the key. YAML has many, and GitHub loads
them all: `- "uses": evil/action@main` contains neither the bare key nor the byte sequence
`uses:`, parses to a genuine step, and was invisible to every test below. So was
`- 'uses':`, so was the explicit key `- ? uses` / `: …`, and so was `"\\x75ses"`. Matching
one spelling of a thing is the failure this repository keeps re-learning; the answer is to
read the structure, not the bytes.

So `scan_text` is a small YAML reader for the **subset this repository writes** — block
mappings and sequences, plain and quoted scalars with real escape handling, block scalars
skipped as the opaque text they are. Anything outside that subset is not guessed at and not
ignored: it raises `Unparsed`, which fails the suite by name. A flow mapping, an anchor, an
alias, a merge key, a tag, an explicit key, a document marker, a `uses:` whose value is on
another line — every one of those can carry a step past a line-oriented reader, so every
one of them stops this test until a person either rewrites the file in the subset or
teaches the reader that construct on purpose. `TheKeyHasManySpellings` is the corpus that
holds it to that, and it asserts *which* outcome each spelling gets, so no case can pass
because some unrelated check happened to trip.

**Nothing is trusted for being on disk.** The set of files to read is everything under
`.github/` plus every `action.y*ml` in `git ls-files`, and every local `uses: ./x` must
resolve to a file in that index — a local composite action runs in the calling job with the
calling job's token, so its own `uses:` lines are checked exactly like the caller's, and a
path this repository does not track is not "a path that moves only with a commit here". The
first version walked the tree with a hardcoded list of directories to skip (`node_modules`,
`dist`, `.venv`), which is the same mistake in a second place: `uses: ./node_modules/probe`
was accepted as immutable while the skip list guaranteed its `action.yml` was never read.
There is deliberately no second walk from the refs: GitHub resolves a local `uses:` to a
directory's `action.y*ml` or to a file under `.github/workflows/`, both of which the seed
list already holds, and a test pins that invariant rather than leaving it as a comment.

**What this cannot check**, stated because a guard that overclaims is the defect twice
over. That a pinned SHA is a real commit of the repository beside it, or that its trailing
`# v1.2.3` is that commit's tag — both need the network, and no test in this suite makes a
network call; a wrong-but-well-formed SHA fails in CI on the next run, loudly, which is the
failure mode you want from an unresolvable ref. That a ref which *is* 40 hex characters is
an object name rather than a branch somebody named after one — GitHub resolves it as a
commit, and the case is theoretical, but it is not excluded here. That a tracked local
action stays benign — it is charter's own code, reviewed the way the rest of it is. And
nothing at all about a `run:` step that pipes a script off the internet: pinning is not a
defence against one, and there is none in these files today.

**What this checks is the refs written in this repository, one hop deep into local
actions and no hops at all into remote ones** (#473). `uses: owner/action@<sha>` pins that
action's tree; it does not pin what that tree then names. A Docker action builds from a
`Dockerfile` whose `FROM` is a tag its publisher rewrites, and a composite action has its
own `uses:` lines — neither is in this repository, neither can be read without the network,
and both run in the job holding `id-token: write`. So the property this file enforces is
"every reference charter *writes* is a content address", which is narrower than "every byte
that runs in the publishing job is pinned", and the gap between those two sentences is
where the next look goes.

**The next place to look**, since that is the question this file exists to keep asking. The
transitive hop above is the first. Then the two mismatches a reader like this always has
with the real one: what counts as a line break — see
`ALineBreakIsTheOtherHalfOfTheSpellingProblem`, which shows the mismatch runs the safe way
round — and a ref that is well formed here and means something else on the runner, such as a
branch somebody named after a 40-character hex string. And last, this reader judges a key by
its **name**, not by its position in GitHub's workflow schema, so an action input that
happens to be called `container:` reads here as a job container. That is a false alarm
somebody resolves in one look, which is the direction this file always takes when it has to
guess — all of it stated rather than quietly assumed.
"""

from __future__ import annotations

import os
import re
import subprocess
import tempfile
import unittest
from pathlib import Path
from typing import NamedTuple

REPO = Path(__file__).resolve().parents[1]
GITHUB = REPO / ".github"

#: A full git object name: the only ref that is a content address rather than a promise.
#: Lowercase, because that is what every tool that prints one emits — an uppercase variant
#: would be a second spelling of the same pin, and two spellings is how audits drift.
_SHA = re.compile(r"^[0-9a-f]{40}$")

#: An OCI digest, for a `docker://` step or a `container:`/`services:` image.
_DIGEST = re.compile(r"^sha256:[0-9a-f]{64}$")

#: The keys that name code somebody else wrote. `uses` is a step or a reusable workflow;
#: `image` is `jobs.<id>.container.image`, `services.<id>.image`, or a Docker action's
#: `runs.image` — all of which run third-party bytes inside the job just as a step does.
_CODE_KEYS = ("uses", "image")

#: The key that introduces a job container. It takes **two shapes**, and that is the whole
#: point of naming it separately: a mapping with an `image:` in it, which `_CODE_KEYS`
#: already reads, or a bare scalar — `container: node:18` — which GitHub documents as the
#: shorthand ("when you only specify a container image, you can omit the `image` keyword").
#: A key set that knew only the long form knew one spelling of the key again.
_CONTAINER = "container"

#: The key whose every child is a container, in the same two shapes. `services.<id>.image`
#: is the documented long form; `services:\n  redis: redis:7` is the string shorthand the
#: workflow schema and the runner both accept, resolved exactly like `container:`.
_SERVICES = "services"

#: What a reference *is*, as opposed to what key spelled it. `immutable` judges an action
#: ref; `immutable_image` judges an image. Tests select on this, not on the key name, so a
#: new spelling of either kind is judged the moment the reader emits it.
ACTION, IMAGE = "action", "image"

#: A block-scalar header: `|`, `>`, with any chomping/indent indicator and a comment.
_BLOCK_HEADER = re.compile(r"^[|>][-+0-9]*\s*(?:#.*)?$")

#: A local reference: a path, resolved by GitHub against the repository root.
_LOCAL = re.compile(r"^\.{1,2}/")


class Unparsed(Exception):
    """A construct the reader does not model.

    Never swallowed and never guessed at. It carries a line number and a name for the
    construct, and it fails the suite — because a YAML shape this file does not understand
    is exactly where the next step hides.
    """

    def __init__(self, line: int, what: str) -> None:
        super().__init__(f"line {line}: {what}")
        self.line = line
        self.what = what


# --------------------------------------------------------------------------- scalars

#: YAML 1.2's double-quoted escapes. Present so that `"\x75ses"` — which is the key `uses`
#: to every real parser — is the key `uses` here too.
_ESCAPES = {
    "0": "\0", "a": "\a", "b": "\b", "t": "\t", "\t": "\t", "n": "\n", "v": "\v",
    "f": "\f", "r": "\r", "e": "\x1b", " ": " ", '"': '"', "/": "/", "\\": "\\",
    "N": "\x85", "_": "\xa0", "L": "\u2028", "P": "\u2029",
}
_HEX = {"x": 2, "u": 4, "U": 8}

#: Characters that begin a YAML construct this reader does not model. Each one is a way to
#: introduce a mapping key out of line-oriented view, so each one stops the suite.
_INDICATORS = {
    "{": "a flow mapping",
    "[": "a flow collection in key position",
    "&": "an anchor",
    "*": "an alias",
    "!": "a tag",
    "?": "an explicit key",
    "%": "a directive",
    "@": "a reserved indicator",
    "`": "a reserved indicator",
}


def _double_quoted(s: str, i: int, line: int) -> tuple[str, int]:
    out: list[str] = []
    i += 1
    while i < len(s):
        c = s[i]
        if c == '"':
            return "".join(out), i + 1
        if c != "\\":
            out.append(c)
            i += 1
            continue
        i += 1
        if i >= len(s):
            raise Unparsed(line, "a double-quoted scalar folded onto the next line")
        e = s[i]
        if e in _HEX:
            width = _HEX[e]
            digits = s[i + 1:i + 1 + width]
            if len(digits) != width or any(d not in "0123456789abcdefABCDEF" for d in digits):
                raise Unparsed(line, f"a malformed \\{e} escape")
            out.append(chr(int(digits, 16)))
            i += 1 + width
        elif e in _ESCAPES:
            out.append(_ESCAPES[e])
            i += 1
        else:
            raise Unparsed(line, f"an escape this reader does not know: \\{e}")
    raise Unparsed(line, "an unterminated double-quoted scalar")


def _single_quoted(s: str, i: int, line: int) -> tuple[str, int]:
    out: list[str] = []
    i += 1
    while i < len(s):
        if s[i] == "'":
            if s[i + 1:i + 2] == "'":
                out.append("'")
                i += 2
                continue
            return "".join(out), i + 1
        out.append(s[i])
        i += 1
    raise Unparsed(line, "an unterminated single-quoted scalar")


def _plain(s: str, i: int) -> tuple[str, int]:
    """A plain scalar, stopping where YAML stops it: at `: `, at ` #`, or at end of line.

    A bare `:` that is not followed by a space does not end it — `https://example` is one
    scalar — and a `#` that is not preceded by a space does not start a comment, which is
    why `SECURITY.md#reporting` survives intact.
    """
    start = i
    while i < len(s):
        if s[i] == ":" and (i + 1 >= len(s) or s[i + 1] in " \t"):
            break
        if s[i] == "#" and i > start and s[i - 1] in " \t":
            break
        i += 1
    return s[start:i].rstrip(), i


def _scalar(s: str, i: int, line: int) -> tuple[str, int]:
    if s[i] == '"':
        return _double_quoted(s, i, line)
    if s[i] == "'":
        return _single_quoted(s, i, line)
    return _plain(s, i)


def _trailing(s: str, i: int, line: int) -> None:
    """Assert nothing but blanks and a comment follow — the line held one value, not two."""
    rest = s[i:].strip()
    if rest and not rest.startswith("#"):
        raise Unparsed(line, "two values on one line, or a scalar this reader misread")


# --------------------------------------------------------------------------- the reader

class Ref(NamedTuple):
    line: int
    key: str
    ref: str
    kind: str   # ACTION or IMAGE — what it names, not which key spelled it


def scan_text(text: str) -> list[Ref]:
    """Every reference to somebody else's code in a YAML document written in the subset.

    An action ref (`uses:`) or an image, in each of the shapes GitHub accepts an image in:
    `image:`, a scalar `container:`, and a scalar under `services:`.

    Raises `Unparsed` on the first construct outside the subset. There is no "skip what I
    do not understand" path, by design: that path is the bypass.
    """
    found: list[Ref] = []
    block_indent: int | None = None
    #: The column of a `container:`/`services:`/`services.<id>:` key whose value is a
    #: nested node, and the column of `services:`'s children. Both exist so a *scalar*
    #: written under such a key — the shorthand, wrapped onto the following line — is seen
    #: rather than dropped as an anonymous value.
    image_block: int | None = None
    services_col: int | None = None
    service_id_col: int | None = None

    for line, raw in enumerate(text.splitlines(), 1):
        if block_indent is not None:
            if not raw.strip() or len(raw) - len(raw.lstrip(" ")) > block_indent:
                continue                      # opaque block-scalar content
            block_indent = None

        if "\t" in raw:
            raise Unparsed(line, "a tab character")
        if raw.strip() in ("---", "..."):
            raise Unparsed(line, "a document marker — this reader loads one document")

        indent = i = len(raw) - len(raw.lstrip(" "))
        if i >= len(raw) or raw[i] == "#":
            continue                          # blank or whole-line comment

        # A sequence item's scalar is a value, not the shorthand: `ports:\n  - 6379:6379`
        # lives inside a service block and names no image.
        had_dash = raw[i] == "-" and (i + 1 >= len(raw) or raw[i + 1] == " ")

        # Close every container scope this line has stepped back out of, before anything
        # in it is judged. Scope is indentation, which is what it is in the real parser.
        if image_block is not None and indent <= image_block:
            image_block = None
        if services_col is not None and indent <= services_col:
            services_col = service_id_col = None

        while raw[i] == "-" and (i + 1 >= len(raw) or raw[i + 1] == " "):
            i += 1
            while i < len(raw) and raw[i] == " ":
                i += 1
            if i >= len(raw) or raw[i] == "#":
                break
        if i >= len(raw) or raw[i] == "#":
            continue                          # a lone `-`: the node is on the next line

        key_col = i
        if raw[i] in _INDICATORS:
            raise Unparsed(line, _INDICATORS[raw[i]])
        if raw[i] in "|>":
            raise Unparsed(line, "a block scalar in key position")

        name, i = _scalar(raw, i, line)
        j = i
        while j < len(raw) and raw[j] == " ":
            j += 1
        if j >= len(raw) or raw[j] != ":" or (j + 1 < len(raw) and raw[j + 1] not in " \t"):
            if image_block is not None and indent > image_block and not had_dash:
                # A bare scalar inside a `container:`/`services:` node *is* the image —
                # the shorthand, wrapped onto the next line. Dropping it as an anonymous
                # value is how `container:\n  evil/img:latest` would walk past.
                raise Unparsed(line, "a container image on a line of its own")
            _trailing(raw, i, line)           # a sequence item's own scalar value
            continue

        if name == "<<":
            raise Unparsed(line, "a merge key")

        # Which of the two container shapes is this key in? `container:` and every direct
        # child of `services:` take an image inline *or* a mapping holding `image:`, and
        # both shapes have to be judged — knowing only the mapping was the last bypass.
        is_service_id = (services_col is not None and key_col > services_col
                         and (service_id_col is None or key_col == service_id_col))
        if is_service_id:
            service_id_col = key_col
        names_image = name == _CONTAINER or is_service_id

        value = raw[j + 1:]
        k = len(value) - len(value.lstrip(" "))
        tail = value[k:]

        if not tail or tail.startswith("#"):
            if name in _CODE_KEYS:
                raise Unparsed(line, f"a `{name}:` whose value is not on its own line")
            if names_image or name == _SERVICES:
                image_block = key_col         # a nested node: watch it for a bare scalar
            if name == _SERVICES:
                services_col, service_id_col = key_col, None
            continue                          # a nested block follows
        if _BLOCK_HEADER.match(tail):
            if name in _CODE_KEYS or names_image or name == _SERVICES:
                raise Unparsed(line, f"a `{name}:` written as a block scalar")
            block_indent = key_col
            continue
        if tail[0] == "[":
            if name in _CODE_KEYS or names_image or name == _SERVICES:
                # A flow sequence is not a shape any of these keys takes, so it is the
                # third way to write a value this reader would otherwise walk past.
                raise Unparsed(line, f"a `{name}:` whose value is a flow sequence")
            close = tail.find("]")
            if close < 0:
                raise Unparsed(line, "a flow collection spanning lines")
            if "{" in tail[:close] or ":" in tail[:close]:
                raise Unparsed(line, "a mapping inside a flow sequence")
            _trailing(tail, close + 1, line)
            continue                          # a flow sequence of scalars: no keys in it
        if tail[0] in _INDICATORS:
            raise Unparsed(line, _INDICATORS[tail[0]])

        if name == _SERVICES:
            raise Unparsed(line, "a `services:` whose value is not a mapping")

        ref, end = _scalar(tail, 0, line)
        _trailing(tail, end, line)
        if ref and name in _CODE_KEYS:
            found.append(Ref(line, name, ref, IMAGE if name == "image" else ACTION))
        elif ref and names_image:
            found.append(Ref(line, name, ref, IMAGE))

    return found


# --------------------------------------------------------------------------- the rule

def _unquote(ref: str) -> str:
    ref = ref.strip()
    if len(ref) >= 2 and ref[0] == ref[-1] and ref[0] in "'\"":
        return ref[1:-1]
    return ref


def is_local(ref: str) -> bool:
    """Does this reference name a path in this repository rather than somebody's repo?"""
    return bool(_LOCAL.match(_unquote(ref)))


def local_target(ref: str) -> str | None:
    """The repo-relative path a local reference names, or None if it leaves the tree.

    `uses: ./x` is resolved by GitHub against the repository root, not against the file
    holding it. A path that normalises to something outside the root is not "in this tree"
    however it is spelled, so it gets no immutability credit here.
    """
    rel = os.path.normpath(_unquote(ref))
    if os.path.isabs(rel) or rel == ".." or rel.startswith(".." + os.sep):
        return None
    return Path(rel).as_posix()


def immutable(ref: str) -> bool:
    """Can this third-party reference be made to mean different bytes by its owner?

    True when it cannot: an image digest, or `owner/repo` (optionally with a subdirectory)
    at a full commit SHA. Local references are not this function's question — they are
    `local_target`'s, because the answer depends on whether the file is tracked here.
    """
    ref = _unquote(ref)
    if ref.startswith("docker://"):
        _, _, tail = ref.partition("docker://")
        _, sep, digest = tail.rpartition("@")
        return bool(sep) and bool(_DIGEST.match(digest))
    owner, sep, rev = ref.rpartition("@")
    if not sep or owner.count("/") < 1:
        return False                      # no ref at all, or not `owner/repo`
    return bool(_SHA.match(rev))


def immutable_image(ref: str) -> bool:
    """A container image is a content address only when it names a digest.

    `node:18` is a tag its publisher rewrites, exactly like `@v4` — and `container.image`
    runs in the job beside whatever token the job holds.
    """
    ref = _unquote(ref)
    if ref.startswith("docker://"):
        ref = ref.partition("docker://")[2]
    _, sep, digest = ref.rpartition("@")
    return bool(sep) and bool(_DIGEST.match(digest))


# --------------------------------------------------------------------------- the files

def tracked() -> set[str]:
    """Every path in this repository's index: the set of files that move only with a commit.

    This replaces a hardcoded list of directories to skip. A denylist of `node_modules`,
    `dist`, `.venv` is a guess at where untracked code lives; the index is the answer.
    """
    out = subprocess.run(
        ["git", "-C", str(REPO), "ls-files", "-z"],
        capture_output=True, check=True, text=True)
    return {p for p in out.stdout.split("\0") if p}


def _action_files(rel: str, index: set[str]) -> list[str]:
    """The tracked file(s) a local reference resolves to, in the two shapes GitHub allows.

    A directory holding `action.yml`/`action.yaml`, or a reusable workflow under
    `.github/workflows/`. There is no third shape, and that is load-bearing: both of these
    are already in `seed_files`, so a local reference never names a file this scan would
    otherwise miss and there is no second walk to get wrong. `TheFileListComesFromTheIndex`
    holds that invariant, so widening this function without widening the seed list fails.
    """
    if rel.endswith((".yml", ".yaml")):
        return [rel] if rel in index and rel.startswith(".github/workflows/") else []
    return [c for c in (f"{rel}/action.yml", f"{rel}/action.yaml") if c in index]


def seed_files(root: Path = REPO, index: set[str] | None = None) -> list[Path]:
    """Where a `uses:` can enter CI, from two sources rather than one walk.

    Everything under `.github/` on disk — a workflow added but not yet committed is still a
    workflow somebody is about to run, and a list of the two files that exist today is a
    list somebody adds a third file beside. Plus every tracked `action.y*ml` anywhere in
    the tree, because a composite action runs inside the calling job.

    `root` and `index` are arguments rather than globals so the traversal can be exercised
    against a tree built for the purpose. This repository has no `action.yml` today, and a
    guard whose only input is a repository that never triggers it is a guard nobody has
    ever seen work.
    """
    index = tracked() if index is None else index
    github = root / ".github"
    found = {p for p in github.rglob("*") if p.is_file() and p.suffix in (".yml", ".yaml")}
    for rel in index:
        if os.path.basename(rel) in ("action.yml", "action.yaml"):
            found.add(root / rel)
    return sorted(found)


def _no_symlink(path: Path) -> None:
    """A tracked symlink moves with a commit; the bytes it points at need not.

    Git stores a symlink as its target string, so `./tools/act` can be a committed link
    into `node_modules` — immutable as a name, and not as code.
    """
    if os.path.realpath(path) != str(path):
        raise Unparsed(0, f"{path} is reached through a symlink")


class Closure(NamedTuple):
    refs: list[tuple[str, Ref]]        # (repo-relative file, ref)
    local: list[tuple[str, Ref, str]]  # (file, ref, resolved repo-relative target)


def closure(root: Path = REPO, index: set[str] | None = None) -> Closure:
    """Every reference CI can reach, and every local reference's resolved target.

    A local composite action runs inside the calling job with the calling job's token, so
    its own `uses:` lines are checked exactly like the caller's — they are here because
    every file a local reference can name is already in `seed_files`, not because this
    function walks a second time. `_action_files` is what makes that true.
    """
    index = tracked() if index is None else index
    refs: list[tuple[str, Ref]] = []
    local: list[tuple[str, Ref, str]] = []

    for path in seed_files(root, index):
        _no_symlink(path)
        name = path.relative_to(root).as_posix()
        for ref in scan_text(path.read_text()):
            refs.append((name, ref))
            if ref.key == "uses" and is_local(ref.ref):
                local.append((name, ref, local_target(ref.ref) or ref.ref))
    return Closure(refs, local)


# --------------------------------------------------------------------------- tests

class TheScannerSeesEverything(unittest.TestCase):
    """A checker that found nothing would pass every test below it."""

    def test_the_workflows_are_where_this_thinks_they_are(self):
        names = {p.relative_to(REPO).as_posix() for p in seed_files()}
        self.assertIn(".github/workflows/release.yml", names)
        self.assertIn(".github/workflows/test.yml", names)

    def test_it_finds_actions_to_check_at_all(self):
        self.assertGreater(len(closure().refs), 0,
                           "no `uses:` found anywhere — the scanner is broken, and a "
                           "broken scanner passes every other test here")

    def test_every_file_is_written_in_the_subset_this_reader_understands(self):
        """Fail-closed, and this is the whole of it. There is no path where a construct is
        skipped: anything the reader does not model raises, so a shape that could carry a
        step past it stops the suite instead of being waved through by it."""
        for path in seed_files():
            with self.subTest(file=path.relative_to(REPO).as_posix()):
                try:
                    scan_text(path.read_text())
                except Unparsed as exc:
                    self.fail(f"{path.relative_to(REPO)}: {exc}. Rewrite it in the subset "
                              f"tests/test_workflows.py reads, or teach the reader this "
                              f"construct deliberately — do not delete the check.")

    def test_it_reads_the_pins_that_are_actually_in_release_yml(self):
        """Anchored to the file, so a reader that silently stops finding things is caught.
        `publish` is the job holding `id-token: write`."""
        text = (GITHUB / "workflows" / "release.yml").read_text()
        refs = scan_text(text)
        actions = {r.ref.rpartition("@")[0] for r in refs}
        self.assertIn("pypa/gh-action-pypi-publish", actions)
        self.assertIn("actions/download-artifact", actions)
        self.assertEqual(
            [r for r in refs if r.kind == ACTION and not r.ref.rpartition("@")[1]], [],
            "a step in release.yml came out with no ref at all")


class EveryActionIsPinnedToSomethingImmutable(unittest.TestCase):
    def test_no_workflow_runs_a_ref_its_owner_can_move(self):
        found = closure()
        for name, ref in found.refs:
            if ref.kind == IMAGE:
                continue
            if is_local(ref.ref):
                continue
            with self.subTest(file=name, line=ref.line):
                self.assertTrue(
                    immutable(ref.ref),
                    f"{ref.ref} is a ref somebody else can move. Pin it to a full commit "
                    f"SHA with the tag in a trailing comment — see release.yml's "
                    f"header for why (#443).")

    def test_no_job_runs_a_container_image_its_publisher_can_move(self):
        """Selected on what the ref *is*, not on the key that spelled it, so both shapes
        of `container:`/`services:` — the `image:` mapping and the bare-scalar shorthand —
        arrive here without this test naming either one."""
        for name, ref in closure().refs:
            if ref.kind != IMAGE:
                continue
            with self.subTest(file=name, line=ref.line):
                self.assertTrue(
                    immutable_image(ref.ref),
                    f"{ref.ref} is an image tag. `container:`/`services:` run third-party "
                    f"bytes in the job exactly as a step does; name a @sha256: digest.")

    def test_every_local_action_is_a_file_committed_to_this_repository(self):
        """`uses: ./x` earns its immutability from being *tracked*, not from being on disk.
        `./node_modules/probe` is a path in this tree that an `npm install` rewrites."""
        index = tracked()
        for name, ref, target in closure().local:
            with self.subTest(file=name, line=ref.line):
                self.assertTrue(
                    _action_files(target, index),
                    f"{ref.ref} names no file tracked in this repository. A local action "
                    f"runs in the calling job with the calling job's token; if it is not "
                    f"committed here, 'it moves only with a commit here' is false.")

    def test_every_pin_says_which_version_it_is(self):
        """The SHA is the security property; the trailing tag is what makes it
        maintainable. Without it nobody can tell whether the pin is a year stale, and a pin
        nobody dares move is how an unpatched action outlives the reason it was pinned."""
        for path in seed_files():
            lines = path.read_text().splitlines()
            for ref in scan_text(path.read_text()):
                if not _SHA.match(_unquote(ref.ref).rpartition("@")[2]):
                    continue
                with self.subTest(file=path.relative_to(REPO).as_posix(), line=ref.line):
                    _, sep, comment = lines[ref.line - 1].partition("#")
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
    five times. Each case below asks the same one question: *given only this ref, can the
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

    def test_an_image_digest_is_a_content_address_too(self):
        self.assertTrue(immutable("docker://alpine@sha256:" + "b" * 64))
        self.assertTrue(immutable_image("alpine@sha256:" + "b" * 64))
        self.assertFalse(immutable_image("node:18"))
        self.assertFalse(immutable_image("node:18@sha256:" + "b" * 63))

    def test_quoting_does_not_change_the_answer(self):
        """YAML lets the same value be written three ways, and a scanner that only knows
        one of them is a scanner with two holes in it."""
        self.assertTrue(immutable('"actions/checkout@' + "a" * 40 + '"'))
        self.assertFalse(immutable("'actions/checkout@v4'"))

    def test_a_local_reference_is_recognised_however_it_is_spelled(self):
        for ref in ("./.github/actions/setup", '"./tools/act"', "'../x'", "./x/action.yml"):
            with self.subTest(ref=ref):
                self.assertTrue(is_local(ref))
        self.assertFalse(is_local("actions/checkout@" + "a" * 40))

    def test_a_path_that_leaves_the_repository_is_not_in_this_tree(self):
        """`immutable` used to return True for anything starting `../`, on the grounds that
        it was "in this tree". `../../evil` is not in this tree."""
        self.assertIsNone(local_target("../../evil"))
        self.assertIsNone(local_target("./a/../../evil"))
        self.assertEqual(local_target("./.github/actions/setup"), ".github/actions/setup")
        self.assertEqual(local_target('"./tools/act"'), "tools/act")

    def test_a_local_reference_earns_nothing_from_merely_existing_on_disk(self):
        """The bypass this replaces, as a unit: `./node_modules/probe` resolves to a path,
        and that path is not in the index, so it is not a pin."""
        self.assertEqual(local_target("./node_modules/probe"), "node_modules/probe")
        self.assertEqual(_action_files("node_modules/probe", {"README.md"}), [])
        self.assertEqual(_action_files("tools/act", {"tools/act/action.yml"}),
                         ["tools/act/action.yml"])


def _steps(*lines: str) -> str:
    """A step list in the shape release.yml writes one, so indentation is realistic."""
    return "jobs:\n  publish:\n    steps:\n" + "".join(lines)


class TheKeyHasManySpellings(unittest.TestCase):
    """The corpus. Every entry is a way to write `uses` that a real YAML parser resolves to
    the key `uses`, and that the first version of this file could not see.

    Each case asserts **which** outcome it gets — parsed-and-refused, or refused as
    unparsed — because "the suite went red" is not evidence that this check did it. A case
    that flipped from parsed to unparsed would still be safe but would mean the reader had
    quietly stopped reading, and that is worth knowing.
    """

    SEEN = [
        # (name, yaml, the value the reader must extract)
        ("a bare key",
         _steps('      - uses: evil/action@main\n'), "evil/action@main"),
        ("a double-quoted key",
         _steps('      - "uses": evil/action@main\n'), "evil/action@main"),
        ("a single-quoted key",
         _steps("      - 'uses': evil/action@main\n"), "evil/action@main"),
        ("a hex-escaped key",
         _steps('      - "\\x75ses": evil/action@main\n'), "evil/action@main"),
        ("a unicode-escaped key",
         _steps('      - "\\u0075ses": evil/action@main\n'), "evil/action@main"),
        ("space before the colon",
         _steps('      - uses : evil/action@main\n'), "evil/action@main"),
        ("space before the colon, quoted",
         _steps('      - "uses"  : evil/action@main\n'), "evil/action@main"),
        ("the dash on its own line",
         _steps('      -\n        uses: evil/action@main\n'), "evil/action@main"),
        ("a double-quoted value",
         _steps('      - uses: "evil/action@main"\n'), "evil/action@main"),
        ("a single-quoted value",
         _steps("      - uses: 'evil/action@main'\n"), "evil/action@main"),
        ("a value that looks like a comment anchor",
         _steps('      - uses: evil/action@main # v1\n'), "evil/action@main"),
        ("a container image",
         "jobs:\n  publish:\n    container:\n      image: node:18\n", "node:18"),
        # The second shape of the same key, and the bypass this corpus grew for. GitHub:
        # "when you only specify a container image, you can omit the `image` keyword".
        ("a container image in the shorthand",
         "jobs:\n  publish:\n    container: evil/img:latest\n", "evil/img:latest"),
        ("a container image in the shorthand, quoted",
         "jobs:\n  publish:\n    container: 'evil/img:latest'\n", "evil/img:latest"),
        ("a container image in the shorthand, hex-escaped key",
         'jobs:\n  publish:\n    "\\x63ontainer": evil/img:latest\n', "evil/img:latest"),
        ("a container image in the shorthand, docker://",
         "jobs:\n  publish:\n    container: docker://evil/img:latest\n",
         "docker://evil/img:latest"),
        ("a service image in the shorthand",
         "jobs:\n  publish:\n    services:\n      redis: evil/redis:latest\n",
         "evil/redis:latest"),
        ("a service image in the long form",
         "jobs:\n  publish:\n    services:\n      redis:\n        image: evil/redis:latest\n",
         "evil/redis:latest"),
    ]

    REFUSED = [
        ("an explicit key", _steps('      - ? uses\n        : evil/action@main\n'),
         "an explicit key"),
        ("a flow mapping step", _steps('      - {uses: evil/action@main}\n'),
         "a flow mapping"),
        ("a flow mapping value", "jobs:\n  publish:\n    steps: [{uses: e/a@main}]\n",
         "a mapping inside a flow sequence"),
        ("an implicit pair in a flow sequence",
         "jobs:\n  publish:\n    steps: [uses: evil/action@main]\n",
         "a mapping inside a flow sequence"),
        ("an anchored step", _steps('      - &s\n        uses: evil/action@main\n'),
         "an anchor"),
        ("an aliased step", _steps('      - *s\n'), "an alias"),
        ("a merge key", _steps('      - <<: *s\n        name: x\n'), "a merge key"),
        ("a tagged step", _steps('      - !!map\n        uses: evil/action@main\n'),
         "a tag"),
        ("an aliased value", _steps('      - uses: *pin\n'), "an alias"),
        ("a value on the next line", _steps('      - uses:\n          evil/action@main\n'),
         "a `uses:` whose value is not on its own line"),
        ("a folded value", _steps('      - uses: >-\n          evil/action@main\n'),
         "a `uses:` written as a block scalar"),
        ("a literal value", _steps('      - uses: |-\n          evil/action@main\n'),
         "a `uses:` written as a block scalar"),
        ("a second document", "jobs: {}\n---\njobs:\n  p:\n    steps:\n"
                              "      - uses: evil/action@main\n", "a flow mapping"),
        ("a tab where a space belongs", _steps('      -\tuses: evil/action@main\n'),
         "a tab character"),
        ("a folded double-quoted key", _steps('      - "us\\\n        es": e/a@main\n'),
         "a double-quoted scalar folded onto the next line"),
        ("a flow sequence left open", "jobs:\n  p:\n    steps: [\n      uses: e/a@main]\n",
         "a flow collection spanning lines"),
        ("a second colon the reader would have to guess about",
         _steps('      - uses: evil/action@main: extra\n'),
         "two values on one line, or a scalar this reader misread"),
        # The shorthand's own escape hatches: an image is still an image when YAML puts it
        # on the following line or folds it, and neither shape is guessed at.
        ("a shorthand image on the next line",
         "jobs:\n  publish:\n    container:\n      evil/img:latest\n",
         "a container image on a line of its own"),
        ("a shorthand service image on the next line",
         "jobs:\n  publish:\n    services:\n      redis:\n        evil/redis:latest\n",
         "a container image on a line of its own"),
        ("a shorthand image folded",
         "jobs:\n  publish:\n    container: >-\n      evil/img:latest\n",
         "a `container:` written as a block scalar"),
        ("a shorthand image as a literal block",
         "jobs:\n  publish:\n    services:\n      redis: |-\n        evil/redis:latest\n",
         "a `redis:` written as a block scalar"),
        ("a services node that is not a mapping of containers",
         "jobs:\n  publish:\n    services: evil/redis:latest\n",
         "a `services:` whose value is not a mapping"),
        ("a services node folded",
         "jobs:\n  publish:\n    services: >-\n      redis: evil/redis:latest\n",
         "a `services:` written as a block scalar"),
        ("a step ref in a flow sequence", _steps('      - uses: [evil/action@main]\n'),
         "a `uses:` whose value is a flow sequence"),
        ("a shorthand image in a flow sequence",
         "jobs:\n  publish:\n    container: [evil/img:latest]\n",
         "a `container:` whose value is a flow sequence"),
    ]

    def test_every_spelling_of_the_key_is_read_and_refused(self):
        for name, text, expected in self.SEEN:
            with self.subTest(spelling=name):
                refs = scan_text(text)
                self.assertEqual([r.ref for r in refs], [expected],
                                 f"{name}: the reader did not extract the value")
                rule = immutable_image if refs[0].kind == IMAGE else immutable
                self.assertFalse(rule(refs[0].ref),
                                 f"{name}: extracted but judged pinned")

    def test_every_construct_outside_the_subset_stops_the_suite_by_name(self):
        for name, text, expected in self.REFUSED:
            with self.subTest(spelling=name):
                with self.assertRaises(Unparsed) as caught:
                    scan_text(text)
                self.assertEqual(caught.exception.what, expected,
                                 f"{name}: refused, but for a different reason than the "
                                 f"one this case exists to exercise")

    def test_a_digest_pinned_container_survives_both_of_its_shapes(self):
        """The other half for the image keys. `test_a_pinned_step_survives_every_spelling`
        skips them because their rule is `immutable_image`, so without this a reader that
        refused every `container:` outright would pass the corpus above and be useless.
        Both shapes, plus the keys that sit beside an image and are not one."""
        digest = "@sha256:" + "b" * 64
        for name, text in (
            ("the shorthand", f"jobs:\n  p:\n    container: node{digest}\n"),
            ("the mapping", f"jobs:\n  p:\n    container:\n      image: node{digest}\n"),
            ("a service, shorthand",
             f"jobs:\n  p:\n    services:\n      redis: redis{digest}\n"),
            ("a service, mapping and its neighbours",
             f"jobs:\n  p:\n    services:\n      redis:\n        image: redis{digest}\n"
             f"        ports:\n          - 6379:6379\n        env:\n          X: y\n"),
            ("a container beside its options",
             f"jobs:\n  p:\n    container:\n      image: node{digest}\n"
             f"      options: --cpus 2\n      credentials:\n        username: u\n"),
        ):
            with self.subTest(shape=name):
                refs = scan_text(text)
                self.assertEqual([r.ref for r in refs], [
                    ("node" if "service" not in name else "redis") + digest], name)
                self.assertEqual(refs[0].kind, IMAGE, name)
                self.assertTrue(immutable_image(refs[0].ref), name)

    def test_the_scope_of_a_container_block_ends_where_its_indentation_does(self):
        """Both container scopes close on dedent, and that is load-bearing in each
        direction. A bare scalar *inside* a container node is the shorthand and stops the
        suite; the same shape outside one is an ordinary wrapped value this reader has
        always let through, and it must stay let through. A key at the column the service
        ids used is a key, not a fourth service — scope is indentation here because it is
        indentation in the real parser."""
        sha, digest = "a" * 40, "@sha256:" + "b" * 64
        text = (f"jobs:\n  p:\n    container:\n      image: node{digest}\n"
                f"    steps:\n      - uses: actions/checkout@{sha}\n"
                f"      - run: echo hi\n")
        self.assertEqual([(r.kind, r.ref) for r in scan_text(text)],
                         [(IMAGE, f"node{digest}"), (ACTION, f"actions/checkout@{sha}")])

        wrapped = (f"jobs:\n  p:\n    container:\n      image: node{digest}\n"
                   f"    name: a job name that\n      wraps onto a second line\n"
                   f"    steps:\n      - uses: actions/checkout@{sha}\n")
        self.assertEqual([r.ref for r in scan_text(wrapped)],
                         [f"node{digest}", f"actions/checkout@{sha}"],
                         "a wrapped value outside the container node was refused as if it "
                         "were the image shorthand")

        beside = (f"jobs:\n  p:\n    services:\n      redis:\n        image: r{digest}\n"
                  f"    env:\n      TOKEN: not-an-image\n")
        self.assertEqual([(r.key, r.ref) for r in scan_text(beside)],
                         [("image", f"r{digest}")],
                         "a key beside the services block was read as another service")

    def test_a_pinned_step_survives_every_spelling_too(self):
        """The corpus above would pass against a reader that refused everything. This is
        the other half: the same spellings, pinned, must come out clean."""
        sha = "a" * 40
        for name, text, _ in self.SEEN:
            if "image" in name:
                continue
            with self.subTest(spelling=name):
                refs = scan_text(text.replace("evil/action@main", f"actions/checkout@{sha}"))
                self.assertEqual(len(refs), 1)
                self.assertTrue(immutable(refs[0].ref), name)


class TheSubsetIsWideEnoughToWriteAWorkflowIn(unittest.TestCase):
    """The other direction. A reader that raised on everything would satisfy every check
    above and be useless — these are constructs this repository's files actually contain,
    and each one must read clean.
    """

    def test_the_shapes_the_real_files_use(self):
        for name, text in (
            ("a flow sequence of scalars", 'on:\n  push:\n    tags: ["v*"]\n'),
            ("a matrix", 'matrix:\n  python-version: ["3.11", "3.12"]\n'),
            ("an expression value", "with:\n  python-version: ${{ matrix.python }}\n"),
            ("a url with a colon in it", "url: https://pypi.org/p/charter-cp\n"),
            ("a url with a fragment", "url: https://x/SECURITY.md#reporting\n"),
            ("a comment after a value", "id-token: write        # REQUIRED: mints it\n"),
            ("a block scalar", "run: |\n  echo hi\n  # uses: evil/action@main\n"),
            ("a folded block scalar", "description: >\n  a b\n  c\n"),
            ("a keyless block", "on:\n  push:\n  pull_request:\n"),
            ("an if expression",
             "if: github.event_name == 'push' && github.ref == 'refs/heads/main'\n"),
            ("a dropdown of plain items",
             "options:\n  - macOS\n  - Windows (WSL)\n  - Other\n"),
            ("a whole-line comment at depth", "jobs:\n  p:\n    # a note\n    x: 1\n"),
        ):
            with self.subTest(shape=name):
                self.assertEqual(scan_text(text), [], name)

    def test_a_block_scalar_hides_nothing_and_invents_nothing(self):
        """`run:` content is opaque text, so a `uses:` inside a shell heredoc is neither a
        step nor a false alarm — and the block ends where the indentation does."""
        text = ("steps:\n"
                "  - run: |\n"
                "      echo 'uses: evil/action@main'\n"
                "  - uses: actions/checkout@" + "a" * 40 + "\n")
        self.assertEqual([r.ref for r in scan_text(text)],
                         ["actions/checkout@" + "a" * 40])


class ALineBreakIsTheOtherHalfOfTheSpellingProblem(unittest.TestCase):
    """Where does a line begin? A reader that works line by line has already answered that,
    usually without noticing — and this repository has been bitten by exactly that answer
    before, by an escape that handled `\n` and let U+2028, U+2029 and U+0085 through.

    Here the relationship runs the safe way round, and it is worth stating as a property
    rather than trusting: `str.splitlines` breaks on a **superset** of what a YAML parser
    calls a line break. So every separator GitHub honours is one this reader has already
    honoured, and it cannot be handed a key on a line it never looked at. The cost of the
    superset is the opposite mistake — see the second test — and the opposite mistake is a
    false alarm somebody reads, not a step somebody misses.
    """

    def test_every_separator_a_parser_honours_this_reader_honours_first(self):
        for sep in ("\n", "\r\n", "\r", "\u2028", "\u2029", "\x85", "\x0b", "\x0c"):
            with self.subTest(separator=repr(sep)):
                text = "steps:" + sep + "  - uses: evil/action@main" + sep
                self.assertEqual([r.ref for r in scan_text(text)], ["evil/action@main"],
                                 "a step on the far side of this separator was not read")

    def test_splitting_too_eagerly_shows_up_as_a_finding_not_a_blind_spot(self):
        """U+2028 is a break to `str.splitlines` and not one in YAML 1.2, so a scalar
        containing it comes apart here and not on the runner. That surfaces a ref which is
        not really a step — noise a person resolves in one look — rather than swallowing
        one that is. Stated because the inverse would be the bug."""
        text = "steps:\n  - run: echo\u2028uses: evil/action@main\n"
        self.assertEqual([r.ref for r in scan_text(text)], ["evil/action@main"])


class TheFileListComesFromTheIndex(unittest.TestCase):
    def test_the_workflows_are_tracked(self):
        index = tracked()
        self.assertIn(".github/workflows/release.yml", index)
        self.assertIn(".github/workflows/test.yml", index)

    def test_untracked_directories_need_no_denylist(self):
        """The hole this replaces: `_PRUNE = {"node_modules", "dist", ".venv", …}` was a
        guess at where untracked code lives, and `uses: ./node_modules/probe` walked past
        it. Nothing in those directories is in the index, so nothing needs naming."""
        index = tracked()
        self.assertFalse([p for p in index
                          if p.split("/")[0] in ("node_modules", "dist", ".venv")])

    def test_a_local_reference_resolves_only_to_files_the_seed_list_already_holds(self):
        """Why there is no second walk. Every shape `_action_files` can return is either
        under `.github/` or is a directory's `action.y*ml` — both already seeds. Widen it
        to a third shape without widening `seed_files` and this fails, which is the only
        thing standing between "no walk needed" and "a file nobody reads"."""
        index = {"tools/act/action.yml", "tools/act/action.yaml", "docs/x.yaml",
                 ".github/workflows/reusable.yml", "tools/rogue.yml"}
        resolved = [hop for rel in ("tools/act", "tools/missing", "docs/x.yaml",
                                    "tools/rogue.yml", ".github/workflows/reusable.yml")
                    for hop in _action_files(rel, index)]
        self.assertEqual(sorted(resolved), [".github/workflows/reusable.yml",
                                            "tools/act/action.yaml", "tools/act/action.yml"])
        for hop in resolved:
            self.assertTrue(
                hop.startswith(".github/") or hop.rsplit("/", 1)[-1].startswith("action."),
                f"{hop} is a shape seed_files does not collect")
        self.assertEqual(_action_files("tools/rogue.yml", index), [],
                         "a reusable workflow outside .github/workflows is not one")

    def test_a_tracked_action_file_anywhere_is_a_seed(self):
        """A composite action does not have to live under `.github/`, and the seed list has
        to find one wherever it is. Driven off a synthetic index because this repository has
        none today — a check that only fires on a file nobody has written is not a check."""
        seeds = seed_files(REPO, {"tools/act/action.yml", "docs/notes.yaml", "README.md"})
        self.assertIn(REPO / "tools/act/action.yml", seeds)
        self.assertNotIn(REPO / "docs/notes.yaml", seeds)
        self.assertIn(GITHUB / "workflows" / "release.yml", seeds)


class ALocalActionIsFollowedIntoTheFileItNames(unittest.TestCase):
    """`uses: ./x` is code running in the calling job with the calling job's token. The
    traversal is exercised against a tree built here, because charter has no local action
    to exercise it with and an untested traversal is how `./node_modules/probe` got in.
    """

    def _tree(self, tmp: Path, action: str) -> None:
        (tmp / ".github/workflows").mkdir(parents=True)
        (tmp / ".github/workflows/w.yml").write_text(
            "jobs:\n  j:\n    steps:\n      - uses: ./tools/act\n")
        (tmp / "tools/act").mkdir(parents=True)
        (tmp / "tools/act/action.yml").write_text(
            "runs:\n  using: composite\n  steps:\n" + action)

    def test_a_local_actions_own_uses_is_checked_like_the_callers(self):
        with tempfile.TemporaryDirectory() as raw:
            tmp = Path(os.path.realpath(raw))
            self._tree(tmp, "    - uses: evil/action@main\n")
            found = closure(tmp, {".github/workflows/w.yml", "tools/act/action.yml"})
            refs = {(name, r.ref) for name, r in found.refs}
            self.assertIn(("tools/act/action.yml", "evil/action@main"), refs,
                          "the local action's own `uses:` was never read")
            self.assertFalse(immutable("evil/action@main"))

    def test_the_hop_is_only_taken_to_a_tracked_file(self):
        """The bypass, end to end: the action file exists on disk and is not in the index,
        so it is reported as an unresolved local reference rather than followed and
        trusted."""
        with tempfile.TemporaryDirectory() as raw:
            tmp = Path(os.path.realpath(raw))
            self._tree(tmp, "    - uses: evil/action@main\n")
            found = closure(tmp, {".github/workflows/w.yml"})
            self.assertEqual([t for _, _, t in found.local], ["tools/act"])
            self.assertEqual(_action_files("tools/act", {".github/workflows/w.yml"}), [])
            self.assertNotIn("evil/action@main", {r.ref for _, r in found.refs})

    def test_a_file_reached_through_a_symlink_is_refused(self):
        """Git tracks a symlink as its target string. The link moves only with a commit;
        what it points at does not, so the file it resolves to is not this repository's."""
        with tempfile.TemporaryDirectory() as raw:
            tmp = Path(os.path.realpath(raw))
            (tmp / "real").mkdir()
            (tmp / "real/action.yml").write_text("runs:\n")
            (tmp / "link").symlink_to(tmp / "real")
            _no_symlink(tmp / "real/action.yml")          # the plain path is fine
            with self.assertRaises(Unparsed) as caught:
                _no_symlink(tmp / "link/action.yml")
            self.assertIn("symlink", caught.exception.what)


if __name__ == "__main__":
    unittest.main()
