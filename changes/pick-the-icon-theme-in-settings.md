### Added

- **Pick the file icons in Settings.** Appearance, at the Project and the Workspace level, has an
  *Icons* list: purlis's own icons and every icon theme an extension approved on this machine
  contributes. A pick that cannot be drawn stays shown, with the reason purlis's own icons are
  drawn instead (#1145).
- **Search results show file icons.** Each file in the Search tab is drawn with its project's
  icon theme, as the file trees draw it (#1145).
- **The Extensions dialog says what an icon theme got wrong.** A contributed icon theme with a
  path that is not path data, a colour that is not an icon token, or a name mapped to no symbol
  is listed on its extension's row, at most three complaints at a time (#1145).
