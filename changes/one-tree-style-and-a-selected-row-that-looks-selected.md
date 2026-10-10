### Changed

- **The chat in front looks selected, and every tree draws its levels the same way.** The Chats
  tree, the explorer and a file tab's files share one tree style: a straight guide per level
  under the row it hangs from, and the selected row (the chat in front, the spot the next chat
  starts in, the file shown) drawn with an accent fill and an edge. A row under the pointer and
  the row the keyboard is on each look different from it. Themes gain `list.selected`,
  `list.selected-edge` and `tree.guide` (#1672).
