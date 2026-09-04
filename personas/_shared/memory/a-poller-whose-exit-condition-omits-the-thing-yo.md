# A POLLER WHOSE EXIT CONDITION OMITS THE THING YOU ARE WAITING FOR SETTLE

_2026-09-04 02:33 · persistent_

A POLLER WHOSE EXIT CONDITION OMITS THE THING YOU ARE WAITING FOR SETTLES INSTANTLY ON THE STATE YOU WANTED TO LEAVE. I sent an agent back to close out two deletion-sweep survivors on PR #872, then started a poller that broke on "CI green and mergeable CLEAN". Both were ALREADY true — the PR was green WITH the survivors — so it reported SETTLED against the unchanged head and the unchanged verdict check name. Waiting for work to be done means watching for evidence the work HAPPENED: the head SHA moving off the known-bad one, or the verdict check name changing to "no survivors". Green is the state before the fix as well as after it. Fourth false green of the same session, all one family: the exit condition, the empty gh body in $((pend + c)), the cached known_marketplaces.json, and a model that "found" a sentinel by running sed on the file I had just named it.
