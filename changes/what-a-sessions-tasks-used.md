### Added

- **What a session's tasks used.** The tab's task menu ends with one line that adds up what
  its tasks used: `5 tasks · 310k tokens · 6m`, the tokens of every task, open or ended, and
  how long they worked, added up. Its tooltip says what it adds up, what it leaves out, and the
  session's own chat's figure, which is not in the total. Rest the pointer on a task, in that
  menu or in the Chats list, and it says the task's tokens in and out. They are the figures each
  chat's harness reported; a task that has ended keeps what it used. A missing figure is a dash
  with the reason (nothing reported yet, nothing reported, or not known), never a zero, and a
  total or a time that misses any task's part says `at least`. No money figure is shown.
  Nothing is read on a timer: the figures are read when the menu opens, when one of its chats
  changes state, and when the pointer rests on a task (#1500).
