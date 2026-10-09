window.BENCHMARK_DATA = {
  "lastUpdate": 1791550872797,
  "repoUrl": "https://github.com/purlis/purlis",
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
          "id": "ac9ba5ced5cfcbb8851301e4196d742f9cde2541",
          "message": "SE-23 e2e: press the gear the driver cannot show, and leave the window as found\n\nsettings-gears.e2e.ts went red on both scenario jobs on its first run\n(run 37260967190), and every spec after it with it.\n\n- The gear: a WebDriver pointer move does not make WebKit match\n  `:hover`, a WebDriver Tab is synthesised in the page and moves no\n  focus, and whether a scripted focus matches `:focus-visible` depends\n  on what earlier specs did with the pointer. The gear is pressed from\n  the page; its quiet stays SettingsGears.test.tsx's to hold.\n- Sign commits: found by its `<label for>`, which wdio's `aria/`\n  selector does not resolve, and chosen by value plus a `change`,\n  because `selectByAttribute` leaves the value unset in this driver.\n- The leak: `close_plane` lets the core go of a project but leaves its\n  tab on the strip, in front, with its Settings tab, and one app\n  process serves the whole run. The after-hook now closes the project\n  from its own tab's close button, as saving.e2e.ts does, and puts the\n  copy's charter.toml back.\n\nRefs #1173\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-10-05T11:24:07+04:00",
          "tree_id": "97af29d756c7745416bb6fbf0219d7066477775f",
          "url": "https://github.com/diazoxide/charter/commit/ac9ba5ced5cfcbb8851301e4196d742f9cde2541"
        },
        "date": 1791185176542,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.445917,
            "unit": "ms",
            "extra": "median of 5 runs: 0.434, 0.444, 0.446, 0.453, 0.456 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.5211865,
            "unit": "ms",
            "extra": "median of 5 runs: 16.357, 16.481, 16.521, 16.605, 16.652 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 101.318011,
            "unit": "ms",
            "extra": "median of 5 runs: 101.042, 101.194, 101.318, 101.999, 102.512 ms"
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
          "id": "ea9c202023f6dd65074ef3e5c9e4908dc49b210a",
          "message": "secret shapes: recognise more token formats\n\nEvery check built on `secret_kind` now also asks the token-prefix rule\nthat `found` and `token_kind` already used, after the existing rules, so\neach kind those rules named stays the one named. The prefix list gains\nthe remaining documented GitLab token prefixes and Slack's app-level and\nworkflow token prefixes, each cited from its provider's documentation.\n\nTests build made-up token bodies at runtime and cover every documented\nformat plus names that only start like a token.\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-10-05T13:15:07+04:00",
          "tree_id": "cea96b623a4caba9075bf5abfcc5346e003787e0",
          "url": "https://github.com/diazoxide/charter/commit/ea9c202023f6dd65074ef3e5c9e4908dc49b210a"
        },
        "date": 1791191840410,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.6108515000000001,
            "unit": "ms",
            "extra": "median of 5 runs: 0.595, 0.600, 0.611, 0.616, 0.617 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.138594,
            "unit": "ms",
            "extra": "median of 5 runs: 15.880, 16.068, 16.139, 16.226, 16.337 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 101.578577,
            "unit": "ms",
            "extra": "median of 5 runs: 100.599, 101.286, 101.579, 102.227, 103.890 ms"
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
          "id": "18291f234aa8a0e79c2e50cd30a6da55695aa94d",
          "message": "train 44: settings-forges e2e finds the Forges button inside the nav\n\nWebdriverIO's `button=Forges` text selector cannot be joined to a CSS\nselector, so `nav[aria-label=\"Groups\"] button=Forges` threw \"is not a\nvalid selector\" on both scenario runners. The spec now finds the nav,\nthen the button inside it, as settings-gears does.\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-10-05T13:41:32+04:00",
          "tree_id": "1d561608fb6e37844a5fd806253846daa8244805",
          "url": "https://github.com/diazoxide/charter/commit/18291f234aa8a0e79c2e50cd30a6da55695aa94d"
        },
        "date": 1791193483636,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.491734,
            "unit": "ms",
            "extra": "median of 5 runs: 0.480, 0.491, 0.492, 0.501, 0.526 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.797514,
            "unit": "ms",
            "extra": "median of 5 runs: 16.280, 16.286, 16.798, 16.975, 17.095 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 104.35280800000001,
            "unit": "ms",
            "extra": "median of 5 runs: 103.336, 103.826, 104.353, 104.627, 105.698 ms"
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
          "id": "dc37c4d0ff80e00998358505e5664f5157343ed1",
          "message": "train 45: the updater tests' manifest server counts a request before it answers\n\n`the_real_check_lists_its_read_in_the_network_log` failed twice in a row on this train's CI\nwith 0 served, expected 1, and `a_release_without_a_weekly_manifest_still_answers_the_check`\nfailed the same way on the first run. Both pass on their own. The stub server bumped its count\nonly after writing the reply, so a client that had read the reply could assert before the\ncount moved. It now counts each request before it writes the reply. Test code only; refs\nissue 1222.\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-10-05T18:09:16+04:00",
          "tree_id": "98d5eded6e53b80fa3bbacc46a2c8e771c9df6f1",
          "url": "https://github.com/purlis/purlis/commit/dc37c4d0ff80e00998358505e5664f5157343ed1"
        },
        "date": 1791209635192,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.410767,
            "unit": "ms",
            "extra": "median of 5 runs: 0.386, 0.398, 0.411, 0.421, 0.425 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.4801145,
            "unit": "ms",
            "extra": "median of 5 runs: 16.184, 16.435, 16.480, 16.602, 16.866 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 101.824378,
            "unit": "ms",
            "extra": "median of 5 runs: 100.878, 101.220, 101.824, 101.891, 102.641 ms"
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
          "id": "79d4fd6ad30b7f2d86456176e1e5b1fc19a4e7b2",
          "message": "rename train 1: pin the keychain and 1Password old names RN-4 moved\n\nRN-1's review round pinned identity::SERVICE_BASE and onepassword::TAG\nto their entries' old names. RN-4 moved both to the purlis spelling, so\nin the train they are pinned to `write`, and the old names each entry\nstill reads are pinned through the constants that read them:\nkeyring::OWN_PREFIXES[1], identity::READ_BASES[1] and onepassword's\nOLD_TAG. The pin count matches the old names held again.\n\nRefs #1254\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-10-05T19:04:59+04:00",
          "tree_id": "24354a48dc7dae5836f9b5fd5e786f9833b47d4e",
          "url": "https://github.com/purlis/purlis/commit/79d4fd6ad30b7f2d86456176e1e5b1fc19a4e7b2"
        },
        "date": 1791212862223,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.47235150000000004,
            "unit": "ms",
            "extra": "median of 5 runs: 0.469, 0.470, 0.472, 0.487, 0.493 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.347838,
            "unit": "ms",
            "extra": "median of 5 runs: 16.205, 16.332, 16.348, 16.393, 16.408 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 101.487864,
            "unit": "ms",
            "extra": "median of 5 runs: 100.552, 100.905, 101.488, 101.562, 101.838 ms"
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
          "id": "76c36affc058bc70df299afda43cc188475da39c",
          "message": "rename train 2: the gauge e2e expects hooks and the statusline through purlis\n\nRN-3 makes the app run its hooks through the purlis binary beside it, so the bundled plugin's hook binary and the armed statusLine command are target/debug/purlis, not the charter alias.\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-10-05T22:22:35+04:00",
          "tree_id": "37bacf0f4d21c743af33fcb0d7d09d220f8783ae",
          "url": "https://github.com/purlis/purlis/commit/76c36affc058bc70df299afda43cc188475da39c"
        },
        "date": 1791224660997,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.5167824999999999,
            "unit": "ms",
            "extra": "median of 5 runs: 0.488, 0.508, 0.517, 0.524, 0.531 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.9201925,
            "unit": "ms",
            "extra": "median of 5 runs: 16.458, 16.763, 16.920, 17.222, 17.480 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 105.00195550000001,
            "unit": "ms",
            "extra": "median of 5 runs: 102.795, 103.146, 105.002, 105.835, 107.490 ms"
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
          "id": "d32a38be70791e7522daa5377dfd0fe3500a24fa",
          "message": "Bump the web group in /app with 3 updates\n\nBumps the web group in /app with 3 updates: [@types/node](https://github.com/DefinitelyTyped/DefinitelyTyped/tree/HEAD/types/node), [globals](https://github.com/sindresorhus/globals) and [vite](https://github.com/vitejs/vite/tree/HEAD/packages/vite).\n\n\nUpdates `@types/node` from 22.20.4 to 22.20.5\n- [Release notes](https://github.com/DefinitelyTyped/DefinitelyTyped/releases)\n- [Commits](https://github.com/DefinitelyTyped/DefinitelyTyped/commits/HEAD/types/node)\n\nUpdates `globals` from 17.12.0 to 17.13.0\n- [Release notes](https://github.com/sindresorhus/globals/releases)\n- [Commits](https://github.com/sindresorhus/globals/compare/v17.12.0...v17.13.0)\n\nUpdates `vite` from 8.3.1 to 8.3.2\n- [Release notes](https://github.com/vitejs/vite/releases)\n- [Changelog](https://github.com/vitejs/vite/blob/main/packages/vite/CHANGELOG.md)\n- [Commits](https://github.com/vitejs/vite/commits/v8.3.2/packages/vite)\n\n---\nupdated-dependencies:\n- dependency-name: \"@types/node\"\n  dependency-version: 22.20.5\n  dependency-type: direct:development\n  update-type: version-update:semver-patch\n  dependency-group: web\n- dependency-name: globals\n  dependency-version: 17.13.0\n  dependency-type: direct:development\n  update-type: version-update:semver-minor\n  dependency-group: web\n- dependency-name: vite\n  dependency-version: 8.3.2\n  dependency-type: direct:development\n  update-type: version-update:semver-patch\n  dependency-group: web\n...\n\nSigned-off-by: dependabot[bot] <support@github.com>",
          "timestamp": "2026-10-05T22:49:35+04:00",
          "tree_id": "1f4ff64121679ad74df52f359e692bfeed6f8a0e",
          "url": "https://github.com/purlis/purlis/commit/d32a38be70791e7522daa5377dfd0fe3500a24fa"
        },
        "date": 1791226269116,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.46841900000000003,
            "unit": "ms",
            "extra": "median of 5 runs: 0.449, 0.463, 0.468, 0.473, 0.476 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.3165385,
            "unit": "ms",
            "extra": "median of 5 runs: 16.142, 16.273, 16.317, 16.779, 16.960 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 101.196714,
            "unit": "ms",
            "extra": "median of 5 runs: 101.090, 101.095, 101.197, 101.802, 101.938 ms"
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
          "id": "697dc80cbcbc0d0daee72d3cb112c9eb54f85b5c",
          "message": "rename-plane review fold-in: the consent guard asks the session's folder too (RN-7)\n\nThe host loads its settings for the folder the session started in, and a\n`cd` in the shell does not move that. `settings::twin_in_force` used to\nanchor its layer lookup on the payload's `cwd` alone. It now takes the\ndirectories the host's settings may come from: `$CLAUDE_PROJECT_DIR`, when\nthe hook is given it, and the call's `cwd`. It requires the purlis twin in\nthe layer at or above each of them, as well as in the project's settings\nand opencode's file. With neither directory present it keeps refusing the\npurlis spelling.\n\n`toolgate::Plane` carries `session_dir`, which the CLI guard reads from\n`CLAUDE_PROJECT_DIR`. A new test covers a session started in a workspace\nwhose layer lacks the twin, with the call's cwd at the project root: it is\nrefused, and so is a call with no folder at all.\n\nRefs #1265\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-10-06T01:32:03+04:00",
          "tree_id": "ee9e6a35b6ce0fd226962ddd03f66016301299b5",
          "url": "https://github.com/purlis/purlis/commit/697dc80cbcbc0d0daee72d3cb112c9eb54f85b5c"
        },
        "date": 1791235999473,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.371544,
            "unit": "ms",
            "extra": "median of 5 runs: 0.353, 0.355, 0.372, 0.375, 0.384 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.474576,
            "unit": "ms",
            "extra": "median of 5 runs: 16.434, 16.459, 16.475, 16.491, 16.551 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 102.060102,
            "unit": "ms",
            "extra": "median of 5 runs: 101.351, 101.689, 102.060, 102.121, 102.620 ms"
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
          "id": "18e02c208a7bc0b60ddb6b3050f8f74467976683",
          "message": "rename train 5: the old-name guard allows rename-local's refusal prose\n\nrenamelocal/busy.rs spells the old name once, in QUIT_FIRST (\"quit every\ncharter/purlis window, chat and terminal first\"). That text is what the\nrefusal tells the operator, so it is counted as visible prose (1) and\nwaits for RN-11a with the rest of the prose entries.\n\nRefs #1263\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-10-06T02:08:21+04:00",
          "tree_id": "97fad8b560deafd3fa6d0aba082b0226b4b000d2",
          "url": "https://github.com/purlis/purlis/commit/18e02c208a7bc0b60ddb6b3050f8f74467976683"
        },
        "date": 1791238180620,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.42861399999999994,
            "unit": "ms",
            "extra": "median of 5 runs: 0.397, 0.426, 0.429, 0.434, 0.455 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.292042000000002,
            "unit": "ms",
            "extra": "median of 5 runs: 16.245, 16.253, 16.292, 16.330, 16.392 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 101.4131735,
            "unit": "ms",
            "extra": "median of 5 runs: 101.284, 101.346, 101.413, 101.854, 103.443 ms"
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
          "id": "b0bbea88b94684d149114f4ad63dd529181816fe",
          "message": "ST-4: A page per harness profile: Add, Remove, and rename when unused\n\nEach [harness.<name>] profile in the local settings file is an entry of a\nnew Settings collection, charter_core::settings::harness_profiles, with\nlisted/add/remove/rename on the ST-3 collection write seam (drawn base,\nopaque ids, exact Undo of a remove, inverse op for an add or a rename).\n\n- Field refusals come from the loader's own rules: profiles::refusals_of\n  and launch_refusals now return every broken rule with the profile key it\n  is about; the loader still takes the first, in the same words.\n- A value shaped like a secret in the name, the kind, the command or a new\n  name is refused under that field by its kind, pointing to the vault, and\n  is never said back (V91m).\n- Remove and rename are refused while a [harness] default in either file\n  names the profile and nothing else would answer to the name; each user is\n  named with a link to Harness & profiles (V91g, V91k).\n- A rename is exact: the profile keeps its place among [harness]'s keys\n  (an inline profile too) and its name keeps its quoting, so the rename\n  back gives the text it started from.\n- Writes go to whichever local settings file the project has (RN-2a).\n- The window: one nav page per profile, set in under Harness & profiles,\n  with its rows, Rename and Remove; Add (name, kind, command) and an Open\n  per profile on Harness & profiles. A Remove or an Undo on a page lands\n  on Harness & profiles. New profile... in the default pickers opens the\n  Add form and picks the new profile.\n- The approval before a profile's first run is unchanged: an added or\n  renamed profile asks, as any profile nothing approved does.\n\nDecided in implementation:\n- D-ST4-1: the core module is settings::harness_profiles, not\n  settings::profiles, since settings already imports crate::profiles.\n- D-ST4-2: one rule set. The loader's rules answer every failure with its\n  field; the collection adds only what a writer can break (a name the file\n  already has, a secret). Rejected: a second copy of the rules.\n- D-ST4-3: the users of a profile are [harness] default in either file,\n  only when the name would then name nothing (a local table shadowing a\n  built-in can go). Chat records are not users: a reopened chat whose\n  profile is gone is skipped by name (ADR 0022).\n- D-ST4-4: Add asks for name, kind and command; the environment is set on\n  the profile's page. No profile field holds a secret, so there is no vault\n  picker; a secret-shaped value is refused under its field, by kind.\n- D-ST4-5: a page per profile (project.profile.<name>, `sub` in the nav)\n  holds that entry alone with Rename and Remove, no Add; its writes are\n  kept under project.harness (Collection.home), so a Remove or an Undo\n  there lands where the Undo is said. Row labels stay \"<name>: kind\" so\n  the filter finds them by the profile's name.\n- D-ST4-6: rename is an EntryOp; its Undo is the rename back through the\n  core, with its reference check. The wire answers the renamed id in\n  EntryWritten.added rather than a new field.\n- D-ST4-7: a renamed profile asks for approval again (the record is keyed\n  by name). Fail-closed; no record is moved.\n- D-ST4-8: New profile... opens the Add form and picks what it added,\n  superseding D-ST1-4's Edit as TOML (kept only when the local file is not\n  TOML). A new table goes after the last profile, after a blank line.\n\nCloses #1236\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-10-06T02:18:37+04:00",
          "tree_id": "9755b1716a8fc566c4b83e125acd898850cd4812",
          "url": "https://github.com/purlis/purlis/commit/b0bbea88b94684d149114f4ad63dd529181816fe"
        },
        "date": 1791239432804,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.4519425,
            "unit": "ms",
            "extra": "median of 5 runs: 0.446, 0.451, 0.452, 0.454, 0.468 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.319095500000003,
            "unit": "ms",
            "extra": "median of 5 runs: 16.263, 16.264, 16.319, 16.467, 16.948 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 101.399489,
            "unit": "ms",
            "extra": "median of 5 runs: 100.807, 100.971, 101.399, 101.788, 102.516 ms"
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
          "id": "51996375944338391ef5d1c219dfe34f12f0937d",
          "message": "Rename the rest of the tracker text: comments, PRs, labels, repo descriptions\n\ntools/tracker-rename-rest.mjs renames, from charter to purlis, what\ntools/tracker-rename.mjs (RN-12) leaves: issue and PR comments, PR review\ncomments, PR reviews' own text, PR titles and bodies, label names and\ndescriptions, and the\nrepos' descriptions. It imports RN-12's renameText, so the rules and the\nkeep-rules are RN-12's own (D-RN12-1..9), and it works the same way: a\nread-only dry run by default that prints a diff per item, counts by kind\nand by rule, and accounts for every occurrence; and `--apply --from\n<plan>`, which re-reads everything and refuses the whole run if any\nplanned item changed or is gone, re-reads each item just before its\nwrite, skips what already reads as done (so a rerun resumes), PATCHes\nonly the changed fields, waits --delay-ms (7.5s) between writes and\nhonours Retry-After.\n\nDecided in implementation:\n- D-RN12b-1 / D-RN12b-9: in the shared rules, every HTML comment stays\n  byte for byte, for every kind, RN-12's issues and milestones included.\n  That covers <!-- charter-save --> (what forge::pr::is_ours reads), the\n  BEGIN/END charter change markers that `change push` finds, the\n  <!-- mutants-report dirty: [...] --> record tools/mutants-report.py\n  parses (issue 480), and quoted fixtures (issue 712).\n- D-RN12b-2: labels the code finds by name keep their names\n  (CODE_LABELS: bug, enhancement, type:adr from the issue templates,\n  dependencies and github_actions for Dependabot, ws:* and\n  charter::ws::* from `ws todo promote`); their descriptions are\n  renamed. None of them says charter today. No code or workflow in\n  either repo reads via-charter-report or gap, so both are renamed.\n  --keep-label adds one.\n- D-RN12b-3: a repo description names the moved repos by their new\n  slugs (diazoxide/charter becomes purlis/purlis), as #1274 asks. The\n  old slug stays as history everywhere else.\n- D-RN12b-4: in the shared rules, `-p charter-app` / `--package\n  charter-app` is the app crate and becomes purlis-app (RN-13, V93c),\n  not the repo's name. This also changes RN-12's own plan for a\n  handful of issues, so its dry run should be run again before apply.\n- D-RN12b-5: comments on M60 issues and PRs in M60 stay as written,\n  as RN-12 leaves M60's issues.\n- D-RN12b-6: in the shared rules, a line where two different old names\n  would read the same afterwards (\"called charter, not charter-app\" as\n  \"called purlis, not purlis\") stays as written, counted as `collapse`\n  and listed in both dry runs for a person to word by hand.\n- D-RN12b-7: in the shared rules, old-plugin-id also keeps `charter-app`\n  said as a plugin's name (\"the plugin is called ..., not charter-app\",\n  \"the bundled `charter-app` plugin\"), short of a sentence's end.\n- D-RN12b-10: the collapse check leaves `cli` edits out (a command\n  names no product), and \"the Python charter\" / \"the old charter\" are\n  kept as retired-python, so the rest of such a line is still renamed.\n- D-RN12b-11: RN-12 rule misses, now kept: the persona's charter as\n  \"the charter body/prose/concatenation\", \"charter-format\" and \"the\n  charter (or personas/<name>/...)\"; `op-item: charter-<persona>`\n  (1Password titles still say charter, issue 1275); and the GitLab label\n  `charter::ws::<name>` the code still writes.\n- D-RN12b-8: a PR review's own text is covered (GET/PUT\n  pulls/{n}/reviews/{id}). One paginated GraphQL read finds the PRs\n  that have reviews, and only those are listed through the paginated\n  reviews endpoint.\n\nTested in node:test against a stand-in gh, per item kind. The real\napply waits for the operator's approval of the dry run.\n\nRefs #1274\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-10-06T02:45:48+04:00",
          "tree_id": "c0bad4d2fe962e2c49864673f0e11955a0c8e15e",
          "url": "https://github.com/purlis/purlis/commit/51996375944338391ef5d1c219dfe34f12f0937d"
        },
        "date": 1791240422743,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.496493,
            "unit": "ms",
            "extra": "median of 5 runs: 0.469, 0.492, 0.496, 0.510, 0.537 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 17.103396500000002,
            "unit": "ms",
            "extra": "median of 5 runs: 16.511, 16.809, 17.103, 17.198, 17.222 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 104.227304,
            "unit": "ms",
            "extra": "median of 5 runs: 103.225, 103.849, 104.227, 104.372, 104.723 ms"
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
          "id": "272fd673a632c304772e6c95d3822f7de5f3fc9e",
          "message": "train 47: the status line's alert e2e expects the drawer's Reinit button\n\nNO-6 draws an Alerts drawer row as a Notice whose button is its way out,\nso the workspace-behind-the-layout row offers Reinit in place of the\n`charter ws reinit --all` text the spec looked for. The spec now asks\nfor the row's words and its Reinit button.\n\nRefs #1238\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-10-06T02:59:05+04:00",
          "tree_id": "3bc3a5289adad8e6eaca2688f78c123b978e1d28",
          "url": "https://github.com/purlis/purlis/commit/272fd673a632c304772e6c95d3822f7de5f3fc9e"
        },
        "date": 1791241770994,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.5978684999999999,
            "unit": "ms",
            "extra": "median of 5 runs: 0.590, 0.594, 0.598, 0.599, 0.622 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.003626999999998,
            "unit": "ms",
            "extra": "median of 5 runs: 15.894, 15.971, 16.004, 16.008, 16.577 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 101.62571,
            "unit": "ms",
            "extra": "median of 5 runs: 101.249, 101.458, 101.626, 101.913, 102.570 ms"
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
          "id": "e691c31272ce7cc01aa7470c18deac99afc1dbfc",
          "message": "save: the secret check also asks each decoded key and string on its own\n\nThe JSON and TOML writers keep an escaped control character as two\ncharacters, so a credential right after an escaped newline or tab had a\nletter in front of it in the re-written document, and the rules that need a\nboundary there missed it. parsed_kind now also asks each decoded key and\nstring value on its own, after the whole-document pass, which still reads a\nkey and its value together as an assignment.\n\nDecided in implementation:\n- D-1295-6 (dispatcher, overrides D-1295-3): the save asks each decoded key\n  and string value as well as the whole document. Until the settings\n  editors move onto parsed_kind (issue 1304, item 1) the save may refuse\n  what an editor accepted, which is the safe direction.\n\nRefs #1295\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-10-06T03:53:49+04:00",
          "tree_id": "469ca42c4ad027a10e87e9091c845e86e5d06122",
          "url": "https://github.com/purlis/purlis/commit/e691c31272ce7cc01aa7470c18deac99afc1dbfc"
        },
        "date": 1791244591191,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.43553,
            "unit": "ms",
            "extra": "median of 5 runs: 0.416, 0.432, 0.436, 0.437, 0.454 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.5720855,
            "unit": "ms",
            "extra": "median of 5 runs: 16.312, 16.365, 16.572, 16.618, 17.053 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 102.4245225,
            "unit": "ms",
            "extra": "median of 5 runs: 101.415, 102.246, 102.425, 102.491, 102.725 ms"
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
          "id": "475e66594cab7d1c1b714519e588d802157a0efa",
          "message": "train 6: the tickets' tests meet main's guards and the renamed server\n\n- RN-8 gave rename-local's `Local` a `plugin` field after RN-6's keychain\n  test was written, so the test's `Local` names it as `None`, as the other\n  rename-local test does.\n- RN-8's three later rename-local plugin tests open with\n  `charter_core::unsteered!()`, as every test in charter-core's tests\n  directory must.\n- fake-harness's ACP handshake test expects the MCP server under its\n  current name (`chattools::SERVER`, `purlis` since RN-8) rather than the\n  old literal.\n\nRefs #1264\nRefs #1266\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-10-06T04:05:49+04:00",
          "tree_id": "ca728e37e1cd9f3c7711b6e0f3452f06ee79ee86",
          "url": "https://github.com/purlis/purlis/commit/475e66594cab7d1c1b714519e588d802157a0efa"
        },
        "date": 1791245615063,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.516165,
            "unit": "ms",
            "extra": "median of 5 runs: 0.482, 0.508, 0.516, 0.518, 0.520 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 17.075515,
            "unit": "ms",
            "extra": "median of 5 runs: 16.555, 17.027, 17.076, 17.081, 17.465 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 104.9795895,
            "unit": "ms",
            "extra": "median of 5 runs: 103.237, 104.544, 104.980, 105.285, 108.416 ms"
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
          "id": "cba7f1f342aa6968261315c641b73c00a9ab3f04",
          "message": "A renamed workspace's todo, memory and session tabs come back at launch\n\nThe rename test's saved record now holds a todo tab (alpha/<slug>), a\nmemory tab (workspace/alpha/<slug>) and a session record tab\n(workspaces/alpha/sessions/<file>). After renaming alpha to beta, the\ntest reads the record the way the next launch does, through\nreopen::read_or_refusal, and checks that each tab is held and keyed by\nbeta, on beta's strip.\n\nThis is the after-a-rename half of the ticket. The rekeying (issue 1248)\nand the widened held() check both already on main make it pass; no code\nchange was needed. While writing it, a session key whose file name is not\na record's name (such as 2026-10-05-review.md) was dropped at read, which\nis correct: held() checks a session key with sessionrecord::locate. The\ntest uses a real record name.\n\nCloses #1297\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-10-06T04:58:32+04:00",
          "tree_id": "bdf323829507a2db89f8bb354f295fd4a5e17076",
          "url": "https://github.com/purlis/purlis/commit/cba7f1f342aa6968261315c641b73c00a9ab3f04"
        },
        "date": 1791248475346,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.276779,
            "unit": "ms",
            "extra": "median of 5 runs: 0.268, 0.270, 0.277, 0.289, 0.305 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.579482,
            "unit": "ms",
            "extra": "median of 5 runs: 16.430, 16.533, 16.579, 16.661, 16.686 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 101.33099899999999,
            "unit": "ms",
            "extra": "median of 5 runs: 101.140, 101.308, 101.331, 101.781, 102.187 ms"
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
          "id": "aded4442c0473c7f79f87548a16feb0a34098d6f",
          "message": "train 7: RN-9's identity test sets the plugin field rename-local has since RN-8\n\nRN-8 (train 6) added a `plugin` field to rename-local's `Local`, and RN-9's new\ntest was written before it. The test now sets it to `None`, as the other\nrename-local tests do. Local clippy caught it.\n\nRefs #1267\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-10-06T05:28:46+04:00",
          "tree_id": "6aa113e5fb984709c6160f2a193baf670fb142a5",
          "url": "https://github.com/purlis/purlis/commit/aded4442c0473c7f79f87548a16feb0a34098d6f"
        },
        "date": 1791250219570,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.533472,
            "unit": "ms",
            "extra": "median of 5 runs: 0.520, 0.526, 0.533, 0.534, 0.535 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.8076875,
            "unit": "ms",
            "extra": "median of 5 runs: 16.341, 16.525, 16.808, 16.888, 17.299 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 102.72969,
            "unit": "ms",
            "extra": "median of 5 runs: 102.026, 102.263, 102.730, 103.283, 104.473 ms"
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
          "id": "d1c462a6108b968446a45432ee37b6f033211b1e",
          "message": "locks: a kept store lock unlocks on drop; the 972 test models an exited host\n\nReview of the first commit for issue 1316:\n\n- held::Store::locked() forgot its guard on purpose, so memstore::lock_store\n  let go only when the descriptor closed: the same leak to a forked child.\n  It now returns a LockedStore that unlocks in Drop, and StoreLock::Held keeps\n  that. A new held test hands a copy of the store's descriptor to a sleeping\n  child, drops the kept lock and takes it again; it failed (refused after\n  the 5 s wait) before this change.\n- The issue-972 event log test passed trivially once Log unlocks on drop. It\n  now models a host that exits without unlocking: a plain File on LOCK,\n  locked, a copy given to `sleep 0.4` as stdin, the File dropped unlocked,\n  and Log::open must succeed after waiting at least 100 ms. It no longer\n  clears close-on-exec, which leaked the lock into parallel tests' children.\n  Setting LOCK_WAIT to zero makes it fail.\n- Log::_lock is declared last, so it drops after the log's own files.\n- filelock's module doc now names which locks use Held and which unlock in\n  guards of their own.\n\nDecided in implementation:\n- D-1316-6: a kept lock is its own type (LockedStore, returned by\n  Store::locked) rather than a Drop on Store. An unlocked Store is held and\n  dropped all the time, and the type says which stores hold a lock until\n  dropped, the way Locked already does for one taken in scope.\n\nRefs #1316\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-10-06T06:16:05+04:00",
          "tree_id": "1feae52f1b8c17087cf5016a30a7d4f63c699175",
          "url": "https://github.com/purlis/purlis/commit/d1c462a6108b968446a45432ee37b6f033211b1e"
        },
        "date": 1791253180210,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.5059530000000001,
            "unit": "ms",
            "extra": "median of 5 runs: 0.495, 0.498, 0.506, 0.528, 0.541 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 17.125711000000003,
            "unit": "ms",
            "extra": "median of 5 runs: 16.486, 16.722, 17.126, 17.132, 17.324 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 103.6045575,
            "unit": "ms",
            "extra": "median of 5 runs: 102.701, 103.057, 103.605, 104.224, 104.956 ms"
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
          "id": "4599019abf222bd61d3a23fbcaf66184d3dc9e2a",
          "message": "recorded: the two news scenarios print the renamed CHANGELOG (RN-11b)\n\n`purlis news` prints CHANGELOG.md, and the docs sweep renamed the product in its prose, so the\ntwo news scenarios are re-recorded. nameonly.py still finds only the name and the two\naccepted statusline crops: 539 scenarios, 254 re-recorded in all.\n\nRefs #1270\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-10-06T07:12:00+04:00",
          "tree_id": "3e0f11ce42325fb2e491f4310a515145004ac26b",
          "url": "https://github.com/purlis/purlis/commit/4599019abf222bd61d3a23fbcaf66184d3dc9e2a"
        },
        "date": 1791256404612,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.4648855,
            "unit": "ms",
            "extra": "median of 5 runs: 0.443, 0.452, 0.465, 0.469, 0.475 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.300114999999998,
            "unit": "ms",
            "extra": "median of 5 runs: 16.227, 16.290, 16.300, 16.488, 16.839 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 101.37131600000001,
            "unit": "ms",
            "extra": "median of 5 runs: 100.910, 100.961, 101.371, 102.214, 102.695 ms"
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
          "id": "29f2ddbaed7e2a60d027161a2924d2446d86dfb7",
          "message": "e2e: the app launched over an old charter install, migrated at launch (RN-10)\n\nA wdio run of its own (`npm run e2e:upgrade`, `wdio.upgrade.conf.ts`) starts the\nreal app over an old install built inside the run's own HOME: the config home\nunder `<home>/.config/charter/` (an approved, pinned project with a pinned\nworkspace, on the dev channel), and a project in git with `.charter/`,\n`charter.toml`, `charter.local.toml` and a keyring vault whose item is under\n`charter/…` in the fenced build's stub keychain.\n\nThe scenario build skips the launch's rename-local (D-RN5-7); the app now runs\nit there when `PURLIS_E2E_RENAME_LOCAL=1`, which only this run sets. The spec\nasserts the approval, the pins and the channel survived, the config home is\n`purlis/` with no `charter/` beside it, the project's state and local settings\nmoved, the vault reads under `purlis/…` with nothing left waiting for the\nkeychain, and the doctor's `rename-plane` runs by name in one commit. The run\nremoves the old install when it passes.\n\nCI is this spec's first real run: wdio does not run reliably on this machine.\nThe fixture and the migration were checked locally instead: the old install\nthe helper writes was migrated by the core's rename-local in a fenced test\nbuild (config home, local settings, state folder, vault copied and read back,\nnothing waiting), and rename-plane then made its one commit on a clean tree.\n\nDecided in implementation:\n- D-RN10-1: The switch is `PURLIS_E2E_RENAME_LOCAL`, read only by the `e2e`\n  build and on only for exactly `1`. It calls `renamelocal::at_launch` with no\n  plugin to move: the build is fenced, so its busy check asks only the\n  project's own sockets, its keychain is the stub, and no harness is touched.\n  The log-folder move stays off, since every run names its log folder.\n- D-RN10-2: A run of its own, as the layout run is: the Tauri service keeps one\n  app per run and the old install has to be on disk before it starts. The base\n  run excludes `*.upgrade.e2e.ts`; CI runs it after the layout run on both\n  OSes; stress.yml ignores its files.\n- D-RN10-3: The old install is written as files in the old layout, not made\n  with the `charter` CLI: CI's CLI is not fenced, so making a keyring vault\n  with it would reach the runner's real keychain. `upgrade.test.ts` holds the\n  fixture to the old layout, and the variable's spelling in the config to the\n  one `lib.rs` reads.\n- D-RN10-4: The config home is `<run tree>/home/.config`, named by\n  `CHARTER_CONFIG_HOME` as well, because GitHub runners export an\n  `XDG_CONFIG_HOME` of their own. The helper refuses a home outside the run's\n  tree.\n- D-RN10-5: The values are ones no launch sets on its own: an approval, a\n  project pin, the `beta` workspace pin (a first open pins the most active\n  workspace, `alpha`), the dev channel. A store lost in the move fails the spec.\n- D-RN10-6: No doctor row offers `rename-plane` (it runs by name only, RN-7),\n  so the spec runs it by name through the Doctor's own command,\n  `plane_doctor_fix`, and asserts one commit, `purlis.toml` and a clean tree.\n  The project is committed clean and its profile program lives outside it.\n- D-RN10-7: The store remembers the project by its real path, which the app\n  resolves it to; `CHARTER_ROOT` is the copied path inside the fence.\n- D-RN10-8: Cleanup in `onComplete`: a passing run removes the project and the\n  config home; a failing one keeps them as evidence. Its own run tree and app\n  process, so nothing reaches a later run.\n- D-RN10-9: No changelog fragment: nothing a user sees changes.\n\nCloses #1268\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-10-06T07:45:53+04:00",
          "tree_id": "4013e9f47dd6c31c99aa9ac18b68ad879f534899",
          "url": "https://github.com/purlis/purlis/commit/29f2ddbaed7e2a60d027161a2924d2446d86dfb7"
        },
        "date": 1791258423011,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.5209405,
            "unit": "ms",
            "extra": "median of 5 runs: 0.505, 0.519, 0.521, 0.529, 0.534 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 17.1283735,
            "unit": "ms",
            "extra": "median of 5 runs: 16.520, 16.808, 17.128, 17.337, 17.406 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 105.49872099999999,
            "unit": "ms",
            "extra": "median of 5 runs: 104.850, 105.092, 105.499, 105.733, 108.273 ms"
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
          "id": "43c789086c103b0c86906e435b9ae8eff7d8ed09",
          "message": "Bump source-map-js from 1.2.1 to 1.2.2 in /app\n\nBumps [source-map-js](https://github.com/7rulnik/source-map-js) from 1.2.1 to 1.2.2.\n- [Release notes](https://github.com/7rulnik/source-map-js/releases)\n- [Changelog](https://github.com/7rulnik/source-map-js/blob/main/CHANGELOG.md)\n- [Commits](https://github.com/7rulnik/source-map-js/compare/v1.2.1...v1.2.2)\n\n---\nupdated-dependencies:\n- dependency-name: source-map-js\n  dependency-version: 1.2.2\n  dependency-type: indirect\n...\n\nSigned-off-by: dependabot[bot] <support@github.com>",
          "timestamp": "2026-10-06T08:13:30+04:00",
          "tree_id": "5d12ce1d84edb271a5a11a244d646635cc0d8dbb",
          "url": "https://github.com/purlis/purlis/commit/43c789086c103b0c86906e435b9ae8eff7d8ed09"
        },
        "date": 1791260088192,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.290132,
            "unit": "ms",
            "extra": "median of 5 runs: 0.264, 0.280, 0.290, 0.297, 0.300 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.613564500000003,
            "unit": "ms",
            "extra": "median of 5 runs: 16.547, 16.600, 16.614, 16.614, 16.682 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 101.675401,
            "unit": "ms",
            "extra": "median of 5 runs: 100.334, 101.624, 101.675, 101.792, 102.265 ms"
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
          "id": "e63fd8affbef57f60e0c43f079e51d893a591a95",
          "message": "Bump smol-toml from 1.8.0 to 1.9.0 in /app\n\nBumps [smol-toml](https://github.com/squirrelchat/smol-toml) from 1.8.0 to 1.9.0.\n- [Release notes](https://github.com/squirrelchat/smol-toml/releases)\n- [Commits](https://github.com/squirrelchat/smol-toml/compare/v1.8.0...v1.9.0)\n\n---\nupdated-dependencies:\n- dependency-name: smol-toml\n  dependency-version: 1.9.0\n  dependency-type: indirect\n...\n\nSigned-off-by: dependabot[bot] <support@github.com>",
          "timestamp": "2026-10-06T08:13:35+04:00",
          "tree_id": "5ee5c8d4594bea953298ef834a78deeb875ba485",
          "url": "https://github.com/purlis/purlis/commit/e63fd8affbef57f60e0c43f079e51d893a591a95"
        },
        "date": 1791261031035,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.526898,
            "unit": "ms",
            "extra": "median of 5 runs: 0.499, 0.505, 0.527, 0.536, 0.537 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.955509,
            "unit": "ms",
            "extra": "median of 5 runs: 16.882, 16.938, 16.956, 16.964, 17.349 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 103.68939750000001,
            "unit": "ms",
            "extra": "median of 5 runs: 102.990, 103.377, 103.689, 104.005, 104.262 ms"
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
          "id": "2c8b8ffd786b204707d0f3b7aceace6b3b74092e",
          "message": "chats: name the local file once in the approval test\n\nThe test that a refused Retry reads the approval again rewrote the\nprofiles' local file in one statement that both named it and read it.\nThe local-layer guard reads that shape as a reader that skips\nsettings::layer_text, and it failed train 51's rust job. The test now\nputs the path in a local first. It is test code, so nothing it decides\nchanges.\n\nTrain fold-in.\n\nRefs #1246\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-10-06T09:55:03+04:00",
          "tree_id": "0205ada3e113972fb18f607577fc8bd251af7b90",
          "url": "https://github.com/purlis/purlis/commit/2c8b8ffd786b204707d0f3b7aceace6b3b74092e"
        },
        "date": 1791266170699,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.47029850000000006,
            "unit": "ms",
            "extra": "median of 5 runs: 0.457, 0.459, 0.470, 0.475, 0.477 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.2398685,
            "unit": "ms",
            "extra": "median of 5 runs: 16.195, 16.232, 16.240, 16.259, 16.331 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 101.713942,
            "unit": "ms",
            "extra": "median of 5 runs: 100.820, 101.597, 101.714, 101.862, 102.450 ms"
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
          "id": "60b73fb0ddc749a676edba292ea3cd53bf58cd60",
          "message": "diffscan: a line-ending backslash with blanks after it, and blank lines, still join\n\nThe commit and push scan now joins the way the lexical decode does. A\nbackslash followed only by blanks to the end of its line joins, and the\njoin carries on through blank and whitespace-only added lines up to the\nnext line that is not blank. Each line is still read into one run at most.\n\nRefs #1315\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-10-06T13:56:42+04:00",
          "tree_id": "30043fe124c0cd9087fc6979806e9a9ab0e840ae",
          "url": "https://github.com/purlis/purlis/commit/60b73fb0ddc749a676edba292ea3cd53bf58cd60"
        },
        "date": 1791280675866,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.5281399999999999,
            "unit": "ms",
            "extra": "median of 5 runs: 0.509, 0.517, 0.528, 0.533, 0.556 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 17.0965445,
            "unit": "ms",
            "extra": "median of 5 runs: 16.749, 16.768, 17.097, 17.151, 17.290 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 105.736081,
            "unit": "ms",
            "extra": "median of 5 runs: 103.591, 104.126, 105.736, 106.055, 106.078 ms"
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
          "id": "d7721362c2c1d2a60c5aa7c3d9d0c4f8ff80a182",
          "message": "tests: the sandbox block's Report may name the tracker it files on\n\nThe address guard (ADR 0083 §9) refused app/src-tauri/src/sandboxing.rs, which\n#1338 added: the Report dialog shows the repository it would file on and files\nthere through report::file on the person's press, as `purlis report` does\n(ADR 0059). It gains its allowance with that reason.\n\nRefs #1338\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-10-06T15:24:52+04:00",
          "tree_id": "b75597403e636b8ad224b2d0a7283341d1631ef9",
          "url": "https://github.com/purlis/purlis/commit/d7721362c2c1d2a60c5aa7c3d9d0c4f8ff80a182"
        },
        "date": 1791286675359,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.528253,
            "unit": "ms",
            "extra": "median of 5 runs: 0.512, 0.524, 0.528, 0.533, 0.552 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 17.198847,
            "unit": "ms",
            "extra": "median of 5 runs: 16.321, 17.015, 17.199, 17.319, 17.361 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 104.4774455,
            "unit": "ms",
            "extra": "median of 5 runs: 102.247, 103.486, 104.477, 104.713, 105.719 ms"
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
          "id": "8f295ead543af155b131f9e75e329cb0603445fd",
          "message": "smart close: the closing Stop is sent, not waited on; the name guard drops session.rs\n\nThe two tests where a Stop closes the chat waited on it with the\n`reported` helper. That helper waits until the board shows the chat\nwaiting, but the Stop closes the chat and takes it off the board, so\nthe wait could never succeed. The Stop is now sent directly, and the\ntests wait for the tab to close.\n\nsession.rs no longer spells an old name, so its STILL_SPELLED entry\nis removed (the guard counts exactly).\n\nRefs #1332\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-10-06T16:27:18+04:00",
          "tree_id": "3c32b46c0c66b569fc8c013312951f9ef4388ff6",
          "url": "https://github.com/purlis/purlis/commit/8f295ead543af155b131f9e75e329cb0603445fd"
        },
        "date": 1791290129251,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.5543435,
            "unit": "ms",
            "extra": "median of 5 runs: 0.502, 0.551, 0.554, 0.564, 0.611 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 17.013863,
            "unit": "ms",
            "extra": "median of 5 runs: 16.759, 16.789, 17.014, 17.746, 18.043 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 105.766386,
            "unit": "ms",
            "extra": "median of 5 runs: 102.206, 104.583, 105.766, 107.030, 109.012 ms"
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
          "id": "af8adc1249e8ac2b501d4e27338a8a3dba341e74",
          "message": "brokered git test: quote the stand-in filter so it really runs\n\nThe operator's global `mark` filter in brokered_git.rs was written\nunquoted, `smudge = sh -c 'touch smudged; cat'`. git reads the `;` as\nthe start of a comment, so the command was cut to `sh -c 'touch smudged`,\nfailed, and, not being `required`, was skipped without a word. The\nterminal's clone therefore never ran it, and the assertion that the\napp's clone does not run it checked nothing. Linux CI caught the first\nhalf.\n\nThe value is now double-quoted and marks one absolute path in the\nworld's base directory, checked and cleared after each run, so the\nterminal's clone must run it and the app's clone and worktree must not.\nThe terminal's runs are asserted successful, and every failure prints\nthe run's stderr. The core broker tests' `evil` filter had the same\nunquoted value and is quoted too, so their \"the filter never ran\"\nassertions are no longer vacuous.\n\nRefs #1335\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-10-06T23:40:08+04:00",
          "tree_id": "48522879f673ca83b1983426a7a0be192c787f03",
          "url": "https://github.com/purlis/purlis/commit/af8adc1249e8ac2b501d4e27338a8a3dba341e74"
        },
        "date": 1791316143863,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.527177,
            "unit": "ms",
            "extra": "median of 5 runs: 0.499, 0.499, 0.527, 0.528, 0.564 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.922821,
            "unit": "ms",
            "extra": "median of 5 runs: 16.699, 16.747, 16.923, 17.117, 17.191 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 104.3217695,
            "unit": "ms",
            "extra": "median of 5 runs: 101.817, 103.494, 104.322, 105.027, 105.753 ms"
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
          "id": "a8b6350e1872f04d6c9d805a9d55550bbe483e8b",
          "message": "settings: Sandbox says what a chat here can do\n\nSettings > Project > Sandbox opens on one sentence that follows the\nfiles as they change: what a chat can write (its folder, the project's\npackage caches with Package registries on, and the folders you allowed\nevery chat on this machine) and what it can reach (each preset on, the\nproject's hosts and yours that are actually granted, and on a Mac the\nsystem's certificate service once certificate checks are on). Until\nthe core has answered it says \"Reading this project's sandbox…\".\n\n- A note says a chat can read any file you can, except vaults and\n  purlis's own keys, so keep secrets in a vault.\n- Internet access is a box per preset, named and listed by the core:\n  Preset::title and Preset::own_hosts, handed to the window in\n  SandboxState.presets with each preset's hosts as sandbox::hosts\n  answers them, the project's forges among code hosting's.\n- sandbox::besides counts what every chat is granted besides its\n  presets: the project's hosts and yours (locks, this machine's\n  addresses and the ignore check applied) and the folders you allowed\n  on this machine that still judge as grantable. It and\n  Compiled::granted read the same two helpers, granted_hosts and\n  granted_folders, and a fixture test holds the two equal for a chat on\n  no persona. SandboxState.besides carries it.\n- The mode is a status line, \"On for everyone in this project. To run\n  one chat without it, use that chat's tab\", with the machine's\n  opt-out count capitalised; where it is off, Turn the sandbox on\n  writes mode = \"on\" with no Undo, as before.\n- Certificate checks can be set here. Each persona's own hosts are\n  named, with who else holds them: a chat naming no persona gets the\n  default persona's, a handoff may hold the asking chat's, and a Resume\n  the default persona's. Harnesses never sandboxed on this machine are said.\n- Two read-only lists, What chats can change (with the harness's own\n  state) and Always protected (Your approvals in purlis, not your\n  logins), each item with one line of why, drawn after the project's\n  hosts and found by the Settings filter.\n- Settings names the project's files as the project names them\n  (charter.toml or purlis.toml): every Project group's text, the file\n  choice, the override badge, and the core's refusals the tab shows\n  (settings::refusals and the hosts collection's, via\n  settings::named_at). Only a whole file name is renamed: a host or a\n  path that holds one (charter.toml.example.com) is quoted as written,\n  and the Sandbox's own pages, which quote hosts a person wrote, are\n  not rewritten at all.\n\nDecided in implementation:\n- D-1340-1: readable preset names live in core beside the word and the\n  host table, so every surface names a preset alike.\n- D-1340-2: presets in force are read from the committed file, so the\n  sentence follows a box before the core is asked again; a file with no\n  egress reads as every preset, as core reads it. Hosts and folders are\n  the core's counts.\n- D-1340-3: the two lists are app copy over ADR 0067's classes; the\n  runner-internals class is left out, since no project chat runs on a\n  runner, and workspace.md editing is left out until #1334 lands.\n- D-1340-4: a group may carry rows drawn after its collection (after),\n  so the explanations never read as entries of the hosts list.\n- D-1340-5: FileSetting gains checks and status kinds rather than\n  hand-written rows, so the driver's write, refusal and Undo rules hold.\n- D-1340-6: \"Your approvals in purlis\" replaces \"approvals and\n  sign-ins\", and the page says reads are not confined, so no login of\n  the person's reads as hidden from a chat.\n- D-1340-7: the files' names are resolved where text reaches the window\n  (core: settings::named_at; app: projectGroups), rather than threading\n  a name through every reader's message; only whole names, never part\n  of a host or a path.\n- D-1340-8: the certificate service is named only where the window runs\n  on a Mac (onAMac), the one system where the setting widens anything.\n- D-1340-9: Settings' counts and a chat's start share one helper for\n  hosts and one for folders, so they cannot drift apart.\n\nFollow-ups: issue 1422.\n\nCloses #1340\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-10-07T03:20:36+04:00",
          "tree_id": "42daaa49ed085915f55d0ea834685a60441a8df4",
          "url": "https://github.com/purlis/purlis/commit/a8b6350e1872f04d6c9d805a9d55550bbe483e8b"
        },
        "date": 1791329385209,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.272841,
            "unit": "ms",
            "extra": "median of 5 runs: 0.264, 0.267, 0.273, 0.274, 0.291 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.5258645,
            "unit": "ms",
            "extra": "median of 5 runs: 16.417, 16.506, 16.526, 16.567, 16.575 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 101.1325025,
            "unit": "ms",
            "extra": "median of 5 runs: 100.330, 100.845, 101.133, 101.372, 101.375 ms"
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
          "id": "38e55d8b8c997b19117ccae8ca78c96bee2da0cc",
          "message": "sandbox: read bare script names, extensions, assignments and comma values\n\nFollow-ups from #1356, the whole #1418 checklist.\n\n- The reporting socket's folder is denied under every name a denied\n  path has: as written, as the kernel resolves it, and across a firmlink\n  (Settings::reporting_on, through the names_of helper now shared).\n- Bare names: a word a program opens as a file of code is named from\n  the folders the command runs in, with or without a `/` (`bash hook.sh`,\n  `awk -f sum.awk`, `awk -fsum.awk`, `python3 -mtools.x`, an MCP\n  server's `node server.js`, `source env.sh`).\n- Extension inference: such a file named without an extension its\n  runner adds is also named with each one (`node -r ./hook` names\n  hook.js and the other JS extensions, `ruby -r./x` names x.rb, x.so\n  and x.bundle, `gawk -i ./lib/util` names util.awk).\n- An assignment's value (`BASH_ENV=./x.sh`, git `-c core.hooksPath=./h`)\n  is named, and so is the whole word.\n- `-f./a=b.awk` names both `./a=b.awk` and `b.awk`.\n- Each value after `-W<letter>,` is read as a word of its own.\n- folds_case probes the real folder, and looks at the case-swapped\n  name with lstat, so a link cannot answer for it.\n- Scan::holds compares every spelling (link, data volume, case fold)\n  of the value against every spelling of the project, home, grantable\n  folders and cache home.\n- seatbelt::profile refuses a profile over MOST_PROFILE (768 KiB) with\n  TOO_LARGE, not an E2BIG at spawn. seatbelt::not_started writes the\n  refusal for the Codex and opencode wraps: the way out is a sentence\n  after \"so nothing was started\", as in the other refusals.\n- Review round 2:\n  - Subcommands are not scripts. deno reads a script only after `run`.\n    bun's own subcommands end the scan, and after `bun run` (or bare\n    `bun x`) a word is a file only with a `/` or a JS extension. tsx skips\n    `watch`. node's `--run` ends the scan.\n  - `python -m a.b` names a/b.py, a/b/__main__.py, a/b/__init__.py and\n    a/__init__.py, never the package folder.\n  - An existing folder given to a JS runner is its entry files, not the\n    folder.\n  - Value-taking long options for node and deno are skipped as values,\n    so the script after them is found.\n  - Launchers re-read the rest as a command (D-1418-9). `make -f`/\n    `--file`/`--makefile` is read, and versioned interpreters\n    (python3.12, node22) are read as their program.\n  - A folder named only after `=` is never denied, unless the variable\n    or git setting loads code (D-1418-10, amended in round 3).\n- Review round 3: RUNS_CODE, one table of the variables and git\n  settings that load code. The launchers read their `=` and attached\n  forms (`env --chdir=`, `-C<dir>`, `uv --directory=`/`--project=`),\n  skip BSD `env -P`/`-S` values, and read `npx tsx@4` as tsx and\n  `uv run -m mod` as python -m mod. The redundant BUN_OWN list is gone:\n  none of bun's own subcommands has a `/` or a JS extension, so the\n  package-script rule already ends the scan for each.\n\nDecided in implementation:\n- D-1418-1 Bare names. A word without a `/` is named only where the\n  program reads it as code: the script operand of a shell, `source`/`.`,\n  node/bun/tsx/ts-node/deno (after `run`), python, ruby, perl and php,\n  and the value of a program-file option (awk/gawk `-f`/`-E`/`-i`/\n  `--file`/`--exec`/`--include`, sed `-f`/`--file`, jq `-f`/\n  `--from-file`, node `-r`/`--require`/`--import`/`--loader`/\n  `--preload`, ruby `-r`, python `-m`, php `-f`, bash `--rcfile`/\n  `--init-file`). Each name is resolved from every folder the command\n  went to, as a `/` word is. These positions come from the program's\n  own grammar, not a guess, so an absolute value is named wherever it is\n  (`gawk -bf/opt/x.awk` now names /opt/x.awk). The script's own\n  arguments, code passed with `-c`/`-e`/`-p`/`-r`, npx's package names\n  and include folders (`-I lib`, `-L lib`) stay unnamed. Naming\n  include folders would make a repo's source folder read-only, and\n  none of those words is code a harness runs. Rejected: naming every\n  bare word (it denies data and log files a hook writes), and keeping\n  #1327's `/` rule (it misses the most common MCP form, `node\n  server.js`). A misparse can only miss a bare name, which today's rule\n  misses already. Over-reading stays inside the folders the command\n  runs in, so it never reaches an unrelated real folder.\n- D-1418-2 Extensions. A file a runner may find by adding an\n  extension is named with each one, only where its name has none of\n  them. JS runners use .js .mjs .cjs .json .node .ts .mts .cts .tsx\n  .jsx. Ruby -r uses .rb .so .bundle. awk -f/-E/-i use .awk. A python\n  -m module a.b names a/b.py, a/b/__main__.py, a/b/__init__.py and each parent\n  package's __init__.py, never the package folder. Search paths\n  (NODE_PATH, $LOAD_PATH, AWKPATH) are not followed. The siblings\n  sit beside a named file, never a new folder (D-1356-7 holds).\n- D-1418-3 Assignments. A word `<name>=<value>`, where the name reads\n  as a variable or a git key, names its value and the word as written\n  (a file may hold `=`). The name can be anything, so an absolute value\n  is a guess and follows D-1356-7: it is kept only where a chat could\n  write it (Scan::holds). `PREFIX=/usr/local` names no /usr/local.\n  Colon lists (PYTHONPATH=a:b) are not split.\n- D-1418-4 `=` precedence: a short option cluster with `=` gives both\n  the attached split and the value after `=`.\n- D-1418-5 Comma values: `-W<letter>,a,b` hands each piece on as a\n  word, so each is read as one (options recursively, so\n  `-Wl,--script=./x.ld` names x.ld). They are exact, so absolute ones\n  are named as a separate word is.\n- D-1418-6 holds() compares spellings (as written, lexical, resolved,\n  firmlink twin) of the value and of each root, case-folded where the\n  root's volume folds case. Over-counting only names more (fail closed).\n- D-1418-7 Case probe: folds_case asks real(path)'s deepest existing\n  folder, and reads the case-swapped name with lstat. A link beside\n  it, or a link on the way onto another volume, no longer answers.\n- D-1418-9 Launchers. env (skipping VAR=val, which stay words, -i,\n  -u NAME and -C D, -CD and --chdir=D as a folder the command runs in,\n  and BSD -P/-S taking a value), exec, nohup,\n  npx/bunx/pnpx, pnpm exec/dlx, yarn (a package's `@<version>` is\n  dropped), uv run (with --directory/--project, also with `=`, as\n  folders the command runs in; `uv run x.py` read as python x.py and\n  `uv run -m mod` as python -m mod), poetry run and pipenv run. Each re-reads the rest as a\n  command of its own. A package runner's command is re-read only when it\n  is a program purlis reads (LOADS). Otherwise it is the package's own\n  tool, and its words are read as before (`npx prettier` names no\n  prettier). `uv run .` keeps the runner folder rule. A versioned name\n  (python3.12, node22, bash5) is read as its program.\n- D-1418-10 A value after `=`, whether an assignment's, an option's\n  (`--target-dir=./target`) or a JVM agent's options, is named only\n  as a file: one that exists, or one that does not exist yet and has\n  an extension. Such a value is a search path or an output folder as\n  often as code, so a folder named only that way stays writable\n  (`CARGO_TARGET_DIR=./target`, `OUT=./dist`, `PYTHONPATH=./src`,\n  `NODE_PATH=./lib`). This rule decides from what is at the path, and\n  a chat can make that: a folder where the file will be, or a dotfile\n  with no extension. So it applies only to values that load no code.\n  Amended in round 3: a variable or git setting that loads code\n  (RUNS_CODE, one table) names its value whatever is there, absolute\n  ones too:\n  - paths: BASH_ENV, ENV, ZDOTDIR, PYTHONSTARTUP, GIT_SSH,\n    GIT_ASKPASS, SSH_ASKPASS, core.hooksPath, core.fsmonitor,\n    core.askPass, gpg.program;\n  - path lists, split at `:` or a space: LD_PRELOAD, LD_AUDIT,\n    DYLD_INSERT_LIBRARIES;\n  - commands, read as a line: PROMPT_COMMAND, GIT_SSH_COMMAND,\n    GIT_EXTERNAL_DIFF, GIT_PAGER, GIT_EDITOR, GIT_SEQUENCE_EDITOR,\n    GIT_PROXY_COMMAND, EDITOR, VISUAL, PAGER, core.sshCommand,\n    core.pager, core.editor, sequence.editor, diff.external,\n    credential.helper;\n  - options, read as that program's: NODE_OPTIONS (node's\n    -r/--require/--import/--loader), RUBYOPT (ruby -r),\n    JAVA_TOOL_OPTIONS, JDK_JAVA_OPTIONS and _JAVA_OPTIONS\n    (-javaagent:);\n  - PERL5OPT: each -I folder, and each -M/-m module as Module/Name.pm\n    from the command's folders and from each -I folder.\n  Search-path variables (PYTHONPATH, NODE_PATH, PERL5LIB, RUBYLIB,\n  LD_LIBRARY_PATH) are left out on purpose. They name the repo's own\n  source folders, which the chat edits by design (as D-1418-1 leaves\n  include folders), so they keep the file-only rule. node's `--env-file` and deno's\n  `--config`/`--import-map` are read as values, not files.\n  `deno --lock`/`--env-file`/`--v8-flags` take their value only after\n  `=`, so they are not skipped as taking the next word.\n- D-1418-8 Profile size: cap rather than `-f <file>`. A file in the\n  chat's tmp could be rewritten by an earlier process of the same chat\n  between the write and sandbox-exec's read. Any other place would be\n  a new trusted location. The cap is on the profile's bytes (768 KiB,\n  three quarters of ARG_MAX: two configs at MOST_NAMED with firmlink\n  twins fit, and a quarter is left for the environment). Bytes are\n  the quantity E2BIG is about, and a count of denials is not. Past it the start is refused with a sentence, through the\n  error path CONTROL already uses (fail closed).\n\nTwo existing tests used `gawk -bf<abs>` as an example of a speculative\nlater-letter split. Under D-1418-1 that form is read by gawk's grammar,\nso they now use `cc -xI<abs>`, whose letters purlis does not know.\n\nLeftovers are filed as one checklist, issue 1425. Round 2 adds `bash -s\narg`/`ruby -S rake` and the first-message E2BIG to it.\n\nCloses #1418\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-10-07T11:20:29+04:00",
          "tree_id": "605ebd03ab47612773226fc08a7c4b5ec730468b",
          "url": "https://github.com/purlis/purlis/commit/38e55d8b8c997b19117ccae8ca78c96bee2da0cc"
        },
        "date": 1791357720008,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.5150684999999999,
            "unit": "ms",
            "extra": "median of 5 runs: 0.487, 0.509, 0.515, 0.517, 0.522 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 17.003363999999998,
            "unit": "ms",
            "extra": "median of 5 runs: 16.318, 16.744, 17.003, 17.054, 17.097 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 104.834697,
            "unit": "ms",
            "extra": "median of 5 runs: 103.376, 104.702, 104.835, 105.334, 105.514 ms"
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
          "id": "8abf6e50b31e25909239a8fef074fe95e7677545",
          "message": "The dev channel build takes about half the time\n\nA dev publish took 37 to 45 minutes, 34 of them in the macOS job. The\nnine decisions of issue 1429 (ADR 0092):\n\n- A dev channel build is compiled with `dev-release`: `release` with\n  thin LTO over sixteen codegen units, and nothing else changed. The\n  command line, the built-in extension and the app all use it.\n- A dev publish makes no `.dmg`, and takes down the one an earlier\n  publish left on the `dev` release.\n- When the AppImage is required, one `tauri build` makes both Linux\n  bundles, on both channels. A build started by hand keeps two steps.\n- `purlis --version`, About purlis and the run summary name the\n  profile. The workflow reads the line off the binary it built and\n  stops if it is not the profile the run asked for.\n- A nightly profile guard builds `main` as a stable build is built,\n  outside the `release` environment, and publishes nothing. A red\n  night opens one issue or rewrites it; a clean night shuts it.\n- The stable build keeps `release`, its `.dmg` and its checks. No job\n  with a secret restores a cache, on either channel.\n\n`plan` now names the two events that get the `release` environment (a\ntag push and a green `ci`), so the schedule gets no key and neither\ndoes an event added later. A build started by hand can pick the\nprofile, which is how the two were timed.\n\nMeasured on the runners, same commit, unsigned path: macOS command\nline 490 s to 263 s and app 1003 s to 708 s; Linux 350 s to 228 s and\n755 s to 593 s. With the `.dmg` gone the macOS job comes to about 17\nminutes. The first real dev publish is the measurement of the whole.\n\nCloses #1429\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-10-07T15:58:29+04:00",
          "tree_id": "95610a72e7a21f8e903c3f0a1fbc1768be4d70ec",
          "url": "https://github.com/purlis/purlis/commit/8abf6e50b31e25909239a8fef074fe95e7677545"
        },
        "date": 1791374524835,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.517252,
            "unit": "ms",
            "extra": "median of 5 runs: 0.487, 0.496, 0.517, 0.529, 0.534 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 17.1458045,
            "unit": "ms",
            "extra": "median of 5 runs: 16.930, 17.079, 17.146, 17.202, 17.426 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 104.554148,
            "unit": "ms",
            "extra": "median of 5 runs: 102.514, 104.345, 104.554, 104.999, 105.139 ms"
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
          "id": "37c74d0e6945400dad80cb021b383f4e0692f2a4",
          "message": "chats: Restart chat, and a Notice when a sandbox change leaves chats behind\n\nA chat keeps the sandbox it started with. After a sandbox setting changed,\nrunning chats kept the old one and nothing said so; the only ways to apply it\nwere to close the tab and resume, or to press Allow on a block Notice.\n\n- Restart chat on a chat tab's menu and in the palette: the chat's program\n  ends and starts again on the same conversation, with the project's sandbox\n  as it compiles now. Mid-turn the row reads \"Restart chat … when this turn\n  ends\" and the restart waits for the turn; the row's note says the wait too,\n  so the palette says it. While a restart runs the chat's pane says\n  \"Restarting this chat…\", and a chat waiting on a permission prompt says, in\n  the core's sentence, that it restarts once that is answered.\n- A Notice after a sandbox setting changes while chats run under an older\n  sandbox: how many, Restart them (Restart it for one) and Dismiss. One\n  Notice for each change. \"Older\" is decided by compiling, never by a file\n  having been written, and by what settings decide alone: the hosts, what the\n  presets widen, and the folders every chat may write. The window asks again\n  after its own sandbox commands return, so revoking a folder in Settings'\n  Granted list raises it.\n- Settings › Sandbox says a change applies to a chat from its next start.\n\nOne restart path. The grant restart (issue 1342), Restart now for persona\ngrants (issue 1362) and Restart chat all go through Chats::restart and the\nrestart_chat command, driven by the window's one \"once the turn ends\" driver.\nThe person asks with ask_chat_restart, a command of the window with no\nhook-line route. restart_chat_for_grant and the old restart_chat are gone;\nChats::start_again is Start fresh alone again. The window's names say restart\nand no longer grant (restartOwed, chatRestarted, answerRestart, restartsSaid,\nand the cause restart:<session>).\n\nA hang that is on main too is mended here. Chats::restarted took what was\nqueued for a chat out of `owed` inside an `if let` and put it back under the\nsame lock, so anything queued while a restart ran (Allow on a block's Notice\nwhile that chat restarted) left the restart waiting on its own lock for good,\nand every close, grant and owed list of the project behind it. The queue is\nnow taken out in a statement of its own, and what was allowed meanwhile is\ncarried to the new run, as D-1342-13 meant.\n\nA person's opt-out is never inherited (ADR 0067 §7): a chat that ran without\nthe sandbox restarts in it, and its tab says so. Nothing here offers an\nopt-out, so a policy that forbids one has nothing to hide.\n\nDecided in implementation:\n- D-1428-1: one core restart, Chats::restart, refused for a chat owed\n  nothing; the person's ask owes it one with nothing to tell.\n- D-1428-2: a chat that reports no state restarts at once when the person\n  asks; its row and the Notice say purlis cannot tell whether it is mid-turn\n  and promise no wait, and only a known mid-turn chat waits.\n- D-1428-3: a chat with no conversation to resume is refused on its pane,\n  with what to do: send it a message first, or Start fresh.\n- D-1428-4: a restart the person asked for that is refused is not owed again,\n  and Restart now asks anew (the persona Notice no longer remembers a press).\n- D-1428-5: the Notice does not count a chat started without the sandbox, a\n  shell, a chat whose sandbox cannot be compiled now, or a chat already owed\n  a restart or restarting.\n- D-1428-6: a dismissal is kept for each chat by a SHA-256 of what settings\n  decide for it now, until no chat is behind that change; any change nobody\n  dismissed shows the Notice, which then counts every chat behind.\n- D-1428-7: the row's mid-turn and no-state words are read as the menu is\n  drawn (Offer.midTurn, Offer.noState), so a chat's turn does not rebuild the\n  catalogue; the palette shows the plain title and the note with the wait.\n- D-1428-8: no opt-out is offered on a restart; the picker and a block's\n  Notice remain the two places to choose it.\n- D-1428-9: a test-only seam decides an off-profile chat's sandbox, because a\n  stand-in harness lives where a chat can write and could never start\n  sandboxed through the real program check.\n- D-1428-10 (dispatcher): the Notice compares only what settings decide, not\n  what a start finds by walking the project's tree; Confines records the\n  writable folders for it, and a run for the chat is not widened by them.\n- D-1428-11: each chat is compiled for the comparison with the persona\n  grants and the chat's own grants it started with, so a grant for one chat\n  or a persona's hosts allowed on its tab never reads as the project's change.\n- D-1428-12: an ask while a chat restarts asks for nothing more, and only\n  what has something to tell is carried to the new run.\n- D-1428-13: a pane says one line of its restart: why it failed, else why\n  not yet, else that it is under way, else that it waits for the person.\n\nCloses #1428\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-10-07T19:31:38+04:00",
          "tree_id": "c94ecd8cc920f7e978fbd19d6e441e1280ee2678",
          "url": "https://github.com/purlis/purlis/commit/37c74d0e6945400dad80cb021b383f4e0692f2a4"
        },
        "date": 1791387628651,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.486674,
            "unit": "ms",
            "extra": "median of 5 runs: 0.468, 0.484, 0.487, 0.493, 0.506 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.761324,
            "unit": "ms",
            "extra": "median of 5 runs: 16.653, 16.654, 16.761, 16.795, 16.818 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 102.45965050000001,
            "unit": "ms",
            "extra": "median of 5 runs: 101.966, 102.379, 102.460, 102.817, 103.161 ms"
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
          "id": "8f3d5172c4a3d1f19722a60b3551c0e90fe42246",
          "message": "hooks: a chat's spool is a file per line, so hooks that spool at once lose none\n\nA hook whose line the host does not take keeps it in the chat's spool for the\nnext open of the project. The spool was one file per chat, appended under a\nlock that each holder kept while it read the whole file and synced. A hook\nwaited at most 250 ms for that lock, so with several hooks of one chat behind\none sync on a loaded machine, a waiter ran out of its wait and said its line\nwas lost. `a_host_restart_during_a_busy_turn_loses_no_event` showed it twice\nin CI (176 of 200, \"another process held this chat's spool for 250 ms\").\n\nThe spool is now a folder per chat, `spool/<n>/`, with a file per line. A hook\ntakes the next number by making `<key>.<seq>.part` exclusively, writes and\nsyncs its line there, links it as `<key>.<seq>.json`, removes the `.part` name\nand syncs the folder. No hook waits for another. The line, its number, its MAC,\n`keys.json`, every check the drain makes and the sandbox's denial of `spool/`\nare unchanged. The drain reads the files that have a line's name, hands them\non, and removes what it read, so hooks may go on spooling while it runs and no\nline it read is handed on twice. A `<n>.jsonl` a build before left, or that\none of its hooks still writes, is drained first and as it was, and a line it\ngave is not taken from the folder again.\n\nA hook is still bounded: the write runs on a thread of its own and is given\n1 s, and a spool that already holds 16,384 names takes no more. Past the cap,\nor on a disk that refuses the write, the hook says the line is lost, as\nbefore. Past the second it now says the line may be lost, since the write is\nleft to finish and the next open records the line if it did.\n\nWhat this does not change: the drain forgets a key once it has drained it. A\nline still being written while a drain runs, like a hook that outlives the\ndrain, is rejected as no-key at the next one and its content is not recorded.\nThat needs a ruling on keys.json and is the first follow-up on issue 983.\n\ndocs/plane-format.md has the new shape and the old file as read-only; ADR 0068\n§6 has a dated note that the shape as built is a folder.\n\nSeen red, then green: a pure test with 16 writers and 2,000 lines for one chat\non a disk that takes 20 ms per line lost 1,208 lines on main's code, each with\nCI's message, and loses none now. The delivery test keeps each hook's error\nand prints them when a line is short. After review, eight more tests were seen\nred on the first head and green now: the copy between the two stores, a line\nfile against its name, a `.part` said and one dated ahead, the last number,\nthe in-flight line, the keys' order, a write that stops, and \"may be lost\".\n\nDecided in implementation:\n- D-983-1: A file per line with the number taken by an exclusive create, over one file appended without a lock (numbers need a writer's turn, and a drain's truncate races a write) and over a shorter critical section with a longer wait (still a wait that can run out, and the drain holds the lock while it records).\n- D-983-2: The number stays contiguous per key and under the MAC: a hook tries the number after the highest it listed, then the one after for each hook that took it first, so gaps mean what they meant.\n- D-983-3: A line gets a `.json` name by a hard link, which fails rather than replace a line, and only after its bytes are synced; the name it linked is checked to be the file it wrote.\n- D-983-4: A number a dead hook took is a gap even while its `.part` file is there, so a `.part` file cannot hide a removed line; the drain removes one an hour old.\n- D-983-5: One bound for the whole write, 1 s on a thread, replaces the 250 ms lock wait and the 250 ms directory wait. A line said lost for the time may still be drained, the side the delivery path already errs on.\n- D-983-6: The size cap counts names, 16,384, where it counted 16 MiB read: a hook lists names and reads no line.\n- D-983-7: The drain holds a lock on the folder that only another drain waits for; hooks never take it.\n- D-983-8: Lines under two keys of one chat are handed on in the order keys.json holds the keys, the order the host issued them in; file times are not read (amended in review, from the files' times).\n- D-983-9: The old `<n>.jsonl` is drained at every open and emptied, never removed and never created, since a hook of the build before may still append to it (N-1).\n- D-983-10: A line in flight while a drain runs is a gap at that drain and is rejected as no-key at the next, unless the host still holds its key, which only a drain that stopped leaves; that is the loss a hook outliving the drain already had, pinned in tests, and keys.json is not changed here (corrected in review).\n- D-983-11: The seam is `Bounds`, a hook's limits and a call made as the line reaches the disk, private to the module; tests slow the disk, fill it, shrink the cap and shorten the wait through it.\n- D-983-12: The drain gives a chat's folder what its old file already gave, and the same key, number and MAC there is repeated; the MAC is compared because a hook that changed builds mid-chat has a number 1 in both.\n- D-983-13: A line's file must hold that one line under its name's key and number, or it is unreadable: hooks number by names, so names are held to the lines.\n- D-983-14: A `.part` with no line of its name is said at each drain as rejected, why `unfinished`, a new value of an existing event's field; one dated in a time to come is a dead hook's.\n- D-983-15: After a link, a line's name that is already gone was drained, so the line landed; a hook removes a `.part` name only while it is still its own file.\n- D-983-16: A line the spool was still writing when the hook stopped waiting \"may be lost\", every other \"is lost\"; a write that ends without answering says it stopped, not that the disk was slow.\n\nRefs #983\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-10-07T23:42:42+04:00",
          "tree_id": "3f9ef8882add7d39b13bb593623873c61eb3fb25",
          "url": "https://github.com/purlis/purlis/commit/8f3d5172c4a3d1f19722a60b3551c0e90fe42246"
        },
        "date": 1791403445232,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.5339725,
            "unit": "ms",
            "extra": "median of 5 runs: 0.512, 0.519, 0.534, 0.538, 0.540 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 17.2264915,
            "unit": "ms",
            "extra": "median of 5 runs: 16.318, 17.213, 17.226, 17.240, 17.266 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 104.068119,
            "unit": "ms",
            "extra": "median of 5 runs: 102.472, 103.623, 104.068, 104.764, 106.107 ms"
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
          "id": "11cebd79cf8549c98e5d11022a240b4ec623fec2",
          "message": "sandbox: what waits to be told to a chat is denied where its hooks run outside (D-T59-19)\n\nA dispatched chat's report, and purlis's own word on a dispatch the person\nwas asked about, wait in the project's state folder until the chat they\nare for takes its next turn. A chat could write there, so it could put a\nline into another chat's turn in purlis's voice, and it could read a report\nwritten for another chat.\n\nThat folder is now denied, for reading and writing and under both names of\nthe state folder, to a sandboxed chat of a harness whose hooks run outside\nits sandbox, which is asked of the harness and is Claude Code today. The\napp leaves those files and purlis's hooks take them, so nothing inside\nsuch a chat needs the folder.\n\nIt is not denied where the sandbox is a wrap around the whole harness,\nCodex and opencode today: the hooks that deliver run inside the wrap and\nread and remove each file, so a denial there would cut every report off.\nOn those a chat can still read and write the folder, and what stands is\nthe check each file gets as it is read. The denial reaches them once\ndelivery moves onto the hook socket (issue 1457).\n\nOne command a chat can run did touch the folder: a workspace rename moves\nthe reports kept for the workspace. Run inside a chat that is denied the\nfolder it now leaves them and says who moves them, and the app does, when\nthe project is next opened, from the rename's own journal: only where the\nold workspace is gone and the new one is there.\n\nTested in the compiled forms of both kinds of harness, and live under the\nwrap: a report left for a wrapped chat can still be taken. ADR 0067's\nclass 2 is amended, and docs/plane-format.md and the changelog say what\nholds on which harness.\n\nD-T59-19, delegated by the dispatcher and flagged for the operator.\n\nRefs #1437\nRefs #1457\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-10-08T00:35:09+04:00",
          "tree_id": "195884ec47b498806e7ecf24efa0c375e1a7f74d",
          "url": "https://github.com/purlis/purlis/commit/11cebd79cf8549c98e5d11022a240b4ec623fec2"
        },
        "date": 1791406973004,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.507305,
            "unit": "ms",
            "extra": "median of 5 runs: 0.493, 0.505, 0.507, 0.519, 0.532 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 17.194558999999998,
            "unit": "ms",
            "extra": "median of 5 runs: 16.756, 16.808, 17.195, 17.416, 17.543 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 103.818565,
            "unit": "ms",
            "extra": "median of 5 runs: 102.602, 103.479, 103.819, 105.280, 106.374 ms"
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
          "id": "0c1db31bf0e6dfccaae4592541503a1da29ad82d",
          "message": "Train 59: the joined tickets agree\n\nSix ticket commits were replayed onto the wiring: the person's ask and what\nhappens when a persona chat ends (issues 1438 and 1443), wait, list, cancel\nand messages (1441, 1442), stop and the needs-you roll-up (1448), and\npersonas no longer being sub-agents (1451). Each was written against a tree\nwithout the others. This commit is where they meet.\n\nD-T59-j1. A task the person started from a chat's tab gives that chat no\npower over it. Its record names the tab's chat, because the report goes\nthere, and every power over a task is one rule in the core\n(`dispatched::is_owner`): wait, read, cancel, tell and answer are refused in\nthe sentence any stranger is told. The task sends that chat no note and no\nquestion either (`dispatchtalk::up`). The chat reads the report and sees the\ntask listed, marked as started by the person. Tested in the core.\n\nD-T59-j2. A chat's `purlis dispatch list` reads the one list of a chat's\npersona chats (`handoff::persona_chats`), which is also what closing the\nchat asks about and what the window marks a task with. One word for a task\nstopped on a prompt: `waiting on the operator`, which is the fourth\nacceptance line of issue 1443. A task the chat dispatched itself still says\nmore where the app knows more: idle, cancelling, asking this chat, how it\nreported.\n\nD-T59-j3. One stop, and one word for \"the operator stopped it\".\n- The engine is the stop machine of issue 1448 (`stopping::Stops`). The\n  close dialog's \"Stop them\" begins the same stop for the chats at work\n  below (`stopping::press_below`), under the hold the asking chat is closed\n  under. It no longer closes them by a road of its own: each gets its one\n  short turn, and none starts a chat meanwhile.\n- The word is written in one place (`handoff::operator_stopped`) and told\n  by one mark, `Handback.stopped`, in one sentence. Stop, \"Stop them\" and\n  the Close of a task that had not reported all end there. The mark carries\n  whose task it was and the stopped chat's session record. The task part's\n  own `stopped` key and its second sentence are gone.\n- A task settled by a stop is settled for good, so the close that follows\n  says nothing twice.\n- Cancel stays the asking chat's own. A task in a stop is not cancelled as\n  well: the cancel is refused in fixed words. A cancelling task can be\n  stopped: its cancel stands down and nothing more is typed for it.\n- A chat in a stop is typed its stop's one line and no other.\n- When a program ends, one mechanism speaks: the cancel's report first,\n  the stop's word for a chat being stopped, else \"ended without a report\".\n- What a stop asks for while a close holds the deciding lock is carried\n  out when the hold is let go. Ending a chat takes that lock, and the\n  close of a chat whose parent waited in a stop would not have returned.\n- A cancelled task whose tab is closed, or that the person stops, is told\n  of as stopped: the person's act is the later word (the dispatcher's\n  ruling). The report a close wrote for it as cancelled is gone, with the\n  call that wrote it under the close's own lock.\n\nD-T59-j4. \"A chat in a stop, or below one, starts nothing\" is about what\nthe chat starts. The person may still ask a persona from that chat's tab,\nand a test says so. A stop now takes the lock a dispatch is decided and\nreserved under, and a dispatch decided first is asked again before its\nprogram runs, so it does not start. This amends D-1443-14.\n\nD-T59-j5 (D-1441-15). The dispatch record has a `cancelled` outcome, in\ndocs/plane-format.md first. A cancelled task is no longer recorded as\nfailed.\n\nD-T59-j10. And a `stopped` outcome beside it, for a chat the person\nstopped or closed before it reported. It did not fail by itself, and its\nrecord no longer says it did.\n\nD-T59-j6. The docs said only the person can stop a chat and that no\ncommand or tool does. A chat can cancel a task it dispatched; only the\nperson stops any other chat. The close dialog says what \"Stop them\" now\ndoes.\n\nD-T59-j7. The person's ask rides the wiring's one decision path\n(`dispatch_it` with `By::Person`): the same limits, profile, record and\npolicy locks as a chat's ask, and no grant. The branch's own twin of the\ndecision is gone. A refusal is still said in the person's words, now over\nthe wiring's refusal kinds.\n\nD-T59-j8 (delegated, flagged for the operator, under V98b). The\nsubcommands of issues 1441 and 1442 are pre-allowed for Claude Code each by\nits own name, beside the wiring's four named rules and with no wildcard\nover the rest: `--wait`, `wait`, `list`, `cancel`, `tell`, `note`, `ask`\nand `answer`. ADR 0064 says in one line each what it can reach.\n\nD-T59-j9 (D-1448-4). A chat the person is stopping is still given a\nticket. Its one last report is sent on one, by either report command, and\nboth asked for a ticket that a stopping chat was refused, so the report\nits stop asks for could not be sent. What is refused is spending a ticket\non an open or a dispatch, which is where it is known what the ticket is\nfor. An app test sends one report by each command, a second that is\nrefused, and an open and a dispatch that are refused.\n\nD-T59-j11. The stop's word names its chat by a task's rule when it is\nread, as the app's word on a held dispatch does (D-T59-14): the name is\ndrawn inside purlis's own sentence. The person can name a chat with the\nmarks purlis's lines are made of, so the app writes the name with them\nreplaced, as a stamp replaces the backtick, and the word of a chat so\nnamed still arrives. A report's name is a chat's, as before.\n\nD-T59-j12. The stop's word carries how the chat was started. A stopped\ntask reads \"the task you dispatched to it\", a stopped handoff \"the work\nyou handed to it\".\n\nD-T59-j13. The refusal of a profile that asks nobody (D-T59-13) holds on\nthe person's road, which is the same decision path. A test asks a persona\nwhose definition names such a profile from a chat's tab, and nothing\nstarts.\n\nD-T59-j14. The messages between an asking chat and its tasks wait in\n`said-<chat>/` inside the store D-T59-19 names, so its one rule holds\nthem: denied with the reports where a harness's hooks run outside its\nsandbox, and left reachable where the hooks run inside it, which is where\nthe hook that takes them runs. A test leaves a message and holds its file\nto both halves.\n\nD-T59-j15. Two lock mistakes, both found by reading after a run that did\nnot end.\n- A chat's list took the ledger's lock twice on one thread for a task it\n  dispatched that was waiting on the person, and never came back. It asks\n  the hold it already has. This was the joining commit's own, in j2.\n- A close holds the deciding lock while it ends the chat's program. The\n  end of that program is heard on another thread, which ended a chat still\n  in a stop and took the same lock to do it. So closing the tab of a chat\n  that was being stopped could wait on itself, where the close waits for\n  the program's end to be heard. A close now lets go of the chat's stop\n  before it ends the program, so that thread finds nothing to end. And a\n  close settles what the chat owed whatever became of the word, which is\n  what the end of a program relies on to not wait for the lock.\n\nD-T59-j16. The app tests that take those locks are bounded. An ask after\na task, a stop, a close and a close that stops the chats below are run on\na thread of their own in the tests of this commit, and a call that has not\ncome back in two minutes fails its test with a sentence, where it would\nhave hung the run with nothing said.\n\nTests the first full run found red, each read against the joined code.\nNone was a product fault but the two locks above.\n- The nine pane tests of issues 1441 and 1442 never had a report taken:\n  their stand-in is read as Claude Code, whose reports the board takes\n  only with a process and a conversation (ADR 0024), and the helper sent\n  neither. It sends both for such a chat now. One of them expected a\n  cancelled report from closing a cancelling task's tab; that is the\n  person's stop (D-1443-11, j3), and it says so.\n- A restart was acted out with the person's Close, which tells the asking\n  chat a task was stopped. It takes the restart's own road now.\n- A wait on a task whose tab was closed is answered with what the asking\n  chat was told, which is that it was stopped.\n- A task's report names its chat on the asking chat's row and is no item\n  (D-1448-6), and the report of the person's own task whose tab chat has\n  gone raises its item by a need of the app's (D-1448-8).\n- \"the persona devops\" is not \"the person\": the test of the refusals the\n  person reads compares words.\n\nD-T59-j17. An app's refusal is printed whole wherever the command prints\none, as D-T59-12 did for the dispatch itself: the refusal of a report, by\neither report command, the refusal of a handoff, and the note about a\nprofile that fell back on a handoff. The refusal of a message or of an ask\nafter a task is whole from the commit that added those commands. Each was\ncut at the 160 characters a name is drawn within, which took the last\nsentence, the one that says what to do.\n\nThe pre-allow test of D-T59-18 holds both lists now, exactly and in order,\nand that no rule is a wildcard over the rest.\n\nD-1448-17 holds. A report to an idle asking chat raises no needs-you item\n(`reported_to_its_asker`), and the asking chat is woken by the report\nitself (`dispatched::told`, typed once the lock is let go). Both are on\nthis train.\n\nA tab's menu test from issue 1448 now expects Restart chat above Start\nfresh, which the base train added.\n\nRecorded behaviour gains nine scenarios for the new subcommands that need\nno app (fold-in 10 of the review of issue 1441): what `list`, `wait`,\n`cancel`, `tell`, `answer`, `note` and `ask` say where no app answers, and\n`tell` and `note` refusing an empty message. Written in the recorder's\nformat and run by hand against the built command; the replay holds them.\nNo recorded row that was there moves: no sentence a recorded scenario\nprints is changed here.\n\nBindings are regenerated.\n\nRefs #1438\nRefs #1441\nRefs #1442\nRefs #1443\nRefs #1448\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-10-08T01:20:08+04:00",
          "tree_id": "3b89c5f71eaead25221b61e2fbe95aa12038876b",
          "url": "https://github.com/purlis/purlis/commit/0c1db31bf0e6dfccaae4592541503a1da29ad82d"
        },
        "date": 1791410722960,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.47187650000000003,
            "unit": "ms",
            "extra": "median of 5 runs: 0.459, 0.471, 0.472, 0.473, 0.476 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.357584,
            "unit": "ms",
            "extra": "median of 5 runs: 16.179, 16.309, 16.358, 16.470, 16.914 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 102.2169155,
            "unit": "ms",
            "extra": "median of 5 runs: 100.907, 101.415, 102.217, 102.518, 103.029 ms"
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
          "id": "0be46c8ba158d16f9c34d42f474f1db70219169e",
          "message": "Train 61: a handoff is held to the limits of the workspace it moves into (D-T61-7)\n\nReconciled: issue 1453 holds a task sent into another workspace to that\nworkspace's dispatch limits as well as the asking chat's, with a 0 there\nholding against a persona's own value (D-1453-17). A handoff has always named\na workspace, and the limits are the project's own configuration, so the one\ndecision now reads them for a handoff too: the same call, under the same lock,\nfirst refusal wins. The workspace is read by the name its folder has, so a\nspelling that differs only in case reads the same limits.\n\nThis amends D-T61-2 in one point. The rest of it stands: a handoff takes no\n`--in`, is cut no worktree, and the standing grant a chat nobody is at needs\nto cross workspaces stays a task's rule (ruling V99f is a handoff's).\n\nSeen in the core: a handoff into a workspace set to 0 is refused in that\nworkspace's sentence for a chat a person is at and for one nobody is at, a\npersona's value does not lift it, the asking chat's own 0 answers first, and\nwith nothing set it opens. One test in the app holds the same through a real\nhandoff; it starts a chat, so only CI runs it.\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-10-08T08:03:16+04:00",
          "tree_id": "c98619ade7834f1e67fcb6f0aff630bdcc60b955",
          "url": "https://github.com/purlis/purlis/commit/0be46c8ba158d16f9c34d42f474f1db70219169e"
        },
        "date": 1791432289164,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.438983,
            "unit": "ms",
            "extra": "median of 5 runs: 0.419, 0.435, 0.439, 0.452, 0.455 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.480835,
            "unit": "ms",
            "extra": "median of 5 runs: 16.366, 16.418, 16.481, 16.605, 16.615 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 103.56867700000001,
            "unit": "ms",
            "extra": "median of 5 runs: 102.025, 102.306, 103.569, 103.831, 104.532 ms"
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
          "id": "1a086ba68815cf4367196e72c492be43346c71d1",
          "message": "worktree: a sandboxed chat in a branch folder commits through the app\n\nA linked worktree keeps its index, its HEAD and its objects in its clone's\n`.git`, outside what a sandboxed chat may write, so `git add` and `git\ncommit` are refused there. By the operator's ruling (V99h) the worktree's git\ndirectory is not opened to the chat: purlis brokers the commit.\n\n`purlis worktree commit -m <message> [--all | <path>…]` sends the app an ask\nover the chat's hook socket that names the message and what to stage, and\nnothing else. The app commits in the folder it recorded the chat as standing\nin, on the branch that folder is on, with the repository's identity or the\nperson's and the chat's trailers (ADR 0074). Every git call is held to the\nchecked git directory wherever in the folder the chat stands, reads no\nglobal or system config, and runs no hook. A repository whose config names a\nprogram, has a config of the worktree's own, borrows objects or signs its\ncommits is refused. What was staged is read back: another repository inside\nthe folder, a change to `.gitmodules`, a name that differs from another only\nby case, and a file its attributes mark for a content filter are unstaged\nand refused. purlis's own scan runs in the app. It only commits.\n\nOne commit runs in a branch folder at a time, by its own deadlines. The app\nnotes what it staged in the worktree's git directory, so a commit it was\nstopped in the middle of is unstaged by the next ask.\n\nA dispatched task's own worktree is such a folder, and the chat started in\nit is now told the command where it was told it could not commit.\n\nADR 0067 is amended with the measurement the ticket asked for and the\ndecision. A chat started in a branch folder is told the command. The ticket\nstays open: no linked worktree has been measured under a sandbox's denial.\n\nDecided in implementation:\n- D-1055-1: a plain clone already commits inside the sandbox, by named\n  denials. The same recipe for a worktree was not taken (ADR 0067's note).\n- D-1055-2: the repository's own hooks do not run for a brokered commit; the\n  answer names each one that is there.\n- D-1055-3: the command is `purlis worktree commit`; `--all` is every change\n  to a tracked file, and a new file is named by path.\n- D-1055-7: no approval, and no pre-allow on Claude Code yet: the rule that\n  asks about a pre-allowed command sharing its call is not on main.\n- D-1055-8: the folder is the one the app recorded for the chat, so a chat\n  standing above a branch folder does not commit in it.\n- D-1055-9: staged changes the app did not stage are refused, so what is\n  committed is exactly what the ask named, and a refusal can put the index\n  back.\n- D-1055-10: the repository's own `user.name` and `user.email` win over the\n  person's global ones, as they do for git.\n- D-1055-11: a repository that signs its commits is refused: a signer is a\n  program and may prompt.\n- D-1055-13: a file marked for a content filter is refused, not committed\n  unfiltered: the person's filters are in the config this commit does not\n  read.\n\nTests: the ask's shape and the path rules are pure; the git half runs\nagainst real git on a worktree whose git data is kept beside it; the tests\non purlis's own layout write under a `.git`, so CI is their first run.\n\nRefs #1055\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-10-08T08:26:52+04:00",
          "tree_id": "757dc9c755910f8a181e06dfaf330e619f1bed54",
          "url": "https://github.com/purlis/purlis/commit/1a086ba68815cf4367196e72c492be43346c71d1"
        },
        "date": 1791433704879,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.5309984999999999,
            "unit": "ms",
            "extra": "median of 5 runs: 0.493, 0.507, 0.531, 0.549, 0.549 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 17.117109499999998,
            "unit": "ms",
            "extra": "median of 5 runs: 16.515, 16.589, 17.117, 17.214, 17.343 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 104.508757,
            "unit": "ms",
            "extra": "median of 5 runs: 103.008, 103.317, 104.509, 104.531, 106.128 ms"
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
          "id": "1d70fbf15c11344b85fbfeedc868afcd4dedabac",
          "message": "A Notice in a pane's corner wraps, and what it opens is drawn under it\n\nReported by the operator from the dev build, 2026-10-08: \"ui is broken for\nquestions modals\". With two panes side by side, a refused vault's Notice had\nits sentence one word wide and fourteen lines tall, its three buttons broken\nover up to five lines beside it, and the block a button opened drawn to the\nright of the Notice and off the window. The dispatch-grant Notice in the other\npane had its brief over the middle of the pane, its buttons far below, and no\nsentence in view. The Notice was drawn over the Ask devops dialog, that\ndialog's \"What to ask\" box was a third of its width with its buttons below the\nwindow, and the band and the sidebar were cut at the left.\n\nCause. `Notice at=\"pane\"` was made for one short line (ADR 0062). `.notice-pane`\nwas one flex row that did not wrap, and the pane's corner was itself one row\nthat did not wrap, so a Notice's `under` was the next ITEM of the corner's row:\nbeside the Notice. Nothing bounded any of it by the pane. A button that\noverflowed was then focused, and the window (`overflow: hidden`, which a focus\ncan still scroll) was scrolled sideways to show it: that is what cut the band\nand the sidebar. The corner's `z-index: 2` sat in the page's root stacking\ncontext, where a dialog portaled to the body has no number, so the corner was\nover every dialog.\n\nWhat changed, as properties of the shared pane Notice and the pane's corner:\n\n- `Notice at=\"pane\"` draws one box (`notice-pane-box`): the line, and under it\n  what a way out opened. The band and the drawer are drawn as before.\n- The corner is a column: the chat at a glance (`pane-chips`), then the pane's\n  Notices one under another (`pane-notices`), never wider or taller than the\n  pane, scrolling when together they are taller. The pane's frame clips.\n- The sentence has the row. The ways out stand beside it only when all of it\n  fits unwrapped, and otherwise go under it; a button is not broken unless it\n  alone is wider than the Notice.\n- `#root` is one stacking context, so nothing in the window is over a dialog\n  or a menu. The terminal is one too, so its own layers stay under the corner.\n- Ask {persona}: the box you write in takes the row, the dialog fits the\n  window, and its answers stay at its bottom edge.\n\nBehaviour, the answer to \"is dispatching a manual job?\": it is the chat's own.\nThe chat had dispatched already and its request was held for the person; the\nbroken layout hid it, and the vault's Notice offered a second, manual, empty\nway beside it.\n\n- The dispatch-grant Notice is the pane's first, and reads sentence, answers,\n  then the brief in a box of about eight lines.\n- A refused vault whose persona this chat has already asked says so and offers\n  Show the request, which goes to that Notice, in place of Dispatch to\n  {persona}…. With nothing held, Dispatch to {persona}… is offered as before.\n- Ask {persona} opened from a chat's Notice says the chat's words are not\n  copied into it. It is still empty: nothing a chat produced is typed for the\n  person.\n\nDecided in implementation:\n\n- D-PN-1 The line and what it opened are one bordered box in a pane, and stay\n  siblings under the strip and in the drawer, where the band moves them.\n- D-PN-2 The sentence is as wide as it reads (`flex: 0 1 auto`) with no fixed\n  minimum: a minimum would widen a short one-liner, and without one the\n  sentence is never narrower than the Notice unless everything fits on a row.\n- D-PN-3 A way out is `flex: none; max-width: 100%` and wraps normally, not\n  `white-space: nowrap`: it is never broken while it fits, and wraps instead\n  of being cut off when it alone is wider than the Notice.\n- D-PN-4 40rem is the cap, on the stack. A Notice that has opened something\n  takes that width whole.\n- D-PN-5 `overflow: clip` on the pane's frame, not `hidden`: a hidden box can\n  be scrolled by a focus, which is the defect.\n- D-PN-6 The corner takes no pointer events of its own; its two rows do. It\n  can be as wide as the pane now, and the terminal is under the empty part.\n- D-PN-7 The stacking order is by `isolation: isolate` on `#root` and on the\n  terminal's holder. The stylesheet has no layer scale, and this adds no\n  number to it.\n- D-PN-8 The order in a pane is written, not ranked: the Notice that waits for\n  an answer before anything starts is first, the rest as they were.\n- D-PN-9 No label is shortened. \"Allow steward to use this vault\" is a verb\n  and what it acts on, as docs/ui-copy.md asks, and it now fits a row.\n- D-PN-10 The Notice's sentence stays purlis's own words. The chat's name for\n  its task is the chat's text, and is not put into it.\n- D-PN-11 Show the request puts the focus on the request's line, never on one\n  of its Allow buttons, so the next key cannot answer it.\n- D-PN-12 Both Notices read the held dispatches from one store\n  (`dispatchesHeld.ts`), so neither says one is waiting after it is answered.\n- D-PN-13 The persona's mark is the sentence's first word in a pane, so it is\n  not left alone on a row above a sentence that takes the next one whole.\n- D-PN-14 The dialog's rules are scoped to Ask {persona} (`ask-persona`), not\n  to every dialog or every settings row.\n- D-PN-15 The e2e case draws the shapes the components draw into real panes\n  and measures the built stylesheet: the suite's fake harness cannot make a\n  vault refusal or a held dispatch. Vitest holds the components to the shapes.\n\nTests: `Notice.pane.test.tsx` (the shape, and the rules as written),\n`VaultRefusedNotice.test.tsx` and `AskPersona.window.test.tsx` (the pointer,\nthe order, the dialog's line), `pane-notices.e2e.ts` (the measuring).\n\nCloses #1481\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>\nAssisted-by: claude-code\nPurlis-Chat: 01M4AREQXXRGBAEWM0SZ0N0WBN\nPurlis-Persona: steward",
          "timestamp": "2026-10-08T10:08:34+04:00",
          "tree_id": "305a9ac7f6bc1b56342dc504b8097acde367b5bd",
          "url": "https://github.com/purlis/purlis/commit/1d70fbf15c11344b85fbfeedc868afcd4dedabac"
        },
        "date": 1791439813592,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.6147245,
            "unit": "ms",
            "extra": "median of 5 runs: 0.595, 0.604, 0.615, 0.615, 0.617 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.468757,
            "unit": "ms",
            "extra": "median of 5 runs: 16.072, 16.217, 16.469, 16.568, 16.599 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 102.4769505,
            "unit": "ms",
            "extra": "median of 5 runs: 101.449, 101.615, 102.477, 102.526, 103.143 ms"
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
          "id": "7d7b166168fc041df8947b1e7e25ec4ea1cbe777",
          "message": "The app's move test writes the keyring stub where its fixture's project is\n\nThe 1Password fixture's project moved to a folder of its own beside the\nstand-in `op`, and one test still wrote the keyring stub under the old root,\na folder that no longer exists.\n\nIt now writes through the fixture's own state folder (`ctx.state`), so the\npath follows the project wherever the fixture puts it.\n\nRefs #1516\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>\nAssisted-by: claude-code\nPurlis-Chat: 01M4AREQXXRGBAEWM0SZ0N0WBN\nPurlis-Persona: steward",
          "timestamp": "2026-10-08T19:55:02+04:00",
          "tree_id": "c8601348b7bdea247ec17c95aae9e2a2336bfc2c",
          "url": "https://github.com/purlis/purlis/commit/7d7b166168fc041df8947b1e7e25ec4ea1cbe777"
        },
        "date": 1791475728559,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.53555,
            "unit": "ms",
            "extra": "median of 5 runs: 0.519, 0.528, 0.536, 0.547, 0.548 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.2565885,
            "unit": "ms",
            "extra": "median of 5 runs: 16.174, 16.207, 16.257, 17.092, 17.235 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 102.9442165,
            "unit": "ms",
            "extra": "median of 5 runs: 100.982, 101.670, 102.944, 105.768, 107.019 ms"
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
          "id": "918ce402c5d90ddc3d5dbbe3d759d85c520b2d5b",
          "message": "Where the tickets of this train meet: four seams the review found, and six smaller ones\n\nThe review of the joining work found four things wrong where two tickets meet,\nnone of them inside one ticket.\n\nA pane left on an ended task could draw another task's report. The finished row\nwas found by the asking chat and the task's name, and a chat dispatches again\nunder one name as a matter of course. FinishedTask now carries the number its\ntask's chat had when it ended in this launch (the ledger's memory of closed\ntasks says whether the number is still its own), and the pane finds its row by\nthat number and never by the name. A row with no number is no pane's.\n\nA session folded by itself over an open task that reads failed. A blocked task\nis no longer ended at its report and stays an open chat, and the list's own fold\ntook every end for over. Only an open chat that reads done or cancelled lets its\nsession fold now; one that failed, went without a report or only reported keeps\nit open, as a finished row that stands alone does. A folded row counts the open\nchats under it that are over with its finished tasks, so a done task the person\nis still reading is in a count.\n\nA task the person was looking at when its bound passed was never ended on a\nharness purlis hears nothing from, or where the reporting turn hangs: the bound\nwas spent, and nothing looked again once they looked away. When the hold goes\nand that turn is still not over, the bound is set again (Ends::Bound). Only for\nthe reporting turn: a later turn still has none.\n\nAfter a relaunch the core could go on taking a tab to show a task that was not\nput back, so the session on screen was notified about each time it needed the\nperson. A launch that leaves a finished task out takes `shows` off the tab that\nshowed it, and the window, which starts from what the core said each tab shows,\nsays \"its own chat\" for whatever it did not put back. docs/plane-format.md now\nsays what turns on `shows`.\n\nSmaller, in the same files. A look that found a task held re-reads once, so a\nmove that landed between the read and the answer is not lost. What a tab shows\nis said again where the core did not take it. A task's row stays in the Chats\nlist until the finished row that replaces it is read, so the rows below move\nonce. A comment that said a chat's number is dealt again at each launch says\nwhat is true. The three tests of the hold were seen failing against the old\ncheck.\n\nFrom the first CI run of the train, which was cancelled with its Rust job hung.\n\nThe hang was a test, and a drop with no bound under it. The test of \"a task\nthat had reported at a quit is a finished row at the next launch\" opened the\nsame project a second time in one process while the first was still open. Each\nopen binds the project's hook socket at one path, and a listener that is\ndropped wakes its own thread by connecting to that path: the first one's\nconnection reached the second listener, or nothing once that had gone, and its\njoin waited for good. Only where a socket can be bound, so not in the sandbox\nthe test was written in. The test now closes the app that quit before the next\none opens, under a bound. And a dropped listener waits five seconds for its\nthread and then leaves it, saying so: nothing that closes a project can be held\nfor good by it.\n\nThree older tests held expectations the tickets had changed and could not run\nwhere they were written. The refusal of a second report after a handoff now\nends \"Another answer needs a task\" (issue 1515's wording). A task whose report\nreaches its open asking chat is answered Finished, not Reported. A cancel of\nthe chat's own closed task says it has finished and that there is nothing to\ncancel; another chat's is still refused as not its own.\n\nThe breadcrumb did not shrink in a real engine: as an item of its line it was\nnever narrower than its own content, so the line clipped its end and no name\ngave way. It has `min-width: 0` now. The chats-list scenario looked for a row\nwith an empty second line, and every row's second line is always there and\nalways has something in the recorded markup: it now changes every second line,\nadding to some and emptying the rest, and checks that no row moves.\n\nThe Rust job in ci.yml has `timeout-minutes: 45`. It takes 19 on main; a hung\ntest now fails the job instead of holding a runner.\n\nRefs #1485, #1486, #1499\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>\nAssisted-by: claude-code\nPurlis-Chat: 01M4AREQXXRGBAEWM0SZ0N0WBN\nPurlis-Persona: steward",
          "timestamp": "2026-10-08T21:54:41+04:00",
          "tree_id": "f2ed9a2046e1691ff4a8ffc7df1cb0695ae9ed8d",
          "url": "https://github.com/purlis/purlis/commit/918ce402c5d90ddc3d5dbbe3d759d85c520b2d5b"
        },
        "date": 1791482570113,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.5337485,
            "unit": "ms",
            "extra": "median of 5 runs: 0.521, 0.523, 0.534, 0.563, 0.567 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.973883999999998,
            "unit": "ms",
            "extra": "median of 5 runs: 16.763, 16.921, 16.974, 17.036, 17.112 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 104.47006049999999,
            "unit": "ms",
            "extra": "median of 5 runs: 102.406, 102.789, 104.470, 104.620, 106.354 ms"
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
          "id": "290ad5650ab4776cccedcfc967b6142da3dd4525",
          "message": "A token is stored for the one vault whose tab it was put in from\n\nReview round for the vault tab's token box.\n\n- A store marks the vault the person is in and no other. Marking every\n  vault read through the same variable is taken out: a vault the\n  committed registry names is never given a token the person did not\n  put in from its own tab. The tab names the other vaults that still\n  have none, each a link to its own tab: a pointer, never a write.\n- A token stored again replaces the old one: the previous record's\n  keyring items are deleted once the new record is saved. A store that\n  fails part way deletes what it had stored and leaves the record as it\n  was.\n- A kept token that does not read the vault is no longer blamed for\n  every failure. The answer says what kind of failure it was, and the\n  tab says to replace the token only for a refused sign-in; a missing or\n  moved program and a network that is down say what they are. The box\n  stays in every case, with Read again beside it.\n- A vault read through several variables draws no box that every press\n  would be refused.\n- A read that fails after a store is never shown beside \"Stored\".\n- An open vault tab reads again when a token is stored in another.\n- An unreadable vault's health line is its refusal, not another run of\n  the provider.\n- The doctor row's hint is true on a machine with no window, the row is\n  not printed inside a chat, and it says \"marked as kept\".\n- The test that no answer carries the token now runs a provider\n  stand-in that prints the token it was handed.\n\nDecided in implementation: D-1526-7 (replaces D-1526-3) to D-1526-11.\n\nRefs #1526\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>\nAssisted-by: claude-code\nPurlis-Chat: 01M4AREQXXRGBAEWM0SZ0N0WBN\nPurlis-Persona: steward",
          "timestamp": "2026-10-09T00:01:06+04:00",
          "tree_id": "cde10f5196fbb0c4a57f180d50641d436c3b41d4",
          "url": "https://github.com/purlis/purlis/commit/290ad5650ab4776cccedcfc967b6142da3dd4525"
        },
        "date": 1791490254888,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.5967985,
            "unit": "ms",
            "extra": "median of 5 runs: 0.594, 0.594, 0.597, 0.610, 0.622 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.077468000000003,
            "unit": "ms",
            "extra": "median of 5 runs: 15.989, 16.052, 16.077, 16.155, 16.522 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 101.872724,
            "unit": "ms",
            "extra": "median of 5 runs: 101.068, 101.271, 101.873, 101.882, 102.175 ms"
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
          "id": "1bab0fda317e95830c4dce7088309e48801933ad",
          "message": "Evidence-only jobs run nightly only, and the tests are built in a step of their own\n\nfedora, windows and the preview-image builds ran on every push to main and\nagain nightly (V70). fedora alone took 20 minutes, so main was 28 minutes\nbehind a merge whose gating jobs finished in 18, and each merge held four more\nrunners. They gate nothing, so they now run on the nightly schedule only. A\nbreak on one of those platforms is reported the next night.\n\nThe rust job's cargo test step was 916 s of an 18 minute job with nothing to\nsay how much was compiling and linking 208 test binaries and how much was\nrunning them. Building with --no-run first puts that split in the step\ntimings of every run.\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>\nAssisted-by: claude-code\nPurlis-Chat: 01M4EJSYV68AZG1SWR231JAEZE\nPurlis-Persona: steward",
          "timestamp": "2026-10-09T01:08:38+04:00",
          "tree_id": "090b5f49a8b5fd1ff6dca10c22393ca63a8bf3b1",
          "url": "https://github.com/purlis/purlis/commit/1bab0fda317e95830c4dce7088309e48801933ad"
        },
        "date": 1791494496168,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.479823,
            "unit": "ms",
            "extra": "median of 5 runs: 0.479, 0.480, 0.480, 0.486, 0.493 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.2989405,
            "unit": "ms",
            "extra": "median of 5 runs: 16.183, 16.240, 16.299, 16.305, 16.401 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 101.494464,
            "unit": "ms",
            "extra": "median of 5 runs: 101.089, 101.415, 101.494, 101.739, 101.771 ms"
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
          "id": "a48646b07353bfdaad39b5b14e2f34ee826b0596",
          "message": "A dev build carries line tables, not full debug info\n\nA cargo test links over two hundred test binaries, each with the full debug\ninfo of every crate it links. That is most of what linking them costs and most\nof what a target directory weighs. Line tables keep file and line in a panic's\nbacktrace; a debugger session asks for variables with\nCARGO_PROFILE_DEV_DEBUG=full.\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>\nAssisted-by: claude-code\nPurlis-Chat: 01M4EJSYV68AZG1SWR231JAEZE\nPurlis-Persona: steward",
          "timestamp": "2026-10-09T01:36:58+04:00",
          "tree_id": "5875d3a78768b8787bf6962c75bef02947617e6b",
          "url": "https://github.com/purlis/purlis/commit/a48646b07353bfdaad39b5b14e2f34ee826b0596"
        },
        "date": 1791497735781,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.2750995,
            "unit": "ms",
            "extra": "median of 5 runs: 0.265, 0.267, 0.275, 0.276, 0.278 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.506196000000003,
            "unit": "ms",
            "extra": "median of 5 runs: 16.458, 16.483, 16.506, 16.507, 16.601 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 101.68803150000001,
            "unit": "ms",
            "extra": "median of 5 runs: 100.488, 101.384, 101.688, 102.006, 102.565 ms"
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
          "id": "50b34380602556a0601af3a5b95846d9aeab9b08",
          "message": "The rust job runs the tests with cargo-nextest\n\ncargo test runs the workspace's test binaries one after another, so the run\nstep took 690 to 715 s on a job whose build step took 333 s. nextest runs every\nbinary's tests at once, each test in a process of its own. The floods of\nunread_answers, which a static lock ran one at a time, keep that order through\na nextest test group. nextest does not run doctests, so cargo test --doc does.\n\nLocally nothing changes: cargo test --workspace runs the same tests.\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>\nAssisted-by: claude-code\nPurlis-Chat: 01M4EJSYV68AZG1SWR231JAEZE\nPurlis-Persona: steward",
          "timestamp": "2026-10-09T02:39:32+04:00",
          "tree_id": "25579888adb3851fc3e3f42877d7688b36143ab7",
          "url": "https://github.com/purlis/purlis/commit/50b34380602556a0601af3a5b95846d9aeab9b08"
        },
        "date": 1791500304908,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.530486,
            "unit": "ms",
            "extra": "median of 5 runs: 0.519, 0.529, 0.530, 0.531, 0.545 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.919734000000002,
            "unit": "ms",
            "extra": "median of 5 runs: 16.359, 16.847, 16.920, 16.955, 17.434 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 104.59214499999999,
            "unit": "ms",
            "extra": "median of 5 runs: 102.017, 103.469, 104.592, 105.076, 105.156 ms"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "eyehollow@gmail.com",
            "name": "Aaron Yordanyan",
            "username": "dbrownsimple"
          },
          "committer": {
            "email": "aaron.yor@gmail.com",
            "name": "Aaron Yordanyan",
            "username": "diazoxide"
          },
          "distinct": true,
          "id": "032d67c5911fa616accaaeb7f28d1cc94b3aff85",
          "message": "A vault only the committed half declares is given no token from a terminal\n\nThe re-review of the guided 1Password set-up found that `--token-stdin` on a vault only\nthe committed vaults.json declares made a keyring record that pinned settings a commit\nchose, which nobody was shown.\n\n- Such a vault is refused before anything is read. Its record is made in its tab in the\n  app, which shows its settings first. A vault this machine's half declares still has only\n  its token changed.\n- The token-missing sentence and doctor print the terminal command only where it works,\n  and point to the vault's tab otherwise.\n- A held set-up's limit is checked on the wall clock as well as the monotonic clock, so it\n  never outlives fifteen minutes of real time across a sleep. Its timer now starts as the\n  set-up is held.\n\nRefs #1527\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>\nAssisted-by: claude-code\nPurlis-Chat: 01M4AREQXXRGBAEWM0SZ0N0WBN\nPurlis-Persona: steward",
          "timestamp": "2026-10-09T02:52:34+04:00",
          "tree_id": "3ec0c97a699d364c93be1d8fc37a859c1c4f80c1",
          "url": "https://github.com/purlis/purlis/commit/032d67c5911fa616accaaeb7f28d1cc94b3aff85"
        },
        "date": 1791500754648,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.51508,
            "unit": "ms",
            "extra": "median of 5 runs: 0.482, 0.509, 0.515, 0.540, 0.557 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.507523000000003,
            "unit": "ms",
            "extra": "median of 5 runs: 16.455, 16.473, 16.508, 16.647, 17.066 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 103.12326850000001,
            "unit": "ms",
            "extra": "median of 5 runs: 102.609, 102.807, 103.123, 103.399, 103.415 ms"
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
          "id": "7b48cac1007caeedd17ddaab9e617934600c21eb",
          "message": "Only main saves a Rust cache\n\nThe repository's Actions caches held 12 GB against GitHub's 10 GB limit on\n2026-10-08: each pull request saved its own 0.8 to 1.2 GB Rust caches, and they\npushed main's out. The run on main after #1541 restored nothing and took 1289 s\nwhere the warm one before it took 914 s. With save-if, main saves and a pull\nrequest restores main's cache and saves none, as rust-cache's README advises.\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>\nAssisted-by: claude-code\nPurlis-Chat: 01M4EJSYV68AZG1SWR231JAEZE\nPurlis-Persona: steward",
          "timestamp": "2026-10-09T03:30:20+04:00",
          "tree_id": "1f996784336313db6ce34760e2e83362990d985c",
          "url": "https://github.com/purlis/purlis/commit/7b48cac1007caeedd17ddaab9e617934600c21eb"
        },
        "date": 1791502417520,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.493039,
            "unit": "ms",
            "extra": "median of 5 runs: 0.488, 0.491, 0.493, 0.508, 0.513 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 17.075015,
            "unit": "ms",
            "extra": "median of 5 runs: 16.541, 16.807, 17.075, 17.088, 17.131 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 103.899193,
            "unit": "ms",
            "extra": "median of 5 runs: 102.403, 103.847, 103.899, 105.534, 106.429 ms"
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
          "id": "92a311251dbd5037ea8c1c753eb6de686231a4e3",
          "message": "Where the tickets of train 65 meet\n\nThe tab chip's menu, the breadcrumb line and the Chats list each meet\nmore than one ticket of this train. What is joined here:\n\n- J1. A working task's line in its tab's menu says what it is doing\n  (#1493's ChatDoingLine, one dim line cut short after how long; nothing\n  while it does not work, not even its gap). It reads its own chat, so no\n  line of the menu is drawn again for it.\n- J2. Brief from the tab's menu (#1494): a button at the end of a task's\n  line, beside the ways to move it, for an open task by its chat and for\n  an ended one by its dispatch record while it has a finished row. The\n  session's own chat has none. Opening it closes the menu.\n- J3. The session's Activity from the tab's menu (#1495): a line after\n  \"End a task\" opens the tab's own chat's Activity in a tab of its own,\n  as the catalogue's tab.activity row does.\n- J4. Fail closed for reads of what chats said: `activity` (a session's\n  timeline) and `task_question` (the question a task is paused on) join\n  `task_brief` on WINDOW_ONLY. Each reads words a chat wrote, about a\n  session the caller names, and no link can say which chat is asking.\n  Session-protocol and app tests pin both; the list is 35 names.\n\nTests: three window cases in TabChip.window.test.tsx for J1 to J3; the\nsession protocol refuses both reads on a link and the app's link client\nhas neither. The menu's line helper and the level test now leave out the\nActivity line, as they leave out \"End a task\".\n\nRefs #1493, #1494, #1495, #1496, #1483\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>\nAssisted-by: claude-code\nPurlis-Chat: 01M4AREQXXRGBAEWM0SZ0N0WBN\nPurlis-Persona: steward",
          "timestamp": "2026-10-09T04:25:01+04:00",
          "tree_id": "f75888a20498c2e0b17fd4025eefc7dff547d6e4",
          "url": "https://github.com/purlis/purlis/commit/92a311251dbd5037ea8c1c753eb6de686231a4e3"
        },
        "date": 1791505726312,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.460195,
            "unit": "ms",
            "extra": "median of 5 runs: 0.444, 0.456, 0.460, 0.467, 0.503 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.681836500000003,
            "unit": "ms",
            "extra": "median of 5 runs: 16.569, 16.659, 16.682, 16.782, 16.853 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 103.66681500000001,
            "unit": "ms",
            "extra": "median of 5 runs: 103.357, 103.438, 103.667, 103.675, 103.947 ms"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "eyehollow@gmail.com",
            "name": "Aaron Yordanyan",
            "username": "dbrownsimple"
          },
          "committer": {
            "email": "aaron.yor@gmail.com",
            "name": "Aaron Yordanyan",
            "username": "diazoxide"
          },
          "distinct": true,
          "id": "1d7cb8d09317f717972e0c3405f49a81aecfb27b",
          "message": "While you were away: one summary of what happened\n\nWhen the person comes back after five minutes or more away from the\nwindow, one Notice under each project's strip sums up what happened\nmeanwhile: \"While you were away: 7 tasks done, 1 failed, 2 waiting on\nyou\". Away is any of three signals: the window not in use (blur to\nfocus), hidden (visibilitychange), or left in front with no key, pointer\nor wheel input, from the last input to the first one after. A window that\nstarts without the focus is away from its start.\n\nEach project keeps its own times away whether or not it is in front: the\nproject's view holds them above where it stops drawing a project behind\nanother, so switching to it shows its summary, and switching back is no\nDismiss.\n\nOnly what happened while away is counted: a finished task by the time\nits record says it ended, in the count every surface puts it in\n(finishedBucketOf); a chat as waiting when it came into the needs-you\nqueue while away and is still there. A chat there only for tasks that\ncame to nothing (every reason it has is one failure's sentence) is\ncounted once, as the failure.\n\nEach part is a link. One item goes straight to it, named in the link's\naccessible name; several open a list. A chat waiting on you is shown\nwhere it lives, where its question or Notice is answered as before. A\nfinished task goes where a needs-you item's Go takes it, through the one\nhelper both now share: its chat if open, else its finished row, else the\nsession that asked, saying why. Going to a failed task marks that\nfailure looked at, as the Go does; that is the one thing a press writes.\nDismiss puts the summary away and nothing else: needs-you items,\nquestion Notices, failed rows and Activity lines all stay.\n\nA summary whose parts have all gone lets its times go, and so does one\nthat still counts nothing ten seconds after the return, so no record\nread later brings an old time back; a reload starts with none.\n\nSettings, You, Chats list: \"Away summary\", on by default, kept as\n`chats.away` in the layout file beside the other Chats list choices;\n`chats` is now described as how chats are listed and summed up. Off,\nnothing is watched and the window is as before.\n\napp/src/awayCounts.ts holds what is counted and said (plain inputs);\napp/src/AwaySummary.tsx watches the window (useTimeAway), keeps each\nproject's summary (useAwaySummary) and draws the Notice. A later source\nof things that happened while away joins as another part there.\n\nDocs: CONTEXT.md (Away summary), docs/plane-format.md and\ndocs/design-system.md (`chats.away`), changes/while-you-were-away.md.\n\nCloses #1514\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>\nAssisted-by: claude-code\nPurlis-Chat: 01M4AREQXXRGBAEWM0SZ0N0WBN\nPurlis-Persona: steward",
          "timestamp": "2026-10-09T04:56:23+04:00",
          "tree_id": "0b5fa39584503b86eea39c8d9ee5eed6f4ac7e92",
          "url": "https://github.com/purlis/purlis/commit/1d7cb8d09317f717972e0c3405f49a81aecfb27b"
        },
        "date": 1791507460210,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.28124299999999997,
            "unit": "ms",
            "extra": "median of 5 runs: 0.272, 0.275, 0.281, 0.295, 0.301 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.563947,
            "unit": "ms",
            "extra": "median of 5 runs: 16.535, 16.537, 16.564, 16.605, 16.686 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 102.3495805,
            "unit": "ms",
            "extra": "median of 5 runs: 101.568, 102.200, 102.350, 102.574, 102.689 ms"
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
          "id": "dc13fc2175c24d13ea72b08b5f8a2e8346e40402",
          "message": "dispatch: the loop rule reads the chain from the dispatch record, not from the chats still open\n\nA task's record of open chats now keeps the personas above it\n(chats[].from.above), written by the app when it starts the task from its\nown record of the asking chat. The loop rule and the person's never for a\nchat above read it there, so a chain still holds when a chat in the middle\nof it has finished, closed or been cleared. Nothing a chat sends goes into\nit. A finished task reopened as an ordinary chat names no asking chat and\nstarts a new chain.\n\nA record written before the key is read from the chats still open, as\nbefore. Where that walk meets a chat that has closed (or a loop), the chain\nis unread and fails closed: any dispatch to another persona is refused,\nand one to its own where the person said never to any persona dispatching\nthere, with a sentence saying an older version kept no record of the\nchain. A kept chain whose length is not the depth, or that names what\ncannot be a persona, reads as not kept.\n\nDocs: plane-format (chats[].from.above), handoff refusal table, ADR 0090\namendment item 18. Changelog fragment.\n\nReview round 1: a chain is read under the decision's lock, and an older chain's refusal says where it began\n\nReview round for the loop rule reading the chain from the dispatch record.\n\n- The asking chat is read from the chats open under the lock the decision\n  is made under, and one that has closed or ended by then is refused\n  (`Refused::NotOpen`). `asked_by_a_chat` no longer takes an earlier read\n  of it, so a chain read short is never written into a new chat.\n- The refusal for a chain that cannot be read whole says the chat's chain\n  began under an older version, which is true for the chats this build\n  dispatched from such a chain too (core and app sentences).\n- A pretend-host test asserts the handoff road writes the chain.\n- ADR 0090's still-open line and the changes fragment point at the\n  follow-up checklist (#1548) and name any chat, not only a task.\n\nCloses #1521\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>\nAssisted-by: claude-code\nPurlis-Chat: 01M4AREQXXRGBAEWM0SZ0N0WBN\nPurlis-Persona: steward",
          "timestamp": "2026-10-09T05:34:49+04:00",
          "tree_id": "3e5fb081925dc3eba0b5c82c4e198d7f874eccae",
          "url": "https://github.com/purlis/purlis/commit/dc13fc2175c24d13ea72b08b5f8a2e8346e40402"
        },
        "date": 1791509917498,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.4710215,
            "unit": "ms",
            "extra": "median of 5 runs: 0.454, 0.469, 0.471, 0.473, 0.480 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.2950275,
            "unit": "ms",
            "extra": "median of 5 runs: 16.166, 16.225, 16.295, 16.339, 16.608 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 101.4872925,
            "unit": "ms",
            "extra": "median of 5 runs: 101.258, 101.427, 101.487, 101.504, 101.900 ms"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "eyehollow@gmail.com",
            "name": "Aaron Yordanyan",
            "username": "dbrownsimple"
          },
          "committer": {
            "email": "aaron.yor@gmail.com",
            "name": "Aaron Yordanyan",
            "username": "diazoxide"
          },
          "distinct": true,
          "id": "70a07de72d1aecb1d8c9971f62076cb6220732d3",
          "message": "Limits review: the clock looks when chats are put back; the footer hides only what it says\n\nFix round for the review of the #1540 and #1545 follow-ups.\n\n- M1: the end of Held::reopen tells the clock, through a channel it\n  waits on with recv_timeout until its next 30-second round. A session\n  past its token limit says so as soon as a launch puts its chats back\n  (after the reopen answer), not up to 30 s later. The 2 s poll is\n  gone. Test: the row shows right after put-back (red without the hook).\n- M2: `own` is set only where tasks alone are counted, and the window\n  leaves the row's words off the footer only where the footer's own\n  numbers show that limit (same number, as many running). A chat nobody\n  is at, and a held chat or one on the default persona, see the limit\n  said. Rust and window tests for both.\n- F1: the store is read with the clock's lock let go.\n- F2: an ended record is kept as the six fields the clock reads.\n- F3: what the clock kept for a closed project is let go of each round.\n- F4: the store's names are a HashSet.\n\nRefs #1540\nRefs #1545\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>\nAssisted-by: claude-code\nPurlis-Chat: 01M4AREQXXRGBAEWM0SZ0N0WBN\nPurlis-Persona: steward",
          "timestamp": "2026-10-09T05:43:59+04:00",
          "tree_id": "4f3cbb237925cfc016a69292e67d7db92cf6d831",
          "url": "https://github.com/purlis/purlis/commit/70a07de72d1aecb1d8c9971f62076cb6220732d3"
        },
        "date": 1791510779470,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.5210535000000001,
            "unit": "ms",
            "extra": "median of 5 runs: 0.503, 0.520, 0.521, 0.523, 0.542 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 17.118962,
            "unit": "ms",
            "extra": "median of 5 runs: 16.522, 16.609, 17.119, 17.303, 17.560 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 104.6947505,
            "unit": "ms",
            "extra": "median of 5 runs: 102.694, 103.770, 104.695, 105.020, 106.108 ms"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "eyehollow@gmail.com",
            "name": "Aaron Yordanyan",
            "username": "dbrownsimple"
          },
          "committer": {
            "email": "aaron.yor@gmail.com",
            "name": "Aaron Yordanyan",
            "username": "diazoxide"
          },
          "distinct": true,
          "id": "13b16012b45ccd2e67edc35a274ae918db61c29e",
          "message": "ChatDoing SC-3 test: give it the heavy window tests' 20 s limit\n\nThe fifty-working-tasks test ran past vitest's 5 s default on CI (5047 ms\nand 5154 ms, main dc13fc21) while the whole suite ran in parallel on a\nloaded runner; alone it takes about a second. What it checks is draws:\nonly the changed chat's second line redraws, no row does, counted through\nthe mocked useDoingSaid and chatDoingId. It never asserted a time budget,\nso the wall-clock limit was incidental. It now carries the same\n{ timeout: 20_000 } as the heavy window tests in MemoryLists and Notices,\nwith a comment saying why. No assertion changed.\n\nRefs #1551\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>\nAssisted-by: claude-code\nPurlis-Chat: 01M4AREQXXRGBAEWM0SZ0N0WBN\nPurlis-Persona: steward",
          "timestamp": "2026-10-09T06:18:33+04:00",
          "tree_id": "9184630f5c8b2e94c83fcf06205e79b86ec7b229",
          "url": "https://github.com/purlis/purlis/commit/13b16012b45ccd2e67edc35a274ae918db61c29e"
        },
        "date": 1791512392710,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.5310145,
            "unit": "ms",
            "extra": "median of 5 runs: 0.525, 0.528, 0.531, 0.531, 0.542 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 17.0032675,
            "unit": "ms",
            "extra": "median of 5 runs: 16.542, 16.839, 17.003, 17.041, 17.042 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 104.29757899999998,
            "unit": "ms",
            "extra": "median of 5 runs: 102.115, 103.986, 104.298, 105.028, 105.541 ms"
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
          "id": "048f82058a5412f22690b98b58ab50558b801364",
          "message": "Brokered git says when your git config would have mattered\n\nA clone or worktree the app makes for a sandboxed chat reads none of the\nperson's git config, by design (D-1335-7), and the difference from the terminal\nwas silent. `repocmd::unread` now says so where it shows:\n\n- a checkout whose top `.gitattributes` names a filter says it was checked out\n  as stored; for Git LFS, that the files are pointer files and `git lfs pull`\n  fetches them. Both a brokered clone and a brokered worktree add say it. The\n  file is read as `HEAD` commits it, through the hardened runner: a regular\n  file of at most 64 KiB, never a link and never the working tree the chat\n  writes;\n- a brokered clone that fails reads the person's global config files, through\n  the hardened runner with `--file` and no includes, only to see whether a\n  proxy, certificate, client certificate, cookie, extra-header or\n  `url.<base>.insteadOf` key would have changed the clone's route, and names\n  the key, never its value, with the terminal as the way round. Nothing read is\n  applied.\n\nThe CLI test that compares a brokered run with the terminal's now expects the\none filter note on the brokered side. `docs/plane-format.md` explains the\ndifference beside the git config purlis sets.\n\nThe `unread` fixtures commit with no global or system git config\n(`testgit::run_unconfigured`), so a machine whose config defines the\nGit LFS filter does not commit their `.gitattributes` as a pointer.\n\nCloses #1413\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>\nAssisted-by: claude-code\nPurlis-Chat: 01M4AREQXXRGBAEWM0SZ0N0WBN\nPurlis-Persona: steward",
          "timestamp": "2026-10-09T06:32:31+04:00",
          "tree_id": "0cf607ddd85641b701b1f5b6a498b0a6a6cbf07a",
          "url": "https://github.com/purlis/purlis/commit/048f82058a5412f22690b98b58ab50558b801364"
        },
        "date": 1791513711038,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.5079605,
            "unit": "ms",
            "extra": "median of 5 runs: 0.492, 0.496, 0.508, 0.511, 0.535 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.852973,
            "unit": "ms",
            "extra": "median of 5 runs: 16.479, 16.782, 16.853, 16.996, 17.022 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 103.9661345,
            "unit": "ms",
            "extra": "median of 5 runs: 103.477, 103.691, 103.966, 104.117, 106.109 ms"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "eyehollow@gmail.com",
            "name": "Aaron Yordanyan",
            "username": "dbrownsimple"
          },
          "committer": {
            "email": "aaron.yor@gmail.com",
            "name": "Aaron Yordanyan",
            "username": "diazoxide"
          },
          "distinct": true,
          "id": "c51ca80f3c4cef5bf770a0585bae25e262a6bb91",
          "message": "changes: what a chat's Activity no longer keeps, and what it says it leaves out\n\nRefs #1520\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>\nAssisted-by: claude-code\nPurlis-Chat: 01M4AREQXXRGBAEWM0SZ0N0WBN\nPurlis-Persona: steward",
          "timestamp": "2026-10-09T06:53:10+04:00",
          "tree_id": "e958a4c705e7b8bec846195f38399a01c1a6b37b",
          "url": "https://github.com/purlis/purlis/commit/c51ca80f3c4cef5bf770a0585bae25e262a6bb91"
        },
        "date": 1791515510135,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.46394250000000004,
            "unit": "ms",
            "extra": "median of 5 runs: 0.454, 0.462, 0.464, 0.467, 0.474 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.8169075,
            "unit": "ms",
            "extra": "median of 5 runs: 16.778, 16.812, 16.817, 16.825, 16.853 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 101.73828,
            "unit": "ms",
            "extra": "median of 5 runs: 101.328, 101.684, 101.738, 101.895, 102.288 ms"
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
          "id": "e914aaf28e242b77c312c93dbd59de589debba94",
          "message": "Where the tickets of train 66 meet: the person's Stop reads the one notion of a prompt\n\nTrain 63's stop engine decided on the board's `asking` alone, while the\nwindow's question and the asking chat's cancel read #1525's prompt rule.\nThe two disagreed both ways: after a prompt answered in the window, Stop\nand get its report interrupted the task and then ended it with no report;\nand with a permission ask open in the window and no Notification heard, a\nstop of the chat above it, or Stop them as its asker closed, interrupted\nthe task and typed its last-turn line into it.\n\nThe engine's facts now carry `ask_open`, and its three rules (the last\nturn, whether an interrupted task takes its line, and when a queued line\nis sent) read `Facts::prompt_showing`, which is\n`dispatched::Seen::prompt_showing`: an ask open in the window is a prompt\nin front of the person, and one answered there is not. A task with one\nopen is ended as it stands and nothing is typed into it; a task whose\nprompt was answered is asked for its report. Tested both ways, by the\ntask's own stop and by its asker's stop with what is below it. The\nasking chat's list and wait read the same rule for \"waiting on the\nperson\".\n\nStop all tasks (#1498) and purlis's stop at a time limit (#1512) press the\nsame stop with each task's own facts, so they read the same rule: a task\nwith an ask open is ended as it stands, one whose prompt was answered is\nasked for its report (tested on one press of both).\n\nThe Activity tab's restore test (#1495) now expects the view tab the person\nleft in front to come back in front, as #1525's restore does.\n\nThe typed smart close's test waits for the window's Closed step as it\nwaits for the tab to go: the chat leaves the open list before its close\nfinishes and tells the window, and the test read the log in between.\n\nReview fold-ins:\n- The UI RPC guard names the persona-hosts Allow once, beside the rule's\n  29, so the two lists make the 34.\n- A #1519 test is named for what it asserts: closed by the person.\n- The restart docs and the changelog name the task row's Try to start\n  again and End task.\n- A test pins that a task put back after a refused End task still carries\n  the instruction to carry on.\n\nRefs #1525, #1488, #1513, #1519, #1362\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>\nAssisted-by: claude-code\nPurlis-Chat: 01M4AREQXXRGBAEWM0SZ0N0WBN\nPurlis-Persona: steward",
          "timestamp": "2026-10-09T08:09:58+04:00",
          "tree_id": "e69de44defed26c372ae0810a72724eb5028f5f5",
          "url": "https://github.com/purlis/purlis/commit/e914aaf28e242b77c312c93dbd59de589debba94"
        },
        "date": 1791519081159,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.5381085,
            "unit": "ms",
            "extra": "median of 5 runs: 0.503, 0.522, 0.538, 0.541, 0.553 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 17.102849499999998,
            "unit": "ms",
            "extra": "median of 5 runs: 16.900, 17.084, 17.103, 17.147, 17.659 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 105.2423445,
            "unit": "ms",
            "extra": "median of 5 runs: 104.219, 104.625, 105.242, 105.274, 106.830 ms"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "eyehollow@gmail.com",
            "name": "Aaron Yordanyan",
            "username": "dbrownsimple"
          },
          "committer": {
            "email": "aaron.yor@gmail.com",
            "name": "Aaron Yordanyan",
            "username": "diazoxide"
          },
          "distinct": true,
          "id": "382f2d649f83856f63a08a1fca284ab850915efc",
          "message": "review of #1551: a new wait is a chat coming into waiting; the Answer dialog holds the keyboard\n\nThe review's must-fix and five fold-ins (Refs #1551):\n\n- M1. \"Waiting on you\" no longer keys a new wait on the board's movedAt\n  stamp, which also moves for what only touches a chat (a task's failure,\n  a sub-agent ending, a report coming back). The window now notes, from\n  the chats store's own changes, each chat that comes into `waiting`, in\n  order, and a chat already in the queue counts only if it came into\n  waiting after the person left, or its item gives a new reason other than\n  one of its tasks' failures (those count as failed). Tests: a waiting\n  chat whose task fails while away (unit and window: \"1 task failed\"\n  alone), and a waiting chat whose sub-agent ends while away.\n- The Answer dialog focuses itself as it opens, so the keyboard is in it\n  in every state: while the question is read, and where there is none.\n- The refused-dispatches part is one link whatever its count, named with\n  the pairs it counts (a menu of identical destinations was a menu for\n  nothing, and could close the list it opened).\n- Opening the needs-you list from the summary reads it again, as a press\n  on the hand does.\n- Tests: the core refusing a stale answer (its sentence, the text kept,\n  the dialog open), the keyboard back on the task's row after Cancel, no\n  question to answer, two refused dispatches.\n- The SC-3 render counter says why only the row may call chatDoingId, and\n  the line says why it spells the id out.\n\nRefs #1551\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>\nAssisted-by: claude-code\nPurlis-Chat: 01M4AREQXXRGBAEWM0SZ0N0WBN\nPurlis-Persona: steward",
          "timestamp": "2026-10-09T08:39:42+04:00",
          "tree_id": "53bd0549bfb506e4770696962e7bc52ece781834",
          "url": "https://github.com/purlis/purlis/commit/382f2d649f83856f63a08a1fca284ab850915efc"
        },
        "date": 1791521011555,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.6146475,
            "unit": "ms",
            "extra": "median of 5 runs: 0.608, 0.610, 0.615, 0.616, 0.617 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.203035,
            "unit": "ms",
            "extra": "median of 5 runs: 16.085, 16.113, 16.203, 16.572, 16.641 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 101.372949,
            "unit": "ms",
            "extra": "median of 5 runs: 100.756, 101.089, 101.373, 101.772, 102.239 ms"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "eyehollow@gmail.com",
            "name": "Aaron Yordanyan",
            "username": "dbrownsimple"
          },
          "committer": {
            "email": "aaron.yor@gmail.com",
            "name": "Aaron Yordanyan",
            "username": "diazoxide"
          },
          "distinct": true,
          "id": "dd6371dd896214f4ec8aef208695b302afe7346b",
          "message": "dispatch: the review of #1543's follow-ups\n\n- The table's Notice and row notes say what is true in each state\n  (M1). Not checked since purlis started: the next dispatch checks\n  first, and where the history reads the grants count. History that\n  cannot be read: none counts; a chat someone is at is asked on its\n  tab, one nobody is at is refused and listed under Needs you. The core\n  tells the two apart (dispatcharrival::unsettled), and the window gets\n  ProjectUnsettled.\n- Nothing is said where nothing of the project's is accepted here: a\n  decline alone, or a commit kept from an earlier settling, no longer\n  shows it (F1).\n- Accepted project rows are greyed in either state (F2).\n- The crossing rule stands aside for an unsandboxed chat by the very\n  comparison the no-sandbox refusal makes, and both go by one read of\n  the asking chat: the decision is now asked with it\n  (request_dispatch_as, requested_as). The session-based entry points\n  remain for the tests (F3).\n\nRefs #1543\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>\nAssisted-by: claude-code\nPurlis-Chat: 01M4AREQXXRGBAEWM0SZ0N0WBN\nPurlis-Persona: steward",
          "timestamp": "2026-10-09T08:54:16+04:00",
          "tree_id": "46abf1c3c2e779db9249102f990acf8502b5c6a4",
          "url": "https://github.com/purlis/purlis/commit/dd6371dd896214f4ec8aef208695b302afe7346b"
        },
        "date": 1791522119693,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.43515800000000004,
            "unit": "ms",
            "extra": "median of 5 runs: 0.418, 0.434, 0.435, 0.444, 0.449 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.397793,
            "unit": "ms",
            "extra": "median of 5 runs: 16.330, 16.384, 16.398, 16.431, 16.832 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 101.89402100000001,
            "unit": "ms",
            "extra": "median of 5 runs: 100.875, 101.718, 101.894, 102.051, 102.790 ms"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "eyehollow@gmail.com",
            "name": "Aaron Yordanyan",
            "username": "dbrownsimple"
          },
          "committer": {
            "email": "aaron.yor@gmail.com",
            "name": "Aaron Yordanyan",
            "username": "diazoxide"
          },
          "distinct": true,
          "id": "1ff31a105f31512d24c102d5792c259673e121af",
          "message": "dispatch: a recovered chain is whole only at the lineage's first chat, never by depth alone\n\nReview round for reading an older chain from the dispatch records.\n\n- Where the records stop at a chat that has closed, the chain is whole only\n  when that chat's id is the asking chat's `root` (the chat the person\n  started, copied down each dispatch), and as deep as the asking chat's\n  record says. The depth is now an extra condition, never enough alone: a\n  chat dispatched below one from before the depth key was given a depth\n  that is short and names no root, so a chain whose records stopped there\n  read as whole while missing the chats above. Such a chain stays unread\n  and refused. Regression tests at the chain and at the decision.\n- An open chat that names no asking chat ends the chain only where it is\n  the chat the asking chat's `root` names, where one is kept.\n- `dispatchchain::Above` has no `Default`: \"nothing above\" offers every box,\n  so each caller now names it.\n- Docs: ADR 0090 item 18, the handoff refusal table and the changelog\n  fragment say what is whole; plane-format's `chats[].from` says the chain\n  is read from the dispatch records before refusing, and that a depth is\n  read as at least the chain the walk finds.\n\nRefs #1548\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>\nAssisted-by: claude-code\nPurlis-Chat: 01M4AREQXXRGBAEWM0SZ0N0WBN\nPurlis-Persona: steward",
          "timestamp": "2026-10-09T09:29:02+04:00",
          "tree_id": "4612163aed865560efc96cbc42f32b994410a8d4",
          "url": "https://github.com/purlis/purlis/commit/1ff31a105f31512d24c102d5792c259673e121af"
        },
        "date": 1791523813573,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.5167165,
            "unit": "ms",
            "extra": "median of 5 runs: 0.499, 0.513, 0.517, 0.528, 0.536 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 17.1335045,
            "unit": "ms",
            "extra": "median of 5 runs: 16.868, 17.086, 17.134, 17.204, 17.218 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 103.64872500000001,
            "unit": "ms",
            "extra": "median of 5 runs: 103.317, 103.381, 103.649, 104.274, 104.594 ms"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "eyehollow@gmail.com",
            "name": "Aaron Yordanyan",
            "username": "dbrownsimple"
          },
          "committer": {
            "email": "aaron.yor@gmail.com",
            "name": "Aaron Yordanyan",
            "username": "diazoxide"
          },
          "distinct": true,
          "id": "97e6f06eb063135c2eef6ed1880e5cd839de98dd",
          "message": "ADR 0067: one chat's Allow is bound too; what stays per task\n\nRecords the #1508 follow-ups decided in implementation: a chat's own\nblock Allow is bound to the block it showed and is the window's alone;\n\"Start without the sandbox\" and a host a block did not name stay one\ntask's answer each; the tab's chip keeps one hand per chat waiting off\nscreen; the blocks are held in memory only, so after a relaunch nothing\nis asked until a chat is blocked again.\n\nRefs #1538\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>\nAssisted-by: claude-code\nPurlis-Chat: 01M4AREQXXRGBAEWM0SZ0N0WBN\nPurlis-Persona: steward",
          "timestamp": "2026-10-09T09:37:28+04:00",
          "tree_id": "63bad37ef3cbc85e757e485db6e703b7ef8878af",
          "url": "https://github.com/purlis/purlis/commit/97e6f06eb063135c2eef6ed1880e5cd839de98dd"
        },
        "date": 1791524594265,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.5925955,
            "unit": "ms",
            "extra": "median of 5 runs: 0.580, 0.592, 0.593, 0.607, 0.621 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.116725,
            "unit": "ms",
            "extra": "median of 5 runs: 16.004, 16.082, 16.117, 16.147, 16.198 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 101.786967,
            "unit": "ms",
            "extra": "median of 5 runs: 100.973, 101.411, 101.787, 102.398, 102.723 ms"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "eyehollow@gmail.com",
            "name": "Aaron Yordanyan",
            "username": "dbrownsimple"
          },
          "committer": {
            "email": "aaron.yor@gmail.com",
            "name": "Aaron Yordanyan",
            "username": "diazoxide"
          },
          "distinct": true,
          "id": "65629aa1378b51556cfdb94d378925a9c0213f61",
          "message": "Review of #1546: a report kept again after a start died in, left before it is claimed\n\n- A start the app died in, followed by a start that did not happen, now keeps\n  the report for its workspace again: give_back re-keeps wherever the record\n  is still owed and the file it names is gone, not only where this start took\n  it (handback::still_kept). Test: died in, refused, let go.\n- hand_to leaves the report for the chat first and claims the record after,\n  under the lock: the app dying between the two leaves the report with the\n  chat and still owed, never with nobody. A second claimant takes back what it\n  left.\n- A task that reports is no longer said to ask again: Talk::close clears the\n  restart mark.\n- Only chats a launch started are marked as brought back; one waiting to start\n  is marked when it starts.\n- The refusal says \"Any question it asked before the restart\", since the task\n  may have asked none.\n- A chat that resumed the asker (a Resume, or a Reopen of a finished task's\n  row) and closes before reading what it was handed keeps it owed: its close\n  matches the records by the chat it resumed too.\n- ADR 0090: the heading says the launch is also held to the profile and the\n  folder, and what \"kept again\" means after a start died in.\n\nRefs #1546\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>\nAssisted-by: claude-code\nPurlis-Chat: 01M4AREQXXRGBAEWM0SZ0N0WBN\nPurlis-Persona: steward",
          "timestamp": "2026-10-09T09:54:03+04:00",
          "tree_id": "24e92adc1eb2bfda5bc9373e8d3d470bbe0df129",
          "url": "https://github.com/purlis/purlis/commit/65629aa1378b51556cfdb94d378925a9c0213f61"
        },
        "date": 1791525802423,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.5295085,
            "unit": "ms",
            "extra": "median of 5 runs: 0.523, 0.525, 0.530, 0.530, 0.545 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.6281215,
            "unit": "ms",
            "extra": "median of 5 runs: 16.365, 16.544, 16.628, 16.826, 17.149 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 103.7269345,
            "unit": "ms",
            "extra": "median of 5 runs: 102.537, 102.738, 103.727, 104.175, 104.217 ms"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "eyehollow@gmail.com",
            "name": "Aaron Yordanyan",
            "username": "dbrownsimple"
          },
          "committer": {
            "email": "aaron.yor@gmail.com",
            "name": "Aaron Yordanyan",
            "username": "diazoxide"
          },
          "distinct": true,
          "id": "3f36e5419fbef9267aa306a1bc27fdac2f18b162",
          "message": "review of #1534: the explorer's remove and merge are the window's alone\n\n- worktree_remove and worktree_merge join WINDOW_ONLY: the explorer's\n  remove (with force, of uncommitted work) and merge of any branch\n  folder are the person's acts in their window, and no link serves them.\n  The count test moves by two; uiRpc.ts is regenerated without them.\n- \"also written by\" names each other chat once, where two are shown\n  under one name.\n- names_folder's doc says what is true: a record that does not parse is\n  skipped, so it keeps nobody off a folder.\n\nRefs #1534\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>\nAssisted-by: claude-code\nPurlis-Chat: 01M4AREQXXRGBAEWM0SZ0N0WBN\nPurlis-Persona: steward",
          "timestamp": "2026-10-09T10:09:16+04:00",
          "tree_id": "be9edb800b230fb497f8abf7fc4603a096536251",
          "url": "https://github.com/purlis/purlis/commit/3f36e5419fbef9267aa306a1bc27fdac2f18b162"
        },
        "date": 1791526782121,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.5098855,
            "unit": "ms",
            "extra": "median of 5 runs: 0.508, 0.510, 0.510, 0.527, 0.529 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.895238499999998,
            "unit": "ms",
            "extra": "median of 5 runs: 16.480, 16.484, 16.895, 17.152, 17.315 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 104.8081605,
            "unit": "ms",
            "extra": "median of 5 runs: 101.692, 102.976, 104.808, 105.314, 105.842 ms"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "eyehollow@gmail.com",
            "name": "Aaron Yordanyan",
            "username": "dbrownsimple"
          },
          "committer": {
            "email": "aaron.yor@gmail.com",
            "name": "Aaron Yordanyan",
            "username": "diazoxide"
          },
          "distinct": true,
          "id": "63148fa5497000ce802cf0b714480bbb555919ea",
          "message": "Say why an ask that leaves the window unanswered is not looked at again\n\nRead every route by which a window ask can leave without an answer: its\ndeadline passing, and its hook going away. In both the hook decides\nnothing, so the harness asks in its pane (the person's to answer, a key\nthat stands the end down), or the harness stopped (its turn's end or its\nprogram's end is the look). None leaves a reported task with neither a\nprompt nor a bound while it works on. A bound the ask spent is\ndeliberately not given anew there, so an unannounced pane prompt is never\nended under the person. Recorded on Seen::prompt_showing. Whether Claude\nCode says a pane prompt with a Notification after a hook decides\nnothing is still to be probed on a real chat.\n\nRefs #1525\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>\nAssisted-by: claude-code\nPurlis-Chat: 01M4AREQXXRGBAEWM0SZ0N0WBN\nPurlis-Persona: steward",
          "timestamp": "2026-10-09T10:38:02+04:00",
          "tree_id": "71a42ee33aaba6ae0d417f0007969be37a60b02a",
          "url": "https://github.com/purlis/purlis/commit/63148fa5497000ce802cf0b714480bbb555919ea"
        },
        "date": 1791527950508,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.5470395000000001,
            "unit": "ms",
            "extra": "median of 5 runs: 0.504, 0.540, 0.547, 0.567, 0.574 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 17.384251499999998,
            "unit": "ms",
            "extra": "median of 5 runs: 16.357, 16.705, 17.384, 17.463, 17.538 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 106.56055,
            "unit": "ms",
            "extra": "median of 5 runs: 101.834, 103.640, 106.561, 107.549, 108.603 ms"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "eyehollow@gmail.com",
            "name": "Aaron Yordanyan",
            "username": "dbrownsimple"
          },
          "committer": {
            "email": "aaron.yor@gmail.com",
            "name": "Aaron Yordanyan",
            "username": "diazoxide"
          },
          "distinct": true,
          "id": "2b348df6e6ad4b748af6a365096fada3918a2071",
          "message": "Policy and the sandbox: refuse a locked host at the write, revoke what is locked, lead with the policy\n\nThree of the #1423 follow-ups (the macOS ACL line waits on a ruling):\n\n- A whole-file save (Edit as TOML, form edits, the Shared/this-machine move) now refuses a host\n  an administrator's policy locks out at the file's level when the write adds it, with the\n  sentence Settings' Add gives. Hosts the file already held are kept and never block another\n  edit (settings::hosts::locked_out, checked in settings::refused_of).\n- Settings > Sandbox > Granted draws Revoke on an entry policy has locked out. The backend\n  already took it away; a test now pins that, audited, while the policy stands.\n- Where policy requires the sandbox, a refusal to start a sandboxed chat leads with\n  \"policy requires the sandbox for every chat on this machine\" instead of the project, and\n  ends with the policy and who set it (sandbox::under_policy, LEAD, POLICY_LEAD), including\n  the app's two refusals that did not name the policy before.\n\nDocs: plane-format's policy section, ADR 0067 §4 and §5 wording, a changes/ fragment.\n\nRefs #1431\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>\nAssisted-by: claude-code\nPurlis-Chat: 01M4AREQXXRGBAEWM0SZ0N0WBN\nPurlis-Persona: steward",
          "timestamp": "2026-10-09T10:57:44+04:00",
          "tree_id": "75ef4b1a351d5b2a86297dd886ea1a6068beff61",
          "url": "https://github.com/purlis/purlis/commit/2b348df6e6ad4b748af6a365096fada3918a2071"
        },
        "date": 1791529130344,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.273453,
            "unit": "ms",
            "extra": "median of 5 runs: 0.255, 0.256, 0.273, 0.287, 0.312 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.386162000000002,
            "unit": "ms",
            "extra": "median of 5 runs: 16.354, 16.386, 16.386, 16.554, 16.602 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 101.690619,
            "unit": "ms",
            "extra": "median of 5 runs: 100.959, 101.089, 101.691, 101.707, 101.788 ms"
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
          "id": "c919b964db0588d621fff28095a2008d10f09eb8",
          "message": "build(deps): bump taiki-e/install-action\n\nBumps the actions group with 1 update in the / directory: [taiki-e/install-action](https://github.com/taiki-e/install-action).\n\n\nUpdates `taiki-e/install-action` from 2.87.22 to 2.87.25\n- [Release notes](https://github.com/taiki-e/install-action/releases)\n- [Changelog](https://github.com/taiki-e/install-action/blob/main/CHANGELOG.md)\n- [Commits](https://github.com/taiki-e/install-action/compare/83ac0ad63c0167e6f06796fab0fce28db1bf3db0...183e4297cca2404691e9380e1307288dced5c82a)\n\n---\nupdated-dependencies:\n- dependency-name: taiki-e/install-action\n  dependency-version: 2.87.25\n  dependency-type: direct:production\n  update-type: version-update:semver-patch\n  dependency-group: actions\n...\n\nSigned-off-by: dependabot[bot] <support@github.com>",
          "timestamp": "2026-10-09T11:27:07+04:00",
          "tree_id": "2ef9bef06fb119213cff1d90f7133283dca249ec",
          "url": "https://github.com/purlis/purlis/commit/c919b964db0588d621fff28095a2008d10f09eb8"
        },
        "date": 1791530896144,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.2848625,
            "unit": "ms",
            "extra": "median of 5 runs: 0.262, 0.284, 0.285, 0.296, 0.297 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.625498999999998,
            "unit": "ms",
            "extra": "median of 5 runs: 16.526, 16.585, 16.625, 16.644, 16.699 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 101.729733,
            "unit": "ms",
            "extra": "median of 5 runs: 100.765, 100.982, 101.730, 101.991, 102.211 ms"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "eyehollow@gmail.com",
            "name": "Aaron Yordanyan",
            "username": "dbrownsimple"
          },
          "committer": {
            "email": "aaron.yor@gmail.com",
            "name": "Aaron Yordanyan",
            "username": "diazoxide"
          },
          "distinct": true,
          "id": "47d653d758c9629da15ae9dbde449a8d1e256d7d",
          "message": "Review of #1556: an owed report's record is kept, a resumed asker is back\n\n- A dispatch record that still owes its asking chat a report is aged\n  from when it was last written, not from its end. It is marked owed\n  when the report reaches no chat, which can be long after the task\n  ended (an asking chat open for weeks that closes with the report\n  unread): aged from its end, it was collected at the next open, and\n  the chat's reopen was never handed the report. The collection\n  wording in the format docs, retention and the workspace removal's\n  sentence says so.\n- A task whose asking chat came back as a chat that resumed it keeps\n  its row and its words: settling at open (from the reopen record),\n  after a launch (open chats and chats waiting to start), and as a task\n  ends while the app runs. That chat is handed the report, so the\n  asking chat is not gone. dispatchrecord::settle_asked takes the\n  asker's answer apart from the worker's.\n- expire_talk and expire_in answer the ids they changed, and the daily\n  sweep tells an open Activity tab the lines whose words it took.\n\nRefs #1556\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>\nAssisted-by: claude-code\nPurlis-Chat: 01M4AREQXXRGBAEWM0SZ0N0WBN\nPurlis-Persona: steward",
          "timestamp": "2026-10-09T11:30:14+04:00",
          "tree_id": "79984aaaea5338d064f9e15f2f5038ba82f86941",
          "url": "https://github.com/purlis/purlis/commit/47d653d758c9629da15ae9dbde449a8d1e256d7d"
        },
        "date": 1791531929195,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.5186615,
            "unit": "ms",
            "extra": "median of 5 runs: 0.485, 0.515, 0.519, 0.546, 0.565 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.919673,
            "unit": "ms",
            "extra": "median of 5 runs: 16.499, 16.808, 16.920, 17.085, 17.117 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 104.342896,
            "unit": "ms",
            "extra": "median of 5 runs: 102.968, 102.968, 104.343, 104.471, 105.846 ms"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "eyehollow@gmail.com",
            "name": "Aaron Yordanyan",
            "username": "dbrownsimple"
          },
          "committer": {
            "email": "aaron.yor@gmail.com",
            "name": "Aaron Yordanyan",
            "username": "diazoxide"
          },
          "distinct": true,
          "id": "5b17d37f635dd3160d1c3b0038fcd7f0372a6d2c",
          "message": "Check every opened project before naming an export, and harden the chat check\n\nReview of #1542: M2 and fold-ins A to E.\n\n- The hint after a conversion names a variable only when no vault of\n  this project or of any other project this machine opened still reads\n  it, since a shell profile's export is the whole machine's. Where\n  another project could not be checked, it speaks for this project\n  alone (`Marked::checked_every_project`, also on the app's\n  `SetupDone`).\n- Only a name a shell can export is ever named, so a committed source\n  never reaches a terminal raw.\n- The wording no longer claims a chat is exposed (a chat starts from an\n  allowlisted environment) or that the token sits outside the Keychain\n  after a change to the app's sign-in: it is the person's own shells\n  and what they start.\n- The `--token-stdin` chat check also reads the machine store under the\n  account's home as the user database records it, so an environment\n  pointed at an empty store does not hide the person's projects. A\n  fenced build reads no store outside its fence.\n- Of each project only the chats' processes are read, leniently, from a\n  record of any version (`reopen::chat_pids`). A doubt says how to clear\n  it: delete the record named, or forget that project under Settings,\n  This machine; and the token can be given in the vault's tab.\n- Tests for each, at the CLI too, and for a project a window had open.\n\nRefs #1542\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>\nAssisted-by: claude-code\nPurlis-Chat: 01M4AREQXXRGBAEWM0SZ0N0WBN\nPurlis-Persona: steward",
          "timestamp": "2026-10-09T11:56:31+04:00",
          "tree_id": "b6d779abe65f6c7a40ad105ecaca1b09044790d0",
          "url": "https://github.com/purlis/purlis/commit/5b17d37f635dd3160d1c3b0038fcd7f0372a6d2c"
        },
        "date": 1791533021009,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.5206999999999999,
            "unit": "ms",
            "extra": "median of 5 runs: 0.510, 0.518, 0.521, 0.558, 0.563 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.949353000000002,
            "unit": "ms",
            "extra": "median of 5 runs: 16.670, 16.798, 16.949, 17.030, 17.150 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 103.364626,
            "unit": "ms",
            "extra": "median of 5 runs: 101.907, 102.216, 103.365, 105.534, 106.931 ms"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "eyehollow@gmail.com",
            "name": "Aaron Yordanyan",
            "username": "dbrownsimple"
          },
          "committer": {
            "email": "aaron.yor@gmail.com",
            "name": "Aaron Yordanyan",
            "username": "diazoxide"
          },
          "distinct": true,
          "id": "7cb5a293d68ebc8aea0ae4105e21c47393c3ada8",
          "message": "Settings test mocks name purlis's own plugin purlis@inline\n\nHarnessPlugins.test.tsx and WorkspaceLevel.test.tsx mocked the core's\nrows for purlis's own plugin as charter@inline. The core names it\npurlis@inline (harness_plugin.rs), so the mocks now do too. The tests\ncheck the UI, not the id, and pass either way.\n\nOnly the TypeScript fixtures item of the RN-8 follow-ups; the other open\nitems stay.\n\nRefs #1305\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>\nAssisted-by: claude-code\nPurlis-Chat: 01M4AREQXXRGBAEWM0SZ0N0WBN\nPurlis-Persona: steward",
          "timestamp": "2026-10-09T12:32:36+04:00",
          "tree_id": "96f782a114ff8fcd0da3cb7123a3e66fcf591850",
          "url": "https://github.com/purlis/purlis/commit/7cb5a293d68ebc8aea0ae4105e21c47393c3ada8"
        },
        "date": 1791535407812,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.5154655,
            "unit": "ms",
            "extra": "median of 5 runs: 0.497, 0.507, 0.515, 0.525, 0.537 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 17.192206499999998,
            "unit": "ms",
            "extra": "median of 5 runs: 16.520, 17.084, 17.192, 17.262, 17.425 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 104.7126835,
            "unit": "ms",
            "extra": "median of 5 runs: 102.991, 103.983, 104.713, 106.146, 106.590 ms"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "eyehollow@gmail.com",
            "name": "Aaron Yordanyan",
            "username": "dbrownsimple"
          },
          "committer": {
            "email": "aaron.yor@gmail.com",
            "name": "Aaron Yordanyan",
            "username": "diazoxide"
          },
          "distinct": true,
          "id": "a50dbf81bd323ff00ad65b53de5a692c862e60b4",
          "message": "Dispatch text guard: read a body a shell runs, give A6's remedy, and say what a listed row holds\n\nReview fold-ins for the dispatch core's loose ends.\n\n- A7e now reads a heredoc body a shell runs (`bash <<'EOF'`) as the shell reads it, with the\n  plan A7 and the leak guard share: a text subcommand in such a body beside a live\n  substitution is refused. A body a reader takes (`cat > notes.md <<'EOF'`) stays data.\n- The refusal gives A6's measured remedy: backslash-escape each backtick, or single-quote the\n  whole text when it holds no apostrophe. For a report it also names the `dispatch_report`\n  tool, whose text no shell reads.\n- ADR 0064's amendment says what a row of the list carries: chat number, name, persona,\n  workspace, state, age, and the branch purlis cut with how it stands; still only the asking\n  chat's own tasks.\n- The spec paragraph is rewrapped.\n\nRefs #1456\nRefs #1463\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>\nAssisted-by: claude-code\nPurlis-Chat: 01M4AREQXXRGBAEWM0SZ0N0WBN\nPurlis-Persona: steward",
          "timestamp": "2026-10-09T13:15:52+04:00",
          "tree_id": "1bdb5d8eeb6c4df49d245525479e8739767cadd6",
          "url": "https://github.com/purlis/purlis/commit/a50dbf81bd323ff00ad65b53de5a692c862e60b4"
        },
        "date": 1791537972522,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.41004450000000003,
            "unit": "ms",
            "extra": "median of 5 runs: 0.396, 0.409, 0.410, 0.415, 0.429 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.5315395,
            "unit": "ms",
            "extra": "median of 5 runs: 16.521, 16.526, 16.532, 16.641, 16.715 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 102.660968,
            "unit": "ms",
            "extra": "median of 5 runs: 102.091, 102.191, 102.661, 103.068, 103.137 ms"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "eyehollow@gmail.com",
            "name": "Aaron Yordanyan",
            "username": "dbrownsimple"
          },
          "committer": {
            "email": "aaron.yor@gmail.com",
            "name": "Aaron Yordanyan",
            "username": "diazoxide"
          },
          "distinct": true,
          "id": "f69e92a4fdfd815ee47d089d65b9a894a69b3284",
          "message": "Add a standing dispatch grant from Settings\n\nSettings › Project › Dispatch has Add a grant: the asking persona, the\ntarget, for me on this machine or for everyone in this project, in one\nworkspace or in any. It is the one way to make a grant with no dispatch\nwaiting, which is what a chat nobody is at needs.\n\nadd_dispatch_grant is window-only (WINDOW_ONLY, STANDING_DISPATCH) and\naudited as the person's under no chat before it counts. add_grant holds it\nto every rule an Allow is held to, refusing with nothing audited: two\npersonas of the project (a persona's own dispatch needs none), a policy\nlock, the person's never or a list of nevers that does not read, a grant\nfor everyone across a pair declined on this machine, a workspace that is\nnot there, one that stands already or is in the file\nalready, and a project file that is missing or not editable. A grant for\neveryone is written and acknowledged here, bound to the project's history\nas any acceptance is. The Allow and Add now keep a standing grant through\none function, keep_standing.\n\nWhere the project has workspaces, the form's workspace choice starts empty\nand must be picked, \"any workspace\" included (ruling 2: narrower is the\ndefault; the asking persona has no workspace of its own to start from).\n\nThe unattended refusal and the crossing refusal now say to grant it under\nSettings › Project › Dispatch, and their pinned rows move with them.\n\nRefs #1465\nRefs #1472\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>\nAssisted-by: claude-code\nPurlis-Chat: 01M4AREQXXRGBAEWM0SZ0N0WBN\nPurlis-Persona: steward",
          "timestamp": "2026-10-09T13:28:51+04:00",
          "tree_id": "70b91ff836285cbcd4d5ffdbad0794b891b2d776",
          "url": "https://github.com/purlis/purlis/commit/f69e92a4fdfd815ee47d089d65b9a894a69b3284"
        },
        "date": 1791539280217,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.286639,
            "unit": "ms",
            "extra": "median of 5 runs: 0.277, 0.279, 0.287, 0.287, 0.306 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.4872315,
            "unit": "ms",
            "extra": "median of 5 runs: 16.428, 16.437, 16.487, 16.513, 16.600 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 101.38734149999999,
            "unit": "ms",
            "extra": "median of 5 runs: 99.810, 100.967, 101.387, 101.642, 102.096 ms"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "eyehollow@gmail.com",
            "name": "Aaron Yordanyan",
            "username": "dbrownsimple"
          },
          "committer": {
            "email": "aaron.yor@gmail.com",
            "name": "Aaron Yordanyan",
            "username": "diazoxide"
          },
          "distinct": true,
          "id": "a2a5758912ee377fc3a207a56c76db4a1a85d5e1",
          "message": "Test the clone that makes its workspace, not the old refusal\n\nrepo_commands.rs pinned the refusal issue 1382 removed: a clone into a\nmissing workspace exited 1. It now checks the new rule end to end: the\nworkspace is made and scaffolded, the clone lands in it, and one line\nsays so; a bad name or a linked workspace is still refused with nothing\nmade or written through the link.\n\nRefs #1382\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>\nAssisted-by: claude-code\nPurlis-Chat: 01M4AREQXXRGBAEWM0SZ0N0WBN\nPurlis-Persona: steward",
          "timestamp": "2026-10-09T14:09:05+04:00",
          "tree_id": "7b7258cc2ca02397e9fcd4b97db308ccf3387edc",
          "url": "https://github.com/purlis/purlis/commit/a2a5758912ee377fc3a207a56c76db4a1a85d5e1"
        },
        "date": 1791542314251,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.4185065,
            "unit": "ms",
            "extra": "median of 5 runs: 0.398, 0.418, 0.419, 0.436, 0.458 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.499956499999996,
            "unit": "ms",
            "extra": "median of 5 runs: 16.283, 16.397, 16.500, 16.596, 16.938 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 101.74356350000001,
            "unit": "ms",
            "extra": "median of 5 runs: 101.370, 101.523, 101.744, 101.771, 101.805 ms"
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
          "id": "5fff8e9fb4d9b3015b890f880e201466c33780a9",
          "message": "Keep the travelling-Local test honest, and say merged before landed\n\nChoices::read now keeps the project root, so the three assertions in\na_local_file_that_would_travel_decides_nothing.rs that compared it with\nchoices made from text would fail: the derived equality compares the\nroot too. `Choices::of_project` attaches the root, `read` uses it, and\nthe test's expected values say where they were read.\n\nWith a blocker merged outside purlis, a merged member that waited on it\nwas described as \"merged ahead of its blocker\", which claims an order\nnobody saw. It merged before its blocker landed, and that is what the\nrow and the changes view now say.\n\nRefs #1371\nRefs #1093\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-10-09T15:07:23+04:00",
          "tree_id": "775d3de89f2d7eeef68a1bdde1e613fd55347985",
          "url": "https://github.com/purlis/purlis/commit/5fff8e9fb4d9b3015b890f880e201466c33780a9"
        },
        "date": 1791545683813,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.271374,
            "unit": "ms",
            "extra": "median of 5 runs: 0.268, 0.270, 0.271, 0.274, 0.284 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.481527,
            "unit": "ms",
            "extra": "median of 5 runs: 16.454, 16.469, 16.482, 16.546, 16.576 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 101.0689565,
            "unit": "ms",
            "extra": "median of 5 runs: 100.164, 100.849, 101.069, 101.451, 101.620 ms"
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
          "id": "f03a11f8db8234bce5914bd541cd8501a829157e",
          "message": "Ask a branch again when the reader gate was full, and say busy plainly\n\nWith six reads hung app-wide, a branch the watch resolves queues behind\nthem past its deadline and fails as busy. The window asks for its set\nonly when the set changes, so that branch went unwatched with nothing\nsaid. resolve() now asks again while the answer is busy, enough times for\nevery watched branch to have had its turn.\n\nThe busy message drops the doubled \"purlis\" and the \"no read came free\"\nmechanism (docs/ui-copy.md): \"purlis could not read the branch: it was\nbusy reading other branches for 30 seconds\". files::was_busy tells it\napart from a read that failed.\n\nRefs #1130\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>\nAssisted-by: claude-code\nPurlis-Chat: 01M4AREQXXRGBAEWM0SZ0N0WBN\nPurlis-Persona: steward",
          "timestamp": "2026-10-09T15:52:20+04:00",
          "tree_id": "1ef47d99a32d01c2c692f419013fc08d4dec4557",
          "url": "https://github.com/purlis/purlis/commit/f03a11f8db8234bce5914bd541cd8501a829157e"
        },
        "date": 1791548484795,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.5180230000000001,
            "unit": "ms",
            "extra": "median of 5 runs: 0.486, 0.487, 0.518, 0.534, 0.536 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.8520395,
            "unit": "ms",
            "extra": "median of 5 runs: 16.544, 16.562, 16.852, 17.054, 17.152 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 104.2836615,
            "unit": "ms",
            "extra": "median of 5 runs: 103.315, 103.467, 104.284, 104.861, 106.051 ms"
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
          "id": "0d94480d8bd59632dd2a4dae27b1e052a26664c7",
          "message": "Needs-you list: rewrap the change note to the fragment width\n\nRefs #1146\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>\nAssisted-by: claude-code\nPurlis-Chat: 01M4AREQXXRGBAEWM0SZ0N0WBN\nPurlis-Persona: steward",
          "timestamp": "2026-10-09T16:30:32+04:00",
          "tree_id": "908e4721b938e75f68bdc474b4ec0168a3bcd42b",
          "url": "https://github.com/purlis/purlis/commit/0d94480d8bd59632dd2a4dae27b1e052a26664c7"
        },
        "date": 1791549570335,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.5442940000000001,
            "unit": "ms",
            "extra": "median of 5 runs: 0.516, 0.524, 0.544, 0.558, 0.579 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.736877,
            "unit": "ms",
            "extra": "median of 5 runs: 16.350, 16.584, 16.737, 16.937, 17.265 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 103.47363949999999,
            "unit": "ms",
            "extra": "median of 5 runs: 102.477, 103.287, 103.474, 106.076, 107.049 ms"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "eyehollow@gmail.com",
            "name": "Aaron Yordanyan",
            "username": "dbrownsimple"
          },
          "committer": {
            "email": "aaron.yor@gmail.com",
            "name": "Aaron Yordanyan",
            "username": "diazoxide"
          },
          "distinct": true,
          "id": "2f1eb2a3072dec038ba91246fc9b38e287218b63",
          "message": "Undo of a memory's Move restores its exact name\n\nA journal memory moved to a persona or shared memory and back by Undo\ncame home only to the minute: the move rebuilds a journal name from the\nstamp line, which holds no seconds. The window knows the first name, so\nUndo now passes it.\n\n- memstore::move_one takes an optional exact name. It must be one plain\n  name, the same memory's name once a journal prefix is taken off both,\n  and carry a journal prefix exactly when the target is a journal. So it\n  can choose only the prefix's stamp, never rename a memory. Anything\n  else is InvalidInput. It goes through every existing check: a name the\n  target holds (a file or a link) is AlreadyExists, nothing is replaced,\n  and the target is reached by descriptor as before.\n- memscope::move_memory_as carries it; move_memory keeps its signature,\n  so purlis-cli is unchanged.\n- The memory_move command gains restore_as: Option<String>. Undo passes\n  the slug the memory had, the tab's Move passes null. bindings.ts and\n  uiRpc.ts regenerated.\n\nKeeping the index line's position on Undo stays open on the issue, and\nthe in-flight guard on Undo is another lane's.\n\nRefs #1190\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>\nAssisted-by: claude-code\nPurlis-Chat: 01M4AREQXXRGBAEWM0SZ0N0WBN\nPurlis-Persona: steward",
          "timestamp": "2026-10-09T16:35:50+04:00",
          "tree_id": "e3aab23181d6029c3b7a0dc8d0cf1e1630788bbc",
          "url": "https://github.com/purlis/purlis/commit/2f1eb2a3072dec038ba91246fc9b38e287218b63"
        },
        "date": 1791550872152,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "keystroke under ten flooding panes",
            "value": 0.47604199999999997,
            "unit": "ms",
            "extra": "median of 5 runs: 0.457, 0.460, 0.476, 0.481, 0.494 ms"
          },
          {
            "name": "2 MB burst, asked to drawn",
            "value": 16.263751499999998,
            "unit": "ms",
            "extra": "median of 5 runs: 16.144, 16.236, 16.264, 16.316, 16.334 ms"
          },
          {
            "name": "13 MB burst, asked to drawn",
            "value": 101.32307399999999,
            "unit": "ms",
            "extra": "median of 5 runs: 100.629, 101.083, 101.323, 101.907, 102.206 ms"
          }
        ]
      }
    ]
  }
}