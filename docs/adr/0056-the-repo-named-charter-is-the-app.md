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
