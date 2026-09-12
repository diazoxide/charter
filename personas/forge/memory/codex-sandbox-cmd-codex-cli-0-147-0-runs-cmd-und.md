# codex sandbox -- CMD (codex-cli 0.147.0) runs CMD under seatbelt with ~/

_2026-09-10 22:57 · persistent_

codex sandbox -- CMD (codex-cli 0.147.0) runs CMD under seatbelt with ~/.codex/config.toml shell_environment_policy applied and no model call, so it measures what env reaches a Codex shell: charter CHARTER_HARNESS=codex arrives, an inherited CHARTER_SESSION_ID and TERM_SESSION_ID survive. codex sandbox macos is not a subcommand in 0.147.0 (it tries to exec macos).
