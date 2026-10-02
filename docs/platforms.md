# Supported platforms

These are the platforms charter is held to. Each row names the CI runner that covers it and what
that job proves, because "supported" should mean "tested", and where a row is not tested it says
so. The same table is in the README. A test (`site/test/platforms.test.mjs`) fails when the two
copies differ, or when a runner the table names is no longer in `.github/workflows/ci.yml`.

| Platform | Floor | CI runner | What CI proves |
|---|---|---|---|
| macOS 27 | Apple silicon | `xcode-27` | Builds the app. The runner image is a preview, so its check is reported and not required |
| macOS 26 | Apple silicon | `macos-latest` | Builds the app, and starts and drives it through every scenario test. The published macOS build is made here |
| Ubuntu 26.04 LTS | x86_64 | `ubuntu-26.04` | Builds the app and holds its cold start to the 2 s limit |
| Ubuntu 24.04 LTS | x86_64 | `ubuntu-24.04` | Every Rust and web test, the app build, the cold start, and every scenario test. The published `.deb` and AppImage are made here |
| Fedora, the current release | x86_64 | `fedora:latest` | Builds the app against Fedora's own libraries, runs the core and CLI tests as a normal user, and starts the app |
| Windows 11 | not supported yet | `windows-latest` | Nothing yet. The job is evidence for the port and allowed to fail. Windows 11 joins this list when the Windows port lands |
| iOS 16.4 or later | no iOS app yet | — | Nothing. The floor any iOS client will be held to |
| Desktop browsers, the current Chrome, Edge, Firefox and Safari | no browser client yet | — | Nothing. The floor any browser client will be held to |

What the table does not promise:

- **Only the two latest macOS majors.** An older macOS may run charter, and nothing tests it.
  Published macOS builds are for Apple silicon only; there is no Intel build.
- **Ubuntu means its LTS releases, on x86_64.** The `.deb` and AppImage are built on 24.04, so a
  system older than 24.04 is not expected to run them. On 26.04, CI builds the app from source;
  it does not install the published `.deb`.
- **Fedora is covered from source.** The Fedora job builds charter there and starts it. It does
  not run the published AppImage, and there is no `.rpm` yet.
- **`macos-latest` moves.** It is macOS 26 today. When the runner images make macOS 27 the latest,
  the second macOS entry in `ci.yml` becomes `macos-26`, so CI keeps covering both majors. Each
  macOS job prints the version it ran on.
- **Windows** is not supported until the port lands. Until then the `windows` job reports what
  breaks, and its result blocks nothing ([ADR 0031](adr/0031-windows-gets-charters-guards-or-it-gets-no-charter.md)).
