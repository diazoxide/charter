"""Five committed settings that reach filesystem, network and guard behaviour.

#339, from the authority audit of 0.47.2. Read out of `charter.toml`, which is committed
and shared, with nothing between the value and the effect. They differ in severity and this
file says which is which, because "they were filed together" is not the same as "they all
needed the same answer".

**Fixed here.**

* `[plane] worktrees` took any absolute path and `git worktree add` then created
  directories there. Relocating the worktree root is a deliberate feature and the reason
  it exists is real (a build tool globbing a worktree finds several copies of a repo), so
  containment inside the plane would have broken it. The documented shape is a SIBLING of
  the plane — `"../charter.worktrees"` — and that is now the boundary: at or under ROOT,
  or a direct child of ROOT's parent. `$CHARTER_WORKTREES` still takes anything, because
  an environment variable is the operator's own choice on their own machine, and it is
  already the value that wins.
* `[[forge]] host` is merged into the host set the SSH guard denies against — and, it
  turns out, into `url.https://<host>/.insteadOf` that `charter git-policy --apply` writes
  into a clone's git config. Verified on 0.47.2: `host = "https://evil.example/x@github.com"`
  was accepted whole and produced an `insteadof` of `https://https://evil.example/x@github.com/`.
  A host is now held to a hostname shape; a block that fails it is skipped and reported,
  which is machinery `declared_forges` and `doctor` already have for a bad `kind`.
* A memory title reaching the briefing is capped — fixed with #338, which shares the
  helper, and asserted here as the setting it also is.

**Documented, working as designed.** Two of the five are guarded already, and they are
here so the guard is on the record rather than rediscovered as a finding next time.

* `[memory] share = "push"` turns Stop into an unattended commit-and-push. `clamp_share`
  fails to `local` on anything unrecognised, the commit is scoped with `--` to the
  workspace's own metadata, and `commit_push` refuses on a secret scan.
* `dispatch-isolation: worktree` and `routing: require` raise an `ask` from committed
  frontmatter. Every `_ask` site passes `data`, so the unattended downgrade applies and an
  autonomous run is not floored.

**And one comment that claimed more than the code did.** `_close_todo`'s docstring said
"there is no path from here to a slug that lives elsewhere". `memstore.resolve` applies no
containment and reaches `unlink`; the todos directory of every workspace lives under the
same data root, so a traversing slug resolved to a NEIGHBOUR's todo rather than being
refused. The inputs are argv rather than committed data, so this was never a finding — but
comments in this repo are load-bearing, and the cheaper fix was to make it true.

**Preconditions are asserted.** Every refusal is paired with the benign value that still
works, so a check that stopped running cannot look like a check that refused.
"""

from __future__ import annotations

import os
import unittest
from pathlib import Path
from unittest import mock

from charter import config, contain, hooks, instance, todos, workspace
from charter.commands_workspace import _close_todo
from charter.forge import registry
from tests._isolation import PersonaIso


# --------------------------------------------------------------------------- #
# 1 — `[plane] worktrees` is a path a committed file chooses                    #
# --------------------------------------------------------------------------- #
class TheWorktreeRootStaysBesideThePlane(PersonaIso):
    def _declare(self, value: str):
        (config.ROOT / "charter.toml").write_text(
            f'schema = 1\n\n[plane]\nworktrees = "{value}"\n')
        return config.worktrees_root_for(config.ROOT, instance.load(config.ROOT))

    def test_the_documented_shape_still_works(self):
        """PRECONDITION and the feature itself: a sibling of the plane is the case the
        setting was introduced for, and the containment rule must not cost it."""
        got = self._declare("../charter.worktrees")
        self.assertIsNotNone(got, "the documented relocation was refused")
        self.assertEqual(Path(got).name, "charter.worktrees")

    def test_a_subdirectory_of_the_plane_still_works(self):
        got = self._declare("wt")
        self.assertIsNotNone(got)
        self.assertEqual(Path(got).parent.resolve(), Path(config.ROOT).resolve())

    def test_a_path_outside_the_plane_falls_back_to_the_default_layout(self):
        for value in ("/etc/charter-worktrees", "~/../../etc/charter-worktrees",
                      "../../elsewhere/wt", "/"):
            self.assertIsNone(self._declare(value), value)

    def test_the_refusal_names_both_ends(self):
        why = contain.plane_adjacent_refusal(config.ROOT, "/etc/charter-worktrees")
        self.assertIsNotNone(why)
        self.assertIn("/etc/charter-worktrees", why)

    def test_an_operator_env_var_is_still_unrestricted(self):
        """`$CHARTER_WORKTREES` is set by the person at the machine, on their machine, and
        already wins over the committed key. Containing it would restrict the one input
        here that is not shared data."""
        with mock.patch.dict(os.environ, {"CHARTER_WORKTREES": "/tmp/charter-wt-elsewhere"}):
            got = config.worktrees_root_for(config.ROOT, {})
        self.assertEqual(Path(got), Path("/tmp/charter-wt-elsewhere").resolve())

    def _doctor_row(self, value: str):
        from charter import doctor
        (config.ROOT / "charter.toml").write_text(
            f'schema = 1\n\n[plane]\nworktrees = "{value}"\n')
        with mock.patch.object(config, "HAS_CONTROL_PLANE", True):
            return doctor, doctor.check_control_plane_config()

    def test_doctor_names_a_refused_root(self):
        """Falling back silently would leave a plane's declared layout quietly ignored."""
        doctor, r = self._doctor_row("/etc/charter-worktrees")
        self.assertEqual(r.status, doctor.WARN)
        self.assertIn("worktrees", (r.detail or "") + (r.hint or ""))
        self.assertIn("/etc/charter-worktrees", (r.hint or ""))

    def test_doctor_is_quiet_about_a_legitimate_root(self):
        doctor, r = self._doctor_row("../charter.worktrees")
        self.assertEqual(r.status, doctor.OK)


