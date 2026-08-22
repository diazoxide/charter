# registry.resolve_host fails closed on host-confusion URLs (user@github.c

_2026-08-22 10:05 · persistent_

registry.resolve_host fails closed on host-confusion URLs (user@github.com@evil.example, github.com.evil.example, github.com:token@evil.example, managed name in the path): all resolve to None = unmanaged, never to a managed forge. That matters because gitpolicy.forge_for turns the answer into a credential helper + insteadOf in the clone and hooks._known_forges into the SSH guard's host set. Pinned in tests/test_guards_that_must_not_be_refactored_away.py; the accidental-substring half stays in tests/test_forge_registry.py.
