### Added

- **A file and line a session record names open from its tab.** A path written as inline code in
  a record, such as `svc/src/lib.rs:42`, is a button that opens the file in the light editor at
  that line, in the repo the path names, or in the workspace's only repo. The record also lists
  the files it names, each with *Open in your editor* at that line. A path the repo's folder does
  not offer is refused, as in the file viewer (#984, #1043).
