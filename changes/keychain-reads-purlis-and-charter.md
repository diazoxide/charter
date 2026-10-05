### Changed

- **New keyring items are stored under `purlis/`, and items under `charter/` are still read.** A
  new keyring vault's items, and a newly stored 1Password identity, are written under `purlis/…`.
  A vault or identity stored before the rename keeps working from its `charter/…` item, and the
  check that an item is charter's own accepts exactly those two prefixes. 1Password items charter
  writes are tagged `purlis` (RN-4, #1261).
