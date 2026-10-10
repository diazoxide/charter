### Fixed

- **Settings' dispatch table reads again once the first check after a launch lands.** In the
  first moments after a launch it says the project's grants you accepted are not checked
  against the project's history yet. It used to stay that way until you pressed *Read again*;
  it now draws what that first check found on its own, and reads again each time purlis says
  what waits of the project's grants may have moved. A check that cannot read the history is
  said as such, and never draws those grants as counting (#1543).
