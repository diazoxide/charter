"""`charter doctor`, run by both implementations against the same plane.

`--json` is a contract other tools read, so it is compared BYTE FOR BYTE: Rust's stdout must
be exactly what Python's `json.dumps(rows, indent=2)` would print for the same rows. The table
is compared the same way, against what Python's own renderer (`commands.cmd_doctor`) prints
for them.

**What "the same rows" means, and the one thing it lets differ.** Python's doctor has about
forty rows and half of them are about parts this binary does not own yet — the guard and the
vaults (M3), forges, the tmux frame, the Claude Code plugin. The Rust side still prints every
one of those rows, under its own name and in its own place, as a WARN that says it was not
checked (`charter-core/src/doctor/deferred.rs`). So:

- the row NAMES must be identical and in the same order — a row that goes missing is a doctor
  telling its reader the problem it reported has gone;
- every row in `DEFERRED` must be the Rust side's not-checked shape (WARN, `not checked (…)`,
  the deferred hint) — never OK, and never silently ported without this list changing;
- every other row must be Python's row, byte for byte.

The expected document is Python's rows with each deferred row replaced by Rust's, re-dumped
with Python's own `json.dumps`, and compared to Rust's stdout as bytes.

The exit status follows: Rust's must be 1 exactly when one of its rows is a FAIL, and equal to
Python's whenever no deferred row is a FAIL in Python.
"""

from __future__ import annotations

import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
from collections.abc import Callable
from dataclasses import dataclass, field
from pathlib import Path

import run as base

#: Every row the Rust charter does not check yet, with why. A row here that Rust answers in
#: any other shape is a failure: port it, then take it off this list.
DEFERRED = {
    "python3": "this binary has no Python; the Python charter the hooks still run needs it",
    "gh": "forges are not ported",
    "gh auth": "forges are not ported",
    "glab": "forges are not ported",
    "glab auth": "forges are not ported",
    "git auth": "the one-credential git policy is not ported",
    "harness": "the harness registry's capability ceilings are not ported",
    "frame": "the tmux frame is the Python charter's",
    "ended tab": "the tmux frame is the Python charter's",
    "plane-root guard": "the guard is M3's",
    "guard seen": "the guard is M3's",
    "workspace layer": "generated-layer drift is not ported",
    "changes": "cross-repo changes are not ported",
    "vaults": "vaults are M3's",
    "vault registry": "vaults are M3's",
    "personas": "persona lint is not ported",
    "persona grant": "persona lint is not ported",
    "news": "release news is not ported",
    "ask rules": "persona tools against ask rules is not ported",
    "handoff gate": "the handoff gate is not ported",
    "shadowed docs": "charter's shipped knowledge is not ported",
    "credential paths": "vaults are M3's",
    "mcp": "vaults are M3's",
    "plugin install": "the Claude Code plugin checks are not ported",
    "plugin": "the Claude Code plugin checks are not ported",
    "plugin files": "the Claude Code plugin checks are not ported",
}

#: What the PYTHON side's deferred rows write outside its plane, with why — never excused
#: for the Rust side. `gh auth status`, which Python's `gh auth` row runs on a runner that has
#: `gh` in `/usr/bin`, records a device id in the home it is given.
PYTHON_WRITES = {
    "home/.local/state/gh/": "`gh auth status`, run by Python's deferred `gh auth` row",
}

#: The first words of the hint every deferred row carries.
DEFERRED_HINT = "This charter does not run this check yet, so its silence means nothing"


