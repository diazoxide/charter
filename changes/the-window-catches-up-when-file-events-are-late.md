### Fixed

- **The window catches up when macOS's file events are late.** On a busy Mac the file-event
  service can hand purlis a change seconds or minutes late, and a todo closed or a memory saved
  in a terminal stayed on the panels until then. While a window is shown, purlis now also looks
  at the project's folders every 5 seconds, and at once when you come back to the window, and
  tells the panels and the sidebar what the events had not. It stops looking while every window
  is hidden. The doctor's new **file events** row says when the events were late in the last
  10 minutes (#756).
