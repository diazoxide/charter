# Supported platforms

These are the platforms charter is held to. Each row names the CI label (the GitHub Actions
image) it is covered on and what that job proves, because "supported" should mean "tested", and
where a row is not tested it says so. A row marked **evidence** is reported on every push to `main`
and every night, not on a pull request, and never gates a merge or a release, like the `windows`
job. The same table is in the README. A
test (`site/test/platforms.test.mjs`) fails when the two copies differ, or when a CI label the
table names is no longer used in `.github/workflows/ci.yml`; it does not check which job uses it.

| Platform | Floor | CI label | What CI proves |
|---|---|---|---|
| macOS 27 | Apple silicon | `xcode-27` | Evidence: builds the app, compiled from scratch on that image. The image is a preview |
| macOS 26 | Apple silicon | `macos-latest` | Builds the app, and starts and drives it through every scenario test. Release builds are made on this label (`release.yml`) |
| Ubuntu 26.04 LTS | x86_64 | `ubuntu-26.04` | Evidence: builds the app from scratch on that image and holds its cold start to the 2 s limit |
| Ubuntu 24.04 LTS | x86_64 | `ubuntu-24.04` | Every Rust and web test, the app build, the cold start, and every scenario test. Release builds are made on this label (`release.yml`) |
| Fedora 44 (the current release) | x86_64 | `fedora:44` | Evidence: builds the app against Fedora's own libraries, runs the core and CLI tests as a normal user, and starts the app |
| Windows 11 | not supported yet | `windows-latest` | Nothing yet. The job is evidence for the port and allowed to fail. Windows 11 joins this list when the Windows port lands |
| iOS 16.4 or later | no iOS app yet | — | Nothing. The floor any iOS client will be held to |
| Desktop browsers, the current Chrome, Edge, Firefox and Safari | no browser client yet | — | Nothing. The floor any browser client will be held to |

What the table does not promise:

- **Only the two latest macOS majors.** An older macOS may run charter, and nothing tests it.
  Release builds are for Apple silicon only; there is no Intel build.
- **Ubuntu means its LTS releases, on x86_64.** Release builds are made on 24.04, so a system
  older than 24.04 is not expected to run them. On 26.04, CI builds the app from source; it does
  not install a release build.
- **Fedora is covered from source.** The Fedora job builds charter there and starts it. It does
  not run the AppImage, and there is no `.rpm`. The row and the job's image are bumped together
  on each Fedora release.
- **Evidence rows can be red without stopping anything.** A preview image or a new Fedora can
  break for reasons that are not charter's; their jobs report it and block nothing. The 26.04
  cold start has run close to the limit (a 1797 ms median against 2000 ms), so it is watched
  there before it gates anything.
- **`-latest` labels move.** `macos-latest` is macOS 26 today. Each job prints the OS it ran on
  and warns when a label has moved off the version its row names. When macOS 27 becomes
  `macos-latest`, the second macOS entry in `ci.yml` becomes `macos-26`, so CI keeps covering
  both majors.
- **Windows** is not supported until the port lands. Until then the `windows` job reports what
  breaks, and its result blocks nothing ([ADR 0031](adr/0031-windows-gets-charters-guards-or-it-gets-no-charter.md)).
