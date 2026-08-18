"""Argument parsing and dispatch for the ``charter`` CLI."""

from __future__ import annotations

import argparse
import os
import sys

from . import (
    __version__,
    commands,
    commands_harness,
    commands_persona,
    commands_report,
    commands_secrets,
    commands_workspace,
    commands_worktree,
    hooks,
    statusline,
    toolgate,
    util,
)
from .browser import PINNED as _PLAYWRIGHT_PIN
from .forge.registry import KINDS as _FORGE_KINDS
from .secrets.registry import PROVIDERS


class _NoAbbrev(argparse.ArgumentParser):
    """An ArgumentParser that refuses prefix abbreviations, for the whole command tree.

    argparse expands any unambiguous prefix by default, so `charter secret get X Y --rev`
    ran as `--reveal` — and the PreToolUse leak guard, which looks for the flag the user
    would have to type, saw nothing to deny. A guard that a three-character abbreviation
    walks past is not a guard.

    Applied by making the ROOT parser this class: `add_subparsers` defaults its
    `parser_class` to `type(self)`, so every subcommand and sub-subcommand inherits it
    without each one having to remember. Setting `allow_abbrev=False` on the root alone
    would have covered only the top level, which is the level with nothing to protect.
    """

    def __init__(self, *a, **kw):
        kw.setdefault("allow_abbrev", False)
        super().__init__(*a, **kw)


