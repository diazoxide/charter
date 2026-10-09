### Added

- **Add a dispatch grant in Settings, with no dispatch waiting.** Settings › Project › Dispatch
  has **Add a grant**: pick the persona whose chats ask, the persona they may dispatch to, for
  you on this machine or for everyone in this project, and in one workspace or in any. Until
  now a grant was only made from the question a dispatch raises on a chat you are at, so a
  chat nobody is at (its permission prompts off) had nothing to run under until someone
  dispatched from another chat first. Its refusal now says to grant it under Settings ›
  Project › Dispatch (#1465).

### Security

- **What Add a grant can and cannot do.** It asks first and says what the grant reaches,
  including that a chat nobody is at may use it. It is held to every rule an Allow is: both
  names must be personas of the project, a pair an administrator's policy locks and a pair
  you said never to are refused, a grant for everyone is refused for a pair whose project
  grant you said Not on my machine to, no grant is kept for a workspace that is not there,
  and a persona's dispatch to itself needs none. Where the project has workspaces, the form
  starts with none chosen: any workspace is picked on purpose. A grant for everyone writes the project's
  committed file and is followed on this machine as it is written, bound to the project's
  history like any acceptance. Each one is recorded in purlis's event log as yours before it
  counts, and its command is the window's alone: nothing a chat sends can make one (#1465).
