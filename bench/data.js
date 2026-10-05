window.BENCHMARK_DATA = {
  "lastUpdate": 1791177092005,
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
          "id": "b6069cea22c6a3efc5621bc933b26e900a0f97c8",
          "message": "Burn-down 1: read-only charter tools pre-allowed, request mode, branch on screen\n\nV79 (#1050): a Claude Code chat the app starts carries an allow for each of\ncharter's five read-only MCP tools (todo_list, memory_search,\nsession_record_list, session_record_read, change_status) beside Smart close's\nallow, in the same per-chat --settings. The list is written out\n(chattools::PRE_ALLOWED), never derived from the read-only mark, so\nask_operator and every write still ask. ADR 0064's SI-8e section and the spec\nare amended, and record what was measured on Claude Code 2.1.288: the\nsession's --mcp-config server wins over a same-named project, local or user\nserver, which is not started at all.\n\nV81 (#1087): prose calls the pr and pr-merge modes \"request mode\" on every\nforge, including prsave's save_branch refusal, the docs and CONTEXT.md, which\ngains a Request mode entry. The config words are unchanged. No recorded\nscenario pinned the old words.\n\n#989: the window calls a piece its branch and its directory the branch's\nfolder. Menu rows name the branch by its own name (Cut carries it) and a\nfolder on no branch as a folder; the bottom bar counts branches. The window's\nmerge, remove, mark-done and list commands send the core's window sentence\n(worktree::Refusal::in_window, the new NotDeclared::in_window), and the\nrefusals charter words itself (seven merge and remove cases) gain one through\nRefusal::Stuck, so no \"worktree\", \"git -C\" or \"--force\" of charter's reaches\nthe window; charter worktree keeps its sentences byte for byte. A merge of a\nmissing piece is refused as missing, and workspace drop says \"branch\nfolders\". CONTEXT.md says so.\n\nCloses #1050\nCloses #1087\nCloses #989\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-10-03T16:46:13+04:00",
          "tree_id": "59dd233ae04b01cabfeebddb11ea45874bb8dbab",
          "url": "https://github.com/diazoxide/charter/commit/b6069cea22c6a3efc5621bc933b26e900a0f97c8"
        },
        "date": 1791031883314,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.4830015,
            "unit": "ms",
            "extra": "median of 5 runs: 0.477, 0.482, 0.483, 0.489, 0.494 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.5059435,
            "unit": "ms",
            "extra": "median of 5 runs: 16.172, 16.214, 16.506, 16.898, 16.994 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 101.1567185,
            "unit": "ms",
            "extra": "median of 5 runs: 100.704, 101.080, 101.157, 101.210, 102.185 ms"
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
          "id": "21e20045093dc40e68448a288967f8a10d0d3624",
          "message": "FM-1: branches in the explorer expand into their files\n\nEach branch in the explorer, and each repo for its own folder (#948), has a\nFiles row that opens folder by folder into its files, inside the same tree.\n\nPrefactor: charter-core's `piecefiles` becomes `files` (git mv), keeping all\nof the light editor's confinement, and its calls take a `Branch` (a piece, or\nno piece for the repo's own folder). The light editor's commands moved onto\nit; their tests pass on the new module.\n\nNew in core: `files::tree(branch, folder)`, one level per call: name, kind\n(folder, file, link), ignored, and the reason it does not open. Plus\n`files::folder` for the watch.\n\nApp: `branch_tree` and `files_watch` (both off the main thread), a\n`files-changed` event from a new `filewatch.rs`, and the explorer's lazy file\nrows with tree semantics, arrows, Enter and Home/End.\n\nDecided in implementation:\n- D-1 Where the files hang: a \"Files\" row is the first child of each branch\n  and each repo. The branch row itself does not fold. A click on a branch\n  picks where the next chat starts, and its chats stay drawn under it (#238).\n  Rejected: folding the branch row, which hides its chats.\n- D-2 Laziness: one listing plus one `git check-ignore` per opened folder.\n  Not `ls-files`, which walks the whole tree. Paths go to git on its standard\n  input (`--stdin -z -v -n`, through a new `git::run_with_input`) as\n  `./<path>`. The answers are matched by position, never by spelling. A name\n  is also asked in composed form, and it counts as ignored when either\n  spelling is.\n- D-3 Ignored: an ignored folder still expands (listing only). An ignored file\n  is refused. `.git` counts as ignored-and-refused, so it is hidden by default\n  and shown with its reason under \"Show ignored files\". That toggle is\n  per-window state and appears only while some branch's files are open.\n- D-4 A nested repository or submodule is drawn, and nothing inside it\n  opens, which matches what `open` answers. A linked folder never expands,\n  even when it stays inside. A link opens only to another offered file.\n- D-5 The repo's own folder: `Branch { piece: None }`, accepted only when\n  `git rev-parse --show-toplevel` is that folder, so a plain folder inside the\n  project's repository is refused. The view key is `ws/repo/` (an empty piece\n  segment, which no piece name can be), and the commands take\n  `piece: string | null`.\n- D-6 Live updates reuse planewatch's notify and debouncer pattern in a new\n  `filewatch.rs`. It watches exactly the folders a window has opened, each\n  non-recursively, as a per-window set, dropped when the window is destroyed.\n  A newer ask always wins over a set that resolved late. Rejected: widening\n  planewatch, which deliberately stays out of clones, and polling.\n- D-7 Wording (V81, CONTEXT.md): the refusals keep burn-1's \"branch's\n  folder\" sentences, which are now on main, on the new `files` module. The new\n  rows say \"Files\", \"Show ignored files\" and \"N more not shown\".\n- D-8 One folder level answers its first 5,000 entries after sorting, plus a\n  count of the rest, drawn as \"N more not shown\". The folder is listed through\n  a descriptor opened one component at a time with O_NOFOLLOW|O_DIRECTORY, so\n  a folder swapped for a link is refused. Windows lists by path and then\n  checks the folder again. A path under a submodule the index records but\n  that has no `.git` on disk counts as nested too, so it never reaches git,\n  and a link to a target whose name is not UTF-8 is refused.\n- D-9 `files_watch` takes at most 256 folders and resolves each branch once\n  (`files::folders`). planewatch and filewatch share `watchset.rs`\n  (`matters`, `follow`), and the window-side `branch()` is shared too.\n\nTests: crates/charter-core/tests/a_branch_expands_into_its_files.rs (20, on\nreal git fixtures, including names git spells differently, a link to one,\nthe 5,000 cap and several folders of one branch); a unit test that a folder\nwhich is a link by the time it is listed is refused; app unit tests for\nbranch_tree and filewatch; src/Explorer.files.test.tsx (mockIPC: refused\nrows with their reasons, ignored toggle, re-read and removal on\nfiles-changed, the count of the rest, arrows, Enter, Home and End);\nworkspace-explorer.e2e.ts expands the fixture branch, opens README.md in its\nfile tab, sees an agent's file appear and go, sees one appear in a nested\nopen folder, and expands a repo's own folder; a link into an uninitialised\nsubmodule; and a folder whose name holds a newline expands.\n\nLeftover: #1128 (on Windows, list a branch's folder through a handle).\n\nCloses #1104\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-10-03T17:56:14+04:00",
          "tree_id": "580acd953ae290e1e5dd814d375bfbb0c3255cd9",
          "url": "https://github.com/diazoxide/charter/commit/21e20045093dc40e68448a288967f8a10d0d3624"
        },
        "date": 1791036095712,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.581055,
            "unit": "ms",
            "extra": "median of 5 runs: 0.565, 0.580, 0.581, 0.590, 0.597 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 17.1382655,
            "unit": "ms",
            "extra": "median of 5 runs: 16.726, 16.745, 17.138, 17.275, 17.408 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 104.21552550000001,
            "unit": "ms",
            "extra": "median of 5 runs: 102.839, 103.990, 104.216, 105.765, 105.873 ms"
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
          "id": "e396e568fb8747269cb79e6942524cbf7cf7e724",
          "message": "FM-2: the file tab becomes a tree beside a resizable preview\n\n\"Browse the files\" shows the branch as the explorer's own tree (FM-1's\nrows, read a folder at a time) instead of a flat list capped at 500,\nbeside a read-only preview, with a divider between them that is dragged\nor moved with the arrow keys. Where the divider was left is kept for the\ntab across a close and a relaunch. Opening the same branch's files again\nstill brings its one tab forward; the view key is unchanged.\n\nCore: files::open now knows an image by its first bytes (PNG, JPEG, GIF,\nWebP) and answers its type and bytes, and the preview cap is 2 MiB\n(was 5 MiB). reopen::View gains `split`, and a `piece-files` view's key\n(`ws/repo/piece`, or `ws/repo/` for the repo's own folder) is now kept in\nthe record, so the tab comes back at a relaunch at all.\n\nApp: PieceFile gains `image` (base64), ViewTab gains `split`; tabs.ts\nkeeps each view's split (`splits`), kept after its tab closes; a new\neditor/BranchTree.tsx draws the tree; PieceFiles has one fetch path and\none union for a file's states (#988).\n\nShared tree component (kept minimal, for FM-4 and FM-7): Explorer.tsx\nexports its file-row pieces, its folder walk is hoisted out of treeOf\nunchanged (folderNode, walkRows, plus fileTreeRows for a branch-rooted\ntree), and TreeItem may carry aria-selected. branchFolders.ts now tells\nthe core the union of every tree's open folders in the window, since\nfiles_watch replaces one set per window and the tab and the explorer\nwould otherwise unwatch each other.\n\nDecided in implementation:\n- D-1 The split is stored as the tree's share of the tab in whole\n  percent (held 10-70), on the record's view line as `split`, plus a\n  window-held map by view key so a closed tab reopens at its width; a\n  branch never moved starts where the last one was left in this window,\n  else 30%. Rejected: pixels (do not follow the window's size); one\n  global width in the layout file (the brief and the spec put it on the\n  tab); an opaque per-view state blob (nothing else needs one yet).\n- D-2 The divider is react-resizable-panels' Separator, as the window's\n  regions and pane splits are: pointer, arrows, Home/End and\n  role=separator with aria-valuenow come with it. Rejected: a hand-made\n  splitter.\n- D-3 Images are decoded in the window with createImageBitmap into a\n  canvas. The CSP gives images no source but the app's own, and this adds\n  none: no data:, blob: or asset protocol. An animated image shows its\n  first frame; an SVG is text and previews as text. Rejected: widening\n  img-src (a security-boundary change for a preview).\n- D-4 An image is known only by its signature bytes; a name never makes\n  one. Raster kinds the web view decodes: PNG, JPEG, GIF, WebP.\n- D-5 Markdown (.md, .markdown) renders by default with react-markdown,\n  skipHtml, http(s)-only links (ReleaseNotes' ExternalLink), and images\n  drawn as their alt words, so a rendered file fetches nothing. \"Source\"\n  (aria-pressed) shows the text in the light editor. A file's own tab\n  opened at a line (a jump) starts on the source.\n- D-6 2 MiB is 2 * 1024 * 1024 bytes and applies to text and images, in\n  both tabs (V86 F4), replacing RC-5's 5 MiB.\n- D-7 The record keeps a `piece-files` key only when it is exactly three\n  names charter would mint (the piece may be empty), for charter's own\n  view; every other view still takes one name or none. A split outside\n  1-99 is forgotten, never a reason to drop the tab. Single-file tabs\n  (key holds a path) are still not kept, as before.\n- D-8 The flat list's type-to-narrow box goes with it: a lazy tree has no\n  whole list to narrow. Finding a file by name is FM-7's ⌘P and the\n  sidebar's type-to-filter is FM-4's.\n- D-9 PieceFiles.test.tsx's list tests were rewritten for the tree and\n  its too-large wording follows the 2 MiB cap; the single-file and\n  your-editor tests are otherwise unchanged (#988's \"unchanged\" is\n  superseded by the list becoming a tree).\n\nReview round (dispatcher decisions F1-F5):\n- D-10 Decoded size is bounded, not just bytes: core reads an image's\n  declared width and height from its header with the `imagesize` crate\n  (MIT, header-only, no dependencies; cargo deny passes) and answers an\n  image past 40 megapixels as `HugeImage` (type and size, no bytes),\n  which the preview says in a sentence. A signature whose header gives no\n  size is answered as binary, never handed to the window to decode.\n  40 MP is about 320 MB decoded at four bytes a pixel, a phone photo\n  still fits. Rejected: hand-parsing four formats; a cap on one side.\n- D-11 The flat list's `piece_files` command is removed from the IPC\n  allow-list and both generated clients; `files::list` stays in core,\n  where `open` still uses it.\n- D-12 One navigation guard for every webview (new `navguard.rs`): a\n  plugin `on_navigation` allows only the app's own origin\n  (`tauri://localhost`, `http(s)://tauri.localhost`, and in a dev build\n  the configured devUrl's origin), and `on_new_window` denies every\n  new-window request on both windows charter builds (`lib.rs`,\n  `windows.rs`). Links still open through the opener plugin. The\n  predicate is unit-tested; that WebKit/WebView2 route a middle click or\n  `window.open` through these hooks is Tauri's and not measured here.\n- docs/plane-format.md gains the `views[]` rows of `app/reopen.json`,\n  with `key`'s piece-files exception and `split`'s 1-99 range.\n\nTests: three core tests in any_file_in_a_piece_opens_in_the_light_editor.rs\n(an image known by its bytes; an image declaring a huge canvas is said\nby its size; the 2 MiB edge for text and images); navguard unit tests; two\nreopen unit tests (the file tab and its split come back; a bad key or a\nwild split); an app unit test that an image crosses as its type and\nbase64; tabs.test.ts (the split is kept across a close, comes back with a\nlaunch); ViewTabs.test.tsx (a relaunch hands the split back to the\nrecord); PieceFiles.test.tsx (the tree, no cap at 600 entries, arrows,\nOpen in a tab of its own, and the markdown, image, huge-image, binary and\nover-2-MiB previews); workspace-explorer.e2e.ts opens the fixture branch's file\ntab, opens a file, drags the divider and moves it with a key, checks the\nrecord, opens the tab twice and keeps one, and closes and reopens it at\nthe same width. FM-1's e2e now sees README.md rendered.\n\nLeftover: #1132 (FM-2 follow-ups).\n\nCloses #1105\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-10-03T21:19:16+04:00",
          "tree_id": "c6da2c54cc6b2e80ab36fa8078a13b5a29ecf71e",
          "url": "https://github.com/diazoxide/charter/commit/e396e568fb8747269cb79e6942524cbf7cf7e724"
        },
        "date": 1791048078313,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.597519,
            "unit": "ms",
            "extra": "median of 5 runs: 0.595, 0.597, 0.598, 0.606, 0.610 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 17.154092999999996,
            "unit": "ms",
            "extra": "median of 5 runs: 16.564, 16.777, 17.154, 17.311, 17.535 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 105.50796550000001,
            "unit": "ms",
            "extra": "median of 5 runs: 104.396, 105.163, 105.508, 105.576, 106.708 ms"
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
          "id": "16d6d40f77572b56d66bc9cbe8ce971e7056ab53",
          "message": "FM-7: ⌘P finds a file by fuzzy name\n\n⌘P (Ctrl+Shift+P off a Mac) finds files as well as projects. Once something\nis typed, a \"Files\" group follows the projects in the switcher: each file with\nits folder, branch and project, and Enter or a click opens it in its file tab\n(the explorer's `pieceFileView`, so FM-2's file tab picks it up in one place).\nThe scope is said above the group and follows focus: the branch picked in the\nexplorer, else the project in front; Tab widens it to the project and then,\nwhile more than one is open, to every open project, Shift+Tab narrows. The\nwider rungs keep the focus as their order: the picked branch leads the\nproject, and the project in front leads every open project, so of two equal\nhits the nearer comes first.\n\nCore: `files::find(scope, query, most)` and `files::Finder` in their own\nsubmodule, `crates/charter-core/src/files/find.rs`. A scope is a list of\n`Place { plane, branch }`; `files::branches(plane)` is every branch of a project\n(each repo's own folder, then its pieces). Names come from the branch's\noffered list (`ls-files --cached --others --exclude-standard`, the light\neditor's own), so ignored files (a secret file git ignores among them) and\n`.git` are never found; a file git does not ignore is offered, as the light\neditor already offers it. A hit is checked as it is shown: a plain file, or a link to\nanother offered file of the same branch; never a link out, a link to an\nignored file, a submodule, a nested repository or a file gone since the\nlisting. A branch that cannot be listed is answered in the core's sentence\nand the rest still answer. A branch is listed up to `files::LISTED`\n(200,000 files) and the answer says so past it. Each keystroke selects the\nbest `2 x most` matches in linear time and sorts only those.\n\nApp: `find_files(session, scope, query)` and `find_files_end(session)` in\n`findfiles.rs`. `FileScope::Project` carries the focused branch (`near`),\nand `FileScope::OpenProjects` the project in front and that branch; they\nonly order the scope. The window names projects by `PlaneId` (vouched by the\nregistry) and branches by name; \"all open projects\" is the registry's own\n`open_now()`. One `Finder` per window per palette session, dropped when the\npalette closes or the window is destroyed; a find that arrives after its\nsession ended keeps nothing. Session ids are seeded from the load time, so a\nreloaded window never reuses one the core still holds. The project in front reports its\npicked branch (`PlaneReport.branch`); a file in another project brings that\nproject forward and opens its tab there (`fileAsked`, settingsAsk's shape).\n\nDecided in implementation:\n- D-1 Matcher: `nucleo-matcher` 0.3 (helix's; fzf's algorithm with path\n  boundary bonuses via `match_paths`, no allocation per match). MPL-2.0, on\n  deny.toml's allow-list; its deps memchr and unicode-segmentation were\n  already in the lockfile, so one crate is added. `cargo deny check licenses\n  bans sources` passes. Rejected: `fuzzy-matcher` (MIT), skim's older\n  matcher, with no release in years and no path-boundary configuration.\n  Patterns use `Pattern::new(.., AtomKind::Fuzzy)`, so `!`, `^`, `'` and `$`\n  in a typed name are letters, not fzf syntax; words split on spaces.\n- D-2 ⌘P was already the project switcher (FR-27). V86 F10 (operator) gives\n  ⌘P to file search, so the two share the one surface: projects first, files\n  as their own group after them, never ranked together (ADR 0079's rule).\n  This replaces D-FR27a's \"the key does nothing with one project open\": with\n  files to find, the key opens whenever a project is open, and a lone project's\n  own row (which could only say it is in front) is not listed.\n  `Projects.test.tsx`'s one-project test now asserts that. Rejected: a second\n  key for files, which would leave F10 unmet; moving the switcher off ⌘P, which\n  amends FR-27.\n- D-3 Scope ladder: branch (the explorer's picked spot in the focused\n  workspace, a piece or a repo's own folder) → project (every branch of every\n  workspace, the picked branch first) → every open project (the one in front\n  first). A missing rung is skipped, and \"every open project\" is skipped\n  while only one is open. Tab cycles\n  round, Shift+Tab goes back. The cockpit's branch (FM-5) is not built yet;\n  it feeds the same `PlaneReport.branch` when it lands (noted on #1108).\n- D-4 Performance: cached per palette session, not streamed. The palette asks\n  with an empty query as it opens, which lists the scope and finds nothing,\n  so the first keystroke pays only for its match. Link checks run only on the\n  hits shown (at most 50), not on every match. Ties go to the branch nearer\n  the front of the scope, then the shorter path. A branch is listed up to\n  200,000 files (about 18 MB of names while the palette is up), said past it.\n- D-5 Only files are hits; a folder is not (submodule, nested repo).\n- D-6 Files only in the ⌘P surface; ⌘K stays for actions (dispatcher).\n\nMeasured, release build, macOS, on a branch of 100,000 tracked files\n(`measured_on_a_hundred_thousand_files`, ignored, run by hand):\n- The core match per keystroke, the core's half of ADR 0086 L1 (50 ms\n  keystroke to screen): at worst 3.5 / 3.6 / 4.0 ms over three runs at load\n  54-59, with the top-k selection (11.2 ms with a full sort, at load 8-10).\n  The window's half (IPC and draw) is not measured here; it stays open on\n  FM-12 (#1115).\n- The first find of a session (git listing + match): 347 / 353 / 418 ms at\n  load 54-59 (134 / 174 / 487 ms at load 8-10). It is past ADR 0079's 100 ms\n  project-search figure, which is why the palette asks it as it opens and not\n  on the first keystroke.\n\nTests: crates/charter-core/tests/a_file_is_found_by_fuzzy_name.rs (11 on real\ngit fixtures: a fuzzy fragment ranks the file first; ignored files and a\nsecret file git ignores never found; links found only to another file of the\nbranch; nothing inside a submodule or nested repo; several branches say which\nbranch each hit is in, nearest first; a project's branches; a finder lists\neach branch once per session; an unlistable branch is said and the rest\nanswer; a branch past the listing cap is searched up to it and says so; the\nbest hits are kept when far more match than are shown; an empty query finds\nnothing and the most is kept; plus the ignored 100k measurement). App unit\ntests in findfiles.rs (a hit names its project and branch in any scope; the\nfocused branch's copy leads the project scope; the project in front and its\nfocused branch lead every open project; a find after its session ended keeps\nnothing; sessions). src/Palette.files.test.tsx (mockIPC: the two groups and\narrows across them, where each file is, the scope and Tab / Shift+Tab, no\n\"every open project\" rung with one project open, nothing matching and a\nrefused branch, a branch listed in part, a lone project).\nworkspace-explorer.e2e.ts: pick the fixture branch, ⌘P, the scope (found by\nits role and name) says the branch, type a fuzzy fragment, the file's row\nsays its folder and branch, Tab to the project, where the file still leads,\nEnter opens it in its file tab.\n\nCloses #1110\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-10-03T22:58:59+04:00",
          "tree_id": "65fd339b135d297be43e1f6b3d6bebfbf8bbbb13",
          "url": "https://github.com/diazoxide/charter/commit/16d6d40f77572b56d66bc9cbe8ce971e7056ab53"
        },
        "date": 1791054022915,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.479157,
            "unit": "ms",
            "extra": "median of 5 runs: 0.475, 0.477, 0.479, 0.480, 0.483 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.2933315,
            "unit": "ms",
            "extra": "median of 5 runs: 16.200, 16.204, 16.293, 16.543, 16.818 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 100.9315075,
            "unit": "ms",
            "extra": "median of 5 runs: 100.495, 100.510, 100.932, 101.212, 102.334 ms"
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
          "id": "390ba08b0b63db8f9de4848d698cad11a00e1f27",
          "message": "HP-19: what each harness can do here, on its own card\n\nA harness's capability card, read off the real sources: its declaration\n(FD-14: [levels], [capabilities], [terminal] ready_to_type) and, for a\nbuilt-in, the adapter charter ships for it (whether charter can sandbox\nit). charter_core::harness_card is neutral: no harness is named in it, and\na harness a project declares gets a card by being declared.\n\n- The card is a view tab, `harness/<name>`, answered by the core in the\n  panel vocabulary and labelled \"What Codex can do here\": a row per line,\n  yes / no with the declaration's reason / not declared.\n- A chat's header names the product it runs; pressing it opens the card.\n- The picker shows the picked harness's card: its label and a line for\n  each thing it lacks.\n- A control that is off because the harness lacks something says the\n  card's line for it and the card's label: the Curate rows (palette and\n  menus) and the first task's rows, which used to name Claude Code and\n  Codex by hand.\n\nReview fixes (HP-19 review of 0e9006b):\n- M1: a harness a project declares has no adapter, so every line charter\n  acts on only through one (hooks, what they report, per-chat plugins,\n  typing a prompt in, the sandbox) answers No(\"charter does this only for\n  a harness it ships an adapter for\"), whatever the declaration says. It\n  keeps what charter does from a declaration alone: resuming, its ACP\n  agent. The curation and first-task refusals for a non-built-in now say\n  the card's line too.\n- M2: a declaration's `title` (<= 60), `tested` (<= 80) and a `no:` reason\n  (<= 200) are refused at parse time unless one line with no control, bidi\n  or zero-width character, the file named; a project title may not be a\n  built-in's. The card also clips them with shown::one_line.\n  docs/plane-format.md records the limits.\n- F1: OpenChat.card and ProfileRow.harness are plain nullable fields, so\n  bindings.ts has no _Serialize/_Deserialize split for them.\n- F2: the first-hour word test covers lines() and lacks() for every\n  built-in and two declared harnesses; ready_to_type's reason is plain.\n\nTests: charter-core harness_card (23: one per adapter-only line, every\ncapability charter reads has a line, every line and off-control sentence\nkeeps to the first hour's words, the card clips) and harness_declaration\n(4: title, tested and a reason refused unless one short plain line, a\nbuilt-in's title); charter-app views (the card tab, a harness gone),\ncuration and first task (the off reason is the card's line, built-in and\ndeclared); vitest HarnessCard.test.tsx (picker, header),\nFirstTask.test.tsx (the off row's tooltip), theme/views.test.tsx (the card\ntab's two render states); e2e harness-card.e2e.ts (header to card tab, and\nan off Curate row saying opencode's line).\n\nDecided in implementation:\n- D-HP19-1 Words: ADR 0072 §3 / ADR 0073 §6 rule the card's words, so the\n  label is \"What <product> can do here\", each \"no\" is one plain line, and\n  \"level\", \"harness\" and \"capability\" stay in code. The levels are on the\n  card as two lines (\"Tells charter whether it is working, waiting or\n  done\", \"Works without a terminal, through its ACP agent\"). A core test\n  keeps every line off the first-hour word list. Rejected: showing \"L2\" /\n  \"level 3\", which the ADRs keep off the card.\n- D-HP19-2 The card tab is a built-in view answered by `open_view` in panel\n  blocks, like the persona and changes views, not a bespoke component and\n  not a new command. Rejected: a dialog (operator rule: tabs hold views).\n- D-HP19-3 The window gets the card at a glance on the answers it already\n  has: `ProfileRow.harness` and `OpenChat.card` (`HarnessGlance`: name,\n  title, label, lines, cannot_type), null for a shell or a kind with no\n  declaration. Rejected: a new `harness_cards` command the picker and\n  header would each have to ask.\n- D-HP19-4 Sandbox comes from `sandbox::compiler(harness)`, the adapter's\n  answer, so #1123 (Codex) and SD-2 change the card without touching it.\n- D-HP19-7 (review S2) A project's title equal to a built-in's, ignoring\n  case, is refused: a card and picker naming it would read as that harness.\n- D-HP19-5 Off-control reason = the card's line + \"See What X can do\n  here.\" as text; a link is in the follow-up (#1134).\n- D-HP19-6 The curation and first-task refusals now read the card's line\n  instead of naming Claude Code / Codex by hand (harness-agnostic).\n\n- D-HP19-8 (train 11) The header button is the pane's first Tab stop: it is\n  drawn in the top-left corner, before split and end at the top right, so\n  the order is reading order and the product stands. keyboard-reach's\n  \"controls first\" now means everything the pane's corners draw, left\n  then right, before the terminal; Window.keyboard.test.tsx lists the\n  button before Split right.\n\nFollow-ups: #1134 (one-time notes at a fallback's point of action, the\nlabel as a link, unreported from the card's lines, a header button for a\ndeclared harness's chat, palette rows per harness).\n\nCloses #677\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-10-04T00:20:02+04:00",
          "tree_id": "1ae5ba27db8e6b10d72b4d8694c30d5e2b41fc22",
          "url": "https://github.com/diazoxide/charter/commit/390ba08b0b63db8f9de4848d698cad11a00e1f27"
        },
        "date": 1791058867756,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.5998695,
            "unit": "ms",
            "extra": "median of 5 runs: 0.577, 0.582, 0.600, 0.601, 0.605 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 17.040411499999998,
            "unit": "ms",
            "extra": "median of 5 runs: 16.701, 16.993, 17.040, 17.259, 17.418 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 105.8035555,
            "unit": "ms",
            "extra": "median of 5 runs: 104.727, 105.176, 105.804, 106.094, 106.410 ms"
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
          "id": "ba656b3531dbef43beaee920d814fc43afa40969",
          "message": "FM-8: ⌘⇧F searches the content of the files in a Search view tab\n\nA content search over a branch, a workspace, a project or every project open in\ncharter (#1103, V86 F9/F10). The core engine is `charter_core::files::search`\n(files/search.rs): the `ignore` crate walks each branch in-process and\n`grep-regex`/`grep-searcher` match line by line, with literal or regex, match\ncase and whole word. `search_files` streams the hits to the window that asked\nas `files-searched` events, tagged with the tab's id and run. A page stops at a\ncap, and \"Show more\" continues the same walk. A new query stops the old one.\n\n⌘⇧F (Ctrl+Shift+F off a Mac, outside a chat) opens a Search view tab keyed by\nscope and query. Hits are grouped project → branch → file, with their matching\nlines and counts. ↓/↑/Home/End step through the hits, and Enter opens one in\nthe branch's file tab at its line, through a new `fileJump.ts` that #984 can\nreuse.\n\nDecided in implementation (operator offline; recommended option taken):\n- D-FM8-1 FM-8 sits on main after FM-7 (train 10) and trains with FM-4. It reuses FM-7's\n  FileScope (which gains a `workspace` variant), `files::{Place, Named,\n  branches}`, the nearest-first ordering (now `findfiles::branches_in`), the\n  registry check (now `findfiles::projects_of`, shared by find_files and\n  search_files), and the App → PlaneView file-ask plumbing (with an optional\n  line).\n- D-FM8-2 git's offered list is the only rule for what is read. The `ignore`\n  walk reads no rule file of its own (git_ignore, git_exclude, git_global and\n  ignore all off). An agent can plant any of those files as a link to a device\n  or a FIFO, and the crate would follow it. The walk descends only into folders\n  that hold an offered path (a range check on the sorted list), so it opens\n  nothing but folders and never walks target/ or node_modules/. The list is\n  one `git ls-files` per branch: operator-initiated git, which stays on the\n  binary (V88a), through the hardened runner (FM-4, in the same train, sets\n  GIT_NO_LAZY_FETCH on every call it makes). The search also never reads:\n  - vault or charter-state paths (the leak guard's one pattern);\n  - credential names (`reposave::secret_name` plus .npmrc/.pypirc, even when\n    committed);\n  - binary files or images (the preview's own sniff);\n  - files past 2 MiB;\n  - links, .git, or nested repositories.\n  A tracked file under an ignore pattern is now searched, as the light editor\n  opens it.\n- D-FM8-3 The branch's folder is opened once from the project's root, one\n  component at a time with O_NOFOLLOW (`files::hold_folder`), and held in the\n  walk. Every file is opened relative to that handle, one component at a time\n  with O_NOFOLLOW, and O_NONBLOCK for the file itself (`files::open_inside`,\n  FM-1's openat walk). A folder swapped for a link mid-search is refused, not\n  followed, whether it is inside the branch or an ancestor of it such as\n  `.worktrees/<repo>` (D-88i). This replaces `contain::open_no_link` here.\n- D-FM8-4 Stop and the page's time are heard inside a file. The matcher reads\n  through `search_reader` from a reader that hands it 16 KiB a fill and\n  checks both before each fill. `heap_limit` is 256 KiB, and a file with a\n  longer line is reported as not searched. A file the page's time runs out in\n  is reported as not searched, and Show more carries on after it. Measured\n  (debug): a 2 MiB file of a pattern that defeats the lazy DFA takes 8 s\n  whole, and a stop raised at 0.5 s now ends the page in well under 1 s.\n- D-FM8-5 A sequential, name-sorted walk, not ignore's parallel walker. It\n  gives a stable order for grouping and a walk Show more can resume. FM-12\n  (#1115) measures whether more is needed.\n- D-FM8-6 Bounds:\n  - 200 matching lines a page, counted by whole files;\n  - 100 lines shown per file, with the full count;\n  - 240 characters of a line, around its first match;\n  - 10 s a page;\n  - a query of at most 1000 characters, on one line;\n  - regex size_limit 1 MiB and dfa_size_limit 8 MiB.\n- D-FM8-7 Hits stream as events sent to the asking window (emit_to), not a\n  Tauri Channel, because channel commands are excluded from the UI RPC client.\n  A new query from a tab is asked without an `end` first: asking stops the\n  tab's last run in the core, so an end and a begin can never arrive out of\n  order. A run that is not the tab's newest is never kept. A window keeps at\n  most 8 searches.\n- D-FM8-8 A hit opens the branch's file tab (FM-2) with the preview on the\n  file at the line. A markdown file shows its source. A hit in another project\n  brings that project forward. ⌘P still opens a file in a tab of its own.\n- D-FM8-9 The key is ⌘⇧F on a Mac. Off a Mac it is Ctrl+Shift+F, except while\n  a chat has the keyboard, where that chord stays the chat's find bar. It takes\n  no byte from a chat (docs/ui-primitives.md).\n- D-FM8-10 A Search tab's key is `kind|workspace|branch|flags|query`. It\n  follows the query as the query settles (250 ms), and it is never written to\n  the reopen record, because the query is the operator's own text.\n\nTests:\n- core: crates/charter-core/tests/content_is_searched_across_branches.rs (23\n  tests): literal, regex (and a refused one), case, whole word; never ignored,\n  link, vault, credential, binary, image, past 2 MiB, .git or a nested repo;\n  a tracked file under an ignore pattern is searched; an ignore file planted\n  as a link to /dev/zero or a FIFO is never opened; a stop raised inside a\n  slow file ends the page in about a second, and so does the page's time;\n  a line past 256 KiB is said; a folder swapped for a link is refused, inside\n  the branch or above it;\n  streaming, the page cap and resuming, per-file and per-line caps; a\n  multi-project scope; a refused branch; bad queries.\n- app: searchfiles.rs unit tests (open projects come from the registry only,\n  the workspace scope, paging, a new run stopping the old, a late run not kept,\n  the window cap, a file not searched said with its branch, a refused query).\n- vitest: SearchTab.test.tsx (render states; a re-ask sends no end first) and theme/views.test.tsx (two\n  Search tab states).\n- e2e: workspace-explorer.e2e.ts, where ⌘⇧F finds a fixture string and opens\n  it at line 5 in the file tab.\n\nLeftovers: #1137 (FM-8 follow-ups).\n\nCloses #1111\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-10-04T02:36:04+04:00",
          "tree_id": "9f0677baa85857669f5a09d824a294f2f8e8c9e6",
          "url": "https://github.com/diazoxide/charter/commit/ba656b3531dbef43beaee920d814fc43afa40969"
        },
        "date": 1791067055542,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.5843700000000001,
            "unit": "ms",
            "extra": "median of 5 runs: 0.582, 0.583, 0.584, 0.587, 0.594 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.8565885,
            "unit": "ms",
            "extra": "median of 5 runs: 16.686, 16.832, 16.857, 17.236, 17.382 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 105.433087,
            "unit": "ms",
            "extra": "median of 5 runs: 103.935, 104.237, 105.433, 105.619, 106.473 ms"
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
          "id": "0393f3736f476a866acff3c7ff1712e91f39c663",
          "message": "SD-2: suggest only harnesses this machine can sandbox; portable program tests\n\nA refusal that suggests another harness (\"Start this chat on a … profile\") listed every\nharness with a sandbox compiler, so it still named Codex, which a sandboxed project refuses\nuntil #1123. It now names only the harnesses `never_on` lets start sandboxed on this system.\n\nTwo program-check tests used an opencode profile, which charter wraps on macOS only, so on\nLinux the \"cannot wrap opencode here\" refusal came first and they failed in CI. They now use a\nClaude Code profile and test the same rule on every system.\n\nRefs #695\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-10-04T03:59:43+04:00",
          "tree_id": "a130d0b543856e86e1d358377503a6b2c26c01b2",
          "url": "https://github.com/diazoxide/charter/commit/0393f3736f476a866acff3c7ff1712e91f39c663"
        },
        "date": 1791072076903,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.607289,
            "unit": "ms",
            "extra": "median of 5 runs: 0.575, 0.582, 0.607, 0.612, 0.621 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 17.3171395,
            "unit": "ms",
            "extra": "median of 5 runs: 17.161, 17.281, 17.317, 17.358, 17.358 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 105.86770100000001,
            "unit": "ms",
            "extra": "median of 5 runs: 104.663, 105.631, 105.868, 106.113, 108.858 ms"
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
          "id": "53f0e4e505caf8667f9e8078f55fa04444eb787f",
          "message": "HP-6: answer a Claude Code permission prompt from the needs-you list\n\nA Claude Code chat's `PermissionRequest` hook now runs `charter hook\npermissionrequest`. The hook hands its payload to the project's hook channel\nand waits there. The host raises an ask in the shared HP-5 registry, and the\nwindow's ✋ list shows it with the options Claude Code offered. The operator's\nchoice goes back on that same connection, and the hook prints Claude Code's\nown decision, so the chat carries on without its pane having focus. If nobody\nanswers, or the hook goes away, nothing is printed and the pane's own prompt\ndecides. charter never allows or denies by itself.\n\n- same-user: `peer_process_of`, the shared ancestry walker\n  (`ancestry.rs`) and `admit_host`. The permission hook writes nothing to a\n  listener unless it is its own ancestor and holds the socket.\n- core: `harness::hooked` reads the hook's ask (with V28c applied),\n  renders Claude Code's decision, and holds open hook asks (`HookAsks`, capped\n  at 32 per chat and 64 KiB). `hookwire::permission` is the one hook line\n  that waits.\n- `Asks::answer` now takes the chat. An answer naming another chat is\n  refused as unknown. ACP's `Chat::answer` uses it too.\n- Claude Code adapter: the permission hook is armed in the session's\n  `--settings`, never in the plugin's `hooks.json`.\n- app: `asking.rs` adds `pending_asks`, `answer_ask` (local-ui) and the\n  `asks-changed` event. NeedsYou draws each ask with its options.\n- session protocol: `answer` is refused (`not_allowed`) on every link.\n  `ui::WINDOW_ONLY` keeps `answer_ask` off the UI RPC, and the generated\n  `uiRpc.ts` leaves it out.\n\nDecided in implementation:\n- D-HP6-1 Who answers. Only the window, through the Tauri IPC, admitted as\n  local-ui. Rejected: answering over the link or `charter inbox` (approval).\n  FD-27 (#664) has not built the scope check, and a link's local-ui\n  credential is a file a chat can still read (V16a). So the session\n  protocol's `answer` is refused for every host, and `answer_ask` is in\n  `ui::WINDOW_ONLY`. This tightens a fail-closed default and is re-opened by\n  FD-27.\n- D-HP6-2 Replay and misroute. `Asks::answer(chat, id, ..)` is atomic. A\n  mismatched chat reads as Unknown, open or closed, so it reveals nothing.\n  First-answer-wins makes a replay \"answered elsewhere\". Rejected: checking\n  `chat_of` and then answering in two locks (ACP's old shape).\n- D-HP6-3 Arming. The hook goes in `--settings` (argv), not the bundled\n  plugin, because a hook that can allow must never come from a file a chat\n  can write. `plugin::tests` still holds that.\n- D-HP6-4 Trust. The host parses the payload with its own hook timeout, so\n  the chat never sets the deadline or the options. The hook re-derives the\n  options and prints only one it offers itself. The reply carries an option\n  id, never a decision.\n- D-HP6-5 V28c. \"From now on\" options (a suggestion saved to a settings\n  file) are dropped from hook asks, so they can be neither shown nor\n  answered. Session-scoped rules stay.\n- D-HP6-6 Timeout. 60 s, Claude Code's default for a hook. The deadline sits\n  2 s below it and the hook waits 59 s. Measured on Claude Code 2.1.288: the\n  pane draws its own prompt while the hook waits, so the wait costs the pane\n  nothing, and either place may answer.\n- D-HP6-7 Scope. Claude Code at level 2 only. Codex's hook decision output is\n  unmeasured, so its hook stays unarmed. The level-1 pane snapshot and typed\n  reply, the e2e, and the audit actions are in #1146. ACP and the Codex\n  app-server are HP-16 (#673).\n- D-HP6-9 (D-88n, dispatcher; review must-fix) The hook authenticates the\n  host before writing anything: the listener's uid must be its own and its pid\n  one of the hook's ancestors (`charter_same_user::admit_host`, new\n  `peer_process_of` and `ancestors`). No env/argv secret, since the chat reads\n  both. Otherwise it prints nothing and the pane decides. ADR 0068 gains the\n  HP-6 amendment: the hook socket's owner stays the chats' ancestor under\n  charterd. The sandbox's integrity denial stays as the second layer.\n  Ancestry is read from /proc on Linux and from /bin/ps by absolute path on\n  macOS, since the workspace denies `unsafe`.\n- D-HP6-10 (review must-fix) Allow only on a whole display. `hooked::ask`\n  drops every allowing option unless the summary is the action word for word\n  (`shown_in_full`): no newline collapsed, no cut at 200, no mask. An edit,\n  which shows only its path, is never allowed from the window. The row then\n  offers Deny and \"Open in its pane\". It is enforced in the core, and the hook\n  re-derives the same options, so an allow cannot be sent for such an ask.\n- D-HP6-11 (rounds 2 and 3, R2-M1/R3-M1; dispatcher tightening of D-88n)\n  The host is the ancestor that holds the socket. A peer pid is a stale\n  number on both kernels: Linux SO_PEERCRED keeps the pid of a listener that\n  exited while a child kept the socket (measured), and macOS LOCAL_PEERPID is a\n  sample. The hook admits the peer only if its pid is a same-uid ancestor AND\n  that process holds a socket bound at the hook path now, with paths compared\n  once resolved (/tmp == /private/tmp):\n  - Linux: a listening /proc/net/unix entry whose inode is in /proc/<pid>/fd.\n  - macOS: /usr/sbin/lsof -nP -a -p <pid> -U -F n, by absolute path with no\n    environment. Chosen over the `libproc` crate, which only moves the\n    `unsafe` into a new dependency.\n  No timestamps: round 2's birth-time guard was forgeable (a symlink,\n  setattrlist, a swap) and is removed, along with the self shortcut and\n  PEER_PID_IS_PINNED. Any error fails closed. ADR 0068's HP-6 amendment says\n  so.\n- D-HP6-12 (round 2, R2-M2) Allow also needs a plain shell input (only\n  command, description and timeout, so `dangerouslyDisableSandbox` and\n  `run_in_background` lose it) and no character drawn otherwise. That is a\n  control, general category Cf, or another Default_Ignorable_Code_Point. Cf\n  comes from the `unicode-properties` crate (MIT/Apache-2.0, unicode-rs,\n  general-category only), not a hand list. A suggestion whose own label (rule\n  text, directory path) holds such a character is not offered.\n- D-HP6-13 (D-88o, dispatcher) The window never offers a permission-mode\n  switch (any `setMode` suggestion). It is dropped in the core like V28c's\n  \"from now on\", so the hook cannot print one either.\n- D-HP6-14 One ancestry walker: `charter_same_user::{Parents, walk}`.\n  `charter_core::process::descends_from` uses it, under the core's fork lock.\n- D-HP6-15 `ask_permission_of_an_admitted_host` (the in-process test seam)\n  is built only under cfg(test) or the new `test-support` feature, which only\n  dev-dependencies turn on.\n- D-HP6-16 The permission row stacks its line above its options and wraps\n  anywhere, so a long token can't push the command off the menu.\n- D-HP6-8 No e2e. A scenario harness would need to run the permission hook.\n  The CLI test drives the real binary over a real socket, the app test\n  drives `Hooks` end to end, and vitest drives the list.\n\nCloses #672\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-10-04T05:39:50+04:00",
          "tree_id": "67fd4492d95c4568550a3148f9d82c0dfec92705",
          "url": "https://github.com/diazoxide/charter/commit/53f0e4e505caf8667f9e8078f55fa04444eb787f"
        },
        "date": 1791078071735,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.383393,
            "unit": "ms",
            "extra": "median of 5 runs: 0.379, 0.381, 0.383, 0.389, 0.396 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.4647565,
            "unit": "ms",
            "extra": "median of 5 runs: 16.358, 16.413, 16.465, 16.488, 16.539 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 101.213261,
            "unit": "ms",
            "extra": "median of 5 runs: 100.520, 100.617, 101.213, 101.249, 101.599 ms"
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
          "id": "de0610e550a425ceccf93329302e8c59ce17be58",
          "message": "FM-9: files into chats, typed as each harness's reference and never sent\n\nA file, a folder, a range of lines or a search hit reaches a chat without anyone typing a path.\n\n- Neutral model, `charter_core::reference`. A reference is a path, an optional line range and\n  whether it is a folder. It is made only by `reference::of`, from a path that `files::place`\n  confined: inside the branch, no `..`, no link, not `.git`. Its text is built in core.\n- `HarnessAdapter::reference` renders it in each harness's own syntax:\n  - Claude Code: `@src/main.rs`, `@src/`, `@src/main.rs#L10-20`, and `@\"…\"` for a name with\n    white space.\n  - Codex: `src/main.rs:10-20`, with double quotes for white space.\n  - opencode: `@src/main.rs#10-20` and `@src/`.\n- Typing goes through the curation path (ADR 0061): one bracketed paste with nothing after it,\n  sent as the operator's input. A running chat is typed into only when it is at its prompt\n  (`reference::may_type_into`). Otherwise, and always on opencode, the reference goes on the\n  clipboard and the window says why.\n- Three ways in:\n  - drag a file or folder row, a search hit, or the preview's selection handle onto a chat's tab\n    or its pane;\n  - \"Ask a chat about this\" and \"Add to a chat's context\" in the preview;\n  - \"Start a chat here\" on every file and folder row (`start_chat_here`), which opens a chat on\n    the project's default profile in the branch's folder, with the reference held as its first\n    prompt.\n\nMeasured (D-FM9-1):\n- Claude Code 2.1.288: read from the mention parser in its own bundle. A mention is `@` at the\n  start or after white space, then `\"…\"` or non-space ending on a word character. `#L<a>` or\n  `#L<a>-<b>` after the name names lines.\n- codex-cli 0.147.0: drove its `@` file search in a pty, with a scratch CODEX_HOME and a\n  stand-in provider. It inserts the relative path, a folder without a trailing `/`, and quotes a\n  path with white space. It attaches nothing, so lines use its own `path:line` vocabulary.\n- opencode 1.18.33: drove its `@` completion in a pty with scratch XDG directories, and read its\n  bundle's completion code. It writes `@path`, `@dir/` and `@path#a-b`.\n\nDecided in implementation:\n- D-FM9-1: each harness's syntax, as measured above. A path that a harness's mention cannot\n  hold is handed over in plain words, which the agent reads with its own tools: a `#` for all\n  three, a `\"` that needs quoting for Claude Code, and white space for opencode. Rejected:\n  inventing an escape that no harness parses.\n- D-FM9-2: a reference is relative to the chat's own folder when the file is inside it, and\n  absolute otherwise, so it resolves for a chat on another branch too. Rejected: always\n  relative to the branch, which breaks for a chat elsewhere.\n- D-FM9-3, the idle check, is `reference::may_type_into`, decided from hooks and the kernel only.\n  A reference is typed only after a hook has said `Waiting`, the chat is not asking, and its\n  terminal is raw. It is never typed:\n  - mid-turn, while the chat asks something, or after it has ended;\n  - into a chat no hook has spoken for (`Unknown`, on any harness: a Codex chat with untrusted\n    hooks stays silent through an approval dialog that is raw and quiet like an input). This\n    tightens it, per review M2;\n  - into a chat whose curation or first-task prompt is still held;\n  - where the paste would be drawn as a placeholder.\n  Rejected: a quiet period (ADR 0061's Codex rule, kept for curation only), and reading the\n  screen.\n- D-FM9-4: a chat that is not at its prompt gets the reference on the clipboard with the reason,\n  not a refusal, so the gesture is never lost. Nothing is queued to type later, since late\n  typing lands in whatever the operator began.\n- D-FM9-5: what is typed is the reference and a trailing space inside one paste, so the\n  operator's words follow it. That holds for \"Start a chat here\" too: the held paste keeps its\n  space (`ChatTyped::then_a_space`, `Typed::hold_paste`). Control characters are stripped again\n  at the paste, and a path holding one (a newline is a legal file-name byte) is refused in core,\n  so nothing can submit. The paste markers and the bracketing are defined once, in\n  `charter_core::curation` (`PASTE_BEGINS`/`PASTE_ENDS`, `bracketed`,\n  `bracketed_then_a_space`), and curation, the first task and references all use them.\n- D-FM9-6: a reference goes through `Held::operator_input`. It is the operator's act, so it\n  cancels a smart close as typing does.\n- D-FM9-7: \"Start a chat here\" uses the project's default profile, as a curation chat does\n  (`curation::default_profile` was extracted for it). It runs as no persona, in the branch's\n  folder, labelled `About <name>`. On a harness charter cannot type into, the chat still opens\n  and the reference is copied (`curation::start_typed_where_it_can_be`).\n- D-FM9-8: \"Ask\" types and brings the chat to the front; \"Add\" types and stays. Both are typed,\n  unsent.\n- D-FM9-9: a drag carries charter's own MIME type with the project in it. A drop from another\n  project is refused in the window, and the core places the path again whatever the window\n  sent.\n- D-FM9-10: \"Start a chat here\" joins FILE_VERBS. It changes nothing in the branch, so V86 F8\n  holds.\n- D-FM9-11: an ignored file can be referenced, since `files::place` places it. Dragging it is an\n  explicit act, and the agent can read its folder anyway.\n- D-FM9-13 (review M1, security): no reference begins with a character a harness reads as a\n  mode or a command. Measured: Claude Code 2.1.288 enters bash mode on a pasted leading `!`, and\n  codex-cli 0.147.0 runs a submitted `!…`. `reference::of` writes a relative path that does not\n  begin with a letter, a digit, `_` or `.` as `./<path>`. An absolute path is quoted (or written\n  `.//…` when it holds a quote) wherever an adapter would hand it over bare, so it is never read\n  as a `/` command. `reference::starts_safely` is the rule; each adapter's test and the core test\n  hold every rendering to it.\n- D-FM9-14 (review F1): a name holding a format, bidirectional, zero-width or other\n  default-ignorable character, or U+2028/U+2029, is refused in `of` and stripped in `pasted`.\n  `reference::not_typeable` reuses HP-6's `hooked::drawn_otherwise` and adds the two\n  separators.\n- D-FM9-12: the e2e uses a fake harness with a new `--raw` mode, which echoes bytes and answers\n  only on Enter. It runs on a Claude-kind profile that reports SessionStart through the real\n  `charter hook`.\n\nLeftovers: #1151 (keyboard path for rows and hits, opencode typing, multi-file drags,\nStart-here with lines, local macOS e2e, and the accepted residual that the check before typing\nis not atomic with the harness).\n\nCloses #1112\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-10-04T06:32:20+04:00",
          "tree_id": "02209006e7edb8c56ff64fae7dcd76ccec4c0f5d",
          "url": "https://github.com/diazoxide/charter/commit/de0610e550a425ceccf93329302e8c59ce17be58"
        },
        "date": 1791081213648,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.612085,
            "unit": "ms",
            "extra": "median of 5 runs: 0.599, 0.604, 0.612, 0.616, 0.618 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.98643,
            "unit": "ms",
            "extra": "median of 5 runs: 16.652, 16.659, 16.986, 17.124, 17.389 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 105.1376075,
            "unit": "ms",
            "extra": "median of 5 runs: 104.246, 104.682, 105.138, 105.485, 107.709 ms"
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
          "id": "ffba151e07b37cdf447cc954dcb5071e2f1a5066",
          "message": "FM-12: measure ⌘P, ⌘⇧F and status on 100,000- and 300,000-file repos\n\nAn ignored test, search_is_measured_on_large_repos.rs, generates a\nrealistic repo: nested packages, mixed kinds and sizes, about 6%\nbinaries, and ignored build output and dependencies. It times the core\ncalls the app makes:\n- ⌘P: a palette session's first find, and each keystroke;\n- ⌘⇧F: the first hit and a 200-line page, for common, rare and absent\n  queries, and a whole scan;\n- status: a read through the bounded reader child.\n\nIt holds the results to new budget rows G2–G4 in docs/spec.md:\n- a ⌘P keystroke within L1's 50 ms;\n- a first find within 3 s;\n- a common query's first hit within 1 s;\n- status within the reader's 30 s deadline.\n\nstress.yml's new `search at scale` job runs it at both sizes on\nmacOS and Ubuntu, as evidence on main and nightly only (V70).\n\nNumbers (macOS, loaded) are on #1115.\n- ⌘P meets L1 at both sizes: 25 ms at worst.\n- A common ⌘⇧F query's first hit takes 0.19 s at 100,000 files and\n  0.57 s at 300,000, warm.\n- A rare query, or one found nowhere, fills no 10 s page: the whole\n  scan takes 61–131 s, the speed this machine reads the files at all.\n\nNo index is built. That one case goes grill → ADR in M52 (#1153),\ntogether with a parallel walk, status at scale (about 4× git status\nat 300,000), and the Linux numbers, which the new job will post.\n\nRefs #1115\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-10-04T07:25:30+04:00",
          "tree_id": "cb3cafae6a9a6285036adb467894998602c79583",
          "url": "https://github.com/diazoxide/charter/commit/ffba151e07b37cdf447cc954dcb5071e2f1a5066"
        },
        "date": 1791084456691,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.5901860000000001,
            "unit": "ms",
            "extra": "median of 5 runs: 0.580, 0.587, 0.590, 0.597, 0.604 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.741025999999998,
            "unit": "ms",
            "extra": "median of 5 runs: 16.558, 16.711, 16.741, 17.198, 17.373 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 103.73469449999999,
            "unit": "ms",
            "extra": "median of 5 runs: 102.264, 103.300, 103.735, 105.377, 106.214 ms"
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
          "id": "4d007619f81b4566d6279a34df454ed3b18dc4d5",
          "message": "sandbox: let a killed probe's child be reaped before reading its group\n\nThe hang test read the probe's process group once, right after killing it.\nOn Linux a killed child stays a zombie in the group until init reaps it, so\nthe read could still find it and fail (train 17, ubuntu). Poll for up to\nfive seconds; a child that really survived the kill still fails the test.\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-10-04T08:25:57+04:00",
          "tree_id": "6d27fe6701186631d22dba53a5d988da8dbf2f61",
          "url": "https://github.com/diazoxide/charter/commit/4d007619f81b4566d6279a34df454ed3b18dc4d5"
        },
        "date": 1791088087003,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.589099,
            "unit": "ms",
            "extra": "median of 5 runs: 0.570, 0.576, 0.589, 0.596, 0.601 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 17.5034435,
            "unit": "ms",
            "extra": "median of 5 runs: 17.195, 17.432, 17.503, 17.664, 17.873 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 105.78711799999999,
            "unit": "ms",
            "extra": "median of 5 runs: 103.622, 105.619, 105.787, 106.020, 106.142 ms"
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
          "id": "d56da346bd376a99860c4be9f081d2280e8ef4b2",
          "message": "browser install: read a sandbox refusal off the failure, write nothing\n\nThe hint that a sandboxed chat may not write a project's skills was\nfound by a write probe: it made the skills folder and a file in it, and\ncould follow a link or leave .claude/ behind, in the very run the\noperator starts outside any sandbox.\n\nThe probe is gone. The hint is now read off the failure alone: the\ngenerator's output, or why it could not run, saying permission denied,\noperation not permitted, EPERM or EACCES. Any other failure gets no\nhint, and nothing is written to find out.\n\nCloses #1057\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-10-04T09:45:28+04:00",
          "tree_id": "fa0d13c971c7c9b63af5c378be6ba788daf21ff9",
          "url": "https://github.com/diazoxide/charter/commit/d56da346bd376a99860c4be9f081d2280e8ef4b2"
        },
        "date": 1791092850449,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.575094,
            "unit": "ms",
            "extra": "median of 5 runs: 0.570, 0.571, 0.575, 0.576, 0.589 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 17.3248925,
            "unit": "ms",
            "extra": "median of 5 runs: 16.986, 17.218, 17.325, 17.484, 17.539 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 106.17664500000001,
            "unit": "ms",
            "extra": "median of 5 runs: 105.155, 105.258, 106.177, 106.300, 106.928 ms"
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
          "id": "5de2ddaef221f35838614f4780bd1c13d945b77b",
          "message": "FG-4 re-review: catch the whole secrets context, pin the record's path\n\nFold-ins from the re-review of 8537772 (approved).\n\n- The guard test flags any use of the secrets context inside a `${{ }}` expression other than\n  exactly `secrets.FORGE_LIVE_TOKEN`: a bare `secrets` (`toJSON(secrets)` and the like sends the\n  whole context to the runner), another name, or an index. `toJSON(secrets)` in the test step's\n  env passed the guard before this commit and fails it now.\n- The ungated upload of what each forge tested is pinned to `path: ${{ runner.temp }}/outcome`,\n  so it can never upload the workspace. `path: .` passed before this commit and fails now.\n- docs/forges.md: create both environments and set their main-only deployment branch policy\n  before adding `FORGE_LIVE_TOKEN` (GitHub creates a missing environment unprotected the first\n  time a job names it), and add no required reviewers, which would hold every nightly run.\n\nRefs #712\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-10-04T10:36:05+04:00",
          "tree_id": "13a5bf2963ee2f31c91662ffe872e5428f58de96",
          "url": "https://github.com/diazoxide/charter/commit/5de2ddaef221f35838614f4780bd1c13d945b77b"
        },
        "date": 1791095907465,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.325017,
            "unit": "ms",
            "extra": "median of 5 runs: 0.318, 0.324, 0.325, 0.330, 0.331 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.581789,
            "unit": "ms",
            "extra": "median of 5 runs: 16.514, 16.529, 16.582, 16.585, 16.724 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 101.14287999999999,
            "unit": "ms",
            "extra": "median of 5 runs: 100.482, 100.926, 101.143, 101.646, 102.024 ms"
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
          "id": "65536fa8d6d99828c92d0809f9fba4c765e5b990",
          "message": "FD-10 review: a partial watch is no watch; #935's render count stays open\n\nMust-fix from the review of f660a58:\n\n- A watch registration the platform refused was dropped, and Held trusted\n  any running watch, so a store left unwatched served stale panels, and\n  the sidebar, for ever (the sidebar's hole dated from FD-10b).\n  watchset::follow now answers the wanted folders the platform would not\n  watch that are still there. The plane watch keeps them and says its\n  Standing: Partial, WholeAgain (once, after a gap) or Whole. Held serves\n  the model only when the watch is Whole. On Partial it reads fresh, and\n  on WholeAgain it rebuilds the model first. A root the platform will\n  not watch fails Watch::start. A test-only refusal list in watchset\n  plays the platform; the tests failed before the fix.\n- The held-plane watch test is robust on macOS: in this crate's tests\n  the plane watch runs on notify's poller (planewatch::Platform), as\n  planewatch's own tests do.\n\nFold-ins:\n- A workspace with a store that could not be read is read again on\n  every ask, not held.\n- The memory commands tell the model inside their blocking task. The\n  todo commands tell it whether or not the write succeeded, as the\n  persona commands do.\n- The property test also writes workspace.json and removes clones.\n\nDecided in review:\n- D-FD10j #935's added acceptance line (count PlaneView renders) is not\n  met for a write in the workspace in front, whose panels state lives in\n  PlaneView. This branch refers to #935 and #650 instead of closing them,\n  and the render-count half is tracked in #1160. Supersedes the closing\n  lines for #935 and #650 in f660a58: the train closes #934 and refers\n  to #935 and #650.\n- D-FD10k A folder that vanished between the listing and its watch is\n  not a failure (the next burst lists again); one still on disk that the\n  platform refused is.\n\nCloses #934\nRefs #935\nRefs #650\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-10-04T11:41:38+04:00",
          "tree_id": "18944feab7f68e5a4ba272cf3edb16b1d90f60f7",
          "url": "https://github.com/diazoxide/charter/commit/65536fa8d6d99828c92d0809f9fba4c765e5b990"
        },
        "date": 1791099803255,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.5650584999999999,
            "unit": "ms",
            "extra": "median of 5 runs: 0.553, 0.558, 0.565, 0.583, 0.585 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 17.1597315,
            "unit": "ms",
            "extra": "median of 5 runs: 16.555, 17.076, 17.160, 17.220, 17.516 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 105.064515,
            "unit": "ms",
            "extra": "median of 5 runs: 104.538, 104.891, 105.065, 106.747, 106.814 ms"
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
          "id": "e92d89bed7eac847e5c309c0b42c1504c1299e16",
          "message": "FD-11 review: fail-closed coverage, no status takes index.lock\n\nFixes the review of 35bd5c0.\n\n- Every change of a repo's coverage, either way, has its standing read\n  again (`Shared::cover`). An answer read before a watch began is not kept\n  for the covered backstop.\n- D-FD11h (delegated, fail closed): only a watch of the whole tree\n  (`How::Whole`: FSEvents, ReadDirectoryChangesW) covers a standing.\n  - inotify clones stay on the uncovered clock.\n  - Folders that could not be listed are their own state (`How::Unread`),\n    never `vec![root]`.\n  - A burst from a rescan, a watcher error or a pathless event\n    (`Burst::lost`) uncovers every kept clone and re-watches its tree. Only\n    a watch that is made covers it again, and that re-cover re-reads the\n    standing.\n- No `git status` charter runs takes `index.lock`. The runner sets\n  `GIT_OPTIONAL_LOCKS=0` for every `status`, so a new reader can't forget\n  it. The briefing's status also passes `--no-optional-locks`.\n  `no_automatic_read_takes_the_index_lock` now also covers the briefing\n  and the status line.\n- An unborn branch's staged files count as tracked dirt in the shared\n  standing.\n- `Root::matters`: a file in the index matters even under an ignore\n  pattern.\n- branchwatch:\n  - A clone found after its plane was let go of is not kept.\n  - A clone the reader refuses is not asked for again for 60 s.\n  - The repeated listing is one `listened()` helper.\n  - The leftover block in `start` is gone.\n- Docs: a covered idle monorepo costs about one read in twenty minutes,\n  not \"no status\".\n\nTests (each seen red against the code it guards):\n- cover on both edges;\n- a lost watch re-watched and re-covered, and a refused re-watch staying\n  uncovered;\n- folder-by-folder and unread clones never covered;\n- the refusal backoff;\n- the let-go race;\n- the runner's status leaving a stat-stale index alone, with a control;\n- the unborn branch;\n- a tracked file under an ignore pattern.\n\nFollow-ups are on #874: inotify coverage, and the reader-child spawn cost\nduring builds.\n\nCloses #651\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-10-04T12:32:34+04:00",
          "tree_id": "f8046fbb5719e35f0cb329b9ef1e4e7809ba501b",
          "url": "https://github.com/diazoxide/charter/commit/e92d89bed7eac847e5c309c0b42c1504c1299e16"
        },
        "date": 1791102893785,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.33716650000000004,
            "unit": "ms",
            "extra": "median of 5 runs: 0.327, 0.331, 0.337, 0.337, 0.344 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.6712695,
            "unit": "ms",
            "extra": "median of 5 runs: 16.589, 16.641, 16.671, 16.708, 16.714 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 101.3700155,
            "unit": "ms",
            "extra": "median of 5 runs: 100.854, 101.215, 101.370, 101.920, 102.154 ms"
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
          "id": "d15c62961c544de087cc9b1ce74189a05e954e23",
          "message": "mutants: install the sandbox's programs and fetch the whole lock\n\nThe nightly's baseline (charter-core's own tests) went red on main e92d89b\nwith six failures that are its runner, not the code: the sandboxed-start\ntests refuse without bubblewrap and socat, which ci.yml installs and\nmutants.yml did not, and the gitoxide guard reads `cargo metadata\n--offline` over the whole workspace, which a `--package charter-core`\nbuild never downloads. The shards had the same gap, where it is worse:\nevery mutant those tests cover read as caught. Both jobs now install the\ntwo programs and run `cargo fetch --locked`.\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-10-04T16:04:06+04:00",
          "tree_id": "119164f88e2573d7c24715bb6507e9d7a9e0b091",
          "url": "https://github.com/diazoxide/charter/commit/d15c62961c544de087cc9b1ce74189a05e954e23"
        },
        "date": 1791115520256,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.5799735,
            "unit": "ms",
            "extra": "median of 5 runs: 0.577, 0.579, 0.580, 0.580, 0.589 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.9627695,
            "unit": "ms",
            "extra": "median of 5 runs: 16.395, 16.729, 16.963, 17.227, 17.243 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 105.44758,
            "unit": "ms",
            "extra": "median of 5 runs: 103.376, 103.789, 105.448, 105.873, 106.176 ms"
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
          "id": "d58ad21b8751b43a8ee679ed75d7b023933415ae",
          "message": "keyhold: the Keychain's not-the-owner code is macOS's alone\n\n`NOT_THE_OWNER` is read only by the macOS `owned_by_another`, so on Linux it\nwas dead code and `-D warnings` stopped every Linux job (train 24, #1181).\nIt is now gated to macOS like the function that reads it.\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-10-04T19:33:20+04:00",
          "tree_id": "c42841d2993b1e9f56de94c8e5be3cb48112ef61",
          "url": "https://github.com/diazoxide/charter/commit/d58ad21b8751b43a8ee679ed75d7b023933415ae"
        },
        "date": 1791128132905,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.425822,
            "unit": "ms",
            "extra": "median of 5 runs: 0.418, 0.419, 0.426, 0.429, 0.430 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.518321999999998,
            "unit": "ms",
            "extra": "median of 5 runs: 16.462, 16.518, 16.518, 16.598, 16.628 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 101.4335515,
            "unit": "ms",
            "extra": "median of 5 runs: 100.840, 101.417, 101.434, 101.554, 102.059 ms"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "49699333+dependabot[bot]@users.noreply.github.com",
            "name": "dependabot[bot]",
            "username": "dependabot[bot]"
          },
          "committer": {
            "email": "aaron.yor@gmail.com",
            "name": "Aaron Yordanyan",
            "username": "diazoxide"
          },
          "distinct": true,
          "id": "3be8db4afd7b14e6971ce777ab88fee263b3f361",
          "message": "Bump taiki-e/install-action\n\nBumps the actions group with 1 update in the / directory: [taiki-e/install-action](https://github.com/taiki-e/install-action).\n\n\nUpdates `taiki-e/install-action` from 2.87.21 to 2.87.22\n- [Release notes](https://github.com/taiki-e/install-action/releases)\n- [Changelog](https://github.com/taiki-e/install-action/blob/main/CHANGELOG.md)\n- [Commits](https://github.com/taiki-e/install-action/compare/4cef1412cce204788f482e778a0b9187f9626a29...83ac0ad63c0167e6f06796fab0fce28db1bf3db0)\n\n---\nupdated-dependencies:\n- dependency-name: taiki-e/install-action\n  dependency-version: 2.87.22\n  dependency-type: direct:production\n  update-type: version-update:semver-patch\n  dependency-group: actions\n...\n\nSigned-off-by: dependabot[bot] <support@github.com>",
          "timestamp": "2026-10-04T20:26:16+04:00",
          "tree_id": "0d3b5bb0fdca12fe228ec3d4a2ea010eb5044008",
          "url": "https://github.com/diazoxide/charter/commit/3be8db4afd7b14e6971ce777ab88fee263b3f361"
        },
        "date": 1791131262365,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.3134925,
            "unit": "ms",
            "extra": "median of 5 runs: 0.311, 0.313, 0.313, 0.323, 0.328 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.571075999999998,
            "unit": "ms",
            "extra": "median of 5 runs: 16.529, 16.561, 16.571, 16.577, 16.590 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 101.35528249999999,
            "unit": "ms",
            "extra": "median of 5 runs: 101.135, 101.137, 101.355, 101.535, 101.736 ms"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "49699333+dependabot[bot]@users.noreply.github.com",
            "name": "dependabot[bot]",
            "username": "dependabot[bot]"
          },
          "committer": {
            "email": "aaron.yor@gmail.com",
            "name": "Aaron Yordanyan",
            "username": "diazoxide"
          },
          "distinct": true,
          "id": "9bbadf17cae2520195a2230531bc2ddc7731b3a5",
          "message": "Bump the rust group across 1 directory with 2 updates\n\nBumps the rust group with 2 updates in the / directory: [imagesize](https://github.com/Roughsketch/imagesize) and [nix](https://github.com/nix-rust/nix).\n\n\nUpdates `imagesize` from 0.14.0 to 0.15.0\n- [Release notes](https://github.com/Roughsketch/imagesize/releases)\n- [Commits](https://github.com/Roughsketch/imagesize/compare/v0.14.0...v0.15.0)\n\nUpdates `nix` from 0.28.0 to 0.31.3\n- [Changelog](https://github.com/nix-rust/nix/blob/master/CHANGELOG.md)\n- [Commits](https://github.com/nix-rust/nix/compare/v0.28.0...v0.31.3)\n\n---\nupdated-dependencies:\n- dependency-name: imagesize\n  dependency-version: 0.15.0\n  dependency-type: direct:production\n  update-type: version-update:semver-minor\n  dependency-group: rust\n- dependency-name: nix\n  dependency-version: 0.31.3\n  dependency-type: direct:production\n  update-type: version-update:semver-minor\n  dependency-group: rust\n...\n\nSigned-off-by: dependabot[bot] <support@github.com>",
          "timestamp": "2026-10-04T20:26:57+04:00",
          "tree_id": "552e2080e0cdf7e54bc3998b06f2d76b109598bb",
          "url": "https://github.com/diazoxide/charter/commit/9bbadf17cae2520195a2230531bc2ddc7731b3a5"
        },
        "date": 1791131966431,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.5877129999999999,
            "unit": "ms",
            "extra": "median of 5 runs: 0.582, 0.584, 0.588, 0.597, 0.598 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 17.073181499999997,
            "unit": "ms",
            "extra": "median of 5 runs: 16.687, 17.071, 17.073, 17.340, 17.499 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 104.548031,
            "unit": "ms",
            "extra": "median of 5 runs: 103.113, 104.062, 104.548, 104.559, 106.733 ms"
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
          "id": "d63df068a20ecf9892dd39257e05887afa39c0ab",
          "message": "stress: search at scale reports its budgets in CI instead of failing\n\n`search at scale` on macOS failed on every main run since FM-12 added it\n(ten runs on 2026-10-04), and passed on every Ubuntu run with the same\ncode. Run 37216804748, job 111483533718, 100,000 files:\n- ⌘P first find 8082 ms (budget 3 s); the same find in a second session\n  of the same run took 434 ms;\n- ⌘P keystroke worst 54.4 ms (budget 50 ms; median 6.8 ms);\n- status 11.8-14.5 s (budget 30 s, not missed).\n\nDiagnosis. The runner, not charter's code:\n- writing the repo took 146 s on macOS against 29 s on Ubuntu (718 s\n  against 96 s at 300,000), and the whole-repo scan 14 s against 4 s\n  (119 s against 9 s). Neither touches the reader child;\n- the reader child is spawned once per status, never per search; ⌘P and\n  ⌘⇧F run in process. perf-1 measured the child at 6-8 ms;\n- locally (M4 Pro, load 6-9) the same 100,000-file repo reads its status\n  in 0.7-1.0 s, with gix's compare trusting all 100,000 stats: 0 racily\n  clean, 0 to update, 0 files read.\nOne real cost class was confirmed locally. A stat that stops matching\nthe index (here every file's ctime changed by an xattr) makes the reader\nhash all 826 MB on every read, because it never writes the index:\n2.3-3.2 s per status. It is not proven to be the runner's cause. The run\nnow prints gix's counters and `git status`'s time next to the reader's,\nso the next main run says which one it was. The follow-up goes on #1153.\n\nWhat changes (ADR 0086, amended 2026-10-04):\n- `CHARTER_MEASURE_BUDGETS=report` prints each miss as\n  `FM-12 | budget missed | ...` and passes. By hand, unset, the budgets\n  still fail the run on the operator's machine;\n- stress.yml runs it that way, turns each miss into a warning, writes\n  every FM-12 line to the job summary and uploads the log. A wrong\n  answer or a panic still fails the job;\n- docs/spec.md G2-G4: release absolute by hand, evidence only in CI.\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-10-04T23:30:46+04:00",
          "tree_id": "928f696fad931e6e304f624d9dcf3a7ca60d1485",
          "url": "https://github.com/diazoxide/charter/commit/d63df068a20ecf9892dd39257e05887afa39c0ab"
        },
        "date": 1791142397566,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.5607205,
            "unit": "ms",
            "extra": "median of 5 runs: 0.540, 0.553, 0.561, 0.561, 0.563 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.996339,
            "unit": "ms",
            "extra": "median of 5 runs: 16.270, 16.762, 16.996, 17.022, 17.141 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 103.8087725,
            "unit": "ms",
            "extra": "median of 5 runs: 103.415, 103.652, 103.809, 104.497, 105.174 ms"
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
          "id": "d908b3ca32f318f349f6305d9eaca93704f46f1b",
          "message": "stress: a run that met every budget doesn't fail its own summary\n\nTrain 25's summary step greps the log for missed budgets and pipes the\nresult into a loop. On a run that met every budget, grep finds nothing and\nexits 1, and under the step's pipefail that failed the step, so every\nsearch-at-scale job went red on main d63df06 (all ubuntu ones included).\nThe grep now tolerates finding nothing.\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-10-05T00:30:17+04:00",
          "tree_id": "41d87aca13a53a0cd22c61f87db8ffee7e1852a8",
          "url": "https://github.com/diazoxide/charter/commit/d908b3ca32f318f349f6305d9eaca93704f46f1b"
        },
        "date": 1791145924446,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.4230125,
            "unit": "ms",
            "extra": "median of 5 runs: 0.412, 0.413, 0.423, 0.425, 0.436 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.666341,
            "unit": "ms",
            "extra": "median of 5 runs: 16.387, 16.590, 16.666, 16.673, 16.792 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 101.75712200000001,
            "unit": "ms",
            "extra": "median of 5 runs: 101.314, 101.554, 101.757, 101.927, 102.121 ms"
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
          "id": "a37080d39c8a6ae7e2ffea3f322a57aa8a5585e2",
          "message": "DS-3d: Rename workspace, Start chat, First run and Updates on the settings set\n\nThe four remaining hand-built forms draw their form parts from the settings\nset (SettingGroup, SettingRow, Field, Choice) instead of the dialogs'\nasks/choices/choice classes. Button rows stay on their old classes for DS-3e\n(#1177).\n\n- Rename workspace: the New name box is a row; the dialog finds the box to\n  focus and select on opening, since a Field takes no ref.\n- Start chat: harness and persona are radio Choices (a harness's ProfileMeta\n  is its option's line), the footer, branch and sandbox boxes are toggles,\n  Name and the sandbox reason are text Fields, and the sandbox block sits in\n  a \"Sandbox\" SettingGroup in every state.\n- First run: the path is a Field row (placeholder an example of the answer)\n  with Open below it; the template a radio Choice disabled while opening.\n- Updates: the channel is a radio Choice marking the current one; the pick\n  is held and written only by \"Use this channel\" (busy-guarded). Success\n  says the new channel and returns focus to it; a refusal shows the channel\n  it is still on.\n- e2e/opening.ts finds the harness rows through the radiogroup labelled\n  \"Harness\" instead of the gone `pick-harness` id.\n- docs/ui-primitives.md: the arrow-pick repair lives in Choice, and the rule\n  for radios that write with no Undo or start something.\n\nDecided in implementation:\n- D-DS3d1 (accepted globally): Choice's radio picks the option a single\n  arrow key moves to. Radix's arrow flag comes from a document listener that\n  hears the key after the roving focus has set its move going, so only a\n  held key picked; StartChat had worked around it with onFocus per row.\n  Choice hears the arrow in the capture phase, picks on the focus that\n  follows, and forgets it on key-up or when focus leaves the group. Guarded\n  by components.test.tsx. Rejected: an opt-in prop, a hand-built group in\n  StartChat. Visible effect: Settings > Your editor follows one arrow.\n- D-DS3d2: Updates' channel pick is held and written by \"Use this channel\",\n  shown only while the pick differs from the channel, forgotten on close.\n  Rejected: writing on pick (the radio trap), an always-shown disabled\n  button. The existing \"puts the machine on the channel the operator picks\"\n  test now presses it.\n- D-DS3d3, D-DS3d4: duplicates of DS-3c's toggle tabIndex={0} and text\n  input autoComplete=\"off\"; DS-3c's side was taken on rebase.\n- D-DS3d5: Option.says widens from string to ReactNode, so a harness's\n  kind, command and source keep their own marks; ProfileMeta's id becomes\n  optional.\n- D-DS3d6: Choice's radio gains Radix's own `disabled` (First run holds the\n  template while opening; Updates holds the channel while writing).\n- D-DS3d7: moot after rebase onto DS-3c: Rename's help keeps\n  <code>workspaces/</code>, First run keeps its /path/to/repo placeholder.\n  The checkbox labels keep their exact wording, which tests name.\n\nLeftover for DS-3e: Start chat's refused list still names a profile with\nthe `who` class, and `.choices-name` now has no user.\n\nCloses #1176\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-10-05T00:34:46+04:00",
          "tree_id": "b5e9e0805631ddc46b481b9efe203d5a176b730c",
          "url": "https://github.com/diazoxide/charter/commit/a37080d39c8a6ae7e2ffea3f322a57aa8a5585e2"
        },
        "date": 1791146674585,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.4282525,
            "unit": "ms",
            "extra": "median of 5 runs: 0.409, 0.419, 0.428, 0.428, 0.431 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.574596999999997,
            "unit": "ms",
            "extra": "median of 5 runs: 16.409, 16.475, 16.575, 16.615, 16.740 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 101.78090599999999,
            "unit": "ms",
            "extra": "median of 5 runs: 100.751, 101.756, 101.781, 101.916, 102.326 ms"
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
          "id": "0cefd0236a739fc65dd572a6f4f1a1eda0f9e397",
          "message": "search at scale: a status the reader cannot finish is a budget missed\n\nOn the macOS runner at 300,000 files, a status runs past the reader child's\nown deadline, and the evidence job (main d908b3c) panicked on\n`.expect(\"a status\")` instead of saying so. A read that comes back with no\nanswer is now pushed as a missed budget, \"status read: no answer (…)\", and\nthe measurement ends there: in report mode it is a warning, by hand it still\nfails the run (ADR 0086 as amended in train 25).\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-10-05T01:14:14+04:00",
          "tree_id": "b20e0b80d2e0a1ee4a63c867a53b55adcc89d4a6",
          "url": "https://github.com/diazoxide/charter/commit/0cefd0236a739fc65dd572a6f4f1a1eda0f9e397"
        },
        "date": 1791148536530,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.5961495,
            "unit": "ms",
            "extra": "median of 5 runs: 0.584, 0.592, 0.596, 0.599, 0.621 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.757609,
            "unit": "ms",
            "extra": "median of 5 runs: 16.590, 16.614, 16.758, 17.305, 17.424 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 104.236635,
            "unit": "ms",
            "extra": "median of 5 runs: 103.286, 103.601, 104.237, 104.803, 106.278 ms"
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
          "id": "8a4cf9719a15fe72fea40edaa76f2df68d1406b8",
          "message": "ADR 0090: agents work together as chats that are listed, observable and stoppable, and no agent's word is consent (AC-1)\n\nProposed, for the operator's acceptance. Restates \"visibility is the\ncontrol\" as Q11 and allows headless chats once AC-12 lists them.\n\n- One primitive, dispatch, in two modes: handoff (as today, at most\n  one report) and task (AC-2, over the MCP Tasks extension).\n- Every act between chats rides the chat scope; no new client scope and\n  no new power; answer stays a human scope's (V16, V75).\n- A dispatch needs a person's yes: charter's own ask per dispatch, or a\n  dispatch grant made on a human scope (coordinator, workflows,\n  triggered chats).\n- A closed list of what a chat may ask of another: dispatch, report,\n  message along its lineage or over a person-made message link, stop\n  its own subtree, leases and claim links through the host.\n- Approvals never travel along a lineage; a dispatcher never answers\n  its task's input request; AC-11's answerer must exclude the asking\n  chat's lineage.\n- Caps on depth, live chats per lineage and message rate; audit\n  actions for every act, with no brief or message text in an entry.\n\nAmends ADR 0066 (handed_from covers both modes) and ADR 0076 (a\ndispatcher cause). CONTEXT.md gains Dispatch, Handoff, Task, Report,\nLineage, Headless chat, Peer message, Mailbox, Dispatch grant and\nMessage link.\n\nRefs #713\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-10-05T01:25:48+04:00",
          "tree_id": "f72505c4b0b3e7e9827be9e10f523892a08517bd",
          "url": "https://github.com/diazoxide/charter/commit/8a4cf9719a15fe72fea40edaa76f2df68d1406b8"
        },
        "date": 1791149336425,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.269614,
            "unit": "ms",
            "extra": "median of 5 runs: 0.254, 0.266, 0.270, 0.287, 0.287 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.425747,
            "unit": "ms",
            "extra": "median of 5 runs: 16.390, 16.410, 16.426, 16.432, 16.458 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 101.486448,
            "unit": "ms",
            "extra": "median of 5 runs: 101.042, 101.326, 101.486, 101.718, 101.989 ms"
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
          "id": "08fb7c1276b471e709462ba4262f841cfc93afca",
          "message": "SE-21 review: sr-only count, refusals stay under a no-match filter\n\n- The live count uses Tailwind's sr-only, and the hand-written hidden rule is gone.\n- A filter that matches nothing says so above the level's standing refusals instead of in\n  their place; a Project-level test holds a refusal on screen under such a filter.\n- The arrow-key test filters by a word that narrows both groups to one setting each, and\n  says so correctly.\n\nCloses #1171\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-10-05T01:47:20+04:00",
          "tree_id": "38ed3871209d22c781c051aaf60d3d0872b821df",
          "url": "https://github.com/diazoxide/charter/commit/08fb7c1276b471e709462ba4262f841cfc93afca"
        },
        "date": 1791150513981,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.5147295000000001,
            "unit": "ms",
            "extra": "median of 5 runs: 0.500, 0.504, 0.515, 0.518, 0.528 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.966683,
            "unit": "ms",
            "extra": "median of 5 runs: 16.483, 16.599, 16.967, 17.086, 17.485 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 104.262693,
            "unit": "ms",
            "extra": "median of 5 runs: 103.357, 103.905, 104.263, 106.796, 107.851 ms"
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
          "id": "3c4f46aee0f3b388744cbbfa75615b886e6150a2",
          "message": "FM-11 review: a theme state, the no-line sentences, honest comments, your editor\n\n- The `piece-diff` view has a state in the theme test (`theme/views.test.tsx`): a text diff\n  whose drawn line is checked, so every view in `OWN_MARKS` is drawn in every theme.\n- A change with no changed line says what changed, by its mark: a pure rename \"Only its name\n  changed: moved from X\", a new empty file \"X was added, and it is empty\", a deleted empty one\n  \"X was deleted, and it was empty\", and only a changed file keeps the mode sentence.\n- The comparison tab has \"Open in your editor\" (the light editor's, at the change's first\n  line), and the binary and too-large sentences point to it.\n- The comments in `piecefiles.rs` and on `files::what_changed` no longer say no git process\n  starts: confining the path finds the branch's folder in the app process (`git worktree list`\n  or `git rev-parse`). Moving that into the reader, announcing the tab's states and naming the\n  merge view are on #1189.\n\nCloses #1114\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-10-05T04:05:23+04:00",
          "tree_id": "e6968cb56696b48bbce9e46321abaddd740b8e7b",
          "url": "https://github.com/diazoxide/charter/commit/3c4f46aee0f3b388744cbbfa75615b886e6150a2"
        },
        "date": 1791158830005,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.465265,
            "unit": "ms",
            "extra": "median of 5 runs: 0.435, 0.461, 0.465, 0.485, 0.488 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.429012,
            "unit": "ms",
            "extra": "median of 5 runs: 16.335, 16.362, 16.429, 16.506, 16.576 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 101.942263,
            "unit": "ms",
            "extra": "median of 5 runs: 100.617, 101.529, 101.942, 102.052, 102.207 ms"
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
          "id": "152e2dda66582b76f612ee03e0d4f47413f4af84",
          "message": "updates: the weekly count's review fold-ins (OB-17)\n\n- Guard: `weekly_endpoint` is a way of naming Charter's address in\n  only_the_updater_and_report_name_charters_own_address. A temporary caller in clipath.rs\n  failed the guard with the new entry and passed it without. The guard also stopped\n  scanning a file at a mid-file `#[cfg(test)] mod name;` declaration, so a caller after\n  line 138 of charter-core's lib.rs went unseen. It now stops only at an inline test module.\n- offer_from gives the week back when the weekly request was never sent, and reads the\n  manifest. A test covers this.\n- docs/updating.md:\n  - the macOS opt-out route (`launchctl setenv DO_NOT_TRACK 1`, or start from a shell) and\n    #1185;\n  - that GitHub can join one IP's weekly fetch to its usual fetches, and that the fallback\n    makes two requests back to back;\n  - two more biases: anyone can inflate the count, and a request counted and then failed\n    is counted again;\n  - the fixed headers named: Host, Accept, Accept-Encoding and the user agent;\n  - ragged lines rewrapped.\n- \"Nothing is published\" now reads \"charter publishes no estimate; the raw count is public,\n  as every asset's is\", in the ADR and in updating.md. main.rs's module comment is rewrapped.\n\nCloses #688\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-10-05T04:40:56+04:00",
          "tree_id": "320d88b65d48a48822e4797eab1fece4937a3800",
          "url": "https://github.com/diazoxide/charter/commit/152e2dda66582b76f612ee03e0d4f47413f4af84"
        },
        "date": 1791161024485,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.441404,
            "unit": "ms",
            "extra": "median of 5 runs: 0.411, 0.436, 0.441, 0.446, 0.446 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.337739,
            "unit": "ms",
            "extra": "median of 5 runs: 16.076, 16.173, 16.338, 16.400, 16.791 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 101.3306105,
            "unit": "ms",
            "extra": "median of 5 runs: 101.007, 101.093, 101.331, 101.633, 101.685 ms"
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
          "id": "e6f44ed5e10f62fdb556131047567048a7aeed51",
          "message": "KN-4 review: a verb on the archive's button, focus after Restore\n\n- The archive's catalogue row, and the heading button that draws it as a\n  glyph, now reads as a verb: \"Open alpha's archive\", \"Open steward's\n  archive\", \"Open the shared archive\". The tab it opens keeps its name\n  (`archiveTitle`).\n- After a Restore, focus moves to the status line that says what it did\n  (tabIndex -1), so a keyboard user is not left on a button that has gone.\n  A test asserts document.activeElement.\n- Hardening follow-up filed as #1194 (D-90c: fail closed) and added to #1191.\n\nRefs #716\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-10-05T04:46:13+04:00",
          "tree_id": "8ac77cfc76762471c11cb2b0ddac55d14ab34320",
          "url": "https://github.com/diazoxide/charter/commit/e6f44ed5e10f62fdb556131047567048a7aeed51"
        },
        "date": 1791161882708,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.46993150000000006,
            "unit": "ms",
            "extra": "median of 5 runs: 0.455, 0.463, 0.470, 0.472, 0.491 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.3554645,
            "unit": "ms",
            "extra": "median of 5 runs: 16.165, 16.307, 16.355, 16.399, 16.562 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 102.0739225,
            "unit": "ms",
            "extra": "median of 5 runs: 100.580, 101.041, 102.074, 102.101, 102.231 ms"
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
          "id": "7692ba246cb842293e14e8ce0c656b6d7c8ec9dc",
          "message": "KN-3 fold-ins: workspace move-memory in two comments, rewrapped lines\n\n- memscope.rs and the core test's header name `charter workspace move-memory`,\n  not `charter workspace move`.\n- Rewrapped to the usual width: memory_move.rs's first doc line, the doc\n  comment above `move_memory`, the changes fragment, and the plane-format\n  line ending \"and the window's Move\".\n\nRefs #715\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-10-05T05:36:36+04:00",
          "tree_id": "c0fc57270d0db47859cfc8a3095015ab7103f769",
          "url": "https://github.com/diazoxide/charter/commit/7692ba246cb842293e14e8ce0c656b6d7c8ec9dc"
        },
        "date": 1791164337665,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.4598055,
            "unit": "ms",
            "extra": "median of 5 runs: 0.450, 0.457, 0.460, 0.460, 0.469 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.293289,
            "unit": "ms",
            "extra": "median of 5 runs: 16.241, 16.248, 16.293, 16.452, 16.479 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 101.63660949999999,
            "unit": "ms",
            "extra": "median of 5 runs: 100.433, 101.388, 101.637, 101.749, 103.041 ms"
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
          "id": "86abd28970f2115e97f44f2b5365e7de431f4533",
          "message": "HY-7 review: hangs are judged per shard; trust_gates is tested, not excluded\n\nThe review found two must-fixes and two fold-ins.\n\n- A TIMEOUT is now a hang only when its own shard's slowest whole suite fits in two thirds\n  of the timeout. A shard with no MISSED mutant has no measurement, so its TIMEOUTs stay\n  survivors. The first version took the largest suite time over all shards. Replaying\n  2026-09-25 (run 36115292771), that rule made 373 suites cut short on slow runners into\n  \"hangs\" and took survivors from 481 to 128, which could have closed #480 on a red night.\n  The per-shard rule gives 480 survivors and 1 hang there. 2026-10-02 and 10-03 are\n  unchanged: 7 and 13 hangs.\n- `trust_gates([claude, claude])` must name \"hooks\" once. That test kills the `&&`→`||`\n  mutant, so `.cargo/mutants.toml` no longer excludes it as equivalent, and the site no\n  longer claims it is.\n- The notice job's checkout sets `persist-credentials: false`.\n- AGENTS.md and the script say \"the same shard\", and that the per-shard rule is what keeps\n  a night like 2026-09-25 red.\n\nAmended:\n- D-HY7c: a TIMEOUT is a hang only when `--timeout` is at least 1.5x the slowest whole\n  suite on its own shard. With no MISSED mutant on that shard, it stays red. Rejected: one\n  suite time for the whole run, because shards run on different runners.\n- D-HY7g: equivalent mutants are excluded with the proof at their site, for `probe` `-`,\n  `default_branch` `&&` and `git_dir_of` `||`. `trust_gates` is a plain function, so a\n  test can give it a gate twice, and its `&&` is killed rather than excluded.\n\nRefs #480\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-10-05T06:30:20+04:00",
          "tree_id": "04385952b8624de51f2a20bd18a13486e960f758",
          "url": "https://github.com/diazoxide/charter/commit/86abd28970f2115e97f44f2b5365e7de431f4533"
        },
        "date": 1791167536878,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.4634115,
            "unit": "ms",
            "extra": "median of 5 runs: 0.448, 0.459, 0.463, 0.473, 0.486 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.271413,
            "unit": "ms",
            "extra": "median of 5 runs: 16.230, 16.238, 16.271, 16.362, 16.418 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 101.47392450000001,
            "unit": "ms",
            "extra": "median of 5 runs: 100.764, 100.931, 101.474, 101.858, 101.874 ms"
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
          "id": "89ee5972ba7549349fce4382f87ef4acb5c5f07d",
          "message": "FW-6a review: per-board iterations, archived items left out, every page checked\n\nFold-ins from the FW-6a review:\n\n- F1: `Iteration.title` is now an `Option<String>`, and `of_forge` and `lasting` take one,\n  because a GitLab cadence's iterations have no title.\n- F2: the `WorkItem` query asks `projectItems(includeArchived: false)`, since GitHub's default is\n  to include them. It also reads `isArchived` and drops an archived item it is given anyway, so an\n  archived board item is never a placement.\n- F3: `fieldValues(first: 50)` reads its `pageInfo`. A second page of board fields is an error,\n  as D-FW6a-5 says for every other connection.\n- F4: `Placement` keeps its own `iteration`. The item-level `iteration` is the first board's.\n- F5: the `Placement` doc says its status is per board, and that FW-6b (#734) adds an item-level\n  status for GitLab, Jira and Linear.\n- F6: GitLab's `read` refuses an answer with no title as malformed, as GitHub does; `create`\n  still falls back to the title it sent. A GitHub `read` of a pull request's number says\n  \"#n is a pull request\". The overflow test now covers sub-issues, closing pull requests, boards\n  and board fields.\n- F7: ADR 0070 §2's `Capability` sketch lists `CloseReasons`, with a note that the enum grows by\n  ticket.\n\nAmends D-FW6a-3: each board keeps its own iteration in `Placement::iteration`, and the\nitem-level `iteration` is the first board's in GitHub's order. FW-9 no longer has to choose an\niteration across boards: it reads the board it groups by.\n\nRefs #733\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-10-05T06:30:29+04:00",
          "tree_id": "c912cef8cf1622c3c8418fe4fe179088584db729",
          "url": "https://github.com/diazoxide/charter/commit/89ee5972ba7549349fce4382f87ef4acb5c5f07d"
        },
        "date": 1791168319871,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.473178,
            "unit": "ms",
            "extra": "median of 5 runs: 0.465, 0.467, 0.473, 0.474, 0.482 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.7056295,
            "unit": "ms",
            "extra": "median of 5 runs: 16.442, 16.482, 16.706, 16.715, 16.912 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 101.560234,
            "unit": "ms",
            "extra": "median of 5 runs: 100.850, 101.499, 101.560, 102.030, 102.610 ms"
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
          "id": "dc620c4046d1ac5a10de9053665525eaa7c17f43",
          "message": "stress: a comparison the capped reader cannot answer is a budget miss, not a panic\n\nThe 300,000-file evidence-only run (37252093589) panicked on the uncommitted\ncomparison's `.expect` when the capped reader stopped without an answer. ADR 0086,\nas amended, says an evidence-only run reports each budget it missed and never\nfails for one.\n\nA new `Held::answered` turns only \"no answer from the capped reader\" (its\ndeadline, its memory cap, its child failing: every refusal that says \"charter\ncould not read the branch\") into a named miss. Any other refusal is still a\nfailure, and so is a wrong answer. `measure_compare` uses it for the file list,\none file's hunks and the uncommitted comparison. It prints and holds each part as\nsoon as that part is measured, so a later miss doesn't drop earlier timings.\n`measure_status` uses it too, and keeps its clean timings when the six-change\nreads miss. The find, search and scan steps don't go through the capped reader.\n\nRefs #1209\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-10-05T07:17:27+04:00",
          "tree_id": "bdf3d52a9bb2ffb5efded39a0f5968203d661539",
          "url": "https://github.com/diazoxide/charter/commit/dc620c4046d1ac5a10de9053665525eaa7c17f43"
        },
        "date": 1791170441603,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.4552035,
            "unit": "ms",
            "extra": "median of 5 runs: 0.435, 0.442, 0.455, 0.469, 0.477 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.650356000000002,
            "unit": "ms",
            "extra": "median of 5 runs: 16.232, 16.277, 16.650, 16.702, 16.821 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 102.254574,
            "unit": "ms",
            "extra": "median of 5 runs: 101.212, 101.277, 102.255, 102.486, 103.160 ms"
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
          "id": "2a02e598adfb3101d158a6e950501f425034a750",
          "message": "HY-8 review fold-ins: every #464 item accounted for, permission tests restore their mode\n\nRe-measuring the item-1 functions the first commit did not name turned\nup survivors that an earlier pass had wrongly read as caught (a broken\nselection in the local helper). They are fixed here.\n\nNew tests (tests/a_pr_mode_save_keeps_one_pr_open_from_the_save_branch.rs\nunless named):\n- still_holds `&&`→`||` ×2: a_block_clears_once_the_plane_is_moved_off_\n  the_commit_it_is_about\n- known_pr_url → None / delete `!`: the known PR stays on the standing\n  through a failed save (in a_pr_whose_state_the_forge_cannot_give_…)\n- knows_a_pr → true: a_clean_request_mode_plane_with_no_pr_known_has_\n  nothing_to_save_or_ask\n- tip_is_in_head → false: a_save_branch_tip_this_clone_pushed_but_lost_\n  the_note_of_is_pushed_over\n- describe `>`→`<`: a_request_carrying_more_saves_than_it_lists_says_\n  how_many_earlier_ones_it_left_out\n- push's kept-branch `!=`→`==` and `delete field branch`: a_save_branch_\n  changed_by_hand_is_pushed_and_given_its_own_pr\n- settle's merged-commit `&&`→`||`: a_merge_the_forge_names_at_a_commit_\n  not_on_the_target_blocks_the_plane\n- plane::place delete `!` (CHARTER_ROOT): a `place` assertion added to\n  HY-7's a_pinned_plane_is_the_one_acted_on_and_an_empty_home_is_no_home\n  in the steering test (one CHARTER_ROOT child, not two)\n- push_save_branch's stale-lease `&&`→`||`: a_push_the_remote_refuses_for_\n  its_own_reason_asks_it_nothing_more and a_push_the_remote_refuses_keeps_\n  the_remotes_reason_when_the_remote_then_goes_away (planegit/prsave/\n  tests.rs, from the reviewer's proof tests). Under the mutant a refused\n  push runs an extra ls-remote, and when that fails the remote's own reason\n  is replaced by \"could not read from remote\".\n- push_save_branch's gone-branch case: the `!expected.is_empty()` guard is\n  gone (D-HY8g, amended), so its mutant no longer exists. The arm that\n  replaced it, `None => Some(String::new())`, is pinned by a_save_branch_\n  deleted_on_the_remote_while_a_first_push_was_on_its_way_is_pushed_afresh:\n  the first push's absent lease is refused, the listing finds the branch\n  gone (the listing's own upload-pack deletes it first), and the second\n  push lands. That test asserts 2 pushes, 1 ls-remote and Ok, and it fails\n  under `delete match arm None` and both `delete !` mutants.\n\nRecorded as equivalent in .cargo/mutants.toml, with the proof at the site:\n- describe's `[one] if total == 1` → true: one subject of the newest 50\n  is a range of one, and `total` falls back to the subjects' count.\n\nItem-1 functions re-measured and caught, with the test that catches them:\n- Claim::within: a_claim_asked_for_within_a_bound_is_had_once_its_holder_\n  lets_go_inside_it. Its `+`→`*` and `Some(Default)` do not compile.\n- planegit summary: a_message_naming_every_group_it_has_counts_none_as_more\n- signer_said: the_signers_words_are_kept_and_gits_progress_hints_and_\n  bare_prefixes_are_not\n- Stage::word: a_stage_is_named_by_the_word_the_window_reads\n- unpushed: a_commit_left_unpushed_is_pushed_by_the_next_save_even_with_\n  nothing_new_to_commit. Its two let-chain `||` mutants do not compile.\n- still_holds → true/false, `!=`: every_pr_mode_save_is_run_against_a_\n  stand_in_forge, and alerts::tests::a_memory_commit_never_pushed_is_red_\n  and_one_awaiting_a_pull_request_is_not\n- tip_is_in_head → true, describe → constants, `==`, `>=`: the PR-mode suite\n- reposave summary: a_generated_message_lists_the_files_and_counts_the_rest\n  and a_generated_message_naming_exactly_as_many_files_as_it_shows_counts_\n  none_as_more\n- nobody_working (its non-empty mutants): autosave::tests::quitting_commits_\n  what_a_repo_with_auto_save_on_had_changed. `vec![]` was already excluded\n  as equivalent.\n- Standing::worth_saving: the autosave::tests quit and quiet-period tests,\n  and a_repo_is_worth_saving_for_changed_files_a_block_or_an_unpushed_\n  commit_and_never_when_off\n- Standing::fingerprint: the_fingerprint_moves_with_the_head_the_changed_\n  count_and_the_unpushed_count_only\n\nWhat the remaining TIMEOUTs came to:\n- plane resolve → Ok(Default): caught, by doctor::tests::a_plane_without_\n  the_report_rule_is_flagged_and_fix_adds_it.\n- plane place delete `!`: a survivor only a steered child can see, now\n  caught (above).\n- personaverbs/stats hh_mm_ss_ff `+=`→`*=` names four sites:\n  - three are caught: recorded::an_hour_alone_is_two_digits_as_\n    fromisoformat_requires, recorded::an_offset_of_hours_minutes_or_\n    seconds_moves_the_instant_and_not_the_printed, and tests::a_time_is_\n    refused_exactly_where_fromisoformat_refuses_it;\n  - the fourth, the digit-skip loop after six fraction digits, was a\n    GENUINE HANG: `p *= 1` never advanced, and only a test's timeout could\n    catch it (50 minutes locally before I stopped it). The loop is now one\n    count of the remaining digits, which reads the same input the same way;\n    its `+=` mutants fail tests::a_time_is_refused_exactly_where_\n    fromisoformat_refuses_it instead of hanging, so no exclusion is needed.\n- workspaces Workspace::manifest_text NotFound → true: a survivor, now\n  caught (first commit).\n\nThe flaky index test #464's comment named\n(reading_where_unsaved_work_sits_never_writes_the_index_a_save_needs) was\nsettled by #527 (1f864a0), which made reading a plane never write its\nindex.\n\nThe two permission tests restore their directory's mode through a drop\nguard, so a failing assertion cannot leave a mode-000/555 directory behind,\nand return early as root (rustix geteuid), which ignores the mode:\n- doctor git_auth_names_the_workspaces_folder_itself_when_it_cannot_be_listed\n- a_merged_pr_the_plane_cannot_move_onto_yet_is_said_in_the_saves_journal,\n  now `#[cfg(unix)]`\n\nDecided in implementation:\n- D-HY8a: Base the work on the last full nightly that finished its shards\n  (run 36308723145, 2026-09-27) plus local runs. The 2026-10-04 nightly\n  tested nothing (red baseline, fixed in d15c629), and the diff nightlies\n  do not cover these files.\n- D-HY8b: A mutant counts as caught when a test related to it fails under\n  it, since that implies the full suite fails. A run whose only failure is\n  an unrelated test does not count as caught (that is what misled the\n  earlier pass). A mutant that survives the targeted tests is re-run\n  against the lib tests and the relevant integration tests before a test\n  is written.\n- D-HY8c: Write behaviour tests rather than equivalence entries for every\n  live survivor. The prsave field deletions are pinned through\n  plane-push.json and the journal, which doctor and the next save read.\n  Exclusions are only for proven equivalents, with the proof at the site.\n- D-HY8d: Keep #489's charter-core steering integration test for the\n  cli-only mutants. Rejected: adding charter-cli as a mutants test package\n  (it runs the CLI suite for every mutant); and recording them as known\n  (that leaves real gaps).\n- D-HY8e: Settle's waiting path is reached with a read-only directory\n  blocking the move, because `reset --keep` overwrites ignored files. This\n  test and doctor's unreadable-workspaces test are unix-only and skip\n  themselves as root. Both restore the mode on drop.\n- D-HY8f: Survivors next to #464's lists but outside them go to #480\n  (HY-7) as one checklist comment, not a new issue.\n- D-HY8g (dispatcher), amended after review: a save branch that is gone\n  from the remote when charter lists it, after a push with nothing expected\n  was refused as stale, IS pushed again with an absent lease, as every gone\n  branch is (the contract on push_save_branch). The first ruling (block\n  instead) was reversed for these reasons:\n  - that path runs only when this clone never pushed the branch, so it has\n    no request of its own, and settle answers Nothing; the next save would\n    recreate the branch anyway;\n  - recording Blocked stops auto-save over a race that has passed;\n  - it contradicted the documented contract.\n  The block message added for it is removed.\n- D-HY8h: the hh_mm_ss_ff hang is removed at its source (a count instead of\n  a step loop) rather than excluded. An exclusion names all four `+=`\n  sites, three of them caught by tests.\n\nCloses #464\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-10-05T07:57:17+04:00",
          "tree_id": "9bef8dec55f7c05140fbe4b525c9e6953af45ba2",
          "url": "https://github.com/diazoxide/charter/commit/2a02e598adfb3101d158a6e950501f425034a750"
        },
        "date": 1791172842940,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.530035,
            "unit": "ms",
            "extra": "median of 5 runs: 0.509, 0.519, 0.530, 0.540, 0.551 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.558281,
            "unit": "ms",
            "extra": "median of 5 runs: 16.529, 16.546, 16.558, 16.686, 16.737 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 103.62584799999999,
            "unit": "ms",
            "extra": "median of 5 runs: 102.598, 102.861, 103.626, 104.104, 105.994 ms"
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
          "id": "e28d6aa9041758c1b0ec5be5afd27e3cdba919ea",
          "message": "mutants: an unreadable alert record holds the whole crate (HY-7b review)\n\nReview fixes to 9cc724c.\n\nMust-fix: `notice` read a malformed record of dirty files as clean. With a clean slice\nover a.rs of {a.rs, b.rs}, the records {}, \"xyz\", [1] and [\"*\"] each produced `close`,\nand null and 5 crashed with a TypeError. `recorded` now accepts only two shapes: a JSON\nlist of path strings, none of which is \"*\", or exactly the string \"*\". Anything else,\nincluding a decode error, counts as the whole crate. A hand-written [] still means no\nfile is left. New tests cover each malformed record (plus \"[\" and a list holding a\nnumber); each one must produce `edit`, never `close`, and never crash.\n\nD-HY7b-6, amended: `notice` runs on `schedule` only, as in HY-7, so a dispatched run,\nfull or not, never touches the alert. Only a full cycle of clean Sunday slices shuts it.\nFor the record-less alert that is open today, that means 13 weeks of clean slices; a\ndispatched full run does not count. The trigger is unchanged. The wording is corrected\nin the workflow's header and `notice` job comments, in AGENTS.md, in the issue body text\nand in mutants-report.py's docstrings.\n\nFold-in: a missing space in `add_parser(\"notice\", help=...)`.\n\nChecks: node --test tools/*.test.mjs (66/66), actionlint mutants.yml (clean).\n\nRefs #1200\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-10-05T08:41:42+04:00",
          "tree_id": "c3f38247541ccf13001003e058a57a9bff3b9a11",
          "url": "https://github.com/diazoxide/charter/commit/e28d6aa9041758c1b0ec5be5afd27e3cdba919ea"
        },
        "date": 1791175456923,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.422231,
            "unit": "ms",
            "extra": "median of 5 runs: 0.393, 0.421, 0.422, 0.431, 0.451 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.398059500000002,
            "unit": "ms",
            "extra": "median of 5 runs: 16.296, 16.315, 16.398, 16.546, 16.905 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 101.69546,
            "unit": "ms",
            "extra": "median of 5 runs: 101.368, 101.582, 101.695, 101.865, 103.198 ms"
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
          "id": "74f2399a8a44a21ec5577842de06973306bab015",
          "message": "FW-6b fold-ins: every recording names a case, overrides are checked\n\nThe two optional points from the FW-6b review, in the contract harness.\n\nA. `every_recording_on_both_forges_names_a_case` walks each forge's\nrecordings and requires every file's stem, with or without the\n`.self_managed` suffix, to name a case in CASES. A self-managed override\nleft behind by a renamed case would otherwise go unread while the\nself-managed run fell back to the main recording. The override loader\n(`own_recording`) now tells \"not there\" from a read that failed: only\nNotFound means no override; any other error fails the case.\n\nB. The network-log check also reads every `{case}.self_managed.json`, so a\nREST path that only an override asks is checked for scene names too.\nGitLab's expected count goes from 22 to 24 (the read override's issue and\nboards calls).\n\nRefs #734\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-10-05T08:42:30+04:00",
          "tree_id": "8c87a34dd20ab4c7b854cd5b83a9dbcf4a3be2e2",
          "url": "https://github.com/diazoxide/charter/commit/74f2399a8a44a21ec5577842de06973306bab015"
        },
        "date": 1791176232550,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.5357555,
            "unit": "ms",
            "extra": "median of 5 runs: 0.506, 0.525, 0.536, 0.551, 0.570 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.905222,
            "unit": "ms",
            "extra": "median of 5 runs: 16.468, 16.650, 16.905, 16.974, 17.154 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 105.10152099999999,
            "unit": "ms",
            "extra": "median of 5 runs: 102.869, 105.020, 105.102, 105.238, 106.275 ms"
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
          "id": "340c80a910098059b5264b211afabb8d36fbe036",
          "message": "Docs site: http-cache-semantics 4.2.0 -> 4.3.0; sharp alerts are the stand-in\n\nhttp-cache-semantics (Dependabot alert 12, GHSA-ch52-4w7c-c8xp, range <= 4.2.0):\n- 4.3.0 was published 2026-10-04, after the advisory. astro@7.3.5 asks for\n  ^4.2.0, so this is a lockfile-only bump: the one entry changes, nothing else.\n- 4.3.0 does not change max-stale handling; its diff is Vary matching (wildcard,\n  inherited properties) and a status() accessor. The upstream maintainer calls\n  the report bogus (kornelski/http-cache-semantics issue 56: RFC 9111 7.3 lets a\n  shared cache reuse a Set-Cookie response; Cache-Control: private is the guard),\n  and github/advisory-database issue 10139 disputes it. The alert should still\n  go away, since 4.3.0 is outside the vulnerable range.\n- Reachability, either way: the only user is astro/dist/assets/build/remote.js,\n  which builds a CachePolicy for a remote image astro fetches itself during\n  `astro build`, to decide how long to keep it in the local build cache. Its\n  requests carry no max-stale, the cache serves no other user, and the site has\n  no remote images (astro.config.mjs uses passthroughImageService, no\n  remotePatterns or domains). The advisory's path is not reachable here.\n\nsharp (alerts 7, 8, 9, 10): no change, and none is possible by a bump. The site\ninstalls no sharp: package.json maps sharp to the local stand-in no-sharp\n(MIT, 0.0.0-not-installed) and overrides every sharp to it, because sharp's\nprebuilt libvips is LGPL-3.0. astro already asks for sharp ^0.35.4 as an\noptional dependency, so a real sharp would be a patched one, but bringing it\nback would bring the LGPL binary with it. osv-scanner.toml already skips sharp\nfor the same reason, and test/no-sharp.test.mjs pins the two together.\nDependabot reads the stand-in as a vulnerable sharp. These four alerts are\nfalse positives, to dismiss as \"vulnerable code is not used\" with that reason.\n\nChecks in site/: npm ci, npm test (27 pass), npm run build (79 pages, links\nvalid). node ../tools/npm-licences.mjs from app/ passes; it covers\napp/package-lock.json only, and the licence here is unchanged (BSD-2-Clause).\n\nRefs #1217\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-10-05T08:58:51+04:00",
          "tree_id": "4313eb917f128f4c1f60ca39c2aa1ee3015280bf",
          "url": "https://github.com/diazoxide/charter/commit/340c80a910098059b5264b211afabb8d36fbe036"
        },
        "date": 1791177090676,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.38640399999999997,
            "unit": "ms",
            "extra": "median of 5 runs: 0.359, 0.363, 0.386, 0.392, 0.418 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.6901535,
            "unit": "ms",
            "extra": "median of 5 runs: 16.670, 16.681, 16.690, 16.703, 16.793 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 101.7695425,
            "unit": "ms",
            "extra": "median of 5 runs: 100.964, 101.732, 101.770, 102.491, 102.732 ms"
          }
        ]
      }
    ]
  }
}