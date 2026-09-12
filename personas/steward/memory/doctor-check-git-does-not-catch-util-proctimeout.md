# doctor.check_git does not catch util.ProcTimeout (main 6526928, charter/

_2026-09-11 15:05 · persistent_

doctor.check_git does not catch util.ProcTimeout (main 6526928, charter/doctor.py check_git: util.run(['git','--version'], check=False) with no try). So a test that patches util.run to time out for EVERY git call takes doctor.run_all() down at check_git, before any later row runs. A test proving 'one slow git costs one row' must scope the hang to its own git call (e.g. argv starting ['git','--no-optional-locks']). Found building harness-profiles Task 1's doctor row, 2026-09-11.
