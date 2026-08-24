"""A name charter reads OUT OF A FILE must name one entry inside its own base, or nothing.

#328: `valid_name` exists in `charter/persona.py` and `charter/workspace.py` and was
called from six places — `persona create`, `workspace create`/`rename`, the `--piece`
argument, `workspace.ensure`. Every one of them is a command handling a name a human just
typed. **No parser called either function.** On the reading side the same values were
joined onto a path or handed to argv with nothing in between, so a committed file could
name a target outside the directory charter meant to look in.

The instances this file covers, all one omission:

* **#337** — `extends:` and `[persona] default` resolve through `def_path()`, which joins
  onto `PERSONAS_DIR` and asks only whether the result exists. A reference with parent
  components became the acting persona, contributing its `vault:`, `role:` and `tools:`.
* **#329** — the same join reached from `uses:`/`borrows:` via `effective_tools`, whose
  result the PreToolUse gate turns into an `allow`.
* **#334** — a `workspace.json` repo name joined onto the workspace directory, then
  `git checkout` and a **credentialed** `git pull` run in whatever that landed on.
* **#325** — the same field from `inventory/repos.json` becoming a `git clone` destination.
* **#335** — an inventory `ssh_url` returned unchanged into the `git clone` argv.
* **#442** — the WORKSPACE twin of `[persona] default`, in both its rungs: `[workspace]
  default` in `charter.toml` and a committed `workspaces/.default`. Both joined onto
  `workspaces/` by `workspace_dir()`, so `workspace current`, `workspace vision` and
  `read_manifest` reported content from outside the plane, and `charter init` aimed its
  first clone there. Every argument for gating the persona rung had already been made and
  shipped; the workspace rung, two ladders over, was simply not on anybody's list. That is
  the reason these tables are written out — an omission is only visible against an
  enumeration.

**Two populations, two shape rules, one containment rule.** Charter mints persona names
(`persona create` enforces `valid_name`), so a persona reference outside that alphabet
cannot name a real persona and `valid_name` is exactly right there. A *forge* mints repo
names, and `org/.github` is both real and common — so repo names get the more permissive
`contain.segment_ok`, which forbids separators and traversal without inventing an
alphabet charter has no business imposing on someone else's forge.

**Containment here is lexical, not `realpath`-based, and that is deliberate.** A name is
refused for its *shape*, before anything is opened; asking the filesystem would make a
traversal succeed exactly when the attacker's target happens to exist. The resolving half
— every *path* charter reads, whether or not a name chose it — is #336, and lives in
`contain.file_refusal`/`dir_refusal` with its own cases in
`tests/test_plane_reads_are_contained.py`. The two are complements, not alternatives:
neither file's cases pass under the other's rule.

**Preconditions are asserted, not assumed.** Each traversal case plants a real canary at
the path the hostile name resolves to and asserts it is there *before* asking charter to
refuse — a refusal because the file happened not to exist would prove nothing, and this
audit produced four vacuous passes exactly that way. The benign half of every table is
just as load-bearing: it is what catches a fix that contains names by refusing all of them.

The tables are the point of the file. They cover every entry point that takes a name from
a file, so the *next* parser to skip containment fails here rather than shipping — which
is how #323's fix was made durable, and how this one omission came to wear six issue
numbers.

**It did not catch #442, and that is worth stating rather than quietly fixing.** The
workspace rungs shipped ungated for four releases after this file claimed to cover "every
entry point that takes a name from a file". A table only catches an omission somebody
compares against it, so it is a place to record a rule, not a mechanism that enforces one.
The nearest thing to a mechanism is the last case in `WorkspaceDefaultTests`, which asserts
that the check a *reader* makes and the check `workspace create` makes are the same
function — an agreement that holds without anyone rereading a list.
"""

from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from charter import (commands, commands_workspace, config, contain, instance, inventory,
                     persona, workspace)
from tests._isolation import PersonaIso

#: Hostile references, by the shape that makes them hostile. Every entry point below is
#: run against every one of these.
#:
#: `..` and `.` are here because they are the two names that traverse without containing a
#: separator, so a check that only rejects `/` lets them through.
_TRAVERSING = ("../outside", "../../beyond", "..", ".", "sub/dir", "sub\\dir", "")

#: Names a real forge really produces. A containment fix that refuses these has broken a
#: working plane, which is a worse outcome than the traversal it set out to prevent —
#: `org/.github` is the special repo GitHub itself tells organisations to create.
_REAL_REPO_NAMES = (".github", "MyRepo", "api", "a.b-c", "repo_1", "x")

#: Names `charter persona create` accepts, and therefore the only ones a persona reference
#: can legitimately carry.
_REAL_PERSONA_NAMES = ("base", "front-door", "a.b_c", "steward2")


