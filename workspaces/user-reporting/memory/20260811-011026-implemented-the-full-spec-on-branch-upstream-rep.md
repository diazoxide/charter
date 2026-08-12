# Implemented the full spec on branch upstream-reporting (2 commits: a75b6

_2026-08-11 01:10 · persistent_

Implemented the full spec on branch upstream-reporting (2 commits: a75b6a3 drafting+detection, d8dc560 send). 90 tests, full suite 1338 green. Everything from spec.md is built EXCEPT creating the via-charter-report and gap labels on diazoxide/charter, which is a repo change needing Aaron's call. Two real findings while building: (1) gh stores its own auth under XDG_CONFIG_HOME, so isolating consent there logs gh out and silently reroutes send to the no-gh fallback — hence CHARTER_CONFIG_HOME; (2) the fingerprint ignores subcommand as well as message, so clone and doctor dying at the same charter line is one bug. Nothing merged to main.
