---
version: unreleased
headline: two charter processes writing the same file stop publishing each other's half — every atomic write now uses a temp file of its own
---

**A frame's gather cache came out of CI existing and zero bytes.** The panel that reads it
drew a frame with no branch on it, on one interpreter out of four, on a branch that touched
none of this code. It is a race, so it passed on the rerun.

`os.replace` is atomic. The file it renames is not — and that is the half twenty writers in
charter were missing.

## One name, two writers

Every atomic writer in the package published through a temp file named after its target and
nothing else: `version` through `version.tmp`, `gather.json` through `gather.json.tmp`,
`workspace.json` through `workspace.json.tmp`. Two processes writing one target therefore
wrote into **one inode**, and the sequence needs no unusual timing:

1. A opens the shared temp with `"w"` — which truncates it to zero bytes.
2. B, a step ahead, renames that same path onto the target. The target is now A's empty
   file, and B has been told its own content landed.
3. A writes its bytes into an inode that is already the target, so a reader between here and
   A's own rename sees whatever has been flushed so far.

Every reader of these files was written to trust them. `gather.cached` *"hands back whatever
parses"* — deliberately, because a frame panel calls it on every repaint and a freshness
check there would cost the property the frame was built for. A truncated cache is read as
truth.

## Why it stopped being theoretical

0.56.0 added `notify.plane_changed_everywhere()`, which writes a gather cache for **every
frame on the plane** and then bumps every version file. Hooks, panels and CLI commands can
all be doing that at once, and the frame process records the plane from a debounce thread
while everything else in that process writes too. Concurrency on one target went from rare
to ordinary, and the CI failure followed.

## One writer, and its temp file is its own

`config.replace_for` is now the only atomic writer in charter — seventeen call sites in
`frame/state.py`, plus the frame's gather cache, the reopen manifest, the tool ceiling and
the workspace manifest. Its temp file carries the target's name, the writing process's pid
and a random tail, beside the target so the rename stays a rename. It is removed on every
failure, because a name nothing can predict is a name nothing can collect: `.charter/frame/`
has one sweep and it touches nothing but `*.transcript`.

Not `tempfile.mkstemp`, which `frame/reopen.py` reached for when it found this defect on its
own file. mkstemp creates at 0600, which is the right mode for `.charter/` and the wrong one
for `workspaces/<name>/workspace.json` — a committed file in your own git tree. The mode
stays `config.write_for`'s answer, which is to ask where the path is.

There is nothing to adopt. The files are the same files, at the same paths, with the same
contents; what changed is that two writers can no longer be inside one of them.

## Pinned by a race, not by a name

The test that holds this runs four writers at one target with a reader watching throughout,
and asserts the property — the target always holds one writer's whole content, never empty
and never a splice. A case that read the temp file's name and looked for a pid in it would
have passed against the defect verbatim: the bug was never that the name lacked a pid, it
was that two writers shared a file.
