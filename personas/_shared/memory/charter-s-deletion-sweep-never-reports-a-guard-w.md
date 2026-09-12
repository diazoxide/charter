# charter's deletion sweep never reports a guard whose plain deletion rais

_2026-09-11 22:30 · persistent_

charter's deletion sweep never reports a guard whose plain deletion raises NameError. Deleting a line such as an assignment that later lines read (hooks.py:1068's delim/expands/dash unpack, PR 972, 2026-09-11) makes the module raise, the mutant dies on that error, and the sweep scores it KILLED — so a line whose real behaviour is unpinned still looks covered. A green sweep therefore does not mean every behavioural line has a test. Cover such a line from the INPUT side instead: find a shape whose verdict differs when the line's value is computed the old way (there, a brief with a split-quote delimiter naming a vault path in prose), and pin that. This sits beside the memories on the sweep scoring by exit code and on masked-cluster survivors: three different ways a green sweep can hide an untested guard.
