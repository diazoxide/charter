### Added

- **A project can say which harness profiles a persona's tasks run on.** `[dispatch.profiles]`
  in the project's committed file lists them per persona (`devops = ["work"]`). A chat
  dispatched to that persona, as a task or a handoff, then starts on one of the listed
  profiles or not at all, whoever chose the profile: the asking chat with `--profile`, the
  persona's own definition, or the asking chat's own profile. The refusal names the profiles
  the project lists. A persona with no list is dispatched to on any profile the project
  offers, as before. A list only narrows: a listed profile is still one the project offers
  and this machine approved, and one that starts its harness with the permission prompts off
  is still refused for any chat another chat starts (#1509).
