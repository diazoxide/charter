# diazoxide/charter has GitHub private vulnerability reporting DISABLED (g

_2026-08-22 10:05 · persistent_

diazoxide/charter has GitHub private vulnerability reporting DISABLED (gh api repos/diazoxide/charter/private-vulnerability-reporting -> {"enabled": false}). SECURITY.md and .github/ISSUE_TEMPLATE/config.yml now route reporters to the maintainer email in pyproject.toml instead of the advisory form; unauthenticated, /security/advisories/new 302s to a GitHub sign-in, so the dead end is only visible after login.
