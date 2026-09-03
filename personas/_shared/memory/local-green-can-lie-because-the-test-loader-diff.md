# LOCAL GREEN CAN LIE BECAUSE THE TEST LOADER DIFFERS FROM CI'S. A test mo

_2026-09-02 21:26 · persistent_

LOCAL GREEN CAN LIE BECAUSE THE TEST LOADER DIFFERS FROM CI'S. A test module using a RELATIVE import ('from ._isolation import ...') passes under 'python -m unittest tests.<mod>' — which loads it as a package member — and FAILS under CI's 'unittest discover -s tests', which loads modules top-level. Found on #822: the failure appeared only in CI, and no amount of local re-running would have reproduced it. Rule: before trusting a green local suite, run the suite THE WAY CI RUNS IT, not the way that is convenient. This is a second instance of the same class as the cwd leak — see [[a-test-that-chdirs-must-restore-cwd-with-addclea]] — where the local invocation and the CI invocation differ in a way that hides a real defect.
