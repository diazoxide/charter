#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = [
#   "charter-cp @ git+https://github.com/diazoxide/charter@50d31dc66835592ccea625bab8f5a0da313f2444",
# ]
# ///
"""Differential test: do `charter-core::planeroot` and `::gitconfig` guard the plane root the way
Python does?

A3 (`_plane_root_branch_reason`) and A3b (`_plane_root_reset_reason`) are the first guards that
ASK A REAL GIT. Whether an operand is a revision or a tracked path, what an alias expands to, how
many commits a reset would destroy, which branch is the default — none of it is in the command
line, so `shellseg.py`'s harness, which compares readings of a STRING, cannot hold them. This is
the second harness stage 4 named: a git-repository fixture, and an agreement about how each side
runs git.

# The fixture

Four plane roots, because the guards' answers depend on the repository's STATE and one state
reaches one branch of each question:

    plane            main, an upstream two commits behind, origin/HEAD -> main, a remote-only
                     branch, a tag, a name that is both a branch and a tracked file, and twenty
                     repo aliases — chains, loops, a shell alias, a `!git` alias, an alias whose
                     body is not UTF-8, an alias past the hop limit
    plane-master     `master`, no remote: the default is guessed, and there is no upstream
    plane-trunk      origin/HEAD -> trunk with a stale local `main`: the remote's answer wins
    plane-nodefault  `dev` only, no remote: there is no default to name

plus the directories a workspace clone can be — one whose `core.worktree` names the plane
(absolute, relative, through a `.git` FILE, quoted, and inside a subsection that does not count) —
a symlink to the root, and a directory that is no repository at all. Each case names its plane
and its cwd, and the ORACLE's `config.ROOT` is moved to that plane for the length of the case.

# How each side runs git

Python's `doctor._git_in` inherits the process environment; the Rust's
`worktree::git::run_as_session` constructs one — `HOME`, a fixed `PATH`, the variables that say
where git's CONFIG lives, nothing else — and turns hooks and the fsmonitor off on the command line.
So the harness gives BOTH sides one environment and makes it say something:

* `HOME` is the fixture's, with a global alias in `~/.gitconfig` (`gsw`);
* `XDG_CONFIG_HOME` is the fixture's, with another in `git/config` (`xdgco`);
* `GIT_CONFIG_COUNT`/`KEY_0`/`VALUE_0` carry a third (`envco`);
* `GIT_CONFIG_NOSYSTEM=1`, so the machine's system config says nothing;
* every other `GIT_*` is removed, and the git on `PATH` is the one the Rust finds first.

The three aliases are what makes the Rust's pass-through MEASURED rather than asserted: a runner
that cleared the environment the way every other git call in the core does would read `git xdgco
feature` as a subcommand it has never heard of and stand aside, and `bra` would diverge.

# What is compared

Every function of the closure, per case, so a divergence is attributed rather than merely seen:
the tables (`tbl`), `_checkout_opt_kind` on every option-shaped word (`cok`), `_git_target` and
`_inline_aliases` per git segment (`gt`), `_plane_root_git` (`prg`), `_created_branch` (`cb`),
`gitconfig.configured_work_tree` over the fixture's directories (`cwt`) and over a config text the
case carries (`ccw`, and `ccp` as a `.git` FILE), the config reader's three layers (`psl`, `gcl`,
`gcp`, `gcs`), and `pathlib`'s string arithmetic (`pp`). Those are cheap and run on every case.
The ones that ask git — `_resolve_git_alias` (`rga`), `_checkout_operand_kind` (`okind`),
`_unpushed_at_risk` (`upr`), `_plane_default_branch` (`pdb`), and the two guards themselves (`bra`,
`rst`) — run on every `--git-every`th case, because a git process costs milliseconds.

**The Python side's git answers are MEMOISED** for the length of a run. Nothing writes to the
fixture once it is built, so the same question gets the same answer; the cache is keyed on the
repository and the whole argv and holds exceptions as well as results. The Rust side is never
cached: every one of its answers is a real process.

**Every path in an answer is normalised** — the fixture's temporary directory becomes `@B@` — and
that is the only normalisation. A case's own paths are written with `@B@` too, so the recorded
corpus is the same file on every machine.

    ./planeroot.py                     # 50,000 cases, the git layer on every 10th
    ./planeroot.py --cases 2000        # fewer, while iterating
    ./planeroot.py --coverage          # which branches the cases REACH — read this first
    ./planeroot.py --surface           # which `pub` items the example reaches AT ALL
    ./planeroot.py --record            # rewrite the checked-in curated corpus
    ./planeroot.py --check             # ...and fail if it moved

The Rust side is `cargo build -p charter-core --example planeroot_oracle`.

# Inputs deliberately NOT generated

The oracle RAISES on three, and a differential cannot arbitrate an input one side has no answer
for (charter#1178): a symlink loop among an invocation's subjects (CPython 3.11/3.12, which is
CI's), a NUL anywhere a path is built, and a default branch whose name is not UTF-8. The Rust's
answers there are pinned by unit tests in `planeroot/tests.rs`. Lone surrogates are out too:
Python's `json` accepts them and `serde_json` does not, which is a fact about the transport.
"""

from __future__ import annotations

import argparse
import functools
import json
import os
import random
import re
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parent.parent
CORPUS = REPO / "fixtures" / "corpora" / "planeroot-oracle.jsonl"
FIXTURE = REPO / "fixtures" / "corpora" / "planeroot-fixture.json"
DEFAULT_BINARY = REPO / "target" / "debug" / "examples" / "planeroot_oracle"

B = "@B@"   # the fixture's base directory, in a case and in a normalised answer

# --------------------------------------------------------------------------- #
# The environment BOTH sides run git in                                         #
# --------------------------------------------------------------------------- #

BASE = Path(os.path.realpath(tempfile.mkdtemp(prefix="planeroot-")))
HOME = BASE / "home"
XDG = BASE / "xdg"

#: `worktree::git::GIT_DIRS`, searched first by the Rust. The oracle's `PATH` starts with the
#: directory the Rust will pick, so the two ask the same git.
GIT_DIRS = ("/usr/bin", "/usr/local/bin", "/opt/homebrew/bin", "/bin")


def _git_dir() -> str:
    for d in GIT_DIRS:
        if os.path.isfile(os.path.join(d, "git")):
            return d
    sys.exit("no git in any of the directories the Rust searches first")


ENV = {k: v for k, v in os.environ.items()
       if not k.startswith("GIT_") and k not in ("XDG_CONFIG_HOME", "CHARTER_ROOT")}
ENV.update({
    "HOME": str(HOME),
    "XDG_CONFIG_HOME": str(XDG),
    "GIT_CONFIG_NOSYSTEM": "1",
    "GIT_CONFIG_COUNT": "1",
    "GIT_CONFIG_KEY_0": "alias.envco",
    "GIT_CONFIG_VALUE_0": "checkout",
    "PATH": _git_dir() + os.pathsep + os.environ.get("PATH", ""),
    "PLANEROOT_BASE": str(BASE),
})
os.environ.clear()
os.environ.update(ENV)

PLANE = BASE / "plane"
PLANES = [PLANE, BASE / "plane-master", BASE / "plane-trunk", BASE / "plane-nodefault"]


#: The fixture as DATA: every directory, file, link and git call, in order, with the base directory
#: written `@B@`. The harness runs it; so does the Rust replay of the recorded corpus
#: (`crates/charter-core/tests/the_plane_root_guards_answer_what_the_python_answers.rs`), from the
#: checked-in copy `--record` writes beside the corpus — so the two fixtures cannot drift apart
#: without `--check` saying so.
STEPS: list[dict] = []

