### Added

- **A persona carries the hosts its chats reach.** `[sandbox.personas.<persona>] hosts` in the
  project's `charter.toml` lists hosts, private addresses and ports included
  (`10.100.39.145:6443` for a cluster over a VPN), that a chat you start as that persona can
  reach, as can a chat on no persona when it is the project's default persona. Because anyone
  who can change the project's settings can change that list, a persona's hosts reach nothing on
  your machine until you allow them there: the project view shows each persona's hosts with
  **Allow** and **Not now**, and Settings › Sandbox has the same Allow. Your Allow is for exactly
  the list you were shown, and for whether the persona is the default: if either changes, its
  chats reach none of the list until you allow it again, and Settings › Sandbox › Granted lists
  the old Allow as waiting, with Revoke. Allowed hosts reach chats started after the Allow; a
  running chat takes them when it restarts. A chat on another persona doesn't reach them. A chat
  that another chat opened by a handoff runs with the asking chat's grants, and a chat resumed
  from a session record as a persona wider than the default runs with the default's, until you
  press **Allow** on its tab; **Restart now** then starts it again, resuming its conversation,
  once its turn ends. A handoff or a Resume never widens what a chat reaches. Each host is
  checked as the project's own hosts are (a private address only as one exact address and
  port), a chat can't write the list, and policy can forbid persona hosts (#1362).
