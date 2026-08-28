#!/usr/bin/env bash
# M3 — what a crash costs. Four ways for a Textual component to fail, each in a real pane
# beside a working charter panel and a working harness pane, so the blast radius is
# something to look at rather than something to argue about.
source "$(dirname "${BASH_SOURCE[0]}")/rig.sh"

seed
start_session
own=$(split_panel repos 8)

show() {   # show <label>
  sleep 3
  echo "== $1 =="
  tm list-panes -t exp -F '  #{pane_id} dead=#{pane_dead} cmd=#{pane_current_command}'
  echo "  --- the failing pane ---"
  tm capture-pane -p -t "$victim" | grep '[^[:space:]]' | head -4 | sed 's/^/  /'
  echo "  --- charter's own repos pane, beside it ---"
  tm capture-pane -p -t "$own" | grep '[^[:space:]]' | head -2 | sed 's/^/  /'
  echo "  --- the harness pane ---"
  tm capture-pane -p -t exp:0.0 | grep '[^[:space:]]' | tail -1 | sed 's/^/  /'
  tm kill-pane -t "$victim" 2>/dev/null || true
  echo
}

# 1. render raises before the app starts.
victim=$(tm split-window -t exp:0.0 -v -l 8 -P -F '#{pane_id}' \
   -e CHARTER_TEXTUAL_FAULT=render -- "$CHARTER" panel textual.live --session "$FID")
show "fault: render raises before Textual starts"

# 2. the app raises from inside Textual's own message pump, one second after mount.
victim=$(tm split-window -t exp:0.0 -v -l 8 -P -F '#{pane_id}' \
   -e CHARTER_TEXTUAL_FAULT=loop -- "$CHARTER" panel textual.live --session "$FID")
show "fault: raises inside Textual's message pump"

# 3. the distribution is not installed at all — the id resolves to nothing.
victim=$(tm split-window -t exp:0.0 -v -l 8 -P -F '#{pane_id}' \
   -- "$CHARTER" panel notinstalled.thing --session "$FID")
show "not installed: charter panel notinstalled.thing"

# 4. the provider is installed but its module cannot be imported. Simulated by pointing
#    PYTHONPATH at a shadowing module of the same name that raises on import.
shadow="$(mktemp -d)"
cat > "$shadow/charter_textual_repos.py" <<'PYEOF'
raise ImportError("the provider's own module could not be imported")
PYEOF
victim=$(tm split-window -t exp:0.0 -v -l 8 -P -F '#{pane_id}' \
   -e "PYTHONPATH=$shadow" -- "$CHARTER" panel textual.live --session "$FID")
show "import raises: a shadowed provider module"
rm -rf "$shadow"
