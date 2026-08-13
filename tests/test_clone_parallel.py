"""Repos clone concurrently, and a git call can no longer hang on a prompt nobody sees.

`cmd_clone` ran `git clone` in a plain loop, so a nine-repo `workspace restore` waited out
nine round trips one after another. Cloning is network-bound, so the wait was almost
entirely idle: `_git` shells out and releases the GIL while it waits.

Two things had to hold before the loop could become a pool.

**Output has to stay in order.** Eight workers calling `util.info`/`ok`/`err` directly
would interleave into something nobody can scan for which repo failed — and that failure
list is the only part of this output anyone reads twice. Workers return records; one
thread renders them, in the order the repos were asked for.

**And a stuck clone has to fail rather than wait.** `util.run` captures stdout and stderr
but leaves stdin INHERITED, so when git falls back to prompting for credentials the prompt
is invisible and the call waits forever. Sequentially that is one hung child you can
interrupt. At eight-way with buffered output it is a command that appears to have died.
`GIT_TERMINAL_PROMPT=0` costs nothing charter supports — its auth design routes every git
operation through a forge CLI's token over HTTPS and treats a prompt as a symptom.

Observed twice while building this: a captured git child blocked on a signing agent, with
a second run queuing behind it and looking like an unrelated regression.
"""
from __future__ import annotations

import threading
import time
import unittest
from types import SimpleNamespace
from unittest import mock

from charter import commands, util


class TestGitCallsCannotBlockOnAnInvisiblePrompt(unittest.TestCase):
    def env_of(self, *cmd) -> dict:
        seen = {}

        def fake(cmd_, **kw):
            seen.update(kw.get("env") or {})
            return SimpleNamespace(returncode=0, stdout="", stderr="")

        with mock.patch("subprocess.run", side_effect=fake):
            util.run(list(cmd), check=False)
        return seen

    def test_a_git_call_disables_the_terminal_prompt(self):
        self.assertEqual(self.env_of("git", "clone", "x").get("GIT_TERMINAL_PROMPT"), "0")

    def test_it_applies_to_every_git_call_not_just_clone(self):
        """The 70 git invocations charter makes were all equally able to hang; the ones
        that do are not only clones."""
        self.assertEqual(self.env_of("git", "status").get("GIT_TERMINAL_PROMPT"), "0")

    def test_a_non_git_command_is_left_alone(self):
        self.assertNotIn("GIT_TERMINAL_PROMPT", self.env_of("gh", "auth", "status"))

    def test_an_explicit_setting_still_wins(self):
        """Nothing in charter passes it today, but a caller that does means it."""
        seen = {}

        def fake(cmd_, **kw):
            seen.update(kw.get("env") or {})
            return SimpleNamespace(returncode=0, stdout="", stderr="")

        with mock.patch("subprocess.run", side_effect=fake):
            util.run(["git", "clone", "x"], check=False, env={"GIT_TERMINAL_PROMPT": "1"})
        self.assertEqual(seen.get("GIT_TERMINAL_PROMPT"), "1")

    def test_the_rest_of_the_environment_still_reaches_the_child(self):
        """A child that loses PATH cannot find git at all — the overlay must stay an
        overlay."""
        self.assertIn("PATH", self.env_of("git", "status"))

    def test_it_reaches_a_real_git_child_not_just_the_kwargs(self):
        """End-to-end, through an actual `git` process: a shell alias inherits git's
        environment, so it reports what git itself was given. Asserting on the kwargs
        alone would pass even if the overlay never made it out of this process."""
        proc = util.run(["git", "-c", "alias.envprobe=!printf %s \"$GIT_TERMINAL_PROMPT\"",
                         "envprobe"], check=False)
        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertEqual(proc.stdout.strip(), "0")


class CloneCase(unittest.TestCase):
    """`_clone_one` is stubbed: this pins the concurrency and the rendering, not git."""

    def targets(self, *names):
        return [{"name": n, "default_branch": "main", "path_with_namespace": f"g/{n}",
                 "forge": "github"} for n in names]

    def run_clone(self, targets, clone_one):
        out = []
        with mock.patch.object(commands, "_clone_one", clone_one), \
             mock.patch.object(commands.util, "info", lambda m, *a: out.append(m)), \
             mock.patch.object(commands.util, "ok", lambda m, *a: out.append(m)), \
             mock.patch.object(commands.util, "err", lambda m, *a: out.append(m)), \
             mock.patch.object(commands, "_hint_repo_docs", lambda *a: None), \
             mock.patch.object(commands.workspace, "resolve", lambda *a, **k: "ws"), \
             mock.patch.object(commands.workspace, "banner", lambda *a, **k: None), \
             mock.patch.object(commands.workspace, "ensure", lambda *a: commands.config.ROOT), \
             mock.patch.object(commands.inventory, "load", lambda: {}), \
             mock.patch.object(commands.inventory, "repos", lambda d=None: targets), \
             mock.patch.object(commands, "_resolve_targets", lambda a, d: targets):
            rc = commands.cmd_clone(SimpleNamespace(repos=[t["name"] for t in targets],
                                                    workspace="ws"))
        return rc, out


