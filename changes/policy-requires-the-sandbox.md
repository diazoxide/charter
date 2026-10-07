### Security

- **A policy that forbids the opt-out now requires the sandbox.** With `"opt-out": false` in
  `/etc/purlis/policy.json`, every chat on that machine runs sandboxed. A project with no
  `[sandbox]`, or one that has not turned it on, is run as if it had, with the default presets
  and within the policy's own presets and hosts. Settings › Sandbox shows the mode as "On,
  required by policy" with who set it, and no control. On a system with no sandbox backend
  (Windows today) a harness chat is refused and never started unconfined, and the refusal names
  the policy and its owner (#1423).

### Changed

- **A refusal no longer points to an opt-out policy forbids.** Where policy locks "Start
  without the sandbox", a chat that cannot start says what to change and names the policy and
  who set it (#1423).
- **A host policy does not allow is refused when you add it in Settings**, with the policy's
  sentence, where it was written and then never reached. The Granted list marks a host or folder
  you granted that policy now drops as not in force, and the Notice of a change to the project's
  hosts names only hosts a chat reaches (#1423).
