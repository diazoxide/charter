"""A workspace holding REAL git clones, for the two change surfaces that ask git.

`charter change revert` and the divergence report are the only parts of the change surface
that leave the record store, and what they leave it for is git in a clone the operator
already has. A fake clone — a directory whose `.git` is a directory, which is all
`workspace.is_clone` asks and all `tests/test_commands_change.py` needs — cannot answer
"does this branch contain that commit", which is the whole question those two ask. So this
builds repositories git will actually answer about.

Not a ``test_*`` module, so discovery skips it — `tests/_statedirscan.py`'s precedent.

**Every commit is made with `-c user.name=…` and `commit.gpgsign=false` on the command
rather than from config**, because the machine running the suite has its own identity and
its own signing key, and a fixture that inherits either is a fixture that fails on somebody
else's laptop for reasons that have nothing to do with the code (`tests/CONTRIBUTING`'s
"your machine is not the runner").

**`-b main` on every `init`.** Without it the branch name comes from the machine's
`init.defaultBranch` — `main` on one box, `master` on another — and
`doctor._plane_default_branch`, which is what the code under test asks, guesses `main`
before `master`. A fixture that depends on a global git setting reads as the code being
wrong rather than the setup.
"""

from __future__ import annotations

import subprocess
from pathlib import Path

from charter import change, config, workspace
from tests._isolation import PersonaIso

#: What every commit in a fixture is made as. One tuple rather than four flags spelled at
#: each site, so a fixture cannot half-inherit the developer's own identity.
IDENT = ("-c", "commit.gpgsign=false", "-c", "user.email=t@e", "-c", "user.name=t")


def git(where, *args) -> subprocess.CompletedProcess:
    """One git command in *where*, raising on failure — a fixture that half-worked is a
    test whose failure describes the wrong thing."""
    return subprocess.run(["git", "-C", str(where), *args], check=True,
                          capture_output=True, text=True)


def sha(where, ref: str = "HEAD") -> str:
    return git(where, "rev-parse", ref).stdout.strip()


class ChangeRepoCase(PersonaIso):
    """A control plane with a workspace whose clones are real repositories."""

    WS = "ws"

    def setUp(self) -> None:
        super().setUp()
        (self.tmp / "charter.toml").write_text("schema = 1\n")
        config.HAS_CONTROL_PLANE = True
        workspace.ensure(self.WS)

    def repo(self, name: str) -> Path:
        """A clone with one commit on `main`, and nothing else.

        **The identity and the signing setting are written into the repo's OWN config**,
        not only passed on the fixture's own commits, and that is a measured requirement
        rather than tidiness. `charter change revert` makes a commit — `git revert` — and
        it deliberately does **not** pass `-c commit.gpgsign=false`: a revert is the
        operator's commit in the operator's repository, and charter silently unsigning it
        would be charter deciding somebody's signing policy, which ADR 0014 puts with the
        host. So on a developer machine configured to sign with an interactive signer, the
        production revert blocks on an approval prompt that no test can answer — measured
        here, as a suite that hung with `op-ssh-sign` waiting. Local config outranks global,
        so setting it on the fixture repo is what makes the code under test runnable
        without changing what it does.

        `user.name`/`user.email` for the second half of the same fact: `git revert` needs
        an identity to commit with, and a CI runner has none.
        """
        d = workspace.workspace_dir(self.WS) / name
        d.mkdir(parents=True)
        subprocess.run(["git", "init", "-q", "-b", "main", str(d)],
                       check=True, capture_output=True)
        git(d, "config", "--local", "commit.gpgsign", "false")
        git(d, "config", "--local", "user.email", "t@e")
        git(d, "config", "--local", "user.name", "t")
        (d / "f").write_text("1\n")
        git(d, "add", "-A")
        git(d, *IDENT, "commit", "-qm", "one")
        return d

    def land_a_merge(self, repo: Path, slug: str, *, trailer: bool = True,
                     squash: bool = False) -> str:
        """Put a member's work on `main` the way `charter change land` would, and answer
        with the sha it created.

        Two shapes, because §3.7 turns on the difference: a **merge** commit has two
        parents and `git revert` needs `-m 1`, while a **squash** is an ordinary one-parent
        commit and `-m` on one fails. Charter asks git which it is rather than remembering,
        so both have to be reachable from a fixture.

        *trailer* off is the divergence case — a landing that looks like charter's and
        carries no `Charter-Change` trailer, which is what a browser merge leaves behind.
        """
        branch = change.default_branch(slug)
        git(repo, "switch", "-q", "-c", branch)
        (repo / branch.replace("/", "_")).write_text("2\n")
        git(repo, "add", "-A")
        git(repo, *IDENT, "commit", "-qm", "member work")
        git(repo, "switch", "-q", "main")
        msg = f"land {slug}"
        if trailer:
            msg += f"\n\nCharter-Change: {slug}"
        if squash:
            git(repo, "merge", "-q", "--squash", branch)
            git(repo, *IDENT, "commit", "-qm", msg)
        else:
            git(repo, *IDENT, "merge", "-q", "--no-ff", "-m", msg, branch)
        return sha(repo)

    def declare(self, slug: str, repo: str, merge: str, *, number=1,
                head: str = "0" * 40, ts: str = "2026-08-29T00:00:00+00:00"):
        """Write the landing declaration charter's own `land` would have written."""
        return change.record_landing(self.WS, slug, repo, number=number, merge=merge,
                                     head=head, ts=ts)

    def make_change(self, slug: str, members, *, why: str = "API 1 -> 2") -> dict:
        """A record with *members* as ``(repo, needs)`` pairs, written to disk."""
        rec = change.new_record(slug, why, "t", "2026-08-29T00:00:00+00:00")
        rec["members"] = [{"repo": r, "branch": change.default_branch(slug),
                           "needs": list(n)} for r, n in members]
        change.write(self.WS, slug, rec)
        return rec