def build_parser() -> argparse.ArgumentParser:
    p = _NoAbbrev(
        prog="charter",
        description="charter — discover, clone, and track org repos on demand.",
    )
    p.add_argument("--version", action="version", version=f"charter {__version__}")
    sub = p.add_subparsers(dest="command", required=True)

    ini = sub.add_parser(
        "init",
        help="Scaffold a fresh control plane here: charter.toml, baseline dirs, "
             ".gitignore, and a status line. Additive + idempotent — never touches "
             "existing content. The first command a stranger runs.",
    )
    ini.add_argument("--forge", choices=sorted(_FORGE_KINDS), default="gitlab",
                     help="Forge this control plane tracks (default: gitlab).")
    ini.add_argument("--owner", help="Group/org/user that owns the repos "
                                     "(GitLab group or GitHub org/user).")
    ini.add_argument("--host", help="Self-hosted forge host (default: the forge's own public host).")
    # The acceptance half of init's one offer. charter has no interactive prompt (it runs
    # inside hooks, where blocking on stdin hangs the turn), so the offer is a printed
    # command and this flag is the command — nothing is ever cloned unasked.
    ini.add_argument("--clone-this-repo", action="store_true",
                     help="Also clone the git repo you are standing in into the first "
                          "workspace. This is how you accept the offer `charter init` "
                          "prints when it finds one; without it, init clones nothing.")
    # The front door is a FILE, not a feature: init writes one generic persona and
    # declares it in charter.toml. The name lives here, in a flag with a default — never
    # in the engine, which knows only that a plane may declare a default persona.
    ini.add_argument("--front-door", metavar="NAME", default="steward",
                     help="Name of the generic front-door persona to scaffold and declare "
                          "(default: steward). Skipped if this plane already has personas.")
    ini.add_argument("--no-front-door", dest="front_door", action="store_const", const=None,
                     help="Scaffold no persona at all; the plane declares no front door.")
    ini.set_defaults(func=commands.cmd_init)

    doc_check = sub.add_parser(
        "doctor",
        help="Preflight: check python/git/glab/auth/ssh/inventory before working.",
    )
    doc_check.add_argument("--json", action="store_true", help="Emit machine-readable results.")
    doc_check.set_defaults(func=commands.cmd_doctor)

    ri = sub.add_parser(
        "reinit",
        help="Heal control-plane drift: create any missing baseline directory "
             "(personas/, inventory/, workspaces/) a newer charter expects. "
             "Idempotent + additive — existing content is never touched.",
    )
    ri.set_defaults(func=commands.cmd_reinit)

    d = sub.add_parser(
        "discover",
        help="Refresh inventory/repos.json from GitLab, then regenerate docs.",
    )
    d.add_argument("--no-probe", action="store_true", help="Skip per-repo stack detection (faster).")
    d.add_argument("--no-docs", action="store_true", help="Do not regenerate docs afterward.")
    d.set_defaults(func=commands.cmd_discover)

    c = sub.add_parser("clone", help="Clone repos on demand into the active workspace.")
    c.add_argument("repos", nargs="*", help="Repo name(s) or full path(s) from the inventory.")
    c.add_argument("--workspace", "-w", help="Target workspace (default: the active one).")
    c.set_defaults(func=commands.cmd_clone)

    s = sub.add_parser("sync", help="Fetch + fast-forward cloned repos in a workspace (skips dirty).")
    s.add_argument("--workspace", "-w", help="Workspace to sync (default: the active one).")
    s.add_argument("--all", action="store_true", help="Sync every workspace.")
    s.set_defaults(func=commands.cmd_sync)

    st = sub.add_parser("status", help="Show workspaces and the cloned repos in the active one.")
    st.add_argument("--workspace", "-w", help="Workspace to detail (default: the active one).")
    st.add_argument("--all", action="store_true", help="Detail every workspace.")
    st.set_defaults(func=commands.cmd_status)

    br = sub.add_parser("browser",
                        help="The browser lane: charter ships the credential bridge, "
                             "Playwright ships the page-driving surface.")
    brsub = br.add_subparsers(dest="browser_cmd", required=True)
    bri = brsub.add_parser("install",
                           help="Generate Playwright's driving-surface skill into this "
                                "plane, from the tool that owns it (charter vendors none "
                                "of it — Apache-2.0, and it ships far more often than "
                                "charter does).")
    bri.add_argument("--version", help=f"@playwright/cli version (default: {_PLAYWRIGHT_PIN}).")
    bri.set_defaults(func=commands.cmd_browser_install)

    doc = sub.add_parser("docs",
                         help="Regenerate this plane's docs/topology.md — or read "
                              "charter's own documentation (`show`, `list`).")
    dsub = doc.add_subparsers(dest="docs_cmd")
    # Bare `charter docs` still generates. It did that long before it grew subcommands,
    # and Makefiles in the wild call it that way, so making the group require a
    # subcommand would break callers that never saw the change.
    doc.set_defaults(func=commands.cmd_docs)
    dgen = dsub.add_parser("generate", help="Regenerate docs/topology.md from the inventory.")
    dgen.set_defaults(func=commands.cmd_docs)
    dls = dsub.add_parser("list", help="List charter's own documentation topics.")
    dls.set_defaults(func=commands.cmd_docs_list)
    dsh = dsub.add_parser("show",
                          help="Print one of charter's own documentation pages — served "
                               "by the install that implements it, so it cannot be a "
                               "version behind the CLI reading it.")
    dsh.add_argument("topic", help="e.g. secrets, personas, git-policy (see `docs list`).")
    dsh.set_defaults(func=commands.cmd_docs_show)

    sv = sub.add_parser("save",
                        help="Commit + push the control plane's own changes via glab (HTTPS token — no SSH pain).")
    sv.add_argument("message", nargs="?", help="Commit message (default: 'charter save: N file(s)').")
    sv.add_argument("--sign", action="store_true", help="Sign the commit (default: unsigned, to avoid signer hangs).")
    sv.add_argument("--no-push", action="store_true", help="Commit only; don't push.")
    sv.set_defaults(func=commands.cmd_save)

    rcl = sub.add_parser("recall",
                         help="The one memory gate: search/list across ALL bases (active workspace + "
                              "active persona own + shared), each hit labeled by source.")
    rcl.add_argument("query", nargs="?", help="Keyword query; omit to list recent memories across bases.")
    rcl.add_argument("--scope", help="Comma list of scopes to search "
                                     "(workspace,persona,shared,refs,ephemeral); default "
                                     "workspace,persona,shared,refs. `refs` is the curated "
                                     "docs a persona collects — committed and shared, so it "
                                     "is searched by default; `ephemeral` is scratch and is "
                                     "not.")
    rcl.add_argument("--ephemeral", action="store_true", help="Also include the persona's session scratch.")
    rcl.add_argument("--full", action="store_true",
                     help="Also print a line of each memory's body (the path is always shown).")
    rcl.add_argument("--persona", help="Search this persona instead of the active one.")
    # One workspace or every workspace — never both. `-w beta --all-workspaces` has no
    # coherent reading, and argparse refusing it beats silently honouring one of them.
    _ws = rcl.add_mutually_exclusive_group()
    _ws.add_argument("--workspace", "-w", help="Search this workspace instead of the active one.")
    _ws.add_argument("--all-workspaces", action="store_true",
                     help="Search EVERY workspace's journal (persona + shared appear once). "
                          "For 'it was two weeks ago and I forget which task'.")
    rcl.add_argument("--since", metavar="WHEN",
                     help="Only memories recorded on/after this: an age (14d, 2w, 3m) or a "
                          "date (2026-07-01). Undated memories are excluded and counted.")
    rcl.add_argument("--limit", type=int, default=8, help="Max results (0 = no cap).")
    rcl.set_defaults(func=commands.cmd_recall)

    gp = sub.add_parser("git-policy",
                        help="Golden rule: one credential — check/apply token-only git auth "
                             "(glab HTTPS, no SSH, no signing) on the control plane + every clone.")
    gp.add_argument("--apply", action="store_true", help="Write the policy (default: report drift).")
    gp.set_defaults(func=commands.cmd_git_policy)

    sl = sub.add_parser("statusline",
                        help="Render the plane's status line — from a JSON payload on "
                             "stdin, or ambiently with --watch on a harness that has no "
                             "status bar of its own.")
    sl.add_argument("--watch", action="store_true",
                    help="Repaint in place until Ctrl-C, in any spare terminal. Needs no "
                         "status-bar socket and no multiplexer, so it is the same render "
                         "on every harness.")
    sl.add_argument("--interval", type=float, default=statusline.WATCH_INTERVAL,
                    help="Seconds between repaints with --watch.")
    sl.set_defaults(func=lambda args: statusline.main(
        (["--watch", "--interval", str(args.interval)] if args.watch else [])))

    gl = sub.add_parser("gl-refresh",
                        help="Refresh the status line's forge state (open MRs/PRs + CI) "
                             "from each clone's own forge.")
    gl.add_argument("--workspace", "-w", help="Workspace to refresh (default: the active one).")
    gl.add_argument("--detach", action="store_true",
                    help="Return at once and refresh in a process that outlives this one. "
                         "What a hook's `async` used to buy, done by charter — one harness "
                         "skips async hooks outright.")
    gl.set_defaults(func=commands.cmd_gl_refresh)

    # Internal: the detached child `statusline` spawns to refresh the update cache.
    # Hidden from help — nobody needs to run it, and it is not part of the UX.
    vc = sub.add_parser("_version-check")
    vc.set_defaults(func=commands.cmd_version_check)

    ver = sub.add_parser("version",
                         help="The control plane's charter version lock: show drift, "
                              "conform this machine to it, or move the pin.")
    vsub = ver.add_subparsers(dest="version_cmd")
    ver.set_defaults(func=commands.cmd_version)     # bare `charter version` = show

    gd = sub.add_parser("guard",
                        help="Force-prompt rules for this plane. Written as Claude Code "
                             "`permissions.ask` rules in .claude/settings.json — charter "
                             "keeps no list of its own (ADR 0014).")
    gsub = gd.add_subparsers(dest="guard_cmd")
    gd.set_defaults(func=commands.cmd_guard_list)
    ga = gsub.add_parser("ask", help="Always prompt before this command runs.")
    ga.add_argument("pattern", help="e.g. 'terraform apply *' — wrapped as Bash(...) "
                                    "unless it already names a tool.")
    ga.set_defaults(func=commands.cmd_guard_ask)
    gl = gsub.add_parser("list", help="Show this plane's force-prompt rules.")
    gl.set_defaults(func=commands.cmd_guard_list)

    vsy = vsub.add_parser("sync",
                          help="Move THIS plane to the version it pins. A plugin is "
                               "installed per project, so no other plane moves.")
    vsy.add_argument("--cli", action="store_true",
                     help="Conform the machine-global `charter` binary instead. Shared by "
                          "every plane on this machine, so it can put others into drift.")
    vsy.set_defaults(func=commands.cmd_version_sync)

    vbp = vsub.add_parser("bump",
                          help="Move the pin: install + verify the target, then write "
                               "charter.toml. Affects every teammate, so in that order.")
    vbp.add_argument("--to", help="Version to pin (default: the latest published).")
    vbp.add_argument("--push", action="store_true",
                     help="Also commit + push the lock, so teammates conform on their "
                          "next session.")
    vbp.set_defaults(func=commands.cmd_version_bump)

    hk = sub.add_parser("hook",
                        help="Dispatch a Claude Code hook by name — what the plugin's "
                             "hooks/hooks.json actually invokes (the plugin ships no Python).")
    hk.add_argument("name", choices=sorted(hooks._HANDLERS), help="Which handler to run.")
    hk.add_argument("--plugin-version", dest="plugin_version",
                    help="The installed plugin's version (hooks/hooks.json bakes this in); "
                         "compared against this CLI's own version to catch skew.")
    hk.set_defaults(func=lambda args: hooks.dispatch(args.name, args.plugin_version))

    tr = sub.add_parser("trace",
                        help="Session observability: guard denials, tool approvals, secret warnings, memory writes.")
    tr.add_argument("--session", help="A specific session id (default: the current one).")
    tr.add_argument("--summary", action="store_true", help="Aggregate counts instead of raw events.")
    tr.add_argument("-n", type=int, default=0, help="Show only the last N raw events.")
    tr.set_defaults(func=commands_persona.cmd_trace)

    _add_harness_parser(sub)
    _add_workspace_parser(sub)
    _add_worktree_parser(sub)
    _add_vault_parser(sub)
    _add_secret_parser(sub)
    _add_persona_parser(sub)
    _add_report_parser(sub)

    return p


