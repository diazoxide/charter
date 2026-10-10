### Changed

- **A repo on a second forge is tracked by the first run's project.** Opening a repo whose
  `origin` is on github.com or gitlab.com, when the project tracks only the other forge, adds a
  forge for it to the project, with the owner the remote names. The repo picker and `discover`
  then see both. A remote on a forge the project already tracks adds nothing, and the forge shows
  in Settings › Project › Forges, where it can be removed (#1094).
- **A self-managed GitLab or GitHub keeps its host.** When a new project's repo is on a host
  whose forge purlis cannot tell, and you name the forge, the project's forge uses the remote's
  host, and the owner its path names, without typing either again (#1094).
