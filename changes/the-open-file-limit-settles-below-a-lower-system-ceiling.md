### Fixed

- **The open-file limit is raised as far as the Mac allows.** When macOS is set to allow a
  process fewer than 10 240 open files, purlis used to stay at the 256 it was started with. It
  now raises the limit to the most the system accepts. On Windows, the launch log no longer says
  the open-file limit is unlimited: it says the platform has none to raise (#824).
