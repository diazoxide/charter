# The two channels differ in optimisation profile and bundle set, and in nothing else

**Accepted 2026-10-07** by the operator, who was grilled on it and agreed nine decisions
(#1429). It builds on
[ADR 0042](0042-charter-updates-itself-and-nothing-it-cannot-verify-reaches-it.md), which set up
the two channels, the `release` environment (amendment of 2026-09-26) and the rule that the
release builds cold from code it pinned (amendment of 2026-09-29). It amends none of them. It
is about how purlis itself is built and published, so it adds three words to the glossary and no
part to any of the five concepts.

## Where purlis was

The dev channel is published after every green `ci` on `main`. Each publish took 37 to 45
minutes, and the macOS runner was the long one:

| Step | macOS | Linux |
|---|---|---|
| The command line and the built-in extension | 7.5 min | 4.7 min |
| The app | 14 min | 10 min |
| The extra bundle (`.dmg`, AppImage) | 7 min | 6 min |
| The whole job | 29 min | 22 min |

Both platforms used Cargo's `release` profile (`lto = true`, `codegen-units = 1`) and built with
no cache, which ADR 0042 requires of the job that holds the signing keys. A dev build exists so
that a merged change can be tried the same hour. At 40 minutes a build, several merges share one
publish, and the one that broke something is harder to name.

## The decision

1. **Only the dev channel build gets faster.** The stable build keeps its profile, its bundles
   and its steps.
2. **No build cache in the job that holds the signing keys, on either channel.** ADR 0042's rule
   and its test in `crates/release-manifest` stay as they are.
3. **A dev publish makes no `.dmg`.** A stable release still does.
4. **The dev channel is compiled with a lighter profile, `dev-release`.**
5. **The build stays on one runner per platform.** It is not split across jobs.
6. **A profile guard runs every night:** `main`, built with the stable profile, holding no
   signing key and publishing nowhere. A failure opens one issue, or rewrites it.
7. **`dev-release` inherits `release` and changes two settings:** `lto = "thin"` and
   `codegen-units = 16`. `opt-level = 3`, `panic = "abort"` and `strip = true` are inherited.
   If the profile had saved less than about five minutes it would have been dropped.
8. **When the AppImage is required, one `tauri build` makes both Linux bundles,** on both
   channels. The two-step shape stays only for an unsigned build started by hand, where the
   extra bundle may fail.
9. **The build says which profile made it:** in `purlis --version`, in About purlis, and in the
   run's summary, which no longer says "release profile" for every build.

## What the channels may differ in, and what they may not

A **dev channel build** and a **stable build** differ in exactly two things:

- **The optimisation profile.** `dev-release` or `release`.
- **The bundle set.** A dev channel build makes no `.dmg`.

They never differ in:

- **Signing.** Both are minisign-signed by the same key, and `release-manifest` checks both the
  same way before a manifest is written. Both get the same macOS signing, checked on the bundle.
  Both are attested, and both ship an SBOM.
- **Cache.** Neither restores or saves one in a job that holds a secret, the `release`
  environment or the power to publish.
- **Build isolation.** Both build on one runner per platform, from that job's own checkout and
  the lockfiles alone.

A third difference is a ruling, not a tuning change. A test holds `dev-release` to its two
settings so that a third cannot arrive as a line in `Cargo.toml`.

`plan` decides the profile and the bundles once per run, from the event that started it, and
every later step reads that decision. The build's own binary is asked which profile made it
before anything is bundled, and the run stops if the answer is not the profile `plan` chose.

| What started the run | Profile | `.dmg` | AppImage | Published |
|---|---|---|---|---|
| A `v*` tag (stable) | `release` | made, may fail | required, one build | stable channel |
| `ci` green on `main` (dev) | `dev-release` | not made | required, one build | dev channel |
| The nightly schedule (profile guard) | `release` | made, may fail | required, one build | nowhere |
| Started by hand | `release`, or `dev-release` when asked | made, may fail | made, may fail | nowhere |

## Why a lighter profile, and what it gives up

Fat LTO over one codegen unit optimises the whole program in one pass on one core. Thin LTO over
sixteen units does most of that work in parallel and keeps most of its result. The binary is
somewhat larger and somewhat slower, and that is the cost. It is paid by the dev channel, which
is for trying a change and not for measuring it: every performance budget is measured on a
`release` build, by `ci`'s own jobs and by `tools/bench.mjs`
([ADR 0086](0086-every-performance-budget-names-the-job-that-measures-it-and-ci-holds-charters-own-cost-by-regression.md)),
and none is measured on a published dev build.

Nothing that changes behaviour is different. `panic = "abort"` is inherited, so a dev channel
build still dies on a panic as a stable one does, and `strip = true` is inherited, so it still
ships no symbols.

## What was measured

**On the runners**, 2026-10-07: the same commit, built twice by hand through the unsigned path,
once with each profile (runs 37598350227 and 37598330266). Both used the two-step shape, so the
steps compare one to one.

| Step | macOS `release` | macOS `dev-release` | Linux `release` | Linux `dev-release` |
|---|---|---|---|---|
| The command line and the built-in extension | 490 s | 263 s | 350 s | 228 s |
| The app | 1003 s | 708 s | 755 s | 593 s |
| The extra bundle | 423 s | 407 s | 424 s | 313 s |
| The whole job | 1984 s | 1438 s | 1616 s | 1223 s |
| The uploaded artifact, zipped | 31.5 MB | 33.8 MB | 117.0 MB | 119.9 MB |

The profile saves 522 seconds on macOS, 8.7 minutes, across the two steps a dev publish keeps.
That is past the five-minute bar, on the runner that sets the length of the run, so the profile
stays. On Linux it saves 284 seconds. The Linux `release` run was slow for its kind (the dev
publish before it took 262 s and 537 s for the same two steps), so the Linux saving is the less
certain of the two.

A dev publish also drops the macOS extra bundle. From these runs its macOS job comes to about
1030 seconds, 17 minutes, where the last dev publish before this record took 2079 (run
37588549446). Its Linux job loses the second `tauri build` and gains the AppImage's own
bundling, which these runs did not time on its own. The jobs after the build are unchanged at
about six minutes, four of them compiling the manifest tool. So a dev publish should take about
24 minutes where it took 37 to 45. **That is a projection.** A publish cannot be run from a
branch, so the first one after this record merges is the measurement.

**On one Apple Silicon Mac**, with `CARGO_BUILD_JOBS=1`, cold, each profile in a target
directory of its own:

| | `release` | `dev-release` |
|---|---|---|
| `cargo build -p purlis-cli -p persona-statistics` | 6 min 17 s | 5 min 34 s |
| `tauri build --bundles app` | 10 min 01 s | 13 min 50 s |
| `purlis` | 9,826,400 bytes | 11,429,872 bytes |
| `charter` | 319,888 bytes | 357,504 bytes |
| `persona-statistics` | 485,488 bytes | 579,168 bytes |
| the app's binary | 21,832,512 bytes | 26,415,328 bytes |

Thin LTO does more work in total and wins by doing it on several cores. With one job it has
one core, which is why the app took longer here with the lighter profile; the `dev-release` app
build also shared the machine with a test build, so its time is the less clean of the two. The
runner's times are the ones the decision rests on. The sizes do not depend on the machine: the
binaries are 16 to 21% larger.

The local `tauri build -- --profile dev-release` also showed where Tauri looks: it built
`target/dev-release/charter-app` and wrote the bundle to `target/dev-release/bundle/`.

## Why the build is not split across runners

The command line and the app could be built by two jobs at once, and the second job handed the
first one's binaries. That was rejected. The job that signs would then sign a binary another job
built: an artifact passed between jobs is a file whatever job uploaded it, which is the reason
ADR 0042 refuses a cache. The signing job builds everything it signs, from its own checkout.

Building the two in separate jobs and signing in each would keep that rule and still split the
key across more jobs than hold it today. The time it would save is the shorter of the two
builds, on the one platform where it matters, and the profile already saves more.

## Why the dev channel has no `.dmg`

The `.dmg` took seven minutes of the macOS job, because a second `tauri build` rebuilds the
front end and links the app again. The updater never reads it: an installed purlis updates from
the `.app.tar.gz`. A person installing a dev build for the first time has the `.app.zip`, which
is the same app. A stable release is where a first install starts, so it keeps its `.dmg`.

The `dev` release's assets are replaced by name, so the `.dmg` an earlier publish left would
have stayed beside every later build, naming an older one. The dev publish removes it.

## Why one `tauri build` when the AppImage is required

The extra bundle was a second `tauri build` so that its failure could be survived: a build that
lost its `.dmg` still had its `.app`. That is right for a bundle that may fail. The AppImage on
a publish may not, because it is the only Linux bundle the updater can replace, so the second
run bought nothing and cost a second link of the app. One run now makes both, on both channels.
A build started by hand keeps the two steps.

## The profile guard

Until this record every dev publish was also a build of the stable profile, 20 or more times a
week. Now nothing builds `release` with fat LTO between one tag and the next, except this: the
`release` workflow runs on a schedule every night and builds `main` the way a stable build is
built. A link that only fails under fat LTO is then found on a night, not on the operator's tag.

The guard is the path a build started by hand already takes. It runs outside the `release`
environment, so it holds no signing key and its bundles carry no updater signature. It publishes
nowhere and is not attested. `plan` names the two events that get the environment, a tag push
and a green `ci`, so the schedule gets none, and neither does any event added later.

The guard treats the AppImage as required, as a stable publish does. A night its bundler breaks
is a red night.

A red night opens one issue, titled *the stable profile does not build on main*, or rewrites
that issue's body. It never opens a second and never comments. The first clean night closes it.
A night that was cancelled leaves the issue as it is. The job that does this holds
`issues: write` and nothing else. It checks nothing out.

## The version line

`purlis --version` prints `purlis 0.4.2 (release profile)`, or `(dev-release profile)`, or
`(debug profile)` for a local build. About purlis says *Built with the `dev-release` profile*
under the version. The word is the name Cargo files the build under, read from the build's
output directory, because the `PROFILE` Cargo hands a build script is `release` for both.

Two builds of the same source are not the same binary. A report of a slow or a crashing one can
now say which it was.

## What was rejected

- **A cache in the build job, for the dev channel only.** It is the largest saving on offer and
  it is the one ADR 0042 refuses, for a reason that does not depend on the channel: the dev
  channel is signed with the same key.
- **Splitting the build across runners**, for the reason above.
- **A lower `opt-level`, or leaving LTO out.** Either would make a dev build a different enough
  program that "it is fine on dev" would stop meaning much.
- **`panic = "unwind"` or symbols in a dev build.** Both change what the program does when it
  fails, or what it ships.
- **A separate workflow file for the profile guard**, with its own copy of the build steps. The
  guard exists to show that the stable build works, and a copy of its steps would show that the
  copy works.
- **Dropping the `.dmg` from stable too.** A first install starts on the release page.

## Consequences

- **A dev channel build is not the binary a stable release will be.** It is the same source,
  optimised less. Performance is never judged on one.
- **A break in the stable profile is found within a day, not within the hour.** Before, the next
  dev publish would have found it.
- **The nightly guard costs about an hour of runner time a night**, 33 minutes of it on macOS.
- **The dev release has no `.dmg`.** A first install of a dev build on macOS uses the `.app.zip`.
- **The version line changed.** `purlis --version` ends with the profile in brackets. Nothing in
  purlis parses that line.
- **The schedule is one more way into the workflow that holds the keys.** It reaches none of
  them, and two tests in `crates/release-manifest` hold that: the events that get the `release`
  environment are named, and the guard's own job has no secret, no environment and no action.
