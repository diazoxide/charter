# A LOADED MACHINE TURNS THE SWEEP INTO NO ANSWER, NOT A GREEN ONE. An age

_2026-09-04 01:47 · persistent_

A LOADED MACHINE TURNS THE SWEEP INTO NO ANSWER, NOT A GREEN ONE. An agent ran tools/sweep.py locally while the full unittest suite was running on the same box and got 22 out_of_time results; the same tree on an idle machine reported 7 real survivors. out_of_time is not a survivor and not a kill — it is the sweep failing to decide, and reading a pile of them as "nothing survived" is a false green you built by overloading the box. Never run the sweep concurrently with the suite, and treat any out_of_time count above a couple as an invalid run to be repeated, not a result.
