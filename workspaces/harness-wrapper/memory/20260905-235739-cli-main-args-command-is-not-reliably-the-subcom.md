# cli.main: args.command is NOT reliably the subcommand — charter secret e

_2026-09-05 23:57 · persistent_

cli.main: args.command is NOT reliably the subcommand — charter secret exec (and persona secret exec) give their own positional the dest 'command' via _sa_exec, so argparse overwrites the subparser name with the child command LIST. Anything gating on the subcommand in main must read argv[0] (post _bare_launch/_split_*), or it raises TypeError: unhashable type 'list'.
