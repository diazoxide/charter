### Fixed

- **A task's record follows the local project to its new place.** When purlis moves the local
  project out of its config folder, a task record that named a folder in the old place by its
  whole path now names it in the new place, as the record of open chats already did. Before, the
  task read as working in a folder that was gone (#1698).
