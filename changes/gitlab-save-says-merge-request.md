### Fixed

- **A save on GitLab calls its merge request a merge request.** In the `pr` and `pr-merge`
  modes, a save whose origin is on GitLab said "pull request #12". It now says "merge request
  !12", as GitLab does. This covers the save's messages, the description it writes, `doctor`,
  the plane-root alert, the memory note, and the Saving view and save indicator. Where repos on
  GitHub and GitLab are listed together, the Saving view calls each one a "request". Link to
  work item now gives a GitLab key as an example too. GitHub's wording is unchanged (FG-3,
  #1067).