class SegmentShapeTests(unittest.TestCase):
    """The permissive shape rule, on its own. Repo names come from a forge, not charter."""

    def test_accepts_the_names_a_forge_really_mints(self):
        for name in _REAL_REPO_NAMES:
            with self.subTest(name=name):
                self.assertTrue(contain.segment_ok(name),
                                f"{name!r} is a real repo name and must stay clonable")

    def test_rejects_every_traversing_shape(self):
        for name in _TRAVERSING:
            with self.subTest(name=name):
                self.assertFalse(contain.segment_ok(name),
                                 f"{name!r} does not name one entry in a directory")

    def test_rejects_an_absolute_path(self):
        for name in ("/etc", "/etc/passwd", "//host/share"):
            with self.subTest(name=name):
                self.assertFalse(contain.segment_ok(name))

    def test_rejects_a_nul_byte(self):
        """A NUL truncates the name inside the C library, so what Python checked and what
        the kernel opened would be two different strings."""
        self.assertFalse(contain.segment_ok("api\x00/../../etc"))


class ContainedJoinTests(unittest.TestCase):
    """The containment rule, on its own — lexical, and it never touches the filesystem."""

    def setUp(self):
        self.base = Path(tempfile.mkdtemp(prefix="contain-test-"))

    def test_a_good_name_joins_to_a_child_of_the_base(self):
        got = contain.child(self.base, "api")
        self.assertEqual(got, self.base / "api")

    def test_a_traversing_name_yields_nothing(self):
        for name in _TRAVERSING + ("/etc",):
            with self.subTest(name=name):
                self.assertIsNone(contain.child(self.base, name))

    def test_it_does_not_consult_the_filesystem(self):
        """A missing directory and a present one must give the same answer: this is a
        question about the name, and making it a question about the disk would mean a
        traversal succeeds exactly when the attacker's target happens to exist."""
        self.assertEqual(contain.child(self.base / "does-not-exist", "api"),
                         self.base / "does-not-exist" / "api")

    def test_a_symlinked_base_is_not_refused(self):
        """#336's containment half is symlinks, and it stays filed. A plane that symlinks
        a persona directory works today and must keep working after this change."""
        real = self.base / "real"
        real.mkdir()
        link = self.base / "link"
        link.symlink_to(real, target_is_directory=True)
        self.assertEqual(contain.child(link, "api"), link / "api")