@dataclass
class DoctorScenario:
    name: str
    plane: str
    #: `doctor` flags. `--json` compares the document, without it the table.
    args: list[str]
    setup: "Callable[[Path], None] | None" = None
    #: Undo what `setup` did that would stop the plane being walked or deleted (a mode 000).
    teardown: "Callable[[Path], None] | None" = None
    #: Rows that are deferred in THIS scenario only — a partly ported row whose unported
    #: branch the setup reaches — each with why.
    deferred_here: dict = field(default_factory=dict)
    #: `(regex, why)`: blanked on both sides before comparing — a parser's own diagnostic.
    mask: list = field(default_factory=list)
    #: Paths (relative to the plane) the tree comparison skips, with why.
    ignore: dict = field(default_factory=dict)
    #: Rows whose Rust wording differs from Python's ON PURPOSE, each with why. Their status
    #: must still match.
    worded_differently: dict = field(default_factory=dict)
    #: Give each side's home a git identity, so `git identity` is not a blocker.
    identity: bool = True
    #: The plane-relative directory both sides run in, and that `$CHARTER_ROOT` names.
    at: str = ""
    #: A plane-relative directory to run in that is NOT the plane `$CHARTER_ROOT` names — a
    #: chat rooted in a workspace or a clone.
    cwd: str = ""
    #: Extra environment for BOTH sides, on top of `run._env`. `{home}` and `{plane}` in a
    #: value are replaced with that side's own directories, because the two sides have
    #: different ones and a scenario is written once. A value of `None` UNSETS the variable.
    #:
    #: `$CLAUDE_CONFIG_DIR` is what this exists for: it names the folder `session root` and
    #: the settings rows read, and nothing else in the fixtures can put a chosen string on a
    #: doctor row.
    env: dict = field(default_factory=dict)


def _identity(home: Path) -> None:
    (home / ".gitconfig").write_text("[user]\n\tname = Fixture User\n\temail = fixture@example.invalid\n")


def _git(cwd: Path, *args: str) -> None:
    env = {
        "PATH": "/usr/bin:/bin", "HOME": str(cwd), "GIT_CONFIG_GLOBAL": "/dev/null",
        "GIT_CONFIG_SYSTEM": "/dev/null", "GIT_AUTHOR_NAME": "t", "GIT_AUTHOR_EMAIL": "t@x.invalid",
        "GIT_COMMITTER_NAME": "t", "GIT_COMMITTER_EMAIL": "t@x.invalid",
        "GIT_AUTHOR_DATE": "2026-05-04T11:32:17+00:00",
        "GIT_COMMITTER_DATE": "2026-05-04T11:32:17+00:00",
    }
    subprocess.run(["git", "-C", str(cwd), *args], env=env, check=True, capture_output=True)


def _plane_repo_off_its_branch_and_dirty(root: Path) -> None:
    _git(root, "init", "-q", "-b", "main", ".")
    _git(root, "add", "charter.toml")
    _git(root, "commit", "-q", "-m", "plane")
    _git(root, "checkout", "-q", "-b", "feature")
    (root / "charter.toml").write_text((root / "charter.toml").read_text() + "# edited\n")


def _plane_repo_with_a_stranded_push(root: Path) -> None:
    _git(root, "init", "-q", "-b", "main", ".")
    _git(root, "add", "charter.toml")
    _git(root, "commit", "-q", "-m", "plane")
    (root / ".charter" / "plane-push.json").write_text(json.dumps(
        {"outcome": "stranded", "branch": "main", "landed": None, "url": None,
         "detail": "remote: protected branch", "head": "0" * 40, "at": 1.0}, indent=2))


def _plane_repo_with_a_crashed_index_lock(root: Path) -> None:
    _git(root, "init", "-q", "-b", "main", ".")
    lock = root / ".git" / "index.lock"
    lock.write_text("")
    old = 1_700_000_000
    os.utime(lock, (old, old))


def _plane_repo_with_a_corrupt_index(root: Path) -> None:
    _git(root, "init", "-q", "-b", "main", ".")
    _git(root, "add", "charter.toml")
    _git(root, "commit", "-q", "-m", "plane")
    (root / ".git" / "index").write_text("not an index")


def _plane_repo_that_would_commit_the_local_file(root: Path) -> None:
    _git(root, "init", "-q", "-b", "main", ".")
    (root / ".gitignore").write_text("")
    _git(root, "add", "charter.toml", ".gitignore")
    _git(root, "commit", "-q", "-m", "plane")
    (root / "charter.local.toml").write_text(
        '[harness.claude-work]\nkind = "claude"\ncommand = ["claude"]\n')


def _build_the_inventory(root: Path) -> None:
    (root / "inventory").mkdir(parents=True, exist_ok=True)
    (root / "inventory" / "repos.json").write_text(json.dumps({
        "group": "acme", "count": 2, "repos": [
            {"name": "api", "path_with_namespace": "acme/api", "forge": "github",
             "ssh_url_to_repo": "git@github.com:acme/api.git",
             "http_url_to_repo": "https://github.com/acme/api.git", "default_branch": "main"},
            {"name": "web", "path_with_namespace": "acme/web", "forge": "github",
             "ssh_url_to_repo": "git@github.com:acme/web.git",
             "http_url_to_repo": "https://github.com/acme/web.git", "default_branch": "main"},
        ]}, indent=2))


