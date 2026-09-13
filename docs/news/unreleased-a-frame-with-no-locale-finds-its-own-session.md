---
version: unreleased
headline: A frame launched from a shell with no UTF-8 locale finds its own tmux session, and `charter handoff` no longer calls its plane another plane's
---

tmux decides what it may print from the locale of the process asking. With `$LANG`,
`$LC_ALL` and `$LC_CTYPE` all unset, or `LC_ALL=C`, tmux 3.7c printed the tab between the
fields of a `-F` format as `_`. Five of the formats charter reads its frame through are
tab-separated, so each row read as one field. Charter could not find a session its own
launcher had made, and `charter handoff` from inside that frame refused with "…probably
another plane's". A plane whose path is not ASCII was mangled the same way: `/tmp/plané_x`
read back as `/tmp/plan__x`.

Charter now passes `-u` to every tmux command it runs, so tmux answers in UTF-8 whatever
the shell's locale says. Setting a UTF-8 locale for the child would have fixed it too, but
tmux copied that locale into the server's environment and into every pane started after it.
`-u` affects only the command that carries it.

The attach that puts a frame on your terminal carries `-u` too. tmux then draws the frame in
UTF-8, pane borders included, even when the locale says the terminal is not UTF-8. On a
UTF-8 terminal whose shell sets no locale, which is the usual case, non-ASCII text now shows
correctly instead of as `_`.

Nothing to adopt ([#984](https://github.com/diazoxide/charter/issues/984)).
