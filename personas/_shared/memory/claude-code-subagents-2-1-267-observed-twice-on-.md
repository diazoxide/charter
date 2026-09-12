# Claude Code subagents (2.1.267, observed twice on 2026-09-11): a subagen

_2026-09-11 05:54 · persistent_

Claude Code subagents (2.1.267, observed twice on 2026-09-11): a subagent that ends its turn while one of its OWN run_in_background commands is still running is re-invoked automatically when that command finishes, and carries on (PR 948's implementer after its 1.5 h sweep; PR 965's after its suite, sweep and CI, which then opened the PR unprompted). The controller gets a 'completed' task-notification at the pause, and that is NOT the report: its result text says it is waiting on background jobs. Do not nudge it with SendMessage; it resumes by itself. A SendMessage that arrives while it is running is 'queued for delivery at its next tool round', which is also how to tell it is already running. Nudge only if the background job has ended and the agent stays quiet.