def _a_plane_that_is_a_checkout_of_its_own_repo(root: Path) -> None:
    _git(root, "init", "-q", "-b", "main", ".")
    _git(root, "remote", "add", "origin", "https://github.com/acme/plane.git")
    _git(root, "add", "charter.toml")
    _git(root, "commit", "-q", "-m", "plane")


def _drift_the_memory_indexes(root: Path) -> None:
    mem = root / "workspaces" / "alpha" / "memory"
    (mem / "20260501-000000-an-unindexed-fact.md").write_text("# An unindexed fact\n")
    with (mem / "MEMORY.md").open("a") as f:
        f.write("- [Gone](20260101-000000-gone.md)\n")


def _link_an_index_out_of_the_plane(root: Path) -> None:
    outside = root.parent / "outside"
    outside.mkdir(parents=True, exist_ok=True)
    (outside / "credentials").write_text("token\n")
    index = root / "personas" / "_shared" / "memory" / "MEMORY.md"
    index.parent.mkdir(parents=True, exist_ok=True)
    index.unlink(missing_ok=True)
    index.symlink_to(outside / "credentials")


def _shut_a_workspace(root: Path) -> None:
    (root / "workspaces" / "beta").chmod(0o000)


def _open_the_workspace(root: Path) -> None:
    (root / "workspaces" / "beta").chmod(0o755)


def _a_clone_behind_its_upstream(root: Path) -> None:
    origin = root.parent / "origin"
    origin.mkdir(parents=True, exist_ok=True)
    _git(origin, "init", "-q", "-b", "main", ".")
    _git(origin, "commit", "-q", "--allow-empty", "-m", "one")
    _git(root / "workspaces" / "beta", "clone", "-q", str(origin), "svc")
    _git(origin, "commit", "-q", "--allow-empty", "-m", "two")
    _git(origin, "commit", "-q", "--allow-empty", "-m", "three")
    _git(root / "workspaces" / "beta" / "svc", "fetch", "-q")


def _rewrite_charter_toml(text: str) -> Callable[[Path], None]:
    def setup(root: Path) -> None:
        (root / "charter.toml").write_text(text)
    return setup


def _declare_an_unapproved_profile(root: Path) -> None:
    (root / "charter.local.toml").write_text(
        '[harness]\ndefault = "nowhere"\n\n'
        '[harness.claude-work]\nkind = "claude"\ncommand = ["claude", "--model", "opus"]\n')


def _nest_a_plane(root: Path) -> None:
    inner = root / "workspaces" / "beta" / "inner"
    inner.mkdir(parents=True, exist_ok=True)
    (inner / "charter.toml").write_text("schema = 1\n")


def _no_plane(root: Path) -> None:
    (root / "charter.toml").unlink()


#: A `$CLAUDE_CONFIG_DIR` that is not ASCII. `session root` prints the folder in use, so the
#: value reaches a row as itself — the only string in a doctor report a scenario gets to
#: choose. The folder is deliberately NOT created: creating it would put the question to the
#: filesystem, and the question here is what each implementation does with the bytes.
_CCD_NON_ASCII = "{home}/.claude-\u65e5\u672c\u8a9e"

#: The same, spelled so that **NFC changes it** — because Claude Code's own binary spells the
#: folder `(CLAUDE_CONFIG_DIR ?? …).normalize("NFC")`, and `charter/harness/claude_code.py`
#: reproduces that. A row naming the folder has to name the one the binary opens.
#:
#: This is the scenario that caught the port: it printed the environment's own bytes, and
#: charter printed the composed folder. Four shapes, not four spellings of one, so a
#: normaliser that got any of them wrong is seen here rather than believed:
#:
#: * `cafe` + U+0301 — the ordinary base-plus-mark composition;
#: * U+212B ANGSTROM SIGN — a singleton, which NFC replaces with U+00C5 outright;
#: * U+0958 DEVANAGARI QA — a composition EXCLUSION, which NFC DECOMPOSES and must not put
#:   back together;
#: * U+1100 + U+1161 — Hangul jamo, which compose by arithmetic rather than by table.
_CCD_DECOMPOSED = "{home}/.claude-cafe\u0301-\u212b-\u0958-\u1100\u1161"