# --------------------------------------------------------------------------- #
# 3 — `[[forge]] host` reaches a deny set AND a git-config rewrite              #
# --------------------------------------------------------------------------- #
#: Accepted whole on 0.47.2. Each ends somewhere a hostname does not: a scheme and a path
#: make `insteadof` produce `https://https://…`, and `@` in a URL is a userinfo separator,
#: so the host git would actually contact is not the one written down.
_NOT_A_HOST = (
    "https://evil.example/x@github.com",
    "github.com/../evil.example",
    "user@github.com",
    "github.com evil.example",
    "-oProxyCommand=x",
    "..",
    "github.com:not-a-port",
)


class AForgeHostIsAHostname(PersonaIso):
    def _declare(self, host: str) -> tuple[dict, list[str]]:
        (config.ROOT / "charter.toml").write_text(
            f'schema = 1\n\n[[forge]]\nkind = "github"\nhost = "{host}"\n')
        return registry.declared_forges(config.ROOT)

    def test_a_real_self_hosted_host_is_still_declared(self):
        """PRECONDITION and the feature: a declared host is the only way a self-hosted
        forge is ever covered, so this must keep working before any refusal means anything."""
        for host in ("git.internal", "gitlab.example.com", "git.internal:8443",
                     "10.0.0.7", "MyForge.Example.COM"):
            forges, errors = self._declare(host)
            self.assertIn(host, forges, host)
            self.assertEqual(errors, [], host)

    def test_a_host_that_is_not_a_hostname_is_skipped_and_reported(self):
        for host in _NOT_A_HOST:
            forges, errors = self._declare(host)
            self.assertNotIn(host, forges, host)
            self.assertTrue(errors, host)
            self.assertIn(host, " ".join(errors), host)

    def test_the_shape_rule_itself_refuses_what_toml_could_carry(self):
        """A newline cannot be written into the file above as a raw byte, but TOML's own
        `\\n` escape carries one — so the rule is asked directly rather than left untested."""
        for host in ("github.com\nevil.example", "", "  ", "github.com/", ".github.com",
                     "github..com", None, 7):
            self.assertFalse(registry.host_ok(host), repr(host))

    def test_a_refused_host_never_reaches_the_guards_deny_set(self):
        self._declare("https://evil.example/x@github.com")
        hosts = hooks._known_forges()
        self.assertNotIn("https://evil.example/x@github.com", hosts)
        # the class defaults survive — one bad block must not disarm the guard
        self.assertIn("github.com", hosts)

    def test_a_refused_host_never_reaches_a_git_config_rewrite(self):
        """`git-policy --apply` writes `url.https://<host>/.insteadOf`. On 0.47.2 the
        value above produced `https://https://evil.example/x@github.com/`."""
        forges, _ = self._declare("https://evil.example/x@github.com")
        for f in forges.values():
            self.assertNotIn("https://https://", f.insteadof()[0])

    def test_a_good_block_beside_a_bad_one_still_resolves(self):
        (config.ROOT / "charter.toml").write_text(
            'schema = 1\n\n[[forge]]\nkind = "github"\nhost = "user@evil.example"\n'
            '\n[[forge]]\nkind = "gitlab"\nhost = "git.internal"\n')
        forges, errors = registry.declared_forges(config.ROOT)
        self.assertIn("git.internal", forges)
        self.assertEqual(len(errors), 1)

    def test_known_forges_never_raises_on_a_bad_host(self):
        self._declare("..")
        self.assertIn("github.com", registry.known_forges(config.ROOT))


