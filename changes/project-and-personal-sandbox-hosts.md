### Added

- **Add your project's own hosts, and your own, to a sandboxed chat's Internet access.**
  Settings › Sandbox lists the project's hosts with Add and Remove. They are kept in
  `charter.toml`, and every chat of everyone who opens the project reaches them with no
  approval. Settings › Sandbox › Your hosts holds hosts for this machine only, in
  `charter.local.toml`; one Settings did not add here waits for your Confirm. A host is a
  domain, `*.domain`, an IP address or any of them with `:port`, private addresses included
  (`10.100.39.145:6443` for a cluster over a VPN). Each is checked as you add it, and refused
  with the reason: this machine, link-local and cloud metadata addresses are never hosts, and a
  name is never reached at one of them, whatever it resolves to. Every harness reaches them:
  Claude Code through its own sandbox, and opencode and Codex through purlis's egress proxy.
  When a project's hosts change, each teammate sees one Notice naming what was added and taken
  away. A chat can never change either list (#1341). The first time a project with hosts is
  opened after this upgrade, each teammate sees one Notice listing them, the `[[forge]]` hosts
  the forge preset reaches included.
