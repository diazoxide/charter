"""`charter init` inside a git repo — the one thing that changes: what it *offers*.

Being inside a repo no longer decides what init PRODUCES. The embedded plane shape is gone
(docs/adr/0007), there is one shape, and `tests/test_init.py` already pins the plane init
builds. What is left is a debt that removal created: charter used to promise a solo user
with one repo could `charter init` and carry on working in that repo, because the `default`
workspace *was* the plane root. It isn't, so init offers the first clone instead — met once
during setup rather than discovered later as "where did my code go?".

**"Offers" is not a prompt.** charter has no interactive input anywhere: `util.py` carries
info/ok/warn/err and nothing that reads stdin, because charter runs inside hooks and agent
sessions where blocking on stdin hangs the turn. So the offer is a printed command, and
running that command (`charter init --clone-this-repo`) IS the acceptance — the same
two-step shape `charter report` uses for consent, where the second command is the consent
(docs/adr/0003).

Real git throughout, the `tests/test_worktree.py` pattern: what these tests are about is a
clone's `.git`, its `origin`, and what `git status` in the plane root says afterwards. A
mocked git would prove nothing about any of them.
"""
from __future__ import annotations

import io
import shutil
import subprocess
import tempfile
import unittest
from contextlib import contextmanager, redirect_stderr
from pathlib import Path
from types import SimpleNamespace

from charter import cli, commands, config, gitpolicy, instance, workspace
from tests import _envguard
from tests._isolation import PersonaIso


def git(repo: Path, *args: str) -> subprocess.CompletedProcess:
    return subprocess.run(["git", "-C", str(repo), *args],
                          check=True, capture_output=True, text=True)


def init_repo(path: Path, branch: str = "main") -> Path:
    """A real git repo with one commit, so HEAD and `git status` both have answers."""
    path.mkdir(parents=True, exist_ok=True)
    subprocess.run(["git", "init", "-q", "-b", branch, str(path)],
                   check=True, capture_output=True)
    git(path, "config", "user.email", "t@example.com")
    git(path, "config", "user.name", "t")
    # Background maintenance races the teardown. `git maintenance run --auto` fires after
    # ordinary commands, takes `.git/maintenance.lock`, and releases it — while
    # `shutil.rmtree` is walking that directory, which then dies on a name that existed
    # when it was listed and not when it was unlinked. Seen on CI as
    # `FileNotFoundError: 'maintenance.lock'` in a test that only deletes a fixture, on one
    # Python job while the others passed the same commit.
    #
    # Disabled at the source rather than tolerated in each teardown: a fixture repo has
    # nothing to maintain, and `ignore_errors=True` on the rmtree would hide real breakage
    # in tests whose whole subject is what is on disk.
    git(path, "config", "gc.auto", "0")
    git(path, "config", "maintenance.auto", "false")
    (path / "README.md").write_text("hello\n")
    git(path, "add", "README.md")
    git(path, "-c", "commit.gpgsign=false", "commit", "-qm", "init")
    return path


def make_repo_plane(root: Path, upstream_home: Path, name: str = "myapp") -> Path:
    """The solo user's situation: the directory they run `charter init` in is their own
    project — a git working tree with an origin they push to. Returns the upstream path.

    The upstream is a bare repo on disk rather than a forge URL so the whole fixture is
    offline and honest at once: `git fetch` from the clone either works or it doesn't."""
    upstream = upstream_home / f"{name}.git"
    subprocess.run(["git", "init", "-q", "--bare", str(upstream)],
                   check=True, capture_output=True)
    init_repo(root)
    git(root, "remote", "add", "origin", str(upstream))
    git(root, "push", "-q", "origin", "main")
    return upstream


@contextmanager
def plane_at(root: Path):
    """Point charter's derived paths at *root* for the duration. `config.use` is the same
    seam `tests/_isolation.py` uses, so a test can stand a second plane up without
    re-deriving twenty-five settings by hand."""
    previous = config.use(root)
    try:
        yield root
    finally:
        config.restore(previous)


