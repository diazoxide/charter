# The repo named charter is the app

**Accepted 2026-09-25**, by the operator.

charter-app began as the rebuild of a Python tool that lived in `diazoxide/charter`, and it took
the name `charter-app` because that name was taken. Neither half of that is true any more. The
app stands alone ([ADR 0044](0044-charter-apps-design-record-lives-in-charter-app.md), [ADR
0045](0045-charters-version-is-the-apps-version.md)), and `diazoxide/charter` has held only the
project's own plane since the Python charter was retired there (charter#1186, tag `cli-final`).
So the repository called charter was a plane, and charter was the repository called
charter-app.

## The decision

**Two renames, in this order:**

1. `diazoxide/charter` becomes **`diazoxide/charter-plane`**. It is the plane charter is
   developed from, public as an example of one, and not the product.
2. `diazoxide/charter-app` becomes **`diazoxide/charter`**. charter-app is charter.

**Every link in this repository that means the plane was rewritten first**, to say
`diazoxide/charter-plane`: the compiled-in news corpus, `news::HISTORY_REPO`, the ADRs, the
spec, the plane format's header and the comments in the core. That had to land before the
second rename, not after, because of what the second rename does to the first one's redirect.

## What this costs

**The redirect from `diazoxide/charter` to the plane ends for good.** GitHub keeps a renamed
repository's old name pointing at the new one only until something else takes the name. The
second rename takes it, so from then on every `diazoxide/charter` link resolves to the app. A
link written in this repository was rewritten before that happened. A link written anywhere
else was not: issue and pull request bodies, commit messages and release notes in both
repositories still say `diazoxide/charter`, and after the rename the ones that meant the plane
point at the app. That is accepted. They are history, and rewriting them is not possible for
commits and not worth it for the rest.

**The stars and forks stay with the plane.** The plane's 18 stars and 4 forks belong to the
repository, not the name, so they go with `charter-plane`, and the app starts with its own.

**Two recorded values in `docs/plane-format.md` still say `diazoxide/charter`**, on purpose.
They are what the Python charter at a fixed commit passed to `claude plugin marketplace add`
and what `claude` wrote into `known_marketplaces.json` as a result. They record what was
measured, and a record does not change its readings.

## What this rules out

- **Ever reusing the name `charter-app`.** An installed app finds its updates at
  `github.com/diazoxide/charter-app/releases/…`, and reaches `diazoxide/charter` only through
  GitHub's redirect from the old name. A new repository named `charter-app` would end that
  redirect and every installed updater with it. The updater's URL moves to the new name in a
  later release; the old name stays unused regardless, for the installs that never take it.
- Changing the app's `diazoxide/charter-app` references before the second rename. Until then
  `diazoxide/charter` is the plane, and an updater pointed at it would ask the plane for
  releases.

## What it does not settle

- **The plugin keeps the name `charter-app`**, so its skills are still `charter-app:<skill>` and
  a session loads it as `charter-app@inline`. So does the Tauri crate, `charter-app`. Renaming
  the plugin means moving planes' settings off `charter-app@inline`, the way the core already
  handles `charter@charter`, and that is its own change, tracked in its own issue.
- **The bundle identifier stays `dev.charter.app`.** It already says charter, and changing it
  would make the operating system treat the app as a different one.

## Amended 2026-09-25: the plugin is called charter (#406)

The first item above is settled. **The plugin the app bundles is named `charter`**, so its
skills reach the model as `charter:<skill>` and a session loads it as `charter@inline`.

**The Python charter's plugin is named `charter` too**, as `charter@charter`. Only the
marketplace after the `@` tells the two apart, and Claude Code loads one plugin per name
(measured on claude 2.1.282, in `charter_core::plugin`'s header): with both available the
`--plugin-dir` copy is loaded, and a session that turns `charter@inline` off gets
`charter@charter` in its place, skills and hooks included. So the pins carry the separation
the old name used to. Every chat's `--settings` pins `charter@inline` on, which a chat cannot
move and which keeps the Python plugin out. It pins `charter@charter` off, which does not
touch `charter@inline`. Each pin matches the whole id, and tests hold both directions.

**The old id is pinned off, not migrated.** charter never wrote `charter-app@inline` into a
file. It was only ever in a session's `--settings`, so no plane's `.claude/settings.json` holds
it unless somebody put it there. The only files that could name it are a project's
`[harness_plugins.claude]` and a workspace's `settings.harness_plugins.claude`. Those the
settings tab never writes for a pinned plugin, and a person would have put it there by hand.
`charter-app@inline` is now a third pin, always off. A file that says `false` agrees with it.
A file that says `true` is refused by the settings tab's save and ignored at a chat's start,
with a sentence that names `charter@inline`. A skill named `charter-app:<skill>` in a
persona's `skills:` is the persona's own text, and the plane changes it.

**The Tauri crate and binary stay `charter-app`, and the bundle identifier stays
`dev.charter.app`.** Neither reaches the model or a plane's files, and renaming the crate is
build churn with nothing to show for it.
