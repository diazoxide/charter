### Changed

- **The file preview plays animated images, and draws an SVG as a picture where it can.** An
  animated GIF, APNG or WebP plays on the preview's canvas, decoded frame by frame from its
  bytes, where the window's webview can decode frames (WebView2 can); it has *Pause*, stops while
  its tab is out of sight or the window is hidden, and shows one frame and *Play* under reduced
  motion. Where the webview cannot, the preview shows the first frame and says so. An SVG is
  decoded as an image, so it runs no script and loads nothing it names, with its text under
  *Source*; where the webview does not decode SVG that way, the preview shows its text as before.
  Nothing new is loaded from a URL (#1132).