def run_init(**kw) -> tuple[int, str]:
    """Call the handler directly (the suite's convention) against the active `config.ROOT`.

    Returns ``(rc, stderr)`` — the offer is user-facing text and `util.info/ok/warn/err`
    all write to **stderr**, so a test asserting on it that only captured stdout would
    assert on an empty string and pass for the wrong reason."""
    args = SimpleNamespace(forge=kw.get("forge", "github"),
                           owner=kw.get("owner", "acme"),
                           host=kw.get("host"),
                           clone_this_repo=kw.get("clone_this_repo", False))
    buf = io.StringIO()
    with redirect_stderr(buf):
        rc = commands.cmd_init(args)
    return rc, buf.getvalue()


def offered_command(stderr: str) -> list[str]:
    """The argv of the command the offer printed, e.g. ``['init', '--clone-this-repo']``."""
    line = next(ln for ln in stderr.splitlines() if "charter init --clone" in ln)
    return line.split("charter", 1)[1].split()


def git_state(repo: Path) -> dict:
    """Everything about *repo*'s git that making a clone could plausibly disturb."""
    return {name: git(repo, *cmd).stdout for name, cmd in (
        ("head", ("rev-parse", "HEAD")),
        ("branch", ("rev-parse", "--abbrev-ref", "HEAD")),
        ("refs", ("show-ref",)),
        ("remotes", ("remote", "-v")),
        ("config", ("config", "--local", "--list")),
        ("status", ("status", "--porcelain")),
    )}


def plane_surface(root: Path) -> dict:
    """What the *control plane* is: the files init writes, and the top-level layout.

    Deliberately excludes what is INSIDE `workspaces/` — that is where a task's clones
    live, it is gitignored, and a clone landing there is precisely what accepting the offer
    is for. The claim under test is that the plane is the same either way, not that the
    filesystem is."""
    return {
        "entries": sorted(p.name + ("/" if p.is_dir() else "") for p in root.iterdir()),
        "files": {rel: (root / rel).read_text()
                  for rel in ("charter.toml", ".gitignore", ".claude/settings.json")},
    }


class RepoPlaneIso(PersonaIso):
    """A plane root that is itself a git repo with an upstream — `charter init` run inside
    your own project, which is the only situation the offer exists for."""

    def setUp(self) -> None:
        super().setUp()
        home = Path(tempfile.mkdtemp(prefix="charter-upstream-"))
        self.addCleanup(shutil.rmtree, home, True)
        self.upstream = make_repo_plane(config.ROOT, home)
        self.clone = config.WORKSPACES_DIR / config.DEFAULT_WORKSPACE / "myapp"


class TestTheOffer(RepoPlaneIso):
    def test_init_inside_a_git_repo_offers_that_repo_as_the_first_clone(self):
        """The whole point: a person who has just made their project a control plane is
        told, once, where work now happens — instead of finding out when their next clone
        lands somewhere they did not expect."""
        rc, err = run_init()
        self.assertEqual(rc, 0)
        self.assertIn("myapp", err)
        self.assertIn("--clone-this-repo", err)

    def test_the_command_the_offer_prints_is_a_command_that_exists(self):
        """An offer whose command does not parse is worse than no offer — it sends the
        person to a `charter init: unrecognized arguments` error at their first step."""
        _, err = run_init()
        args = cli.build_parser().parse_args(offered_command(err))
        self.assertIs(args.func, commands.cmd_init)
        self.assertTrue(args.clone_this_repo)

    def test_the_offer_on_its_own_clones_nothing(self):
        """Declining is the default, and it has to cost nothing: not a clone, not even the
        first workspace's directory."""
        run_init()
        self.assertEqual(list(config.WORKSPACES_DIR.iterdir()), [])

    def test_declining_leaves_a_valid_plane(self):
        """The offer is never a requirement — a plane you said no to is a finished plane."""
        rc, _ = run_init()
        self.assertEqual(rc, 0)
        self.assertEqual(instance.drift(config.ROOT), [])
        self.assertTrue((config.ROOT / "charter.toml").is_file())

    def test_the_offer_stops_once_the_repo_is_cloned(self):
        """init is idempotent and gets re-run — after adding a forge, after an upgrade.
        Repeating an offer that has already been taken trains people to skip init's
        output, which is where its actual errors are."""
        run_init(clone_this_repo=True)
        _, err = run_init()
        self.assertNotIn("--clone-this-repo", err)


