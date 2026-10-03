### Changed

- **The Rust toolchain is pinned.** `rust-toolchain.toml` names Rust 1.99.0 instead of `stable`,
  so a new Rust release no longer changes a local build or CI on the day it ships. CI installs
  exactly that version, and Dependabot proposes each bump (#888).
