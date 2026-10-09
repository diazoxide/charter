### Changed

- **The project-root guard's `cd` hint says what `&` does, and comes with every named refusal.**
  A `cd` sent to the background by `&` never moves the shell at all, which the hint now says
  apart from the `;`, `||` and newline case. The refusal of an alias chain longer than purlis
  follows gains the same hint where such a `cd` comes before it. Nothing that was refused is
  allowed now (#1093).
