## Context & decisions

<!-- Started from charter's Monorepo project template. Replace it with what you learn. -->

How a change in this repo is checked:

- the workspace tool's own commands first (pnpm, turbo, nx, Cargo or Go workspaces): read its config at the repo's top level
- build and test only the packages the change touches, and the packages that depend on them
- the whole repo's checks once, before you hand back

`monorepo-engineer` makes changes and `monorepo-reviewer` reviews them.
