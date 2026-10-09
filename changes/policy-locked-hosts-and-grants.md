### Changed

- **A host an administrator's policy forbids is refused when you save the file.** Saving
  settings under Edit as TOML now refuses a new host the policy does not allow, as Add already
  did. Hosts the file already held are kept and still reach nothing (#1431).
- **You can revoke a grant the policy has locked out.** On Settings › Sandbox › Granted, an
  entry the policy no longer allows still has Revoke (#1431).
- **A refusal says when the policy requires the sandbox.** Where an administrator's policy
  requires the sandbox, a chat that cannot start sandboxed is told it is the policy that
  requires it, not the project (#1431).
