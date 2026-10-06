### Security

- **An administrator's policy locks sandbox values, and the strictest wins.** purlis reads this
  machine's `/etc/purlis/policy.json`, only where root owns it and its folder and no one else can
  write either (never a link), and no chat may write its folder. It can fix the Internet access
  presets and the hosts any level may add (a project's forges included), and forbid your own
  hosts, a persona's own hosts, a person's opt-out in a project whose sandbox is on, and folders
  granted to a chat. Each locked value shows "Locked by policy" and who set it in Settings ›
  Sandbox, with no control; the new-chat picker, a block's Notice and a held persona's Notice
  offer nothing policy forbids and still say why and who to ask. A policy file, or its folder,
  that purlis cannot trust or read locks everything it could lock but the presets (#1343).
