### Security

- **A typed `/smart-close` is read from your own keys.** purlis now gives a chat its smart-close
  pass only when you typed `/smart-close` or `/purlis:smart-close` into its pane, with or
  without words after it, or typed at least `/sm` and pressed Enter on the picker. A report from
  inside the chat can no longer give it a pass on its own. If you reach Smart close another way,
  such as an arrow key in the picker or a recalled line, the chat still saves its session record,
  and the tab stays open with a notice that says the record was saved and offers **Close tab**. The tab's **Smart close** works
  as before (#1361).
