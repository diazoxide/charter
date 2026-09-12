# charter tools/sweep.py selection sizing, measured 2026-09-12 on PR #981

_2026-09-12 08:01 · persistent_

charter tools/sweep.py selection sizing, measured 2026-09-12 on PR #981 (135 mutations, 10 files, 479 test modules). A mutation is charged the test modules that EXECUTE its line, so a module-level constant is charged every test module that imports the file: commands_frame.py message literals selected 470 of 479 modules each, state._PROFILE_FILE 126, tmuxctl._PANE_PID_FORMAT 97, while ordinary in-function mutations had a median of 8. That is a property of where the constant sits, not of the feature under test — unlike the pre-ruling-43 profiles.py case, where a whole FILE was on the import path and every mutation in it cost ~347 modules. Read a per-file predicted-minutes line in the "Size the sweep" job with that in mind: commands_frame.py 87 min came mostly from four string literals.
