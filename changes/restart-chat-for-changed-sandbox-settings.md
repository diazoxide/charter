### Added

- **Restart a chat to pick up changed sandbox settings.** A chat keeps the sandbox it started
  with, so a change in Settings › Sandbox used to reach a running chat only if you closed its
  tab and resumed it. A chat's tab menu now has **Restart chat**: its program ends and starts
  again on the same conversation, with the project's settings as they are now. Mid-turn the row
  reads **Restart chat … when this turn ends**, and the chat restarts then; while it restarts,
  its pane says so. A chat that reports no state restarts at once, and its row says that purlis
  cannot tell whether it is mid-turn. After a sandbox setting changes while chats are running, a
  Notice says how many keep the old sandbox until they restart, with **Restart them** and
  **Dismiss**; it appears once for each change, and not at all when no running chat is behind.
  It counts what settings decide (the hosts, what the presets widen, and the folders every chat
  may write), so revoking a folder in Settings › Sandbox › Granted raises it, and a clone or an
  install in the project does not. Settings › Sandbox now says that a change applies to a chat
  from its next start. A chat you started without the sandbox restarts in the sandbox, and its
  tab says so: that choice lasts for one start (#1428).

### Fixed

- **Allowing something for a chat while it restarted hung its project.** Pressing **Allow** on
  a sandbox block's Notice while that chat was already restarting left the restart waiting for
  good: the chat never came back, and closing any chat of that project, allowing anything else
  and the list of chats owed a restart all waited behind it until purlis was quit. What is
  allowed meanwhile is now carried to the chat's new run, as it was meant to be (#1428).
- **Restart now on a persona's hosts Notice works again after a restart that was refused.** The
  button used to stay dead once pressed. A refused restart now also says what to do: send the
  chat a message first if it has only just started, or use Start fresh (#1428).
