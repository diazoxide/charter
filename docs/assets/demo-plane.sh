#!/usr/bin/env bash
# Build a throwaway control plane whose status line is worth screenshotting.
#
# Everything here is real: real `charter` commands, real git repos, real branches
# and real dirty/ahead state. Only the *org* is invented, so the render can show a
# plausible multi-repo day without exposing anyone's actual work.
set -euo pipefail

PLANE="${1:?usage: demo-plane.sh <dir>}"
rm -rf "$PLANE"
mkdir -p "$PLANE"
cd "$PLANE"

# The caller's session may hold a workspace lock / active persona. Scrub it so the
# demo plane resolves purely from its own state directory.
unset $(env | grep -o '^CHARTER_[A-Z_]*' || true) 2>/dev/null || true

charter init --forge github --owner acme >/dev/null

# The render resolves its workspace from the *session*, and a captured render has no
# session lock — so the plane names its own default instead.
cat >> charter.toml <<'EOF'

[workspace]
default = "billing-migration"
EOF

# ── inventory ────────────────────────────────────────────────────────────────
# `charter discover` would write this from the forge; the demo has no forge, so the
# same file is written directly. Shape matches inventory/repos.json exactly.
python3 - <<'PY'
import json, pathlib
repos = [
    ("billing-api",      "api",      "python"),
    ("billing-core",     "core",     "java-maven"),
    ("checkout-ui",      "frontend", "node"),
    ("payments-service", "service",  "go"),
    ("platform-infra",   "app",      "terraform"),
    ("acme-docs",        "docs",     "unknown"),
]
doc = {"group": "acme", "count": len(repos), "repos": [
    {"name": n, "path_with_namespace": f"acme/{n}", "default_branch": "main",
     "forge": "github", "web_url": f"https://github.com/acme/{n}",
     "ssh_url": "", "description": "", "topics": [], "kind": k, "stack": s}
    for n, k, s in repos
]}
p = pathlib.Path("inventory/repos.json"); p.parent.mkdir(exist_ok=True)
p.write_text(json.dumps(doc, indent=2))
PY

# ── the workspace under the render ───────────────────────────────────────────
charter workspace create billing-migration --use \
  --vision "Move billing off the legacy ledger without a customer-visible outage." >/dev/null
charter ws todo "drain the legacy ledger queue before cutover" >/dev/null
charter ws todo "get payments-service onto the new idempotency keys" >/dev/null

# Real clones would come from `charter clone`; with no forge to clone from, these are
# real git repos placed where a clone would land, so every marker the status line
# draws (branch, dirty, ahead) is git's own answer rather than a fixture.
mk() { # <repo> <branch> <dirty?> <ahead>
  local d="workspaces/billing-migration/$1"
  mkdir -p "$d" && git -C "$d" init -q -b main
  git -C "$d" config user.email demo@acme.test && git -C "$d" config user.name Demo
  echo "# $1" > "$d/README.md"
  git -C "$d" add -A && git -C "$d" commit -qm "initial commit"
  # A bare "remote" so ahead/behind is genuinely computed against an upstream.
  git init -q --bare "$d/../.remotes/$1.git"
  git -C "$d" remote add origin "../.remotes/$1.git"
  git -C "$d" push -q -u origin main
  [ "$2" != main ] && git -C "$d" checkout -qb "$2" && git -C "$d" push -q -u origin "$2"
  # `seq 1 0` counts DOWN on BSD/macOS (prints "1 0") where GNU seq prints nothing —
  # unguarded, every "0 commits ahead" repo silently got two.
  if [ "$4" -gt 0 ]; then
    for i in $(seq 1 "$4"); do
      echo "change $i" >> "$d/README.md"
      git -C "$d" commit -qam "wire up step $i"
    done
  fi
  [ "$3" = dirty ] && echo "work in progress" >> "$d/README.md"
  return 0
}
mk billing-api      migrate/ledger-cutover dirty 2
mk billing-core     migrate/ledger-cutover clean 0
mk payments-service fix/idempotency-keys   dirty 1
mk checkout-ui      main                   clean 0

# ── personas ─────────────────────────────────────────────────────────────────
charter persona create devops   --role "DevOps Engineer" \
  --delegate-when "CI/CD pipelines, k8s deploys, cluster access" --with-vault >/dev/null
charter persona create qa       --role "QA Engineer" \
  --delegate-when "e2e suites, flaky tests, release verification" --with-vault >/dev/null
charter persona create reviewer --role "Code Reviewer" \
  --delegate-when "PR review, architecture critique" >/dev/null

# Drafts get no sub-agent and render a ⚑; these are meant to look like working roles.
for p in devops qa reviewer; do
  f="personas/$p/persona.md"
  [ -f "$f" ] && python3 - "$f" <<'PY'
import sys, pathlib
p = pathlib.Path(sys.argv[1])
p.write_text("\n".join(l for l in p.read_text().splitlines() if l.strip() != "draft: true") + "\n")
PY
done

charter persona remember devops "prod kubeconfig lives in the devops vault, key KUBECONFIG" >/dev/null
charter persona remember devops "billing deploys gate on the e2e suite, not the unit suite" >/dev/null
charter persona remember qa "the flaky checkout test is a DNS timeout in CI, not the code" >/dev/null
# `persona use` writes a per-SESSION pointer, so it names the active persona only for the
# shell that ran this script. Every other session — anyone regenerating these assets — falls
# through to charter.toml, which `charter init` now seeds with the scaffolded `steward`. The
# capture then showed `steward` active for its author and `devops` for nobody else, silently
# depending on the operator's session id. Declaring the default is the same fix
# `[workspace] default` above already applies, one noun over.
charter persona default devops >/dev/null

