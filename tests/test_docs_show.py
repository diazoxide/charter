"""charter serves its own documentation, so a control plane does not have to carry a copy.

The failure this prevents is one the project has now watched happen downstream. A plane
that vendors its own copy of `personas.md` or `secrets.md` starts identical and then
drifts, in both directions at once: the copy falls behind a feature it never learned about
(a plane was still describing vaults as plain-file only, long after reference vaults and
1Password landed — while *using* reference vaults), and it also runs ahead, growing
sections upstream never receives. Neither half is visible from either side, because
nothing compares them.

`charter docs show <topic>` removes the reason to copy: the CLI that implements the
behaviour is the thing that describes it, so the page can never be a version behind the
binary reading it.

Two properties carry that promise and are pinned here:

* **Every page ships.** The wheel installs `charter/`, not the repo, so a page that is not
  force-included does not exist for an installed user. Adding `docs/newthing.md` without
  wiring it would leave `charter docs show newthing` failing on every machine except the
  contributor's checkout, where the fallback quietly covers it up.
* **An unknown topic classifies rather than guesses** (ADR 0009). Printing the nearest
  match would be a guess about intent; naming the real topics lets the caller choose.
"""
from __future__ import annotations

import shutil
import subprocess
import sys
import tempfile
import tomllib
import unittest
from pathlib import Path

from charter import docsrc

REPO_ROOT = Path(__file__).resolve().parents[1]
DOCS_DIR = REPO_ROOT / "docs"

#: The pages `docs show` serves: the top-level topic pages. `adr/`, `audits/` and
#: `superpowers/` are deliberately not topics — they are design record and working notes,
#: not "how do I use this", and shipping them would put internal plans in every wheel.
def _page_names() -> list[str]:
    return sorted(p.stem for p in DOCS_DIR.glob("*.md"))


class TestDocSource(unittest.TestCase):
    def test_every_page_is_a_topic(self):
        """A page nobody can reach is the same as no page."""
        from charter import docsrc

        self.assertEqual(_page_names(), sorted(docsrc.topics()))

    def test_a_topic_reads_back_its_page(self):
        from charter import docsrc

        body = docsrc.read("git-policy")
        self.assertIsNotNone(body, "git-policy is a page in docs/")
        self.assertEqual((DOCS_DIR / "git-policy.md").read_text(), body)

    def test_an_unknown_topic_reads_as_none(self):
        from charter import docsrc

        self.assertIsNone(docsrc.read("no-such-topic"))

    def test_a_topic_cannot_escape_the_doc_directory(self):
        """`read` takes a topic, not a path. Without this, `charter docs show
        ../../etc/passwd` is a file-read primitive wearing a documentation command."""
        from charter import docsrc

        for hostile in ("../pyproject", "../../etc/passwd", "adr/0014", "a/b"):
            self.assertIsNone(docsrc.read(hostile), hostile)


class TestPagesShip(unittest.TestCase):
    def test_every_page_is_force_included_in_the_wheel(self):
        """The wheel ships `packages = ["charter"]`; `docs/` is outside it. Each page
        therefore needs an explicit force-include, and adding a page without one is a
        drift that only shows up on someone else's machine — the contributor's own
        checkout falls back to `docs/` and looks fine."""
        pyproject = tomllib.loads((REPO_ROOT / "pyproject.toml").read_text())
        forced = (pyproject["tool"]["hatch"]["build"]["targets"]["wheel"]
                  .get("force-include", {}))
        for name in _page_names():
            src = f"docs/{name}.md"
            self.assertIn(src, forced, f"{src} would not ship in the wheel")
            self.assertEqual(forced[src], f"charter/_docs/{name}.md", src)

    def test_nothing_else_is_force_included(self):
        """Keeps `adr/`, `audits/` and `superpowers/` — internal record and working
        notes — out of every user's site-packages."""
        pyproject = tomllib.loads((REPO_ROOT / "pyproject.toml").read_text())
        forced = (pyproject["tool"]["hatch"]["build"]["targets"]["wheel"]
                  .get("force-include", {}))
        self.assertEqual(sorted(forced), sorted(f"docs/{n}.md" for n in _page_names()))


