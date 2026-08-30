# Two constants in charter/frame were documentation rather than mechanism 

_2026-08-30 03:04 · persistent_

Two constants in charter/frame were documentation rather than mechanism until Phase 5 Stage 5b: layout._DROP_ORDER was read by nothing (visible_slots spelled the order out as s != 'right' / s != 'top', so adding a name changed no behaviour — now layout._ROW_DROPS derives from it), and instance.component_tables asked 'cid in builtins.SLOT_OF' where it meant 'is this one charter places', so a built-in charter registers with an edge but no committed [frame] slots word could not be placed by ANY config — builtins.places(cid, reg) asks it directly off Registry.on_edge.
