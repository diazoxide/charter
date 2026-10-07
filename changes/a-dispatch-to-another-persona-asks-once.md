### Added

- **A dispatch to another persona asks you once.** The dispatch grant is in place: when a chat
  dispatches to a persona other than its own and nothing covers the pair, nothing starts and a
  Notice on the asking chat's tab says who wants to dispatch to whom and shows the first brief
  whole, as the chat wrote it. You choose **Allow for this chat**, **Allow for me on this
  machine**, **Allow for everyone in this project** or **Keep blocked**. After an Allow that
  dispatch starts, and so does every later one the grant covers, with no prompt. The persona
  chat starts with its own persona's hosts and vault, with nothing to allow on its tab. A chat
  dispatching to its own persona needs no grant. Settings › Project › Dispatch lists every
  dispatch grant with its level, who granted it and when, and **Revoke**; the next dispatch
  across a revoked pair asks again, and a chat already running is left alone. A grant for
  everyone is kept in the project's committed file under `[dispatch.grants]`, and each teammate
  is told once when that list changes. Every Allow and Revoke is recorded in purlis's event log
  (#1437).

### Security

- **Only you make a dispatch grant.** Nothing a chat sends creates, widens or revokes one: the
  asking persona is purlis's own record of the chat, never the request's, the brief is shown as
  plain text apart from purlis's words, and a write purlis makes for a chat never changes
  `[dispatch]` in the project's file. An administrator's policy can lock a pair, or all
  dispatch, in `/etc/purlis/policy.json` (`dispatch.locked`, `dispatch.allow`): a locked
  dispatch is refused with the policy's sentence and who set it, and offers no Allow (#1437).
