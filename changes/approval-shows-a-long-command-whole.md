### Security

- **Approving a profile shows its whole command, one way only.** The new-chat picker, the
  first-task tab and a waiting chat's *Review and approve…* used to clip a long command at the
  display limit, so the end of it was never on screen when you approved it. Every word of the
  command and its environment, and the profile's kind, is now shown, escaped and quoted as a
  shell would need it typed, and the approval is checked against that whole line. A profile
  whose `env` names a variable a shell could not set (anything but letters, digits and `_`,
  not starting with a digit) is now refused (#1014).
