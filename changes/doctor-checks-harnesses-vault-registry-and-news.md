### Changed

- **`purlis doctor` checks three more of the rows it used to leave unchecked.** `harness` names
  every harness the project has and the most each offers (hooks, ACP or its terminal alone),
  and warns about a declaration in `harnesses/` that purlis refused. `vault registry` reads both
  halves of the vault registry, lists the vaults with their providers, and warns about a half
  that cannot be read or an entry no command can use; it opens no vault and asks no provider.
  `news` says whether this build carries its own release notes. The `mcp` row, still unchecked,
  now gives a reason of its own (#994).