class PersonaReferenceTests(PersonaIso):
    """#337 and #329: every rung that turns a persona *reference* into a persona."""

    def setUp(self):
        super().setUp()
        # The canary that a traversing reference resolves to. Inside the tmp root but
        # OUTSIDE `personas/`, which is the base being defended.
        outside = self.tmp / "outside"
        outside.mkdir(parents=True, exist_ok=True)
        (outside / "persona.md").write_text(
            "---\nname: outside\nrole: Outside The Base\nvault: outside-vault\n"
            "tools: CanaryTool\n---\n\nCANARY-BODY-FROM-OUTSIDE-THE-BASE\n")
        self.canary = outside / "persona.md"
        # And one genuinely outside the plane, reached by an absolute reference.
        beyond = Path(tempfile.mkdtemp(prefix="beyond-plane-"))
        (beyond / "persona.md").write_text(
            "---\nname: beyond\ntools: CanaryTool\n---\n\nCANARY-BEYOND-THE-PLANE\n")
        self.beyond_ref = str(beyond)
        self.addCleanup(lambda: __import__("shutil").rmtree(beyond, ignore_errors=True))

    def _persona(self, name, body="body", **meta):
        d = config.PERSONAS_DIR / name
        d.mkdir(parents=True, exist_ok=True)
        fm = "\n".join(f"{k}: {v}" for k, v in {"name": name, **meta}.items())
        (d / "persona.md").write_text(f"---\n{fm}\n---\n\n{body}\n")

    def _hostile_refs(self):
        """Every hostile reference, each proven to reach a real file first."""
        self.assertTrue(self.canary.is_file(),
                        "PRECONDITION: the canary persona must exist outside the base")
        rel = "../outside"
        self.assertTrue((config.PERSONAS_DIR / rel / "persona.md").is_file(),
                        "PRECONDITION: '../outside' must reach the canary from PERSONAS_DIR, "
                        "or a refusal proves only that the file was missing")
        self.assertTrue((Path(self.beyond_ref) / "persona.md").is_file(),
                        "PRECONDITION: the absolute reference must reach a real persona file")
        return (rel, self.beyond_ref)

    # -- the parser itself ------------------------------------------------- #
    def test_load_refuses_a_reference_that_is_not_a_persona_name(self):
        for ref in self._hostile_refs():
            with self.subTest(ref=ref):
                self.assertIsNone(persona.load(ref),
                                  f"load({ref!r}) read a definition outside PERSONAS_DIR")

    def test_load_still_reads_every_name_charter_mints(self):
        for name in _REAL_PERSONA_NAMES:
            self._persona(name)
            with self.subTest(name=name):
                self.assertIsNotNone(persona.load(name))

    def test_resolve_refuses_the_same_references(self):
        for ref in self._hostile_refs():
            with self.subTest(ref=ref):
                self.assertIsNone(persona.resolve(ref))

    # -- #337: extends: ----------------------------------------------------- #
    def test_extends_does_not_pull_a_charter_from_outside_the_base(self):
        for ref in self._hostile_refs():
            with self.subTest(ref=ref):
                self._persona("victim", "VICTIM BODY", extends=ref)
                self.assertEqual(persona.lineage("victim"), ["victim"],
                                 f"extends: {ref!r} joined a chain outside PERSONAS_DIR")
                self.assertNotIn("CANARY", persona.resolve("victim")["charter"])

    def test_extends_does_not_inherit_scalars_from_outside_the_base(self):
        """`resolve` merges the parent's `vault:` and `role:`, so containment failing here
        hands the child an identity out of a file the plane does not contain."""
        self._persona("victim", "VICTIM BODY", vault="victim-vault", extends="../outside")
        meta = persona.resolve("victim")["meta"]
        self.assertEqual(meta["vault"], "victim-vault")
        self.assertNotEqual(meta.get("role"), "Outside The Base")

    # -- #329: uses:/borrows: → effective_tools → the PreToolUse gate -------- #
    def test_uses_does_not_grant_tools_from_outside_the_base(self):
        for ref in self._hostile_refs():
            with self.subTest(ref=ref):
                self._persona("front", "front door", tools="Read", uses=ref)
                self.assertEqual(persona.effective_tools("front"), {"Read"},
                                 f"uses: {ref!r} granted a tool declared outside PERSONAS_DIR")

    def test_borrows_does_not_grant_tools_from_outside_the_base(self):
        for ref in self._hostile_refs():
            with self.subTest(ref=ref):
                self._persona("front", "front door", tools="Read", borrows=ref)
                self.assertEqual(persona.effective_tools("front"), {"Read"})

    def test_uses_still_grants_tools_from_a_real_persona(self):
        self._persona("helper", "helper", tools="Bash")
        self._persona("front", "front", tools="Read", uses="helper")
        self.assertEqual(persona.effective_tools("front"), {"Read", "Bash"})

    # -- #337: the declared front door -------------------------------------- #
    def test_charter_toml_default_cannot_name_a_file_outside_the_base(self):
        for ref in self._hostile_refs():
            with self.subTest(ref=ref):
                (config.ROOT / "charter.toml").write_text(
                    f'schema = 1\n\n[persona]\ndefault = "{ref}"\n')
                self.assertIsNone(persona.declared_default(),
                                  f"[persona] default = {ref!r} became the acting identity")
                self.assertNotEqual(persona.resolve_active(), ref)

    def test_charter_toml_default_still_resolves_a_real_persona(self):
        self._persona("steward2")
        (config.ROOT / "charter.toml").write_text(
            'schema = 1\n\n[persona]\ndefault = "steward2"\n')
        self.assertEqual(persona.declared_default(), "steward2")

    def test_legacy_default_dotfile_cannot_name_a_file_outside_the_base(self):
        for ref in self._hostile_refs():
            with self.subTest(ref=ref):
                (config.PERSONAS_DIR / ".default").write_text(ref + "\n")
                self.assertIsNone(persona.default_persona())

    def test_legacy_default_dotfile_still_resolves_a_real_persona(self):
        self._persona("base")
        (config.PERSONAS_DIR / ".default").write_text("base\n")
        self.assertEqual(persona.default_persona(), "base")


class LintAndResolverAgreeTests(PersonaIso):
    """#328's tell: `structural_errors` tested membership in a name set while the resolver
    resolved a path, so `lint` called a reference dangling that the gate honoured.

    The rule this pins is not "both reject the same strings today" — it is that the
    *operator's own check* can never say a reference is inert while the resolver acts on
    it. That is the property whose absence produced all five issues.
    """

    def setUp(self):
        super().setUp()
        outside = self.tmp / "outside"
        outside.mkdir(parents=True, exist_ok=True)
        (outside / "persona.md").write_text("---\nname: outside\ntools: CanaryTool\n---\n\nx\n")

    def _persona(self, name, **meta):
        d = config.PERSONAS_DIR / name
        d.mkdir(parents=True, exist_ok=True)
        fm = "\n".join(f"{k}: {v}" for k, v in {"name": name, **meta}.items())
        (d / "persona.md").write_text(f"---\n{fm}\n---\n\nbody\n")

    def test_a_reference_lint_calls_broken_is_one_the_resolver_refuses(self):
        refs = ("../outside", "..", "sub/dir", "no-such-persona")
        for field in ("uses", "borrows", "extends"):
            for ref in refs:
                with self.subTest(field=field, ref=ref):
                    self._persona("subject", **{field: ref})
                    errs = persona.structural_errors("subject")
                    self.assertTrue(errs, f"lint stayed silent about {field}: {ref!r}")
                    self.assertIsNone(
                        persona.load(ref),
                        f"lint reports {field}: {ref!r} broken while the resolver loads it")

    def test_a_traversing_reference_is_not_reported_as_a_typo(self):
        """"no such persona (dangling)" sends whoever fixes the file hunting for a typo.
        A reference that is not a name at all is a different defect and reads differently."""
        self._persona("subject", uses="../outside")
        messages = " ".join(m for _, m in persona.structural_errors("subject"))
        self.assertIn("../outside", messages)
        self.assertNotIn("dangling", messages,
                         "a path-shaped reference is not a dangling name")

    def test_lint_stays_silent_about_a_reference_that_resolves(self):
        self._persona("helper")
        self._persona("subject", uses="helper")
        self.assertEqual(persona.structural_errors("subject"), [])