#: Every fixture git call runs with these: signing off whatever `HOME` says, and an identity.
FIXTURE_GIT = ["-c", "commit.gpgsign=false", "-c", "tag.gpgsign=false",
               "-c", "user.name=t", "-c", "user.email=t@t"]


def g(cwd: str, *args: str) -> None:
    STEPS.append({"git": cwd, "args": [*FIXTURE_GIT, *args]})


def mkdir(rel: str) -> None:
    STEPS.append({"mkdir": rel})


def write(rel: str, text: str) -> None:
    STEPS.append({"write": rel, "text": text})


def symlink(rel: str, target: str) -> None:
    STEPS.append({"symlink": rel, "target": target})


def fixture_steps() -> None:
    write("home/.gitconfig", "[alias]\n\tgsw = switch\n\tglob = checkout -q\n")
    write("xdg/git/config", "[alias]\n\txdgco = checkout\n")

    # ---- the cloned-shaped plane
    g(".", "init", "-q", "--bare", "-b", "main", f"{B}/up.git")
    mkdir("plane")
    g("plane", "init", "-q", "-b", "main", ".")
    for rel, text in (("README", "r\n"), ("charter.toml", "schema = 1\n"), ("docs/a.md", "a\n"),
                      ("both", "b\n"), ("a b", "s\n"), ("sub/deep/f", "f\n")):
        write(f"plane/{rel}", text)
    g("plane", "add", "-A")
    g("plane", "commit", "-q", "-m", "one")
    g("plane", "branch", "feature")
    g("plane", "branch", "both")
    g("plane", "tag", "v1")
    g("plane", "remote", "add", "origin", f"{B}/up.git")
    g("plane", "push", "-q", "-u", "origin", "main", "feature")
    g("plane", "branch", "lonely")
    g("plane", "push", "-q", "origin", "lonely")
    g("plane", "branch", "-D", "lonely")          # remote-only: git's DWIM would create it
    g("plane", "remote", "set-head", "origin", "main")
    g("plane", "commit", "-q", "--allow-empty", "-m", "unpushed one")
    g("plane", "commit", "-q", "--allow-empty", "-m", "unpushed two")
    for name, body in (
        ("co", "checkout"), ("ck", "co"), ("sw", "switch -c"), ("wipe", "reset --hard"),
        ("hard", "reset --hard HEAD~2"), ("soft", "reset --soft"), ("sh", "!sh -c 'echo x'"),
        ("gco", "!git checkout"), ("gsh", "!git status"), ("st", "status"),
        ("lpa", "lpb"), ("lpb", "lpa"), ("d1", "d2"), ("d2", "d3"), ("d3", "d4"), ("d4", "d5"),
        ("d5", "checkout"), ("empty", ""), ("bad", "checkout 'unterminated"),
        ("Mixed", "checkout"), ("bang", "!"), ("gitonly", "!git"), ("sp", "  checkout   "),
        ("rs", "reset"), ("dash", "checkout --detach"),
    ):
        g("plane", "config", f"alias.{name}", body)
    # An alias body that is not UTF-8: Python's strict decode raises and the resolver stands
    # aside; a lossy reader would resolve it to `checkout`. The one step that is BYTES.
    STEPS.append({"append_hex": "plane/.git/config",
                  "hex": b"[alias]\n\tnu = checkout \xff\n".hex()})

    # ---- `master`, no remote
    mkdir("plane-master")
    g("plane-master", "init", "-q", "-b", "master", ".")
    write("plane-master/README", "m\n")
    g("plane-master", "add", "-A")
    g("plane-master", "commit", "-q", "-m", "one")
    g("plane-master", "branch", "feature")

    # ---- origin/HEAD -> trunk, and a stale local `main`
    g(".", "init", "-q", "--bare", "-b", "trunk", f"{B}/up2.git")
    mkdir("plane-trunk")
    g("plane-trunk", "init", "-q", "-b", "trunk", ".")
    write("plane-trunk/README", "t\n")
    g("plane-trunk", "add", "-A")
    g("plane-trunk", "commit", "-q", "-m", "one")
    g("plane-trunk", "branch", "main")
    g("plane-trunk", "branch", "feature")
    g("plane-trunk", "remote", "add", "origin", f"{B}/up2.git")
    g("plane-trunk", "push", "-q", "-u", "origin", "trunk")
    g("plane-trunk", "remote", "set-head", "origin", "trunk")

    # ---- no default at all
    mkdir("plane-nodefault")
    g("plane-nodefault", "init", "-q", "-b", "dev", ".")
    write("plane-nodefault/README", "d\n")
    g("plane-nodefault", "add", "-A")
    g("plane-nodefault", "commit", "-q", "-m", "one")
    g("plane-nodefault", "branch", "feature")

    # ---- the directories a workspace clone can be. Their `.git` is written by hand: the guard
    # never runs git in them, it only READS their config (`gitconfig`).
    write("ws/clone/.git/config", "[core]\n\trepositoryformatversion = 0\n")
    write("ws/redirected/.git/config", f"[core]\n\tworktree = {B}/plane\n")
    write("ws/relredirect/.git/config", "[core]\n\tworktree = ../../../plane\n")
    write("ws/linked-gd/config", f"[core]\n\tWorkTree = {B}/plane\n")
    write("ws/linked/.git", "gitdir: ../linked-gd\n")
    write("ws/subsec/.git/config", f'[core "x"]\n\tworktree = {B}/plane\n')
    write("ws/quoted/.git/config", f'[CORE]\n\tworktree = "{B}/plane" ; a comment\n')
    write("ws/oneline/.git/config", f"[core] worktree = {B}/plane\n")
    write("ws/continued/.git/config", f"[core]\n\tworktree = {B}/pl\\\nane\n")
    mkdir("ws/clone/deep")
    mkdir("elsewhere")
    symlink("link-to-plane", f"{B}/plane")
    symlink("ws/up", "..")
    symlink("dangling", f"{B}/nowhere")


fixture_steps()


def run_steps(steps: list[dict]) -> None:
    """Build the fixture from its steps — the executor the Rust replay mirrors line for line."""
    def at(rel: str) -> Path:
        return BASE / rel

    for s in steps:
        if "git" in s:
            subprocess.run(["git", *(a.replace(B, str(BASE)) for a in s["args"])],
                           cwd=at(s["git"]), check=True, capture_output=True)
        elif "mkdir" in s:
            at(s["mkdir"]).mkdir(parents=True, exist_ok=True)
        elif "write" in s:
            at(s["write"]).parent.mkdir(parents=True, exist_ok=True)
            at(s["write"]).write_text(s["text"].replace(B, str(BASE)), encoding="utf-8")
        elif "append_hex" in s:
            with open(at(s["append_hex"]), "ab") as fh:
                fh.write(bytes.fromhex(s["hex"]))
        elif "symlink" in s:
            at(s["symlink"]).symlink_to(s["target"].replace(B, str(BASE)))
        else:
            raise ValueError(f"a fixture step nothing runs: {s}")


run_steps(STEPS)
ORIGINAL_CWD = Path.cwd()
os.chdir(BASE)      # both sides stand here: a relative path resolves against the PROCESS's cwd
os.environ["CHARTER_HOME"] = str(PLANE / ".charter")

from charter import config as charter_config  # noqa: E402
from charter import doctor, gitconfig, hooks  # noqa: E402

# **The plane this run is about, pinned** — the reason stage 4 gave: `config.ROOT` is derived at
# import by walking up from the process's directory, and this checkout lives inside the
# operator's real plane (charter-app#129). Each case moves it to that case's plane.
charter_config.use(PLANE)
assert str(charter_config.ROOT) == str(PLANE), charter_config.ROOT

_REAL_GIT_IN = doctor._git_in