def _add_report_parser(sub) -> None:
    r = sub.add_parser("report",
                       help="Report a charter bug or missing capability upstream (drafts "
                            "locally; nothing is published without a second command).")
    rsub = r.add_subparsers(dest="report_cmd", required=True)

    bug = rsub.add_parser("bug", help="Charter did something wrong.")
    # Optional, and joined by --from-file/--stdin: a body worth filing carries backticks,
    # `$` and fenced code, none of which survives being a shell argument. Same two flags,
    # same spelling, as `secret set` — there to keep a value off argv, here to keep one
    # intact.
    bug.add_argument("text", nargs="?", help="What went wrong, in your own words. Use `-` to read it from stdin.")
    bug.add_argument("--from-file", help="Read the body verbatim from a file.")
    bug.add_argument("--stdin", action="store_true", help="Read the body from stdin.")
    bug.set_defaults(func=commands_report.cmd_report_bug)

    gap = rsub.add_parser("gap", help="Charter cannot do something it should.")
    # Optional, and joined by --from-file/--stdin: a body worth filing carries backticks,
    # `$` and fenced code, none of which survives being a shell argument. Same two flags,
    # same spelling, as `secret set` — there to keep a value off argv, here to keep one
    # intact.
    gap.add_argument("text", nargs="?", help="What is missing, in your own words. Use `-` to read it from stdin.")
    gap.add_argument("--from-file", help="Read the body verbatim from a file.")
    gap.add_argument("--stdin", action="store_true", help="Read the body from stdin.")
    gap.set_defaults(func=commands_report.cmd_report_gap)

    ls = rsub.add_parser("list", help="Reports drafted on this machine, and what was sent.")
    ls.set_defaults(func=commands_report.cmd_report_list)

    sh = rsub.add_parser("show", help="Print one report exactly as it would be published.")
    sh.add_argument("id")
    sh.set_defaults(func=commands_report.cmd_report_show)

    # The undo for drafting. Drafting is cheap and local on purpose, which only works if
    # discarding is too — otherwise a redrafted report leaves its superseded twin in `list`
    # forever and the "not sent" column stops meaning anything.
    dl = rsub.add_parser("delete", aliases=["discard"],
                         help="Discard a drafted report (a sent one needs --force).")
    dl.add_argument("id")
    dl.add_argument("--force", action="store_true",
                    help="Discard even one already sent — that also drops the pointer a "
                         "later identical crash would have reused.")
    dl.set_defaults(func=commands_report.cmd_report_delete)

    # Its own command, not a flag on `send`: a flag an agent can pass is a flag it will
    # pass every time, where a one-off command has a single auditable purpose (ADR 0003).
    cs = rsub.add_parser("consent",
                         help="Agree, once, that reports may be published under your own "
                              "GitHub identity. Nothing sends until you do.")
    cs.set_defaults(func=commands_report.cmd_report_consent)

    sd = rsub.add_parser("send",
                         help="Publish a drafted report. THIS is the approval step — the "
                              "only reporting command that touches the network.")
    sd.add_argument("id")
    sd.add_argument("--new", action="store_true",
                    help="File even though charter found a possible duplicate.")
    sd.add_argument("--dry-run", action="store_true",
                    help="Show exactly what would be published, and send nothing.")
    sd.set_defaults(func=commands_report.cmd_report_send)

    cm = rsub.add_parser("comment",
                         help="Add your details to an existing upstream issue — the right "
                              "answer to a duplicate, since it keeps your reproduction.")
    cm.add_argument("id")
    cm.add_argument("--on", required=True, help="Upstream issue number to comment on.")
    cm.set_defaults(func=commands_report.cmd_report_comment)


