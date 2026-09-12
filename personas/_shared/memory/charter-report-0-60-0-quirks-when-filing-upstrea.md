# charter report (0.60.0) quirks when filing upstream: (1) the issue title

_2026-09-11 11:50 · persistent_

charter report (0.60.0) quirks when filing upstream: (1) the issue title is the body first line, markdown heading marker stripped, CUT at 72 chars with an ellipsis (report.py _TITLE_MAX) - write a title of 72 or fewer or it publishes truncated; (2) the scrubber replaces any word equal to a local workspace name with [workspace] even in prose naming a feature (a workspace named after the feature it builds loses the feature name) - reword so the literal name is absent, then charter report delete the old draft and re-draft, since drafts are keyed by a hash of the text; (3) the PreToolUse guard refuses any charter report line containing a live command substitution, so pass --from-file and read the id from the output; (4) send --dry-run shows the exact body but runs no duplicate search - send runs a title search and needs --new once candidates are judged not duplicates.