@functools.lru_cache(maxsize=None)
def _asked(root: str, args: tuple[str, ...]):
    try:
        return ("ok", _REAL_GIT_IN(Path(root), *args))
    except Exception as e:  # noqa: BLE001 — the cache holds what the call RAISED, to re-raise
        return ("raised", e)


def _memo_git_in(root, *args):
    kind, value = _asked(str(root), tuple(args))
    if kind == "raised":
        raise value
    return value


# `hooks` imports `_git_in` inside each function, from the module, at call time — so replacing
# the module's attribute is what every guard sees. A pure read of a fixture nothing writes to.
doctor._git_in = _memo_git_in

# --------------------------------------------------------------------------- #
# Probes a case carries                                                          #
# --------------------------------------------------------------------------- #

CWDS = [f"{B}/plane", f"{B}/plane/docs", f"{B}/plane/sub/deep", f"{B}/plane/.git",
        f"{B}/ws/clone", f"{B}/ws/clone/deep", f"{B}/ws/redirected", f"{B}/ws/relredirect",
        f"{B}/ws/linked", f"{B}/ws/subsec", f"{B}/ws/quoted", f"{B}/ws/oneline",
        f"{B}/ws/continued", f"{B}/elsewhere", f"{B}/link-to-plane", "", "plane", "ws/up/plane",
        f"{B}/plane-master", f"{B}/plane-trunk", f"{B}/plane-nodefault"]

#: `(dir, named git dir)` pairs `configured_work_tree` is asked about, two per case.
CWT_PROBES = [(c, None) for c in CWDS] + [
    (f"{B}/elsewhere", f"{B}/ws/redirected/.git"), (f"{B}/elsewhere", "ws/relredirect/.git"),
    (f"{B}/plane", f"{B}/ws/linked-gd"), (f"{B}/plane", f"{B}/nowhere/.git"),
    (f"{B}/elsewhere", f"{B}/ws/subsec/.git"), (f"{B}/nowhere/at/all", None),
    (f"{B}/dangling", None), (f"{B}/ws/linked/.", None), (f"{B}/ws/linked/../linked", None),
]

#: Operands `_checkout_operand_kind` is asked about besides the case's own.
#:
#: **No re-cased names** (`BOTH`, `readme`), and that is a limit, stated: on a case-insensitive
#: filesystem git answers them as `both` and `README` (a loose ref is a file, and `git init` sets
#: `core.ignorecase`), on Linux it does not, and the recorded corpus has to be one file on both.
#: The fuzz would compare the two implementations on ONE machine either way — the corpus is what
#: cannot carry them.
OPERANDS = ["feature", "main", "master", "trunk", "dev", "README", "both", "lonely", "nosuch",
            "-", "HEAD", "HEAD~1", "origin/main", "v1", ".", "docs/a.md", "docs/*.md", ":/",
            "a b", "charter.toml", "@{upstream}", "--", "-q", "sub", "README^", "feature:README",
            "origin/lonely", "$BR", "*", ""]

#: Reset targets `_unpushed_at_risk` is asked about besides the case's own.
TARGETS = ["origin/main", "HEAD", "HEAD~1", "HEAD~2", "HEAD~3", "README", "feature", "nosuch",
           "@{upstream}", "v1", "main", "master", "trunk", "origin/trunk", "-", "--", "HEAD^"]

#: Strings `pathlib`'s arithmetic is put to.
PATHS = ["", ".", "..", "/", "//", "///", "a", "a/", "a//b", "a/./b", "./a", "a/..", "//a/./b/",
         "/a/b/../c", "a/.", ".//", "/.", "//.", "../a/", "a b/c"]


def rotation(n: int, table: list, count: int) -> list:
    """`count` rows of `table` starting at `n` — the length of the case AS WRITTEN, with `@B@`
    in it, so the rows a case is probed with are the same on every machine."""
    return [table[(n + k) % len(table)] for k in range(count)]


def sub_base(s):
    return s.replace(B, str(BASE)) if isinstance(s, str) else s


def normalise(v):
    """The fixture's temporary directory as `@B@`, and a config probe's scratch directory as
    `@CFG@` — the Rust writes one per thread, the Python one per run."""
    if isinstance(v, str):
        s = v.replace(str(BASE), B)
        return re.sub(r"@B@/cfg(?:py|rs-\d+)", "@CFG@", s)
    if isinstance(v, (list, tuple)):
        return [normalise(x) for x in v]
    if isinstance(v, dict):
        return {k: normalise(x) for k, x in v.items()}
    return v


# --------------------------------------------------------------------------- #
# The oracle                                                                     #
# --------------------------------------------------------------------------- #

def tables(rot: int) -> list:
    # The frozensets have no order and the Rust's arrays have the source's, so every set is
    # compared SORTED. Order is not part of what any of them means: each is only ever asked
    # "is this in you".
    known = sorted(hooks._GIT_KNOWN_SUBCOMMANDS)
    return [
        list(hooks._BRANCH_MOVERS),
        sorted(hooks._BRANCH_CREATOR_OPTS),
        sorted(hooks._DETACH_OPTS),
        sorted(hooks._RESTORE_OPTS),
        sorted(hooks._RESTORE_SHORTS),
        hooks._MAX_CHECKOUT_OPERANDS,
        list(hooks._RESET_TREE_MODES),
        [[k, v] for k, v in hooks._GIT_DIR_ENV.items()],
        [len(known), rotation(rot, known, 4)],
        hooks._MAX_ALIAS_HOPS,
        gitconfig.MAX_CONFIG_BYTES,
    ]


def derive(post: list[str]):
    opts = [a for a in post if a.startswith("-") and a not in ("-", "--")]
    classes = [hooks._checkout_opt_kind(o) for o in opts]
    wants = [a for a in post if a == "-" or not a.startswith("-")]
    return opts, classes, wants


def config_probe(where: Path, text: str, as_pointer: bool):
    import shutil
    shutil.rmtree(where, ignore_errors=True)
    where.mkdir(parents=True)
    if as_pointer:
        (where / ".git").write_text(text, encoding="utf-8")
    else:
        (where / ".git").mkdir()
        (where / ".git" / "config").write_text(text, encoding="utf-8")
    got = gitconfig.configured_work_tree(where, None)
    return None if got is None else str(got)


def request(case: dict) -> dict:
    """The case as the Rust is handed it: paths substituted, probes derived."""
    cmd, cwd = sub_base(case["cmd"]), sub_base(case["cwd"])
    root = str(PLANES[case["plane"]])
    segs, _parsed = hooks._segment_argv_parsed(cmd)
    opts = [t for seg in segs for t in seg if t.startswith("-") and t not in ("-", "--")][:12]
    prg = list(hooks._plane_root_git(cmd, cwd, Path(root).resolve()))
    ops, targets = [], []
    for _sub, post, _pre in prg:
        ops += [a for a in post if a == "-" or not a.startswith("-")][:3]
        targets += [a for a in post if not a.startswith("-")][:2]
    rot = len(case["cmd"])
    ops = (ops[:4] + rotation(rot, OPERANDS, 2))
    targets = (targets[:2] + rotation(rot, TARGETS, 1))
    toks = [t for seg in segs for t in seg][:3]
    pp = [[a, b] for a, b in zip(toks + rotation(rot, PATHS, 2), rotation(rot + 7, PATHS, 5))]
    cwt = [[sub_base(d), sub_base(n)] for d, n in rotation(rot, CWT_PROBES, 2)]
    return {"cmd": cmd, "cwd": cwd, "root": root, "cfg": sub_base(case["cfg"]), "git": case["git"],
            "rot": rot,
            "opts": opts, "ops": ops, "targets": targets, "pp": pp, "cwt": cwt}


