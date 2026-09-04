# A BACKGROUNDED SUITE CAN REPORT EXIT 0 WHILE ITS TRAILER SAYS FAILED — R

_2026-09-03 15:49 · persistent_

A BACKGROUNDED SUITE CAN REPORT EXIT 0 WHILE ITS TRAILER SAYS FAILED — READ THE TRAILER, NOT THE STATUS. Measured on #857: the harness reported 'exit code 0' for a backgrounded test run whose own output ended 'FAILED (failures=109, errors=4)'. The exit status came from the wrapper, not the suite. This is a THIRD distinct false-green mechanism in this repo, alongside macOS having no 'timeout' ([[macos-has-no-timeout-command-and-the-failure-is-]]) and a pipeline ending in grep ([[a-pipeline-that-ends-in-grep-reports-a-green-sui]]). Rule, now three times over: NEVER accept an exit code as evidence a suite passed. Find and quote the 'Ran N tests ... OK' or 'FAILED (...)' line. If you cannot find that line, the run did not finish and you know nothing.
