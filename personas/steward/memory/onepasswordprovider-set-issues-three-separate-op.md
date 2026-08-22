# OnePasswordProvider.set() issues THREE separate 'op item get' calls (_fi

_2026-08-22 10:33 · persistent_

OnePasswordProvider.set() issues THREE separate 'op item get' calls (_fields, _item_present, _existing_ids) all fetching the same document — the redundancy that made #354 reachable; collapsing them into one read is issue #355, deferred because no machine in the project has a real 1Password vault to verify the write round-trip against
