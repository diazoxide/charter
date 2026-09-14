---
version: unreleased
headline: Two projects open at once no longer share a tmux server, so neither one's chats, panels or profiles show up in the other's frame
---

Every charter frame on a machine used to run on one tmux server, `tmux -L charter`, and a
workspace's session was joined by its name. Open charter in a second project whose workspace
had the same name — `default`, which every project has — and that project's chat became a
window in the first project's session, under the first project's panels, tab strips, key
bindings and F2 palette, reading the first project's state and profiles.

Each project (each plane) now has a tmux server of its own, `tmux -L charter-plane-<12 hex>`,
named from a hash of its `.charter/` directory. Two projects with a `default` workspace open
are two sessions on two servers. You do not need the name: a detach prints the reattach
command with it, and `charter` in the project attaches. The cost is one tmux server process
per project that has a frame open.

**A frame started before you upgrade keeps running until it ends.** It stays on the old
`charter` server, and close, quit, a panel restarting, a resize and switching between its chats
find it there, because every chat records which server it is on. What it cannot do is add a
chat: `+`, and a workspace tab for a workspace it has no session for, refuse and say why,
because the new chat would open on the project's new server where that frame cannot show it.
`F2 → charter: quit`, then `charter`, puts the project back on its own server.

Inside a tmux you run yourself, charter still opens each chat as a window in your session, and
now marks each window with the project it belongs to, so a close or quit in one project cannot
stop another project's chat of the same name there.

Nothing to adopt.
