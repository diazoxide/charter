# A change that only DELETES a disjunct offers the sweep nothing: tools/sw

_2026-09-01 13:44 · persistent_

A change that only DELETES a disjunct offers the sweep nothing: tools/sweep.py reported 'NOTHING TO SWEEP — 0 mutations applied' for #791, whose whole code change was 'return ws_mod.for_session(fid) or frame_workspace(fid)' -> 'return frame_workspace(fid)'. Read the mutation COUNT, not the verdict (#782), and supply hand mutations as the evidence instead.
