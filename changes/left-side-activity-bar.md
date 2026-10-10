### Changed

- **The left side has an activity bar, and shows one view at a time.** An icon strip at the
  window's edge switches the left side between Chats and Explorer; pressing the open view's icon
  puts the side away, and the strip stays with a count of the chats that need you. ⌘B puts the
  side away and brings it back, ⌘⇧E shows Explorer and ⌘⇧C shows Chats (Ctrl elsewhere), and
  both views are in the palette. Each project remembers which view it showed, how wide the side
  was and whether it was away. The explorer no longer lists chats: the Chats view is the one
  place they are. The status line's toggle for the left side is called Navigation now (#1673).
- **The layout file is version 2.** Each project keeps its own arrangement under `projects`,
  and the left region is `navigation`. A version 1 file is read and moved forward at the first
  change, so every arrangement you saved is kept (#1673).
