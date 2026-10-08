### Added

- **A dispatch refused while you were away is shown when you are back.** A chat running with
  its permission prompts off dispatches only under a grant that already stands, as before, and
  is refused at once where there is none. That refusal is now kept, and the hand in the title
  bar lists it: "steward wanted devops while you were away", how many times and when, with
  **Allow from now on** and **Dismiss**. Allow from now on makes the grant for you on this
  machine for that one pair, so the next run works; it starts nothing. Dismiss takes the item
  away. The chat is told its refusal as before, and that you will see it. The item belongs to
  no chat: no tab shows a hand for it (#1507).

### Security

- **What that item can and cannot do.** Nothing is asked of a chat nobody is at, and what
  such a chat may do at the time is unchanged. Allow from now on grants one named pair, for
  you, on this machine, and says so before you press; it never offers a grant for everyone in
  the project or for any persona. It is checked again when you press: a pair you said never
  to, or one a policy locks, is not granted, and such an item is gone by the time you look.
  The grant is recorded in purlis's event log as yours. Only a refusal a grant would mend is
  kept: not one for a never, a policy lock, a limit or a loop. A chat asking over and over
  raises one item's count, the list has a cap, and items go after 30 days. The brief is not
  kept, and the item's words are purlis's own, apart from the task's name (#1507).
