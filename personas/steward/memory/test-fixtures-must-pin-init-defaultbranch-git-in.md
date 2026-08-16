# Test fixtures must pin init.defaultBranch: 'git init --bare' WITHOUT -b

_2026-08-16 09:23 · persistent_

Test fixtures must pin init.defaultBranch: 'git init --bare' WITHOUT -b takes HEAD from the machine's git config, so a clone of it has no upstream on a runner defaulting to master while working fine on a Mac defaulting to main. Every 'behind' assertion then reads as the code being wrong. Pass -b main to bare repos too, and make fixtures assert their own preconditions.
