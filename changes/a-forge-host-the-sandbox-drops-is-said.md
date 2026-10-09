### Changed

- **A forge host the sandbox does not let through is named.** While a project's sandbox and its
  `forge` preset are on, a `[[forge]]` host that no chat may reach (this machine, a link-local or
  metadata address, a name ending in a number) used to be left out without a word. Saving one in
  Settings now says which host it is and why. A host already in the file does not stop other
  saves, and turning the sandbox on is never refused because of one (#1405).
