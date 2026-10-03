### Security

- **A chat's push is scanned before it is sent.** git's `pre-push` in a chat now runs the commit
  scan over every commit the push would send that the remote does not have, so a secret in a
  commit that never passed the commit scan (made in your own terminal, or by a cherry-pick,
  rebase or `git am`) is refused before it is published. The refusal names the file and line,
  masks the value, and puts the chat in needs-you. A push that would publish a change to the
  scan's allowlist is refused too: you push those yourself. The repository's own `pre-push` still
  runs, and the guard refuses `git push --no-verify` and `git send-pack` (SQ-7, #586).
