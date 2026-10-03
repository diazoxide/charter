window.BENCHMARK_DATA = {
  "lastUpdate": 1791029981199,
  "repoUrl": "https://github.com/diazoxide/charter",
  "entries": {
    "session layer (ubuntu-24.04)": [
      {
        "commit": {
          "author": {
            "email": "aaron.yor@gmail.com",
            "name": "Aaron Yordanyan",
            "username": "diazoxide"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "23d07b28b6f1fb3c7cd113124ee70b695fa82f05",
          "message": "FD-25: per-device logs are named by the device id, not the hostname (#943)\n\n* Device id replaces hostname keys in per-device logs (FD-25)\n\nThe dispatch and skill logs, the piece claim log, the landing log and\npending landings are named by this device's id from the machine store\n(ADR 0066), with the hostname kept as a label. A writer reads the id and\nnever mints it; before one exists the file keeps the hostname name.\nRenaming files written under a hostname is FR-9's (#607).\n\nRefs #662\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>\n\n* FD-25: the new device-log tests shed the shell's steering first\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>\n\n---------\n\nCo-authored-by: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-10-02T23:15:14+04:00",
          "tree_id": "76f31c359d011ba0d270b8fbc447db8e6f7c02ec",
          "url": "https://github.com/diazoxide/charter/commit/23d07b28b6f1fb3c7cd113124ee70b695fa82f05"
        },
        "date": 1790968660241,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.5616239999999999,
            "unit": "ms",
            "extra": "median of 5 runs: 0.538, 0.546, 0.562, 0.569, 0.574 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.9554925,
            "unit": "ms",
            "extra": "median of 5 runs: 16.823, 16.866, 16.955, 16.979, 17.432 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 104.4088925,
            "unit": "ms",
            "extra": "median of 5 runs: 103.633, 104.332, 104.409, 104.813, 105.135 ms"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "aaron.yor@gmail.com",
            "name": "Aaron Yordanyan",
            "username": "diazoxide"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "0c8dfc58d02c260fe5efd7f577ede05b9b790add",
          "message": "Every view is drawn from tokens, in both themes (DS-1) (#957)\n\n* Every view is drawn from tokens, in both themes (DS-1)\n\nA rendered token test: each of charter's own views and an extension's is\ndrawn in each built-in theme, and every element is checked for a colour\nin an inline style or SVG paint attribute, a var() that is not a token,\na token the theme in force does not set, and an arbitrary-value class.\nIt covers colours built at run time, which the source guard cannot see.\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>\n\n* Token test draws every state of every view, and its loopholes are shut (DS-1 review)\n\nFixtures now reach every state a view tab draws: the changes view's\nPush, Land and a refused landing; bars and columns charts; every tone\nand row mark, with row actions; gone, refused and waiting views; heading\noffers; a revealed secret and an unreadable vault; memory edit and new\nmemory; a picked theme and a custom workspace colour.\n\nBoth guards share one definition of a colour (theme/literal.ts): every\nCSS named colour and the system colours, case-insensitively, and colours\ninside data URLs. The rendered checker reads var() in any case, holds an\nSVG paint's var() to the tokens and the theme, and refuses an arbitrary\nvalue behind a variant, \"!\", a modifier, or as a bare property. Each\nloophole has a probe that was seen red.\n\nOWN_MARKS is exported, and a test fails when a view in it has no state.\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>\n\n---------\n\nCo-authored-by: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-10-02T23:49:08+04:00",
          "tree_id": "991c7b248fe4fd1e7f774fc3e58e16c9825a26b1",
          "url": "https://github.com/diazoxide/charter/commit/0c8dfc58d02c260fe5efd7f577ede05b9b790add"
        },
        "date": 1790973453199,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.5707875,
            "unit": "ms",
            "extra": "median of 5 runs: 0.564, 0.567, 0.571, 0.587, 0.591 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 17.112492500000002,
            "unit": "ms",
            "extra": "median of 5 runs: 17.046, 17.057, 17.112, 17.174, 17.300 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 104.62407300000001,
            "unit": "ms",
            "extra": "median of 5 runs: 103.450, 104.200, 104.624, 105.191, 105.457 ms"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "aaron.yor@gmail.com",
            "name": "Aaron Yordanyan",
            "username": "diazoxide"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "4ab28aca7ab92e55366185d172a54c7813cc67a2",
          "message": "First switch into a busy project: the explorer's chat rows paint without layers of their own (#891) (#974)\n\n* A chat's explorer row and state mark paint without layers of their own (#891)\n\nThe first switch of each round into the launch project set L9's p95.\nProfiling on CI (#912) showed the core answers watch_session and\nunwatch_session in under 5 ms on the main thread, with no stall: the\ntime was the WebView's own paint of the project's fifty explorer chat\nrows. Each row had opacity 0.85 and its unknown mark opacity 0.5 inside\nit, two nested transparency layers per row, and the mark a dashed round\nborder. The dimming is now mixed into the colour and the unknown ring is\ntwo solid arcs, still a different shape from done's whole ring.\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>\n\n* paint.test.ts scans every per-chat rule; unreported and the row icon mixed too (#891 review)\n\nThe test read only the first rule with the exact selector, so a later\nor grouped rule, filter: opacity(), a dashed border on another state\nselector or a deleted fallback all passed. It now scans every rule\nwhose selector list names a per-chat element, rejects opacity, filter\nopacity() and dashed or dotted borders in each, and wants a plain\ndeclaration before each color-mix one. .explorer .unreported and a\nrow's icon are dimmed in their colour too.\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>\n\n---------\n\nCo-authored-by: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-10-03T01:00:35+04:00",
          "tree_id": "33e9f65a14472c61656677d24ec7bde95b6999e5",
          "url": "https://github.com/diazoxide/charter/commit/4ab28aca7ab92e55366185d172a54c7813cc67a2"
        },
        "date": 1790975913298,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.49838499999999997,
            "unit": "ms",
            "extra": "median of 5 runs: 0.492, 0.498, 0.498, 0.506, 0.511 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.276179499999998,
            "unit": "ms",
            "extra": "median of 5 runs: 16.238, 16.239, 16.276, 16.283, 16.354 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 101.7259865,
            "unit": "ms",
            "extra": "median of 5 runs: 101.139, 101.207, 101.726, 101.939, 102.950 ms"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "aaron.yor@gmail.com",
            "name": "Aaron Yordanyan",
            "username": "diazoxide"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "85d00c8bf37956d42b97f28bd6b1fd213da21697",
          "message": "bench: a throwaway profile still being written to no longer fails the gate (#1001)\n\nAfter the app exits, a WebKit helper can still be writing into the bench's\nthrowaway profile, and a single recursive rmSync then meets ENOTEMPTY. That\nturned main's ubuntu \"app builds\" job red while the cold start itself was\nwithin its limit. tools/cleanup.mjs removes the profile with retries; if it\nstill can't, it reports and leaves the directory behind. Cleanup is\nhousekeeping, not part of the measurement.\n\nRefs #954\n\nCo-authored-by: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-10-03T02:02:03+04:00",
          "tree_id": "456a119a62554a7a09c12e296fdca09e93e0b658",
          "url": "https://github.com/diazoxide/charter/commit/85d00c8bf37956d42b97f28bd6b1fd213da21697"
        },
        "date": 1790979479686,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.6255135000000001,
            "unit": "ms",
            "extra": "median of 5 runs: 0.605, 0.613, 0.626, 0.627, 0.632 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.002759,
            "unit": "ms",
            "extra": "median of 5 runs: 15.928, 15.979, 16.003, 16.045, 16.444 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 103.099471,
            "unit": "ms",
            "extra": "median of 5 runs: 101.850, 101.853, 103.099, 103.178, 103.471 ms"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "aaron.yor@gmail.com",
            "name": "Aaron Yordanyan",
            "username": "diazoxide"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "fe47ef5d2195bc2c4768749747ec5efd6e3f8f97",
          "message": "theme guards: a token-only color-mix dims, and the worktree views are drawn (#1020)\n\nMain's web job is red on two tests. Each comes from two PRs that merged separately:\n\n- #974 dims explorer rows with color-mix(in srgb, currentcolor N%,\n  transparent), and #957's colour-literal guard refuses every colour\n  function. A mix of only currentcolor, transparent and var(--token) adds\n  no colour of its own, so both guards (the stylesheet one and the\n  rendered one) now let it through. A mix with a hex, rgb() or named\n  colour inside it is still refused, and probes cover both cases.\n- #950 added the piece-files and piece-file view kinds, and #957's\n  guard requires every view to have a drawn state. Both now have one.\n\nCloses #1017\n\nCo-authored-by: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-10-03T02:24:24+04:00",
          "tree_id": "a182c06294779ba39a19b7a258258f7a98272558",
          "url": "https://github.com/diazoxide/charter/commit/fe47ef5d2195bc2c4768749747ec5efd6e3f8f97"
        },
        "date": 1790980235517,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.42856700000000003,
            "unit": "ms",
            "extra": "median of 5 runs: 0.404, 0.406, 0.429, 0.442, 0.444 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.6733015,
            "unit": "ms",
            "extra": "median of 5 runs: 16.473, 16.629, 16.673, 16.686, 16.726 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 102.13193050000001,
            "unit": "ms",
            "extra": "median of 5 runs: 100.632, 101.826, 102.132, 102.266, 102.397 ms"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "aaron.yor@gmail.com",
            "name": "Aaron Yordanyan",
            "username": "diazoxide"
          },
          "committer": {
            "email": "aaron.yor@gmail.com",
            "name": "Aaron Yordanyan",
            "username": "diazoxide"
          },
          "distinct": true,
          "id": "d09addcb16ee0384fc057340f943ea5b459a4f48",
          "message": "Hook spool with sequence numbers: no hook call is lost when the host is down (FD-30)\n\nA hook now delivers its line before it answers its harness. The host says it\ntook the line only once its hearer has recorded the event and fsynced the\nevent log. Otherwise the line goes to the chat's own spool in a project's\n.charter/app/spool/, fsynced, with the next number in that key's sequence and\na MAC under a key derived from the chat's token. The app drains every spool\nwhen it opens a project, before any chat starts, and records each line that\nchecks under the run it ran in, plus each gap and each line that did not\ncheck (hook.spool.gap, hook.spool.rejected, hook.spool.drained).\n\n- The chat sandbox is denied reading and writing every spool and its keys.\n  Nothing spools, and no key is written, where that denial does not reach.\n- A line that is not text, or a keys file that does not read, is rejected and\n  never stops the drain or the next hook.\n- The fsync follows each seal of the event log (FD-24) onto the segment being\n  written.\n- A refused commit is recorded as hook.commit_refused.\n- ADR 0068 §6 is amended by ruling V63, and ADR 0075's registry gains\n  hook.commit_refused.\n\nThis closes the two departures from ADR 0075 §7 that FD-9 recorded: the hook\nanswered before its event was recorded, and the event log was not fsynced.\n\nCloses #667\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-10-03T07:45:44+04:00",
          "tree_id": "354e0ec590a5befff7bb6f1a0c8688e90b38e3fa",
          "url": "https://github.com/diazoxide/charter/commit/d09addcb16ee0384fc057340f943ea5b459a4f48"
        },
        "date": 1790999209612,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.621307,
            "unit": "ms",
            "extra": "median of 5 runs: 0.603, 0.608, 0.621, 0.630, 0.631 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.0742715,
            "unit": "ms",
            "extra": "median of 5 runs: 15.891, 16.064, 16.074, 16.124, 16.510 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 101.4633965,
            "unit": "ms",
            "extra": "median of 5 runs: 101.131, 101.259, 101.463, 101.639, 101.723 ms"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "aaron.yor@gmail.com",
            "name": "Aaron Yordanyan",
            "username": "diazoxide"
          },
          "committer": {
            "email": "aaron.yor@gmail.com",
            "name": "Aaron Yordanyan",
            "username": "diazoxide"
          },
          "distinct": true,
          "id": "d7d8244356d26b66f1426859cecc6e636e33f704",
          "message": "e2e: Undo of a memory waits for its index line as well as its file\n\nThe memory store's unarchive renames the file back, then appends its\nindex line, under its own lock. The test polled for the file and then\nread the index straight away, so it could land between the two writes.\nIt failed once on train 1 (passed on re-run) and on both platforms on\ntrain 2.\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-10-03T08:13:50+04:00",
          "tree_id": "8daa4763c488e2a4c588673e0940ada30215a949",
          "url": "https://github.com/diazoxide/charter/commit/d7d8244356d26b66f1426859cecc6e636e33f704"
        },
        "date": 1791000891269,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.3439535,
            "unit": "ms",
            "extra": "median of 5 runs: 0.341, 0.343, 0.344, 0.345, 0.359 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.673347999999997,
            "unit": "ms",
            "extra": "median of 5 runs: 16.653, 16.672, 16.673, 16.708, 16.775 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 101.08881500000001,
            "unit": "ms",
            "extra": "median of 5 runs: 100.316, 100.334, 101.089, 101.230, 101.267 ms"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "aaron.yor@gmail.com",
            "name": "Aaron Yordanyan",
            "username": "diazoxide"
          },
          "committer": {
            "email": "aaron.yor@gmail.com",
            "name": "Aaron Yordanyan",
            "username": "diazoxide"
          },
          "distinct": true,
          "id": "47940265034a153fe1aee01065882e3afa21bb1c",
          "message": "changelog fold: a guessed flag folds nothing, and a stray file is refused\n\nTwo agents ran `node tools/changelog-fold.mjs --help` expecting help. It\nfolded their fragments into CHANGELOG.md and deleted them instead. The\nscript now:\n\n- answers `--help` and `-h` with its usage;\n- refuses any other argument with usage and exit 2, writing nothing;\n- refuses a file in changes/ that isn't README.md or a <slug>.md\n  fragment, because it used to be skipped silently and its entry lost;\n- recognises itself when run through a symlinked path. It used to do\n  nothing and exit 0, so `--check` passed with a fragment waiting.\n\nRefs #1023\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-10-03T09:46:36+04:00",
          "tree_id": "940b06ffea13112343e0b7d3cdd69af6f69ea332",
          "url": "https://github.com/diazoxide/charter/commit/47940265034a153fe1aee01065882e3afa21bb1c"
        },
        "date": 1791006459617,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.587981,
            "unit": "ms",
            "extra": "median of 5 runs: 0.581, 0.586, 0.588, 0.597, 0.598 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.904024,
            "unit": "ms",
            "extra": "median of 5 runs: 16.375, 16.466, 16.904, 17.328, 17.488 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 103.4111,
            "unit": "ms",
            "extra": "median of 5 runs: 103.167, 103.278, 103.411, 104.357, 106.405 ms"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "aaron.yor@gmail.com",
            "name": "Aaron Yordanyan",
            "username": "diazoxide"
          },
          "committer": {
            "email": "aaron.yor@gmail.com",
            "name": "Aaron Yordanyan",
            "username": "diazoxide"
          },
          "distinct": true,
          "id": "9055f7a4cfd2b366aaf8d17444bb357220374a73",
          "message": "SC-3: a per-chat store, so a chat moving redraws only its own rows\n\nWhat every chat in a project is doing now lives in a store outside React (chatState.ts:\nuseChats, useChatsSelect, ChatsHere), not in a hook's state at the top of PlaneView. Each\nreader subscribes to its own share through useSyncExternalStore:\n\n- a tab's mark and an explorer row's mark read their own chat (ChatRows.tsx: ChatStateMark,\n  memo'd, and TabMarks, moved out of PlaneView);\n- a pane reads its own chat's last move and whether it is running, for its gauge;\n- the status line reads the running count itself;\n- PlaneView reads only the queue, reports, refusals, the quiet list and the show-more orders,\n  each compared so that an unchanged share does not redraw it;\n- the quit list's states and the newest move are reported to the window straight off the\n  store, and PlaneView is memo'd, so the window redrawing for that report does not redraw it.\n\nThe reducer keeps what a move did not change as the same object (the queue, and each chat's\nreports and refusals), so identity checks hold. A move that carries no queue or no report\nlists is still taken, reading them as empty. useChatsSelect is React's own\nuse-sync-external-store/with-selector (MIT, the React team's package), not a hand-written\nselector cache.\n\nA resting tab no longer keeps dnd-kit's one-frame `transition: transform 0ms linear`\n(sortable.tsx). dnd-kit sets it on the render after a strip's items change and relies on a\nnext render to take it off; with the project view no longer redrawn on every move, a strip\nread once never got one, and the inline rule hid the stylesheet's transition (motion.e2e.ts,\ntrain 3). Workspaces.test.tsx guards it.\n\nMeasured with MovedRenders.test.tsx (five chats, one moves; ChatMark and pane renders):\nbefore, 10 marks and 2 pane renders per move; after, 1 mark and 0 panes for a state change,\nand 1 mark and 1 pane when the needs-you queue changes too (#1034).\n\nCloses #681\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-10-03T11:55:03+04:00",
          "tree_id": "c98a836a08feda0728581c05f7bea69f5012c837",
          "url": "https://github.com/diazoxide/charter/commit/9055f7a4cfd2b366aaf8d17444bb357220374a73"
        },
        "date": 1791014179978,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.4331585,
            "unit": "ms",
            "extra": "median of 5 runs: 0.417, 0.427, 0.433, 0.433, 0.443 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.6302965,
            "unit": "ms",
            "extra": "median of 5 runs: 16.565, 16.584, 16.630, 16.638, 16.663 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 101.4856775,
            "unit": "ms",
            "extra": "median of 5 runs: 100.897, 101.456, 101.486, 102.050, 102.053 ms"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "aaron.yor@gmail.com",
            "name": "Aaron Yordanyan",
            "username": "diazoxide"
          },
          "committer": {
            "email": "aaron.yor@gmail.com",
            "name": "Aaron Yordanyan",
            "username": "diazoxide"
          },
          "distinct": true,
          "id": "85b76f2b126a0bfe8e7de24eda40023d74e19cc7",
          "message": "DS-9: the first run counts its prompts and fails above three\n\nW10's interrupt budget: at most three prompts before the first answered\nagent turn. app/src/interruptBudget.ts counts every prompt a page shows:\neach dialog and alertdialog (an open native <dialog> included), and each\ninline question: a group or radiogroup named by its question, or a\nfieldset whose legend is one, so an ask moved onto the page is still\ncounted. Its docstring lists what it cannot see. Every\nscenario in FirstRun.test.tsx now counts its prompts and fails above\nthree, naming each one. A new test walks the longest way to a chat\n(forge question, trust question, picker) and pins it at exactly three,\nso one more prompt on that way fails the build.\n\nThe ForgeQuestion, RepoInstructionsTab and PlaneView comments now describe the\nbudget this way, and docs/design-system.md has a section on it.\n\nCloses #631\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-10-03T14:10:49+04:00",
          "tree_id": "fa953d4b643dc47b7bebf3050bd399866f57d756",
          "url": "https://github.com/diazoxide/charter/commit/85b76f2b126a0bfe8e7de24eda40023d74e19cc7"
        },
        "date": 1791022433731,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.5827484999999999,
            "unit": "ms",
            "extra": "median of 5 runs: 0.571, 0.578, 0.583, 0.583, 0.586 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 17.004761000000002,
            "unit": "ms",
            "extra": "median of 5 runs: 16.554, 16.999, 17.005, 17.041, 17.223 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 104.692088,
            "unit": "ms",
            "extra": "median of 5 runs: 103.502, 103.688, 104.692, 105.160, 105.807 ms"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "aaron.yor@gmail.com",
            "name": "Aaron Yordanyan",
            "username": "diazoxide"
          },
          "committer": {
            "email": "aaron.yor@gmail.com",
            "name": "Aaron Yordanyan",
            "username": "diazoxide"
          },
          "distinct": true,
          "id": "4d5d9490c64cdc98951f54397c84974abf7a0164",
          "message": "HY-18: ticket and ADR issue forms\n\n.github/ISSUE_TEMPLATE/ticket.yml carries the sections the program map\nfiles a ticket with, and adr.yml an ADR proposal's; both ask for the\nmilestone and each label family HY-18 set up, and adr.yml applies\ntype:adr. The community test holds both forms.\n\nCloses #1097\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-10-03T16:09:03+04:00",
          "tree_id": "8c5dc2868fd48b7277ef0a00ecdb5c9b5fd88fee",
          "url": "https://github.com/diazoxide/charter/commit/4d5d9490c64cdc98951f54397c84974abf7a0164"
        },
        "date": 1791029980769,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.592132,
            "unit": "ms",
            "extra": "median of 5 runs: 0.580, 0.590, 0.592, 0.597, 0.597 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.956614000000002,
            "unit": "ms",
            "extra": "median of 5 runs: 16.710, 16.882, 16.957, 17.254, 17.264 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 105.096882,
            "unit": "ms",
            "extra": "median of 5 runs: 102.718, 105.044, 105.097, 105.338, 105.396 ms"
          }
        ]
      }
    ]
  }
}