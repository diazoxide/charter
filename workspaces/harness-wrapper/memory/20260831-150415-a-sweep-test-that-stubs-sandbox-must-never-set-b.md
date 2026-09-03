# A sweep test that stubs Sandbox must never set box.path to Path('.') — s

_2026-08-31 15:04 · persistent_

A sweep test that stubs Sandbox must never set box.path to Path('.') — sweep() ends with shutil.rmtree(box.path) for every box, so a stub pointing at the cwd DELETES the working tree. It cost a whole worktree of uncommitted work on 2026-08-31. Commit before running any test that constructs a Sandbox stub.
