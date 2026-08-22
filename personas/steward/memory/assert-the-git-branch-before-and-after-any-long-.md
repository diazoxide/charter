# Assert the git branch BEFORE and AFTER any long verification run in a wo

_2026-08-22 11:25 · persistent_

Assert the git branch BEFORE and AFTER any long verification run in a workspace clone: a full-suite run on the wrong branch still prints OK, so a silently-moved HEAD renders as a passing verification of code that was never tested — the same 'failure as benign state' class the authority audit is about. Track the expected test count too; 3214-vs-3227 was the only reason it was caught