def _add_workspace_parser(sub) -> None:
    w = sub.add_parser("workspace", aliases=["ws"],
                       help="Manage isolated per-task workspaces (workspaces/<workspace>/<repo>).")
    wsub = w.add_subparsers(dest="workspace_cmd", required=True)

    lst = wsub.add_parser("list", help="List workspaces and their clones; mark the active one.")
    lst.set_defaults(func=commands_workspace.cmd_workspace_list)

    cur = wsub.add_parser("current", help="Print the active workspace and how it was resolved.")
    cur.set_defaults(func=commands_workspace.cmd_workspace_current)

    rec = wsub.add_parser("_reconcile")  # internal: SessionStart hook
    rec.set_defaults(func=commands_workspace.cmd_workspace_reconcile)

    cr = wsub.add_parser("create", help="Create a workspace (optionally select it and clone repos).")
    cr.add_argument("name")
    cr.add_argument("--use", action="store_true", help="Make it the active workspace (locks the session to it).")
    cr.add_argument("--force", action="store_true",
                    help="With --use: switch even if the session is already locked to another workspace.")
    cr.add_argument("--live", action="store_true",
                    help="Make it LIVE (shareable — manifest + memory committed/synced/auto-saved). "
                         "Default is LOCAL (private, nothing committed).")
    cr.add_argument("--vision", "--about", dest="vision",
                    help="The goal/idea for this workspace — seeds its living charter "
                         "(workspaces/<name>/workspace.md). Ask the developer if you don't know it.")
    cr.add_argument("repos", nargs="*", help="Repos to clone into it immediately.")
    cr.set_defaults(func=commands_workspace.cmd_workspace_create)

    lv = wsub.add_parser("live",
                         help="Make a workspace LIVE (shareable) — or LOCAL with --off (private).")
    lv.add_argument("name")
    lv.add_argument("--off", action="store_true", help="Make it LOCAL (private) instead — stop committing it.")
    lv.set_defaults(func=commands_workspace.cmd_workspace_live)

    use = wsub.add_parser("use",
                          help="Set + lock the active workspace for this session (no mid-session switch).")
    use.add_argument("name")
    use.add_argument("--force", action="store_true",
                     help="Override the session lock and switch to another workspace mid-session.")
    use.add_argument("--create", action="store_true",
                     help="Create the workspace if it does not exist. Without this an "
                          "unknown name is an error with a did-you-mean — a typo used to "
                          "be created AND session-locked, so the correction was refused.")
    use.set_defaults(func=commands_workspace.cmd_workspace_use)

    unl = wsub.add_parser("unlock",
                          help="Release this session's workspace lock so another can be selected.")
    unl.set_defaults(func=commands_workspace.cmd_workspace_unlock)

    rm = wsub.add_parser("remove", help="Delete a workspace and its clones (guards unpushed work).")
    rm.add_argument("name")
    rm.add_argument("--force", action="store_true", help="Remove even with uncommitted/unpushed work.")
    rm.set_defaults(func=commands_workspace.cmd_workspace_remove)

    rn = wsub.add_parser("rename", aliases=["mv"],
                         help="Rename a workspace (moves its clones + memory; commits the move if LIVE).")
    rn.add_argument("old", help="Current workspace name.")
    rn.add_argument("new", help="New workspace name.")
    rn.add_argument("-m", "--message", help="Commit message for a LIVE workspace's rename.")
    rn.set_defaults(func=commands_workspace.cmd_workspace_rename)

    fk = wsub.add_parser("fork", aliases=["duplicate"],
                         help="Fork a workspace: copy its charter + manifest + memo so you can "
                              "branch off with full context (repos via --restore or on demand).")
    fk.add_argument("src", help="Source workspace to fork from.")
    fk.add_argument("new", help="Name for the fork.")
    fk.add_argument("--restore", action="store_true", help="Also clone the inherited repos now.")
    fk.add_argument("--live", action="store_true", help="Make the fork LIVE (default LOCAL).")
    fk.set_defaults(func=commands_workspace.cmd_workspace_fork)

    ri = wsub.add_parser("reinit",
                         help="Upgrade a workspace's structure to the current layout "
                              "(create missing workspace.md/memory/refs; stamp the version).")
    ri.add_argument("name", nargs="?", help="Workspace to reinit (default: the active one).")
    ri.add_argument("--all", action="store_true", help="Reinit every workspace (after a charter upgrade).")
    ri.set_defaults(func=commands_workspace.cmd_workspace_reinit)

    dflt = wsub.add_parser("default",
                           help="Nominate the workspace a session lands on when nothing "
                                "else decided (committed; mirrors `persona default`).")
    dflt.add_argument("name", nargs="?", help="Workspace to nominate; omit to show it.")
    dflt.add_argument("--clear", action="store_true", help="Remove the declared default.")
    dflt.set_defaults(func=commands_workspace.cmd_workspace_default)

    opt = wsub.add_parser("optimize",
                          help="Curate a workspace's memory: collapse exact duplicates and "
                               "repair the index with --apply; propose the rest.")
    opt.add_argument("name", nargs="?", help="Workspace to optimize (default: every one).")
    opt.add_argument("--all", action="store_true", help="Every workspace (the default).")
    opt.add_argument("--apply", action="store_true",
                     help="Apply the safe, reversible ops. Proposals always stay manual.")
    opt.add_argument("--stale-days", type=int, default=90, dest="stale_days",
                     help="Age at which a memory is proposed for review (default: 90).")
    opt.set_defaults(func=commands_workspace.cmd_workspace_optimize)

    rem = wsub.add_parser("remember",
                          help="Record one workspace memory (its own file, indexed) — the task journal.")
    rem.add_argument("text", nargs="?", help="Memory text; omit to list the workspace's memories.")
    rem.add_argument("--title", help="Optional title (else derived from the first line).")
    rem.add_argument("--workspace", "-w", help="Target workspace (default: the active one).")
    rem.add_argument("--no-sync", action="store_true", dest="no_sync",
                     help="Don't reactively commit+push it now (LIVE workspaces; sync later).")
    rem.set_defaults(func=commands_workspace.cmd_workspace_remember)

    rc = wsub.add_parser("recall", help="Search the workspace's memories (--query) or list them all.")
    rc.add_argument("--query", "-q", help="Keyword query; omit to list every memory chronologically.")
    rc.add_argument("--workspace", "-w", help="Target workspace (default: the active one).")
    rc.set_defaults(func=commands_workspace.cmd_workspace_recall)

    fg = wsub.add_parser("forget", help="Delete one workspace memory by slug or filename.")
    fg.add_argument("slug", help="Memory slug or filename (see `charter workspace recall`).")
    fg.add_argument("--workspace", "-w", help="Target workspace (default: the active one).")
    fg.set_defaults(func=commands_workspace.cmd_workspace_forget)

    td = wsub.add_parser("todo",
                         help="Record what this task still means to do — or list them. "
                              "Intent, kept apart from memory (what it learned) and the "
                              "journal (what happened).")
    # Text optional, exactly like `remember`: the bare verb lists. A `list` SUBCOMMAND
    # would be indistinguishable from recording a todo whose text is "list".
    #
    # `done`/`forget` are read as verbs rather than as todo text, which the same argument
    # would seem to forbid — except that each takes a SLUG after it, and recording never
    # has a second positional. So the two are told apart by the shape of the call, not by
    # the word: `todo "forget the labels"` is one argument and records; `todo forget
    # 20260101-120000-labels` is two and closes. Real subparsers can't do this — argparse
    # matches the leading positional against `text` before any subcommand gets a look.
    td.add_argument("text", nargs="?",
                    help="The todo — or `done`/`forget` with a slug after it; omit to list.")
    td.add_argument("slug", nargs="?",
                    help="With `done` (finished — journalled) or `forget` (abandoned — "
                         "silent): which todo to close. Closing deletes it; the journal is "
                         "already the record of what happened.")
    td.add_argument("--workspace", "-w", help="Target workspace (default: the active one).")
    td.add_argument("--query", "-q", help="Search the todos instead of listing them all.")
    td.set_defaults(func=commands_workspace.cmd_workspace_todo)

    nt = wsub.add_parser("note", help="Alias for `remember` — record a workspace memory (or list them).")
    nt.add_argument("message", nargs="?", help="Memory text; omit to list the workspace's memories.")
    nt.add_argument("--workspace", "-w", help="Target workspace (default: the active one).")
    nt.add_argument("--no-sync", action="store_true", dest="no_sync",
                    help="Don't reactively commit+push it now (LIVE workspaces; sync later).")
    nt.set_defaults(func=commands_workspace.cmd_workspace_note)

    vs = wsub.add_parser("vision",
                         help="Show or set the workspace's Vision (the north star in workspace.md).")
    vs.add_argument("text", nargs="?", help="Vision text to set; omit to show it.")
    vs.add_argument("--workspace", "-w", help="Target workspace (default: the active one).")
    vs.set_defaults(func=commands_workspace.cmd_workspace_vision)

    snp = wsub.add_parser("snapshot",
                          help="Capture repos+branches into the committed manifest (workspace.json).")
    snp.add_argument("name", nargs="?", help="Workspace (default: the active one).")
    snp.add_argument("--description", help="Set/update the workspace description.")
    snp.add_argument("--force", action="store_true",
                     help="Snapshot even if a repo has uncommitted/unpushed work.")
    snp.set_defaults(func=commands_workspace.cmd_workspace_snapshot)

    rst = wsub.add_parser("restore",
                          help="Rebuild a workspace from its manifest — clone repos + checkout branches.")
    rst.add_argument("name")
    rst.add_argument("--on-demand", dest="on_demand", action="store_true",
                     help="Don't clone now; clone each repo when you enter it.")
    rst.set_defaults(func=commands_workspace.cmd_workspace_restore)

    syn = wsub.add_parser("sync",
                          help="Pull the control plane for fresh workspace manifests + memory (before working).")
    syn.set_defaults(func=commands_workspace.cmd_workspace_sync)

    sav = wsub.add_parser("save",
                          help="Commit + push this workspace's manifest + memory (secret-scanned, via glab).")
    sav.add_argument("name", nargs="?", help="Workspace (default: the active one).")
    sav.add_argument("--message", "-m", help="Commit message.")
    sav.set_defaults(func=commands_workspace.cmd_workspace_save)

    aus = wsub.add_parser("_autosave")  # internal: Stop hook — debounced auto-save
    aus.set_defaults(func=commands_workspace.cmd_workspace_autosave)
    pbg = wsub.add_parser("_pushbg")    # internal: background push half of autosave
    pbg.set_defaults(func=commands_workspace.cmd_workspace_pushbg)


