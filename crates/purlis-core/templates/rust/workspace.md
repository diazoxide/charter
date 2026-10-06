## Context & decisions

<!-- Started from charter's Rust project template. Replace it with what you learn. -->

How a change in this repo is checked:

- `cargo fmt --all --check`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test` (one test target with `--test <name>` while you work, all of them before you hand back)

`rust-engineer` makes changes and `rust-reviewer` reviews them.
