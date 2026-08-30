# Charter's signature defect has a recurring sub-shape worth checking firs

_2026-08-30 13:40 · persistent_

Charter's signature defect has a recurring sub-shape worth checking first: the property IS written down and a test DOES assert it, but the assertion sits on the one code path that already satisfies it. Found 3x in one review of the 2026-08-30 frame PRs — slots._repos settles the scroll bound on the path that reaches settle while three early returns go round it; _RESIZE_FLAG's axis map is asserted for the four names in it while a placed component has none; the pad-bound doc checker matches backticked digits so the same wrong sentence passes unbackticked. When reviewing, do not ask 'is there a test' — ask 'which paths does that test reach, and what returns above it'.