def _add_harness_parser(sub) -> None:
    h = sub.add_parser("harness",
                       help="Agent runtimes charter can run inside (Claude Code, opencode, "
                            "Codex) — list them, or arm the one `init` will not.")
    hsub = h.add_subparsers(dest="harness_cmd", required=True)

    lst = hsub.add_parser("list", help="Every registered harness, its ceilings, and which "
                                       "one this session is in.")
    lst.set_defaults(func=commands_harness.cmd_harness_list)

    ins = hsub.add_parser("install", help="Arm a harness whose wiring lives outside the "
                                          "plane (Codex). Running it IS the consent.")
    ins.add_argument("name", help="Harness name, e.g. codex.")
    ins.set_defaults(func=commands_harness.cmd_harness_install)


def _add_worktree_parser(sub) -> None:
    w = sub.add_parser("worktree", aliases=["wt"],
                       help="Worktrees of a workspace's clone — parallel pieces of one task "
                            "(workspaces/<ws>/.worktrees/<repo>/<piece>).")
    wsub = w.add_subparsers(dest="worktree_cmd", required=True)

    add = wsub.add_parser("add", help="Create a worktree on a new branch off the clone's HEAD.")
    add.add_argument("repo")
    add.add_argument("piece", help="Name of the piece; also the new branch name.")
    add.add_argument("--branch", help="Check out this EXISTING branch instead of creating one.")
    add.add_argument("--workspace", "-w", help="Target workspace (default: the active one).")
    add.set_defaults(func=commands_worktree.cmd_worktree_add)

    lst = wsub.add_parser("list", help="List the worktrees of one clone, or of every clone.")
    lst.add_argument("repo", nargs="?", help="Only this repo (default: all clones).")
    lst.add_argument("--workspace", "-w", help="Target workspace (default: the active one).")
    lst.set_defaults(func=commands_worktree.cmd_worktree_list)

    # `done` and `abandon` take no piece argument on purpose: the piece is where you are
    # standing, and an argument is an opportunity to declare someone else's finished.
    done = wsub.add_parser("done", help="Declare the piece you are standing in finished.")
    done.set_defaults(func=commands_worktree.cmd_worktree_done)

    ab = wsub.add_parser("abandon", help="Declare the piece you are standing in given up.")
    ab.add_argument("reason", help="Why you stopped — what whoever picks this up reads first.")
    ab.set_defaults(func=commands_worktree.cmd_worktree_abandon)

    # Its own command, not `list --history`: `list` answers what is running here (from git),
    # this answers what happened here (from the record), and ADR 0010 is about not letting
    # one name cover both.
    hist = wsub.add_parser("history", help="What happened to this workspace's pieces, "
                                           "including ones whose worktree is gone.")
    hist.add_argument("repo", nargs="?", help="Only this repo (default: all).")
    hist.add_argument("piece", nargs="?", help="Only this piece (default: all).")
    hist.add_argument("--workspace", "-w", help="Target workspace (default: the active one).")
    hist.set_defaults(func=commands_worktree.cmd_worktree_history)

    rm = wsub.add_parser("remove", help="Remove a worktree (refuses to lose uncommitted "
                                        "or unpushed work).")
    rm.add_argument("repo")
    rm.add_argument("piece")
    rm.add_argument("--force", action="store_true",
                    help="Remove even with uncommitted or unpushed work (DISCARDS it).")
    rm.add_argument("--delete-branch", action="store_true", dest="delete_branch",
                    help="Also delete the piece's branch.")
    rm.add_argument("--workspace", "-w", help="Target workspace (default: the active one).")
    rm.set_defaults(func=commands_worktree.cmd_worktree_remove)


def _add_vault_parser(sub) -> None:
    v = sub.add_parser("vault", help="Manage secret vaults (provider + config + persona).")
    vsub = v.add_subparsers(dest="vault_cmd", required=True)

    add = vsub.add_parser("add", help="Register a vault.")
    add.add_argument("name")
    add.add_argument("--provider", default="plain-file", choices=sorted(PROVIDERS),
                     help="Vault backend (default: plain-file).")
    add.add_argument("--file", help="File path for plain-file/reference vaults "
                                    "(default: .charter/vaults/<name>.json).")
    add.add_argument("--op-vault", metavar="NAME",
                     help="1Password vault charter keeps its item in (provider: 1password).")
    add.add_argument("--op-item",
                     help="1Password item whose fields are this vault's secrets "
                          "(provider: 1password). Default: charter-<vault>. Point it at "
                          "an item you already curate to adopt it as-is.")
    add.add_argument("--account", help="1Password account to pin to (provider: 1password); "
                                       "needed when signed into more than one.")
    add.add_argument("--persona", help="Tag this vault for a persona (e.g. devops, qa).")
    add.add_argument("--env", action="append", metavar="TARGET=SOURCE", default=[],
                     help="Bind the identity this vault is read through: TARGET is the "
                          "variable the CLI reads, SOURCE the one this machine carries it "
                          "in (e.g. OP_SERVICE_ACCOUNT_TOKEN=OP_ACME_DEVOPS_TOKEN). Only "
                          "NAMES are stored, never values. Repeatable.")
    add.add_argument("--token-env", metavar="SOURCE",
                     help="Shorthand for --env OP_SERVICE_ACCOUNT_TOKEN=SOURCE, the "
                          "1Password service-account case.")
    add.add_argument("--share", action="store_true",
                     help="Record it in the COMMITTED registry (vaults.json at the plane "
                          "root) so teammates inherit the wiring. Default is local-only: a "
                          "registry names which personas hold credentials and where their "
                          "files are, so it is never published by accident. The 1Password "
                          "--account pin always stays local.")
    add.add_argument("--force", action="store_true",
                     help="Replace an existing registration of this name. Does NOT migrate "
                          "its secrets — the old vault's file is left where it is, with "
                          "nothing pointing at it.")
    add.set_defaults(func=commands_secrets.cmd_vault_add)

    lst = vsub.add_parser("list", help="List configured vaults (names/status only, never values).")
    lst.set_defaults(func=commands_secrets.cmd_vault_list)

    vfy = vsub.add_parser("verify",
                          help="Resolve every reference for real and report what does NOT "
                               "resolve. `list` and `doctor` only check the vault is "
                               "reachable — a reference can be registered and still be dead.")
    vfy.add_argument("name", nargs="?", help="One vault (default: all of them).")
    vfy.set_defaults(func=commands_secrets.cmd_vault_verify)

    rm = vsub.add_parser("remove", help="Unregister a vault (leaves its file on disk).")
    rm.add_argument("name")
    rm.set_defaults(func=commands_secrets.cmd_vault_remove)


# Secret-operation arguments (shared by `charter secret` and `charter persona secret`).
def _sa_set(p):
    p.add_argument("key")
    p.add_argument("--stdin", action="store_true", help="Read the value from stdin.")
    p.add_argument("--from-file", help="Read the value verbatim from a file.")
    p.add_argument("--value", help="Inline value (discouraged: visible in shell history).")
    p.add_argument("--allow-empty", action="store_true", dest="allow_empty",
                   help="Permit storing an empty value. Refused by default: an empty "
                        "secret reads as present and healthy everywhere charter looks, so "
                        "the mistake only surfaces later as a 401.")


def _sa_get(p):
    p.add_argument("key")
    p.add_argument("--reveal", action="store_true", help="Print plaintext (interactive terminals only).")
    p.add_argument("--force", action="store_true", help="Allow --reveal to a non-interactive stdout.")