class ManifestRepoNameTests(PersonaIso):
    """#334: `workspace restore` reads repo names out of a committed `workspace.json`,
    joins them onto the workspace directory and runs git — including a **credentialed**
    `pull` — in whatever the join landed on.

    Asserts the argv and the working directory charter builds, never what git does with
    them: the invariant charter owns is that no git command runs outside the workspace,
    and it is checkable without a network, a credential, or a real repository.
    """

    def _manifest(self, ws, rows):
        workspace.ensure(ws)
        workspace.write_manifest(ws, {"name": ws, "repos": rows})

    def _run_restore(self, ws):
        """Run restore with git stubbed; return every ``(argv, cwd)`` it attempted."""
        calls = []

        def fake_git(args, cwd=None):
            calls.append((list(args), None if cwd is None else Path(cwd)))
            return SimpleNamespace(stdout="", stderr="", returncode=0)

        with mock.patch.object(commands_workspace, "_git", fake_git), \
             mock.patch.object(commands_workspace, "cmd_clone", lambda *a, **k: 0), \
             mock.patch.object(commands_workspace.gitpolicy, "forge_for",
                               lambda d: SimpleNamespace(
                                   cli="gh", credential_helper=lambda: "!gh auth git-credential")), \
             mock.patch.object(commands_workspace.workspace, "is_git_repo", lambda d: True):
            commands_workspace.cmd_workspace_restore(
                SimpleNamespace(name=ws, on_demand=False))
        return calls

    def test_a_traversing_repo_name_runs_no_git_anywhere(self):
        ws = "probe"
        for name in _TRAVERSING:
            with self.subTest(name=name):
                self._manifest(ws, [{"name": name, "branch": "main"}])
                wd = workspace.workspace_dir(ws)
                for argv, cwd in self._run_restore(ws):
                    self.assertIsNotNone(cwd, f"{argv!r} ran with no working directory")
                    self.assertTrue(
                        str(cwd.resolve()).startswith(str(wd.resolve())),
                        f"git ran OUTSIDE the workspace: {argv!r} in {cwd}")

    def test_a_real_repo_name_still_gets_checked_out_and_pulled(self):
        """The precondition for every refusal above: this code path really does run git,
        so 'no git ran' is a refusal rather than a command that never executed."""
        ws = "probe"
        self._manifest(ws, [{"name": ".github", "branch": "main"}])
        calls = self._run_restore(ws)
        verbs = [argv[0] if argv[0] != "-c" else argv[2] for argv, _ in calls]
        self.assertIn("checkout", verbs, f"restore ran no checkout at all: {calls!r}")
        self.assertTrue(any("pull" in argv for argv, _ in calls),
                        f"restore ran no pull at all: {calls!r}")

    def test_a_branch_cannot_reach_git_as_an_option(self):
        """#334's second half. `git checkout -B x` writes; a manifest is a committed file
        anyone on the team can edit, so a branch beginning with a dash must not be read as
        a flag. Verified against git 2.50.1: `check-ref-format refs/heads/-b` *accepts*
        `-b`, so ref grammar is not what closes this — argv position is."""
        ws = "probe"
        for branch in ("-B", "--orphan", "-b", "--pathspec-from-file=/etc/passwd"):
            with self.subTest(branch=branch):
                self._manifest(ws, [{"name": "api", "branch": branch}])
                for argv, _ in self._run_restore(ws):
                    self.assertNotIn(branch, argv,
                                     f"a manifest branch reached git argv as {branch!r}: {argv!r}")

    def test_a_branch_with_a_slash_still_reaches_git(self):
        """The regression the careless fix causes: `feature/x` is the convention most
        teams use, and it is not a traversal — the branch is a ref, not a path segment."""
        ws = "probe"
        self._manifest(ws, [{"name": "api", "branch": "feature/nested/branch"}])
        argvs = [argv for argv, _ in self._run_restore(ws)]
        self.assertTrue(any("feature/nested/branch" in argv for argv in argvs),
                        f"the branch must reach git as written: {argvs!r}")


