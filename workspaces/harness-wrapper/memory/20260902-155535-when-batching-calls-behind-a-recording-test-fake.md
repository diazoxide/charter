# When batching calls behind a recording test fake, teach the FAKE to spli

_2026-09-02 15:55 · persistent_

When batching calls behind a recording test fake, teach the FAKE to split the batch back into its constituent calls so its call list keeps one entry per logical call. Otherwise dozens of positional assertions (a[-2], a[-1]) silently start reading only the last command of a batch and stop meaning anything — while still passing.
