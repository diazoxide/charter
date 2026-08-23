# charter dev sandbox has no separately installed charter-cp — python3 -m 

_2026-08-23 18:27 · persistent_

charter dev sandbox has no separately installed charter-cp — python3 -m charter only resolves the package via the -m cwd-prepend trick (repo root as cwd). Testing the #390 -m/-P cwd-shadowing bug hermetically needs PYTHONPATH=<repo> as a stand-in for a real install; -P/PYTHONSAFEPATH strip only the cwd/script-dir sys.path entry, never PYTHONPATH or site-packages, so this is a faithful substitute.
