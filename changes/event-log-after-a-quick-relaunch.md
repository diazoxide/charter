### Fixed

- **The event log no longer turns away a quick relaunch.** Opening the device's event log waits
  up to a second for a lock that a moment ago belonged to this host or a just-exited one, held
  only by a program still starting, instead of running the session without recording hook calls
  as if another host were writing it (#972).