def _sa_key(p):
    p.add_argument("key")


def _sa_cp(p):
    p.add_argument("key")
    p.add_argument("dest")


def _sa_exec(p):
    p.add_argument("--env", action="append", metavar="NAME=key",
                   help="Inject secret <key> as env var NAME (repeatable).")
    p.add_argument("--file", action="append", metavar="ENVVAR=key",
                   help="Write secret <key> to a temp 0600 file; set ENVVAR to its path (repeatable).")
    p.add_argument("--dotenv", action="append", metavar="ENVVAR=NAME:key",
                   help="Add secret <key> as NAME to a temp 0600 dotenv file; set "
                        "ENVVAR to its path (repeatable — repeats sharing an "
                        "ENVVAR merge into one file). For tools that read a "
                        "dotenv secrets file, e.g. PLAYWRIGHT_MCP_SECRETS_FILE.")
    p.add_argument("--stream", dest="stream_mode", action="store_true",
                   help="Run the command as a CHILD with stdio inherited, wait for it, then "
                        "shred any --file/--dotenv temp files. For a long-running child "
                        "whose credential must be a FILE (Google ADC's "
                        "GOOGLE_APPLICATION_CREDENTIALS takes a path, not a value) — the "
                        "case --exec cannot serve, because exec leaves nothing alive to "
                        "clean up. Output is NOT redacted (nothing is captured). Note: a "
                        "SIGKILLed charter runs no cleanup and the 0600 file survives.")
    p.add_argument("--exec", dest="exec_mode", action="store_true",
                   help="Replace this process with the command (os.exec) instead of capturing "
                        "it, so stdio streams through — required for a long-running child such "
                        "as an MCP stdio server. Output is NOT redacted (nothing is captured); "
                        "incompatible with --file and --dotenv — use --stream for those.")
    p.add_argument("command", nargs="*",
                   help="Command to run; put it after `--`, e.g. -- kubectl get pods.")


def _add_secret_parser(sub) -> None:
    s = sub.add_parser("secret", help="Read/write secrets in a vault; values stay out of the model.")
    ssub = s.add_subparsers(dest="secret_cmd", required=True)

    st = ssub.add_parser("set", help="Store a secret (value via --stdin/--from-file, not argv).")
    st.add_argument("vault"); _sa_set(st); st.set_defaults(func=commands_secrets.cmd_secret_set)

    ls = ssub.add_parser("list", help="List secret keys in a vault (never the values).")
    ls.add_argument("vault"); ls.set_defaults(func=commands_secrets.cmd_secret_list)

    au = ssub.add_parser("audit", help="Flag secrets older than --days for rotation.")
    au.add_argument("vault")
    au.add_argument("--days", type=int, default=90, help="Staleness threshold in days (default 90).")
    au.set_defaults(func=commands_secrets.cmd_secret_audit)

    g = ssub.add_parser("get", help="Show a secret (masked by default; --reveal for humans).")
    g.add_argument("vault"); _sa_get(g); g.set_defaults(func=commands_secrets.cmd_secret_get)

    rm = ssub.add_parser("rm", help="Delete a secret.")
    rm.add_argument("vault"); _sa_key(rm); rm.set_defaults(func=commands_secrets.cmd_secret_rm)

    cp = ssub.add_parser("cp", help="Materialize a secret to a 0600 file (e.g. a kubeconfig).")
    cp.add_argument("vault"); _sa_cp(cp); cp.set_defaults(func=commands_secrets.cmd_secret_cp)

    ex = ssub.add_parser("exec", help="Run a command with secrets injected as env/files, redacted.")
    ex.add_argument("vault"); _sa_exec(ex); ex.set_defaults(func=commands_secrets.cmd_secret_exec)