#: The same, holding a `Cf` character with no glyph of its own (U+200B ZERO WIDTH SPACE) and
#: one assigned after the hand-written tables in this repo were pasted (U+0890).
_CCD_FORMAT_CHARS = "{home}/.claude-\u200bzero\u0890width"


def _link_an_index_to_a_name_with_no_glyph(root: Path) -> None:
    """`_link_an_index_out_of_the_plane`, with the outside file named in characters that a
    report has to escape.

    This is the one path in `doctor` where a scenario can drive `contain.one_line` over a
    chosen string: the refusal quotes the link's RESOLVED target, and the target's name is
    the scenario's to pick. U+200B is `Cf`, U+0301 is a combining mark that must survive as
    itself, and U+0890 is `Cf` assigned after one of the two hand-written tables this repo
    carried was written — so before they were generated from one CPython, `shown` escaped it
    and `pyrepr` printed it.
    """
    outside = root.parent / "outside"
    outside.mkdir(parents=True, exist_ok=True)
    named = outside / "cre\u200bdentials-cafe\u0301-\u0890.md"
    named.write_text("token\n")
    index = root / "personas" / "_shared" / "memory" / "MEMORY.md"
    index.parent.mkdir(parents=True, exist_ok=True)
    index.unlink(missing_ok=True)
    index.symlink_to(named)


_GIT_INTERNALS = "each side's setup made its own repository, so its index and objects differ"
_TOML_DIAGNOSTIC = (r"is not valid TOML: [^\"]*",
                    "tomllib's diagnostic against toml's; the sentence around it must match")