class TestAcceptingTheOffer(RepoPlaneIso):
    def test_accepting_leaves_the_repo_cloned_and_ready_to_work_in(self):
        rc, _ = run_init(clone_this_repo=True)
        self.assertEqual(rc, 0)
        self.assertTrue((self.clone / ".git").is_dir())
        self.assertEqual((self.clone / "README.md").read_text(), "hello\n")

    def test_the_clone_carries_the_commits_you_were_standing_on(self):
        """Cloned from the plane root on disk, not re-fetched from the forge — so work
        that is committed but not pushed comes along, which for the one person standing in
        their own project is the work they were in the middle of."""
        (config.ROOT / "wip.txt").write_text("unpushed\n")
        git(config.ROOT, "add", "wip.txt")
        git(config.ROOT, "-c", "commit.gpgsign=false", "commit", "-qm", "wip")
        run_init(clone_this_repo=True)
        self.assertEqual(git(self.clone, "rev-parse", "HEAD").stdout,
                         git(config.ROOT, "rev-parse", "HEAD").stdout)
        self.assertTrue((self.clone / "wip.txt").exists())

    def test_the_clone_talks_to_the_same_upstream_the_repo_did(self):
        """"Ready to work in" means you can push what you write there. A clone left
        pointing at the plane root looks right and fails on the first push — git refuses a
        push to a non-bare repo's checked-out branch."""
        run_init(clone_this_repo=True)
        self.assertEqual(git(self.clone, "remote", "get-url", "origin").stdout.strip(),
                         str(self.upstream))
        git(self.clone, "fetch", "origin")      # raises (fails the test) if it cannot

    def test_the_clone_is_named_after_the_repo_not_the_directory_the_plane_sits_in(self):
        """`charter clone` names a clone after its inventory record — the forge's name for
        the repo. Naming this one after the plane's directory instead would let a later
        `charter clone myapp` land a SECOND copy beside it, with neither obviously real."""
        run_init(clone_this_repo=True)
        self.assertEqual([d.name for d in workspace.clones(config.DEFAULT_WORKSPACE)],
                         ["myapp"])

    def test_accepting_twice_never_reclones_over_work_already_there(self):
        """Same additive discipline as the rest of init: re-running is always safe."""
        run_init(clone_this_repo=True)
        (self.clone / "LOCAL.md").write_text("mine\n")
        rc, _ = run_init(clone_this_repo=True)
        self.assertEqual(rc, 0)
        self.assertEqual((self.clone / "LOCAL.md").read_text(), "mine\n")

    def test_nothing_about_the_offer_writes_to_the_plane_roots_git_state(self):
        """The plane root is a repo someone is in the middle of working in. Cloning out of
        it must not touch its HEAD, its branch, its refs, its remotes, its local config or
        its working tree — snapshotted around the accepted run only, so init's own writes
        (charter.toml, .gitignore, .claude/) are on both sides and cannot mask a change."""
        run_init()
        before = git_state(config.ROOT)
        rc, _ = run_init(clone_this_repo=True)
        self.assertEqual(rc, 0)
        self.assertEqual(git_state(config.ROOT), before)


class TestOutsideAGitRepo(PersonaIso):
    """`charter init` in a plain directory — the README's 60-second path — must behave
    exactly as it did before the offer existed."""

    def test_init_outside_a_git_repo_makes_no_offer(self):
        rc, err = run_init()
        self.assertEqual(rc, 0)
        self.assertNotIn("--clone-this-repo", err)
        self.assertEqual(instance.drift(config.ROOT), [])

    def test_asking_for_the_clone_outside_a_git_repo_fails_loudly(self):
        """Non-zero because a caller asked for something that did not happen — the same
        rule `cmd_init` already applies to a blocked baseline directory and a malformed
        settings.json. The plane is still scaffolded: the flag is an extra, not a gate."""
        rc, err = run_init(clone_this_repo=True)
        self.assertEqual(rc, 1)
        self.assertIn("--clone-this-repo", err)
        self.assertTrue((config.ROOT / "charter.toml").is_file())
        self.assertEqual(list(config.WORKSPACES_DIR.iterdir()), [])


