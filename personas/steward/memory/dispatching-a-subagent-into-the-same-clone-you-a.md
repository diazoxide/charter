# Dispatching a subagent into the SAME clone you are committing from costs

_2026-08-19 13:12 · persistent_

Dispatching a subagent into the SAME clone you are committing from costs you its work silently: on 2026-08-19 my 'git add -A && git commit' swept a release-persona agent's in-progress Task 4 diff into a commit whose message described only my own file. The agent noticed and split the history correctly, but nothing warned either of us at the time. Two fixes, both cheap: stage explicit paths (git add <files>) whenever an agent is live in the same tree, or give the agent its own worktree. Also: a spec/plan written in the PLANE ROOT is invisible to a workspace clone — 'charter clone' pulls from the remote, so uncommitted plane-root files do not exist there. Copy docs into the clone (or commit them first) BEFORE telling an agent to read them by path; I dispatched two agents pointed at paths that did not exist on their side.
