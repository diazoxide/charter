# charter/contain.py (added 0.47.x) is the ONE containment helper for name

_2026-08-21 18:38 · persistent_

charter/contain.py (added 0.47.x) is the ONE containment helper for names read out of committed files: segment_ok (shape) + child (lexical join). Called from persona.reference_ok, commands._clone_one, commands._https_url, commands_workspace.cmd_workspace_restore, inventory.merge. Containment is LEXICAL on purpose — following symlinks would do half of #336 and would refuse planes that symlink a persona dir.
