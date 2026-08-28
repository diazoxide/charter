#!/usr/bin/env bash
# M7 — the picture. A charter frame with charter's own `repos` table and both Textual
# components in it at once, captured with `capture-pane -p -e` so the escapes are real.
#
# Panes are split in charter's own order (`layout.panel_argvs`: `top` is the only `-b`,
# every other split is a plain `-v`, so a slot split later sits ABOVE one split earlier),
# then printed top to bottom with each pane's real geometry.
#
#   ./m7_shot.sh          plain text, for reading and for a PR body
#   ./m7_shot.sh esc      `capture-pane -p -e`, with ESC shown as `\e`
source "$(dirname "${BASH_SOURCE[0]}")/rig.sh"

seed
COLS=150 ROWS=44 start_session

split_panel identity 1  >/dev/null
split_panel attention 1 >/dev/null
split_panel repos 9     >/dev/null
tm split-window -t exp:0.0 -v -l 12 -- "$CHARTER" panel textual.repos --session "$FID" >/dev/null
split_panel textual.live 12 >/dev/null
sleep 4
tm select-pane -t "%0"
sleep 0.5

order=$(tm list-panes -t exp -F '#{pane_top} #{pane_id}' | sort -n | awk '{print $2}')
for p in $order; do
  read -r top bot w h alt cmd <<<"$(tm display -p -t "$p" \
     '#{pane_top} #{pane_bottom} #{pane_width} #{pane_height} #{alternate_on} #{pane_start_command}')"
  what=$(printf '%s' "$cmd" | sed -E 's#.*/charter (panel [a-z.]+).*#\1#; s#^sh -c.*#the harness (stand-in)#')
  printf '┌─ %s  rows %s-%s  %sx%s  alternate_on=%s  %s\n' "$p" "$top" "$bot" "$w" "$h" "$alt" "$what"
  if [ "${1:-plain}" = esc ]; then
    # ESC shown as `\e`, and nothing else touched — `cat -v` would mangle the UTF-8
    # glyphs the table is made of, which is half of what there is to look at.
    tm capture-pane -p -e -t "$p" | sed $'s/\033/\\\\e/g' | sed 's/^/│ /'
  else
    tm capture-pane -p -t "$p" | sed 's/^/│ /'
  fi
done
printf '└─ tmux %s · window %s · session mouse %s · %s\n' \
  "$(tmux -V | cut -d" " -f2)" "$(tm display -p -t exp "#{window_width}x#{window_height}")" \
  "$(tm show -t exp -v mouse)" "$("$PY" -c "import sys;print(\"python \"+sys.version.split()[0])")"
