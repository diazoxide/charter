# Running charter init to build a THROWAWAY plane (a demo, a capture, a pr

_2026-09-11 02:21 · persistent_

Running charter init to build a THROWAWAY plane (a demo, a capture, a probe) touches the operator's real Claude Code. With claude on PATH, init runs claude plugin marketplace add and claude plugin install charter@charter at project scope for that directory, using the caller's HOME. So any script that builds a scratch plane must strip claude from PATH or give it an isolated HOME. docs/assets/demo-plane.sh did not, and the review of PR 958 caught it on 2026-09-11. Two related capture gotchas: docs/assets/capture-frame.sh used to run on the SHARED charter tmux socket; and a tmux socket path under the Claude Code session scratchpad exceeds tmux's 104-byte limit, so use a short directory under /tmp.
