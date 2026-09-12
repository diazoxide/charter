# In charter's deletion sweep, a mutation on a MODULE-LEVEL CONSTANT selec

_2026-09-12 10:18 · persistent_

In charter's deletion sweep, a mutation on a MODULE-LEVEL CONSTANT selects every test module that imports that file, so its covering set says where the constant lives, not what the feature costs. Measured on PR 981 (harness profiles task 2, 2026-09-12): 152 mutations over 10 files had a median of 8 modules and a mean of 26, but four message literals in commands_frame.py selected 470 modules each, state._PROFILE_FILE 126, tmuxctl._PANE_PID_FORMAT 97. Read a sizing report per mutation kind before concluding a file needs the ruling-43 treatment (moving code off an import path): that was right for profiles.py, whose EVERY mutation ran ~347 modules, and wrong here. A shard budget overrun from sheer volume is a different problem: split the branch or read the count as a floor.
