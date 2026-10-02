window.BENCHMARK_DATA = {
  "lastUpdate": 1790968661074,
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
      }
    ]
  }
}