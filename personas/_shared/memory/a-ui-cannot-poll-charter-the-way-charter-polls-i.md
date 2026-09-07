# A UI CANNOT POLL CHARTER THE WAY CHARTER POLLS ITSELF — 90 ms SUBPROCESS

_2026-09-07 11:19 · persistent_

A UI CANNOT POLL CHARTER THE WAY CHARTER POLLS ITSELF — 90 ms SUBPROCESS FLOOR vs 25 us IN-PROCESS. Measured on this machine, median of 12, warmed: state.version() in-process 25.3 us (confirms the documented ~26 us); full render model in-process 0.79 ms; `charter --version` 90 ms; `charter status` 163 ms; python3 -c pass 18 ms. About 60 ms of every invocation is interpreter plus import before charter does any work. Polling `charter status` at charters own 5 Hz would cost 81% of ONE CORE for ONE panel; charters own loop costs 0.013%. So "the UI just shells out to the CLI in a loop" is wrong by ~3600x. The right shape is the one charter already uses: poll something that costs a stat, fetch the expensive thing only when it moves — which needs a `charter frame watch --json` streaming the version bump, because state.version is a file today with no command that prints it.
