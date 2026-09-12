# Codex hook trust, measured 2026-09-12 (codex-cli 0.147.0): [hooks.state.

_2026-09-12 19:35 · persistent_

Codex hook trust, measured 2026-09-12 (codex-cli 0.147.0): [hooks.state."charter@charter:hooks/hooks.json:<event_snake>:<group>:<hook>"] carries trusted_hash = sha256:<hex>, and Codex writes those entries LAZILY, one per hook as each first fires — a machine running charter under Codex for weeks held 12 of the plugin's 18 keys. So 'a trust entry for every hook key' reads a wired machine as unwired. The hash cannot be recomputed from the plugin's hooks/hooks.json: the command string, the hook object as JSON (sorted/compact/indented), the matcher group and the joined commands were all tried and none matched. Charter's rule is therefore 'at least one trusted charter hook in that home'. Also: `codex plugin` has add/list/marketplace/remove — no install, no update.
