# Local clone at workspaces/chat-handoff/charter/charter can be several co

_2026-09-10 17:28 · persistent_

Local clone at workspaces/chat-handoff/charter/charter can be several commits behind origin/main (git status shows 'behind, can fast-forward'). Before verifying a 'still true on main' claim, git fetch origin main and read via 'git show origin/main:<path>' rather than trusting the checked-out tree.