class CloneDestinationTests(PersonaIso):
    """#325: an `inventory/repos.json` name becoming a `git clone` destination."""

    def test_a_traversing_repo_name_never_becomes_a_clone_destination(self):
        wd = self.tmp / "workspaces" / "probe"
        wd.mkdir(parents=True, exist_ok=True)
        for name in _TRAVERSING:
            with self.subTest(name=name):
                calls = []

                def fake_git(args, cwd=None):
                    calls.append(list(args))
                    return SimpleNamespace(stdout="", stderr="", returncode=1)

                r = {"name": name, "default_branch": "main", "forge": "github",
                     "web_url": "https://github.com/acme/api",
                     "path_with_namespace": "acme/api"}
                with mock.patch.object(commands, "_git", fake_git):
                    commands._clone_one(r, wd)
                for argv in calls:
                    for element in argv:
                        self.assertFalse(
                            str(element).startswith(str(self.tmp)) and
                            not str(Path(element).resolve()).startswith(str(wd.resolve())),
                            f"clone destination escaped the workspace: {argv!r}")

    def test_a_real_repo_name_still_clones(self):
        wd = self.tmp / "workspaces" / "probe"
        wd.mkdir(parents=True, exist_ok=True)
        calls = []

        def fake_git(args, cwd=None):
            calls.append(list(args))
            return SimpleNamespace(stdout="", stderr="", returncode=0)

        r = {"name": ".github", "default_branch": "main", "forge": "github",
             "web_url": "https://github.com/acme/.github",
             "path_with_namespace": "acme/.github"}
        with mock.patch.object(commands, "_git", fake_git), \
             mock.patch.object(commands, "gitpolicy", SimpleNamespace(apply=lambda d: None),
                               create=True):
            res = commands._clone_one(r, wd)
        self.assertEqual(res["status"], "ok", f"a real repo name must still clone: {res!r}")
        self.assertTrue(any("clone" in argv for argv in calls))
        self.assertEqual(res["dest"], wd / ".github")


class InventoryIdentityTests(unittest.TestCase):
    """The other half of the defence in depth, per #325's own fix direction.

    `inventory.merge` is where a repo's *identity* is decided — its bare name is already
    load-bearing there, which is why it carries collision logic that refuses two repos
    sharing one. A name with a separator in it is not a valid identity, so it is refused
    where identity is settled, not only where a path is joined.

    Both layers exist deliberately and neither is redundant. `inventory/repos.json` is a
    tracked file: a hand-edited or PR-modified inventory never passes through `merge`, so
    the identity check alone is a guard an attacker walks straight around — which is why
    every join asserts independently. And a join-only check would let a bad identity sit
    in the inventory being reported by `status` and `discover` as though it were a repo.
    """

    def _record(self, name):
        return {"name": name, "path_with_namespace": f"acme/{name}", "forge": "github",
                "default_branch": "main", "web_url": f"https://github.com/acme/{name}"}

    def test_a_name_that_is_not_a_segment_is_not_given_an_identity(self):
        for name in _TRAVERSING:
            with self.subTest(name=name):
                got = inventory.merge([[self._record(name)]])
                self.assertEqual([r["name"] for r in got], [],
                                 f"{name!r} was recorded as a repo identity")

    def test_real_repo_names_keep_their_identity(self):
        got = inventory.merge([[self._record(n) for n in _REAL_REPO_NAMES]])
        self.assertEqual(sorted(r["name"] for r in got), sorted(_REAL_REPO_NAMES))

    def test_a_bad_name_does_not_take_the_good_ones_with_it(self):
        """Per-entry, never per-file — the same rule `restore` follows. One bad row in a
        shared, committed file must not deny the whole inventory to the team."""
        rows = [self._record("api"), self._record("../escape"), self._record(".github")]
        got = inventory.merge([rows])
        self.assertEqual(sorted(r["name"] for r in got), [".github", "api"])


