# Reviewing a Monorepo change

What `monorepo-reviewer` checks in every change. Edit it to match how this project works: it is
yours, and a chat reads the copy here.

- [ ] Which packages does the change touch, and does each one's own build and tests pass?
- [ ] Do the packages that depend on a changed one still build against it?
- [ ] Is a shared change versioned and released the way the repo releases its packages?
- [ ] Did the change stay inside the packages it meant to touch, with no unrelated file swept in?
- [ ] Is a new dependency added to the package that uses it, and not to the root?
- [ ] Do the tests describe behaviour, and was each new one seen to fail first?
