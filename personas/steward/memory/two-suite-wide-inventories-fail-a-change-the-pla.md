# Two suite-wide inventories fail a change the plan's per-task test lists

_2026-09-11 15:20 · persistent_

Two suite-wide inventories fail a change the plan's per-task test lists never mention (measured on harness-profiles Task 1, main 6526928): (1) tests/test_the_end_of_a_name_is_the_end_of_the_string.py discovers EVERY module-level compiled regex in charter/ ending in a bare $ and fails until it is filed in ADMITTERS (with a sample value, asked via fullmatch), DETECTORS, LINE_SCANNERS or SUBSTITUTIONS; (2) adding a name to tests/_planeguard._GUARDED_SETTINGS refuses every plain unittest.TestCase that reaches a reader of that config value against the real plane — tests/test_cmd_harness.HarnessList had to move onto PersonaIso when charter harness list began reading config.PROFILES. Run the whole suite after the first change (ruling 31) rather than trusting a plan's list.
