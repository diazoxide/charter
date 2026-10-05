### Added

- **A request budget per forge account.** Every request purlis sends as a forge account is
  counted as its forge counts it (GitHub does not count a `304`, GitLab does), background refreshes
  wait once the account's 1,000 requests for the hour are spent, polling is conditional and paced
  by focus, stops while every window is hidden and backs off below a fifth of the forge's own
  limit, and `purlis doctor` shows each account's budget and its use (FW-4, #731).
