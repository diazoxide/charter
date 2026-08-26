# Guard-bypass technique that keeps working: when a parse bug is found, se

_2026-08-26 19:42 · persistent_

Guard-bypass technique that keeps working: when a parse bug is found, search for the same SHAPE, not the same spelling. The #547 fix (glued short form read after the long form's '=') was told to sweep siblings and found env -C was a LIVE bypass, not theoretical: after mkdir 'x=y', 'env -Cx=y/../.charter/vaults cat x.json' printed the vault and the guard said ALLOW. Same parse in sudo -D and xargs -a. Two more neighbours filed (#555 non-identifier assignments, #556 values glued to BUNDLED short options) and confirmed live on main by reproduction, not reasoning.
