## Context & decisions

<!-- Started from charter's TypeScript project template. Replace it with what you learn. -->

How a change in this repo is checked:

- the package's own scripts first: read `package.json` for `typecheck`, `lint` and `test`
- `npx tsc --noEmit` when there is no typecheck script
- one test file at a time while you work, the whole suite before you hand back

`typescript-engineer` makes changes and `typescript-reviewer` reviews them.