DOCTOR_SCENARIOS = [
    DoctorScenario(name="doctor-json-on-a-plane-in-use", plane="daily", args=["--json"]),
    DoctorScenario(name="doctor-json-preflight", plane="daily", args=["--json", "--preflight"]),
    DoctorScenario(name="doctor-table-on-a-plane-in-use", plane="daily", args=[]),
    DoctorScenario(name="doctor-table-preflight", plane="daily", args=["--preflight"]),
    DoctorScenario(
        name="doctor-json-without-a-git-identity-is-a-blocker", plane="minimal",
        args=["--json"], identity=False),
    DoctorScenario(
        name="doctor-table-without-a-git-identity-names-the-blocker", plane="minimal",
        args=[], identity=False),
    DoctorScenario(
        name="doctor-plane-root-off-its-branch-and-dirty", plane="daily", args=["--json"],
        setup=_plane_repo_off_its_branch_and_dirty, ignore={".git": _GIT_INTERNALS}),
    DoctorScenario(
        name="doctor-plane-root-with-a-memory-commit-never-pushed", plane="daily",
        args=["--json"], setup=_plane_repo_with_a_stranded_push,
        ignore={".git": _GIT_INTERNALS}),
    DoctorScenario(
        name="doctor-index-lock-left-by-a-crash", plane="daily", args=["--json"],
        setup=_plane_repo_with_a_crashed_index_lock, ignore={".git": _GIT_INTERNALS},
        # The lock is years old either way, so both call it a crash; only the count differs.
        mask=[(r"\d+d old", "Python's clock is pinned for the run and Rust's is the wall "
                            "clock, so the lock's age in days differs by the gap between them")]),
    DoctorScenario(
        name="doctor-a-git-status-that-fails-is-not-a-clean-root", plane="daily",
        args=["--json"], setup=_plane_repo_with_a_corrupt_index,
        ignore={".git": _GIT_INTERNALS}),
    DoctorScenario(
        name="doctor-a-local-file-git-would-commit-refuses-its-profiles", plane="daily",
        args=["--json"], setup=_plane_repo_that_would_commit_the_local_file,
        ignore={".git": _GIT_INTERNALS, "charter.local.toml": "written by this setup",
                ".gitignore": "written by this setup"}),
    DoctorScenario(
        name="doctor-preflight-leaves-the-local-file-to-a-typed-doctor", plane="daily",
        args=["--json", "--preflight"], setup=_plane_repo_that_would_commit_the_local_file,
        ignore={".git": _GIT_INTERNALS, "charter.local.toml": "written by this setup",
                ".gitignore": "written by this setup"}),
    DoctorScenario(
        name="doctor-an-inventory-that-was-built", plane="daily", args=["--json"],
        setup=_build_the_inventory,
        ignore={"inventory/repos.json": "written by this scenario's own setup"}),
    DoctorScenario(
        name="doctor-a-plane-that-can-clone-its-own-repo-without-discover", plane="daily",
        args=["--json"], setup=_a_plane_that_is_a_checkout_of_its_own_repo,
        ignore={".git": _GIT_INTERNALS}),
    DoctorScenario(
        name="doctor-memory-indexes-that-drifted", plane="daily", args=["--json"],
        setup=_drift_the_memory_indexes),
    DoctorScenario(
        name="doctor-memory-index-linked-out-of-the-plane", plane="daily", args=["--json"],
        setup=_link_an_index_out_of_the_plane),
    DoctorScenario(
        name="doctor-a-workspace-it-cannot-read-is-named-not-skipped", plane="daily",
        args=["--json"], setup=_shut_a_workspace, teardown=_open_the_workspace),
    DoctorScenario(
        name="doctor-a-clone-behind-its-upstream", plane="daily", args=["--json"],
        setup=_a_clone_behind_its_upstream,
        ignore={"workspaces/beta/svc": _GIT_INTERNALS}),
    DoctorScenario(
        name="doctor-charter-toml-with-a-bad-forge-and-a-default-it-cannot-launch",
        plane="daily", args=["--json"],
        setup=_rewrite_charter_toml(
            'schema = 1\n\n[[forge]]\nkind = "github"\nowner = "acme"\n\n[[forge]]\n'
            'kind = "bitbucket"\n\n[harness]\ndefault = "clyde"\n')),
    DoctorScenario(
        name="doctor-charter-toml-with-a-default-it-cannot-launch", plane="daily",
        args=["--json"],
        setup=_rewrite_charter_toml(
            'schema = 1\n\n[[forge]]\nkind = "github"\n\n[harness]\ndefault = "clyde"\n'
            '\n[persona]\ndefault = "nobody"\n')),
    DoctorScenario(
        name="doctor-charter-toml-with-worktrees-outside-the-plane", plane="daily",
        args=["--json"],
        setup=_rewrite_charter_toml('schema = 1\n\n[plane]\nworktrees = "../../far/away"\n')),
    DoctorScenario(
        name="doctor-a-plane-from-the-future", plane="daily", args=["--json"],
        setup=_rewrite_charter_toml("schema = 2\n")),
    DoctorScenario(
        name="doctor-a-charter-toml-that-will-not-parse", plane="daily", args=["--json"],
        setup=_rewrite_charter_toml("[harness\n"), mask=[_TOML_DIAGNOSTIC]),
    DoctorScenario(
        name="doctor-a-profile-nobody-approved-is-not-probed", plane="daily", args=["--json"],
        setup=_declare_an_unapproved_profile,
        ignore={"charter.local.toml": "written by this scenario's own setup"}),
    DoctorScenario(
        name="doctor-a-plane-pinned-inside-another-planes-workspaces", plane="daily",
        args=["--json"], setup=_nest_a_plane, at="workspaces/beta/inner"),
    DoctorScenario(
        name="doctor-where-there-is-no-plane", plane="minimal", args=["--json"],
        setup=_no_plane),
    DoctorScenario(
        name="doctor-from-a-workspace-directory-says-which-settings-answered", plane="daily",
        args=["--json"], cwd="workspaces/alpha"),
    DoctorScenario(
        name="doctor-from-a-clone-names-its-own-trust-acceptance", plane="daily",
        args=["--json"], setup=_a_clone_behind_its_upstream, cwd="workspaces/beta/svc",
        ignore={"workspaces/beta/svc": _GIT_INTERNALS}),
    # `$CLAUDE_CONFIG_DIR` reaches a row only from a session that is NOT the plane, which is
    # what `cwd` is for here: `session root` names the config folder in use precisely because
    # the plane's own `.claude/` is not the one the rows below it read.
    DoctorScenario(
        name="doctor-with-a-non-ascii-claude-config-dir", plane="daily", args=["--json"],
        cwd="workspaces/alpha", env={"CLAUDE_CONFIG_DIR": _CCD_NON_ASCII}),
    DoctorScenario(
        name="doctor-with-a-claude-config-dir-that-is-not-nfc-normalised", plane="daily",
        args=["--json"], cwd="workspaces/alpha", env={"CLAUDE_CONFIG_DIR": _CCD_DECOMPOSED}),
    DoctorScenario(
        name="doctor-with-format-characters-in-the-claude-config-dir", plane="daily",
        args=["--json"], cwd="workspaces/alpha",
        env={"CLAUDE_CONFIG_DIR": _CCD_FORMAT_CHARS}),
    # And the same characters where a row ESCAPES them rather than printing them, which is
    # the half the three above cannot reach.
    DoctorScenario(
        name="doctor-a-memory-index-linked-to-a-name-with-no-glyph", plane="daily",
        args=["--json"], setup=_link_an_index_to_a_name_with_no_glyph),
]


