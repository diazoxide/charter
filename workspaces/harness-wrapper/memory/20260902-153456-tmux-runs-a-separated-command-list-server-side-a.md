# tmux runs a ';'-separated command list server-side and ABORTS the rest a

_2026-09-02 15:34 · persistent_

tmux runs a ';'-separated command list server-side and ABORTS the rest at the first refusal — measured on 3.7c and at the 3.2 floor. So batching charter's fire-and-forget tmux writes needs a fallback that re-issues each write alone on a non-zero chain, not a bare chain(). A chain of split-window -P -F answers with one pane id per line in order, so even the splits batch; a timeout must never be replayed because charter cannot know it is repeating.
