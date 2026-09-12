# charter CLI: build_parser() costs ~10ms per call (measured 2026-09-12, p

_2026-09-12 02:22 · persistent_

charter CLI: build_parser() costs ~10ms per call (measured 2026-09-12, py3.14), so cli.main builds it once and hands it to _profile_launch — asking command_words() there would build a second parser in every 'charter hook ...' process, which fires per tool call. And ruling 42's unattended half needs a recorded verdict (state.record_launch): _launch's eager dead-status ask completes 6-14ms after the start while the pane launcher's first line runs at 19-22ms, so there is otherwise nothing left to report from.
