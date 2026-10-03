### Fixed

- **A save to a branch that a GitHub ruleset protects takes the protected-branch path.** GitHub's
  refusal for such a push says "Changes must be made through a pull request" or "Cannot update
  this protected ref". That is now read like a protected branch's refusal, as a GitLab protected
  branch already was. The project's save pushes to a branch of its own and links to opening a
  pull request, and a repo's save names the setting that takes it through one. A push that a
  ruleset refuses for what it carries, a secret or a file, still reports the push failure. Before,
  every such save said only that the push failed (FG-3, #711).
- **The palette's "Open changes" row says "request"**, not "pull request", because a member may
  be on GitLab (FG-3, #711).
