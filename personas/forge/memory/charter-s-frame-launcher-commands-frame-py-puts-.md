# charter's frame launcher (commands_frame.py) puts every workspace on ONE

_2026-09-10 23:37 · persistent_

charter's frame launcher (commands_frame.py) puts every workspace on ONE shared tmux server under a fixed socket name (SOCKET = "charter", commands_frame.py:172) — there is no way to point a real 'charter claude'/'charter frame' launch at an isolated private server for testing. To measure tmux argv behavior safely, call charter.frame.layout.session_argv/chat_window_argv directly with a private -L socket name and a scratch CHARTER_ROOT, swapping the harness binary for a recorder stand-in, rather than driving the real CLI end to end.
