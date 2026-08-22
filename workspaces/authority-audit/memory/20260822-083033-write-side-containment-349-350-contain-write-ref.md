# Write-side containment (#349/#350): contain.write_refusal is file_refusa

_2026-08-22 08:30 · persistent_

Write-side containment (#349/#350): contain.write_refusal is file_refusal's twin, one shared body, ONE difference — on a write ENOENT means 'about to be created', not a refusal. Both directions need a test: read it as a refusal and every first write on a fresh plane breaks; skip the check because exists() is false and a DANGLING link still creates its target wherever it points.
