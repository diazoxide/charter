### Changed

- **A hidden window reads nothing on a timer.** While the purlis window is minimised or out of
  sight, it stops reading the save standing, a running chat's context gauge, a running
  dispatch and the time a chat has been in its state. Each is read once the moment the window is
  shown again, so what changed meanwhile is on screen at once (#1392).
