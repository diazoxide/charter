# git check-ref-format ACCEPTS refs/heads/-b — a leading dash is legal ins

_2026-08-21 18:38 · persistent_

git check-ref-format ACCEPTS refs/heads/-b — a leading dash is legal inside a ref, so ref grammar is not argv safety. Guarding a branch name taken from a file means refusing a leading '-' plus argv position (checkout <branch> --), not check-ref-format. Verified git 2.50.1.
