## Context & decisions

<!-- Started from charter's Go project template. Replace it with what you learn. -->

How a change in this repo is checked:

- `gofmt -l .` prints nothing
- `go vet ./...`
- `go test ./<package>/...` while you work, `go test -race ./...` before you hand back

`go-engineer` makes changes and `go-reviewer` reviews them.