def oracle(req: dict) -> dict:
    """What the frozen Python says about one request."""
    cmd, cwd, cfg = req["cmd"], req["cwd"], req["cfg"]
    charter_config.ROOT = req["root"]
    root = Path(req["root"]).resolve()
    segs, _parsed = hooks._segment_argv_parsed(cmd)
    gt = []
    for toks, before in zip(segs, hooks._exported_env(segs)):
        prog, env, argv = hooks._split_env(toks)
        if os.path.basename(prog or "") != "git":
            continue
        pre, _rest = hooks._git_globals(argv)
        gt.append([[str(t) for t in hooks._git_target(cwd, pre, before + env)],
                   [[k, v] for k, v in hooks._inline_aliases(pre).items()]])
    prg = list(hooks._plane_root_git(cmd, cwd, root))
    out = {
        "tbl": tables(req["rot"]),
        "cok": [[o, hooks._checkout_opt_kind(o)] for o in req["opts"]],
        "gt": gt,
        "prg": [[s, list(p), list(pre)] for s, p, pre in prg],
        "cb": [hooks._created_branch(*derive(p)) for _s, p, _pre in prg],
        "cwt": [(lambda r: None if r is None else str(r))(
            gitconfig.configured_work_tree(Path(d), None if n is None else Path(n)))
            for d, n in req["cwt"]],
        "pp": [[str(Path(a)), str(Path(a) / b)] for a, b in req["pp"]],
        "psl": cfg.splitlines(),
        "gcl": list(gitconfig._logical_lines(cfg)),
        "gcp": [list(t) for t in gitconfig._pairs(cfg)],
        "gcs": gitconfig._scalar(cfg),
        "ccw": config_probe(BASE / "cfgpy", cfg, False),
        "ccp": config_probe(BASE / "cfgpy.f", cfg, True),
    }
    if req["git"]:
        upr = []
        for t in req["targets"]:
            r = hooks._unpushed_at_risk(root, t)
            upr.append([t, None if r is None else list(r)])
        out.update({
            "rga": [list(hooks._resolve_git_alias(root, s, list(p), list(pre)))
                    for s, p, pre in prg],
            "okind": [[w, hooks._checkout_operand_kind(root, w)] for w in req["ops"]],
            "upr": upr,
            "pdb": doctor._plane_default_branch(root),
            "bra": hooks._plane_root_branch_reason(cmd, cwd),
            "rst": hooks._plane_root_reset_reason(cmd, cwd),
        })
    return normalise(json.loads(json.dumps(out)))


# --------------------------------------------------------------------------- #
# The curated corpus: every command line a docstring names as a bypass that     #
# shipped, a rule a review round pinned, or a defect filed from this port        #
# --------------------------------------------------------------------------- #

def _c(cmd: str, cwd: str = f"{B}/plane", plane: int = 0, cfg: str = "") -> dict:
    return {"cmd": cmd, "cwd": cwd, "plane": plane, "cfg": cfg, "git": True}


