### Added

- **A project can say which harness profiles a persona's tasks run on.** `[dispatch.profiles]`
  in the project's committed file lists them per persona (`devops = ["work"]`). A chat
  dispatched to that persona, as a task or a handoff or by your own "Ask a persona" from a tab,
  then starts on one of the listed profiles or not at all, whoever chose the profile: the
  asking chat with `--profile`, the persona's own definition, or the asking chat's own
  profile. The refusal names the profiles the project lists and says what can be done about
  it. A persona with no list of its own is held to the list of the persona it extends; one
  with none at all is dispatched to on any profile the project offers, as before. A list only
  narrows: a listed profile is still one the project offers and this machine approved, and one
  that starts its harness with the permission prompts off is still refused for any chat
  another chat starts (#1509).

### Security

- **A dispatched chat is held to its profile's rules each time it is started again.** When
  purlis is opened again, and on Restart chat, the restart a grant owes and Start fresh, a chat
  that another chat dispatched is no longer started on a profile whose command has come to
  switch the harness's permission prompts off since, or on one the project has stopped listing
  for its persona. Its row says why and what to do (#1509).
- **A Codex profile that sets its approval policy to never as a configuration override is
  treated as one that asks nobody**, as `--ask-for-approval never` already was, and so is a
  short flag written with its value attached. This holds for the chats that may be dispatched
  on such a profile and for the asking chat's own dispatches: a chat on one dispatches under
  standing grants only (#1509).
