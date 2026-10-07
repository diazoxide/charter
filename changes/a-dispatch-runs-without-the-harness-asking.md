### Changed

- **Claude Code no longer asks before `purlis dispatch`.** A chat the app starts runs `purlis
  dispatch`, `purlis dispatch report` and the `dispatch` and `dispatch_report` tools without
  its harness's permission prompt. What consents to a dispatch is purlis's own dispatch
  grant: the app asks you once for a pair of personas, on the asking chat's tab, with the
  brief in front of you, and starts nothing until you allow it. A chat dispatching to its own
  persona needs no grant and asks nothing. Your own `ask` or `deny` rule for any of them still
  wins, and the handoff command still asks (ADR 0064, amended).
