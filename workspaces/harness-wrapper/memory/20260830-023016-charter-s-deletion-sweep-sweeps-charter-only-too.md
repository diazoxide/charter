# charter's deletion sweep sweeps `charter/` only (tools/sweep.py --path d

_2026-08-30 02:30 · persistent_

charter's deletion sweep sweeps `charter/` only (tools/sweep.py --path defaults to charter), so a guard implemented as a test in tests/ yields 0 mutations and a trivially green gate. The honest equivalent is to mutate the WORLD the guard asserts about — the tracked record, the workflow file — one change at a time, and check which test reddens and with what message. Did this for #473: 18 world-mutations, 18 red, 0 survivors.
