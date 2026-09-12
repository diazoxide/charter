# charter tests gotcha (measured 2026-09-12): mock.patch('charter.commands

_2026-09-12 02:33 · persistent_

charter tests gotcha (measured 2026-09-12): mock.patch('charter.commands_frame.subprocess.run', ...) replaces the attribute on the shared subprocess MODULE, so the fake answers EVERY subprocess in the process — charter.util.run included. Once cmd_new_chat/_open_workspace run the profile ignore check, a tmux fake that answers rc 0 with empty stdout makes util.git_path_state read 'no lines' as TRACKED, and every profile is refused in a fixture meant to describe the opposite. A tmux fake must pass non-tmux argv through to the real subprocess.run.