class TestClonesRunConcurrently(CloneCase):
    def test_eight_slow_clones_overlap_rather_than_queue(self):
        """The point of the change. Sequentially this is 8 × 0.10s; concurrently it is
        about one of them. The threshold is loose so a busy machine cannot flake it, but
        it is far below the sequential floor."""
        def slow(r, wd):
            time.sleep(0.10)
            return {"repo": r, "dest": commands.config.ROOT / r["name"], "status": "ok",
                    "forge": SimpleNamespace(cli="gh")}

        t0 = time.monotonic()
        rc, _ = self.run_clone(self.targets(*"abcdefgh"), slow)
        self.assertEqual(rc, 0)
        self.assertLess(time.monotonic() - t0, 0.40)

    def test_no_more_than_the_cap_run_at_once(self):
        """Unbounded threads would open a connection per repo — a 50-repo restore would
        hammer one forge."""
        live = peak = 0
        lock = threading.Lock()

        def counted(r, wd):
            nonlocal live, peak
            with lock:
                live += 1
                peak = max(peak, live)
            time.sleep(0.02)
            with lock:
                live -= 1
            return {"repo": r, "dest": commands.config.ROOT / r["name"], "status": "ok",
                    "forge": SimpleNamespace(cli="gh")}

        self.run_clone(self.targets(*[f"r{i}" for i in range(20)]), counted)
        self.assertLessEqual(peak, commands.CLONE_WORKERS)

    def test_every_repo_is_still_cloned(self):
        seen = []
        lock = threading.Lock()

        def rec(r, wd):
            with lock:
                seen.append(r["name"])
            return {"repo": r, "dest": commands.config.ROOT / r["name"], "status": "ok",
                    "forge": SimpleNamespace(cli="gh")}

        self.run_clone(self.targets(*"abcdef"), rec)
        self.assertEqual(sorted(seen), list("abcdef"))


class TestOutputStaysInOrder(CloneCase):
    def finishing_backwards(self, r, wd):
        """Later repos finish FIRST, so completion order is the reverse of ask order."""
        n = r["name"][1:]
        time.sleep(0.01 * (10 - int(n)) if n.isdigit() else 0)
        return {"repo": r, "dest": commands.config.ROOT / r["name"], "status": "ok",
                "forge": SimpleNamespace(cli="gh")}

    def test_results_print_in_the_order_the_repos_were_asked_for(self):
        _, out = self.run_clone(self.targets(*[f"r{i}" for i in range(6)]),
                                self.finishing_backwards)
        names = [ln.split()[0] for ln in out if ln.startswith("r")]
        self.assertEqual(names, [f"r{i}" for i in range(6)])

    def test_a_failure_names_its_repo_and_carries_the_git_error(self):
        def fail(r, wd):
            return {"repo": r, "dest": commands.config.ROOT / r["name"], "status": "failed",
                    "forge": SimpleNamespace(cli="gh"), "stderr": "fatal: not found"}

        rc, out = self.run_clone(self.targets("only"), fail)
        self.assertEqual(rc, 1)
        joined = "\n".join(out)
        self.assertIn("only", joined)
        self.assertIn("fatal: not found", joined)
        self.assertIn("gh auth status", joined)

    def test_one_failure_does_not_stop_the_others(self):
        """Partial access is normal — `restore` documents it. A pool must not turn one
        unreachable repo into a lost batch."""
        def mixed(r, wd):
            st = "failed" if r["name"] == "b" else "ok"
            return {"repo": r, "dest": commands.config.ROOT / r["name"], "status": st,
                    "forge": SimpleNamespace(cli="gh"), "stderr": "denied"}

        rc, out = self.run_clone(self.targets("a", "b", "c"), mixed)
        self.assertEqual(rc, 1)
        joined = "\n".join(out)
        self.assertIn("a →", joined)
        self.assertIn("c →", joined)

    def test_an_already_cloned_repo_is_reported_not_recloned(self):
        def exists(r, wd):
            return {"repo": r, "dest": commands.config.ROOT / r["name"], "status": "exists"}

        rc, out = self.run_clone(self.targets("a"), exists)
        self.assertEqual(rc, 0)
        self.assertIn("already cloned", "\n".join(out))

    def test_a_batch_announces_itself_before_the_quiet_part(self):
        """Buffering means nothing prints until the clones finish; without this line a
        nine-repo restore looks hung."""
        _, out = self.run_clone(self.targets("a", "b"), self.finishing_backwards)
        self.assertIn("2 repo(s)", out[0])
        self.assertIn("at a time", out[0])

    def test_a_single_repo_does_not_get_a_batch_header(self):
        _, out = self.run_clone(self.targets("a"), self.finishing_backwards)
        self.assertNotIn("at a time", "\n".join(out))


if __name__ == "__main__":
    unittest.main()
