### Changed

- **A chat moving redraws only that chat's mark.** What every chat is doing is held in a store
  outside the project view, and each tab, explorer row and pane reads its own chat from it. A
  chat's move used to redraw every tab and pane in the project, twice; now it redraws that
  chat's mark. When the needs-you queue changes with it, as most moves between running and
  waiting do, the pane in front is redrawn once as well (#1034) (SC-3, #681).
