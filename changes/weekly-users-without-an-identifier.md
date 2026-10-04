### Added

- **charter estimates weekly users without an identifier.** The first update check of each
  ISO week reads the channel's weekly manifest, `latest-weekly.json` or `dev-weekly.json`, which
  has the same bytes as the manifest. Its download count is the estimate. The request is the
  same from every machine and carries no id. `DO_NOT_TRACK=1` opts out.
  `docs/updating.md` gives the estimator and its biases (OB-17, #688).
