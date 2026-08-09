# They were NOT in lockstep for twelve releases — CLI at 0.13.1, both plug

_2026-08-09 22:54 · persistent_

They were NOT in lockstep for twelve releases — CLI at 0.13.1, both plugin artifacts at 0.1.0 — because skew_message is one-directional (speaks only when the plugin is newer) and the only test reading those flags checked they existed, not their value.
