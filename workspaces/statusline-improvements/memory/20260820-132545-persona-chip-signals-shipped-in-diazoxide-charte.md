# Persona chip signals shipped in diazoxide/charter#309 (branch statusline

_2026-08-20 13:25 · persistent_

Persona chip signals shipped in diazoxide/charter#309 (branch statusline-persona-chip-signals, commit 545b5ff): vault mark renders only when the vault is UNUSABLE (dim ◦ = declared but not registered here or file not created yet; yellow ! = registered+unhealthy; nothing = no vault or healthy), and each chip carries ⚡ + count-when->1 + age-of-oldest for in-flight dispatches. inflight.live_records() now returns (agent, started_at) and live() is a projection of it. Session strip dropped the name list for a bare '⚡ N'. Full suite 2846 OK.
