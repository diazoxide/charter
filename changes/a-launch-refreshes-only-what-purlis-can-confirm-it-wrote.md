### Security

- **A launch brings a generated or mirrored file up to date only when purlis can confirm it
  wrote the one there.** In a workspace folder and in a checkout, purlis used to overwrite a
  settings file, agent or skill with the project's newer text on the word of a record kept
  beside it, which a chat might be able to write. Now the project's app state, which no chat
  can write, must also note offering that very text at that path. A file only the record
  vouches for is left as it is, and `purlis workspace reinit` says so and what replaces it
  (#1583).
