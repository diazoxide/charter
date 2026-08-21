# Forge CLI calls are bounded by two constants in charter/forge/base.py: S

_2026-08-21 23:18 · persistent_

Forge CLI calls are bounded by two constants in charter/forge/base.py: STATUS_TIMEOUT=10s for the best-effort path (_api, ci_status, check_auth) and LIST_TIMEOUT=60s for the strict/paging path. The split is per-COST-of-being-wrong, not per-caller: every site is one CLI invocation making one API request, so a per-call bound is the same question everywhere; what differs is that a status failure costs a blank column while a strict failure aborts a whole discover (#324).
