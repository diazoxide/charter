### Added

- **What a session's tasks used.** The tab's task menu ends with one line that adds up what
  its chats used: `5 tasks · 310k tokens · 6m`, the tokens of the session's own chat and of
  every task, open or ended, and how long the tasks worked, added up. Its tooltip says what it
  adds up. Rest the pointer on a task, in that menu or in the Chats list, and it says the
  task's tokens in and out. They are the figures each chat's harness reported; a task that has
  ended keeps what it used. A harness that reports no tokens shows a dash, never a zero, and the
  total then says `at least`. No money figure is shown. Nothing is read on a timer: the figures
  are read when the menu opens, when one of its chats changes state, and when the pointer comes
  onto a task (#1500).