class TestDocsCli(unittest.TestCase):
    """Drives the real entrypoint: `docs` grew subcommands, and the parser change is
    the part that can silently break an existing caller."""

    def _run(self, *args: str) -> subprocess.CompletedProcess:
        return subprocess.run([sys.executable, "-m", "charter", "docs", *args],
                              cwd=REPO_ROOT, capture_output=True, text=True, timeout=60)

    def test_show_prints_the_page(self):
        r = self._run("show", "git-policy")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertIn("one credential", r.stdout.lower())

    def test_list_names_every_topic(self):
        r = self._run("list")
        self.assertEqual(r.returncode, 0, r.stderr)
        for name in _page_names():
            self.assertIn(name, r.stdout, name)

    def test_unknown_topic_fails_and_names_the_real_ones(self):
        """ADR 0009 — errors classify, they do not guess. A near-miss must not be
        silently resolved to the page charter thinks you meant."""
        r = self._run("show", "persona")           # the page is `personas`
        self.assertNotEqual(r.returncode, 0)
        out = r.stdout + r.stderr
        self.assertIn("personas", out)
        self.assertNotIn("# Personas", out, "naming the topics is not printing one")

    def test_bare_docs_still_generates(self):
        """`charter docs` regenerated the plane's topology long before it grew
        subcommands, and Makefiles in the wild call it that way. Turning it into a
        group that demands a subcommand would break them at a distance."""
        r = subprocess.run([sys.executable, "-m", "charter", "docs", "--help"],
                           cwd=REPO_ROOT, capture_output=True, text=True, timeout=60)
        self.assertEqual(r.returncode, 0, r.stderr)
        from charter import cli

        args = cli.build_parser().parse_args(["docs"])
        from charter import commands

        self.assertIs(args.func, commands.cmd_docs)


if __name__ == "__main__":
    unittest.main()


class TestTheContainmentCheckIsReal(unittest.TestCase):
    """`page.parent` is `root` by construction, so checking it asked a question whose
    answer was always yes. A symlink inside the docs directory pointed anywhere on disk,
    `is_file()` followed it, and `read_text()` printed it — through a command whose whole
    contract is "a topic names one page, and nothing else"."""

    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.tmp, True)
        self.docs = self.tmp / "_docs"
        self.docs.mkdir()
        (self.docs / "real.md").write_text("# real\n")
        self._orig = docsrc._PACKAGED
        docsrc._PACKAGED = self.docs
        self.addCleanup(setattr, docsrc, "_PACKAGED", self._orig)

    def test_a_symlink_out_of_the_docs_dir_is_not_read(self):
        secret = self.tmp / "outside.txt"
        secret.write_text("NOT A DOCUMENTATION PAGE\n")
        (self.docs / "leak.md").symlink_to(secret)
        self.assertIsNone(docsrc.read("leak"))

    def test_a_real_page_still_reads(self):
        self.assertEqual(docsrc.read("real"), "# real\n")


class TestTheCheckoutFallbackIsNotSitePackages(unittest.TestCase):
    """Installed, `_CHECKOUT` resolves to `<site-packages>/docs` — a path that belongs to
    nobody, which another distribution can create by shipping a stray top-level directory.
    A wheel built without its `_docs` must report the broken build `source` describes,
    not quietly serve a stranger's pages as charter's."""

    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.tmp, True)
        self._pkg, self._chk = docsrc._PACKAGED, docsrc._CHECKOUT
        self.addCleanup(setattr, docsrc, "_PACKAGED", self._pkg)
        self.addCleanup(setattr, docsrc, "_CHECKOUT", self._chk)
        docsrc._PACKAGED = self.tmp / "absent"          # the broken-wheel case

    def test_a_stray_site_packages_docs_dir_is_not_a_source(self):
        stray = self.tmp / "site-packages" / "docs"
        stray.mkdir(parents=True)
        (stray / "install.md").write_text("# someone else's page\n")
        docsrc._CHECKOUT = stray
        self.assertIsNone(docsrc.source())
        self.assertIsNone(docsrc.read("install"))

    def test_a_real_checkout_still_resolves(self):
        repo = self.tmp / "repo"
        (repo / "docs").mkdir(parents=True)
        (repo / "pyproject.toml").write_text("[project]\nname='charter-cp'\n")
        (repo / "docs" / "install.md").write_text("# ours\n")
        docsrc._CHECKOUT = repo / "docs"
        self.assertEqual(docsrc.source(), repo / "docs")
        self.assertEqual(docsrc.read("install"), "# ours\n")