P = f"{B}/plane"
E = f"{B}/elsewhere"
CURATED = [
    # ---- the branch guard's plain shapes, and the remedy (#157)
    _c("git checkout feature"), _c("git switch feature"), _c("git checkout main"),
    _c("git switch main"), _c("git checkout -f main"), _c("git checkout -q main"),
    _c("git checkout"), _c("git status"), _c("git commit -m x"), _c("echo git checkout feature"),
    # ---- #461: a file restore is not a branch move, and the ambiguous case says so
    _c("git checkout charter.toml"), _c("git checkout README"), _c("git checkout both"),
    _c("git checkout lonely"), _c("git checkout nosuch"), _c("git checkout -- README"),
    _c("git checkout -- nosuch"), _c("git checkout feature README"),
    _c("git checkout feature nosuch"), _c("git checkout feature -- README"),
    _c("git checkout --ours README"), _c("git checkout -2 README"), _c("git checkout ."),
    _c("git checkout 'docs/*.md'"), _c("git checkout :/"), _c("git checkout 'a b'"),
    _c("git checkout $(echo feature)"), _c('git checkout "$BR"'), _c("git checkout -"),
    _c("git checkout HEAD~1"), _c("git checkout v1"), _c("git checkout origin/main"),
    # ---- the round-one bypasses: an option decides what the operand means
    _c("git checkout --orphan README"), _c("git checkout --orphan=README"),
    _c("git checkout -bREADME"), _c("git checkout -qbREADME"), _c("git checkout -b neu"),
    _c("git checkout -B neu"), _c("git switch -c neu"), _c("git switch -C neu"),
    _c("git switch --create neu"), _c("git switch --force-create=neu"),
    _c("git checkout --no-orphan README"), _c("git checkout --track README"),
    _c("git checkout --guess README"), _c("git checkout -t origin/lonely"),
    _c("git checkout -fq README"), _c("git checkout --no-quiet README"),
    _c("git checkout -b"), _c("git checkout --orphan="), _c("git checkout -b ''"),
    # ---- `--detach` and `-d`, with and without the default branch beside them
    _c("git checkout --detach"), _c("git switch --detach"), _c("git checkout --detach main"),
    _c("git switch -d main"), _c("git checkout -qd main"), _c("git checkout -dq main"),
    _c("git checkout --detach main --"), _c("git switch --detach -- main"),
    _c("git checkout -d feature"), _c("git dash main"),
    # ---- `--` separates refs from PATHS, and only what follows it counts
    _c("git checkout feature --"), _c("git checkout -b neu --"), _c("git checkout -b neu -- README"),
    _c("git switch -- feature"), _c("git switch -q -- feature"), _c("git checkout main --"),
    # ---- #183: the workflow the denial recommends
    _c(f"cd {E} && git checkout -b x"), _c("cd docs && git checkout feature"),
    _c(f"cd {E}; git checkout feature", cwd=E), _c(f"cd {P} && git checkout feature", cwd=E),
    # ---- #477 / #483 / #504: every subject a git invocation can name
    _c(f"git -C {P} checkout feature", cwd=E), _c(f"git -C {P} switch -C neu", cwd=E),
    _c("git -C ../.. checkout feature", cwd=f"{B}/plane/sub/deep"),
    _c("git -C ../.. -C . checkout feature", cwd=f"{B}/plane/sub/deep"),
    _c("git -C '' checkout feature"), _c("git -C"), _c(f"git -C{P} checkout feature", cwd=E),
    _c(f"git --git-dir {P}/.git checkout feature", cwd=E),
    _c(f"git --git-dir={P}/.git/refs/.. reset --hard origin/main", cwd=E),
    _c(f"git --work-tree={E} reset --hard origin/main"),
    _c(f"git --work-tree {P} checkout feature", cwd=E),
    _c(f"git --git-dir={E}/.git --work-tree={E} checkout feature"),
    _c(f"GIT_DIR={P}/.git git checkout feature", cwd=E),
    _c(f"GIT_WORK_TREE={P} git checkout feature", cwd=E),
    _c(f"export GIT_DIR={P}/.git && git checkout feature", cwd=E),
    _c(f"GIT_DIR={P}/.git; export GIT_DIR; git checkout feature", cwd=E),
    _c(f"set -a; GIT_DIR={P}/.git; git checkout feature", cwd=E),
    _c(f"declare -x GIT_DIR={P}/.git; git checkout feature", cwd=E),
    _c(f"declare GIT_DIR={P}/.git; git checkout feature", cwd=E),
    _c(f"git -C {B}/link-to-plane checkout feature", cwd=E),
    _c("git checkout feature", cwd=f"{B}/ws/redirected"),
    _c("git checkout feature", cwd=f"{B}/ws/relredirect"),
    _c("git checkout feature", cwd=f"{B}/ws/linked"),
    _c("git checkout feature", cwd=f"{B}/ws/subsec"),
    _c("git checkout feature", cwd=f"{B}/ws/quoted"),
    _c("git checkout feature", cwd=f"{B}/ws/oneline"),
    _c("git checkout feature", cwd=f"{B}/ws/continued"),
    _c("git checkout feature", cwd=f"{B}/ws/clone"),
    _c(f"git --git-dir={B}/ws/redirected/.git checkout feature", cwd=E),
    _c("git -c commit.gpgsign=false reset --hard origin/main"),
    _c("git -c core.x=y checkout feature"), _c("git --namespace x checkout feature"),
    _c("env git checkout feature"), _c("sudo git checkout feature"),
    _c("( git checkout feature )"), _c("git status && git checkout feature"),
    _c("git checkout 'feature"),     # does not tokenize: the plane-root walk stands aside
    # ---- aliases (#461 round two, #467)
    _c("git co feature"), _c("git ck feature"), _c("git sw neu"), _c("git gco feature"),
    _c("git gsh"), _c("git sh feature"), _c("git st"), _c("git lpa feature"),
    _c("git d1 feature"), _c("git empty feature"), _c("git bad feature"), _c("git mixed feature"),
    _c("git Mixed feature"), _c("git bang"), _c("git gitonly"), _c("git sp feature"),
    _c("git nu feature"), _c("git gsw feature"), _c("git glob feature"), _c("git xdgco feature"),
    _c("git envco feature"), _c("git co main"), _c("git co README"), _c("git co --orphan README"),
    _c("git -c alias.zz=checkout zz feature"), _c("git -c alias.zz='checkout -b' zz neu"),
    _c("git -c alias.zz=co zz feature"), _c("git -c alias.zz=checkout -c alias.zz=status zz x"),
    _c("git -c alias.z='reset --hard origin/main' z"), _c("git -calias.zz=checkout zz feature"),
    _c("git wipe origin/main"), _c("git hard"), _c("git soft origin/main"), _c("git rs --hard HEAD~1"),
    # ---- the reset guard (#401): only a destroying mode, only what is really lost
    _c("git reset --hard origin/main"), _c("git reset --merge origin/main"),
    _c("git reset --keep HEAD~1"), _c("git reset --hard HEAD~1"), _c("git reset --hard HEAD~2"),
    _c("git reset --hard HEAD"), _c("git reset --hard"), _c("git reset --soft HEAD~1"),
    _c("git reset --mixed origin/main"), _c("git reset HEAD -- README"),
    _c("git reset --hard README"), _c("git reset --hard origin/main -- README"),
    _c("git reset --hard origin/main --"), _c("git reset --hard feature"),
    _c("git reset --hard nosuch"), _c("git reset -q --hard origin/main"),
    _c("echo reset --hard origin/main"), _c("git reset --hard origin/main", cwd=E),
    _c("git reset --hard HEAD~1", plane=1, cwd=f"{B}/plane-master"),
    _c("git reset --hard origin/trunk", plane=2, cwd=f"{B}/plane-trunk"),
    # ---- the other planes: which branch is the remedy
    _c("git checkout master", plane=1, cwd=f"{B}/plane-master"),
    _c("git checkout feature", plane=1, cwd=f"{B}/plane-master"),
    _c("git checkout trunk", plane=2, cwd=f"{B}/plane-trunk"),
    _c("git checkout main", plane=2, cwd=f"{B}/plane-trunk"),
    _c("git checkout dev", plane=3, cwd=f"{B}/plane-nodefault"),
    _c("git checkout feature", plane=3, cwd=f"{B}/plane-nodefault"),
    _c("git checkout -b neu", plane=3, cwd=f"{B}/plane-nodefault"),
    # ---- charter#1176: git and the root recognised by SPELLING (reproduced, pinned)
    _c("GIT checkout feature"), _c("/usr/bin/git checkout feature"),
    _c(f"git -C {B}/PLANE checkout feature", cwd=E),
    _c("git -c alias.ZZ=checkout zz feature"), _c("git -c alias.zz=checkout ZZ feature"),
    # ---- charter#1177: a `cd` that fails still moves the guard (reproduced, pinned)
    _c("cd /nonexistent-dir || git checkout feature"),
    _c("cd /nonexistent-dir; git checkout feature"),
    _c("cd /nonexistent-dir; git reset --hard origin/main"),
    # ---- the config reader, through a clone's own config text
    _c("true", cfg="[core]\n\tworktree = /abs\n"),
    _c("true", cfg='[core]\n\tworktree = "/pa th  " # c\n'),
    _c("true", cfg="[core \"x\"]\n\tworktree = /x\n[core]\n\tworktree = rel/../y\n"),
    _c("true", cfg="[core]\n\tworktree = a\\\n b\n"),
    _c("true", cfg="[core]\n\tworktree = a\\\\\n"),
    _c("true", cfg="[core]\n\tworktree = \"a\\tb\\n\\\"c\\\\\"\n"),
    _c("true", cfg="[core]\n\tworktree\n\tworktree =\n"),
    _c("true", cfg="[Core]\r\n\tWORKTREE = /a\r\n"),
    _c("true", cfg="[core\n\tworktree = /no-header-end\n"),
    _c("true", cfg="; [core]\n# worktree = /c\n[core] ; x\n\tworktree = /d ; e\n"),
    _c("true", cfg="[core]\x1c\tworktree = /sep\x1f "),
    _c("true", cfg="gitdir: ../plane/.git\n"),
    _c("true", cfg="GitDir:   ws/redirected/.git  \nsecond line"),
    _c("true", cfg=f"gitdir: {B}/ws/linked-gd"),
    _c("true", cfg="gitdir:\n"),
    _c("true", cfg="no colon here\n"),
    _c("true", cfg="[core]\n\tworktree = éàİ\n"),
]

# --------------------------------------------------------------------------- #
# The generators                                                                 #
# --------------------------------------------------------------------------- #

PREFIXES = ["", "", "", "", "cd docs && ", f"cd {E} && ", f"cd {E}; ", f"cd {P} && ",
            "cd /nonexistent-dir || ", f"export GIT_DIR={P}/.git && ", f"GIT_DIR={P}/.git ",
            f"GIT_WORK_TREE={P} ", "FOO=1; export FOO && ", f"set -a; GIT_DIR={P}/.git; ",
            "env ", "sudo ", "( ", "true && ", f"declare -x GIT_WORK_TREE={P}; ",
            f"GIT_DIR={E}/.git ", "cd .. && ", "cd sub && cd deep && "]
GITS = ["git", "git", "git", "git", "/usr/bin/git", "GIT", "./git", "git"]
GLOBALS = ["", "", "", "", f" -C {P}", " -C ..", f" -C {B}/link-to-plane", f" -C {B}/PLANE",
           f" --git-dir={P}/.git", f" --git-dir {P}/.git/refs/..", f" --work-tree={P}",
           f" --work-tree {E}", " -c alias.zz=checkout", " -c alias.ZZ=checkout",
           " -c alias.zz='reset --hard'", " -c alias.zz='checkout -b'", " -c core.x=y",
           " -C ''", " --namespace x", " -c commit.gpgsign=false", f" -C {B}/ws/redirected",
           f" --git-dir={B}/ws/redirected/.git", f" -C {E} -C {P}", " -C ../.."]
SUBS = ["checkout", "checkout", "checkout", "switch", "switch", "co", "ck", "sw", "gco", "gsh",
        "sh", "st", "status", "lpa", "d1", "empty", "bad", "mixed", "Mixed", "nu", "zz", "ZZ",
        "gsw", "glob", "xdgco", "envco", "commit", "log", "restore", "reset", "reset", "wipe",
        "hard", "soft", "rs", "dash", "bang", "gitonly", "sp", "CHECKOUT", "nosuchalias"]
