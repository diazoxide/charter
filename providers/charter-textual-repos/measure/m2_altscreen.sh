#!/usr/bin/env bash
# M2 — what the alternate screen costs charter: pane scrollback, `capture-pane`, resize,
# and what tmux is left holding when the pane's process goes away.
source "$(dirname "${BASH_SOURCE[0]}")/rig.sh"

seed
start_session

own=$(split_panel repos 14)
live=$(split_panel textual.live 14)
sleep 3

# A panel writes into the pane it is given, so both panes have had output. The question is
# whether tmux has any HISTORY for them: charter's own panel repaints with
# `\x1b[H\x1b[2J` on the normal screen, and Textual runs on the alternate screen, where
# tmux keeps no history at all.
echo "== alternate-screen flag (tmux's own view) =="
tm list-panes -t exp -F '#{pane_id} #{pane_current_command} alternate_on=#{alternate_on} height=#{pane_height}'

echo
echo "== history size per pane =="
tm list-panes -t exp -F '#{pane_id} history_size=#{history_size} history_bytes=#{history_bytes}'

echo
echo "== capture-pane -p (visible) line counts =="
echo "charter repos : $(tm capture-pane -p -t "$own"  | grep -c '[^[:space:]]') non-blank"
echo "textual live  : $(tm capture-pane -p -t "$live" | grep -c '[^[:space:]]') non-blank"

echo
echo "== capture-pane -p -e first two lines of the Textual pane =="
tm capture-pane -p -e -t "$live" | head -2 | cat -v

echo
echo "== capture-pane -S -200 (scrollback) line counts =="
echo "harness pane  : $(tm capture-pane -p -S -200 -t exp:0.0 | grep -c '[^[:space:]]') non-blank"
echo "charter repos : $(tm capture-pane -p -S -200 -t "$own"  | grep -c '[^[:space:]]') non-blank"
echo "textual live  : $(tm capture-pane -p -S -200 -t "$live" | grep -c '[^[:space:]]') non-blank"

echo
echo "== resize the window 150x40 -> 100x30, then back =="
tm resize-window -t exp -x 100 -y 30; sleep 1.5
echo "--- textual pane at $(tm display -p -t "$live" '#{pane_width}x#{pane_height}') ---"
tm capture-pane -p -t "$live" | head -4
echo "  longest captured line: $(tm capture-pane -p -t "$live" | awk '{print length}' | sort -n | tail -1) cells"
tm resize-window -t exp -x 150 -y 40; sleep 1.5
echo "--- textual pane back at $(tm display -p -t "$live" '#{pane_width}x#{pane_height}') ---"
tm capture-pane -p -t "$live" | head -4
echo "  longest captured line: $(tm capture-pane -p -t "$live" | awk '{print length}' | sort -n | tail -1) cells"

echo
echo "== kill the panel process and see what tmux is holding =="
tm set -t exp remain-on-exit on
pid=$(tm list-panes -t exp -F '#{pane_id} #{pane_pid}' | awk -v p="$live" '$1==p{print $2}')
kill -9 "$pid"; sleep 1
tm list-panes -t exp -F '#{pane_id} dead=#{pane_dead} status=#{pane_dead_status} alternate_on=#{alternate_on}'
echo "--- what the dead pane shows ---"
tm capture-pane -p -t "$live" | head -6

echo
echo "== respawn-pane, the way commands_frame.cmd_respawn does after pane-died =="
tm respawn-pane -k -t "$live" -- "$CHARTER" panel textual.live --session "$FID"
sleep 3
tm list-panes -t exp -F '  #{pane_id} dead=#{pane_dead} alternate_on=#{alternate_on} #{pane_width}x#{pane_height}'
echo "  --- respawned pane ---"
tm capture-pane -p -t "$live" | grep '[^[:space:]]' | head -3 | sed 's/^/  /'