def _normalise(text: str, side: Path) -> str:
    """Each side's own directory as `<side>`, whichever spelling of it the text uses."""
    for spelling in (str(side.resolve()), str(side)):
        text = text.replace(spelling, "<side>")
    return text


def _masked(text: str, masks: list) -> str:
    for pattern, _why in masks:
        text = re.sub(pattern, "<masked>", text)
    return text


#: Python's own table, drawn from rows handed to it — so the expected table is the oracle's
#: renderer, and not a second copy of it written here.
_RENDER = """\
import json, sys
from argparse import Namespace
from charter import commands, doctor, tui
rows = [doctor.Result(**r) for r in json.load(sys.stdin)]
doctor.iter_all = lambda preflight=False: iter(rows)
doctor.run_all = lambda preflight=False: list(rows)
doctor.name_width = lambda preflight=False: tui.column("", [r.name for r in rows])
sys.exit(commands.cmd_doctor(Namespace(json=False, preflight=False, fix=False)))
"""


def check(s: DoctorScenario, binary: Path) -> bool:
    problems: list[str] = []
    with tempfile.TemporaryDirectory() as tmp:
        scratch = Path(tmp)
        py_root, py_home, py_pins = base._lay_out(scratch, s.plane, "python")
        rs_root, rs_home, rs_pins = base._lay_out(scratch, s.plane, "rust")
        for root, home in ((py_root, py_home), (rs_root, rs_home)):
            if s.identity:
                _identity(home)
            if s.setup is not None:
                s.setup(root)
        py_before = base._outside(scratch, "python", py_root)
        rs_before = base._outside(scratch, "rust", rs_root)

        def env(root: Path, home: Path, pins: Path) -> dict:
            e = base._env(root / s.at if s.at else root, home, pins)
            for name, value in s.env.items():
                if value is None:
                    e.pop(name, None)
                else:
                    e[name] = value.format(home=home, plane=root)
            return e

        json_args = s.args if "--json" in s.args else [*s.args, "--json"]
        py = subprocess.run([sys.executable, "-m", "charter", "doctor", *json_args],
                            cwd=py_root / (s.cwd or s.at), env=env(py_root, py_home, py_pins),
                            capture_output=True, text=True)
        rs = subprocess.run([str(binary), "doctor", *s.args],
                            cwd=rs_root / (s.cwd or s.at), env=env(rs_root, rs_home, rs_pins),
                            capture_output=True, text=True)
        rs_rows_run = rs
        if "--json" not in s.args:
            rs_rows_run = subprocess.run([str(binary), "doctor", *json_args],
                                         cwd=rs_root / (s.cwd or s.at), env=env(rs_root, rs_home, rs_pins),
                                         capture_output=True, text=True)
        # Each side's own directory is `<side>` BEFORE anything is compared or substituted, so
        # a row taken from one side and a row taken from the other read the same paths.
        try:
            py_rows = json.loads(_normalise(py.stdout, scratch / "python"))
        except ValueError:
            problems.append(f"    python did not print JSON ({py.returncode}): {py.stderr.strip()!r}")
            py_rows = None
        try:
            rs_rows = json.loads(_normalise(rs_rows_run.stdout, scratch / "rust"))
        except ValueError:
            problems.append(f"    rust did not print JSON ({rs_rows_run.returncode}): "
                            f"{rs_rows_run.stderr.strip()!r}")
            rs_rows = None

        if py_rows is not None and rs_rows is not None:
            problems += _compare_rows(s, py_rows, rs_rows, scratch)
            expected_rows = _substituted(s, py_rows, rs_rows)
            rust_fails = any(r["status"] == "fail" for r in rs_rows)
            if rs.returncode != int(rust_fails):
                problems.append(f"    rust exited {rs.returncode} with "
                                f"{'a' if rust_fails else 'no'} blocker row")
            deferred_fail = any(p["status"] == "fail" and p["name"] in _deferred(s)
                                for p in py_rows)
            if not deferred_fail and py.returncode != rs.returncode:
                problems.append(f"    exit status differs: python {py.returncode}, rust "
                                f"{rs.returncode}")
            if "--json" in s.args:
                want = json.dumps(expected_rows, indent=2) + "\n"
            else:
                drawn = subprocess.run(
                    [sys.executable, "-c", _RENDER], input=json.dumps(expected_rows),
                    cwd=py_root / (s.cwd or s.at), env=env(py_root, py_home, py_pins),
                    capture_output=True, text=True)
                want = drawn.stdout
            want = _masked(want, s.mask)
            got = _masked(_normalise(rs.stdout, scratch / "rust"), s.mask)
            if want != got:
                problems.append("    stdout differs from the expected document:")
                import difflib
                for line in list(difflib.unified_diff(want.splitlines(), got.splitlines(),
                                                      "expected", "rust", lineterm="",
                                                      n=1))[:40]:
                    problems.append(f"      {line}")
        if rs.stderr:
            problems.append(f"    rust wrote to stderr: {rs.stderr.strip()!r}")

        if s.teardown is not None:
            s.teardown(py_root)
            s.teardown(rs_root)
        problems += base._diff_trees(py_root, rs_root, s.ignore)
        problems += [
            line for line in
            base._escaped("python", py_before, base._outside(scratch, "python", py_root))
            if not any(f" {prefix}" in line for prefix in PYTHON_WRITES)
        ]
        problems += base._escaped("rust", rs_before, base._outside(scratch, "rust", rs_root))

    print(("ok   " if not problems else "DIFF ") + s.name)
    for line in problems:
        print(line)
    return not problems


