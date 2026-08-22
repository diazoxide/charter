# cli._split_frame_argv intercepts EVERY argv[0] in _frame_command_names()

_2026-08-22 18:37 · persistent_

cli._split_frame_argv intercepts EVERY argv[0] in _frame_command_names() (every harness cli_name plus 'frame') and grafts everything past a fixed leading run of _OWN_FLAGS onto args.rest BEFORE argparse ever routes a subcommand. This means a subparser nested under charter's 'frame' parser (e.g. fr.add_subparsers() for 'frame action'/'frame menu') is unreachable dead code -- argparse never sees 'action'/'menu' as tokens to dispatch on. Internal frame-only commands (never typed by an operator, only fired by tmux via run-shell) must be registered as TOP-LEVEL siblings of 'frame' instead, exactly like 'panel' already is (charter/cli.py's _add_frame_parsers). Task 10 used 'frame-action'/'frame-menu' (hyphenated, distinct literal tokens _split_frame_argv never matches) for the hotkey-menu commands for this reason.