OPTS = ["", "", "", "", " -b", " -bREADME", " -B x", " --orphan", " --orphan=README", " -c",
        " -C", " --detach", " -d", " -qd", " -dq", " -f", " -q", " -fq", " --ours", " --track",
        " -t", " --guess", " --no-orphan", " --no-quiet", " --force", " -p", " -2",
        " --merge", " --conflict=diff3", " --hard", " --hard", " --soft", " --keep", " --mixed",
        " --create", " --force-create=x", " -qbx", " --no-", " -", " -m"]
OPERANDS_GEN = ["", "", " feature", " main", " master", " trunk", " dev", " README", " both",
                " lonely", " nosuch", " -", " HEAD", " HEAD~1", " HEAD~2", " origin/main", " v1",
                " .", " docs/a.md", " 'docs/*.md'", " :/", " 'a b'", ' "$BR"', " $(echo feature)",
                " README docs/a.md", " feature README", " feature nosuch", " @{upstream}",
                " charter.toml", " origin/trunk", " ''", " feature feature feature", " both", " both"]
SEPS = ["", "", "", " --", " -- README", " -- nosuch", " -- README docs/a.md"]
TAILS = ["", "", "", "; git status", " && git log", " | cat", " &", "\ngit checkout feature",
         " 2>/dev/null", " || true", "; git reset --hard origin/main"]

CFG_PIECES = ["[core]", "[CORE]", '[core "x"]', "[core ", "[alias]", "[core] worktree = a",
              "\tworktree = ", " worktree=", "WorkTree =", "/abs/path", "../rel", "rel",
              '"quoted  "', '"a;b"', "\\\n", "\\\\", "\\t", '\\"', "\\x", "# c", "; c", "\n",
              "\r\n", "\r", " ", "\x1c", "\x0b", " ", "  ", "\t", "=", "gitdir: ../x",
              "GITDIR:", "gitdir: ", "worktree", "é", "İ", "K", "#", ";", '"']

#: A `.git` FILE's first line, pointing somewhere real — relative to the scratch directory, which
#: sits directly in the fixture's base on both sides.
POINTERS = ["gitdir: ../ws/redirected/.git", "gitdir: ../ws/linked-gd", "GitDir:  ../ws/subsec/.git ",
            "gitdir: ../ws/clone/.git", "gitdir: ../plane/.git", "gitdir:../ws/quoted/.git",
            "gitdir: ws/redirected/.git", "gitdir ../ws/redirected/.git"]


def a_config(rng: random.Random) -> str:
    r = rng.random()
    if r < 0.25:
        return ""
    if r < 0.40:
        return rng.choice(POINTERS) + rng.choice(("", "\n", "\r\nx", " y", "\x0c"))
    return "".join(rng.choice(CFG_PIECES) for _ in range(rng.randint(1, 10)))


#: Each plane's default branch, and a name that is not it — the remedy family's operands.
DEFAULTS = ["main", "master", "trunk", "dev"]
RESTORE_ONLY = ["", "", "", " -f", " -q", " -fq", " --quiet", " --force", " -m"]
RESET_MODES = [" --hard", " --hard", " --merge", " --keep", " --soft", " --mixed", "", " -q --hard"]
RESET_TARGETS = ["", "", "", " origin/main", " HEAD", " HEAD~1", " HEAD~2", " HEAD~3", " feature", " README",
                 " nosuch", " @{upstream}", " origin/trunk", " v1", " -- README", " origin/main --",
                 " HEAD -- README docs/a.md"]
BREAKAGE = ["", "", "", "", "", "", "", "", "", "", "", "", "", " '", ' "', " $(", " `x"]


def a_command(rng: random.Random, plane: int) -> str:
    """A git command line built from the grammar the two guards read — the same lesson stage 4
    measured: a random join reaches a plane-root branch move about as often as it opens a heredoc.

    Families, because one shared grammar left the rarest branches at a tenth of a percent: the
    documented REMEDY (the plane's own default branch, with only restore-shaped options), the
    RESET guard's modes and targets, an alias defined INLINE and then used, and the general
    grammar for the rest."""
    r = rng.random()
    if r < 0.08:
        # A fragment join, for everything the grammar does not think of.
        pool = PREFIXES + GITS + GLOBALS + SUBS + OPTS + OPERANDS_GEN + SEPS + TAILS + BREAKAGE
        return "".join(rng.choice(pool) for _ in range(rng.randint(1, 7))).strip()
    if r < 0.18:
        name = DEFAULTS[plane] if rng.random() < 0.7 else rng.choice(DEFAULTS)
        verb = rng.choice(("checkout", "switch", "co", "gsw", "xdgco", "checkout"))
        operand = rng.choice((f" {name}", f" {name}", "", f" {name} --", f" -- {name}"))
        return (f"{rng.choice(PREFIXES[:4])}git{rng.choice(GLOBALS[:6])} {verb}"
                f"{rng.choice(RESTORE_ONLY)}{operand}{rng.choice(BREAKAGE)}")
    if r < 0.30:
        verb = rng.choice(("reset", "reset", "reset", "wipe", "rs", "hard", "soft"))
        return (f"{rng.choice(PREFIXES)}{rng.choice(GITS)}{rng.choice(GLOBALS)} {verb}"
                f"{rng.choice(RESET_MODES)}{rng.choice(RESET_TARGETS)}{rng.choice(TAILS)}")
    if r < 0.36:
        body = rng.choice(("checkout", "'checkout -b'", "co", "'reset --hard'", "status",
                           "'!git checkout'", "'!sh -c x'", "switch", "'switch -d'"))
        name = rng.choice(("zz", "zz", "ZZ", "Zz", "alias2"))
        used = rng.choice((name, name, name.lower(), "zz"))
        return (f"git -c alias.{name}={body}{rng.choice(GLOBALS[:5])} {used}"
                f"{rng.choice(OPTS)}{rng.choice(OPERANDS_GEN)}{rng.choice(SEPS)}")
    parts = [rng.choice(PREFIXES), rng.choice(GITS), rng.choice(GLOBALS), " ", rng.choice(SUBS)]
    for _ in range(rng.choice((0, 1, 1, 2))):
        parts.append(rng.choice(OPTS))
    parts.append(rng.choice(OPERANDS_GEN))
    parts.append(rng.choice(SEPS))
    parts.append(rng.choice(TAILS))
    parts.append(rng.choice(BREAKAGE))
    cmd = "".join(parts)
    if cmd.startswith("( "):
        cmd += " )"
    return cmd


def a_case(rng: random.Random, git: bool) -> dict:
    plane = 0 if rng.random() < 0.7 else rng.randint(1, 3)
    if rng.random() < 0.55:
        cwd = str(PLANES[plane]).replace(str(BASE), B)
    else:
        cwd = rng.choice(CWDS)
    cmd = a_command(rng, plane)
    if plane:
        # A plane other than the first is reached through ITS path, not `plane`'s.
        name = PLANES[plane].name
        cmd = cmd.replace(f"{B}/plane/", f"{B}/{name}/").replace(f"{B}/plane ", f"{B}/{name} ")
    return {"cmd": cmd, "cwd": cwd, "plane": plane, "cfg": a_config(rng), "git": git}


def cases_for(count: int, seed: int, every: int) -> list[dict]:
    rng = random.Random(seed)
    out = [dict(c) for c in CURATED]
    for i in range(max(0, count - len(out))):
        out.append(a_case(rng, i % every == 0))
    return out


# --------------------------------------------------------------------------- #
# What the fuzz REACHES                                                          #
# --------------------------------------------------------------------------- #