class CloneUrlTests(unittest.TestCase):
    """#335: `_https_url` returned whatever the inventory said when no SSH form matched.

    The bound today is git's own `protocol.*.allow`, which charter neither sets nor owns —
    a plane on a git where `ext` has been enabled has no bound at all. A function whose
    entire purpose is normalising a URL so the right credential is used should not be able
    to return something that is not a URL.
    """

    #: Every shape the fallthrough returned verbatim on 0.47.2, confirmed by calling it.
    _HOSTILE_URLS = (
        "ext::sh -c 'touch /tmp/charter-pwned'",   # a transport that runs a command
        "--upload-pack=touch /tmp/charter-pwned",  # not a URL at all — a git option
        "/etc/passwd",                             # a local path git will happily clone
        "file:///etc",
        "-",
        "",
    )

    def test_a_url_that_is_not_https_is_refused(self):
        for url in self._HOSTILE_URLS:
            with self.subTest(url=url):
                got = commands._https_url({"ssh_url": url, "forge": "github",
                                           "path_with_namespace": "acme/api"})
                self.assertFalse(
                    got, f"_https_url returned a non-HTTPS value into the clone argv: {got!r}")

    def test_a_web_url_still_becomes_a_clone_url(self):
        got = commands._https_url({"web_url": "https://github.com/acme/api",
                                   "forge": "github", "path_with_namespace": "acme/api"})
        self.assertEqual(got, "https://github.com/acme/api.git")

    def test_a_known_ssh_form_is_still_rewritten(self):
        got = commands._https_url({"ssh_url": "git@github.com:acme/api.git",
                                   "forge": "github", "path_with_namespace": "acme/api"})
        self.assertTrue(got.startswith("https://"), got)
        self.assertIn("acme/api", got)

    def test_a_repo_whose_url_cannot_be_classified_is_not_cloned(self):
        """Refusing the URL has to mean refusing the clone — returning "" and then handing
        "" to `git clone` would be a different bug wearing the same fix."""
        calls = []

        def fake_git(args, cwd=None):
            calls.append(list(args))
            return SimpleNamespace(stdout="", stderr="", returncode=0)

        wd = Path(tempfile.mkdtemp(prefix="clone-url-test-"))
        r = {"name": "api", "default_branch": "main", "forge": "github",
             "path_with_namespace": "acme/api",
             "ssh_url": "ext::sh -c 'touch /tmp/charter-pwned'"}
        with mock.patch.object(commands, "_git", fake_git):
            res = commands._clone_one(r, wd)
        self.assertNotEqual(res["status"], "ok")
        self.assertEqual(calls, [], f"a refused URL still reached git: {calls!r}")


#: Names `charter workspace create` accepts, and therefore the only ones either committed
#: workspace rung can legitimately carry. `default` is in the list on purpose: it is the
#: built-in fallback, and a containment fix that refused it would break every plane.
_REAL_WORKSPACE_NAMES = ("default", "alpha", "a.b-c", "ws_1", "x", "Web2")


