# Reviewing a Python change

What `python-reviewer` checks in every change. Edit it to match how this project works: it is
yours, and a chat reads the copy here.

- [ ] Does it run on the Python versions the project supports?
- [ ] Is every exception caught for a reason, and never a bare `except:`?
- [ ] Are files, sockets and processes closed, with `with` where it fits?
- [ ] Is user input kept out of `eval`, `exec`, `pickle`, a shell string and a raw SQL string?
- [ ] Does a change to a public function or a command-line option come with a version bump and a changelog line?
- [ ] Is a new dependency needed, pinned the way the project pins, and licensed in a way this project accepts?
- [ ] Do the tests describe behaviour, and was each new one seen to fail first?
