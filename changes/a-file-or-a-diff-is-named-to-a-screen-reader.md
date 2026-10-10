### Fixed

- **A screen reader names the text of a file and both sides of a diff.** The light editor's text
  was a text box with no name, so a screen reader said only "text box". It now says the file's
  path, and in a comparison which side it is, before or after. Every control the Settings tab and
  the editor tabs draw is now checked for a name (DS-6, #629).