# --------------------------------------------------------------------------- #
# 5 — a memory title reaching the briefing (fixed with #338; same helper)       #
# --------------------------------------------------------------------------- #
class AMemoryTitleIsBoundedWhereItIsInjected(PersonaIso):
    def test_a_hand_edited_title_cannot_become_the_briefing(self):
        from charter import persona
        self.make_persona("helper")
        idx = persona.index_of(persona.memory_dir("helper"))
        idx.write_text("# helper\n\n- [" + "LONG " * 400 + "TAIL](note.md)\n")
        titles = hooks._index_titles(idx)
        self.assertEqual(len(titles), 1, "precondition: the index line was not read")
        self.assertNotIn("TAIL", titles[0])
        self.assertLessEqual(len(titles[0]), hooks._COMMITTED_LINE_CAP + 40)

    def test_the_link_survives_the_cap(self):
        """The title is capped inside the line, not the line itself: `charter recall`
        reaches the memory through that link, and a pointer to nothing is worse."""
        from charter import persona
        self.make_persona("helper")
        idx = persona.index_of(persona.memory_dir("helper"))
        idx.write_text("# helper\n\n- [" + "LONG " * 400 + "TAIL](note.md)\n")
        self.assertTrue(hooks._index_titles(idx)[0].endswith("](note.md)"))

    def test_a_real_title_is_untouched(self):
        from charter import persona
        self.make_persona("helper")
        persona.remember("helper", "a fact worth keeping about the release guard")
        idx = persona.index_of(persona.memory_dir("helper"))
        raw = [ln for ln in idx.read_text().splitlines() if ln.startswith("- [")]
        self.assertEqual(hooks._index_titles(idx), raw)


# --------------------------------------------------------------------------- #
# 2 and 4 — already guarded, recorded so the guard cannot be refactored away    #
# --------------------------------------------------------------------------- #
class TheGuardsThatWereAlreadyThere(PersonaIso):
    def test_an_unrecognised_share_posture_falls_back_to_local(self):
        """`[memory] share` switches on an unattended commit-and-push. A typo — or a
        hostile value — must fail towards the posture that publishes nothing."""
        for value in ("push ", "PUSH", "publish", "", None, "../push"):
            self.assertEqual(instance.clamp_share(value), "local", repr(value))
        self.assertEqual(instance.clamp_share("push"), "push")

    def test_every_ask_site_passes_the_hook_payload(self):
        """`_ask` downgrades to `allow` under `bypassPermissions` — but only when it is
        given the payload that says so. A site that dropped it would floor an unattended
        run on committed frontmatter (`dispatch-isolation`, `routing`)."""
        import inspect
        import re as _re
        src = inspect.getsource(hooks)
        calls = _re.findall(r"_ask\(\s*\"[^\"]+\",(?:[^()]|\([^()]*\))*?\)", src)
        # Two since #371 removed the clone-commit nudge: `pretooluse_dispatch` and
        # `pretooluse_edit`. The number is a PRECONDITION, not the claim — without it a
        # regex that stopped matching would make the loop below pass over nothing.
        self.assertGreaterEqual(len(calls), 2, "precondition: no _ask calls were found")
        for call in calls:
            self.assertRegex(call, r",\s*data\s*[,)]",
                             f"an _ask site does not pass the payload:\n{call}")


# --------------------------------------------------------------------------- #
# the comment that claimed more than the code did                              #
# --------------------------------------------------------------------------- #
class AWorkspaceClosesOnlyItsOwnTodos(PersonaIso):
    def setUp(self) -> None:
        super().setUp()
        workspace.ensure("here")
        workspace.ensure("neighbour")
        todos.add("neighbour", "the neighbour's own intent")
        # `here` gets a todo of its own so its `todos/` directory EXISTS. Without it the
        # traversal cannot resolve at all — `workspaces/here/todos/../..` needs every
        # component to be real — and every refusal below would pass vacuously. This is one
        # of the vacuous passes this audit keeps producing; it was caught by probing
        # `memstore.resolve` directly and finding it traverses fine when the dir is there.
        todos.add("here", "our own intent")
        self.victim = next(iter(todos.open_todos("neighbour")))["slug"]
        self.mine = next(iter(todos.open_todos("here")))["slug"]

    def test_the_traversal_resolves_when_nothing_stops_it(self):
        """PRECONDITION, at the layer below: the store really does reach a neighbour's
        file. If this stops being true the refusals below prove nothing."""
        from charter import memstore
        target = todos.todos_dir("neighbour") / f"{self.victim}.md"
        self.assertTrue(target.exists())
        self.assertIsNotNone(
            memstore.resolve(todos.todos_dir("here"),
                             f"../../neighbour/todos/{self.victim}"))

    def test_a_workspace_can_still_close_its_own(self):
        """PRECONDITION: the close path works, so a refusal below is a refusal."""
        self.assertEqual(_close_todo("here", self.mine, journal=False), 0)
        self.assertEqual(todos.open_todos("here"), [])

    def test_a_traversing_slug_is_refused(self):
        for ident in (f"../../neighbour/todos/{self.victim}",
                      f"../../neighbour/todos/{self.victim}.md"):
            rc = _close_todo("here", ident, journal=False)
            self.assertEqual(rc, 1, ident)
            self.assertEqual(len(todos.open_todos("neighbour")), 1,
                             f"a neighbour's todo was closed via {ident}")


if __name__ == "__main__":
    unittest.main()
