### Security

- **Your own ask and deny rules hold under every spelling of the program they name.** A harness
  matches `purlis guard ask` rules against the command as written, so another spelling of the
  same program could run with no prompt. purlis's guard now refuses those spellings, with the
  same reading it uses for its own consent rules, and a rule on the command line holds under
  both of its names (#1286).
