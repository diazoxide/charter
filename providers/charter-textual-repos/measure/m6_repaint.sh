#!/usr/bin/env bash
# M6 — does charter's version-bump repaint model coexist with Textual's own loop?
#
# charter's panel is a poll: `panel._tick` reads `state.version(fid)` five times a second
# and repaints when it moves. Three panes are watched here through two bumps that change
# the plane's CI state from `failed` to `passed`:
#
#   repos          charter's own renderer, the control
#   textual.repos  the adapter — render returns, so charter calls it again
#   textual.live   the takeover — render never returns, so charter never calls it again
#   textual.live   the same, with CHARTER_TEXTUAL_LIVE_REFRESH=1, which polls the version
#                  itself from inside the app
source "$(dirname "${BASH_SOURCE[0]}")/rig.sh"

seed failed
start_session

own=$(split_panel repos 8)
adapt=$(tm split-window -t exp:0.0 -v -l 10 -P -F '#{pane_id}' \
   -- "$CHARTER" panel textual.repos --session "$FID")
live=$(split_panel textual.live 10)
livep=$(tm split-window -t exp:0.0 -v -l 10 -P -F '#{pane_id}' \
   -e CHARTER_TEXTUAL_LIVE_REFRESH=1 -- "$CHARTER" panel textual.live --session "$FID")
sleep 3.5

row() {  # the charter row for the current repo, from whichever pane
  tm capture-pane -p -t "$1" | grep -m1 'charter' | tr -s ' ' | cut -c1-72
}

echo "== seeded with ci=failed =="
printf '  %-14s %s\n' "repos"         "$(row "$own")"
printf '  %-14s %s\n' "textual.repos" "$(row "$adapt")"
printf '  %-14s %s\n' "textual.live"  "$(row "$live")"
printf '  %-14s %s\n' "live+refresh"  "$(row "$livep")"

seed passed          # rewrites the gather cache and bumps the frame's version
sleep 2

echo
echo "== after gather rewritten to ci=passed and state.bump() =="
printf '  %-14s %s\n' "repos"         "$(row "$own")"
printf '  %-14s %s\n' "textual.repos" "$(row "$adapt")"
printf '  %-14s %s\n' "textual.live"  "$(row "$live")"
printf '  %-14s %s\n' "live+refresh"  "$(row "$livep")"

echo
echo "== the adapter pane in full, after the bump =="
tm capture-pane -p -t "$adapt" | sed 's/^/  /'

echo
echo "== and the snapshot clock each pane is showing =="
for p in "$adapt" "$live" "$livep"; do
  printf '  %s %s\n' "$p" "$(tm capture-pane -p -t "$p" | grep -m1 snapshot | tr -s ' ')"
done
