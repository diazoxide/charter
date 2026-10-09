### Fixed

- **Undoing a memory's Move puts it back under its exact name.** A workspace memory moved to a
  persona or shared memory and back by Undo now has its first name again to the second. Before,
  the seconds of its name came back as `00`, because the memory's stamp line holds only the
  minute (#1190).