class TestOnlyTheRepoYouAreActuallyStandingIn(PersonaIso):
    """The offer keys off the plane root BEING a working tree's top level, not off git
    finding a repo somewhere up the tree. A great many people keep $HOME under git for
    their dotfiles; scaffolding a plane at ~/planes/acme must not offer to clone their
    home directory into it."""

    def test_a_plane_below_a_repos_top_level_gets_no_offer(self):
        init_repo(config.ROOT)
        sub = config.ROOT / "plane"
        sub.mkdir()
        with plane_at(sub):
            rc, err = run_init()
        self.assertEqual(rc, 0)
        self.assertNotIn("--clone-this-repo", err)


class TestARepoWithNoUpstreamYet(PersonaIso):
    """A `git init`-ed project that has never been pushed is still a repo you are standing
    in, and the offer must still land a workspace you can work in — cloned from the plane
    root, because that copy is the only one there is."""

    def test_a_repo_with_no_origin_is_still_cloned_into_the_first_workspace(self):
        init_repo(config.ROOT)
        rc, err = run_init(clone_this_repo=True)
        self.assertEqual(rc, 0)
        clone = config.WORKSPACES_DIR / config.DEFAULT_WORKSPACE / config.ROOT.name
        self.assertTrue((clone / ".git").is_dir())
        self.assertEqual((clone / "README.md").read_text(), "hello\n")
        # Said out loud, because this clone's origin is the plane root and pushing to it
        # will be refused — a silent surprise at the first push otherwise.
        self.assertIn("no origin", err)


class TestTheFirstCloneTalksOverATokenNotSSH(PersonaIso):
    """Golden rule 0 — one credential per forge, over HTTPS — governs this clone like
    every other. Plenty of people's own repos have an SSH origin, and inheriting it
    verbatim would hand charter's very first workspace exactly what the rule exists to
    avoid: a remote that needs an ssh-agent, in the setup step that runs before `doctor`
    has checked anything."""

    def test_an_ssh_origin_becomes_its_forges_https_form_under_that_forges_token(self):
        init_repo(config.ROOT)
        git(config.ROOT, "remote", "add", "origin", "git@github.com:acme/api.git")
        rc, _ = run_init(clone_this_repo=True)
        self.assertEqual(rc, 0)
        clone = config.WORKSPACES_DIR / config.DEFAULT_WORKSPACE / "api"
        self.assertEqual(git(clone, "remote", "get-url", "origin").stdout.strip(),
                         "https://github.com/acme/api.git")
        self.assertEqual(gitpolicy.check(clone), [])   # charter's own compliance checker


class TestThePlaneIsTheSameEitherWay(unittest.TestCase):
    """The acceptance criterion the whole design turns on: being inside a git repo changes
    what init OFFERS, never what it BUILDS. Two identical repo-planes, one accepting and
    one declining, must produce the same control plane."""
    def setUp(self) -> None:
        # Outside a frame, with no session id and no pinned workspace: stated here
        # rather than inherited from the shell the suite was launched from
        # (#519, #521, #528).
        _envguard.unset_all()

    def _plane(self, accept: bool) -> Path:
        tmp = Path(tempfile.mkdtemp(prefix="charter-either-way-")).resolve()
        self.addCleanup(shutil.rmtree, tmp, True)
        root, upstream_home = tmp / "plane", tmp / "upstream"
        root.mkdir()
        upstream_home.mkdir()
        make_repo_plane(root, upstream_home)
        with plane_at(root):
            rc, _ = run_init(clone_this_repo=accept)
            self.assertEqual(rc, 0)
            if accept:      # the run must really have cloned, or this proves nothing
                self.assertTrue((root / "workspaces" / config.DEFAULT_WORKSPACE
                                 / "myapp" / ".git").is_dir())
        return root

    def test_the_plane_is_identical_whether_or_not_the_offer_was_accepted(self):
        self.assertEqual(plane_surface(self._plane(accept=True)),
                         plane_surface(self._plane(accept=False)))


class TestTheFlagIsReachableFromTheCLI(unittest.TestCase):
    """Every other test here calls the handler directly, so none of them would notice the
    flag missing from the parser — and the command the offer prints would not exist."""

    def test_init_takes_the_flag(self):
        self.assertTrue(cli.build_parser().parse_args(["init", "--clone-this-repo"])
                        .clone_this_repo)

    def test_plain_init_does_not_ask_for_the_clone(self):
        self.assertFalse(cli.build_parser().parse_args(["init"]).clone_this_repo)


if __name__ == "__main__":
    unittest.main()
