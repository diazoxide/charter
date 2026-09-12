# To pre-resolve tools/sweep.py survivors without editing a tracked file o

_2026-09-11 16:11 · persistent_

To pre-resolve tools/sweep.py survivors without editing a tracked file or invalidating a running sweep: in a separate python process, take inspect.getsource(fn) of the shipped function, str.replace the guard with its mutant, exec the result into the module's __dict__, and diff _leak_reason verdicts (exceptions included) against the shipped function over the fixture corpus plus generated shapes. 0 diffs names a deletion candidate; any diff hands you the pin input. Used on charter #973 round 1 (11,471 inputs): it separated a masked cluster of three empty-command filters (delete two, pin the third) from the load-bearing guards before the sweep finished.
