# 0.46.0's ask->allow under bypassPermissions has a sharp edge found by as

_2026-08-19 18:36 · persistent_

0.46.0's ask->allow under bypassPermissions has a sharp edge found by asking 'what would an autonomous agent do with this same task': _clone_commit_reason matches _GIT_WRITE_RE which includes tag AND push, so 'git tag v0.46.0' from a workspace clone used to return ASK — and a hook ask floors at a prompt in EVERY permission mode, so it actually stopped unattended runs. After 0.46.0 it returns ALLOW, and pushing that tag fires release.yml -> irreversible PyPI publish. gh pr merge and gh release create were never guarded at all (not git verbs). Verified by probe on 0.46.0. Filed as #299. The general lesson: when converting an ask to an allow, enumerate what the ask was INCIDENTALLY covering — the clone-commit nudge was never designed to guard releases, it just did.
