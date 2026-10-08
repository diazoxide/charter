### Added

- **Two ways to say no to a dispatch, and "any persona" as a grant you set yourself.** The
  Notice that asks whether one persona may dispatch to another has a fifth answer, **Never for
  this pair**: it is kept for you on this machine, and from then on no chat of that persona is
  asked or allowed to dispatch to that persona, whatever the project grants, until you lift
  it. **Keep blocked** now holds for that chat's life: the same chat asking again is told at
  once that you said no, and you are not asked a second time; a new chat is asked. A grant can
  also be "any persona", for you on this machine or for everyone in the project
  (`steward = ["*"]` under `[dispatch.grants]`), which covers personas added later. It is set
  only on purpose, never by answering a question. Settings › Project › Dispatch gains the
  list and the buttons for these in #1504; the core and the window commands are here (#1503).

### Security

- **A never is yours alone, and "any persona" is never an answer.** A never for a pair is kept
  in this machine's own record, beats every grant including the project's and "any persona",
  and is lifted only by you. Nothing in the project's file lifts it. No answer on a dispatch's
  Notice writes "any persona", and a chat cannot ask for it: its target must be a persona's
  name. A teammate's "any persona" in the project's file covers nothing on your machine until
  you allow it there in Settings. A record edited by hand grants no more than it names: a `*`
  written where a persona's name goes grants nothing. A grant from one persona to another
  allows nothing the other way. Saying never, lifting it, and granting or revoking "any
  persona" are each recorded in purlis's event log (#1503).
