# BLOCKED ON HARDWARE, not on a decision: #355 (three-fetch collapse in th

_2026-08-22 17:35 · persistent_

BLOCKED ON HARDWARE, not on a decision: #355 (three-fetch collapse in the 1Password provider) needs a machine with a real 1Password account — its correctness lives in the op item edit/create round trip, and there is none configured here. A fake runner cannot verify it, and shipping on a fake is the 0.46.2 mistake.
