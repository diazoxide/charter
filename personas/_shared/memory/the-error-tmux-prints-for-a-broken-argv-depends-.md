# The error tmux prints for a broken argv depends on SERVER STATE, so neve

_2026-09-11 03:58 · persistent_

The error tmux prints for a broken argv depends on SERVER STATE, so never document one string as what 'every launch' shows. Measured 2026-09-11 on 3.7c and 3.2, a new-session whose -e CHARTER_ROOT value ends in ';': with a server already on the socket, rc 1 'unknown command: -e'; with NO server yet (first chat after install/reboot), rc 1 'error connecting to <socket> (No such file or directory)', because the parse fails before the client starts one. Say what holds in every state (tmux refused the launch) and name each string with the state it was measured in (#959, three review rounds).
