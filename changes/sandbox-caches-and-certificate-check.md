### Fixed

- **Builds work in a sandboxed chat.** With the *toolchains* preset on, a project's sandboxed
  chats download into package caches of the project's own, so `cargo build`, `npm install`,
  `pip install`, `go build`, Gradle, yarn and pnpm no longer fail with "Operation not permitted".
  purlis keeps them in `cache-homes/<project>` under its data home (on macOS,
  `~/Library/Application Support/purlis/cache-homes/`; remove a project's folder there to free
  its space) and points each tool at them; your own caches are only read. The project's cargo home starts with where your crates come from (registries and source
  replacements), never your tokens. Each project downloads once into its own caches, which
  costs disk (#1337).

### Added

- **`certificate-checks` under `[sandbox]`.** Set it to `true` to let Go tools such as `gh`
  verify certificates in a sandboxed chat on macOS. It also lets a chat reach hosts named inside
  a certificate, so it is off by default (#1337).

### Security

- **A sandboxed chat never writes your package caches.** A cached package is code a later
  build runs, so a chat's downloads go to caches only that project's sandboxed chats use. Each
  cache folder purlis makes is pinned, and a link planted in one is taken out, unfollowed,
  before the chat starts.
  In cargo's git cache, a chat writes inside each repository cargo made, never a new one, and
  never a repository's config or hooks.
