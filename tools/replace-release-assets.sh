#!/usr/bin/env bash
# Replace assets on a GitHub release, one file at a time and in the order given.
#
#   tools/replace-release-assets.sh <tag> <file>...
#
# Needs GH_TOKEN and GITHUB_REPOSITORY, as the release workflow's publish job has them.
#
# **Why not `gh release upload --clobber`.** `--clobber` reads the release's asset list, then
# deletes the asset of the same name by the id that list gave it, then uploads. GitHub's asset
# list lags a replace: right after one publish has replaced `charter-….AppImage`, the next
# publish can be handed the id of the asset that publish already deleted. The delete answers
# 404, `gh` stops, and the publish dies with half its assets replaced. That happened twice on
# 2026-09-23 (runs 35837883711 and 35848325753, `HTTP 404 … releases/assets/583169419`), with
# a green run between them, and it is why the dev channel never served #207.
#
# So each file is looked up by NAME in a fresh listing, a 404 on its delete means somebody
# already deleted it (which is what we wanted), and the upload is retried when GitHub answers
# 404 or 422 — 422 being "an asset with this name already exists", the same lag seen from the
# other side — with the delete run again first.
#
# **Order is the caller's, and it matters.** A manifest (`dev.json`, `latest.json`) goes last,
# so no machine ever reads a manifest that names an asset not uploaded yet.
set -euo pipefail

tag=${1:?usage: replace-release-assets.sh <tag> <file>...}
shift
repo=${GITHUB_REPOSITORY:?GITHUB_REPOSITORY is not set}
release=$(gh api "repos/$repo/releases/tags/$tag" --jq .id)

# Delete every asset named $1 on the release, by id, from a listing made now.
delete_named() {
  local name=$1 ids id out
  ids=$(gh api --paginate "repos/$repo/releases/$release/assets" \
    --jq ".[] | select(.name == \"$name\") | .id")
  for id in $ids; do
    if ! out=$(gh api -X DELETE "repos/$repo/releases/assets/$id" 2>&1); then
      if grep -q 'HTTP 404' <<<"$out"; then
        echo "  $name (asset $id) was already gone"
      else
        echo "$out" >&2
        return 1
      fi
    fi
  done
}

for file in "$@"; do
  name=$(basename "$file")
  echo "replacing $name on $tag"
  for attempt in 1 2 3; do
    delete_named "$name"
    if out=$(gh release upload "$tag" --repo "$repo" "$file" 2>&1); then
      break
    fi
    echo "$out" >&2
    if [ "$attempt" = 3 ] || ! grep -q -E 'HTTP (404|422)' <<<"$out"; then
      echo "::error::could not upload $name to $tag" >&2
      exit 1
    fi
    echo "  GitHub's asset list is behind; trying $name again"
    sleep $((attempt * 5))
  done
done