# Vaults with something in them. Without this every row reads "not created yet", and the
# capture shows charter's empty state beside prose about credentials it is holding. The
# values are invented; what the render is claiming is the *count* and the provider.
printf 'demo-not-a-real-value\n' | charter secret set devops KUBECONFIG --stdin >/dev/null
printf 'demo-not-a-real-value\n' | charter secret set devops REGISTRY_TOKEN --stdin >/dev/null
printf 'demo-not-a-real-value\n' | charter secret set qa E2E_PASSWORD --stdin >/dev/null

# ── pieces (worktrees) ───────────────────────────────────────────────────────
# The fleet story is invisible without these: nested worktree rows, a declared outcome,
# and pieces that have said nothing for a while. Created through the real commands, AFTER
# the personas exist, so each claim carries the persona that made it rather than `null`.
charter worktree add billing-api  ledger-backfill  -w billing-migration >/dev/null
charter worktree add billing-api  ledger-verify    -w billing-migration >/dev/null
charter worktree add billing-core idempotency-keys -w billing-migration >/dev/null

# One piece declares an outcome; the rest stay silent, which is the state ADR 0011 exists
# to report — a worker that dies declares nothing, and that ABSENCE is what gets shown.
( cd "workspaces/billing-migration/.worktrees/billing-api/ledger-verify" \
    && charter worktree done >/dev/null )

# Who was last seen in which tree, and how long ago. Written directly for the same reason
# `inventory/repos.json` is: these are the files the every-turn hook would have written, in
# exactly their shape, and the demo has no session to write them. Back-dated so the render
# shows a real spread rather than three identical `now`s.
python3 - "$PWD" <<'PYP'
import json, pathlib, sys
from datetime import datetime, timedelta, timezone
root = pathlib.Path(sys.argv[1]) / "workspaces" / "billing-migration" / "pieces" / "seen"
now = datetime.now(timezone.utc)
def beat(repo, piece, persona, minutes):
    d = root / repo if piece else root
    d.mkdir(parents=True, exist_ok=True)
    at = (now - timedelta(minutes=minutes)).isoformat(timespec="seconds")
    (d / (f"{piece}.json" if piece else f"{repo}.json")).write_text(
        json.dumps({"ts": at, "session": "demo", "persona": persona,
                    "by": {persona: at}}, sort_keys=True) + "\n")
beat("billing-api",      "ledger-backfill",  "devops",   0)
beat("billing-core",     "idempotency-keys", "qa",       6)
beat("checkout-ui",      None,               "reviewer", 3)
beat("payments-service", None,               "devops",   22)
PYP

# ── dispatches in flight ─────────────────────────────────────────────────────
# charter writes one of these when a persona sub-agent STARTS and clears it when the agent
# returns (charter/inflight.py), so the record only exists while work is genuinely out.
# Written directly here for the same reason as the two blocks above: the demo has no live
# session to dispatch anything, and these are the files a dispatch would have left, in
# exactly their shape. Without them the persona column cannot draw the running badge at
# all, and the capture shows a plane where nobody is working beside prose about the badge.
# TWO records for one persona because the count renders only above one — a lone `⚡1` is
# deliberately drawn as `⚡`, so a single record would not show the number at all.
python3 - <<'PY'
import json, pathlib, subprocess, tempfile, time
root = pathlib.Path.cwd()
state = json.loads(subprocess.run(
    ["python3", "-c",
     "import charter.config as c; import json; print(json.dumps(str(c.STATE_DIR)))"],
    capture_output=True, text=True, cwd=root).stdout or '""')
state = pathlib.Path(state) if state else root / ".charter"
d = state / "dispatch-inflight"
d.mkdir(parents=True, exist_ok=True)
# Back-dated four minutes: a plausible mid-run age, and far inside the 30-minute
# presumed-dead threshold, so the badge renders `⚡2 4m` and not the `?` of a stuck run.
started = time.time() - 4 * 60
for _ in range(2):
    # Same naming as inflight.start: the agent name in the prefix, a unique suffix, so
    # two concurrent dispatches of one persona are two records rather than one overwrite.
    fd, path = tempfile.mkstemp(prefix="devops.", suffix=".json", dir=d)
    with open(fd, "w") as fh:
        json.dump({"agent": "devops", "ts": started}, fh)
print(f"dispatch-inflight → {d}")
PY

# ── forge state ──────────────────────────────────────────────────────────────
# `charter gl-refresh` writes this from gh/glab. The demo has no forge, so the same
# cache is written directly — same schema, same TTL fields the renderer reads.
python3 - <<'PY'
import json, os, pathlib, subprocess, time
root = pathlib.Path.cwd()
state = json.loads(subprocess.run(
    ["python3", "-c",
     "import charter.config as c; import json; print(json.dumps(str(c.STATE_DIR)))"],
    capture_output=True, text=True, cwd=root).stdout or '""')
state = pathlib.Path(state) if state else root / ".charter"
now = time.time()
ws = root / "workspaces" / "billing-migration"
entries = {
    "billing-api":      ("migrate/ledger-cutover", 412, "success"),
    "billing-core":     ("migrate/ledger-cutover", 413, "running"),
    "payments-service": ("fix/idempotency-keys",   398, "failed"),
    "checkout-ui":      ("main",                   None, "success"),
}
cache = {str(ws / n): {"branch": b, "change": c, "ci": ci, "sigil": "#", "ts": now}
         for n, (b, c, ci) in entries.items()}
f = state / "cache" / "glstate.json"
f.parent.mkdir(parents=True, exist_ok=True)
f.write_text(json.dumps(cache))
print(f"glstate → {f}")
PY

echo "demo plane ready: $PLANE"
