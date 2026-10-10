### Fixed

- **Renaming a workspace keeps the links to its todos.** A chat linked to one of the workspace's
  todos still reaches it under the new name: `purlis workspace rename` writes a `renamed` alias
  for each todo key a work link names, and a rename finished after an interruption writes each
  alias once. Renaming a workspace back to a name it had says which links stay under the name it
  left (#887).
