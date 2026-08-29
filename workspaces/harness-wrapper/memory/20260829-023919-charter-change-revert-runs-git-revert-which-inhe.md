# charter change revert runs 'git revert', which inherits the operator's c

_2026-08-29 02:39 · persistent_

charter change revert runs 'git revert', which inherits the operator's commit.gpgsign — on a machine using 1Password's op-ssh-sign that blocks forever waiting for approval and the test suite hangs with no output. Production must NOT pass -c commit.gpgsign=false (a revert is the operator's commit; ADR 0014 puts signing policy with the host, and charter git-policy --apply already writes commit.gpgsign=false into each managed clone's LOCAL config). Fix is in the fixture: git config --local commit.gpgsign false on every test repo. Measured 2026-08-29.
