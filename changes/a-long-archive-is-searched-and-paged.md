### Changed

- **A store's archive tab is searched and paged, and a numbered archive name restores under its
  own name.** An archive holding more than a page shows one page and _Show more_, with a search
  box over each archived memory's title, stamp and text, the way a memory list does. A memory
  archiving had to number (`freeze-2`) comes back as `freeze` when the store has no `freeze`, and
  under its archived name when it has, and the tab says which name it came back under (#1191).
