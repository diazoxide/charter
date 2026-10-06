# Reviewing a Rust change

What `rust-reviewer` checks in every change. Edit it to match how this project works: it is
yours, and a chat reads the copy here.

- [ ] Does it build without new warnings, and does clippy pass with `-D warnings`?
- [ ] Is every `unwrap`, `expect` and `panic!` on a path that truly cannot fail, and does the message say why?
- [ ] Is there `unsafe`? It needs a `// SAFETY:` comment that a reviewer can check.
- [ ] Do errors carry enough context for the person who reads them, and are they returned rather than printed?
- [ ] Did a public type, function or feature flag change? Then the version and the changelog say so.
- [ ] Are new dependencies needed, maintained, and licensed in a way this project accepts?
- [ ] Do the tests describe behaviour, and was each new one seen to fail first?
