# Reviewing a Go change

What `go-reviewer` checks in every change. Edit it to match how this project works: it is
yours, and a chat reads the copy here.

- [ ] Is every returned error checked, and wrapped with `%w` where the caller needs to tell it apart?
- [ ] Is every goroutine given a way to stop, and does a `context.Context` reach the calls that can block?
- [ ] Is shared state guarded, and does `go test -race` pass?
- [ ] Does an exported name have a doc comment, and did an exported API change on purpose?
- [ ] Is a new module dependency needed, maintained, and licensed in a way this project accepts? Are `go.mod` and `go.sum` tidy?
- [ ] Do the tests describe behaviour, and was each new one seen to fail first?