def _add_persona_parser(sub) -> None:
    pp = sub.add_parser("persona", help="Manage shared personas (committed) and use their vaults.")
    psub = pp.add_subparsers(dest="persona_cmd", required=True)

    cr = psub.add_parser("create", help="Create a persona → committed personas/<name>.md.")
    cr.add_argument("name")
    cr.add_argument("--role", help='Human role, e.g. "DevOps Engineer".')
    cr.add_argument("--delegate-when", metavar="WHEN",
                    help='REQUIRED (unless --extends): when the steward should route work '
                         'here, e.g. "CI/CD pipelines, k8s deploys". Becomes the persona\'s '
                         'routing line in its dispatchable description.')
    cr.add_argument("--vault", help="Vault this persona uses (default: the persona name).")
    cr.add_argument("--extends", metavar="PARENT",
                    help="Inherit another persona's charter + tools; this one adds its own on top.")
    cr.add_argument("--with-vault", action="store_true", help="Also register a local plain-file vault now.")
    cr.add_argument("--use", action="store_true", help="Make it the active persona.")
    cr.add_argument("--force", action="store_true", help="Overwrite an existing definition.")
    cr.set_defaults(func=commands_persona.cmd_persona_create)

    lst = psub.add_parser("list", help="List personas; mark active; show role + vault status.")
    lst.set_defaults(func=commands_persona.cmd_persona_list)

    show = psub.add_parser("show", help="Print a persona's metadata and charter.")
    show.add_argument("name"); show.set_defaults(func=commands_persona.cmd_persona_show)

    use = psub.add_parser("use", help="Set the active persona (writes .charter/active-persona).")
    use.add_argument("name"); use.set_defaults(func=commands_persona.cmd_persona_use)

    cur = psub.add_parser("current", help="Print the active persona and how it resolved.")
    cur.set_defaults(func=commands_persona.cmd_persona_current)

    clr = psub.add_parser("clear", help="Clear the active persona.")
    clr.set_defaults(func=commands_persona.cmd_persona_clear)

    df = psub.add_parser("default",
                         help="Show/set/clear the committed team-wide default persona (personas/.default).")
    df.add_argument("name", nargs="?", help="Persona to make the default (omit to show).")
    df.add_argument("--clear", action="store_true", help="Remove the committed default.")
    df.set_defaults(func=commands_persona.cmd_persona_default)

    rmv = psub.add_parser("remove", help="Delete a persona definition (commit the deletion).")
    rmv.add_argument("name")
    rmv.add_argument("--force", action="store_true",
                     help="Remove even if another persona extends/uses it (leaves a dangling ref).")
    rmv.set_defaults(func=commands_persona.cmd_persona_remove)

    sa = psub.add_parser("sync-agents",
                         help="Generate a Claude Code sub-agent (.claude/agents/<name>.md) per persona.")
    sa.add_argument("--persona", help="Only sync this persona (default: all).")
    sa.set_defaults(func=commands_persona.cmd_persona_sync_agents)

    mig = psub.add_parser("migrate",
                          help="Convert legacy personas/<name>.md → <name>/persona.md + memory/refs.")
    mig.add_argument("name", nargs="?", help="Only this persona (default: all).")
    mig.set_defaults(func=commands_persona.cmd_persona_migrate)

    # memory: the persona decides persistent (committed) vs ephemeral (session scratch).
    rem = psub.add_parser("remember", help="Write a memory (persistent by default; --ephemeral for scratch).")
    rem.add_argument("name", nargs="?", help="Persona (default: the active one).")
    rem.add_argument("text", help="The fact/note to remember.")
    rem.add_argument("--title", help="Short title (default: first line of the text).")
    rem.add_argument("--shared", action="store_true", help="Write to the cross-persona _shared namespace.")
    rem.add_argument("--ephemeral", action="store_true", help="Session scratch, deleted after the session.")
    rem.add_argument("--no-sync", action="store_true", dest="no_sync",
                     help="Don't reactively commit+push it now (record locally; sync later).")
    rem.set_defaults(func=commands_persona.cmd_persona_remember)

    rec = psub.add_parser("recall", help="Show a persona's memory, or --query to search it.")
    rec.add_argument("name", nargs="?", help="Persona (default: the active one).")
    rec.add_argument("--query", "-q", help="Keyword-search memories (ranked) instead of listing all.")
    rec.add_argument("--log", type=int, default=8, help="Log lines to show / max search hits (default: 8).")
    rec.set_defaults(func=commands_persona.cmd_persona_recall)

    dd = psub.add_parser("dedupe", help="Report near-duplicate memories (Jaccard overlap) to prune.")
    dd.add_argument("name", nargs="?", help="Persona (default: the active one).")
    dd.add_argument("--threshold", type=float, default=0.5, help="Overlap to flag (0-1, default 0.5).")
    dd.set_defaults(func=commands_persona.cmd_persona_dedupe)

    lt = psub.add_parser("lint", help="Config eval: dangling uses:, missing role/vault/delegate-when, stale agents.")
    lt.add_argument("name", nargs="?", help="Only this persona (default: all).")
    lt.set_defaults(func=commands_persona.cmd_persona_lint)

    stt = psub.add_parser("stats",
                          help="Roster health from committed memory: usage (count/recency) + "
                               "quality proxy (verification / dedup ratios); flags prune candidates.")
    stt.add_argument("name", nargs="?", help="Only this persona (default: all + _shared).")
    stt.add_argument("--recent-days", type=int, default=14, dest="recent_days",
                     help="Window for the RECENT column (default 14).")
    stt.set_defaults(func=commands_persona.cmd_persona_stats)

    bf = psub.add_parser("dispatch-backfill",
                         help="Seed the committed dispatch tally from past sessions' transcripts "
                              "(counts + dates only), so the routing baseline exists today.")
    bf.set_defaults(func=commands_persona.cmd_persona_dispatch_backfill)

    opt = psub.add_parser("optimize",
                          help="Curate persona memory: auto-apply safe ops (--apply: collapse exact "
                               "dups + repair index) and print tier-2 proposals for the steward to quiz.")
    opt.add_argument("name", nargs="?", help="Only this persona (default: all + _shared).")
    opt.add_argument("--all", action="store_true", help="Every persona (same as omitting name).")
    opt.add_argument("--apply", action="store_true",
                     help="Auto-apply the safe/reversible ops (else read-only report).")
    opt.add_argument("--stale-days", type=int, default=90, dest="stale_days",
                     help="Age (days) past which a memory is proposed for archival (default 90).")
    opt.set_defaults(func=commands_persona.cmd_persona_optimize)

    ms = psub.add_parser("memory-sync",
                         help="Commit + push all pending persona memory/refs in one step (secret-guarded).")
    ms.add_argument("--no-push", action="store_true", help="Commit only; don't push.")
    ms.set_defaults(func=commands_persona.cmd_persona_memory_sync)

    fgt = psub.add_parser("forget", help="Delete one memory by slug/filename.")
    fgt.add_argument("name"); fgt.add_argument("slug")
    fgt.add_argument("--shared", action="store_true")
    fgt.add_argument("--ephemeral", action="store_true")
    fgt.set_defaults(func=commands_persona.cmd_persona_forget)

    lg = psub.add_parser("log", help="Append to (with a message) or show the persona's activity log.")
    lg.add_argument("name", nargs="?", help="Persona (default: the active one).")
    lg.add_argument("message", nargs="?", help="Message to append; omit to show recent entries.")
    lg.add_argument("-n", type=int, default=20, help="How many entries to show (default: 20).")
    lg.set_defaults(func=commands_persona.cmd_persona_log)

    gc = psub.add_parser("_gc", help=argparse.SUPPRESS)  # SessionStart: prune ended-session scratch
    gc.add_argument("--detach", action="store_true", help=argparse.SUPPRESS)
    gc.set_defaults(func=commands_persona.cmd_persona_gc)

    tg = psub.add_parser("tool-gate",
                         help="PreToolUse gate: auto-approve the active persona's declared tools (reads stdin).")
    tg.set_defaults(func=lambda args: toolgate.main())

    sec = psub.add_parser("secret", help="Read/write the ACTIVE persona's vault (values stay out of the model).")
    xsub = sec.add_subparsers(dest="psecret_cmd", required=True)

    def _p(name, help_):
        q = xsub.add_parser(name, help=help_)
        q.add_argument("--persona", help="Override the active persona.")
        return q

    st = _p("set", "Store a secret in the persona's vault."); _sa_set(st)
    st.set_defaults(func=commands_persona.cmd_persona_secret_set)
    ls = _p("list", "List secret keys in the persona's vault.")
    ls.set_defaults(func=commands_persona.cmd_persona_secret_list)
    au = _p("audit", "Flag the persona's secrets older than --days for rotation.")
    au.add_argument("--days", type=int, default=90, help="Staleness threshold in days (default 90).")
    au.set_defaults(func=commands_persona.cmd_persona_secret_audit)
    g = _p("get", "Show a secret (masked; --reveal for humans)."); _sa_get(g)
    g.set_defaults(func=commands_persona.cmd_persona_secret_get)
    rm = _p("rm", "Delete a secret from the persona's vault."); _sa_key(rm)
    rm.set_defaults(func=commands_persona.cmd_persona_secret_rm)
    cp = _p("cp", "Materialize a secret to a 0600 file."); _sa_cp(cp)
    cp.set_defaults(func=commands_persona.cmd_persona_secret_cp)
    ex = _p("exec", "Run a command with the persona's secrets injected + redacted."); _sa_exec(ex)
    ex.set_defaults(func=commands_persona.cmd_persona_secret_exec)


# The persona *memory* subcommands mix an optional positional (persona name) with
# free-text and flags. argparse can't split positionals across an interspersed flag
# (`remember dev --shared "note"` fails), so we hoist their known flags to the end
# first — making `--shared`/`--ephemeral`/`--title` (etc.) work in any position while
# leaving positional text untouched. Scoped to these commands so no other command's
# flags are affected. (bool-flags, value-flags) per subcommand:
_MEM_FLAG_SPEC = {
    "remember": ({"--shared", "--ephemeral"}, {"--title"}),
    "recall": (set(), {"--log", "--query", "-q"}),
    "log": (set(), {"-n"}),
    "forget": ({"--shared", "--ephemeral"}, set()),
    "dedupe": (set(), {"--threshold"}),
}


