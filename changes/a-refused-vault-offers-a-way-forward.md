### Added

- **A vault refused for a chat's persona offers a way forward.** When a sandboxed chat asks for
  a vault that is not tagged for the persona it was opened as, its tab shows a notice naming the
  vault and the persona, with *Allow {persona} to use this vault* and *Keep blocked*. Allow is
  kept on this machine, recorded, listed in Settings › Sandbox › Granted and revocable there;
  the chat does not restart. purlis tells the chat to run the command again when it is waiting,
  or when its turn ends, and otherwise says to ask it. An administrator's policy can forbid the
  Allow with `"vault-grants": false` (#1430).
- **`purlis vault list` works from a sandboxed chat.** The app answers it with each vault's
  name, provider and persona, and whether this chat may use it. No value and no provider
  session is involved (#1430).

### Changed

- **A refused vault names what to do next.** The refusal a chat reads names the ways forward:
  the operator's Allow on the chat's tab, or a dispatch to the persona the vault is tagged for.
  A vault name the project does not register is refused as that, with nothing to press. A
  persona sub-agent is told at start that in a sandboxed chat it runs with that chat's persona's
  vaults (#1430).
