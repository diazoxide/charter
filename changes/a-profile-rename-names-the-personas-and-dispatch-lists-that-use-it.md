### Changed

- **Renaming or removing a profile names the personas and dispatch lists that use it.** A
  persona whose definition names the profile with `profile:`, and a `[dispatch.profiles]` list
  in the project's file that holds it, now stop a rename, a rename everywhere and a removal, and
  each is named before anything changes. Neither file is rewritten from this machine (#1380).
