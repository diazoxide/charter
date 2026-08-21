# charter validates a name when a human types it (valid_name at persona.py

_2026-08-21 17:55 · persistent_

charter validates a name when a human types it (valid_name at persona.py:41 and workspace.py:81, six command call sites) and never when it reads the same name from a committed file — extends:/uses:/[persona] default, workspace.json repo names, inventory repo names and ssh_url all reach paths or argv unvalidated. Tracked as issue #328; forge-surface instances are #323 and #325.
