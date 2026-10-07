### Added

- **A persona has an icon and a colour, shown wherever it appears.** A persona's definition may
  carry `icon:`, one of purlis's forty persona icons, and `color:`, a palette name or
  `#rrggbb` as a workspace's colour is. Its folder may hold a custom `icon.png` instead. With
  none of these it is its initials, on a colour picked by its name. The mark is on its chats'
  tabs, their rows in the explorer, the Personas panel, its own view, needs-you items, Notices
  that name it and the new-chat picker. The persona's view has the picker, and a pick is
  written to the definition at once. A custom image is treated as untrusted: a PNG of at most
  64 KB, drawn as pixels and never as markup. One that cannot be drawn shows the initials, and
  the persona's view says why. An `icon.svg` is not drawn: the view says to save it as a PNG
  (#1449).
