### Security

- **A persona's hosts reach nothing on your machine until you allow them.** The hosts
  `[sandbox.personas.<persona>]` lists in the project's settings are ones a teammate can change,
  and they reach past the presets, often into a private network. The project view now shows
  each persona's list with **Allow for <persona> chats** and **Not now**, and a chat as that
  persona reaches them from its next start once you allow them. The Allow is kept for exactly
  the list you were shown: if the list changes, a host added or taken away, the persona's chats
  reach none of it until you allow it again. Your Allow is listed in Settings › Sandbox ›
  Granted, credited to the persona, with Revoke. A persona's hosts are no longer named in the
  project's own hosts notice. On a machine where a persona's hosts were in use, its chats lose
  them until you press Allow once (#1362).
