### Fixed

- **Two branch changes on one repo run one at a time.** Remove, Done, New branch and Merge from
  the window now wait for one another on the same repo, instead of the second failing on git's
  lock in git's words. One still waiting after ten seconds is refused with a sentence that says
  another change to the repo is running (#1610).