def _deferred(s: DoctorScenario) -> set:
    return set(DEFERRED) | set(s.deferred_here)


def _compare_rows(s: DoctorScenario, py_rows: list, rs_rows: list, scratch: Path) -> list:
    out = []
    py_names = [r["name"] for r in py_rows]
    rs_names = [r["name"] for r in rs_rows]
    if py_names != rs_names:
        out.append("    the rows differ — every row Python prints must be printed, in order:")
        out.append(f"      python {py_names}")
        out.append(f"      rust   {rs_names}")
        return out
    for p, r in zip(py_rows, rs_rows):
        name = p["name"]
        if name in _deferred(s):
            if (r["status"] != "warn" or not r["detail"].startswith("not checked (")
                    or not r["hint"].startswith(DEFERRED_HINT)):
                out.append(f"    {name!r} is on the deferred list, and rust answered it: {r!r} "
                           f"— if it is ported now, take it off the list")
        elif name in s.worded_differently:
            if p["status"] != r["status"]:
                out.append(f"    {name!r}: status python {p['status']}, rust {r['status']}")
    return out


def _substituted(s: DoctorScenario, py_rows: list, rs_rows: list) -> list:
    """Python's rows, with each row Rust does not check — or words differently on purpose —
    replaced by Rust's."""
    keep_rust = _deferred(s) | set(s.worded_differently)
    return [r if p["name"] in keep_rust else p for p, r in zip(py_rows, rs_rows)]


def time_preflight(binary: Path, runs: int) -> None:
    """What `charter doctor --preflight` costs on the plane in use, for each implementation —
    the SessionStart hook pays it at every session start."""
    import statistics
    import time

    with tempfile.TemporaryDirectory() as tmp:
        scratch = Path(tmp)
        for side, argv in (("python", [sys.executable, "-m", "charter"]), ("rust", [str(binary)])):
            root, home, pins = base._lay_out(scratch, "daily", side)
            _identity(home)
            took = []
            for _ in range(runs):
                start = time.perf_counter()
                subprocess.run([*argv, "doctor", "--preflight"], cwd=root,
                               env=base._env(root, home, pins), capture_output=True)
                took.append((time.perf_counter() - start) * 1000)
            print(f"preflight {side:<6} median {statistics.median(took):7.1f} ms   "
                  f"max {max(took):7.1f} ms   over {runs} runs")
