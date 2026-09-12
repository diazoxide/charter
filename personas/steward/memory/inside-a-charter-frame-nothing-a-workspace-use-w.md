# Inside a charter frame nothing a 'workspace use' writes in one chat reac

_2026-09-10 21:36 · persistent_

Inside a charter frame nothing a 'workspace use' writes in one chat reaches a NEW chat: a chat ends when its harness exits, a closed chat's .charter/sessions/<fid>.* is reaped (frame/state._forget_session), and a reopen goes through cmd_launch, which reaps then allocates. A new chat resolves by its own launch record (workspace.for_frame outranks the terminal pointer and the declared default). So a scope note promising 'kept across closing/reopening Claude' or 'a new session starts at default' is false in a chat (#936 review round 3). Also: both launch paths call commands_frame._pin_workspace BEFORE state.record_workspace, so nothing can claim the pin line names an existing record.