class WorkspaceDefaultTests(PersonaIso):
    """#442: the two committed rungs that name the workspace a session lands on.

    `[workspace] default` (charter.toml) and `workspaces/.default` are the workspace twins
    of `[persona] default` and `personas/.default`, and only the persona pair was gated.
    Both returned the committed value verbatim into `workspace_dir()`, which joins it onto
    `workspaces/`.

    The canary is planted OUTSIDE `workspaces/` and asserted present before each refusal,
    for the reason this file's header gives: a refusal because the target happened not to
    exist proves nothing. Both readers are exercised — a charter is markdown and a manifest
    is JSON, and they fail differently.
    """

    def setUp(self):
        super().setUp()
        config.WORKSPACES_DIR.mkdir(parents=True, exist_ok=True)
        outside = self.tmp / "outside"
        outside.mkdir(parents=True, exist_ok=True)
        (outside / "workspace.md").write_text(
            "---\nname: outside\n---\n\n## Vision\nCANARY-VISION-FROM-OUTSIDE\n")
        (outside / "workspace.json").write_text(
            json.dumps({"name": "outside",
                        "repos": [{"name": "CANARY-REPO", "branch": "main"}]}))
        self.canary = outside
        # And one genuinely outside the plane, reached by an absolute name.
        beyond = Path(tempfile.mkdtemp(prefix="beyond-plane-ws-"))
        (beyond / "workspace.md").write_text(
            "---\nname: beyond\n---\n\n## Vision\nCANARY-BEYOND-THE-PLANE\n")
        self.beyond = str(beyond)
        self.addCleanup(lambda: __import__("shutil").rmtree(beyond, ignore_errors=True))

    def _hostile_names(self):
        """Every hostile value, each proven to reach real content first."""
        rel = "../outside"
        self.assertTrue((config.WORKSPACES_DIR / rel / "workspace.md").is_file(),
                        "PRECONDITION: '../outside' must reach the canary charter from "
                        "WORKSPACES_DIR, or a refusal proves only that it was missing")
        self.assertTrue((config.WORKSPACES_DIR / rel / "workspace.json").is_file(),
                        "PRECONDITION: the canary manifest must be reachable too")
        self.assertTrue((Path(self.beyond) / "workspace.md").is_file(),
                        "PRECONDITION: the absolute name must reach a real charter")
        return (rel, self.beyond)

    def _resolve(self):
        # The plane root: outside every workspace tree, which is the case the declared
        # default exists for and the one the rungs below actually decide.
        return workspace.resolve(cwd=str(config.ROOT))

    def _declare_in_toml(self, value):
        (config.ROOT / "charter.toml").write_text(
            f'schema = 1\n\n[workspace]\ndefault = "{value}"\n')
        # `config.DEFAULT_WORKSPACE` is derived at load, so the file has to be re-read the
        # way a fresh process would read it.
        config.use(config.ROOT)

    # -- rung 1: `[workspace] default` in charter.toml ---------------------- #
    def test_charter_toml_default_cannot_name_a_directory_outside_workspaces(self):
        for name in self._hostile_names():
            with self.subTest(name=name):
                self._declare_in_toml(name)
                self.assertEqual(config.DEFAULT_WORKSPACE,
                                 config.DEFAULT_WORKSPACE_FALLBACK,
                                 f"[workspace] default = {name!r} became the plane's default")
                self.assertNotEqual(self._resolve(), name)

    def test_charter_toml_default_reads_no_charter_from_outside(self):
        for name in self._hostile_names():
            with self.subTest(name=name):
                self._declare_in_toml(name)
                self.assertNotIn("CANARY", workspace.read_charter(self._resolve()))
                self.assertEqual(workspace.read_manifest(self._resolve()), {})

    def test_every_traversing_shape_degrades_to_the_fallback(self):
        for name in _TRAVERSING:
            with self.subTest(name=name):
                got = instance.default_workspace_of({"workspace": {"default": name}},
                                                    "fallback-ws")
                self.assertEqual(got, "fallback-ws",
                                 f"{name!r} was accepted as a workspace name")

    def test_charter_toml_default_still_names_a_real_workspace(self):
        """The benign half, and it is what catches a fix that contains names by refusing
        all of them."""
        for name in _REAL_WORKSPACE_NAMES:
            with self.subTest(name=name):
                got = instance.default_workspace_of({"workspace": {"default": name}},
                                                    "fallback-ws")
                self.assertEqual(got, name)

    def test_a_declared_workspaces_charter_is_still_read(self):
        """The precondition for every refusal above: this path really does read a charter,
        so "no canary" is a refusal rather than a reader that never ran."""
        self._declare_in_toml("alpha")
        workspace.ensure("alpha")
        workspace.scaffold_charter("alpha", vision="REAL-VISION-INSIDE-THE-PLANE")
        self.assertEqual(self._resolve(), "alpha")
        self.assertIn("REAL-VISION-INSIDE-THE-PLANE", workspace.read_charter("alpha"))

    # -- rung 2: the committed `workspaces/.default` dotfile ---------------- #
    def test_the_default_dotfile_cannot_name_a_directory_outside_workspaces(self):
        for name in self._hostile_names():
            with self.subTest(name=name):
                workspace.default_file().write_text(name + "\n")
                self.assertIsNone(workspace.declared_default(),
                                  f"workspaces/.default = {name!r} became the default")
                self.assertNotEqual(self._resolve(), name)
                self.assertNotIn("CANARY", workspace.read_charter(self._resolve()))

    def test_the_default_dotfile_still_names_a_real_workspace(self):
        for name in _REAL_WORKSPACE_NAMES:
            with self.subTest(name=name):
                workspace.default_file().write_text(name + "\n")
                self.assertEqual(workspace.declared_default(), name)

    def test_a_dotfile_that_is_itself_a_link_out_of_the_plane_is_refused(self):
        """The dotfile has two hostile shapes and they are caught by two different checks.
        Above: the CONTENT is not a name — `valid_name` refuses it. Here: the content is a
        perfectly ordinary name and the FILE is a link to somewhere the plane does not
        contain, which only `contain.file_refusal` sees.

        The distinction is asserted rather than assumed. The planted content is `alpha`, a
        real workspace that exists — so if this were caught by the name check it would not
        be caught at all, and a fix that dropped `file_refusal` would still pass.

        A directory would NOT make this case: `read_text` raises `IsADirectoryError`, which
        the `except OSError` already swallows, so the test would pass with the guard
        removed. The other shape a name check cannot see is a FIFO, whose whole failure is
        that it never returns — it is pinned under a watchdog in
        `tests/test_plane_reads_are_bounded.py` rather than here.
        """
        workspace.ensure("alpha")
        outside = Path(tempfile.mkdtemp(prefix="ws-default-link-"))
        self.addCleanup(lambda: __import__("shutil").rmtree(outside, ignore_errors=True))
        planted = outside / "pointer"
        planted.write_text("alpha\n")
        workspace.default_file().symlink_to(planted)
        self.assertEqual(planted.read_text().strip(), "alpha")
        self.assertTrue(workspace.valid_name("alpha"),
                        "PRECONDITION: the planted content must be a LEGAL name, or the "
                        "name check would refuse it and this proves nothing about "
                        "containment")
        self.assertTrue(workspace.workspace_dir("alpha").is_dir(),
                        "PRECONDITION: and it must name a workspace that really exists")
        self.assertIsNone(workspace.declared_default(),
                          "a committed link at workspaces/.default was followed out of "
                          "the plane")
        self.assertEqual(self._resolve(), config.DEFAULT_WORKSPACE)

    # -- the setter, which writes the file rung 2 reads ---------------------- #
    def test_the_setter_refuses_a_name_that_is_not_a_name(self):
        """`workspace_dir(name).exists()` was the only gate, and `../../esc` exists — the
        directory is really there, it is simply not a workspace. A setter that writes a
        value its own reader discards is a setting that silently does nothing."""
        workspace.ensure("alpha")
        # `""` is left out on purpose and is not a hole: to this command an empty name is
        # "no name given", which is its READ form — `charter workspace default` prints the
        # current one and writes nothing. The write assertion below still covers it.
        for name in self._hostile_names() + tuple(n for n in _TRAVERSING if n):
            with self.subTest(name=name):
                workspace.clear_declared_default()
                rc = commands_workspace.cmd_workspace_default(
                    SimpleNamespace(name=name, clear=False))
                self.assertEqual(rc, 1, f"`workspace default {name!r}` was accepted")
                self.assertFalse(workspace.default_file().exists(),
                                 f"{name!r} was written to workspaces/.default")

    def test_the_setter_still_accepts_a_real_workspace(self):
        workspace.ensure("alpha")
        rc = commands_workspace.cmd_workspace_default(
            SimpleNamespace(name="alpha", clear=False))
        self.assertEqual(rc, 0)
        self.assertEqual(workspace.declared_default(), "alpha")

    def test_the_writer_refuses_too(self):
        """Two layers, and neither is redundant — the command is what a human sees a
        sentence from, and this is what a future caller that skips the command hits."""
        for name in _TRAVERSING:
            with self.subTest(name=name):
                with self.assertRaises(ValueError):
                    workspace.set_declared_default(name)

    # -- the next spelling: a legal NAME whose DIRECTORY is the link -------- #
    def test_a_symlinked_workspace_directory_reads_nothing_from_outside(self):
        """The spelling that survives every name check, asked because containing a name is
        not the same as containing a read.

        `evil` is a name `workspace create` would mint. The escape is a committed
        `workspaces/evil -> ../../outside`, after which `workspaces/evil/workspace.md` is an
        ORDINARY REGULAR FILE — `file_refusal` inspects it with `lstat`, finds nothing to
        object to, and reads it. That is the variant `contain.dir_refusal` exists for and
        `persona.definition_refusal` has always asked; the workspace readers asked only the
        file half. Measured on 0.51.0: `read_charter` returned the outside vision and
        `read_manifest` the outside repo list.
        """
        link = config.WORKSPACES_DIR / "evil"
        link.symlink_to(self.canary, target_is_directory=True)
        self.assertTrue((link / "workspace.md").is_file(),
                        "PRECONDITION: the link must reach a real charter")
        self.assertFalse((link / "workspace.md").is_symlink(),
                         "PRECONDITION: the FILE must not be a link — if it were, the "
                         "per-file check would catch it and this case would prove nothing")
        self.assertTrue(workspace.valid_name("evil"),
                        "PRECONDITION: the name must be one charter would mint, or this "
                        "is just the name check again under another spelling")
        self._declare_in_toml("evil")
        self.assertEqual(self._resolve(), "evil",
                         "PRECONDITION: the name is legal, so it must still resolve — "
                         "the read is what has to refuse")
        self.assertNotIn("CANARY", workspace.read_charter("evil"))
        self.assertEqual(workspace.read_manifest("evil"), {})

    def test_a_real_workspace_directory_is_still_read(self):
        """The benign half of the case above: an ordinary directory under `workspaces/`
        must keep answering, or the guard has closed the hole by closing the feature."""
        workspace.ensure("alpha")
        workspace.scaffold_charter("alpha", vision="REAL-VISION-INSIDE-THE-PLANE")
        workspace.write_manifest("alpha", {"name": "alpha",
                                           "repos": [{"name": "api", "branch": "main"}]})
        self.assertIn("REAL-VISION-INSIDE-THE-PLANE", workspace.read_charter("alpha"))
        self.assertEqual(workspace.read_manifest("alpha")["repos"],
                         [{"name": "api", "branch": "main"}])

    # -- the two checks are one rule ---------------------------------------- #
    def test_the_creation_check_and_the_reading_check_are_the_same_function(self):
        """#328's tell was two copies of a rule drifting apart. `workspace create` enforces
        `valid_name`; both rungs above ask `instance.workspace_name_ok`. If those ever
        disagree, a name charter cannot create becomes a name charter will resolve."""
        for name in _TRAVERSING + _REAL_WORKSPACE_NAMES + ("..x", ".hidden", "a b"):
            with self.subTest(name=name):
                self.assertEqual(workspace.valid_name(name),
                                 instance.workspace_name_ok(name))
                if not workspace.valid_name(name):
                    with self.assertRaises(ValueError):
                        workspace.ensure(name)


if __name__ == "__main__":
    unittest.main()
