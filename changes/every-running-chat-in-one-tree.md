### Added

- **Every running chat of the project is in one tree.** The left region has a **Chats** section
  above the explorer: every chat in every workspace, nested under the chat that started it,
  each with its persona's mark, its name, its workspace, its state and a hand when it needs
  you. A chat whose parent has closed stays at the top and says where it came from. Pressing a
  row brings that chat forward. In the explorer, a chat that started one in another workspace
  shows it under its row, with a badge naming that workspace. A handoff opens as a tab, as
  before. A chat started as a task is listed with no tab until you press its row, which opens
  an ordinary tab, and that tab comes back after a reload or a relaunch (#1447).
