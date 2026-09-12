# Probing a harness through a user-declared command IS running that comman

_2026-09-11 13:07 · persistent_

Probing a harness through a user-declared command IS running that command. Found planning harness profiles (2026-09-11): detecting a profile's wiring with [*command, plugin, list, --json] would have let charter doctor, which the SessionStart hook runs, execute a command nobody approved. Rule: gate every probe on the same approval record a launch needs; an unapproved profile gets a not-approved-yet row and no probe.
