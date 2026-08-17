#!/usr/bin/env bash
# Capture the quickstart by actually running it, then sanitize the capture.
#
# The commands are real and so is every byte of their output: this runs against a real
# forge and clones a real repo. What it does NOT do is publish the operator's machine.
# Four substitutions run over the capture, all of them mechanical and listed below, so
# the result is reproducible rather than hand-edited:
#
#   1. the scratch directory  ->  ~/my-control-plane
#   2. the git identity line  ->  a placeholder name and address
#   3. the forge account name ->  the owner you passed
#   4. `discover`'s repo list ->  first few names, then a count
#
# Everything else is verbatim. `charter doctor` is deliberately not in the script: its
# output is mostly a report about one machine, which is the least interesting thing a
# newcomer could be shown and the most leaky thing to publish.
#
#   ./capture-demo.sh <scratch-dir> <owner> <repo> > demo.ansi
set -uo pipefail

DIR="${1:?usage: capture-demo.sh <scratch-dir> <owner> <repo>}"
OWNER="${2:?owner}"
REPO="${3:?repo}"
PROMPT=$'\033[32m❯\033[0m'
COLS="${COLUMNS:-88}"

# Resolved BEFORE the `cd` below. `BASH_SOURCE[0]` is usually relative — invoking this
# as `./docs/assets/capture-demo.sh` and resolving it after moving into the scratch
# directory produced an empty HERE and a `python3 /ptyrun.py` that could not open its
# own helper. Every command then captured a traceback instead of output, the capture
# "succeeded" with exit 0, and the assets quietly stopped being regenerable.
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# The capture must show THIS tree's charter, not whichever version happens to be on the
# developer's PATH. Without it the asset silently documents an older build: regenerating
# against an installed CLI faithfully reproduces output the tree has already changed.
SRC_ROOT="$(cd "$HERE/../.." && pwd)"
SHIM="$(mktemp -d -t charter-shim)"
cat > "$SHIM/charter" <<SH
#!/bin/sh
exec python3 -c 'import sys; sys.path.insert(0, "$SRC_ROOT"); sys.path = [p for p in sys.path if p not in ("", "$DIR")]; from charter.cli import main; sys.exit(main())' "\$@"
SH
chmod +x "$SHIM/charter"
PATH="$SHIM:$PATH"

rm -rf "$DIR"; mkdir -p "$DIR"; cd "$DIR" || exit 1
REAL="$(pwd -P)"
unset $(env | grep -o '^CHARTER_[A-Z_]*' || true) 2>/dev/null || true

# The sanitizer goes in a file rather than a heredoc on python's stdin: a heredoc IS
# stdin, so `... | python3 - <<PY` reads the script where the capture should be and
# emits nothing at all.
SANITIZER="$(mktemp -t charter-sanitize)"
trap 'rm -f "$SANITIZER"' EXIT
cat > "$SANITIZER" <<'PY'
import re, sys
real, owner = sys.argv[1], sys.argv[2]
text = sys.stdin.read()

# `script` hands back the tty's EOF echo; it is not part of any command's output.
text = text.replace("\x04", "").replace("\r", "")

text = text.replace(real, "~/my-control-plane")
text = re.sub(r"(git identity\s+\S*\s*)\S.*", r"\1Ada Lovelace <ada@example.com>", text)
text = re.sub(r"account \S+ \(keyring\)", f"account {owner} (keyring)", text)

# discover lists every repo it found. On a real account that is both enormous and
# nobody's business; keep enough to show the shape.
def trim(m):
    names = [n.strip() for n in m.group(2).split(",")]
    keep = names[:5]
    rest = len(names) - len(keep)
    return m.group(1) + ", ".join(keep) + (f", … and {rest} more" if rest > 0 else "")
text = re.sub(r"(New in group: )(.*)", trim, text)

sys.stdout.write(text)
PY

sanitize() { python3 "$SANITIZER" "$REAL" "$OWNER"; }

RAW="$(mktemp -t charter-capture)"
trap 'rm -f "$SANITIZER" "$RAW"' EXIT

run() {
  printf '%s %s\n' "$PROMPT" "$*" >> "$RAW"
  # ptyrun, not `script`: charter decides colour from sys.stderr.isatty() at import time
  # with no env override, so the child needs a real terminal — and `script` refuses to
  # provide one unless the parent already has one, which an agent shell or CI does not.
  COLUMNS="$COLS" python3 "$HERE/ptyrun.py" "$@" >> "$RAW" 2>&1
  printf '\n' >> "$RAW"
}

printf '%s %s\n\n' "$PROMPT" "mkdir my-control-plane && cd my-control-plane" >> "$RAW"
run charter init --forge github --owner "$OWNER"
run charter discover
run charter clone "$REPO"
run charter status

sanitize < "$RAW"
