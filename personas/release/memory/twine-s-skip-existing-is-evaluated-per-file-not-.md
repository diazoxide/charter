# twine's --skip-existing is evaluated PER FILE, not per version: upload.p

_2026-09-03 16:25 · persistent_

twine's --skip-existing is evaluated PER FILE, not per version: upload.py loops 'for package in packages_to_upload' and calls repository.package_is_uploaded(package) on each. Verified by reading twine's source (uv run --with twine). So a workflow_dispatch retry can ADD a file to an already-published version if the build ever emits an artefact set that is not fully determined by project name + version. release.yml's guard does NOT close this: it curls the version endpoint with -o /dev/null and reads only the HTTP status, so a 200 proves the version exists, not which files are under it. (#835)