def _hoist(tokens: list[str], bool_flags: set[str], value_flags: set[str]) -> list[str]:
    """Move recognized flags (and each value-flag's argument) after the positionals.
    Only the listed flags are moved; anything else — including free text that starts
    with '-' — stays a positional, so argparse handles it as before."""
    pos: list[str] = []
    flags: list[str] = []
    i = 0
    while i < len(tokens):
        t = tokens[i]
        if t in bool_flags:
            flags.append(t)
        elif t in value_flags:
            flags.append(t)
            if i + 1 < len(tokens):
                flags.append(tokens[i + 1])
                i += 1
        else:
            pos.append(t)
        i += 1
    return pos + flags


def _hoist_persona_memory(argv: list[str]) -> list[str]:
    if len(argv) >= 2 and argv[0] == "persona" and argv[1] in _MEM_FLAG_SPEC:
        bool_f, value_f = _MEM_FLAG_SPEC[argv[1]]
        return argv[:2] + _hoist(argv[2:], bool_f, value_f)
    return argv


#: argv prefixes whose trailing `-- <command…>` must survive argparse untouched.
_EXEC_PREFIXES = (("secret", "exec"), ("persona", "secret", "exec"))


def _split_exec_command(argv: list[str]) -> tuple[list[str], list[str] | None]:
    """Peel a trailing ``-- <command…>`` off an ``exec`` invocation before parsing.

    On **Python 3.11** argparse cannot hold a ``nargs="*"`` positional that follows
    repeated optionals, so the documented shape

        charter secret exec <vault> --env NAME=key -- kubectl get pods

    dies with "unrecognized arguments: -- kubectl get pods". 3.12 parses it fine —
    which is why this went unnoticed: the suite calls ``cmd_secret_exec`` directly
    and never crosses argparse, so CI was green on every version while the CLI was
    broken on the one charter declares as its floor.

    Splitting here makes the behaviour identical across versions. Only the `exec`
    subcommands are touched, and only at the FIRST ``--`` — anything after it is
    the child's, flags included.
    """
    for prefix in _EXEC_PREFIXES:
        if tuple(argv[:len(prefix)]) == prefix and "--" in argv[len(prefix):]:
            cut = argv.index("--", len(prefix))
            return argv[:cut], argv[cut + 1:]
    return argv, None


def _subcommand_names(parser: argparse.ArgumentParser) -> set[str]:
    """Every top-level subcommand the parser accepts.

    Reads the subparsers action directly. ``_SubParsersAction`` is nominally private but
    has been stable for the life of argparse, and the alternative — keeping a hand-written
    list of command names in sync with :func:`build_parser` — is the kind of duplication
    that goes wrong silently.
    """
    for action in parser._actions:
        if isinstance(action, argparse._SubParsersAction):
            return set(action.choices)
    return set()


def _hint_gap_if_unknown_command(parser: argparse.ArgumentParser, argv: list[str]) -> None:
    """Offer gap reporting when someone reached for a command charter does not have.

    This is the *only* mechanical signal charter gets that a capability is missing. The
    reporting feature is advertised on failure and nowhere else — which costs no prompt
    budget and is perfectly targeted — but a gap prints nothing on its own, so without this
    it would have no delivery mechanism at all.

    Narrow on purpose. A bad flag or a missing argument is a typo, not a missing
    capability, and offering to open an upstream issue every time someone mistypes would
    turn the prompt into noise — which is how a feature like this gets switched off.
    """
    if not argv or argv[0].startswith("-") or argv[0] in _subcommand_names(parser):
        return
    util.err(f"↳ charter has no `{argv[0]}` command. If that is something charter should "
             f'do, say so: charter report gap "<what you were trying to do>"')


def _record_crash(exc: BaseException, subcommand: str) -> None:
    """Draft a report for a crash, locally. Never raises.

    Wrapped defensively because a bug in the bug reporter is the worst possible thing to
    surface in place of the real error: whatever happens here, the caller re-raises the
    original exception and the developer still sees what actually broke.

    Nothing is published. This only writes to the Reporter's own disk, which is what lets
    detection default to on without charter reading as telemetry (docs/adr/0003).
    """
    try:
        from . import report
        rid = report.record_bug(exc, subcommand)
        if not rid:
            return
        rec = report.load(rid) or {}
        if rec.get("issue_url"):
            # Already filed from this machine. Pointing at the existing issue is the whole
            # reason a sent report is kept rather than deleted — local dedupe, no API call.
            util.err(f"↳ this is a charter bug you already reported → {rec['issue_url']}")
        else:
            util.err(f"↳ this is a charter bug, drafted locally as {rid} — nothing sent. "
                     f"Review it: charter report show {rid}")
    except Exception:  # noqa: BLE001 - see the docstring; this must never win over `exc`
        pass


def main(argv=None) -> int:
    argv = list(sys.argv[1:] if argv is None else argv)
    argv = _hoist_persona_memory(argv)
    argv, exec_command = _split_exec_command(argv)
    parser = build_parser()
    try:
        args = parser.parse_args(argv)
    except SystemExit:
        # `parse_args` exits from inside itself, above the crash handler below — so the
        # gap signal needs catching here rather than a third `except` down there.
        _hint_gap_if_unknown_command(parser, argv)
        raise
    if exec_command is not None:
        args.command = exec_command
    try:
        return args.func(args) or 0
    except KeyboardInterrupt:
        return 130
    except BrokenPipeError:
        # `charter … | head` is ordinary shell usage: the reader stopped, so the next write
        # fails. A condition, not a bug — charter did nothing wrong, so this joins the two
        # clauses below rather than falling through to the crash reporter, which used to
        # file a report every time somebody piped output into `head`.
        #
        # Redirecting at the fd level is what stops the interpreter trying to flush the
        # dead pipe again on the way out; without it the process exits 120 (a failed
        # shutdown flush) whatever is returned here.
        #
        # BOTH streams, and stderr is the one that actually matters: charter's progress
        # output goes there via util.ok/info, so under the usual `charter … 2>&1 | head`
        # it is stderr that hits the closed pipe. Redirecting only stdout left the exit
        # code at 120 with the report correctly suppressed — half a fix that looked whole.
        #
        # Guarded because a captured stream has no fileno: under test the redirect is
        # skipped, and there is no real pipe there to suppress anyway.
        for stream in (sys.stdout, sys.stderr):
            try:
                os.dup2(os.open(os.devnull, os.O_WRONLY), stream.fileno())
            except Exception:  # noqa: BLE001 - a best-effort tidy-up, never the story
                pass
        return 141  # 128 + SIGPIPE, matching the 128 + SIGINT returned below
    except util.ProcTimeout as e:
        # A child that outlived its budget is a condition, not a bug. Only
        # KeyboardInterrupt was caught here, so a timeout reached the user as a traceback
        # from inside charter — which reads as charter crashing rather than as the tool it
        # called failing to answer.
        util.err(str(e))
        return 1
    except Exception as e:
        # Anything reaching here IS a charter bug — the two clauses above are exactly the
        # conditions, and that distinction predates this feature. charter is the only thing
        # that reliably observes its own crash (no hook does: PostToolUse matches only
        # Write|Edit|MultiEdit and Task|Agent, and PreToolUse runs *before* the command),
        # and it already holds the exception, the subcommand and the version that a hook
        # would have to reconstruct from a string.
        _record_crash(e, argv[0] if argv else "?")
        raise  # Recording is not handling. The developer still gets their traceback.
