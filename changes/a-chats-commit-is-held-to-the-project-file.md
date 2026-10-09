### Changed

- **A chat's commit is held to the project's file as it stands.** A chat's commit is refused
  where the project's file it stages differs in `[sandbox]`, `[chat_env]` or `[dispatch]`
  from that file in the working tree, wherever the project sits in its repository. A staged
  or working file purlis cannot read, or one staged as anything but a regular file, is
  refused too. Committing the file as it stands is not refused (#1464).