COVERAGE_BRANCHES = (
    "git-seg", "root-invocation", "via-cwd", "via-C", "via-git-dir", "via-work-tree",
    "via-env", "via-export", "via-core-worktree", "via-cd", "unparseable",
    "alias-asked", "alias-resolved", "alias-inline", "alias-shell-git", "alias-shell-other",
    "mover", "creating", "detaching", "unplaced", "restore-opts", "sep-paths", "sep-trailing",
    "bare", "remedy", "kind-path", "kind-both", "kind-neither", "multi-operand",
    "a3-denied", "a3-denied-generic",
    "reset", "reset-tree-mode", "reset-sep-paths", "reset-no-target", "reset-at-risk",
    "a3b-denied", "default-origin-head", "default-guessed", "default-none",
    "cfg-worktree", "cfg-pointer",
)


def reached(case: dict) -> set[str]:
    """The branches of A3 and A3b one case gets to, asked of the Python."""
    req = request(case)
    cmd, cwd = req["cmd"], req["cwd"]
    charter_config.ROOT = req["root"]
    root = Path(req["root"]).resolve()
    hit: set[str] = set()
    segs, parsed = hooks._segment_argv_parsed(cmd)
    if not parsed:
        hit.add("unparseable")
    befores = hooks._exported_env(segs)
    here = cwd
    for toks, before in zip(segs, befores):
        prog, env, argv = hooks._split_env(toks)
        base = os.path.basename(prog or "")
        if base == "cd":
            dest = next((a for a in argv[1:] if not a.startswith("-")), None)
            if dest:
                here = str(Path(here or ".") / dest) if not os.path.isabs(dest) else dest
            continue
        if base != "git":
            continue
        pre, rest = hooks._git_globals(argv)
        if not rest:
            continue
        hit.add("git-seg")
        subjects = hooks._git_target(here, pre, before + env)
        on = [t for t in subjects if Path(t).resolve() == root]
        if not on:
            continue
        hit.add("root-invocation")
        if here != cwd:
            hit.add("via-cd")
        if Path(cwd or ".").resolve() == root and here == cwd and "-C" not in pre:
            hit.add("via-cwd")
        if "-C" in pre:
            hit.add("via-C")
        if any(p.startswith("--git-dir") for p in pre):
            hit.add("via-git-dir")
        if any(p.startswith("--work-tree") for p in pre):
            hit.add("via-work-tree")
        if any(e.startswith(("GIT_DIR=", "GIT_WORK_TREE=")) for e in env):
            hit.add("via-env")
        if any(e.startswith(("GIT_DIR=", "GIT_WORK_TREE=")) for e in before):
            hit.add("via-export")
        conf = gitconfig.configured_work_tree(Path(here or "."), None)
        if conf is not None and conf.resolve() == root:
            hit.add("via-core-worktree")
        sub, post = rest[0], rest[1:]
        if sub not in hooks._BRANCH_MOVERS and sub not in hooks._GIT_KNOWN_SUBCOMMANDS:
            hit.add("alias-asked")
            if sub in hooks._inline_aliases(pre):
                hit.add("alias-inline")
            nsub, npost = hooks._resolve_git_alias(root, sub, post, pre)
            if nsub != sub:
                hit.add("alias-resolved")
            body = hooks._inline_aliases(pre).get(sub)
            if body is None:
                try:
                    r = doctor._git_in(root, "config", "--get", f"alias.{sub}")
                    body = r.stdout.strip() if r.returncode == 0 else ""
                except ValueError:
                    body = ""        # `nu`: a body that is not UTF-8

            if body.startswith("!git"):
                hit.add("alias-shell-git")
            elif body.startswith("!"):
                hit.add("alias-shell-other")
            sub, post = nsub, npost
        if sub in hooks._BRANCH_MOVERS:
            hit.add("mover")
            opts, classes, wants = derive(post)
            if "create" in classes:
                hit.add("creating")
            if "detach" in classes:
                hit.add("detaching")
            if "unknown" in classes:
                hit.add("unplaced")
            if classes and all(c == "restore" for c in classes):
                hit.add("restore-opts")
            if sub == "checkout" and "--" in post:
                hit.add("sep-paths" if post[post.index("--") + 1:] else "sep-trailing")
            if not wants and not classes:
                hit.add("bare")
            default = doctor._plane_default_branch(root)
            if wants and default is not None and wants[0] == default and all(
                    c == "restore" for c in classes):
                hit.add("remedy")
            if sub == "checkout" and wants and all(c == "restore" for c in classes):
                k = hooks._checkout_operand_kind(root, wants[0])
                hit.add({"path": "kind-path", "both": "kind-both",
                         "neither": "kind-neither"}.get(k, "kind-other"))
                if len(wants) > 1:
                    hit.add("multi-operand")
        if sub == "reset":
            hit.add("reset")
            if any(a in hooks._RESET_TREE_MODES for a in post):
                hit.add("reset-tree-mode")
                if "--" in post and post[post.index("--") + 1:]:
                    hit.add("reset-sep-paths")
                elif not [a for a in post if not a.startswith("-") and a != "--"]:
                    hit.add("reset-no-target")
                else:
                    t = next(a for a in post if not a.startswith("-"))
                    if hooks._unpushed_at_risk(root, t):
                        hit.add("reset-at-risk")
    said = hooks._plane_root_branch_reason(cmd, cwd)
    if said:
        hit.add("a3-denied")
        if said.startswith("would "):
            hit.add("a3-denied-generic")
    if hooks._plane_root_reset_reason(cmd, cwd):
        hit.add("a3b-denied")
    ref = doctor._git_in(root, "symbolic-ref", "--quiet", "--short", "refs/remotes/origin/HEAD")
    default = doctor._plane_default_branch(root)
    if ref.returncode == 0:
        hit.add("default-origin-head")
    elif default is not None:
        hit.add("default-guessed")
    else:
        hit.add("default-none")
    if req["cfg"] and config_probe(BASE / "cfgpy", req["cfg"], False) is not None:
        hit.add("cfg-worktree")
    if req["cfg"] and config_probe(BASE / "cfgpy.f", req["cfg"], True) is not None:
        hit.add("cfg-pointer")
    return hit


# --------------------------------------------------------------------------- #
# What is compared, and whether that is everything                              #
# --------------------------------------------------------------------------- #

KEYS = ("tbl", "cok", "gt", "prg", "cb", "cwt", "pp", "psl", "gcl", "gcp", "gcs", "ccw", "ccp",
        "rga", "okind", "upr", "pdb", "bra", "rst")

#: The `pub` items of the modules below that the example does NOT name, and the key that covers
#: each. Asserted by :func:`surface`, in both directions, as `shellseg.py` asserts its own.
COVERED_ELSEWHERE = {
    "planeroot::RootInvocation": "prg — every field of it is in that answer",
    "planeroot::OperandKind": "okind — every variant's string is that answer",
    "gitconfig::MAX_CONFIG_BYTES": "tbl",
}

#: `module::item` for every item this harness is the evidence for: the two modules whole, plus
#: what stage 5 added to modules another harness owns.
GUARD_MODULES = ("planeroot", "gitconfig")
EXTRA_ITEMS = ("pypath::pure_path", "pypath::path_div", "worktree::git::run_as_session",
               "worktree::git::RawRun", "worktree::git::CONFIG_LOCATION_ENV")
EXTRA_COVERED = {
    "worktree::git::run_as_session": "rga, okind, upr, pdb, bra, rst — every git question the "
                                     "guards ask goes through it",
    "worktree::git::RawRun": "rga — the alias whose body is not UTF-8 (`nu`) is told apart "
                             "from a lossy reading only through it",
    "worktree::git::CONFIG_LOCATION_ENV": "rga, bra — `xdgco` and `envco` are aliases only "
                                          "that pass-through lets the Rust's git see",
}

_PUB_RE = re.compile(
    r"^\s*pub\s+(?:fn|const|static|struct|enum|type)\s+([A-Za-z_][A-Za-z0-9_]*)", re.M)
_LINE_COMMENT_RE = re.compile(r"^[ \t]*//.*$", re.M)


