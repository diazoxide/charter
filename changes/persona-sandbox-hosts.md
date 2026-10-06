### Added

- **A persona carries the hosts its chats reach.** `[sandbox.personas.<persona>] hosts` in the
  project's `charter.toml` lists hosts, private addresses and ports included
  (`10.100.39.145:6443` for a cluster over a VPN), that a chat you start as that persona reaches,
  as does a chat on no persona when it is the project's default persona. A chat on another persona
  does not, and the grants are fixed when the chat starts. A chat that another chat opened by a
  handoff runs with the asking chat's grants, and a chat resumed from a session record as a
  persona wider than the default runs with the default's, until you press **Allow** on its tab;
  **Restart now** then starts it again, resuming its conversation, once its turn ends. A handoff
  or a Resume never widens what a chat reaches. Each host is
  checked as the project's own hosts are, teammates are told once when a persona's hosts change,
  and a chat can't write them (#1362).