def _pub_items() -> list[str]:
    out = []
    src = REPO / "crates" / "charter-core" / "src"
    for mod in GUARD_MODULES:
        text = (src / f"{mod}.rs").read_text(encoding="utf-8")
        text = text.split("\n#[cfg(test)]")[0]
        out += [f"{mod}::{n}" for n in _PUB_RE.findall(_LINE_COMMENT_RE.sub("", text))]
    for item in EXTRA_ITEMS:
        *path, name = item.split("::")
        text = (src / ("/".join(path) + ".rs")).read_text(encoding="utf-8")
        if not re.search(rf"^\s*pub\s+(?:fn|const|static|struct|enum|type)\s+{name}\b", text, re.M):
            out.append(f"{item} (MISSING)")
        else:
            out.append(item)
    return out


def surface() -> list[str]:
    example = _LINE_COMMENT_RE.sub("", (
        REPO / "crates" / "charter-core" / "examples" / "planeroot_oracle.rs"
    ).read_text(encoding="utf-8"))
    covered = {**COVERED_ELSEWHERE, **EXTRA_COVERED}
    items = _pub_items()
    bad = []
    for item in items:
        if item.endswith("(MISSING)"):
            bad.append(f"{item}: EXTRA_ITEMS names it and no module has it any more")
            continue
        name = item.rsplit("::", 1)[1]
        if item in covered:
            continue
        if re.search(rf"\b{re.escape(name)}\b", example):
            continue
        bad.append(f"{item}: the example never reaches it and COVERED_ELSEWHERE does not name "
                   f"the key that does")
    known = set(items)
    for item in covered:
        if item not in known:
            bad.append(f"{item}: COVERED_ELSEWHERE names it and no module has it any more")
    return bad


# --------------------------------------------------------------------------- #
# Running                                                                        #
# --------------------------------------------------------------------------- #

def ask_rust(binary: Path, reqs: list[dict]) -> list[dict]:
    payload = "".join(json.dumps(r) + "\n" for r in reqs)
    run = subprocess.run([str(binary)], input=payload, capture_output=True, text=True,
                         check=False, env=dict(os.environ), cwd=str(BASE))
    if run.returncode != 0:
        sys.exit(f"{binary} exited {run.returncode}\n{run.stderr}")
    lines = [ln for ln in run.stdout.split("\n") if ln]
    if len(lines) != len(reqs):
        sys.exit(f"{binary} answered {len(lines)} of {len(reqs)} cases")
    return [normalise(json.loads(ln)) for ln in lines]


def attribute(want: dict, got: dict) -> list[str]:
    return [k for k in KEYS if want.get(k) != got.get(k)]


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--cases", type=int, default=50_000)
    ap.add_argument("--git-every", type=int, default=10,
                    help="ask the git layer on every Nth generated case (curated: always)")
    ap.add_argument("--seed", type=int, default=20260922)
    ap.add_argument("--binary", type=Path, default=DEFAULT_BINARY)
    ap.add_argument("--batch", type=int, default=2_000)
    ap.add_argument("--record", action="store_true")
    ap.add_argument("--check", action="store_true")
    ap.add_argument("--surface", action="store_true")
    ap.add_argument("--coverage", action="store_true")
    ap.add_argument("--dump-oracle", type=Path, default=None,
                    help="write the Python's answers for this run's cases and stop")
    ap.add_argument("--against", type=Path, default=None,
                    help="compare the Rust against a --dump-oracle file instead of the Python")
    args = ap.parse_args()
    # Relative to where the harness was STARTED: it stands in the fixture by now.
    for name in ("binary", "dump_oracle", "against"):
        p = getattr(args, name)
        if p is not None and not p.is_absolute():
            setattr(args, name, ORIGINAL_CWD / p)

    wrong = surface()
    if wrong:
        print("the evidence does not cover the surface:", file=sys.stderr)
        for line in wrong:
            print(f"  {line}", file=sys.stderr)
        return 1
    if args.surface:
        print(f"{len(_pub_items())} pub items, every one reached or accounted for")
        return 0

    if args.record or args.check:
        # The REQUEST is recorded beside the answer: its probes are derived by the Python, and
        # the Rust replay has no Python to derive them with.
        rows = []
        for c in CURATED:
            req = request(c)
            rows.append(json.dumps({"case": c, "request": normalise(req), "answer": oracle(req)},
                                   sort_keys=True, ensure_ascii=False))
        text = "".join(r + "\n" for r in rows)
        steps = json.dumps(STEPS, indent=1, ensure_ascii=False) + "\n"
        if args.record:
            CORPUS.parent.mkdir(parents=True, exist_ok=True)
            CORPUS.write_text(text, encoding="utf-8")
            FIXTURE.write_text(steps, encoding="utf-8")
            print(f"wrote {len(rows)} cases to {CORPUS} and {len(STEPS)} steps to {FIXTURE}")
            return 0
        if CORPUS.read_text(encoding="utf-8") != text:
            print(f"{CORPUS} is not what the oracle says; run --record", file=sys.stderr)
            return 1
        if FIXTURE.read_text(encoding="utf-8") != steps:
            print(f"{FIXTURE} is not the fixture this harness builds; run --record",
                  file=sys.stderr)
            return 1
        print(f"{CORPUS}: {len(rows)} cases, unchanged; {FIXTURE}: {len(STEPS)} steps, unchanged")
        return 0

    if args.coverage:
        cases = cases_for(args.cases, args.seed, args.git_every)
        generated = cases[len(CURATED):]
        tally = dict.fromkeys(COVERAGE_BRANCHES, 0)
        for case in generated:
            for name in reached(case):
                if name in tally:
                    tally[name] += 1
        print(f"{len(generated)} GENERATED cases (the {len(CURATED)} curated rows excluded)")
        for name in COVERAGE_BRANCHES:
            n = tally[name]
            print(f"  {name:22s} {n:8d}  {100.0 * n / max(1, len(generated)):5.1f}%")
        return 0 if all(tally.values()) else 1

    if not args.binary.exists():
        sys.exit(f"{args.binary} is not built — "
                 f"`cargo build -p charter-core --example planeroot_oracle`")

    cases = cases_for(args.cases, args.seed, args.git_every)
    cached = None
    if args.against is not None:
        rows = [json.loads(ln) for ln in args.against.read_text(encoding="utf-8").splitlines() if ln]
        if [r["case"] for r in rows] != cases:
            sys.exit(f"{args.against} is stale — dump it again")
        cached = [r["answer"] for r in rows]
    if args.dump_oracle is not None:
        with args.dump_oracle.open("w", encoding="utf-8") as fh:
            for c in cases:
                fh.write(json.dumps({"case": c, "answer": oracle(request(c))}) + "\n")
        print(f"wrote what the Python says about {len(cases)} cases to {args.dump_oracle}")
        return 0

    bad = 0
    by_key: dict[str, int] = {}
    git_cases = sum(1 for c in cases if c["git"])
    for lo in range(0, len(cases), args.batch):
        chunk = cases[lo:lo + args.batch]
        reqs = [request(c) for c in chunk]
        for k, (case, got) in enumerate(zip(chunk, ask_rust(args.binary, reqs), strict=True)):
            want = cached[lo + k] if cached is not None else oracle(reqs[k])
            if want == got:
                continue
            bad += 1
            for key in attribute(want, got):
                by_key[key] = by_key.get(key, 0) + 1
            if bad <= 20:
                print(f"--- diverged on {case!r}")
                for key in attribute(want, got):
                    print(f"    {key}: python={want.get(key)!r}")
                    print(f"    {key}:   rust={got.get(key)!r}")
    print(f"{len(cases)} cases ({git_cases} asking git), {bad} divergences")
    if bad:
        print(f"by answer: {by_key}")
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
